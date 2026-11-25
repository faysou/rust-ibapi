use crate::market_data::{TickAttributeBidAsk, TickAttributeLast, TickBidAsk, TickLast, TickMidpoint};
use crate::proto;
use crate::proto::decoders::{parse_decimal_or_zero, parse_optional_decimal, ts};
use crate::Error;

pub(in crate::market_data) fn decode_tick_last(t: proto::HistoricalTickLast) -> Result<TickLast, Error> {
    let attr = t.tick_attrib_last.as_ref();
    Ok(TickLast {
        time: ts(t.time.unwrap_or_default()),
        price: t.price.unwrap_or_default(),
        size: parse_decimal_or_zero(t.size.as_deref())?,
        attributes: TickAttributeLast {
            past_limit: attr.and_then(|a| a.past_limit).unwrap_or_default(),
            unreported: attr.and_then(|a| a.unreported).unwrap_or_default(),
        },
        exchange: t.exchange.unwrap_or_default(),
        special_conditions: t.special_conditions.unwrap_or_default(),
    })
}

pub(in crate::market_data) fn decode_tick_bid_ask(t: proto::HistoricalTickBidAsk) -> Result<TickBidAsk, Error> {
    let attr = t.tick_attrib_bid_ask.as_ref();
    Ok(TickBidAsk {
        time: ts(t.time.unwrap_or_default()),
        price_bid: t.price_bid.unwrap_or_default(),
        price_ask: t.price_ask.unwrap_or_default(),
        size_bid: parse_decimal_or_zero(t.size_bid.as_deref())?,
        size_ask: parse_decimal_or_zero(t.size_ask.as_deref())?,
        attributes: TickAttributeBidAsk {
            bid_past_low: attr.and_then(|a| a.bid_past_low).unwrap_or_default(),
            ask_past_high: attr.and_then(|a| a.ask_past_high).unwrap_or_default(),
        },
    })
}

/// Infallible: the wire `size` is not decoded (see [`TickMidpoint`]).
pub(in crate::market_data) fn decode_tick_midpoint(t: proto::HistoricalTick) -> TickMidpoint {
    TickMidpoint {
        time: ts(t.time.unwrap_or_default()),
        price: t.price.unwrap_or_default(),
    }
}

/// A historical or realtime bar's volume, WAP and trade count, each `None` where the bar has none.
pub(in crate::market_data) struct BarVolume {
    pub(in crate::market_data) volume: Option<f64>,
    pub(in crate::market_data) wap: Option<f64>,
    pub(in crate::market_data) count: Option<i32>,
}

impl BarVolume {
    /// Applies [`decode_bar_volume`]'s rule to values already read: a negative volume or count
    /// is `None`, and the WAP is kept only beside a volume. Deserializing a `Bar` applies it too,
    /// so a bar stored by 5.x with `-1` reads back as `None`.
    pub(in crate::market_data) fn new(volume: Option<f64>, wap: Option<f64>, count: Option<i32>) -> Self {
        let volume = volume.filter(|volume| *volume >= 0.0);
        Self {
            wap: wap.filter(|_| volume.is_some()),
            volume,
            count: count.filter(|count| *count >= 0),
        }
    }
}

/// A bar with no traded volume (MIDPOINT, BID, ASK, BID_ASK) carries none: TWS leaves the three
/// fields out or sends them as `-1`, and for a contract with a price magnifier it sends the WAP as
/// `-1` times the magnifier, so the WAP is kept only beside a volume. The rule goes by value, not
/// by `WhatToShow` (`docs/wire-facts/market-data.md#bar-volume-and-wap-on-non-trades-bars`).
pub(in crate::market_data) fn decode_bar_volume(volume: Option<&str>, wap: Option<&str>, count: Option<i32>) -> Result<BarVolume, Error> {
    let volume = parse_optional_decimal(volume)?.filter(|volume| *volume >= 0.0);
    let wap = match volume {
        Some(_) => parse_optional_decimal(wap)?,
        None => None,
    };
    Ok(BarVolume::new(volume, wap, count))
}

#[cfg(test)]
#[path = "decoders_tests.rs"]
mod tests;
