#[path = "support/native_field_identity.rs"]
mod native_field_identity;
use native_field_identity::NativeFieldIdentity;
use safe_cex_types::*;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

fn roundtrip<T: Serialize + DeserializeOwned + NativeFieldIdentity>(value: Value) {
    let model: T = serde_json::from_value(value.clone()).unwrap();
    model.assert_native_fields(&value);
    if let Some(fields) = value.as_object() {
        for (field, original) in fields {
            let replacement = match original {
                Value::Bool(value) => json!(!value),
                Value::Number(_) => json!(123.125),
                Value::String(_) => json!(format!("identity-{field}")),
                _ => continue,
            };
            let mut distinct = value.clone();
            distinct[field] = replacement;
            if let Ok(native) = serde_json::from_value::<T>(distinct.clone()) {
                native.assert_native_fields(&distinct);
            }
        }
    }
    assert_json_equivalent(&serde_json::to_value(model).unwrap(), &value);
}

// TS numbers have no JSON integer/float distinction (0 and 0.0 are the same value).
fn assert_json_equivalent(actual: &Value, expected: &Value) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(a.as_f64(), b.as_f64()),
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len());
            for (key, value) in b {
                assert_json_equivalent(a.get(key).expect("missing key"), value);
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                assert_json_equivalent(a, b);
            }
        }
        _ => assert_eq!(actual, expected),
    }
}

#[test]
fn model_fields_preserve_units_zero_values_and_optional_absence() {
    let order = json!({"id":"o", "status":"open", "symbol":"BTCUSDT", "type":"stop_market",
        "side":"buy", "price":0, "amount":2.5, "filled":0.5, "remaining":2,
        "reduceOnly":false, "lastUpdate":1000});
    let position = json!({"id":"p", "symbol":"BTCUSDT", "side":"long", "entryPrice":0,
        "notional":0, "leverage":0, "unrealizedPnl":0, "contracts":0, "liquidationPrice":0});
    let market = json!({"id":"m", "symbol":"BTCUSDT", "base":"BTC", "quote":"USDT",
        "active":true, "precision":{"amount":0.001, "price":0.1},
        "limits":{"amount":{"min":0, "max":0}, "leverage":{"min":0, "max":0}}});
    roundtrip::<Order>(order.clone());
    roundtrip::<Position>(position.clone());
    roundtrip::<Market>(market.clone());
    let mut missing_price = order.clone();
    missing_price.as_object_mut().unwrap().remove("price");
    let decoded: Order = serde_json::from_value(missing_price).unwrap();
    assert!(decoded.price.is_absent());
    assert!(
        serde_json::to_value(decoded)
            .unwrap()
            .get("price")
            .is_none()
    );
    for field in ["remaining", "lastUpdate"] {
        let mut incomplete = order.clone();
        incomplete.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<Order>(incomplete).is_err(),
            "{field}"
        );
    }
    for field in [
        "entryPrice",
        "notional",
        "leverage",
        "unrealizedPnl",
        "contracts",
        "liquidationPrice",
    ] {
        let mut incomplete = position.clone();
        incomplete.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<Position>(incomplete).is_err(),
            "{field}"
        );
    }
    let mut full_order = order.clone();
    full_order["extra"] = json!({"remainingNative":8, "filledInPlacementUnits":0.5,
        "remainingNotional":20, "conditionalOrderPrice":42, "rejectReason":"PostOnly",
        "isFromChaser":false, "chaserId":"c", "chaserAttempts":0, "chaserMaxAttempts":3});
    full_order["parentId"] = json!("parent");
    full_order["close"] = json!(false);
    full_order["timeInForce"] = json!("ImmediateOrCancel");
    full_order["triggerType"] = json!("mark_price");
    full_order["tpTriggerType"] = json!("last_price");
    full_order["slTriggerType"] = json!("index_price");
    full_order["takeProfit"] = json!(0);
    full_order["stopLoss"] = json!(0);
    roundtrip::<Order>(full_order);
    let mut full_position = position.clone();
    full_position["extra"] = json!({"contractsNative":0, "unrealizedPnlNative":0});
    full_position["marginType"] = json!("cross");
    full_position["positionMode"] = json!("oneway");
    full_position["realizedPnl"] = json!(0);
    full_position["marginSize"] = json!(0);
    full_position["lastUpdate"] = json!(0);
    roundtrip::<Position>(full_position);
    for info in [
        json!(null),
        json!({"venue":[1,"raw",false]}),
        json!("opaque"),
    ] {
        let mut with_info = order.clone();
        with_info["info"] = info.clone();
        roundtrip::<Order>(with_info);
        let mut with_info = position.clone();
        with_info["info"] = info.clone();
        roundtrip::<Position>(with_info);
        let mut with_info = market.clone();
        with_info["info"] = info;
        roundtrip::<Market>(with_info);
    }
    let mut invalid = order;
    invalid["stopLoss"] = Value::Null;
    assert!(serde_json::from_value::<Order>(invalid).is_err());
    let mut invalid = position;
    invalid["extra"] = json!({"contractsNative":null});
    assert!(serde_json::from_value::<Position>(invalid).is_err());
    let mut invalid = market;
    invalid["isSpot"] = Value::Null;
    assert!(serde_json::from_value::<Market>(invalid).is_err());
}

#[test]
fn every_enum_member_roundtrips_its_exact_ts_literal() {
    macro_rules! check {
        ($ty:ty, $($variant:path => $literal:literal),+ $(,)?) => {{
            $(assert_eq!(serde_json::to_value($variant).unwrap(), json!($literal));
              assert_eq!(serde_json::from_value::<$ty>(json!($literal)).unwrap(), $variant);)+
            assert!(serde_json::from_value::<$ty>(json!("unknown")).is_err());
        }};
    }
    check!(OrderStatus, OrderStatus::Open => "open", OrderStatus::Closed => "closed",
        OrderStatus::Canceled => "canceled", OrderStatus::Creating => "creating",
        OrderStatus::Updating => "updating", OrderStatus::Canceling => "canceling");
    check!(OrderType, OrderType::Market => "market", OrderType::Limit => "limit",
        OrderType::Conditional => "conditional", OrderType::StopLoss => "stop_market",
        OrderType::TakeProfit => "take_profit_market", OrderType::TrailingStopLoss => "trailing_stop_market");
    check!(OrderSide, OrderSide::Buy => "buy", OrderSide::Sell => "sell");
    check!(OrderTimeInForce, OrderTimeInForce::GoodTillCancel => "GoodTillCancel",
        OrderTimeInForce::ImmediateOrCancel => "ImmediateOrCancel", OrderTimeInForce::FillOrKill => "FillOrKill",
        OrderTimeInForce::PostOnly => "PostOnly");
    check!(OrderStopTriggerType, OrderStopTriggerType::MarkPrice => "mark_price",
        OrderStopTriggerType::LastPrice => "last_price", OrderStopTriggerType::IndexPrice => "index_price");
    check!(OrderRejectReason, OrderRejectReason::PostOnly => "PostOnly",
        OrderRejectReason::InsufficientBalance => "InsufficientBalance", OrderRejectReason::ReduceOnly => "ReduceOnly");
    check!(PositionSide, PositionSide::Long => "long", PositionSide::Short => "short");
    check!(PositionMode, PositionMode::Hedged => "hedged", PositionMode::OneWay => "oneway");
    check!(MarginType, MarginType::Isolated => "isolated", MarginType::Cross => "cross");
}

#[test]
fn ticker_derivative_sentinels_are_required_and_preserve_fractional_numbers() {
    let ticker = json!({"id":"spot", "symbol":"BTCUSDT", "bid":0, "ask":0,
        "last":0, "mark":0, "index":0, "percentage":0, "openInterest":0,
        "fundingRate":0, "fundingTime":0, "volume":0, "quoteVolume":0});
    roundtrip::<Ticker>(ticker.clone());
    for field in [
        "mark",
        "index",
        "fundingRate",
        "fundingTime",
        "openInterest",
    ] {
        let mut invalid = ticker.clone();
        invalid.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<Ticker>(invalid).is_err(),
            "{field}"
        );
        let mut invalid = ticker.clone();
        invalid[field] = Value::Null;
        if field == "index" {
            assert_eq!(
                serde_json::from_value::<Ticker>(invalid.clone())
                    .unwrap()
                    .index,
                None
            );
            roundtrip::<Ticker>(invalid);
        } else {
            assert!(
                serde_json::from_value::<Ticker>(invalid).is_err(),
                "{field}"
            );
        }
    }
    let mut full = ticker;
    full["fundingTime"] = json!(1234.5);
    full["fundingRate"] = json!(-0.0001);
    full["midPrice"] = json!(0);
    full["info"] = Value::Null;
    roundtrip::<Ticker>(full);
}

#[test]
fn recursive_balances_and_execution_legs_keep_absence_distinct_from_zero() {
    roundtrip::<Balance>(json!({"used":0, "free":0, "total":0, "upnl":-0.5,
        "info":null, "currencies":{"USD":{"used":0,"free":2.5,"total":2.5,"upnl":0}},
        "coins":{"BTC":{"base":"BTC", "amount":0.1,"notional":20}}}));
    let fill = json!({"timestamp":1234.5, "symbol":"BTC", "price":0, "qty":0.5, "side":"buy"});
    roundtrip::<Execution>(fill.clone());
    for side in ["long", "short", "both"] {
        let mut full = fill.clone();
        full["positionSide"] = json!(side);
        full["realizedPnl"] = json!(0);
        full["commission"] = json!(0);
        full["venueReportedRevenue"] = json!(0);
        roundtrip::<Execution>(full);
    }
    for side in [json!(null), json!("net"), json!("BOTH")] {
        let mut invalid = fill.clone();
        invalid["positionSide"] = side;
        assert!(serde_json::from_value::<Execution>(invalid).is_err());
    }
    roundtrip::<OrderBook>(json!({"bids":[{"price":0,"amount":0.25,"total":0}], "asks":[]}));
    roundtrip::<Candle>(
        json!({"timestamp":1.5,"open":0,"high":0,"low":0,"close":0,"volume":0,"lastSide":"sell"}),
    );
}

#[test]
fn account_and_options_preserve_wire_fields_and_reject_runtime_capabilities() {
    roundtrip::<ExchangeAccountConfig>(json!({"exchange":"bybit", "name":"main",
        "apiKey":"", "apiSecret":"", "secondary_apiKey":"secondary",
        "secondary_apiSecret":"", "bybitSiteId":"BRA_BTL", "healthStatus":"startup_failed",
        "updatedAt":1.5, "deletedAt":0}));
    for account in [
        json!({"userId":"u"}),
        json!({"userId":"u","subId":null,"affiliateId":null}),
        json!({"userId":"u","subId":"s","affiliateId":"a"}),
    ] {
        roundtrip::<ExchangeAccount>(account);
    }
    let options = json!({"key":"", "secret":"", "secondary_key":"s", "secondaryProxyUrl":"url",
        "auth":{"userAddress":"u"}, "hyperliquidBuilder":{"address":"a","feeTenthsBps":1.5},
        "tuning":{"activeTickerSymbols":["BTC"],"dataUpdateThrottleMultiplier":0},
        "extra":{"asterdexApiVersion":"v3","lighterBuilderFeeMode":"dynamic",
            "kucoinBroker":{"partner":"p","brokerKey":"k","brokerName":"n","verify":false}},
        "externalPublicData":{"type":"websocket","url":"ws://example","exchangeId":"bybit","authToken":""}});
    roundtrip::<ExchangeOptions>(options.clone());
    for field in ["remoteSigner", "payloadSigner", "transport"] {
        for value in [json!(null), json!({}), json!("handle")] {
            let mut invalid = options.clone();
            invalid[field] = value;
            assert!(
                serde_json::from_value::<ExchangeOptions>(invalid).is_err(),
                "{field}"
            );
        }
    }
    let mut invalid = options;
    invalid["externalPublicData"] = json!({"type":"in-process","source":{}});
    assert!(serde_json::from_value::<ExchangeOptions>(invalid).is_err());
    roundtrip::<PublicExchangeOptions>(json!({"publicDataOnly":true,"forceAllProxiesOff":false}));
    for value in [
        json!({}),
        json!({"publicDataOnly":false}),
        json!({"publicDataOnly":null}),
    ] {
        assert!(serde_json::from_value::<PublicExchangeOptions>(value).is_err());
    }
    roundtrip::<RequestOutcomeCounts>(
        json!({"ok":0,"rateLimit":1.5,"auth":0,"network":0,"other":0}),
    );
    assert!(serde_json::from_value::<RequestOutcomeCounts>(json!({"ok":0})).is_err());
}

#[test]
fn update_order_union_retains_both_declared_fields() {
    let order = json!({"id":"o", "status":"open", "symbol":"BTC", "type":"limit", "side":"buy",
        "price":0, "amount":0, "filled":0, "remaining":0, "reduceOnly":false,"lastUpdate":0});
    for update in [
        json!({"amount":0.5}),
        json!({"price":0}),
        json!({"amount":0.5,"price":0}),
    ] {
        roundtrip::<UpdateOrderOpts>(json!({"order":order,"update":update}));
    }
    assert!(serde_json::from_value::<UpdateOrderOpts>(json!({"order":order,"update":{}})).is_err());
}

#[test]
fn request_stats_consumer_uses_semantic_outcomes_without_changing_wire_values() {
    for (outcome, literal) in [
        (RequestOutcome::Ok, "ok"),
        (RequestOutcome::RateLimit, "rateLimit"),
        (RequestOutcome::Auth, "auth"),
        (RequestOutcome::Network, "network"),
        (RequestOutcome::Other, "other"),
    ] {
        assert_eq!(serde_json::to_value(&outcome).unwrap(), json!(literal));
        assert_eq!(
            serde_json::from_value::<RequestOutcome>(json!(literal)).unwrap(),
            outcome
        );
    }
    assert!(serde_json::from_value::<RequestOutcome>(json!("RateLimit")).is_err());
}

#[test]
fn any_timeframe_preserves_distinct_minute_and_month_branches() {
    let minute = AnyTimeframe::Variant1(Timeframe::M1);
    let month = AnyTimeframe::Variant2(ExtendedTimeframe::MN1);
    assert_ne!(minute, month);
    for (value, literal) in [(minute, "1m"), (month, "1M")] {
        assert_eq!(serde_json::to_value(&value).unwrap(), json!(literal));
        assert_eq!(
            serde_json::from_value::<AnyTimeframe>(json!(literal)).unwrap(),
            value
        );
    }
    assert!(serde_json::from_value::<Timeframe>(json!("1M")).is_err());
    assert!(serde_json::from_value::<ExtendedTimeframe>(json!("1m")).is_err());
    assert!(serde_json::from_value::<AnyTimeframe>(json!("1H")).is_err());
}

#[test]
fn hyphenated_and_keyword_shaped_variants_keep_their_wire_literals() {
    for (variant, literal) in [
        (ExchangeName::LighterRobinhood, "lighter-robinhood"),
        (ExchangeName::TealstreetDex, "tealstreet-dex"),
    ] {
        assert_eq!(serde_json::to_value(&variant).unwrap(), json!(literal));
        assert_eq!(
            serde_json::from_value::<ExchangeName>(json!(literal)).unwrap(),
            variant
        );
    }
    let mode = ExchangeOptionsExtraLighterBuilderFeeMode::Static;
    assert_eq!(serde_json::to_value(&mode).unwrap(), json!("static"));
    assert_eq!(
        serde_json::from_value::<ExchangeOptionsExtraLighterBuilderFeeMode>(json!("static"))
            .unwrap(),
        mode
    );
    let source = ExchangeOptionsExternalPublicDataVariant1Type::InProcess;
    assert_eq!(serde_json::to_value(&source).unwrap(), json!("in-process"));
    assert_eq!(
        serde_json::from_value::<ExchangeOptionsExternalPublicDataVariant1Type>(json!(
            "in-process"
        ))
        .unwrap(),
        source
    );
    assert!(serde_json::from_value::<ExchangeName>(json!("LighterRobinhood")).is_err());
}
