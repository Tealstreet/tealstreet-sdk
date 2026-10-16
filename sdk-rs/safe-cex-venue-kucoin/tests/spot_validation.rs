#[path = "support/numeric_bits.rs"]
mod numeric_bits;
use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_store::store::DefaultStore;
use safe_cex_transport::provider::{
    RestAdapter, RestFailure, RestFuture, RestRequest, RestResponse,
};
use safe_cex_types::{Balance, ExchangeName};
use safe_cex_venue_kucoin::accounts::validate_spot_account;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
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

struct Adapter {
    case: Value,
    calls: RefCell<Vec<Value>>,
    active: Cell<bool>,
}
struct WaitGuard<'a>(&'a Cell<bool>);
impl Drop for WaitGuard<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
impl RestAdapter for Adapter {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            assert!(request.body.is_none());
            self.calls
                .borrow_mut()
                .push(json!({"method":request.method,"url":request.url,"params":request.params}));
            if self.case["pending"] == true {
                self.active.set(true);
                let _guard = WaitGuard(&self.active);
                std::future::pending::<()>().await;
            }
            if let Some(failure) = self.case.get("failure") {
                let mut error =
                    RestFailure::pre_transmit(failure["message"].as_str().unwrap().into(), request);
                error.response = failure.get("response").map(|r| {
                    Box::new(RestResponse {
                        status: 503,
                        reason: None,
                        headers: Default::default(),
                        body: safe_cex_transport::RestResponseBody::Encoded(
                            r["raw"]
                                .as_str()
                                .map(|s| s.as_bytes().to_vec())
                                .unwrap_or_else(|| serde_json::to_vec(&r["data"]).unwrap()),
                        ),
                    })
                });
                return Err(error);
            }
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: safe_cex_transport::RestResponseBody::Encoded(
                    self.case["raw"]
                        .as_str()
                        .map(|s| s.as_bytes().to_vec())
                        .unwrap_or_else(|| serde_json::to_vec(&self.case["data"]).unwrap()),
                ),
            })
        })
    }
}
fn setup(case: Value) -> (BaseExchange, Adapter, Rc<Runtime>) {
    let runtime = Rc::new(Runtime::default());
    let base = BaseExchange::new(
        ExchangeName::Kucoinspot,
        serde_json::from_value(json!({"key":"fixture","secret":"fixture"})).unwrap(),
        Rc::new(DefaultStore::new()),
        runtime.clone(),
        ExchangeSettings::default(),
    );
    (
        base,
        Adapter {
            case,
            calls: RefCell::new(vec![]),
            active: Cell::new(false),
        },
        runtime,
    )
}
fn ready<T>(future: impl Future<Output = T>) -> T {
    let mut cx = Context::from_waker(Waker::noop());
    match std::pin::pin!(future).as_mut().poll(&mut cx) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("fixture unexpectedly blocked"),
    }
}
fn fixture_balance(case: &Value) -> Balance {
    let value: Value = case["balanceRaw"]
        .as_str()
        .map(|s| serde_json::from_str(s).unwrap())
        .unwrap_or_else(|| {
            case.get("balance")
                .cloned()
                .unwrap_or(json!({"used":1,"free":2,"total":3,"upnl":0}))
        });
    let number =
        |field: &str| safe_cex_num::numeric_ingress::js_value_as_f64(&value[field]).unwrap();
    Balance {
        used: number("used"),
        free: number("free"),
        total: number("total"),
        upnl: number("upnl"),
        info: None,
        currencies: None,
        coins: None,
    }
}
fn run(case: &Value) -> Value {
    let (base, adapter, runtime) = setup(case.clone());
    let count = Cell::new(0);
    let result = ready(validate_spot_account(&base, &adapter, || async {
        count.set(count.get() + 1);
        if case["balanceFailure"] == true {
            Err(())
        } else {
            Ok(fixture_balance(case))
        }
    }))
    .unwrap();
    let bits = result
        .balance
        .as_ref()
        .map(|b| [b.used, b.free, b.total, b.upnl].map(numeric_bits::bits));
    json!({"result":result,"errors":runtime.errors.borrow().clone(),"calls":adapter.calls.borrow().clone(),"balanceCalls":count.get(),"bits":bits})
}
// Compare JSON number spellings through the shared JS projection. Numeric
// identity remains independently asserted in the pre-serialization bits field.
fn wire_value(value: Value) -> Value {
    let encoded = safe_cex_num::js_json_stringify(&value.into())
        .unwrap()
        .unwrap();
    serde_json::from_str(&encoded).unwrap()
}
fn source(cases: &[Value]) -> Vec<Value> {
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/spot-validation.cjs"
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
        .write_all(&serde_json::to_vec(cases).unwrap())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let captures: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        captures.len(),
        cases.len(),
        "KUCOIN-CARDINALITY spot_validation source case count"
    );
    captures
}
#[test]
fn live_spot_validation_preserves_rejection_and_balance_catch_order() {
    let mut cases = vec![
        json!({"data":{"code":"200000"}}),
        json!({"data":{"code":"200000"},"balanceFailure":true}),
    ];
    for data in [
        json!(null),
        json!({}),
        json!(false),
        json!(0),
        json!([]),
        json!({"code":200000}),
        json!({"code":"bad"}),
    ] {
        cases.push(json!({"data":data}));
    }
    for message in [
        json!("denied"),
        json!(""),
        json!(false),
        json!(0),
        json!(null),
        json!(" café 🚀 \u{feff}\t"),
    ] {
        cases.push(json!({"data":{"code":"bad","msg":message}}));
        cases.push(json!({"failure":{"message":"network","response":{"data":{"msg":message}}}}));
    }
    cases.push(json!({"failure":{"message":""}}));
    cases.push(json!({"failure":{"message":"network"}}));
    cases.push(json!({"data":{"code":"200000"},"balanceRaw":r#"{"used":-0,"free":1e400,"total":-1e400,"upnl":-1e-4000}"#}));
    for raw in [
        r#"{"used":-0,"free":2,"total":3,"upnl":5}"#,
        r#"{"used":7,"free":2,"total":3,"upnl":-0}"#,
    ] {
        cases.push(json!({"data":{"code":"200000"},"balanceRaw":raw}));
    }
    let expected = source(&cases);
    assert_eq!(expected.len(), cases.len());
    for (case, mut expected) in cases.iter().zip(expected) {
        let capture = expected.as_object_mut().unwrap();
        assert!(
            capture
                .remove("captureArchitecture")
                .unwrap()
                .as_str()
                .is_some_and(|arch| !arch.is_empty()),
            "validation capture architecture"
        );
        assert!(
            capture
                .remove("captureNodeMajor")
                .unwrap()
                .as_u64()
                .is_some_and(|major| major > 0),
            "validation capture Node major"
        );
        capture.remove("ownBalance");
        assert_eq!(
            wire_value(run(case)),
            wire_value(expected),
            "live Spot validation ordering: {case}"
        );
    }
}
#[test]
fn validation_source_catch_mutation_is_not_a_noop() {
    let out = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "live_spot_validation_preserves_rejection_and_balance_catch_order",
            "--nocapture",
        ])
        .env("KUCOIN_VALIDATION_MUTATION", "propagate-balance")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(101));
    assert!(String::from_utf8_lossy(&out.stderr).contains("live Spot validation ordering"));
}
#[test]
fn validation_typed_error_and_undefined_presence_findings_are_visible() {
    let cases = vec![
        json!({"data":{"code":"bad","msg":77}}),
        json!({"failure":{"message":"network","response":{"data":{"msg":{"why":"no"}}}}}),
        json!({"data":{"code":"200000"},"balanceFailure":true}),
        json!({"data":{"code":"bad"}}),
    ];
    let (base, adapter, runtime) = setup(cases[0].clone());
    let native = ready(validate_spot_account(&base, &adapter, || async {
        Err::<Balance, ()>(())
    }))
    .unwrap();
    assert!(
        native.error.as_value().is_number(),
        "KUCOIN-VALIDATION-NATIVE numeric rejection type"
    );
    assert_eq!(
        native.error.as_value().as_i64(),
        Some(77),
        "KUCOIN-VALIDATION-NATIVE numeric rejection value"
    );
    assert!(
        runtime.errors.borrow().is_empty(),
        "KUCOIN-VALIDATION-NATIVE rejection never emits"
    );
    assert!(
        native.balance.is_none(),
        "KUCOIN-VALIDATION-NATIVE no balance on rejection"
    );
    assert_eq!(
        adapter.calls.borrow().len(),
        1,
        "KUCOIN-VALIDATION-NATIVE one account request"
    );
    let expected = source(&cases);
    let actual = run(&cases[0]);
    assert_eq!(actual["result"]["error"], expected[0]["result"]["error"]);
    assert_eq!(actual["errors"], json!([]));
    for i in 1..2 {
        let actual = run(&cases[i]);
        assert!(!expected[i]["result"]["error"].is_string());
        assert!(
            actual["result"]["error"]
                .as_str()
                .unwrap()
                .contains("shared string representation")
        );
        assert_eq!(actual["errors"], json!([actual["result"]["error"]]));
        assert_eq!(actual["balanceCalls"], 0);
    }
    assert_eq!(expected[2]["ownBalance"], true);
    assert_eq!(expected[3]["ownBalance"], false);
    for case in &cases[2..4] {
        assert!(run(case)["result"].get("balance").is_none());
    }
}
#[test]
fn validation_native_rejection_values_are_retained_without_emission() {
    let messages = [
        json!(true),
        json!({"why":"denied"}),
        json!([3, 7]),
        json!(77),
    ];
    for message in &messages {
        let (base, adapter, runtime) = setup(json!({"data":{"code":"bad","msg":message}}));
        let result = ready(validate_spot_account(&base, &adapter, || async {
            panic!("balance fetch after rejection");
            #[allow(unreachable_code)]
            Ok::<Balance, ()>(fixture_balance(&json!({})))
        }))
        .unwrap();
        assert_eq!(
            result.error.as_value(),
            message,
            "KUCOIN-VALIDATION-NATIVE original rejection value"
        );
        assert!(
            runtime.errors.borrow().is_empty(),
            "KUCOIN-VALIDATION-NATIVE raw rejection has no emission"
        );
        assert!(result.balance.is_none());
    }
    let cases = messages
        .iter()
        .map(|message| json!({"data":{"code":"bad","msg":message}}))
        .collect::<Vec<_>>();
    let reference = source(&cases);
    assert_eq!(
        reference.len(),
        4,
        "KUCOIN-VALIDATION-SOURCE rejection floor stops object/array/bool/number"
    );
    for (case, expected) in cases.iter().zip(reference) {
        let actual = run(case);
        assert_eq!(actual["result"]["error"], expected["result"]["error"]);
        assert_eq!(actual["errors"], expected["errors"]);
    }
}

#[test]
fn validation_balance_alias_is_an_owned_snapshot() {
    let case = json!({"data":{"code":"200000"},"alias":true});
    let expected = source(std::slice::from_ref(&case)).pop().unwrap();
    assert_eq!(expected["before"]["balance"]["free"], 2);
    assert_eq!(expected["result"]["balance"]["free"], 77);
    assert_eq!(expected["fetched"]["free"], 77);
    let (base, adapter, _) = setup(case.clone());
    let mut fetched = fixture_balance(&case);
    let result = ready(validate_spot_account(&base, &adapter, || {
        std::future::ready(Ok::<_, ()>(fetched.clone()))
    }))
    .unwrap();
    fetched.free = 77.0;
    assert_eq!(fetched.free, 77.0);
    assert_eq!(result.balance.unwrap().free, 2.0);
}
#[test]
fn validation_drop_releases_each_await_without_logging_or_latch() {
    for phase in ["request", "balance"] {
        let (base, adapter, runtime) =
            setup(json!({"data":{"code":"200000"},"pending":phase=="request"}));
        let called = Cell::new(0);
        let balance_active = Cell::new(false);
        for _ in 0..2 {
            let mut future = Box::pin(validate_spot_account(&base, &adapter, || async {
                called.set(called.get() + 1);
                balance_active.set(true);
                let _guard = WaitGuard(&balance_active);
                std::future::pending::<Result<Balance, ()>>().await
            }));
            assert!(
                future
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
            assert_eq!(adapter.active.get(), phase == "request");
            assert_eq!(balance_active.get(), phase == "balance");
            drop(future);
            assert!(!adapter.active.get() && !balance_active.get());
        }
        assert_eq!(called.get(), if phase == "balance" { 2 } else { 0 });
        assert!(runtime.errors.borrow().is_empty());
    }
}
#[test]
fn validation_failure_surrogate_refusal_never_becomes_network_fallback() {
    let cases = vec![
        json!({"failure":{"message":"network","response":{"raw":r#"{"msg":"\ud800"}"#}},"surrogate":true}),
        json!({"raw":r#"{"code":"bad","msg":"\udfff"}"#,"surrogate":true}),
    ];
    for (case, expected) in cases.iter().zip(source(&cases)) {
        assert!(expected["resultJson"].as_str().unwrap().contains("\\ud"));
        let actual = run(case);
        let error = actual["result"]["error"].as_str().unwrap();
        assert!(error.contains("representation refusal"), "{actual}");
        assert_ne!(error, "network");
        assert_eq!(actual["errors"], json!([error]));
    }
}

#[test]
fn validation_axios_raw_response_finding_preserves_source_no_emit() {
    let cases = vec![json!({"raw":"not-json"}), json!({"raw":""})];
    for case in &cases {
        let (base, adapter, runtime) = setup(case.clone());
        let result = ready(validate_spot_account(&base, &adapter, || async {
            panic!("unexpected balance fetch");
            #[allow(unreachable_code)]
            Ok::<Balance, ()>(fixture_balance(case))
        }))
        .unwrap();
        assert_eq!(
            result.error.as_value(),
            &json!("Failed to validate KuCoin Spot account"),
            "KUCOIN-STAGING native validation fallback"
        );
        assert!(result.balance.is_none());
        assert!(
            runtime.errors.borrow().is_empty(),
            "KUCOIN-STAGING native no emit"
        );
        assert_eq!(adapter.calls.borrow().len(), 1);
    }
    let captured = source(&cases);
    assert_eq!(captured.len(), 2);
    for expected in captured {
        assert_eq!(
            expected["result"]["error"],
            "Failed to validate KuCoin Spot account"
        );
        assert_eq!(expected["errors"], json!([]));
        assert_eq!(expected["balanceCalls"], 0);
    }
}

#[test]
fn validation_oracle_missing_dependency_fails_loudly() {
    for (variable, value) in [
        ("KUCOIN_VALIDATION_MISSING_RUNTIME", "1"),
        (
            "SAFE_CEX_TYPESCRIPT_PATH",
            "/missing-kucoin-compiler-control",
        ),
    ] {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/spot-validation.cjs"
            ))
            .env(variable, value)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("MODULE_NOT_FOUND"));
    }
}

#[test]
fn validation_native_json_number_and_object_floor() {
    let cases = vec![
        json!({"raw":r#"{"code":"rejected","msg":9007199254740993}"#,"captureRejection":true}),
        json!({"raw":r#"{"code":"rejected","msg":1e400}"#,"captureRejection":true}),
        json!({"raw":r#"{"code":"rejected","msg":{"$serde_json::private::Number":"42"}}"#,"captureRejection":true}),
    ];
    let mut observed = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let (base, adapter, runtime) = setup(case.clone());
        let result = ready(validate_spot_account(&base, &adapter, || async {
            panic!("unexpected balance");
            #[allow(unreachable_code)]
            Ok::<Balance, ()>(fixture_balance(case))
        }))
        .unwrap();
        let error = result.error.as_value();
        if index < 2 {
            if index == 0 {
                assert_eq!(
                    error.as_u64(),
                    Some(9007199254740992),
                    "KUCOIN-STAGING native exact rounded integer"
                );
            }
            let number = error
                .as_number()
                .expect("KUCOIN-STAGING native number kind")
                .to_string()
                .parse::<f64>()
                .unwrap();
            assert_eq!(
                number.to_bits(),
                if index == 0 {
                    9007199254740992f64.to_bits()
                } else {
                    f64::INFINITY.to_bits()
                },
                "KUCOIN-STAGING native rejected numeric value"
            );
            observed.push(("number", Some(numeric_bits::bits(number))));
        } else {
            let object = error
                .as_object()
                .expect("KUCOIN-STAGING native private-key object");
            assert_eq!(object.len(), 1);
            assert_eq!(object["$serde_json::private::Number"].as_str(), Some("42"));
            observed.push(("object", None));
        }
        assert!(result.balance.is_none());
        assert!(runtime.errors.borrow().is_empty());
        assert_eq!(adapter.calls.borrow().len(), 1);
    }
    let (base, adapter, _) = setup(cases[1].clone());
    let overflow = ready(validate_spot_account(&base, &adapter, || async {
        Err::<Balance, ()>(())
    }))
    .unwrap();
    assert_eq!(
        overflow.error.as_value().as_number().unwrap().to_string(),
        "1e+400"
    );
    let native_wire = serde_json::to_string(&overflow).unwrap();
    assert!(
        native_wire.contains("1e+400"),
        "KUCOIN-STAGING retained overflow wire finding"
    );
    let captured = source(&cases);
    assert_eq!(captured.len(), 3);
    assert_eq!(captured[1]["rejectionJSON"], r#"{"error":null}"#);
    assert_ne!(native_wire, captured[1]["rejectionJSON"].as_str().unwrap());
    for ((kind, bits), reference) in observed.into_iter().zip(captured) {
        assert_eq!(reference["errorKind"], kind);
        assert_eq!(reference["errorBits"], json!(bits));
        if kind == "object" {
            assert_eq!(
                reference["errorKeys"],
                json!(["$serde_json::private::Number"])
            );
            assert_eq!(reference["privateNumber"], "42");
        }
        assert_eq!(reference["errors"], json!([]));
        assert_eq!(reference["balanceCalls"], 0);
    }
}
