use safe_cex_registry::public_data::*;
use safe_cex_types::gateway::*;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

fn round_trip<T: DeserializeOwned + Serialize>(value: Value) {
    let decoded: T = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), value);
}

#[test]
fn message_tags_and_optional_fields_preserve_the_wire_shape() {
    for tag in [
        "subscribe",
        "unsubscribe",
        "subscribed",
        "unsubscribed",
        "data",
        "error",
        "ping",
        "pong",
    ] {
        round_trip::<GatewayMessage>(json!({"type":tag,"exchangeId":"bybit"}));
    }
    for channel in [
        "ticker",
        "tickers",
        "orderbook",
        "candle",
        "trade",
        "market",
    ] {
        round_trip::<GatewayMessage>(
            json!({"type":"data","exchangeId":"bybit","channel":channel,"symbol":"BTC","interval":"1m","data":null,"error":"","requestId":"r"}),
        );
    }
    for bad in [
        json!({"type":"bogus","exchangeId":"bybit"}),
        json!({"type":"data","exchangeId":"bybit","channel":"bogus"}),
        json!({"type":"data","exchangeId":"bybit","symbol":null}),
    ] {
        assert!(serde_json::from_value::<GatewayMessage>(bad).is_err());
    }
}

#[test]
fn specialized_updates_enforce_data_and_channel_literals() {
    let ticker = json!({"id":"BTC","symbol":"BTC","bid":1.0,"ask":1.0,"last":1.0,"mark":1.0,"index":1.0,"percentage":0.0,"openInterest":0.0,"fundingRate":0.0,"fundingTime":0.0,"volume":0.0,"quoteVolume":0.0});
    for channel in ["ticker", "tickers"] {
        for data in [ticker.clone(), json!([ticker])] {
            round_trip::<GatewayTickerUpdate>(
                json!({"type":"data","exchangeId":"bybit","channel":channel,"data":data}),
            );
        }
    }
    let orderbook = json!({"type":"data","exchangeId":"bybit","channel":"orderbook","symbol":"BTC","data":{"bids":[],"asks":[]}});
    round_trip::<GatewayOrderbookUpdate>(orderbook.clone());
    let candle = json!({"type":"data","exchangeId":"bybit","channel":"candle","symbol":"BTC","interval":"1m","data":{"timestamp":1.0,"open":1.0,"high":1.0,"low":1.0,"close":1.0,"volume":1.0}});
    round_trip::<GatewayCandleUpdate>(candle.clone());
    let market = json!({"type":"data","exchangeId":"bybit","channel":"market","data":[]});
    round_trip::<GatewayMarketUpdate>(market.clone());
    for (mut wrong, kind) in [
        (orderbook, "orderbook"),
        (candle, "candle"),
        (market, "market"),
    ] {
        wrong["type"] = json!("ping");
        let rejects = match kind {
            "orderbook" => serde_json::from_value::<GatewayOrderbookUpdate>(wrong).is_err(),
            "candle" => serde_json::from_value::<GatewayCandleUpdate>(wrong).is_err(),
            _ => serde_json::from_value::<GatewayMarketUpdate>(wrong).is_err(),
        };
        assert!(rejects);
    }
    assert!(
        serde_json::from_value::<GatewayTickerUpdate>(
            json!({"type":"data","exchangeId":"bybit","channel":"market","data":[]})
        )
        .is_err()
    );
}

#[test]
fn subscription_keys_split_at_the_last_delimiter_and_keep_source_edge_cases() {
    assert_eq!(get_candle_subscription_key("BTC:USDT", "1h"), "BTC:USDT:1h");
    assert_eq!(
        parse_candle_subscription_key("BTC:USDT:1h"),
        ("BTC:USDT".into(), "1h".into())
    );
    assert_eq!(parse_candle_subscription_key(""), ("".into(), "".into()));
    assert_eq!(
        parse_candle_subscription_key("BTC"),
        ("BT".into(), "BTC".into())
    );
    assert_eq!(
        parse_candle_subscription_key(":1m"),
        ("".into(), "1m".into())
    );
}

#[test]
fn malformed_no_separator_astral_key_has_a_declared_utf16_divergence() {
    assert_eq!(
        parse_candle_subscription_key("😀"),
        ("".into(), "😀".into())
    );
    assert_eq!(
        parse_candle_subscription_key("BTC😀"),
        ("BTC".into(), "BTC😀".into())
    );
    assert_eq!(
        parse_candle_subscription_key("😀:1m"),
        ("😀".into(), "1m".into())
    );
}

#[test]
fn gateway_auth_cast_and_explicit_null_divergences_are_pinned() {
    round_trip::<GatewayMessage>(
        json!({"type":"subscribed","exchangeId":"bybit","channel":"auth"}),
    );
    for field in ["channel", "symbol", "interval", "error", "requestId"] {
        let mut frame = json!({"type":"data","exchangeId":"bybit"});
        frame[field] = Value::Null;
        assert!(
            serde_json::from_value::<GatewayMessage>(frame).is_err(),
            "{field}"
        );
    }
}

#[test]
fn producers_construct_all_four_envelopes_with_fixed_wire_tags() {
    let ticker = GatewayTickerUpdate::new("bybit".into(), true, GatewayTickerData::Tickers(vec![]));
    let book = GatewayOrderbookUpdate::new(
        "bybit".into(),
        "BTC".into(),
        serde_json::from_value(json!({"bids":[],"asks":[]})).unwrap(),
    );
    let candle = GatewayCandleUpdate::new(
        "bybit".into(),
        "BTC".into(),
        serde_json::from_value(json!("1m")).unwrap(),
        serde_json::from_value(
            json!({"timestamp":1,"open":1,"high":1,"low":1,"close":1,"volume":1}),
        )
        .unwrap(),
    );
    let market = GatewayMarketUpdate::new("bybit".into(), vec![]);
    for (value, channel) in [
        (serde_json::to_value(ticker).unwrap(), "tickers"),
        (serde_json::to_value(book).unwrap(), "orderbook"),
        (serde_json::to_value(candle).unwrap(), "candle"),
        (serde_json::to_value(market).unwrap(), "market"),
    ] {
        assert_eq!(value["type"], "data");
        assert_eq!(value["channel"], channel);
        assert_eq!(value["exchangeId"], "bybit");
    }
    assert_eq!(
        serde_json::to_value(GatewayTickerUpdate::new(
            "bybit".into(),
            false,
            GatewayTickerData::Tickers(vec![])
        ))
        .unwrap()["channel"],
        "ticker"
    );
}

#[test]
fn gateway_whole_frame_validation_divergence_is_explicit() {
    for frame in [
        json!({"type":"data"}),
        json!({"type":null,"exchangeId":"bybit"}),
        json!({"type":"data","exchangeId":null}),
        json!({"type":"data","exchangeId":"bybit","channel":"funding"}),
        json!({"type":"data","exchangeId":"bybit","interval":"7m"}),
        json!({"type":"data","exchangeId":"bybit","symbol":1}),
        json!({"type":"data","exchangeId":"bybit","channel":"tickers","interval":"7M","data":[]}),
    ] {
        assert!(
            serde_json::from_value::<GatewayMessage>(frame.clone()).is_err(),
            "{frame}"
        );
    }
    let monthly =
        json!({"type":"data","exchangeId":"bybit","channel":"tickers","interval":"1M","data":[]});
    let decoded: GatewayMessage = serde_json::from_value(monthly.clone()).unwrap();
    assert!(matches!(
        decoded.interval,
        Some(safe_cex_types::AnyTimeframe::Variant2(
            safe_cex_types::ExtendedTimeframe::MN1
        ))
    ));
    round_trip::<GatewayMessage>(monthly);
    round_trip::<GatewayMessage>(json!({"type":"data","exchangeId":"bybit","data":null}));
}

// Snapshot independently extracted from the TS registry. The live oracle separately
// detects source changes; this test remains effective if that oracle is gutted.
#[test]
fn registry_descriptors_and_registered_name_order_match_reviewed_snapshot() {
    use safe_cex_registry::registry::{EXCHANGE_REGISTRY, get_registered_exchange_names};
    let expected: Value = serde_json::from_str(include_str!("fixtures/registry.json")).unwrap();
    assert_eq!(expected.as_array().unwrap().len(), 49);
    assert_eq!(serde_json::to_value(EXCHANGE_REGISTRY).unwrap(), expected);
    let names: Vec<_> = expected
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["name"].clone())
        .collect();
    assert_eq!(
        serde_json::to_value(get_registered_exchange_names()).unwrap(),
        json!(names)
    );
}

#[test]
fn gateway_unknown_top_level_fields_are_declared_lossy() {
    let frame = json!({"type":"data","exchangeId":"bybit","seq":42,"data":null});
    let typed: GatewayMessage = serde_json::from_value(frame).unwrap();
    assert_eq!(
        serde_json::to_value(typed).unwrap(),
        json!({"type":"data","exchangeId":"bybit","data":null})
    );
}
