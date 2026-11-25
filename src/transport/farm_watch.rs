//! Historical-data farm state from the global notices, for the wait in
//! `Client::resubscribing` after a 10182 ended a keep-up-to-date bar request,
//! and the 1101s after which it reopens a live stream.
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
    data_lost: u64,
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
    /// 1101 notices seen, and the last one.
    data_lost: u64,
    last_data_lost: Option<Notice>,
    /// Transport resets seen.
    sessions: u64,
    closed: bool,
}

impl FarmState {
    /// Record `notice`; `true` when it changed the state.
    fn observe(&mut self, notice: &Notice) -> bool {
        if notice.code == CONNECTIVITY_RESTORED_DATA_LOST_CODE {
            self.data_lost += 1;
            self.last_data_lost = Some(notice.clone());
        }
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
            data_lost: self.data_lost,
            sessions: self.sessions,
        }
    }

    /// The last 1101, if one arrived after `mark`.
    fn data_lost_since(&self, mark: FarmMark) -> Option<&Notice> {
        self.last_data_lost.as_ref().filter(|_| self.data_lost != mark.data_lost)
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
    use std::sync::{Arc, Condvar, Mutex, PoisonError, Weak};
    use std::time::Instant;

    use crossbeam::channel::{self, Receiver, Sender};

    use super::{FarmMark, FarmRecovery, FarmState};
    use crate::messages::{Notice, CONNECTIVITY_RESTORED_DATA_LOST_CODE};
    use crate::transport::common::lock;

    /// Blocking farm watch: waiters park on a `Condvar` woken by every change.
    /// A reader blocked on a subscription can't park there too, so it selects
    /// on a [`DataLostSignal`] instead, which every 1101 wakes.
    #[derive(Debug, Default)]
    pub(crate) struct FarmWatch {
        state: Mutex<FarmState>,
        changed: Condvar,
        /// One per live [`DataLostSignal`]; a dropped signal's entry no longer upgrades.
        data_lost_signals: Mutex<Vec<Weak<Sender<()>>>>,
    }

    /// Receives `()` after each 1101, coalesced while unread. A wake-up only:
    /// [`FarmWatch::data_lost_since`] says whether a 1101 came after a mark.
    #[derive(Debug)]
    pub(crate) struct DataLostSignal {
        receiver: Receiver<()>,
        _sender: Arc<Sender<()>>,
    }

    impl DataLostSignal {
        pub(crate) fn receiver(&self) -> &Receiver<()> {
            &self.receiver
        }
    }

    impl FarmWatch {
        pub(crate) fn observe(&self, notice: &Notice) {
            if lock(&self.state).observe(notice) {
                self.changed.notify_all();
            }
            if notice.code == CONNECTIVITY_RESTORED_DATA_LOST_CODE {
                lock(&self.data_lost_signals).retain(|signal| {
                    signal
                        .upgrade()
                        .map(|sender| {
                            // Full means a wake-up is already pending.
                            let _ = sender.try_send(());
                        })
                        .is_some()
                });
            }
        }

        pub(crate) fn data_lost_signal(&self) -> DataLostSignal {
            let (sender, receiver) = channel::bounded(1);
            let sender = Arc::new(sender);
            let mut signals = lock(&self.data_lost_signals);
            signals.retain(|signal| signal.strong_count() > 0);
            signals.push(Arc::downgrade(&sender));
            DataLostSignal { receiver, _sender: sender }
        }

        /// The last 1101, if one arrived after `mark`.
        pub(crate) fn data_lost_since(&self, mark: FarmMark) -> Option<Notice> {
            lock(&self.state).data_lost_since(mark).cloned()
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

        /// Resolve with the first 1101 after `mark`; pending forever if none comes.
        pub(crate) async fn data_lost(&self, mark: FarmMark) -> Notice {
            let mut receiver = self.sender.subscribe();
            // The sender lives as long as `self`, so `wait_for` cannot fail.
            let notice = match receiver.wait_for(|state| state.data_lost_since(mark).is_some()).await {
                Ok(state) => state.data_lost_since(mark).cloned(),
                Err(_) => None,
            };
            match notice {
                Some(notice) => notice,
                None => std::future::pending().await,
            }
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
