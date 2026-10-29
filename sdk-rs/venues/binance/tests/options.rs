//! Production-binding controls pinned to the retained TS Options fixtures.
mod support;
use safe_cex_engine::Venue;
use safe_cex_engine::model::{OrderType, PlaceOrderOpts, PositionSide, ProductKind};
use safe_cex_engine::{Command, Event, Outcome};
use safe_cex_venue_binance::Variant;
use support::{Harness, Options, keyed};

const SYMBOL: &str = "BTC-261225-85000-C";
const INFO: &str = r#"{"optionContracts":[{"underlying":"BTCUSDT","baseAsset":"BTC","settleAsset":"USDT"}],"optionSymbols":[{"symbol":"BTC-261225-85000-C","underlying":"BTCUSDT","quoteAsset":"USDT","status":"TRADING","side":"CALL","strikePrice":"85000.000","expiryDate":1798185600000,"filters":[{"filterType":"PRICE_FILTER","tickSize":"5.000","minPrice":"775"},{"filterType":"LOT_SIZE","stepSize":"0.01","minQty":"0.01","maxQty":"200"}]}]}"#;
const DAILY: &str = r#"[{"symbol":"BTC-261225-85000-C","priceChangePercent":"0.185","lastPrice":"4520","volume":"2.22","amount":"9831.45","bidPrice":"3885","askPrice":"3930","exercisePrice":"81565.34009019"}]"#;
const MARK: &str = r#"[{"symbol":"BTC-261225-85000-C","markPrice":"3845.503"}]"#;
const ACCOUNT: &str = r#"{"asset":[{"asset":"USDT","equity":"10094.44662","available":"8725.92524","initialMargin":"1084.52138","unrealizedPNL":"-5.00138"}]}"#;
const POSITIONS: &str = r#"[{"symbol":"BTC-261225-85000-C","side":"SHORT","quantity":"-0.1","entryPrice":"1000","markValue":"105.00138","unrealizedPNL":"-5.00138"}]"#;
const ORDERS: &str = r#"[{"orderId":4611875134427365000,"symbol":"BTC-261225-85000-C","side":"BUY","status":"PARTIALLY_FILLED","quantity":"0.1","executedQty":"0.02","price":"1000","clientOrderId":"existing","timeInForce":"GTC"}]"#;
fn started(private: bool) -> Harness {
    let options = if private {
        keyed(Variant::Options, "options-key", "options-secret")
    } else {
        Options {
            variant: Variant::Options,
            public_data_only: true,
            ..Options::default()
        }
    };
    let mut h = Harness::started(options);
    assert!(
        h.requests[0]
            .url
            .starts_with("https://eapi.binance.com/eapi/v1/exchangeInfo")
    );
    h.respond("/eapi/v1/exchangeInfo", 200, INFO);
    h.respond("/eapi/v1/ticker", 200, DAILY);
    h.respond("/eapi/v1/mark", 200, MARK);
    h
}
fn limit() -> PlaceOrderOpts {
    serde_json::from_str(r#"{"symbol":"BTC-261225-85000-C","type":"limit","side":"buy","amount":0.13,"price":1004,"timeInForce":"PostOnly","clientOrderId":"options-caller"}"#).unwrap()
}
#[test]
fn catalogue_and_tickers_keep_option_terms_and_native_contract_units() {
    let h = started(false);
    let store = h.engine.store();
    let market = &store.markets()[0];
    assert_eq!(market.symbol, SYMBOL);
    assert_eq!(market.precision.amount, 0.01);
    assert_eq!(market.precision.price, 5.);
    assert_eq!(market.contract_size, Some(1.));
    assert_eq!(market.is_expiring, Some(true));
    let product = &store.derivative_products().unwrap()[0];
    assert_eq!(product.kind, ProductKind::Call);
    assert_eq!(
        (product.underlying.as_str(), product.expiry, product.strike),
        ("BTC", Some(1798185600000.), Some(85000.))
    );
    let ticker = &store.tickers()[0];
    assert_eq!(
        (
            ticker.last,
            ticker.mark,
            ticker.percentage,
            ticker.quote_volume
        ),
        (4520., 3845.503, 18.5, 9831.45)
    );
    assert_eq!(ticker.funding_rate, 0.);
    assert!(h.requests.is_empty());
    assert!(
        h.sockets
            .iter()
            .all(|(_, u)| u.starts_with("wss://nbstream.binance.com/eoptions/ws"))
    );
}
#[test]
fn private_start_uses_eapi_only_and_maps_signed_positions_and_currency_balances() {
    let mut h = started(true);
    assert!(
        h.requests
            .iter()
            .all(|r| r.url.starts_with("https://eapi.binance.com/eapi/"))
    );
    assert!(
        !h.paths()
            .iter()
            .any(|p| p.contains("leverage") || p.contains("dual") || p.contains("Algo"))
    );
    h.respond("/eapi/v1/marginAccount", 200, ACCOUNT);
    h.respond("/eapi/v1/position", 200, POSITIONS);
    h.respond("/eapi/v1/openOrders", 200, ORDERS);
    let store = h.engine.store();
    assert_eq!(store.balance().total, 10094.44662);
    assert_eq!(store.balance().free, 8725.92524);
    assert_eq!(store.currency_balances().unwrap()[0].currency, "USDT");
    let pos = &store.positions()[0];
    assert_eq!(pos.side, PositionSide::Short);
    assert_eq!(pos.contracts, 0.1);
    assert_eq!(pos.notional, 105.00138);
    assert_eq!(pos.leverage, 1.);
    let order = &store.orders()[0];
    assert_eq!(order.id, "4611875134427365000");
    assert_eq!(order.amount, 0.1);
    assert_eq!(order.filled, 0.02);
    assert_eq!(order.kind, OrderType::Limit);
}
#[test]
fn limit_post_only_uses_eapi_fields_and_dispatch_time_hmac() {
    let mut h = started(true);
    h.requests.clear();
    h.event(Event::Command(Command::PlaceOrders {
        command: 77,
        orders: vec![limit()],
    }));
    let request = h.take("/eapi/v1/order?");
    assert!(request.url.contains("symbol=BTC-261225-85000-C&side=BUY&type=LIMIT&quantity=0.13&price=1005&timeInForce=GTC&postOnly=true&clientOrderId=options-caller"), "{}", request.url);
    assert!(request.url.contains("recvWindow=5000"));
    assert!(request.url.contains("signature="));
    assert!(!request.url.contains("positionSide"));
    assert!(!request.url.contains("workingType"));
    assert_eq!(
        request
            .headers
            .iter()
            .find(|(k, _)| *k == "X-MBX-APIKEY")
            .unwrap()
            .1,
        "options-key"
    );
    h.event(Event::HttpResponse {
        request: request.id,
        response: safe_cex_engine::Response {
            status: 200,
            body: r#"{"orderId":4611875134427365001}"#,
            headers: "",
        },
    });
    assert_eq!(
        h.done.last(),
        Some(&(77, Ok("[\"4611875134427365001\"]".into())))
    );
}
#[test]
fn invalid_member_refuses_the_whole_batch_before_any_write() {
    for kind in [
        OrderType::Market,
        OrderType::StopLoss,
        OrderType::TrailingStopLoss,
    ] {
        let mut h = started(true);
        h.requests.clear();
        let mut invalid = limit();
        invalid.kind = kind;
        invalid.client_order_id = None;
        h.event(Event::Command(Command::PlaceOrders {
            command: 78,
            orders: vec![limit(), invalid],
        }));
        assert!(h.requests.is_empty());
        assert_eq!(h.failures.len(), 1);
        assert_eq!(h.failures[0].1.outcome, Some(Outcome::NotSent));
    }
}
#[test]
fn maximum_is_refused_without_splitting_or_reusing_client_identity() {
    let mut h = started(true);
    h.requests.clear();
    let mut order = limit();
    order.amount = 200.01;
    h.event(Event::Command(Command::PlaceOrders {
        command: 79,
        orders: vec![order],
    }));
    assert!(h.requests.is_empty());
    assert_eq!(h.failures[0].1.outcome, Some(Outcome::NotSent));
}
#[test]
fn malformed_orders_never_publish_or_clear_a_clean_snapshot() {
    let mut h = started(true);
    h.respond("/eapi/v1/openOrders", 200, ORDERS);
    let before = h.engine.store().orders().to_vec();
    let timer = h.timers.iter().find(|(_, at)| *at == 6000.).unwrap().0;
    h.now = 6000.;
    h.event(Event::TimerFired { timer });
    h.respond(
        "/eapi/v1/openOrders",
        200,
        &ORDERS.replace("PARTIALLY_FILLED", "UNRECOGNIZED"),
    );
    assert_eq!(h.engine.store().orders(), before.as_slice());
}
#[test]
fn option_book_partial_snapshot_does_not_issue_futures_depth_reads() {
    let mut h = started(false);
    let socket = h.sockets[0].0;
    h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: SYMBOL.into(),
    }));
    h.event(Event::WsOpened { socket });
    assert!(
        h.sent
            .iter()
            .any(|(_, s)| s.contains("BTC-261225-85000-C@depth20@100ms"))
    );
    h.event(Event::WsMessage { socket,text:r#"{"e":"depth","s":"BTC-261225-85000-C","U":1,"u":2,"b":[["1000","0.5"]],"a":[["1005","0.2"]]}"# });
    assert!(h.requests.is_empty());
    assert!(h.engine.venue().book(SYMBOL).is_some());
}
