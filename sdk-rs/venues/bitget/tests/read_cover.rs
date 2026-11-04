//! Read covers on a real Bitget source (docs/plans/RUST_FRAME_FED_READ_COVER.md
//! venue pair 2): the open-order census (normal, protection and trailing pages,
//! each its own read) commits its rows with the cover of its earliest page.
mod support;
use safe_cex_engine::wire::section;
use support::{ACCOUNT, Harness, OPEN_ORDER, POSITION};

const EMPTY_PLANS: &str = r#"{"code":"00000","data":{"entrustedList":[]}}"#;
const EMPTY_TRAILING: &str = r#"{"code":"00000","data":[]}"#;

/// A classic account at startup with its first census's normal page still out.
fn census_out() -> Harness {
    let mut h = Harness::private_started();
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"40084","msg":"Classic Account mode","data":null}"#,
    );
    h.respond("/all-position", 200, POSITION);
    h.respond("/account/accounts", 200, ACCOUNT);
    h.login();
    assert!(h.requests.iter().any(|r| r.url.contains("/orders-pending")));
    h
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

/// Each page answers 1 s after the last: the cover is the first page's send.
#[test]
fn a_census_cover_is_its_first_pages_send_beside_orders() {
    let mut h = census_out();
    let first = h.now;
    h.now += 1_000.0;
    h.respond("/orders-pending", 200, OPEN_ORDER);
    h.now += 1_000.0;
    h.respond("planType=profit_loss", 200, EMPTY_PLANS);
    h.now += 1_000.0;
    h.respond("planType=track_plan", 200, EMPTY_TRAILING);
    let (kinds, cover) = account_cover(&mut h);
    let orders = kinds
        .iter()
        .position(|kind| *kind == section::ORDERS)
        .unwrap();
    assert_eq!(kinds[orders + 1], section::ORDERS_READ, "{kinds:?}");
    assert_eq!(cover, Some(first), "the earliest page, not the last");
}

/// H10: the stream opens an order after the census went out; the census lacks
/// it, keeps the row, and proves no cover.
#[test]
fn a_stream_order_after_the_census_went_out_leaves_no_cover() {
    let mut h = census_out();
    h.private_message(r#"{"arg":{"channel":"orders"},"data":[{"orderId":"7","symbol":"BTCUSDT","size":"1","price":"88","side":"buy","orderType":"limit","status":"live"}]}"#);
    h.orders(OPEN_ORDER);
    assert!(
        h.engine.store().order("7").is_some(),
        "the stream row stays"
    );
    assert_eq!(account_cover(&mut h).1, None);
}

/// A row the census cannot map proves no account cover.
#[test]
fn an_unmappable_census_row_proves_no_cover() {
    let mut h = census_out();
    h.orders(r#"{"code":"00000","data":{"entrustedList":[{"orderId":"9","symbol":"NOTAMARKET","size":"1","price":"1","side":"buy","orderType":"limit","status":"live"}],"endId":""}}"#);
    assert!(
        h.engine.store().order("9").is_none(),
        "the row was not mapped"
    );
    assert_eq!(account_cover(&mut h).1, None);
}

/// The follow-up a refused census owes waits at least 2 s from that census's
/// start, so a busy symbol cannot drive back-to-back census reads.
#[test]
fn the_follow_up_to_a_refused_census_is_rate_bounded() {
    let mut h = census_out();
    let started = h.now;
    h.private_message(r#"{"arg":{"channel":"orders"},"data":[{"orderId":"7","symbol":"BTCUSDT","size":"1","price":"88","side":"buy","orderType":"limit","status":"live"}]}"#);
    let before: Vec<_> = h.timers.iter().map(|(timer, _)| *timer).collect();
    h.orders(OPEN_ORDER);
    assert!(
        !h.requests.iter().any(|r| r.url.contains("/orders-pending")),
        "no census straight after the refused one"
    );
    let armed: Vec<f64> = h
        .timers
        .iter()
        .filter(|(timer, _)| !before.contains(timer))
        .map(|(_, at)| *at)
        .collect();
    assert_eq!(armed.len(), 1, "one follow-up: {armed:?}");
    assert!(
        armed[0] >= started + 2_000.0 - 0.01,
        "no sooner than 2 s: {armed:?}"
    );
}
