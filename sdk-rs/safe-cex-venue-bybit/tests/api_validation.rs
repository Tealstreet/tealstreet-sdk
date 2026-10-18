use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_venue_bybit::api_validation::*;
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn drive<F: Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..5 {
        if let Poll::Ready(value) = future.as_mut().poll(&mut cx) {
            return value;
        }
    }
    panic!("API native future did not settle by poll 5");
}
async fn pending_once() {
    let mut pending = true;
    std::future::poll_fn(|cx| {
        if pending {
            pending = false;
            cx.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    })
    .await;
}
fn error(phase: &str) -> Rc<Value> {
    Rc::new(
        serde_json::json!({"name":format!("{phase}Error"),"message":format!("{phase} failed"),"code":17}),
    )
}
fn errors() -> BTreeMap<&'static str, Rc<Value>> {
    [
        "first", "second", "request", "setter", "log", "batch", "user", "fallback", "account",
    ]
    .into_iter()
    .map(|p| (p, error(p)))
    .collect()
}
fn number(actual: f64, expected: &Value, label: &str) {
    let bits = if actual.is_nan() {
        "nan".into()
    } else {
        format!("{:016x}", actual.to_bits())
    };
    assert_eq!(
        bits,
        expected["number"].as_str().unwrap(),
        "API native number {label}"
    );
}
fn native(actual: &Value, expected: &Value, label: &str) {
    match actual {
        Value::Number(_) => number(
            safe_cex_num::numeric_ingress::js_value_as_f64(actual).unwrap(),
            expected,
            label,
        ),
        Value::Array(values) => {
            let expected = expected.as_array().unwrap();
            assert_eq!(
                values.len(),
                expected.len(),
                "API native array count {label}"
            );
            for (v, e) in values.iter().zip(expected) {
                native(v, e, label);
            }
        }
        Value::Object(values) => {
            let expected = expected.as_object().unwrap();
            assert_eq!(
                values.len(),
                expected.len(),
                "API native field count {label}"
            );
            for (k, v) in values {
                native(v, expected.get(k).expect(k), label);
            }
        }
        _ => assert_eq!(actual, expected, "API native scalar {label}"),
    }
}
fn optional(actual: Option<&Value>, expected: &Value, label: &str) {
    if expected["undefined"] == true {
        assert!(actual.is_none(), "API native absent {label}");
    } else {
        assert!(actual.is_some(), "API native present {label}");
        native(actual.unwrap(), expected, label);
    }
}
fn outcome<T>(actual: &Result<T, Rc<Value>>, expected: &Value, errors: &BTreeMap<&str, Rc<Value>>) {
    assert_eq!(
        actual.is_ok(),
        expected.get("error").is_none(),
        "API native rejection"
    );
    if let Err(error) = actual {
        assert_eq!(
            error.get("name"),
            expected.get("error"),
            "API native error name"
        );
        assert_eq!(
            error.get("message"),
            expected.get("message"),
            "API native error message"
        );
        if let Some(origin) = expected["origin"].as_str() {
            assert!(
                Rc::ptr_eq(error, &errors[origin]),
                "API native original error identity"
            );
        }
    }
}
#[derive(Debug)]
enum Event {
    Get(String),
    Resolved,
    Balance,
    Log(Option<Value>),
    Setter(Option<Rc<Value>>),
    Fetch,
    Batch,
    User(String, Option<Rc<Value>>),
    Text(String),
    Account,
    NumberSetter(f64),
}
fn trace(actual: &[Event], expected: &Value) {
    let expected = expected.as_array().unwrap();
    assert_eq!(actual.len(), expected.len(), "API native callback count");
    for (a, e) in actual.iter().zip(expected) {
        let kind = match a {
            Event::Get(endpoint) => {
                assert_eq!(endpoint, e[1].as_str().unwrap(), "API native endpoint");
                "get"
            }
            Event::Resolved => "resolved",
            Event::Balance => "balance",
            Event::Log(value) => {
                optional(value.as_ref(), &e[1], "log argument");
                "log"
            }
            Event::Setter(value) => {
                optional(value.as_deref(), &e[1], "setter argument");
                "setter"
            }
            Event::Fetch => "fetch",
            Event::Batch => "batch",
            Event::User(key, value) => {
                let fields = e[1].as_object().unwrap();
                assert_eq!(fields.len(), 1, "API native user-data field count");
                assert!(fields.contains_key(key), "API native user-data key");
                optional(value.as_deref(), &fields[key], "user-data value");
                "user"
            }
            Event::Text(value) => {
                assert_eq!(value, e[1].as_str().unwrap(), "API native log text");
                "log"
            }
            Event::Account => "account",
            Event::NumberSetter(value) => {
                number(*value, &e[1], "setter value");
                "setter"
            }
        };
        assert_eq!(kind, e[0].as_str().unwrap(), "API native callback order");
    }
}
#[test]
#[ignore = "requires installed live TypeScript"]
fn api_validation_native_source_parity() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/api-validation-fixtures.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let fixtures: Value = serde_json::from_slice(&output.stdout).unwrap();
    for (group, count) in [
        ("validations", 60),
        ("hyro", 6),
        ("fetches", 28),
        ("caches", 32),
        ("vips", 70),
    ] {
        assert_eq!(
            fixtures[group].as_array().unwrap().len(),
            count,
            "API independent {group} floor"
        );
    }
    for row in fixtures["validations"].as_array().unwrap() {
        let events = RefCell::new(Vec::new());
        let errors = errors();
        let balance = Rc::new(serde_json::json!({"marker":"BALANCE-73"}));
        let calls = Cell::new(0);
        let logs = Cell::new(0);
        let rich = Rc::new(
            serde_json::json!({"name":"RequestError","message":"fallback message","response":{"data":{"message":"LOG-37","msg":"RESULT-91"}}}),
        );
        let get = |endpoint: &str| {
            events.borrow_mut().push(Event::Get(endpoint.into()));
            let events = &events;
            async move {
                pending_once().await;
                events.borrow_mut().push(Event::Resolved);
                if row["hasReject"] == true {
                    Err(Rc::new(
                        serde_json::from_str(row["rejectRaw"].as_str().unwrap()).unwrap(),
                    ))
                } else {
                    Ok(serde_json::from_str(row["dataRaw"].as_str().unwrap()).unwrap())
                }
            }
        };
        let fetch = || {
            calls.set(calls.get() + 1);
            events.borrow_mut().push(Event::Balance);
            if row["balanceSyncFail"] == true {
                return Err(Rc::clone(&rich));
            }
            Ok(async {
                pending_once().await;
                if row["balanceFail"] == true {
                    Err(Rc::clone(&rich))
                } else {
                    Ok(Rc::clone(&balance))
                }
            })
        };
        let emit = |value| {
            logs.set(logs.get() + 1);
            events.borrow_mut().push(Event::Log(value));
            if row["failLogs"]
                .as_array()
                .unwrap()
                .iter()
                .any(|n| n.as_u64() == Some(logs.get()))
            {
                Err(Rc::clone(
                    &errors[if logs.get() == 1 { "first" } else { "second" }],
                ))
            } else {
                Ok(())
            }
        };
        let actual = if row["family"] == "linear" {
            drive(validate_account(get, fetch, emit))
        } else {
            drive(validate_derived_account(get, fetch, emit))
        };
        outcome(&actual, &row["result"], &errors);
        if let Ok((error, value)) = actual {
            optional(
                error.as_ref(),
                &row["result"]["value"]["error"],
                "validation error",
            );
            optional(
                value.as_deref(),
                row["result"]["value"]
                    .get("balance")
                    .unwrap_or(&serde_json::json!({"undefined":true})),
                "validation balance",
            );
            assert_eq!(
                value.as_ref().is_some_and(|v| Rc::ptr_eq(v, &balance)),
                row["result"]["sameBalance"] == true,
                "API native validation balance identity"
            );
        }
        assert_eq!(
            calls.get(),
            row["balanceCalls"].as_u64().unwrap(),
            "API native balance call count"
        );
        trace(&events.borrow(), &row["trace"]);
    }
    for row in fixtures["hyro"].as_array().unwrap() {
        let input = &row["input"];
        let events = RefCell::new(Vec::new());
        let balance = Rc::new(serde_json::json!({"marker":"HYRO-29"}));
        let mut error = ExchangeError::new(input["message"].as_str().unwrap_or("hyro failed"));
        error.name = "BalanceError".into();
        error.code = Some(safe_cex_exchange::js_runtime::value::from_json(
            &serde_json::json!(17),
        ));
        let original = Rc::new(error);
        let mut e = ExchangeError::new("log failed");
        e.name = "LogError".into();
        let logging = Rc::new(e);
        let actual = drive(hyro_validate_account(
            || {
                events.borrow_mut().push(Event::Balance);
                if input["syncFail"] == true {
                    return Err(Rc::clone(&original));
                }
                Ok(async {
                    pending_once().await;
                    if input["fail"] == true {
                        Err(Rc::clone(&original))
                    } else {
                        Ok(Rc::clone(&balance))
                    }
                })
            },
            |message| {
                events.borrow_mut().push(Event::Text(message.into()));
                if input["logFail"] == true {
                    Err(Rc::clone(&logging))
                } else {
                    Ok(())
                }
            },
        ));
        assert_eq!(
            actual.is_ok(),
            row["result"].get("error").is_none(),
            "API native Hyro rejection"
        );
        match actual {
            Ok((error, value)) => {
                assert_eq!(
                    error,
                    row["result"]["value"]["error"].as_str().unwrap(),
                    "API native Hyro error"
                );
                optional(
                    value.as_deref(),
                    row["result"]["value"]
                        .get("balance")
                        .unwrap_or(&serde_json::json!({"undefined":true})),
                    "Hyro balance",
                );
                assert_eq!(
                    value.as_ref().is_some_and(|v| Rc::ptr_eq(v, &balance)),
                    row["result"]["sameBalance"] == true,
                    "API native Hyro balance identity"
                );
            }
            Err(error) => {
                assert!(
                    Rc::ptr_eq(&error, &logging),
                    "API native Hyro log error identity"
                );
                assert_eq!(
                    error.name,
                    row["result"]["error"].as_str().unwrap(),
                    "API native Hyro error name"
                );
                assert_eq!(
                    error.message,
                    row["result"]["message"].as_str().unwrap(),
                    "API native Hyro error message"
                );
            }
        }
        trace(&events.borrow(), &row["trace"]);
    }
    for row in fixtures["fetches"].as_array().unwrap() {
        let input = &row["input"];
        let phase = input["phase"].as_str().unwrap_or("");
        let events = RefCell::new(Vec::new());
        let errors = errors();
        let cached = RefCell::new(Some(Rc::new(serde_json::json!({"vipLevel":"OLD"}))));
        let received = RefCell::new(None);
        let sets = Cell::new(0);
        let actual = drive(fetch_api_key_info(
            |endpoint| {
                events.borrow_mut().push(Event::Get(endpoint.into()));
                let events = &events;
                let errors = &errors;
                async move {
                    pending_once().await;
                    events.borrow_mut().push(Event::Resolved);
                    if phase == "request" {
                        Err(Rc::clone(&errors["request"]))
                    } else {
                        Ok(Rc::new(input["data"].clone()))
                    }
                }
            },
            |value| {
                events.borrow_mut().push(Event::Log(value));
                if phase == "log" {
                    Err(Rc::clone(&errors["log"]))
                } else {
                    Ok(())
                }
            },
            |value| {
                sets.set(sets.get() + 1);
                events.borrow_mut().push(Event::Setter(value.clone()));
                *received.borrow_mut() = value.clone();
                if phase == "setter" {
                    return Err(Rc::clone(&errors["setter"]));
                }
                *cached.borrow_mut() = value;
                Ok(())
            },
        ));
        outcome(&actual, &row["result"], &errors);
        optional(
            cached.borrow().as_deref(),
            &row["cached"],
            "cached key info",
        );
        optional(
            received.borrow().as_deref(),
            &row["received"],
            "received key info",
        );
        assert_eq!(
            sets.get(),
            row["sets"].as_u64().unwrap(),
            "API native key setter count"
        );
        if let Ok(value) = actual {
            optional(value.as_deref(), &row["result"]["value"], "key result");
            for (other, key) in [(&cached, "sameCache"), (&received, "sameReceived")] {
                let same = match (value.as_ref(), other.borrow().as_ref()) {
                    (Some(a), Some(b)) => Rc::ptr_eq(a, b),
                    (None, None) => true,
                    _ => false,
                };
                assert_eq!(
                    same,
                    row["result"][key] == true,
                    "API native key identity {key}"
                );
            }
        }
        trace(&events.borrow(), &row["trace"]);
    }
    for row in fixtures["caches"].as_array().unwrap() {
        let input = &row["input"];
        let phase = input["phase"].as_str().unwrap_or("");
        let events = RefCell::new(Vec::new());
        let errors = errors();
        let cached = RefCell::new(Some(Rc::new(serde_json::json!({"vipLevel":"OLD"}))));
        let batch = Cell::new(7.);
        let user = RefCell::new(None::<(String, Option<Rc<Value>>)>);
        let updates = Cell::new(0);
        let fetch = || {
            events.borrow_mut().push(Event::Fetch);
            if phase == "requestSync" {
                return Err(Rc::clone(&errors["request"]));
            }
            Ok(async {
                pending_once().await;
                if ["request", "fallbackLog"].contains(&phase) {
                    return Err(Rc::clone(&errors["request"]));
                }
                *cached.borrow_mut() = if input["missingInfo"] == true {
                    None
                } else {
                    Some(Rc::new(
                        input
                            .get("info")
                            .cloned()
                            .unwrap_or_else(|| serde_json::json!({"vipLevel":"VIP-1"})),
                    ))
                };
                Ok(())
            })
        };
        let cache = || cached.borrow().clone();
        let update_batch = || {
            events.borrow_mut().push(Event::Batch);
            if phase == "batch" {
                return Err(Rc::clone(&errors["batch"]));
            }
            batch.set(20.);
            if let Some(info) = input.get("replaceOnBatch") {
                *cached.borrow_mut() = Some(Rc::new(info.clone()));
            }
            Ok(())
        };
        let update_user = |key: &str, info: Option<Rc<Value>>| {
            events
                .borrow_mut()
                .push(Event::User(key.into(), info.clone()));
            updates.set(updates.get() + 1);
            if phase == "user" {
                return Err(Rc::clone(&errors["user"]));
            }
            *user.borrow_mut() = Some((key.into(), info));
            if let Some(info) = input.get("replaceOnUser") {
                *cached.borrow_mut() = Some(Rc::new(info.clone()));
            }
            Ok(())
        };
        let limit = || batch.get();
        let log = |message: &str| {
            events.borrow_mut().push(Event::Text(message.into()));
            if message == "Failed to fetch API key info, continuing without it" {
                if phase == "fallbackLog" || input["fallbackFails"] == true {
                    return Err(Rc::clone(&errors["fallback"]));
                }
            } else if phase == "log" {
                return Err(Rc::clone(&errors["log"]));
            }
            Ok(())
        };
        let actual = if row["family"] == "inverse" {
            drive(fetch_and_cache_inverse_api_key_info(
                fetch,
                cache,
                update_batch,
                update_user,
                limit,
                log,
            ))
        } else {
            drive(fetch_and_cache_api_key_info(
                fetch,
                cache,
                update_batch,
                update_user,
                limit,
                log,
            ))
        };
        outcome(&actual, &row["result"], &errors);
        optional(cached.borrow().as_deref(), &row["cached"], "cached result");
        number(batch.get(), &row["batch"], "cached batch limit");
        assert_eq!(
            updates.get(),
            row["updates"].as_u64().unwrap(),
            "API native user-data update count"
        );
        if row["user"]["undefined"] == true {
            assert!(user.borrow().is_none(), "API native absent user data");
        } else {
            let user = user.borrow();
            let (key, value) = user.as_ref().expect("API native present user data");
            let expected = row["user"].as_object().unwrap();
            assert_eq!(expected.len(), 1, "API native user-data output field count");
            assert!(expected.contains_key(key), "API native user-data key");
            optional(value.as_deref(), &expected[key], "user data");
        }
        trace(&events.borrow(), &row["trace"]);
    }
    for row in fixtures["vips"].as_array().unwrap() {
        let input = &row["input"];
        let phase = input["phase"].as_str().unwrap_or("");
        let events = RefCell::new(Vec::new());
        let errors = errors();
        let level = RefCell::new(input["level"].as_str().map(str::to_owned));
        let batch = Cell::new(input["batch"].as_str().unwrap().parse::<f64>().unwrap());
        let reads = Cell::new(0);
        let sets = Cell::new(0);
        let actual = update_batch_limit_based_on_vip_level(
            || {
                reads.set(reads.get() + 1);
                Ok(level.borrow().clone())
            },
            || {
                events.borrow_mut().push(Event::Account);
                if phase == "account" {
                    return Err(Rc::clone(&errors["account"]));
                }
                if let Some(next) = input["replaceOnAccount"].as_str() {
                    *level.borrow_mut() = Some(next.into());
                }
                Ok(input["account"].as_str().unwrap().into())
            },
            || batch.get(),
            |value| {
                sets.set(sets.get() + 1);
                events.borrow_mut().push(Event::NumberSetter(value));
                if phase == "setter" {
                    return Err(Rc::clone(&errors["setter"]));
                }
                batch.set(value);
                if let Some(next) = input["replaceOnSetter"].as_str() {
                    *level.borrow_mut() = Some(next.into());
                }
                Ok(())
            },
            |message| {
                events.borrow_mut().push(Event::Text(message.into()));
                if phase == "log" {
                    Err(Rc::clone(&errors["log"]))
                } else {
                    Ok(())
                }
            },
        );
        outcome(&actual, &row["result"], &errors);
        number(batch.get(), &row["batch"], "VIP batch limit");
        assert_eq!(
            reads.get(),
            row["reads"].as_u64().unwrap(),
            "API native VIP reread count"
        );
        assert_eq!(
            sets.get(),
            row["sets"].as_u64().unwrap(),
            "API native VIP setter count"
        );
        optional(
            level
                .borrow()
                .as_ref()
                .map(|v| Value::String(v.clone()))
                .as_ref(),
            &row["level"],
            "VIP level",
        );
        trace(&events.borrow(), &row["trace"]);
    }
}
