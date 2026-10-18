#[path = "support/oracle_clock.rs"]
mod oracle_clock;
#[path = "support/retained_observe.rs"]
mod retained_observe;

use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_num::numeric_ingress::{JsNumberInput, js_value_as_f64};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_types::*;
use safe_cex_venue_bingx::{
    data_reads::{BingxDataReads, ReadTransport},
    private_dispatch::*,
    public_stream::*,
    read_history::*,
    stream_subscriptions::*,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
fn corpus() -> &'static Value {
    static C: OnceLock<Value> = OnceLock::new();
    C.get_or_init(|| {
        let o = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/read_streams_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "BINGX-RS actual source {}",
            String::from_utf8_lossy(&o.stderr)
        );
        serde_json::from_slice(&o.stdout).unwrap()
    })
}
fn ready<F: Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    for _ in 0..1000 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            return v;
        }
    }
    panic!("fixture did not settle in 1000 polls")
}
fn ready_on<F: Future>(host: &Host, f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    for _ in 0..1000 {
        host.pump();
        if let Poll::Ready(value) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            return value;
        }
    }
    panic!("fixture runtime did not settle in 1000 turns")
}
fn number(n: f64) -> Value {
    json!({"number":if n.is_nan(){"NaN".into()}else{format!("{:016x}",n.to_bits())}})
}
fn observe(v: &Value) -> Value {
    match v {
        Value::Number(_) => number(js_value_as_f64(v).unwrap()),
        Value::Array(a) => json!(a.iter().map(observe).collect::<Vec<_>>()),
        Value::Object(m) => Value::Object(m.iter().map(|(k, v)| (k.clone(), observe(v))).collect()),
        _ => v.clone(),
    }
}
fn candle_value(c: &Candle) -> Value {
    assert!(c.last_side.is_none(), "BINGX-RS native candle shape");
    json!({"timestamp":number(c.timestamp),"open":number(c.open),"high":number(c.high),"low":number(c.low),"close":number(c.close),"volume":number(c.volume)})
}
fn book_value(b: &OrderBook) -> Value {
    let levels = |rows: &[OrderBookOrders]| {
        json!(rows.iter().map(|r|json!({"price":number(r.price),"amount":number(r.amount),"total":number(r.total)})).collect::<Vec<_>>())
    };
    json!({"bids":levels(&b.bids),"asks":levels(&b.asks)})
}
fn order_value(o: &Order) -> Value {
    let mut v = json!({"id":o.id,"symbol":o.symbol,"status":o.status,"type":o.r#type,"side":o.side,"price":number((o.price).to_number()),"amount":number(o.amount),"remaining":number(o.remaining),"filled":number(o.filled),"reduceOnly":o.reduce_only,"lastUpdate":number(o.last_update)});
    if let Some(b) = o.close.into_value() {
        v["close"] = json!(b);
    }
    if let Some(t) = o.time_in_force.into_value() {
        v["timeInForce"] = json!(t);
    }
    if let Some(info) = o.info.as_ref() {
        v["info"] = retained_observe::observe(info);
    }
    if let Some(t) = o.sl_trigger_type.into_value() {
        v["slTriggerType"] = json!(t);
    }
    if let Some(t) = o.tp_trigger_type.into_value() {
        v["tpTriggerType"] = json!(t);
    }
    v
}
fn execution_value(e: &Execution) -> Value {
    assert!(
        e.id.is_none()
            && e.position_side.is_none()
            && e.realized_pnl.is_none()
            && e.maker.is_none()
            && e.venue_reported_revenue.is_none()
            && e.venue_reported_revenue_asset.is_none(),
        "BINGX-RS native execution shape"
    );
    json!({"timestamp":number(e.timestamp),"symbol":e.symbol,"orderId":e.order_id.as_ref().map_or(json!({"undefined":true}),|s|json!(s)),"price":number(e.price),"qty":number(e.qty),"side":e.side,"commission":number(e.commission.unwrap()),"commissionAsset":e.commission_asset.as_ref().unwrap()})
}
type Timer = (u64, Rc<dyn Fn() -> Result<(), ExchangeError>>, bool);
type LoopCallback = Rc<dyn Fn() -> ExchangeFuture<'static, ()>>;
struct Host {
    input: Value,
    calls: RefCell<Vec<Value>>,
    reports: RefCell<Vec<String>>,
    logs: RefCell<Vec<Value>>,
    connected: Cell<bool>,
    disposed: Cell<bool>,
    ids: Cell<usize>,
    timers: RefCell<Vec<Timer>>,
    loops: RefCell<Vec<LoopCallback>>,
    tasks: RefCell<Vec<Pin<Box<dyn Future<Output = ()>>>>>,
    exceptions: RefCell<Vec<String>>,
}
impl Host {
    fn call(&self, v: Value) {
        self.calls.borrow_mut().push(v);
    }
    fn pump(&self) {
        let tasks = std::mem::take(&mut *self.tasks.borrow_mut());
        for mut task in tasks {
            if task
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
            {
                self.tasks.borrow_mut().push(task);
            }
        }
    }
    fn run_timers(&self) {
        let callbacks = self
            .timers
            .borrow()
            .iter()
            .filter(|(_, _, c)| !*c)
            .map(|(_, f, _)| f.clone())
            .collect::<Vec<_>>();
        for f in callbacks {
            f().unwrap();
        }
    }
}
impl ExchangeRuntime for Host {
    fn monotonic_now_ms(&self) -> f64 {
        1234.0
    }
    fn now_ms(&self) -> f64 {
        1234.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        panic!("unexpected sleep")
    }
    fn spawn_local(&self, f: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        struct T;
        impl ExchangeTask for T {
            fn cancel(&self) {}
        }
        self.tasks.borrow_mut().push(f);
        Box::new(T)
    }
    fn console_values(&self, severity: &str, values: &[safe_cex_exchange::js_runtime::value::Js]) {
        let rendered = safe_cex_exchange::runtime::console_text(values);
        let message = rendered.as_str();
        if severity == "error" && message.starts_with("exchange error ") {
            self.reports.borrow_mut().push(message[15..].into());
        } else if matches!(severity, "debug" | "warning") {
            let [message, observed_severity] = values else {
                panic!("BaseExchange.log console observation requires message and severity");
            };
            assert!(
                observed_severity
                    .strict_equals(&safe_cex_exchange::js_runtime::value::Js::str(severity)),
                "BaseExchange.log console severity argument must match its recorded channel"
            );
            let rendered = safe_cex_exchange::runtime::console_text(std::slice::from_ref(message));
            let message = rendered.as_str();
            self.logs.borrow_mut().push(json!([message, severity]));
        }
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
    fn report_raw_exception(
        &self,
        _: &safe_cex_exchange::js_runtime::value::Js,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(
        &self,
        e: &ExchangeError,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        self.exceptions.borrow_mut().push(e.message.clone());
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("unexpected worker")
    }
}
impl HistoryHost for Host {
    fn now_ms(&self) -> f64 {
        1740000000000.0
    }
    fn subtract_date(&self, end: f64, amount: f64, unit: &str) -> Result<f64, ExchangeError> {
        let multiplier = match unit {
            "m" => 60000.0,
            "h" => 3600000.0,
            "d" => 86400000.0,
            "w" => 604800000.0,
            _ => panic!("fixture date unit {unit}"),
        };
        Ok(safe_cex_num::js_time_clip(end - amount * multiplier))
    }
    fn local_to_utc(&self, ms: f64) -> f64 {
        ms
    }
    fn is_public_data_only(&self) -> bool {
        self.input["publicOnly"] == true
    }
    fn loop_after(&self, delay: Option<f64>, f: LoopCallback) -> Result<(), ExchangeError> {
        self.call(json!({"op":"loop","delay":delay.unwrap()}));
        self.loops.borrow_mut().push(f);
        Ok(())
    }
}
impl SubscriptionHost for Host {
    fn is_connected(&self) -> bool {
        self.connected.get()
    }
    fn set_timeout(
        &self,
        delay: f64,
        f: Rc<dyn Fn() -> Result<(), ExchangeError>>,
    ) -> Result<u64, ExchangeError> {
        let id = self.timers.borrow().len() as u64 + 1;
        self.timers.borrow_mut().push((id, f, false));
        self.call(json!({"op":"timer","delay":delay,"id":id}));
        Ok(id)
    }
    fn clear_timeout(&self, id: u64) -> Result<(), ExchangeError> {
        self.call(json!({"op":"clear","id":id}));
        self.timers
            .borrow_mut()
            .iter_mut()
            .find(|(n, _, _)| *n == id)
            .unwrap()
            .2 = true;
        Ok(())
    }
    fn cleanup_stats(&self, topic: &str) -> Result<(), ExchangeError> {
        self.call(json!({"op":"cleanupStats","topic":topic}));
        Ok(())
    }
}
impl PublicSocketHost for Host {
    fn is_disposed(&self) -> bool {
        self.disposed.get()
    }
    fn nanoid(&self) -> String {
        self.ids.set(self.ids.get() + 1);
        format!("n{}", self.ids.get())
    }
    fn monotonic_now_ms(&self) -> f64 {
        1234.0
    }
    fn iso_time(&self) -> Result<String, ExchangeError> {
        panic!("no heartbeat")
    }
    fn create_socket(&self, _: &str) -> Result<(), ExchangeError> {
        panic!("no connect")
    }
    fn add_listener(&self, _: SocketListener) -> Result<(), ExchangeError> {
        panic!("no constructor")
    }
    fn send_with_log(&self, s: &str) -> Result<(), ExchangeError> {
        self.call(json!({"op":"send","payload":serde_json::from_str::<Value>(s).unwrap()}));
        Ok(())
    }
    fn set_ping_at(&self, _: f64) -> Result<(), ExchangeError> {
        panic!("no heartbeat")
    }
    fn track_last_received(&self) -> Result<(), ExchangeError> {
        self.call(json!({"op":"track"}));
        Ok(())
    }
    fn handle_pong(&self) -> Result<(), ExchangeError> {
        self.call(json!({"op":"pong"}));
        Ok(())
    }
    fn record_stats(&self, s: &str) -> Result<(), ExchangeError> {
        self.call(json!({"op":"stats","topic":s}));
        Ok(())
    }
    fn diagnostic_receive(
        &self,
        _: &str,
        _: &dyn Fn() -> bool,
        _: &dyn Fn() -> Result<Option<String>, ExchangeError>,
    ) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn read_blob(&self, _: u64) -> ExchangeFuture<'_, String> {
        panic!("no public blob")
    }
    fn report_exception(&self, e: &ExchangeError) {
        self.exceptions.borrow_mut().push(e.message.clone());
    }
}
impl PrivateMessageHost for Host {
    fn is_disposed(&self) -> bool {
        self.disposed.get()
    }
    fn track_last_received(&self) -> Result<(), ExchangeError> {
        self.call(json!({"op":"track"}));
        Ok(())
    }
    fn read_blob(&self, _: u64) -> ExchangeFuture<'_, String> {
        self.call(json!({"op":"blob"}));
        Box::pin(async move {
            if self.input["disposeAfterBlob"] == true {
                self.disposed.set(true);
            }
            Ok(self.input["raw"].as_str().unwrap().into())
        })
    }
    fn diagnostic_receive(&self, raw: &str) -> Result<(), ExchangeError> {
        self.call(json!({"op":"diagnostic","raw":raw}));
        Ok(())
    }
    fn send_with_log(&self, raw: &str) -> Result<(), ExchangeError> {
        self.call(json!({"op":"privateSend","raw":raw}));
        Ok(())
    }
    fn handle_pong(&self) -> Result<(), ExchangeError> {
        self.call(json!({"op":"pong"}));
        Ok(())
    }
    fn record_stats(&self, s: &str) -> Result<(), ExchangeError> {
        self.call(json!({"op":"stats","topic":s}));
        Ok(())
    }
}
impl PrivateDispatchOperations for Host {
    fn rebuild(&self) -> ExchangeFuture<'_, ()> {
        Box::pin(async move {
            self.call(json!({"op":"rebuild"}));
            Ok(())
        })
    }
    fn account(
        &self,
        events: Vec<safe_cex_types::retained_json::RetainedJsonInput>,
    ) -> ExchangeFuture<'_, ()> {
        Box::pin(async move {
            self.call(json!({"op":"account","events":events}));
            Ok(())
        })
    }
    fn order(
        &self,
        events: &[safe_cex_types::retained_json::RetainedJsonInput],
    ) -> Result<(), ExchangeError> {
        self.call(json!({"op":"order","events":events}));
        Ok(())
    }
}
struct Transport {
    host: Rc<Host>,
}
impl ReadTransport for Transport {
    fn send(&self, r: safe_cex_transport::RestRequest) -> ExchangeFuture<'_, JsNumberInput> {
        let mut call = json!({"op":"GET","url":r.url});
        if let Some(p) = r.params {
            call["params"] = p;
        }
        self.host.call(call);
        let input = &self.host.input;
        let which = match r.url.as_str() {
            "/openApi/swap/v2/trade/openOrders" => "orders",
            "/openApi/swap/v3/quote/klines" => "kline",
            "/openApi/swap/v2/trade/fillHistory" => "fills",
            "/openApi/swap/v3/user/balance" => "balance",
            "/openApi/swap/v2/user/positions" => "positions",
            "/openApi/swap/v1/positionSide/dual" => "mode",
            _ => panic!("unexpected URL {}", r.url),
        };
        let result = if input["reject"] == which {
            let mut e = ExchangeError::new("network cause");
            e.response = Some(json!({"data":{"msg":"wire denied"}}));
            Err(e)
        } else {
            let value = match which {
                "orders" => input["ordersResponse"].clone(),
                "kline" => input["klineResponse"].clone(),
                "fills" => input["fillsResponse"].clone(),
                "balance" => {
                    json!({"code":0,"data":[{"asset":"USDT","balance":"10","availableMargin":"8","usedMargin":"2","unrealizedProfit":"1"}]})
                }
                "positions" => json!({"code":0,"data":[]}),
                _ => json!({"code":0,"data":{"dualSidePosition":"true"}}),
            };
            Ok(serde_json::from_value(value).unwrap())
        };
        Box::pin(std::future::ready(result))
    }
}
fn fixture(
    input: Value,
) -> (
    Rc<BingxReadHistory>,
    Rc<BingxSubscriptions>,
    Rc<DefaultStore>,
    Rc<Host>,
) {
    let store = Rc::new(DefaultStore::new());
    Store::update(
        store.as_ref(),
        StoreUpdate {
            markets: Some(serde_json::from_value(input["markets"].clone()).unwrap()),
            tickers: Some(serde_json::from_value(input["tickers"].clone()).unwrap()),
            orders: Some(serde_json::from_value(input["cached"].clone()).unwrap()),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let host = Rc::new(Host {
        connected: Cell::new(input["connected"].as_bool().unwrap()),
        disposed: Cell::new(input["disposed"].as_bool().unwrap()),
        input,
        calls: RefCell::default(),
        reports: RefCell::default(),
        logs: RefCell::default(),
        ids: Cell::new(0),
        timers: RefCell::default(),
        loops: RefCell::default(),
        tasks: RefCell::default(),
        exceptions: RefCell::default(),
    });
    let base = Rc::new(BaseExchange::new(
        ExchangeName::Bingx,
        serde_json::from_value(json!({"key":"key","secret":"secret"})).unwrap(),
        store.clone(),
        host.clone(),
        ExchangeSettings {
            tick_interval: Some(100.0),
            ..Default::default()
        },
    ));
    if host.disposed.get() {
        base.dispose().unwrap();
    }
    let transport = Rc::new(Transport { host: host.clone() });
    let reads = Rc::new(BingxDataReads::new(base, transport.clone()));
    let stream = BingxPublicStream::new(reads.clone(), host.clone());
    (
        BingxReadHistory::new(reads, transport, host.clone()),
        BingxSubscriptions::new(stream, host.clone()),
        store,
        host,
    )
}
fn state(store: &DefaultStore) -> Value {
    let l = store.loaded();
    let b = store.balance();
    json!({"orders":store.orders().iter().map(order_value).collect::<Vec<_>>(),"loaded":{"orders":l.orders,"balance":l.balance,"positions":l.positions,"markets":l.markets,"tickers":l.tickers},"balance":{"total":number(b.total),"free":number(b.free),"used":number(b.used),"upnl":number(b.upnl)}})
}
fn subscribe(
    subs: &Rc<BingxSubscriptions>,
    host: &Rc<Host>,
    method: &str,
    input: &Value,
) -> Result<Value, ExchangeError> {
    let is_book = method == "public.listenOrderBook";
    let opts: OHLCVOptions = serde_json::from_value(input["opts"].clone()).unwrap();
    let snapshots = Rc::new(RefCell::new(vec![]));
    let retained = Rc::new(RefCell::new(Vec::<Rc<RefCell<OrderBook>>>::new()));
    let candles = Rc::new(RefCell::new(vec![]));
    let register = || {
        if is_book {
            let snapshots = snapshots.clone();
            let retained = retained.clone();
            subs.listen_order_book(
                &opts.symbol,
                Rc::new(move |b| {
                    snapshots.borrow_mut().push(book_value(&b.borrow()));
                    retained.borrow_mut().push(b);
                    Ok(())
                }),
            )
        } else {
            let snapshots = snapshots.clone();
            let candles = candles.clone();
            subs.listen_ohlcv(
                &opts,
                Rc::new(move |c| {
                    let value = candle_value(&c);
                    snapshots.borrow_mut().push(value.clone());
                    candles.borrow_mut().push(value);
                    Ok(())
                }),
            )
        }
    };
    let mut cleanup = vec![register()?];
    if input["duplicate"] == true {
        cleanup.push(register()?);
    }
    if input["earlyCleanup"] == true {
        cleanup[0]()?;
    }
    if input["connected"] == false {
        host.connected.set(true);
        host.run_timers();
    }
    let topic = subs
        .stream
        .message_handlers
        .borrow()
        .first()
        .map(|(k, _)| k.clone());
    let first = if is_book {
        json!({"data":{"bids":[{"price":"2","volume":"1.25"},{"price":"1","volume":"2.5"}],"asks":[{"price":"3","volume":"4"},{"price":"4","volume":"-0"}]}})
    } else {
        json!({"data":{"klineInfosVo":[{"time":1000,"open":"1.25","high":"3","low":"-0","close":"2.5","volume":"4"}]}})
    };
    let dispatch = |frame: Value| -> Result<(), ExchangeError> {
        if let Some(topic) = &topic {
            let list = subs
                .stream
                .message_handlers
                .borrow()
                .iter()
                .find(|(k, _)| k == topic)
                .unwrap()
                .1
                .clone();
            for parser in list.borrow().iter() {
                parser(&[frame.clone()])?;
            }
        }
        Ok(())
    };
    dispatch(first.clone())?;
    if input["duplicate"] == false && input["earlyCleanup"] == false {
        let mut next = first;
        if is_book {
            next["data"]["bids"] = json!([{"price":"9","volume":"5"}]);
        } else {
            next["data"]["klineInfosVo"][0]["time"] = json!(2000);
        }
        dispatch(next)?;
    }
    if input["earlyCleanup"] == false {
        for c in cleanup {
            c()?;
        }
    }
    let topics = Value::Object(
        subs.stream
            .topics
            .borrow()
            .iter()
            .map(|(k, n)| (k.clone(), number(*n)))
            .collect(),
    );
    let handlers = Value::Object(
        subs.stream
            .message_handlers
            .borrow()
            .iter()
            .map(|(k, v)| (k.clone(), number(v.borrow().len() as f64)))
            .collect(),
    );
    let retained_values = if is_book {
        retained
            .borrow()
            .iter()
            .map(|b| book_value(&b.borrow()))
            .collect::<Vec<_>>()
    } else {
        candles.borrow().clone()
    };
    let same = if is_book && retained.borrow().len() > 1 {
        json!(Rc::ptr_eq(&retained.borrow()[0], &retained.borrow()[1]))
    } else {
        Value::Null
    };
    Ok(
        json!({"callbacks":*snapshots.borrow(),"topics":topics,"handlers":handlers,"retained":retained_values,"sameObject":same}),
    )
}
fn invoke(
    history: &Rc<BingxReadHistory>,
    subs: &Rc<BingxSubscriptions>,
    host: &Rc<Host>,
    method: &str,
    input: &Value,
) -> Result<Value, ExchangeError> {
    match method {
        "fetchOrders" => ready(history.fetch_orders())
            .map(|v| json!(v.iter().map(order_value).collect::<Vec<_>>())),
        "fetchOHLCV" => {
            ready(history.fetch_ohlcv(&serde_json::from_value(input["opts"].clone()).unwrap()))
                .map(|v| json!(v.iter().map(candle_value).collect::<Vec<_>>()))
        }
        "fetchHistoricalFills" => {
            ready(history.fetch_historical_fills(
                &serde_json::from_value(input["fillsOpts"].clone()).unwrap(),
            ))
            .map(|v| json!(v.iter().map(execution_value).collect::<Vec<_>>()))
        }
        "public.listenOHLCV" | "public.listenOrderBook" => subscribe(subs, host, method, input),
        "exchange.listenOHLCV" | "exchange.listenOrderBook" => {
            let arg = Rc::new(RefCell::new(input["opts"].clone()));
            let callback = Rc::new(());
            let original_arg = arg.clone();
            let check = |arg: Rc<RefCell<Value>>, cb: Rc<()>| {
                host.call(json!({"op":"forward","name":method.split('.').nth(1).unwrap(),"argSame":Rc::ptr_eq(&arg,&original_arg),"callbackSame":Rc::ptr_eq(&cb,&callback)}));
                if input["wrapperMode"] == 1 {
                    return Err(ExchangeError::new("operation cause"));
                }
                if input["wrapperMode"] == 3 && method == "exchange.listenOHLCV" {
                    arg.borrow_mut()["extra"] = json!("mutated");
                }
                Ok(
                    json!({"same":input["wrapperMode"]!=2,"missing":input["wrapperMode"]==2,"mutated":arg.borrow()["extra"]=="mutated"}),
                )
            };
            if method == "exchange.listenOHLCV" {
                exchange_listen_ohlcv(arg.clone(), callback.clone(), check)
            } else {
                exchange_listen_order_book(arg.clone(), callback.clone(), check)
            }
        }
        "private.onMessage" => ready(
            BingxPrivateDispatcher {
                host: host.clone(),
                operations: host.clone(),
            }
            .on_message(if input["blob"] == true {
                PublicFrame::Blob(1)
            } else {
                PublicFrame::Text(input["raw"].as_str().unwrap().into())
            }),
        )
        .map(|()| json!({"undefined":true})),
        "tick" => ready_on(
            host,
            history.tick(js_value_as_f64(&input["count"]).unwrap()),
        )
        .map(|()| json!({"undefined":true})),
        "dispose" => dispose_exchange(
            || {
                host.call(json!({"op":"baseDispose"}));
                if input["failDispose"] == "base" {
                    Err(ExchangeError::new("operation cause"))
                } else {
                    history.reads.base.dispose()
                }
            },
            || {
                host.call(json!({"op":"publicDispose"}));
                if input["failDispose"] == "public" {
                    Err(ExchangeError::new("operation cause"))
                } else {
                    Ok(())
                }
            },
            || {
                host.call(json!({"op":"privateDispose"}));
                if input["failDispose"] == "private" {
                    Err(ExchangeError::new("operation cause"))
                } else {
                    Ok(())
                }
            },
        )
        .map(|()| json!({"undefined":true})),
        _ => panic!("unknown method"),
    }
}
fn check(case: &Value) {
    let input = &case["input"];
    let (h, s, store, host) = fixture(input.clone());
    let method = case["method"].as_str().unwrap();
    let clock = oracle_clock::ClockWindow::start();
    let invocation = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        invoke(&h, &s, &host, method, input)
    }));
    assert!(
        invocation.is_ok(),
        "BINGX-RS native call completed {method}"
    );
    let clock = clock.finish();
    let mut output = match invocation.unwrap() {
        Ok(v) => json!({"ok":v}),
        Err(e) => json!({"error":{"name":e.name,"message":e.message}}),
    };
    if method == "fetchOrders" && output.get("ok").is_some() {
        clock.normalize_orders(&mut output["ok"], &case["output"]["ok"]);
    }
    let mut native_state = state(&store);
    clock.normalize_orders(&mut native_state["orders"], &case["state"]["orders"]);
    assert_eq!(
        output, case["output"],
        "BINGX-RS decoded native output {method} {}",
        case["variant"]
    );
    assert_eq!(
        native_state, case["state"],
        "BINGX-RS decoded Store {method}"
    );
    assert_eq!(
        observe(&json!(*host.calls.borrow())),
        case["calls"],
        "BINGX-RS calls {method} {}",
        case["variant"]
    );
    assert_eq!(
        *host.reports.borrow(),
        case["reports"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r[0].as_str().unwrap().to_owned())
            .collect::<Vec<_>>(),
        "BINGX-RS reports {method}"
    );
    assert_eq!(
        json!(*host.logs.borrow()),
        case["logs"],
        "BINGX-RS logs {method}"
    );
    assert!(
        host.tasks.borrow().is_empty(),
        "BINGX-RS no abandoned work {method}"
    );
    assert_eq!(
        *host.exceptions.borrow(),
        case["reports"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| format!("Exchange Error: {}", r[0].as_str().unwrap()))
            .collect::<Vec<_>>(),
        "BINGX-RS shared Base visible reports {method}"
    );
}
#[test]
fn read_streams_match_actual_typescript() {
    let names = [
        "fetchOrders",
        "fetchOHLCV",
        "fetchHistoricalFills",
        "exchange.listenOHLCV",
        "exchange.listenOrderBook",
        "public.listenOHLCV",
        "public.listenOrderBook",
        "private.onMessage",
        "tick",
        "dispose",
    ];
    assert_eq!(
        corpus()["names"],
        json!(names),
        "BINGX-RS source row census"
    );
    let cases = corpus()["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 40, "BINGX-RS fixed floor");
    for (i, c) in cases.iter().enumerate() {
        assert_eq!(c["method"], names[i / 4], "BINGX-RS source method order");
        assert_eq!(c["variant"], json!(i % 4), "BINGX-RS source variant order");
        check(c);
    }
    let extras = corpus()["extras"].as_array().unwrap();
    assert_eq!(extras.len(), 12, "BINGX-RS fixed extra floor");
    for c in extras {
        check(c);
    }
    println!("BINGX-RS runtime {}", corpus()["runtime"]);
}

#[test]
fn forwarding_and_disposal_preserve_opaque_identity() {
    let source = &corpus()["boundary"]["opaque"];
    for name in [
        "exchange.listenOHLCV",
        "exchange.listenOrderBook",
        "base",
        "public",
        "private",
    ] {
        assert_eq!(source[name], true, "BINGX-RS source opaque identity {name}");
    }
    let arg = Rc::new(RefCell::new(3));
    let callback = Rc::new(());
    let result = Rc::new(RefCell::new(7));
    let bound = |a: Rc<RefCell<i32>>, c: Rc<()>| {
        assert!(
            Rc::ptr_eq(&a, &arg) && Rc::ptr_eq(&c, &callback),
            "BINGX-RS forwarding arguments retain identity"
        );
        result.clone()
    };
    let r = exchange_listen_ohlcv(arg.clone(), callback.clone(), bound);
    assert!(
        Rc::ptr_eq(&r, &result),
        "BINGX-RS candle forwarding result identity"
    );
    let r = exchange_listen_order_book(arg.clone(), callback.clone(), bound);
    assert!(
        Rc::ptr_eq(&r, &result),
        "BINGX-RS book forwarding result identity"
    );
    let failure = Rc::new(RefCell::new(19));
    let bad = |_: Rc<RefCell<i32>>, _: Rc<()>| Err::<(), _>(failure.clone());
    assert!(
        Rc::ptr_eq(
            &exchange_listen_ohlcv(arg.clone(), callback.clone(), bad).unwrap_err(),
            &failure
        ),
        "BINGX-RS candle forwarding error identity"
    );
    assert!(
        Rc::ptr_eq(
            &exchange_listen_order_book(arg, callback, bad).unwrap_err(),
            &failure
        ),
        "BINGX-RS book forwarding error identity"
    );
    for fail in 0..3 {
        let calls = RefCell::new(vec![]);
        let op = |index| {
            calls.borrow_mut().push(index);
            if index == fail {
                Err(failure.clone())
            } else {
                Ok(())
            }
        };
        let e = dispose_exchange(|| op(0), || op(1), || op(2)).unwrap_err();
        assert!(Rc::ptr_eq(&e, &failure), "BINGX-RS disposal error identity");
        assert_eq!(
            *calls.borrow(),
            (0..=fail).collect::<Vec<_>>(),
            "BINGX-RS disposal short circuit"
        );
    }
}
#[test]
fn history_typed_boundaries_are_explicit() {
    let source = &corpus()["boundary"];
    assert_eq!(
        source["nanQuery"]["calls"][0]["params"]["startTime"],
        json!({"number":"NaN"}),
        "BINGX-RS actual source NaN range"
    );
    let mut input = corpus()["cases"][4]["input"].clone();
    input["opts"]["limit"] = json!(0);
    let (h, _, _, host) = fixture(input.clone());
    let mut opts: OHLCVOptions = serde_json::from_value(input["opts"].clone()).unwrap();
    opts.from = Some(f64::INFINITY);
    assert!(
        ready(h.fetch_ohlcv(&opts)).unwrap().is_empty(),
        "BINGX-RS native NaN range refusal result"
    );
    assert!(
        host.calls.borrow().is_empty(),
        "BINGX-RS no native NaN query"
    );
    assert_eq!(
        *host.reports.borrow(),
        vec!["BingX typed history query refuses NaN"],
        "BINGX-RS visible NaN query refusal"
    );
    assert_eq!(
        source["numericId"]["output"]["ok"][1]["orderId"],
        number(77.0),
        "BINGX-RS source numeric execution id"
    );
    let mut input = corpus()["cases"][8]["input"].clone();
    input["fillsResponse"]["data"]["fill_history_orders"][1]["orderId"] = json!(77);
    let (h, _, _, host) = fixture(input.clone());
    let out = ready(
        h.fetch_historical_fills(&serde_json::from_value(input["fillsOpts"].clone()).unwrap()),
    )
    .unwrap();
    assert_eq!(
        out.len(),
        1,
        "BINGX-RS typed execution preserves completed prefix"
    );
    assert_eq!(
        out[0].order_id.as_deref(),
        Some("iso"),
        "BINGX-RS native execution prefix id"
    );
    assert_eq!(
        host.logs.borrow().last().unwrap(),
        &json!([
            "Failed to fetch executions: BingX typed Execution orderId requires a string",
            "warning"
        ]),
        "BINGX-RS numeric id refusal visible"
    );
    assert_eq!(
        source["callableTif"], "function",
        "BINGX-RS actual source callable order TIF"
    );
    let mut input = corpus()["cases"][0]["input"].clone();
    input["ordersResponse"]["data"]["orders"][0]["timeInForce"] = json!("constructor");
    let (h, _, _, _) = fixture(input);
    assert!(
        ready(h.fetch_orders())
            .unwrap_err()
            .message
            .contains("inherited callable"),
        "BINGX-RS order TIF refusal"
    );
}
#[test]
fn stream_representation_boundaries_are_explicit() {
    let source = &corpus()["boundary"];
    assert_eq!(
        source["utfUnit"], 0xd800,
        "BINGX-RS actual source private lone surrogate"
    );
    assert_eq!(
        source["utfFrame"]["calls"][2]["raw"], "Pong",
        "BINGX-RS actual source private surrogate ping"
    );
    let mut input = corpus()["cases"][28]["input"].clone();
    input["raw"] = json!("{\"ping\":\"\\ud800\"}");
    let (_, _, _, host) = fixture(input);
    ready(
        BingxPrivateDispatcher {
            host: host.clone(),
            operations: host.clone(),
        }
        .on_message(PublicFrame::Text(
            host.input["raw"].as_str().unwrap().into(),
        )),
    )
    .expect("retained private ping accepts lone UTF16");
    assert_eq!(
        *host.calls.borrow(),
        *source["utfFrame"]["calls"].as_array().unwrap(),
        "BINGX-RS native retained ping diagnostics and Pong match source"
    );
    assert!(
        source["bookUnits"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n == 0xd800),
        "BINGX-RS source book symbol retains surrogate"
    );
    assert!(
        serde_json::from_str::<String>("\"\\ud800\"").is_err(),
        "BINGX-RS native book string admission refuses surrogate"
    );
    assert!(
        source["monthly"]["output"]["ok"]["topics"]
            .get("market.kline.BTC-USDT.1month.trade.utc+8")
            .is_some(),
        "BINGX-RS actual source monthly candle subscription"
    );
    let mut opts = corpus()["cases"][20]["input"]["opts"].clone();
    opts["interval"] = json!("1M");
    let monthly = serde_json::from_value::<OHLCVOptions>(opts)
        .expect("BINGX-RS native timeframe admits the source monthly interval");
    assert!(matches!(
        monthly.interval,
        AnyTimeframe::Variant2(ExtendedTimeframe::MN1)
    ));
}
#[test]
fn polling_owner_release_is_visible() {
    let source = corpus()["boundary"]["retainedTick"].as_array().unwrap();
    assert_eq!(
        source.iter().filter(|v| v["op"] == "loop").count(),
        2,
        "BINGX-RS source polling callback retains owner"
    );
    assert!(
        source
            .iter()
            .any(|v| v["url"] == "/openApi/swap/v2/trade/openOrders"),
        "BINGX-RS source retained callback executes next count"
    );
    let (h, s, _, host) = fixture(corpus()["cases"][32]["input"].clone());
    ready_on(&host, h.tick(1.0)).unwrap();
    let callback = host.loops.borrow_mut().remove(0);
    drop(h);
    drop(s);
    let e = ready(callback()).unwrap_err();
    assert_eq!(
        e.message, "BingX history polling owner released",
        "BINGX-RS released polling owner refusal"
    );
}

#[test]
fn book_parser_preserves_partial_mutation_and_live_alias() {
    let (_, subs, _, _) = fixture(corpus()["cases"][24]["input"].clone());
    let retained = Rc::new(RefCell::new(None));
    let seen = retained.clone();
    let _cleanup = subs
        .listen_order_book(
            "BTCUSDT",
            Rc::new(move |book| {
                *seen.borrow_mut() = Some(book);
                Ok(())
            }),
        )
        .unwrap();
    let parser = subs.stream.message_handlers.borrow()[0].1.borrow()[0].clone();
    parser(&[
        json!({"data":{"bids":[{"price":"2","volume":"1"}],"asks":[{"price":"3","volume":"4"}]}}),
    ])
    .unwrap();
    let original = retained.borrow().as_ref().unwrap().clone();
    let error =
        parser(&[json!({"data":{"bids":[{"price":"9","volume":"7"}],"asks":null}})]).unwrap_err();
    let source = &corpus()["boundary"]["partialBook"];
    assert_eq!(
        json!({"name":error.name,"message":error.message}),
        source["error"],
        "BINGX-RS actual book parser failure"
    );
    assert_eq!(
        book_value(&original.borrow()),
        source["retained"],
        "BINGX-RS book live alias sees partial mutation"
    );
    assert_eq!(
        original.borrow().bids[0].price,
        9.0,
        "BINGX-RS completed bids survive asks failure"
    );
    assert_eq!(
        original.borrow().bids[0].total,
        0.0,
        "BINGX-RS totals wait until both sides complete"
    );
}
