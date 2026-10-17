//! The production binding end to end: `HyperliquidExchange::new` over a
//! scripted host, then every `Exchange` trait method.
#[allow(dead_code)]
#[path = "support/binding_runtime.rs"]
mod host;
use host::{Host, Routes};
use safe_cex_exchange::exchange::Exchange;
use safe_cex_exchange::runtime::{Cleanup, ExchangeSigners, RemoteSigner};
use safe_cex_store::store::DefaultStore;
use safe_cex_venue_hyperliquid::{
    binding::HyperliquidDependencies, exchange::HyperliquidExchange, rate_limit::RateLimiter,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

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
#[allow(clippy::arc_with_non_send_sync)]
fn fixture(routes: Vec<(&str, Value)>) -> Fixture {
    fixture_with_options(
        routes,
        json!({
            "key": agent_address(),
            "secret": AGENT_KEY,
            "applicationId": MASTER,
        }),
    )
}

#[allow(clippy::arc_with_non_send_sync)]
fn fixture_with_options(routes: Vec<(&str, Value)>, options: Value) -> Fixture {
    fixture_with_signers(routes, options, ExchangeSigners::default())
}

#[allow(clippy::arc_with_non_send_sync)]
fn fixture_with_signers(
    routes: Vec<(&str, Value)>,
    options: Value,
    signers: ExchangeSigners,
) -> Fixture {
    fixture_with_provider(routes, options, signers, None)
}

#[allow(clippy::arc_with_non_send_sync)]
fn fixture_with_provider(
    routes: Vec<(&str, Value)>,
    options: Value,
    signers: ExchangeSigners,
    external_public_data: Option<
        safe_cex_exchange::js_runtime::base_exchange::ExternalPublicDataFactory,
    >,
) -> Fixture {
    let host = Rc::new(Host::default());
    let routes = Routes::new(routes);
    let clock = Arc::new(safe_cex_venue_hyperliquid::binding::RuntimeClock::new(
        host.clone(),
    ));
    let options = serde_json::from_value(options).unwrap();
    let exchange = HyperliquidExchange::new(
        options,
        Rc::new(DefaultStore::new()),
        HyperliquidDependencies {
            runtime: host.clone(),
            external_public_data,
            http: routes.clone(),
            limiter: Rc::new(RateLimiter::new(clock, 1200.0, 60_000.0)),
            signers,
        },
    )
    .unwrap_or_else(|e| panic!("construct: {e:?}"));
    Fixture {
        host,
        routes,
        exchange,
    }
}

#[derive(Default)]
struct ExternalProvider {
    connects: Cell<usize>,
    disposes: Cell<usize>,
}
impl safe_cex_exchange::external::ExternalPublicData for ExternalProvider {
    fn connect(&self) {
        self.connects.set(self.connects.get() + 1);
    }
    fn dispose(&self) {
        self.disposes.set(self.disposes.get() + 1);
    }
    fn is_initialized(&self) -> bool {
        true
    }
    fn is_healthy(&self) -> bool {
        true
    }
    fn last_message_timestamp(&self) -> Option<f64> {
        Some(0.0)
    }
    fn listen_ohlcv(
        &self,
        _: safe_cex_types::OHLCVOptions,
        _: Rc<dyn Fn(safe_cex_types::Candle)>,
    ) -> Result<
        safe_cex_exchange::runtime::ListenerCleanup,
        safe_cex_exchange::runtime::ExchangeError,
    > {
        panic!("startup does not subscribe to candles")
    }
    fn listen_order_book(
        &self,
        _: String,
        _: Rc<dyn Fn(safe_cex_types::OrderBook)>,
    ) -> Result<
        safe_cex_exchange::runtime::ListenerCleanup,
        safe_cex_exchange::runtime::ExchangeError,
    > {
        panic!("startup does not subscribe to order books")
    }
}

fn external_options() -> Value {
    json!({"key":"", "secret":"", "publicDataOnly":true,
        "externalPublicData":{"type":"websocket", "url":"ws://gateway.local", "exchangeId":"hyperliquid"}})
}

#[test]
fn external_startup_waits_for_both_loaded_flags_and_disposes_its_provider() {
    use std::task::Context;
    let provider = Rc::new(ExternalProvider::default());
    let supplied = provider.clone();
    let parent = Rc::new(RefCell::new(None));
    let observed = parent.clone();
    let f = fixture_with_provider(
        vec![],
        external_options(),
        ExchangeSigners::default(),
        Some(Rc::new(move |_, owner| {
            *observed.borrow_mut() = Some(owner);
            supplied.clone()
        })),
    );
    let mut start = f.exchange.start();
    let mut context = Context::from_waker(std::task::Waker::noop());
    assert!(start.as_mut().poll(&mut context).is_pending());
    f.host.settle();
    assert_eq!(provider.connects.get(), 1);
    assert!(Rc::ptr_eq(
        &parent.borrow().as_ref().unwrap().store,
        &f.exchange.base().store,
    ));
    let store = &f.exchange.base().store;
    let mut loaded = store.loaded().clone();
    loaded.markets = true;
    store
        .update(
            safe_cex_store::StoreUpdate {
                loaded: Some(loaded.clone()),
                ..Default::default()
            },
            false,
        )
        .unwrap();
    f.host.advance(100.0);
    assert!(
        start.as_mut().poll(&mut context).is_pending(),
        "tickers must also hydrate"
    );
    loaded.tickers = true;
    store
        .update(
            safe_cex_store::StoreUpdate {
                loaded: Some(loaded),
                ..Default::default()
            },
            false,
        )
        .unwrap();
    f.host.block_on(start).unwrap();
    assert!(
        f.routes.requests.borrow().is_empty(),
        "external startup skips venue public REST"
    );
    assert!(
        f.host.sockets.borrow().is_empty(),
        "external startup skips venue public sockets"
    );
    f.exchange.dispose().unwrap();
    assert_eq!(provider.disposes.get(), 1);
}

#[test]
fn external_configuration_without_a_provider_fails_visibly_before_network() {
    let f = fixture_with_options(vec![], external_options());
    let error = f.host.block_on(f.exchange.start()).unwrap_err();
    assert_eq!(
        error.message,
        "externalPublicData is configured but no external public data provider was supplied"
    );
    assert!(f.routes.requests.borrow().is_empty());
    assert!(f.host.sockets.borrow().is_empty());
}

#[derive(Default)]
struct HostedSigner {
    available: Cell<bool>,
    listener: Rc<RefCell<Option<Rc<dyn Fn(bool)>>>>,
}
impl safe_cex_transport::signing::RemoteSigner for HostedSigner {
    fn sign(
        &self,
        _: safe_cex_transport::signing::RemoteSignerRequest,
    ) -> safe_cex_transport::TransportFuture<'_, safe_cex_transport::signing::RemoteSignerResponse>
    {
        Box::pin(async {
            Err(safe_cex_transport::TransportError::Host(
                "availability test must not send a signing request".into(),
            ))
        })
    }
}
impl RemoteSigner for HostedSigner {
    fn is_available(&self) -> bool {
        self.available.get()
    }
    fn on_availability_change(&self, callback: Rc<dyn Fn(bool)>) -> Cleanup {
        assert!(self.listener.borrow_mut().replace(callback).is_none());
        let listener = self.listener.clone();
        Box::new(move || {
            listener.borrow_mut().take();
        })
    }
}

#[test]
fn hosted_signer_reaches_native_capability_and_availability_without_a_secret() {
    let signer = Rc::new(HostedSigner::default());
    let f = fixture_with_signers(
        vec![],
        json!({
            "key": agent_address(), "secret":"", "applicationId":MASTER,
        }),
        ExchangeSigners {
            remote_signer: Some(signer.clone()),
            payload_signer: None,
        },
    );
    assert!(
        f.exchange.can_sign(),
        "native capability must see the hosted signer"
    );
    assert!(f.exchange.base().options().secret.is_empty());
    let wait = f.exchange.base().wait_for_remote_signer();
    let callback = signer
        .listener
        .borrow()
        .clone()
        .expect("availability subscribed");
    signer.available.set(true);
    callback(true);
    assert!(f.host.block_on(wait).unwrap());
    assert!(
        signer.listener.borrow().is_none(),
        "settled wait removes listener"
    );
    assert!(f.routes.requests.borrow().is_empty());
}

#[test]
fn constructs_through_the_ported_class_constructor() {
    let f = fixture(vec![]);
    assert!(!f.exchange.base().is_disposed());
    assert_eq!(
        f.routes.requests.borrow().len(),
        0,
        "the constructor sends nothing"
    );
    assert!(
        f.host.sockets.borrow().is_empty(),
        "the constructor dials nothing"
    );
}

#[test]
fn historical_positions_fold_real_fills_with_account_bounds_and_settlement() {
    let f = started();
    f.routes.routes.borrow_mut().extend([
        ("info:userFillsByTime".into(), json!([
            {"coin":"BTC","tid":1,"oid":10,"time":1699999998000u64,"side":"B","px":"61000","sz":"1","fee":"1","feeToken":"USDC","closedPnl":"0","dir":"Open Long","startPosition":"0"},
            {"coin":"BTC","tid":2,"oid":11,"time":1699999999000u64,"side":"A","px":"62000","sz":"1","fee":"2","feeToken":"USDC","closedPnl":"1000","dir":"Close Long","startPosition":"1"}
        ])),
        ("info:userFunding".into(), json!([])),
    ]);
    let records = f
        .host
        .block_on(f.exchange.fetch_historical_positions(
            safe_cex_exchange::journal::HistoricalPositionOptions {
                account: "audit-account".into(),
                from: Some(1_699_999_000_000.0),
                to: Some(1_700_000_000_000.0),
            },
        ))
        .unwrap();
    assert_eq!(
        records.len(),
        1,
        "a real closed round trip must be returned; requests {:?}; console {:?}; reports {:?}",
        f.routes.requests.borrow(),
        f.host.console.borrow(),
        f.host.reports.borrow()
    );
    let record: safe_cex_types::PositionRecord =
        serde_json::from_value(records[0].clone()).unwrap();
    assert_eq!(record.account, "audit-account");
    assert_eq!(record.symbol, "BTC-PERP");
    assert_eq!(
        record.settle_currency,
        f.exchange.base().store.markets_by_symbol()["BTC-PERP"].quote
    );
    assert_eq!(record.realized_pnl, 1000.0);
    assert_eq!(record.fees, 3.0);
    let requests = f.routes.requests.borrow();
    let income = requests
        .iter()
        .find(|(route, _)| route == "info:userFillsByTime")
        .unwrap();
    assert_eq!(
        income.1["startTime"].as_f64(),
        Some(1_699_999_000_000.0 - 14.0 * 86400000.0)
    );
    assert_eq!(income.1["endTime"], json!(1_700_000_000_000u64));
    assert_eq!(income.1["user"], MASTER);
    assert!(
        f.routes.count("info:userFillsByTime") > 1,
        "symbol-scoped fold fetches executions after income discovery"
    );
}

#[test]
fn historical_positions_default_window_uses_integer_date_now_and_shared_lookback() {
    let f = fixture(vec![
        ("info:userFillsByTime", json!([])),
        ("info:userFunding", json!([])),
    ]);
    f.host.now.set(0.75);
    let records = f
        .host
        .block_on(f.exchange.fetch_historical_positions(
            safe_cex_exchange::journal::HistoricalPositionOptions {
                account: "empty-account".into(),
                from: None,
                to: None,
            },
        ))
        .unwrap();
    assert!(records.is_empty());
    let requests = f.routes.requests.borrow();
    let first = &requests[0].1;
    assert_eq!(first["type"], "userFillsByTime");
    assert_eq!(first["endTime"].as_f64(), Some(1_700_000_000_000.0));
    assert_eq!(
        first["startTime"].as_f64(),
        Some(1_700_000_000_000.0 - 44.0 * 86400000.0)
    );
    assert_eq!(f.routes.count("info:userFunding"), 1);
    assert!(f.host.console.borrow().is_empty());
    assert!(f.host.reports.borrow().is_empty());
}

#[test]
fn fetch_orders_reads_frontend_open_orders() {
    let f = fixture(vec![("info:frontendOpenOrders", json!([]))]);
    let orders = f
        .host
        .block_on(f.exchange.fetch_orders(None))
        .unwrap_or_else(|e| panic!("{e:?}"));
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
        (
            "info:userFees",
            json!({"userCrossRate":"0.00035","userAddRate":"0.0001"}),
        ),
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
    assert!(
        result.is_ok(),
        "start: {result:?}; reports {:?}",
        f.host.reports.borrow()
    );
    let store = f.exchange.base().store.clone();
    assert_eq!(
        store.markets().len(),
        2,
        "core perp markets loaded; requests {:?}; console {:?}; reports {:?}",
        f.routes
            .requests
            .borrow()
            .iter()
            .map(|(r, _)| r.clone())
            .collect::<Vec<_>>(),
        f.host.console.borrow(),
        f.host.reports.borrow()
    );
    assert!(
        store.loaded().markets && store.loaded().tickers,
        "loaded flags {:?}",
        *store.loaded()
    );
    assert_eq!(
        f.host.sockets.borrow().len(),
        2,
        "public and private sockets dialed"
    );
    let problems: Vec<_> = f
        .host
        .console
        .borrow()
        .iter()
        .filter(|(severity, _)| severity == "error" || severity == "warning")
        .cloned()
        .collect();
    assert!(problems.is_empty(), "startup logged {problems:?}");
    assert!(
        f.host.reports.borrow().is_empty(),
        "startup reported {:?}",
        f.host.reports.borrow()
    );
}

#[test]
fn resolved_legacy_identity_reaches_native_options_and_privacy_checks() {
    for role in ["user", "vault", "agent", "subAccount"] {
        let mut routes = venue_routes();
        routes.push((
            "info:userRole",
            json!({"role":role,"data":{"user":MASTER,"master":MASTER}}),
        ));
        let f = fixture_with_options(
            routes,
            json!({
                "key": agent_address(),
                "secret": AGENT_KEY,
            }),
        );
        assert!(f.exchange.is_public_data_only());
        f.host.block_on(f.exchange.start()).unwrap();
        let options = f.exchange.base().options();
        let (identity, expected) = match role {
            "vault" => (options.vault.as_deref(), agent_address()),
            "user" => (options.application_id.as_deref(), agent_address()),
            _ => (options.application_id.as_deref(), MASTER.to_string()),
        };
        assert_eq!(
            identity,
            Some(expected.as_str()),
            "resolved identity must reach the native BaseExchange options for {role}"
        );
        assert!(!f.exchange.is_public_data_only(), "{role}");
        assert!(f.exchange.has_private_session_identity(), "{role}");
        assert_eq!(f.routes.count("info:userRole"), 1, "{role}");
    }
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
    f.host
        .block_on(f.exchange.start())
        .unwrap_or_else(|e| panic!("start: {e:?}"));
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
        .unwrap_or_else(|| {
            panic!(
                "no {route} request; sent {:?}",
                f.routes
                    .requests
                    .borrow()
                    .iter()
                    .map(|(r, _)| r.clone())
                    .collect::<Vec<_>>()
            )
        })
}

#[test]
fn validate_account_checks_the_key_wallet_pair_without_a_request() {
    let f = fixture(vec![]);
    let result = f
        .host
        .block_on(f.exchange.validate_account())
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(
        result.error.as_value(),
        &json!(""),
        "matching pair: {result:?}"
    );
    assert!(f.routes.requests.borrow().is_empty());
}

#[test]
fn fetch_positions_returns_core_positions() {
    let f = started();
    let positions = f
        .host
        .block_on(f.exchange.fetch_positions(None))
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert!(
        positions.iter().all(|p| p.contracts == 0.0),
        "no open positions: {positions:?}"
    );
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
    assert!(
        wire["c"].as_str().is_some_and(|c| c.len() == 34),
        "minted cloid: {wire}"
    );
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
    f.host
        .block_on(f.exchange.cancel_orders(vec![order]))
        .unwrap_or_else(|e| panic!("{e:?}"));
    let body = last_body(&f, "exchange:cancel");
    assert_eq!(body["action"]["cancels"][0]["o"], json!(777));
}

#[test]
fn set_leverage_and_margin_type_post_update_leverage() {
    let f = started();
    f.host
        .block_on(f.exchange.set_leverage("BTC-PERP".into(), 7.0, None))
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(
        last_body(&f, "exchange:updateLeverage")["action"]["leverage"],
        json!(7)
    );
    f.host
        .block_on(f.exchange.change_margin_type(true, Some("BTC-PERP".into())))
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(
        last_body(&f, "exchange:updateLeverage")["action"]["isCross"],
        json!(false)
    );
}

#[test]
fn schedule_cancel_arms_and_clears() {
    let f = started();
    let at = safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms() + 60_000.0;
    f.host
        .block_on(f.exchange.schedule_cancel(Some(Some(at))))
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(
        last_body(&f, "exchange:scheduleCancel")["action"]["time"],
        json!(at as u64)
    );
    f.host
        .block_on(f.exchange.schedule_cancel(None))
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert!(
        last_body(&f, "exchange:scheduleCancel")["action"]
            .get("time")
            .is_none(),
        "clear omits time"
    );
}

#[test]
fn inherited_base_methods_reject_without_a_request() {
    let f = fixture(vec![]);
    let h = &f.host;
    let e = &f.exchange;
    let messages = [
        h.block_on(e.get_account()).map(|_| ()).unwrap_err().message,
        h.block_on(e.fetch_markets())
            .map(|_| ())
            .unwrap_err()
            .message,
        h.block_on(e.fetch_tickers())
            .map(|_| ())
            .unwrap_err()
            .message,
        h.block_on(e.fetch_symbol_max_leverage("BTC-PERP".into()))
            .map(|_| ())
            .unwrap_err()
            .message,
        h.block_on(e.change_position_mode(true, None))
            .map(|_| ())
            .unwrap_err()
            .message,
        h.block_on(e.fetch_public_trades("BTC-PERP".into(), None))
            .map(|_| ())
            .unwrap_err()
            .message,
    ];
    assert!(
        messages.iter().all(|m| m == "Not implemented"),
        "{messages:?}"
    );
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
    let ids = f
        .host
        .block_on(f.exchange.update_order(opts))
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(ids, vec!["777".to_string()]);
    let sent: Vec<String> = f
        .routes
        .requests
        .borrow()
        .iter()
        .map(|(r, _)| r.clone())
        .collect();
    let cancel = sent
        .iter()
        .position(|r| r == "exchange:cancel")
        .expect("cancel sent");
    let order = sent
        .iter()
        .rposition(|r| r == "exchange:order")
        .expect("replacement sent");
    assert!(cancel < order, "cancel precedes the replacement: {sent:?}");
    assert_eq!(
        last_body(&f, "exchange:cancel")["action"]["cancels"][0]["o"],
        json!(555)
    );
    let wire = &last_body(&f, "exchange:order")["action"]["orders"][0];
    assert_eq!(wire["p"], json!("59000"));
    assert!(wire["c"].as_str().is_some_and(|c| c.len() == 34));
}

#[test]
fn fractional_host_clock_still_produces_integer_date_now_signing_nonces() {
    let f = fixture(vec![(
        "exchange:scheduleCancel",
        json!({"status":"ok","response":{"type":"scheduleCancel","data":{"status":"success"}}}),
    )]);
    f.host.now.set(0.75);
    f.host.block_on(f.exchange.schedule_cancel(None)).unwrap();
    let body = last_body(&f, "exchange:scheduleCancel");
    assert_eq!(body["nonce"].as_f64(), Some(1_700_000_000_000.0));
    assert!(body["signature"]["r"].is_string());
}

fn production_trigger_opts(amount: f64) -> safe_cex_types::PlaceOrderOpts {
    serde_json::from_value(json!({"symbol":"BTC-PERP","type":"stop_market","side":"sell", "amount":amount,"price":59000,"reduceOnly":true})).unwrap()
}

#[test]
fn production_import_position_tpsl_posts_signed_zero_size_trigger() {
    let f = started();
    let ids = f
        .host
        .block_on(
            f.exchange
                .base()
                .place_order(&*f.exchange, production_trigger_opts(0.0)),
        )
        .expect("production position TP/SL must resolve its helpers");
    assert_eq!(ids, ["777"]);
    let body = last_body(&f, "exchange:order");
    assert_eq!(body["action"]["grouping"], "positionTpsl");
    assert_eq!(body["action"]["orders"][0]["s"], "0");
    assert_eq!(
        body["action"]["orders"][0]["t"]["trigger"]["triggerPx"],
        "59000"
    );
    assert_eq!(body["action"]["orders"][0]["t"]["trigger"]["tpsl"], "sl");
    assert!(body["signature"]["r"].is_string());
}

#[test]
fn production_import_partial_tpsl_posts_its_amount_with_normal_grouping() {
    let f = started();
    let ids = f
        .host
        .block_on(
            f.exchange
                .base()
                .place_order(&*f.exchange, production_trigger_opts(0.01)),
        )
        .expect("production partial TP/SL must resolve its helpers");
    assert_eq!(ids, ["777"]);
    let body = last_body(&f, "exchange:order");
    assert_eq!(body["action"]["grouping"], "na");
    assert_eq!(body["action"]["orders"][0]["s"], "0.01");
    assert!(body["signature"]["r"].is_string());
}

#[test]
fn production_import_waiting_tpsl_returns_empty_ids_and_schedules_refresh() {
    let f = started();
    for (name, response) in &mut *f.routes.routes.borrow_mut() {
        if name == "exchange:order" {
            *response = json!({"status":"ok","response":{"type":"order","data":{"statuses":["waitingForTrigger"]}}});
        }
    }
    let before = f.routes.count("info:frontendOpenOrders");
    let result = f.host.block_on(
        f.exchange
            .base()
            .place_order(&*f.exchange, production_trigger_opts(0.0)),
    );
    assert_eq!(
        f.routes.count("exchange:order"),
        1,
        "the waiting trigger is already posted before its response is interpreted"
    );
    let ids = result.expect("an already-live waiting trigger must not become a ReferenceError");
    assert!(ids.is_empty());
    let after_return = f.routes.count("info:frontendOpenOrders");
    assert!(after_return >= before);
    f.host.advance(500.0);
    assert_eq!(
        f.routes.count("info:frontendOpenOrders"),
        after_return + 1,
        "waiting TP/SL schedules the actual order refresh"
    );
}

fn production_batch_opts() -> Vec<safe_cex_types::PlaceOrderOpts> {
    [json!({"symbol":"BTC-PERP","type":"limit","side":"buy","amount":0.01,"price":60000,"clientOrderId":"0x11111111111111111111111111111111"}), json!({"symbol":"BTC-PERP","type":"limit","side":"sell","amount":0.02,"price":62000,"clientOrderId":"0x22222222222222222222222222222222"})].into_iter().map(|v|serde_json::from_value(v).unwrap()).collect()
}

#[test]
fn production_import_place_orders_posts_and_flattens_both_results_in_order() {
    let f = started();
    let ids = f
        .host
        .block_on(
            f.exchange
                .base()
                .place_orders(&*f.exchange, production_batch_opts()),
        )
        .expect("production placeOrders must resolve mapSeries");
    assert_eq!(ids, ["777", "777"]);
    let requests = f.routes.requests.borrow();
    let orders = requests
        .iter()
        .filter(|(r, _)| r == "exchange:order")
        .map(|(_, b)| b)
        .collect::<Vec<_>>();
    assert_eq!(orders.len(), 2);
    for (index, (amount, price, cloid)) in [
        ("0.01", "60000", "0x11111111111111111111111111111111"),
        ("0.02", "62000", "0x22222222222222222222222222222222"),
    ]
    .into_iter()
    .enumerate()
    {
        let wire = &orders[index]["action"]["orders"][0];
        assert_eq!(wire["s"], amount);
        assert_eq!(wire["p"], price);
        assert_eq!(wire["c"], cloid);
        assert!(orders[index]["signature"]["r"].is_string());
    }
}

#[test]
fn production_import_place_orders_stops_after_the_first_venue_failure() {
    let f = started();
    for (name, response) in &mut *f.routes.routes.borrow_mut() {
        if name == "exchange:order" {
            *response = json!({"status":"ok","response":{"type":"order","data":{"statuses":[{"error":"first batch denied"}]}}});
        }
    }
    let error = f
        .host
        .block_on(
            f.exchange
                .base()
                .place_orders(&*f.exchange, production_batch_opts()),
        )
        .unwrap_err();
    assert!(
        error.message.contains("first batch denied"),
        "original venue error must survive: {error:?}"
    );
    assert_eq!(
        f.routes.count("exchange:order"),
        1,
        "serial placement must not start the second order after first rejection"
    );
}

#[test]
fn production_import_live_fills_emit_each_fill_update_later_positions_and_refresh_hip3() {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_transport::socket::SocketFrame;
    let f = started();
    let events = Rc::new(RefCell::new(Vec::new()));
    let observed = events.clone();
    f.exchange.base().on(
        "fill",
        Rc::new(move |_, event| {
            let event = safe_cex_exchange::js_runtime::value::to_json(event).unwrap();
            observed.borrow_mut().push(event.clone());
            Ok(())
        }),
    );
    let wires = f.host.sockets.borrow().clone();
    assert_eq!(wires.len(), 2);
    for wire in &wires {
        wire.push(WsEvent::Open);
    }
    f.host.settle();
    let private = wires
        .iter()
        .find(|wire| {
            wire.sent.borrow().iter().any(|text| {
                serde_json::from_str::<Value>(text)
                    .is_ok_and(|v| v["subscription"]["type"] == "userFills")
            })
        })
        .expect("production private socket subscribes to userFills");
    let fill = |coin: &str, size: &str, start: &str, tid: u64| json!({"coin":coin,"px":"60000","sz":size,"side":"B","time":1700000000000u64+tid,"startPosition":start,"dir":"Open Long","closedPnl":"0","hash":format!("fill-{tid}"),"oid":tid+1000,"crossed":false,"fee":"0","tid":tid});
    let frame = json!({"channel":"userFills","data":{"isSnapshot":false,"user":MASTER,"fills":[fill("BTC","0.1","0",101),fill("BTC","0.2","0.1",102),fill("xyz:BTC","0.05","0",103)]}}).to_string();
    private.push(WsEvent::Message(SocketFrame::Text(frame.clone())));
    f.host.settle();
    assert_eq!(
        events.borrow().len(),
        3,
        "every live fill must reach the actual BaseExchange emitter"
    );
    for (i, event) in events.borrow().iter().enumerate() {
        assert_eq!(event["id"], (101 + i).to_string());
        assert_eq!(event["orderId"], (1101 + i).to_string());
        assert_eq!(event["side"], "buy");
    }
    let positions = f.exchange.base().store.positions();
    let position = positions
        .iter()
        .find(|p| p.symbol == "BTC-PERP" && p.side == safe_cex_types::PositionSide::Long)
        .expect("optimistic core position");
    assert!(
        (position.contracts - 0.3).abs() < 1e-12,
        "later fills must update positions: {position:?}"
    );
    drop(positions);
    private.push(WsEvent::Message(SocketFrame::Text(frame)));
    f.host.settle();
    assert_eq!(
        events.borrow().len(),
        3,
        "shared cache deduplicates notification replay"
    );
    assert!(
        !f.routes
            .requests
            .borrow()
            .iter()
            .any(|(r, b)| r == "info:clearinghouseState" && b["dex"] == "xyz")
    );
    // The debounce starts the request; the shared limiter still owns its pacing.
    f.host.advance(350.0);
    f.host.advance(5000.0);
    assert!(
        f.routes
            .requests
            .borrow()
            .iter()
            .any(|(r, b)| r == "info:clearinghouseState" && b["dex"] == "xyz"),
        "HIP-3 fill scheduling must survive the whole frame: console={:?} reports={:?} requests={:?}",
        f.host.console.borrow(),
        f.host.reports.borrow(),
        f.routes.requests.borrow()
    );
}

fn seam_started() -> Fixture {
    let f = started();
    // Join the startup's actual coalesced order read before seeding assertions.
    f.host.block_on(f.exchange.fetch_orders(None)).unwrap();
    f.host.settle();
    f
}
fn seam_order() -> safe_cex_types::Order {
    serde_json::from_value(json!({"id":"777","status":"open","symbol":"BTC-PERP","type":"stop_market","side":"sell","price":59000,"amount":0.01,"filled":0,"remaining":0.01,"reduceOnly":true,"lastUpdate":1700000000000u64})).unwrap()
}
fn seam_route(f: &Fixture, route: &str, response: Value) {
    let mut routes = f.routes.routes.borrow_mut();
    if let Some((_, value)) = routes.iter_mut().find(|(name, _)| name == route) {
        *value = response;
    } else {
        routes.push((route.into(), response));
    }
}
fn seam_fill() -> Value {
    json!({"coin":"BTC","tid":901,"oid":777,"time":1699999999000u64,"side":"A","px":"62000","sz":"0.25","fee":"2","feeToken":"USDC","closedPnl":"125","dir":"Close Long","startPosition":"0.25","crossed":true,"hash":"authored-history-fill"})
}
fn seam_core_state() -> Value {
    json!({"marginSummary":{"accountValue":"2400","totalNtlPos":"15000","totalRawUsd":"2400","totalMarginUsed":"60"},"crossMarginSummary":{"accountValue":"2400","totalNtlPos":"15000","totalRawUsd":"2400","totalMarginUsed":"60"},"withdrawable":"2340","crossMaintenanceMarginUsed":"30","time":1700000000100u64,"assetPositions":[{"type":"oneWay","position":{"coin":"BTC","szi":"0.25","entryPx":"60000","positionValue":"15000","unrealizedPnl":"125","returnOnEquity":"0.1","marginUsed":"60","liquidationPx":null,"maxLeverage":40,"leverage":{"type":"cross","value":5},"cumFunding":{"allTime":"0","sinceOpen":"0","sinceChange":"0"}}}]})
}

#[test]
fn production_seam_terminal_cancel_removes_the_local_order_after_the_venue_miss() {
    let f = seam_started();
    let order = seam_order();
    f.exchange
        .base()
        .store
        .add_or_update_orders(vec![order.clone()])
        .unwrap();
    seam_route(
        &f,
        "exchange:cancel",
        json!({"status":"ok","response":{"type":"cancel","data":{"statuses":[{"error":"Order was never placed, already canceled, or filled. asset=0"}]}}}),
    );
    let result = f.host.block_on(f.exchange.cancel_orders(vec![order]));
    assert_eq!(f.routes.count("exchange:cancel"), 1);
    result.expect("a terminal venue miss must resolve through the real production import");
    assert!(
        f.exchange
            .base()
            .store
            .orders()
            .iter()
            .all(|o| o.id != "777"),
        "successful terminal cancellation clears the actual Store"
    );
}

#[test]
fn production_seam_nonterminal_cancel_retains_the_original_error_and_order() {
    let f = seam_started();
    let order = seam_order();
    f.exchange
        .base()
        .store
        .add_or_update_orders(vec![order.clone()])
        .unwrap();
    seam_route(
        &f,
        "exchange:cancel",
        json!({"status":"ok","response":{"type":"cancel","data":{"statuses":[{"error":"authored cancel rejection"}]}}}),
    );
    let error = f
        .host
        .block_on(f.exchange.cancel_orders(vec![order]))
        .unwrap_err();
    assert!(
        error.message.contains("authored cancel rejection"),
        "original error must survive classification: {error:?}"
    );
    assert!(
        f.exchange
            .base()
            .store
            .orders()
            .iter()
            .any(|o| o.id == "777")
    );
}

#[test]
fn production_seam_fetch_candles_reaches_the_venue_and_returns_native_values() {
    let f = seam_started();
    seam_route(
        &f,
        "info:candleSnapshot",
        json!([{"t":1699999940000u64,"T":1700000000000u64,"o":"60000","h":"61000","l":"59000","c":"60500","v":"12.5"}]),
    );
    let options = serde_json::from_value(
        json!({"symbol":"BTC-PERP","interval":"1m","limit":2,"to":1700000000000u64}),
    )
    .unwrap();
    let candles = f.host.block_on(f.exchange.fetch_ohlcv(options)).unwrap();
    assert_eq!(
        candles.len(),
        1,
        "the shared empty default must not handle candles"
    );
    let candle = &candles[0];
    assert_eq!(
        (
            candle.timestamp,
            candle.open,
            candle.high,
            candle.low,
            candle.close,
            candle.volume
        ),
        (1699999940.0, 60000.0, 61000.0, 59000.0, 60500.0, 12.5)
    );
    let request = last_body(&f, "info:candleSnapshot");
    assert_eq!(
        request["req"],
        json!({"coin":"BTC","interval":"1m","startTime":1699999880000u64,"endTime":1700000000000u64})
    );
}

#[test]
fn production_seam_lookup_uses_order_status_by_cloid_without_mutating_orders() {
    use safe_cex_exchange::client_order_id::ClientOrderLookup;
    let f = seam_started();
    let id = "0x11111111111111111111111111111111";
    seam_route(
        &f,
        "info:orderStatus",
        json!({"status":"order","order":{"status":"filled","order":{"coin":"BTC","oid":777,"side":"B","limitPx":"60000","sz":"0","origSz":"0.25","timestamp":1699999999000u64,"orderType":"Limit","reduceOnly":false,"cloid":id}}}),
    );
    let before = f.routes.count("info:frontendOpenOrders");
    let result = f
        .host
        .block_on(
            f.exchange
                .find_order_by_client_order_id("BTC-PERP".into(), id.into()),
        )
        .unwrap();
    let ClientOrderLookup::Found(order) = result else {
        panic!("production lookup must find the filled order via orderStatus")
    };
    assert_eq!(order.id, "777");
    assert_eq!(order.status, safe_cex_types::OrderStatus::Closed);
    assert_eq!(order.client_order_id.as_deref(), Some(id));
    assert_eq!(
        last_body(&f, "info:orderStatus"),
        json!({"type":"orderStatus","user":MASTER,"oid":id})
    );
    assert_eq!(
        f.routes.count("info:frontendOpenOrders"),
        before,
        "lookup must not refresh/mutate the open-order snapshot"
    );
    assert!(f.exchange.base().store.orders().is_empty());
}

#[test]
fn production_seam_reconnect_snapshot_refreshes_core_positions_and_balance() {
    let f = seam_started();
    seam_route(&f, "info:clearinghouseState", seam_core_state());
    let before = f.routes.count("info:clearinghouseState");
    f.host
        .block_on(f.exchange.fetch_and_set_private_snapshot(3))
        .unwrap();
    assert_eq!(
        f.routes.count("info:clearinghouseState"),
        before + 1,
        "the real reconnect override fetches core state alongside orders"
    );
    let positions = f.exchange.base().store.positions();
    let btc = positions
        .iter()
        .find(|p| p.symbol == "BTC-PERP" && p.side == safe_cex_types::PositionSide::Long)
        .expect("core position");
    assert_eq!(btc.contracts, 0.25);
    assert_eq!(f.exchange.base().store.balance().total, 2275.0);
    assert!(f.exchange.base().store.loaded().orders);
}

#[test]
fn production_seam_historical_fills_reaches_the_native_execution_mapper() {
    let f = seam_started();
    seam_route(&f, "info:userFillsByTime", json!([seam_fill()]));
    let options = serde_json::from_value(
        json!({"symbol":"BTC-PERP","from":1699999900000u64,"to":1700000000000u64,"limit":10}),
    )
    .unwrap();
    let fills = f
        .host
        .block_on(f.exchange.fetch_historical_fills(options))
        .unwrap();
    assert_eq!(
        fills.len(),
        1,
        "actual execution rows must replace the empty default"
    );
    let fill = &fills[0];
    assert_eq!(fill.id.as_deref(), Some("901"));
    assert_eq!(fill.order_id.as_deref(), Some("777"));
    assert_eq!(
        (fill.price, fill.qty, fill.timestamp),
        (62000.0, 0.25, 1699999999.0)
    );
    assert_eq!(fill.symbol, "BTC-PERP");
    assert_eq!(fill.commission, Some(2.0));
    assert_eq!(fill.side, safe_cex_types::OrderSide::Sell);
    assert!(fill.position_side.is_none());
    assert_eq!(last_body(&f, "info:userFillsByTime")["user"], MASTER);
}

#[test]
fn production_seam_historical_orders_collapses_and_maps_real_venue_rows() {
    let f = seam_started();
    let row = |status: &str, remaining: &str, time: u64| json!({"order":{"coin":"BTC","oid":777,"timestamp":1699999998000u64,"side":"B","origSz":"0.25","sz":remaining,"orderType":"Limit","limitPx":"60000","reduceOnly":false,"cloid":"0x11111111111111111111111111111111"},"status":status,"statusTimestamp":time});
    seam_route(
        &f,
        "info:historicalOrders",
        json!([
            row("open", "0.25", 1699999998000),
            row("filled", "0", 1699999999000)
        ]),
    );
    let options = serde_json::from_value(
        json!({"symbol":"BTC-PERP","from":1699999990000u64,"to":1700000000000u64}),
    )
    .unwrap();
    let orders = f
        .host
        .block_on(f.exchange.fetch_historical_orders(Some(options)))
        .unwrap();
    assert_eq!(
        orders.len(),
        1,
        "collapse and mapping imports must both execute"
    );
    let order = &orders[0];
    assert_eq!(order.id, "777");
    assert_eq!(order.symbol, "BTC-PERP");
    assert_eq!(order.status, safe_cex_types::OrderStatus::Closed);
    assert_eq!(
        (order.amount, order.filled, order.created_time),
        (0.25, 0.25, 1699999998000.0)
    );
    assert_eq!(
        last_body(&f, "info:historicalOrders"),
        json!({"type":"historicalOrders","user":MASTER})
    );
}

fn seam_income(f: &Fixture) {
    seam_route(f, "info:userFillsByTime", json!([seam_fill()]));
    seam_route(f, "info:userFunding", json!([]));
}
fn assert_seam_income(records: &[safe_cex_types::IncomeRecord]) {
    assert_eq!(
        records.len(),
        2,
        "the real fill produces both realized PnL and fee rows"
    );
    for (kind, income) in [("REALIZED_PNL", 125.0), ("COMMISSION", -2.0)] {
        let row = records
            .iter()
            .find(|r| r.income_type == kind)
            .expect("typed income row");
        assert_eq!(row.symbol, "BTC-PERP");
        assert_eq!(row.income, income);
        assert_eq!(row.asset, "USDC");
        assert_eq!(row.timestamp, 1699999999000.0);
    }
}
#[test]
fn production_seam_income_history_reaches_both_fill_income_rows() {
    let f = seam_started();
    seam_income(&f);
    let options =
        serde_json::from_value(json!({"startTime":1699999900000u64,"endTime":1700000000000u64}))
            .unwrap();
    let records = f
        .host
        .block_on(f.exchange.fetch_income_history(Some(options)))
        .unwrap();
    assert_seam_income(&records);
    assert_eq!(
        f.routes
            .requests
            .borrow()
            .iter()
            .find(|(route, _)| route == "info:userFillsByTime")
            .unwrap()
            .1["startTime"],
        json!(1699999900000u64)
    );
}
#[test]
fn production_seam_income_range_reaches_the_same_actual_ledger() {
    let f = seam_started();
    seam_income(&f);
    let records = f
        .host
        .block_on(f.exchange.fetch_income_history_range(
            1699999900000.0,
            1700000000000.0,
            Some("REALIZED_PNL".into()),
        ))
        .unwrap();
    // Maintained TS selects fills for every non-funding type, retaining both rows.
    assert_seam_income(&records);
    let requests = f.routes.requests.borrow();
    let body = &requests
        .iter()
        .find(|(route, _)| route == "info:userFillsByTime")
        .unwrap()
        .1;
    assert_eq!(body["startTime"], json!(1699999900000u64));
    assert_eq!(body["endTime"], json!(1700000000000u64));
}

fn seam_socket(f: &Fixture, topic: &str) -> Rc<host::Wire> {
    use safe_cex_exchange::websocket::WsEvent;
    let wires = f.host.sockets.borrow().clone();
    for wire in &wires {
        wire.push(WsEvent::Open);
    }
    f.host.settle();
    // Complete actual open/reconnect snapshots before injecting the witness frame.
    f.host
        .block_on(f.exchange.fetch_and_set_private_snapshot(1))
        .unwrap();
    wires
        .into_iter()
        .find(|wire| {
            wire.sent.borrow().iter().any(|text| {
                serde_json::from_str::<Value>(text)
                    .is_ok_and(|v| v["subscription"]["type"] == topic)
            })
        })
        .unwrap_or_else(|| panic!("production socket never subscribed to {topic}"))
}
fn seam_push(f: &Fixture, wire: &host::Wire, frame: Value) {
    wire.push(safe_cex_exchange::websocket::WsEvent::Message(
        safe_cex_transport::socket::SocketFrame::Text(frame.to_string()),
    ));
    f.host.settle();
}
fn seam_observe_fills(f: &Fixture) -> Rc<RefCell<Vec<Value>>> {
    let values = Rc::new(RefCell::new(Vec::new()));
    let observed = values.clone();
    f.exchange.base().on(
        "fill",
        Rc::new(move |_, event| {
            let event = safe_cex_exchange::js_runtime::value::to_json(event).unwrap();
            observed.borrow_mut().push(event.clone());
            Ok(())
        }),
    );
    values
}

#[test]
fn production_seam_public_listener_candles_deliver_and_cleanup() {
    let f = seam_started();
    let wire = seam_socket(&f, "fastAssetCtxs");
    let values = Rc::new(RefCell::new(Vec::new()));
    let seen = values.clone();
    let options = serde_json::from_value(json!({"symbol":"BTC-PERP","interval":"1m"})).unwrap();
    let cleanup = f
        .exchange
        .listen_ohlcv(
            options,
            Rc::new(move |candle| seen.borrow_mut().push(candle)),
        )
        .unwrap();
    assert!(
        wire.sent
            .borrow()
            .iter()
            .any(|text| serde_json::from_str::<Value>(text).is_ok_and(
                |v| v["subscription"] == json!({"type":"candle","coin":"BTC","interval":"1m"})
            )),
        "actual candle subscription"
    );
    let frame = |time: u64| json!({"channel":"candle","data":[{"s":"BTC","i":"1m","t":time,"o":"60000","h":"61000","l":"59000","c":"60500","v":"12.5"}]});
    seam_push(&f, &wire, frame(1700000000000));
    assert_eq!(
        values.borrow().len(),
        1,
        "real public candle must reach the native callback"
    );
    let candles = values.borrow();
    assert_eq!(
        (candles[0].timestamp, candles[0].close, candles[0].volume),
        (1700000000.0, 60500.0, 12.5)
    );
    drop(candles);
    let sends = wire.sent.borrow().len();
    cleanup().unwrap();
    seam_push(&f, &wire, frame(1700000060000));
    assert_eq!(
        values.borrow().len(),
        1,
        "cleanup removes the actual parser"
    );
    assert_eq!(
        wire.sent.borrow().len(),
        sends,
        "source candle cleanup does not send unsubscribe"
    );
}

#[test]
fn production_seam_public_listener_books_deliver_and_cleanup() {
    let f = seam_started();
    let wire = seam_socket(&f, "fastAssetCtxs");
    let values = Rc::new(RefCell::new(Vec::new()));
    let seen = values.clone();
    let cleanup = f
        .exchange
        .listen_order_book(
            "BTC-PERP".into(),
            Rc::new(move |book| seen.borrow_mut().push(book)),
        )
        .unwrap();
    let frame = json!({"channel":"l2Book","data":{"coin":"BTC","levels":[[{"px":"60000","sz":"0.2"},{"px":"59900","sz":"0.3"}],[{"px":"60100","sz":"0.4"},{"px":"60200","sz":"0.5"}]]}});
    seam_push(&f, &wire, frame.clone());
    assert_eq!(
        values.borrow().len(),
        1,
        "real public book must reach the native callback"
    );
    let books = values.borrow();
    assert_eq!(
        (
            books[0].bids[0].price,
            books[0].bids[0].amount,
            books[0].asks[0].price,
            books[0].asks[0].amount
        ),
        (60000.0, 0.2, 60100.0, 0.4)
    );
    drop(books);
    cleanup().unwrap();
    assert!(wire.sent.borrow().iter().any(|text| {
        serde_json::from_str::<Value>(text).is_ok_and(|v| {
            v["method"] == "unsubscribe"
                && v["subscription"] == json!({"type":"l2Book","coin":"BTC"})
        })
    }));
    seam_push(&f, &wire, frame);
    assert_eq!(values.borrow().len(), 1);
}

#[test]
fn production_seam_private_orders_frame_updates_the_store_without_a_fill() {
    let f = seam_started();
    let wire = seam_socket(&f, "orderUpdates");
    let fills = seam_observe_fills(&f);
    let cloid = "0x22222222222222222222222222222222";
    seam_push(
        &f,
        &wire,
        json!({"channel":"orderUpdates","data":[{"order":{"coin":"BTC","oid":888,"side":"B","limitPx":"60010","sz":"0.4","origSz":"0.5","timestamp":1700000000100u64,"orderType":"Limit","reduceOnly":false,"cloid":cloid},"status":"open","statusTimestamp":1700000000100u64}]}),
    );
    let orders = f.exchange.base().store.orders();
    let order = orders
        .iter()
        .find(|order| order.id == "888")
        .expect("actual private order reaches Store");
    assert_eq!(order.symbol, "BTC-PERP");
    assert_eq!(order.side, safe_cex_types::OrderSide::Buy);
    assert_eq!(order.r#type, safe_cex_types::OrderType::Limit);
    assert_eq!(order.price, 60010.0);
    assert_eq!(order.amount, 0.5);
    assert!((order.filled - 0.1).abs() < 1e-12);
    assert_eq!(order.remaining, 0.4);
    assert_eq!(order.client_order_id.as_deref(), Some(cloid));
    assert!(
        fills.borrow().is_empty(),
        "order state alone does not invent fill events"
    );
}

#[test]
fn production_seam_private_core_positions_frame_updates_the_store_without_a_fill() {
    let f = seam_started();
    let wire = seam_socket(&f, "clearinghouseState");
    let fills = seam_observe_fills(&f);
    seam_push(
        &f,
        &wire,
        json!({"channel":"clearinghouseState","data":{"user":MASTER,"dex":"","clearinghouseState":seam_core_state()}}),
    );
    let positions = f.exchange.base().store.positions();
    let position = positions
        .iter()
        .find(|p| p.symbol == "BTC-PERP" && p.side == safe_cex_types::PositionSide::Long)
        .expect("actual core private position");
    assert_eq!(
        (
            position.contracts,
            position.entry_price,
            position.unrealized_pnl,
            position.leverage
        ),
        (0.25, 60000.0, 125.0, 5.0)
    );
    assert!(
        fills.borrow().is_empty(),
        "position snapshots do not invent fill events"
    );
}

#[test]
fn production_seam_private_core_balance_frame_updates_the_store_without_a_fill() {
    let f = seam_started();
    let wire = seam_socket(&f, "clearinghouseState");
    let fills = seam_observe_fills(&f);
    let mut state = seam_core_state();
    state["assetPositions"] = json!([]);
    for key in ["marginSummary", "crossMarginSummary"] {
        state[key]["accountValue"] = json!("1375");
        state[key]["totalMarginUsed"] = json!("25");
    }
    seam_push(
        &f,
        &wire,
        json!({"channel":"clearinghouseState","data":{"user":MASTER,"dex":"","clearinghouseState":state}}),
    );
    let balance = f.exchange.base().store.balance();
    assert_eq!(
        (balance.total, balance.used, balance.free, balance.upnl),
        (1375.0, 25.0, 1350.0, 0.0)
    );
    assert!(
        fills.borrow().is_empty(),
        "balance snapshots do not invent fill events"
    );
}

#[test]
fn production_reconcile_clears_undefined_tif_and_then_stops_rewriting() {
    let f = seam_started();
    let cloid = "0x11111111111111111111111111111111";
    let mut seed = serde_json::to_value(seam_order()).unwrap();
    seed["timeInForce"] =
        serde_json::to_value(safe_cex_types::OrderTimeInForce::GoodTillCancel).unwrap();
    f.exchange
        .base()
        .store
        .add_order(serde_json::from_value(seed).unwrap())
        .unwrap();
    seam_route(
        &f,
        "info:frontendOpenOrders",
        json!([{
            "coin":"BTC", "oid":777, "side":"A", "limitPx":"60000", "sz":"0.01",
            "origSz":"0.01", "timestamp":1700000000000u64, "orderType":"Limit",
            "reduceOnly":false, "isTrigger":false, "tif":"Ioc", "cloid":cloid
        }]),
    );
    let fetched = f.host.block_on(f.exchange.fetch_orders(None)).unwrap();
    f.host.settle();
    assert_eq!(fetched.len(), 1);
    assert!(
        fetched[0].time_in_force.is_undefined(),
        "source mapper owns an undefined TIF"
    );
    {
        let orders = f.exchange.base().store.orders();
        let row = orders.iter().find(|row| row.id == "777").unwrap();
        assert_eq!(
            row.client_order_id.as_deref(),
            Some(cloid),
            "#465 restores the client order ID"
        );
        assert!(
            row.time_in_force.is_undefined(),
            "realm updateOrder must retain the mapper's own undefined TIF"
        );
    }
    let revision = f.exchange.base().store.update_ids().orders;
    f.host.block_on(f.exchange.fetch_orders(None)).unwrap();
    f.host.settle();
    assert_eq!(
        f.exchange.base().store.update_ids().orders,
        revision,
        "an identical REST refresh must not rewrite the reconciled order"
    );
}

#[test]
fn native_nonfinite_order_size_reaches_the_signed_order_post() {
    for (amount, expected) in [
        (f64::NAN, "NaN"),
        (f64::INFINITY, "Infinity"),
        (f64::NEG_INFINITY, "-Infinity"),
    ] {
        let f = started();
        let mut opts: safe_cex_types::PlaceOrderOpts = serde_json::from_value(json!({
            "symbol":"BTC-PERP","type":"limit","side":"buy","amount":0.01,"price":60000
        }))
        .unwrap();
        opts.amount = amount;
        let result = f
            .host
            .block_on(f.exchange.base().place_order(&*f.exchange, opts));
        assert!(
            result.is_ok(),
            "{expected} wire must reach the actual signed order request: {result:?}"
        );
        let body = last_body(&f, "exchange:order");
        assert_eq!(body["action"]["orders"][0]["s"], expected);
        assert_eq!(body["action"]["orders"][0]["p"], "60000");
        assert_eq!(body["action"]["orders"][0]["b"], true);
        assert_eq!(body["action"]["orders"][0]["a"], 0);
        assert!(
            body["signature"]["r"]
                .as_str()
                .is_some_and(|s| s.len() == 66)
        );
        assert_eq!(f.routes.count("exchange:order"), 1);
    }
}

#[path = "../../parity/test-support/ticker_coercion.rs"]
mod ticker_coercion;

#[test]
fn production_ticker_coercion_matches_constructed_typescript() {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_store::{Store, StoreUpdate};
    use safe_cex_transport::socket::SocketFrame;
    let source = ticker_coercion::source("hyperliquid");
    assert_eq!(source["cases"].as_array().unwrap().len(), 24);
    let f = started();
    let wire = seam_socket(&f, "allMids");
    for case in source["cases"].as_array().unwrap() {
        Store::update(
            f.exchange.base().store.as_ref(),
            StoreUpdate {
                tickers: Some(ticker_coercion::seed("hyperliquid")),
                positions: Some(vec![]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        for frame in case["frames"].as_array().unwrap() {
            wire.push(WsEvent::Message(SocketFrame::Text(
                frame.as_str().unwrap().into(),
            )));
            f.host.settle();
        }
        ticker_coercion::assert_native(&f.exchange.base().store.tickers(), case);
    }
    assert!(
        f.host.reports.borrow().is_empty(),
        "{:?}",
        f.host.reports.borrow()
    );
    f.exchange.dispose().unwrap();
}
