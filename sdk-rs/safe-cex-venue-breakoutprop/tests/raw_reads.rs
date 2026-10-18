use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{
    js_number_output,
    numeric_ingress::{JsNumberInput, JsNumberOutput, js_deserialize_passthrough, js_value_as_f64},
};
use safe_cex_transport::provider::RestRequest;
use safe_cex_venue_breakoutprop::raw_reads::*;
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::{Future, ready},
    process::Command,
    sync::LazyLock,
};
const SOURCE_CENSUS: &str = include_str!("support/raw_reads-census.json");
include!("support/cardinality.rs");
static ORACLE: LazyLock<Value> = LazyLock::new(|| {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/rest_reads_oracle.cjs"
        ))
        .env("TZ", "UTC")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let corpus: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_source_census(&corpus, SOURCE_CENSUS);
    corpus
});
#[derive(Serialize)]
#[serde(transparent)]
struct Raw {
    value: Value,
}
impl JsNumberOutput for Raw {
    fn check_number_output(&self) {
        js_number_output!(self,struct {} passthrough {value});
    }
}
fn observe(v: &Value) -> Value {
    match v {
        Value::Number(_) => {
            let n = js_value_as_f64(v).unwrap();
            json!({"number":format!("{:016x}",if n.is_nan(){0x7ff8000000000000}else{n.to_bits()})})
        }
        Value::Array(a) => Value::Array(a.iter().map(observe).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), observe(v))).collect()),
        v => v.clone(),
    }
}
fn native(input: JsNumberInput) -> Value {
    observe(
        &input
            .decode_with(|d| js_deserialize_passthrough(d).map(|value| Raw { value }))
            .unwrap()
            .value,
    )
}
fn run<F: Future>(future: F) -> F::Output {
    let mut f = std::pin::pin!(future);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    match f.as_mut().poll(&mut cx) {
        std::task::Poll::Ready(v) => v,
        std::task::Poll::Pending => panic!("ready fixture pending"),
    }
}
#[test]
fn actual_raw_reads() {
    let mut comparisons = 0;
    eprintln!("raw read runtime {}", ORACLE["runtime"]);
    assert_eq!(ORACLE["cases"].as_array().unwrap().len(), 45);
    for case in ORACLE["cases"].as_array().unwrap() {
        let method = case["method"].as_str().unwrap();
        let scenario = case["scenario"].as_str().unwrap();
        let trace = RefCell::new(Vec::<Value>::new());
        let errors = RefCell::new(Vec::<Value>::new());
        let mut error = ExchangeError::new("fallback-message");
        error.response = Some(
            json!({"data":{"msg":match scenario{"empty-msg"=>json!(""),"numeric-msg"=>json!(19),"object-msg"=>json!({"detail":"object"}),_=>json!("venue-message")}}}),
        );
        let mut logger_error = ExchangeError::new("logger-failure");
        logger_error.store_subscriber_ids = vec![317];
        let session = || {
            trace.borrow_mut().push(json!("session"));
            ready(if scenario == "session" {
                Err(error.clone())
            } else {
                Ok("token".into())
            })
        };
        let account = || {
            trace.borrow_mut().push(json!("account"));
            ready(if scenario == "account" {
                Err(error.clone())
            } else {
                Ok("account-A".into())
            })
        };
        let request = |r: RestRequest| {
            trace
                .borrow_mut()
                .push(json!({"method":r.method,"url":r.url,"config":{"params":r.params}}));
            trace.borrow_mut().push(json!("http"));
            ready(
                if [
                    "http",
                    "empty-msg",
                    "numeric-msg",
                    "object-msg",
                    "logger-failure",
                ]
                .contains(&scenario)
                {
                    Err(error.clone())
                } else {
                    Ok(serde_json::from_str(case["raw"].as_str().unwrap()).unwrap())
                },
            )
        };
        let report = |value: Value| {
            errors.borrow_mut().push(observe(&value));
            if scenario == "logger-failure" {
                Err(logger_error.clone())
            } else {
                Ok(())
            }
        };
        let output = match method {
            "fetchInstrumentsRaw" => run(fetch_instruments_raw(session, account, request, &report)),
            "fetchInstrumentDetailsRaw" => run(fetch_instrument_details_raw(
                session, account, request, &report,
            )),
            "fetchPortfolioRaw" => run(fetch_portfolio_raw(session, account, request, &report)),
            _ => unreachable!(),
        };
        let mut actual = json!({"trace":trace.borrow().clone(),"errors":errors.borrow().clone()});
        let mut expected = json!({"trace":case["trace"],"errors":case["errors"]});
        match output {
            Ok(value) => {
                if method == "fetchPortfolioRaw" && scenario == "null-root" {
                    assert!(
                        value.is_some(),
                        "RAW-DISCRIMINANT successful null remains Some"
                    );
                }
                actual["ok"] = value.map(native).unwrap_or(Value::Null);
            }
            Err(e) => {
                assert_eq!(e, logger_error, "raw_logger_identity_{method}");
                actual["rejected"] = json!(e.message);
                actual["sameLoggerError"] = json!(true);
            }
        }
        if case.get("rejected").is_some() {
            expected["rejected"] = case["rejected"].clone();
            expected["sameLoggerError"] = case["sameLoggerError"].clone();
        } else {
            expected["ok"] = case["ok"].clone();
        }
        assert_native_cardinality(&actual, &expected, &format!("raw_read_{method}"));
        assert_eq!(actual, expected, "raw_read_{method}: {scenario}");
        comparisons += 1;
    }
    assert_eq!(comparisons, 45, "COMPARISON-COUNT raw_reads");
}

#[test]
fn raw_owned_and_utf8_boundaries() {
    assert_eq!(ORACLE["aliases"].as_array().unwrap().len(), 3);
    for case in ORACLE["aliases"].as_array().unwrap() {
        let method = case["method"].as_str().unwrap();
        let field = case["field"].as_str().unwrap();
        let input: JsNumberInput = serde_json::from_str(case["raw"].as_str().unwrap()).unwrap();
        let session = || ready(Ok("token".into()));
        let account = || ready(Ok("account-A".into()));
        let request = |_: RestRequest| ready(Ok(input.clone()));
        let output = match method {
            "fetchInstrumentsRaw" => {
                run(fetch_instruments_raw(
                    session,
                    account,
                    request,
                    &|_| Ok(()),
                ))
            }
            "fetchInstrumentDetailsRaw" => run(fetch_instrument_details_raw(
                session,
                account,
                request,
                &|_| Ok(()),
            )),
            "fetchPortfolioRaw" => run(fetch_portfolio_raw(session, account, request, &|_| Ok(()))),
            _ => unreachable!(),
        }
        .unwrap()
        .unwrap();
        let mut output = output
            .decode_with(|d| js_deserialize_passthrough(d).map(|value| Raw { value }))
            .unwrap();
        output.value[field][0]["marker"] = json!(777);
        assert_eq!(output.value[field][0]["marker"], 777);
        let original = input
            .decode_with(|d| js_deserialize_passthrough(d).map(|value| Raw { value }))
            .unwrap();
        assert_eq!(
            js_value_as_f64(&original.value[field][0]["marker"]),
            Some(19.0),
            "raw_owned_{method}"
        );
        assert_eq!(case["changed"], 777, "source_raw_alias_{method}");
    }
    assert_eq!(ORACLE["utf8"].as_array().unwrap().len(), 3);
    for case in ORACLE["utf8"].as_array().unwrap() {
        assert_eq!(case["codeUnit"], 55296);
        let input: JsNumberInput = serde_json::from_str(case["raw"].as_str().unwrap()).unwrap();
        let method = case["method"].as_str().unwrap();
        let session = || ready(Ok("token".into()));
        let account = || ready(Ok("account-A".into()));
        let request = |_| ready(Ok(input));
        let report = |_: Value| -> Result<(), ExchangeError> {
            panic!("raw UTF16 input must not report a failure")
        };
        let output = match method {
            "fetchInstrumentsRaw" => run(fetch_instruments_raw(session, account, request, &report)),
            "fetchInstrumentDetailsRaw" => run(fetch_instrument_details_raw(
                session, account, request, &report,
            )),
            "fetchPortfolioRaw" => run(fetch_portfolio_raw(session, account, request, &report)),
            _ => panic!("unknown raw read {method}"),
        }
        .unwrap()
        .unwrap();
        let field = match method {
            "fetchInstrumentsRaw" => "instruments",
            "fetchInstrumentDetailsRaw" => "instrumentDetails",
            _ => "portfolios",
        };
        let retained: safe_cex_types::retained_json::RetainedJsonInput = output.decode().unwrap();
        assert_eq!(
            retained.get(field).unwrap().as_array().unwrap()[0]
                .get("symbol")
                .unwrap()
                .as_text(),
            Some([0xd800].as_slice())
        );
    }
}

#[test]
fn raw_abandoned_promise_and_future_drop() {
    use std::{cell::Cell, rc::Rc, task::Poll};
    struct Release(Rc<Cell<usize>>);
    impl Drop for Release {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    assert_eq!(ORACLE["abandoned"].as_array().unwrap().len(), 3);
    for case in ORACLE["abandoned"].as_array().unwrap() {
        let method = case["method"].as_str().unwrap();
        assert_eq!(
            case["before"],
            json!(["session"]),
            "source_raw_prefix_{method}"
        );
        assert_eq!(
            case["after"],
            json!(["session", "account", "request"]),
            "source_raw_abandoned_{method}"
        );
        for mode in ["unpolled", "pending", "complete"] {
            let trace = RefCell::new(Vec::<&str>::new());
            let settled = Cell::new(false);
            let drops = Rc::new(Cell::new(0));
            let guard = Release(drops.clone());
            let session = || {
                trace.borrow_mut().push("session");
                let settled = &settled;
                async move {
                    let _guard = guard;
                    std::future::poll_fn(|_| {
                        if settled.get() {
                            Poll::Ready(Ok("token".into()))
                        } else {
                            Poll::Pending
                        }
                    })
                    .await
                }
            };
            let account = || {
                trace.borrow_mut().push("account");
                ready(Ok("account-A".into()))
            };
            let request = |_: RestRequest| {
                trace.borrow_mut().push("request");
                ready(Ok(
                    serde_json::from_str(case["raw"].as_str().unwrap()).unwrap()
                ))
            };
            type RawFuture<'a> = std::pin::Pin<
                Box<dyn Future<Output = Result<Option<JsNumberInput>, ExchangeError>> + 'a>,
            >;
            let mut future: RawFuture<'_> = match method {
                "fetchInstrumentsRaw" => Box::pin(fetch_instruments_raw(
                    session,
                    account,
                    request,
                    &|_| Ok(()),
                )),
                "fetchInstrumentDetailsRaw" => Box::pin(fetch_instrument_details_raw(
                    session,
                    account,
                    request,
                    &|_| Ok(()),
                )),
                "fetchPortfolioRaw" => {
                    Box::pin(fetch_portfolio_raw(session, account, request, &|_| Ok(())))
                }
                _ => unreachable!(),
            };
            assert_eq!(
                *trace.borrow(),
                vec!["session"],
                "raw_eager_session_{method}"
            );
            if mode != "unpolled" {
                let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
                assert!(future.as_mut().poll(&mut cx).is_pending());
            }
            if mode == "complete" {
                settled.set(true);
                assert!(run(future).unwrap().is_some());
                assert_eq!(
                    *trace.borrow(),
                    vec!["session", "account", "request"],
                    "raw_completed_{method}"
                );
            } else {
                drop(future);
                settled.set(true);
                assert_eq!(
                    *trace.borrow(),
                    vec!["session"],
                    "raw_drop_stops_{method}_{mode}"
                );
            }
            assert_eq!(drops.get(), 1, "raw_session_release_{method}_{mode}");
        }
    }
}
