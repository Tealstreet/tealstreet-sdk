#[path = "../../safe-cex-exchange/tests/support/store.rs"]
mod test_store;
use safe_cex_types::*;
use safe_cex_webworker::postprocessors::*;
use serde_json::json;

fn displayed(notional: f64, side: PositionSide) -> DisplayedPosition {
    let mut row: DisplayedPosition = serde_json::from_value(json!({
        "id":"p","symbol":"BTC","side":"long","entryPrice":1,"notional":0,
        "leverage":1,"unrealizedPnl":0,"contracts":1,"liquidationPrice":0,"account":"a"
    }))
    .unwrap();
    row.position.notional = notional;
    row.position.side = side.into();
    row
}

#[test]
fn margin_preserves_addition_order_and_js_nan_falsiness() {
    let mut data = test_store::TestStore::new().data.into_inner();
    data.displayed_positions = vec![
        displayed(-1e16, PositionSide::Long),
        displayed(1.0, PositionSide::Short),
        displayed(1.0, PositionSide::Short),
        displayed(f64::NAN, PositionSide::Long),
    ];
    DisplayedMarginPostprocessor.process(&mut data);
    let margin = data.displayed_margin.as_ref().unwrap();
    assert_eq!(margin.long_exposure, 1e16);
    assert_eq!(margin.short_exposure, 2.0);
    assert_eq!(margin.total_notional, 1e16);
    data.displayed_positions.clear();
    DisplayedMarginPostprocessor.process(&mut data);
    assert_eq!(data.displayed_margin.as_ref().unwrap().total_notional, 0.0);
}

#[test]
fn chaser_last_owner_wins_and_absence_does_not_clear_old_enrichment() {
    let mut data = test_store::TestStore::new().data.into_inner();
    data.orders.push(
        serde_json::from_value(json!({
            "id":"order","symbol":"BTC","status":"open","type":"limit","side":"buy",
            "price":1,"amount":2,"filled":0,"remaining":2,"reduceOnly":false,"lastUpdate":0,
            "extra":{"remainingNotional":42}
        }))
        .unwrap(),
    );
    for (id, attempts) in [("first", 1), ("last", 3)] {
        data.active_chasers.push(
            serde_json::from_value(json!({
                "id":id,"symbol":"BTC","size":2,"side":"buy","maxAttempts":7,
                "attempts":attempts,"isStopped":false,"stopReason":"","currentOrderIds":["order"]
            }))
            .unwrap(),
        );
    }
    ChaserOrderEnrichmentPostprocessor.process(&mut data);
    let extra = data.orders[0].extra.clone().unwrap();
    assert_eq!(extra.chaser_id.as_deref(), Some("last"));
    assert_eq!(extra.chaser_attempts, Some(3.0));
    assert_eq!(extra.chaser_max_attempts, Some(7.0));
    assert_eq!(extra.is_from_chaser, Some(true));
    assert_eq!(extra.remaining_notional, Some(42.0));
    data.active_chasers.clear();
    ChaserOrderEnrichmentPostprocessor.process(&mut data);
    assert_eq!(data.orders[0].extra.as_ref(), Some(&extra));
}

#[test]
fn chaser_enrichment_matches_live_plain_object_ownership_and_prototype_keys() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/chaser_enrichment_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 24);
    for (index, case) in cases.iter().enumerate() {
        let mut data = test_store::TestStore::new().data.into_inner();
        data.active_chasers =
            serde_json::from_value(case["input"]["activeChasers"].clone()).unwrap();
        data.orders = serde_json::from_value(case["input"]["orders"].clone()).unwrap();
        ChaserOrderEnrichmentPostprocessor.process(&mut data);
        for (actual, source) in data.orders.iter().zip(case["expected"].as_array().unwrap()) {
            assert_eq!(actual.id, source["id"], "ENRICHMENT-IDENTITY native id");
            assert_eq!(
                actual.symbol, source["symbol"],
                "ENRICHMENT-IDENTITY native symbol"
            );
            for (field, value) in [
                ("price", actual.price.to_number()),
                ("amount", actual.amount),
                ("filled", actual.filled),
                ("remaining", actual.remaining),
                ("lastUpdate", actual.last_update),
            ] {
                assert_eq!(
                    value.to_bits(),
                    source[field].as_f64().unwrap().to_bits(),
                    "ENRICHMENT-IDENTITY native {field}"
                );
            }
            if let ModelField::Value(extra) = &actual.extra {
                for (field, value) in [
                    ("chaserAttempts", extra.chaser_attempts),
                    ("chaserMaxAttempts", extra.chaser_max_attempts),
                    ("remainingNotional", extra.remaining_notional),
                ] {
                    assert_eq!(
                        value.map(f64::to_bits),
                        source["extra"][field].as_f64().map(f64::to_bits),
                        "ENRICHMENT-IDENTITY native {field}"
                    );
                }
                assert_eq!(
                    extra.chaser_id.as_deref(),
                    source["extra"]["chaserId"].as_str(),
                    "ENRICHMENT-IDENTITY native chaserId"
                );
                assert_eq!(
                    extra.is_from_chaser,
                    source["extra"]["isFromChaser"].as_bool(),
                    "ENRICHMENT-IDENTITY native isFromChaser"
                );
            }
        }
        // Compare typed whole orders, retaining omitted enrichment fields.
        let expected: Vec<Order> = serde_json::from_value(case["expected"].clone()).unwrap();
        assert_eq!(data.orders, expected, "live enrichment case {index}");
    }
}

#[test]
fn margin_field_identity_matches_distinct_live_source_exposures() {
    let result = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/margin_identity_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let source: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    for row in source["rows"].as_array().unwrap() {
        let mut data = test_store::TestStore::new().data.into_inner();
        data.displayed_positions = row["positions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                displayed(
                    p["notional"].as_f64().unwrap(),
                    if p["side"] == "long" {
                        PositionSide::Long
                    } else {
                        PositionSide::Short
                    },
                )
            })
            .collect();
        DisplayedMarginPostprocessor.process(&mut data);
        let margin = data.displayed_margin.unwrap();
        for (field, value) in [
            ("longExposure", margin.long_exposure),
            ("shortExposure", margin.short_exposure),
            ("totalNotional", margin.total_notional),
        ] {
            assert_eq!(
                value.to_bits(),
                row["expected"][field].as_f64().unwrap().to_bits(),
                "MARGIN-IDENTITY native {field}"
            );
        }
    }
}

#[test]
fn chaser_enrichment_retains_optional_field_identity() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/chaser_enrichment_identity_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(capture["label"], "chaser-enrichment-field-identity");
    let rows = capture["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 12);
    macro_rules! native_enum {
        ($value:expr, $source:expr, $field:literal, $ty:ident; $($variant:ident => $wire:literal),+ $(,)?) => {
            assert_eq!($value.map(|v| match v {$($ty::$variant => $wire),+}), $source[$field].as_str(), concat!("ENRICHMENT-OPTIONAL native ", $field));
        };
    }
    for row in rows {
        let mut data = test_store::TestStore::new().data.into_inner();
        data.active_chasers =
            serde_json::from_value(row["input"]["activeChasers"].clone()).unwrap();
        data.orders = serde_json::from_value(row["input"]["orders"].clone()).unwrap();
        ChaserOrderEnrichmentPostprocessor.process(&mut data);
        assert_eq!(data.orders.len(), 2, "ENRICHMENT-OPTIONAL order count");
        let expected_orders = row["expected"].as_array().unwrap();
        assert_eq!(
            expected_orders.len(),
            data.orders.len(),
            "ENRICHMENT-OPTIONAL expected/native order count"
        );
        for (actual, source) in data.orders.iter().zip(expected_orders) {
            for (field, value) in [
                ("id", Some(actual.id.as_str())),
                ("symbol", Some(actual.symbol.as_str())),
                ("parentId", actual.parent_id.as_deref()),
                ("clientOrderId", actual.client_order_id.as_deref()),
            ] {
                assert_eq!(
                    value,
                    source[field].as_str(),
                    "ENRICHMENT-OPTIONAL native {field}"
                );
            }
            for (field, value) in [
                ("price", actual.price.value().copied()),
                ("amount", Some(actual.amount)),
                ("filled", Some(actual.filled)),
                ("remaining", Some(actual.remaining)),
                ("lastUpdate", Some(actual.last_update)),
                ("takeProfit", actual.take_profit.value().copied()),
                ("stopLoss", actual.stop_loss.value().copied()),
            ] {
                assert_eq!(
                    value.map(f64::to_bits),
                    source[field].as_f64().map(f64::to_bits),
                    "ENRICHMENT-OPTIONAL native {field}"
                );
            }
            assert_eq!(
                actual.reduce_only.into_value(),
                source["reduceOnly"].as_bool(),
                "ENRICHMENT-OPTIONAL native reduceOnly"
            );
            assert_eq!(
                actual.close,
                source["close"].as_bool(),
                "ENRICHMENT-OPTIONAL native close"
            );
            assert_eq!(
                actual.info.as_ref(),
                source.get("info").cloned().map(Into::into).as_ref(),
                "ENRICHMENT-OPTIONAL native info"
            );
            native_enum!(actual.status, source, "status", OrderStatus; Open=>"open", Closed=>"closed", Canceled=>"canceled", Creating=>"creating", Updating=>"updating", Canceling=>"canceling");
            native_enum!(actual.r#type, source, "type", OrderType; Market=>"market", Limit=>"limit", Conditional=>"conditional", StopLoss=>"stop_market", TakeProfit=>"take_profit_market", TrailingStopLoss=>"trailing_stop_market");
            native_enum!(actual.side, source, "side", OrderSide; Buy=>"buy", Sell=>"sell");
            native_enum!(actual.time_in_force, source, "timeInForce", OrderTimeInForce; GoodTillCancel=>"GoodTillCancel", ImmediateOrCancel=>"ImmediateOrCancel", FillOrKill=>"FillOrKill", PostOnly=>"PostOnly");
            for (field, value) in [
                ("triggerType", actual.trigger_type),
                ("tpTriggerType", actual.tp_trigger_type),
                ("slTriggerType", actual.sl_trigger_type),
            ] {
                let wire = value.map(|v| match v {
                    OrderStopTriggerType::MarkPrice => "mark_price",
                    OrderStopTriggerType::LastPrice => "last_price",
                    OrderStopTriggerType::IndexPrice => "index_price",
                });
                assert_eq!(
                    wire,
                    source[field].as_str(),
                    "ENRICHMENT-OPTIONAL native {field}"
                );
            }
            let extra = actual
                .extra
                .as_ref()
                .expect("ENRICHMENT-OPTIONAL retained extra");
            let expected = &source["extra"];
            for (field, value) in [
                ("remainingNative", extra.remaining_native),
                ("filledInPlacementUnits", extra.filled_in_placement_units),
                ("remainingNotional", extra.remaining_notional),
                ("conditionalOrderPrice", extra.conditional_order_price),
                ("chaserAttempts", extra.chaser_attempts),
                ("chaserMaxAttempts", extra.chaser_max_attempts),
            ] {
                assert_eq!(
                    value.map(f64::to_bits),
                    expected[field].as_f64().map(f64::to_bits),
                    "ENRICHMENT-OPTIONAL native extra.{field}"
                );
            }
            assert_eq!(
                extra.chaser_id.as_deref(),
                expected["chaserId"].as_str(),
                "ENRICHMENT-OPTIONAL native chaserId"
            );
            assert_eq!(
                extra.is_from_chaser,
                expected["isFromChaser"].as_bool(),
                "ENRICHMENT-OPTIONAL native isFromChaser"
            );
            native_enum!(extra.reject_reason, expected, "rejectReason", OrderRejectReason; PostOnly=>"PostOnly", InsufficientBalance=>"InsufficientBalance", ReduceOnly=>"ReduceOnly");
        }
    }
}
