// Construct the production host and call the typed Exchange surface through its
// shared JavaScript functions/promises, with HTTP as the sole scripted leaf.
#[allow(dead_code)]
mod clock {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../safe-cex-webworker/tests/support/runtime.rs"
    ));
}
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
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_transport::{TransportProvider, socket::TunnelHost};
use safe_cex_types::*;
#[cfg(test)]
use safe_cex_venue_breakoutprop::native_reads::NativeRuntimeHost;
use safe_cex_venue_breakoutprop::{
    host::{BreakoutDependencies, ProductionHost},
    lifecycle::BreakoutpropExchange,
};
use serde_json::{Value, json};
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
    faults: RefCell<VecDeque<(String, u16, Value)>>,
    errors: RefCell<VecDeque<(String, Js)>>,
    candle_response: RefCell<Option<Value>>,
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
                ("GET", "/users/k") => {
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
        Box::pin(async move {
            Ok(Js::object(vec![
                ("status", Js::Number(status as f64)),
                ("data", from_json(&data)),
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
        for _ in 0..512 {
            if let Poll::Ready(value) = future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
            {
                return value;
            }
            self.runtime.advance(self.runtime.now.get() + 25);
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
        f.store.loaded().markets
            && f.store.loaded().tickers
            && f.store.loaded().balance
            && f.store.loaded().orders
            && f.store.loaded().positions
    );
    assert_eq!(f.store.balance().total, 50.25);
    assert_eq!(f.store.markets().len(), 1);
    assert_eq!(f.store.tickers().len(), 1);
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
    let before = f.store.positions().to_vec();
    let rows = f
        .run(f.exchange.fetch_positions(Some(vec!["BTCUSD".into()])))
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].contracts, 0.0);
    assert_eq!(&*f.store.positions(), before.as_slice());
    assert!(!f.store.loaded().positions);
    f.request("GET", "accounts/default:A/portfolio");
}
#[test]
fn fetch_orders_is_a_pure_portfolio_read() {
    let f = fixture();
    let rows = f.run(f.exchange.fetch_orders(None)).unwrap();
    assert!(rows.is_empty());
    assert!(f.store.orders().is_empty());
    assert!(!f.store.loaded().orders);
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
    assert_eq!(failure.code, Some(json!("ECONNABORTED")));
    assert!(failure.is_axios_error);
    assert_eq!(failure.config.as_ref().unwrap()["url"], "instruments/query");
}

#[test]
fn poll_loop_uses_source_fallback_and_disables_infinity() {
    for delay in [
        None,
        Some(0.0),
        Some(-1.0),
        Some(f64::NAN),
        Some(125.0),
        Some(f64::INFINITY),
    ] {
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
        if delay == Some(f64::INFINITY) {
            assert_eq!(
                f.runtime.tasks.borrow().len(),
                before,
                "infinite interval schedules nothing"
            );
            continue;
        }
        let deadline = if delay == Some(125.0) { 125 } else { 5000 };
        f.runtime.advance(0);
        assert_eq!(calls.get(), 0, "poll loop must not run immediately");
        f.runtime.advance(deadline - 1);
        assert_eq!(calls.get(), 0, "source interval must elapse");
        f.runtime.advance(deadline);
        assert_eq!(calls.get(), 0, "source enqueues callback after sleep");
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
                    info: Some(json!({"refusal":"retained-info"})),
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
        let balance = f.store.balance();
        (
            serde_json::to_value(f.store.serialize()).unwrap(),
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
fn expected_inherited_refusal() -> Value {
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
            expected_inherited_refusal()
        );
    }
}
#[test]
fn inherited_set_leverage_retains_native_state_without_http() {
    for input in inherited_refusal_cases() {
        assert_eq!(
            accepted_refusal_set_leverage(&input),
            expected_inherited_refusal()
        );
    }
}
#[test]
fn inherited_position_mode_retains_native_state_without_http() {
    for input in inherited_refusal_cases() {
        assert_eq!(
            accepted_refusal_change_position_mode(&input),
            expected_inherited_refusal()
        );
    }
}
#[test]
fn inherited_margin_type_retains_native_state_without_http() {
    for input in inherited_refusal_cases() {
        assert_eq!(
            accepted_refusal_change_margin_type(&input),
            expected_inherited_refusal()
        );
    }
}
#[test]
fn inherited_public_trades_retains_native_state_without_http() {
    for input in inherited_refusal_cases() {
        assert_eq!(
            accepted_refusal_fetch_public_trades(&input),
            expected_inherited_refusal()
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
    assert!(f.store.loaded().balance && f.store.loaded().positions && f.store.loaded().orders);
    assert_eq!(f.store.orders().len(), 1, "unknown rows must be retained");
    let row = if field == "positionSide" {
        serde_json::to_value(&f.store.positions()[0]).unwrap()
    } else {
        serde_json::to_value(&f.store.orders()[0]).unwrap()
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
