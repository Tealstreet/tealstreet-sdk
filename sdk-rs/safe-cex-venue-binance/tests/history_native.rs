use safe_cex_num::{BuiltinIdentity, ProtoMember, PrototypeRealm};
use safe_cex_types::{ExecutionPositionSide, OrderSide, OrderStatus, OrderType, PositionSide};
use safe_cex_venue_binance::{
    order_history_native::map_binance_historical_order_native,
    private_event_value::{EventValue, PrivateEnum},
};
use serde_json::{Value, json};
use std::rc::Rc;
const MEMBERS: [(&str, ProtoMember); 12] = [
    ("constructor", ProtoMember::Constructor),
    ("__defineGetter__", ProtoMember::DefineGetter),
    ("__defineSetter__", ProtoMember::DefineSetter),
    ("hasOwnProperty", ProtoMember::HasOwnProperty),
    ("__lookupGetter__", ProtoMember::LookupGetter),
    ("__lookupSetter__", ProtoMember::LookupSetter),
    ("isPrototypeOf", ProtoMember::IsPrototypeOf),
    ("propertyIsEnumerable", ProtoMember::PropertyIsEnumerable),
    ("toString", ProtoMember::ToString),
    ("valueOf", ProtoMember::ValueOf),
    ("__proto__", ProtoMember::Prototype),
    ("toLocaleString", ProtoMember::ToLocaleString),
];
fn base() -> Value {
    json!({"orderId":"12","symbol":"BTCUSDT","time":1,"side":"BUY","origType":"STOP_MARKET","type":"MARKET","status":"FILLED","price":"1","avgPrice":"2","origQty":"3","executedQty":"2","reduceOnly":false,"positionSide":"LONG","updateTime":2})
}
fn normalized(value: &EventValue) -> Value {
    match value {
        EventValue::Undefined => json!({"undefined":true}),
        EventValue::Null => Value::Null,
        EventValue::Bool(v) => json!(v),
        EventValue::String(v) => json!(v),
        EventValue::Number(n) => {
            json!({"number":if *n==0.0&&n.is_sign_negative(){"-0".into()}else{safe_cex_num::js_number_string(*n)}})
        }
        EventValue::Enum(v) => v.wire(),
        EventValue::Object(record) => Value::Object(
            record
                .borrow()
                .fields
                .iter()
                .map(|(key, value)| (key.clone(), normalized(value)))
                .collect(),
        ),
        EventValue::Builtin(reference) if reference.is_callable() => {
            let EventValue::String(name) = value.get("name").unwrap() else {
                panic!("native callable name")
            };
            json!({"function":name})
        }
        EventValue::Builtin(reference) => Value::Object(
            reference
                .own_fields()
                .into_iter()
                .map(|(key, value)| (key, normalized(&EventValue::from_prototype(value.into()))))
                .collect(),
        ),
        other => panic!("native history normalization requires explicit value kind: {other:?}"),
    }
}
fn standard_fields(result: &EventValue, changed: &str) {
    let EventValue::Object(record) = result else {
        panic!("native historical order is a record")
    };
    assert_eq!(
        record.borrow().fields.len(),
        13,
        "independent source return-object property count"
    );
    assert!(
        matches!(result.get("id").unwrap(),EventValue::String(ref v) if v=="12"),
        "native historical id before wire"
    );
    assert!(
        matches!(result.get("symbol").unwrap(),EventValue::String(ref v) if v=="BTCUSDT"),
        "native historical symbol before wire"
    );
    assert!(
        matches!(
            result.get("side").unwrap(),
            EventValue::Enum(PrivateEnum::Side(OrderSide::Buy))
        ),
        "native historical side enum before wire"
    );
    for (field, expected) in [
        ("price", 1.0),
        ("averagePrice", 2.0),
        ("amount", 3.0),
        ("filled", 2.0),
        ("createdTime", 1.0),
        ("updatedTime", 2.0),
    ] {
        if field == changed {
            continue;
        }
        assert!(
            matches!(result.get(field).unwrap(),EventValue::Number(n) if n==expected),
            "native historical {field} before wire"
        );
    }
    assert!(
        matches!(result.get("reduceOnly").unwrap(), EventValue::Bool(false)),
        "native historical reduce-only before wire"
    );
    if changed != "type" {
        assert!(
            matches!(
                result.get("type").unwrap(),
                EventValue::Enum(PrivateEnum::Type(OrderType::StopLoss))
            ),
            "native historical original type enum before wire"
        );
    }
    if changed != "status" {
        assert!(
            matches!(
                result.get("status").unwrap(),
                EventValue::Enum(PrivateEnum::Status(OrderStatus::Closed))
            ),
            "native historical status enum before wire"
        );
    }
    if changed != "positionSide" {
        assert!(
            matches!(result.get("positionSide").unwrap(),EventValue::Enum(PrivateEnum::Execution(value)) if *value==ExecutionPositionSide::Side(PositionSide::Long)),
            "native historical position-side enum before wire"
        );
    }
}
#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn historical_native_referents_and_entire_record_precede_original_source_comparison() {
    let mut observations = vec![];
    for (input_field, output_field) in [
        ("origType", "type"),
        ("status", "status"),
        ("positionSide", "positionSide"),
    ] {
        for (name, member) in MEMBERS {
            let realm = Rc::<PrototypeRealm>::default();
            let _binding = realm.bind();
            let mut row = base();
            row[input_field] = json!(name);
            let result =
                map_binance_historical_order_native(&EventValue::from_json(row.clone()), &realm)
                    .unwrap()
                    .expect("native inherited output retains complete historical record");
            standard_fields(&result, output_field);
            let EventValue::Builtin(reference) = result.get(output_field).unwrap() else {
                panic!("native historical inherited output is a real builtin reference")
            };
            assert_eq!(
                reference.member(),
                BuiltinIdentity::Object(member),
                "native exact inherited historical referent identity"
            );
            assert!(
                Rc::ptr_eq(reference.realm(), &realm),
                "native historical referent retains originating realm"
            );
            assert_eq!(
                reference.is_callable(),
                member != ProtoMember::Prototype,
                "native inherited historical object/callable kind"
            );
            if member != ProtoMember::Prototype {
                let expected = if name == "constructor" {
                    "Object"
                } else {
                    name
                };
                assert!(
                    matches!(result.get(output_field).unwrap().get("name").unwrap(),EventValue::String(ref value) if value==expected),
                    "native inherited historical callable name"
                );
            }
            observations.push((row, normalized(&result)));
        }
    }
    for (input_field, output_field) in [
        ("time", "createdTime"),
        ("updateTime", "updatedTime"),
        ("origQty", "amount"),
    ] {
        let realm = Rc::<PrototypeRealm>::default();
        let _binding = realm.bind();
        let mut row = base();
        row[input_field] = json!("Infinity");
        let result =
            map_binance_historical_order_native(&EventValue::from_json(row.clone()), &realm)
                .unwrap()
                .expect("native nonfinite numeric inputs retain eligible historical record");
        standard_fields(&result, output_field);
        let EventValue::Number(value) = result.get(output_field).unwrap() else {
            panic!("native nonfinite historical output is numeric")
        };
        if output_field == "amount" {
            assert_eq!(
                value, 0.0,
                "native nonfinite parsed quantity clamps to zero"
            );
        } else {
            assert!(
                value.is_infinite() && value.is_sign_positive(),
                "native historical timestamp retains positive Infinity before wire"
            );
        }
        observations.push((row, normalized(&result)));
    }
    assert_eq!(
        observations.len(),
        39,
        "authored twelve inherited members by three output fields plus three numeric paths"
    );
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/order-history.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    for (index, (row, actual)) in observations.into_iter().enumerate() {
        let case = source
            .iter()
            .find(|case| {
                serde_json::from_str::<Value>(case["row"].as_str().unwrap()).unwrap() == row
            })
            .expect("authored historical fixture must exist in source corpus");
        assert_eq!(
            actual, case["original"]["value"],
            "original source native historical record case{index}"
        );
    }
}
