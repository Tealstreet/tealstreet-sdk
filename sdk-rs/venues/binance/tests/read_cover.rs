//! Read covers on a real Binance USD-M source (docs/plans/RUST_FRAME_FED_READ_COVER.md
//! step 1d): the open-orders read commits its rows with the cover it proves, and
//! native order state changes only for the symbols it may replace.
mod support;
use safe_cex_engine::Event;
use safe_cex_engine::wire::section;
use support::Harness;
use support::Options;

const POSITION_RISK: &str = r#"[{"symbol":"BTCUSDT","positionSide":"BOTH","positionAmt":"0.010","entryPrice":"95000.0","breakEvenPrice":"95010.0","markPrice":"96000.0","unRealizedProfit":"10.00","liquidationPrice":"80000.5","isolatedMargin":"0","notional":"960.0","marginAsset":"USDT","isolatedWallet":"0","initialMargin":"48","maintMargin":"3.8","positionInitialMargin":"48","openOrderInitialMargin":"0","adl":1,"bidNotional":"0","askNotional":"0","updateTime":1720736417660}]"#;
const SYMBOL_CONFIG: &str = r#"[{"symbol":"BTCUSDT","marginType":"CROSSED","isAutoAddMargin":false,"leverage":20,"maxNotionalValue":"$1000000"}]"#;
const ACCOUNT: &str = r#"{"totalInitialMargin":"48","totalWalletBalance":"1000","availableBalance":"952","assets":[{"asset":"USDT","walletBalance":"1000.0","unrealizedProfit":"10.0","marginBalance":"1010.0","maintMargin":"3.8","initialMargin":"48.0","availableBalance":"952.0","updateTime":1625474304765},{"asset":"BTC","walletBalance":"0.0","unrealizedProfit":"0.0","initialMargin":"0","availableBalance":"0"}],"positions":[]}"#;
const NEW_ORDER: &str = r#"{"e":"ORDER_TRADE_UPDATE","E":1568879465651,"T":1568879465650,"o":{"s":"BTCUSDT","c":"TEST","S":"SELL","o":"LIMIT","f":"GTC","q":"0.002","p":"97000","ap":"0","sp":"0","x":"NEW","X":"NEW","i":8886774,"l":"0","z":"0","L":"0","T":1568879465650,"t":0,"b":"0","a":"0","m":false,"R":false,"wt":"CONTRACT_PRICE","ot":"LIMIT","ps":"BOTH","cp":false,"rp":"0"}}"#;

fn credentials() -> Options {
    Options {
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Default::default()
    }
}

/// A private source with every startup read answered but its open-orders reads,
/// and its sockets open.
fn orders_read_out() -> Harness {
    let mut h = Harness::started(credentials());
    h.respond("/fapi/v1/exchangeInfo", 200, support::EXCHANGE_INFO);
    h.respond(
        "/fapi/v1/leverageBracket",
        200,
        r#"[{"symbol":"BTCUSDT","brackets":[{"bracket":1,"initialLeverage":125,"notionalCap":50000}]}]"#,
    );
    h.respond("/fapi/v1/ticker/24hr", 200, support::DAILY);
    h.respond("/fapi/v1/ticker/bookTicker", 200, support::BOOK_TICKERS);
    h.respond("/fapi/v1/premiumIndex", 200, support::PREMIUM);
    h.respond(
        "/fapi/v1/listenKey",
        200,
        r#"{"listenKey":"pqia91ma19a5s61cv6a81va65sdf19v8a65a1a5s61cv6a81va65sdf19v8a65a1"}"#,
    );
    h.respond(
        "/fapi/v1/positionSide/dual",
        200,
        r#"{"dualSidePosition":false}"#,
    );
    h.respond("/fapi/v3/positionRisk", 200, POSITION_RISK);
    h.respond("/fapi/v1/symbolConfig", 200, SYMBOL_CONFIG);
    h.respond("/fapi/v3/account", 200, ACCOUNT);
    for path in ["/public/ws", "/market/ws", "/private/ws"] {
        let socket = h.socket(path).unwrap();
        h.event(Event::WsOpened { socket });
    }
    assert!(h.has_request("/fapi/v1/openOrders"), "{:?}", h.paths());
    h
}

fn answer_orders(h: &mut Harness, regular: &str) {
    h.respond("/fapi/v1/openOrders", 200, regular);
    h.respond("/fapi/v1/openAlgoOrders", 200, "[]");
}

fn account_cover(h: &mut Harness) -> (Vec<u16>, Option<f64>) {
    let frame = h.engine.snapshot_frame(h.now).to_vec();
    let kinds = safe_cex_engine::wire::sections(&frame)
        .map(|(kind, _)| kind)
        .collect();
    let cover = safe_cex_engine::wire::sections(&frame)
        .find(|(kind, _)| *kind == section::ORDERS_READ)
        .map(|(_, payload)| f64::from_le_bytes(payload[8..16].try_into().unwrap()))
        .filter(|cover| !cover.is_nan());
    (kinds, cover)
}

#[test]
fn a_complete_open_orders_read_sends_its_cover_beside_orders() {
    let mut h = orders_read_out();
    let observed = h.now;
    h.advance(4_000.0);
    answer_orders(&mut h, "[]");
    let (kinds, cover) = account_cover(&mut h);
    let orders = kinds
        .iter()
        .position(|kind| *kind == section::ORDERS)
        .unwrap();
    assert_eq!(kinds[orders + 1], section::ORDERS_READ, "{kinds:?}");
    let cover = cover.unwrap();
    assert!(
        cover <= observed && cover < h.now,
        "send time, not landing: {cover}"
    );
}

/// H10: the user stream reports an order after the read's release; the read
/// lacks it, keeps the row, and proves no cover.
#[test]
fn a_stream_order_after_the_reads_release_leaves_no_cover() {
    let mut h = orders_read_out();
    h.message("/private/ws", NEW_ORDER);
    answer_orders(&mut h, "[]");
    assert!(
        h.engine.store().order("8886774").is_some(),
        "the stream row stays"
    );
    assert!(
        h.engine.store().order("7").is_none(),
        "the row was not mapped"
    );
    assert_eq!(account_cover(&mut h).1, None);
}

/// A row the read cannot map (an unsupported native type) proves no account cover.
#[test]
fn an_unmappable_open_order_proves_no_cover() {
    let mut h = orders_read_out();
    answer_orders(
        &mut h,
        r#"[{"symbol":"BTCUSDT","orderId":7,"clientOrderId":"bad","price":"100","origQty":"1","executedQty":"0","side":"BUY","positionSide":"BOTH","status":"NEW","type":"NOT_A_BINANCE_TYPE","timeInForce":"GTC","stopPrice":"0","reduceOnly":false,"closePosition":false,"workingType":"CONTRACT_PRICE","updateTime":1}]"#,
    );
    assert!(
        h.engine.store().order("7").is_none(),
        "the row was not mapped"
    );
    assert_eq!(account_cover(&mut h).1, None);
}

#[test]
fn split_reads_cover_uses_earliest_actual_claim() {
    let mut h = Harness::started_deferred(credentials());
    h.hold_signatures(true);
    h.respond("/fapi/v1/exchangeInfo", 200, support::EXCHANGE_INFO);
    h.sign("/fapi/v1/openOrders");
    let early = h.now;
    h.respond("/fapi/v1/openOrders", 200, "[]");
    h.now += 1000.;
    h.sign("/fapi/v1/openAlgoOrders");
    h.respond("/fapi/v1/openAlgoOrders", 200, "[]");
    assert_eq!(
        account_cover(&mut h).1,
        Some(early),
        "all rows are only as fresh as the earliest actual leg release"
    );
}
fn live_stop(h: &mut Harness) {
    let stop = NEW_ORDER
        .replace(r#""o":"LIMIT""#, r#""o":"STOP""#)
        .replace(r#""ot":"LIMIT""#, r#""ot":"STOP""#)
        .replace(r#""sp":"0""#, r#""sp":"90000""#);
    h.message("/private/ws", &stop);
    assert_eq!(
        h.engine.store().order("8886774").unwrap().kind,
        safe_cex_engine::model::OrderType::StopLoss
    );
}
fn amend_must_stay_refused(h: &mut Harness) {
    // The stop stays on its own side of the price, so the venue's stop-limit refusal decides.
    h.engine.store_mut().update_ticker("BTCUSDT", |ticker| {
        ticker.mark = 95_000.;
        ticker.last = 95_000.;
    });
    h.event(Event::Command(safe_cex_engine::Command::AmendOrder {
        command: 900,
        id: "8886774".into(),
        price: Some(91000.),
        amount: None,
    }));
    assert!(
        !h.requests
            .iter()
            .any(|r| r.method != safe_cex_engine::Method::Get),
        "refused stale read cannot remove stop-limit protection: {:?}",
        h.paths()
    );
    assert!(
        h.failures
            .iter()
            .any(|(id, e)| *id == 900 && e.kind == Some(safe_cex_engine::ErrorKind::Unsupported))
    );
}
#[test]
fn stream_stop_limit_amend_is_refused_control() {
    let mut h = orders_read_out();
    live_stop(&mut h);
    amend_must_stay_refused(&mut h);
}
#[test]
fn refused_old_read_cannot_change_stop_limit_classification() {
    let mut h = orders_read_out();
    live_stop(&mut h);
    answer_orders(
        &mut h,
        r#"[{"symbol":"BTCUSDT","orderId":8886774,"clientOrderId":"TEST","price":"97000","origQty":"0.002","executedQty":"0","side":"SELL","positionSide":"BOTH","status":"NEW","type":"STOP_MARKET","timeInForce":"GTC","stopPrice":"90000","reduceOnly":false,"closePosition":false,"workingType":"CONTRACT_PRICE","updateTime":1}]"#,
    );
    assert_eq!(
        h.engine.store().order("8886774").unwrap().kind,
        safe_cex_engine::model::OrderType::StopLoss
    );
    assert_eq!(account_cover(&mut h).1, None);
    amend_must_stay_refused(&mut h);
}
#[test]
fn unidentified_row_grants_no_cover() {
    let mut h = orders_read_out();
    answer_orders(&mut h, r#"[{"broken":true}]"#);
    assert_eq!(account_cover(&mut h).1, None);
}
