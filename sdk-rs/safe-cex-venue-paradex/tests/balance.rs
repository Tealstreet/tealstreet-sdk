use safe_cex_num::{JsCoercionError, numeric_ingress::js_deserialize_passthrough};
use safe_cex_venue_paradex::{
    balance::{BalanceEffects, fetch_balance},
    request_helpers::describe_error,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};

struct Pending<T> {
    ready: Rc<Cell<bool>>,
    value: Option<T>,
}
impl<T: Unpin> Future for Pending<T> {
    type Output = T;
    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<T> {
        if self.ready.get() {
            Poll::Ready(self.value.take().unwrap())
        } else {
            Poll::Pending
        }
    }
}
#[derive(Debug)]
struct Error {
    tag: &'static str,
    name: Option<&'static str>,
    message: String,
}
type ErrorRef = Rc<Error>;
fn tagged(tag: &'static str, message: &str) -> ErrorRef {
    Rc::new(Error {
        tag,
        name: None,
        message: message.into(),
    })
}
fn bits(value: f64) -> String {
    format!(
        "{:016x}",
        if value.is_nan() {
            0x7ff8000000000000
        } else {
            value.to_bits()
        }
    )
}
fn decode(raw: &str) -> Value {
    js_deserialize_passthrough(&mut serde_json::Deserializer::from_str(raw)).unwrap()
}

#[test]
fn actual_typescript_balance() {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/balance_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "balance_source_execution {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let corpus: Value = serde_json::from_slice(&out.stdout).unwrap();
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => other,
    };
    assert_eq!(corpus["header"]["arch"], arch, "capture_architecture");
    assert!(
        corpus["header"]["nodeMajor"].as_u64().unwrap() > 0,
        "capture_node_major"
    );
    println!("capture_header {}", corpus["header"]);
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 514, "balance_source_census");
    let mut cx = Context::from_waker(Waker::noop());
    let mut compared = 0;
    for c in cases {
        let trace = RefCell::new(Vec::<Value>::new());
        let logs = Cell::new(0);
        let string_calls = Cell::new(0);
        let init_ready = Rc::new(Cell::new(!c["init"].as_bool().unwrap()));
        let get_ready = Rc::new(Cell::new(false));
        let original = tagged("original", "network");
        let logger = tagged("logger", "logger");
        let replacement = tagged("replacement", "replacement");
        let init = c["init"].as_bool().unwrap().then(|| Pending {
            ready: init_ready.clone(),
            value: Some(if c["initError"] == true {
                Err(original.clone())
            } else {
                Ok(())
            }),
        });
        let get_result = if c["getError"] == "async" {
            Err(original.clone())
        } else {
            Ok(c["raw"].as_str().map(decode))
        };
        let stringify = |e: &ErrorRef| -> Result<String, ErrorRef> {
            if e.tag == "native" {
                return Ok(format!("{}: {}", e.name.unwrap(), e.message));
            }
            trace.borrow_mut().push(json!(["stringify", e.tag]));
            if e.tag == "original" {
                string_calls.set(string_calls.get() + 1);
                if c["stringifyError"] == true
                    || (c["describeError"] == true && string_calls.get() == 2)
                {
                    return Err(replacement.clone());
                }
            }
            Ok(format!("{}:{}", e.tag, e.message))
        };
        let mut effects = BalanceEffects {
            log: |message: String, severity: &'static str| -> Result<(), ErrorRef> {
                trace.borrow_mut().push(json!(["log", message, severity]));
                let index = logs.get();
                logs.set(index + 1);
                if c["throwLog"].as_u64() == Some(index) {
                    Err(logger.clone())
                } else {
                    Ok(())
                }
            },
            stringify_error: |e: &ErrorRef| stringify(e),
            describe_error: |e: &ErrorRef| {
                trace.borrow_mut().push(json!(["describe", e.tag]));
                describe_error(
                    || {
                        if e.tag == "original" && c["describeError"] == true {
                            return Err(replacement.clone());
                        }
                        if e.tag == "original" && c["describeUndefined"] == true {
                            return Ok(None);
                        }
                        Ok(Some(if e.tag == "native" {
                            "{}".into()
                        } else {
                            format!("{{\"tag\":\"{}\",\"message\":\"{}\"}}", e.tag, e.message)
                        }))
                    },
                    || stringify(e),
                )
            },
            missing_data_error: || {
                Rc::new(Error {
                    tag: "native",
                    name: Some("Error"),
                    message: "Balance API returned no data".into(),
                })
            },
            coercion_error: |e: JsCoercionError| {
                Rc::new(Error {
                    tag: "native",
                    name: Some(e.name),
                    message: e.message.into(),
                })
            },
        };
        let mut future = Box::pin(fetch_balance(
            init,
            || {
                trace.borrow_mut().push(json!(["jwt"]));
                c["jwt"] == true && init_ready.get()
            },
            |url| {
                trace.borrow_mut().push(json!(["get", url]));
                if c["getError"] == "sync" {
                    Err(original.clone())
                } else {
                    Ok(Pending {
                        ready: get_ready.clone(),
                        value: Some(get_result),
                    })
                }
            },
            &mut effects,
        ));
        let mut outcome = match future.as_mut().poll(&mut cx) {
            Poll::Ready(v) => Some(v),
            Poll::Pending => None,
        };
        assert_eq!(
            json!(*trace.borrow()),
            c["before"],
            "balance_before_initialization {}",
            c["label"]
        );
        init_ready.set(true);
        if outcome.is_none()
            && let Poll::Ready(v) = future.as_mut().poll(&mut cx)
        {
            outcome = Some(v);
        }
        assert_eq!(
            json!(*trace.borrow()),
            c["afterInit"],
            "balance_after_initialization {}",
            c["label"]
        );
        get_ready.set(true);
        if outcome.is_none()
            && let Poll::Ready(v) = future.as_mut().poll(&mut cx)
        {
            outcome = Some(v);
        }
        match outcome.expect("balance_must_settle") {
            Ok(b) => {
                assert_eq!(
                    c["outcome"]["ok"], true,
                    "balance_expected_success {}",
                    c["label"]
                );
                let mut native_fields = 0;
                for (name, value) in [
                    ("total", b.total),
                    ("free", b.free),
                    ("used", b.used),
                    ("upnl", b.upnl),
                ] {
                    assert_eq!(
                        bits(value),
                        c["outcome"]["balance"][name],
                        "balance_native_{name} {}",
                        c["label"]
                    );
                    native_fields += 1;
                }
                assert_eq!(native_fields, 4, "balance_completed_native_fields");
                assert!(
                    b.info.is_none() && b.currencies.is_none() && b.coins.is_none(),
                    "balance_absent_extras"
                );
                assert_eq!(
                    c["outcome"]["keys"],
                    json!(["total", "free", "used", "upnl"]),
                    "balance_source_keys"
                );
            }
            Err(e) => {
                assert_eq!(
                    c["outcome"]["ok"], false,
                    "balance_expected_error {}",
                    c["label"]
                );
                if e.tag != "native" {
                    let expected = match c["outcome"]["tag"].as_str().unwrap() {
                        "original" => &original,
                        "logger" => &logger,
                        "replacement" => &replacement,
                        _ => panic!("expected native error"),
                    };
                    assert!(
                        Rc::ptr_eq(&e, expected),
                        "balance_error_identity {}",
                        c["label"]
                    );
                }
                assert_eq!(
                    json!({"tag":e.tag,"name":e.name,"message":e.message}),
                    json!({"tag":c["outcome"]["tag"],"name":c["outcome"]["name"],"message":c["outcome"]["message"]}),
                    "balance_error_metadata {}",
                    c["label"]
                );
            }
        }
        assert_eq!(
            json!(*trace.borrow()),
            c["trace"],
            "balance_effect_trace {}",
            c["label"]
        );
        compared += 1;
    }
    assert_eq!(compared, 514, "balance_completed_comparisons");
    assert_eq!(
        corpus["surrogateLogCodeUnit"], 0xd800,
        "balance_source_surrogate_log"
    );
    assert!(
        js_deserialize_passthrough(&mut serde_json::Deserializer::from_str(
            corpus["surrogateRaw"].as_str().unwrap()
        ))
        .is_err(),
        "balance_utf8_refusal"
    );
    assert_eq!(
        corpus["getterTrace"],
        json!(["total", "account"]),
        "balance_source_eager_getters"
    );
    assert_eq!(
        corpus["getterMessage"], "getter",
        "balance_source_getter_throw"
    );
}
