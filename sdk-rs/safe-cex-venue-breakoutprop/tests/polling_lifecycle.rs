#[path = "../../safe-cex-exchange/tests/support/runtime.rs"]
mod runtime;
use safe_cex_exchange::{
    exchange::BaseExchange,
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_num::numeric_ingress::{JsNumberInput, js_from_value};
use safe_cex_store::{Store, store::DefaultStore};
use safe_cex_transport::provider::{RestBody, RestRequest};
use safe_cex_types::{ExchangeName, OHLCVOptions, OrderBook, Ticker};
use safe_cex_venue_breakoutprop::{lifecycle::*, polling::*};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::Future,
    pin::Pin,
    rc::{Rc, Weak},
    task::{Context, Poll, Waker},
};

#[derive(Default)]
struct Clock {
    runtime: runtime::Runtime,
    hook: RefCell<Option<Rc<dyn Fn()>>>,
}
impl ExchangeRuntime for Clock {
    fn now_ms(&self) -> f64 {
        let hook = self.hook.borrow_mut().take();
        if let Some(hook) = hook {
            hook();
        }
        self.runtime.now_ms()
    }
    fn monotonic_now_ms(&self) -> f64 {
        self.runtime.monotonic_now_ms()
    }
    fn sleep(&self, d: u64) -> ExchangeFuture<'static, ()> {
        self.runtime.sleep(d)
    }
    fn spawn_local(&self, f: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        self.runtime.spawn_local(f)
    }
    fn console_values(&self, l: &str, m: &[safe_cex_exchange::js_runtime::value::Js]) {
        self.runtime.console_values(l, m);
    }
    fn unique_id(&self) -> String {
        self.runtime.unique_id()
    }
    fn is_worker(&self) -> bool {
        true
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_raw_exception(
        &self,
        _: &safe_cex_exchange::js_runtime::value::Js,
        _: &serde_json::Value,
    ) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, e: &ExchangeError, c: &Value) {
        self.runtime.report_exception(e, c);
    }
    fn execute_worker_request(&self, v: Value) -> ExchangeFuture<'static, Value> {
        self.runtime.execute_worker_request(v)
    }
}
#[derive(Default)]
struct State {
    clock: Rc<Clock>,
    events: RefCell<Vec<String>>,
    next: Cell<u32>,
    requests: RefCell<Vec<(u32, RestRequest)>>,
    replies: RefCell<VecDeque<Result<JsNumberInput, ExchangeError>>>,
    ready: Rc<Cell<bool>>,
    tasks: RefCell<Vec<ExchangeFuture<'static, ()>>>,
    reports: RefCell<Vec<ExchangeError>>,
    table: RefCell<Option<HandlerTable>>,
    parent: RefCell<Weak<BreakoutpropExchange>>,
    original_store: RefCell<Option<Rc<dyn Store>>>,
    socket_disposes: Cell<u32>,
}
struct Host(Rc<State>);
struct Client {
    state: Rc<State>,
    id: u32,
}
impl DxClient for Client {
    fn rate_limit(&self, _: f64) -> Result<(), ExchangeError> {
        Err(ExchangeError::new("fixture host owns rate limiting"))
    }
    fn request(&self, request: RestRequest) -> ExchangeFuture<'static, JsNumberInput> {
        self.state.requests.borrow_mut().push((self.id, request));
        let reply = self
            .state
            .replies
            .borrow_mut()
            .pop_front()
            .unwrap_or_else(|| Ok(input("token", "0:0:1")));
        let ready = self.state.ready.clone();
        let mut reply = Some(reply);
        Box::pin(std::future::poll_fn(move |_| {
            if ready.get() {
                Poll::Ready(reply.take().unwrap())
            } else {
                Poll::Pending
            }
        }))
    }
}
struct Ancestor(Rc<State>);
impl PollingAncestor for Ancestor {
    fn now_ms(&self) -> f64 {
        self.0.clock.now_ms()
    }
    fn cleanup_message_handler_stats(&self, topic: &str) -> Result<(), ExchangeError> {
        let table = self.0.table.borrow().clone().unwrap();
        assert_eq!(
            table
                .borrow()
                .iter()
                .find(|(k, _)| k == topic)
                .unwrap()
                .1
                .borrow()
                .len(),
            0,
            "POLL native empty live stats table"
        );
        self.0.events.borrow_mut().push(format!("cleanup:{topic}"));
        Ok(())
    }
    fn dispose(&self) -> Result<(), ExchangeError> {
        assert!(
            self.0.parent.borrow().upgrade().unwrap().base.is_disposed(),
            "CTOR native parent disposed before socket"
        );
        self.0.socket_disposes.set(self.0.socket_disposes.get() + 1);
        self.0.events.borrow_mut().push("socket-dispose".into());
        Ok(())
    }
}
impl BreakoutHost for Host {
    fn runtime_host(&self) -> Rc<dyn safe_cex_venue_breakoutprop::native_reads::NativeRuntimeHost> {
        Rc::new(Host(self.0.clone()))
    }
    fn construct_base(
        &self,
        n: ExchangeName,
        o: safe_cex_types::ExchangeOptions,
        s: Rc<dyn Store>,
    ) -> Result<BaseExchange, ExchangeError> {
        self.0.events.borrow_mut().push("base".into());
        *self.0.original_store.borrow_mut() = Some(s.clone());
        Ok(BaseExchange::new(
            n,
            o,
            s,
            self.0.clock.clone(),
            Default::default(),
        ))
    }
    fn create_api(&self, ex: Rc<BreakoutpropExchange>) -> Result<Rc<dyn DxClient>, ExchangeError> {
        let id = self.0.next.get();
        self.0.next.set(id + 1);
        assert!(
            Rc::ptr_eq(
                &ex.base.store,
                self.0.original_store.borrow().as_ref().unwrap()
            ),
            "CTOR native exact store"
        );
        if id == 1 {
            assert!(
                ex.xhr.borrow().is_some(),
                "CTOR native limited API assigned before unlimited creation"
            );
        }
        assert!(ex.public_websocket.borrow().is_none());
        assert!(ex.session_token.borrow().is_none());
        self.0.events.borrow_mut().push(format!("api:{id}"));
        Ok(Rc::new(Client {
            state: self.0.clone(),
            id,
        }))
    }
    fn rate_limit(&self, c: Rc<dyn DxClient>, r: f64) -> Result<Rc<dyn DxClient>, ExchangeError> {
        assert_eq!(
            r.to_bits(),
            5f64.to_bits(),
            "CTOR native five requests per second"
        );
        self.0.events.borrow_mut().push("limit:5".into());
        Ok(c)
    }
    fn construct_public_ancestor(
        &self,
        ex: Rc<BreakoutpropExchange>,
        table: HandlerTable,
    ) -> Result<Rc<dyn PollingAncestor>, ExchangeError> {
        assert!(ex.xhr.borrow().is_some() && ex.unlimited_xhr.borrow().is_some());
        assert!(table.borrow().is_empty(), "CTOR native fresh polling table");
        *self.0.table.borrow_mut() = Some(table);
        *self.0.parent.borrow_mut() = Rc::downgrade(&ex);
        self.0.events.borrow_mut().push("socket".into());
        Ok(Rc::new(Ancestor(self.0.clone())))
    }
    fn spawn_login(&self, f: ExchangeFuture<'static, ()>) {
        self.0.tasks.borrow_mut().push(f);
    }
    fn report_abandoned_login(&self, e: &ExchangeError) {
        self.0.reports.borrow_mut().push(e.clone());
    }
}
fn input(token: &str, timeout: &str) -> JsNumberInput {
    js_from_value(json!({"sessionToken":token,"timeout":timeout})).unwrap()
}
fn fixture() -> (Rc<State>, Rc<BreakoutpropExchange>) {
    let s = Rc::new(State::default());
    s.ready.set(true);
    let ex = BreakoutpropExchange::new(
        Rc::new(Host(s.clone())),
        ExchangeName::Breakoutprop,
        serde_json::from_value(json!({"key":"k","secret":"s"})).unwrap(),
        Rc::new(DefaultStore::new()),
    )
    .unwrap();
    (s, ex)
}
fn poll<T>(f: &mut ExchangeFuture<'static, T>) -> Poll<Result<T, ExchangeError>> {
    f.as_mut().poll(&mut Context::from_waker(Waker::noop()))
}
fn drain(s: &State) {
    for _iteration in 0..32 {
        let tasks = std::mem::take(&mut *s.tasks.borrow_mut());
        for mut f in tasks {
            match poll(&mut f) {
                Poll::Pending => s.tasks.borrow_mut().push(f),
                Poll::Ready(r) => r.unwrap(),
            }
        }
    }
}
fn finish<T>(s: &State, mut f: ExchangeFuture<'static, T>) -> Result<T, ExchangeError> {
    for _iteration in 0..32 {
        match poll(&mut f) {
            Poll::Ready(r) => return r,
            Poll::Pending => drain(s),
        }
    }
    panic!("DX native pending at iteration32")
}
fn ticker() -> Ticker {
    serde_json::from_value(json!({"id":"t","symbol":"BTC","bid":100,"ask":102,"last":101,"mark":101,"index":101,"percentage":0,"openInterest":0,"fundingRate":0,"fundingTime":0,"volume":0,"quoteVolume":0})).unwrap()
}
fn opts() -> OHLCVOptions {
    serde_json::from_value(json!({"symbol":"BTC","interval":"1m"})).unwrap()
}
fn source() -> Value {
    let o = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/polling_lifecycle_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "DX actual source {}",
        String::from_utf8_lossy(&o.stderr)
    );
    serde_json::from_slice(&o.stdout).unwrap()
}
#[test]
fn polling_live_lists_fresh_candles_same_book_and_numeric_fields() {
    let (s, ex) = fixture();
    s.clock.runtime.advance(129999);
    let socket = ex.socket().unwrap();
    assert!(
        Rc::ptr_eq(&socket.message_handlers, s.table.borrow().as_ref().unwrap()),
        "POLL native ancestor exact live table"
    );
    assert!(
        Rc::ptr_eq(&ex, &s.parent.borrow().upgrade().unwrap()),
        "POLL native ancestor exact parent"
    );
    let candles = Rc::new(RefCell::new(vec![]));
    let copy = candles.clone();
    let u1 = socket.listen_ohlcv(
        opts(),
        Rc::new(move |c| {
            copy.borrow_mut().push(c);
            Ok(())
        }),
    );
    let old = socket.handlers("poll_kline.BTC").unwrap();
    let copy = candles.clone();
    let u2 = socket.listen_ohlcv(
        opts(),
        Rc::new(move |c| {
            copy.borrow_mut().push(c);
            Ok(())
        }),
    );
    let list = socket.handlers("poll_kline.BTC").unwrap();
    assert_eq!(old.borrow().len(), 1);
    assert_eq!(
        list.borrow().len(),
        2,
        "POLL native independent subscription count"
    );
    assert!(!Rc::ptr_eq(&old, &list));
    let mut t = ticker();
    t.last = -0.;
    for cb in list.borrow().clone() {
        cb(&t).unwrap();
    }
    assert_eq!(candles.borrow().len(), 2);
    assert!(
        !Rc::ptr_eq(&candles.borrow()[0], &candles.borrow()[1]),
        "POLL native fresh candle per parser"
    );
    for c in candles.borrow().iter() {
        let c = c.borrow();
        assert_eq!(c.timestamp.to_bits(), 120f64.to_bits());
        for n in [c.open, c.high, c.low, c.close] {
            assert_eq!(
                n.to_bits(),
                (-0f64).to_bits(),
                "POLL native candle negative zero"
            );
        }
        assert_eq!(c.volume.to_bits(), 0f64.to_bits());
        assert!(c.last_side.is_none());
    }
    s.clock.runtime.advance(240001);
    t.last = 5.;
    list.borrow()[0](&t).unwrap();
    assert_eq!(candles.borrow().len(), 3);
    assert!(
        !Rc::ptr_eq(&candles.borrow()[0], &candles.borrow()[2]),
        "POLL native fresh candle on repeat parser call"
    );
    assert_eq!(
        candles.borrow()[0].borrow().open.to_bits(),
        (-0f64).to_bits()
    );
    assert_eq!(candles.borrow()[2].borrow().open.to_bits(), 5f64.to_bits());
    assert_eq!(
        candles.borrow()[2].borrow().timestamp.to_bits(),
        240f64.to_bits()
    );
    u1().unwrap();
    assert_eq!(
        list.borrow().len(),
        2,
        "POLL native retained pre-unsubscribe array"
    );
    assert_eq!(socket.handlers("poll_kline.BTC").unwrap().borrow().len(), 1);
    u2().unwrap();
    u2().unwrap();
    assert_eq!(
        s.events
            .borrow()
            .iter()
            .filter(|e| e.as_str() == "cleanup:poll_kline.BTC")
            .count(),
        2
    );
    let books: Rc<RefCell<Vec<Rc<RefCell<OrderBook>>>>> = Rc::default();
    let copy = books.clone();
    let ub = socket.listen_order_book(
        "BTC".into(),
        Rc::new(move |b| {
            copy.borrow_mut().push(b);
            Ok(())
        }),
    );
    let cb = socket.handlers("poll_book.BTC").unwrap().borrow()[0].clone();
    t.bid = -0.;
    t.ask = f64::INFINITY;
    cb(&t).unwrap();
    {
        let rows = books.borrow();
        let b = rows[0].borrow();
        assert_eq!(b.bids.len(), 2, "POLL native two bid rows");
        assert_eq!(b.asks.len(), 2, "POLL native two ask rows");
        for r in &b.bids {
            assert_eq!(r.price.to_bits(), (-0f64).to_bits());
            assert_eq!(r.amount.to_bits(), 1f64.to_bits());
            assert_eq!(r.total.to_bits(), 1f64.to_bits());
        }
        for r in &b.asks {
            assert_eq!(r.price.to_bits(), f64::INFINITY.to_bits());
            assert_eq!(r.amount.to_bits(), 1f64.to_bits());
            assert_eq!(r.total.to_bits(), 1f64.to_bits());
        }
    }
    t.bid = 99.;
    t.ask = 103.;
    cb(&t).unwrap();
    assert!(
        Rc::ptr_eq(&books.borrow()[0], &books.borrow()[1]),
        "POLL native same live book object"
    );
    assert_eq!(
        books.borrow()[0].borrow().bids[0].price.to_bits(),
        99f64.to_bits()
    );
    ub().unwrap();
    let src = source();
    assert_eq!(src["candleTimestamp"], 120);
    assert_eq!(src["negativeZero"], "8000000000000000");
    assert_eq!(src["positiveInfinity"], "7ff0000000000000");
    assert_eq!(src["cleanupCount"], 3);
}
#[test]
fn constructor_wrappers_and_dispose_preserve_order_and_failures() {
    let (s, ex) = fixture();
    assert_eq!(
        &*s.events.borrow(),
        &["base", "api:0", "limit:5", "api:1", "socket"],
        "CTOR native exact constructor order"
    );
    assert!(!Rc::ptr_eq(
        ex.xhr.borrow().as_ref().unwrap(),
        ex.unlimited_xhr.borrow().as_ref().unwrap()
    ));
    assert!(ex.account_code.borrow().is_none());
    assert!(ex.session_token_request.borrow().is_none());
    assert!(ex.recent_ws_requests.borrow().is_empty());
    assert_eq!(ex.session_token_expires_at.get().to_bits(), 0f64.to_bits());
    assert_eq!(
        ex.grafana_endpoints_every,
        [
            "accounts/{accountCode}/orders",
            "accounts/{accountCode}/orders/{orderCode}"
        ]
    );
    assert_eq!(
        ex.grafana_endpoints_periodic,
        ["accounts/{accountCode}/portfolio"]
    );
    let calls = Rc::new(Cell::new(0));
    let copy = calls.clone();
    let book_unsub = ex
        .listen_order_book(
            "BTC".into(),
            Rc::new(move |_| {
                copy.set(copy.get() + 1);
                Ok(())
            }),
        )
        .unwrap();
    let socket = ex.socket().unwrap();
    assert!(
        socket.handlers("poll_book.BTC").is_some(),
        "WRAPPER native original symbol handler exists"
    );
    assert_eq!(
        socket.handlers("poll_book.BTC").unwrap().borrow().len(),
        1,
        "WRAPPER native symbol passed unchanged"
    );
    socket.handlers("poll_book.BTC").unwrap().borrow()[0](&ticker()).unwrap();
    assert_eq!(calls.get(), 1);
    let copy = calls.clone();
    let candle_unsub = ex
        .listen_ohlcv(
            opts(),
            Rc::new(move |_| {
                copy.set(copy.get() + 1);
                Ok(())
            }),
        )
        .unwrap();
    socket.handlers("poll_kline.BTC").unwrap().borrow()[0](&ticker()).unwrap();
    assert_eq!(calls.get(), 2);
    book_unsub().unwrap();
    candle_unsub().unwrap();
    ex.dispose().unwrap();
    assert_eq!(s.socket_disposes.get(), 1);
    assert!(ex.base.is_disposed());
    let (s2, ex2) = fixture();
    let _subscription = ex2.base.store.subscribe(Box::new(|_, _| {
        Err(safe_cex_types::StoreSubscriberError::new("reset failed"))
    }));
    let err = ex2.dispose().unwrap_err();
    assert_eq!(err.message, "reset failed");
    assert_eq!(
        s2.socket_disposes.get(),
        0,
        "CTOR native base failure stops socket disposal"
    );
    assert!(ex2.base.is_disposed());
    let (s3, ex3) = fixture();
    *ex3.public_websocket.borrow_mut() = None;
    for (member, error) in [
        (
            "listenOrderBook",
            ex3.listen_order_book("BTC".into(), Rc::new(|_| Ok(())))
                .err()
                .unwrap(),
        ),
        (
            "listenOHLCV",
            ex3.listen_ohlcv(opts(), Rc::new(|_| Ok(()))).err().unwrap(),
        ),
        ("dispose", ex3.dispose().unwrap_err()),
    ] {
        assert_eq!(error.name, "TypeError");
        assert_eq!(
            error.message,
            format!("Cannot read properties of undefined (reading '{member}')")
        );
    }
    assert_eq!(s3.socket_disposes.get(), 0);
    let src = source();
    assert_eq!(
        src["ctor"],
        json!(["base", "api:0", "limit:5", "api:1", "socket"])
    );
    assert_eq!(src["wrapperCalls"], 2);
    assert_eq!(src["disposeCalls"], 1);
    assert_eq!(src["failureDisposeCalls"], 0);
}
#[test]
fn session_coalesces_expiry_retry_finally_and_reentrant_clock_values() {
    let (s, ex) = fixture();
    s.ready.set(false);
    let mut a = ex.get_session_token();
    let marker = ex.session_token_request.borrow().clone().unwrap();
    let mut b = ex.get_session_token();
    assert!(
        Rc::ptr_eq(&marker, ex.session_token_request.borrow().as_ref().unwrap()),
        "SESSION native one shared login marker"
    );
    assert_eq!(
        s.requests.borrow().len(),
        1,
        "SESSION native one coalesced login request"
    );
    assert!(poll(&mut a).is_pending() && poll(&mut b).is_pending());
    {
        let requests = s.requests.borrow();
        let (id, r) = &requests[0];
        assert_eq!(*id, 0);
        assert_eq!(r.method, "POST");
        assert_eq!(r.url, "/login");
        assert!(r.params.is_none());
        assert!(
            matches!(&r.body,Some(RestBody::Untransformed(v))if v==&json!({"domain":"default","username":"k","password":"s"}))
        );
    }
    s.ready.set(true);
    drain(&s);
    assert_eq!(poll(&mut a), Poll::Ready(Ok("token".into())));
    assert_eq!(poll(&mut b), Poll::Ready(Ok("token".into())));
    assert!(ex.session_token_request.borrow().is_none());
    assert_eq!(
        ex.session_token_expires_at.get().to_bits(),
        1000f64.to_bits()
    );
    assert_eq!(finish(&s, ex.get_session_token()).unwrap(), "token");
    assert_eq!(s.requests.borrow().len(), 1);
    let weak = Rc::downgrade(&ex);
    *s.clock.hook.borrow_mut() = Some(Rc::new(move || {
        *weak.upgrade().unwrap().session_token.borrow_mut() = Some("clock-live".into())
    }));
    assert_eq!(
        finish(&s, ex.get_session_token()).unwrap(),
        "clock-live",
        "SESSION native cache returns live token after clock"
    );
    s.clock.runtime.advance(1000);
    s.replies
        .borrow_mut()
        .push_back(Ok(input("renewed", "0:0:2")));
    assert_eq!(finish(&s, ex.get_session_token()).unwrap(), "renewed");
    drain(&s);
    assert_eq!(
        s.requests.borrow().len(),
        2,
        "SESSION native exact expiry refreshes"
    );
    assert_eq!(
        ex.session_token_expires_at.get().to_bits(),
        3000f64.to_bits()
    );
    ex.invalidate_session();
    s.replies.borrow_mut().push_back(Ok(input("", "0:0:1")));
    assert_eq!(
        finish(&s, ex.get_session_token()).unwrap_err().message,
        "No session token received"
    );
    drain(&s);
    assert!(ex.session_token_request.borrow().is_none());
    s.replies.borrow_mut().push_back(Ok(input("bad", "0:60:0")));
    assert_eq!(
        finish(&s, ex.get_session_token()).unwrap_err().message,
        "Invalid DXtrade session timeout: 0:60:0"
    );
    drain(&s);
    assert!(ex.session_token.borrow().is_none());
    let weak = Rc::downgrade(&ex);
    *s.clock.hook.borrow_mut() = Some(Rc::new(move || {
        *weak.upgrade().unwrap().session_token.borrow_mut() = Some("settle-live".into())
    }));
    s.replies
        .borrow_mut()
        .push_back(Ok(input("wire-token", "0:0:1")));
    assert_eq!(
        finish(&s, ex.get_session_token()).unwrap(),
        "settle-live",
        "SESSION native login returns live token after clock"
    );
    drain(&s);
    let src = source();
    assert_eq!(src["coalescedRequests"], 1);
    assert_eq!(src["expiry"], 3000);
    assert_eq!(src["clockLive"], "clock-live");
    assert_eq!(src["settleLive"], "settle-live");
}
#[test]
fn dropped_login_calls_continue_clear_marker_and_report_abandoned_errors() {
    let (s, ex) = fixture();
    s.ready.set(false);
    drop(ex.get_session_token());
    assert_eq!(s.requests.borrow().len(), 1);
    drain(&s);
    assert!(ex.session_token_request.borrow().is_some());
    s.ready.set(true);
    drain(&s);
    assert_eq!(ex.session_token.borrow().as_deref(), Some("token"));
    assert!(
        ex.session_token_request.borrow().is_none(),
        "SESSION native abandoned success finally clears marker"
    );
    ex.invalidate_session();
    s.ready.set(false);
    s.replies
        .borrow_mut()
        .push_back(Err(ExchangeError::new("login failed")));
    drop(ex.get_session_token());
    s.ready.set(true);
    drain(&s);
    assert!(ex.session_token_request.borrow().is_none());
    assert_eq!(
        s.reports.borrow().len(),
        1,
        "SESSION native one abandoned rejection report"
    );
    assert_eq!(s.reports.borrow()[0].message, "login failed");
    ex.invalidate_session();
    s.replies
        .borrow_mut()
        .push_back(Err(ExchangeError::new("handled")));
    assert_eq!(
        finish(&s, ex.get_session_token()).unwrap_err().message,
        "handled"
    );
    drain(&s);
    assert_eq!(
        s.reports.borrow().len(),
        1,
        "SESSION native handled rejection is not abandoned"
    );
    let src = source();
    assert_eq!(src["abandonedToken"], "token");
    assert_eq!(src["abandonedReports"], json!(["login failed"]));
}

use safe_cex_venue_breakoutprop::{
    api::*,
    runtime::{BreakoutRuntime, OrderUpdate, RuntimeReads, prepare_order},
};
fn final_source() -> Value {
    let out = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/final_runtime_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "FINAL source oracle: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn market() -> safe_cex_types::Market {
    js_from_value(json!({"id":"BTC","symbol":"BTC","base":"BTC","quote":"USD","active":true,"precision":{"price":0.1,"amount":0.01},"limits":{"amount":{"min":0,"max":100},"leverage":{"min":1,"max":20}}})).unwrap()
}
fn order(id: &str) -> safe_cex_types::Order {
    js_from_value(json!({"id":id,"status":"open","symbol":"BTC","type":"limit","side":"buy","price":99.26,"amount":1.234,"filled":0,"remaining":1.234,"reduceOnly":true,"lastUpdate":0})).unwrap()
}
fn place_opts() -> safe_cex_types::PlaceOrderOpts {
    js_from_value(json!({"symbol":"BTC","type":"limit","side":"buy","amount":1.234,"price":99.26,"reduceOnly":true})).unwrap()
}
struct Reads {
    state: Rc<State>,
    external: Cell<bool>,
    markets: RefCell<Vec<safe_cex_types::Market>>,
    tickers: RefCell<Option<Vec<Ticker>>>,
    next: RefCell<Vec<Rc<dyn Fn() -> ExchangeFuture<'static, ()>>>>,
    delays: RefCell<Vec<Option<f64>>>,
    portfolio: RefCell<Option<JsNumberInput>>,
    ticker_hook: RefCell<Option<Rc<dyn Fn()>>>,
}
impl RuntimeReads for Reads {
    fn init_external_public_data(&self) -> ExchangeFuture<'static, bool> {
        self.state.events.borrow_mut().push("external".into());
        let v = self.external.get();
        Box::pin(async move { Ok(v) })
    }
    fn get_account_code(&self) -> ExchangeFuture<'static, String> {
        self.state.events.borrow_mut().push("account".into());
        Box::pin(async { Ok("A".into()) })
    }
    fn fetch_markets(&self) -> ExchangeFuture<'static, Vec<safe_cex_types::Market>> {
        self.state.events.borrow_mut().push("markets".into());
        let v = self.markets.borrow().clone();
        Box::pin(async move { Ok(v) })
    }
    fn fetch_tickers(&self) -> ExchangeFuture<'static, Option<Vec<Ticker>>> {
        self.state.events.borrow_mut().push("tickers".into());
        let hook = self.ticker_hook.borrow_mut().take();
        if let Some(hook) = hook {
            hook();
        }
        let v = self.tickers.borrow().clone();
        Box::pin(async move { Ok(v) })
    }
    fn fetch_portfolio_raw(&self) -> ExchangeFuture<'static, Option<JsNumberInput>> {
        self.state.events.borrow_mut().push("portfolio".into());
        let v = self.portfolio.borrow().clone();
        Box::pin(async move { Ok(v) })
    }
    fn local_to_utc(&self, n: f64) -> f64 {
        n
    }
    fn uuid(&self) -> String {
        self.state.events.borrow_mut().push("uuid".into());
        "fixed".into()
    }
    fn spawn_order(&self, f: ExchangeFuture<'static, ()>) {
        self.state.tasks.borrow_mut().push(f);
    }
    fn schedule_loop(
        &self,
        next: Rc<dyn Fn() -> ExchangeFuture<'static, ()>>,
        delay: Option<f64>,
    ) -> Result<(), ExchangeError> {
        self.state.events.borrow_mut().push("loop".into());
        self.next.borrow_mut().push(next);
        self.delays.borrow_mut().push(delay);
        Ok(())
    }
}
fn context(s: Rc<State>, ex: Rc<BreakoutpropExchange>) -> (Rc<BreakoutRuntime>, Rc<Reads>) {
    *ex.session_token.borrow_mut() = Some("cached".into());
    ex.session_token_expires_at.set(10000.);
    let reads = Rc::new(Reads {
        state: s,
        external: Cell::new(false),
        markets: RefCell::new(vec![market()]),
        tickers: RefCell::new(Some(vec![ticker()])),
        next: RefCell::new(vec![]),
        delays: RefCell::new(vec![]),
        portfolio: RefCell::new(None),
        ticker_hook: RefCell::new(None),
    });
    (
        Rc::new(BreakoutRuntime {
            exchange: ex,
            reads: reads.clone(),
        }),
        reads,
    )
}
#[derive(Default)]
struct Wire {
    trace: RefCell<Vec<String>>,
    interceptor: RefCell<std::rc::Weak<safe_cex_venue_breakoutprop::api::DxApi>>,
    requests: RefCell<Vec<RestRequest>>,
    unauthorized: Cell<u32>,
    status: Cell<f64>,
    faults: RefCell<VecDeque<(f64, bool)>>,
}
impl ApiWire for Wire {
    fn install_request_interceptor(
        &self,
        api: std::rc::Weak<safe_cex_venue_breakoutprop::api::DxApi>,
    ) -> Result<(), ExchangeError> {
        *self.interceptor.borrow_mut() = api;
        Ok(())
    }
    fn rate_limit(&self, max: f64) -> Result<(), ExchangeError> {
        self.trace.borrow_mut().push(format!("limit:{max}"));
        Ok(())
    }
    fn request(
        &self,
        c: Config,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<JsNumberInput, ApiFailure>>>> {
        if let Err(error) = self
            .interceptor
            .borrow()
            .upgrade()
            .unwrap()
            .intercept_request(&mut c.borrow_mut())
        {
            return Box::pin(async move {
                Err(ApiFailure {
                    error,
                    status: None,
                    config: None,
                })
            });
        }
        let request = c.borrow().request.clone();
        self.trace.borrow_mut().push(request.url.clone());
        self.requests.borrow_mut().push(request.clone());
        let fault = self.faults.borrow_mut().pop_front();
        let reply = if let Some((status, has_config)) = fault {
            let mut error = ExchangeError::new("guard error");
            error.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
                "EXACT"
            )));
            Err(ApiFailure {
                error,
                status: Some(status),
                config: has_config.then_some(c.clone()),
            })
        } else if request.url == "/login" {
            Ok(input("renewed", "0:0:1"))
        } else if self.unauthorized.get() > 0 {
            self.unauthorized.set(self.unauthorized.get() - 1);
            let mut error = ExchangeError::new("wire unauthorized");
            error.code = Some(safe_cex_exchange::js_runtime::value::Js::str("EXACT"));
            Err(ApiFailure {
                error,
                status: Some(self.status.get()),
                config: Some(c),
            })
        } else {
            Ok(js_from_value(json!({"orderId":"placed"})).unwrap())
        };
        Box::pin(async move { reply })
    }
}
struct ApiFixture(Rc<Wire>);
impl ApiHost for ApiFixture {
    fn create(
        &self,
        d: ApiDefaults,
        o: &safe_cex_types::ExchangeOptions,
        ex: &Rc<BreakoutpropExchange>,
    ) -> Result<Rc<dyn ApiWire>, ExchangeError> {
        assert_eq!(
            d.base_url, "https://breakout.prosp.devexperts.com/dxsca-web",
            "API native base URL"
        );
        assert_eq!(
            d.timeout.to_bits(),
            0f64.to_bits(),
            "API native explicit zero timeout"
        );
        assert_eq!(d.content_type, "application/json");
        assert_eq!(o.key, ex.base.options().key);
        let values = safe_cex_types::QueryValue::Array(vec![
            safe_cex_types::QueryValue::String("a b".into()),
            safe_cex_types::QueryValue::String("c".into()),
        ]);
        assert_eq!(
            (d.serialize_params)(&values),
            "0=a%20b&1=c",
            "API native qs profile"
        );
        self.0.trace.borrow_mut().push("create".into());
        Ok(self.0.clone())
    }
    fn install_network_retry(
        &self,
        _: &Rc<dyn ApiWire>,
        n: usize,
        reads: safe_cex_transport::retry::ReadClassification,
    ) -> Result<(), ExchangeError> {
        assert_eq!(n, 3, "API native retry count");
        for (method, url, read) in [
            ("POST", "/login", true),
            ("DELETE", "marketdata?x=1", true),
            ("GET", "/orders", true),
            ("POST", "/orders", false),
        ] {
            assert_eq!(
                matches!(
                    reads.classify(RestRequest {
                        method: method.into(),
                        url: url.into(),
                        ..Default::default()
                    }),
                    safe_cex_transport::retry::ClassifiedRequest::Read(_)
                ),
                read,
                "API native retry classification {method} {url}"
            );
        }
        self.0.trace.borrow_mut().push("retry".into());
        Ok(())
    }
    fn install_transport_adapter(
        &self,
        _: &Rc<dyn ApiWire>,
        _: &safe_cex_types::ExchangeOptions,
        _: &Rc<BreakoutpropExchange>,
    ) -> Result<(), ExchangeError> {
        self.0.trace.borrow_mut().push("adapter".into());
        Ok(())
    }
}
#[test]
fn final_api_native_auth_and_retry() {
    let (s, ex) = fixture();
    let options =
        serde_json::from_value(json!({"key":"k","secret":"s","extra":{"recvWindow":0}})).unwrap();
    let wire = Rc::new(Wire::default());
    wire.status.set(401.);
    let api = create_api(&ApiFixture(wire.clone()), options, &ex).unwrap();
    assert_eq!(&*wire.trace.borrow(), &["create", "retry"]);
    let mut config = DxConfig {
        request: RestRequest {
            url: "accounts/{accountCode}".into(),
            params: Some(json!({"accountCode":"A","keep":"B"})),
            ..Default::default()
        },
        session_retried: false,
    };
    assert_eq!(
        api.intercept_request(&mut config).unwrap_err().message,
        "No session token",
        "API native absent auth refusal"
    );
    *ex.session_token.borrow_mut() = Some("old".into());
    api.intercept_request(&mut config).unwrap();
    assert_eq!(
        config.request.url, "accounts/A",
        "API native substituted URL"
    );
    assert_eq!(config.request.params, Some(json!({"keep":"B"})));
    assert_eq!(
        config.request.headers["Coinbase-Authorization"], "DXAPI old",
        "API native direct auth key"
    );
    *ex.xhr.borrow_mut() = Some(api.clone());
    wire.unauthorized.set(1);
    let config = Rc::new(RefCell::new(config));
    finish(&s, api.request_config(config.clone())).unwrap();
    assert!(
        config.borrow().session_retried,
        "API native retry marker on error config"
    );
    assert_eq!(
        &*wire.trace.borrow(),
        &["create", "retry", "accounts/A", "/login", "accounts/A"],
        "API native relogin replay order"
    );
    assert_eq!(
        wire.requests.borrow().last().unwrap().headers["Coinbase-Authorization"],
        "DXAPI renewed"
    );
    wire.unauthorized.set(2);
    config.borrow_mut().session_retried = false;
    let error = finish(&s, api.request_config(config.clone())).unwrap_err();
    assert_eq!(observed_error_code(&error.code), Some(json!("EXACT")));
    assert!(config.borrow().session_retried);
    let mut login = DxConfig {
        request: RestRequest {
            url: "/login".into(),
            ..Default::default()
        },
        session_retried: false,
    };
    ex.invalidate_session();
    api.intercept_request(&mut login).unwrap();
    assert_eq!(
        login.request.headers["Coinbase-Authorization"],
        "DXAPI null"
    );
    for (url, status, has_config) in [
        ("/orders", 403., true),
        ("/orders", 401., false),
        ("/login", 401., true),
    ] {
        *ex.session_token.borrow_mut() = Some("cached".into());
        wire.faults.borrow_mut().push_back((status, has_config));
        let before = wire.requests.borrow().len();
        let error = finish(
            &s,
            api.request(RestRequest {
                url: url.into(),
                ..Default::default()
            }),
        )
        .unwrap_err();
        assert_eq!(error.message, "guard error");
        assert_eq!(observed_error_code(&error.code), Some(json!("EXACT")));
        assert_eq!(
            wire.requests.borrow().len(),
            before + 1,
            "API native guard refuses relogin {url} {status}"
        );
        assert_eq!(ex.session_token.borrow().as_deref(), Some("cached"));
    }
    ex.invalidate_session();
    let tunnel = create_api(
        &ApiFixture(wire.clone()),
        serde_json::from_value(
            json!({"key":"k","secret":"s","tealstreetTunnel":true,"extra":{"recvWindow":0}}),
        )
        .unwrap(),
        &ex,
    )
    .unwrap();
    tunnel.intercept_request(&mut login).unwrap();
    assert_eq!(login.request.headers["Authorization"], "DXAPI null");
    assert_eq!(
        &wire.trace.borrow()[wire.trace.borrow().len() - 3..],
        &["create", "retry", "adapter"]
    );
    assert_eq!(final_source()["apiRetryCount"], 3);
}
#[test]
fn final_cancel_batches_and_update() {
    let (s, ex) = fixture();
    let (ctx, _) = context(s.clone(), ex.clone());
    s.events.borrow_mut().clear();
    let orders = (0..11).map(|i| order(&i.to_string())).collect();
    let mut pending = ctx.cancel_orders(orders);
    s.ready.set(false);
    assert!(poll(&mut pending).is_pending());
    assert_eq!(
        s.requests.borrow().len(),
        10,
        "CANCEL native first lot cardinality"
    );
    for (i, (_, request)) in s.requests.borrow().iter().enumerate() {
        assert_eq!(request.method, "DELETE");
        assert_eq!(request.params.as_ref().unwrap()["orderCode"], i.to_string());
        assert_eq!(request.params.as_ref().unwrap()["accountCode"], "A");
    }
    s.ready.set(true);
    finish(&s, pending).unwrap();
    drain(&s);
    assert_eq!(
        s.requests.borrow().len(),
        11,
        "CANCEL native second lot after first"
    );
    s.requests.borrow_mut().clear();
    s.replies
        .borrow_mut()
        .push_back(Err(ExchangeError::new("delete failed")));
    s.ready.set(false);
    let mut pending = ctx.cancel_orders((0..11).map(|i| order(&i.to_string())).collect());
    assert!(poll(&mut pending).is_pending());
    assert_eq!(s.requests.borrow().len(), 10);
    s.ready.set(true);
    assert_eq!(finish(&s, pending).unwrap_err().message, "delete failed");
    drain(&s);
    assert_eq!(
        s.requests.borrow().len(),
        10,
        "CANCEL native no second lot after rejection"
    );
    assert!(
        s.tasks.borrow().is_empty(),
        "CANCEL native siblings settle after rejection"
    );
    // Rejection of session before siblings poll: their retained drivers still DELETE.
    s.requests.borrow_mut().clear();
    s.replies
        .borrow_mut()
        .push_back(Err(ExchangeError::new("first")));
    let pending = ctx.cancel_orders(vec![order("a"), order("b")]);
    assert!(finish(&s, pending).is_err());
    drain(&s);
    assert_eq!(
        s.requests.borrow().len(),
        2,
        "CANCEL native retained losing sibling sends DELETE"
    );
    ex.base
        .store
        .update(
            safe_cex_store::StoreUpdate {
                markets: Some(vec![market()]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
    s.requests.borrow_mut().clear();
    s.replies
        .borrow_mut()
        .push_back(Ok(js_from_value(json!({})).unwrap()));
    s.replies
        .borrow_mut()
        .push_back(Ok(js_from_value(json!({"orderId":["new",42]})).unwrap()));
    let prepared = safe_cex_venue_breakoutprop::runtime::prepare_update_order(
        &order("old"),
        OrderUpdate {
            price: Some(Some(102.16)),
            amount: Some(2.345),
        },
    );
    assert_eq!(
        prepared.price.unwrap().to_bits(),
        102.16f64.to_bits(),
        "UPDATE native new-order price"
    );
    assert_eq!(
        prepared.amount.to_bits(),
        2.345f64.to_bits(),
        "UPDATE native new-order amount"
    );
    assert_eq!(prepared.reduce_only, Some(true));
    assert_eq!(prepared.symbol, "BTC");
    let result = finish(
        &s,
        ctx.update_order(
            order("old"),
            OrderUpdate {
                price: Some(Some(102.16)),
                amount: Some(2.345),
            },
        ),
    )
    .unwrap();
    assert_eq!(
        result,
        json!(["new", 42]),
        "UPDATE native unnormalized order ID array"
    );
    assert_eq!(s.requests.borrow().len(), 2);
    assert_eq!(s.requests.borrow()[0].1.method, "DELETE");
    let requests = s.requests.borrow();
    let body = requests[1]
        .1
        .body
        .as_ref()
        .unwrap()
        .untransformed()
        .unwrap();
    assert_eq!(body["limitPrice"], 102.2, "UPDATE native patched price");
    assert_eq!(body["quantity"], 2.35, "UPDATE native patched amount");
    drop(requests);
    assert_eq!(final_source()["cancelCount"], 11);
}
#[test]
fn final_place_native_numeric_payload_and_failures() {
    let (s, ex) = fixture();
    let (ctx, reads) = context(s.clone(), ex.clone());
    let opts = place_opts();
    let m = market();
    let payload = prepare_order(&opts, &m, "fixed".into()).unwrap();
    assert_eq!(
        payload.quantity.to_bits(),
        1.23f64.to_bits(),
        "PLACE native quantity precision"
    );
    assert_eq!(
        payload.limit_price.unwrap().to_bits(),
        99.3f64.to_bits(),
        "PLACE native price precision"
    );
    assert_eq!(payload.stop_price, None);
    assert_eq!(payload.position_effect, "CLOSE");
    assert_eq!(payload.side, Some("BUY"));
    assert_eq!(payload.tif, "GTC");
    assert_eq!(payload.order_code, "tealstreet-fixed");
    let mut stop = opts.clone();
    stop.r#type = safe_cex_types::OrderType::StopLoss;
    stop.trigger_price = Some(91.24);
    stop.stop_loss = Some(1.);
    let native = prepare_order(&stop, &m, "fixed".into()).unwrap();
    assert_eq!(
        native.stop_price.unwrap().to_bits(),
        91.2f64.to_bits(),
        "PLACE native trigger precedence"
    );
    assert_eq!(native.limit_price, None);
    let mut edge = opts.clone();
    edge.amount = -0.;
    edge.price = Some(f64::INFINITY);
    let native = prepare_order(&edge, &m, "fixed".into()).unwrap();
    assert_eq!(
        native.quantity.to_bits(),
        (-0f64).to_bits(),
        "PLACE native signed zero quantity"
    );
    assert_eq!(
        native.limit_price.unwrap(),
        f64::INFINITY,
        "PLACE native infinite price before wire"
    );
    edge.r#type = safe_cex_types::OrderType::TrailingStopLoss;
    assert_eq!(
        prepare_order(&edge, &m, "fixed".into())
            .unwrap_err()
            .message,
        "Breakout Prop does not support trailing_stop_market orders"
    );
    let prefix_reports = Rc::new(Cell::new(0));
    let reports = prefix_reports.clone();
    ex.base.on(
        "error",
        Rc::new(move |_, _| {
            reports.set(reports.get() + 1);
            Ok(())
        }),
    );
    let pending = ctx.place_order(opts.clone());
    assert_eq!(
        prefix_reports.get(),
        1,
        "PLACE native missing market logs before poll"
    );
    let error = finish(&s, pending).unwrap_err();
    assert_eq!(error.message, "Market not found");
    assert!(s.requests.borrow().is_empty());
    ex.base
        .store
        .update(
            safe_cex_store::StoreUpdate {
                markets: Some(vec![m]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
    s.replies
        .borrow_mut()
        .push_back(Ok(js_from_value(json!({"orderId":"placed"})).unwrap()));
    assert_eq!(finish(&s, ctx.place_order(opts)).unwrap(), "placed");
    assert_eq!(s.requests.borrow().len(), 1);
    let requests = s.requests.borrow();
    assert_eq!(requests[0].1.params, Some(json!({"accountCode":"A"})));
    let body = requests[0]
        .1
        .body
        .as_ref()
        .unwrap()
        .untransformed()
        .unwrap();
    assert_eq!(body["instrument"], "BTC");
    assert_eq!(body["quantity"], 1.23);
    assert_eq!(body["limitPrice"], 99.3);
    assert!(body.get("stopPrice").is_none());
    drop(requests);
    let mut bad = place_opts();
    bad.price = None;
    s.events.borrow_mut().clear();
    assert_eq!(
        finish(&s, ctx.place_order(bad)).unwrap_err().message,
        "Breakout Prop limit orders require a price"
    );
    assert!(!s.events.borrow().contains(&"uuid".into()));
    assert_eq!(reads.markets.borrow().len(), 1);
    assert_eq!(final_source()["quantity"], 1.23);
}
#[test]
fn final_start_tick_native_order_and_disposal() {
    let (s, ex) = fixture();
    let (ctx, reads) = context(s.clone(), ex.clone());
    s.events.borrow_mut().clear();
    let calls = Rc::new(RefCell::new(vec![]));
    let b = calls.clone();
    let c = calls.clone();
    let socket = ex.socket().unwrap();
    socket.message_handlers.borrow_mut().push((
        "poll_book.BTC".into(),
        Rc::new(RefCell::new(vec![Rc::new(move |_| {
            b.borrow_mut().push("book");
            Ok(())
        })])),
    ));
    socket.message_handlers.borrow_mut().push((
        "poll_kline.BTC".into(),
        Rc::new(RefCell::new(vec![Rc::new(move |_| {
            c.borrow_mut().push("candle");
            Ok(())
        })])),
    ));
    finish(&s, ctx.start()).unwrap();
    assert_eq!(
        &*s.events.borrow(),
        &[
            "external",
            "account",
            "markets",
            "tickers",
            "portfolio",
            "portfolio",
            "tickers",
            "loop"
        ],
        "START native initialization call order"
    );
    assert!(ex.base.store.loaded().markets && ex.base.store.loaded().tickers);
    assert_eq!(ex.base.store.tickers()[0].bid.to_bits(), 100f64.to_bits());
    assert_eq!(
        &*calls.borrow(),
        &["book", "candle"],
        "TICK native book before candle"
    );
    assert_eq!(reads.next.borrow().len(), 1);
    // forEach captures old array identity and initial length; a table replacement
    // by the first callback must not change the already captured book list.
    let table = socket.message_handlers.clone();
    let live_calls = calls.clone();
    let old = Rc::new(RefCell::new(vec![]));
    let old_for_cb = old.clone();
    let list: HandlerList = old.clone();
    list.borrow_mut().push(Rc::new(move |_| {
        live_calls.borrow_mut().push("replace");
        old_for_cb.borrow_mut().push(Rc::new(|_| {
            panic!("TICK appended callback beyond captured length")
        }));
        table.borrow_mut()[0].1 = Rc::new(RefCell::new(vec![]));
        Ok(())
    }));
    let remaining = calls.clone();
    list.borrow_mut().push(Rc::new(move |_| {
        remaining.borrow_mut().push("old-second");
        Ok(())
    }));
    socket.message_handlers.borrow_mut()[0].1 = list;
    calls.borrow_mut().clear();
    finish(&s, ctx.tick()).unwrap();
    assert_eq!(
        &*calls.borrow(),
        &["replace", "old-second", "candle"],
        "TICK native captured old list and length"
    );
    let dispose_ex = ex.clone();
    *reads.ticker_hook.borrow_mut() = Some(Rc::new(move || dispose_ex.dispose().unwrap()));
    let before = reads.next.borrow().len();
    finish(&s, ctx.tick()).unwrap();
    assert_eq!(
        reads.next.borrow().len(),
        before,
        "TICK native disposal stops scheduling"
    );
    let pub_s = Rc::new(State::default());
    pub_s.ready.set(true);
    let pub_ex = BreakoutpropExchange::new(
        Rc::new(Host(pub_s.clone())),
        ExchangeName::Breakoutprop,
        serde_json::from_value(json!({"key":"k","secret":"s","publicDataOnly":true})).unwrap(),
        Rc::new(DefaultStore::new()),
    )
    .unwrap();
    let (pub_ctx, pub_reads) = context(pub_s.clone(), pub_ex.clone());
    pub_s.events.borrow_mut().clear();
    finish(&pub_s, pub_ctx.start()).unwrap();
    assert_eq!(
        &*pub_s.events.borrow(),
        &["external", "markets", "tickers"],
        "START native public-only skips account portfolio loop"
    );
    assert!(
        pub_ex.base.store.loaded().balance
            && pub_ex.base.store.loaded().positions
            && pub_ex.base.store.loaded().orders
    );
    assert!(pub_reads.next.borrow().is_empty());
    finish(&pub_s, pub_ctx.tick()).unwrap();
    assert!(pub_reads.next.borrow().is_empty());
    assert_eq!(final_source()["delivery"], json!(["book", "candle"]));
}
fn portfolio_input() -> JsNumberInput {
    js_from_value(json!({"portfolios":[{"account":"A","version":1,"balances":[{"account":"A","version":1,"currency":"USD","value":50.25}],"positions":[],"orders":[]}]})).unwrap()
}
#[test]
fn final_portfolio_and_notification_cutovers() {
    let (s, ex) = fixture();
    let (ctx, reads) = context(s.clone(), ex.clone());
    *reads.portfolio.borrow_mut() = Some(portfolio_input());
    finish(&s, ctx.start()).unwrap();
    assert_eq!(
        ex.base.store.balance().total.to_bits(),
        50.25f64.to_bits(),
        "START native formatted balance"
    );
    assert_eq!(ex.base.store.balance().free.to_bits(), 50.25f64.to_bits());
    assert_eq!(ex.base.store.balance().used.to_bits(), 0f64.to_bits());
    assert_eq!(
        ex.base.store.positions().len(),
        1,
        "START native missing-symbol position count"
    );
    assert_eq!(ex.base.store.positions()[0].symbol, "BTC");
    assert_eq!(
        ex.base.store.positions()[0].contracts.to_bits(),
        0f64.to_bits()
    );
    assert!(ex.base.store.orders().is_empty());
    let loaded = ex.base.store.loaded();
    assert!(loaded.balance && loaded.positions && loaded.orders);
    drop(loaded);
    let (fail_s, fail_ex) = fixture();
    let (fail_ctx, fail_reads) = context(fail_s.clone(), fail_ex.clone());
    let mut original = ExchangeError::new("notification failed");
    original.name = "CustomError".into();
    original.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
        "SUB"
    )));
    original.response = Some(json!({"data":{"msg":"source message"}}));
    original.context = Some(json!({"trace":42}));
    let expected = original.clone();
    let _unsub = fail_ex
        .base
        .store
        .subscribe(Box::new(move |_, _| Err(original.clone().into())));
    fail_s.events.borrow_mut().clear();
    let error = finish(&fail_s, fail_ctx.start()).unwrap_err();
    assert_eq!(error.message, expected.message);
    assert_eq!(error.name, expected.name);
    assert_eq!(
        observed_error_code(&error.code),
        observed_error_code(&expected.code)
    );
    assert_eq!(error.response, expected.response);
    assert_eq!(error.context, expected.context);
    assert_eq!(
        error.store_subscriber_ids.len(),
        1,
        "START native subscriber metadata preserved"
    );
    assert_eq!(
        &*fail_s.events.borrow(),
        &["external", "account", "markets"],
        "START native failed update aborts later reads"
    );
    assert!(fail_reads.next.borrow().is_empty());
    finish(&fail_s, fail_ctx.tick()).unwrap();
    assert_eq!(
        fail_reads.next.borrow().len(),
        1,
        "TICK native caught update error schedules next loop"
    );
    let mut report_error = ExchangeError::new("reporter failed");
    report_error.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
        "REPORT"
    )));
    let reporter = report_error.clone();
    fail_ex
        .base
        .on("error", Rc::new(move |_, _| Err(reporter.clone())));
    assert_eq!(
        finish(&fail_s, fail_ctx.tick()).unwrap_err(),
        report_error,
        "TICK native reporter failure propagated intact"
    );
    assert_eq!(
        fail_reads.next.borrow().len(),
        1,
        "TICK native reporter failure prevents loop"
    );
    assert_eq!(final_source()["balance"], 50.25);
}
struct NativeHost(Rc<Reads>);
impl safe_cex_venue_breakoutprop::native_reads::NativeRuntimeHost for NativeHost {
    fn create_external(
        &self,
        _: &safe_cex_types::ExchangeOptionsExternalPublicData,
    ) -> Rc<dyn safe_cex_exchange::external::ExternalPublicData> {
        panic!("NATIVE no-config fixture must not construct external provider")
    }
    fn local_to_utc(&self, n: f64) -> f64 {
        n
    }
    fn uuid(&self) -> String {
        self.0.uuid()
    }
    fn spawn_order(&self, f: ExchangeFuture<'static, ()>) {
        self.0.spawn_order(f)
    }
    fn schedule_loop(
        &self,
        next: Rc<dyn Fn() -> ExchangeFuture<'static, ()>>,
        delay: Option<f64>,
    ) -> Result<(), ExchangeError> {
        self.0.schedule_loop(next, delay)
    }
}
#[test]
fn final_native_read_bindings() {
    let (s, ex) = fixture();
    let (_, reads) = context(s.clone(), ex.clone());
    let native = Rc::new(safe_cex_venue_breakoutprop::native_reads::NativeReads {
        exchange: ex.clone(),
        host: Rc::new(NativeHost(reads)),
    });
    *ex.account_code.borrow_mut() = Some("A".into());
    assert_eq!(finish(&s, native.get_account_code()).unwrap(), "A");
    assert!(s.requests.borrow().is_empty());
    assert!(!finish(&s, native.init_external_public_data()).unwrap());
    s.replies.borrow_mut().push_back(Ok(js_from_value(json!({"events":[{"symbol":"BTC","type":"Quote","ask":102,"bid":100,"time":"2024-01-01T00:00:00Z"}]})).unwrap()));
    ex.base
        .store
        .update(
            safe_cex_store::StoreUpdate {
                markets: Some(vec![market()]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
    let tickers = finish(&s, native.fetch_tickers()).unwrap().unwrap();
    assert_eq!(tickers.len(), 1, "NATIVE decoded quote count");
    assert_eq!(tickers[0].bid.to_bits(), 100f64.to_bits());
    assert_eq!(tickers[0].ask.to_bits(), 102f64.to_bits());
    assert_eq!(s.requests.borrow()[0].1.url, "marketdata");
    s.replies.borrow_mut().push_back(Ok(portfolio_input()));
    let input = finish(&s, native.fetch_portfolio_raw()).unwrap().unwrap();
    let decoded: safe_cex_venue_breakoutprop::types::PortfolioResponse = input.decode().unwrap();
    assert_eq!(
        decoded.portfolios[0].balances[0].value.to_bits(),
        50.25f64.to_bits(),
        "NATIVE decoded portfolio value"
    );
    assert_eq!(
        s.requests.borrow()[1].1.url,
        "accounts/{accountCode}/portfolio"
    );
    s.replies
        .borrow_mut()
        .push_back(Ok(js_from_value(json!({"instruments":[]})).unwrap()));
    s.replies
        .borrow_mut()
        .push_back(Ok(js_from_value(json!({"instrumentDetails":[]})).unwrap()));
    let markets = finish(&s, native.fetch_markets()).unwrap();
    assert_eq!(
        markets.len(),
        1,
        "NATIVE empty combined fallback uses Store"
    );
    assert_eq!(markets[0].symbol, "BTC");
    assert_eq!(
        s.requests.borrow().len(),
        4,
        "NATIVE two raw instrument requests"
    );
}

impl safe_cex_venue_breakoutprop::native_reads::NativeRuntimeHost for Host {
    fn create_external(
        &self,
        _: &safe_cex_types::ExchangeOptionsExternalPublicData,
    ) -> Rc<dyn safe_cex_exchange::external::ExternalPublicData> {
        panic!("constructor fixture has no external source")
    }
    fn local_to_utc(&self, value: f64) -> f64 {
        value
    }
    fn uuid(&self) -> String {
        "fixed".into()
    }
    fn spawn_order(&self, future: ExchangeFuture<'static, ()>) {
        self.0.tasks.borrow_mut().push(future);
    }
    fn schedule_loop(
        &self,
        next: Rc<dyn Fn() -> ExchangeFuture<'static, ()>>,
        delay: Option<f64>,
    ) -> Result<(), ExchangeError> {
        let clock = self.0.clock.clone();
        self.0.tasks.borrow_mut().push(Box::pin(async move {
            clock.sleep(delay.unwrap_or(0.0).max(0.0) as u64).await?;
            next().await
        }));
        Ok(())
    }
}

fn observed_error_code(
    code: &Option<safe_cex_exchange::js_runtime::value::Js>,
) -> Option<serde_json::Value> {
    code.as_ref().map(|value| {
        let finite =
            safe_cex_exchange::js_runtime::value::to_json(value).expect("finite JSON code fixture");
        let text = safe_cex_num::js_json_stringify(&finite.into())
            .expect("source JSON code fixture")
            .expect("defined code fixture");
        serde_json::from_str(&text).expect("JSON code observation")
    })
}
