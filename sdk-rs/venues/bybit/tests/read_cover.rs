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
