//! Historical-data farm state from the global notices, for the wait in
//! `Client::resubscribing` after a 10182 ended a keep-up-to-date bar request.
//!
//! Each bus's notice broadcaster owns one and feeds it every notice before
//! fanning it out, so the state is current before any caller sees the notice
//! and an OK that arrives before a caller starts waiting is not missed. The
//! bus resets it when the transport drops, before the reconnect's handshake
//! notices arrive.

use std::collections::HashSet;

use crate::messages::{ConnectivityStatus, DataFarmKind, Notice, CONNECTIVITY_RESTORED_DATA_LOST_CODE, CONNECTIVITY_RESTORED_DATA_MAINTAINED_CODE};

/// What the farms looked like when a wait began; see [`FarmState::mark`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FarmMark {
    restorations: u64,
    sessions: u64,
}

/// Why a wait for the historical-data farms ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FarmRecovery {
    /// No historical-data farm reports broken, or TWS reported its
    /// connectivity restored (1101 or 1102) after the mark.
    Recovered,
    /// The transport dropped or the bus shut down after the mark: the
    /// caller's request is gone with the old session.
    SessionEnded,
    /// The wait's deadline passed first.
    TimedOut,
}

#[derive(Debug, Default)]
struct FarmState {
    /// Historical-data farms whose last notice reported them broken, by name.
    /// A farm that went inactive (2107) instead of reporting OK leaves it.
    broken: HashSet<Option<String>>,
    /// 1101 and 1102 notices seen.
    restorations: u64,
    /// Transport resets seen.
    sessions: u64,
    closed: bool,
}

impl FarmState {
    /// Record `notice`; `true` when it changed the state.
    fn observe(&mut self, notice: &Notice) -> bool {
        if matches!(
            notice.code,
            CONNECTIVITY_RESTORED_DATA_LOST_CODE | CONNECTIVITY_RESTORED_DATA_MAINTAINED_CODE
        ) {
            self.restorations += 1;
            return true;
        }
        match notice.data_farm() {
            Some(farm) if farm.kind == DataFarmKind::HistoricalData => {
                if farm.status == ConnectivityStatus::Broken {
                    self.broken.insert(farm.name)
                } else {
                    self.broken.remove(&farm.name)
                }
            }
            _ => false,
        }
    }

    fn reset(&mut self) {
        self.broken.clear();
        self.sessions += 1;
    }

    fn mark(&self) -> FarmMark {
        FarmMark {
            restorations: self.restorations,
            sessions: self.sessions,
        }
    }

    /// How a wait begun at `mark` ends now, or `None` to keep waiting.
    fn recovery(&self, mark: FarmMark) -> Option<FarmRecovery> {
        if self.closed || self.sessions != mark.sessions {
            Some(FarmRecovery::SessionEnded)
        } else if self.broken.is_empty() || self.restorations != mark.restorations {
            Some(FarmRecovery::Recovered)
        } else {
            None
        }
    }
}

#[cfg(feature = "sync")]
pub(crate) mod sync_impl {
    use std::sync::{Condvar, Mutex, PoisonError};
    use std::time::Instant;

    use super::{FarmMark, FarmRecovery, FarmState};
    use crate::messages::Notice;
    use crate::transport::common::lock;

    /// Blocking farm watch: waiters park on a `Condvar` woken by every change.
    #[derive(Debug, Default)]
    pub(crate) struct FarmWatch {
        state: Mutex<FarmState>,
        changed: Condvar,
    }

    impl FarmWatch {
        pub(crate) fn observe(&self, notice: &Notice) {
            if lock(&self.state).observe(notice) {
                self.changed.notify_all();
            }
        }

        pub(crate) fn reset(&self) {
            lock(&self.state).reset();
            self.changed.notify_all();
        }

        pub(crate) fn close(&self) {
            lock(&self.state).closed = true;
            self.changed.notify_all();
        }

        pub(crate) fn mark(&self) -> FarmMark {
            lock(&self.state).mark()
        }

        /// Block until the historical-data farms recover after `mark`, the
        /// session ends, or `deadline` passes.
        pub(crate) fn wait(&self, mark: FarmMark, deadline: Instant) -> FarmRecovery {
            let mut state = lock(&self.state);
            loop {
                if let Some(recovery) = state.recovery(mark) {
                    return recovery;
                }
                let Some(remaining) = deadline.checked_duration_since(Instant::now()).filter(|d| !d.is_zero()) else {
                    return FarmRecovery::TimedOut;
                };
                state = self.changed.wait_timeout(state, remaining).unwrap_or_else(PoisonError::into_inner).0;
            }
        }
    }
}

#[cfg(feature = "async")]
pub(crate) mod async_impl {
    use tokio::sync::watch;
    use tokio::time::Instant;

    use super::{FarmMark, FarmRecovery, FarmState};
    use crate::messages::Notice;

    /// Async farm watch: a `watch` channel, so a waiter dropped mid-wait
    /// leaves nothing behind and the dispatcher needs no runtime to update it.
    #[derive(Debug)]
    pub(crate) struct FarmWatch {
        sender: watch::Sender<FarmState>,
    }

    impl Default for FarmWatch {
        fn default() -> Self {
            Self {
                sender: watch::channel(FarmState::default()).0,
            }
        }
    }

    impl FarmWatch {
        pub(crate) fn observe(&self, notice: &Notice) {
            self.sender.send_if_modified(|state| state.observe(notice));
        }

        pub(crate) fn reset(&self) {
            self.sender.send_modify(FarmState::reset);
        }

        pub(crate) fn close(&self) {
            self.sender.send_modify(|state| state.closed = true);
        }

        pub(crate) fn mark(&self) -> FarmMark {
            self.sender.borrow().mark()
        }

        /// Resolve once the historical-data farms recover after `mark`, the
        /// session ends, or `deadline` passes.
        pub(crate) async fn wait(&self, mark: FarmMark, deadline: Instant) -> FarmRecovery {
            let mut receiver = self.sender.subscribe();
            loop {
                if let Some(recovery) = receiver.borrow_and_update().recovery(mark) {
                    return recovery;
                }
                // The sender lives as long as `self`, so `changed` cannot fail.
                if tokio::time::timeout_at(deadline, receiver.changed()).await.is_err() {
                    return FarmRecovery::TimedOut;
                }
            }
        }

        /// Waiters currently parked in [`wait`](Self::wait).
        #[cfg(test)]
        pub(crate) fn waiters(&self) -> usize {
            self.sender.receiver_count()
        }
    }
}

#[cfg(test)]
#[path = "farm_watch_tests.rs"]
mod tests;
