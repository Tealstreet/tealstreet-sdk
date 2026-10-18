//! Structural alias witnesses through a constructed LighterNativeExchange.
use super::*;
use num_bigint::BigInt;
use safe_cex_exchange::{
    js_runtime::{
        client::JsHttpHost,
        promise::JsFuture,
        value::from_json,
    },
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
    websocket::{WebSocketConnectionOptions, WsRuntime, WsSocket},
};
use safe_cex_store::Store;
use safe_cex_transport::{TransportProvider, TransportRegistry, socket::TunnelHost};
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, Waker},
};

#[derive(Default)]
struct Runtime {
    now: Cell<f64>,
    logs: RefCell<Vec<String>>,
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
        self.now.get()
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        Box::pin(async { Ok(()) })
    }
    fn spawn_local(&self, future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        self.jobs.borrow_mut().push(future);
        Box::new(Task)
    }
    fn console_values(&self, severity: &str, args: &[Js]) {
        self.logs.borrow_mut().push(format!("{severity}: {}", safe_cex_exchange::runtime::console_text(args)));
    }
    fn unique_id(&self) -> String {
        "id".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_exception(
        &self,
        _: &ExchangeError,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
    }
    fn report_raw_exception(
        &self,
        _: &safe_cex_exchange::js_runtime::value::Js,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        Box::pin(async { Ok(Value::Null) })
    }
}
impl WsRuntime for Runtime {
    fn transport(&self) -> Arc<dyn TransportProvider> {
        Arc::new(safe_cex_transport::DirectTransport)
    }
    fn tunnel_host(&self) -> Rc<dyn TunnelHost> {
        panic!("no tunnel in sdk client tests")
    }
    fn dial(
        &self,
        _: String,
        _: Vec<String>,
        _: WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        panic!("no sockets in sdk client tests")
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

/// Records every wire config and answers with the next scripted `data`.
struct Http {
    runtime: Rc<Runtime>,
    sent: RefCell<Vec<Js>>,
    replies: RefCell<VecDeque<Js>>,
}
impl JsHttpHost for Http {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.runtime.clone()
    }
    fn send(&self, config: Js) -> JsFuture<'static> {
        self.sent.borrow_mut().push(config);
        let data = self
            .replies
            .borrow_mut()
            .pop_front()
            .expect("scripted reply");
        Box::pin(async move {
            Ok(Js::object(vec![
                ("status", Js::Number(200.0)),
                ("data", data),
            ]))
        })
    }
}

struct World {
    runtime: Rc<Runtime>,
    http: Rc<Http>,
    venue: Rc<LighterNativeExchange>,
}
impl World {
    fn new() -> Self {
        let runtime = Rc::new(Runtime::default());
        runtime.now.set(1_700_000_000_000.0);
        let http = Rc::new(Http { runtime: runtime.clone(), sent: RefCell::default(), replies: RefCell::default() });
        let options = serde_json::from_value(json!({"key":"", "secret":"01000000000000000000000000000000000000000000000000000000000000000000000000000000", "apiKeyIndex":"3", "applicationId":"42"})).unwrap();
        let venue = LighterNativeExchange::new(ExchangeName::Lighter, options,
            Rc::new(safe_cex_store::store::DefaultStore::new()), LighterDependencies {
                runtime: runtime.clone(), http: http.clone(), registry: Rc::new(TransportRegistry::default()),
                signing_entropy: Some(Rc::new(|bytes| { bytes.fill(0x42); Ok(()) })),
            }).expect("actual parent constructor");
        Self { runtime, http, venue }
    }
    fn member(&self, name: &str) -> Js { self.venue.this.borrow().get(name).unwrap() }
    fn settle(&self, value: Js) -> Result<Js, Thrown> {
        let runtime: Rc<dyn ExchangeRuntime> = self.runtime.clone();
        let mut future = Box::pin(await_js(&runtime, value));
        for _ in 0..128 {
            if let Poll::Ready(value) = future.as_mut().poll(&mut Context::from_waker(Waker::noop())) { return value; }
            let jobs = std::mem::take(&mut *self.runtime.jobs.borrow_mut());
            for mut job in jobs {
                if job.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending() { self.runtime.jobs.borrow_mut().push(job); }
            }
        }
        panic!("native promise did not settle");
    }
}

#[test]
fn structural_alias_query_keeps_native_values_through_real_sdk_and_http() {
    let world = World::new();
    let cursor = Js::utf16(&[0xd800, 0x61, 0xdc00]);
    let identity = Js::object(vec![("native", Js::Number(f64::INFINITY))]);
    let response = Js::object(vec![("original", identity.clone())]);
    world.http.replies.borrow_mut().push_back(Js::object(vec![
        ("success", Js::Bool(true)), ("data", response.clone()),
    ]));
    let promise = world.member("orderApi").call_method("getAccountInactiveOrders", vec![
        Js::BigInt(Rc::new(BigInt::from(42))),
        Js::object(vec![("cursor", cursor.clone()), ("sortBy", identity.clone()), ("sortOrder", Js::Number(f64::INFINITY))]),
    ]).unwrap();
    let result = world.settle(promise).unwrap();
    assert!(result.strict_equals(&response), "native response object identity");
    let sent = world.http.sent.borrow();
    assert_eq!(sent.len(), 1);
    let params = sent[0].get("params").unwrap();
    assert!(params.get("cursor").unwrap().strict_equals(&cursor), "UTF16 record cursor code units");
    assert!(params.get("sort_by").unwrap().strict_equals(&identity), "native query object identity");
    assert!(matches!(params.get("sort_order").unwrap(), Js::Number(n) if n == f64::INFINITY), "native query Infinity");
    assert_eq!(params.get("account_index").unwrap().to_display(), "42");
}

#[test]
fn structural_alias_private_warning_map_is_shared_and_keeps_cooldown() {
    let world = World::new();
    let socket = world.member("privateWebsocket");
    let class = world.venue.realm.global("LighterPrivateWebsocket").unwrap();
    let map = class.get("readonlyFallbackWarningAtByAccount").unwrap();
    let warn = || socket.call_method("emitReadonlyFallbackWarning", vec![Js::Bool(true)]).unwrap();
    warn();
    let Js::Map(times) = &map else { panic!("actual class warning map"); };
    let first = times.borrow().clone();
    assert_eq!(first.len(), 1, "first warning records one account");
    assert!(matches!(&first[0].1, Js::Number(n) if *n == world.runtime.now.get()), "class view mirrors native warning timestamp");
    let count = world.runtime.logs.borrow().len();
    warn();
    assert_eq!(world.runtime.logs.borrow().len(), count, "shared native cooldown suppresses repeated warning");
    world.runtime.now.set(world.runtime.now.get() + 30_000.0);
    warn();
    assert_eq!(world.runtime.logs.borrow().len(), count + 1, "cooldown boundary permits warning");
    assert!(class.get("readonlyFallbackWarningAtByAccount").unwrap().strict_equals(&map), "same class map identity");
    assert!(matches!(&times.borrow()[0].1, Js::Number(n) if *n == world.runtime.now.get()), "native timestamp is updated in retained view");
}
