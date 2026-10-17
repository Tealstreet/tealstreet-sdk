use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_venue_kucoin::mappers::{map_order, map_spot_order};
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
};
#[test]
fn live_wire_order_mapping() {
    let market = json!({"id":"XBTUSDTM","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":1,"price":0.1},"limits":{"amount":{"min":1,"max":10000},"leverage":{"min":1,"max":100}},"contractSize":0.001});
    let base = json!({"id":"parent","symbol":"XBTUSDTM","type":"limit","side":"buy","status":"open","price":"100","size":"10","filledSize":"2","timeInForce":"GTC","createdAt":"1700000000000123456"});
    let mut cases = vec![];
    for venue in ["kucoin", "kucoinspot"] {
        cases.push(json!({"venue":venue,"row":base,"markets":[market]}));
        for (post_only, stop) in [(true, false), (false, true)] {
            let mut row = base.clone();
            row["postOnly"] = json!(post_only);
            row["stop"] = json!(stop);
            cases.push(json!({"venue":venue,"row":row,"markets":[market]}));
        }
        for (key, values) in [
            (
                "remainSize",
                vec![
                    json!(null),
                    json!(0),
                    json!("0"),
                    json!("3.5tail"),
                    json!(""),
                    json!("bad"),
                ],
            ),
            (
                "stop",
                vec![
                    json!(null),
                    json!(false),
                    json!(true),
                    json!(""),
                    json!("down"),
                    json!("up"),
                    json!("sl"),
                    json!("profit"),
                ],
            ),
            (
                "type",
                vec![
                    json!("market"),
                    json!("stop"),
                    json!("pos_profit"),
                    json!("pos_loss"),
                    json!(""),
                ],
            ),
            ("side", vec![json!("sell"), json!("BUY"), json!("")]),
            (
                "status",
                vec![
                    json!("filled"),
                    json!("done"),
                    json!("canceled"),
                    json!("unknown"),
                    json!(""),
                ],
            ),
            (
                "timeInForce",
                vec![
                    json!("IOC"),
                    json!("fok"),
                    json!("FOK"),
                    json!("post_only"),
                    json!(null),
                ],
            ),
            (
                "size",
                vec![
                    json!(0),
                    json!("0x10"),
                    json!("12tail"),
                    json!("Infinity"),
                    json!("-Infinity"),
                    json!("NaN"),
                    json!(true),
                    json!(["4"]),
                    json!({"toString":null}),
                ],
            ),
            (
                "createdAt",
                vec![
                    json!(null),
                    json!(0),
                    json!(" 17000000001234567"),
                    json!("\u{feff}1700000000123"),
                    json!("bad"),
                ],
            ),
        ] {
            for value in values {
                let mut row = base.clone();
                row[key] = value;
                cases.push(json!({"venue":venue,"row":row,"markets":[market]}));
            }
        }
        for side in ["buy", "sell"] {
            for (loss, profit) in [
                (json!(0), json!(null)),
                (json!("0"), json!("0")),
                (json!("95"), json!("105")),
            ] {
                let mut row = base.clone();
                row["side"] = json!(side);
                row["triggerStopDownPrice"] = loss;
                row["triggerStopUpPrice"] = profit;
                row["stopPrice"] = json!("99");
                row["stop"] = json!(true);
                row["stopPriceType"] = json!("TP");
                row["postOnly"] = json!(true);
                cases.push(json!({"venue":venue,"row":row,"markets":[market]}));
            }
        }
    }
    let expected = oracle(&cases);
    for (c, expected) in cases.iter().zip(expected) {
        let store = DefaultStore::new();
        Store::update(
            &store,
            StoreUpdate {
                markets: Some(serde_json::from_value(c["markets"].clone()).unwrap()),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let row = c["row"].as_object().unwrap();
        let actual = if c["venue"] == "kucoin" {
            map_order(row, &store, 1700000000123.0)
                .map(|v| serde_json::Value::Array(v.iter().map(tag_order).collect()))
        } else {
            map_spot_order(row, 1700000000123.0).map(|v| tag_order(&v))
        };
        let actual = match actual {
            Ok(v) => json!({"ok":v}),
            Err(e) => json!({"error":e}),
        };
        compare(&actual, &expected);
    }
}
#[test]
fn prototype_object_survives_serialization_but_typed_order_refuses() {
    let mut cases = vec![];
    for venue in ["kucoin", "kucoinspot"] {
        for field in ["status", "type", "timeInForce"] {
            let mut row = json!({"id":"p","symbol":"BTC-USDT","status":"open","type":"limit","timeInForce":"GTC"});
            row[field] = json!("__proto__");
            cases.push(
                json!({"venue":venue,"markets":[],"row":row,"field":field,"inspectInherited":true}),
            );
        }
    }
    for (case, reference) in cases.iter().zip(oracle(&cases)) {
        let (record, types) = if case["venue"] == "kucoin" {
            (&reference["ok"][0], &reference["$types"][0])
        } else {
            (&reference["ok"], &reference["$types"])
        };
        let field = case["field"].as_str().unwrap();
        assert_eq!(types[field], "object");
        assert_eq!(
            record[field],
            json!({}),
            "prototype object is retained, never omitted"
        );
        assert!(
            record.get("id").is_some(),
            "TS retains the rest of the record"
        );
        assert!(mapping_error(case).contains(&format!(
            "Order.{field} refuses inherited Prototype; whole record refused"
        )));
    }
}
fn mapping_error(case: &Value) -> String {
    let row = case["row"].as_object().unwrap();
    if case["venue"] == "kucoin" {
        map_order(row, &DefaultStore::new(), 1.0).unwrap_err()
    } else {
        map_spot_order(row, 1.0).unwrap_err()
    }
}
#[test]
fn malformed_identity_divergence_has_live_witness() {
    let mut cases = vec![];
    for venue in ["kucoin", "kucoinspot"] {
        for (field, value) in [("id", json!(42)), ("symbol", json!(42))] {
            let mut row = json!({"id":"p","symbol":"BTC-USDT"});
            row[field] = value;
            cases.push(json!({"venue":venue,"row":row,"markets":[]}));
        }
    }
    for (c, reference) in cases.iter().zip(oracle(&cases)) {
        let row = c["row"].as_object().unwrap();
        let error = if c["venue"] == "kucoin" {
            map_order(row, &DefaultStore::new(), 1.0).unwrap_err()
        } else {
            map_spot_order(row, 1.0).unwrap_err()
        };
        assert!(error.contains("must be a string"));
        if c["row"]["id"] == 42 || c["venue"] == "kucoinspot" {
            assert!(reference.get("ok").is_some());
        } else {
            assert!(reference.get("error").is_some());
        }
    }
}
fn tag_number(n: f64) -> Value {
    if n.is_finite() {
        json!(n)
    } else {
        json!({"$number":if n.is_nan(){"NaN"}else if n.is_sign_positive(){"Infinity"}else{"-Infinity"}})
    }
}
fn tag_order(order: &safe_cex_types::Order) -> Value {
    let mut value = json!(order);
    for (key, n) in [
        ("price", order.price.to_number()),
        ("amount", order.amount),
        ("filled", order.filled),
        ("remaining", order.remaining),
        ("lastUpdate", order.last_update),
    ] {
        value[key] = tag_number(n);
    }
    if let Some(extra) = order.extra.as_ref() {
        if let Some(n) = extra.remaining_native {
            value["extra"]["remainingNative"] = tag_number(n);
        }
        if let Some(n) = extra.filled_in_placement_units {
            value["extra"]["filledInPlacementUnits"] = tag_number(n);
        }
    }
    value
}
fn oracle(cases: &[Value]) -> Vec<Value> {
    let mut child = Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/mappers.cjs"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(cases).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(expected.len(), cases.len());
    expected
}
fn compare(a: &Value, b: &Value) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(
            a.as_f64(),
            b.as_f64(),
            "KuCoin mapper numeric field identity"
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                compare(a, b)
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len());
            for (k, a) in a {
                compare(a, b.get(k).unwrap())
            }
        }
        _ => assert_eq!(a, b, "KuCoin mapper field identity"),
    }
}

#[test]
fn inherited_functions_suppress_each_order_table_fallback() {
    let mut cases = vec![];
    for venue in ["kucoin", "kucoinspot"] {
        for field in ["status", "type", "timeInForce"] {
            for key in [
                "constructor",
                "toString",
                "valueOf",
                "hasOwnProperty",
                "isPrototypeOf",
                "propertyIsEnumerable",
                "toLocaleString",
                "__defineGetter__",
                "__defineSetter__",
                "__lookupGetter__",
                "__lookupSetter__",
            ] {
                let mut row = json!({"id":"prototype","symbol":"BTC-USDT","status":"unknown","type":"unknown","timeInForce":"unknown"});
                row[field] = json!(key);
                cases.push(json!({"venue":venue,"markets":[],"row":row,"field":field,"inspectInherited":true}));
            }
        }
    }
    for (case, reference) in cases.iter().zip(oracle(&cases)) {
        let source = if case["venue"] == "kucoin" {
            &reference["ok"][0]
        } else {
            &reference["ok"]
        };
        let field = case["field"].as_str().unwrap();
        let types = if case["venue"] == "kucoin" {
            &reference["$types"][0]
        } else {
            &reference["$types"]
        };
        assert_eq!(
            types[field], "function",
            "source typeof before JSON serialization: {case}"
        );
        assert!(
            source.get(field).is_none(),
            "function-valued field must be omitted by source JSON: {case}"
        );
        assert!(
            source.get("id").is_some(),
            "TS retains the record with one missing field"
        );
        assert!(
            mapping_error(case).contains(&format!("Order.{field} refuses inherited")),
            "typed whole-record refusal: {case}"
        );
    }
}

#[test]
fn source_overrides_can_replace_inherited_values_before_typed_output() {
    let mut cases = vec![];
    for key in ["constructor", "__proto__"] {
        for (stop_type, stop_price_type) in [("down", ""), ("up", ""), ("unknown", "TP")] {
            cases.push(json!({"venue":"kucoin","markets":[],"row":{"id":"p","symbol":"BTC-USDT","type":key,"stop":true,"stopType":stop_type,"stopPriceType":stop_price_type,"stopPrice":"2"}}));
        }
        cases.push(json!({"venue":"kucoinspot","markets":[],"row":{"id":"p","symbol":"BTC-USDT","timeInForce":key,"postOnly":true}}));
    }
    for (case, reference) in cases.iter().zip(oracle(&cases)) {
        let row = case["row"].as_object().unwrap();
        let actual = if case["venue"] == "kucoin" {
            json!({"ok":map_order(row,&DefaultStore::new(),1_700_000_000_123.0).unwrap().iter().map(tag_order).collect::<Vec<_>>()})
        } else {
            json!({"ok":tag_order(&map_spot_order(row,1_700_000_000_123.0).unwrap())})
        };
        compare(&actual, &reference);
    }
}

#[test]
fn source_own_only_table_mutation_fails_pre_serialization_witness() {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "inherited_functions_suppress_each_order_table_fallback",
            "--nocapture",
        ])
        .env(
            "NODE_OPTIONS",
            concat!(
                "--require=",
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/oracle-controls.cjs"
            ),
        )
        .env("KUCOIN_ORACLE_MUTATION", "mapper-prototype")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("source typeof before JSON serialization")
    );
}

#[test]
fn attached_order_coercion_precedes_whole_batch_output_refusal() {
    let mut cases = vec![];
    for inherited in ["constructor", "__proto__"] {
        for price in [json!("2"), json!({"toString":null})] {
            cases.push(json!({"venue":"kucoin","markets":[],"row":{"id":"p","symbol":"BTC-USDT","type":inherited,"side":"buy","stopLoss":price},"inspectInherited":true}));
        }
    }
    for (case, reference) in cases.iter().zip(oracle(&cases)) {
        let error = mapping_error(case);
        if case["row"]["stopLoss"].is_object() {
            assert_eq!(
                json!({"error":error}),
                reference,
                "attached coercion must precede typed output refusal"
            );
        } else {
            assert!(error.contains("Order.type refuses inherited"));
            assert_eq!(reference["ok"].as_array().unwrap().len(), 2);
            assert_eq!(reference["ok"][1]["type"], "stop_market");
            assert_eq!(reference["ok"][1]["price"], 2);
            assert_eq!(reference["ok"][1]["id"], "p__stop_loss");
        }
    }
}

#[test]
fn live_spot_mapper_source_error_precedes_typed_refusal() {
    let base = json!({"id":"identity","symbol":"BTC-USDT","size":"7","dealSize":"2","remainSize":"5","price":"11","status":"open","type":"limit","timeInForce":"GTC","createdAt":"1700000000123"});
    let mut cases = vec![];
    for identity in ["id", "symbol"] {
        for field in [
            "size",
            "dealSize",
            "remainSize",
            "timeInForce",
            "status",
            "type",
            "price",
            "createdAt",
        ] {
            let mut row = base.clone();
            row[identity] = json!(42);
            row[field] = json!({"toString":null});
            cases.push(json!({"venue":"kucoinspot","markets":[],"row":row}));
        }
    }
    for inherited in ["constructor", "__proto__", "__lookupGetter__"] {
        for field in ["status", "type", "timeInForce"] {
            let mut row = base.clone();
            row[field] = json!(inherited);
            row["price"] = json!({"toString":null});
            cases.push(json!({"venue":"kucoinspot","markets":[],"row":row}));
        }
    }
    for (case, expected) in cases.iter().zip(oracle(&cases)) {
        assert_eq!(
            expected["error"],
            "Cannot convert object to primitive value"
        );
        assert_eq!(
            map_spot_order(case["row"].as_object().unwrap(), 1.0).unwrap_err(),
            expected["error"].as_str().unwrap(),
            "Spot mapper source error precedes typed refusal: {}",
            case["row"]
        );
    }
    let mut row = base;
    row["postOnly"] = json!(true);
    row["timeInForce"] = json!({"toString":null});
    let case = json!({"venue":"kucoinspot","markets":[],"row":row});
    let expected = oracle(&[case]);
    compare(
        &tag_order(&map_spot_order(row.as_object().unwrap(), 1.0).unwrap()),
        &expected[0]["ok"],
    );
}

#[test]
fn live_futures_mapper_coercion_message() {
    let mut cases = vec![];
    for field in [
        "size",
        "filledSize",
        "remainSize",
        "price",
        "updateTime",
        "status",
        "timeInForce",
        "type",
        "stopPrice",
        "stopType",
    ] {
        let mut row = json!({"id":"identity","symbol":"BTC-USDT","size":"7","filledSize":"2","remainSize":"5","price":"11","status":"open","type":"limit","timeInForce":"GTC","createdAt":"1700000000123"});
        row[field] = json!({"toString":null});
        if field == "stopPrice" || field == "stopType" {
            row["stop"] = json!(true);
        }
        cases.push(json!({"venue":"kucoin","markets":[],"row":row}));
    }
    for (case, expected) in cases.iter().zip(oracle(&cases)) {
        assert_eq!(
            map_order(case["row"].as_object().unwrap(), &DefaultStore::new(), 1.0).unwrap_err(),
            expected["error"].as_str().unwrap(),
            "Futures mapper original coercion message: {}",
            case["row"]
        );
    }
}

#[test]
fn attached_order_cardinality_supplement_retains_parent_and_children() {
    let row = json!({"id":"parent","symbol":"XBTUSDTM","type":"limit","side":"buy",
        "status":"open","size":"7","filledSize":"2","price":"13",
        "stopLoss":"11","takeProfit":"17","createdAt":1700000000123_i64});
    let source = oracle(&[json!({"venue":"kucoin","row":row,"markets":[]})]);
    assert_eq!(source.len(), 1, "KUCOIN-CARDINALITY mapper source cases");
    assert_eq!(
        source[0]["ok"].as_array().unwrap().len(),
        3,
        "KUCOIN-CARDINALITY mapper source rows"
    );
    let native = map_order(
        row.as_object().unwrap(),
        &DefaultStore::new(),
        1700000000123.0,
    )
    .unwrap();
    assert_eq!(
        native.len(),
        3,
        "KUCOIN-CARDINALITY mapper native retained rows"
    );
    let mut comparisons = 0;
    for index in 0..3 {
        assert_eq!(
            native[index].id,
            ["parent", "parent__stop_loss", "parent__take_profit"][index]
        );
        assert_eq!(native[index].id, source[0]["ok"][index]["id"]);
        comparisons += 1;
    }
    assert_eq!(
        comparisons, 3,
        "KUCOIN-CARDINALITY mapper native comparisons"
    );
}
