#[path = "support/model_identity.rs"]
mod model_identity;
use safe_cex_exchange::store_payload::*;
use safe_cex_types::StoreData;
use serde_json::{Value, json};
fn normalized(v: Value) -> Value {
    match v {
        Value::Number(n) => json!(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.into_iter().map(normalized).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, normalized(v))).collect()),
        v => v,
    }
}
#[test]
fn store_payload_preparation_matches_live_full_snapshots_and_incremental_transitions() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/store_preparation_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        data["fieldIdentity"].as_array().unwrap().len(),
        36,
        "SNAPSHOT-IDENTITY case inventory"
    );
    for case in data["fieldIdentity"].as_array().unwrap() {
        let input: StoreData = serde_json::from_value(case["input"].clone()).unwrap();
        let sanitized = sanitize_for_post_message(&input, false, false);
        assert_snapshot_identity(&sanitized, &case["sanitized"]);
        let prepared = StorePayloadPreparer::default().prepare(&input, 1.0, 2.0, false, Some(true));
        assert_snapshot_identity(&prepared, &case["prepared"]);
    }
    let cases = data["sanitize"].as_array().unwrap();
    assert_eq!(cases.len(), 48);
    for (i, case) in cases.iter().enumerate() {
        let input: StoreData = serde_json::from_value(case["input"].clone()).unwrap();
        let output = sanitize_for_post_message(
            &input,
            case["omitOrders"].as_bool().unwrap(),
            case["omitPositions"].as_bool().unwrap(),
        );
        assert_eq!(
            normalized(serde_json::to_value(output).unwrap()),
            normalized(case["expected"].clone()),
            "sanitize case{i}"
        );
    }
    let scenarios = data["scenarios"].as_array().unwrap();
    assert_eq!(scenarios.len(), 72, "SNAPSHOT-CARDINALITY scenario total");
    let mut comparisons = 0;
    for (i, scenario) in scenarios.iter().enumerate() {
        let mut preparer = StorePayloadPreparer::default();
        let steps = scenario["steps"].as_array().unwrap();
        assert_eq!(steps.len(), 17, "SNAPSHOT-CARDINALITY steps per scenario");
        for j in 0..17 {
            let step = steps
                .get(j)
                .expect("SNAPSHOT-CARDINALITY missing fixed step");
            let input: StoreData = serde_json::from_value(step["input"].clone()).unwrap();
            let output = preparer.prepare(
                &input,
                step["marketsVersion"].as_f64().unwrap(),
                step["tickersVersion"].as_f64().unwrap(),
                step["hostedMode"].as_bool().unwrap(),
                step.get("force").map(|v| v.as_bool().unwrap()),
            );
            let actual = normalized(serde_json::to_value(output).unwrap());
            let expected = &step["expected"];
            for field in [
                "markets",
                "tickers",
                "orders",
                "positions",
                "displayedPositions",
                "activeChasers",
                "activeTWAPs",
            ] {
                assert_eq!(
                    actual[field].as_array().map(Vec::len),
                    expected[field].as_array().map(Vec::len),
                    "SNAPSHOT-CARDINALITY expected/native {field}"
                );
            }
            assert_eq!(
                actual,
                normalized(expected.clone()),
                "SNAPSHOT-CARDINALITY complete output scenario{i},step{j}"
            );
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, 1224, "SNAPSHOT-CARDINALITY native comparisons");
    assert_eq!(
        data["aliasWitness"],
        json!({"updateId":0,"loaded":false,"socketHealthy":false,"balance":99,"orderAmount":99})
    );
    let mut input: StoreData = serde_json::from_value(cases[0]["input"].clone()).unwrap();
    let held = sanitize_for_post_message(&input, false, false);
    input.update_ids.orders = 99.0;
    input.loaded.orders = true;
    *input
        .socket_health
        .as_mut()
        .unwrap()
        .get_mut("publicHealthy")
        .unwrap() = json!(true).into();
    input.balance.total = 99.0;
    input.orders[1].amount = 99.0;
    assert_eq!(held.update_ids.unwrap().orders, Some(0.0));
    assert!(!held.loaded.unwrap().orders);
    assert!(
        !held
            .socket_health
            .unwrap()
            .get("publicHealthy")
            .and_then(safe_cex_types::retained_json::RetainedJsonInput::as_bool)
            .unwrap()
    );
    assert_eq!(
        held.balance.unwrap().total,
        0.0,
        "CHOSEN owned snapshot omits balance alias"
    );
    assert_eq!(
        held.orders.unwrap()[1].amount,
        2.0,
        "CHOSEN owned snapshot omits untouched order alias"
    );
    // These must remain typed copies, not a JSON round-trip that converts to null.
    input.balance.total = f64::INFINITY;
    input.orders[0].price = f64::NAN.into();
    input.positions[0].contracts = f64::NEG_INFINITY;
    input.displayed_positions[0].position.entry_price = -0.0;
    let output = sanitize_for_post_message(&input, false, false);
    assert_eq!(output.balance.unwrap().total, f64::INFINITY);
    assert!(output.orders.unwrap()[0].price.to_number().is_nan());
    assert_eq!(output.positions.unwrap()[0].contracts, f64::NEG_INFINITY);
    assert_eq!(
        output.displayed_positions.unwrap()[0]
            .position
            .entry_price
            .to_bits(),
        (-0.0_f64).to_bits()
    );
}

fn assert_snapshot_identity(
    actual: &safe_cex_webworker::payload::StoreWorkerStorePayload,
    expected: &Value,
) {
    macro_rules! check {
        ($value:expr, $source:expr; $($key:literal => $field:ident),+ $(,)?) => {
            $(assert_eq!(normalized(json!(&$value.$field)), normalized($source[$key].clone()), "SNAPSHOT-IDENTITY native {}", $key);)+
        };
    }
    let balance = actual.balance.as_ref().unwrap();
    check!(balance, expected["balance"]; "used"=>used,"free"=>free,"total"=>total,"upnl"=>upnl);
    assert_eq!(
        model_identity::market(&actual.markets.as_ref().unwrap()[0]),
        expected["markets"][0],
        "SNAPSHOT-IDENTITY native market"
    );
    let ticker = &actual.tickers.as_ref().unwrap()[0];
    check!(ticker,expected["tickers"][0]; "id"=>id,"symbol"=>symbol,"bid"=>bid,"ask"=>ask,"last"=>last,"mark"=>mark,"index"=>index,"percentage"=>percentage,"openInterest"=>open_interest,"fundingRate"=>funding_rate,"fundingTime"=>funding_time,"volume"=>volume,"quoteVolume"=>quote_volume);
    let order = &actual.orders.as_ref().unwrap()[0];
    check!(order,expected["orders"][0]; "id"=>id,"symbol"=>symbol,"price"=>price,"amount"=>amount,"filled"=>filled,"remaining"=>remaining,"lastUpdate"=>last_update,"reduceOnly"=>reduce_only);
    assert!(
        order.side == safe_cex_types::OrderSide::Buy,
        "SNAPSHOT-IDENTITY native order side"
    );
    assert!(
        order.r#type == safe_cex_types::OrderType::Limit,
        "SNAPSHOT-IDENTITY native order type"
    );
    assert!(
        order.status == safe_cex_types::OrderStatus::Open,
        "SNAPSHOT-IDENTITY native order status"
    );
    for (position, source) in [
        (
            &actual.positions.as_ref().unwrap()[0],
            &expected["positions"][0],
        ),
        (
            &actual.displayed_positions.as_ref().unwrap()[0].position,
            &expected["displayedPositions"][0],
        ),
    ] {
        check!(position,source; "id"=>id,"symbol"=>symbol,"entryPrice"=>entry_price,"contracts"=>contracts,"notional"=>notional,"leverage"=>leverage,"unrealizedPnl"=>unrealized_pnl,"liquidationPrice"=>liquidation_price);
        assert!(
            position.side == safe_cex_types::PositionSide::Long,
            "SNAPSHOT-IDENTITY native position side"
        );
    }
    assert_eq!(
        actual.displayed_positions.as_ref().unwrap()[0].account,
        expected["displayedPositions"][0]["account"],
        "SNAPSHOT-IDENTITY native account"
    );
}
