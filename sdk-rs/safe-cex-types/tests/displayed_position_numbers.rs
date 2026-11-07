use safe_cex_types::DisplayedPosition;
use serde::Deserialize;

const DECIMAL: &str = r#"{
  "id":"linear-A","symbol":"BTC-PERP","side":"long",
  "entryPrice":1.25,"notional":2.5,"leverage":3.75,
  "marginType":"isolated","positionMode":"hedged",
  "unrealizedPnl":-4.125,"realizedPnl":5.25,"contracts":6.5,
  "liquidationPrice":7.75,"marginSize":8.875,"lastUpdate":9.125,
  "account":"acct","key":"display","oneMinUpnl":10.25,
  "fiveMinUpnl":11.5,"chaserSize":12.75
}"#;

#[test]
fn direct_decimal_record_preserves_native_position_and_display_fields() {
    let value: DisplayedPosition = serde_json::from_str(DECIMAL).unwrap();
    assert_eq!(value.position.id, "linear-A");
    assert_eq!(value.position.symbol, "BTC-PERP");
    assert_eq!(value.account, "acct");
    assert_eq!(value.key.as_deref(), Some("display"));
    for (actual, expected) in [
        (value.position.entry_price, 1.25_f64),
        (value.position.notional, 2.5),
        (value.position.leverage, 3.75),
        (value.position.unrealized_pnl, -4.125),
        (value.position.realized_pnl.unwrap(), 5.25),
        (value.position.contracts, 6.5),
        (value.position.liquidation_price, 7.75),
        (value.position.margin_size.unwrap(), 8.875),
        (value.position.last_update.unwrap(), 9.125),
        (value.one_min_upnl.unwrap(), 10.25),
        (value.five_min_upnl.unwrap(), 11.5),
        (value.chaser_size.unwrap(), 12.75),
    ] {
        assert_eq!(actual.to_bits(), expected.to_bits());
    }
}

#[test]
fn direct_negative_zero_record_keeps_sign_through_qualified_deserialize() {
    let text = r#"{"id":"zero","symbol":"ZERO","side":"short",
        "entryPrice":-0,"notional":1,"leverage":2,"unrealizedPnl":-0,
        "contracts":-0,"liquidationPrice":3,"account":"zero-account",
        "oneMinUpnl":-0,"fiveMinUpnl":-0,"chaserSize":-0}"#;
    let mut deserializer = serde_json::Deserializer::from_str(text);
    let value = <DisplayedPosition as Deserialize>::deserialize(&mut deserializer).unwrap();
    deserializer.end().unwrap();
    for actual in [
        value.position.entry_price,
        value.position.unrealized_pnl,
        value.position.contracts,
        value.one_min_upnl.unwrap(),
        value.five_min_upnl.unwrap(),
        value.chaser_size.unwrap(),
    ] {
        assert_eq!(actual.to_bits(), (-0.0_f64).to_bits());
    }
    assert_eq!(value.position.notional, 1.0);
    assert_eq!(value.position.leverage, 2.0);
    assert_eq!(value.position.liquidation_price, 3.0);
}

#[test]
fn streaming_map_keeps_nested_numbers_retained_info_and_late_display_fields() {
    let text = DECIMAL.replace(
        "\"account\":\"acct\"",
        r#""info":{"large":9007199254740993,"zero":-0,"over":1e400},
        "ticker":{"id":"tick","symbol":"BTC-PERP","bid":1.125,
            "ask":2.25,"last":3.5,"mark":4.75,"index":5.875,
            "percentage":6.125,"openInterest":7.25,"fundingRate":8.5,
            "fundingTime":9.75,"volume":10.875,"quoteVolume":11.125},
        "balance":{"used":-0,"free":6.25,"total":7.5,"upnl":-8.75},
        "exchangeName":"venue","hasActiveChaser":true,"hasActiveTWAP":false,
        "isFakePosition":true,"hasFullStop":false,"hasFullTP":true,"account":"acct""#,
    );
    let value: DisplayedPosition = serde_json::from_str(&text).unwrap();
    let ticker = value.ticker.unwrap();
    for (actual, expected) in [
        (ticker.bid, 1.125_f64),
        (ticker.ask, 2.25),
        (ticker.last, 3.5),
        (ticker.mark, 4.75),
        (ticker.index.unwrap(), 5.875),
        (ticker.percentage, 6.125),
        (ticker.open_interest, 7.25),
        (ticker.funding_rate, 8.5),
        (ticker.funding_time, 9.75),
        (ticker.volume, 10.875),
        (ticker.quote_volume, 11.125),
    ] {
        assert_eq!(actual.to_bits(), expected.to_bits());
    }
    let balance = value.balance.unwrap();
    assert_eq!(balance.used.to_bits(), (-0.0_f64).to_bits());
    assert_eq!(balance.free, 6.25);
    assert_eq!(balance.total, 7.5);
    assert_eq!(balance.upnl, -8.75);
    let info = value.position.info.unwrap();
    assert_eq!(
        match info.get("large").unwrap() {
            safe_cex_types::retained_json::RetainedJsonInput::Lexeme(text) => text.as_str(),
            other => panic!("expected retained lexeme, got {other:?}"),
        },
        "9007199254740993"
    );
    assert_eq!(
        match info.get("zero").unwrap() {
            safe_cex_types::retained_json::RetainedJsonInput::Lexeme(text) => text.as_str(),
            other => panic!("expected retained lexeme, got {other:?}"),
        },
        "-0"
    );
    assert_eq!(
        match info.get("over").unwrap() {
            safe_cex_types::retained_json::RetainedJsonInput::Lexeme(text) => text.as_str(),
            other => panic!("expected retained lexeme, got {other:?}"),
        },
        "1e400"
    );
    assert_eq!(value.exchange_name.as_deref(), Some("venue"));
    assert_eq!(value.has_active_chaser, Some(true));
    assert_eq!(value.has_active_twap, Some(false));
    assert_eq!(value.is_fake_position, Some(true));
    assert_eq!(value.has_full_stop, Some(false));
    assert_eq!(value.has_full_tp, Some(true));
    assert_eq!(value.one_min_upnl, Some(10.25));
    assert_eq!(value.five_min_upnl, Some(11.5));
    assert_eq!(value.chaser_size, Some(12.75));
}

#[test]
fn streaming_map_keeps_optional_presence_duplicates_and_object_admission() {
    let minimal = r#"{"id":"plain","symbol":"P","side":"long","entryPrice":1,
        "notional":2,"leverage":3,"unrealizedPnl":4,"contracts":5,
        "liquidationPrice":6,"account":"a"}"#;
    let value: DisplayedPosition = serde_json::from_str(minimal).unwrap();
    assert!(value.key.is_none());
    assert!(value.ticker.is_none());
    assert!(value.market.is_none());
    assert!(value.balance.is_none());
    assert!(value.exchange_name.is_none());
    assert!(value.one_min_upnl.is_none());
    assert!(value.five_min_upnl.is_none());
    assert!(value.chaser_size.is_none());
    assert!(value.has_active_chaser.is_none());
    assert!(value.has_active_twap.is_none());
    assert!(value.is_fake_position.is_none());
    assert!(value.has_full_stop.is_none());
    assert!(value.has_full_tp.is_none());
    for field in [
        "key",
        "ticker",
        "market",
        "balance",
        "exchangeName",
        "oneMinUpnl",
        "fiveMinUpnl",
        "chaserSize",
        "hasActiveChaser",
        "hasActiveTWAP",
        "isFakePosition",
        "hasFullStop",
        "hasFullTP",
        "account",
    ] {
        let null = format!("{{\"{field}\":null,{}", &minimal[1..]);
        assert!(
            serde_json::from_str::<DisplayedPosition>(&null).is_err(),
            "{field} null"
        );
    }
    for (field, literal) in [
        ("key", "\"k\""),
        ("account", "\"a\""),
        ("exchangeName", "\"v\""),
        ("oneMinUpnl", "1"),
        ("fiveMinUpnl", "2"),
        ("chaserSize", "3"),
        ("hasActiveChaser", "true"),
        ("hasActiveTWAP", "false"),
        ("isFakePosition", "true"),
        ("hasFullStop", "false"),
        ("hasFullTP", "true"),
    ] {
        let duplicate = format!(
            "{{\"{field}\":{literal},\"{field}\":{literal},{}",
            &minimal[1..]
        );
        let error = serde_json::from_str::<DisplayedPosition>(&duplicate).unwrap_err();
        assert!(
            error.to_string().contains("duplicate field"),
            "{field}: {error}"
        );
    }
    assert!(
        serde_json::from_str::<DisplayedPosition>(&minimal.replace(",\"account\":\"a\"", ""))
            .is_err()
    );
    for wrong in ["null", "[]", "true", "3", "\"record\""] {
        assert!(serde_json::from_str::<DisplayedPosition>(wrong).is_err());
    }
}
