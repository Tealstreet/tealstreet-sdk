use crate::js_runtime::value::{Js, Thrown};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

fn view(value: &Js) -> Value {
    match value {
        Js::String(s) => json!(["string", s.as_ref()]),
        Js::Number(n) => json!(["number", format!("{:016x}", n.to_bits())]),
        Js::Bool(b) => json!(["boolean", b]),
        Js::Null => json!(["null", null]),
        Js::Undefined => json!(["undefined", null]),
        Js::Array(_) => {
            let len = value.get("length").unwrap().to_number().unwrap() as usize;
            let keys: Vec<_> = value
                .own_keys()
                .iter()
                .map(|s| String::from_utf16(s).unwrap())
                .collect();
            json!([
                "array",
                len,
                keys,
                (0..len)
                    .map(|i| {
                        let key = i.to_string();
                        if value.has_own(&key) {
                            view(&value.get(&key).unwrap())
                        } else {
                            json!(["hole"])
                        }
                    })
                    .collect::<Vec<_>>()
            ])
        }
        _ => panic!("unobserved native value"),
    }
}
fn entries(headers: &Js) -> Value {
    json!(
        headers
            .own_entries()
            .unwrap()
            .iter()
            .map(|(key, value)| json!([key.as_ref(), view(value)]))
            .collect::<Vec<_>>()
    )
}
fn scalar(kind: &str) -> Js {
    match kind {
        "undefined" => Js::Undefined,
        "null" => Js::Null,
        "false" => Js::Bool(false),
        "negative-zero" => Js::Number(-0.0),
        "nan" => Js::Number(f64::NAN),
        "infinity" => Js::Number(f64::INFINITY),
        "text" => Js::utf16(&[0xd800]),
        "number" => Js::Number(429.0),
        "string-status" => Js::str("429"),
        _ => unreachable!(),
    }
}
pub fn from(input: &Value, operation: impl Fn(&Js) -> Result<Js, Thrown>) -> Value {
    let source = match input.as_str().unwrap() {
        "scalar" => Js::object(vec![
            ("X", Js::Number(-0.0)),
            ("Null", Js::Null),
            ("Off", Js::Bool(false)),
            ("Utf", Js::utf16(&[0xd800])),
        ]),
        "array" => Js::object(vec![(
            "X",
            Js::array(vec![Js::Number(123.0), Js::Null, Js::Bool(false)]),
        )]),
        "empty" => Js::Undefined,
        _ => unreachable!(),
    };
    let result = operation(&source).unwrap();
    json!({"entries":entries(&result),"get":result.get("get").unwrap().is_callable(),"has":result.get("has").unwrap().is_callable(),"same":result.same_reference(&source)})
}
pub fn key(
    input: &Value,
    operation: impl Fn(&Js, &Js) -> Result<Option<Rc<[u16]>>, Thrown>,
) -> Value {
    let header = Js::object(vec![
        ("X", Js::str("first")),
        ("x", Js::str("second")),
        ("Other", Js::str("third")),
    ]);
    let name = match input.as_str().unwrap() {
        "hit" => Js::str(" X "),
        "miss" => Js::str("missing"),
        "falsy" => Js::Number(-0.0),
        _ => unreachable!(),
    };
    let touched = Rc::new(RefCell::new(false));
    let captured = touched.clone();
    header
        .define_accessor(
            "Unread",
            Some(Rc::new(move |_| {
                *captured.borrow_mut() = true;
                Err(Thrown::error("Error", "unread header value"))
            })),
            None,
        )
        .unwrap();
    let key = operation(&header, &name).unwrap();
    json!({"key":key.map(|k|k.to_vec()),"readValue":*touched.borrow()})
}
pub fn normalize(input: &Value, operation: impl Fn(Js) -> Result<Js, Thrown>) -> Value {
    let kind = input.as_str().unwrap();
    let trace = Rc::new(RefCell::new(Vec::<String>::new()));
    let token = Js::object(vec![]);
    let value = if kind == "nested" {
        Js::array(vec![
            Js::Bool(false),
            Js::Null,
            Js::Undefined,
            Js::array(vec![
                Js::str("\t x\r\n "),
                Js::Number(123.0),
                Js::utf16(&[0xd800]),
            ]),
        ])
    } else {
        Js::array(vec![Js::Number(1.0)])
    };
    match kind {
        "sparse" => {
            value.set("length", Js::Number(4.0)).unwrap();
            value
                .delete(&"0".encode_utf16().collect::<Vec<_>>())
                .unwrap();
            value.set("1", Js::str(" a ")).unwrap();
            value.set("3", Js::Undefined).unwrap();
        }
        "custom-map" => {
            let owner = match &value {
                Js::Array(a) => Rc::downgrade(a),
                _ => unreachable!(),
            };
            let trace = trace.clone();
            value
                .define(
                    "map",
                    Js::function("map", move |this, args| {
                        trace.borrow_mut().push(
                            if owner
                                .upgrade()
                                .is_some_and(|a| this.same_reference(&Js::Array(a)))
                            {
                                "receiver"
                            } else {
                                "wrong"
                            }
                            .into(),
                        );
                        Ok(Js::array(vec![
                            args[0].call(Js::Undefined, vec![Js::str(" X ")])?,
                            args[0].call(Js::Undefined, vec![Js::Bool(false)])?,
                        ]))
                    }),
                )
                .unwrap();
        }
        "map-throws" => {
            let trace = trace.clone();
            let token = token.clone();
            value
                .define_accessor(
                    "map",
                    Some(Rc::new(move |_| {
                        trace.borrow_mut().push("get-map".into());
                        Err(Thrown::new(token.clone()))
                    })),
                    None,
                )
                .unwrap();
        }
        "map-noncallable" => value.define("map", Js::Number(0.0)).unwrap(),
        "nested" => {}
        _ => unreachable!(),
    }
    match operation(value) {
        Ok(value) => json!({"ok":true,"value":view(&value),"trace":trace.borrow().clone()}),
        Err(error) => {
            json!({"ok":false,"name":if error.value.same_reference(&token){"original".into()}else{error.value.get("name").unwrap().to_display()},"trace":trace.borrow().clone()})
        }
    }
}
pub fn set(input: &Value, operation: impl Fn(&Js, Js, Js, Js) -> Result<(), Thrown>) -> Value {
    let headers = Js::object(vec![("X", Js::Bool(false))]);
    let rewrite = match input.as_str().unwrap() {
        "default" => Js::Undefined,
        "true" => Js::Bool(true),
        "false" => Js::Bool(false),
        _ => unreachable!(),
    };
    operation(&headers, Js::str(" x "), Js::str(" new\r\n "), rewrite).unwrap();
    operation(&headers, Js::str("Empty"), Js::Null, Js::Undefined).unwrap();
    json!({"entries":entries(&headers)})
}
pub fn params(input: &Value, operation: impl Fn(&Js) -> Result<Js, Thrown>) -> Value {
    let kind = input.as_str().unwrap();
    let token = Js::object(vec![]);
    let trace = Rc::new(RefCell::new(Vec::<String>::new()));
    let calls = trace.clone();
    let marker = token.clone();
    let function = Js::function("serialize", move |this, args| {
        calls.borrow_mut().push(
            if this.is_undefined() {
                "undefined"
            } else {
                "owner"
            }
            .into(),
        );
        let options = args.get(1).cloned().unwrap_or(Js::Undefined);
        let kind = if options.get("identity")?.truthy() {
            "same-options"
        } else if options.get("serialize")?.is_callable() {
            "wrapped-options"
        } else {
            "wrong-options"
        };
        calls.borrow_mut().push(kind.into());
        if args[0].get("throws")?.truthy() {
            return Err(Thrown::new(marker.clone()));
        }
        Ok(Js::utf16(&[0xd800, 38, 120]))
    });
    let serializer = match kind {
        "function" => function.clone(),
        "object" | "throws" => Js::object(vec![
            ("serialize", function.clone()),
            ("identity", Js::Bool(true)),
        ]),
        "missing" => Js::Undefined,
        "noncallable" => Js::object(vec![("serialize", Js::Bool(false))]),
        _ => unreachable!(),
    };
    let config = Js::object(vec![("paramsSerializer", serializer)]);
    let selected = operation(&config).unwrap();
    let result = if selected.is_callable() {
        match selected.call(
            Js::Undefined,
            vec![Js::object(vec![("throws", Js::Bool(kind == "throws"))])],
        ) {
            Ok(value) => json!({"ok":true,"value":view(&value)}),
            Err(error) => json!({"ok":false,"original":error.value.same_reference(&token)}),
        }
    } else {
        json!({"value":view(&selected)})
    };
    json!({"callable":selected.is_callable(),"result":result,"trace":trace.borrow().clone()})
}
pub fn status(input: &Value, operation: impl Fn(&Js) -> Result<Option<f64>, Thrown>) -> Value {
    let value = scalar(input.as_str().unwrap());
    let result = operation(&Js::object(vec![("status", value)])).unwrap();
    json!(result.map(|n| format!("{:016x}", n.to_bits())))
}
pub fn same(input: &Value, operation: impl Fn(&Js, &Js) -> Result<bool, Thrown>) -> Value {
    let kind = input.as_str().unwrap();
    let (left, right) = match kind {
        "equal" => (
            Js::object(vec![("x", Js::Number(1.0))]),
            Js::object(vec![("x", Js::Number(1.0))]),
        ),
        "different" => (
            Js::object(vec![("x", Js::Number(1.0))]),
            Js::object(vec![("x", Js::Number(2.0))]),
        ),
        "zero" => (Js::Number(-0.0), Js::Number(0.0)),
        "undefined-null" => (Js::Undefined, Js::Null),
        "nonfinite" => (Js::Number(f64::NAN), Js::Null),
        _ => unreachable!(),
    };
    json!(operation(&left, &right).unwrap())
}
