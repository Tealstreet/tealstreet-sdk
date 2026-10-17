use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_store::store::DefaultStore;
use safe_cex_transport::provider::{
    RestAdapter, RestFailure, RestFuture, RestRequest, RestResponse,
};
use safe_cex_types::ExchangeName;
use safe_cex_venue_kucoin::accounts::{get_account, get_spot_account};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    io::Write,
    pin::Pin,
    process::{Command, Stdio},
    rc::Rc,
    task::{Context, Poll, Waker},
};
#[derive(Default)]
struct Runtime {
    errors: std::cell::RefCell<Vec<String>>,
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        1.0
    }
    fn now_ms(&self) -> f64 {
        1.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        panic!("unexpected sleep")
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("unexpected task")
    }
    fn console(&self, level: &str, message: &str) {
        if level == "error" {
            self.errors
                .borrow_mut()
                .push(message.strip_prefix("exchange error ").unwrap().to_owned());
        }
    }
    fn unique_id(&self) -> String {
        "fixture-id".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_raw_exception(&self, _: &serde_json::Value, _: &serde_json::Value) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, _: &ExchangeError, _: &Value) {}
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("unexpected worker")
    }
}

fn fixture_body(case: &Value) -> safe_cex_transport::RestResponseBody {
    use safe_cex_transport::RestResponseBody;
    if let Some(raw) = case.get("raw").and_then(Value::as_str) {
        RestResponseBody::Encoded(raw.as_bytes().to_vec())
    } else if case["phase"] == "untransformed" {
        RestResponseBody::Untransformed(
            safe_cex_types::retained_json::RetainedJsonInput::from_value(case["data"].clone())
                .unwrap(),
        )
    } else if case["phase"] == "encoded" {
        RestResponseBody::Encoded(serde_json::to_vec(&case["data"]).unwrap())
    } else {
        RestResponseBody::Transformed(
            safe_cex_types::retained_json::RetainedJsonInput::from_value(case["data"].clone())
                .unwrap(),
        )
    }
}
struct Adapter {
    case: Value,
    calls: RefCell<Vec<Value>>,
}
impl RestAdapter for Adapter {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.calls
            .borrow_mut()
            .push(json!({"method":request.method,"url":request.url}));
        assert!(request.params.is_none() && request.body.is_none());
        Box::pin(async move {
            if let Some(failure) = self.case.get("failure") {
                let response = failure.get("response").map(|r| {
                    Box::new(RestResponse {
                        status: 400,
                        reason: Some("Bad Request".into()),
                        headers: Default::default(),
                        body: fixture_body(r),
                    })
                });
                Err(RestFailure {
                    code: None,
                    message: failure["message"].as_str().unwrap().into(),
                    response,
                    request: Box::new(request),
                    context: None,
                    transport_error: None,
                })
            } else {
                Ok(RestResponse {
                    status: 200,
                    reason: None,
                    headers: Default::default(),
                    body: fixture_body(&self.case),
                })
            }
        })
    }
}
fn ready<T>(future: impl Future<Output = T>) -> T {
    let waker = Waker::noop();
    let mut cx = Context::from_waker(waker);
    match std::pin::pin!(future).as_mut().poll(&mut cx) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("fixture unexpectedly blocked"),
    }
}
#[test]
fn live_account_reads_and_response_errors() {
    let mut cases = vec![];
    for venue in ["kucoin", "kucoinspot"] {
        for data in [
            json!({"code":"200000","data":{"userId":"primary","uid":"fallback","subUserId":"sub","referrerUserId":"ref"}}),
            json!({"code":"200000","data":{"userId":"","uid":"fallback","subUserId":false}}),
            json!({"code":"200000","data":{}}),
            json!({"code":"200000"}),
            json!({"code":"200000","data":null}),
            json!({"code":200000,"msg":"numeric code"}),
            json!({"code":"400000","msg":""}),
            json!({"code":"400000","msg":"venue rejection"}),
            json!(null),
            json!(false),
        ] {
            cases.push(json!({"venue":venue,"data":data}));
        }
        for failure in [
            json!({"message":"network failure"}),
            json!({"message":"transport message","response":{"data":{"msg":"venue body"}}}),
            json!({"message":"transport message","response":{"data":{"msg":""}}}),
            json!({"message":"","response":{"data":null}}),
        ] {
            cases.push(json!({"venue":venue,"failure":failure}));
        }
    }
    let reference = oracle(&cases);
    assert_eq!(reference.len(), cases.len());
    for (case, expected) in cases.into_iter().zip(reference) {
        let runtime = Rc::new(Runtime::default());
        let spot = case["venue"] == "kucoinspot";
        let base = BaseExchange::new(
            if spot {
                ExchangeName::Kucoinspot
            } else {
                ExchangeName::Kucoin
            },
            serde_json::from_value(json!({"key":"fixture","secret":"fixture"})).unwrap(),
            Rc::new(DefaultStore::new()),
            runtime.clone(),
            ExchangeSettings::default(),
        );
        let adapter = Adapter {
            case,
            calls: RefCell::new(vec![]),
        };
        let account = if spot {
            ready(get_spot_account(&base, &adapter)).unwrap()
        } else {
            ready(get_account(&base, &adapter)).unwrap()
        };
        let actual = json!({"account":account,"calls":&*adapter.calls.borrow(),"errors":&*runtime.errors.borrow()});
        assert_eq!(actual, expected, "{}", adapter.case);
    }
}

#[test]
fn account_typed_boundary_rejects_truthy_nonstring_ids() {
    let mut inputs = Vec::new();
    for spot in [false, true] {
        for field in ["userId", "subUserId", "referrerUserId"] {
            for value in [json!(42), json!({}), json!([])] {
                let mut data = json!({"code":"200000","data":{}});
                data["data"][field] = value;
                inputs.push(json!({"venue":if spot{"kucoinspot"}else{"kucoin"},"data":data}));
            }
        }
    }
    let captured = oracle(&inputs);
    assert_eq!(captured.len(), inputs.len());
    let mut captured = captured.iter();
    for spot in [false, true] {
        let runtime = Rc::new(Runtime::default());
        let base = BaseExchange::new(
            ExchangeName::Kucoin,
            serde_json::from_value(json!({"key":"fixture","secret":"fixture"})).unwrap(),
            Rc::new(DefaultStore::new()),
            runtime.clone(),
            ExchangeSettings::default(),
        );
        for field in ["userId", "subUserId", "referrerUserId"] {
            for value in [json!(42), json!({}), json!([])] {
                let mut data = json!({"code":"200000","data":{}});
                data["data"][field] = value;
                let adapter = Adapter {
                    case: json!({"venue":if spot{"kucoinspot"}else{"kucoin"},"data":data}),
                    calls: RefCell::new(vec![]),
                };
                let reference = std::slice::from_ref(captured.next().unwrap());
                let output_field = match field {
                    "subUserId" => "subId",
                    "referrerUserId" => "affiliateId",
                    _ => "userId",
                };
                assert_eq!(
                    reference[0]["account"][output_field],
                    adapter.case["data"]["data"][field]
                );
                assert_eq!(reference[0]["errors"], json!([]));
                let account = if spot {
                    ready(get_spot_account(&base, &adapter)).unwrap()
                } else {
                    ready(get_account(&base, &adapter)).unwrap()
                };
                assert_eq!(
                    serde_json::to_value(account).unwrap(),
                    json!({"userId":"","subId":null,"affiliateId":null})
                );
                assert_eq!(
                    runtime.errors.borrow().last().unwrap(),
                    "KuCoin account strings must use the shared string representation"
                );
            }
        }
    }
}

struct PendingAdapter(std::cell::Cell<usize>);
impl RestAdapter for PendingAdapter {
    fn execute(&self, _: RestRequest) -> RestFuture<'_, RestResponse> {
        self.0.set(self.0.get() + 1);
        Box::pin(std::future::pending())
    }
}
#[test]
fn account_read_drop_has_no_latch_or_late_error() {
    let runtime = Rc::new(Runtime::default());
    let base = BaseExchange::new(
        ExchangeName::Kucoin,
        serde_json::from_value(json!({"key":"fixture","secret":"fixture"})).unwrap(),
        Rc::new(DefaultStore::new()),
        runtime.clone(),
        ExchangeSettings::default(),
    );
    let adapter = PendingAdapter(std::cell::Cell::new(0));
    drop(get_account(&base, &adapter));
    assert_eq!(adapter.0.get(), 0);
    let waker = Waker::noop();
    let mut cx = Context::from_waker(waker);
    for _ in 0..2 {
        let mut future = Box::pin(get_account(&base, &adapter));
        assert!(future.as_mut().poll(&mut cx).is_pending());
        drop(future);
    }
    assert_eq!(adapter.0.get(), 2);
    assert!(runtime.errors.borrow().is_empty());
    let adapter = Adapter {
        case: json!({"data":{"code":"200000","data":{"userId":"after-drop"}}}),
        calls: RefCell::new(vec![]),
    };
    assert_eq!(
        ready(get_account(&base, &adapter)).unwrap().user_id,
        "after-drop"
    );
}

fn oracle(cases: &[Value]) -> Vec<Value> {
    let mut child = Command::new("node")
        .arg(format!(
            "{}/scripts/accounts.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&cases).unwrap().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let reference: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    reference
}

#[test]
fn account_utf8_representation_refusal_preserves_live_witness() {
    let specimens: [(&str, &[u16]); 6] = [
        (r"\ud800", &[0xd800]),
        (r"\udfff", &[0xdfff]),
        (r"\ud83d\ude00", &[0xd83d, 0xde00]),
        (r"\u00e9", &[0xe9]),
        (r"\ufeff", &[0xfeff]),
        (" ", &[0x20]),
    ];
    let mut inputs = Vec::new();
    let mut accounts = Vec::new();
    for venue in ["kucoin", "kucoinspot"] {
        for (escaped, units) in specimens {
            let raw = format!(r#"{{"code":"200000","data":{{"userId":"{escaped}"}}}}"#);
            let case = json!({"venue":venue,"raw":raw,"observeUser":true});
            let runtime = Rc::new(Runtime::default());
            let base = BaseExchange::new(
                if venue == "kucoinspot" {
                    ExchangeName::Kucoinspot
                } else {
                    ExchangeName::Kucoin
                },
                serde_json::from_value(json!({"key":"fixture","secret":"fixture"})).unwrap(),
                Rc::new(DefaultStore::new()),
                runtime.clone(),
                ExchangeSettings::default(),
            );
            let adapter = Adapter {
                case: case.clone(),
                calls: RefCell::new(vec![]),
            };
            let account = if venue == "kucoinspot" {
                ready(get_spot_account(&base, &adapter))
            } else {
                ready(get_account(&base, &adapter))
            }
            .unwrap();
            if [r"\ud800", r"\udfff"].contains(&escaped) {
                assert_eq!(
                    account.user_id, "",
                    "KUCOIN-STAGING native whole-record surrogate refusal"
                );
                assert_eq!(account.sub_id, Some(None));
                assert_eq!(account.affiliate_id, Some(None));
                let errors = runtime.errors.borrow();
                assert_eq!(errors.len(), 1);
                assert!(errors[0].contains("representation refusal"));
                assert!(!errors[0].contains('\u{fffd}'));
            } else {
                assert_eq!(
                    account.user_id.encode_utf16().collect::<Vec<_>>(),
                    units,
                    "KUCOIN-STAGING native Unicode scalar units"
                );
                assert!(runtime.errors.borrow().is_empty());
            }
            assert_eq!(adapter.calls.borrow().len(), 1);
            inputs.push(case);
            accounts.push(account);
        }
    }
    assert_eq!(accounts.len(), 12);
    let captured = oracle(&inputs);
    assert_eq!(captured.len(), 12);
    for (index, reference) in captured.iter().enumerate() {
        let (escaped, units) = specimens[index % 6];
        assert_eq!(reference["userKind"], "string");
        assert_eq!(reference["userLength"], if index % 6 == 2 { 2 } else { 1 });
        assert_eq!(
            reference["userUnits"],
            json!(units),
            "actual source returned userId units before wire"
        );
        assert_eq!(reference["errors"], json!([]));
        if [r"\ud800", r"\udfff"].contains(&escaped) {
            assert!(reference["accountJSON"].as_str().unwrap().contains(escaped));
        } else {
            assert_eq!(
                serde_json::to_string(&accounts[index]).unwrap(),
                reference["accountJSON"].as_str().unwrap()
            );
        }
    }
}

#[test]
fn account_failure_body_reports_representation_refusal() {
    let runtime = Rc::new(Runtime::default());
    let base = BaseExchange::new(
        ExchangeName::Kucoinspot,
        serde_json::from_value(json!({"key":"fixture","secret":"fixture"})).unwrap(),
        Rc::new(DefaultStore::new()),
        runtime.clone(),
        ExchangeSettings::default(),
    );
    let adapter = Adapter {
        case: json!({"failure":{"message":"outer fallback","response":{"raw":r#"{"msg":"\ud800"}"#}}}),
        calls: RefCell::new(vec![]),
    };
    assert!(
        ready(get_spot_account(&base, &adapter))
            .unwrap()
            .user_id
            .is_empty()
    );
    assert!(runtime.errors.borrow()[0].contains("representation refusal"));
    assert!(!runtime.errors.borrow()[0].contains("outer fallback"));
}

#[test]
fn account_actual_axios_raw_response_staging_findings() {
    let mut inputs = Vec::new();
    for venue in ["kucoin", "kucoinspot"] {
        for raw in ["", "not-json"] {
            for failed in [false, true] {
                inputs.push(if failed { json!({"venue":venue,"failure":{"message":"network message","response":{"raw":raw}}}) } else { json!({"venue":venue,"raw":raw}) });
            }
        }
    }
    let mut actuals = Vec::new();
    for case in &inputs {
        let spot = case["venue"] == "kucoinspot";
        let failed = case.get("failure").is_some();
        let runtime = Rc::new(Runtime::default());
        let base = BaseExchange::new(
            if spot {
                ExchangeName::Kucoinspot
            } else {
                ExchangeName::Kucoin
            },
            serde_json::from_value(json!({"key":"fixture","secret":"fixture"})).unwrap(),
            Rc::new(DefaultStore::new()),
            runtime.clone(),
            ExchangeSettings::default(),
        );
        let adapter = Adapter {
            case: case.clone(),
            calls: RefCell::new(vec![]),
        };
        let account = if spot {
            ready(get_spot_account(&base, &adapter))
        } else {
            ready(get_account(&base, &adapter))
        }
        .unwrap();
        assert_eq!(account.user_id, "", "KUCOIN-STAGING native user");
        assert_eq!(account.sub_id, Some(None));
        assert_eq!(account.affiliate_id, Some(None));
        let message = if failed {
            "network message"
        } else if spot {
            "Failed to fetch KuCoin Spot account summary"
        } else {
            "Failed to get account info"
        };
        assert_eq!(
            &*runtime.errors.borrow(),
            &[message],
            "KUCOIN-STAGING native fallback"
        );
        assert_eq!(adapter.calls.borrow().len(), 1);
        let mut actual =
            json!({"calls":&*adapter.calls.borrow(),"errors":&*runtime.errors.borrow()});
        if failed {
            actual["account"] = serde_json::to_value(account).unwrap();
        } else {
            actual["accountJSON"] = json!(serde_json::to_string(&account).unwrap());
        }
        actuals.push(actual);
    }
    assert_eq!(actuals.len(), 8);
    let captured = oracle(&inputs);
    assert_eq!(captured.len(), 8);
    assert_eq!(actuals, captured);
}
#[test]
fn rest_read_oracle_runtime_and_compiler_fail_loudly() {
    for script in ["accounts", "spot-reads"] {
        assert!(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("scripts/{script}.cjs"))
                .is_file(),
            "missing oracle must not masquerade as an intentional runtime/compiler refusal"
        );
        for (key, value) in [
            ("KUCOIN_READS_MISSING_RUNTIME", "1"),
            (
                "SAFE_CEX_TYPESCRIPT_PATH",
                "missing-kucoin-typescript-control",
            ),
        ] {
            let mut child = std::process::Command::new("node")
                .arg(format!(
                    "{}/scripts/{script}.cjs",
                    env!("CARGO_MANIFEST_DIR")
                ))
                .env(key, value)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            use std::io::Write;
            child.stdin.take().unwrap().write_all(b"[]").unwrap();
            let output = child.wait_with_output().unwrap();
            assert!(!output.status.success());
            assert!(String::from_utf8_lossy(&output.stderr).contains("MODULE_NOT_FOUND"));
        }
    }
}

#[test]
fn account_native_phase_and_deep_json_floor() {
    let deep = format!(
        "{{\"code\":\"200000\",\"data\":{{\"userId\":\"deep\"}},\"unused\":{}0{}}}",
        "[".repeat(170),
        "]".repeat(170)
    );
    let mut cases = Vec::new();
    for venue in ["kucoin", "kucoinspot"] {
        cases.push(json!({"venue":venue,"raw":deep}));
        for phase in ["encoded", "untransformed", "transformed"] {
            cases.push(json!({"venue":venue,"phase":phase,"data":"{\"code\":\"200000\",\"data\":{\"userId\":\"once\"}}"}));
        }
    }
    let mut native = Vec::new();
    for case in &cases {
        let spot = case["venue"] == "kucoinspot";
        let runtime = Rc::new(Runtime::default());
        let base = BaseExchange::new(
            if spot {
                ExchangeName::Kucoinspot
            } else {
                ExchangeName::Kucoin
            },
            serde_json::from_value(json!({"key":"fixture","secret":"fixture"})).unwrap(),
            Rc::new(DefaultStore::new()),
            runtime.clone(),
            ExchangeSettings::default(),
        );
        let adapter = Adapter {
            case: case.clone(),
            calls: RefCell::new(vec![]),
        };
        let account = if spot {
            ready(get_spot_account(&base, &adapter))
        } else {
            ready(get_account(&base, &adapter))
        }
        .unwrap();
        let expected = if case.get("raw").is_some() {
            "deep"
        } else if case["phase"] == "untransformed" {
            "once"
        } else {
            ""
        };
        assert_eq!(
            account.user_id, expected,
            "KUCOIN-STAGING native phase/depth"
        );
        assert_eq!(account.sub_id, Some(None));
        assert_eq!(account.affiliate_id, Some(None));
        let errors: Vec<String> = if expected.is_empty() {
            vec![
                if spot {
                    "Failed to fetch KuCoin Spot account summary"
                } else {
                    "Failed to get account info"
                }
                .into(),
            ]
        } else {
            vec![]
        };
        assert_eq!(*runtime.errors.borrow(), errors);
        assert_eq!(adapter.calls.borrow().len(), 1);
        native.push((account, adapter.calls.into_inner(), errors));
    }
    assert_eq!(native.len(), 8);
    let captured = oracle(&cases);
    assert_eq!(captured.len(), 8);
    for ((account, calls, errors), reference) in native.into_iter().zip(captured) {
        if let Some(encoded) = reference.get("accountJSON") {
            assert_eq!(
                serde_json::to_string(&account).unwrap(),
                encoded.as_str().unwrap()
            );
        } else {
            assert_eq!(serde_json::to_value(account).unwrap(), reference["account"]);
        }
        assert_eq!(json!(calls), reference["calls"]);
        assert_eq!(json!(errors), reference["errors"]);
    }
}
