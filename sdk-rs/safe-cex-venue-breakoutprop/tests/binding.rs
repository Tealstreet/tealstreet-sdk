//! Construct the production host and call the typed Exchange surface through its
//! shared JavaScript functions/promises, with HTTP as the sole scripted leaf.
#[allow(dead_code)]
#[path = "../../safe-cex-webworker/tests/support/runtime.rs"]
mod clock;
use safe_cex_exchange::{
    exchange::Exchange,
    js_runtime::{
        client::JsHttpHost,
        promise::JsFuture,
        value::{Js, from_json, to_json},
    },
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime},
    websocket::{WebSocketConnectionOptions, WsRuntime, WsSocket},
};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_transport::{TransportProvider, socket::TunnelHost};
use safe_cex_types::*;
use safe_cex_venue_breakoutprop::{
    host::{BreakoutDependencies, ProductionHost},
    lifecycle::BreakoutpropExchange,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
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
        host,
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
