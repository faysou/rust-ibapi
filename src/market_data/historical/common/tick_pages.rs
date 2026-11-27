//! Shared paging for historical ticks backward from an end time.
//!
//! TWS answers a historical ticks request ending at `end` with ticks before `end`, read at whole
//! seconds: a fractional `end` is truncated and the end second itself is excluded. A page asked
//! for `n` ticks holds whole seconds, so it can exceed `n`, except at its earliest second: a trade
//! page holds every trade of that second, but a bid/ask or midpoint page holds only one of its
//! ticks. [`TickPages`] pages backward under those rules: each request ends where the previous
//! page's complete seconds begin. A single-second quote page is kept as returned so paging
//! advances. Paging stops at `start`,
//! at an empty page, or once a limit of ticks is collected. See the wire facts in
//! `docs/wire-facts/market-data.md`.

use std::ops::ControlFlow;

use time::{Duration, OffsetDateTime};

use crate::market_data::historical::{PagingStop, TicksBetween, TicksBetweenError};
use crate::market_data::{TickBidAsk, TickLast, TickMidpoint};
use crate::Error;

pub(crate) const PAGE_SIZE: i32 = 1_000;

/// A historical tick type, as [`TickPages`] pages it.
pub(crate) trait PagedTick {
    /// Whether a page holds every tick of its earliest second.
    const PAGE_HOLDS_EARLIEST_SECOND: bool;

    fn time(&self) -> OffsetDateTime;
}

impl PagedTick for TickLast {
    const PAGE_HOLDS_EARLIEST_SECOND: bool = true;

    fn time(&self) -> OffsetDateTime {
        self.time
    }
}

impl PagedTick for TickBidAsk {
    const PAGE_HOLDS_EARLIEST_SECOND: bool = false;

    fn time(&self) -> OffsetDateTime {
        self.time
    }
}

impl PagedTick for TickMidpoint {
    const PAGE_HOLDS_EARLIEST_SECOND: bool = false;

    fn time(&self) -> OffsetDateTime {
        self.time
    }
}

/// The pages of ticks fetched backward from `end`, back to `start` when there is one, and the end
/// of the next request.
pub(crate) struct TickPages<T> {
    start: Option<OffsetDateTime>,
    end: OffsetDateTime,
    limit: Option<usize>,
    request_end: OffsetDateTime,
    unverified_before: Option<OffsetDateTime>,
    pages: Vec<Vec<T>>,
    /// The ticks in the pages that fall in the range, which `limit` counts.
    collected: usize,
}

impl<T: PagedTick> TickPages<T> {
    /// Starts paging backward from `end`, back to `start` when given, until `limit` ticks are
    /// collected when given. Round a fractional end up for the wire and filter ticks
    /// against the caller's exact exclusive bound.
    pub(crate) fn new(start: Option<OffsetDateTime>, end: OffsetDateTime, limit: Option<usize>) -> Result<Self, TicksBetweenError<T>> {
        if start.is_some_and(|start| end <= start) {
            return Err(TicksBetweenError {
                error: Error::InvalidArgument("historical_ticks_between requires end > start".to_owned()),
                ticks: Vec::new(),
                from: None,
            });
        }

        let request_end = if end.nanosecond() == 0 || limit == Some(0) {
            end
        } else {
            whole_second(end).checked_add(Duration::SECOND).ok_or_else(|| TicksBetweenError {
                error: Error::InvalidArgument("historical ticks end exceeds the last whole second".to_owned()),
                ticks: Vec::new(),
                from: None,
            })?
        };

        Ok(Self {
            start,
            end,
            limit,
            request_end,
            unverified_before: None,
            pages: Vec::new(),
            collected: 0,
        })
    }

    /// Whether the pages hold the limit of ticks, so no further request is needed.
    pub(crate) fn limit_reached(&self) -> bool {
        self.limit.is_some_and(|limit| self.collected >= limit)
    }

    /// The end of the next request.
    pub(crate) fn request_end(&self) -> OffsetDateTime {
        self.request_end
    }

    /// Adds the ticks a request ending at [`request_end`](Self::request_end) returned, and
    /// returns whether another request is needed or why paging stops.
    ///
    /// The page keeps its complete seconds: ticks at or after the request end are dropped, and so
    /// is a partial earliest second, which the next request fetches whole. Paging stops at a page
    /// that holds no ticks or whose complete seconds reach back to `start`.
    pub(crate) fn push(&mut self, page: Vec<T>) -> ControlFlow<PagingStop> {
        let Some(earliest) = page.iter().map(PagedTick::time).filter(|time| *time < self.request_end).min() else {
            return ControlFlow::Break(PagingStop::EmptyPage);
        };

        let earliest_second = whole_second(earliest);
        let mut complete_from = if T::PAGE_HOLDS_EARLIEST_SECOND {
            earliest_second
        } else {
            earliest_second + Duration::SECOND
        };
        // Keep a sole quote second to advance the pager. A single tick cannot prove it is whole.
        if complete_from >= self.request_end {
            if in_range(self.start, self.end, earliest) && page.iter().filter(|tick| tick.time() < self.request_end).take(2).count() == 1 {
                self.unverified_before.get_or_insert(self.request_end);
            }
            complete_from = earliest_second;
        }

        let (from, to) = (complete_from, self.request_end);
        let page: Vec<T> = page.into_iter().filter(|tick| (from..to).contains(&tick.time())).collect();
        self.collected += page.iter().filter(|tick| in_range(self.start, self.end, tick.time())).count();
        self.pages.push(page);

        if self.start.is_some_and(|start| complete_from <= start) {
            return ControlFlow::Break(PagingStop::ReachedStart);
        }

        self.request_end = complete_from;
        ControlFlow::Continue(())
    }

    /// The ticks fetched once paging stopped for `stop`.
    ///
    /// Over the limit, only the latest whole seconds holding the limit are kept: the earliest one
    /// kept is never cut, so the ticks can exceed the limit by fewer than that second holds. The
    /// stop is then [`PagingStop::LimitReached`], even when the pages reached `start`.
    pub(crate) fn finish(self, mut stop: PagingStop) -> TicksBetween<T> {
        let mut from = match stop {
            PagingStop::ReachedStart => self.start,
            PagingStop::EmptyPage | PagingStop::LimitReached | PagingStop::UnverifiedQuoteSecond => self.fetched_from(),
        };
        let limit = self.limit;
        let end = self.end;
        let unverified_before = self.unverified_before;
        let mut ticks = self.into_ticks();

        if let Some(limit) = limit.filter(|limit| (1..=ticks.len()).contains(limit)) {
            let kept_from = whole_second(ticks[ticks.len() - limit].time());
            let dropped = ticks.partition_point(|tick| tick.time() < kept_from);
            if dropped > 0 {
                ticks.drain(..dropped);
                from = Some(kept_from);
                stop = PagingStop::LimitReached;
            }
        }

        let verified_from = covered_from(from, unverified_before, end);
        if verified_from != from {
            stop = PagingStop::UnverifiedQuoteSecond;
        }
        TicksBetween {
            ticks,
            from: verified_from,
            stop,
        }
    }

    /// The ticks fetched before a request failed with `error`.
    pub(crate) fn fail(self, error: Error) -> TicksBetweenError<T> {
        TicksBetweenError {
            from: covered_from(self.fetched_from(), self.unverified_before, self.end),
            ticks: self.into_ticks(),
            error,
        }
    }

    // Each page that leaves the pager running moves the next request end back.
    fn fetched_from(&self) -> Option<OffsetDateTime> {
        (!self.pages.is_empty()).then_some(self.request_end)
    }

    /// The ticks in the range, oldest first.
    fn into_ticks(self) -> Vec<T> {
        let Self { start, end, pages, .. } = self;
        pages
            .into_iter()
            .rev()
            .flatten()
            .filter(|tick| in_range(start, end, tick.time()))
            .collect()
    }
}

// Coverage resumes after the most recent uncertain second, unless the kept range
// already starts after it. Older ticks remain available as observations.
fn covered_from(from: Option<OffsetDateTime>, unverified_before: Option<OffsetDateTime>, end: OffsetDateTime) -> Option<OffsetDateTime> {
    let from = from?;
    match unverified_before {
        Some(boundary) if from < boundary => (boundary < end).then_some(boundary),
        _ => Some(from),
    }
}

fn in_range(start: Option<OffsetDateTime>, end: OffsetDateTime, time: OffsetDateTime) -> bool {
    time < end && start.is_none_or(|start| time >= start)
}

fn whole_second(time: OffsetDateTime) -> OffsetDateTime {
    time.replace_nanosecond(0).expect("zero is a valid nanosecond")
}

#[cfg(test)]
#[path = "tick_pages_tests.rs"]
mod tests;
