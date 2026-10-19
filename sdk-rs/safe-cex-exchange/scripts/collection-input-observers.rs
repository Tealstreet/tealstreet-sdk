use crate::js_runtime::collections::Kind;
use crate::js_runtime::value::{Js, Prop, Thrown};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

fn view(value: &Js, token: &Js) -> Value {
    match value {
        Js::Undefined => json!(["undefined"]),
        Js::Number(n) => json!(["number", format!("{:016x}", n.to_bits())]),
        Js::String(units) => json!(["string", units.as_ref()]),
        Js::Array(_) => json!(
            (0..value.get("length").unwrap().to_number().unwrap() as usize)
                .map(|i| view(&value.get(&i.to_string()).unwrap(), token))
                .collect::<Vec<_>>()
        ),
        _ => json!(["reference", value.same_reference(token)]),
    }
}
fn step(iterator: &Js, token: &Js) -> Value {
    let result = iterator.call_method("next", vec![]).unwrap();
    json!({"done":result.get("done").unwrap().truthy(),"value":view(&result.get("value").unwrap(),token)})
}
pub fn collection(input: &Value, operation: impl Fn(Js, Kind) -> Js) -> Value {
    let mode = input.as_str().unwrap();
    let is_map = mode.starts_with("map");
    let owner = if is_map { Js::new_map() } else { Js::new_set() };
    let token = Js::object(vec![]);
    let insert = |key: &str, value: Js| {
        if is_map {
            owner.call_method("set", vec![Js::str(key), value]).unwrap();
        } else {
            owner.call_method("add", vec![value]).unwrap();
        }
    };
    insert("a", Js::Number(-0.0));
    insert("b", token.clone());
    let kind = if mode.ends_with("keys") {
        Kind::Key
    } else if mode.ends_with("entries") {
        Kind::Entry
    } else {
        Kind::Value
    };
    let iterator = operation(owner.clone(), kind);
    let mut steps = vec![step(&iterator, &token)];
    if mode.ends_with("clear") {
        owner.call_method("clear", vec![]).unwrap();
    } else if mode.ends_with("reinsert") {
        owner
            .call_method(
                "delete",
                vec![if is_map { Js::str("b") } else { token.clone() }],
            )
            .unwrap();
    }
    insert("c", Js::utf16(&[0xd800]));
    if mode.ends_with("reinsert") {
        insert("b", token.clone());
    }
    for _ in 0..4 {
        steps.push(step(&iterator, &token));
    }
    insert("d", Js::Number(7.0));
    steps.push(step(&iterator, &token));
    json!({"ok":true,"self":iterator.iterator().unwrap().same_reference(&iterator),"steps":steps})
}
pub fn method(input: &Value, operation: impl Fn(&Js) -> Result<Js, Thrown>) -> Value {
    lookup(input, |value, _| operation(value))
}
pub fn lookup(input: &Value, operation: impl Fn(&Js, &Js) -> Result<Js, Thrown>) -> Value {
    let mode = input.as_str().unwrap();
    let token = Js::object(vec![]);
    let owner = Js::object(vec![]);
    let prototype = Js::object(vec![]);
    let function = Js::function("custom", |this, _| Ok(this));
    let calls = Rc::new(RefCell::new(Vec::<bool>::new()));
    let holder = if mode.starts_with("inherited") {
        &prototype
    } else {
        &owner
    };
    let slot = holder.symbol_iterator_slot().unwrap();
    let weak = match &owner {
        Js::Object(data) => Rc::downgrade(data),
        _ => unreachable!(),
    };
    let seen = calls.clone();
    let expected = function.clone();
    let cause = token.clone();
    let throws = mode.ends_with("throw");
    *slot.borrow_mut() = Some(if mode.ends_with("data") {
        Prop::Data(Js::Number(-0.0))
    } else if mode.ends_with("absent") {
        Prop::Accessor {
            get: None,
            set: None,
            enumerable: false,
            configurable: false,
        }
    } else {
        Prop::Accessor {
            get: Some(Js::function("get iterator", move |this, _| {
                seen.borrow_mut().push(
                    weak.upgrade()
                        .is_some_and(|data| this.same_reference(&Js::Object(data))),
                );
                if throws {
                    Err(Thrown::new(cause.clone()))
                } else {
                    Ok(expected.clone())
                }
            })),
            set: None,
            enumerable: false,
            configurable: false,
        }
    });
    if mode.starts_with("inherited") {
        owner.set("__proto__", prototype).unwrap();
    }
    let result = operation(&owner, &owner);
    let (ok, value, original) = match result {
        Ok(value) => (
            true,
            if value.same_reference(&function) {
                json!(["method"])
            } else {
                view(&value, &token)
            },
            false,
        ),
        Err(error) => (false, Value::Null, error.value.same_reference(&token)),
    };
    json!({"ok":ok,"value":value,"original":original,"receivers":calls.borrow().clone()})
}
