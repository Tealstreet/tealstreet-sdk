use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
    websocket::{WebSocketConnectionOptions, WsEvent, WsRuntime, WsSocket},
};
use safe_cex_store::store::DefaultStore;
use safe_cex_transport::{
    RestAdapter, RestFuture, RestRequest, RestResponse, RestResponseBody, TransportProvider,
    socket::TunnelHost,
};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use safe_cex_venue_blofin::native_exchange::BlofinNativeExchange;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::Arc,
    task::{Context, Poll, Waker},
};
type Job = (Pin<Box<dyn Future<Output = ()>>>, Rc<Cell<bool>>);
pub struct Runtime {
    pub now: Rc<Cell<f64>>,
    pub jobs: RefCell<Vec<Job>>,
    pub errors: RefCell<Vec<String>>,
    pub dials: RefCell<Vec<String>>,
    pub opened: RefCell<Vec<Rc<Socket>>>,
    pub open: Cell<bool>,
}
struct Task(Rc<Cell<bool>>);
impl ExchangeTask for Task {
    fn cancel(&self) {
        self.0.set(true);
    }
}
impl Runtime {
    pub fn pump(&self) {
        let jobs = std::mem::take(&mut *self.jobs.borrow_mut());
        let mut pending = vec![];
        for (mut f, c) in jobs {
            if !c.get()
                && f.as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            {
                pending.push((f, c));
            }
        }
        self.jobs.borrow_mut().extend(pending);
    }
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        self.now.get()
    }
    fn now_ms(&self) -> f64 {
        self.now.get()
    }
    fn sleep(&self, d: u64) -> ExchangeFuture<'static, ()> {
        let at = self.now.get() + d as f64;
        let now = self.now.clone();
        Box::pin(std::future::poll_fn(move |_| {
            if now.get() >= at {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        }))
    }
    fn spawn_local(&self, f: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        let c = Rc::new(Cell::new(false));
        self.jobs.borrow_mut().push((f, c.clone()));
        Box::new(Task(c))
    }
    fn console(&self, _: &str, _: &str) {}
    fn unique_id(&self) -> String {
        "nanoid".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.375
    }
    fn report_raw_exception(&self, _: &Value, _: &Value) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, e: &ExchangeError, _: &Value) {
        self.errors.borrow_mut().push(e.message.clone());
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        Box::pin(async { Err(ExchangeError::new("Worker unused in native fixture")) })
    }
}
impl WsRuntime for Runtime {
    fn transport(&self) -> Arc<dyn TransportProvider> {
        Arc::new(safe_cex_transport::DirectTransport)
    }
    fn tunnel_host(&self) -> Rc<dyn TunnelHost> {
        panic!("fixture refuses tunnel transport")
    }
    fn dial(
        &self,
        url: String,
        _: Vec<String>,
        _: WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        self.dials.borrow_mut().push(url);
        if self.open.get() {
            let socket = Rc::new(Socket::default());
            socket.events.borrow_mut().push_back(WsEvent::Open);
            self.opened.borrow_mut().push(socket.clone());
            Box::pin(async move { Ok(socket as Rc<dyn WsSocket>) })
        } else {
            Box::pin(std::future::pending())
        }
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
        3000.0
    }
    fn diagnostics_enabled(&self) -> bool {
        false
    }
    fn lifecycle_logs_enabled(&self) -> bool {
        false
    }
}
/// Scripted venue: responses keyed by path (query stripped); unknown paths answer an empty page.
#[derive(Default)]
pub struct Wire {
    pub requests: RefCell<Vec<RestRequest>>,
    pub bodies: RefCell<BTreeMap<String, Value>>,
}
pub fn path(url: &str) -> &str {
    url.split('?').next().unwrap_or(url)
}
impl RestAdapter for Wire {
    fn execute(&self, r: RestRequest) -> RestFuture<'_, RestResponse> {
        self.requests.borrow_mut().push(r.clone());
        // Active-order pagination completes on the page after a cursor.
        let paged = r.params.as_ref().is_some_and(|p| !p["after"].is_null());
        let body = if paged {
            json!({"code":"0","msg":"success","data":[]})
        } else {
            self.bodies
                .borrow()
                .get(path(&r.url))
                .cloned()
                .unwrap_or(json!({"code":"0","msg":"success","data":[]}))
        };
        Box::pin(async move {
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: BTreeMap::new(),
                body: RestResponseBody::Untransformed(
                    safe_cex_types::retained_json::RetainedJsonInput::from_value(body).unwrap(),
                ),
            })
        })
    }
}
pub fn fixture() -> (
    Rc<BlofinNativeExchange>,
    Rc<Runtime>,
    Rc<DefaultStore>,
    Rc<Wire>,
) {
    let runtime = Rc::new(Runtime {
        now: Rc::new(Cell::new(1_710_000_000_000.0)),
        jobs: RefCell::default(),
        errors: RefCell::default(),
        dials: RefCell::default(),
        opened: RefCell::default(),
        open: Cell::new(false),
    });
    let store = Rc::new(DefaultStore::new());
    let opts: ExchangeOptions =
        serde_json::from_value(json!({"key":"k","secret":"s","passphrase":"p"})).unwrap();
    let base = BaseExchange::new(
        ExchangeName::Blofin,
        opts,
        store.clone(),
        runtime.clone(),
        ExchangeSettings::default(),
    );
    let wire = Rc::new(Wire::default());
    let adapter = wire.clone();
    let venue = BlofinNativeExchange::new(
        base,
        move |_, base_url, budget| {
            assert_eq!(base_url, "https://openapi.blofin.com", "livenet REST base");
            assert_eq!(budget, 20_000, "createAPI timeout budget");
            Ok(adapter.clone() as Rc<dyn RestAdapter>)
        },
        runtime.clone(),
        Rc::new(|_, _| panic!("fixture has no external public data")),
        None,
    )
    .unwrap();
    (venue, runtime, store, wire)
}

#[derive(Default)]
pub struct Socket {
    pub events: Rc<RefCell<std::collections::VecDeque<WsEvent>>>,
    pub sent: RefCell<Vec<String>>,
}
impl WsSocket for Socket {
    fn ready_state(&self) -> u8 {
        1
    }
    fn next_event(&self) -> ExchangeFuture<'static, WsEvent> {
        let events = self.events.clone();
        Box::pin(std::future::poll_fn(move |_| {
            events
                .borrow_mut()
                .pop_front()
                .map_or(Poll::Pending, |e| Poll::Ready(Ok(e)))
        }))
    }
    fn send(&self, message: &str) -> Result<(), ExchangeError> {
        self.sent.borrow_mut().push(message.into());
        Ok(())
    }
    fn close(&self) {}
    fn terminate(&self) {}
    fn reconnect(&self) {}
    fn is_reconnecting(&self) -> bool {
        false
    }
}
