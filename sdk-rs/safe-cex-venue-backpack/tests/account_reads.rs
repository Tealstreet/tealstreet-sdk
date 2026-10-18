use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::JsJsonValue;
use safe_cex_venue_backpack::account_reads::{
    change_margin_type, change_position_mode, fetch_account_info, get_account, get_position_mode,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    process::Command,
    task::{Context, Poll, Waker},
};
fn from_observation(value: &Value) -> JsJsonValue {
    match value[0].as_str().unwrap() {
        "undefined" => JsJsonValue::Undefined,
        "null" => JsJsonValue::Null,
        "boolean" => JsJsonValue::Bool(value[1].as_bool().unwrap()),
        "number" => JsJsonValue::Number(f64::from_bits(
            u64::from_str_radix(value[1].as_str().unwrap(), 16).unwrap(),
        )),
        "bigint" => JsJsonValue::BigInt(value[1].as_str().unwrap().parse().unwrap()),
        "string" => JsJsonValue::String(value[1].as_str().unwrap().into()),
        "array" => JsJsonValue::Array(
            value[1]
                .as_array()
                .unwrap()
                .iter()
                .map(from_observation)
                .collect(),
        ),
        "object" => JsJsonValue::Object(
            value[1]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| {
                    (
                        entry[0].as_str().unwrap().into(),
                        from_observation(&entry[1]),
                    )
                })
                .collect(),
        ),
        kind => panic!("unknown captured type {kind}"),
    }
}

fn bits(value: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(value.as_str().unwrap(), 16).unwrap())
}
fn ready<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("CPU signing future must complete when polled"),
    }
}

fn observed_bits(value: f64) -> String {
    format!(
        "{:016x}",
        if value.is_nan() {
            0x7ff8000000000000
        } else {
            value.to_bits()
        }
    )
}

fn oracle() -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/account_read_oracle.cjs"
        ))
        .output()
        .expect("actual TypeScript account methods");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

// Probe the native graph directly, including BigInt and IEEE bits; no serializer.
fn assert_native(actual: &JsJsonValue, expected: &Value, label: &str) {
    match (actual, expected[0].as_str().unwrap()) {
        (JsJsonValue::Undefined, "undefined") | (JsJsonValue::Null, "null") => (),
        (JsJsonValue::Bool(value), "boolean") => {
            assert_eq!(*value, expected[1].as_bool().unwrap(), "{label}")
        }
        (JsJsonValue::Number(value), "number") => {
            assert_eq!(observed_bits(*value), expected[1], "{label}")
        }
        (JsJsonValue::String(value), "string") => assert_eq!(value, &expected[1], "{label}"),
        (JsJsonValue::BigInt(value), "bigint") => {
            assert_eq!(value.to_string(), expected[1], "{label}")
        }
        (JsJsonValue::Array(values), "array") => {
            assert_eq!(
                values.len(),
                expected[1].as_array().unwrap().len(),
                "{label}"
            );
            for (i, value) in values.iter().enumerate() {
                assert_native(value, &expected[1][i], label);
            }
        }
        (JsJsonValue::Object(fields), "object") => {
            assert_eq!(
                fields.len(),
                expected[1].as_array().unwrap().len(),
                "{label}"
            );
            for (i, (key, value)) in fields.iter().enumerate() {
                assert_eq!(key, &expected[1][i][0], "{label}");
                assert_native(value, &expected[1][i][1], label);
            }
        }
        _ => panic!("{label}: native variant {actual:?} differs from {expected}"),
    }
}

#[test]
fn actual_typescript_account_identity() {
    let corpus = oracle();
    let cases = corpus["accounts"].as_array().unwrap();
    assert_eq!(cases.len(), 52, "ACCOUNT-IDENTITY source cases");
    let mut completed = [0; 3];
    for (i, case) in cases.iter().enumerate() {
        let calls = Cell::new(0);
        let logs = RefCell::new(Vec::new());
        let response = if case["readError"].is_null() {
            Ok(from_observation(&case["input"]))
        } else {
            let mut error = ExchangeError::new(case["readError"]["message"].as_str().unwrap());
            error.name = case["readError"]["name"].as_str().unwrap().into();
            error.store_subscriber_ids = vec![3];
            Err(error)
        };
        let pending = get_account(
            |endpoint| {
                assert_eq!(endpoint, "/api/v1/account", "ACCOUNT-IDENTITY endpoint {i}");
                calls.set(calls.get() + 1);
                std::future::ready(response)
            },
            |message, severity| {
                logs.borrow_mut().push(json!([message, severity]));
                if case["logThrows"] == true {
                    let mut error = ExchangeError::new("logger failed");
                    error.name = "SubscriberError".into();
                    error.store_subscriber_ids = vec![5, 9];
                    Err(error)
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(calls.get(), 1, "ACCOUNT-IDENTITY native eager call {i}");
        assert!(
            logs.borrow().is_empty(),
            "ACCOUNT-IDENTITY native deferred logging {i}"
        );
        let result = ready(pending);
        assert_eq!(
            case["callsAtConstruction"], 1,
            "ACCOUNT-IDENTITY source eager call {i}"
        );
        assert_eq!(
            case["logsAtConstruction"], 0,
            "ACCOUNT-IDENTITY source deferred logging {i}"
        );
        assert_eq!(
            case["calls"],
            json!([["/api/v1/account"]]),
            "ACCOUNT-IDENTITY source request {i}"
        );
        let expected_logs = usize::from([30, 31, 32, 33].contains(&i) || i >= 44);
        assert_eq!(
            logs.borrow().len(),
            expected_logs,
            "ACCOUNT-IDENTITY independent log count {i}"
        );
        assert_eq!(
            case["logs"].as_array().unwrap().len(),
            expected_logs,
            "ACCOUNT-IDENTITY source log count {i}"
        );
        assert_eq!(
            logs.borrow().as_slice(),
            case["logs"].as_array().unwrap(),
            "ACCOUNT-IDENTITY native logs {i}"
        );
        if case["outcome"]["ok"] == true {
            let identity = result.expect("ACCOUNT-IDENTITY native success");
            assert_native(
                &identity.user_id,
                &case["outcome"]["userId"],
                &format!("ACCOUNT-IDENTITY native user id {i}"),
            );
            assert!(
                matches!(identity.affiliate_id, JsJsonValue::Null),
                "ACCOUNT-IDENTITY native affiliate {i}"
            );
            assert_eq!(case["outcome"]["affiliateId"], json!(["null"]));
            completed[0] += 1;
        } else {
            let error = result.expect_err("ACCOUNT-IDENTITY native rejection");
            assert_eq!(
                error.name, case["outcome"]["name"],
                "ACCOUNT-IDENTITY native error name {i}"
            );
            assert_eq!(
                error.message, case["outcome"]["message"],
                "ACCOUNT-IDENTITY native error message {i}"
            );
            if case["logThrows"] == true {
                assert_eq!(case["outcome"]["sameLoggerError"], true);
                assert_eq!(
                    error.store_subscriber_ids,
                    [5, 9],
                    "ACCOUNT-IDENTITY logger metadata {i}"
                );
                completed[1] += 1;
            } else {
                if i >= 44 {
                    assert_eq!(case["outcome"]["sameReadError"], true);
                    assert_eq!(error.store_subscriber_ids, [3]);
                }
                completed[2] += 1;
            }
        }
    }
    assert_eq!(
        completed,
        [40, 6, 6],
        "ACCOUNT-IDENTITY completed native comparisons"
    );
}

#[test]
fn actual_typescript_position_mode() {
    assert_eq!(oracle()["positionMode"], "hedged", "POSITION-MODE source");
    assert_eq!(
        ready(get_position_mode()).unwrap(),
        safe_cex_types::PositionMode::Hedged,
        "POSITION-MODE native enum"
    );
}

#[test]
fn actual_typescript_unsupported_mode_changes() {
    let corpus = oracle();
    let cases = corpus["modes"].as_array().unwrap();
    assert_eq!(cases.len(), 12, "UNSUPPORTED-MODE independent source cases");
    let mut completed = [0; 2];
    for (i, case) in cases.iter().enumerate() {
        let trace = RefCell::new(Vec::new());
        let mut failure =
            ExchangeError::new(format!("{} failed", case["throws"].as_str().unwrap()));
        failure.name = "SubscriberError".into();
        failure.store_subscriber_ids = vec![5, 9];
        let emit = |event: &str, error: ExchangeError| {
            trace
                .borrow_mut()
                .push(json!(["emit", event, error.name, error.message]));
            if case["throws"] == "emit" {
                Err(failure.clone())
            } else {
                Ok(())
            }
        };
        let log = |message: &str, severity: &str| {
            trace.borrow_mut().push(json!(["log", message, severity]));
            if case["throws"] == "log" {
                Err(failure.clone())
            } else {
                Ok(())
            }
        };
        let is_position = case["method"] == "changePositionMode";
        let pending = if is_position {
            change_position_mode(case["enabled"].as_bool().unwrap(), emit, log)
        } else {
            change_margin_type(
                case["enabled"].as_bool().unwrap(),
                Some("ignored λ"),
                emit,
                log,
            )
        };
        let count = if case["throws"] == "emit" { 1 } else { 2 };
        assert_eq!(
            trace.borrow().len(),
            count,
            "UNSUPPORTED-MODE native eager effects {i}"
        );
        assert_eq!(
            case["atConstruction"], count,
            "UNSUPPORTED-MODE source eager effects {i}"
        );
        assert_eq!(
            trace.borrow().as_slice(),
            case["trace"].as_array().unwrap(),
            "UNSUPPORTED-MODE native event/log trace {i}"
        );
        let result = ready(pending);
        if case["throws"] == "none" {
            result.unwrap();
            assert_eq!(case["outcome"]["ok"], true);
        } else {
            assert_eq!(
                result.unwrap_err(),
                failure,
                "UNSUPPORTED-MODE native original failure {i}"
            );
            assert_eq!(case["outcome"]["sameFailure"], true);
            assert_eq!(case["outcome"]["message"], failure.message);
        }
        completed[usize::from(!is_position)] += 1;
    }
    assert_eq!(
        completed,
        [6, 6],
        "UNSUPPORTED-MODE completed native comparisons"
    );
}
#[test]
fn actual_typescript_account_read_state_and_logging() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/account_read_oracle.cjs"
        ))
        .output()
        .expect("one actual TypeScript account read corpus");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    for key in ["node", "arch", "platform"] {
        assert!(!corpus["runtime"][key].as_str().unwrap().is_empty());
    }
    eprintln!("account read runtime: {}", corpus["runtime"]);
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 98, "ACCOUNT-READ source cases");
    assert_eq!(
        cases.iter().filter(|c| !c["readError"].is_null()).count(),
        32,
        "ACCOUNT-READ source rejected reads"
    );
    let mut compared = [0; 4];
    for (i, case) in cases.iter().enumerate() {
        let state = Cell::new(bits(&case["initial"]));
        let calls = RefCell::new(Vec::new());
        let logs = RefCell::new(Vec::new());
        let response = if case["readError"].is_null() {
            Ok(from_observation(&case["input"]))
        } else {
            let mut error = ExchangeError::new(case["readError"]["message"].as_str().unwrap());
            error.name = case["readError"]["name"].as_str().unwrap().into();
            Err(error)
        };
        let pending = fetch_account_info(
            &state,
            |endpoint| {
                calls.borrow_mut().push(endpoint.to_owned());
                std::future::ready(response)
            },
            |message, severity| {
                logs.borrow_mut()
                    .push((message.to_owned(), severity.to_owned()));
                if case["logThrows"] == true {
                    let mut error = ExchangeError::new("logger failed");
                    error.name = "SubscriberError".into();
                    error.store_subscriber_ids = vec![5, 9];
                    Err(error)
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(
            case["callsAtConstruction"], 1,
            "ACCOUNT-READ source eager call case {i}"
        );
        assert_eq!(
            calls.borrow().len(),
            1,
            "ACCOUNT-READ native eager call case {i}"
        );
        assert_eq!(
            observed_bits(state.get()),
            case["before"],
            "ACCOUNT-READ native pre-await state case {i}"
        );
        let result = ready(pending);
        assert_eq!(
            calls.borrow().as_slice(),
            ["/api/v1/account"],
            "ACCOUNT-READ native request case {i}"
        );
        assert_eq!(
            case["calls"],
            json!([["/api/v1/account"]]),
            "ACCOUNT-READ actual request case {i}"
        );
        assert_eq!(
            observed_bits(state.get()),
            case["after"],
            "ACCOUNT-READ native leverage bits case {i}"
        );
        let log_count = usize::from([46, 47, 50, 51, 52, 53].contains(&i) || i >= 66);
        assert_eq!(
            case["logs"].as_array().unwrap().len(),
            log_count,
            "ACCOUNT-READ source log count case {i}"
        );
        assert_eq!(
            logs.borrow().len(),
            log_count,
            "ACCOUNT-READ independent native log count case {i}"
        );
        for (j, (message, severity)) in logs.borrow().iter().enumerate() {
            assert_eq!(
                message, &case["logs"][j][0],
                "ACCOUNT-READ native log message case {i}"
            );
            assert_eq!(
                severity, &case["logs"][j][1],
                "ACCOUNT-READ native log severity case {i}"
            );
            compared[1] += 1;
        }
        if case["outcome"]["ok"] == true {
            result.expect("source returned undefined");
            compared[2] += 1;
        } else {
            assert_eq!(
                case["outcome"]["sameLoggerError"], true,
                "source original logging error case {i}"
            );
            let error = result.expect_err("ACCOUNT-READ logging failure propagates");
            assert_eq!(
                error.name, case["outcome"]["name"],
                "ACCOUNT-READ native error name case {i}"
            );
            assert_eq!(
                error.message, case["outcome"]["message"],
                "ACCOUNT-READ native error message case {i}"
            );
            assert_eq!(
                error.store_subscriber_ids,
                [5, 9],
                "ACCOUNT-READ subscriber identity case {i}"
            );
            compared[3] += 1;
        }
        compared[0] += 1;
    }
    assert_eq!(
        compared,
        [98, 38, 79, 19],
        "ACCOUNT-READ completed comparisons"
    );
    let state = Cell::new(17.0);
    let released = Cell::new(false);
    let call_count = Cell::new(0);
    let mut pending = fetch_account_info(
        &state,
        |endpoint| {
            assert_eq!(
                endpoint, "/api/v1/account",
                "ACCOUNT-READ deferred endpoint"
            );
            call_count.set(call_count.get() + 1);
            std::future::poll_fn(|_| {
                if released.get() {
                    Poll::Ready(Ok(JsJsonValue::Object(vec![(
                        "leverageLimit".into(),
                        JsJsonValue::String("2.5".into()),
                    )])))
                } else {
                    Poll::Pending
                }
            })
        },
        |_, _| panic!("unexpected native deferred log"),
    );
    assert_eq!(
        corpus["deferred"]["callsAtConstruction"], 1,
        "actual deferred eager call"
    );
    assert_eq!(call_count.get(), 1, "ACCOUNT-READ deferred eager call");
    assert_eq!(
        observed_bits(state.get()),
        corpus["deferred"]["before"],
        "ACCOUNT-READ deferred initial state"
    );
    state.set(41.0);
    let mut context = Context::from_waker(Waker::noop());
    assert!(
        matches!(pending.as_mut().poll(&mut context), Poll::Pending),
        "ACCOUNT-READ suspension"
    );
    assert_eq!(
        observed_bits(state.get()),
        corpus["deferred"]["whilePending"],
        "ACCOUNT-READ state while suspended"
    );
    released.set(true);
    assert!(
        matches!(pending.as_mut().poll(&mut context), Poll::Ready(Ok(()))),
        "ACCOUNT-READ resolved completion"
    );
    assert_eq!(
        observed_bits(state.get()),
        corpus["deferred"]["after"],
        "ACCOUNT-READ deferred state commit"
    );
    assert_eq!(
        call_count.get(),
        1,
        "ACCOUNT-READ no duplicate deferred request"
    );
    assert_eq!(
        corpus["deferred"]["calls"],
        json!([["/api/v1/account"]]),
        "actual deferred request count"
    );
    let strings = corpus["strings"].as_array().unwrap();
    assert_eq!(strings.len(), 2, "ACCOUNT-READ source string boundaries");
    for case in strings {
        assert_eq!(
            case["requests"],
            json!([["/api/v1/account"]]),
            "actual string boundary request"
        );
    }
    let input =
        safe_cex_num::numeric_ingress::js_from_str::<String>(strings[0]["raw"].as_str().unwrap())
            .unwrap();
    let positive = Cell::new(17.0);
    let count = Cell::new(0);
    ready(fetch_account_info(
        &positive,
        |endpoint| {
            assert_eq!(endpoint, "/api/v1/account", "ACCOUNT-READ Unicode endpoint");
            count.set(count.get() + 1);
            std::future::ready(Ok(JsJsonValue::Object(vec![(
                "leverageLimit".into(),
                JsJsonValue::String(input),
            )])))
        },
        |_, _| panic!("unexpected native Unicode log"),
    ))
    .unwrap();
    assert_eq!(count.get(), 1, "ACCOUNT-READ Unicode request count");
    assert_eq!(
        observed_bits(positive.get()),
        strings[0]["after"],
        "ACCOUNT-READ Unicode native bits"
    );
    typed_utf16::assert_projection(
        strings[1]["raw"].as_str().unwrap(),
        &(safe_cex_num::numeric_ingress::js_from_str::<String>(
            strings[1]["raw"].as_str().unwrap(),
        ))
        .expect("typed surrogate record is retained"),
    );
    assert_eq!(
        strings[1]["after"], "4000000000000000",
        "actual surrogate parseFloat result"
    );
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;
