//! js_runtime semantics a venue binding depends on, over a recording runtime.
use safe_cex_exchange::{
    js_runtime::{
        client::{self, JsHttpHost},
        promise::{JsFuture, Promise, await_js},
        value::{Js, Thrown, arg, to_json},
    },
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

#[derive(Default)]
struct Runtime {
    jobs: RefCell<Vec<Pin<Box<dyn Future<Output = ()>>>>>,
}
struct Task;
impl ExchangeTask for Task {
    fn cancel(&self) {}
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        0.0
    }
    fn now_ms(&self) -> f64 {
        0.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        Box::pin(async { Ok(()) })
    }
    fn spawn_local(&self, future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        self.jobs.borrow_mut().push(future);
        Box::new(Task)
    }
    fn console(&self, _: &str, _: &str) {}
    fn unique_id(&self) -> String {
        "id".into()
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
    runtime: Rc<Runtime>,
    sent: RefCell<Vec<Value>>,
}
impl JsHttpHost for Http {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.runtime.clone()
    }
    fn send(&self, config: Js) -> JsFuture<'static> {
        self.sent.borrow_mut().push(to_json(&config).unwrap());
        let status = if config.get("url").unwrap().to_display() == "/fail" {
            500.0
        } else {
            200.0
        };
        Box::pin(async move {
            Ok(Js::object(vec![
                ("status", Js::Number(status)),
                ("data", Js::object(vec![("ok", Js::Bool(true))])),
            ]))
        })
    }
}

fn settle(runtime: &Runtime, value: Js) -> Result<Js, Thrown> {
    let mut future = Box::pin(await_js(value));
    for _ in 0..16 {
        if let Poll::Ready(v) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return v;
        }
        let jobs = std::mem::take(&mut *runtime.jobs.borrow_mut());
        for mut job in jobs {
            if job
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
            {
                runtime.jobs.borrow_mut().push(job);
            }
        }
    }
    panic!("promise did not settle")
}

#[test]
fn client_runs_request_interceptors_lifo_and_response_interceptors_fifo() {
    let runtime = Rc::new(Runtime::default());
    let http = Rc::new(Http {
        runtime: runtime.clone(),
        sent: RefCell::default(),
    });
    let order = Rc::new(RefCell::new(vec![]));
    let xhr = client::create(
        http.clone(),
        Js::object(vec![("headers", Js::object(vec![("A", Js::str("1"))]))]),
    );
    let interceptors = xhr.get("interceptors").unwrap();
    for tag in ["first", "second"] {
        let request_order = order.clone();
        interceptors
            .get("request")
            .unwrap()
            .call_method(
                "use",
                vec![Js::function("", move |_, a| {
                    request_order.borrow_mut().push(format!("request {tag}"));
                    let config = arg(&a, 0);
                    config.get("headers")?.set(tag, Js::str(tag))?;
                    Ok(config)
                })],
            )
            .unwrap();
        let response_order = order.clone();
        interceptors
            .get("response")
            .unwrap()
            .call_method(
                "use",
                vec![Js::function("", move |_, a| {
                    response_order.borrow_mut().push(format!("response {tag}"));
                    Ok(arg(&a, 0))
                })],
            )
            .unwrap();
    }
    let response = settle(
        &runtime,
        xhr.call_method("get", vec![Js::str("/ok")]).unwrap(),
    )
    .unwrap();
    assert_eq!(
        to_json(&response.get("data").unwrap()).unwrap(),
        json!({"ok": true})
    );
    assert_eq!(
        *order.borrow(),
        [
            "request second",
            "request first",
            "response first",
            "response second"
        ]
    );
    let sent = http.sent.borrow()[0].clone();
    assert_eq!(
        sent["headers"],
        json!({"A": "1", "second": "second", "first": "first"})
    );

    let failure = settle(
        &runtime,
        xhr.call_method("get", vec![Js::str("/fail")]).unwrap(),
    )
    .unwrap_err();
    assert_eq!(failure.message(), "Request failed with status code 500");
    assert_eq!(
        to_json(
            &failure
                .value
                .get("response")
                .unwrap()
                .get("status")
                .unwrap()
        )
        .unwrap(),
        json!(500.0)
    );
}

#[test]
fn promise_bodies_start_eagerly_and_settle_through_the_runtime() {
    let runtime: Rc<dyn ExchangeRuntime> = Rc::new(Runtime::default());
    let started = Rc::new(RefCell::new(false));
    let flag = started.clone();
    let promise = Promise::spawn(
        &runtime,
        Box::pin(async move {
            *flag.borrow_mut() = true;
            Ok(Js::Number(1.0))
        }),
    );
    assert!(*started.borrow(), "body ran before anyone awaited it");
    assert!(
        promise
            .result()
            .is_some_and(|r| r.unwrap().strict_equals(&Js::Number(1.0)))
    );
}

#[test]
fn bigint_and_url_search_params_follow_ecmascript() {
    let big = Js::BigInt(Rc::new(12_345_678_901_234_567_890_u128.into()));
    assert_eq!(big.type_of(), "bigint");
    assert!(big.truthy());
    assert!(!Js::BigInt(Rc::new(0.into())).truthy());
    assert_eq!(big.to_display(), "12345678901234567890");
    assert!(big.strict_equals(&Js::BigInt(Rc::new(12_345_678_901_234_567_890_u128.into()))));
    assert_eq!(
        big.to_number().unwrap_err().message(),
        "Cannot convert a BigInt value to a number"
    );
    assert_eq!(to_json(&big).unwrap(), json!("12345678901234567890"));

    let globals = Js::object(vec![]);
    safe_cex_exchange::js_runtime::globals::install_builtins::<()>(&globals, std::rc::Weak::new());
    let params = globals
        .get("URLSearchParams")
        .unwrap()
        .call(
            Js::Undefined,
            vec![Js::object(vec![
                ("a b", Js::str("x y&z")),
                ("k", Js::str("~*")),
            ])],
        )
        .unwrap();
    assert_eq!(
        params.call_method("toString", vec![]).unwrap().to_display(),
        "a+b=x+y%26z&k=%7E*"
    );
}
