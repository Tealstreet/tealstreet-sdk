use crate::private_event_value::{EventArray, EventOps, EventValue};
use crate::public_ticker_events::{Failure, TickerFields};
use safe_cex_exchange::runtime::ExchangeError;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn bits(value: f64) -> String {
    if value.is_nan() {
        "7ff8000000000000".into()
    } else {
        format!("{:016x}", value.to_bits())
    }
}

fn native_number(input: &Value) -> Option<f64> {
    match input["nativeNumber"].as_str()? {
        "nan" => Some(f64::NAN),
        "positive-infinity" => Some(f64::INFINITY),
        "negative-infinity" => Some(f64::NEG_INFINITY),
        "negative-zero" => Some(-0.0),
        profile => panic!("unknown reviewed native Number profile: {profile}"),
    }
}

type Dictionary = Rc<Vec<String>>;
type Capture = Box<dyn FnOnce() -> Result<Dictionary, Failure>>;
type Lookup = Box<dyn FnMut(&Dictionary, &str) -> Result<Option<String>, Failure>>;
type Update = Box<dyn FnOnce(String, TickerFields) -> Result<(), Failure>>;
type GetUpdate = Box<dyn FnMut() -> Result<Update, Failure>>;

pub fn handle(
    input: &Value,
    run: impl FnOnce(&EventArray, Capture, Lookup, GetUpdate) -> Result<(), Failure>,
) -> Value {
    let events: EventArray = Rc::new(RefCell::new(
        input["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| Some(EventValue::from_json(row.clone())))
            .collect(),
    ));
    if let Some(value) = native_number(input) {
        let event = events.borrow()[0].clone().unwrap();
        for key in ["c", "p", "b"] {
            event.write(key, EventValue::Number(value));
        }
    }
    let trace = Rc::new(RefCell::new(Vec::<String>::new()));
    let updates = Rc::new(RefCell::new(Vec::<Value>::new()));
    let mode = input["mode"].as_str().unwrap_or("ordinary").to_string();
    let original = Rc::new(ExchangeError::new("configured ticker callback failure"));
    let log = trace.clone();
    let error = original.clone();
    let phase = mode.clone();
    let capture: Capture = Box::new(move || {
        log.borrow_mut().push("capture".into());
        if phase == "capture-error" {
            Err(error)
        } else {
            Ok(Rc::new(vec!["BTC".into(), "ETH".into()]))
        }
    });
    let log = trace.clone();
    let error = original.clone();
    let phase = mode.clone();
    let lookup: Lookup = Box::new(move |dictionary, key| {
        log.borrow_mut().push(format!("lookup:{key}"));
        if phase == "lookup-error" {
            return Err(error.clone());
        }
        Ok(dictionary.iter().find(|row| row.as_str() == key).cloned())
    });
    let log = trace.clone();
    let error = original.clone();
    let records = updates.clone();
    let mutable_events = events.clone();
    let changed = Rc::new(Cell::new(false));
    let append = input.get("append").cloned();
    let get_update: GetUpdate = Box::new(move || {
        log.borrow_mut().push("get-update".into());
        if mode == "get-update-error" {
            return Err(error.clone());
        }
        let log = log.clone();
        let records = records.clone();
        let mutable_events = mutable_events.clone();
        let changed = changed.clone();
        let append = append.clone();
        let phase = mode.clone();
        let error = error.clone();
        Ok(Box::new(move |key, fields| {
            log.borrow_mut().push(format!("update:{key}"));
            records.borrow_mut().push(json!([
                key,
                fields
                    .into_iter()
                    .map(|(field, value)| json!([field.name(), bits(value)]))
                    .collect::<Vec<_>>()
            ]));
            if !changed.replace(true) {
                if phase == "append" {
                    mutable_events
                        .borrow_mut()
                        .push(Some(EventValue::from_json(append.unwrap())));
                } else if phase == "truncate" {
                    mutable_events.borrow_mut().clear();
                }
            }
            if phase == "update-error" {
                Err(error)
            } else {
                Ok(())
            }
        }))
    });
    let result = run(&events, capture, lookup, get_update);
    json!({"ok":result.is_ok(), "originalError":result.as_ref().err().is_some_and(|error| Rc::ptr_eq(error, &original)),
        "trace":*trace.borrow(), "updates":*updates.borrow(), "length":events.borrow().len()})
}

pub fn field(input: &Value, read: impl FnOnce(&EventValue, &str) -> Result<f64, Failure>) -> Value {
    let event = EventValue::from_json(input["event"].clone());
    if let Some(value) = native_number(input) {
        event.write("c", EventValue::Number(value));
    }
    match read(&event, input["key"].as_str().unwrap()) {
        Ok(value) => json!({"ok":true,"bits":bits(value)}),
        Err(error) => json!({"ok":false,"name":error.name}),
    }
}
