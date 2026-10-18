use safe_cex_num::numeric_ingress::js_from_str;
use safe_cex_types::{OrderSide, OrderStatus, OrderTimeInForce, OrderType};
use safe_cex_venue_backpack::{order_utils, private_order_utils as private, types::BackpackOrder};
use serde_json::{Value, json};
use std::{cell::RefCell, process::Command, rc::Rc};

fn oracle() -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/order_mapping_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn bits(n: f64) -> String {
    format!(
        "{:016x}",
        if n.is_nan() {
            0x7ff8000000000000
        } else {
            n.to_bits()
        }
    )
}
fn kind(v: &Value) -> OrderType {
    match v.as_str().unwrap() {
        "limit" => OrderType::Limit,
        "market" => OrderType::Market,
        "stop_market" => OrderType::StopLoss,
        "take_profit_market" => OrderType::TakeProfit,
        s => panic!("unexpected type {s}"),
    }
}
fn status(v: &Value) -> OrderStatus {
    match v.as_str().unwrap() {
        "open" => OrderStatus::Open,
        "closed" => OrderStatus::Closed,
        "canceled" => OrderStatus::Canceled,
        s => panic!("unexpected status {s}"),
    }
}
fn tif(v: &Value) -> OrderTimeInForce {
    match v.as_str().unwrap() {
        "GoodTillCancel" => OrderTimeInForce::GoodTillCancel,
        "ImmediateOrCancel" => OrderTimeInForce::ImmediateOrCancel,
        "FillOrKill" => OrderTimeInForce::FillOrKill,
        "PostOnly" => OrderTimeInForce::PostOnly,
        s => panic!("unexpected tif {s}"),
    }
}
fn mapping_inputs() -> Vec<Vec<u16>> {
    let mut inputs: Vec<Vec<u16>> = [
        "Limit", "limit", "LIMIT", "Market", "market", "MARKET", "mArKeT", "marKet", "MaRKeT", "",
        "Market ", "stop", "λ",
    ]
    .iter()
    .map(|s| s.encode_utf16().collect())
    .collect();
    inputs.push(vec![0xd800]);
    inputs.extend(
        [
            "New",
            "PartiallyFilled",
            "Filled",
            "Cancelled",
            "Expired",
            "TriggerPending",
            "new",
            "GTC",
            "IOC",
            "FOK",
            "PO",
            "ioc",
        ]
        .iter()
        .map(|s| s.encode_utf16().collect::<Vec<_>>()),
    );
    inputs
}
fn mappings(c: &Value) -> (&Vec<Value>, Vec<Vec<u16>>) {
    let rows = c["mappings"].as_array().unwrap();
    let inputs = mapping_inputs();
    assert_eq!(rows.len(), 26, "PRIVATE-MAPPING independent cases");
    assert_eq!(inputs.len(), 26);
    for (i, (row, input)) in rows.iter().zip(&inputs).enumerate() {
        assert_eq!(
            row["input"],
            json!(input),
            "PRIVATE-MAPPING independent UTF16 input {i}"
        );
    }
    (rows, inputs)
}
#[test]
fn actual_typescript_private_order_type() {
    let c = oracle();
    let (rows, inputs) = mappings(&c);
    for (i, (row, input)) in rows.iter().zip(inputs).enumerate() {
        assert_eq!(
            private::map_order_type(&input),
            kind(&row["type"]),
            "PRIVATE-TYPE native enum case {i}"
        );
    }
}
#[test]
fn actual_typescript_private_order_status() {
    let c = oracle();
    let (rows, inputs) = mappings(&c);
    for (i, (row, input)) in rows.iter().zip(inputs).enumerate() {
        assert_eq!(
            private::map_order_status(&input),
            status(&row["status"]),
            "PRIVATE-STATUS native enum case {i}"
        );
    }
}
#[test]
fn actual_typescript_private_time_in_force() {
    let c = oracle();
    let (rows, inputs) = mappings(&c);
    for (i, (row, input)) in rows.iter().zip(inputs).enumerate() {
        assert_eq!(
            private::map_time_in_force(&input),
            tif(&row["tif"]),
            "PRIVATE-TIF native enum case {i}"
        );
    }
}
#[test]
fn actual_typescript_private_trigger_type() {
    let c = oracle();
    let rows = c["classifications"].as_array().unwrap();
    assert_eq!(rows.len(), 198, "PRIVATE-TRIGGER independent cases");
    let utf16 = |s: &str| s.encode_utf16().collect::<Vec<_>>();
    let btc = utf16("BTC");
    let mut surrogate = btc.clone();
    surrogate.push(0xd800);
    let ticker = |last, mark| (btc.clone(), last, mark);
    let scenarios = [
        vec![],
        vec![(utf16("OTHER"), Some(100.0), Some(200.0))],
        vec![ticker(None, Some(100.0))],
        vec![ticker(Some(0.0), Some(100.0))],
        vec![ticker(Some(-0.0), Some(100.0))],
        vec![ticker(Some(f64::NAN), Some(100.0))],
        vec![ticker(Some(f64::INFINITY), Some(100.0))],
        vec![ticker(Some(f64::NEG_INFINITY), Some(100.0))],
        vec![ticker(Some(100.0), Some(200.0))],
        vec![
            ticker(Some(100.0), Some(200.0)),
            ticker(Some(1000.0), Some(2000.0)),
        ],
        vec![(surrogate.clone(), Some(100.0), Some(200.0))],
    ];
    let triggers = [
        None,
        Some(f64::NEG_INFINITY),
        Some(-0.0),
        Some(0.0),
        Some(99.0),
        Some(100.0),
        Some(101.0),
        Some(f64::INFINITY),
        Some(f64::NAN),
    ];
    for (i, row) in rows.iter().enumerate() {
        let scenario = i / 18;
        let side = [OrderSide::Buy, OrderSide::Sell][(i / 9) % 2];
        let trigger = triggers[i % 9];
        let tickers = &scenarios[scenario];
        let symbol = if scenario == 10 { &surrogate } else { &btc };
        assert_eq!(row["scenario"], scenario);
        assert_eq!(row["symbol"], json!(symbol));
        assert_eq!(
            row["side"],
            if side == OrderSide::Buy {
                "buy"
            } else {
                "sell"
            }
        );
        assert_eq!(
            row["trigger"],
            json!(trigger.map(bits)),
            "PRIVATE-TRIGGER independent argument {i}"
        );
        assert_eq!(
            row["tickers"],
            json!(
                tickers
                    .iter()
                    .map(|(symbol, last, mark)| json!([symbol, last.map(bits), mark.map(bits)]))
                    .collect::<Vec<_>>()
            ),
            "PRIVATE-TRIGGER independent ticker content {i}"
        );
        assert_eq!(
            private::determine_trigger_order_type(tickers, symbol, side, trigger),
            kind(&row["result"]),
            "PRIVATE-TRIGGER native enum case {i}"
        );
    }
}
fn order_inputs() -> Vec<(&'static str, Value)> {
    let base = json!({"id":"order λ","symbol":"BTC","side":"Bid","orderType":"Limit","price":"105.25","quantity":"8.5","timeInForce":"GTC","selfTradePrevention":"Allow","status":"New","createdAt":1234.5,"executedQuantity":"2.25"});
    let patches = vec![
        ("base", json!({})),
        ("market", json!({"orderType":"Market"})),
        (
            "trigger buy",
            json!({"orderType":"Market","triggerPrice":"101","triggerQuantity":"6"}),
        ),
        (
            "trigger sell",
            json!({"orderType":"Market","side":"Ask","triggerPrice":"99","triggerQuantity":"6"}),
        ),
        (
            "trigger equality",
            json!({"orderType":"Market","triggerPrice":"100"}),
        ),
        (
            "trigger nan",
            json!({"orderType":"Market","triggerPrice":"bad"}),
        ),
        (
            "trigger zero",
            json!({"orderType":"Market","triggerPrice":"0"}),
        ),
        (
            "trigger minus zero",
            json!({"orderType":"Market","triggerPrice":"-0"}),
        ),
        (
            "limit trigger",
            json!({"triggerPrice":"12","triggerQuantity":"3"}),
        ),
        ("empty trigger", json!({"triggerPrice":""})),
        (
            "empty trigger quantity",
            json!({"orderType":"Market","triggerPrice":"99","triggerQuantity":""}),
        ),
        (
            "zero quantity",
            json!({"orderType":"Market","triggerPrice":"99","quantity":"0"}),
        ),
        (
            "nan quantity",
            json!({"orderType":"Market","triggerPrice":"99","quantity":"bad"}),
        ),
        (
            "negative zero quantity",
            json!({"orderType":"Market","triggerPrice":"99","triggerQuantity":"-0"}),
        ),
        ("infinite quantity", json!({"quantity":"Infinity"})),
        ("missing filled", json!({"executedQuantity":null})),
        ("empty filled", json!({"executedQuantity":""})),
        ("bad filled", json!({"executedQuantity":"bad"})),
        ("negative zero filled", json!({"executedQuantity":"-0"})),
        (
            "reduce false",
            json!({"orderType":"Market","triggerPrice":"101","reduceOnly":false}),
        ),
        ("reduce true", json!({"reduceOnly":true})),
        ("tif po", json!({"timeInForce":"PO"})),
        ("tif ioc", json!({"timeInForce":"IOC"})),
        ("tif fok", json!({"timeInForce":"FOK"})),
        ("status partial", json!({"status":"PartiallyFilled"})),
        ("status filled", json!({"status":"Filled"})),
        ("status cancelled", json!({"status":"Cancelled"})),
        ("status expired", json!({"status":"Expired"})),
        ("status trigger pending", json!({"status":"TriggerPending"})),
        (
            "price prefix",
            json!({"price":" 7.25junk","quantity":"9.5tail"}),
        ),
        ("price nan", json!({"price":"bad"})),
        ("quantity negative", json!({"quantity":"-5"})),
    ];
    patches
        .into_iter()
        .map(|(label, patch)| {
            let mut input = base.clone();
            for (key, value) in patch.as_object().unwrap() {
                if value.is_null() {
                    input.as_object_mut().unwrap().remove(key);
                } else {
                    input[key] = value.clone();
                }
            }
            (label, input)
        })
        .collect()
}
#[test]
fn actual_typescript_map_order() {
    let c = oracle();
    let rows = c["orders"].as_array().unwrap();
    let inputs = order_inputs();
    assert_eq!(inputs.len(), 32, "MAP-ORDER independent inputs");
    assert_eq!(rows.len(), 96, "MAP-ORDER independent cases");
    let mut completed = [0; 2];
    for (i, row) in rows.iter().enumerate() {
        let index = i / 3;
        let (label, input) = &inputs[index];
        let failure = ["none", "classify", "log"][i % 3];
        assert_eq!(row["index"], index);
        assert_eq!(row["label"], *label);
        assert_eq!(row["failure"], failure);
        assert_eq!(&row["input"], input, "MAP-ORDER independent input {i}");
        let order = js_from_str::<BackpackOrder>(&input.to_string()).unwrap();
        assert_eq!(order.id, input["id"]);
        assert_eq!(order.symbol, input["symbol"]);
        assert_eq!(order.price, input["price"]);
        assert_eq!(order.quantity, input["quantity"]);
        assert_eq!(
            order.triggerPrice.as_deref(),
            input["triggerPrice"].as_str()
        );
        assert_eq!(
            order.triggerQuantity.as_deref(),
            input["triggerQuantity"].as_str()
        );
        assert_eq!(
            order.executedQuantity.as_deref(),
            input["executedQuantity"].as_str()
        );
        assert_eq!(order.reduceOnly, input["reduceOnly"].as_bool());
        assert_eq!(order.createdAt, 1234.5);
        let trace = RefCell::new(Vec::<Value>::new());
        let opaque = Rc::new("original failure".to_string());
        let result = order_utils::map_order(
            &order,
            |symbol, side, trigger| {
                trace.borrow_mut().push(json!([
                    "classify",
                    symbol,
                    if side == OrderSide::Buy {
                        "buy"
                    } else {
                        "sell"
                    },
                    bits(trigger)
                ]));
                if failure == "classify" {
                    Err(opaque.clone())
                } else {
                    Ok(order_utils::determine_trigger_order_type(
                        &[order_utils::TickerPrice {
                            symbol: "BTC",
                            last: Some(100.0),
                            mark: Some(200.0),
                        }],
                        symbol,
                        side,
                        trigger,
                    ))
                }
            },
            |message, severity| {
                trace.borrow_mut().push(json!(["log", message, severity]));
                if failure == "log" {
                    Err(opaque.clone())
                } else {
                    Ok(())
                }
            },
        );
        let classifies = [2, 3, 4, 5, 6, 7, 10, 11, 12, 13, 19].contains(&index);
        let warns = [11, 12, 13].contains(&index);
        let rejected = (failure == "classify" && classifies) || (failure == "log" && warns);
        assert_eq!(
            trace.borrow().len(),
            usize::from(classifies) + usize::from(warns && !(failure == "classify" && classifies)),
            "MAP-ORDER independent callback count {i}"
        );
        if rejected {
            let actual = result.expect_err("native rejection");
            assert!(
                Rc::ptr_eq(&actual, &opaque),
                "MAP-ORDER native original error identity {i}"
            );
            assert_eq!(row["outcome"]["ok"], false);
            assert_eq!(row["outcome"]["sameFailure"], true);
            completed[1] += 1;
        } else {
            let actual = result.unwrap();
            let expected = &row["outcome"]["values"];
            assert_eq!(actual.id, expected["id"], "MAP-ORDER native id {i}");
            assert_eq!(actual.symbol, expected["symbol"]);
            assert_eq!(
                actual.r#type,
                kind(&expected["type"]),
                "MAP-ORDER native type {i}"
            );
            assert_eq!(
                actual.side,
                if expected["side"] == "buy" {
                    OrderSide::Buy
                } else {
                    assert_eq!(expected["side"], "sell");
                    OrderSide::Sell
                }
            );
            assert_eq!(
                bits((actual.price).to_number()),
                expected["price"],
                "MAP-ORDER native price bits {i}"
            );
            assert_eq!(
                bits(actual.amount),
                expected["amount"],
                "MAP-ORDER native amount bits {i}"
            );
            assert_eq!(
                bits(actual.filled),
                expected["filled"],
                "MAP-ORDER native filled bits {i}"
            );
            assert_eq!(
                bits(actual.remaining),
                expected["remaining"],
                "MAP-ORDER native remaining bits {i}"
            );
            assert_eq!(
                bits(actual.last_update),
                expected["lastUpdate"],
                "MAP-ORDER native timestamp bits {i}"
            );
            assert_eq!(actual.status, status(&expected["status"]));
            assert_eq!(
                actual.reduce_only,
                expected["reduceOnly"].as_bool().unwrap()
            );
            assert_eq!(actual.time_in_force, Some(tif(&expected["timeInForce"])));
            assert!(
                actual.parent_id.is_none()
                    && actual.close.is_none()
                    && actual.info.is_none()
                    && actual.trigger_type.is_none()
                    && actual.tp_trigger_type.is_none()
                    && actual.sl_trigger_type.is_none()
                    && actual.take_profit.is_none()
                    && actual.stop_loss.is_none()
                    && actual.extra.is_none()
            );
            assert_eq!(row["outcome"]["ok"], true);
            assert_eq!(
                row["outcome"]["keys"],
                json!([
                    "id",
                    "symbol",
                    "type",
                    "side",
                    "price",
                    "amount",
                    "filled",
                    "remaining",
                    "status",
                    "lastUpdate",
                    "reduceOnly",
                    "timeInForce"
                ])
            );
            completed[0] += 1;
        }
        assert_eq!(
            trace.borrow().as_slice(),
            row["trace"].as_array().unwrap(),
            "MAP-ORDER native callback trace {i}"
        );
    }
    assert_eq!(
        completed,
        [82, 14],
        "MAP-ORDER independent completed assertions"
    );
    let mut boundary = inputs[0].1.clone();
    boundary["reduceOnly"] = Value::Null;
    boundary["triggerPrice"] = Value::Null;
    boundary["executedQuantity"] = Value::Null;
    assert_eq!(
        c["boundary"]["input"], boundary,
        "MAP-ORDER independent boundary input"
    );
    assert!(
        js_from_str::<BackpackOrder>(&boundary.to_string()).is_err(),
        "MAP-ORDER native null optional ingress refusal"
    );
    assert_eq!(c["boundary"]["reduceOnly"], false);
    assert_eq!(c["boundary"]["filled"], bits(0.0));
    assert_eq!(c["boundary"]["remaining"], bits(8.5));
}
