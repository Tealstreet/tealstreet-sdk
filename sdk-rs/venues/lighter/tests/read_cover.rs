//! Read covers on a real Lighter source (docs/plans/RUST_FRAME_FED_READ_COVER.md
//! venue pair 3): the open-order census (one all-market read, or one read per
//! market) commits its rows with the cover of its earliest read.
mod support;
use safe_cex_engine::wire::section;
use safe_cex_engine::{Action, Engine, Event, HttpRequest};
use safe_cex_venue_lighter::Lighter;
use support::*;

const EMPTY: &str = r#"{"code":200,"orders":[]}"#;
const STREAM_ORDER: &str = r#"{"channel":"account_all_orders:718895","orders":{"1":[{"order_index":"77","market_id":1,"status":"open","initial_base_amount":"1","price":"95"}]}}"#;

fn account_cover(e: &mut Engine<Lighter>, now: f64) -> (Vec<u16>, Option<f64>) {
    let frame = e.snapshot_frame(now).to_vec();
    let kinds = safe_cex_engine::wire::sections(&frame)
        .map(|(kind, _)| kind)
        .collect();
    let cover = safe_cex_engine::wire::sections(&frame)
        .find(|(kind, _)| *kind == section::ORDERS_READ)
        .map(|(_, payload)| f64::from_le_bytes(payload[8..16].try_into().unwrap()))
        .filter(|cover| !cover.is_nan());
    (kinds, cover)
}

/// A fresh census sent at `at` (the socket reopening asks for one), with
/// the account read beside it answered.
fn census(e: &mut Engine<Lighter>, socket: safe_cex_engine::SocketId, at: f64) -> HttpRequest {
    e.handle(at, Event::WsOpened { socket });
    let mut orders = None;
    for r in requests_at(e, at) {
        if r.url.contains("ActiveOrders") {
            orders = Some(r);
        } else if r.url.contains("/account?") {
            reply_at(e, at, &r, 200, ACCOUNT);
        }
    }
    orders.expect("an open-order census")
}

fn next_census_read(e: &mut Engine<Lighter>, now: f64) -> HttpRequest {
    requests_at(e, now)
        .into_iter()
        .find(|r| r.url.contains("ActiveOrders"))
        .expect("the census's next read")
}

/// Per-market reads, each 1 s after the last: the cover is the census's first
/// read, never its last.
#[test]
fn a_census_cover_is_its_earliest_reads_send_beside_orders() {
    let (mut e, s) = ready();
    let sent = NOW + 1_000.0;
    let all = census(&mut e, s, sent);
    reply_at(
        &mut e,
        sent + 1_000.0,
        &all,
        400,
        r#"{"code":400,"message":"invalid market parameter"}"#,
    );
    let first = next_census_read(&mut e, sent + 1_000.0);
    reply_at(&mut e, sent + 2_000.0, &first, 200, OPEN);
    let second = next_census_read(&mut e, sent + 2_000.0);
    reply_at(&mut e, sent + 3_000.0, &second, 200, EMPTY);
    assert!(e.store().order("42").is_some());
    let (kinds, cover) = account_cover(&mut e, sent + 3_000.0);
    let orders = kinds
        .iter()
        .position(|kind| *kind == section::ORDERS)
        .unwrap();
    assert_eq!(kinds[orders + 1], section::ORDERS_READ, "{kinds:?}");
    assert_eq!(cover, Some(sent), "the earliest read, not the last");
}

/// H10: the stream opens an order after the census went out; the census lacks
/// it, keeps the row, and the cover stays at the census before it.
#[test]
fn a_stream_order_after_the_census_went_out_keeps_the_older_cover() {
    let (mut e, s) = ready();
    assert_eq!(
        account_cover(&mut e, NOW).1,
        Some(NOW),
        "the startup census"
    );
    let sent = NOW + 1_000.0;
    let read = census(&mut e, s, sent);
    message(&mut e, s, sent + 1.0, STREAM_ORDER);
    reply_at(&mut e, sent + 2.0, &read, 200, OPEN);
    assert!(e.store().order("77").is_some(), "the stream row stays");
    assert_eq!(account_cover(&mut e, sent + 2.0).1, Some(NOW));
}

/// A working row on a market the catalogue lacks proves no newer cover; the
/// rest of the census still commits.
#[test]
fn an_unmappable_working_row_proves_no_newer_cover() {
    let unmapped = r#"{"order_index":"9","market_id":99,"status":"open","type":"limit","is_ask":false,"initial_base_amount":"1","price":"1"}"#;
    let mapped = r#"{"order_index":"43","market_id":2,"status":"open","type":"limit","is_ask":false,"initial_base_amount":"1","remaining_base_amount":"1","price":"150"}"#;
    for (rows, cover) in [
        (format!("{unmapped},{mapped}"), NOW),
        (mapped.to_owned(), NOW + 1_000.0),
    ] {
        let (mut e, s) = ready();
        let sent = NOW + 1_000.0;
        let read = census(&mut e, s, sent);
        let body = format!(r#"{{"code":200,"orders":[{rows}]}}"#);
        reply_at(&mut e, sent + 1.0, &read, 200, &body);
        assert!(e.store().order("43").is_some(), "the mapped row commits");
        assert!(e.store().order("9").is_none());
        assert_eq!(account_cover(&mut e, sent + 1.0).1, Some(cover), "{rows}");
    }
}

/// The follow-up a refused census owes waits at least 2 s from that census's
/// start, so a busy symbol cannot drive back-to-back census reads; it reads once.
#[test]
fn the_follow_up_to_a_refused_census_is_rate_bounded() {
    let (mut e, s) = ready();
    let sent = NOW + 1_000.0;
    let read = census(&mut e, s, sent);
    message(&mut e, s, sent + 1.0, STREAM_ORDER);
    requests_at(&mut e, sent + 1.0);
    e.handle(
        sent + 2.0,
        Event::HttpResponse {
            request: read.id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body: OPEN,
            },
        },
    );
    let out = safe_cex_engine::test_support::prepared_actions(&mut e, sent + 2.0);
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("ActiveOrders"))),
        "no census straight after the refused one"
    );
    let follow_ups: Vec<_> = out
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == sent + 2_000.0 => Some(*timer),
            _ => None,
        })
        .collect();
    assert_eq!(
        follow_ups.len(),
        1,
        "one follow-up 2 s after the census: {out:?}"
    );
    e.handle(
        sent + 2_000.0,
        Event::TimerFired {
            timer: follow_ups[0],
        },
    );
    let reads = requests_at(&mut e, sent + 2_000.0);
    assert_eq!(
        reads
            .iter()
            .filter(|r| r.url.contains("ActiveOrders"))
            .count(),
        1
    );
}
