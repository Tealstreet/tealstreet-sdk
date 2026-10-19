use crate::js_runtime::value::Js;
use serde_json::{Value, json};

fn view(value: &Js, token: &Js) -> Value {
    match value {
        Js::Undefined => json!(["undefined"]),
        Js::Null => json!(["null"]),
        Js::Bool(b) => json!(["boolean", b]),
        Js::Number(n) if n.is_nan() => json!(["nan"]),
        Js::Number(n) => json!(["number", format!("{:016x}", n.to_bits())]),
        Js::String(units) => json!(["string", units.as_ref()]),
        Js::BigInt(n) => json!(["bigint", n.to_string()]),
        Js::SymbolIterator => json!(["symbol"]),
        _ => json!(["reference", value.same_reference(token)]),
    }
}

fn pair(mode: &str) -> (Js, Js) {
    match mode {
        "nan" => (Js::Number(f64::NAN), Js::Number(f64::NAN)),
        "nan-payload" => (
            Js::Number(f64::from_bits(0x7ff8_0000_0000_0001)),
            Js::Number(f64::from_bits(0x7ff8_0000_0000_0002)),
        ),
        "nan-sign" => (Js::Number(f64::NAN), Js::Number(-f64::NAN)),
        "zero" => (Js::Number(-0.0), Js::Number(0.0)),
        "minus-zero" => (Js::Number(-0.0), Js::Number(-0.0)),
        "plus-zero" => (Js::Number(0.0), Js::Number(0.0)),
        "infinity" => (Js::Number(f64::INFINITY), Js::Number(f64::INFINITY)),
        "infinity-sign" => (Js::Number(f64::INFINITY), Js::Number(f64::NEG_INFINITY)),
        "number" => (Js::Number(1.0), Js::Number(1.0)),
        "other-number" => (Js::Number(1.0), Js::Number(2.0)),
        "nullish" => (Js::Null, Js::Undefined),
        "undefined" => (Js::Undefined, Js::Undefined),
        "null" => (Js::Null, Js::Null),
        "boolean" => (Js::Bool(true), Js::Bool(true)),
        "other-boolean" => (Js::Bool(true), Js::Bool(false)),
        "boolean-number" => (Js::Bool(true), Js::Number(1.0)),
        "number-string" => (Js::Number(1.0), Js::str("1")),
        "units" => (Js::utf16(&[0xd800]), Js::utf16(&[0xd800])),
        "other-units" => (Js::utf16(&[0xd800]), Js::utf16(&[0xfffd])),
        "high-low" => (Js::utf16(&[0xd800]), Js::utf16(&[0xdc00])),
        "bigint" => (
            Js::BigInt(std::rc::Rc::new(17.into())),
            Js::BigInt(std::rc::Rc::new(17.into())),
        ),
        "other-bigint" => (
            Js::BigInt(std::rc::Rc::new(17.into())),
            Js::BigInt(std::rc::Rc::new(18.into())),
        ),
        "bigint-number" => (Js::BigInt(std::rc::Rc::new(17.into())), Js::Number(17.0)),
        "symbol" => (Js::SymbolIterator, Js::SymbolIterator),
        "object" | "other-object" | "array" | "other-array" | "function" | "other-function"
        | "map" | "other-map" | "set" | "other-set" => {
            let make = || match mode.trim_start_matches("other-") {
                "array" => Js::array(vec![]),
                "function" => Js::function("f", |_, _| Ok(Js::Undefined)),
                "map" => Js::new_map(),
                "set" => Js::new_set(),
                _ => Js::object(vec![]),
            };
            let left = make();
            let right = if mode.starts_with("other-") {
                make()
            } else {
                left.clone()
            };
            (left, right)
        }
        _ => panic!("unreviewed value pair: {mode}"),
    }
}

pub fn equality(input: &Value, operation: impl Fn(&Js, &Js) -> bool) -> Value {
    let (left, right) = pair(input.as_str().unwrap());
    json!({"ok":true,"equal":operation(&left,&right),"left":view(&left,&left),"right":view(&right,&left)})
}

pub fn optional(input: &Value, operation: impl Fn(&Option<Js>, &Option<Js>) -> bool) -> Value {
    let mode = input.as_str().unwrap();
    let (left, right) = match mode {
        "none" => (None, None),
        "missing-left" => (None, Some(Js::Undefined)),
        "missing-right" => (Some(Js::Undefined), None),
        _ => {
            let (a, b) = pair(mode);
            (Some(a), Some(b))
        }
    };
    json!({"ok":true,"equal":operation(&left,&right),"leftPresent":left.is_some(),"rightPresent":right.is_some()})
}

pub fn argument(input: &Value, operation: impl Fn(&[Js], usize) -> Js) -> Value {
    let token = Js::object(vec![]);
    let array = Js::array(vec![]);
    let function = Js::function("token", |_, _| Ok(Js::Undefined));
    let values = vec![
        Js::Undefined,
        Js::Null,
        Js::Number(-0.0),
        Js::Number(42.0),
        Js::Number(f64::NAN),
        Js::utf16(&[0xd800, 0xdc00, 0xdfff]),
        token.clone(),
        array.clone(),
        function.clone(),
        Js::BigInt(std::rc::Rc::new(17.into())),
    ];
    let value = operation(&values, input.as_u64().unwrap() as usize);
    let view = match &value {
        Js::Undefined => json!(["undefined"]),
        Js::Null => json!(["null"]),
        Js::Number(n) if n.is_nan() => json!(["nan"]),
        Js::Number(n) => json!(["number", format!("{:016x}", n.to_bits())]),
        Js::String(units) => json!(["string", units.as_ref()]),
        Js::BigInt(n) => json!(["bigint", n.to_string()]),
        _ => json!([
            "reference",
            value.same_reference(&token),
            value.same_reference(&array),
            value.same_reference(&function)
        ]),
    };
    json!({"ok":true,"value":view,"originalReference":values.get(input.as_u64().unwrap() as usize).is_some_and(|original|value.same_reference(original))})
}
