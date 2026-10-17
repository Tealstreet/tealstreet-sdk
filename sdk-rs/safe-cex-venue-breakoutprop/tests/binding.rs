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
                    json!({"events":[{"symbol":"BTCUSD","type":"Quote","bid":99,"ask":101,"time":"2026-01-01T00:00:00Z"}]})
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
        install_transport: Rc::new(|_| Err(ExchangeError::new("fixture has no tunnel transport"))),
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
