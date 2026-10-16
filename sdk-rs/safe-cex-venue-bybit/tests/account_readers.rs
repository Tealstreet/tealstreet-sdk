use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::MarginType;
use safe_cex_venue_bybit::account_readers::*;
use serde_json::{Map, Value};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};

fn drive<F: Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    let waker = Waker::noop().clone();
    let mut context = Context::from_waker(&waker);
    for _ in 0..4 {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
    }
    panic!("READER controlled future did not resolve in four polls");
}
async fn pending_once() {
    let mut pending = true;
    std::future::poll_fn(|context| {
        if pending {
            pending = false;
            context.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    })
    .await;
}
fn error(name: &str, message: &str) -> Rc<ExchangeError> {
    let mut error = ExchangeError::new(message);
    error.name = name.into();
    Rc::new(error)
}
fn native_value(actual: &Value, expected: &Value, label: &str) {
    match actual {
        Value::Number(_) => {
            let number = safe_cex_num::numeric_ingress::js_value_as_f64(actual).unwrap();
            assert_eq!(
                format!("{:016x}", number.to_bits()),
                expected["number"].as_str().unwrap(),
                "READER native number {label}"
            );
        }
        Value::Array(values) => {
            let expected = expected.as_array().unwrap();
            assert_eq!(values.len(), expected.len(), "READER native array {label}");
            for (value, expected) in values.iter().zip(expected) {
                native_value(value, expected, label);
            }
        }
        Value::Object(values) => {
            let expected = expected.as_object().unwrap();
            assert_eq!(values.len(), expected.len(), "READER native fields {label}");
            for (key, value) in values {
                native_value(value, expected.get(key).expect(key), label);
            }
        }
        _ => assert_eq!(actual, expected, "READER native scalar {label}"),
    }
}
fn optional(actual: Option<&Value>, expected: &Value, label: &str) {
    if expected["undefined"] == true {
        assert!(actual.is_none(), "READER native absent {label}");
    } else {
        assert!(actual.is_some(), "READER native present {label}");
        native_value(actual.expect(label), expected, label);
    }
}
fn result<T>(
    actual: &Result<T, Rc<ExchangeError>>,
    expected: &Value,
    remote: &Rc<ExchangeError>,
    logging: &Rc<ExchangeError>,
    writer: &Rc<ExchangeError>,
) {
    assert_eq!(
        actual.is_ok(),
        expected.get("error").is_none(),
        "READER native rejection"
    );
    if let Err(actual) = actual {
        assert_eq!(actual.name, expected["error"], "READER native error name");
        assert_eq!(
            actual.message, expected["message"],
            "READER native error message"
        );
        for (original, key) in [
            (remote, "sameRemote"),
            (logging, "sameLogging"),
            (writer, "sameWriter"),
        ] {
            assert_eq!(
                Rc::ptr_eq(actual, original),
                expected[key] == true,
                "READER native error identity {key}"
            );
        }
    }
}
#[derive(Debug)]
enum Event {
    Get(String),
    Resolved,
    Log(Option<Value>),
    Margin(MarginType),
    Status(Option<Value>),
    Disposed(bool),
    Stored,
    Raw,
    Format(Rc<Value>),
    Balance,
    Update(Rc<Value>),
}
fn events(actual: &[Event], expected: &Value) {
    let expected = expected.as_array().unwrap();
    assert_eq!(actual.len(), expected.len(), "READER native callback count");
    for (event, expected) in actual.iter().zip(expected) {
        let kind = match event {
            Event::Get(value) => {
                assert_eq!(value, &expected[1], "READER native endpoint");
                "get"
            }
            Event::Resolved => "resolved",
            Event::Log(value) => {
                optional(value.as_ref(), &expected[1], "log argument");
                "log"
            }
            Event::Margin(value) => {
                assert_eq!(
                    *value,
                    if expected[1] == "isolated" {
                        MarginType::Isolated
                    } else {
                        MarginType::Cross
                    },
                    "READER native margin callback"
                );
                "margin"
            }
            Event::Status(value) => {
                optional(value.as_ref(), &expected[1], "status argument");
                "status"
            }
            Event::Disposed(value) => {
                assert_eq!(
                    *value,
                    expected[1] == true,
                    "READER native disposed callback"
                );
                "disposed"
            }
            Event::Stored => "stored",
            Event::Raw => "raw",
            Event::Format(value) => {
                native_value(value, &expected[1], "format argument");
                "format"
            }
            Event::Balance => "balance",
            Event::Update(value) => {
                native_value(value, &expected[1], "update argument");
                "update"
            }
        };
        assert_eq!(kind, expected[0], "READER native callback order");
    }
}
fn data(row: &Value) -> Map<String, Value> {
    serde_json::from_str(row["dataRaw"].as_str().unwrap()).unwrap()
}

#[test]
#[ignore = "requires installed live TypeScript"]
fn account_readers_native_source_parity() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/account-reader-fixtures.cjs"
        ))
        .output()
        .expect("live source oracle");
    assert!(
        output.status.success(),
        "READER source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let fixtures: Value = serde_json::from_slice(&output.stdout).unwrap();
    let accounts = fixtures["accounts"].as_array().unwrap();
    let margins = fixtures["margins"].as_array().unwrap();
    let balances = fixtures["balances"].as_array().unwrap();
    let hyro = fixtures["hyro"].as_array().unwrap();
    assert_eq!(accounts.len(), 36, "READER independent account floor");
    assert_eq!(margins.len(), 36, "READER independent margin floor");
    assert_eq!(balances.len(), 24, "READER independent balance floor");
    assert_eq!(hyro.len(), 8, "READER independent Hyro floor");
    for row in accounts {
        let trace = RefCell::new(Vec::new());
        let remote = error("RemoteError", "transport failed");
        let logging = error("Unused", "unused");
        let writer = error("Unused", "unused");
        let actual = drive(get_account(|endpoint| {
            trace.borrow_mut().push(Event::Get(endpoint.into()));
            let remote = Rc::clone(&remote);
            let trace = &trace;
            async move {
                pending_once().await;
                trace.borrow_mut().push(Event::Resolved);
                if row["remoteFail"] == true {
                    Err(remote)
                } else {
                    Ok(data(row))
                }
            }
        }));
        result(&actual, &row["result"], &remote, &logging, &writer);
        if let Ok((user_id, affiliate_id)) = actual {
            optional(
                user_id.as_ref(),
                &row["result"]["value"]["userId"],
                "userId",
            );
            optional(
                affiliate_id.as_ref(),
                &row["result"]["value"]["affiliateId"],
                "affiliateId",
            );
        }
        events(&trace.borrow(), &row["events"]);
    }
    for row in margins {
        let trace = RefCell::new(Vec::new());
        let remote = error("RemoteError", "transport failed");
        let logging = error("LogError", "logging failed");
        let writer = error("WriteError", "write failed");
        let margin = Cell::new(MarginType::Cross);
        let status = RefCell::new(Some(serde_json::json!(3)));
        let get = |endpoint: &str| {
            trace.borrow_mut().push(Event::Get(endpoint.into()));
            let remote = Rc::clone(&remote);
            let trace = &trace;
            async move {
                pending_once().await;
                trace.borrow_mut().push(Event::Resolved);
                if row["remoteFail"] == true {
                    Err(remote)
                } else {
                    Ok(data(row))
                }
            }
        };
        let emit = |value: Option<&Value>| {
            trace.borrow_mut().push(Event::Log(value.cloned()));
            if row["logFail"] == true {
                Err(Rc::clone(&logging))
            } else {
                Ok(())
            }
        };
        let set_status = |value: Option<Value>| {
            trace.borrow_mut().push(Event::Status(value.clone()));
            if row["writeFail"] == "status" {
                return Err(Rc::clone(&writer));
            }
            *status.borrow_mut() = value;
            Ok(())
        };
        let actual = if row["family"] == "spot" {
            drive(spot_fetch_margin_account_infos(get, emit, set_status))
        } else {
            drive(fetch_margin_account_infos(
                get,
                emit,
                |value| {
                    trace.borrow_mut().push(Event::Margin(value));
                    if row["writeFail"] == "margin" {
                        return Err(Rc::clone(&writer));
                    }
                    margin.set(value);
                    Ok(())
                },
                set_status,
            ))
        };
        result(&actual, &row["result"], &remote, &logging, &writer);
        assert_eq!(
            margin.get(),
            if row["margin"] == "isolated" {
                MarginType::Isolated
            } else {
                MarginType::Cross
            },
            "READER native margin type"
        );
        optional(
            status.borrow().as_ref(),
            &row["status"],
            "unifiedMarginStatus",
        );
        events(&trace.borrow(), &row["events"]);
    }
    for row in balances {
        let input = &row["input"];
        let trace = RefCell::new(Vec::new());
        let disposed = Cell::new(input["disposed"] == true);
        let remote = error("RemoteError", "raw balance failed");
        let logging = error("Unused", "unused");
        let writer = error("FormatError", "format failed");
        let raw = Rc::new(serde_json::json!([{ "marker": "raw-37" }]));
        let formatted = Rc::new(serde_json::json!({"total":91,"marker":"formatted"}));
        let stored = Rc::new(serde_json::json!({"total":7,"marker":"stored-after-await"}));
        let current = RefCell::new(Rc::new(serde_json::json!({"total":3,"marker":"initial"})));
        let actual = drive(fetch_balance(
            || async {
                trace.borrow_mut().push(Event::Raw);
                pending_once().await;
                disposed.set(input["after"] == true);
                *current.borrow_mut() = Rc::clone(&stored);
                trace.borrow_mut().push(Event::Resolved);
                if input["rawFail"] == true {
                    Err(Rc::clone(&remote))
                } else {
                    Ok(Rc::clone(&raw))
                }
            },
            || {
                trace.borrow_mut().push(Event::Disposed(disposed.get()));
                disposed.get()
            },
            || {
                trace.borrow_mut().push(Event::Stored);
                Rc::clone(&current.borrow())
            },
            |received| {
                assert!(
                    Rc::ptr_eq(&received, &raw),
                    "READER native raw balance identity"
                );
                trace.borrow_mut().push(Event::Format(received));
                if input["formatFail"] == true {
                    Err(Rc::clone(&writer))
                } else {
                    Ok(Rc::clone(&formatted))
                }
            },
        ));
        result(&actual, &row["result"], &remote, &logging, &writer);
        if let Ok(value) = actual {
            assert_eq!(
                Rc::ptr_eq(&value, &stored),
                row["result"]["sameStored"] == true,
                "READER native stored balance identity"
            );
            assert_eq!(
                Rc::ptr_eq(&value, &formatted),
                row["result"]["sameFormatted"] == true,
                "READER native formatted balance identity"
            );
            native_value(&value, &row["result"]["value"], "balance");
        }
        events(&trace.borrow(), &row["events"]);
    }
    for row in hyro {
        let input = &row["input"];
        let trace = RefCell::new(Vec::new());
        let disposed = Cell::new(input["disposed"] == true);
        let remote = error("RemoteError", "balance failed");
        let logging = error("Unused", "unused");
        let writer = error("UpdateError", "update failed");
        let balance = Rc::new(serde_json::json!({"total":91,"marker":"hyro-balance"}));
        let updated = RefCell::new(None::<Rc<Value>>);
        let actual = drive(hyro_fetch_margin_account_infos(
            || async {
                trace.borrow_mut().push(Event::Balance);
                pending_once().await;
                disposed.set(input["after"] == true);
                trace.borrow_mut().push(Event::Resolved);
                if input["fetchFail"] == true {
                    Err(Rc::clone(&remote))
                } else {
                    Ok(Rc::clone(&balance))
                }
            },
            || {
                trace.borrow_mut().push(Event::Disposed(disposed.get()));
                disposed.get()
            },
            |received| {
                assert!(
                    Rc::ptr_eq(&received, &balance),
                    "READER native update balance identity"
                );
                trace.borrow_mut().push(Event::Update(Rc::clone(&received)));
                if input["updateFail"] == true {
                    return Err(Rc::clone(&writer));
                }
                *updated.borrow_mut() = Some(received);
                Ok(())
            },
        ));
        result(&actual, &row["result"], &remote, &logging, &writer);
        optional(
            updated.borrow().as_deref(),
            &row["updated"],
            "updated balance",
        );
        assert_eq!(
            updated
                .borrow()
                .as_ref()
                .is_some_and(|value| Rc::ptr_eq(value, &balance)),
            row["sameBalance"] == true,
            "READER native updated balance identity"
        );
        events(&trace.borrow(), &row["events"]);
    }
}
