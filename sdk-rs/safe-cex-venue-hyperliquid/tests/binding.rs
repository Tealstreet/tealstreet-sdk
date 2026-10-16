//! The production binding end to end: `HyperliquidExchange::new` over a
//! scripted host, then every `Exchange` trait method.
#[allow(dead_code)]
#[path = "support/binding_runtime.rs"]
mod host;
use host::{Host, Routes};
use safe_cex_exchange::exchange::Exchange;
use safe_cex_store::store::DefaultStore;
use safe_cex_venue_hyperliquid::{
    binding::HyperliquidDependencies, exchange::HyperliquidExchange, rate_limit::RateLimiter,
};
use serde_json::{Value, json};
use std::{rc::Rc, sync::Arc};

const AGENT_KEY: &str = "0x0123456789012345678901234567890123456789012345678901234567890123";
const MASTER: &str = "0x14dc79964da2c08b23698b3d3cc7ca32193d9955";

struct Fixture {
    host: Rc<Host>,
    routes: Arc<Routes>,
    exchange: Rc<HyperliquidExchange>,
}
fn fixture(routes: Vec<(&str, Value)>) -> Fixture {
    let host = Rc::new(Host::default());
    let routes = Routes::new(routes);
    let clock = Arc::new(safe_cex_venue_hyperliquid::binding::RuntimeClock::new(host.clone()));
    let options = serde_json::from_value(json!({
        "key": "0x5e6bE1C1cBdE7d1D1b2d4ff9Ab6F6DA1db1D1e6F",
        "secret": AGENT_KEY,
        "applicationId": MASTER,
    }))
    .unwrap();
    let exchange = HyperliquidExchange::new(
        options,
        Rc::new(DefaultStore::new()),
        HyperliquidDependencies {
            runtime: host.clone(),
            http: routes.clone(),
            limiter: Rc::new(RateLimiter::new(clock, 1200.0, 60_000.0)),
        },
    )
    .unwrap_or_else(|e| panic!("construct: {e:?}"));
    Fixture { host, routes, exchange }
}

#[test]
fn constructs_through_the_ported_class_constructor() {
    let f = fixture(vec![]);
    assert!(!f.exchange.base().is_disposed());
    assert_eq!(f.routes.requests.borrow().len(), 0, "the constructor sends nothing");
    assert!(f.host.sockets.borrow().is_empty(), "the constructor dials nothing");
}

#[test]
fn fetch_orders_reads_frontend_open_orders() {
    let f = fixture(vec![("info:frontendOpenOrders", json!([]))]);
    let orders = f.host.block_on(f.exchange.fetch_orders(None)).unwrap_or_else(|e| panic!("{e:?}"));
    assert!(orders.is_empty());
    assert_eq!(f.routes.count("info:frontendOpenOrders"), 1);
}
