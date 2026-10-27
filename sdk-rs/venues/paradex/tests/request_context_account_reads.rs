//! Private JWT reads (account snapshots and identity) on the managed funnel:
//! each is a Read bound to the account's JWT Session, so the engine attaches the
//! current Bearer at prepare and bounds the read by its 20 s window.
mod support;
use safe_cex_engine::{Action, Command, ErrorKind, Event, Method, Notice, Outcome, VenueError};
use safe_cex_venue_paradex::Options;
use support::*;

fn private() -> Harness {
    let mut h = Harness::empty(Options {
        key: Some(format!("0x{}1", "0".repeat(63))),
        paradex_address: Some("0x0123".into()),
        subkey_address: Some("0x456".into()),
        ..Options::default()
    });
    h.reply("/markets", MARKETS);
    h.reply("summary", TICKERS);
    h.reply(
        "/system/config",
        r#"{"starknet_chain_id":"PRIVATE_SN_PARACLEAR_MAINNET"}"#,
    );
    h.reply("/auth/", r#"{"jwt_token":"jwt"}"#);
    h.reply("/account/margin", r#"{"configs":[]}"#);
    h.reply("/positions", r#"{"results":[]}"#);
    h.reply(
        "/account",
        r#"{"total_collateral":"1000","free_collateral":"1000"}"#,
    );
    h.reply("/orders", r#"{"results":[]}"#);
    h.reply("/account/margin", r#"{"configs":[]}"#);
    assert!(h.pending.is_empty(), "{:?}", h.pending);
    h
}

/// A timer armed exactly `after` ms from now among the actions since `from`.
fn window(h: &Harness, from: usize, after: f64) -> Option<safe_cex_engine::TimerId> {
    h.actions[from..].iter().rev().find_map(|a| match a {
        Action::SetTimer { timer, at_ms } if *at_ms - h.now == after => Some(*timer),
        _ => None,
    })
}

fn failed(h: &Harness, id: u32) -> VenueError {
    h.actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command, error }) if *command == id => {
                Some(error.clone())
            }
            _ => None,
        })
        .expect("command failed")
}

fn header<'a>(request: &'a safe_cex_engine::HttpRequest, name: &str) -> Option<&'a str> {
    request
        .headers
        .iter()
        .find(|(header, _)| header.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

#[test]
fn a_snapshot_read_that_never_answers_ends_at_its_window_and_the_cycle_continues() {
    let mut h = private();
    let from = h.actions.len();
    h.timer(h.now + 10_000.);
    let positions = h.take("/positions");
    assert_eq!(positions.method, Method::Get);
    // Unanswered, the read must not hold the account cycle forever.
    let timer = window(&h, from, 20_000.);
    assert!(timer.is_some(), "the positions read has no deadline");
    h.now += 20_000.;
    h.e.handle(
        h.now,
        Event::TimerFired {
            timer: timer.unwrap(),
        },
    );
    h.collect();
    let balance = h.take("/account");
    assert_eq!(balance.method, Method::Get);
}

#[test]
fn an_identity_read_that_never_answers_fails_at_its_window() {
    let mut h = private();
    let from = h.actions.len();
    h.command(Command::FetchAccount { command: 71 });
    let request = h.take("/account");
    assert_eq!(header(&request, "Authorization"), Some("Bearer jwt"));
    let timer = window(&h, from, 20_000.);
    assert!(timer.is_some(), "the identity read has no deadline");
    h.now += 20_000.;
    h.e.handle(
        h.now,
        Event::TimerFired {
            timer: timer.unwrap(),
        },
    );
    h.collect();
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 71, .. }))),
        "the identity read never ended"
    );
    // A read has no financial outcome.
    assert_eq!(failed(&h, 71).outcome, None);
}

#[test]
fn an_auth_refusal_fails_a_waiting_identity_read_typed() {
    let mut h = private();
    // Past the JWT refresh point the read waits for re-authentication.
    h.now += 180_000.;
    h.command(Command::FetchAccount { command: 72 });
    assert!(!h.pending.iter().any(|r| r.url.ends_with("/account")));
    let auth = h.take("/auth/");
    h.respond(
        auth,
        401,
        r#"{"error":"INVALID_SIGNATURE","message":"unknown subkey"}"#,
    );
    let error = failed(&h, 72);
    assert_eq!(error.kind, Some(ErrorKind::AuthInvalid));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert!(h.pending.is_empty(), "{:?}", h.pending);
}

#[test]
fn private_reads_carry_the_current_bearer_before_their_own_headers() {
    let mut h = private();
    h.command(Command::FetchAccount { command: 73 });
    let request = h.take("/account");
    let names: Vec<&str> = request.headers.iter().map(|(name, _)| *name).collect();
    assert_eq!(names, ["Authorization", "Content-Type", "Accept"]);
    assert_eq!(header(&request, "Authorization"), Some("Bearer jwt"));
    h.respond(request, 200, r#"{"account_number":"42"}"#);
    assert_eq!(h.result(73), Ok(r#"{"userId":"42","affiliateId":null}"#));
}

#[test]
fn a_read_issued_with_a_stale_token_carries_the_refreshed_bearer() {
    let mut h = private();
    h.now += 180_000.;
    h.command(Command::FetchAccount { command: 74 });
    assert!(!h.pending.iter().any(|r| r.url.ends_with("/account")));
    h.reply("/auth/", r#"{"jwt_token":"jwt-2"}"#);
    let request = h.take("/account");
    assert_eq!(header(&request, "Authorization"), Some("Bearer jwt-2"));
    h.respond(request, 200, r#"{"account_number":"42"}"#);
    assert!(h.result(74).is_ok());
}

#[test]
fn a_jwt_that_is_not_header_safe_never_reaches_the_wire() {
    let mut h = private();
    h.now += 180_000.;
    h.command(Command::FetchAccount { command: 75 });
    h.reply("/auth/", r#"{"jwt_token":"jwt\r\nX-Injected: 1"}"#);
    // The Bearer is refused at prepare, so the read waits instead of sending.
    assert!(!h.pending.iter().any(|r| r.url.ends_with("/account")));
    assert!(
        !h.pending
            .iter()
            .any(|r| r.headers.iter().any(|(_, v)| v.contains("X-Injected")))
    );
}

#[test]
fn a_parked_snapshot_read_ends_unsent_on_an_auth_refusal_and_the_cycle_rests() {
    let mut h = private();
    let poll = h.timer_at(h.now + 10_000.);
    // The poll fires past the JWT refresh point: positions parks behind `/auth`.
    h.now += 190_000.;
    h.e.handle(h.now, Event::TimerFired { timer: poll });
    h.collect();
    assert!(!h.pending.iter().any(|r| r.url.ends_with("/positions")));
    let from = h.actions.len();
    let auth = h.take("/auth/");
    h.respond(
        auth,
        401,
        r#"{"error":"INVALID_SIGNATURE","message":"unknown subkey"}"#,
    );
    // Nothing private is sent after the refusal, and the chain does not hang:
    // the cycle ends and waits for its next poll.
    assert!(h.pending.is_empty(), "{:?}", h.pending);
    assert!(
        h.actions[from..]
            .iter()
            .any(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == h.now + 10_000.)),
        "the account cycle re-arms its poll"
    );
}
