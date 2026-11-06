//! While the private stream is logged in, positions, balance and orders come
//! from its topics: REST re-reads them as a 60 s safety reconcile, plus
//! positions every poll while one is open (the venue's position PnL feeds the
//! balance uPnL). A stream that is down falls back to the per-poll schedule.
mod support;
use support::{ACCOUNT, Harness, POSITION};

const FLAT: &str = r#"{"code":"00000","data":[]}"#;
const NO_ORDERS: &str = r#"{"code":"00000","data":{"entrustedList":[],"endId":""}}"#;
const PONG: &str = "pong";

/// Answers every outstanding read; positions with `positions`.
fn answer(h: &mut Harness, positions: &str) {
    while let Some(request) = h.requests.first().cloned() {
        let body = if request.url.contains("/all-position") {
            positions
        } else if request.url.contains("/account/accounts") {
            ACCOUNT
        } else if request.url.contains("planType=track_plan") {
            FLAT
        } else if request.url.contains("orders-pending")
            || request.url.contains("orders-plan-pending")
        {
            NO_ORDERS
        } else {
            FLAT
        };
        h.requests.remove(0);
        h.reply(request.id, 200, body);
    }
}

fn reads(h: &Harness, path: &str) -> bool {
    h.requests.iter().any(|request| request.url.contains(path))
}

/// One 2 s poll on a live stream (the venue answers pings).
fn poll(h: &mut Harness) {
    h.private_message(PONG);
    h.timer_after(2_000.0);
}

#[test]
fn an_open_position_is_read_every_poll_while_balance_and_orders_wait_for_the_reconcile() {
    let mut h = Harness::classic();
    answer(&mut h, POSITION);
    // The first live poll is the reconcile (nothing was read while live yet).
    poll(&mut h);
    assert!(reads(&h, "/account/accounts") && reads(&h, "orders-pending"));
    answer(&mut h, POSITION);
    for _ in 0..28 {
        poll(&mut h);
        assert!(reads(&h, "/all-position"), "{:?}", h.requests);
        assert!(!reads(&h, "/account/accounts"), "{:?}", h.requests);
        assert!(!reads(&h, "orders-pending"), "{:?}", h.requests);
        answer(&mut h, POSITION);
    }
    // 60 s after the first reconcile.
    for _ in 0..2 {
        poll(&mut h);
        if reads(&h, "/account/accounts") {
            break;
        }
        answer(&mut h, POSITION);
    }
    assert!(reads(&h, "/account/accounts") && reads(&h, "orders-pending"));
}

#[test]
fn a_flat_live_account_reads_nothing_between_reconciles() {
    let mut h = Harness::classic();
    answer(&mut h, FLAT);
    poll(&mut h);
    answer(&mut h, FLAT);
    for _ in 0..28 {
        poll(&mut h);
        assert!(h.requests.is_empty(), "{:?}", h.requests);
    }
}

#[test]
fn a_logged_out_stream_falls_back_to_every_poll() {
    let mut h = Harness::private_started();
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"40084","msg":"Classic Account mode","data":null}"#,
    );
    answer(&mut h, FLAT);
    // Never logged in: positions every poll, balance every second poll.
    h.timer_after(2_000.0);
    assert!(reads(&h, "/all-position"), "{:?}", h.requests);
    answer(&mut h, FLAT);
    h.timer_after(2_000.0);
    assert!(
        reads(&h, "/all-position") && reads(&h, "/account/accounts"),
        "{:?}",
        h.requests
    );
}
