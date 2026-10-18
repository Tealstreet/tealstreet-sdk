//! Shared retry state, delay and deadline behavior against installed axios-retry.
use safe_cex_exchange::{
    js_runtime::{
        client::{self, JsHttpHost},
        promise::{JsFuture, Promise, await_js},
        value::{Js, Thrown, arg},
    },
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};
#[path = "../../test-support/release.rs"]
mod release;
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
#[path = "support/retry_observer_native.rs"]
mod retry_observer_native;
#[derive(Debug, PartialEq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Trace {
    Send { count: f64, timeout: f64, time: f64 },
    Condition { count: f64, time: f64 },
    Delay { count: f64, status: f64, time: f64 },
    Sleep { ms: f64, time: f64 },
}
#[derive(Debug, PartialEq, serde::Deserialize)]
struct Outcome {
    ok: bool,
    count: f64,
}
#[derive(Debug, PartialEq, serde::Deserialize)]
struct Capture {
    trace: Vec<Trace>,
    outcome: Outcome,
}
#[derive(Default)]
struct Runtime {
    now: Rc<Cell<f64>>,
    timer_release: release::Release,
    jobs: RefCell<Vec<Pin<Box<dyn Future<Output = ()>>>>>,
    trace: Rc<RefCell<Vec<Trace>>>,
    sleep_error: Cell<bool>,
}
struct Task;
impl ExchangeTask for Task {
    fn cancel(&self) {}
}
impl ExchangeRuntime for Runtime {
    fn console(&self, _: &str, _: &str) {}
    fn monotonic_now_ms(&self) -> f64 {
        self.now.get()
    }
    fn now_ms(&self) -> f64 {
        self.now.get()
    }
    fn sleep(&self, ms: u64) -> ExchangeFuture<'static, ()> {
        self.trace.borrow_mut().push(Trace::Sleep {
            ms: ms as f64,
            time: self.now.get(),
        });
        let (now, released, fail) = (
            self.now.clone(),
            self.timer_release.wait(),
            self.sleep_error.get(),
        );
        Box::pin(async move {
            released.await;
            if fail {
                let mut error = ExchangeError::new("scripted timer failure");
                error.store_subscriber_ids = vec![47];
                return Err(error);
            }
            now.set(now.get() + ms as f64);
            Ok(())
        })
    }
    fn spawn_local(&self, f: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        self.jobs.borrow_mut().push(f);
        Box::new(Task)
    }
    fn unique_id(&self) -> String {
        "retry-test".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_exception(&self, _: &ExchangeError, _: &Value) {}
    fn report_raw_exception(&self, _: &Value, _: &Value) {}
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        Box::pin(async { Ok(Value::Null) })
    }
}
struct Http {
    rt: Rc<Runtime>,
    attempts: Cell<usize>,
    failures: usize,
}
impl JsHttpHost for Http {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.rt.clone()
    }
    fn send(&self, config: Js) -> JsFuture<'static> {
        let count = config
            .get("axios-retry")
            .unwrap()
            .get("retryCount")
            .unwrap();
        self.rt.trace.borrow_mut().push(Trace::Send {
            count: count.to_number().unwrap(),
            timeout: config.get("timeout").unwrap().as_f64().unwrap(),
            time: self.rt.now.get(),
        });
        self.rt.now.set(self.rt.now.get() + 20.0);
        let status = if self.attempts.get() < self.failures {
            429.0
        } else {
            200.0
        };
        self.attempts.set(self.attempts.get() + 1);
        Box::pin(async move {
            Ok(Js::object(vec![
                ("status", Js::Number(status)),
                ("data", Js::object(vec![])),
                ("config", config),
            ]))
        })
    }
}
fn jobs(rt: &Runtime) {
    let pending = std::mem::take(&mut *rt.jobs.borrow_mut());
    for mut job in pending {
        if job
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
        {
            rt.jobs.borrow_mut().push(job);
        }
    }
}
fn settle(rt: &Runtime, value: Js) -> Result<Js, Thrown> {
    let mut future = Box::pin(await_js(value));
    for _ in 0..32 {
        if let Poll::Ready(result) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return result;
        }
        jobs(rt);
    }
    panic!("retry producer did not settle")
}
fn setup(s: &Value) -> (Rc<Runtime>, Rc<Http>, Js, Js) {
    let rt = Rc::new(Runtime::default());
    rt.timer_release.set(true);
    rt.now.set(1000.0);
    let http = Rc::new(Http {
        rt: rt.clone(),
        attempts: Cell::new(0),
        failures: s["failures"].as_u64().unwrap() as usize,
    });
    let xhr = client::create(
        http.clone(),
        Js::object(vec![(
            "timeout",
            Js::Number(s["timeout"].as_f64().unwrap()),
        )]),
    );
    let (condition_rt, delay_rt) = (rt.clone(), rt.clone());
    let stop = s["stop"].as_f64().unwrap();
    let delay = s["delay"].as_f64().unwrap();
    let async_refusal = s["asyncRefusal"].as_bool().unwrap();
    let null_condition = s["nullCondition"].as_bool().unwrap_or(false);
    let async_approval = s["asyncApproval"].as_bool().unwrap_or(false);
    let async_rejection = s["asyncRejection"].as_bool().unwrap_or(false);
    xhr.call_method(
        "__configureRetry",
        vec![Js::object(vec![
            ("retries", Js::Number(3.0)),
            (
                "shouldResetTimeout",
                Js::Bool(s["reset"].as_bool().unwrap()),
            ),
            (
                "retryCondition",
                Js::function("retryCondition", move |_, a| {
                    let count = arg(&a, 0)
                        .get("config")?
                        .get("axios-retry")?
                        .get("retryCount")?;
                    condition_rt.trace.borrow_mut().push(Trace::Condition {
                        count: count.to_number()?,
                        time: condition_rt.now.get(),
                    });
                    if null_condition {
                        return Ok(Js::Null);
                    }
                    if async_approval || async_rejection {
                        let runtime: Rc<dyn ExchangeRuntime> = condition_rt.clone();
                        let outcome = if async_approval {
                            Ok(Js::Undefined)
                        } else {
                            Err(Thrown::error("Error", "predicate refusal"))
                        };
                        return Ok(Js::Promise(Promise::settled(&runtime, outcome)));
                    }
                    if async_refusal {
                        let runtime: Rc<dyn ExchangeRuntime> = condition_rt.clone();
                        return Ok(Js::Promise(Promise::settled(&runtime, Ok(Js::Bool(false)))));
                    }
                    Ok(Js::Bool(count.to_number()? < stop))
                }),
            ),
            (
                "retryDelay",
                Js::function("retryDelay", move |_, a| {
                    let count = arg(&a, 0);
                    let error = arg(&a, 1);
                    delay_rt.trace.borrow_mut().push(Trace::Delay {
                        count: count.to_number()?,
                        status: error.get("response")?.get("status")?.to_number()?,
                        time: delay_rt.now.get(),
                    });
                    Ok(Js::Number(delay * count.to_number()?))
                }),
            ),
        ])],
    )
    .unwrap();
    let config = Js::object(vec![]);
    if let Some(seed) = s["seed"].as_f64() {
        config
            .define(
                "axios-retry",
                Js::object(vec![
                    ("retryCount", Js::Number(seed)),
                    ("retries", Js::Number(2.0)),
                ]),
            )
            .unwrap();
    }
    (rt, http, xhr, config)
}
fn scenarios() -> Value {
    json!([
        {"timeout":500,"reset":false,"failures":2,"stop":2,"delay":125,"seed":null,"asyncRefusal":false},
        {"timeout":500,"reset":true,"failures":2,"stop":2,"delay":125,"seed":null,"asyncRefusal":false},
        {"timeout":100,"reset":false,"failures":2,"stop":2,"delay":125,"seed":null,"asyncRefusal":false},
        {"timeout":500,"reset":true,"failures":4,"stop":3,"delay":125,"seed":1,"asyncRefusal":false},
        {"timeout":30000,"reset":true,"failures":4,"stop":1,"delay":10000,"seed":null,"asyncRefusal":false},
        {"timeout":500,"reset":false,"failures":2,"stop":2,"delay":125,"seed":null,"asyncRefusal":true},
        {"timeout":500,"reset":false,"failures":1,"stop":2,"delay":125,"seed":null,"asyncRefusal":false,"asyncApproval":true},
        {"timeout":500,"reset":false,"failures":2,"stop":2,"delay":125,"seed":null,"asyncRefusal":false,"asyncRejection":true},
        {"timeout":500,"reset":false,"failures":1,"stop":2,"delay":125,"seed":null,"asyncRefusal":false,"nullCondition":true}
    ])
}
#[test]
fn configured_retry_matches_live_axios_counts_delays_deadlines_and_refusal() {
    let inputs = scenarios();
    let capture = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/client_retry_oracle.cjs"
        ))
        .arg(inputs.to_string())
        .output()
        .unwrap();
    assert!(
        capture.status.success(),
        "{}",
        String::from_utf8_lossy(&capture.stderr)
    );
    let expected: Vec<Capture> = serde_json::from_slice(&capture.stdout).unwrap();
    assert_eq!(expected.len(), 9, "fixed source scenario census");
    for (index, (s, expected)) in inputs.as_array().unwrap().iter().zip(expected).enumerate() {
        let (rt, _, xhr, config) = setup(s);
        let result = settle(
            &rt,
            xhr.call_method("get", vec![Js::str("/retry"), config])
                .unwrap(),
        );
        let (ok, config) = match result {
            Ok(response) => (true, response.get("config").unwrap()),
            Err(error) => (false, error.value.get("config").unwrap()),
        };
        let actual = Capture {
            trace: std::mem::take(&mut *rt.trace.borrow_mut()),
            outcome: Outcome {
                ok,
                count: config
                    .get("axios-retry")
                    .unwrap()
                    .get("retryCount")
                    .unwrap()
                    .to_number()
                    .unwrap(),
            },
        };
        assert_eq!(actual, expected, "live retry trace case {index}");
    }
}
#[test]
fn dropping_retry_observer_keeps_owned_timer_and_replay_alive() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/retry_observer_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source,
        json!({"immediate":0,"beforeReplay":1,"afterReplay":2})
    );
    #[cfg(all(feature = "native", not(target_arch = "wasm32")))]
    assert_eq!(
        retry_observer_native::capture(),
        source,
        "production NativeRuntime and HTTP observer-drop trace"
    );
    let input = json!({"timeout":500,"reset":true,"failures":1,"stop":2,"delay":125,"seed":null,"asyncRefusal":false});
    let (rt, http, xhr, config) = setup(&input);
    rt.timer_release.set(false);
    let observer = xhr
        .call_method("get", vec![Js::str("/retry"), config])
        .unwrap();
    assert_eq!(
        http.attempts.get(),
        source["immediate"].as_u64().unwrap() as usize,
        "Axios starts the request after returning its observer"
    );
    assert_eq!(rt.jobs.borrow().len(), 1, "runtime owns retry continuation");
    for _ in 0..8 {
        jobs(&rt);
        if rt
            .trace
            .borrow()
            .iter()
            .any(|event| matches!(event, Trace::Sleep { .. }))
        {
            break;
        }
    }
    assert!(
        matches!(
            rt.trace.borrow().last(),
            Some(Trace::Sleep { ms, .. }) if *ms == 125.0
        ),
        "producer reached the pending retry timer"
    );
    drop(observer);
    jobs(&rt);
    assert_eq!(http.attempts.get(), 1, "no retry before timer completes");
    rt.timer_release.set(true);
    for _ in 0..8 {
        jobs(&rt);
    }
    assert_eq!(
        http.attempts.get(),
        2,
        "observer drop cannot cancel producer replay"
    );
    assert!(
        rt.jobs.borrow().is_empty(),
        "completed retry producer releases its job"
    );
}
#[test]
fn rejected_retry_timer_keeps_native_error_and_never_resends() {
    let input = json!({"timeout":500,"reset":true,"failures":1,"stop":2,"delay":125,"seed":null,"asyncRefusal":false});
    let (rt, http, xhr, config) = setup(&input);
    rt.sleep_error.set(true);
    let error = settle(
        &rt,
        xhr.call_method("get", vec![Js::str("/retry"), config])
            .unwrap(),
    )
    .err()
    .unwrap()
    .to_exchange_error();
    assert_eq!(error.message, "scripted timer failure");
    assert_eq!(error.store_subscriber_ids, vec![47]);
    assert_eq!(
        http.attempts.get(),
        1,
        "failed timer cannot run a retry early"
    );
}
