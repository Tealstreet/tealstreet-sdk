//! Native typed values cross the runtime before any JSON wire projection.
use safe_cex_exchange::js_runtime::value::{Js, from_typed, to_json, to_retained_input, to_typed};
use safe_cex_types::retained_json::RetainedJsonInput;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Numbers {
    scalar: f64,
    values: Vec<Option<f64>>,
}

#[test]
fn typed_numbers_retain_native_bits_in_both_directions() {
    let values = [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        -0.0,
        0.0,
        f64::MAX,
        5e-324,
    ];
    for value in values {
        let native = Numbers {
            scalar: value,
            values: vec![Some(value), None, Some(-0.0)],
        };
        let js = from_typed(&native).unwrap();
        assert_eq!(
            js.get("scalar")
                .unwrap()
                .as_f64()
                .expect("native-to-JS scalar Number retains bits")
                .to_bits(),
            value.to_bits(),
            "native-to-JS scalar bits"
        );
        let array = js
            .get("values")
            .unwrap()
            .array_values()
            .expect("fixture array property reads")
            .unwrap();
        assert_eq!(
            array[0].as_f64().unwrap().to_bits(),
            value.to_bits(),
            "native-to-JS nested bits"
        );
        assert!(matches!(array[1], Js::Null));

        // Decode an independently built receiver value, not the encoder's output.
        let input = Js::object(vec![
            ("scalar", Js::Number(value)),
            (
                "values",
                Js::array(vec![Js::Number(value), Js::Null, Js::Number(-0.0)]),
            ),
        ]);
        let decoded: Numbers =
            to_typed(&input).expect("JS-to-native Number decode retains native bits");
        assert_eq!(
            decoded.scalar.to_bits(),
            value.to_bits(),
            "JS-to-native scalar bits"
        );
        assert_eq!(decoded.values.len(), 3);
        assert_eq!(
            decoded.values[0].unwrap().to_bits(),
            value.to_bits(),
            "JS-to-native nested bits"
        );
        assert!(decoded.values[1].is_none());
        assert_eq!(decoded.values[2].unwrap().to_bits(), (-0.0_f64).to_bits());
    }
    assert!(to_typed::<f64>(&Js::Null).is_err(), "null is not a Number");
    assert!(
        to_typed::<f64>(&Js::Undefined).is_err(),
        "undefined is not a Number"
    );
    assert!(
        to_typed::<serde_json::Value>(&Js::Number(f64::INFINITY)).is_err(),
        "opaque JSON cannot silently erase Infinity"
    );
    assert_eq!(
        to_json(&Js::Number(f64::INFINITY)).unwrap(),
        serde_json::Value::Null,
        "explicit JSON wire policy remains separate"
    );
}

#[test]
fn typed_order_amount_reaches_receiver_math_and_returns_without_json_projection() {
    let mut order: safe_cex_types::PlaceOrderOpts = serde_json::from_value(serde_json::json!({
        "symbol":"BTCUSDT", "type":"limit", "side":"buy", "amount":1, "price":123
    }))
    .unwrap();
    order.amount = f64::INFINITY;
    let js = from_typed(&order).unwrap();
    let amount = js.get("amount").unwrap().to_number().unwrap();
    assert_eq!(
        safe_cex_num::math_min(100.0, safe_cex_num::math_max(1.0, amount)),
        100.0,
        "infinite amount reaches source clamp as Infinity"
    );
    let decoded: safe_cex_types::PlaceOrderOpts = to_typed(&Js::object(vec![
        ("symbol", Js::str("BTCUSDT")),
        ("type", Js::str("limit")),
        ("side", Js::str("buy")),
        ("amount", Js::Number(f64::INFINITY)),
        ("price", Js::Number(123.0)),
    ]))
    .unwrap();
    assert_eq!(
        decoded.amount,
        f64::INFINITY,
        "typed output retains infinite amount"
    );
    assert_eq!(decoded.price, Some(123.0));
}

#[test]
fn typed_projection_keeps_enum_and_optional_object_shapes() {
    #[derive(Debug, Deserialize, PartialEq)]
    enum Choice {
        Unit,
        Value(f64),
    }
    #[derive(Debug, Deserialize)]
    struct Optional {
        optional: Option<f64>,
    }
    assert_eq!(to_typed::<Choice>(&Js::str("Unit")).unwrap(), Choice::Unit);
    assert_eq!(
        to_typed::<Choice>(&Js::object(vec![("Value", Js::Number(f64::INFINITY))])).unwrap(),
        Choice::Value(f64::INFINITY)
    );
    assert!(
        to_typed::<Optional>(&Js::object(vec![("optional", Js::Undefined)]))
            .unwrap()
            .optional
            .is_none()
    );
    assert!(
        to_typed::<Optional>(&Js::object(vec![("optional", Js::Null)]))
            .unwrap()
            .optional
            .is_none()
    );
    let raw = Js::String(vec![0xd800].into());
    assert_eq!(
        to_retained_input(&raw).unwrap(),
        RetainedJsonInput::Text(vec![0xd800])
    );
    assert_eq!(to_typed::<String>(&raw).unwrap(), "\u{fffd}");
    assert_eq!(
        to_retained_input(&raw).unwrap(),
        RetainedJsonInput::Text(vec![0xd800]),
        "typed replacement leaves the native string's original units intact"
    );
}

#[test]
fn currency_rows_keep_source_shapes_through_typed_store_updates() {
    use safe_cex_exchange::js_runtime::{store, value::from_json};
    use safe_cex_exchange::native_store::DefaultStore;
    use safe_cex_types::CurrencyBalance;
    use serde_json::json;
    use std::rc::Rc;
    let source = json!({"used":6.0,"free":9.0,"total":15.0,"upnl":0.0,"currencies":{
        "USD":{"available":9.0,"locked":6.0},
        "BTC":{"used":1.0,"free":2.0,"total":3.0,"upnl":4.0}
    }});
    let native = Rc::new(DefaultStore::new());
    let facade = store::facade(native.clone()).unwrap();
    facade
        .call_method("update", vec![from_json(&json!({"balance":source}))])
        .unwrap();
    let actual = native.balance().unwrap();
    assert_eq!(
        serde_json::to_value(&actual).unwrap(),
        source,
        "Store preserves currency field names without normalization"
    );
    let currencies = actual.currencies.as_ref().unwrap();
    assert!(matches!(
        currencies.get("USD").unwrap(),
        CurrencyBalance::AvailableLocked {
            available: 9.0,
            locked: 6.0
        }
    ));
    assert_eq!(
        currencies.get("BTC").unwrap().as_balance().unwrap().total,
        3.0
    );
    let js = facade.get("balance").unwrap();
    assert_eq!(to_json(&js).unwrap(), source);
    let usd = js.get("currencies").unwrap().get("USD").unwrap();
    usd.set("available", Js::Number(f64::INFINITY)).unwrap();
    usd.set("locked", Js::Number(-0.0)).unwrap();
    let native: safe_cex_types::Balance = to_typed(&js).unwrap();
    match native.currencies.as_ref().unwrap().get("USD").unwrap() {
        CurrencyBalance::AvailableLocked { available, locked } => {
            assert_eq!(*available, f64::INFINITY);
            assert_eq!(locked.to_bits(), (-0.0_f64).to_bits());
        }
        _ => panic!("source currency shape was normalized"),
    }
    let projected = from_typed(&native)
        .unwrap()
        .get("currencies")
        .unwrap()
        .get("USD")
        .unwrap();
    assert_eq!(
        projected.get("available").unwrap().as_f64().unwrap(),
        f64::INFINITY
    );
    assert_eq!(
        projected.get("locked").unwrap().as_f64().unwrap().to_bits(),
        (-0.0_f64).to_bits()
    );
    for invalid in [
        json!([1, 2]),
        json!({"available":1}),
        json!({"available":null,"locked":2}),
    ] {
        assert!(serde_json::from_value::<CurrencyBalance>(invalid).is_err());
    }
}

#[test]
fn declared_dto_profile_keeps_present_undefined_and_refuses_callables() {
    use safe_cex_exchange::js_runtime::value::to_typed_preserving_undefined;
    use std::collections::BTreeMap;
    let input = Js::object(vec![
        ("missing_value", Js::Undefined),
        ("null_value", Js::Null),
        ("negative_zero", Js::Number(-0.0)),
        ("infinity", Js::Number(f64::INFINITY)),
    ]);
    let dto: BTreeMap<String, Option<f64>> = to_typed_preserving_undefined(&input).unwrap();
    assert_eq!(dto.len(), 4, "DTO own undefined remains a present field");
    assert_eq!(dto.get("missing_value"), Some(&None));
    assert_eq!(dto.get("null_value"), Some(&None));
    assert!(!dto.contains_key("absent"));
    assert_eq!(dto["negative_zero"].unwrap().to_bits(), (-0.0f64).to_bits());
    assert_eq!(dto["infinity"], Some(f64::INFINITY));
    let json_shape: BTreeMap<String, Option<f64>> = to_typed(&input).unwrap();
    assert!(!json_shape.contains_key("missing_value"));
    assert_eq!(json_shape.get("null_value"), Some(&None));
    let native = Js::Builtin(
        std::rc::Rc::<safe_cex_num::PrototypeRealm>::default()
            .reference(safe_cex_num::ProtoMember::Constructor),
    );
    input.define("callable", native).unwrap();
    assert!(
        to_typed_preserving_undefined::<BTreeMap<String, Option<f64>>>(&input).is_err(),
        "DTO callable field refuses rather than silently omitting"
    );
}

#[test]
fn integer_fields_accept_whole_js_numbers_and_reject_fractional_or_overflow() {
    assert_eq!(to_typed::<u32>(&Js::Number(1.0)).unwrap(), 1);
    assert_eq!(to_typed::<u32>(&Js::Number(-0.0)).unwrap(), 0);
    assert_eq!(
        to_typed::<u32>(&Js::Number(u32::MAX as f64)).unwrap(),
        u32::MAX
    );
    assert_eq!(
        to_typed::<i32>(&Js::Number(i32::MIN as f64)).unwrap(),
        i32::MIN
    );
    assert_eq!(
        to_typed::<i128>(&Js::Number(-2.0f64.powi(127))).unwrap(),
        i128::MIN
    );
    assert_eq!(
        to_typed::<u128>(&Js::Number(2.0f64.powi(127))).unwrap(),
        1u128 << 127
    );
    for number in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        -1.0,
        0.5,
        4294967296.0,
    ] {
        assert!(to_typed::<u32>(&Js::Number(number)).is_err(), "{number}");
    }
    assert!(to_typed::<i32>(&Js::Number(2147483648.0)).is_err());
    assert!(to_typed::<i64>(&Js::Number(2.0f64.powi(63))).is_err());
    assert!(to_typed::<u64>(&Js::Number(2.0f64.powi(64))).is_err());
    assert!(to_typed::<u128>(&Js::Number(2.0f64.powi(128))).is_err());
    assert!(to_typed::<i128>(&Js::Number(2.0f64.powi(127))).is_err());
    assert_eq!(to_typed::<Option<u32>>(&Js::Number(7.0)).unwrap(), Some(7));
    assert!(to_typed::<u32>(&Js::str("7")).is_err());
}
