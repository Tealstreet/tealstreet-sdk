use safe_cex_exchange::{
    exchange::{BaseExchange, Exchange, ExchangeSettings},
    runtime::*,
    socket_lifecycle::SocketSlot,
    websocket::*,
};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_transport::{
    DirectTransport, RestAdapter, RestFuture, RestRequest, RestResponse, RestResponseBody,
    TransportProvider,
    socket::{SocketFrame, TunnelHost},
};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use safe_cex_venue_mexc::{
    exchange::MexcExchange, sockets::MexcSocketDependencies, utils::normalize_market,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::Arc,
    task::{Context, Waker},
};
type Work = Pin<Box<dyn Future<Output = ()>>>;
struct Task(Rc<RefCell<Option<Work>>>);
impl ExchangeTask for Task {
    fn cancel(&self) {
        self.0.borrow_mut().take();
    }
}
#[derive(Default)]
struct Runtime {
    tasks: RefCell<Vec<Rc<RefCell<Option<Work>>>>>,
    sleeps: RefCell<Vec<u64>>,
    timers: RefCell<Vec<(u64, Rc<Cell<bool>>)>>,
    sent: Rc<RefCell<Vec<String>>>,
    sequence: Cell<u64>,
    elapsed: Cell<f64>,
    errors: RefCell<Vec<String>>,
}
impl Runtime {
    fn turn(&self) {
        let tasks = self.tasks.borrow().clone();
        for task in tasks {
            let work = task.borrow_mut().take();
            if let Some(mut work) = work {
                if work
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
                {
                    *task.borrow_mut() = Some(work);
                }
            }
        }
    }
}
impl ExchangeRuntime for Runtime {
    fn now_ms(&self) -> f64 {
        12500.
    }
    fn monotonic_now_ms(&self) -> f64 {
        1250. + self.elapsed.get()
    }
    fn sleep(&self, delay: u64) -> ExchangeFuture<'static, ()> {
        self.sleeps.borrow_mut().push(delay);
        let fired = Rc::new(Cell::new(false));
        self.timers.borrow_mut().push((delay, fired.clone()));
        Box::pin(std::future::poll_fn(move |_| {
            if fired.get() {
                std::task::Poll::Ready(Ok(()))
            } else {
                std::task::Poll::Pending
            }
        }))
    }
    fn spawn_local(&self, mut work: Work) -> Box<dyn ExchangeTask> {
        let pending = work
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending();
        let task = Rc::new(RefCell::new(pending.then_some(work)));
        self.tasks.borrow_mut().push(task.clone());
        Box::new(Task(task))
    }
    fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {}
    fn unique_id(&self) -> String {
        let next = self.sequence.get() + 1;
        self.sequence.set(next);
        format!("fixture-{next}")
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.
    }
    fn report_raw_exception(&self, _: &safe_cex_exchange::js_runtime::value::Js, _: &serde_json::Value) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, error: &ExchangeError, _: &Value) {
        self.errors.borrow_mut().push(error.message.clone());
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("unexpected worker")
    }
}
struct Socket {
    events: RefCell<VecDeque<WsEvent>>,
    sent: Rc<RefCell<Vec<String>>>,
    closed: Cell<bool>,
}
impl WsSocket for Socket {
    fn ready_state(&self) -> u8 {
        if self.closed.get() { 3 } else { 1 }
    }
    fn next_event(&self) -> ExchangeFuture<'static, WsEvent> {
        match self.events.borrow_mut().pop_front() {
            Some(event) => Box::pin(async move { Ok(event) }),
            None => Box::pin(std::future::pending()),
        }
    }
    fn send(&self, message: &str) -> Result<(), ExchangeError> {
        self.sent.borrow_mut().push(message.into());
        Ok(())
    }
    fn close(&self) {
        self.closed.set(true);
    }
    fn terminate(&self) {
        self.closed.set(true);
    }
    fn reconnect(&self) {}
    fn is_reconnecting(&self) -> bool {
        false
    }
}
impl WsRuntime for Runtime {
    fn transport(&self) -> Arc<dyn TransportProvider> {
        Arc::new(DirectTransport)
    }
    fn tunnel_host(&self) -> Rc<dyn TunnelHost> {
        panic!("unexpected tunnel")
    }
    fn dial(
        &self,
        _: String,
        _: Vec<String>,
        _: WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        let socket: Rc<dyn WsSocket> = Rc::new(Socket {
            events: RefCell::new(VecDeque::from([WsEvent::Open])),
            sent: self.sent.clone(),
            closed: Cell::new(false),
        });
        Box::pin(async move { Ok(socket) })
    }
    fn headers_capable(&self) -> bool {
        true
    }
    fn enable_reconnecting(&self) -> bool {
        false
    }
    fn min_reconnection_delay_ms(&self) -> f64 {
        1000.
    }
    fn library_min_reconnection_delay_ms(&self) -> f64 {
        1000.
    }
    fn diagnostics_enabled(&self) -> bool {
        false
    }
    fn lifecycle_logs_enabled(&self) -> bool {
        false
    }
}
struct Response;
impl RestAdapter for Response {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        let body = if request.url.contains("/depth/") {
            br#"{"data":{"asks":[[31,4]],"bids":[[29,6]],"version":"snap-9"}}"#.to_vec()
        } else {
            b"[]".to_vec()
        };
        Box::pin(async move {
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: RestResponseBody::Encoded(body),
            })
        })
    }
}
fn venue(
    runtime: Rc<Runtime>,
    fills: Rc<RefCell<Vec<safe_cex_venue_mexc::ws_private::MexcFillEvent>>>,
) -> Rc<MexcExchange> {
    venue_with_adapter(runtime, fills, Rc::new(Response))
}
fn venue_with_adapter(
    runtime: Rc<Runtime>,
    fills: Rc<RefCell<Vec<safe_cex_venue_mexc::ws_private::MexcFillEvent>>>,
    http: Rc<dyn RestAdapter>,
) -> Rc<MexcExchange> {
    let store = Rc::new(DefaultStore::new());
    let market = normalize_market(
        json!({"symbol":"BTC_USDT","contractSize":0.5,"maxLeverage":20})
            .as_object()
            .unwrap(),
    )
    .unwrap();
    Store::update(
        store.as_ref(),
        StoreUpdate {
            markets: Some(vec![market]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let base = BaseExchange::new(
        ExchangeName::Mexc,
        serde_json::from_value::<ExchangeOptions>(json!({"key":"fixture","secret":"secret"}))
            .unwrap(),
        store,
        runtime.clone(),
        ExchangeSettings::default(),
    );
    MexcExchange::new(
        base,
        http,
        Rc::new(|v| v),
        MexcSocketDependencies {
            current_time_ms: {
                let runtime = runtime.clone();
                Rc::new(move || runtime.now_ms())
            },
            runtime,
            remote_account_name: None,
            emit_fill: Rc::new(move |fill| {
                fills.borrow_mut().push(fill);
                Ok(())
            }),
        },
        Rc::new(|_, _| panic!("no externalPublicData configured")),
    )
}
#[test]
fn native_constructor_registers_both_sockets_and_private_dispatch_uses_exchange_store() {
    let runtime = Rc::new(Runtime::default());
    let fills = Rc::new(RefCell::new(Vec::new()));
    let venue = venue(runtime, fills.clone());
    let public = venue
        .base()
        .get_socket(SocketSlot::Public)
        .expect("MEXC constructor public socket");
    let private = venue
        .base()
        .get_socket(SocketSlot::Private)
        .expect("MEXC constructor private socket");
    assert!(
        !Rc::ptr_eq(&public, &private),
        "MEXC independent socket owners"
    );
    assert_eq!(
        venue.base().get_all_sockets().len(),
        2,
        "MEXC constructor fixed socket count"
    );
    assert!(
        private.treat_any_message_as_pong.get(),
        "MEXC private treat-any-message pong"
    );
    assert!(
        public.treat_any_message_as_pong.get(),
        "MEXC public treat-any-message pong"
    );
    private.on_message(&SocketFrame::Text(json!({"channel":"push.personal.order.deal","data":{"id":"fill-7","orderId":"order-3","symbol":"BTC_USDT","side":1,"vol":6,"price":21,"fee":2,"profit":5,"role":1,"createTime":1000}}).to_string())).unwrap();
    let fills = fills.borrow();
    assert_eq!(
        fills.len(),
        1,
        "MEXC registered private dispatch emits once"
    );
    assert_eq!(fills[0].id, "fill-7", "MEXC registered native fill id");
    assert_eq!(
        fills[0].order_id, "order-3",
        "MEXC registered native fill order id"
    );
    assert_eq!(
        fills[0].symbol, "BTCUSDT",
        "MEXC registered native market symbol"
    );
    assert_eq!(
        fills[0].amount.to_bits(),
        3_f64.to_bits(),
        "MEXC registered market contract-size dependency"
    );
}
#[test]
fn registered_public_open_starts_8000ms_heartbeat_and_reconciles_tickers() {
    let runtime = Rc::new(Runtime::default());
    let venue = venue(runtime.clone(), Rc::default());
    let public = venue.base().get_socket(SocketSlot::Public).unwrap();
    public.connect_and_subscribe(false).unwrap();
    runtime.turn();
    let frames = runtime.sent.borrow().clone();
    assert_eq!(
        frames.len(),
        3,
        "MEXC public open ping reconcile resubscribe frame floor"
    );
    assert_eq!(
        frames[0], r#"{"method":"ping"}"#,
        "MEXC public open first ping"
    );
    assert_eq!(
        runtime
            .sleeps
            .borrow()
            .iter()
            .filter(|delay| **delay == 8000)
            .count(),
        1,
        "MEXC public fixed heartbeat period"
    );
    assert_eq!(
        frames[1], r#"{"method":"sub.ticker","param":{"symbol":"BTC_USDT"}}"#,
        "MEXC public reconcile subscribes market"
    );
    assert_eq!(
        frames[2], frames[1],
        "MEXC source resubscribe follows reconciliation"
    );
    public.dispose();
    venue
        .base()
        .set_active_ticker_symbols(&["ETH/USDT".into()])
        .unwrap();
    assert_eq!(
        runtime.sent.borrow().len(),
        3,
        "MEXC disposed listener sends nothing"
    );
    assert!(
        runtime.errors.borrow().is_empty(),
        "MEXC public assembly background failures"
    );
}

#[test]
fn registered_public_ticker_route_commits_before_original_subscriber_throw() {
    let venue = venue(Rc::default(), Rc::default());
    let public = venue.base().get_socket(SocketSlot::Public).unwrap();
    public
        .on_message(&SocketFrame::Text(r#"{"channel":"rs.ping"}"#.into()))
        .unwrap();
    let market = venue.base().store.markets()[0].clone();
    let ticker = safe_cex_venue_mexc::utils::normalize_ticker(
        json!({"symbol":"BTC_USDT","lastPrice":17,"bid1":16,"ask1":18})
            .as_object()
            .unwrap(),
        &market,
        &|v| v,
    )
    .unwrap();
    venue
        .base()
        .store
        .update(
            StoreUpdate {
                tickers: Some(vec![ticker]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
    let first = Rc::new(Cell::new(0));
    let later = Rc::new(Cell::new(0));
    let mut original = ExchangeError::new("public subscriber origin");
    original.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(313)));
    let failure = original.clone();
    let calls = first.clone();
    let _first = venue.base().store.subscribe(Box::new(move |_, _| {
        calls.set(calls.get() + 1);
        Err(failure.clone().into())
    }));
    let calls = later.clone();
    let _later = venue.base().store.subscribe(Box::new(move |_, _| {
        calls.set(calls.get() + 1);
        Ok(())
    }));
    let error=public.on_message(&SocketFrame::Text(json!({"channel":"push.ticker","symbol":"BTC_USDT","data":{"lastPrice":29,"bid1":28,"ask1":30}}).to_string())).unwrap_err();
    assert_eq!(
        venue.base().store.tickers()[0].last.to_bits(),
        29_f64.to_bits(),
        "MEXC public route committed native price"
    );
    assert_eq!(first.get(), 1, "MEXC public route first subscriber floor");
    assert_eq!(later.get(), 0, "MEXC public route later subscriber cutoff");
    assert_eq!(
        observed_error_code(&error.code),
        Some(json!(313)),
        "MEXC public route original native cause code"
    );
    assert_eq!(
        error.message, original.message,
        "MEXC public route original native cause message"
    );
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/socket_assembly_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "MEXC public route SOURCE {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source,
        json!({"last":29,"first":1,"later":0,"message":"public subscriber origin","code":313}),
        "MEXC public route secondary source trace"
    );
}

#[test]
fn registered_candle_subscription_routes_native_fields_and_cleanup_unsubscribes() {
    let runtime = Rc::new(Runtime::default());
    let venue = venue(runtime.clone(), Rc::default());
    let public = venue.base().get_socket(SocketSlot::Public).unwrap();
    public.connect_and_subscribe(false).unwrap();
    let candles = Rc::new(RefCell::new(Vec::new()));
    let sink = candles.clone();
    let cleanup = Exchange::listen_ohlcv(
        venue.as_ref(),
        safe_cex_types::OHLCVOptions {
            symbol: "BTCUSDT".into(),
            interval: safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::M1),
            from: None,
            to: None,
            limit: None,
        },
        Rc::new(move |candle| sink.borrow_mut().push(candle)),
    )
    .unwrap();
    let frame=SocketFrame::Text(json!({"channel":"push.kline","symbol":"BTC_USDT","data":{"interval":"Min1","time":1700000000,"open":11,"high":13,"low":7,"close":12,"vol":5}}).to_string());
    public.on_message(&frame).unwrap();
    assert_eq!(
        candles.borrow().len(),
        1,
        "MEXC registered candle callback floor"
    );
    let candle = candles.borrow()[0].clone();
    for (actual, expected, name) in [
        (candle.timestamp, 1700000000_f64, "time"),
        (candle.open, 11., "open"),
        (candle.high, 13., "high"),
        (candle.low, 7., "low"),
        (candle.close, 12., "close"),
        (candle.volume, 5., "volume"),
    ] {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "MEXC registered candle native {name}"
        );
    }
    cleanup().unwrap();
    public.on_message(&frame).unwrap();
    assert_eq!(
        candles.borrow().len(),
        1,
        "MEXC candle cleanup removes actual callback"
    );
    assert_eq!(
        runtime.sent.borrow().last().unwrap(),
        r#"{"method":"unsub.kline","param":{"symbol":"BTC_USDT","interval":"Min1"}}"#,
        "MEXC candle cleanup actual subscription wire"
    );
}
#[test]
fn registered_book_subscription_retains_snapshot_reaction_and_depth_callback() {
    let runtime = Rc::new(Runtime::default());
    let venue = venue(runtime.clone(), Rc::default());
    let public = venue.base().get_socket(SocketSlot::Public).unwrap();
    public.connect_and_subscribe(false).unwrap();
    let books = Rc::new(RefCell::new(Vec::new()));
    let sink = books.clone();
    let cleanup = Exchange::listen_order_book(
        venue.as_ref(),
        "BTCUSDT".into(),
        Rc::new(move |book| sink.borrow_mut().push(book)),
    )
    .unwrap();
    assert_eq!(
        books.borrow().len(),
        0,
        "MEXC snapshot Ready reaction does not run inline"
    );
    runtime.turn();
    assert_eq!(
        books.borrow().len(),
        1,
        "MEXC registered book snapshot callback floor"
    );
    assert_eq!(
        books.borrow()[0].bids[0].price.to_bits(),
        29_f64.to_bits(),
        "MEXC registered book snapshot native price"
    );
    assert_eq!(
        books.borrow()[0].bids[0].amount.to_bits(),
        3_f64.to_bits(),
        "MEXC registered book snapshot native amount"
    );
    public.on_message(&SocketFrame::Text(json!({"channel":"push.depth.full","symbol":"BTC_USDT","data":{"bids":[[41,8]],"asks":[[43,10]]}}).to_string())).unwrap();
    assert_eq!(
        books.borrow().len(),
        2,
        "MEXC registered book live callback floor"
    );
    assert_eq!(
        books.borrow()[1].bids[0].price.to_bits(),
        41_f64.to_bits(),
        "MEXC registered book live native price"
    );
    assert_eq!(
        books.borrow()[1].bids[0].amount.to_bits(),
        4_f64.to_bits(),
        "MEXC registered book live native amount"
    );
    cleanup().unwrap();
    assert_eq!(
        runtime.sent.borrow().last().unwrap(),
        r#"{"method":"unsub.depth.full","param":{"symbol":"BTC_USDT","limit":20}}"#,
        "MEXC book cleanup native subscription wire"
    );
    assert!(
        runtime.errors.borrow().is_empty(),
        "MEXC book assembly background failures"
    );
}

#[test]
fn registered_private_open_suspends_ready_login_until_after_reconnected() {
    let runtime = Rc::new(Runtime::default());
    let venue = venue(runtime.clone(), Rc::default());
    let reconnected = Rc::new(Cell::new(0));
    let count = reconnected.clone();
    let sent = runtime.sent.clone();
    venue.base().on(
        "ws-reconnected",
        Rc::new(move |_, _| {
            assert_eq!(
                sent.borrow().len(),
                1,
                "MEXC Ready login await preserves reconnected prefix"
            );
            count.set(count.get() + 1);
            Ok(())
        }),
    );
    let private = venue.base().get_socket(SocketSlot::Private).unwrap();
    private.connect_and_subscribe(false).unwrap();
    assert_eq!(
        reconnected.get(),
        1,
        "MEXC private synchronous reconnected floor"
    );
    assert_eq!(
        runtime.sent.borrow().len(),
        1,
        "MEXC private Ready login is not sent inline"
    );
    assert_eq!(
        runtime.sent.borrow()[0],
        r#"{"method":"ping"}"#,
        "MEXC private open native ping"
    );
    runtime.turn();
    assert_eq!(
        runtime.sent.borrow().len(),
        2,
        "MEXC private retained login continuation sends once"
    );
    let payload: Value = serde_json::from_str(&runtime.sent.borrow()[1]).unwrap();
    assert_eq!(
        payload["method"], "login",
        "MEXC actual private login method"
    );
    assert_eq!(
        payload["param"]["apiKey"], "fixture",
        "MEXC actual private login key"
    );
    assert_eq!(
        payload["param"]["reqTime"], "12500",
        "MEXC actual private login time"
    );
    assert_eq!(
        payload["param"]["signature"],
        "e141a304bb3672663951a0522655ab3739be2a40e7039a98727f3b91f3290cb3",
        "MEXC actual private login signature"
    );
    assert_eq!(
        runtime
            .sleeps
            .borrow()
            .iter()
            .filter(|v| **v == 8000)
            .count(),
        1,
        "MEXC private fixed heartbeat period"
    );
    assert!(
        runtime.errors.borrow().is_empty(),
        "MEXC private assembly background failures"
    );
}

#[test]
fn registered_private_asset_refresh_swallows_balance_fetch_failure() {
    struct FailingBalance(Rc<Cell<usize>>);
    impl RestAdapter for FailingBalance {
        fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
            assert_eq!(
                request.url, "/api/v1/private/account/assets",
                "asset refresh uses production fetchBalance"
            );
            self.0.set(self.0.get() + 1);
            Box::pin(async move {
                Err(safe_cex_transport::RestFailure::pre_transmit(
                    "balance offline".into(),
                    request,
                ))
            })
        }
    }
    let runtime = Rc::new(Runtime::default());
    let calls = Rc::new(Cell::new(0));
    let venue = venue_with_adapter(
        runtime.clone(),
        Rc::default(),
        Rc::new(FailingBalance(calls.clone())),
    );
    let private = venue.base().get_socket(SocketSlot::Private).unwrap();
    private
        .on_message(&SocketFrame::Text(
            json!({"channel":"push.personal.asset","data":{}}).to_string(),
        ))
        .unwrap();
    assert_eq!(calls.get(), 0, "refresh waits for its source timeout");
    runtime.turn();
    assert!(
        runtime
            .timers
            .borrow()
            .iter()
            .any(|(delay, _)| *delay == 1000),
        "production refresh timeout registered"
    );
    runtime.elapsed.set(1000.0);
    for (delay, fired) in runtime.timers.borrow().iter() {
        if *delay == 1000 {
            fired.set(true);
        }
    }
    for _ in 0..4 {
        runtime.turn();
    }
    assert_eq!(
        calls.get(),
        1,
        "production asset handler fetched balance once"
    );
    assert!(
        runtime.errors.borrow().is_empty(),
        "source ignores balance refresh errors: {:?}",
        runtime.errors.borrow()
    );
    venue.dispose().unwrap();
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
