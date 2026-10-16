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

fn agent_address() -> String {
    safe_cex_crypto::wallet_utils::create_wallet_from_private_key(AGENT_KEY)
        .unwrap()
        .address()
        .to_string()
}

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
        "key": agent_address(),
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

fn venue_routes() -> Vec<(&'static str, Value)> {
    let ctx = |mark: &str| {
        json!({"funding":"0.0000125","openInterest":"100.5","prevDayPx":"60000.0","dayNtlVlm":"1000000.0",
               "premium":"0.0001","oraclePx":mark,"markPx":mark,"midPx":mark,"impactPxs":[mark,mark],"dayBaseVlm":"16.4"})
    };
    vec![
        (
            "info:metaAndAssetCtxs",
            json!([
                {"universe":[{"name":"BTC","szDecimals":5,"maxLeverage":40},{"name":"ETH","szDecimals":4,"maxLeverage":25}]},
                [ctx("61000.0"), ctx("3000.0")]
            ]),
        ),
        ("info:perpDexs", json!([null])),
        ("info:userFees", json!({"userCrossRate":"0.00035","userAddRate":"0.0001"})),
        ("info:maxBuilderFee", json!(10)),
        ("info:userAbstraction", json!("default")),
        (
            "info:clearinghouseState",
            json!({"marginSummary":{"accountValue":"1000.0","totalNtlPos":"0.0","totalRawUsd":"1000.0","totalMarginUsed":"0.0"},
                   "crossMarginSummary":{"accountValue":"1000.0","totalNtlPos":"0.0","totalRawUsd":"1000.0","totalMarginUsed":"0.0"},
                   "crossMaintenanceMarginUsed":"0.0","withdrawable":"1000.0","assetPositions":[],"time":1700000000000u64}),
        ),
        ("info:frontendOpenOrders", json!([])),
    ]
}

#[test]
fn start_loads_markets_balance_and_dials_both_sockets() {
    let f = fixture(venue_routes());
    let result = f.host.block_on(f.exchange.start());
    f.host.settle();
    assert!(result.is_ok(), "start: {result:?}; reports {:?}", f.host.reports.borrow());
    let store = f.exchange.base().store.clone();
    assert_eq!(
        store.markets().len(),
        2,
        "core perp markets loaded; requests {:?}; console {:?}; reports {:?}",
        f.routes.requests.borrow().iter().map(|(r, _)| r.clone()).collect::<Vec<_>>(),
        f.host.console.borrow(),
        f.host.reports.borrow()
    );
    assert!(store.loaded().markets && store.loaded().tickers, "loaded flags {:?}", *store.loaded());
    assert_eq!(f.host.sockets.borrow().len(), 2, "public and private sockets dialed");
    let problems: Vec<_> = f
        .host
        .console
        .borrow()
        .iter()
        .filter(|(severity, _)| severity == "error" || severity == "warning")
        .cloned()
        .collect();
    assert!(problems.is_empty(), "startup logged {problems:?}");
    assert!(f.host.reports.borrow().is_empty(), "startup reported {:?}", f.host.reports.borrow());
}

fn ok_action() -> Value {
    json!({"status":"ok","response":{"type":"default"}})
}
/// A started exchange with order/cancel/leverage actions answering success.
fn started() -> Fixture {
    let mut routes = venue_routes();
    routes.extend([
        ("exchange:order", json!({"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":777}}]}}})),
        ("exchange:cancel", json!({"status":"ok","response":{"type":"cancel","data":{"statuses":["success"]}}})),
        ("exchange:updateLeverage", ok_action()),
        ("exchange:scheduleCancel", ok_action()),
    ]);
    let f = fixture(routes);
    f.host.block_on(f.exchange.start()).unwrap_or_else(|e| panic!("start: {e:?}"));
    f.host.settle();
    f
}
fn last_body(f: &Fixture, route: &str) -> Value {
    f.routes
        .requests
        .borrow()
        .iter()
        .rev()
        .find(|(r, _)| r == route)
        .map(|(_, b)| b.clone())
        .unwrap_or_else(|| panic!("no {route} request; sent {:?}", f.routes.requests.borrow().iter().map(|(r, _)| r.clone()).collect::<Vec<_>>()))
}

#[test]
fn validate_account_checks_the_key_wallet_pair_without_a_request() {
    let f = fixture(vec![]);
    let result = f.host.block_on(f.exchange.validate_account()).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(result.error.as_value(), &json!(""), "matching pair: {result:?}");
    assert!(f.routes.requests.borrow().is_empty());
}

#[test]
fn fetch_positions_returns_core_positions() {
    let f = started();
    let positions = f.host.block_on(f.exchange.fetch_positions(None)).unwrap_or_else(|e| panic!("{e:?}"));
    assert!(positions.iter().all(|p| p.contracts == 0.0), "no open positions: {positions:?}");
}

#[test]
fn place_order_signs_a_cloid_order_and_returns_the_resting_oid() {
    let f = started();
    let opts: safe_cex_types::PlaceOrderOpts = serde_json::from_value(json!({
        "symbol":"BTC-PERP","type":"limit","side":"buy","amount":0.01,"price":60000
    }))
    .unwrap();
    let ids = f
        .host
        .block_on(f.exchange.base().place_order(&*f.exchange, opts))
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(ids, vec!["777".to_string()]);
    let body = last_body(&f, "exchange:order");
    let wire = &body["action"]["orders"][0];
    assert_eq!(wire["b"], json!(true));
    assert_eq!(wire["p"], json!("60000"));
    assert_eq!(wire["s"], json!("0.01"));
    assert!(wire["c"].as_str().is_some_and(|c| c.len() == 34), "minted cloid: {wire}");
    assert!(body["signature"]["r"].is_string(), "signed: {body}");
}

#[test]
fn cancel_orders_posts_one_cancel_per_order() {
    let f = started();
    let order: safe_cex_types::Order = serde_json::from_value(json!({
        "id":"777","status":"open","symbol":"BTC-PERP","type":"limit","side":"buy","price":60000,
        "amount":0.01,"filled":0,"remaining":0.01,"reduceOnly":false,"lastUpdate":1700000000000u64
    }))
    .unwrap();
    f.host.block_on(f.exchange.cancel_orders(vec![order])).unwrap_or_else(|e| panic!("{e:?}"));
    let body = last_body(&f, "exchange:cancel");
    assert_eq!(body["action"]["cancels"][0]["o"], json!(777));
}

#[test]
fn set_leverage_and_margin_type_post_update_leverage() {
    let f = started();
    f.host
        .block_on(f.exchange.set_leverage("BTC-PERP".into(), 7.0, None))
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(last_body(&f, "exchange:updateLeverage")["action"]["leverage"], json!(7));
    f.host
        .block_on(f.exchange.change_margin_type(true, Some("BTC-PERP".into())))
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(last_body(&f, "exchange:updateLeverage")["action"]["isCross"], json!(false));
}

#[test]
fn schedule_cancel_arms_and_clears() {
    let f = started();
    let at = 1_700_000_000_000.0 + 60_000.0;
    f.host.block_on(f.exchange.schedule_cancel(Some(Some(at)))).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(last_body(&f, "exchange:scheduleCancel")["action"]["time"], json!(at as u64));
    f.host.block_on(f.exchange.schedule_cancel(None)).unwrap_or_else(|e| panic!("{e:?}"));
    assert!(last_body(&f, "exchange:scheduleCancel")["action"].get("time").is_none(), "clear omits time");
}

#[test]
fn inherited_base_methods_reject_without_a_request() {
    let f = fixture(vec![]);
    let h = &f.host;
    let e = &f.exchange;
    let messages = [
        h.block_on(e.get_account()).map(|_| ()).unwrap_err().message,
        h.block_on(e.fetch_markets()).map(|_| ()).unwrap_err().message,
        h.block_on(e.fetch_tickers()).map(|_| ()).unwrap_err().message,
        h.block_on(e.fetch_symbol_max_leverage("BTC-PERP".into())).map(|_| ()).unwrap_err().message,
        h.block_on(e.change_position_mode(true, None)).map(|_| ()).unwrap_err().message,
        h.block_on(e.fetch_public_trades("BTC-PERP".into(), None)).map(|_| ()).unwrap_err().message,
    ];
    assert!(messages.iter().all(|m| m == "Not implemented"), "{messages:?}");
    assert!(f.routes.requests.borrow().is_empty());
}

#[test]
fn update_order_cancels_then_places_under_a_fresh_cloid() {
    let f = started();
    let opts: safe_cex_types::UpdateOrderOpts = serde_json::from_value(json!({
        "order": {"id":"555","status":"open","symbol":"BTC-PERP","type":"limit","side":"buy","price":60000,
                  "amount":0.01,"filled":0,"remaining":0.01,"reduceOnly":false,"lastUpdate":1700000000000u64},
        "update": {"price": 59000}
    }))
    .unwrap();
    let ids = f.host.block_on(f.exchange.update_order(opts)).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(ids, vec!["777".to_string()]);
    let sent: Vec<String> = f.routes.requests.borrow().iter().map(|(r, _)| r.clone()).collect();
    let cancel = sent.iter().position(|r| r == "exchange:cancel").expect("cancel sent");
    let order = sent.iter().rposition(|r| r == "exchange:order").expect("replacement sent");
    assert!(cancel < order, "cancel precedes the replacement: {sent:?}");
    assert_eq!(last_body(&f, "exchange:cancel")["action"]["cancels"][0]["o"], json!(555));
    let wire = &last_body(&f, "exchange:order")["action"]["orders"][0];
    assert_eq!(wire["p"], json!("59000"));
    assert!(wire["c"].as_str().is_some_and(|c| c.len() == 34));
}
