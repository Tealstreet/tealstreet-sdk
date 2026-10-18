use super::*;

pub(super) fn raw_reporting_base() -> BaseExchange {
    let runtime = Rc::new(ReportingRuntime {
        worker: false,
        consoles: RefCell::default(),
        reports: RefCell::default(),
        raw_reports: RefCell::default(),
        trace: Rc::default(),
    });
    BaseExchange::new(
        ExchangeName::Mexc,
        serde_json::from_value(json!({"key":"key","secret":"secret","publicDataOnly":false}))
            .unwrap(),
        Rc::new(DefaultStore::new()),
        runtime,
        ExchangeSettings::default(),
    )
}

struct ReportingRuntime {
    worker: bool,
    consoles: RefCell<Vec<String>>,
    reports: RefCell<Vec<Value>>,
    raw_reports: RefCell<Vec<safe_cex_exchange::js_runtime::value::Js>>,
    trace: Rc<RefCell<Vec<String>>>,
}
impl ExchangeRuntime for ReportingRuntime {
    fn monotonic_now_ms(&self) -> f64 {
        100.
    }
    fn now_ms(&self) -> f64 {
        100.
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        panic!("unexpected sleep")
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("unexpected spawn")
    }
    fn console_values(&self, severity: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {
        self.trace.borrow_mut().push(severity.into());
        self.consoles.borrow_mut().push(severity.into());
    }
    fn unique_id(&self) -> String {
        "reporting-id".into()
    }
    fn is_worker(&self) -> bool {
        self.worker
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_raw_exception(
        &self,
        error: &safe_cex_exchange::js_runtime::value::Js,
        data: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        // Retain the host's original value before any finite fixture observation.
        self.raw_reports.borrow_mut().push(error.clone());
        let error = native_report_shape(error);
        self.trace.borrow_mut().push("report".into());
        self.reports
            .borrow_mut()
            .push(json!({"error":error,"data":data}));
    }
    fn report_exception(
        &self,
        error: &ExchangeError,
        data: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        self.trace.borrow_mut().push("typed-report".into());
        self.reports
            .borrow_mut()
            .push(json!({"error":{"name":error.name,"message":error.message},"data":data}));
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("unexpected worker request")
    }
}
struct ReportingFailure {
    failure: AccountFailure,
    gets: Cell<usize>,
    trace: Option<Rc<RefCell<Vec<String>>>>,
}

fn assert_source_tree(source: &Value, native: &Value, path: &str) {
    match (source, native) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(
            a.as_f64().unwrap().to_bits(),
            b.as_f64().unwrap().to_bits(),
            "Source/native decoded Number bits {path}"
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "Source/native array fields {path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                assert_source_tree(a, b, &format!("{path}/{i}"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            let keys = |o: &serde_json::Map<String, Value>| {
                o.keys().cloned().collect::<std::collections::BTreeSet<_>>()
            };
            assert_eq!(keys(a), keys(b), "Source/native object fields {path}");
            for (key, a) in a {
                assert_source_tree(a, &b[key], &format!("{path}/{key}"));
            }
        }
        _ => assert_eq!(source, native, "Source/native decoded value {path}"),
    }
}
impl RestAdapter for ReportingFailure {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        if let Some(trace) = &self.trace {
            trace.borrow_mut().push("get".into());
        }
        assert_eq!(request.method, "get", "native reporting GET verb");
        assert!(
            matches!(
                request.url.as_str(),
                "/api/v1/private/position/open_positions" | "/api/v1/private/account/assets"
            ),
            "native reporting request route"
        );
        self.gets.set(self.gets.get() + 1);
        self.failure.execute(request)
    }
}

#[test]
fn mexc_raw_reporting_listener_failure_prevents_fallback() {
    let mut observations = vec![];
    for worker in [false, true] {
        for method in ["positions", "validate", "account"] {
            let store = Rc::new(history_store::ObservedStore::new());
            let runtime = Rc::new(ReportingRuntime {
                worker,
                consoles: RefCell::default(),
                reports: RefCell::default(),
                raw_reports: RefCell::default(),
                trace: Rc::default(),
            });
            let failure = Rc::new(ReportingFailure {
                gets: Cell::new(0),
                trace: Some(runtime.trace.clone()),
                failure: AccountFailure {
                    message: "transport".into(),
                    response_data: Some(json!({"message":7})),
                },
            });
            let mut exchange = venue(Rc::new(DefaultStore::new()), 100., false);
            exchange.base = Rc::new(BaseExchange::new(
                ExchangeName::Mexc,
                serde_json::from_value(json!({"key":"key","secret":"secret"})).unwrap(),
                store.clone(),
                runtime.clone(),
                ExchangeSettings::default(),
            ));
            exchange.xhr = failure.clone();
            let mut original = ExchangeError::new("listener failed");
            original.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
                "LISTENER"
            )));
            original.origin = Some(safe_cex_exchange::runtime::ExchangeErrorOrigin::new(
                std::io::Error::other("listener origin"),
            ));
            let first = Rc::new(Cell::new(0));
            let second = Rc::new(Cell::new(0));
            let first_calls = first.clone();
            let trace = runtime.trace.clone();
            let thrown = original.clone();
            let reported_json = Rc::new(RefCell::new(Value::Null));
            let captured_json = reported_json.clone();
            exchange.base.on_args(
                "error",
                Rc::new(move |_, args| {
                    if worker {
                        let raw = args[1].get("sentryData")?.get("error")?;
                        match &raw {
                            safe_cex_exchange::js_runtime::value::Js::Number(number) => {
                                assert_eq!(
                                    number.to_bits(),
                                    7_f64.to_bits(),
                                    "native report Number bits before projection"
                                );
                            }
                            _ => panic!("raw worker report must retain Number type"),
                        }
                        *captured_json.borrow_mut() = json!(native_stringify(&raw));
                    }
                    let args: Vec<serde_json::Value> = args
                        .iter()
                        .map(|value| safe_cex_exchange::js_runtime::value::to_json(value).unwrap())
                        .collect();
                    first_calls.set(first_calls.get() + 1);
                    trace.borrow_mut().push("first".into());
                    assert_eq!(args[0], "7", "native decoded listener raw formatted value");
                    if worker {
                        assert_source_tree(
                            &json!(7),
                            &args[1]["sentryData"]["error"],
                            "native worker raw report before listener",
                        );
                    }
                    Err(thrown.clone())
                }),
            );
            let second_calls = second.clone();
            exchange.base.on(
                "error",
                Rc::new(move |_, _| {
                    second_calls.set(second_calls.get() + 1);
                    Ok(())
                }),
            );
            let result = match method {
                "positions" => {
                    futures::executor::block_on(exchange.fetch_positions(None)).map(|_| ())
                }
                "validate" => futures::executor::block_on(exchange.validate_account()).map(|_| ()),
                "account" => futures::executor::block_on(exchange.get_account()).map(|_| ()),
                _ => unreachable!(),
            };
            assert!(
                result.is_err(),
                "native listener rejection propagates before fallback {method}"
            );
            let error = result.unwrap_err();
            assert_eq!(
                error, original,
                "native same structured listener error and origin through {method}"
            );
            assert_eq!(
                (
                    first.get(),
                    second.get(),
                    failure.gets.get(),
                    store.position_reads.get()
                ),
                (1, 0, 1, 0),
                "native independent throwing-listener child floors"
            );
            assert_eq!(
                runtime.reports.borrow().len(),
                usize::from(!worker),
                "native main raw reporting before listener"
            );
            if !worker {
                assert_source_tree(
                    &json!(7),
                    &runtime.reports.borrow()[0]["error"],
                    "native raw main report Number bits before listener",
                );
            }
            let expected = if worker {
                vec!["get", "error", "trace", "first"]
            } else {
                vec!["get", "error", "trace", "report", "first"]
            };
            assert_eq!(
                *runtime.trace.borrow(),
                expected,
                "native exact throwing-listener call order"
            );
            observations.push(json!({"worker":worker,"method":method,"error":{"name":error.name,"message":error.message,"code":observed_error_code(&error.code)},
                "trace":*runtime.trace.borrow(),"gets":failure.gets.get(),"positionReads":store.position_reads.get(),"reportedJson":*reported_json.borrow()}));
        }
    }
    assert_eq!(
        observations.len(),
        6,
        "native independent throwing-listener root floor"
    );
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/reporting_values_oracle.mjs"
        ))
        .arg("--listeners")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "SOURCE actual throwing-listener path: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_source_tree(&source, &Value::Array(observations), "listener");
}

fn observed_error_code(
    code: &Option<safe_cex_exchange::js_runtime::value::Js>,
) -> Option<serde_json::Value> {
    code.as_ref().map(|value| {
        safe_cex_exchange::js_runtime::value::to_json(value).expect("finite JSON code fixture")
    })
}

// Exercise the same operation installed as JSON.stringify, never serde's formatter.
fn native_stringify(value: &safe_cex_exchange::js_runtime::value::Js) -> String {
    let units = safe_cex_exchange::js_runtime::globals::stringify_json(value)
        .expect("native reporting JSON.stringify")
        .expect("report value is JSON serializable");
    String::from_utf16(&units).expect("report JSON contains valid UTF-16")
}

// Match the source observer's Error property reads without redefining their
// enumerability. JSON.stringify is checked separately on the same native value.
fn native_report_shape(raw: &safe_cex_exchange::js_runtime::value::Js) -> Value {
    use safe_cex_exchange::js_runtime::value::{Js, to_json};
    if matches!(raw, Js::Object(object) if object.class == "Error") {
        json!({"name": to_json(&raw.get("name").unwrap()).unwrap(), "message": to_json(&raw.get("message").unwrap()).unwrap()})
    } else {
        to_json(raw).expect("finite raw reporting fixture")
    }
}
