//! Resubscribe a stream across transport reconnects: the packaged form of the
//! `is_connection_lost` → `wait_connected` → resubscribe loop. Also reopens a
//! keep-up-to-date bar request that 10182 ended once the historical-data farms
//! are back, and any stream after a 1101 (connectivity restored, data lost).
//!
//! Entry points are `Client::resubscribing` (sync and async). Only types marked
//! [`Resubscribable`] are accepted, so the helper can't re-send a request that
//! places, cancels or exercises.

use std::time::Duration;

use crate::errors::Error;
use crate::messages::{resubscribed_notice, Notice, SUBSCRIPTION_DISCONNECTED_CODE};
use crate::subscriptions::{StreamDecoder, SubscriptionItem};
use crate::transport::farm_watch::{FarmMark, FarmRecovery};

/// The longest wait for the historical-data farms after a 10182 before
/// reopening anyway, in case TWS never reports the farm back. Reopening early
/// is harmless: TWS holds a request sent during an outage and answers it once
/// the farm is back, and a 10182 on it starts another wait. Live, the farm
/// came back 2 s to 6 min after the 10182, with the outage.
const FARM_RECOVERY_LIMIT: Duration = Duration::from_secs(15 * 60);

/// Stream item types that are safe to resubscribe after a reconnect.
///
/// Sealed. The accepted streams, and what each repeats after a resubscribe, are
/// listed on `Client::resubscribing`. Order types are excluded (a resubscribe
/// would place, cancel or exercise again), and so are requests that end on their
/// own (contract details, option chains). Historical bars are the exception:
/// `subscribe` shares [`HistoricalDataFrame`](crate::market_data::historical::HistoricalDataFrame)
/// with `subscribe_with_updates`, so a `subscribe` cut by a reconnect before
/// `End` is sent again from the start; the stream ends after `End`. Display
/// groups are left out: their
/// `DisplayGroupSubscription` wrapper isn't a `Subscription`.
#[allow(private_bounds)]
pub trait Resubscribable: StreamDecoder<Self> + Sized {}

macro_rules! resubscribable {
    ($($ty:ty),* $(,)?) => {
        $(impl Resubscribable for $ty {})*
    };
}

resubscribable!(
    crate::market_data::realtime::TickTypes,
    crate::market_data::realtime::Bar,
    crate::market_data::TickLast,
    crate::market_data::TickBidAsk,
    crate::market_data::TickMidpoint,
    crate::market_data::realtime::MarketDepths,
    crate::market_data::historical::HistoricalDataFrame,
    crate::accounts::AccountUpdate,
    crate::accounts::AccountUpdateMulti,
    crate::accounts::AccountSummaryResult,
    crate::accounts::PositionUpdate,
    crate::accounts::PositionUpdateMulti,
    crate::accounts::PnL,
    crate::accounts::PnLSingle,
    Vec<crate::scanner::ScannerData>,
    crate::news::NewsBulletin,
);

/// Where the loop is. Shared by the sync iterator and the async stream.
enum State<S> {
    /// Opening a subscription; `gap` is set when it replaces one the reconnect killed.
    Opening {
        gap: bool,
    },
    /// Reading `S`, whose request went out after `FarmMark`: a 1101 since voids it.
    Reading(S, FarmMark),
    /// A 10182 ended the request: waiting for the historical-data farms from `mark`.
    Waiting(FarmMark),
    Done,
}

/// Whether `error` is the 10182 TWS sends when it ends a keep-up-to-date bar
/// request because its historical-data farm broke.
fn farm_disconnected(error: &Error) -> Option<&Notice> {
    match error {
        Error::Notice(notice) if notice.code == SUBSCRIPTION_DISCONNECTED_CODE => Some(notice),
        _ => None,
    }
}

/// What to do with an `open()` result.
enum Opened<S> {
    Subscribed(S),
    /// Connection lost before the request went out: wait, then open again.
    Lost,
    Failed(Error),
}

impl<S> From<Result<S, Error>> for Opened<S> {
    fn from(result: Result<S, Error>) -> Self {
        match result {
            Ok(subscription) => Opened::Subscribed(subscription),
            Err(e) if e.is_connection_lost() => Opened::Lost,
            Err(e) => Opened::Failed(e),
        }
    }
}

/// Marks where a resubscribe replaced a subscription the reconnect killed.
fn gap_notice<T>() -> SubscriptionItem<T> {
    SubscriptionItem::Notice(resubscribed_notice())
}

#[cfg(feature = "sync")]
pub use sync_impl::Resubscribing;

#[cfg(feature = "sync")]
mod sync_impl {
    use std::time::Instant;

    use super::*;
    use crate::client::sync::Client;
    use crate::subscriptions::sync::Subscription;
    use crate::transport::farm_watch::sync_impl::DataLostSignal;

    /// Blocking iterator returned by
    /// [`Client::resubscribing`](crate::client::blocking::Client::resubscribing).
    #[must_use = "iterators are lazy and do nothing unless consumed"]
    pub struct Resubscribing<'a, T: Resubscribable, F> {
        client: &'a Client,
        open: F,
        state: State<Subscription<T>>,
        /// Wakes a read blocked on the subscription when a 1101 arrives.
        data_lost: DataLostSignal,
    }

    impl<'a, T: Resubscribable, F> Resubscribing<'a, T, F> {
        pub(crate) fn new(client: &'a Client, open: F) -> Self {
            Self {
                client,
                open,
                state: State::Opening { gap: false },
                data_lost: client.message_bus.farm_watch().data_lost_signal(),
            }
        }
    }

    impl<T, F> Iterator for Resubscribing<'_, T, F>
    where
        T: Resubscribable,
        F: FnMut() -> Result<Subscription<T>, Error>,
    {
        type Item = Result<SubscriptionItem<T>, Error>;

        fn next(&mut self) -> Option<Self::Item> {
            loop {
                // Done unless an arm puts a live state back.
                match std::mem::replace(&mut self.state, State::Done) {
                    State::Done => return None,
                    State::Opening { gap } => {
                        // Before the request goes out, so a 1101 while it does counts.
                        let mark = self.client.message_bus.farm_watch().mark();
                        match Opened::from((self.open)()) {
                            Opened::Subscribed(subscription) => {
                                self.state = State::Reading(subscription, mark);
                                if gap {
                                    return Some(Ok(gap_notice()));
                                }
                            }
                            Opened::Lost => {
                                if let Err(e) = self.client.wait_connected() {
                                    return Some(Err(e));
                                }
                                self.state = State::Opening { gap };
                            }
                            Opened::Failed(e) => return Some(Err(e)),
                        }
                    }
                    State::Reading(subscription, mark) => {
                        if let Some(notice) = self.client.message_bus.farm_watch().data_lost_since(mark) {
                            // Written on this thread, so before the reopen's request.
                            subscription.cancel();
                            self.state = State::Opening { gap: false };
                            return Some(Ok(SubscriptionItem::Notice(notice)));
                        }
                        let Some(next) = subscription.next_unless(self.data_lost.receiver()) else {
                            self.state = State::Reading(subscription, mark);
                            continue;
                        };
                        match next {
                            Some(Err(e)) if e.is_connection_lost() => {
                                // Its request died with the old session; dropping it sends no cancel.
                                drop(subscription);
                                if let Err(e) = self.client.wait_connected() {
                                    return Some(Err(e));
                                }
                                self.state = State::Opening { gap: true };
                            }
                            Some(Ok(item)) => {
                                // A finished snapshot sends nothing more, and its
                                // channel stays open: end here rather than wait,
                                // and resend it after a later reconnect.
                                if !subscription.snapshot_ended() {
                                    self.state = State::Reading(subscription, mark);
                                }
                                return Some(Ok(item));
                            }
                            Some(Err(e)) => match farm_disconnected(&e) {
                                Some(notice) => {
                                    let notice = notice.clone();
                                    drop(subscription);
                                    self.state = State::Waiting(self.client.message_bus.farm_watch().mark());
                                    return Some(Ok(SubscriptionItem::Notice(notice)));
                                }
                                None => return Some(Err(e)),
                            },
                            None => return None,
                        }
                    }
                    State::Waiting(mark) => {
                        let deadline = Instant::now() + FARM_RECOVERY_LIMIT;
                        match self.client.message_bus.farm_watch().wait(mark, deadline) {
                            FarmRecovery::Recovered | FarmRecovery::TimedOut => self.state = State::Opening { gap: false },
                            FarmRecovery::SessionEnded => {
                                if let Err(e) = self.client.wait_connected() {
                                    return Some(Err(e));
                                }
                                self.state = State::Opening { gap: true };
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(feature = "async")]
pub(crate) use async_impl::resubscribing;

#[cfg(feature = "async")]
mod async_impl {
    use std::future::Future;

    use futures::{Stream, StreamExt};

    use super::*;
    use crate::client::r#async::Client;
    use crate::subscriptions::r#async::Subscription;

    /// The stream behind the async `Client::resubscribing`.
    pub(crate) fn resubscribing<'a, T, F, Fut>(client: &'a Client, open: F) -> impl Stream<Item = Result<SubscriptionItem<T>, Error>> + 'a
    where
        T: Resubscribable + Send + 'static,
        F: FnMut() -> Fut + 'a,
        Fut: Future<Output = Result<Subscription<T>, Error>> + 'a,
    {
        futures::stream::unfold((State::Opening { gap: false }, open), move |(mut state, mut open)| async move {
            loop {
                match state {
                    State::Done => return None,
                    State::Opening { gap } => {
                        // Before the request goes out, so a 1101 while it does counts.
                        let mark = client.message_bus.farm_watch().mark();
                        match Opened::from(open().await) {
                            Opened::Subscribed(subscription) => {
                                state = State::Reading(subscription, mark);
                                if gap {
                                    return Some((Ok(gap_notice()), (state, open)));
                                }
                            }
                            Opened::Lost => {
                                if let Err(e) = client.wait_connected().await {
                                    return Some((Err(e), (State::Done, open)));
                                }
                                // `state` is still `Opening { gap }`: the match only copied `gap`.
                            }
                            Opened::Failed(e) => return Some((Err(e), (State::Done, open))),
                        }
                    }
                    State::Reading(mut subscription, mark) => {
                        let next = tokio::select! {
                            // A 1101 first: the old request's queued items are void with it.
                            biased;
                            notice = client.message_bus.farm_watch().data_lost(mark) => Err(notice),
                            item = subscription.next() => Ok(item),
                        };
                        let next = match next {
                            Ok(next) => next,
                            Err(notice) => {
                                // Awaited: a dropped subscription's cancel is spawned, and
                                // could reach TWS after the reopen's request.
                                subscription.cancel().await;
                                return Some((Ok(SubscriptionItem::Notice(notice)), (State::Opening { gap: false }, open)));
                            }
                        };
                        match next {
                            Some(Err(e)) if e.is_connection_lost() => {
                                // Its request died with the old session; dropping it sends no cancel.
                                drop(subscription);
                                if let Err(e) = client.wait_connected().await {
                                    return Some((Err(e), (State::Done, open)));
                                }
                                state = State::Opening { gap: true };
                            }
                            Some(Ok(item)) => {
                                // As in the sync iterator: a finished snapshot ends the stream.
                                let next = if subscription.snapshot_ended() {
                                    State::Done
                                } else {
                                    State::Reading(subscription, mark)
                                };
                                return Some((Ok(item), (next, open)));
                            }
                            Some(Err(e)) => match farm_disconnected(&e) {
                                Some(notice) => {
                                    let notice = notice.clone();
                                    drop(subscription);
                                    let waiting = State::Waiting(client.message_bus.farm_watch().mark());
                                    return Some((Ok(SubscriptionItem::Notice(notice)), (waiting, open)));
                                }
                                None => return Some((Err(e), (State::Done, open))),
                            },
                            None => return None,
                        }
                    }
                    State::Waiting(mark) => {
                        let deadline = tokio::time::Instant::now() + FARM_RECOVERY_LIMIT;
                        state = match client.message_bus.farm_watch().wait(mark, deadline).await {
                            FarmRecovery::Recovered | FarmRecovery::TimedOut => State::Opening { gap: false },
                            FarmRecovery::SessionEnded => {
                                if let Err(e) = client.wait_connected().await {
                                    return Some((Err(e), (State::Done, open)));
                                }
                                State::Opening { gap: true }
                            }
                        };
                    }
                }
            }
        })
    }
}

#[cfg(test)]
#[path = "resubscribe_tests.rs"]
mod tests;
