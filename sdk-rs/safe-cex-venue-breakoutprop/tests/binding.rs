// Construct the production host and call the typed Exchange surface through its
// shared JavaScript functions/promises, with HTTP as the sole scripted leaf.
#[allow(dead_code)]
mod clock {
    pub use virtual_time::WakeSignal;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../safe-cex-exchange/tests/support/runtime.rs"
    ));
}
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{
    exchange::Exchange,
    js_runtime::{
        client::JsHttpHost,
        promise::JsFuture,
        value::{Js, Thrown, from_json, to_json},
    },
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime},
    websocket::{WebSocketConnectionOptions, WsRuntime, WsSocket},
};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_transport::{TransportProvider, socket::TunnelHost};
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use safe_cex_types::*;
#[cfg(test)]
use safe_cex_venue_breakoutprop::native_reads::NativeRuntimeHost;
use safe_cex_venue_breakoutprop::{
    host::{BreakoutDependencies, ProductionHost},
    lifecycle::BreakoutpropExchange,
};
use serde_json::{Value, json};
fn assert_finite_native(actual: &Input, expected: &Value) {
    match (actual, expected) {
        (Input::Null, Value::Null) => {}
        (Input::Bool(actual), Value::Bool(expected)) => assert_eq!(actual, expected),
        (Input::Float(_) | Input::Lexeme(_), Value::Number(expected)) => {
            assert_eq!(
                actual.as_f64().unwrap().to_bits(),
                expected.as_f64().unwrap().to_bits(),
                "retained numeric bits"
            );
        }
        (Input::Text(actual), Value::String(expected)) => {
            assert!(
                actual.iter().copied().eq(expected.encode_utf16()),
                "retained text units"
            );
        }
        (Input::Array(actual), Value::Array(expected)) => {
            assert_eq!(actual.len(), expected.len(), "retained array cardinality");
            for (actual, expected) in actual.iter().zip(expected) {
                assert_finite_native(actual, expected);
            }
        }
        (Input::Object(actual), Value::Object(expected)) => {
            assert_eq!(actual.len(), expected.len(), "retained own-key count");
            for ((key, actual), (expected_key, expected)) in actual.iter().zip(expected) {
                assert!(
                    key.iter().copied().eq(expected_key.encode_utf16()),
                    "retained own-key order/units"
                );
                assert_finite_native(actual, expected);
            }
        }
        _ => panic!("retained native kind differs from finite fixture: {actual:?}, {expected:?}"),
    }
}

use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::Future,
    rc::Rc,
    sync::Arc,
    task::{Context, Poll, Waker},
};
impl WsRuntime for clock::Runtime {
    fn transport(&self) -> Arc<dyn TransportProvider> {
        Arc::new(safe_cex_transport::DirectTransport)
    }
    fn tunnel_host(&self) -> Rc<dyn TunnelHost> {
        panic!("DXtrade polling must not request a tunnel socket")
    }
    fn dial(
        &self,
        _: String,
        _: Vec<String>,
        _: WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        panic!("DXtrade polling must not dial a websocket")
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
#[derive(Default)]
struct Wire {
    runtime: Rc<clock::Runtime>,
    requests: RefCell<Vec<Value>>,
    native_requests: RefCell<Vec<Js>>,
    faults: RefCell<VecDeque<(String, u16, Value)>>,
    errors: RefCell<VecDeque<(String, Js)>>,
    candle_response: RefCell<Option<Value>>,
    native_responses: RefCell<VecDeque<(String, Js)>>,
}
fn instrument() -> Value {
    json!({"type":"FOREX","symbol":"BTCUSD","version":1,"description":"Bitcoin","priceIncrement":0.1,"pipSize":0.01,"currency":"USD","lotSize":1,"multiplier":1,"firstCurrency":"BTC"})
}
fn detail() -> Value {
    json!({"account":"default:A","type":"FOREX","symbol":"BTCUSD","tradingStatus":"TRADING","minOrderSize":0.01,"maxOrderSize":{"value":100,"limitType":"UNITS"},"minOrderSizeIncrement":0.01,"marginRate":{"rateType":"FIXED","tiers":[{"endVolume":"Infinity","value":0.05}]}})
}
fn portfolio() -> Value {
    json!({"portfolios":[{"account":"default:A","version":1,"balances":[{"account":"default:A","version":1,"currency":"USD","value":50.25}],"positions":[],"orders":[]}]})
}
impl JsHttpHost for Wire {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.runtime.clone()
    }
    fn send(&self, config: Js) -> JsFuture<'static> {
        let observed = to_json(&config).unwrap();
        let url = config.get("url").unwrap().to_display();
        let method = config
            .get("method")
            .unwrap()
            .to_display()
            .to_ascii_uppercase();
        self.requests.borrow_mut().push(observed);
        self.native_requests.borrow_mut().push(config.clone());
        if self
            .errors
            .borrow()
            .front()
            .is_some_and(|(target, _)| target == &url)
        {
            let (_, error) = self.errors.borrow_mut().pop_front().unwrap();
            error.define("config", config).unwrap();
            return Box::pin(async move { Err(Thrown::new(error)) });
        }
        let mut status = 200;
        let data = if self
            .faults
            .borrow()
            .front()
            .is_some_and(|(target, _, _)| target == &url)
        {
            let (_, code, body) = self.faults.borrow_mut().pop_front().unwrap();
            status = code;
            body
        } else {
            match (method.as_str(), url.as_str()) {
                ("POST", "/login") => json!({"sessionToken":"token","timeout":"1:00:00"}),
                ("GET", "/users/k" | "/users/Infinity") => {
                    json!({"userDetails":[{"login":"k","domain":"default","version":1,"fullName":"Trader","username":"k","accounts":[{"owner":"k","account":"default:A","version":1,"baseCurrency":"USD","registrationTime":"2026-01-01T00:00:00Z","accountStatus":"FULL_TRADING","pricingStream":"quote","positionBased":false}]}]})
                }
                ("GET", "instruments/query") => json!({"instruments":[instrument()]}),
                ("GET", "accounts/default:A/instruments/query") => {
                    json!({"instrumentDetails":[detail()]})
                }
                ("GET", "accounts/default:A/portfolio") => portfolio(),
                ("POST", "marketdata") => {
                    self.candle_response.borrow().clone().unwrap_or_else(||
                        json!({"events":[{"symbol":"BTCUSD","type":"Quote","bid":99,"ask":101,"time":"2026-01-01T00:00:00Z"}]}))
                }
                ("POST", "accounts/default:A/orders") => json!({"orderId":"placed"}),
                ("DELETE", path) if path.starts_with("accounts/default:A/orders/") => json!({}),
                _ => panic!("unscripted DXtrade request {method} {url}"),
            }
        };
        let data = if self
            .native_responses
            .borrow()
            .front()
            .is_some_and(|(target, _)| target == &url)
        {
            self.native_responses.borrow_mut().pop_front().unwrap().1
        } else {
            from_json(&data)
        };
        Box::pin(async move {
            Ok(Js::object(vec![
                ("status", Js::Number(status as f64)),
                ("data", data),
                ("headers", Js::object(vec![])),
                ("config", config),
            ]))
        })
    }
}
struct Fixture {
    runtime: Rc<clock::Runtime>,
    wire: Rc<Wire>,
    store: Rc<DefaultStore>,
    exchange: Rc<BreakoutpropExchange>,
    host: Rc<ProductionHost>,
}
impl Fixture {
    fn run<T>(&self, future: impl Future<Output = T>) -> T {
        let mut future = Box::pin(future);
        let signal = Arc::new(clock::WakeSignal::default());
        let waker = signal.waker();
        for _ in 0..512 {
            signal.take();
            if let Poll::Ready(value) = future.as_mut().poll(&mut Context::from_waker(&waker)) {
                return value;
            }
            // Promise reactions are ready work, not elapsed timer intervals.
            self.runtime.advance(self.runtime.now.get());
            if !signal.take() {
                self.runtime.advance(self.runtime.now.get() + 25);
            }
        }
        panic!("production binding exceeded 512 deterministic polls")
    }
    fn fetch_markets(&self) {
        let markets = self.run(self.exchange.fetch_markets()).unwrap();
        Store::update(
            self.store.as_ref(),
            StoreUpdate {
                markets: Some(markets),
                ..Default::default()
            },
            false,
        )
        .unwrap();
    }
    fn request(&self, method: &str, path: &str) -> Value {
        self.wire
            .requests
            .borrow()
            .iter()
            .rev()
            .find(|r| {
                r["method"]
                    .as_str()
                    .is_some_and(|m| m.eq_ignore_ascii_case(method))
                    && r["url"] == path
            })
            .cloned()
            .unwrap_or_else(|| panic!("request {method} {path} was not sent"))
    }
}
fn fixture() -> Fixture {
    let runtime = Rc::new(clock::Runtime::default());
    let wire = Rc::new(Wire {
        runtime: runtime.clone(),
        ..Default::default()
    });
    let store = Rc::new(DefaultStore::new());
    let host = ProductionHost::new(BreakoutDependencies {
        runtime: runtime.clone(),
        http: wire.clone(),
        external_public_data: None,
        local_to_utc: Rc::new(|value| value),
    });
    let exchange = BreakoutpropExchange::new(
        host.clone(),
        ExchangeName::Breakoutprop,
        serde_json::from_value(json!({"key":"k","secret":"s"})).unwrap(),
        store.clone(),
    )
    .unwrap();
    Fixture {
        runtime,
        wire,
        store,
        exchange,
        host,
    }
}
fn opts() -> PlaceOrderOpts {
    serde_json::from_value(
        json!({"symbol":"BTCUSD","type":"limit","side":"buy","amount":1.234,"price":99.26}),
    )
    .unwrap()
}
fn order() -> Order {
    serde_json::from_value(json!({"id":"old","status":"open","symbol":"BTCUSD","type":"limit","side":"buy","price":99.26,"amount":1.234,"filled":0,"remaining":1.234,"reduceOnly":false,"lastUpdate":0})).unwrap()
}
#[test]
fn start_loads_the_store_and_uses_polling_without_a_socket() {
    let f = fixture();
    f.run(f.exchange.start()).unwrap();
    assert!(
        f.store.loaded().unwrap().markets
            && f.store.loaded().unwrap().tickers
            && f.store.loaded().unwrap().balance
            && f.store.loaded().unwrap().orders
            && f.store.loaded().unwrap().positions
    );
    assert_eq!(f.store.balance().unwrap().total, 50.25);
    assert_eq!(f.store.markets().unwrap().len(), 1);
    assert_eq!(f.store.tickers().unwrap().len(), 1);
    f.exchange.dispose().unwrap();
    assert!(f.runtime.exceptions.borrow().is_empty());
}
#[test]
fn validate_account_logs_in_through_the_production_api() {
    let f = fixture();
    assert_eq!(
        f.run(f.exchange.validate_account())
            .unwrap()
            .error
            .as_value(),
        &json!("")
    );
    assert_eq!(
        f.request("POST", "/login")["data"],
        json!({"domain":"default","username":"k","password":"s"})
    );
}
#[test]
fn fetch_markets_reaches_both_instrument_endpoints() {
    let f = fixture();
    let rows = f.run(f.exchange.fetch_markets()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].symbol, "BTCUSD");
    assert_eq!(rows[0].precision.price, 0.1);
    assert_eq!(rows[0].precision.amount, 0.01);
    assert_eq!(rows[0].limits.leverage.max, 20.0);
    f.request("GET", "instruments/query");
    f.request("GET", "accounts/default:A/instruments/query");
}
#[test]
fn fetch_tickers_reads_quotes() {
    let f = fixture();
    f.fetch_markets();
    let rows = f.run(f.exchange.fetch_tickers()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].bid, 99.0);
    assert_eq!(rows[0].ask, 101.0);
    assert_eq!(rows[0].last, 99.0);
    f.request("POST", "marketdata");
}
#[test]
fn fetch_positions_is_a_pure_portfolio_read() {
    let f = fixture();
    f.fetch_markets();
    let before = f.store.positions().unwrap().to_vec();
    let rows = f
        .run(f.exchange.fetch_positions(Some(vec!["BTCUSD".into()])))
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].contracts, 0.0);
    assert_eq!(&f.store.positions().unwrap(), before.as_slice());
    assert!(!f.store.loaded().unwrap().positions);
    f.request("GET", "accounts/default:A/portfolio");
}
#[test]
fn fetch_orders_is_a_pure_portfolio_read() {
    let f = fixture();
    let rows = f.run(f.exchange.fetch_orders(None)).unwrap();
    assert!(rows.is_empty());
    assert!(f.store.orders().unwrap().is_empty());
    assert!(!f.store.loaded().unwrap().orders);
    let request = f.request("GET", "accounts/default:A/portfolio");
    assert_eq!(request["headers"]["Coinbase-Authorization"], "DXAPI token");
}
#[test]
fn place_order_uses_source_steps_and_returns_the_native_id_list() {
    let f = fixture();
    f.fetch_markets();
    let ids = f
        .run(f.exchange.base.place_order(f.exchange.as_ref(), opts()))
        .unwrap();
    assert_eq!(ids, vec!["placed"]);
    let request = f.request("POST", "accounts/default:A/orders");
    assert_eq!(request["data"]["quantity"], 1.23);
    assert_eq!(request["data"]["limitPrice"], 99.3);
    assert_eq!(request["data"]["orderCode"], "tealstreet-id");
    assert_eq!(request["data"]["positionEffect"], "OPEN");
}
#[test]
fn cancel_orders_sends_the_source_delete() {
    let f = fixture();
    f.run(f.exchange.cancel_orders(vec![order()])).unwrap();
    f.request("DELETE", "accounts/default:A/orders/old");
}
#[test]
fn update_order_cancels_before_replacement() {
    let f = fixture();
    f.fetch_markets();
    let update =
        serde_json::from_value::<UpdateOrderOptsUpdate>(json!({"amount":2.345,"price":102.16}))
            .unwrap();
    let ids = f
        .run(f.exchange.update_order(UpdateOrderOpts {
            order: order(),
            update,
        }))
        .unwrap();
    assert_eq!(ids, vec!["placed"]);
    let requests = f.wire.requests.borrow();
    let cancel = requests
        .iter()
        .position(|r| r["method"] == "DELETE")
        .unwrap();
    let place = requests
        .iter()
        .position(|r| r["method"] == "POST" && r["url"] == "accounts/default:A/orders")
        .unwrap();
    assert!(cancel < place);
    assert_eq!(requests[place]["data"]["quantity"], 2.35);
    assert_eq!(requests[place]["data"]["limitPrice"], 102.2);
}
#[test]
fn inherited_members_reject_exactly_without_a_request() {
    let f = fixture();
    let errors = vec![
        f.run(f.exchange.get_account()).unwrap_err(),
        f.run(f.exchange.fetch_symbol_max_leverage("BTCUSD".into()))
            .unwrap_err(),
        f.run(f.exchange.set_leverage("BTCUSD".into(), 2.0, None))
            .unwrap_err(),
        f.run(f.exchange.change_position_mode(true, None))
            .unwrap_err(),
        f.run(f.exchange.change_margin_type(true, None))
            .unwrap_err(),
        f.run(f.exchange.schedule_cancel(None)).unwrap_err(),
        f.run(f.exchange.fetch_public_trades("BTCUSD".into(), None))
            .unwrap_err(),
    ];
    assert_eq!(errors.len(), 7);
    for error in errors {
        assert_eq!(error.message, "Not implemented");
        assert_eq!(error.name, "Error");
    }
    assert!(f.wire.requests.borrow().is_empty());
}
#[test]
fn caller_client_id_refuses_before_login_or_http() {
    let f = fixture();
    let mut options = opts();
    options.client_order_id = Some("caller-id".into());
    let error = f
        .run(f.exchange.base.place_order(f.exchange.as_ref(), options))
        .unwrap_err();
    assert!(error.message.contains("clientOrderId"));
    assert!(f.wire.requests.borrow().is_empty());
}
#[test]
fn session_401_recovers_once_and_status_403_does_not_relogin() {
    let f = fixture();
    f.run(f.exchange.validate_account()).unwrap();
    f.wire.faults.borrow_mut().push_back((
        "accounts/default:A/portfolio".into(),
        401,
        json!({"msg":"expired"}),
    ));
    assert!(f.run(f.exchange.fetch_orders(None)).unwrap().is_empty());
    assert_eq!(
        f.wire
            .requests
            .borrow()
            .iter()
            .filter(|r| r["url"] == "/login")
            .count(),
        2
    );
    f.wire.faults.borrow_mut().push_back((
        "accounts/default:A/portfolio".into(),
        403,
        json!({"msg":"token expired"}),
    ));
    assert!(f.run(f.exchange.fetch_orders(None)).unwrap().is_empty());
    assert_eq!(
        f.wire
            .requests
            .borrow()
            .iter()
            .filter(|r| r["url"] == "/login")
            .count(),
        2
    );
}

#[test]
fn http_failure_preserves_axios_code_and_config() {
    let f = fixture();
    let error = safe_cex_exchange::js_runtime::value::error_object("AxiosError", "timeout");
    error.define("code", Js::str("ECONNABORTED")).unwrap();
    error.define("isAxiosError", Js::Bool(true)).unwrap();
    f.wire
        .errors
        .borrow_mut()
        .push_back(("instruments/query".into(), error));
    f.run(f.exchange.get_session_token()).unwrap();
    let client = f.exchange.xhr.borrow().clone().unwrap();
    let failure = f
        .run(client.request(safe_cex_transport::RestRequest {
            method: "GET".into(),
            url: "instruments/query".into(),
            ..Default::default()
        }))
        .unwrap_err();
    assert_eq!(failure.name, "AxiosError");
    assert_eq!(
        observed_error_code(&failure.code),
        Some(json!("ECONNABORTED"))
    );
    assert!(failure.is_axios_error);
    assert_eq!(failure.config.as_ref().unwrap()["url"], "instruments/query");
}

#[test]
fn poll_loop_uses_source_fallback_and_disables_infinity() {
    let source = scheduling_source();
    for (index, delay) in [
        None,
        Some(0.0),
        Some(-1.0),
        Some(f64::NAN),
        Some(125.0),
        Some(f64::INFINITY),
    ]
    .into_iter()
    .enumerate()
    {
        let f = fixture();
        let calls = Rc::new(Cell::new(0));
        let observed = calls.clone();
        let before = f.runtime.tasks.borrow().len();
        f.host
            .schedule_loop(
                Rc::new(move || {
                    observed.set(observed.get() + 1);
                    Box::pin(async { Ok(()) })
                }),
                delay,
            )
            .unwrap();
        let source = &source["loops"][index];
        if delay == Some(f64::INFINITY) {
            assert_eq!(source, &json!({"disabled":true,"calls":0}));
            assert_eq!(calls.get(), 0);
            assert_eq!(
                f.runtime.tasks.borrow().len(),
                before,
                "infinite interval schedules nothing"
            );
            continue;
        }
        let deadline = source["sleep_ms"].as_u64().unwrap();
        assert_eq!(source["queued_calls"], 0);
        assert_eq!(source["callback_ms"], 0);
        f.runtime.advance(0);
        assert_eq!(calls.get(), 0, "poll loop must not run immediately");
        f.runtime.advance(deadline - 1);
        assert_eq!(calls.get(), 0, "source interval must elapse");
        f.runtime.advance(deadline);
        // advance drains all ready turns, including the source's zero timer.
        assert_eq!(calls.get(), source["drained_calls"].as_u64().unwrap());
        f.runtime.advance(deadline);
        assert_eq!(calls.get(), 1, "deferred callback runs exactly once");
        f.exchange.dispose().unwrap();
    }
}

#[test]
fn cloned_production_host_retains_its_live_account_attachment() {
    let f = fixture();
    let cloned = Rc::new(f.host.as_ref().clone());
    let result = BreakoutpropExchange::new(
        cloned,
        ExchangeName::Breakoutprop,
        serde_json::from_value(json!({"key":"another-user","secret":"another-secret"})).unwrap(),
        Rc::new(DefaultStore::new()),
    );
    let error = match result {
        Err(error) => error,
        Ok(_) => panic!("cloning the host must not detach its live-account guard"),
    };
    assert_eq!(
        error.message,
        "Breakout Prop host already belongs to another account"
    );
    assert!(f.wire.requests.borrow().is_empty());
    f.run(f.exchange.validate_account()).unwrap();
    let login = f.request("POST", "/login");
    assert_eq!(login["data"]["username"], "k");
    assert_eq!(login["data"]["password"], "s");
}

// Finite production-method observations for the five compiler-indexed inherited
// refusals. These helpers are also included by accepted-set native adapters.
fn refusal_fixture(input: &Value) -> Fixture {
    let f = fixture();
    if input["seeded"].as_bool().unwrap() {
        *f.exchange.session_token.borrow_mut() = Some("cached-refusal-token".into());
        f.exchange.session_token_expires_at.set(9_876.25);
        *f.exchange.account_code.borrow_mut() = Some("cached-refusal-account".into());
        f.exchange
            .recent_ws_requests
            .borrow_mut()
            .push(("retained-poll-cache".into(), (42.25, vec![])));
        Store::update(
            f.store.as_ref(),
            StoreUpdate {
                balance: Some(Balance {
                    used: 1.25,
                    free: 3.5,
                    total: 4.75,
                    upnl: -0.0,
                    info: Some(json!({"refusal":"retained-info"}).into()),
                    currencies: None,
                    coins: None,
                }),
                ..Default::default()
            },
            false,
        )
        .unwrap();
    }
    f
}
fn same_refusal_handle<T: ?Sized>(a: &Option<Rc<T>>, b: &Option<Rc<T>>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Rc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}
fn observe_refusal(f: &Fixture, invoke: impl FnOnce() -> ExchangeError) -> Value {
    let notifications = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = notifications.clone();
    let unsubscribe = f.store.subscribe(Arc::new(move |_, _| {
        observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }));
    let emitted = Rc::new(Cell::new(0));
    let received = emitted.clone();
    f.exchange.base.on(
        "error",
        Rc::new(move |_, _| {
            received.set(received.get() + 1);
            Ok(())
        }),
    );
    let snapshot = || {
        let balance = f.store.balance().unwrap();
        (
            serde_json::to_value(f.store.serialize().unwrap()).unwrap(),
            [
                balance.used.to_bits(),
                balance.free.to_bits(),
                balance.total.to_bits(),
                balance.upnl.to_bits(),
            ],
            serde_json::to_value(f.exchange.base.options()).unwrap(),
            f.exchange.session_token.borrow().clone(),
            f.exchange.session_token_expires_at.get().to_bits(),
            f.exchange.account_code.borrow().clone(),
            f.exchange.recent_ws_requests.borrow().clone(),
            f.exchange.base.is_disposed(),
        )
    };
    let before = snapshot();
    let now = f.runtime.now.get();
    let tasks = f.runtime.tasks.borrow().len();
    let reports = f.runtime.exceptions.borrow().len();
    let pending = f.exchange.session_token_request.borrow().clone();
    let limited = f.exchange.xhr.borrow().clone();
    let unlimited = f.exchange.unlimited_xhr.borrow().clone();
    let socket = f.exchange.public_websocket.borrow().clone();
    let error = invoke();
    let result = json!({
        "error": error.message,
        "error_name": error.name,
        "metadata_default": *error.metadata == safe_cex_exchange::runtime::ExchangeErrorMetadata::default(),
        "http": f.wire.requests.borrow().len(),
        "state_unchanged": before == snapshot(),
        "notifications": notifications.load(std::sync::atomic::Ordering::SeqCst),
        "errors_emitted": emitted.get(),
        "now_unchanged": now == f.runtime.now.get(),
        "tasks_unchanged": tasks == f.runtime.tasks.borrow().len(),
        "reports_unchanged": reports == f.runtime.exceptions.borrow().len(),
        "login_handle_unchanged": same_refusal_handle(&pending, &f.exchange.session_token_request.borrow()),
        "limited_client_unchanged": same_refusal_handle(&limited, &f.exchange.xhr.borrow()),
        "unlimited_client_unchanged": same_refusal_handle(&unlimited, &f.exchange.unlimited_xhr.borrow()),
        "socket_handle_unchanged": same_refusal_handle(&socket, &f.exchange.public_websocket.borrow()),
    });
    unsubscribe();
    result
}
pub fn accepted_refusal_fetch_symbol_max_leverage(input: &Value) -> Value {
    let f = refusal_fixture(input);
    observe_refusal(&f, || {
        f.run(
            f.exchange
                .fetch_symbol_max_leverage(input["symbol"].as_str().unwrap().into()),
        )
        .unwrap_err()
    })
}
pub fn accepted_refusal_set_leverage(input: &Value) -> Value {
    let f = refusal_fixture(input);
    observe_refusal(&f, || {
        f.run(f.exchange.set_leverage(
            input["symbol"].as_str().unwrap().into(),
            input["leverage"].as_f64().unwrap(),
            None,
        ))
        .unwrap_err()
    })
}
pub fn accepted_refusal_change_position_mode(input: &Value) -> Value {
    let f = refusal_fixture(input);
    observe_refusal(&f, || {
        f.run(f.exchange.change_position_mode(
            input["flag"].as_bool().unwrap(),
            input["symbol"].as_str().map(str::to_owned),
        ))
        .unwrap_err()
    })
}
pub fn accepted_refusal_change_margin_type(input: &Value) -> Value {
    let f = refusal_fixture(input);
    observe_refusal(&f, || {
        f.run(f.exchange.change_margin_type(
            input["flag"].as_bool().unwrap(),
            input["symbol"].as_str().map(str::to_owned),
        ))
        .unwrap_err()
    })
}
pub fn accepted_refusal_fetch_public_trades(input: &Value) -> Value {
    let f = refusal_fixture(input);
    observe_refusal(&f, || {
        f.run(f.exchange.fetch_public_trades(
            input["symbol"].as_str().unwrap().into(),
            input["limit"].as_f64(),
        ))
        .unwrap_err()
    })
}
#[cfg(test)]
fn scheduling_source() -> &'static Value {
    static SOURCE: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    SOURCE.get_or_init(|| {
        let result = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/binding_scheduling_source.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let source: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(source["defaults"].as_object().unwrap().len(), 5);
        assert_eq!(source["loops"].as_array().unwrap().len(), 6);
        source
    })
}
#[cfg(test)]
fn expected_inherited_refusal(method: &str) -> Value {
    assert_eq!(
        scheduling_source()["defaults"][method],
        json!({"error":"Not implemented","error_name":"Error","timers":0})
    );
    json!({"error":"Not implemented","error_name":"Error","metadata_default":true,"http":0,"state_unchanged":true,"notifications":0,"errors_emitted":0,"now_unchanged":true,"tasks_unchanged":true,"reports_unchanged":true,"login_handle_unchanged":true,"limited_client_unchanged":true,"unlimited_client_unchanged":true,"socket_handle_unchanged":true})
}
fn inherited_refusal_cases() -> [Value; 2] {
    [
        json!({"seeded":false,"symbol":"BTCUSD","flag":true,"leverage":2.25,"limit":7}),
        json!({"seeded":true,"symbol":"雪/ETH","flag":false,"leverage":17.5,"limit":null}),
    ]
}
#[test]
fn inherited_max_leverage_retains_native_state_without_http() {
    for input in inherited_refusal_cases() {
        assert_eq!(
            accepted_refusal_fetch_symbol_max_leverage(&input),
            expected_inherited_refusal("fetchSymbolMaxLeverage")
        );
    }
}
#[test]
fn inherited_set_leverage_retains_native_state_without_http() {
    for input in inherited_refusal_cases() {
        assert_eq!(
            accepted_refusal_set_leverage(&input),
            expected_inherited_refusal("setLeverage")
        );
    }
}
#[test]
fn inherited_position_mode_retains_native_state_without_http() {
    for input in inherited_refusal_cases() {
        assert_eq!(
            accepted_refusal_change_position_mode(&input),
            expected_inherited_refusal("changePositionMode")
        );
    }
}
#[test]
fn inherited_margin_type_retains_native_state_without_http() {
    for input in inherited_refusal_cases() {
        assert_eq!(
            accepted_refusal_change_margin_type(&input),
            expected_inherited_refusal("changeMarginType")
        );
    }
}
#[test]
fn inherited_public_trades_retains_native_state_without_http() {
    for input in inherited_refusal_cases() {
        assert_eq!(
            accepted_refusal_fetch_public_trades(&input),
            expected_inherited_refusal("fetchPublicTrades")
        );
    }
}

// The accepted-set expression names its exact compiler owner and invokes the
// typed trait method on this observer's production receiver.
pub fn accepted_typed_refusal(
    input: &Value,
    invoke: impl FnOnce(
        Rc<BreakoutpropExchange>,
    ) -> std::pin::Pin<Box<dyn Future<Output = ExchangeError>>>,
) -> Value {
    let f = refusal_fixture(input);
    observe_refusal(&f, || f.run(invoke(f.exchange.clone())))
}

fn malformed_market_mapping(field: &str, value: Value, cause: &str) {
    let f = fixture();
    f.fetch_markets();
    let cached = f.store.markets().unwrap().to_vec();
    let mut row = detail();
    row[field] = value;
    f.wire.faults.borrow_mut().push_back((
        "accounts/default:A/instruments/query".into(),
        200,
        json!({"instrumentDetails":[detail(),row]}),
    ));
    let errors = Rc::new(RefCell::new(Vec::<Value>::new()));
    let observed = errors.clone();
    f.exchange.base.on(
        "error",
        Rc::new(move |_, value| {
            let value = safe_cex_exchange::js_runtime::value::to_json(value).unwrap();
            observed.borrow_mut().push(value.clone());
            Ok(())
        }),
    );
    let result = f.run(f.exchange.fetch_markets());
    assert!(
        result.is_ok(),
        "production malformed mapping must return cached markets: {result:?}"
    );
    assert_eq!(
        result.unwrap(),
        cached,
        "a valid prefix must not replace the cached markets"
    );
    assert_eq!(&f.store.markets().unwrap(), cached.as_slice());
    assert_eq!(errors.borrow().len(), 1);
    assert!(
        errors.borrow()[0].to_string().contains(cause),
        "source mapping failure cause is visible: {:?}",
        errors.borrow()
    );
}
#[test]
fn markets_null_margin_rate_reports_and_preserves_cache() {
    malformed_market_mapping(
        "marginRate",
        Value::Null,
        "Cannot read properties of null (reading 'tiers')",
    );
}
#[test]
fn markets_null_tiers_reports_and_preserves_cache() {
    malformed_market_mapping(
        "marginRate",
        json!({"tiers":null}),
        "Cannot read properties of null (reading '0')",
    );
}
#[test]
fn markets_null_maximum_reports_and_preserves_cache() {
    malformed_market_mapping(
        "maxOrderSize",
        Value::Null,
        "Cannot read properties of null (reading 'value')",
    );
}
#[test]
fn markets_ignore_unused_instrument_fields() {
    let f = fixture();
    let mut row = instrument();
    row.as_object_mut().unwrap().remove("description");
    row["priceIncrement"] = json!(0.25);
    f.wire.faults.borrow_mut().push_back((
        "instruments/query".into(),
        200,
        json!({"instruments":[row]}),
    ));
    let result = f.run(f.exchange.fetch_markets());
    assert!(
        result.is_ok(),
        "production market mapping must only read source fields: {result:?}"
    );
    let markets = result.unwrap();
    assert_eq!(markets.len(), 1);
    assert_eq!(markets[0].precision.price, 0.25);
}

fn unknown_enum_portfolio(field: &str) -> Value {
    let mut data = portfolio();
    data["portfolios"][0]["orders"] = json!([{
        "account":"default:A", "version":1, "orderId":7, "orderCode":"unknown-enum",
        "clientOrderId":"", "actionCode":"NEW", "legCount":1, "type":"LIMIT",
        "instrument":"BTCUSD", "status":"WORKING", "finalStatus":false,
        "legs":[{"instrument":"BTCUSD", "positionEffect":"OPEN", "positionCode":"p",
            "side":"BUY", "price":100, "legRatio":1, "quantity":2,
            "filledQuantity":0, "remainingQuantity":2, "averagePrice":0}],
        "side":"BUY", "tif":"GTC", "marginRate":0.05,
        "issueTime":"2026-01-01T00:00:00Z", "transactionTime":"2026-01-01T00:00:00Z",
        "executions":[], "cashTransactions":[]
    }]);
    if field == "positionSide" {
        data["portfolios"][0]["positions"] = json!([{
            "account":"default:A", "version":1, "positionCode":"p", "symbol":"BTCUSD",
            "quantity":2, "side":"UNKNOWN", "quantityNotional":200,
            "openTime":"2026-01-01T00:00:00Z", "openPrice":100,
            "lastUpdateTime":"2026-01-01T00:00:00Z", "marginRate":0.05
        }]);
    } else if field == "side" {
        data["portfolios"][0]["orders"][0]["legs"][0]["side"] = json!("UNKNOWN");
    } else {
        data["portfolios"][0]["orders"][0][field] = json!("UNKNOWN");
    }
    data
}

fn assert_unknown_enum_survives_start(field: &str) {
    let f = fixture();
    let data = unknown_enum_portfolio(field);
    // Startup reads the portfolio once, then starts its first polling tick.
    for _ in 0..2 {
        f.wire.faults.borrow_mut().push_back((
            "accounts/default:A/portfolio".into(),
            200,
            data.clone(),
        ));
    }
    f.run(f.exchange.start())
        .expect("unknown enum must not abort production startup");
    assert!(
        f.store.loaded().unwrap().balance
            && f.store.loaded().unwrap().positions
            && f.store.loaded().unwrap().orders
    );
    assert_eq!(
        f.store.orders().unwrap().len(),
        1,
        "unknown rows must be retained"
    );
    let row = if field == "positionSide" {
        serde_json::to_value(&f.store.positions().unwrap()[0]).unwrap()
    } else {
        serde_json::to_value(&f.store.orders().unwrap()[0]).unwrap()
    };
    let omitted = if field == "positionSide" {
        "side"
    } else {
        field
    };
    assert!(
        !row.as_object().unwrap().contains_key(omitted),
        "undefined must be omitted, not replaced: {row}"
    );
    assert!(
        f.wire
            .requests
            .borrow()
            .iter()
            .filter(|r| r["url"] == "marketdata")
            .count()
            >= 2,
        "unknown portfolio rows must not skip ticker polling"
    );
    f.exchange.dispose().unwrap();
}

#[test]
fn unknown_order_type_keeps_production_startup_and_polling_alive() {
    assert_unknown_enum_survives_start("type");
}
#[test]
fn unknown_order_status_keeps_production_startup_and_polling_alive() {
    assert_unknown_enum_survives_start("status");
}
#[test]
fn unknown_order_side_keeps_production_startup_and_polling_alive() {
    assert_unknown_enum_survives_start("side");
}
#[test]
fn unknown_position_side_keeps_production_startup_and_polling_alive() {
    assert_unknown_enum_survives_start("positionSide");
}

fn fetch_unknown_enum_order(f: &Fixture, field: &str) -> Order {
    f.fetch_markets();
    f.wire.faults.borrow_mut().push_back((
        "accounts/default:A/portfolio".into(),
        200,
        unknown_enum_portfolio(field),
    ));
    f.run(f.exchange.fetch_orders(None))
        .expect("unknown enum replacement row must survive real fetch")
        .remove(0)
}
#[test]
fn unknown_order_side_is_omitted_from_the_production_replacement_payload() {
    let f = fixture();
    let order = fetch_unknown_enum_order(&f, "side");
    let update = serde_json::from_value(json!({"price":102.16})).unwrap();
    assert_eq!(
        f.run(f.exchange.update_order(UpdateOrderOpts { order, update }))
            .unwrap(),
        vec!["placed"]
    );
    let requests = f.wire.requests.borrow();
    let payload = &requests
        .iter()
        .find(|r| r["method"] == "POST" && r["url"] == "accounts/default:A/orders")
        .unwrap()["data"];
    assert!(
        !payload.as_object().unwrap().contains_key("side"),
        "copied undefined side must stay omitted: {payload}"
    );
    assert_eq!(payload["type"], "LIMIT");
}
#[test]
fn unknown_order_type_replacement_reports_undefined_after_cancellation() {
    let f = fixture();
    let order = fetch_unknown_enum_order(&f, "type");
    let update = serde_json::from_value(json!({"price":102.16})).unwrap();
    let error = f
        .run(f.exchange.update_order(UpdateOrderOpts { order, update }))
        .unwrap_err();
    assert_eq!(
        error.message,
        "Breakout Prop does not support undefined orders"
    );
    assert!(
        f.wire
            .requests
            .borrow()
            .iter()
            .any(|r| r["method"] == "DELETE")
    );
    assert!(
        !f.wire
            .requests
            .borrow()
            .iter()
            .any(|r| r["method"] == "POST" && r["url"] == "accounts/default:A/orders")
    );
}

#[test]
fn generated_order_code_strips_nanoid_separators() {
    let f = fixture();
    f.fetch_markets();
    *f.runtime.id.borrow_mut() = Some("A_b-cD_e-fG_h-iJ_k-lM".into());
    assert_eq!(
        f.run(f.exchange.base.place_order(f.exchange.as_ref(), opts()))
            .unwrap(),
        ["placed"]
    );
    let request = f.request("POST", "accounts/default:A/orders");
    assert_eq!(
        request["data"]["orderCode"], "tealstreet-AbcDefGhiJklM",
        "production UUID strips both nanoid separators before prefixing"
    );
    assert_eq!(request["headers"]["Coinbase-Authorization"], "DXAPI token");
}

#[test]
fn real_trait_candles_reach_signed_post_and_preserve_both_rows() {
    let f = fixture();
    f.fetch_markets();
    *f.wire.candle_response.borrow_mut() = Some(json!({"events":[
        {"symbol":"BTCUSD","type":"Candle","candleType":"m","open":11,"high":15,"low":10,"close":14,"volume":7,"time":"2024-01-01T00:00:00Z"},
        {"symbol":"BTCUSD","type":"Candle","candleType":"m","open":21,"high":25,"low":20,"close":24,"volume":9,"time":"2024-01-01T00:01:00Z"}
    ]}));
    let venue: &dyn Exchange = f.exchange.as_ref();
    let rows=f.run(venue.fetch_ohlcv(serde_json::from_value(json!({"symbol":"BTCUSD","interval":"1m","from":1704067200123.0,"to":1704067260456.0,"limit":1})).unwrap())).unwrap();
    assert_eq!(
        rows.len(),
        2,
        "source ignores limit and retains all returned candles"
    );
    assert_eq!(
        (
            rows[0].timestamp,
            rows[0].open,
            rows[0].high,
            rows[0].low,
            rows[0].close,
            rows[0].volume
        ),
        (1704067200.0, 11.0, 15.0, 10.0, 14.0, 7.0)
    );
    assert_eq!(
        (rows[1].timestamp, rows[1].close, rows[1].volume),
        (1704067260.0, 24.0, 9.0)
    );
    let request = f.request("POST", "marketdata");
    let body: Value =
        serde_json::from_str(request["data"].as_str().expect("encoded candle JSON")).unwrap();
    assert_eq!(
        body,
        json!({"eventTypes":[{"type":"Candle","candleType":"m","fromTime":"2024-01-01T00:00:00.123Z","toTime":"2024-01-01T00:01:00.456Z"}],"symbols":["BTCUSD"],"account":"default:A"})
    );
    assert_eq!(request["headers"]["Coinbase-Authorization"], "DXAPI token");
    f.exchange.dispose().unwrap();
}

#[test]
fn real_trait_candles_propagate_original_http_failure() {
    let f = fixture();
    f.wire.faults.borrow_mut().push_back((
        "marketdata".into(),
        400,
        json!({"msg":"candle refusal"}),
    ));
    let venue: &dyn Exchange = f.exchange.as_ref();
    let error = f
        .run(venue.fetch_ohlcv(
            serde_json::from_value(json!({"symbol":"BTCUSD","interval":"1m"})).unwrap(),
        ))
        .expect_err("source rethrows the candle refusal");
    assert_eq!(
        error.response.as_ref().unwrap()["status"].as_f64(),
        Some(400.0)
    );
    assert_eq!(
        error.response.as_ref().unwrap()["data"]["msg"],
        "candle refusal"
    );
    assert_eq!(error.config.as_ref().unwrap()["url"], "marketdata");
    f.exchange.dispose().unwrap();
}

fn advance_real_poll(f: &Fixture) {
    for _ in 0..24 {
        f.runtime.advance(f.runtime.now.get() + 500);
    }
}

#[test]
fn real_trait_book_receives_polling_ticks_then_cleanup_removes_handler() {
    let f = fixture();
    let seen = Rc::new(RefCell::new(Vec::<OrderBook>::new()));
    let sink = seen.clone();
    let venue: &dyn Exchange = f.exchange.as_ref();
    let cleanup = venue
        .listen_order_book(
            "BTCUSD".into(),
            Rc::new(move |book| sink.borrow_mut().push(book)),
        )
        .unwrap();
    f.run(venue.start()).unwrap();
    advance_real_poll(&f);
    assert!(!seen.borrow().is_empty(), "actual quote loop delivers book");
    let book = seen.borrow().last().unwrap().clone();
    assert_eq!((book.bids.len(), book.asks.len()), (2, 2));
    assert_eq!(
        (
            book.bids[0].price,
            book.bids[1].price,
            book.asks[0].price,
            book.asks[1].price
        ),
        (99.0, 99.0, 101.0, 101.0)
    );
    assert_eq!((book.bids[0].amount, book.bids[0].total), (1.0, 1.0));
    cleanup().unwrap();
    let count = seen.borrow().len();
    advance_real_poll(&f);
    assert_eq!(
        seen.borrow().len(),
        count,
        "cleanup removes the source polling parser"
    );
    assert!(
        f.exchange
            .socket()
            .unwrap()
            .handlers("poll_book.BTCUSD")
            .unwrap()
            .borrow()
            .is_empty()
    );
    f.exchange.dispose().unwrap();
    assert!(f.runtime.exceptions.borrow().is_empty());
}

#[test]
fn real_trait_candle_listener_receives_polling_ticks_then_cleans_up() {
    let f = fixture();
    f.runtime.now.set(61_000);
    let seen = Rc::new(RefCell::new(Vec::<Candle>::new()));
    let sink = seen.clone();
    let venue: &dyn Exchange = f.exchange.as_ref();
    let cleanup = venue
        .listen_ohlcv(
            serde_json::from_value(json!({"symbol":"BTCUSD","interval":"1m"})).unwrap(),
            Rc::new(move |row| sink.borrow_mut().push(row)),
        )
        .unwrap();
    f.run(venue.start()).unwrap();
    advance_real_poll(&f);
    assert!(
        !seen.borrow().is_empty(),
        "actual quote loop delivers candle"
    );
    let row = seen.borrow().last().unwrap().clone();
    assert_eq!(
        (row.open, row.high, row.low, row.close, row.volume),
        (99.0, 99.0, 99.0, 99.0, 0.0)
    );
    assert_eq!(
        row.timestamp, 60.0,
        "clock-aligned minute from real polling host"
    );
    cleanup().unwrap();
    let count = seen.borrow().len();
    advance_real_poll(&f);
    assert_eq!(
        seen.borrow().len(),
        count,
        "cleanup removes source candle parser"
    );
    assert!(
        f.exchange
            .socket()
            .unwrap()
            .handlers("poll_kline.BTCUSD")
            .unwrap()
            .borrow()
            .is_empty()
    );
    f.exchange.dispose().unwrap();
    assert!(f.runtime.exceptions.borrow().is_empty());
}

#[test]
fn real_trait_candles_invalid_date_refuses_before_marketdata_post() {
    let f = fixture();
    let venue: &dyn Exchange = f.exchange.as_ref();
    let error = f
        .run(
            venue.fetch_ohlcv(
                serde_json::from_value(
                    json!({"symbol":"BTCUSD","interval":"1m","from":8640000000000001.0}),
                )
                .unwrap(),
            ),
        )
        .expect_err("source Date.toISOString refuses outside TimeClip");
    assert_eq!(error.name, "RangeError");
    assert_eq!(error.message, "Invalid time value");
    assert!(
        f.wire
            .requests
            .borrow()
            .iter()
            .all(|request| request["url"] != "marketdata")
    );
    f.exchange.dispose().unwrap();
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

#[test]
fn ticker_reads_ignore_missing_unused_declaration_fields() {
    let f = fixture();
    f.fetch_markets();
    let row = json!({"symbol":"BTCUSD","bid":91.25,"ask":94.5});
    *f.wire.candle_response.borrow_mut() = Some(json!({"events":[row.clone()]}));
    let rows = f.run(f.exchange.fetch_tickers()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (rows[0].bid, rows[0].ask, rows[0].last),
        (91.25, 94.5, 91.25)
    );
    assert_finite_native(rows[0].info.as_ref().unwrap(), &row);
}

#[test]
fn ticker_reads_ignore_malformed_unused_declaration_fields() {
    let f = fixture();
    f.fetch_markets();
    let row = json!({"symbol":"BTCUSD","bid":81.25,"ask":84.5,"type":{},"time":null});
    *f.wire.candle_response.borrow_mut() = Some(json!({"events":[row.clone()]}));
    let rows = f.run(f.exchange.fetch_tickers()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (rows[0].bid, rows[0].ask, rows[0].mark),
        (81.25, 84.5, 81.25)
    );
    assert_finite_native(rows[0].info.as_ref().unwrap(), &row);
}

fn malformed_ticker_fallback(prefix: Value) {
    let f = fixture();
    f.fetch_markets();
    let cached = f.run(f.exchange.fetch_tickers()).unwrap();
    Store::update(
        f.store.as_ref(),
        StoreUpdate {
            tickers: Some(cached.clone()),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let cached = f.store.tickers().unwrap().to_vec();
    let errors = Rc::new(RefCell::new(Vec::<Js>::new()));
    let observed = errors.clone();
    f.exchange.base.on(
        "error",
        Rc::new(move |_, value| {
            observed.borrow_mut().push(value.clone());
            Ok(())
        }),
    );
    *f.wire.candle_response.borrow_mut() = Some(json!({"events":[
        prefix,null
    ]}));
    let rows = f.run(f.exchange.fetch_tickers()).unwrap();
    assert_eq!(rows, cached);
    assert_eq!(f.store.tickers().unwrap().as_slice(), cached.as_slice());
    assert_eq!(errors.borrow().len(), 1);
    assert!(
        String::from_utf16(
            errors.borrow()[0]
                .as_str()
                .expect("source error event is a string")
        )
        .expect("fixture error message is valid UTF-16")
        .contains("Cannot read properties of null (reading 'symbol')")
    );
}

#[test]
fn account_lookup_reads_only_selected_account_fields() {
    let f = fixture();
    f.wire.faults.borrow_mut().push_back((
        "/users/k".into(),
        200,
        json!({"userDetails":[{"accounts":[
            {"accountStatus":"DISABLED"},
            {"account":"default:A","accountStatus":"FULL_TRADING"}
        ]}]}),
    ));
    f.run(f.exchange.start()).unwrap();
    assert_eq!(
        f.exchange.account_code.borrow().as_deref(),
        Some("default:A")
    );
    assert!(
        f.store.loaded().unwrap().orders
            && f.store.loaded().unwrap().positions
            && f.store.loaded().unwrap().balance
    );
    f.request("GET", "accounts/default:A/portfolio");
    f.run(f.exchange.fetch_orders(None)).unwrap();
    assert_eq!(
        f.wire
            .requests
            .borrow()
            .iter()
            .filter(|r| r["url"] == "/users/k")
            .count(),
        1
    );
    f.exchange.dispose().unwrap();
}

#[test]
fn account_lookup_ignores_unused_metadata_and_later_users() {
    let f = fixture();
    f.wire.faults.borrow_mut().push_back((
        "/users/k".into(),
        200,
        json!({"userDetails":[{
            "login":null,"domain":[],"version":{},"fullName":false,"username":12,
            "accounts":[{"account":"default:A","accountStatus":"FULL_TRADING",
                "owner":[],"version":null,"baseCurrency":{},"registrationTime":false,
                "pricingStream":12,"positionBased":"unused"}]
        },null,{"accounts":"ignored"}]}),
    ));
    f.run(f.exchange.fetch_orders(None)).unwrap();
    assert_eq!(
        f.exchange.account_code.borrow().as_deref(),
        Some("default:A")
    );
    f.request("GET", "accounts/default:A/portfolio");
}

fn source_only_portfolio() -> Value {
    json!({"portfolios":[{
        "balances":[{"currency":"USD","value":50.25}],
        "positions":[{"symbol":"BTCUSD","side":"BUY","openPrice":100.0,
            "quantity":2.0,"quantityNotional":200.0,"marginRate":0.05,
            "lastUpdateTime":"2026-01-01T00:00:00Z"}],
        "orders":[{"orderCode":"source-order","status":"WORKING","instrument":"BTCUSD",
            "type":"LIMIT","transactionTime":"2026-01-01T00:00:00Z",
            "legs":[{"side":"BUY","price":101.25,"quantity":3.0,"filledQuantity":1.0,
                "remainingQuantity":2.0,"positionEffect":"OPEN"}]}]
    }]})
}
fn queue_portfolio(f: &Fixture, value: Value) {
    f.wire
        .faults
        .borrow_mut()
        .push_back(("accounts/default:A/portfolio".into(), 200, value));
}
#[test]
fn portfolio_orders_ignore_unused_declaration_fields_and_unread_collections() {
    let f = fixture();
    let mut data = source_only_portfolio();
    data["portfolios"][0]["balances"] = json!([null]);
    data["portfolios"][0]["positions"] = json!([null]);
    queue_portfolio(&f, data.clone());
    let orders = f
        .run(f.exchange.fetch_orders(None))
        .expect("TS reads only orders here");
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].id, "source-order");
    assert_eq!(orders[0].amount, 3.0);
    assert_eq!(orders[0].price.value(), Some(&101.25));
    assert_finite_native(
        orders[0].info.value().unwrap(),
        &data["portfolios"][0]["orders"][0],
    );
}
#[test]
fn portfolio_positions_ignore_unused_declaration_fields_and_unread_collections() {
    let f = fixture();
    let mut data = source_only_portfolio();
    data["portfolios"][0]["balances"] = json!([null]);
    data["portfolios"][0]["orders"] = json!([null]);
    queue_portfolio(&f, data.clone());
    let positions = f
        .run(f.exchange.fetch_positions(Some(vec!["BTCUSD".into()])))
        .expect("TS reads only positions here");
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].symbol, "BTCUSD");
    assert_eq!(positions[0].contracts, 2.0);
    assert_eq!(positions[0].margin_size.value(), Some(&10.0));
    assert_finite_native(
        positions[0].info.value().unwrap(),
        &data["portfolios"][0]["positions"][0],
    );
}
#[test]
fn portfolio_balance_ignores_unused_declaration_fields_and_unread_orders() {
    let f = fixture();
    let mut data = source_only_portfolio();
    data["portfolios"][0]["orders"] = json!([null]);
    queue_portfolio(&f, data.clone());
    queue_portfolio(&f, data.clone());
    f.run(f.exchange.start())
        .expect("TS balance ignores order rows");
    let balance = f.store.balance().unwrap().clone();
    assert_eq!(balance.total, 48.25);
    assert_eq!(balance.upnl, -2.0);
    assert_eq!(balance.used, 10.0);
    assert_eq!(balance.free, 38.25);
    assert_finite_native(balance.info.as_ref().unwrap(), &data);
    f.exchange.dispose().unwrap();
}

#[test]
fn portfolio_array_like_root_retains_order_and_position_info() {
    let f = fixture();
    let mut data = source_only_portfolio();
    let portfolio = data["portfolios"][0].clone();
    data["portfolios"] = json!({"0":portfolio});
    queue_portfolio(&f, data.clone());
    let orders = f.run(f.exchange.fetch_orders(None)).unwrap();
    assert_eq!(orders.len(), 1);
    assert_finite_native(
        orders[0].info.value().unwrap(),
        &data["portfolios"]["0"]["orders"][0],
    );
    queue_portfolio(&f, data.clone());
    let positions = f
        .run(f.exchange.fetch_positions(Some(vec!["BTCUSD".into()])))
        .unwrap();
    assert_eq!(positions.len(), 1);
    assert_finite_native(
        positions[0].info.value().unwrap(),
        &data["portfolios"]["0"]["positions"][0],
    );
}

#[test]
fn portfolio_absent_open_positions_retains_filter_type_error() {
    let f = fixture();
    queue_portfolio(&f, json!({"portfolios":[]}));
    let error = f
        .run(f.exchange.fetch_positions(Some(vec!["BTCUSD".into()])))
        .unwrap_err();
    assert_eq!(error.name, "TypeError");
    assert_eq!(
        error.message,
        "Cannot read properties of undefined (reading 'filter')"
    );
}

#[test]
fn ticker_reads_report_consumed_field_failure_and_return_cached_tickers() {
    malformed_ticker_fallback(
        json!({"symbol":"BTCUSD","type":"Quote","time":"unused","bid":1,"ask":2}),
    );
}

#[test]
fn ticker_reads_defer_scalar_projection_until_source_mapping_completes() {
    malformed_ticker_fallback(
        json!({"symbol":"BTCUSD","type":"Quote","time":"unused","bid":"13","ask":14}),
    );
}

fn portfolio_error_log(f: &Fixture) -> Rc<RefCell<Vec<Js>>> {
    let errors = Rc::new(RefCell::new(Vec::new()));
    let observed = errors.clone();
    f.exchange.base.on(
        "error",
        Rc::new(move |_, value| {
            observed.borrow_mut().push(value.clone());
            Ok(())
        }),
    );
    errors
}

#[test]
fn portfolio_orders_defer_projection_until_source_mapping_completes() {
    let f = fixture();
    let errors = portfolio_error_log(&f);
    let mut data = source_only_portfolio();
    data["portfolios"][0]["orders"][0]["legs"][0]["price"] = json!("13");
    data["portfolios"][0]["orders"]
        .as_array_mut()
        .unwrap()
        .push(Value::Null);
    queue_portfolio(&f, data);
    let rows = f.run(f.exchange.fetch_orders(None)).unwrap();
    assert!(rows.is_empty());
    assert_eq!(errors.borrow().len(), 1);
    assert!(
        String::from_utf16(
            errors.borrow()[0]
                .as_str()
                .expect("source error event is a string")
        )
        .expect("fixture error message is valid UTF-16")
        .contains("Cannot read properties of null (reading 'orderCode')")
    );
}

#[test]
fn portfolio_positions_defer_projection_until_source_mapping_completes() {
    let f = fixture();
    let errors = portfolio_error_log(&f);
    let mut data = source_only_portfolio();
    data["portfolios"][0]["positions"][0]["openPrice"] = json!("100");
    data["portfolios"][0]["positions"]
        .as_array_mut()
        .unwrap()
        .push(Value::Null);
    queue_portfolio(&f, data);
    let rows = f.run(f.exchange.fetch_positions(Some(vec![]))).unwrap();
    assert!(rows.is_empty());
    assert_eq!(errors.borrow().len(), 1);
    assert!(
        String::from_utf16(
            errors.borrow()[0]
                .as_str()
                .expect("source error event is a string")
        )
        .expect("fixture error message is valid UTF-16")
        .contains("Cannot read properties of null (reading 'side')")
    );
}

#[test]
fn portfolio_balance_and_filtered_positions_accept_native_intermediate_scalars() {
    for side in ["BUY", "constructor"] {
        let f = fixture();
        let mut data = source_only_portfolio();
        let position = &mut data["portfolios"][0]["positions"][0];
        position["symbol"] = json!("EXCLUDED");
        position["openPrice"] = json!("100");
        position["quantity"] = json!("2");
        position["side"] = json!(side);
        queue_portfolio(&f, data.clone());
        queue_portfolio(&f, data);
        f.run(f.exchange.start()).unwrap();
        assert_eq!(
            (
                f.store.balance().unwrap().total,
                f.store.balance().unwrap().used,
                f.store.balance().unwrap().free
            ),
            (50.25, 10.0, 40.25)
        );
        assert_eq!(f.store.positions().unwrap().len(), 1);
        assert_eq!(f.store.positions().unwrap()[0].symbol, "BTCUSD");
        assert_eq!(f.store.positions().unwrap()[0].contracts, 0.0);
        f.exchange.dispose().unwrap();
    }
}

fn auth_request() -> safe_cex_transport::RestRequest {
    safe_cex_transport::RestRequest {
        method: "GET".into(),
        url: "/users/k".into(),
        ..Default::default()
    }
}
fn pump_pending<T>(f: &Fixture, future: &mut std::pin::Pin<Box<impl Future<Output = T>>>) {
    for _ in 0..32 {
        assert!(
            future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
        f.runtime.advance(f.runtime.now.get());
    }
}
#[test]
fn auth_pipeline_checks_session_after_rate_limit_dequeue() {
    let f = fixture();
    f.run(f.exchange.get_session_token()).unwrap();
    let client = f.exchange.xhr.borrow().clone().unwrap();
    client.rate_limit(1.).unwrap();
    f.run(client.request(auth_request())).unwrap();
    let mut queued = Box::pin(client.request(auth_request()));
    pump_pending(&f, &mut queued);
    f.exchange.invalidate_session();
    let error = f.run(queued).unwrap_err();
    assert_eq!(error.message, "No session token");
    assert_eq!(
        f.wire
            .requests
            .borrow()
            .iter()
            .filter(|r| r["url"] == "/users/k")
            .count(),
        1
    );
}
#[test]
fn auth_pipeline_reads_replacement_token_after_dequeue() {
    let f = fixture();
    f.run(f.exchange.get_session_token()).unwrap();
    let client = f.exchange.xhr.borrow().clone().unwrap();
    client.rate_limit(1.).unwrap();
    f.run(client.request(auth_request())).unwrap();
    let mut queued = Box::pin(client.request(auth_request()));
    pump_pending(&f, &mut queued);
    *f.exchange.session_token.borrow_mut() = Some("replacement".into());
    f.run(queued).unwrap();
    assert_eq!(
        f.request("GET", "/users/k")["headers"]["Coinbase-Authorization"],
        "DXAPI replacement"
    );
}
#[test]
fn auth_pipeline_reauthenticates_network_retry() {
    let f = fixture();
    f.run(f.exchange.get_session_token()).unwrap();
    let client = f.exchange.xhr.borrow().clone().unwrap();
    client.rate_limit(1.).unwrap();
    let error = safe_cex_exchange::js_runtime::value::error_object("Error", "Network Error");
    error.define("code", Js::str("ERR_NETWORK")).unwrap();
    f.wire
        .errors
        .borrow_mut()
        .push_back(("/users/k".into(), error));
    let mut retrying = Box::pin(client.request(auth_request()));
    pump_pending(&f, &mut retrying);
    assert_eq!(
        f.wire
            .requests
            .borrow()
            .iter()
            .filter(|r| r["url"] == "/users/k")
            .count(),
        1
    );
    *f.exchange.session_token.borrow_mut() = Some("replacement".into());
    f.run(retrying).unwrap();
    assert_eq!(
        f.request("GET", "/users/k")["headers"]["Coinbase-Authorization"],
        "DXAPI replacement"
    );
}

#[test]
fn auth_pipeline_preserves_overflow_url_params() {
    let f = fixture();
    f.run(f.exchange.get_session_token()).unwrap();
    let client = f.exchange.xhr.borrow().clone().unwrap();
    let mut request = auth_request();
    request.url = "/users/{username}".into();
    request.params = Some(serde_json::from_str(r#"{"username":1e400,"untouched":1e400}"#).unwrap());
    f.run(client.request(request)).unwrap();
    let sent = f.request("GET", "/users/Infinity");
    assert_eq!(sent["params"]["untouched"], Value::Null);
    let requests = f.wire.native_requests.borrow();
    let native = requests.last().unwrap();
    assert_eq!(
        native
            .get("params")
            .unwrap()
            .get("untouched")
            .unwrap()
            .as_f64(),
        Some(f64::INFINITY)
    );
    assert!(sent["params"].get("username").is_none());
}

#[test]
fn amendment_preserves_caller_id_and_refuses_replacement_after_cancellation() {
    let source = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/final_runtime_oracle.mjs"
        ))
        .arg("amendmentCallerId")
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let source: Value = serde_json::from_slice(&source.stdout).unwrap();
    let cases = source["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 4, "actual source caller-ID presence floor");
    for case in cases {
        let label = case["label"].as_str().unwrap();
        let f = fixture();
        f.fetch_markets();
        f.wire.requests.borrow_mut().clear();
        let emitted = Rc::new(RefCell::new(Vec::new()));
        let seen = emitted.clone();
        f.exchange.base.on_args(
            "error",
            Rc::new(move |_, args| {
                seen.borrow_mut().push(args.to_vec());
                Ok(())
            }),
        );
        let mut order = order();
        order.client_order_id = match label {
            "absent" => ModelField::Absent,
            "undefined" => ModelField::Undefined,
            "value" => ModelField::Value("caller-id".into()),
            "empty" => ModelField::Value(String::new()),
            _ => panic!("unexpected source case"),
        };
        let result = f.run(f.exchange.update_order(UpdateOrderOpts {
            order,
            update: serde_json::from_value(json!({"amount":2.345,"price":102.16})).unwrap(),
        }));
        let refused = case["result"].get("error").is_some();
        assert_eq!(
            result.is_err(),
            refused,
            "replacement refusal {label}: {result:?}"
        );
        if refused {
            assert_eq!(
                result.unwrap_err().message,
                case["result"]["error"].as_str().unwrap(),
                "original preflight error {label}"
            );
            let emitted = emitted.borrow();
            assert_eq!(emitted.len(), 1, "one visible refusal {label}");
            assert!(
                String::from_utf16(emitted[0][0].as_str().unwrap().as_ref())
                    .unwrap()
                    .contains(case["result"]["error"].as_str().unwrap()),
                "visible caller-ID refusal {label}: {emitted:?}"
            );
        } else {
            assert_eq!(result.unwrap(), ["placed"], "accepted replacement {label}");
            assert!(emitted.borrow().is_empty());
        }
        let requests = f.wire.requests.borrow();
        let money_methods: Vec<_> = requests
            .iter()
            .filter(|r| {
                r["url"]
                    .as_str()
                    .unwrap()
                    .starts_with("accounts/default:A/orders")
            })
            .map(|r| r["method"].as_str().unwrap())
            .collect();
        let expected: Vec<_> = case["methods"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m.as_str().unwrap())
            .collect();
        assert_eq!(
            money_methods, expected,
            "cancel then guarded replacement {label}"
        );
    }
}

fn retained_probe() -> Js {
    let probe = Js::object(vec![
        ("nan", Js::Number(f64::from_bits(0x7ff8000000000042))),
        ("infinity", Js::Number(f64::INFINITY)),
        ("negativeInfinity", Js::Number(f64::NEG_INFINITY)),
        ("zero", Js::Number(-0.0)),
        ("undefined", Js::Undefined),
        ("text", Js::utf16(&[0xd800])),
    ]);
    probe.define_key(&[0xdfff], Js::utf16(&[0xd800])).unwrap();
    probe
}
fn assert_retained_probe(model: &impl serde::Serialize, path: &[&str]) {
    let mut value = safe_cex_exchange::js_runtime::value::from_typed(model)
        .unwrap()
        .get("info")
        .unwrap();
    for key in path {
        value = value.get(key).unwrap();
    }
    let probe = value.get("retained_probe").unwrap();
    for (field, bits) in [
        ("nan", 0x7ff8000000000042),
        ("infinity", f64::INFINITY.to_bits()),
        ("negativeInfinity", f64::NEG_INFINITY.to_bits()),
        ("zero", (-0.0f64).to_bits()),
    ] {
        assert_eq!(
            probe.get(field).unwrap().as_f64().unwrap().to_bits(),
            bits,
            "native info {field}"
        );
    }
    assert!(probe.has_own("undefined"));
    assert!(probe.get("undefined").unwrap().is_undefined());
    assert!(matches!(probe.get("text").unwrap(), Js::String(units) if units.as_ref() == [0xd800]));
    assert!(
        matches!(probe.get_key(&[0xdfff]).unwrap(), Js::String(units) if units.as_ref() == [0xd800])
    );
}
#[test]
fn native_market_info_retains_nonfinite_undefined_and_utf16() {
    let f = fixture();
    let row = from_json(&instrument());
    row.set("retained_probe", retained_probe()).unwrap();
    f.wire.native_responses.borrow_mut().push_back((
        "instruments/query".into(),
        Js::object(vec![("instruments", Js::array(vec![row]))]),
    ));
    let rows = f.run(f.exchange.fetch_markets()).unwrap();
    assert_eq!(rows.len(), 1, "native market raw info row retained");
    assert_retained_probe(&rows[0], &["instrument"]);
    Store::update(
        f.store.as_ref(),
        StoreUpdate {
            markets: Some(rows),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    assert_retained_probe(&f.store.markets().unwrap()[0], &["instrument"]);
}
#[test]
fn native_portfolio_info_retains_nonfinite_undefined_and_utf16() {
    for method in ["order", "position"] {
        let f = fixture();
        f.fetch_markets();
        let body = from_json(&source_only_portfolio());
        let portfolio = body.get("portfolios").unwrap().get("0").unwrap();
        portfolio
            .get(if method == "order" {
                "orders"
            } else {
                "positions"
            })
            .unwrap()
            .get("0")
            .unwrap()
            .set("retained_probe", retained_probe())
            .unwrap();
        f.wire
            .native_responses
            .borrow_mut()
            .push_back(("accounts/default:A/portfolio".into(), body));
        if method == "order" {
            let rows = f.run(f.exchange.fetch_orders(None)).unwrap();
            assert_eq!(rows.len(), 1, "native order raw info row retained");
            assert_retained_probe(&rows[0], &[]);
        } else {
            let rows = f
                .run(f.exchange.fetch_positions(Some(vec!["BTCUSD".into()])))
                .unwrap();
            assert_eq!(rows.len(), 1, "native position raw info row retained");
            assert_retained_probe(&rows[0], &[]);
        }
    }
}
#[test]
fn native_ticker_info_retains_nonfinite_undefined_and_utf16() {
    let f = fixture();
    f.fetch_markets();
    let row = from_json(&json!({"symbol":"BTCUSD","bid":91.25,"ask":94.5}));
    row.set("retained_probe", retained_probe()).unwrap();
    f.wire.native_responses.borrow_mut().push_back((
        "marketdata".into(),
        Js::object(vec![("events", Js::array(vec![row]))]),
    ));
    let rows = f.run(f.exchange.fetch_tickers()).unwrap();
    assert_eq!(rows.len(), 1, "native ticker raw info row retained");
    assert_retained_probe(&rows[0], &[]);
}

#[test]
fn native_balance_keeps_surrogate_symbol_info_without_premature_ticker_projection() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let raw = source_only_portfolio()
        .to_string()
        .replace("\"symbol\":\"BTCUSD\"", "\"symbol\":\"\\ud800\"");
    let mut source = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/balance-symbol-oracle.cjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    source
        .stdin
        .take()
        .unwrap()
        .write_all(raw.as_bytes())
        .unwrap();
    let output = source.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(expected["symbolUnits"], json!([0xd800]));
    assert_eq!(expected["errors"], json!([]));
    let input: retained_json::RetainedJsonInput = serde_json::from_str(&raw).unwrap();
    let f = fixture();
    let errors = portfolio_error_log(&f);
    for _ in 0..2 {
        f.wire.native_responses.borrow_mut().push_back((
            "accounts/default:A/portfolio".into(),
            safe_cex_exchange::js_runtime::value::from_retained_input(input.clone()),
        ));
    }
    f.run(f.exchange.start()).unwrap();
    let balance = f.store.balance().unwrap().clone();
    for (name, number) in [
        ("total", balance.total),
        ("free", balance.free),
        ("used", balance.used),
        ("upnl", balance.upnl),
    ] {
        assert_eq!(
            format!("{:016x}", number.to_bits()),
            expected[name].as_str().unwrap(),
            "actual native balance {name}"
        );
    }
    let info = balance.info.as_ref().unwrap();
    let symbol = info.get("portfolios").unwrap().as_array().unwrap()[0]
        .get("positions")
        .unwrap()
        .as_array()
        .unwrap()[0]
        .get("symbol")
        .unwrap();
    assert_eq!(symbol.as_text(), Some([0xd800].as_slice()));
    assert_eq!(
        errors.borrow().len(),
        0,
        "ticker miss is not a mapping failure"
    );
    assert_eq!(
        f.wire
            .requests
            .borrow()
            .iter()
            .filter(|request| request["url"] == "accounts/default:A/portfolio")
            .count(),
        2
    );
    f.exchange.dispose().unwrap();
}

#[test]
fn native_candles_ignore_surrogate_symbol_without_declared_record_validation() {
    let source = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/candles_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let source: Value = serde_json::from_slice(&source.stdout).unwrap();
    let raw = source["supplements"]["surrogate"]["raw"].as_str().unwrap();
    let input: Input = serde_json::from_str(raw).unwrap();
    assert_eq!(
        input.get("events").unwrap().as_array().unwrap()[0]
            .get("symbol")
            .unwrap()
            .as_text(),
        Some([0xd800].as_slice())
    );
    let f = fixture();
    let errors = portfolio_error_log(&f);
    f.wire.native_responses.borrow_mut().push_back((
        "marketdata".into(),
        safe_cex_exchange::js_runtime::value::from_retained_input(input),
    ));
    let rows = f
        .run(f.exchange.fetch_ohlcv(OHLCVOptions {
            symbol: "BTCUSD".into(),
            interval: AnyTimeframe::Variant1(Timeframe::M1),
            from: None,
            to: None,
            limit: Some(1.0),
        }))
        .expect("source ignores the raw symbol field");
    assert_eq!(
        rows.len(),
        source["supplements"]["surrogate"]["count"]
            .as_u64()
            .unwrap() as usize
    );
    let expected = source["cases"][0]["expected"]["ok"].as_array().unwrap();
    assert_eq!(rows.len(), expected.len());
    for (row, expected) in rows.iter().zip(expected) {
        for (field, value) in [
            ("timestamp", row.timestamp),
            ("open", row.open),
            ("high", row.high),
            ("low", row.low),
            ("close", row.close),
            ("volume", row.volume),
        ] {
            assert_eq!(
                format!("{:016x}", value.to_bits()),
                expected[field]["number"].as_str().unwrap(),
                "actual candle field {field}"
            );
        }
    }
    assert!(errors.borrow().is_empty());
    assert_eq!(
        f.wire
            .requests
            .borrow()
            .iter()
            .filter(|r| r["url"] == "marketdata")
            .count(),
        1
    );
    f.exchange.dispose().unwrap();
}
