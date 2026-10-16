use safe_cex_types::{Order, OrderStopTriggerType, PlaceOrderOpts, Position, PositionMode};
use safe_cex_venue_kucoin::order_helpers::*;
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
};
fn result(value: Result<&str, String>) -> Value {
    match value {
        Ok(value) => json!({"ok":value}),
        Err(error) => json!({"error":error}),
    }
}
#[test]
fn live_order_decisions() {
    let mut cases = vec![];
    for ty in [
        "market",
        "limit",
        "conditional",
        "stop_market",
        "take_profit_market",
        "trailing_stop_market",
    ] {
        for side in ["buy", "sell"] {
            for reduce in [None, Some(false), Some(true)] {
                for mode in ["oneway", "hedged"] {
                    for position_mode in [None, Some("oneway"), Some("hedged")] {
                        let mut opts = json!({"symbol":"BTCUSDT","type":ty,"side":side,"amount":1});
                        if let Some(reduce) = reduce {
                            opts["reduceOnly"] = json!(reduce);
                        }
                        let mut position = json!({"id":"p","symbol":"BTCUSDT","side":"long","entryPrice":1,"notional":17,"leverage":3,"unrealizedPnl":19,"contracts":2,"liquidationPrice":23,"marginType":"cross"});
                        if let Some(mode) = position_mode {
                            position["positionMode"] = json!(mode);
                        }
                        let mut short = position.clone();
                        short["side"] = json!("short");
                        short["id"] = json!("s");
                        short["marginType"] = json!("isolated");
                        for trigger in [
                            None,
                            Some("last_price"),
                            Some("mark_price"),
                            Some("index_price"),
                        ] {
                            cases.push(json!({"opts":opts,"positions":[position.clone(),short],"position":position,"mode":mode,"id":"parent__stop_loss","trigger":trigger}));
                        }
                    }
                }
            }
        }
    }
    for id in [
        "plain",
        "__stop_loss",
        "x__take_profit",
        "x__stop_loss_suffix",
        "x__STOP_LOSS",
        "",
    ] {
        let mut case = cases[0].clone();
        case["id"] = json!(id);
        case["positions"] = json!([]);
        case["position"] = Value::Null;
        cases.push(case);
    }
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/order-helpers.cjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&cases).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        cases.len(),
        870,
        "KUCOIN-CARDINALITY order_helpers input total"
    );
    assert_eq!(
        expected.len(),
        870,
        "KUCOIN-CARDINALITY order_helpers source total"
    );
    let mut compared_cases = 0;
    for index in 0..870 {
        let case = &cases[index];
        let expected = &expected[index];
        compared_cases += 1;
        let opts: PlaceOrderOpts = serde_json::from_value(case["opts"].clone()).unwrap();
        let positions: Vec<Position> = serde_json::from_value(case["positions"].clone()).unwrap();
        let position: Option<Position> = serde_json::from_value(case["position"].clone()).unwrap();
        let mode: PositionMode = serde_json::from_value(case["mode"].clone()).unwrap();
        let trigger: Option<OrderStopTriggerType> =
            serde_json::from_value(case["trigger"].clone()).unwrap();
        let order:Order=serde_json::from_value(json!({"id":case["id"],"symbol":"BTCUSDT","type":"limit","side":"buy","status":"open","price":1,"amount":1,"filled":0,"remaining":1,"reduceOnly":false,"timeInForce":"GoodTillCancel","lastUpdate":1})).unwrap();
        let actual = json!({
            "getOrderPlanType":result(get_order_plan_type(opts.r#type)),
            "getOrderSideV2":{"ok":get_order_side_v2(&opts,mode)},
            "getOrderSide":{"ok":get_order_side(&opts,mode)},
            "getAlgoOrderSide":result(get_algo_order_side(&opts)),
            "getPositionForOrder":{"ok":get_position_for_order(&opts,&positions)},
            "getKucoinMarginMode":{"ok":get_kucoin_margin_mode(position.as_ref())},
            "getKucoinPositionSide":{"ok":get_kucoin_position_side(&opts,position.as_ref())},
            "mapStopPriceType":{"ok":map_stop_price_type(trigger)},
            "getKucoinStopDirection":result(get_kucoin_stop_direction(&opts)),
            "isVirtualTPSLOrder":{"ok":is_virtual_tpsl_order(&order)},
            "isAlgoOrder":{"ok":is_algo_order(&opts)},
        });
        compare(&actual, expected);
    }
    assert_eq!(
        compared_cases, 870,
        "KUCOIN-CARDINALITY order_helpers compared cases"
    );
}
fn compare(a: &Value, b: &Value) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(a.as_f64(), b.as_f64()),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                compare(a, b);
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len());
            for (k, a) in a {
                compare(a, b.get(k).unwrap());
            }
        }
        _ => assert_eq!(a, b),
    }
}
