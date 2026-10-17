use safe_cex_exchange::{error_reporting::*, runtime::ExchangeError};
use serde_json::json;
#[test]
fn http_failure_reads_status_only_and_correlates_once_boundary() {
    let response = json!({"status":429,"statusText":"Too Many Requests","status_text":"alternate"});
    let failure = get_http_failure(&response).unwrap();
    assert_eq!(failure.status, 429.);
    assert_eq!(failure.status_text, "Too Many Requests");
    assert!(get_http_failure(&json!({"status":"429"})).is_none());
    assert!(get_http_failure(&json!({"status":399})).is_none());
    assert_eq!(
        append_http_failure(
            Some(&json!("market stale")),
            &json!({"status":404,"statusText":"Not Found"})
        ),
        "market stale (HTTP 404)"
    );
    assert_eq!(
        append_http_failure(Some(&json!("Error HTTP 429")), &response),
        "Error HTTP 429"
    );
    assert_eq!(
        append_http_failure(Some(&json!(42)), &response),
        "42 (HTTP 429)"
    );
    let record = record_http_failure(&response, 10000., Some("/path?secret=key")).unwrap();
    let tags =
        serde_json::to_value(read_http_failure_context(Some(&record), 11000.).unwrap()).unwrap();
    assert_eq!(tags["recentHttpUrl"], "/path");
    assert_eq!(tags["recentHttpAgeMs"], json!(1000.0));
    assert!(read_http_failure_context(Some(&record), 11001.).is_none());
    assert!(read_http_failure_context(Some(&record), 9999.).is_none());
    assert!(record_http_failure(&json!({"status":200}), 10000., None).is_none());
    assert_eq!(
        describe_http_failure("Bybit", &json!({"status":503,"data":{"message":"edge"}})),
        "Bybit API error (HTTP 503): {\"message\":\"edge\"}"
    );
}
#[test]
fn enhanced_error_causal_status_and_config_projection() {
    let mut error = ExchangeError::new("failure");
    error.context = Some(
        json!({"isNetworkError":true,"isTimeout":false,"statusCode":403,"statusText":"Forbidden","url":"/private","errorCode":"X","timestamp":"time"}),
    );
    error.enhanced_message = Some("enhanced".into());
    error.config = Some(
        json!({"baseURL":"https://venue","timeout":20000,"headers":{"Authorization":"secret","Content-Type":"application/json"},"params":{"key":"secret"}}),
    );
    error.original_error = Some(json!({"message":"original"}));
    let result =
        safe_cex_exchange::js_runtime::value::to_json(&format_error_for_sentry(&error)).unwrap();
    assert_eq!(result["tags"]["errorType"], "network");
    assert_eq!(result["tags"]["statusCode"].as_f64(), Some(403.));
    assert_eq!(result["extra"]["originalMessage"], "original");
    assert_eq!(
        result["extra"]["config"]["headers"],
        json!(["Authorization", "Content-Type"])
    );
    assert!(result["extra"]["config"].get("params").is_none());
    assert!(result["tags"].get("statusText").is_none());
}

#[path = "support/store.rs"]
mod store;
use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    runtime::{ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
};
#[derive(Default)]
struct Runtime {
    now: Cell<f64>,
    worker: Cell<bool>,
    id: Cell<u64>,
    reports: RefCell<Vec<serde_json::Value>>,
    console: RefCell<Vec<String>>,
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        self.now.get()
    }
    fn now_ms(&self) -> f64 {
        self.now.get()
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        Box::pin(async { Ok(()) })
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("No async tasks in telemetry test")
    }
    fn console(&self, severity: &str, message: &str) {
        self.console
            .borrow_mut()
            .push(format!("{severity}: {message}"));
    }
    fn unique_id(&self) -> String {
        let id = self.id.get() + 1;
        self.id.set(id);
        id.to_string()
    }
    fn is_worker(&self) -> bool {
        self.worker.get()
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_raw_exception(&self, _: &serde_json::Value, _: &serde_json::Value) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, error: &ExchangeError, data: &serde_json::Value) {
        self.reports
            .borrow_mut()
            .push(json!({"error":{"name":error.name,"message":error.message},"data":data}));
    }
    fn execute_worker_request(
        &self,
        _: serde_json::Value,
    ) -> ExchangeFuture<'static, serde_json::Value> {
        panic!("No worker request in telemetry test")
    }
}
fn error_input(input: &serde_json::Value) -> ExchangeErrorInput {
    if input.is_null() {
        return ExchangeErrorInput::Missing;
    }
    if let Some(text) = input.as_str() {
        return ExchangeErrorInput::Message(text.into());
    }
    let mut error = ExchangeError::new(input["message"].as_str().unwrap_or(""));
    if let Some(name) = input["name"].as_str() {
        error.name = name.into();
    }
    error.context = input.get("context").cloned();
    error.config = input.get("config").cloned();
    error.enhanced_message = input
        .get("enhancedMessage")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    error.url = input
        .get("url")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    error.method = input
        .get("method")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    error.original_error = input.get("originalError").cloned();
    ExchangeErrorInput::Error(Box::new(error))
}
fn observe_event(value: &safe_cex_exchange::js_runtime::value::Js) -> serde_json::Value {
    use safe_cex_exchange::js_runtime::value::{Js, to_json};
    match value {
        Js::Object(object) if object.class == "Error" => json!({
            "name": to_json(&value.get("name").unwrap()).unwrap(),
            "message": to_json(&value.get("message").unwrap()).unwrap(),
        }),
        Js::Object(_) => serde_json::Value::Object(
            value
                .own_entries()
                .unwrap()
                .into_iter()
                .filter(|(_, value)| !value.is_undefined())
                .map(|(key, value)| (String::from_utf16_lossy(&key), observe_event(&value)))
                .collect(),
        ),
        Js::Array(items) => serde_json::Value::Array(
            items
                .borrow()
                .iter()
                .map(|value| {
                    value
                        .as_ref()
                        .map(observe_event)
                        .unwrap_or(serde_json::Value::Null)
                })
                .collect(),
        ),
        _ => to_json(value).unwrap(),
    }
}
fn normalize_numbers(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Number(number) => {
            if let Some(n) = number.as_f64() {
                *value = json!(n)
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                normalize_numbers(item)
            }
        }
        serde_json::Value::Object(items) => {
            for item in items.values_mut() {
                normalize_numbers(item)
            }
        }
        _ => {}
    }
}
#[test]
fn base_error_events_match_live_typescript_oracle() {
    let mut oracle: serde_json::Value =
        serde_json::from_str(include_str!("error_reporting_oracle.json")).unwrap();
    assert_eq!(oracle["cases"].as_array().unwrap().len(), 18);
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/generate-error-reporting.cjs"
        ))
        .arg("--identity")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "live reporting identity source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let supplement: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = supplement["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 36, "live reporting identity case count");
    let combinations: std::collections::BTreeSet<_> = cases
        .iter()
        .map(|case| {
            let worker = case["worker"].as_bool().expect("identity worker boolean");
            let testnet = case["options"]["testnet"]
                .as_bool()
                .expect("identity testnet boolean");
            let public = case["options"]["publicDataOnly"]
                .as_bool()
                .expect("identity publicDataOnly boolean");
            assert_ne!(testnet, public, "identity flags must differ");
            (worker, testnet, public)
        })
        .collect();
    assert_eq!(
        combinations,
        [
            (false, false, true),
            (false, true, false),
            (true, false, true),
            (true, true, false)
        ]
        .into_iter()
        .collect(),
        "identity corpus covers both flag directions on both hosts"
    );
    oracle["cases"]
        .as_array_mut()
        .unwrap()
        .extend(cases.iter().cloned());
    for cases in oracle["cases"].as_array().unwrap().chunks(9) {
        let runtime = Rc::new(Runtime::default());
        runtime.now.set(10000.);
        runtime.worker.set(cases[0]["worker"].as_bool().unwrap());
        let base = BaseExchange::new(
            safe_cex_types::ExchangeName::Bybit,
            serde_json::from_value(
                json!({"key":"k","secret":"s","testnet":cases[0]["options"]["testnet"].as_bool().unwrap_or(false),"publicDataOnly":cases[0]["options"]["publicDataOnly"].as_bool().unwrap_or(false)}),
            )
            .unwrap(),
            Rc::new(store::TestStore::new()),
            runtime.clone(),
            ExchangeSettings::default(),
        );
        let events = Rc::new(RefCell::new(Vec::new()));
        let captured = events.clone();
        let _listener = base.on_args(
            "error",
            Rc::new(move |event, data| {
                {
                    let mut row = vec![json!(event)];
                    row.extend(data.iter().map(observe_event));
                    captured.borrow_mut().push(serde_json::Value::Array(row));
                };
                Ok(())
            }),
        );
        for (index, case) in cases.iter().enumerate() {
            match index {
                1 => {
                    base.note_http_response(
                        &json!({"status":429,"statusText":"Too Many Requests"}),
                        Some("/private?nonce=secret"),
                    );
                    runtime.now.set(10550.);
                }
                3 => base.note_http_response(
                    &json!({"status":403,"status_text":"Forbidden"}),
                    Some("/keys?secret=raw"),
                ),
                5 => {
                    base.note_http_response(&json!({"status":500}), Some("/old"));
                    runtime.now.set(11551.);
                }
                6 => {
                    base.note_http_response(&json!({"status":404}), Some("/missing"));
                    base.note_http_response(&json!({"status":200}), Some("/ok"));
                }
                8 => base.dispose().unwrap(),
                _ => {}
            }
            events.borrow_mut().clear();
            runtime.reports.borrow_mut().clear();
            base.emit_and_log_error(
                error_input(&case["input"]),
                case["msg"].as_str(),
                case.get("extra"),
            )
            .unwrap();
            let mut actual = json!({"events":events.borrow().clone(),"reports":runtime.reports.borrow().clone()});
            let mut expected = json!({"events":case["events"],"reports":case["reports"]});
            normalize_numbers(&mut actual);
            normalize_numbers(&mut expected);
            assert_eq!(
                actual, expected,
                "source error report preserves testnet/publicDataOnly field identity: {case}"
            );
        }
    }
}

#[test]
fn removed_original_message_preserves_worker_extra_order() {
    let runtime = Rc::new(Runtime::default());
    let base = BaseExchange::new(
        safe_cex_types::ExchangeName::Bybit,
        serde_json::from_value(json!({"key":"k","secret":"s"})).unwrap(),
        Rc::new(safe_cex_store::store::DefaultStore::new()),
        runtime.clone(),
        ExchangeSettings::default(),
    );
    let mut error = ExchangeError::new("failure");
    error.context = Some(json!({"timestamp":"now"}));
    error.enhanced_message = Some("enhanced".into());
    error.config = Some(json!({"timeout":1000}));
    base.emit_and_log_error(ExchangeErrorInput::Error(Box::new(error)), None, None)
        .unwrap();
    let reports = runtime.reports.borrow();
    let keys: Vec<_> = reports[0]["data"]["extra"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "exchangeOptions",
            "errorType",
            "enhancedMessage",
            "timestamp",
            "config"
        ],
        "removing originalMessage must preserve worker extra insertion order"
    );
}
