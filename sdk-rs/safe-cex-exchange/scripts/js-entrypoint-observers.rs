use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

fn descriptor(target: &Js, key: &[u16], value: Js) {
    let globals = Js::object(vec![]);
    install_builtins::<()>(&globals, std::rc::Weak::new());
    globals
        .get("Object")
        .unwrap()
        .call_method(
            "defineProperty",
            vec![target.clone(), Js::utf16(key), value],
        )
        .unwrap();
}
fn number(value: &Js) -> f64 {
    value.to_number().unwrap()
}
fn view(value: &Js, token: &Js) -> Value {
    match value {
        Js::Undefined => json!(["undefined"]),
        Js::Null => json!(["null"]),
        Js::Bool(v) => json!(["boolean", v]),
        Js::Number(v) => json!(["number", format!("{:016x}", v.to_bits())]),
        Js::String(v) => json!(["string", v.as_ref()]),
        _ => json!(["reference", value.same_reference(token)]),
    }
}
fn array_view(array: &Js, token: &Js) -> Value {
    let len = number(&array.get("length").unwrap()) as usize;
    json!(
        (0..len)
            .map(|i| if array.has_index(i) {
                view(&array.get(&i.to_string()).unwrap(), token)
            } else {
                json!(["hole"])
            })
            .collect::<Vec<_>>()
    )
}
fn pair(kind: &str) -> (Js, Js) {
    match kind {
        "nan" => (Js::Number(f64::NAN), Js::Number(f64::NAN)),
        "zero" => (Js::Number(-0.0), Js::Number(0.0)),
        "units" => (Js::utf16(&[0xd800]), Js::utf16(&[0xd800])),
        "differentUnits" => (Js::utf16(&[0xd800]), Js::utf16(&[0xfffd])),
        "nullish" => (Js::Null, Js::Undefined),
        "boolean" => (Js::Bool(true), Js::Bool(true)),
        "numberString" => (Js::Number(1.0), Js::str("1")),
        "array" | "otherArray" => {
            let a = Js::array(vec![]);
            let b = if kind == "array" {
                a.clone()
            } else {
                Js::array(vec![])
            };
            (a, b)
        }
        "function" | "otherFunction" => {
            let a = Js::function("f", |_, _| Ok(Js::Undefined));
            let b = if kind == "function" {
                a.clone()
            } else {
                Js::function("f", |_, _| Ok(Js::Undefined))
            };
            (a, b)
        }
        "map" => {
            let a = Js::new_map();
            (a.clone(), a)
        }
        "set" => {
            let a = Js::new_set();
            (a.clone(), a)
        }
        "object" | "otherObject" => {
            let a = Js::object(vec![]);
            let b = if kind == "object" {
                a.clone()
            } else {
                Js::object(vec![])
            };
            (a, b)
        }
        _ => panic!("unreviewed pair"),
    }
}
pub fn equality(input: &Value, operation: impl Fn(&Js, &Js) -> bool) -> Value {
    let (a, b) = pair(input.as_str().unwrap());
    json!(operation(&a, &b))
}
pub fn instance(input: &Value, operation: impl Fn(&Js, &Js) -> Result<bool, Thrown>) -> Value {
    let mode = input.as_str().unwrap();
    let token = Js::object(vec![]);
    let f = Js::function("F", |_, _| Ok(Js::Undefined));
    let proto = Js::object(vec![]);
    f.define("prototype", proto.clone()).unwrap();
    let v = Js::object(vec![]);
    if mode != "miss" {
        v.set("__proto__", proto).unwrap();
    }
    let f = match mode {
        "bound" => f.call_method("bind", vec![Js::Null]).unwrap(),
        "invalid" => {
            f.define("prototype", Js::Number(1.0)).unwrap();
            f
        }
        "throw" => {
            let cause = token.clone();
            f.define_accessor(
                "prototype",
                Some(Rc::new(move |_| Err(Thrown::new(cause.clone())))),
                None,
            )
            .unwrap();
            f
        }
        "noncallable" => Js::object(vec![]),
        _ => f,
    };
    let value = if mode == "primitive" {
        Js::Number(1.0)
    } else {
        v
    };
    match operation(&f, &value) {
        Ok(result) => json!(["ok", result]),
        Err(e) => json!(["error", e.value.same_reference(&token)]),
    }
}
pub fn property(input: &Value, operation: impl Fn(&Js, &[u16], Js) -> Result<(), Thrown>) -> Value {
    let mode = input.as_str().unwrap();
    let key = [0xd800, 0x78];
    let object = Js::object(vec![]);
    let token = Js::object(vec![]);
    let events = Rc::new(RefCell::new(Vec::<bool>::new()));
    if ["setter", "throw"].contains(&mode) {
        let calls = events.clone();
        let expected = match &object {
            Js::Object(data) => Some(Rc::downgrade(data)),
            _ => None,
        };
        let cause = token.clone();
        let fails = mode == "throw";
        // The authored raw key is defined through the descriptor API, never UTF-8.
        descriptor(
            &object,
            &key,
            Js::object(vec![
                ("configurable", Js::Bool(true)),
                (
                    "set",
                    Js::function("set", move |this, args| {
                        calls.borrow_mut().push(
                            this.same_reference(&Js::Object(
                                expected.as_ref().unwrap().upgrade().unwrap(),
                            )) && args[0].same_reference(&cause),
                        );
                        if fails {
                            Err(Thrown::new(cause.clone()))
                        } else {
                            Ok(Js::Undefined)
                        }
                    }),
                ),
            ]),
        );
    }
    if mode == "readonly" {
        descriptor(&object, &key, Js::object(vec![("value", Js::Number(7.0))]));
    }
    let value = if mode == "zero" {
        Js::Number(-0.0)
    } else {
        token.clone()
    };
    let result = operation(&object, &key, value);
    let status = match result {
        Ok(()) => json!(["ok"]),
        Err(e) => json!(["error", e.value.same_reference(&token)]),
    };
    json!({"status":status,"value":view(&object.get_key(&key).unwrap(),&token),"calls":*events.borrow(),"rawKey":object.own_property_names().unwrap().iter().any(|k|k.as_ref()==key)})
}
pub fn observer(
    input: &Value,
    install: impl Fn(&Js, Option<WriteObserver>) -> Result<(), Thrown>,
) -> Value {
    let mode = input.as_str().unwrap();
    let token = Js::object(vec![]);
    let object = if mode == "array" {
        Js::array(vec![])
    } else {
        Js::object(vec![])
    };
    let events = Rc::new(RefCell::new(Vec::<Value>::new()));
    let calls = events.clone();
    let expected = match &object {
        Js::Object(data) => Some(Rc::downgrade(data)),
        _ => None,
    };
    let cause = token.clone();
    let failing = mode == "throw";
    let result = install(
        &object,
        Some(Rc::new(move |receiver, key, value| {
            calls.borrow_mut().push(json!([
                receiver.same_reference(&Js::Object(expected.as_ref().unwrap().upgrade().unwrap())),
                key,
                receiver.get_key(key)?.same_reference(value),
                value.same_reference(&cause)
            ]));
            if failing {
                Err(Thrown::new(cause.clone()))
            } else {
                Ok(())
            }
        })),
    );
    if mode == "array" {
        return json!({"install":result.is_ok(),"calls":[],"status":["none"]});
    }
    result.unwrap();
    if mode == "clear" {
        install(&object, None).unwrap();
    }
    let key = [0xd800];
    let result = object.set_key(&key, token.clone());
    let status = match result {
        Ok(()) => json!(["ok"]),
        Err(e) => json!(["error", e.value.same_reference(&token)]),
    };
    json!({"install":true,"calls":*events.borrow(),"status":status})
}
pub fn push(input: &Value, operation: impl Fn(&Js, Js) -> Result<(), Thrown>) -> Value {
    let mode = input.as_str().unwrap();
    let token = Js::object(vec![]);
    let array = if mode == "notArray" {
        Js::Null
    } else {
        Js::holey_array(vec![None, Some(Js::Undefined)])
    };
    if mode == "readonly" {
        descriptor(
            &array,
            &"length".encode_utf16().collect::<Vec<_>>(),
            Js::object(vec![("writable", Js::Bool(false))]),
        );
    }
    let value = if mode == "zero" {
        Js::Number(-0.0)
    } else {
        token.clone()
    };
    let result = operation(&array, value);
    json!({"ok":result.is_ok(),"array":if mode=="notArray" {Value::Null} else {array_view(&array,&token)}})
}
type Compare<'a> = dyn FnMut(&Js, &Js) -> Result<std::cmp::Ordering, Thrown> + 'a;
pub fn sort(
    input: &Value,
    operation: impl Fn(&Js, &mut Compare<'_>) -> Result<(), Thrown>,
) -> Value {
    let mode = input.as_str().unwrap();
    let token = Js::object(vec![]);
    let array = Js::holey_array(vec![
        Some(Js::Number(3.0)),
        None,
        Some(Js::Undefined),
        Some(Js::Number(-0.0)),
        Some(Js::Number(2.0)),
    ]);
    let mut trace = Vec::new();
    let result = operation(&array, &mut |a, b| {
        trace.push([number(a) as i64, number(b) as i64]);
        if mode == "throw" {
            return Err(Thrown::new(token.clone()));
        }
        let order = number(a).partial_cmp(&number(b)).unwrap();
        Ok(if mode == "descending" {
            order.reverse()
        } else {
            order
        })
    });
    json!({"ok":result.is_ok(),"originalError":result.err().is_some_and(|e|e.value.same_reference(&token)),"array":array_view(&array,&token),"trace":trace})
}
pub fn splice(
    input: &Value,
    operation: impl Fn(&Js, usize, usize, usize, Vec<Js>) -> Result<(), Thrown>,
) -> Value {
    let mode = input.as_str().unwrap();
    let token = Js::object(vec![]);
    let array = Js::holey_array(vec![
        Some(Js::Number(-0.0)),
        None,
        Some(Js::Undefined),
        Some(Js::utf16(&[0xd800])),
        Some(token.clone()),
    ]);
    let (start, count, insert) = match mode {
        "grow" => (1, 0, vec![token.clone(), Js::Number(-0.0)]),
        "shrink" => (1, 3, vec![]),
        "replace" => (0, 2, vec![Js::Undefined]),
        _ => panic!("unreviewed splice"),
    };
    operation(&array, 5, start, count, insert).unwrap();
    array_view(&array, &token)
}
pub fn array(input: &Value, create: impl Fn(Vec<Js>) -> Js) -> Value {
    let token = Js::object(vec![]);
    let value = match input.as_str().unwrap() {
        "zero" => Js::Number(-0.0),
        "units" => Js::utf16(&[0xd800]),
        "reference" => token.clone(),
        _ => panic!("unreviewed native array input"),
    };
    array_view(&create(vec![value, Js::Undefined]), &token)
}
pub fn holey(input: &Value, create: impl Fn(Vec<Option<Js>>) -> Js) -> Value {
    let token = Js::object(vec![]);
    let value = match input.as_str().unwrap() {
        "zero" => Js::Number(-0.0),
        "units" => Js::utf16(&[0xd800]),
        "reference" => token.clone(),
        _ => panic!("unreviewed native holey input"),
    };
    array_view(
        &create(vec![None, Some(value), Some(Js::Undefined)]),
        &token,
    )
}
pub fn call(input: &Value, invoke: impl Fn(&Js, Js, Vec<Js>) -> Result<Js, Thrown>) -> Value {
    let mode = input.as_str().unwrap();
    let token = Js::object(vec![]);
    let expected = token.clone();
    let f = Js::function("native", move |this, args| {
        if matches!(args.first(), Some(Js::Bool(true))) {
            return Err(Thrown::new(expected.clone()));
        }
        assert!(
            this.same_reference(&expected),
            "JS_ENTRYPOINT original call receiver"
        );
        assert!(
            args[0].same_reference(&expected),
            "JS_ENTRYPOINT original call argument"
        );
        Ok(args[1].clone())
    });
    let value = if mode == "zero" {
        Js::Number(-0.0)
    } else {
        Js::utf16(&[0xd800])
    };
    let args = if mode == "throw" {
        vec![Js::Bool(true)]
    } else {
        vec![token.clone(), value]
    };
    match invoke(&f, token.clone(), args) {
        Ok(value) => json!(["ok", view(&value, &token)]),
        Err(e) => json!(["error", e.value.same_reference(&token)]),
    }
}
