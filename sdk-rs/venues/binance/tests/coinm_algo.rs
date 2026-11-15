//! COIN-M stop types go through /dapi/v1/algoOrder since Binance's CM-UM integration
//! (/products/derivatives-trading-coin-futures/Important-CM-UM-Integration-Notice, C.5-C.6):
//! /dapi/v1/order and batchOrders answer them -4120. Stops placed before that stay on the
//! order endpoint and are cancelled there.
use support::Options;
mod support;

use safe_cex_engine::model::{OrderSide, OrderStatus, OrderType, PlaceOrderOpts};
use safe_cex_engine::{Command, Event, Method};
use safe_cex_venue_binance::Variant;
use support::Harness;

const LIMIT_ROW: &str = r#"{"symbol":"BTCUSD_PERP","orderId":12,"clientOrderId":"maker","price":"9000","origQty":"7","executedQty":"0","type":"LIMIT","side":"BUY","timeInForce":"GTC"}"#;
const LEGACY_STOP_ROW: &str = r#"{"symbol":"BTCUSD_PERP","orderId":14,"clientOrderId":"legacy","price":"0","stopPrice":"8000","origQty":"7","executedQty":"0","type":"STOP_MARKET","side":"SELL","reduceOnly":true,"workingType":"MARK_PRICE"}"#;
const ALGO_ROW: &str = r#"{"algoId":5550,"clientAlgoId":"algo","algoType":"CONDITIONAL","orderType":"STOP_MARKET","symbol":"BTCUSD_PERP","side":"SELL","positionSide":"BOTH","quantity":"7","algoStatus":"NEW","triggerPrice":"8500","price":"0","workingType":"MARK_PRICE","reduceOnly":true,"closePosition":false,"createTime":1}"#;

fn ready() -> Harness {
    ready_with_algo(&format!("[{ALGO_ROW}]"))
}

fn ready_with_algo(algo: &str) -> Harness {
    let mut h = Harness::started(Options {
        variant: Variant::CoinM,
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    h.respond("/dapi/v1/exchangeInfo", 200, r#"{"symbols":[{"symbol":"BTCUSD_PERP","status":"TRADING","baseAsset":"BTC","quoteAsset":"USD","marginAsset":"BTC","contractType":"PERPETUAL","contractSize":100,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.1"},{"filterType":"LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"100"},{"filterType":"MARKET_LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"100"}]}]}"#);
    h.respond("/dapi/v2/leverageBracket", 200, "[]");
    h.respond("/dapi/v1/ticker/24hr", 200, r#"[{"symbol":"BTCUSD_PERP","lastPrice":"10000","priceChangePercent":"0","volume":"1","baseVolume":"1"}]"#);
    h.respond(
        "/dapi/v1/ticker/bookTicker",
        200,
        r#"[{"symbol":"BTCUSD_PERP","bidPrice":"9999","askPrice":"10001"}]"#,
    );
    h.respond("/dapi/v1/premiumIndex", 200, r#"[{"symbol":"BTCUSD_PERP","markPrice":"10000","indexPrice":"10000","lastFundingRate":"0","nextFundingTime":100000}]"#);
    h.respond("/dapi/v1/listenKey", 200, r#"{"listenKey":"coin-key"}"#);
    h.respond(
        "/dapi/v1/positionSide/dual",
        200,
        r#"{"dualSidePosition":false}"#,
    );
    h.respond("/dapi/v1/positionRisk", 200, r#"[{"symbol":"BTCUSD_PERP","positionSide":"BOTH","positionAmt":"7","entryPrice":"9000","markPrice":"10000","unRealizedProfit":"0","liquidationPrice":"7000","leverage":"20","marginType":"cross","isolatedMargin":"0"}]"#);
    h.respond("/dapi/v1/account", 200, r#"{"assets":[]}"#);
    h.respond(
        "/dapi/v1/openOrders",
        200,
        &format!("[{LIMIT_ROW},{LEGACY_STOP_ROW}]"),
    );
    // Conditional so the limit pin also runs against master, which has no algo read.
    if h.has_request("/dapi/v1/openAlgoOrders") {
        h.respond("/dapi/v1/openAlgoOrders", 200, algo);
    }
    let socket = h.socket("/ws/coin-key").unwrap();
    h.event(Event::WsOpened { socket });
    h.answer_open_position_read();
    h.requests.clear();
    h
}

fn order(kind: OrderType, side: OrderSide, amount: f64, price: f64) -> PlaceOrderOpts {
    let mut order: PlaceOrderOpts = serde_json::from_value(serde_json::json!({
        "symbol": "BTCUSD_PERP",
        "type": "limit",
        "side": "buy",
        "amount": amount,
        "price": price,
    }))
    .unwrap();
    order.kind = kind;
    order.side = side;
    order
}

fn place(h: &mut Harness, command: u32, order: PlaceOrderOpts) {
    h.event(Event::Command(Command::PlaceOrders {
        command,
        orders: vec![order],
    }));
}

fn param(url: &str, name: &str) -> Option<String> {
    url.split_once('?')?
        .1
        .split('&')
        .find_map(|pair| pair.strip_prefix(&format!("{name}=")))
        .map(str::to_owned)
}

fn requested(h: &Harness, method: Method, path: &str) -> usize {
    h.requests
        .iter()
        .filter(|r| r.method == method && r.url.contains(&format!("{path}?")))
        .count()
}

#[test]
fn the_open_algo_read_lists_coin_m_algo_orders() {
    let h = ready();
    let row = h
        .engine
        .store()
        .order("5550")
        .expect("the algo row is listed");
    assert_eq!(row.kind, OrderType::StopLoss);
    assert_eq!((row.price, row.amount), (8500.0, 7.0));
    assert_eq!(
        row.extra.as_ref().and_then(|e| e.remaining_native),
        Some(7.0)
    );
}

#[test]
fn an_algo_order_cancels_by_its_algo_id() {
    let mut h = ready();
    h.event(Event::Command(Command::CancelOrders {
        command: 1,
        ids: vec!["5550".into()],
    }));
    assert_eq!(requested(&h, Method::Delete, "/dapi/v1/order"), 0);
    let cancel = h.take("/dapi/v1/algoOrder");
    assert_eq!(cancel.method, Method::Delete);
    assert_eq!(param(&cancel.url, "algoId").as_deref(), Some("5550"));
    h.reply(
        cancel.id,
        200,
        "",
        r#"{"algoId":5550,"clientAlgoId":"algo","code":"200","msg":"success"}"#,
    );
    assert!(matches!(h.result(1), Some(Ok(_))), "{:?}", h.result(1));
    assert_eq!(
        h.engine.store().order("5550").unwrap().status,
        OrderStatus::Canceled
    );
}

#[test]
fn a_stop_placed_before_the_migration_cancels_on_the_order_endpoint() {
    let mut h = ready();
    h.event(Event::Command(Command::CancelOrders {
        command: 2,
        ids: vec!["14".into()],
    }));
    assert!(!h.has_request("/dapi/v1/algoOrder"), "{:?}", h.paths());
    let cancel = h.take("/dapi/v1/order");
    assert_eq!(cancel.method, Method::Delete);
    assert_eq!(param(&cancel.url, "orderId").as_deref(), Some("14"));
}

#[test]
fn cancel_all_covers_regular_legacy_and_algo_orders() {
    let mut h = ready();
    h.event(Event::Command(Command::CancelSymbolOrders {
        command: 3,
        symbol: "BTCUSD_PERP".into(),
    }));
    let all = h.take("/dapi/v1/allOpenOrders");
    assert_eq!(all.method, Method::Delete);
    let algo = h.take("/dapi/v1/algoOrder");
    assert_eq!(algo.method, Method::Delete);
    assert_eq!(param(&algo.url, "algoId").as_deref(), Some("5550"));
    assert!(h.requests.is_empty(), "{:?}", h.paths());
    h.reply(
        all.id,
        200,
        "",
        r#"{"code":200,"msg":"The operation of cancel all open order is done."}"#,
    );
    h.reply(
        algo.id,
        200,
        "",
        r#"{"algoId":5550,"clientAlgoId":"algo","code":"200","msg":"success"}"#,
    );
    assert!(matches!(h.result(3), Some(Ok(_))), "{:?}", h.result(3));
    for id in ["12", "14", "5550"] {
        assert_eq!(
            h.engine.store().order(id).unwrap().status,
            OrderStatus::Canceled,
            "{id}"
        );
    }
}

#[test]
fn an_algo_order_amends_through_the_algo_endpoint() {
    let mut h = ready();
    h.event(Event::Command(Command::AmendOrder {
        command: 94,
        id: "5550".into(),
        price: Some(8600.0),
        amount: None,
    }));
    let cancel = h.take("/dapi/v1/algoOrder");
    assert_eq!(cancel.method, Method::Delete);
    h.reply(
        cancel.id,
        200,
        "",
        r#"{"algoId":5550,"clientAlgoId":"algo","code":"200","msg":"success"}"#,
    );
    let replacement = h.take("/dapi/v1/algoOrder");
    assert_eq!(replacement.method, Method::Post);
    assert_eq!(
        param(&replacement.url, "type").as_deref(),
        Some("STOP_MARKET")
    );
    assert_eq!(
        param(&replacement.url, "triggerPrice").as_deref(),
        Some("8600")
    );
    assert_eq!(param(&replacement.url, "quantity").as_deref(), Some("7"));
    assert_eq!(
        param(&replacement.url, "reduceOnly").as_deref(),
        Some("true")
    );
    h.reply(
        replacement.id,
        200,
        "",
        r#"{"algoId":5551,"clientAlgoId":"x","algoStatus":"NEW"}"#,
    );
    assert_eq!(h.result(94), Some(&Ok(r#"["5551"]"#.to_owned())));
}

#[test]
fn a_trailing_stop_is_an_algo_order_with_its_callback_and_activation() {
    let mut h = ready();
    let mut trailing = order(OrderType::TrailingStopLoss, OrderSide::Sell, 7.0, 9800.0);
    trailing.reduce_only = Some(true);
    place(&mut h, 6, trailing);
    assert_eq!(requested(&h, Method::Post, "/dapi/v1/order"), 0);
    let sent = h.take("/dapi/v1/algoOrder");
    assert_eq!(
        param(&sent.url, "type").as_deref(),
        Some("TRAILING_STOP_MARKET")
    );
    assert_eq!(param(&sent.url, "algoType").as_deref(), Some("CONDITIONAL"));
    assert_eq!(param(&sent.url, "callbackRate").as_deref(), Some("2"));
    assert_eq!(param(&sent.url, "activatePrice").as_deref(), Some("9800"));
    assert_eq!(param(&sent.url, "activationPrice"), None);
}

#[test]
fn an_entrys_brackets_follow_it_as_algo_orders() {
    let mut h = ready();
    let mut entry = order(OrderType::Limit, OrderSide::Buy, 3.0, 9000.0);
    entry.stop_loss = Some(8000.0);
    entry.take_profit = Some(11000.0);
    place(&mut h, 7, entry);
    let limit = h.take("/dapi/v1/order");
    assert_eq!(limit.method, Method::Post);
    assert_eq!(param(&limit.url, "type").as_deref(), Some("LIMIT"));
    assert!(!h.has_request("/dapi/v1/algoOrder"), "{:?}", h.paths());
    h.reply(
        limit.id,
        200,
        "",
        r#"{"symbol":"BTCUSD_PERP","orderId":20,"status":"NEW"}"#,
    );
    for (kind, trigger) in [("STOP_MARKET", "8000"), ("TAKE_PROFIT_MARKET", "11000")] {
        let bracket = h.take("/dapi/v1/algoOrder");
        assert_eq!(param(&bracket.url, "type").as_deref(), Some(kind));
        assert_eq!(
            param(&bracket.url, "triggerPrice").as_deref(),
            Some(trigger)
        );
        assert_eq!(param(&bracket.url, "quantity").as_deref(), Some("3"));
        assert_eq!(param(&bracket.url, "reduceOnly").as_deref(), Some("true"));
        h.reply(
            bracket.id,
            200,
            "",
            r#"{"algoId":30,"clientAlgoId":"x","algoStatus":"NEW"}"#,
        );
    }
}

#[test]
fn a_streamed_algo_update_is_an_algo_order() {
    let mut h = ready();
    h.message("/ws/coin-key", r#"{"e":"ALGO_UPDATE","o":{"aid":6000,"caid":"x","at":"CONDITIONAL","o":"TAKE_PROFIT_MARKET","s":"BTCUSD_PERP","S":"SELL","ps":"BOTH","f":"GTC","q":"7","X":"NEW","tp":"12000","p":"0","wt":"MARK_PRICE","R":true,"cp":false}}"#);
    h.requests.clear();
    h.event(Event::Command(Command::CancelOrders {
        command: 8,
        ids: vec!["6000".into()],
    }));
    let cancel = h.take("/dapi/v1/algoOrder");
    assert_eq!(param(&cancel.url, "algoId").as_deref(), Some("6000"));
}

#[test]
fn a_4120_refusal_says_the_stop_type_moved_to_the_algo_endpoint() {
    let mut h = ready();
    place(
        &mut h,
        9,
        order(OrderType::StopLoss, OrderSide::Sell, 7.0, 8000.0),
    );
    let sent = h.take("/dapi/v1/algoOrder");
    h.reply(
        sent.id,
        400,
        "",
        r#"{"code":-4120,"msg":"Order type not supported for this endpoint. Please use the Algo Order API endpoints instead."}"#,
    );
    let failure = &h.failures.iter().find(|(c, _)| *c == 9).unwrap().1;
    assert!(
        failure
            .message
            .contains("no longer accepts this stop type on its regular order endpoint"),
        "{}",
        failure.message
    );
    assert_eq!(
        failure.kind,
        Some(safe_cex_engine::error::ErrorKind::Unsupported)
    );
    assert_eq!(failure.outcome, Some(safe_cex_engine::Outcome::Rejected));
}

#[test]
fn a_4120_batch_element_gets_the_same_refusal() {
    let mut h = ready();
    // Above the 100-contract maximum the entry splits into a batch.
    place(
        &mut h,
        10,
        order(OrderType::Limit, OrderSide::Buy, 150.0, 9000.0),
    );
    let batch = h.take("/dapi/v1/batchOrders");
    // TS binanceinverse.exchange.ts:919-924 emits three 50-contract rows.
    h.reply(
        batch.id,
        200,
        "",
        r#"[{"code":-4120,"msg":"Order type not supported for this endpoint. Please use the Algo Order API endpoints instead."},{"code":-4120,"msg":"Order type not supported for this endpoint. Please use the Algo Order API endpoints instead."},{"code":-4120,"msg":"Order type not supported for this endpoint. Please use the Algo Order API endpoints instead."}]"#,
    );
    let failure = &h.failures.iter().find(|(c, _)| *c == 10).unwrap().1;
    assert!(
        failure
            .message
            .contains("no longer accepts this stop type on its regular order endpoint"),
        "{}",
        failure.message
    );
}

/// Master's bytes for an ordinary COIN-M limit placement and amend.
#[test]
fn an_ordinary_limit_is_unchanged() {
    let mut h = ready();
    place(
        &mut h,
        11,
        order(OrderType::Limit, OrderSide::Buy, 3.0, 9000.0),
    );
    let limit = h.take("/dapi/v1/order");
    assert_eq!(
        format!("{} {}", limit.method.as_str(), limit.url),
        "POST https://dapi.binance.com/dapi/v1/order?symbol=BTCUSD_PERP&positionSide=BOTH&side=BUY&type=LIMIT&workingType=MARK_PRICE&price=9000&timeInForce=GTC&quantity=3&newClientOrderId=x-s3vFFRQppqV3rlMxg4abZkEOJ6Sr&timestamp=1000&recvWindow=20000&signature=1ad424491948e0844c96f461491127b1bfdbde398fbad0e0bf8b1d783609b82e"
    );
    h.event(Event::Command(Command::AmendOrder {
        command: 12,
        id: "12".into(),
        price: Some(9100.0),
        amount: None,
    }));
    let cancel = h.take("/dapi/v1/order");
    assert_eq!(
        format!("{} {}", cancel.method.as_str(), cancel.url),
        "DELETE https://dapi.binance.com/dapi/v1/order?symbol=BTCUSD_PERP&orderId=12&timestamp=1000&recvWindow=20000&signature=42a8e2dedf1da41966e6617094d7718670a699b029a319cf6fbce6e71282843b"
    );
    h.reply(
        cancel.id,
        200,
        "",
        r#"{"symbol":"BTCUSD_PERP","orderId":12,"clientOrderId":"cancel-request","origClientOrderId":"maker","origQty":"7","executedQty":"0","status":"CANCELED"}"#,
    );
    let replacement = h.take("/dapi/v1/order");
    assert_eq!(
        format!("{} {}", replacement.method.as_str(), replacement.url),
        "POST https://dapi.binance.com/dapi/v1/order?symbol=BTCUSD_PERP&positionSide=BOTH&side=BUY&type=LIMIT&workingType=MARK_PRICE&price=9100&timeInForce=GTC&quantity=7&newClientOrderId=x-s3vFFRQpj7sHRjFY3nmpX4uRVHBu&timestamp=1000&recvWindow=20000&signature=36ee5df57d02fc0698c861e3049ecdc200a8e4976380ec12891debeab7e07000"
    );
}

/// A frame-fed COIN-M engine (no sockets, no account reads) fed `source`'s snapshot:
/// the algo routing rides each order row, so its writes match the source's.
fn fed_from(source: &mut Harness) -> Harness {
    let mut engine = support::engine(Options {
        variant: Variant::CoinM,
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    engine.set_frame_fed().unwrap();
    let mut fed = Harness::from_engine(engine);
    fed.respond("/dapi/v1/exchangeInfo", 200, r#"{"symbols":[{"symbol":"BTCUSD_PERP","status":"TRADING","baseAsset":"BTC","quoteAsset":"USD","marginAsset":"BTC","contractType":"PERPETUAL","contractSize":100,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.1"},{"filterType":"LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"100"},{"filterType":"MARKET_LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"100"}]}]}"#);
    assert!(fed.requests.is_empty(), "{:?}", fed.paths());
    fed.now = source.now;
    let frame = source.engine.snapshot_frame(source.now).to_vec();
    fed.engine.apply_frame(fed.now, &frame, true).unwrap();
    fed.collect();
    assert!(
        fed.requests.is_empty(),
        "ingest reads nothing: {:?}",
        fed.paths()
    );
    for id in ["5550", "14"] {
        assert!(fed.engine.store().order(id).is_some(), "{id} ingested");
    }
    fed
}

#[test]
fn a_frame_fed_engine_cancels_an_algo_stop_by_its_algo_id_and_a_legacy_one_on_the_order_endpoint() {
    let mut source = ready();
    let mut fed = fed_from(&mut source);
    fed.event(Event::Command(Command::CancelOrders {
        command: 40,
        ids: vec!["5550".into()],
    }));
    let cancel = fed.take("/dapi/v1/algoOrder");
    assert_eq!(cancel.method, Method::Delete);
    assert_eq!(param(&cancel.url, "algoId").as_deref(), Some("5550"));
    assert!(fed.requests.is_empty(), "{:?}", fed.paths());
    fed.event(Event::Command(Command::CancelOrders {
        command: 41,
        ids: vec!["14".into()],
    }));
    let legacy = fed.take("/dapi/v1/order");
    assert_eq!(legacy.method, Method::Delete);
    assert_eq!(param(&legacy.url, "orderId").as_deref(), Some("14"));
}

#[test]
fn a_frame_fed_engine_amends_an_algo_stop_through_the_algo_endpoint() {
    let mut source = ready();
    let mut fed = fed_from(&mut source);
    fed.event(Event::Command(Command::AmendOrder {
        command: 42,
        id: "5550".into(),
        price: Some(8600.0),
        amount: None,
    }));
    let cancel = fed.take("/dapi/v1/algoOrder");
    assert_eq!(cancel.method, Method::Delete);
    assert!(!fed.has_request("/dapi/v1/order?"), "{:?}", fed.paths());
    fed.reply(
        cancel.id,
        200,
        "",
        r#"{"algoId":5550,"clientAlgoId":"algo","code":"200","msg":"success"}"#,
    );
    let replacement = fed.take("/dapi/v1/algoOrder");
    assert_eq!(replacement.method, Method::Post);
    assert_eq!(
        param(&replacement.url, "triggerPrice").as_deref(),
        Some("8600")
    );
}

#[test]
fn a_frame_fed_cancel_all_cancels_its_algo_stops_by_algo_id() {
    let mut source = ready();
    let mut fed = fed_from(&mut source);
    fed.event(Event::Command(Command::CancelSymbolOrders {
        command: 43,
        symbol: "BTCUSD_PERP".into(),
    }));
    let all = fed.take("/dapi/v1/allOpenOrders");
    let algo = fed.take("/dapi/v1/algoOrder");
    assert_eq!(algo.method, Method::Delete);
    assert_eq!(param(&algo.url, "algoId").as_deref(), Some("5550"));
    fed.reply(
        all.id,
        200,
        "",
        r#"{"code":200,"msg":"The operation of cancel all open order is done."}"#,
    );
    // Until its own cancel answers, the algo stop is not counted cancelled.
    assert_ne!(
        fed.engine.store().order("5550").unwrap().status,
        OrderStatus::Canceled
    );
    fed.reply(
        algo.id,
        200,
        "",
        r#"{"algoId":5550,"clientAlgoId":"algo","code":"200","msg":"success"}"#,
    );
    assert_eq!(
        fed.engine.store().order("5550").unwrap().status,
        OrderStatus::Canceled
    );
}

/// A COIN-M algo trailing stop's refused replacement is put back on the algo endpoint
/// with its native callbackRate and activatePrice (the algo name, never activationPrice).
#[test]
fn an_algo_trailing_stops_put_back_restores_its_native_rate_on_the_algo_endpoint() {
    let trailing = ALGO_ROW
        .replace(r#""algoId":5550"#, r#""algoId":5560"#)
        .replace(
            r#""orderType":"STOP_MARKET""#,
            r#""orderType":"TRAILING_STOP_MARKET""#,
        )
        .replace(
            r#""triggerPrice":"8500""#,
            r#""triggerPrice":"9800","callbackRate":"1.5","activatePrice":"10300""#,
        );
    let mut h = ready_with_algo(&format!("[{ALGO_ROW},{trailing}]"));
    h.event(Event::Command(Command::AmendOrder {
        command: 50,
        id: "5560".into(),
        price: Some(9700.0),
        amount: None,
    }));
    let cancel = h.take("/dapi/v1/algoOrder");
    assert_eq!(cancel.method, Method::Delete);
    assert_eq!(param(&cancel.url, "algoId").as_deref(), Some("5560"));
    h.reply(
        cancel.id,
        200,
        "",
        r#"{"algoId":5560,"clientAlgoId":"algo","code":"200","msg":"success"}"#,
    );
    let replacement = h.take("/dapi/v1/algoOrder");
    assert_eq!(replacement.method, Method::Post);
    assert_eq!(
        param(&replacement.url, "callbackRate").as_deref(),
        Some("3")
    );
    h.reply(
        replacement.id,
        400,
        "",
        r#"{"code":-2021,"msg":"Order would immediately trigger."}"#,
    );
    let put_back = h.take("/dapi/v1/algoOrder");
    assert_eq!(put_back.method, Method::Post);
    assert!(!h.has_request("/dapi/v1/order?"), "{:?}", h.paths());
    assert_eq!(
        param(&put_back.url, "type").as_deref(),
        Some("TRAILING_STOP_MARKET")
    );
    assert_eq!(param(&put_back.url, "callbackRate").as_deref(), Some("1.5"));
    assert_eq!(
        param(&put_back.url, "activatePrice").as_deref(),
        Some("10300")
    );
    assert_eq!(param(&put_back.url, "activationPrice"), None);
}
