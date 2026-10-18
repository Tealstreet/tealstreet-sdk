#[path = "support/oracle_clock.rs"]
mod oracle_clock;
#[path = "support/retained_observe.rs"]
mod retained_observe;

use safe_cex_exchange::{
    exchange::BaseExchange,
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_num::numeric_ingress::{JsNumberInput, js_value_as_f64};
use safe_cex_num::{JsRootCollator, js_string};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_transport::{
    RestBody, RestRequest,
    retry::{ClassifiedRequest, ReadClassification},
};
use safe_cex_types::*;
use safe_cex_venue_bingx::{
    api::*, data_reads::ReadTransport, exchange::*, listen_key::*,
    private_dispatch::PrivateMessageHost, private_events::PrivateEventHost, public_stream::*,
    stream_subscriptions::*, websocket::*,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
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
                "/tests/completion_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "BINGX-C actual source {}",
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
    panic!("fixture didn't settle")
}
fn observe(v: &Value) -> Value {
    match v {
        Value::Number(_) => {
            let n = js_value_as_f64(v).unwrap();
            json!({"bits":if n.is_nan(){"NaN".into()}else{format!("{:016x}",n.to_bits())}})
        }
        Value::Object(m) => Value::Object(m.iter().map(|(k, v)| (k.clone(), observe(v))).collect()),
        Value::Array(v) => json!(v.iter().map(observe).collect::<Vec<_>>()),
        _ => v.clone(),
    }
}
fn order_value(o: &Order) -> Value {
    let mut v = serde_json::to_value(o).unwrap();
    v.as_object_mut().unwrap().remove("parentId");
    v.as_object_mut().unwrap().remove("triggerType");
    v
}
type Timer = Rc<dyn Fn() -> Result<(), ExchangeError>>;
struct Host {
    input: Value,
    calls: RefCell<Vec<Value>>,
    reports: RefCell<Vec<String>>,
    exceptions: RefCell<Vec<String>>,
    logs: RefCell<Vec<Value>>,
    timers: RefCell<Vec<Timer>>,
    tasks: RefCell<Vec<Pin<Box<dyn Future<Output = ()>>>>>,
    private: RefCell<Option<PrivateCallbacks>>,
    public_close: RefCell<Option<CloseCallback>>,
    listeners: RefCell<Vec<SocketListener>>,
    api_case: Option<Value>,
    serializer: RefCell<Option<ParamsSerializer>>,
}
impl Host {
    fn call(&self, v: Value) {
        self.calls.borrow_mut().push(v);
    }
    fn drain(&self) {
        let tasks = self.tasks.take();
        for f in tasks {
            ready(f)
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
    fn spawn_local(&self, mut f: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        struct Task;
        impl ExchangeTask for Task {
            fn cancel(&self) {}
        }
        if f.as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
        {
            self.tasks.borrow_mut().push(f);
        }
        Box::new(Task)
    }
    fn console_values(&self, severity: &str, message: &[safe_cex_exchange::js_runtime::value::Js]) {
        let rendered = safe_cex_exchange::runtime::console_text(message);
        let message = rendered.as_str();
        if severity == "error" && message.starts_with("exchange error ") {
            self.reports.borrow_mut().push(message[15..].into())
        } else if matches!(severity, "info" | "warning" | "debug") {
            self.logs.borrow_mut().push(json!([message, severity]))
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
        _: &serde_json::Value,
    ) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, e: &ExchangeError, _: &Value) {
        if !e.message.starts_with("Exchange Error:") {
            self.exceptions.borrow_mut().push(e.message.clone());
        }
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("unexpected worker")
    }
}
struct Transport {
    host: Rc<Host>,
}
impl ReadTransport for Transport {
    fn send(&self, r: RestRequest) -> ExchangeFuture<'_, JsNumberInput> {
        let host = self.host.clone();
        let mut call = json!({"op":"http","method":r.method.to_uppercase(),"url":r.url});
        if let Some(body) = &r.body {
            call["body"] = body.untransformed().unwrap().clone().into_value().unwrap()
        } else if let Some(p) = &r.params {
            call["body"] = p.clone()
        }
        host.call(call);
        Box::pin(async move {
            if host.input["reject"] == true && r.url == "/openApi/swap/v1/trade/amend" {
                let mut e = ExchangeError::new("transport cause");
                e.response = Some(json!({"data":{"msg":"wire denied"}}));
                return Err(e);
            }
            let data = match r.url.as_str() {
                "/openApi/swap/v2/quote/contracts" => {
                    json!({"code":0,"data":[{"symbol":"BTC-USDT","asset":"BTC","currency":"USDT","status":1,"tradeMinLimit":0.1,"tradeMaxLimit":5,"quantityPrecision":2,"pricePrecision":2,"size":0.01,"tradeMinQuantity":0.1,"tradeMaxQuantity":5,"tradeMinUSDT":1,"maxLongLeverage":50,"maxShortLeverage":50}]})
                }
                "/openApi/swap/v2/quote/ticker" => {
                    json!({"code":0,"data":[{"symbol":"BTC-USDT","lastPrice":"100","priceChangePercent":"0","volume":"9","quoteVolume":"900"}]})
                }
                "/openApi/swap/v2/quote/premiumIndex" => {
                    json!({"code":0,"data":[{"symbol":"BTC-USDT","markPrice":"100","indexPrice":"100","lastFundingRate":"0","nextFundingTime":0}]})
                }
                "/openApi/swap/v1/positionSide/dual" => {
                    json!({"code":0,"data":{"dualSidePosition":"false"}})
                }
                "/openApi/swap/v3/user/balance" => {
                    json!({"code":0,"data":[{"asset":"USDT","balance":"10","availableMargin":"8","usedMargin":"2","unrealizedProfit":"1"}]})
                }
                "/openApi/swap/v2/user/positions" | "/openApi/swap/v2/trade/openOrders" => {
                    json!({"code":0,"data":[]})
                }
                "/openApi/user/auth/userDataStream" => json!({"listenKey":"key1"}),
                _ => host.input["response"].clone(),
            };
            Ok(serde_json::from_value(data).unwrap())
        })
    }
}
struct ApiImport {
    host: Rc<Host>,
}
fn qs(v: &Value) -> String {
    fn enc(s: &str) -> String {
        s.bytes()
            .map(|b| {
                if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                    char::from(b).to_string()
                } else {
                    format!("%{b:02X}")
                }
            })
            .collect()
    }
    fn visit(v: &Value, key: &str, c: &JsRootCollator, o: &mut Vec<String>) {
        match v {
            Value::Array(a) => {
                for v in a {
                    visit(v, key, c, o)
                }
            }
            Value::Object(m) => {
                let mut keys = m.keys().collect::<Vec<_>>();
                keys.sort_by(|a, b| c.compare(a, b));
                for k in keys {
                    visit(&m[k], &format!("{key}[{k}]"), c, o)
                }
            }
            Value::Null => o.push(format!("{}=", enc(key))),
            _ => o.push(format!(
                "{}={}",
                enc(key),
                enc(&js_string(Some(v)).unwrap())
            )),
        }
    }
    let c = JsRootCollator::try_new().unwrap();
    let mut out = Vec::new();
    if let Value::Object(m) = v {
        let mut keys = m.keys().collect::<Vec<_>>();
        keys.sort_by(|a, b| c.compare(a, b));
        for k in keys {
            visit(&m[k], k, &c, &mut out)
        }
    }
    out.join("&")
}
impl ApiHost for ApiImport {
    fn create(
        &self,
        d: ApiDefaults,
        opts: &ExchangeOptions,
        _: &Rc<BaseExchange>,
    ) -> Result<Rc<dyn ReadTransport>, ExchangeError> {
        if self.host.api_case.is_some() {
            *self.host.serializer.borrow_mut() = Some(d.params_serializer.clone());
            assert!(d.bigint_response, "BINGX-C bigint decoder selected");
            self.host.call(json!({"op":"create","baseURL":d.base_url,"timeout":d.timeout,"headers":d.headers.into_iter().map(|(k,v)|(k,json!(v))).collect::<serde_json::Map<_,_>>(),"bigint":{"orderId":"9007199254740993"}}));
        } else {
            self.host.call(json!({"op":"api","key":opts.key}));
        }
        Ok(Rc::new(Transport {
            host: self.host.clone(),
        }))
    }
    fn retry(
        &self,
        _: &Rc<dyn ReadTransport>,
        n: usize,
        policy: ReadClassification,
        signature: safe_cex_transport::signature_rejection::IsSignatureRejection,
    ) -> Result<(), ExchangeError> {
        assert_eq!(n, 3, "BINGX-C imported retry count");
        for (data, rejected) in signature_corpus() {
            assert_eq!(
                signature(&data).unwrap(),
                rejected,
                "BINGX-C retry signature predicate {data}"
            );
        }
        for (method, url, read) in [
            ("GET", "/read", true),
            ("HEAD", "/read", true),
            ("POST", "/openApi/user/auth/userDataStream?x=1", true),
            ("PUT", "/write", false),
            ("POST", "/write", false),
        ] {
            assert_eq!(
                matches!(
                    policy.classify(RestRequest {
                        method: method.into(),
                        url: url.into(),
                        ..Default::default()
                    }),
                    ClassifiedRequest::Read(_)
                ),
                read,
                "BINGX-C imported retry classification"
            );
        }
        if self.host.api_case.is_some() {
            self.host.call(json!({"op":"retry","retries":n}))
        }
        Ok(())
    }
    fn install_adapter(
        &self,
        _: &Rc<dyn ReadTransport>,
        _: &ExchangeOptions,
        _: &Rc<BaseExchange>,
    ) -> Result<(), ExchangeError> {
        if self.host.api_case.is_some() {
            self.host.call(json!({"op":"adapter"}))
        }
        Ok(())
    }
    fn serialize(
        &self,
        v: &safe_cex_types::retained_json::RetainedJsonInput,
    ) -> Result<String, ExchangeError> {
        Ok(qs(&v.clone().into_value().unwrap()))
    }
    fn hmac_sha256_hex(&self, key: &str, message: &str) -> Result<String, ExchangeError> {
        self.host
            .call(json!({"op":"hmac","alg":"sha256","key":key,"message":message}));
        Ok("signature".into())
    }
    fn now_ms(&self) -> f64 {
        1234.0
    }
}
impl ExchangeHost for Host {
    fn secondary(
        &self,
        opts: &ExchangeOptions,
        create: &ApiFactory<'_>,
        rps: f64,
    ) -> Result<Rc<dyn ReadTransport>, ExchangeError> {
        self.call(json!({"op":"secondary"}));
        let client = create(opts.clone())?;
        self.call(json!({"op":"rateLimit","maxRPS":rps}));
        Ok(client)
    }
    fn init_external_public_data(&self, _: Rc<BaseExchange>) -> ExchangeFuture<'_, bool> {
        self.call(json!({"op":"external"}));
        Box::pin(std::future::ready(Ok(self.input["external"]
            .as_bool()
            .unwrap())))
    }
    fn is_public_data_only(&self, base: &BaseExchange) -> bool {
        base.options().public_data_only == Some(true)
    }
}
impl safe_cex_venue_bingx::read_history::HistoryHost for Host {
    fn now_ms(&self) -> f64 {
        1234.0
    }
    fn subtract_date(&self, _: f64, _: f64, _: &str) -> Result<f64, ExchangeError> {
        panic!("unexpected date")
    }
    fn local_to_utc(&self, t: f64) -> f64 {
        t
    }
    fn is_public_data_only(&self) -> bool {
        self.input["options"]["publicDataOnly"] == true
    }
    fn loop_after(
        &self,
        delay: Option<f64>,
        _: Rc<dyn Fn() -> ExchangeFuture<'static, ()>>,
    ) -> Result<(), ExchangeError> {
        self.call(json!({"op":"loop","delay":delay.unwrap()}));
        Ok(())
    }
}
impl safe_cex_venue_bingx::placement::PlacementHost for Host {
    fn uuid(&self) -> Result<String, ExchangeError> {
        Ok("ABCD-0123".into())
    }
}
impl PublicSocketHost for Host {
    fn is_disposed(&self) -> bool {
        false
    }
    fn nanoid(&self) -> String {
        "n1".into()
    }
    fn monotonic_now_ms(&self) -> f64 {
        1234.0
    }
    fn iso_time(&self) -> Result<String, ExchangeError> {
        Ok("1970-01-01T00:00:01.234Z".into())
    }
    fn create_socket(&self, url: &str) -> Result<(), ExchangeError> {
        self.call(json!({"op":"socket","url":url}));
        Ok(())
    }
    fn add_listener(&self, l: SocketListener) -> Result<(), ExchangeError> {
        let name = match &l {
            SocketListener::Open(_) => "open",
            SocketListener::Message(_) => "message",
            SocketListener::Close => "close",
        };
        self.call(json!({"op":"listener","name":name}));
        self.listeners.borrow_mut().push(l);
        Ok(())
    }
    fn send_with_log(&self, raw: &str) -> Result<(), ExchangeError> {
        self.call(json!({"op":"send","raw":raw}));
        Ok(())
    }
    fn set_ping_at(&self, _: f64) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn track_last_received(&self) -> Result<(), ExchangeError> {
        self.call(json!({"op":"track"}));
        Ok(())
    }
    fn handle_pong(&self) -> Result<(), ExchangeError> {
        self.call(json!({"op":"pong"}));
        Ok(())
    }
    fn record_stats(&self, t: &str) -> Result<(), ExchangeError> {
        self.call(json!({"op":"stats","topic":t}));
        Ok(())
    }
    fn diagnostic_receive(
        &self,
        raw: &str,
        _: &dyn Fn() -> bool,
        _: &dyn Fn() -> Result<Option<String>, ExchangeError>,
    ) -> Result<(), ExchangeError> {
        self.call(json!({"op":"diagnostic","raw":raw}));
        Ok(())
    }
    fn read_blob(&self, _: u64) -> ExchangeFuture<'_, String> {
        panic!("unexpected blob")
    }
    fn report_exception(&self, e: &ExchangeError) {
        panic!("unexpected public exception: {}", e.message)
    }
}
impl SubscriptionHost for Host {
    fn is_connected(&self) -> bool {
        true
    }
    fn set_timeout(&self, _: f64, _: Timer) -> Result<u64, ExchangeError> {
        panic!("unexpected subscription timer")
    }
    fn clear_timeout(&self, _: u64) -> Result<(), ExchangeError> {
        panic!("unexpected subscription clear")
    }
    fn cleanup_stats(&self, t: &str) -> Result<(), ExchangeError> {
        self.call(json!({"op":"cleanup","topic":t}));
        Ok(())
    }
}
impl ListenKeyHost for Host {
    fn is_disposed(&self) -> bool {
        false
    }
    fn testnet(&self) -> bool {
        self.input["options"]["testnet"] == true
    }
    fn create_socket(&self, url: &str) -> Result<u64, ExchangeError> {
        if self.input["failPrivateCreate"] == true {
            return Err(ExchangeError::new("private socket cause"));
        }
        self.call(json!({"op":"socket","url":url}));
        Ok(1)
    }
    fn add_listener(&self, _: u64, l: PrivateListener) -> Result<(), ExchangeError> {
        self.call(json!({"op":"listener","name":match l{PrivateListener::Open=>"open",PrivateListener::Message=>"message",PrivateListener::Close=>"close"}}));
        Ok(())
    }
    fn remove_listener(&self, _: u64, l: PrivateListener) -> Result<(), ExchangeError> {
        self.call(json!({"op":"remove","name":match l{PrivateListener::Open=>"open",PrivateListener::Message=>"message",PrivateListener::Close=>"close"}}));
        Ok(())
    }
    fn close_socket(&self, _: u64) -> Result<(), ExchangeError> {
        self.call(json!({"op":"closeSocket"}));
        Ok(())
    }
    fn retry_failed_connect(&self, message: &str) -> Result<(), ExchangeError> {
        self.call(json!({"op":"retryConnect","message":message}));
        Ok(())
    }
    fn warning(&self, m: &str) -> Result<(), ExchangeError> {
        self.logs.borrow_mut().push(json!([m, "warning"]));
        Ok(())
    }
    fn set_timeout(&self, delay: f64, f: Timer) -> Result<u64, ExchangeError> {
        let id = self.timers.borrow().len() as u64 + 1;
        self.timers.borrow_mut().push(f);
        self.call(json!({"op":"timer","delay":delay,"id":id}));
        Ok(id)
    }
    fn clear_timeout(&self, id: u64) -> Result<(), ExchangeError> {
        self.call(json!({"op":"clear","id":id}));
        Ok(())
    }
    fn spawn(&self, f: ExchangeFuture<'static, ()>) -> Result<(), ExchangeError> {
        ready(f)
    }
    fn monotonic_now_ms(&self) -> Result<f64, ExchangeError> {
        Ok(1234.0)
    }
    fn set_ping_at(&self, _: f64) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn send_with_log(&self, raw: &str) -> Result<(), ExchangeError> {
        self.call(json!({"op":"send","raw":raw}));
        Ok(())
    }
    fn base_dispose(&self) -> Result<(), ExchangeError> {
        self.call(json!({"op":"privateDispose"}));
        Ok(())
    }
    fn report_exception(&self, e: &ExchangeError) {
        panic!("unexpected key exception: {}", e.message)
    }
}
impl PrivateEventHost for Host {
    fn is_disposed(&self) -> bool {
        false
    }
    fn now_ms(&self) -> f64 {
        1234.0
    }
    fn set_timeout(&self, d: f64, f: Timer) -> Result<(), ExchangeError> {
        ListenKeyHost::set_timeout(self, d, f)?;
        Ok(())
    }
    fn spawn(&self, f: ExchangeFuture<'static, ()>) -> Result<(), ExchangeError> {
        ready(f)
    }
    fn emit_fill(
        &self,
        v: safe_cex_types::retained_json::RetainedJsonInput,
    ) -> Result<(), ExchangeError> {
        self.call(json!({"op":"fill","payload":v}));
        Ok(())
    }
    fn report_exception(&self, e: &ExchangeError) {
        panic!("unexpected event exception: {}", e.message)
    }
}
impl PrivateMessageHost for Host {
    fn is_disposed(&self) -> bool {
        false
    }
    fn track_last_received(&self) -> Result<(), ExchangeError> {
        PublicSocketHost::track_last_received(self)
    }
    fn read_blob(&self, _: u64) -> ExchangeFuture<'_, String> {
        panic!("unexpected blob")
    }
    fn diagnostic_receive(&self, raw: &str) -> Result<(), ExchangeError> {
        self.call(json!({"op":"diagnostic","raw":raw}));
        Ok(())
    }
    fn send_with_log(&self, raw: &str) -> Result<(), ExchangeError> {
        ListenKeyHost::send_with_log(self, raw)
    }
    fn handle_pong(&self) -> Result<(), ExchangeError> {
        PublicSocketHost::handle_pong(self)
    }
    fn record_stats(&self, t: &str) -> Result<(), ExchangeError> {
        PublicSocketHost::record_stats(self, t)
    }
}
impl WebsocketBindings for Host {
    fn bind_private(&self, c: PrivateCallbacks) -> Result<(), ExchangeError> {
        *self.private.borrow_mut() = Some(c);
        Ok(())
    }
    fn bind_public_close(&self, c: CloseCallback) -> Result<(), ExchangeError> {
        *self.public_close.borrow_mut() = Some(c);
        Ok(())
    }
    fn public_base_close(&self, event: Option<Value>) -> Result<(), ExchangeError> {
        self.call(json!({"op":"publicClose","event":event}));
        Ok(())
    }
    fn private_base_close(&self, event: Option<Value>) -> Result<(), ExchangeError> {
        self.call(json!({"op":"privateClose","event":event}));
        Ok(())
    }
    fn public_base_dispose(&self) -> Result<(), ExchangeError> {
        self.call(json!({"op":"publicDispose"}));
        Ok(())
    }
}
fn host(input: Value, api_case: Option<Value>) -> Rc<Host> {
    Rc::new(Host {
        input,
        api_case,
        serializer: RefCell::default(),
        calls: RefCell::default(),
        reports: RefCell::default(),
        exceptions: RefCell::default(),
        logs: RefCell::default(),
        timers: RefCell::default(),
        tasks: RefCell::default(),
        private: RefCell::default(),
        public_close: RefCell::default(),
        listeners: RefCell::default(),
    })
}
fn fixture(input: &Value) -> (Rc<BingxExchange>, Rc<Host>, Rc<DefaultStore>) {
    let h = host(input.clone(), None);
    let store = Rc::new(DefaultStore::new());
    Store::update(
        store.as_ref(),
        StoreUpdate {
            markets: Some(serde_json::from_value(input["markets"].clone()).unwrap()),
            tickers: Some(serde_json::from_value(input["tickers"].clone()).unwrap()),
            orders: Some(vec![
                serde_json::from_value(input["order"].clone()).unwrap(),
            ]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    h.call(json!({"op":"base","name":"bingx"}));
    let venue = BingxExchange::new(
        ExchangeName::Bingx,
        serde_json::from_value(input["options"].clone()).unwrap(),
        store.clone(),
        h.clone(),
        BingxHosts {
            api: Rc::new(ApiImport { host: h.clone() }),
            exchange: h.clone(),
            history: h.clone(),
            placement: h.clone(),
            public_socket: h.clone(),
            subscription: h.clone(),
            private_keys: h.clone(),
            private_events: h.clone(),
            private_message: h.clone(),
            bindings: h.clone(),
        },
    )
    .unwrap();
    (venue, h, store)
}
fn run(case: &Value) {
    let i = &case["input"];
    let (v, h, s) = fixture(i);
    let method = i["method"].as_str().unwrap();
    let clock = oracle_clock::ClockWindow::start();
    let result = match method {
        "updateOrder" => ready(v.amendment.update_order(
            &serde_json::from_value(json!({"order":i["order"],"update":i["update"]})).unwrap(),
        ))
        .map(|ids| json!(ids)),
        "replaceTriggerOrderPrice" => ready(v.amendment.replace_trigger_order_price(
            &serde_json::from_value(i["order"].clone()).unwrap(),
            i["nextPrice"].as_f64().unwrap(),
        ))
        .map(|ids| json!(ids)),
        "cancelReplaceLimitOrderPrice" => ready(v.amendment.cancel_replace_limit_order_price(
            &serde_json::from_value(i["order"].clone()).unwrap(),
            i["nextPrice"].as_f64().unwrap(),
        ))
        .map(|ids| json!(ids)),
        "start" => {
            let mut f = Box::pin(v.start());
            let r = loop {
                match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
                    Poll::Ready(r) => break r,
                    Poll::Pending => {
                        let tasks = h.tasks.take();
                        for mut task in tasks {
                            if task
                                .as_mut()
                                .poll(&mut Context::from_waker(Waker::noop()))
                                .is_pending()
                            {
                                h.tasks.borrow_mut().push(task);
                            }
                        }
                    }
                }
            };
            h.drain();
            r.map(|_| Value::Null)
        }
        _ => {
            v.public_websocket.stream.connect_and_subscribe().unwrap();
            ready(v.private_websocket.keys.connect_and_subscribe()).unwrap();
            let callbacks = h.private.borrow();
            let c = callbacks.as_ref().unwrap();
            (c.open)().unwrap();
            ready((c.message)(PublicFrame::Text(
                i["frame"].as_str().unwrap().into(),
            )))
            .unwrap();
            (c.close)(Some(json!({"code":1001}))).unwrap();
            (h.public_close.borrow().as_ref().unwrap())(Some(json!({"code":1000}))).unwrap();
            let p = &v.private_websocket;
            let e = &p.events;
            assert!(
                Rc::ptr_eq(
                    &v.public_websocket.stream,
                    &v.public_websocket.subscriptions.stream
                ),
                "BINGX-C complete public carrier shares stream"
            );
            assert_eq!(
                e.position_fetch_throttle_ms.get().to_bits(),
                750.0_f64.to_bits(),
                "BINGX-C direct private constructor throttle"
            );
            assert_eq!(
                e.last_position_fetch_time.get().to_bits(),
                0.0_f64.to_bits(),
                "BINGX-C direct private constructor fetch clock"
            );
            let unlimited: Rc<dyn ReadTransport> = v.unlimited_xhr.clone();
            assert!(
                !Rc::ptr_eq(&v.xhr, &unlimited),
                "BINGX-C constructor distinct configured clients"
            );
            Ok(
                json!({"apiDistinct":true,"parentSame":Rc::ptr_eq(&v.reads,&v.public_websocket.stream.parent)&&Rc::ptr_eq(&v.reads,&e.parent),"publicHandlers":v.public_websocket.stream.message_handlers.borrow().len(),"publicTopics":v.public_websocket.stream.topics.borrow().len(),"filledMap":e.emitted_fill_qty_by_order_id.borrow().len(),"lastFetch":e.last_position_fetch_time.get(),"throttle":e.position_fetch_throttle_ms.get(),"scheduled":e.position_fetch_scheduled.get(),"pending":e.has_pending_position_events.get(),"connecting":p.keys.connecting.get(),"rebuilding":p.keys.rebuilding.get(),"refreshUnset":p.keys.refresh_timer.get().is_none()}),
            )
        }
    };
    let clock = clock.finish();
    let mut native_orders = json!(s.orders().iter().map(order_value).collect::<Vec<_>>());
    clock.normalize_orders(&mut native_orders, &case["orders"]);
    for (native, expected) in s.orders().iter().zip(case["orders"].as_array().unwrap()) {
        for (field, n) in [
            ("price", native.price.to_number()),
            ("amount", native.amount),
            ("remaining", native.remaining),
            ("filled", native.filled),
        ] {
            assert_eq!(
                n.to_bits(),
                expected[field].as_f64().unwrap().to_bits(),
                "BINGX-C direct native order {field} {method}"
            );
        }
        let expected: Order = serde_json::from_value(expected.clone()).unwrap();
        assert_eq!(native.id, expected.id, "BINGX-C direct native order id");
        assert_eq!(
            native.status, expected.status,
            "BINGX-C direct native order status"
        );
        assert_eq!(
            native.side, expected.side,
            "BINGX-C direct native order side"
        );
        assert_eq!(
            native.r#type, expected.r#type,
            "BINGX-C direct native order type"
        );
        assert_eq!(
            native.time_in_force, expected.time_in_force,
            "BINGX-C direct native order TIF"
        );
        assert_eq!(
            native.reduce_only, expected.reduce_only,
            "BINGX-C direct native reduce-only"
        );
        assert_eq!(
            native.info.as_ref().map(retained_observe::observe),
            expected.info.as_ref().map(retained_observe::observe),
            "BINGX-C direct native retained info"
        );
    }
    let native_balance = s.balance();
    assert_eq!(
        native_balance.info,
        case["balance"].get("info").cloned().map(Into::into),
        "BINGX-C direct native balance info"
    );
    for (field, n) in [
        ("total", native_balance.total),
        ("free", native_balance.free),
        ("used", native_balance.used),
        ("upnl", native_balance.upnl),
    ] {
        assert_eq!(
            n.to_bits(),
            case["balance"][field].as_f64().unwrap().to_bits(),
            "BINGX-C direct native balance {field} {method}"
        );
    }
    let loaded = s.loaded();
    for (field, value) in [
        ("orders", loaded.orders),
        ("balance", loaded.balance),
        ("positions", loaded.positions),
        ("markets", loaded.markets),
        ("tickers", loaded.tickers),
    ] {
        assert_eq!(
            value,
            case["loaded"][field].as_bool().unwrap(),
            "BINGX-C loaded {method}"
        );
    }
    let output = match result {
        Ok(v) => json!({"ok":v}),
        Err(e) => json!({"error":{"name":e.name,"message":e.message}}),
    };
    assert_eq!(
        observe(&output),
        observe(&case["output"]),
        "BINGX-C output {method} {}",
        case["variant"]
    );
    assert_eq!(
        observe(&json!(h.calls.borrow().clone())),
        observe(&case["calls"]),
        "BINGX-C calls {method} {}",
        case["variant"]
    );
    assert_eq!(
        json!(h.reports.borrow().clone()),
        case["reports"],
        "BINGX-C reports {method}"
    );
    assert_eq!(
        json!(h.logs.borrow().clone()),
        case["logs"],
        "BINGX-C logs {method}"
    );
    assert_eq!(
        observe(&native_orders),
        observe(&case["orders"]),
        "BINGX-C native orders {method}"
    );
    assert_eq!(
        serde_json::to_value(&*s.loaded()).unwrap(),
        case["loaded"],
        "BINGX-C loaded {method}"
    );
    let b = s.balance();
    assert_eq!(
        observe(&serde_json::to_value(&*b).unwrap()),
        observe(&case["balance"]),
        "BINGX-C native balance {method}"
    );
}
#[test]
fn completion_matches_actual_typescript() {
    let c = corpus();
    assert_eq!(c["names"].as_array().unwrap().len(), 9);
    assert_eq!(c["cases"].as_array().unwrap().len(), 36);
    assert_eq!(c["extras"].as_array().unwrap().len(), 11);
    for case in c["cases"]
        .as_array()
        .unwrap()
        .iter()
        .chain(c["extras"].as_array().unwrap())
    {
        run(case)
    }
}
#[test]
fn api_factory_matches_actual_typescript() {
    let c = corpus();
    assert_eq!(c["api"].as_array().unwrap().len(), 4);
    for (n, case) in c["api"].as_array().unwrap().iter().enumerate() {
        let h = host(case["input"].clone(), Some(case.clone()));
        let base = Rc::new(BaseExchange::new(
            ExchangeName::Bingx,
            serde_json::from_value(case["input"]["options"].clone()).unwrap(),
            Rc::new(DefaultStore::new()),
            h.clone(),
            Default::default(),
        ));
        let api = create_api(
            base.options(),
            &base,
            Rc::new(ApiImport { host: h.clone() }),
        )
        .unwrap();
        let c = &case["input"]["config"];
        let request = RestRequest {
            method: c["method"].as_str().unwrap().into(),
            url: c["url"].as_str().unwrap().into(),
            params: Some(c["params"].clone()),
            body: c.get("data").cloned().map(|value| {
                RestBody::Untransformed(
                    safe_cex_types::retained_json::RetainedJsonInput::from_value(value).unwrap(),
                )
            }),
            ..Default::default()
        };
        let original = request.clone();
        let prepared = api.prepare(request).unwrap();
        let mut value = json!({"method":prepared.method,"url":prepared.url,"headers":prepared.headers,"params":prepared.params});
        if let Some(body) = &prepared.body {
            value["data"] = body.untransformed().unwrap().clone().into_value().unwrap();
        }
        assert_eq!(
            observe(&value),
            observe(&case["prepared"]),
            "BINGX-C API prepared {n}"
        );
        assert_eq!(
            observe(&json!(h.calls.borrow().clone())),
            observe(&case["calls"]),
            "BINGX-C API factory {n}"
        );
        assert_eq!(
            (h.serializer.borrow().as_ref().unwrap())(
                prepared
                    .params
                    .as_ref()
                    .cloned()
                    .or_else(|| prepared.body.as_ref().map(|b| b
                        .untransformed()
                        .unwrap()
                        .clone()
                        .into_value()
                        .unwrap()))
                    .as_ref()
                    .unwrap()
            )
            .unwrap(),
            case["serialized"],
            "BINGX-C imported qs {n}"
        );
        assert_eq!(
            original.params,
            c.get("params").cloned(),
            "BINGX-C native request snapshot"
        );
        // unsignedPayload signs a copy of data/params; POST still writes the
        // caller's headers object.
        let (caller, input) = (&case["caller"], &case["input"]["config"]);
        for field in ["data", "params"] {
            assert_eq!(
                caller.get(field),
                input.get(field),
                "BINGX-C source signs a copy of the caller {field}"
            );
        }
        if n == 1 {
            assert_eq!(
                caller["headers"]["Content-Type"], "application/json",
                "BINGX-C source POST mutates caller headers"
            );
        } else {
            assert_eq!(caller, input, "BINGX-C source leaves the caller config");
        }
    }
}
#[test]
fn typed_amendment_admission_has_own_source_witnesses() {
    for case in corpus()["boundary"].as_array().unwrap() {
        assert!(
            case["output"].get("error").is_some(),
            "BINGX-C source runs invalid side preflight"
        );
        let e = serde_json::from_value::<Order>(case["input"]["order"].clone()).unwrap_err();
        assert!(
            e.to_string().contains("constructor"),
            "BINGX-C native side admission refuses callable name"
        );
    }
}
#[test]
fn bound_callbacks_refuse_released_owner() {
    let i = &corpus()["cases"][0]["input"];
    let (v, h, _) = fixture(i);
    drop(v);
    let error = (h.private.borrow().as_ref().unwrap().close)(None).unwrap_err();
    assert_eq!(
        error.message, "BingX private close owner released",
        "BINGX-C weak close owner boundary"
    );
    assert_eq!(
        corpus()["retainedClose"],
        json!({"op":"privateClose","event":{"code":1006}}),
        "BINGX-C source close retains original owner"
    );
}

#[test]
fn constructor_and_public_class_boundaries_are_explicit() {
    let c = corpus();
    let (v, _, _) = fixture(&c["cases"][0]["input"]);
    assert_eq!(
        c["constructorAlias"], "changed-after-construction",
        "BINGX-C source constructor options alias"
    );
    assert_eq!(
        v.base.options().key,
        "",
        "BINGX-C native constructor owns options snapshot"
    );
    assert_eq!(
        c["publicInherited"], "function",
        "BINGX-C source public topic inherited callable"
    );
    assert!(
        !v.public_websocket
            .stream
            .topics
            .borrow()
            .iter()
            .any(|(key, _)| key == "constructor"),
        "BINGX-C native public topics contain own entries only"
    );
}

#[test]
fn startup_reports_private_task_rejection() {
    let mut i = corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["method"] == "start" && c["variant"] == 3)
        .unwrap()["input"]
        .clone();
    i["failPrivateCreate"] = json!(true);
    let (v, h, _) = fixture(&i);
    ready(v.start()).unwrap();
    h.drain();
    assert_eq!(
        corpus()["startupRejection"]["unhandled"],
        json!(["private socket cause"]),
        "BINGX-C source unawaited startup rejection"
    );
    assert_eq!(
        *h.exceptions.borrow(),
        vec!["private socket cause"],
        "BINGX-C native startup reports cause"
    );
}

#[test]
fn api_nan_field_signs_before_final_json_projection() {
    let c = &corpus()["nanApi"];
    assert_eq!(
        c["serializedNaNAsNull"], true,
        "BINGX-C source POST unary number yields NaN, serialized as null"
    );
    let h = host(c["input"].clone(), Some(c.clone()));
    let base = Rc::new(BaseExchange::new(
        ExchangeName::Bingx,
        serde_json::from_value(c["input"]["options"].clone()).unwrap(),
        Rc::new(DefaultStore::new()),
        h.clone(),
        Default::default(),
    ));
    let api = create_api(
        base.options(),
        &base,
        Rc::new(ApiImport { host: h.clone() }),
    )
    .unwrap();
    let request = RestRequest {
        method: "post".into(),
        url: "/api".into(),
        params: Some(c["input"]["config"]["params"].clone()),
        body: Some(RestBody::Untransformed(
            safe_cex_types::retained_json::RetainedJsonInput::from_value(
                c["input"]["config"]["data"].clone(),
            )
            .unwrap(),
        )),
        ..Default::default()
    };
    let prepared = api.prepare(request).unwrap();
    let body = prepared
        .body
        .as_ref()
        .unwrap()
        .untransformed()
        .unwrap()
        .clone()
        .into_value()
        .unwrap();
    assert_eq!(
        body, c["prepared"]["data"],
        "BINGX-C source final JSON retains NaN as null"
    );
    assert_eq!(
        observe(&json!(*h.calls.borrow())),
        observe(&c["calls"]),
        "BINGX-C complete source signing sequence precedes final JSON projection"
    );
    let wire: Value = serde_json::from_str(body.as_str().unwrap()).unwrap();
    assert!(
        wire["quantity"].is_null(),
        "BINGX-C NaN projects only at the final JSON boundary"
    );
}

#[test]
fn api_prototype_key_refusal_is_visible() {
    let c = &corpus()["protoApi"];
    assert!(
        c["input"]["config"]["data"].get("__proto__").is_some(),
        "BINGX-C source own prototype key input"
    );
    assert!(
        c["prepared"]["data"].get("__proto__").is_none(),
        "BINGX-C source reduction assigns object prototype"
    );
    let h = host(c["input"].clone(), Some(c.clone()));
    let base = Rc::new(BaseExchange::new(
        ExchangeName::Bingx,
        serde_json::from_value(c["input"]["options"].clone()).unwrap(),
        Rc::new(DefaultStore::new()),
        h.clone(),
        Default::default(),
    ));
    let api = create_api(
        base.options(),
        &base,
        Rc::new(ApiImport { host: h.clone() }),
    )
    .unwrap();
    let e = api
        .prepare(RestRequest {
            method: "post".into(),
            url: "/api".into(),
            body: Some(RestBody::Untransformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(
                    c["input"]["config"]["data"].clone(),
                )
                .unwrap(),
            )),
            ..Default::default()
        })
        .unwrap_err();
    assert_eq!(
        e.message, "BingX native signing refuses prototype-mutating key",
        "BINGX-C native signing prototype key refusal"
    );
}

#[test]
fn close_bindings_do_not_cycle_the_native_host() {
    let (v, h, _) = fixture(&corpus()["cases"][0]["input"]);
    let saved = h.public_close.borrow().as_ref().unwrap().clone();
    let weak = Rc::downgrade(&h);
    drop(v);
    drop(h);
    assert!(
        weak.upgrade().is_none(),
        "BINGX-C bound callbacks release host"
    );
    assert_eq!(
        saved(None).unwrap_err().message,
        "BingX public close bindings released",
        "BINGX-C released public bindings cause"
    );
    assert_eq!(
        corpus()["retainedPublicClose"],
        json!({"op":"publicClose","event":{"code":1007}}),
        "BINGX-C source retained public close still runs"
    );
}

fn signature_corpus() -> Vec<(Value, bool)> {
    vec![
        (json!({"code":100001}), true),
        (json!({"code":"100001"}), true),
        (
            json!({"code":-1,"msg":"Signature verification failed"}),
            true,
        ),
        (json!({"msg":"SIGNATURE VERIFICATION FAILED, retry"}), true),
        (json!({"msg":["signature verification failed"]}), true),
        (json!({"code":[100001]}), true),
        (json!({"code":0,"msg":""}), false),
        (json!({"code":100002}), false),
        (json!({"code":null,"msg":null}), false),
        (json!({"msg":"signature mismatch"}), false),
        (json!("100001"), false),
        (json!([]), false),
        (json!(null), false),
    ]
}
#[test]
fn signature_rejection_predicate_matches_actual_typescript() {
    let mut inputs = vec![];
    for (data, rejected) in signature_corpus() {
        assert_eq!(
            is_bingx_signature_rejection(&data).unwrap(),
            rejected,
            "BINGX-C native signature predicate {data}"
        );
        inputs.push(data);
    }
    let source = &corpus()["signature"];
    assert_eq!(
        source["corpus"],
        Value::Array(inputs),
        "BINGX-C source corpus"
    );
    let natives: Vec<_> = signature_corpus()
        .into_iter()
        .map(|(_, rejected)| json!(rejected))
        .collect();
    assert_eq!(
        source["results"],
        Value::Array(natives),
        "BINGX-C source signature predicate"
    );
}
