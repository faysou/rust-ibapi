use super::*;
use crate::common::test_utils::wire_enum::check_wire_code_round_trip;

#[test]
fn depth_operation_round_trips_every_wire_code() {
    check_wire_code_round_trip(
        &[
            (DepthOperation::Insert, 0),
            (DepthOperation::Update, 1),
            (DepthOperation::Delete, 2),
            (DepthOperation::Unknown(3), 3),
            (DepthOperation::Unknown(-1), -1),
        ],
        DepthOperation::Unknown,
    );
}

#[test]
fn depth_side_round_trips_every_wire_code() {
    check_wire_code_round_trip(
        &[
            (DepthSide::Ask, 0),
            (DepthSide::Bid, 1),
            (DepthSide::Unknown(2), 2),
            (DepthSide::Unknown(-1), -1),
        ],
        DepthSide::Unknown,
    );
}

#[test]
fn what_to_show_round_trip() {
    use crate::common::test_utils::wire_enum::{check_wire_enum_rejects_unknown, check_wire_enum_round_trip};

    check_wire_enum_round_trip(&[
        (WhatToShow::Trades, "TRADES"),
        (WhatToShow::MidPoint, "MIDPOINT"),
        (WhatToShow::Bid, "BID"),
        (WhatToShow::Ask, "ASK"),
        (WhatToShow::AggTrades, "AGGTRADES"),
    ]);
    // Historical-only codes and the variant name are not realtime wire values, and matching is
    // case-sensitive (the historical `WhatToShow` is not).
    check_wire_enum_rejects_unknown::<WhatToShow>(&["", "BID_ASK", "MidPoint", "trades"]);
}

/// Only 317 becomes `Reset`; other notices on a depth stream, including the
/// other data advisories, stay `Notice` (#899).
#[test]
fn market_depths_data_from_notice() {
    let notice = |code| Notice::synthesized(code, String::new());

    assert_eq!(
        <MarketDepths as StreamDecoder<MarketDepths>>::data_from_notice(&notice(317)),
        Some(MarketDepths::Reset)
    );
    for code in [316, 2104, 2188, 10167] {
        assert_eq!(
            <MarketDepths as StreamDecoder<MarketDepths>>::data_from_notice(&notice(code)),
            None,
            "code {code}"
        );
    }
}

// The `Default` tests destructure every field without `..`, so a new field fails to compile
// here until its default is chosen.

#[test]
fn bar_default_is_zero_at_epoch() {
    let Bar {
        date,
        open,
        high,
        low,
        close,
        volume,
        wap,
        count,
    } = Bar::default();
    assert_eq!(date, OffsetDateTime::UNIX_EPOCH);
    assert_eq!((open, high, low, close), (0.0, 0.0, 0.0, 0.0));
    assert_eq!((volume, wap, count), (None, None, None));
}

/// A bar stored by 5.x carries the `-1` TWS sends for a bar with no traded volume; it reads back
/// as `None`, as the decoder reads it. Real values, a quiet bar's zeros included, round-trip.
#[test]
fn bar_deserialize_applies_the_decoder_volume_rule() {
    let traded = Bar {
        volume: Some(1200.0),
        wap: Some(101.25),
        count: Some(37),
        ..Bar::default()
    };
    let quiet = Bar {
        volume: Some(0.0),
        wap: Some(0.0),
        count: Some(0),
        ..Bar::default()
    };
    let stored = |volume: f64, wap: f64, count: i32| {
        let mut json = serde_json::to_value(Bar::default()).unwrap();
        json["volume"] = serde_json::json!(volume);
        json["wap"] = serde_json::json!(wap);
        json["count"] = serde_json::json!(count);
        serde_json::from_value::<Bar>(json).unwrap()
    };
    let read = |bar: &Bar| (bar.volume, bar.wap, bar.count);

    assert_eq!(read(&stored(-1.0, -1.0, -1)), (None, None, None), "5.x sentinel");
    assert_eq!(read(&stored(-1.0, -100.0, -1)), (None, None, None), "magnified WAP sentinel");
    assert_eq!(read(&stored(-1.0, 101.25, 5)), (None, None, Some(5)), "WAP needs a volume");
    for bar in [traded, quiet] {
        let back: Bar = serde_json::from_value(serde_json::to_value(&bar).unwrap()).unwrap();
        assert_eq!(back, bar);
    }
}
