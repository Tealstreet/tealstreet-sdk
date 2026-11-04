//! Read covers on a real BloFin source (docs/plans/RUST_FRAME_FED_READ_COVER.md
//! venue pair 2): the pending-order census (normal, TP/SL and algo families,
//! each its own read) commits its rows with the cover of its earliest page.
mod support;
use safe_cex_engine::Event;
use safe_cex_engine::wire::section;
use support::Harness;

const EMPTY: &str = r#"{"code":"0","data":[]}"#;
const STREAMED: &str = r#"{"instId":"BTC-USDT","orderCategory":"normal","orderId":"streamed","orderType":"limit","side":"buy","marginMode":"cross","positionSide":"net","size":"10","price":"59000","state":"live"}"#;

fn account_cover(h: &mut Harness) -> (Vec<u16>, Option<f64>) {
    let frame = h.e.snapshot_frame(h.now).to_vec();
    let kinds = safe_cex_engine::wire::sections(&frame)
        .map(|(kind, _)| kind)
        .collect();
    let cover = safe_cex_engine::wire::sections(&frame)
        .find(|(kind, _)| *kind == section::ORDERS_READ)
        .map(|(_, payload)| f64::from_le_bytes(payload[8..16].try_into().unwrap()))
        .filter(|cover| !cover.is_nan());
    (kinds, cover)
}

/// A seeded account whose next census has started at 10 s.
fn census_out() -> (Harness, Option<f64>) {
    let mut h = Harness::new();
    h.open_private();
    h.seed();
    let before = account_cover(&mut h).1;
    h.handle(10_000., Event::TimerFired { timer: h.tick });
    h.collect();
    (h, before)
}

/// Each family answers 1 s after the last: the cover is the first page's send.
#[test]
fn a_census_cover_is_its_first_pages_send_beside_orders() {
    let (mut h, _) = census_out();
    let first = h.take("orders-pending");
    h.respond_at(first, EMPTY, 11_000.);
    let tpsl = h.take("orders-tpsl-pending");
    h.respond_at(tpsl, EMPTY, 12_000.);
    let algo = h.take("orders-algo-pending");
    h.respond_at(algo, EMPTY, 13_000.);
    let (kinds, cover) = account_cover(&mut h);
    let orders = kinds
        .iter()
        .position(|kind| *kind == section::ORDERS)
        .unwrap();
    assert_eq!(kinds[orders + 1], section::ORDERS_READ, "{kinds:?}");
    assert_eq!(cover, Some(10_000.), "the earliest page, not the last");
}

/// H10: the stream opens an order after the census went out; the census lacks
/// it, keeps the row, and proves no newer cover.
#[test]
fn a_stream_order_after_the_census_went_out_leaves_its_cover_where_it_was() {
    let (mut h, before) = census_out();
    h.ws(&format!(
        r#"{{"arg":{{"channel":"orders"}},"data":[{STREAMED}]}}"#
    ));
    for path in [
        "orders-pending",
        "orders-tpsl-pending",
        "orders-algo-pending",
    ] {
        h.reply(path, EMPTY);
    }
    assert!(
        h.e.store().order("streamed").is_some(),
        "the stream row stays"
    );
    assert_eq!(account_cover(&mut h).1, before);
}

/// A row the census cannot map proves no newer cover.
#[test]
fn an_unmappable_census_row_proves_no_newer_cover() {
    let (mut h, before) = census_out();
    h.reply(
        "orders-pending",
        r#"{"code":"0","data":[{"instId":"BTC-USDT","orderCategory":"normal","orderId":"bad","orderType":"limit","side":"buy","marginMode":"cross","positionSide":"net","size":["undecodable"],"price":"1","state":"live"}]}"#,
    );
    // A page with rows is followed by the family's next page.
    h.reply("orders-pending", EMPTY);
    h.reply("orders-tpsl-pending", EMPTY);
    h.reply("orders-algo-pending", EMPTY);
    assert!(h.e.store().order("bad").is_none(), "the row was not mapped");
    assert_eq!(account_cover(&mut h).1, before);
}
