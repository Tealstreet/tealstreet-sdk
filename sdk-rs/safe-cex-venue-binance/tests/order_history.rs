use safe_cex_num::js_number_string;
use safe_cex_types::HistoricalOrder;
use safe_cex_venue_binance::order_history::{
    BinanceHistoricalOrderError, map_binance_historical_order_typed,
};
use serde_json::{Value, json};
fn number(n: f64) -> Value {
    json!({"number":if n == 0.0 && n.is_sign_negative() { "-0".into() } else { js_number_string(n) }})
}
fn optional(n: Option<f64>) -> Value {
    n.map(number).unwrap_or(json!({"undefined":true}))
}
fn normalize(v: HistoricalOrder) -> Value {
    json!({"id":v.id,"symbol":v.symbol,"side":v.side,"type":v.r#type,"status":v.status,
    "price":optional(v.price),"averagePrice":optional(v.average_price),"amount":number(v.amount),
    "filled":number(v.filled),"reduceOnly":v.reduce_only,"positionSide":v.position_side.map(|s|json!(s)).unwrap_or(json!({"undefined":true})),
    "createdTime":number(v.created_time),"updatedTime":optional(v.updated_time)})
}
#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn historical_typed_projection_pins_explicit_closed_output_choice() {
    typed_projection_native_policy();
    let output = std::process::Command::new("node")
        .arg(format!(
            "{}/scripts/order-history.mjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 4754, "SOURCE-CARDINALITY order_history total");
    let mut differences = 0;
    let mut completed_comparisons = 0;
    for (index, case) in cases.iter().enumerate() {
        let row = serde_json::from_str(case["row"].as_str().unwrap()).unwrap();
        let actual = match map_binance_historical_order_typed(&row) {
            Ok(result) => json!({"value":result.map(normalize)}),
            Err(BinanceHistoricalOrderError::Native(error)) => json!({"error":error.name}),
            Err(BinanceHistoricalOrderError::ClosedOutput { field, member }) => {
                let expected = &case["expected"]["refusal"];
                assert!(
                    expected.is_object(),
                    "unexpected closed-output refusal before source error/return case {index}: {case}"
                );
                let empty: [(&str, ()); 0] = [];
                assert_eq!(
                    safe_cex_num::TableHit::Inherited(member),
                    safe_cex_num::js_table_lookup(
                        &empty,
                        expected["key"]
                            .as_str()
                            .expect("source-derived refusal key")
                    ),
                    "refused member identity case {index}"
                );
                json!({"refusal":{"field":field,"key":expected["key"],"kind":if member == safe_cex_num::ProtoMember::Prototype {"object"}else{"function"}}})
            }
            Err(BinanceHistoricalOrderError::Projection(error)) => {
                panic!("unexpected categorical projection failure case {index}: {error}; {case}")
            }
        };
        assert_eq!(
            actual, case["expected"],
            "historical order case {index}: {case}"
        );
        if case["original"] != case["expected"] {
            differences += 1;
        }
        completed_comparisons += 1;
    }
    assert_eq!(
        completed_comparisons, 4754,
        "NATIVE-CARDINALITY order_history completed comparisons"
    );
    assert!(
        differences > 0,
        "source prototype bug must remain witnessed"
    );
    eprintln!(
        "{} historical order cases, {differences} explicit typed output refusals",
        cases.len()
    );
}

fn typed_projection_native_policy() {
    use safe_cex_num::{ProtoMember, PrototypeRealm, PrototypeScalar};
    use safe_cex_venue_binance::private_event_value::EventValue;
    use std::rc::Rc;
    let realm = Rc::<PrototypeRealm>::default();
    let _binding = realm.bind();
    let optional_row = json!({"orderId":"typed-optionals","symbol":"BTCUSDT","time":1,"side":"BUY","origType":"MARKET","status":"NEW"});
    let native = safe_cex_venue_binance::order_history::map_binance_historical_order(
        &EventValue::from_json(optional_row.clone()),
        &realm,
    )
    .unwrap()
    .unwrap();
    assert!(
        matches!(&native, EventValue::Object(record) if record.borrow().fields.len() == 13),
        "native canonical history retains thirteen independently authored own fields"
    );
    for field in ["price", "averagePrice", "positionSide", "updatedTime"] {
        assert!(
            matches!(native.get(field).unwrap(), EventValue::Undefined),
            "native canonical history keeps own undefined optional field {field}"
        );
    }
    let typed = map_binance_historical_order_typed(optional_row.as_object().unwrap())
        .expect("native typed projection accepts undefined optional values")
        .unwrap();
    assert!(
        typed.price.is_none()
            && typed.average_price.is_none()
            && typed.position_side.is_none()
            && typed.updated_time.is_none(),
        "native typed projection preserves absent optional values"
    );
    let populated = json!({"orderId":"typed-present","symbol":"BTCUSDT","time":1,"side":"BUY","origType":"MARKET","status":"NEW","price":"4.2","avgPrice":"3.6","positionSide":"LONG","updateTime":2,"origQty":"-0","executedQty":"0","reduceOnly":false});
    let typed = map_binance_historical_order_typed(populated.as_object().unwrap())
        .expect("native typed projection accepts populated optional values")
        .unwrap();
    assert_eq!(typed.price, Some(4.2), "native typed price stays present");
    assert_eq!(
        typed.average_price,
        Some(3.6),
        "native typed average stays present"
    );
    assert_eq!(
        typed.updated_time,
        Some(2.0),
        "native typed time stays present"
    );
    assert_eq!(
        typed.position_side,
        Some(safe_cex_types::ExecutionPositionSide::Side(
            safe_cex_types::PositionSide::Long
        )),
        "native typed position side stays present"
    );
    assert!(
        typed.amount == 0.0 && typed.amount.is_sign_negative(),
        "native typed amount preserves signed zero"
    );
    assert_eq!(typed.filled, 0.0, "native typed filled preserves zero");
    assert_eq!(
        typed.reduce_only,
        Some(false),
        "native typed reduce-only preserves false"
    );
    realm
        .reference(ProtoMember::Prototype)
        .set("nullablePosition", PrototypeScalar::Null);
    let mut null_row = optional_row.clone();
    null_row["positionSide"] = json!("nullablePosition");
    let native_null = safe_cex_venue_binance::order_history::map_binance_historical_order(
        &EventValue::from_json(null_row.clone()),
        &realm,
    )
    .unwrap()
    .unwrap();
    assert!(
        matches!(native_null.get("positionSide").unwrap(), EventValue::Null),
        "native canonical history retains present null"
    );
    assert!(
        matches!(
            map_binance_historical_order_typed(null_row.as_object().unwrap()),
            Err(BinanceHistoricalOrderError::Projection(_))
        ),
        "native typed projection refuses present null instead of omitting it"
    );
    for (input, output) in [
        ("origType", "type"),
        ("status", "status"),
        ("positionSide", "positionSide"),
    ] {
        for (key, member) in [
            ("constructor", ProtoMember::Constructor),
            ("__proto__", ProtoMember::Prototype),
        ] {
            let mut row = json!({"orderId":"typed-witness","symbol":"BTCUSDT","time":1,"side":"BUY","origType":"MARKET","status":"NEW","positionSide":"LONG"});
            row[input] = json!(key);
            let result = safe_cex_venue_binance::order_history::map_binance_historical_order(
                &EventValue::from_json(row.clone()),
                &realm,
            )
            .unwrap()
            .unwrap();
            assert!(
                matches!(result.get(output).unwrap(),EventValue::Builtin(ref value) if value.member()==safe_cex_num::BuiltinIdentity::Object(member)),
                "native canonical history keeps inherited object/callable before typed choice"
            );
            assert!(
                matches!(map_binance_historical_order_typed(row.as_object().unwrap()),Err(BinanceHistoricalOrderError::ClosedOutput{field,member:actual}) if field==output && actual==member),
                "native explicit typed adapter identifies its inherited output refusal"
            );
        }
    }
    realm
        .reference(ProtoMember::Prototype)
        .set("lastMessageTimestamp", PrototypeScalar::Number(1000.0));
    let row = json!({"orderId":"typed-scalar","symbol":"BTCUSDT","time":1,"side":"BUY","origType":"lastMessageTimestamp","type":"MARKET","status":"NEW","positionSide":"LONG"});
    let result = safe_cex_venue_binance::order_history::map_binance_historical_order(
        &EventValue::from_json(row.clone()),
        &realm,
    )
    .unwrap()
    .unwrap();
    assert!(
        matches!(result.get("type").unwrap(), EventValue::Number(1000.0)),
        "native canonical history keeps inherited numeric 1000 rather than MARKET"
    );
    assert!(
        matches!(
            map_binance_historical_order_typed(row.as_object().unwrap()),
            Err(BinanceHistoricalOrderError::Projection(_))
        ),
        "native typed adapter refuses inherited numeric 1000 rather than falling back to MARKET"
    );
}
