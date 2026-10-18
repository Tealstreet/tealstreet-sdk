//! Installed Lodash scheduling compared through the production global factory.
use safe_cex_exchange::{
    js_runtime::{
        client::JsHttpHost,
        globals, library,
        promise::JsFuture,
        scope::ScopeShared,
        value::{Js, Thrown, from_json, to_json},
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

type Job = Pin<Box<dyn Future<Output = ()>>>;
#[derive(Default)]
struct Runtime {
    mono: Rc<Cell<u64>>,
    wall: Cell<f64>,
    jobs: RefCell<Vec<(Rc<Cell<bool>>, Job)>>,
}
struct Task(Rc<Cell<bool>>);
impl ExchangeTask for Task {
    fn cancel(&self) {
        self.0.set(true);
    }
}
impl Runtime {
    fn drain(&self) {
        for _ in 0..1000 {
            let jobs = std::mem::take(&mut *self.jobs.borrow_mut());
            let mut kept = vec![];
            let mut completed = false;
            for (cancelled, mut job) in jobs {
                if cancelled.get() {
                    completed = true;
                    continue;
                }
                if job
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
                {
                    kept.push((cancelled, job));
                } else {
                    completed = true;
                }
            }
            let spawned = !self.jobs.borrow().is_empty();
            kept.append(&mut self.jobs.borrow_mut());
            *self.jobs.borrow_mut() = kept;
            if !completed && !spawned {
                return;
            }
        }
        panic!("timer loop");
    }
    fn advance(&self, at: u64) {
        while self.mono.get() < at {
            self.mono.set(self.mono.get() + 1);
            self.wall.set(self.wall.get() + 1.);
            self.drain();
        }
    }
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        self.mono.get() as f64
    }
    fn now_ms(&self) -> f64 {
        self.wall.get()
    }
    fn sleep(&self, ms: u64) -> ExchangeFuture<'static, ()> {
        let clock = self.mono.clone();
        let due = clock.get() + ms;
        let mut first = true;
        Box::pin(std::future::poll_fn(move |_| {
            if first {
                first = false;
                return Poll::Pending;
            }
            if clock.get() >= due {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        }))
    }
    fn spawn_local(&self, mut f: Job) -> Box<dyn ExchangeTask> {
        let cancelled = Rc::new(Cell::new(false));
        if f.as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
        {
            self.jobs.borrow_mut().push((cancelled.clone(), f));
        }
        Box::new(Task(cancelled))
    }
    fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {}
    fn unique_id(&self) -> String {
        "fixture".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_exception(&self, e: &ExchangeError, _: &Value) {
        panic!("unexpected timer error: {}", e.message)
    }
    fn report_raw_exception(&self, _: &safe_cex_exchange::js_runtime::value::Js, _: &Value) {
        panic!("unexpected raw error")
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("unexpected worker")
    }
}
impl safe_cex_exchange::websocket::WsRuntime for Runtime {
    fn transport(&self) -> std::sync::Arc<dyn safe_cex_transport::TransportProvider> {
        std::sync::Arc::new(safe_cex_transport::DirectTransport)
    }
    fn tunnel_host(&self) -> Rc<dyn safe_cex_transport::socket::TunnelHost> {
        panic!("no tunnel in js_runtime tests")
    }
    fn dial(
        &self,
        url: String,
        protocols: Vec<String>,
        options: safe_cex_exchange::websocket::WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn safe_cex_exchange::websocket::WsSocket>> {
        let _ = (url, protocols, options);
        Box::pin(async { Err(ExchangeError::new("recorded dial")) })
    }
    fn headers_capable(&self) -> bool {
        false
    }
    fn enable_reconnecting(&self) -> bool {
        false
    }
    fn min_reconnection_delay_ms(&self) -> f64 {
        1000.0
    }
    fn library_min_reconnection_delay_ms(&self) -> f64 {
        1000.0
    }
    fn diagnostics_enabled(&self) -> bool {
        false
    }
    fn lifecycle_logs_enabled(&self) -> bool {
        false
    }
}

struct Http(Rc<Runtime>);
impl JsHttpHost for Http {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.0.clone()
    }
    fn send(&self, _: Js) -> JsFuture<'static> {
        panic!("unexpected HTTP")
    }
}
fn fixture() -> (Rc<Runtime>, Rc<ScopeShared<()>>) {
    let runtime = Rc::new(Runtime::default());
    runtime.wall.set(1000.);
    let shared = Rc::new(ScopeShared::new(
        runtime.clone(),
        Rc::new(Http(runtime.clone())),
        Rc::new(safe_cex_store::store::DefaultStore::new()),
        Js::object(vec![]),
        (),
    ));
    globals::install_builtins(&shared.globals, Rc::downgrade(&shared));
    library::install_library(&shared.globals, Rc::downgrade(&shared));
    (runtime, shared)
}
fn result(value: Js) -> Value {
    if value.is_undefined() {
        json!("undefined")
    } else {
        to_json(&value).unwrap()
    }
}
#[test]
fn lodash_throttle_matches_source_clock_receiver_arguments_and_results() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_throttle_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 294, "independent throttle source case floor");
    for index in 0..294 {
        let case = &cases[index];
        let (runtime, shared) = fixture();
        let trace = Rc::new(RefCell::new(vec![]));
        let (clock, events) = (runtime.clone(), trace.clone());
        let callback = Js::function("callback", move |this, args| {
            events.borrow_mut().push(json!([
                "invoke",
                clock.now_ms(),
                to_json(&this.get("id")?)?,
                to_json(&Js::array(args.clone()))?
            ]));
            Ok(Js::Number(args[0].to_number()? * 10.))
        });
        let options = if case["options"].is_null() {
            Js::Undefined
        } else {
            from_json(&case["options"])
        };
        if case["options"]["leadingUndefined"] == true {
            options.define("leading", Js::Undefined).unwrap();
        }
        if case["options"]["trailingUndefined"] == true {
            options.define("trailing", Js::Undefined).unwrap();
        }
        let f = shared
            .globals
            .get("throttle")
            .unwrap()
            .call(
                Js::Undefined,
                vec![callback, from_json(&case["wait"]), options],
            )
            .unwrap();
        for action in case["actions"].as_array().unwrap() {
            let at = action[0].as_u64().unwrap();
            let op = action[1].as_str().unwrap();
            if op == "stall" {
                runtime
                    .wall
                    .set(runtime.wall.get() + (at - runtime.mono.get()) as f64);
                runtime.mono.set(at);
            } else {
                runtime.advance(at);
            }
            if op == "rollback" {
                runtime.wall.set(runtime.wall.get() - 50.);
            }
            let returned = if op == "call" || op == "stall" || op == "rollback" {
                let value = action[2].as_u64().unwrap();
                f.call(
                    Js::object(vec![("id", Js::str(&format!("receiver-{value}")))]),
                    vec![Js::Number(value as f64), Js::str(&format!("arg-{value}"))],
                )
                .unwrap()
            } else {
                f.call_method(op, vec![]).unwrap()
            };
            trace
                .borrow_mut()
                .push(json!([op, runtime.now_ms(), result(returned)]));
        }
        runtime.advance(150);
        assert_eq!(
            json!(*trace.borrow()),
            to_json(&from_json(&case["trace"])).unwrap(),
            "throttle source case {case}"
        );
        assert!(shared.timers.borrow().is_empty(), "fired timers released");
    }
}
#[test]
fn throttle_preserves_callback_error_identity_and_rejects_nonfunctions() {
    let (runtime, shared) = fixture();
    let factory = shared.globals.get("throttle").unwrap();
    assert_eq!(
        factory
            .call(Js::Undefined, vec![Js::Number(1.)])
            .unwrap_err()
            .message(),
        "Expected a function"
    );
    let error = Js::object(vec![("sentinel", Js::Bool(true))]);
    let thrown = error.clone();
    let f = factory
        .call(
            Js::Undefined,
            vec![
                Js::function("throws", move |_, _| Err(Thrown::new(thrown.clone()))),
                Js::Number(10.),
            ],
        )
        .unwrap();
    assert!(
        f.call(Js::Undefined, vec![])
            .unwrap_err()
            .value
            .same_reference(&error)
    );
    f.call_method("cancel", vec![]).unwrap();
    runtime.drain();
    assert!(shared.timers.borrow().is_empty());
}
