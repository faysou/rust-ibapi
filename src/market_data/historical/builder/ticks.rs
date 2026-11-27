use time::OffsetDateTime;

use crate::contracts::Contract;
use crate::market_data::historical::common::tick_pages::PagedTick;
use crate::market_data::historical::{TickBidAsk, TickDecoder, TickLast, TickMidpoint, WhatToShow};
use crate::market_data::{IgnoreSize, TradingHours};
use crate::Error;

#[cfg(test)]
#[path = "ticks_tests.rs"]
mod tests;

/// Builder for the historical-ticks API.
///
/// IBKR requires at least one of [`starting`](Self::starting) /
/// [`ending`](Self::ending) to anchor the query. The builder does not
/// pre-flight this — the wire encoder accepts both unset, and IBKR
/// surfaces its own error if neither is provided.
///
/// The terminals ([`last`](Self::last), [`mid_point`](Self::mid_point),
/// [`bid_ask`](Self::bid_ask)) send the request and return the whole reply as
/// a `Vec`, oldest tick first as TWS sends them. TWS returns whole seconds, so
/// the `Vec` can be much longer than asked for, except that a
/// [`mid_point`](Self::mid_point) or [`bid_ask`](Self::bid_ask) reply holds
/// one tick of its earliest second; with [`starting`](Self::starting) that
/// tick is stamped before the start. TWS rejects a request for more than
/// 1000. For a contract without trade ticks, such as a forex pair, a spot
/// metal or an index CFD, TWS answers a trades request with midpoint ticks
/// and nothing more, and [`last`](Self::last) returns
/// [`Error::UnexpectedTickKind`]. Use [`mid_point`](Self::mid_point) or
/// [`bid_ask`](Self::bid_ask) there (live 2026-11-24, paper;
/// `docs/wire-facts/market-data.md#historical-ticks-of-another-kind`).
///
/// # Errors
///
/// Each terminal returns:
/// - the TWS rejection, as [`Error::Notice`], if TWS refuses the request.
/// - [`Error::UnexpectedEndOfStream`] if the connection closes before TWS
///   marks the reply complete.
/// - [`Error::Timeout`] if the reply isn't complete within the client's
///   `one_shot_timeout`.
/// - [`Error::UnexpectedTickKind`] if TWS answers with ticks of another kind.
///
/// Without a `one_shot_timeout`, a terminal waits without a time limit. An error before
/// the reply completes cancels the request where TWS supports historical tick cancellation.
#[must_use = "HistoricalTicksBuilder does nothing until you call .last(), .mid_point(), or .bid_ask(...)"]
pub struct HistoricalTicksBuilder<'a, C> {
    client: &'a C,
    contract: &'a Contract,
    number_of_ticks: i32,
    start: Option<OffsetDateTime>,
    end: Option<OffsetDateTime>,
    trading_hours: TradingHours,
}

impl<'a, C> HistoricalTicksBuilder<'a, C> {
    pub(crate) fn new(client: &'a C, contract: &'a Contract, number_of_ticks: i32) -> Self {
        Self {
            client,
            contract,
            number_of_ticks,
            start: None,
            end: None,
            trading_hours: TradingHours::Regular,
        }
    }

    /// Anchor the query at the start date (fetch forward in time).
    pub fn starting(mut self, start: OffsetDateTime) -> Self {
        self.start = Some(start);
        self
    }

    /// Anchor the query at the end date (fetch backward in time).
    ///
    /// TWS reads the end to the whole second and excludes that second: ending at 14:30:00.5
    /// returns ticks up to 14:29:59. Paging by hand with `ending()` therefore skips ticks at
    /// each quote page boundary. Use the client's `historical_ticks_between` to page a range.
    pub fn ending(mut self, end: OffsetDateTime) -> Self {
        self.end = Some(end);
        self
    }

    /// Override regular- vs extended-hours (defaults to `TradingHours::Regular`).
    pub fn trading_hours(mut self, trading_hours: TradingHours) -> Self {
        self.trading_hours = trading_hours;
        self
    }
}

/// Builder for historical ticks paged backward from an end time, returned by
/// the client's `historical_ticks_between` and `historical_ticks_latest` methods.
///
/// The terminals page backward and return ticks in `[start, end)`, or the latest ones before
/// `end` under a [`limit`](Self::limit), oldest first. A `start` not before `end` fails
/// with [`Error::InvalidArgument`] and nothing is sent.
///
/// For bid/ask and midpoint pages, TWS can send only one tick of the earliest second. The
/// pager fetches that second again on the next page. If the reply holds just one second, the
/// pager keeps it and advances to avoid repeating the same request. TWS does not confirm
/// completeness for a sole tick in that second. When it falls in the kept range,
/// [`PagingStop::UnverifiedQuoteSecond`] marks the gap in coverage.
///
///
/// A terminal returns [`TicksBetween`] once paging stops, and [`TicksBetweenError`] when a
/// request fails. Each page has its own `one_shot_timeout` when configured.
/// Resume a failed range from its original start up to the error's `from` when present.
/// Keep returned ticks at or after that boundary when joining the resumed range.
/// With no verified boundary, retry the original range and use the new result.
/// `?` converts the error into [`Error`], dropping the ticks it holds.
#[must_use = "HistoricalTicksBetweenBuilder does nothing until you call .last(), .mid_point(), or .bid_ask(...)"]
pub struct HistoricalTicksBetweenBuilder<'a, C> {
    client: &'a C,
    contract: &'a Contract,
    start: Option<OffsetDateTime>,
    end: OffsetDateTime,
    limit: Option<usize>,
    trading_hours: TradingHours,
}

impl<'a, C> HistoricalTicksBetweenBuilder<'a, C> {
    pub(crate) fn new(client: &'a C, contract: &'a Contract, start: Option<OffsetDateTime>, end: OffsetDateTime, limit: Option<usize>) -> Self {
        Self {
            client,
            contract,
            start,
            end,
            limit,
            trading_hours: TradingHours::Regular,
        }
    }

    /// Override regular- vs extended-hours (defaults to `TradingHours::Regular`).
    pub fn trading_hours(mut self, trading_hours: TradingHours) -> Self {
        self.trading_hours = trading_hours;
        self
    }

    /// Keep only the latest `limit` ticks, and stop paging once the pages hold that many.
    ///
    /// The ticks kept are whole seconds: the earliest second kept is never cut, so the result
    /// can hold more than `limit` ticks, by fewer than that second holds. [`TicksBetween::from`]
    /// is then the start of that second and the stop is [`PagingStop::LimitReached`]. A `limit`
    /// smaller than one second's ticks returns that whole second. A `limit` of 0 sends nothing
    /// and returns no ticks.
    ///
    /// # Examples
    #[cfg_attr(
        feature = "sync",
        doc = r#"
```no_run
use ibapi::client::blocking::Client;
use ibapi::contracts::Contract;
use time::macros::datetime;

let client = Client::connect("127.0.0.1:4002", 100).expect("connection failed");
let contract = Contract::stock("AAPL").build();

let trades = client
    .historical_ticks_between(&contract, datetime!(2026-11-13 14:30 UTC), datetime!(2026-11-13 21:00 UTC))
    .limit(2500)
    .last()
    .expect("historical ticks request failed");

println!("{} trades from {:?}, stopped at {:?}", trades.ticks.len(), trades.from, trades.stop);
```
"#
    )]
    #[cfg_attr(
        feature = "async",
        doc = r#"
```no_run
use ibapi::prelude::*;
use time::macros::datetime;

#[tokio::main]
async fn main() {
    let client = Client::connect("127.0.0.1:4002", 100).await.expect("connection failed");
    let contract = Contract::stock("AAPL").build();

    let trades = client
        .historical_ticks_between(&contract, datetime!(2026-11-13 14:30 UTC), datetime!(2026-11-13 21:00 UTC))
        .limit(2500)
        .last()
        .await
        .expect("historical ticks request failed");

    println!("{} trades from {:?}, stopped at {:?}", trades.ticks.len(), trades.from, trades.stop);
}
```
"#
    )]
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }
}

#[cfg(feature = "sync")]
impl HistoricalTicksBetweenBuilder<'_, crate::client::sync::Client> {
    /// Fetch the trade ticks in the range.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::client::blocking::Client;
    /// use ibapi::contracts::Contract;
    /// use time::macros::datetime;
    ///
    /// let client = Client::connect("127.0.0.1:4002", 100).expect("connection failed");
    /// let contract = Contract::stock("AAPL").build();
    ///
    /// let result = client
    ///     .historical_ticks_between(&contract, datetime!(2026-11-13 14:30 UTC), datetime!(2026-11-13 14:35 UTC))
    ///     .last();
    ///
    /// match result {
    ///     Ok(trades) => println!("{} trades, stopped at {:?}", trades.ticks.len(), trades.stop),
    ///     Err(e) => println!("{e}: kept {} trades complete from {:?}", e.ticks.len(), e.from),
    /// }
    /// ```
    pub fn last(self) -> Result<TicksBetween<TickLast>, TicksBetweenError<TickLast>> {
        self.fetch(WhatToShow::Trades, false)
    }

    /// Fetch the midpoint ticks in the range.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::client::blocking::Client;
    /// use ibapi::contracts::Contract;
    /// use time::macros::datetime;
    ///
    /// let client = Client::connect("127.0.0.1:4002", 100).expect("connection failed");
    /// let contract = Contract::stock("AAPL").build();
    ///
    /// let ticks = client
    ///     .historical_ticks_between(&contract, datetime!(2026-11-13 14:30 UTC), datetime!(2026-11-13 14:35 UTC))
    ///     .mid_point()
    ///     .expect("historical ticks request failed")
    ///     .ticks;
    ///
    /// println!("{} midpoints", ticks.len());
    /// ```
    pub fn mid_point(self) -> Result<TicksBetween<TickMidpoint>, TicksBetweenError<TickMidpoint>> {
        self.fetch(WhatToShow::MidPoint, false)
    }

    /// Fetch the bid/ask ticks in the range. Pass [`IgnoreSize::Yes`] to skip updates that
    /// change only a size; see [`IgnoreSize`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::client::blocking::Client;
    /// use ibapi::contracts::Contract;
    /// use ibapi::market_data::IgnoreSize;
    /// use time::macros::datetime;
    ///
    /// let client = Client::connect("127.0.0.1:4002", 100).expect("connection failed");
    /// let contract = Contract::stock("AAPL").build();
    ///
    /// let ticks = client
    ///     .historical_ticks_between(&contract, datetime!(2026-11-13 14:30 UTC), datetime!(2026-11-13 14:35 UTC))
    ///     .bid_ask(IgnoreSize::No)
    ///     .expect("historical ticks request failed")
    ///     .ticks;
    ///
    /// println!("{} quotes", ticks.len());
    /// ```
    pub fn bid_ask(self, ignore_size: IgnoreSize) -> Result<TicksBetween<TickBidAsk>, TicksBetweenError<TickBidAsk>> {
        self.fetch(WhatToShow::BidAsk, matches!(ignore_size, IgnoreSize::Yes))
    }

    fn fetch<T: TickDecoder<T> + PagedTick>(self, what_to_show: WhatToShow, ignore_size: bool) -> Result<TicksBetween<T>, TicksBetweenError<T>> {
        crate::market_data::historical::sync::historical_ticks_between::<T>(
            self.client,
            self.contract,
            self.start,
            self.end,
            self.limit,
            what_to_show,
            self.trading_hours,
            ignore_size,
        )
    }
}

#[cfg(feature = "async")]
impl HistoricalTicksBetweenBuilder<'_, crate::client::r#async::Client> {
    /// Fetch the trade ticks in the range.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::prelude::*;
    /// use time::macros::datetime;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let client = Client::connect("127.0.0.1:4002", 100).await.expect("connection failed");
    ///     let contract = Contract::stock("AAPL").build();
    ///
    ///     let result = client
    ///         .historical_ticks_between(&contract, datetime!(2026-11-13 14:30 UTC), datetime!(2026-11-13 14:35 UTC))
    ///         .last()
    ///         .await;
    ///
    ///     match result {
    ///         Ok(trades) => println!("{} trades, stopped at {:?}", trades.ticks.len(), trades.stop),
    ///         Err(e) => println!("{e}: kept {} trades complete from {:?}", e.ticks.len(), e.from),
    ///     }
    /// }
    /// ```
    pub async fn last(self) -> Result<TicksBetween<TickLast>, TicksBetweenError<TickLast>> {
        self.fetch(WhatToShow::Trades, false).await
    }

    /// Fetch the midpoint ticks in the range.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::prelude::*;
    /// use time::macros::datetime;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let client = Client::connect("127.0.0.1:4002", 100).await.expect("connection failed");
    ///     let contract = Contract::stock("AAPL").build();
    ///
    ///     let ticks = client
    ///         .historical_ticks_between(&contract, datetime!(2026-11-13 14:30 UTC), datetime!(2026-11-13 14:35 UTC))
    ///         .mid_point()
    ///         .await
    ///         .expect("historical ticks request failed")
    ///         .ticks;
    ///
    ///     println!("{} midpoints", ticks.len());
    /// }
    /// ```
    pub async fn mid_point(self) -> Result<TicksBetween<TickMidpoint>, TicksBetweenError<TickMidpoint>> {
        self.fetch(WhatToShow::MidPoint, false).await
    }

    /// Fetch the bid/ask ticks in the range. Pass [`IgnoreSize::Yes`] to skip updates that
    /// change only a size; see [`IgnoreSize`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::prelude::*;
    /// use ibapi::market_data::IgnoreSize;
    /// use time::macros::datetime;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let client = Client::connect("127.0.0.1:4002", 100).await.expect("connection failed");
    ///     let contract = Contract::stock("AAPL").build();
    ///
    ///     let ticks = client
    ///         .historical_ticks_between(&contract, datetime!(2026-11-13 14:30 UTC), datetime!(2026-11-13 14:35 UTC))
    ///         .bid_ask(IgnoreSize::No)
    ///         .await
    ///         .expect("historical ticks request failed")
    ///         .ticks;
    ///
    ///     println!("{} quotes", ticks.len());
    /// }
    /// ```
    pub async fn bid_ask(self, ignore_size: IgnoreSize) -> Result<TicksBetween<TickBidAsk>, TicksBetweenError<TickBidAsk>> {
        self.fetch(WhatToShow::BidAsk, matches!(ignore_size, IgnoreSize::Yes)).await
    }

    async fn fetch<T: TickDecoder<T> + PagedTick + Send + 'static>(
        self,
        what_to_show: WhatToShow,
        ignore_size: bool,
    ) -> Result<TicksBetween<T>, TicksBetweenError<T>> {
        crate::market_data::historical::r#async::historical_ticks_between::<T>(
            self.client,
            self.contract,
            self.start,
            self.end,
            self.limit,
            what_to_show,
            self.trading_hours,
            ignore_size,
        )
        .await
    }
}

/// The ticks a [`HistoricalTicksBetweenBuilder`] terminal fetched, and how the paging stopped.
#[derive(Debug, Clone, PartialEq)]
pub struct TicksBetween<T> {
    /// The ticks in `[start, end)`, or all ticks before `end` without a start, oldest first.
    /// Under a limit, the latest seconds that cover it; see
    /// [`HistoricalTicksBetweenBuilder::limit`].
    pub ticks: Vec<T>,
    /// The start of the covered window ending before `end`. This is `start` when paging
    /// reached it, the earliest second kept when the limit dropped older ticks, or the earliest
    /// second fetched. It is `None` when no contiguous coverage is verified. See
    /// [`HistoricalTicksBetweenBuilder`] for the single-second quote caveat.
    pub from: Option<OffsetDateTime>,
    /// Why paging stopped.
    pub stop: PagingStop,
}

/// Why a [`HistoricalTicksBetweenBuilder`] terminal stopped paging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PagingStop {
    /// A page reached back to `start` and the ticks cover `[start, end)`. A range with no trading,
    /// such as a weekend, ends here with no ticks, as TWS answers with earlier ticks.
    ReachedStart,
    /// TWS returned no earlier ticks for the selected trading hours.
    EmptyPage,
    /// The pages held the [`limit`](HistoricalTicksBetweenBuilder::limit) of ticks. Older ticks
    /// may exist before [`TicksBetween::from`]. Also reported when the pages reached `start` but
    /// the limit dropped ticks after it.
    LimitReached,
    /// The kept range contains a quote second for which TWS returned a single tick
    /// and no other seconds in the reply. [`TicksBetween::from`] covers only the
    /// contiguous data after that second, if any. Earlier ticks remain in the result.
    UnverifiedQuoteSecond,
}

/// A failed request in a [`HistoricalTicksBetweenBuilder`] terminal, with the ticks fetched
/// before it.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{error} ({} ticks fetched before the failed request)", .ticks.len())]
pub struct TicksBetweenError<T> {
    /// Why the request failed.
    pub error: Error,
    /// Ticks collected before the failure, oldest first. Fewer than the limit, if set.
    pub ticks: Vec<T>,
    /// Start of the verified coverage through the exclusive end, if any. See
    /// [`HistoricalTicksBetweenBuilder`] for the single-second quote caveat.
    pub from: Option<OffsetDateTime>,
}

impl<T> From<TicksBetweenError<T>> for Error {
    fn from(error: TicksBetweenError<T>) -> Self {
        error.error
    }
}

#[cfg(feature = "sync")]
impl<'a> HistoricalTicksBuilder<'a, crate::client::sync::Client> {
    /// Submit the request and return trade ticks.
    ///
    /// # Errors
    ///
    /// See [the builder's errors](HistoricalTicksBuilder#errors).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::client::blocking::Client;
    /// use ibapi::contracts::Contract;
    /// use time::macros::datetime;
    ///
    /// let client = Client::connect("127.0.0.1:4002", 100).expect("connection failed");
    /// let contract = Contract::stock("TSLA").build();
    ///
    /// let ticks = client
    ///     .historical_ticks(&contract, 100)
    ///     .starting(datetime!(2023-04-15 0:00 UTC))
    ///     .last()
    ///     .expect("historical ticks request failed");
    ///
    /// for tick in &ticks {
    ///     println!("{tick:?}");
    /// }
    /// ```
    pub fn last(self) -> Result<Vec<TickLast>, Error> {
        crate::market_data::historical::sync::historical_ticks::<TickLast>(
            self.client,
            self.contract,
            self.start,
            self.end,
            self.number_of_ticks,
            WhatToShow::Trades,
            self.trading_hours,
            false,
        )
    }

    /// Submit the request and return midpoint ticks.
    ///
    /// # Errors
    ///
    /// See [the builder's errors](HistoricalTicksBuilder#errors).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::client::blocking::Client;
    /// use ibapi::contracts::Contract;
    /// use time::macros::datetime;
    ///
    /// let client = Client::connect("127.0.0.1:4002", 100).expect("connection failed");
    /// let contract = Contract::stock("TSLA").build();
    ///
    /// let ticks = client
    ///     .historical_ticks(&contract, 100)
    ///     .ending(datetime!(2023-04-15 0:00 UTC))
    ///     .mid_point()
    ///     .expect("historical ticks request failed");
    ///
    /// for tick in &ticks {
    ///     println!("{tick:?}");
    /// }
    /// ```
    pub fn mid_point(self) -> Result<Vec<TickMidpoint>, Error> {
        crate::market_data::historical::sync::historical_ticks::<TickMidpoint>(
            self.client,
            self.contract,
            self.start,
            self.end,
            self.number_of_ticks,
            WhatToShow::MidPoint,
            self.trading_hours,
            false,
        )
    }

    /// Submit the request and return bid/ask ticks. Pass [`IgnoreSize::Yes`]
    /// to skip updates that change only a size; see [`IgnoreSize`].
    ///
    /// # Errors
    ///
    /// See [the builder's errors](HistoricalTicksBuilder#errors).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::client::blocking::Client;
    /// use ibapi::contracts::Contract;
    /// use ibapi::market_data::IgnoreSize;
    /// use time::macros::datetime;
    ///
    /// let client = Client::connect("127.0.0.1:4002", 100).expect("connection failed");
    /// let contract = Contract::stock("TSLA").build();
    ///
    /// let ticks = client
    ///     .historical_ticks(&contract, 100)
    ///     .starting(datetime!(2023-04-15 0:00 UTC))
    ///     .bid_ask(IgnoreSize::No)
    ///     .expect("historical ticks request failed");
    ///
    /// for tick in &ticks {
    ///     println!("{tick:?}");
    /// }
    /// ```
    pub fn bid_ask(self, ignore_size: IgnoreSize) -> Result<Vec<TickBidAsk>, Error> {
        crate::market_data::historical::sync::historical_ticks::<TickBidAsk>(
            self.client,
            self.contract,
            self.start,
            self.end,
            self.number_of_ticks,
            WhatToShow::BidAsk,
            self.trading_hours,
            matches!(ignore_size, IgnoreSize::Yes),
        )
    }
}

#[cfg(feature = "async")]
impl<'a> HistoricalTicksBuilder<'a, crate::client::r#async::Client> {
    /// Submit the request and return trade ticks.
    ///
    /// # Errors
    ///
    /// See [the builder's errors](HistoricalTicksBuilder#errors).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::prelude::*;
    /// use time::macros::datetime;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let client = Client::connect("127.0.0.1:4002", 100).await.expect("connection failed");
    ///     let contract = Contract::stock("TSLA").build();
    ///
    ///     let ticks = client
    ///         .historical_ticks(&contract, 100)
    ///         .starting(datetime!(2023-04-15 0:00 UTC))
    ///         .last()
    ///         .await
    ///         .expect("historical ticks request failed");
    ///
    ///     for tick in &ticks {
    ///         println!("{tick:?}");
    ///     }
    /// }
    /// ```
    pub async fn last(self) -> Result<Vec<TickLast>, Error> {
        crate::market_data::historical::r#async::historical_ticks::<TickLast>(
            self.client,
            self.contract,
            self.start,
            self.end,
            self.number_of_ticks,
            WhatToShow::Trades,
            self.trading_hours,
            false,
        )
        .await
    }

    /// Submit the request and return midpoint ticks.
    ///
    /// # Errors
    ///
    /// See [the builder's errors](HistoricalTicksBuilder#errors).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::prelude::*;
    /// use time::macros::datetime;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let client = Client::connect("127.0.0.1:4002", 100).await.expect("connection failed");
    ///     let contract = Contract::stock("TSLA").build();
    ///
    ///     let ticks = client
    ///         .historical_ticks(&contract, 100)
    ///         .ending(datetime!(2023-04-15 0:00 UTC))
    ///         .mid_point()
    ///         .await
    ///         .expect("historical ticks request failed");
    ///
    ///     for tick in &ticks {
    ///         println!("{tick:?}");
    ///     }
    /// }
    /// ```
    pub async fn mid_point(self) -> Result<Vec<TickMidpoint>, Error> {
        crate::market_data::historical::r#async::historical_ticks::<TickMidpoint>(
            self.client,
            self.contract,
            self.start,
            self.end,
            self.number_of_ticks,
            WhatToShow::MidPoint,
            self.trading_hours,
            false,
        )
        .await
    }

    /// Submit the request and return bid/ask ticks. Pass [`IgnoreSize::Yes`]
    /// to skip updates that change only a size; see [`IgnoreSize`].
    ///
    /// # Errors
    ///
    /// See [the builder's errors](HistoricalTicksBuilder#errors).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::prelude::*;
    /// use ibapi::market_data::IgnoreSize;
    /// use time::macros::datetime;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let client = Client::connect("127.0.0.1:4002", 100).await.expect("connection failed");
    ///     let contract = Contract::stock("TSLA").build();
    ///
    ///     let ticks = client
    ///         .historical_ticks(&contract, 100)
    ///         .starting(datetime!(2023-04-15 0:00 UTC))
    ///         .bid_ask(IgnoreSize::No)
    ///         .await
    ///         .expect("historical ticks request failed");
    ///
    ///     for tick in &ticks {
    ///         println!("{tick:?}");
    ///     }
    /// }
    /// ```
    pub async fn bid_ask(self, ignore_size: IgnoreSize) -> Result<Vec<TickBidAsk>, Error> {
        crate::market_data::historical::r#async::historical_ticks::<TickBidAsk>(
            self.client,
            self.contract,
            self.start,
            self.end,
            self.number_of_ticks,
            WhatToShow::BidAsk,
            self.trading_hours,
            matches!(ignore_size, IgnoreSize::Yes),
        )
        .await
    }
}
