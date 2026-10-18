use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_store::store::DefaultStore;
use safe_cex_types::ExchangeName;
use safe_cex_venue_asterdex::exchange_helpers::assert_v3_feature;
use serde_json::{Value, json};
use std::{cell::RefCell, future::Future, pin::Pin, rc::Rc};

struct Runtime {
    calls: Rc<RefCell<Vec<Value>>>,
}
impl ExchangeRuntime for Runtime {
    fn now_ms(&self) -> f64 {
        1000.0
    }
    fn monotonic_now_ms(&self) -> f64 {
        self.now_ms()
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        panic!("unexpected guard timer")
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("unexpected guard task")
    }
    fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {}
    fn unique_id(&self) -> String {
        "guard-listener".to_owned()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_raw_exception(
        &self,
        _: &safe_cex_exchange::js_runtime::value::Js,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(
        &self,
        error: &ExchangeError,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        self.calls
            .borrow_mut()
            .push(json!(["report", error.name, error.message]));
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("unexpected guard worker request")
    }
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn feature_guard_matches_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/exchange-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Value = serde_json::from_slice(&output.stdout).unwrap();
    for case in cases["featureGuards"].as_array().unwrap() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let base = BaseExchange::new(
            ExchangeName::Asterdex,
            serde_json::from_value(json!({"key":"", "secret":""})).unwrap(),
            Rc::new(DefaultStore::new()),
            Rc::new(Runtime {
                calls: calls.clone(),
            }),
            ExchangeSettings::default(),
        );
        let listener_calls = calls.clone();
        base.on(
            "error",
            Rc::new(move |_, payload| {
                let payload = safe_cex_exchange::js_runtime::value::to_json(payload).unwrap();
                listener_calls.borrow_mut().push(json!(["emit", payload]));
                Ok(())
            }),
        );
        let result = assert_v3_feature(
            &base,
            serde_json::from_value(case["version"].clone()).unwrap(),
            case["feature"].as_str().unwrap(),
        );
        assert_eq!(
            result.is_ok(),
            case["success"].as_bool().unwrap(),
            "assertV3Feature {case}"
        );
        if let Err(error) = result {
            calls
                .borrow_mut()
                .push(json!(["throw", error.name, error.message]));
        }
        assert_eq!(
            json!(&*calls.borrow()),
            case["calls"],
            "assertV3Feature emit-before-throw {case}"
        );
    }
    for case in cases["featureSurrogates"].as_array().unwrap() {
        let raw = case["inputJson"].as_str().unwrap();
        assert!(
            serde_json::from_str::<String>(raw).is_err(),
            "valid JSON lone surrogate representation refusal"
        );
        let escaped = raw.trim_matches('"');
        assert!(case["outputJson"].as_str().unwrap().contains(escaped));
        assert!(case["callsJson"].as_str().unwrap().contains(escaped));
    }
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn feature_oracle_rejects_returned_error() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "feature_guard_matches_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_FEATURE_CONTROL", "return-error")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(diagnostics.contains("assertV3Feature"), "{diagnostics}");
}
