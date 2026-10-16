use safe_cex_venue_bitrue::refresh_entrypoints as entry;
use serde_json::Value;
use std::{
    cell::RefCell,
    future::Future,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};

#[derive(Debug, Clone)]
enum NativeValue {
    Undefined,
    Null,
    String(Vec<u16>),
    Number(u64),
    Boolean(bool),
    Identity(Rc<()>),
}
impl PartialEq for NativeValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Undefined, Self::Undefined) | (Self::Null, Self::Null) => true,
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Number(a), Self::Number(b)) => a == b,
            (Self::Boolean(a), Self::Boolean(b)) => a == b,
            (Self::Identity(a), Self::Identity(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}
#[derive(Debug, PartialEq)]
enum Event {
    Resolve,
    Parent,
    Read,
    Queue(Vec<NativeValue>),
    Emit(Vec<u16>),
    Construct(Vec<u16>),
}
#[derive(Debug)]
enum Outcome {
    Void,
    Original(Rc<()>),
    Constructed(Rc<()>),
}
impl PartialEq for Outcome {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Void, Self::Void) => true,
            (Self::Original(a), Self::Original(b)) => Rc::ptr_eq(a, b),
            (Self::Constructed(a), Self::Constructed(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}
fn units(value: &Value) -> Vec<u16> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| u16::try_from(v.as_u64().unwrap()).unwrap())
        .collect()
}
fn decode_value(value: &Value, token: &Rc<()>) -> NativeValue {
    match value["kind"].as_str().unwrap() {
        "undefined" => NativeValue::Undefined,
        "null" => NativeValue::Null,
        "string" => NativeValue::String(units(&value["units"])),
        "number" => {
            NativeValue::Number(u64::from_str_radix(value["bits"].as_str().unwrap(), 16).unwrap())
        }
        "boolean" => NativeValue::Boolean(value["boolean"].as_bool().unwrap()),
        "identity" => NativeValue::Identity(token.clone()),
        other => panic!("unexpected source value {other}"),
    }
}
fn decode_event(value: &Value, token: &Rc<()>) -> Event {
    match value["kind"].as_str().unwrap() {
        "resolve" => Event::Resolve,
        "parent" => Event::Parent,
        "read" => Event::Read,
        "queue" => Event::Queue(
            value["args"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| decode_value(v, token))
                .collect(),
        ),
        "emit" => Event::Emit(units(&value["message"])),
        "construct" => Event::Construct(units(&value["message"])),
        other => panic!("unexpected source event {other}"),
    }
}
fn decode_outcome(value: &Value, thrown: &Rc<()>, constructed: &Rc<()>) -> Outcome {
    match value["kind"].as_str().unwrap() {
        "void" => Outcome::Void,
        "original" => Outcome::Original(thrown.clone()),
        "constructed" => Outcome::Constructed(constructed.clone()),
        other => panic!("unexpected source result {other}"),
    }
}
fn ready<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("REFRESH source async body has no suspension"),
    }
}
fn completion(result: Result<(), Rc<()>>) -> Outcome {
    match result {
        Ok(()) => Outcome::Void,
        Err(value) => Outcome::Original(value),
    }
}
fn symbol(id: &str, token: &Rc<()>) -> NativeValue {
    match id {
        "empty" => NativeValue::String(vec![]),
        "string" => NativeValue::String("E-BTC-USDT".encode_utf16().collect()),
        "surrogate" => NativeValue::String(vec![0xd800, 0x58, 0xdfff]),
        "zero" => NativeValue::Number((-0.0_f64).to_bits()),
        "nan" => NativeValue::Number(f64::NAN.to_bits()),
        "false" => NativeValue::Boolean(false),
        "null-symbol" => NativeValue::Null,
        "identity" | "getter" | "getter-throw" | "queue-throw" | "resolve-throw" => {
            NativeValue::Identity(token.clone())
        }
        "null" | "undefined" | "missing" | "primitive" => NativeValue::Undefined,
        other => panic!("unexpected fixed input {other}"),
    }
}
const FILL_IDS: [&str; 16] = [
    "null",
    "undefined",
    "missing",
    "empty",
    "string",
    "surrogate",
    "zero",
    "nan",
    "false",
    "null-symbol",
    "identity",
    "primitive",
    "resolve-throw",
    "getter",
    "getter-throw",
    "queue-throw",
];
const ACCOUNT_IDS: [&str; 6] = [
    "undefined",
    "null",
    "identity",
    "proxy",
    "queue-throw",
    "resolve-throw",
];
const MARGIN_IDS: [&str; 5] = [
    "false",
    "true",
    "ignored-proxies",
    "emit-throw",
    "constructor-throw",
];

#[test]
fn refresh_entrypoints_match_actual_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/refresh-entrypoints-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "REFRESH source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let token = Rc::new(());
    let thrown = Rc::new(());
    let constructed = Rc::new(());
    let mut compared = 0;
    for (method, ids, start) in [
        ("fill", FILL_IDS.as_slice(), 0),
        ("account", ACCOUNT_IDS.as_slice(), 16),
        ("margin", MARGIN_IDS.as_slice(), 22),
    ] {
        for (index, id) in ids.iter().copied().enumerate() {
            let row = &rows[start + index];
            let events = RefCell::new(Vec::new());
            let actual = match method {
                "fill" => {
                    let fill = (!matches!(id, "null" | "undefined")).then_some(());
                    completion(entry::handle_fill_for_fast_refresh(
                        fill.as_ref(),
                        || {
                            events.borrow_mut().push(Event::Resolve);
                            if id == "resolve-throw" {
                                return Err(thrown.clone());
                            }
                            Ok(|argument: Option<NativeValue>| {
                                events.borrow_mut().push(Event::Queue(vec![
                                    argument.unwrap_or(NativeValue::Undefined),
                                ]));
                                if id == "queue-throw" {
                                    Err(thrown.clone())
                                } else {
                                    Ok(token.clone())
                                }
                            })
                        },
                        |_| {
                            if matches!(id, "getter" | "getter-throw" | "resolve-throw") {
                                events.borrow_mut().push(Event::Read);
                            }
                            if id == "getter-throw" {
                                Err(thrown.clone())
                            } else {
                                Ok(symbol(id, &token))
                            }
                        },
                    ))
                }
                "account" => completion(entry::handle_account_update(&token, || {
                    events.borrow_mut().extend([Event::Parent, Event::Resolve]);
                    if id == "resolve-throw" {
                        return Err(thrown.clone());
                    }
                    Ok(|| {
                        events.borrow_mut().push(Event::Queue(vec![]));
                        if id == "queue-throw" {
                            Err(thrown.clone())
                        } else {
                            Ok(token.clone())
                        }
                    })
                })),
                "margin" => {
                    let pending = entry::change_margin_type(
                        &token,
                        &token,
                        |message| {
                            events
                                .borrow_mut()
                                .push(Event::Emit(message.encode_utf16().collect()));
                            if id == "emit-throw" {
                                Err(thrown.clone())
                            } else {
                                Ok(token.clone())
                            }
                        },
                        |message| {
                            events
                                .borrow_mut()
                                .push(Event::Construct(message.encode_utf16().collect()));
                            if id == "constructor-throw" {
                                Err(thrown.clone())
                            } else {
                                Ok(constructed.clone())
                            }
                        },
                    );
                    let expected_immediate: Vec<_> = row["immediate"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|event| decode_event(event, &token))
                        .collect();
                    assert_eq!(
                        *events.borrow(),
                        expected_immediate,
                        "BITRUE-REFRESH-NATIVE-BEFORE-POLL margin case{index} {id}"
                    );
                    match ready(pending) {
                        Ok(()) => Outcome::Void,
                        Err(error) if Rc::ptr_eq(&error, &thrown) => Outcome::Original(error),
                        Err(error) => Outcome::Constructed(error),
                    }
                }
                _ => unreachable!(),
            };
            // Native result and ordered effects are decoded independently of Rust serialization.
            assert_eq!(
                actual,
                decode_outcome(&row["result"], &thrown, &constructed),
                "BITRUE-REFRESH-NATIVE-RESULT {method} case{index} {id}"
            );
            let expected_events: Vec<_> = row["events"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| decode_event(e, &token))
                .collect();
            assert_eq!(
                *events.borrow(),
                expected_events,
                "BITRUE-REFRESH-NATIVE-EFFECTS {method} case{index} {id}"
            );
            compared += 1;
        }
    }
    // All native assertions precede the independently prescribed inventory checks.
    assert_eq!(compared, 27, "REFRESH independent native floor");
    assert_eq!(rows.len(), 27, "REFRESH independent oracle floor");
    for (method, ids, start) in [
        ("fill", FILL_IDS.as_slice(), 0),
        ("account", ACCOUNT_IDS.as_slice(), 16),
        ("margin", MARGIN_IDS.as_slice(), 22),
    ] {
        for (index, id) in ids.iter().enumerate() {
            assert_eq!(rows[start + index]["method"].as_str(), Some(method));
            assert_eq!(rows[start + index]["id"].as_str(), Some(*id));
        }
    }
    println!("BITRUE-REFRESH floor: 27 native cases; last margin case4 constructor-throw");
}
