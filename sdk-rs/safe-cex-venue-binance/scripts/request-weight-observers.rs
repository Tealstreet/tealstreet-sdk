// Finite observations of the actual header and rate-limit production functions.
use crate::request_weight::{Failure, HeaderGetMember};
use safe_cex_exchange::runtime::ExchangeError;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};
fn value(v: &Value) -> Value {
    match v {
        Value::Number(n) => json!({"bits":format!("{:016x}",n.as_f64().unwrap().to_bits())}),
        _ => v.clone(),
    }
}
fn outcome(result: Result<Option<Value>, Failure>, original: &Failure) -> Value {
    match result {
        Ok(Some(v)) => json!({"some":value(&v)}),
        Ok(None) => json!({"none":true}),
        Err(e) => json!({"name":e.name,"message":e.message,"original":Rc::ptr_eq(&e,original)}),
    }
}
pub fn header(
    input: &Value,
    f: impl FnOnce(
        bool,
        &str,
        &mut dyn FnMut() -> Result<HeaderGetMember, Failure>,
        &mut dyn FnMut(&str) -> Result<Option<Value>, Failure>,
    ) -> Result<Option<Value>, Failure>,
) -> Value {
    let events = Rc::new(RefCell::new(Vec::<Value>::new()));
    let original = Rc::new(ExchangeError::new("header failure"));
    let mut reads = 0;
    let mut get = || {
        reads += 1;
        events.borrow_mut().push(json!(["get", reads]));
        if input["mode"] == "member-error" {
            return Err(original.clone());
        }
        if input["mode"] == "call" || (input["mode"] == "flip" && reads == 1) {
            let events = events.clone();
            let answer = input["answer"].clone();
            Ok(HeaderGetMember::Callable(Rc::new(move |name| {
                events.borrow_mut().push(json!(["call", name]));
                Ok(Some(answer.clone()))
            })))
        } else {
            Ok(HeaderGetMember::NotCallable)
        }
    };
    let mut prop = |key: &str| {
        events.borrow_mut().push(json!(["property", key]));
        if input["mode"] == "property-error" {
            return Err(original.clone());
        }
        Ok(input["properties"].get(key).cloned())
    };
    let result = f(
        input["has"].as_bool().unwrap(),
        "X-Test",
        &mut get,
        &mut prop,
    );
    json!({"result":outcome(result,&original),"events":*events.borrow()})
}
pub fn weight(
    input: &Value,
    f: impl FnOnce(
        Option<&Value>,
        &mut dyn FnMut(&str) -> Result<Option<Value>, Failure>,
        &mut dyn FnMut(f64) -> Result<(), Failure>,
    ) -> Result<(), Failure>,
) -> Value {
    let events = RefCell::new(vec![]);
    let original = Rc::new(ExchangeError::new("weight failure"));
    let mut header = |key: &str| {
        events.borrow_mut().push(json!(["header", key]));
        if input["fail"] == "header" {
            return Err(original.clone());
        }
        Ok(input.get("value").cloned())
    };
    let mut pause = |duration: f64| {
        events
            .borrow_mut()
            .push(json!(["pause", format!("{:016x}", duration.to_bits())]));
        if input["fail"] == "pause" {
            Err(original.clone())
        } else {
            Ok(())
        }
    };
    let result = f(input.get("status"), &mut header, &mut pause);
    json!({"result":outcome(result.map(|()| None),&original),"events":*events.borrow()})
}
pub fn diagnostic(input: &Value, f: impl FnOnce(String) -> Failure) -> Value {
    let error = f(input.as_str().unwrap().to_owned());
    assert!(
        error.code.is_none()
            && error.retained_response_input().is_none()
            && error.original_error.is_none()
    );
    json!({"name":error.name,"units":error.message.encode_utf16().collect::<Vec<_>>()})
}
