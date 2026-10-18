use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::js_number_string;
use safe_cex_store::Store;
use safe_cex_types::{PlaceOrderOpts, Position, retained_json::RetainedJsonInput as Input};
use safe_cex_venue_bitget::{
    inverse_logic::*, normalization::*, numeric::*, order_logic::*, pnl::*,
};
use serde_json::{Value, json};
fn number(v: &Value) -> f64 {
    match v["$number"].as_str().unwrap() {
        "NaN" => f64::NAN,
        "Infinity" => f64::INFINITY,
        "-Infinity" => f64::NEG_INFINITY,
        s => s.parse().unwrap(),
    }
}
fn encoded(n: f64) -> Value {
    json!({"$number":if n==0.0 && n.is_sign_negative(){"-0".into()}else{js_number_string(n)}})
}
fn decode(v: &Value) -> Value {
    if v.get("$number").is_some() {
        return json!(number(v));
    }
    match v {
        Value::Array(items) => items.iter().map(decode).collect(),
        Value::Object(fields) => {
            Value::Object(fields.iter().map(|(k, v)| (k.clone(), decode(v))).collect())
        }
        _ => v.clone(),
    }
}
fn retained(v: &Value) -> Input {
    if v.get("$number").is_some() {
        return Input::Float(number(v));
    }
    if v.get("$undefined") == Some(&json!(true)) {
        return Input::Undefined;
    }
    match v {
        Value::Array(items) => Input::Array(items.iter().map(retained).collect()),
        Value::Object(fields) => Input::Object(
            fields
                .iter()
                .map(|(key, value)| (key.encode_utf16().collect(), retained(value)))
                .collect(),
        ),
        _ => v.clone().into(),
    }
}
fn error(e: ExchangeError) -> Value {
    json!({"error":{"name":e.name,"message":e.message}})
}
fn finite_input(v: &Value) -> FiniteNumberInput<'_> {
    if v.get("$number").is_some() {
        FiniteNumberInput::Number(number(v))
    } else if let Some(s) = v.as_str() {
        FiniteNumberInput::String(s)
    } else {
        FiniteNumberInput::Other
    }
}
#[path = "../../parity/evidence/retained_model_values.rs"]
pub mod retained_model_values;
fn encode_json_numbers(value: &(impl serde::Serialize + ?Sized)) -> Value {
    retained_model_values::observe(value, encoded)
}

fn encoded_position(p: Position) -> Value {
    let mut value = json!({
        "id":p.id,"symbol":p.symbol,"side":p.side,"entryPrice":encoded(p.entry_price),
        "notional":encoded(p.notional),"leverage":encoded(p.leverage),
        "unrealizedPnl":encoded(p.unrealized_pnl),"realizedPnl":encoded(p.realized_pnl.unwrap()),
        "contracts":encoded(p.contracts),"liquidationPrice":encoded(p.liquidation_price),
        "positionMode":p.position_mode,"marginType":p.margin_type,
        "lastUpdate":encoded(p.last_update.unwrap())
    });
    if let Some(info) = p.info.into_value() {
        value["info"] = encode_json_numbers(&info);
    }
    if let Some(margin) = p.margin_size.into_value() {
        value["marginSize"] = encoded(margin);
    }
    value
}
// Native field presence is checked before the legacy JSON fixture comparison.
fn omit_undefined(value: &Value) -> Value {
    match value {
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .filter(|(_, v)| {
                    v.get("$undefined") != Some(&json!(true))
                        && v.get("undefined") != Some(&json!(true))
                })
                .map(|(k, v)| (k.clone(), omit_undefined(v)))
                .collect(),
        ),
        Value::Array(values) => values.iter().map(omit_undefined).collect(),
        _ => value.clone(),
    }
}
// Observe the native property state before JSON projection erases own undefined.
// Walk every output/property: a successful first row cannot hide later rows.
fn native_undefined_fields(order: &safe_cex_types::Order, expected: &Value, name: &Value) -> bool {
    let mut found = false;
    for (field, value) in expected.as_object().unwrap() {
        if value.get("$undefined") != Some(&json!(true)) {
            continue;
        }
        found = true;
        let undefined = match field.as_str() {
            "type" => order.r#type.is_undefined(),
            "status" => order.status.is_undefined(),
            "side" => order.side.is_undefined(),
            "timeInForce" => order.time_in_force.is_undefined(),
            "price" => order.price.is_undefined(),
            other => panic!("unreviewed source own undefined field {other}: {name}"),
        };
        assert!(undefined, "native own undefined {field}: {name}");
    }
    found
}
fn check_mapped_orders(oracle: &Value, field_identity: bool) {
    use safe_cex_venue_bitget::orders::map_order;
    let mut accepted = 0;
    let mut refused = 0;
    let mut visited = 0;
    let mut source_success = 0;
    let mut accepted_undefined = 0;
    for case in oracle["mapOrderReference"]
        .as_array()
        .unwrap()
        .iter()
        .chain(oracle["mapOrderCases"].as_array().unwrap())
    {
        visited += 1;
        let input = retained(&case["input"]);
        let mut clock_calls = 0;
        let actual = map_order(
            Some(&input),
            case.get("product")
                .and_then(Value::as_str)
                .unwrap_or("usdt-futures"),
            || {
                clock_calls += 1;
                1700000000123.0
            },
        );
        assert_eq!(
            json!(clock_calls),
            case["clockCalls"],
            "mapOrder clock {}",
            case["name"]
        );
        if case["result"].get("error").is_some() {
            assert_eq!(
                error(
                    actual.expect_err(&format!("mapOrder expected source throw {}", case["name"]))
                ),
                case["result"],
                "mapOrder source throw {}",
                case["name"]
            );
            continue;
        }
        source_success += 1;
        let expected = case["result"]["value"].as_array().unwrap();
        // Test the actual shared type's accepted set. Replace only tagged numeric
        // scalars for this shape check; compare their original bits below.
        let mut representable = true;
        for item in expected {
            let mut shape = decode(&omit_undefined(item));
            for field in ["price", "amount", "filled", "remaining", "lastUpdate"] {
                shape[field] = json!(0);
            }
            representable &= serde_json::from_value::<safe_cex_types::Order>(shape).is_ok();
        }
        if !representable {
            refused += 1;
            assert_eq!(
                actual
                    .expect_err(&format!("mapOrder typed output refusal {}", case["name"]))
                    .name,
                "RepresentationError",
                "mapOrder typed output refusal {}",
                case["name"]
            );
            continue;
        }
        accepted += 1;
        let actual = actual.unwrap_or_else(|e| panic!("mapOrder {}: {e:?}", case["name"]));
        assert_eq!(
            actual.len(),
            expected.len(),
            "native mapOrder cardinality {}",
            case["name"]
        );
        let mut own_undefined = false;
        for (order, expected) in actual.iter().zip(expected) {
            assert_eq!(
                encode_json_numbers(order.info.as_ref().unwrap()),
                encode_json_numbers(&input),
                "raw info retains every native field before fixture comparison"
            );
            own_undefined |= native_undefined_fields(order, expected, &case["name"]);
        }
        accepted_undefined += usize::from(own_undefined);
        let actual: Vec<_> = actual
            .into_iter()
            .map(|order| {
                let mut value = encode_json_numbers(&order);
                for (key, undefined) in [
                    ("status", order.status.is_undefined()),
                    ("type", order.r#type.is_undefined()),
                    ("side", order.side.is_undefined()),
                    ("timeInForce", order.time_in_force.is_undefined()),
                ] {
                    if undefined {
                        value.as_object_mut().unwrap().remove(key);
                    }
                }
                for (key, n) in [
                    ("price", order.price.to_number()),
                    ("amount", order.amount),
                    ("filled", order.filled),
                    ("remaining", order.remaining),
                    ("lastUpdate", order.last_update),
                ] {
                    value[key] = encoded(n);
                }
                value
            })
            .collect();
        assert_eq!(
            omit_undefined(&json!(actual)),
            omit_undefined(&case["result"]["value"]),
            "mapOrder value {}",
            case["name"]
        );
    }
    assert_eq!(
        (visited, source_success, accepted_undefined),
        (
            1327 + usize::from(field_identity),
            1263 + usize::from(field_identity),
            78
        ),
        "mapOrder source corpus and accepted own-undefined cases"
    );
    assert!(
        accepted > 500 && refused > 100,
        "mapOrder must exercise successful and refused records: {accepted}/{refused}"
    );
    for case in oracle["mapOrderUtf16"].as_array().unwrap() {
        assert!(
            serde_json::from_str::<Value>(case["inputJson"].as_str().unwrap()).is_err(),
            "mapOrder chosen UTF8 input refusal"
        );
        assert!(
            serde_json::from_str::<String>(case["symbolJson"].as_str().unwrap()).is_err(),
            "mapOrder TS retains lone surrogate symbol"
        );
    }
    for case in oracle["mapOrderReference"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["name"].as_str().unwrap().starts_with("status:"))
    {
        let serialized: Value =
            serde_json::from_str(case["result"]["serialized"].as_str().unwrap()).unwrap();
        if case["name"] == "status:__proto__" {
            assert_eq!(
                serialized[0]["status"],
                json!({}),
                "mapOrder prototype object remains present in source JSON"
            );
        } else {
            assert!(
                serialized[0].get("status").is_none(),
                "mapOrder callable status omitted only at JSON serialization {}",
                case["name"]
            );
            assert!(
                case["result"]["value"][0]["status"]
                    .get("$function")
                    .is_some(),
                "mapOrder callable status exists before serialization {}",
                case["name"]
            );
        }
    }
    let numeric_info = &oracle["mapOrderNumericInfo"];
    assert_eq!(
        numeric_info["output"]["info"]["nested"]["amount"],
        encoded(f64::NAN),
        "mapOrder source info holds NaN before any alias mutation"
    );
    let mut numeric_input = retained(&oracle["mapOrderReference"][0]["input"]);
    numeric_input.as_object_mut().unwrap().push((
        "nested".encode_utf16().collect(),
        Input::Object(vec![(
            "amount".encode_utf16().collect(),
            Input::Float(f64::NAN),
        )]),
    ));
    let numeric_order = map_order(Some(&numeric_input), "usdt-futures", || 1.0).unwrap();
    assert!(
        numeric_order[0]
            .info
            .as_ref()
            .unwrap()
            .get("nested")
            .unwrap()
            .get("amount")
            .unwrap()
            .as_f64()
            .unwrap()
            .is_nan(),
        "native mapOrder info retains NaN"
    );
    let alias = &oracle["mapOrderAlias"];
    assert_eq!(
        alias["sameReference"], true,
        "mapOrder source info is live gateway reference"
    );
    assert_eq!(
        alias["afterInput"]["label"], "after-input",
        "mapOrder gateway relabel propagates to info"
    );
    assert_eq!(
        alias["afterInput"]["price"],
        encoded(f64::NAN),
        "mapOrder live info preserves later NaN separately from aliasing"
    );
    assert_eq!(
        alias["afterOutput"]["label"], "after-output",
        "mapOrder info relabel propagates to gateway"
    );
    let mut input = retained(&oracle["mapOrderReference"][0]["input"]);
    input
        .as_object_mut()
        .unwrap()
        .push(("label".encode_utf16().collect(), json!("before").into()));
    let mut orders = map_order(Some(&input), "usdt-futures", || 1.0).unwrap();
    *input.get_mut("label").unwrap() = json!("after-input").into();
    assert!(
        orders[0]
            .info
            .as_ref()
            .unwrap()
            .get("label")
            .unwrap()
            .text_equals("before"),
        "CHOSEN mapOrder owned info does not observe gateway mutation"
    );
    *orders[0].info.as_mut().unwrap().get_mut("label").unwrap() = json!("after-output").into();
    assert!(
        input.get("label").unwrap().text_equals("after-input"),
        "CHOSEN mapOrder gateway does not observe owned info mutation"
    );
}
fn check_inverse_orders(oracle: &Value, field_identity: bool) {
    use safe_cex_venue_bitget::orders::map_inverse_order;
    let reference = &oracle["inverseOrder"];
    let mut accepted = 0;
    let mut refused = 0;
    let mut visited = 0;
    let mut source_success = 0;
    let mut accepted_undefined = 0;
    for case in reference["cases"].as_array().unwrap() {
        visited += 1;
        let name = case["name"].as_str().unwrap();
        let input = retained(&case["input"]);
        let mut calls = Vec::new();
        let mut clock_calls = 0;
        let actual = map_inverse_order(
            Some(&input),
            case["product"].as_str().unwrap(),
            |symbol| {
                calls.push(symbol.to_owned());
                if symbol == "BTCUSD" {
                    case["inverse"].as_bool()
                } else {
                    None
                }
            },
            || {
                clock_calls += 1;
                1700000000123.0
            },
        );
        assert_eq!(
            json!(calls),
            case["calls"],
            "inverse mapOrder market lookup {name}"
        );
        assert_eq!(
            json!(clock_calls),
            case["clockCalls"],
            "inverse mapOrder clock {name}"
        );
        if case["result"].get("error").is_some() {
            assert_eq!(
                error(actual.expect_err(&format!("inverse mapOrder source throw {name}"))),
                case["result"],
                "inverse mapOrder source throw {name}"
            );
            continue;
        }
        source_success += 1;
        let expected = case["result"]["value"].as_array().unwrap();
        let mut representable = true;
        for item in expected {
            let mut shape = decode(&omit_undefined(item));
            for field in ["price", "amount", "filled", "remaining", "lastUpdate"] {
                shape[field] = json!(0);
            }
            shape["extra"]["remainingNative"] = json!(0);
            representable &= serde_json::from_value::<safe_cex_types::Order>(shape).is_ok();
        }
        if !representable {
            refused += 1;
            assert_eq!(
                actual
                    .expect_err(&format!("inverse mapOrder typed refusal {name}"))
                    .name,
                "RepresentationError",
                "inverse mapOrder typed refusal {name}"
            );
            continue;
        }
        accepted += 1;
        let actual = actual.unwrap_or_else(|e| panic!("inverse mapOrder {name}: {e:?}"));
        assert_eq!(
            actual.len(),
            expected.len(),
            "native inverse mapOrder cardinality {name}"
        );
        let mut own_undefined = false;
        for (order, expected) in actual.iter().zip(expected) {
            assert_eq!(
                encode_json_numbers(order.info.as_ref().unwrap()),
                encode_json_numbers(&input),
                "raw info retains every native field before fixture comparison"
            );
            own_undefined |= native_undefined_fields(order, expected, &case["name"]);
        }
        accepted_undefined += usize::from(own_undefined);
        let mapped: Vec<_> = actual
            .into_iter()
            .map(|order| {
                let mut value = encode_json_numbers(&order);
                for (key, undefined) in [
                    ("status", order.status.is_undefined()),
                    ("type", order.r#type.is_undefined()),
                    ("side", order.side.is_undefined()),
                    ("timeInForce", order.time_in_force.is_undefined()),
                ] {
                    if undefined {
                        value.as_object_mut().unwrap().remove(key);
                    }
                }
                for (key, n) in [
                    ("price", order.price.to_number()),
                    ("amount", order.amount),
                    ("filled", order.filled),
                    ("remaining", order.remaining),
                    ("lastUpdate", order.last_update),
                ] {
                    value[key] = encoded(n);
                }
                value["extra"]["remainingNative"] =
                    encoded(order.extra.unwrap().remaining_native.unwrap());
                value
            })
            .collect();
        assert_eq!(
            omit_undefined(&json!(mapped)),
            omit_undefined(&case["result"]["value"]),
            "inverse mapOrder value {name}"
        );
    }
    assert_eq!(
        (visited, source_success, accepted_undefined),
        (
            1166 + usize::from(field_identity),
            1095 + usize::from(field_identity),
            100
        ),
        "inverse mapOrder source corpus and accepted own-undefined cases"
    );
    assert!(
        accepted > 500 && refused > 100,
        "inverse mapOrder success/refusal inventory {accepted}/{refused}"
    );
    for case in reference["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["name"].as_str().unwrap().starts_with("prototype:status:"))
    {
        let serialized: Value =
            serde_json::from_str(case["result"]["serialized"].as_str().unwrap()).unwrap();
        if case["name"] == "prototype:status:__proto__" {
            assert_eq!(
                serialized[0]["status"],
                json!({}),
                "inverse mapOrder prototype object remains serialized"
            );
            assert_eq!(
                case["result"]["value"][0]["status"]["$prototype"], "Object.prototype",
                "inverse mapOrder prototype identity"
            );
        } else {
            assert!(
                serialized[0].get("status").is_none(),
                "inverse mapOrder callable omitted from serialized record"
            );
            assert!(
                case["result"]["value"][0]["status"]
                    .get("$function")
                    .is_some(),
                "inverse mapOrder callable exists before serialization"
            );
        }
    }
    for case in reference["utf16"].as_array().unwrap() {
        assert!(
            serde_json::from_str::<Value>(case["inputJson"].as_str().unwrap()).is_err(),
            "inverse mapOrder chosen UTF8 ingress refusal"
        );
        assert!(
            serde_json::from_str::<String>(case["symbolJson"].as_str().unwrap()).is_err(),
            "inverse mapOrder source retains lone surrogate"
        );
    }
    assert_eq!(
        reference["numericInfo"]["output"]["info"]["nested"]["amount"],
        encoded(f64::NAN),
        "inverse mapOrder source info holds NaN independently of alias mutation"
    );
    let mut numeric_input = retained(&reference["numericInfo"]["input"]);
    numeric_input.as_object_mut().unwrap().push((
        "nested".encode_utf16().collect(),
        Input::Object(vec![(
            "amount".encode_utf16().collect(),
            Input::Float(f64::NAN),
        )]),
    ));
    let numeric_order =
        map_inverse_order(Some(&numeric_input), "coin-futures", |_| None, || 1.0).unwrap();
    assert!(
        numeric_order[0]
            .info
            .as_ref()
            .unwrap()
            .get("nested")
            .unwrap()
            .get("amount")
            .unwrap()
            .as_f64()
            .unwrap()
            .is_nan(),
        "native inverse mapOrder info retains NaN"
    );
    let extra_alias = &reference["extraAlias"];
    assert_eq!(
        extra_alias["sameReference"], true,
        "inverse mapOrder source extra is shared by primary and attached legs"
    );
    assert_eq!(
        extra_alias["afterPrimary"].as_array().unwrap().len(),
        3,
        "inverse mapOrder primary/SL/TP extra inventory"
    );
    for extra in extra_alias["afterPrimary"].as_array().unwrap() {
        assert_eq!(
            extra["remainingNative"],
            encoded(17.0),
            "inverse mapOrder primary extra mutation propagates to both legs"
        );
    }
    for extra in extra_alias["afterAttached"].as_array().unwrap() {
        assert_eq!(
            extra["remainingNative"],
            encoded(f64::NAN),
            "inverse mapOrder attached extra mutation propagates to primary and sibling"
        );
    }
    let alias = &reference["alias"];
    assert_eq!(
        alias["sameReference"], true,
        "inverse mapOrder attached legs share gateway info"
    );
    for order in alias["afterInput"].as_array().unwrap() {
        assert_eq!(
            order["info"]["label"], "after-input",
            "inverse mapOrder source input relabel propagates to each leg"
        );
        assert_eq!(
            order["info"]["nested"]["amount"],
            encoded(f64::NAN),
            "inverse mapOrder live info retains NaN"
        );
    }
    assert_eq!(
        alias["afterOutput"]["label"], "after-output",
        "inverse mapOrder attached info relabel propagates to gateway"
    );
    let mut input: Input = json!({"orderId":"x","status":"new","orderType":"limit","side":"buy","symbol":"BTCUSD","size":"3","price":"2","presetStopLossPrice":"2","presetStopSurplusPrice":"3","label":"before"}).into();
    let mut orders = map_inverse_order(Some(&input), "coin-futures", |_| None, || 1.0).unwrap();
    *input.get_mut("label").unwrap() = json!("after-input").into();
    assert!(
        orders[0]
            .info
            .as_ref()
            .unwrap()
            .get("label")
            .unwrap()
            .text_equals("before"),
        "CHOSEN inverse mapOrder info snapshot retains old input"
    );
    assert_eq!(
        orders.len(),
        3,
        "inverse mapOrder owned extra leg inventory"
    );
    orders[0].extra.as_mut().unwrap().remaining_native = Some(17.0);
    assert_eq!(
        orders[1].extra.as_ref().unwrap().remaining_native,
        Some(3.0),
        "CHOSEN inverse mapOrder primary extra mutation leaves attached snapshot unchanged"
    );
    orders[1].extra.as_mut().unwrap().remaining_native = Some(f64::NAN);
    assert_eq!(
        orders[0].extra.as_ref().unwrap().remaining_native,
        Some(17.0),
        "CHOSEN inverse mapOrder attached extra mutation leaves primary unchanged"
    );
    assert_eq!(
        orders[2].extra.as_ref().unwrap().remaining_native,
        Some(3.0),
        "CHOSEN inverse mapOrder attached extra mutation leaves sibling unchanged"
    );
    *orders[1].info.as_mut().unwrap().get_mut("label").unwrap() = json!("after-output").into();
    assert!(
        input.get("label").unwrap().text_equals("after-input"),
        "CHOSEN inverse mapOrder gateway does not share owned leg info"
    );
    assert!(
        orders[0]
            .info
            .as_ref()
            .unwrap()
            .get("label")
            .unwrap()
            .text_equals("before"),
        "CHOSEN inverse mapOrder legs do not share info"
    );
}
fn check(oracle: Value, field_identity: bool) {
    for cases in [&oracle["mapOrderCases"], &oracle["inverseOrder"]["cases"]] {
        assert_eq!(
            cases
                .as_array()
                .unwrap()
                .iter()
                .filter(|case| case["name"] == "field-identity:all")
                .count(),
            usize::from(field_identity),
            "exact field-identity supplement admission"
        );
    }
    check_mapped_orders(&oracle, field_identity);
    check_inverse_orders(&oracle, field_identity);
    use safe_cex_venue_bitget::stream_positions::*;
    for (i, c) in oracle["payloadTimeCases"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let mut position: Position=serde_json::from_value(json!({"id":"p","symbol":"BTCUSDT","side":"long","contracts":1,"entryPrice":1,"notional":1,"leverage":1,"unrealizedPnl":0,"liquidationPrice":0})).unwrap();
        position.info = c.get("info").map(|value| decode(value).into()).into();
        let result = get_position_payload_update_time(&position)
            .map(encoded)
            .unwrap_or_else(|| json!({"$undefined":true}));
        assert_eq!(result, c["result"], "getPositionPayloadUpdateTime case {i}");
    }
    for c in oracle["payloadTimeExcluded"].as_array().unwrap() {
        assert_eq!(
            c["source"],
            json!({"$undefined":true}),
            "source non-finite first timestamp stops alias walk"
        );
        assert_eq!(
            c["projected"],
            encoded(17.0),
            "CHOSEN JSON projection changes timestamp alias selection"
        );
    }
    for (i, c) in oracle["positionFoldCases"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let rest: Vec<Position> = serde_json::from_value(decode(&c["rest"])).unwrap();
        let current: Vec<Position> = serde_json::from_value(decode(&c["current"])).unwrap();
        for (name, rows) in [("rest", &rest), ("current", &current)] {
            for (at, row) in rows.iter().enumerate() {
                let source = &c[name][at];
                assert_eq!(
                    row.id, source["id"],
                    "fold native id identity {name}:{i}:{at}"
                );
                assert_eq!(
                    row.symbol, source["symbol"],
                    "fold native symbol identity {name}:{i}:{at}"
                );
                for (field, value) in [
                    ("contracts", row.contracts),
                    ("entryPrice", row.entry_price),
                    ("notional", row.notional),
                    ("leverage", row.leverage),
                    ("unrealizedPnl", row.unrealized_pnl),
                    ("liquidationPrice", row.liquidation_price),
                ] {
                    assert_eq!(
                        value.to_bits(),
                        number(&source[field]).to_bits(),
                        "fold native field identity {field} {name}:{i}:{at}"
                    );
                }
            }
        }
        let snapshot_ids = serde_json::from_value(c["snapshotIds"].clone()).unwrap();
        let updated = c["updated"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| (entry[0].as_str().unwrap().to_owned(), number(&entry[1])))
            .collect();
        let result = merge_rest_positions_with_fresh_ws_state(
            &rest,
            &current,
            number(&c["start"]),
            number(&c["snapshotAt"]),
            &snapshot_ids,
            &updated,
            5000.0,
        );
        let origins: Vec<Value> = result
            .into_iter()
            .map(|p| {
                if let Some(index) = rest.iter().position(|candidate| std::ptr::eq(candidate, p)) {
                    json!(["rest", index])
                } else {
                    json!([
                        "current",
                        current
                            .iter()
                            .position(|candidate| std::ptr::eq(candidate, p))
                            .expect("fold retains an input record")
                    ])
                }
            })
            .collect();
        assert_eq!(
            json!(origins),
            c["origins"],
            "mergeRestPositionsWithFreshWsState case {i}"
        );
    }
    for c in oracle["utf16PositionFold"].as_array().unwrap() {
        assert!(
            serde_json::from_str::<Value>(c["json"].as_str().unwrap()).is_err(),
            "shared UTF8 position fold ingress refusal"
        );
        assert_eq!(
            c["selectedCurrent"], true,
            "source surrogate position key preserved"
        );
    }

    for (i, c) in oracle["cancelPlanCases"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let mut raw = decode(&c["order"]);
        raw["amount"] = json!(0);
        let mut order: safe_cex_types::Order = serde_json::from_value(raw).unwrap();
        order.amount = number(&c["order"]["amount"]);
        let positions: Vec<Position> = serde_json::from_value(decode(&c["positions"])).unwrap();
        let mut errors = Vec::new();
        let result = match get_cancel_plan_type(
            &order,
            &positions,
            c.get("step").map(number),
            |m| errors.push(m),
        ) {
            Ok(safe_cex_num::TableHit::Mapped(value)) => json!({"value":value}),
            Ok(safe_cex_num::TableHit::Missing) => json!({"value":{"$undefined":true}}),
            Ok(safe_cex_num::TableHit::Inherited(member)) => {
                use safe_cex_num::ProtoMember::*;
                let key = match member {
                    Constructor => "constructor",
                    DefineGetter => "__defineGetter__",
                    DefineSetter => "__defineSetter__",
                    HasOwnProperty => "hasOwnProperty",
                    LookupGetter => "__lookupGetter__",
                    LookupSetter => "__lookupSetter__",
                    IsPrototypeOf => "isPrototypeOf",
                    PropertyIsEnumerable => "propertyIsEnumerable",
                    ToString => "toString",
                    ValueOf => "valueOf",
                    Prototype => "__proto__",
                    ToLocaleString => "toLocaleString",
                };
                if member == Prototype {
                    json!({"value":{"$prototype":"Object.prototype"}})
                } else {
                    json!({"value":{"$function":if member==Constructor {"Object"}else{key},"$nativeMember":key}})
                }
            }
            Err(e) => error(e),
        };
        assert_eq!(result, c["result"], "getCancelPlanType case {i}");
        assert_eq!(
            json!(errors),
            c["errors"],
            "getCancelPlanType warnings case {i}"
        );
    }
    for c in oracle["utf16CancelPlan"].as_array().unwrap() {
        assert!(
            serde_json::from_str::<Value>(c["json"].as_str().unwrap()).is_err(),
            "shared UTF8 cancellation input refusal"
        );
        assert!(
            serde_json::from_str::<Value>(c["errorsJson"].as_str().unwrap()).is_err(),
            "source cancellation warning preserves lone surrogate"
        );
    }

    use safe_cex_venue_bitget::stream_predicates::*;
    let stream_result = |result: Result<String, safe_cex_num::JsCoercionError>| match result {
        Ok(value) => json!({"value":value}),
        Err(error) => json!({"error":{"name":error.name,"message":error.message}}),
    };
    for (i, case) in oracle["streamNormalizeCases"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let input = case.get("input").map(decode);
        assert_eq!(
            stream_result(normalize_order_status(input.as_ref())),
            case["status"],
            "normalizeOrderStatus case {i}"
        );
        assert_eq!(
            stream_result(normalize_channel(input.as_ref())),
            case["channel"],
            "normalizeChannel case {i}"
        );
    }
    for (i, case) in oracle["streamRawCases"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let input = case.get("input").map(decode);
        assert_eq!(
            stream_result(get_raw_order_status(input.as_ref())),
            case["result"],
            "getRawOrderStatus case {i}"
        );
    }
    for (i, case) in oracle["streamTerminalCases"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        assert_eq!(
            is_terminal_fill_status(case["input"].as_str().unwrap()),
            case["value"].as_bool().unwrap(),
            "isTerminalFillStatus case {i}"
        );
    }
    for (i, case) in oracle["streamFiniteCases"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        assert_eq!(
            encoded(to_finite_number(
                finite_input(&case["input"]),
                number(&case["fallback"])
            )),
            case["value"],
            "private stream toFiniteNumber case {i}"
        );
    }
    for case in oracle["streamExcluded"].as_array().unwrap() {
        assert_eq!(case["json"], "null", "source non-finite JSON projection");
        assert_ne!(
            case["status"], case["projected"],
            "CHOSEN stream non-finite input exclusion"
        );
        assert_eq!(
            case["status"], case["raw"],
            "source raw non-finite status retained"
        );
    }
    for case in oracle["utf16Stream"].as_array().unwrap() {
        assert!(
            serde_json::from_str::<String>(case["json"].as_str().unwrap()).is_err(),
            "shared UTF-8 stream input refusal"
        );
        assert_eq!(
            case["terminal"], false,
            "source lone surrogate terminal predicate"
        );
        for field in ["statusJson", "channelJson", "rawJson"] {
            assert!(
                serde_json::from_str::<String>(case[field].as_str().unwrap()).is_err(),
                "source {field} preserves lone surrogate"
            );
        }
    }

    let frozen: Value = serde_json::from_str(include_str!("order-oracle.json")).unwrap();
    let references = oracle["mapOrderReference"].as_array().unwrap();
    assert_eq!(references.len(), 81, "mapOrder reference inventory");
    for (actual, expected) in references
        .iter()
        .zip(frozen["mapOrderReference"].as_array().unwrap())
    {
        assert_eq!(
            actual, expected,
            "mapOrder open-value witness {}",
            expected["name"]
        );
    }
    let reference = |name: &str| references.iter().find(|case| case["name"] == name).unwrap();
    assert_eq!(
        reference("orderType:constructor")["result"]["value"][0]["type"],
        json!({"$function":"Object","$nativeMember":"constructor"}),
        "mapOrder constructor keeps inherited function"
    );
    assert_eq!(
        reference("orderType:constructor")["result"]["value"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "mapOrder constructor suppresses strategy recovery"
    );
    assert_eq!(
        reference("unknown-type-strategy")["result"]["value"]
            .as_array()
            .unwrap()
            .len(),
        2,
        "mapOrder unknown type runs strategy recovery"
    );
    for (name, field) in [
        ("unknown-status", "status"),
        ("missing-type", "type"),
        ("missing-id", "id"),
    ] {
        assert_eq!(
            reference(name)["result"]["value"][0][field],
            json!({"$undefined":true}),
            "mapOrder {name} retains own undefined"
        );
    }
    let baseline = decode(&reference("baseline")["result"]["value"][0]);
    assert!(
        serde_json::from_value::<safe_cex_types::Order>(baseline.clone()).is_ok(),
        "mapOrder baseline shared Order acceptance"
    );
    let mut numeric_id = baseline.clone();
    numeric_id["id"] = json!(42);
    assert!(
        serde_json::from_value::<safe_cex_types::Order>(numeric_id).is_err(),
        "CHOSEN shared Order contract refuses native numeric id"
    );
    // JSON.stringify omits own undefined object fields. Keep that projection
    // separate from the source's own-present undefined tags above.
    for (name, field) in [
        ("unknown-status", "status"),
        ("missing-type", "type"),
        ("missing-id", "id"),
    ] {
        let mut projected = baseline.clone();
        projected.as_object_mut().unwrap().remove(field);
        let decoded = serde_json::from_value::<safe_cex_types::Order>(projected);
        if field == "id" {
            assert!(decoded.is_err(), "shared Order still requires id");
        } else {
            let decoded = decoded.expect("optional enum accepts an omitted JSON field");
            assert!(
                match field {
                    "status" => decoded.status.is_absent(),
                    "type" => decoded.r#type.is_absent(),
                    _ => unreachable!(),
                },
                "JSON omission decodes as absent: {name}"
            );
        }
    }

    for (i, c) in oracle["strategyOptsCases"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let mut input = decode(&c["input"]);
        input["price"] = json!(0);
        input["amount"] = json!(0);
        let mut order: safe_cex_types::Order = serde_json::from_value(input).unwrap();
        order.price = (number(&c["input"]["price"])).into();
        order.amount = number(&c["input"]["amount"]);
        let opts = build_uta_strategy_place_opts_from_order(&order);
        let mut actual = serde_json::to_value(&opts).unwrap();
        actual["price"] = encoded(opts.price.unwrap());
        actual["amount"] = encoded(opts.amount);
        assert_eq!(
            actual, c["output"],
            "buildUtaStrategyPlaceOptsFromOrder case {i}"
        );
        let mut projected_keys: Vec<_> = actual
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        projected_keys.sort();
        let mut expected_keys: Vec<_> = c["ownKeys"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|k| !c["undefinedKeys"].as_array().unwrap().contains(k))
            .map(|k| k.as_str().unwrap())
            .collect();
        expected_keys.sort();
        assert_eq!(
            projected_keys, expected_keys,
            "strategy option undefined-property projection case {i}"
        );
        for key in c["undefinedKeys"].as_array().unwrap() {
            assert!(
                actual.get(key.as_str().unwrap()).is_none(),
                "chosen omission of own undefined property"
            );
        }
    }
    for c in oracle["utf16StrategyOpts"].as_array().unwrap() {
        assert!(
            serde_json::from_str::<Value>(c["json"].as_str().unwrap()).is_err(),
            "strategy options shared UTF-8 input"
        );
        assert!(
            serde_json::from_str::<Value>(c["outputJson"].as_str().unwrap()).is_err(),
            "TS strategy options preserves surrogate"
        );
    }

    for (i, c) in oracle["stateGetterCases"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        assert_eq!(
            safe_cex_venue_bitget::profile::uses_uta_routing(
                c["isUta"].as_bool().unwrap(),
                c["supportsUtaRouting"].as_bool().unwrap(),
                c["testnet"].as_bool().unwrap_or(false)
            ),
            c["routing"].as_bool().unwrap(),
            "usesUtaRouting case {i}"
        );
        assert_eq!(
            safe_cex_venue_bitget::profile::suppress_bitget_validation_error_emission(
                c["suppressed"].as_bool().unwrap()
            ),
            c["suppression"].as_bool().unwrap(),
            "suppressBitgetValidationErrorEmission case {i}"
        );
    }
    for c in oracle["utf16UtaPosition"].as_array().unwrap() {
        assert!(
            serde_json::from_str::<Value>(c["json"].as_str().unwrap()).is_err(),
            "mapUtaPosition UTF-8 input"
        );
        assert!(
            serde_json::from_str::<Value>(c["outputJson"].as_str().unwrap()).is_err(),
            "mapUtaPosition UTF-8 output"
        );
    }

    for c in oracle["utf16UtaPosition"].as_array().unwrap() {
        assert!(
            serde_json::from_str::<Value>(c["json"].as_str().unwrap()).is_err(),
            "mapPosition UTF-8 input"
        );
        assert!(
            serde_json::from_str::<Value>(c["classicOutputJson"].as_str().unwrap()).is_err(),
            "mapPosition UTF-8 output"
        );
    }

    for (i, c) in oracle["utaPositionCases"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let input = c.get("input").map(decode);
        let mut stored = Vec::new();
        if let Some(price) = c.get("stored") {
            let mut p:Position=serde_json::from_value(json!({"id":"BTCUSDT","symbol":"BTCUSDT","side":"long","entryPrice":0,"notional":0,"leverage":1,"unrealizedPnl":0,"contracts":1,"liquidationPrice":0})).unwrap();
            p.liquidation_price = number(price);
            stored.push(p);
        }
        let mut calls = Vec::new();
        let mut clock_calls = 0;
        let result = safe_cex_venue_bitget::positions::map_uta_position(
            input.as_ref(),
            |raw| {
                normalize_bitget_symbol(raw, SymbolProfile::Usdt, false, |s| {
                    calls.push(s.to_owned());
                    Ok(if s == "BTC" {
                        Some("BTCUSDT".into())
                    } else {
                        None
                    })
                })
            },
            &stored,
            || {
                clock_calls += 1;
                1700000000123.0
            },
        );
        let result = match result {
            Ok(p) => json!({"value":encoded_position(p)}),
            Err(e) => error(e),
        };
        assert_eq!(result, c["result"], "mapUtaPosition case {i}");
        assert_eq!(
            json!(calls),
            c["calls"],
            "mapUtaPosition lookup timing case {i}"
        );
        assert_eq!(
            json!(clock_calls),
            c["clockCalls"],
            "mapUtaPosition clock timing case {i}"
        );
    }

    for (i, c) in oracle["classicPositionCases"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let input = c.get("input").map(decode);
        let mut stored = Vec::new();
        if let Some(price) = c.get("stored") {
            let mut p:Position=serde_json::from_value(json!({"id":"BTCUSDT","symbol":"BTCUSDT","side":"long","entryPrice":0,"notional":0,"leverage":1,"unrealizedPnl":0,"contracts":1,"liquidationPrice":0})).unwrap();
            p.liquidation_price = number(price);
            stored.push(p);
        }
        let mut calls = Vec::new();
        let mut clock_calls = 0;
        let result = safe_cex_venue_bitget::positions::map_position(
            input.as_ref(),
            |raw| {
                normalize_bitget_symbol(raw, SymbolProfile::Usdt, false, |s| {
                    calls.push(s.to_owned());
                    Ok(if s == "BTC" {
                        Some("BTCUSDT".into())
                    } else {
                        None
                    })
                })
            },
            &stored,
            || {
                clock_calls += 1;
                1700000000123.0
            },
        );
        let result = match result {
            Ok(p) => json!({"value":encoded_position(p)}),
            Err(e) => error(e),
        };
        assert_eq!(result, c["result"], "mapPosition case {i}");
        assert_eq!(
            json!(calls),
            c["calls"],
            "mapPosition lookup timing case {i}"
        );
        assert_eq!(
            json!(clock_calls),
            c["clockCalls"],
            "mapPosition clock timing case {i}"
        );
    }

    for (i, c) in oracle["symbolCases"].as_array().unwrap().iter().enumerate() {
        let profile = match c["profile"].as_str().unwrap() {
            "main" => SymbolProfile::Usdt,
            "usdc" => SymbolProfile::Usdc,
            "inverse" => SymbolProfile::Inverse,
            _ => unreachable!(),
        };
        let input = c.get("input").map(decode);
        let mut calls = Vec::new();
        let actual = normalize_bitget_symbol(
            input.as_ref(),
            profile,
            c["testnet"].as_bool().unwrap(),
            |s| {
                calls.push(s.to_owned());
                match c["resolution"].as_str().unwrap() {
                    "missing" | "null" => Ok(None),
                    "empty" => Ok(Some(String::new())),
                    "mapped" => Ok(Some("MAPPED".into())),
                    "throw" => Err(ExchangeError {
                        name: "TypeError".into(),
                        message: "resolver failure".into(),
                        metadata: Box::default(),
                    }),
                    _ => unreachable!(),
                }
            },
        );
        let actual = match actual {
            Ok(s) => json!({"value":s}),
            Err(e) => error(e),
        };
        assert_eq!(actual, c["result"], "normalizeBitgetSymbol case {i}");
        assert_eq!(
            json!(calls),
            c["calls"],
            "normalizeBitgetSymbol lookup timing case {i}"
        );
    }
    for c in oracle["utf16Symbol"].as_array().unwrap() {
        assert!(
            serde_json::from_str::<String>(c["json"].as_str().unwrap()).is_err(),
            "normalizeBitgetSymbol UTF-8 input"
        );
        assert!(
            serde_json::from_str::<String>(c["outputJson"].as_str().unwrap()).is_err(),
            "normalizeBitgetSymbol UTF-8 output"
        );
    }

    let mut inherited_witnesses = 0;
    for (i, c) in oracle["marketCases"].as_array().unwrap().iter().enumerate() {
        let markets: Vec<safe_cex_types::Market> =
            serde_json::from_value(c["markets"].clone()).unwrap();
        let symbol = c["symbol"].as_str().unwrap();
        let mut calls = Vec::new();
        let result = safe_cex_venue_bitget::market::resolve_market(
            symbol,
            c["marginCoin"].as_str().unwrap(),
            &markets,
            |s| {
                calls.push(s.to_owned());
                markets.iter().rev().find(|m| m.symbol == s)
            },
        );
        if c["result"].get("inheritedType").is_some() {
            inherited_witnesses += 1;
            let projected_fallback = if symbol == "constructor"
                && c["marginCoin"] == "USDT"
                && markets.iter().any(|m| m.id == "proto-fallback")
            {
                Some("proto-fallback")
            } else {
                None
            };
            assert_eq!(
                result.map(|m| m.id.as_str()),
                projected_fallback,
                "CORRECTED inherited non-Market lookup case {i}"
            );
            assert_eq!(
                c["resolveCalls"],
                json!([symbol]),
                "TS inherited lookup returns before fallback"
            );
        } else {
            assert_eq!(
                result.map(|m| json!({"id":m.id})).unwrap_or(Value::Null),
                c["result"],
                "_resolveMarket case {i}"
            );
            assert_eq!(
                json!(calls),
                c["resolveCalls"],
                "_resolveMarket lookup order case {i}"
            );
        }
        let mut calls = Vec::new();
        let coin = safe_cex_venue_bitget::market::get_margin_coin_for_symbol(symbol, |s| {
            calls.push(s.to_owned());
            markets.iter().rev().find(|m| m.symbol == s)
        });
        if c["coin"].get("$undefined").is_some() {
            assert_eq!(
                coin, symbol,
                "CORRECTED inherited .base yields undefined in TS case {i}"
            );
        } else {
            assert_eq!(json!(coin), c["coin"], "getMarginCoinForSymbol case {i}");
        }
        assert_eq!(
            json!(calls),
            c["coinCalls"],
            "getMarginCoinForSymbol lookup order case {i}"
        );
    }
    assert_eq!(
        inherited_witnesses, 284,
        "maintained inherited non-Market divergence witness inventory"
    );
    for c in oracle["utf16Market"].as_array().unwrap() {
        assert!(
            serde_json::from_str::<Value>(c["json"].as_str().unwrap()).is_err(),
            "market shared UTF-8 input"
        );
        assert!(c["resolved"].is_null());
        assert!(
            serde_json::from_str::<Value>(c["coinJson"].as_str().unwrap()).is_err(),
            "market shared UTF-8 output"
        );
    }

    for c in oracle["profileCases"].as_array().unwrap() {
        use safe_cex_venue_bitget::profile::*;
        let testnet = c["testnet"].as_bool().unwrap_or(false);
        let (product, margin) = match c["name"].as_str().unwrap() {
            "main" => (api_product_type(testnet), api_margin_coin(testnet)),
            "usdc" => (
                usdc_api_product_type(testnet),
                usdc_api_margin_coin(testnet),
            ),
            "inverse" => (inverse_api_product_type(testnet), inverse_api_margin_coin()),
            _ => unreachable!(),
        };
        assert_eq!(
            json!(product),
            c["product"],
            "profile apiProductType {} testnet{testnet}",
            c["name"]
        );
        assert_eq!(
            json!(margin),
            c["margin"],
            "profile apiMarginCoin {} testnet{testnet}",
            c["name"]
        );
        assert_eq!(
            json!(api_uta_category(product)),
            c["category"],
            "profile apiUtaCategory {} testnet{testnet}",
            c["name"]
        );
        assert_eq!(
            serde_json::from_value::<safe_cex_venue_bitget::types::BitgetUtaCategory>(
                c["category"].clone()
            )
            .is_err(),
            testnet,
            "ts-bug apiUtaCategory testnet declaration mismatch"
        );
    }
    for c in oracle["categoryCases"].as_array().unwrap() {
        assert_eq!(
            json!(safe_cex_venue_bitget::profile::api_uta_category(
                c["product"].as_str().unwrap()
            )),
            c["value"],
            "apiUtaCategory Unicode uppercase"
        );
    }
    for c in oracle["utf16Category"].as_array().unwrap() {
        assert!(
            serde_json::from_str::<Value>(c["json"].as_str().unwrap()).is_err(),
            "apiUtaCategory shared UTF-8 input"
        );
        assert!(
            serde_json::from_str::<Value>(c["outputJson"].as_str().unwrap()).is_err(),
            "apiUtaCategory shared UTF-8 output"
        );
    }

    for c in oracle["storePositionCases"].as_array().unwrap() {
        use safe_cex_exchange::native_store::DefaultStore;
        use safe_cex_store::model_patch::ModelPatch;
        let price = number(&c["input"]);
        assert_eq!(
            c["patched"], c["input"],
            "TS Store patch preserves nonfinite and signed zero"
        );
        assert_eq!(
            c["afterUnrelated"], c["input"],
            "TS Store unrelated patch preserves nonfinite and signed zero"
        );
        let position = |price| {
            let mut p:Position=serde_json::from_value(json!({"id":"p","symbol":"BTCUSDT","side":"long","contracts":2,"entryPrice":10,"notional":20,"leverage":1,"unrealizedPnl":0,"liquidationPrice":0})).unwrap();
            p.liquidation_price = price;
            p
        };
        let fresh = DefaultStore::new();
        fresh.add_or_update_position(position(price)).unwrap();
        assert_eq!(
            encoded(fresh.positions().unwrap()[0].liquidation_price),
            c["inserted"],
            "stored liquidation typed insertion"
        );
        fresh
            .update_position("p", json!({"contracts":3}).as_object().unwrap())
            .expect("unrelated position patch succeeds");
        assert_eq!(
            fresh.positions().unwrap()[0].contracts,
            3.0,
            "unrelated contracts update is applied"
        );
        assert_eq!(
            encoded(fresh.positions().unwrap()[0].liquidation_price),
            c["afterUnrelated"],
            "shared native Store preserves nonfinite target during unrelated patch"
        );
        let patched = DefaultStore::new();
        patched.add_or_update_position(position(17.0)).unwrap();
        let mut native = ModelPatch::default();
        native.set_number("liquidationPrice", price);
        patched
            .update_position("p", &native)
            .expect("native position numeric patch succeeds");
        assert!(
            c["unrelatedError"].is_null() && c["patchError"].is_null(),
            "TS Store accepts nonfinite position updates"
        );
        assert_eq!(
            encoded(patched.positions().unwrap()[0].liquidation_price),
            c["patched"],
            "shared native Store patch preserves nonfinite and signed zero"
        );
        if !price.is_finite() {
            assert!(
                patched
                    .update_position("p", json!({"liquidationPrice":price}).as_object().unwrap())
                    .is_err(),
                "lossy JSON null patch refuses explicitly instead of panicking"
            );
            assert_eq!(
                encoded(patched.positions().unwrap()[0].liquidation_price),
                c["patched"],
                "refused JSON patch does not erase native value"
            );
        }
    }

    for (i, c) in oracle["successCases"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let input = c.get("input").map(decode);
        let actual = match is_bitget_success_code(input.as_ref()) {
            Ok(v) => json!({"value":v}),
            Err(e) => json!({"error":{"name":e.name,"message":e.message}}),
        };
        assert_eq!(actual, c["result"], "isBitgetSuccessCode case {i}");
    }
    for (i, c) in oracle["liquidationCases"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let input = c.get("input").map(decode);
        let positions: Vec<Position> = c["positions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                let mut raw = decode(p);
                raw["liquidationPrice"] = json!(0);
                let mut position: Position = serde_json::from_value(raw).unwrap();
                position.liquidation_price = number(&p["liquidationPrice"]);
                position
            })
            .collect();
        let actual = match resolve_liquidation_price(
            c["target"].as_str().unwrap(),
            input.as_ref(),
            &positions,
        ) {
            Ok(v) => json!({"value":encoded(v)}),
            Err(e) => json!({"error":{"name":e.name,"message":e.message}}),
        };
        assert_eq!(actual, c["result"], "resolveLiquidationPrice case {i}");
    }
    for c in oracle["utf16Coercion"].as_array().unwrap() {
        assert!(
            serde_json::from_str::<Value>(c["json"].as_str().unwrap()).is_err(),
            "coercion shared UTF-8 boundary"
        );
        assert!(c["success"].is_boolean());
        assert!(c["liquidation"].get("$number").is_some());
    }

    for (i, c) in oracle["normalizeCases"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let input = c.get("input").map(decode);
        let result = match c["name"].as_str().unwrap() {
            "mode" => normalize_bitget_position_mode(input.as_ref()).map(|v| json!(v)),
            "margin" => normalize_bitget_margin_type(input.as_ref()).map(|v| json!(v)),
            "side" => normalize_bitget_position_side(input.as_ref()).map(|v| json!(v)),
            _ => unreachable!(),
        };
        let result = match result {
            Ok(v) => json!({"value":v}),
            Err(e) => json!({"error":{"name":e.name,"message":e.message}}),
        };
        assert_eq!(result, c["result"], "normalization {} case {i}", c["name"]);
    }
    for c in oracle["utf16Normalize"].as_array().unwrap() {
        assert!(
            serde_json::from_str::<Value>(c["json"].as_str().unwrap()).is_err(),
            "normalization shared UTF-8 boundary"
        );
        assert_eq!(c["mode"], json!("oneway"));
        assert_eq!(c["margin"], json!("isolated"));
        assert_eq!(c["side"], json!("long"));
    }

    for (i, c) in oracle["finiteCases"].as_array().unwrap().iter().enumerate() {
        assert_eq!(
            encoded(to_finite_number(
                finite_input(&c["input"]),
                number(&c["fallback"])
            )),
            c["value"],
            "toFiniteNumber case {i}"
        );
    }
    for (i, c) in oracle["firstFiniteCases"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let values: Vec<_> = c["values"]
            .as_array()
            .unwrap()
            .iter()
            .map(finite_input)
            .collect();
        assert_eq!(
            encoded(first_finite_number(&values, number(&c["fallback"]))),
            c["value"],
            "firstFiniteNumber case {i}"
        );
    }
    for c in oracle["utf16Finite"].as_array().unwrap() {
        assert!(
            serde_json::from_str::<Value>(c["json"].as_str().unwrap()).is_err(),
            "numeric helper shared UTF-8 boundary"
        );
        assert!(c["value"].get("$number").is_some());
        assert!(c["first"].get("$number").is_some());
    }

    for c in oracle["pnlCases"].as_array().unwrap() {
        let positions: Vec<Position> = c["positions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                let mut raw = decode(v);
                for key in ["contracts", "entryPrice", "unrealizedPnl"] {
                    raw[key] = json!(0);
                }
                let mut p: Position = serde_json::from_value(raw).unwrap();
                p.contracts = number(&v["contracts"]);
                p.entry_price = number(&v["entryPrice"]);
                p.unrealized_pnl = number(&v["unrealizedPnl"]);
                p
            })
            .collect();
        let last = c.get("last").map(number);
        let mut calls = Vec::new();
        let upnl = calculate_upnl_from_last_price(&positions, |symbol| {
            calls.push(symbol.to_owned());
            last
        });
        assert_eq!(encoded(upnl), c["upnl"], "calculateUpnlFromLastPrice {c}");
        assert_eq!(
            json!(calls),
            c["upnlCalls"],
            "calculateUpnlFromLastPrice lookup order {c}"
        );
        calls.clear();
        let account = calculate_account_upnl(number(&c["apiUpnl"]), &positions, |symbol| {
            calls.push(symbol.to_owned());
            last
        });
        assert_eq!(encoded(account), c["account"], "calculateAccountUpnl {c}");
        assert_eq!(
            json!(calls),
            c["accountCalls"],
            "calculateAccountUpnl lookup order {c}"
        );
    }
    for c in oracle["utf16Pnl"].as_array().unwrap() {
        assert!(
            serde_json::from_str::<Vec<Position>>(c["json"].as_str().unwrap()).is_err(),
            "PnL UTF-8 boundary"
        );
        assert_eq!(c["upnl"], json!({"$number":"8"}), "TS surrogate PnL");
        assert_eq!(c["account"], c["upnl"], "TS surrogate account PnL");
        assert!(
            c["callsJson"].as_str().unwrap().contains("\\ud"),
            "TS surrogate lookup"
        );
    }

    for c in oracle["uuidCases"].as_array().unwrap() {
        let draws = c["draws"].as_array().unwrap();
        let mut consumed = 0;
        let value = generate_uuid(|| {
            let value = draws[consumed].as_f64().unwrap();
            consumed += 1;
            value
        });
        assert_eq!(value, c["value"], "BitgetInverseExchange.generateUuid {c}");
        assert_eq!(
            consumed as u64,
            c["consumed"].as_u64().unwrap(),
            "generateUuid draw count"
        );
    }

    for c in oracle["utf16Sizing"].as_array().unwrap() {
        assert!(
            serde_json::from_str::<PlaceOrderOpts>(c["json"].as_str().unwrap()).is_err(),
            "UTF-8 symbol boundary changed"
        );
        assert_eq!(
            c["result"],
            json!({"value":["pos_loss", {"$number":"0"}]}),
            "TS surrogate symbol sizing"
        );
        assert!(
            c["errorsJson"].as_str().unwrap().contains("\\ud"),
            "TS warning preserves lone surrogate"
        );
    }
    for c in oracle["sideCases"].as_array().unwrap() {
        let side = serde_json::from_value(c["side"].clone()).unwrap();
        let mode = serde_json::from_value(c["mode"].clone()).unwrap();
        let order_type = serde_json::from_value(c["type"].clone()).unwrap();
        let reduce_only = c.get("reduceOnly").and_then(Value::as_bool);
        assert_eq!(
            serde_json::to_value(get_order_side_v2(side, reduce_only, mode)).unwrap(),
            c["v2"],
            "getOrderSideV2 {c}"
        );
        assert_eq!(
            serde_json::to_value(get_order_side_uta(side, reduce_only, mode)).unwrap(),
            c["uta"],
            "getOrderSideUta {c}"
        );
        assert_eq!(
            serde_json::to_value(get_uta_strategy_order_side(side, mode)).unwrap(),
            c["strategy"],
            "getUtaStrategyOrderSide {c}"
        );
        let algo = match get_algo_order_side(order_type, side, mode) {
            Ok(v) => json!({"value":v}),
            Err(e) => error(e),
        };
        assert_eq!(algo, c["algo"], "getAlgoOrderSide {c}");
        let inverse = match get_inverse_algo_order_side(order_type, side, mode) {
            Ok(v) => json!({"value":v}),
            Err(e) => error(e),
        };
        assert_eq!(inverse, c["inverseAlgo"], "getInverseAlgoOrderSide {c}");
        assert_eq!(is_algo_order(order_type), c["isAlgo"], "isAlgoOrder {c}");
    }
    for c in oracle["triggers"].as_array().unwrap() {
        let trigger = c
            .get("trigger")
            .map(|v| serde_json::from_value(v.clone()).unwrap());
        assert_eq!(
            get_uta_strategy_trigger_by(trigger),
            c["value"],
            "getUtaStrategyTriggerBy {c}"
        );
    }
    for c in oracle["ids"].as_array().unwrap() {
        assert_eq!(
            is_virtual_tpsl_order(c["id"].as_str().unwrap()),
            c["value"],
            "isVirtualTPSLOrder {c}"
        );
    }
    for c in oracle["roundCases"].as_array().unwrap() {
        let amount = number(&c["amount"]);
        let step = c.get("step").map(number);
        assert_eq!(
            encoded(plan_type_comparable_amount(amount, step)),
            c["comparable"],
            "planTypeComparableAmount {c}"
        );
        assert_eq!(
            encoded(floor_to_lot(amount, step)),
            c["floor"],
            "floorToLot {c}"
        );
    }
    for c in oracle["sizingCases"].as_array().unwrap() {
        let mut opts_value = decode(&c["opts"]);
        opts_value["amount"] = json!(0);
        let mut opts: PlaceOrderOpts = serde_json::from_value(opts_value).unwrap();
        opts.amount = number(&c["opts"]["amount"]);
        let positions: Vec<Position> = c["positions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                let mut raw = decode(v);
                raw["contracts"] = json!(0);
                let mut p: Position = serde_json::from_value(raw).unwrap();
                p.contracts = number(&v["contracts"]);
                p
            })
            .collect();
        let step = c.get("step").map(number);
        let mut errors = vec![];
        let result = match select_plan_type_and_amount(&opts, &positions, step, |m| errors.push(m))
        {
            Ok((plan, amount)) => json!({"value":[plan,encoded(amount)]}),
            Err(e) => error(e),
        };
        assert_eq!(result, c["result"], "selectPlanTypeAndAmount {c}");
        assert_eq!(
            json!(errors),
            c["errors"],
            "selectPlanTypeAndAmount errors {c}"
        );
    }
    for c in oracle["utf16Ids"].as_array().unwrap() {
        assert_eq!(c["value"], true, "TS accepts surrogate virtual ID");
        assert!(
            serde_json::from_str::<String>(c["json"].as_str().unwrap()).is_err(),
            "UTF-8 boundary changed"
        );
    }
}
#[test]
fn frozen_order_decisions() {
    check(
        serde_json::from_str(include_str!("order-oracle.json")).unwrap(),
        false,
    );
}
#[test]
#[ignore = "requires Node and TypeScript"]
fn live_order_decisions() {
    let r = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/order-oracle.cjs"
        ))
        .output()
        .expect("execute Node");
    assert!(
        r.status.success(),
        "order oracle failed: {}",
        String::from_utf8_lossy(&r.stderr)
    );
    let oracle: Value = serde_json::from_slice(&r.stdout).unwrap();
    check(oracle.clone(), false);
    assert_eq!(
        oracle,
        serde_json::from_str::<Value>(include_str!("order-oracle.json")).unwrap(),
        "maintained order oracle drift"
    );
}

#[test]
#[ignore = "requires Node and TypeScript"]
fn live_api_uta_category_all_unicode_scalars() {
    let input: String = (0..=0x10ffff).filter_map(char::from_u32).collect();
    let expected = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/order-oracle.cjs"
        ))
        .arg("--unicode-profile")
        .output()
        .unwrap();
    assert!(
        expected.status.success(),
        "{}",
        String::from_utf8_lossy(&expected.stderr)
    );
    let actual = safe_cex_venue_bitget::profile::api_uta_category(&input);
    if actual.as_bytes() != expected.stdout {
        let expected = String::from_utf8(expected.stdout).unwrap();
        let mismatch = actual
            .chars()
            .zip(expected.chars())
            .position(|(a, b)| a != b);
        panic!(
            "apiUtaCategory scalar uppercase differs at output index {mismatch:?}; Rust Unicode {:?}",
            char::UNICODE_VERSION
        );
    }
}

#[test]
#[ignore = "requires Node and TypeScript"]
fn order_oracle_rejects_source_trigger_swap() {
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "live_order_decisions",
            "--exact",
            "--ignored",
            "--nocapture",
        ])
        .env("BITGET_ORDER_MUTATION", "strategy-trigger-swap")
        .output()
        .unwrap();
    assert_eq!(
        result.status.code(),
        Some(101),
        "source trigger correction must fail Rust assertion"
    );
    let text = String::from_utf8_lossy(&result.stdout).to_string()
        + &String::from_utf8_lossy(&result.stderr);
    assert!(
        text.contains("buildUtaStrategyPlaceOptsFromOrder case"),
        "named strategy option assertion: {text}"
    );
}

#[test]
#[ignore = "requires Node and TypeScript"]
fn order_oracle_rejects_source_prototype_fix() {
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "live_order_decisions",
            "--exact",
            "--ignored",
            "--nocapture",
        ])
        .env("BITGET_ORDER_MUTATION", "own-type-only")
        .output()
        .unwrap();
    assert_eq!(
        result.status.code(),
        Some(101),
        "source prototype correction must fail Rust assertion"
    );
    let text = String::from_utf8_lossy(&result.stdout).to_string()
        + &String::from_utf8_lossy(&result.stderr);
    assert!(
        text.contains("mapOrder") && text.contains("orderType:constructor"),
        "named prototype correction assertion: {text}"
    );
}

#[test]
#[ignore = "requires Node and TypeScript"]
fn stream_oracle_rejects_ambiguous_fill() {
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "live_order_decisions",
            "--exact",
            "--ignored",
            "--nocapture",
        ])
        .env("BITGET_STREAM_MUTATION", "ambiguous-fill")
        .output()
        .unwrap();
    assert_eq!(
        result.status.code(),
        Some(101),
        "stream source correction must fail assertion"
    );
    let text = String::from_utf8_lossy(&result.stdout).to_string()
        + &String::from_utf8_lossy(&result.stderr);
    assert!(
        text.contains("isTerminalFillStatus case"),
        "named stream assertion: {text}"
    );
}

#[test]
#[ignore = "requires Node and TypeScript"]
fn cancel_oracle_rejects_own_only_lookup() {
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "live_order_decisions",
            "--exact",
            "--ignored",
            "--nocapture",
        ])
        .env("BITGET_CANCEL_MUTATION", "own-only")
        .output()
        .unwrap();
    assert_eq!(
        result.status.code(),
        Some(101),
        "cancellation source correction must fail assertion"
    );
    let text = String::from_utf8_lossy(&result.stdout).to_string()
        + &String::from_utf8_lossy(&result.stderr);
    assert!(
        text.contains("getCancelPlanType case"),
        "named cancellation assertion: {text}"
    );
}

#[test]
#[ignore = "requires Node and TypeScript"]
fn position_fold_oracle_rejects_rest_deduplication() {
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "live_order_decisions",
            "--exact",
            "--ignored",
            "--nocapture",
        ])
        .env("BITGET_FOLD_MUTATION", "dedupe-rest")
        .output()
        .unwrap();
    assert_eq!(
        result.status.code(),
        Some(101),
        "position fold source correction must fail assertion"
    );
    let text = String::from_utf8_lossy(&result.stdout).to_string()
        + &String::from_utf8_lossy(&result.stderr);
    assert!(
        text.contains("mergeRestPositionsWithFreshWsState case"),
        "named position fold assertion: {text}"
    );
}

#[test]
#[ignore = "requires Node and TypeScript"]
fn inverse_order_oracle_rejects_boolean_coercion() {
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "live_order_decisions",
            "--exact",
            "--ignored",
            "--nocapture",
        ])
        .env("BITGET_INVERSE_MUTATION", "boolean-reduce")
        .output()
        .unwrap();
    assert_eq!(
        result.status.code(),
        Some(101),
        "inverse source correction must fail assertion"
    );
    let text = String::from_utf8_lossy(&result.stdout).to_string()
        + &String::from_utf8_lossy(&result.stderr);
    assert!(
        text.contains("inverse mapOrder value mode:"),
        "named inverse reduceOnly assertion: {text}"
    );
}

#[test]
#[ignore = "requires live Node and TypeScript"]
fn order_field_identity_supplement() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/order-oracle.cjs"
        ))
        .arg("--field-identity")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    for cases in [&oracle["mapOrderCases"], &oracle["inverseOrder"]["cases"]] {
        assert!(
            cases
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["name"] == "field-identity:all"),
            "order identity supplement inventory"
        );
    }
    check(oracle, true);
}
