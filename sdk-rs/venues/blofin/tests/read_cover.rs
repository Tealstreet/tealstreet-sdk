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

fn finish_row(h: &mut Harness, category: &str) {
    h.reply("orders-pending", &format!(r#"{{"code":"0","data":[{{"instId":"BTC-USDT","orderCategory":"{category}","orderId":"new-working","orderType":"limit","side":"buy","marginMode":"cross","positionSide":"net","size":"10","price":"1","state":"live"}}]}}"#));
    h.reply("orders-pending", EMPTY);
    h.reply("orders-tpsl-pending", EMPTY);
    h.reply("orders-algo-pending", EMPTY);
}
#[test]
fn supported_working_row_advances_cover_control() {
    let (mut h, _) = census_out();
    finish_row(&mut h, "normal");
    assert!(h.e.store().order("new-working").is_some());
    assert_eq!(account_cover(&mut h).1, Some(10_000.));
}
#[test]
fn unsupported_working_row_must_not_advance_cover() {
    let (mut h, before) = census_out();
    finish_row(&mut h, "future_category");
    assert!(h.e.store().order("new-working").is_none());
    assert_eq!(
        account_cover(&mut h).1,
        before,
        "unmapped live owned order is not proof of absence"
    );
}
fn state(h: &mut Harness) -> Vec<u8> {
    let frame = h.e.snapshot_frame(h.now).to_vec();
    safe_cex_engine::wire::sections(&frame)
        .find(|(k, _)| *k == section::VENUE_STATE)
        .unwrap()
        .1
        .to_vec()
}
#[test]
fn same_clock_stream_source_and_rows_survive_refused_snapshot() {
    let (mut h, before) = census_out();
    h.ws(&format!(
        r#"{{"arg":{{"channel":"orders"}},"data":[{STREAMED}]}}"#
    ));
    let row = h.e.store().order("streamed").unwrap().clone();
    let metadata = state(&mut h);
    h.reply(
        "orders-pending",
        &format!(
            r#"{{"code":"0","data":[{}]}}"#,
            STREAMED
                .replace("cross", "isolated")
                .replace("net", "long")
                .replace("59000", "1")
        ),
    );
    h.reply("orders-pending", EMPTY);
    h.reply("orders-tpsl-pending", EMPTY);
    h.reply("orders-algo-pending", EMPTY);
    assert_eq!(h.e.store().order("streamed"), Some(&row));
    assert_eq!(state(&mut h), metadata);
    assert_eq!(account_cover(&mut h).1, before);
}

/// BloFin advertises read covers in its own snapshot.
#[test]
fn blofin_advertises_read_covers() {
    let mut h = Harness::new();
    let frame = h.e.snapshot_frame(h.now).to_vec();
    let (_, payload) = safe_cex_engine::wire::sections(&frame)
        .find(|(kind, _)| *kind == section::CAPABILITIES)
        .unwrap();
    let flags = u32::from_le_bytes(payload[..4].try_into().unwrap());
    assert_ne!(
        flags & 1 << safe_cex_engine::Capabilities::ORDER_READ_COVERS_BIT,
        0
    );
}

fn reads(h: &Harness) -> usize {
    h.pending
        .iter()
        .filter(|r| r.url.contains("/orders-pending"))
        .count()
}

fn ask(h: &mut Harness, at: f64, command: u32, symbols: Option<Vec<String>>) {
    h.handle(
        at,
        Event::Command(safe_cex_engine::Command::Venue {
            command,
            action: safe_cex_engine::VenueAction::RefreshOrders { symbols },
        }),
    );
    h.collect();
}

/// Answers a whole census (every family) at `at`.
fn answer(h: &mut Harness, at: f64) {
    for path in [
        "orders-pending",
        "orders-tpsl-pending",
        "orders-algo-pending",
    ] {
        let read = h.take(path);
        h.respond_at(read, EMPTY, at);
    }
}

/// A seeded account with no census out.
fn loaded() -> Harness {
    let mut h = Harness::new();
    h.open_private();
    h.seed();
    assert_eq!(reads(&h), 0);
    h
}

/// A refresh ask reads every order family, and the census publishes an
/// account cover at the ask's send time.
#[test]
fn a_refresh_ask_reads_orders_and_publishes_its_send_time() {
    for symbols in [None, Some(vec!["BTC-USDT".to_owned()])] {
        let mut h = loaded();
        ask(&mut h, 5_000., 50, symbols.clone());
        assert_eq!(reads(&h), 1, "{symbols:?}: the ask reads");
        answer(&mut h, 6_000.);
        assert_eq!(account_cover(&mut h).1, Some(5_000.), "{symbols:?}");
        assert_eq!(reads(&h), 0, "{symbols:?}: nothing more");
    }
}

/// H7/H3: asks while the read is out start nothing; exactly one follow-up
/// census starts after it lands, and none after that.
#[test]
fn asks_while_a_read_is_out_get_one_follow_up() {
    let mut h = loaded();
    ask(&mut h, 5_000., 50, None);
    assert_eq!(reads(&h), 1);
    for command in 51..150 {
        ask(
            &mut h,
            5_000. + f64::from(command),
            command,
            Some(vec!["BTC-USDT".to_owned()]),
        );
    }
    assert_eq!(reads(&h), 1, "H7");
    answer(&mut h, 6_000.);
    assert_eq!(reads(&h), 1, "one follow-up");
    answer(&mut h, 7_000.);
    assert_eq!(account_cover(&mut h).1, Some(6_000.));
    assert_eq!(reads(&h), 0, "none after it");
}
