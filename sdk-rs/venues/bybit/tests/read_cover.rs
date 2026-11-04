//! Read covers on a real Bybit source (docs/plans/RUST_FRAME_FED_READ_COVER.md
//! step 1b): every complete orders read commits its rows with the cover it
//! proves, which goes out directly after ORDERS on every output.
mod support;
use safe_cex_engine::Event;
use safe_cex_engine::wire::section;
use support::{Harness, Options};

const EMPTY_LIST: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[],"nextPageCursor":"","category":"linear"},"retExtInfo":{},"time":1684765770483}"#;
const WALLET: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"accountType":"UNIFIED","totalEquity":"1100","totalWalletBalance":"1000","totalInitialMargin":"10","totalMaintenanceMargin":"1","totalAvailableBalance":"900","coin":[]}]}}"#;
/// "Order", the documented private stream frame, as a resting linear order.
const STREAMED_ORDER: &str = r#"{"topic":"order","data":[{"category":"linear","symbol":"BTCUSDT","orderId":"streamed","side":"Buy","orderType":"Limit","orderStatus":"New","price":"50000","qty":"0.01","leavesQty":"0.01","cumExecQty":"0","updatedTime":"1672364262457"}]}"#;

fn started() -> Harness {
    let mut h = Harness::started(Options {
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    h.respond("/v5/account/fee-rate", 200, support::FEE_RATES);
    h.respond("/v5/account/info", 200, support::ACCOUNT_INFO);
    h.load_public();
    assert!(h.has_request("/v5/order/realtime"), "{:?}", h.paths());
    h
}

fn answer_all(h: &mut Harness, path: &str, body: &str) {
    while h.has_request(path) {
        h.respond(path, 200, body);
    }
}

fn answer_account(h: &mut Harness) {
    answer_all(h, "/v5/order/realtime", EMPTY_LIST);
    answer_all(h, "/v5/position/list", EMPTY_LIST);
    answer_all(h, "/v5/account/wallet-balance", WALLET);
}

fn stream_order(h: &mut Harness, read_after: bool) {
    let socket = h.socket("/v5/private").expect("private socket");
    h.event(Event::WsOpened { socket });
    h.message(
        "/v5/private",
        r#"{"success":true,"ret_msg":"","op":"auth"}"#,
    );
    h.message("/v5/private", STREAMED_ORDER);
    if read_after {
        // A subscription acknowledgement releases a fresh account read, after
        // the stream row and in the same millisecond.
        h.message(
            "/v5/private",
            r#"{"success":true,"ret_msg":"","op":"subscribe"}"#,
        );
    }
}

/// The section kinds of the source's snapshot, and its account cover.
fn snapshot(h: &mut Harness) -> (Vec<u16>, Option<f64>) {
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
fn a_complete_orders_read_sends_its_send_time_beside_orders() {
    let mut h = started();
    let observed = h.now;
    h.advance(4_000.0);
    answer_account(&mut h);
    assert!(h.engine.store().loaded().orders);
    let (kinds, cover) = snapshot(&mut h);
    let orders = kinds
        .iter()
        .position(|kind| *kind == section::ORDERS)
        .unwrap();
    assert_eq!(kinds[orders + 1], section::ORDERS_READ, "{kinds:?}");
    let cover = cover.unwrap();
    assert!(
        cover <= observed && cover < h.now,
        "the cover is when the read went out ({cover}), not when it landed ({})",
        h.now
    );
    let delta = h.engine.take_frame(h.now).unwrap().to_vec();
    assert!(safe_cex_engine::wire::sections(&delta).any(|(kind, _)| kind == section::ORDERS_READ));
}

/// H10 on a real source: the private stream inserts an order in the same
/// millisecond as, and after, the orders read's release. The read lacks it,
/// so it proves nothing for that symbol, and no account cover is published.
#[test]
fn a_stream_order_after_the_reads_release_leaves_no_cover_from_that_read() {
    let mut h = started();
    stream_order(&mut h, false);
    answer_account(&mut h);
    assert!(
        h.engine
            .store()
            .orders()
            .iter()
            .any(|order| order.id == "streamed"),
        "the stream row stays"
    );
    assert_eq!(snapshot(&mut h).1, None);
}

/// The reverse order: the stream row lands first, then a read released after
/// it in the same millisecond commits and proves its cover.
#[test]
fn a_read_released_after_a_same_millisecond_stream_order_proves_its_cover() {
    let mut h = started();
    answer_account(&mut h);
    let startup = snapshot(&mut h).1.unwrap();
    h.advance(2_000.0);
    stream_order(&mut h, true);
    let released = h.now;
    answer_account(&mut h);
    assert!(
        h.engine
            .store()
            .orders()
            .iter()
            .all(|order| order.id != "streamed"),
        "the later read speaks for that row: it is no longer open"
    );
    let cover = snapshot(&mut h).1.unwrap();
    assert!(
        cover > startup && cover <= released,
        "the read released after the stream row proves its cover: {cover}"
    );
}

/// codex-hcmq3f (#3005): a rejected native row, named or not, proves no
/// account cover; the healthy rows still publish.
fn rejected_row_read_cover(unidentified: bool) {
    let mut h = started();
    let mut row =
        serde_json::from_str::<serde_json::Value>(STREAMED_ORDER).unwrap()["data"][0].clone();
    row["price"] = serde_json::json!("NaN");
    if unidentified {
        row = serde_json::json!({"price":"NaN"});
    }
    let body = serde_json::json!({"retCode":0,"retMsg":"OK","result":{"list":[row],"nextPageCursor":"","category":"linear"}}).to_string();
    answer_all(&mut h, "/v5/order/realtime", &body);
    answer_all(&mut h, "/v5/position/list", EMPTY_LIST);
    answer_all(&mut h, "/v5/account/wallet-balance", WALLET);
    assert!(h.notices.iter().any(|n| matches!(n, safe_cex_engine::Notice::Event(e) if e.code == safe_cex_engine::log::code::ROW_SKIPPED)), "actual rejected native row");
    assert!(h.engine.store().orders().is_empty());
    assert_eq!(
        snapshot(&mut h).1,
        None,
        "a rejected native order cannot prove account-wide absence"
    );
}

#[test]
fn an_unreadable_known_symbol_cannot_publish_an_account_cover() {
    rejected_row_read_cover(false);
}

#[test]
fn an_unidentified_order_cannot_publish_an_account_cover() {
    rejected_row_read_cover(true);
}

/// codex-hcmq3f (#3005): an unsupported native order type proves no account cover.
#[test]
fn an_unsupported_order_cannot_publish_an_account_cover() {
    let mut h = started();
    let mut row =
        serde_json::from_str::<serde_json::Value>(STREAMED_ORDER).unwrap()["data"][0].clone();
    row["orderType"] = serde_json::json!("UnrecognizedNativeType");
    let body = serde_json::json!({"retCode":0,"retMsg":"OK","result":{"list":[row],"nextPageCursor":"","category":"linear"}}).to_string();
    answer_all(&mut h, "/v5/order/realtime", &body);
    answer_all(&mut h, "/v5/position/list", EMPTY_LIST);
    answer_all(&mut h, "/v5/account/wallet-balance", WALLET);
    assert!(
        h.logs.iter().any(|line| line.contains("unsupported type")),
        "actual unsupported native type: {:?}",
        h.logs
    );
    assert!(h.engine.store().orders().is_empty());
    assert_eq!(
        snapshot(&mut h).1,
        None,
        "an unmapped native order cannot prove account-wide absence"
    );
}

/// codex-hcmq3f (#3005): a stream-created full position stop, kept by the
/// sequence refusal, keeps its position-bound route: its amend goes to
/// /v5/position/trading-stop, never /v5/order/amend.
#[test]
fn a_refused_read_keeps_the_stream_stops_position_route() {
    let mut h = started();
    let socket = h.socket("/v5/private").unwrap();
    h.event(Event::WsOpened { socket });
    h.message("/v5/private", r#"{"success":true,"op":"auth"}"#);
    h.message("/v5/private", r#"{"topic":"order","data":[{"category":"linear","symbol":"BTCUSDT","orderId":"position-stop","side":"Sell","orderType":"Market","orderStatus":"Untriggered","stopOrderType":"StopLoss","tpslMode":"Full","qty":"0.005","triggerPrice":"86000","triggerBy":"MarkPrice","reduceOnly":true,"closeOnTrigger":true,"positionIdx":0}]}"#);
    answer_account(&mut h);
    assert!(
        h.engine.store().order("position-stop").is_some(),
        "sequence refusal retains the native row"
    );
    h.requests.clear();
    h.command(safe_cex_engine::Command::AmendOrder {
        command: 901,
        id: "position-stop".into(),
        price: Some(87000.),
        amount: None,
    });
    if h.has_request("/v5/position/list?category=linear&symbol=BTCUSDT") {
        h.respond("/v5/position/list?category=linear&symbol=BTCUSDT", 200, r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"positionIdx":0,"riskId":1,"riskLimitValue":"2000000","symbol":"BTCUSDT","side":"Buy","size":"0.010","avgPrice":"95000","positionValue":"950","tradeMode":0,"positionStatus":"Normal","autoAddMargin":0,"adlRankIndicator":2,"leverage":"10","breakEvenPrice":"95010","positionBalance":"95","markPrice":"96000","liqPrice":"86000.5","bustPrice":"","positionMM":"5","positionMMByMp":"5","positionIM":"95","positionIMByMp":"95","tpslMode":"Full","takeProfit":"0.00","stopLoss":"0.00","trailingStop":"0.00","unrealisedPnl":"10","curRealisedPnl":"-0.5","cumRealisedPnl":"-1","seq":5723621632,"isReduceOnly":false,"mmrSysUpdateTime":"","leverageSysUpdatedTime":"","sessionAvgPrice":"","createdTime":"1676538056258","updatedTime":"1697673600012"},{"positionIdx":0,"symbol":"ETHUSDT","side":"","size":"0","avgPrice":"0","positionValue":"0","leverage":"20","unrealisedPnl":"0","curRealisedPnl":"0","liqPrice":"","positionIM":"0","seq":-1,"positionStatus":"","updatedTime":""}],"nextPageCursor":"","category":"linear"},"retExtInfo":{},"time":1697684980172}"#);
    }
    assert!(
        h.has_request("/v5/position/trading-stop"),
        "position-bound native routing must stay with the retained row; actual requests {:?}, terminals {:?}",
        h.paths(),
        h.done
    );
    assert!(!h.has_request("/v5/order/amend"));
}

/// The control: the same stop streamed after the read landed keeps its route.
#[test]
fn a_stream_stop_after_the_read_keeps_its_position_route() {
    let mut h = started();
    answer_account(&mut h);
    let socket = h.socket("/v5/private").unwrap();
    h.event(Event::WsOpened { socket });
    h.message("/v5/private", r#"{"success":true,"op":"auth"}"#);
    h.message("/v5/private", r#"{"topic":"order","data":[{"category":"linear","symbol":"BTCUSDT","orderId":"position-stop","side":"Sell","orderType":"Market","orderStatus":"Untriggered","stopOrderType":"StopLoss","tpslMode":"Full","qty":"0.005","triggerPrice":"86000","triggerBy":"MarkPrice","reduceOnly":true,"closeOnTrigger":true,"positionIdx":0}]}"#);
    assert!(
        h.engine.store().order("position-stop").is_some(),
        "sequence refusal retains the native row"
    );
    h.requests.clear();
    h.command(safe_cex_engine::Command::AmendOrder {
        command: 901,
        id: "position-stop".into(),
        price: Some(87000.),
        amount: None,
    });
    if h.has_request("/v5/position/list?category=linear&symbol=BTCUSDT") {
        h.respond("/v5/position/list?category=linear&symbol=BTCUSDT", 200, r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"positionIdx":0,"riskId":1,"riskLimitValue":"2000000","symbol":"BTCUSDT","side":"Buy","size":"0.010","avgPrice":"95000","positionValue":"950","tradeMode":0,"positionStatus":"Normal","autoAddMargin":0,"adlRankIndicator":2,"leverage":"10","breakEvenPrice":"95010","positionBalance":"95","markPrice":"96000","liqPrice":"86000.5","bustPrice":"","positionMM":"5","positionMMByMp":"5","positionIM":"95","positionIMByMp":"95","tpslMode":"Full","takeProfit":"0.00","stopLoss":"0.00","trailingStop":"0.00","unrealisedPnl":"10","curRealisedPnl":"-0.5","cumRealisedPnl":"-1","seq":5723621632,"isReduceOnly":false,"mmrSysUpdateTime":"","leverageSysUpdatedTime":"","sessionAvgPrice":"","createdTime":"1676538056258","updatedTime":"1697673600012"},{"positionIdx":0,"symbol":"ETHUSDT","side":"","size":"0","avgPrice":"0","positionValue":"0","leverage":"20","unrealisedPnl":"0","curRealisedPnl":"0","liqPrice":"","positionIM":"0","seq":-1,"positionStatus":"","updatedTime":""}],"nextPageCursor":"","category":"linear"},"retExtInfo":{},"time":1697684980172}"#);
    }
    assert!(
        h.has_request("/v5/position/trading-stop"),
        "position-bound native routing must stay with the retained row; actual requests {:?}, terminals {:?}",
        h.paths(),
        h.done
    );
    assert!(!h.has_request("/v5/order/amend"));
}
