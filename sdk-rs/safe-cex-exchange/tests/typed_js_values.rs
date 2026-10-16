//! Native typed values cross the runtime before any JSON wire projection.
use safe_cex_exchange::js_runtime::value::{Js, from_typed, to_json, to_typed};
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
            js.get("scalar").unwrap().as_f64().unwrap().to_bits(),
            value.to_bits(),
            "native-to-JS scalar bits"
        );
        let array = js.get("values").unwrap().array_values().unwrap();
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
    assert!(to_typed::<String>(&Js::String(vec![0xd800].into())).is_err());
}
