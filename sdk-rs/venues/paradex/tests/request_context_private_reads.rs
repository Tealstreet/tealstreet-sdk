//! Private history pages and FetchLeverage on the managed funnel: each is a
//! command Read bound to the account's JWT Session, so every page is bounded by
//! its 20 s window and an `/auth` refusal ends a waiting read typed.
mod support;
use safe_cex_engine::{Action, Command, ErrorKind, Event, Notice, Outcome, Query, VenueError};
use safe_cex_venue_paradex::Options;
use support::*;

fn private() -> Harness {
    let mut h = Harness::keyed(
        Options {
            paradex_address: Some("0x0123".into()),
            subkey_address: Some("0x456".into()),
            ..Options::default()
        },
        STARK_KEY,
    );
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

fn order_history(command: u32) -> Command {
    Command::Query {
        command,
        query: Query::OrderHistory {
            symbol: Some("BTC-USD-PERP".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    }
}

fn fetch_leverage(command: u32) -> Command {
    Command::FetchLeverage {
        command,
        symbol: "BTC-USD-PERP".into(),
    }
}

/// A timer armed exactly `after` ms from now among the actions since `from`.
fn window(h: &Harness, from: usize, after: f64) -> Option<safe_cex_engine::TimerId> {
    h.actions[from..].iter().rev().find_map(|a| match a {
        Action::SetTimer { timer, at_ms } if *at_ms - h.now == after => Some(*timer),
        _ => None,
    })
}

fn failed(h: &Harness, id: u32) -> Option<VenueError> {
    h.actions.iter().rev().find_map(|a| match a {
        Action::Notice(Notice::CommandFailed { command, error }) if *command == id => {
            Some(error.clone())
        }
        _ => None,
    })
}

fn header<'a>(request: &'a safe_cex_engine::HttpRequest, name: &str) -> Option<&'a str> {
    request
        .headers
        .iter()
        .find(|(header, _)| header.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

/// The read is sent with the current Bearer and, unanswered, ends at its own
/// 20 s window as a read failure with no financial outcome.
fn ends_at_its_window(command: Command, id: u32, path: &str) {
    let mut h = private();
    let from = h.actions.len();
    h.command(command);
    let request = h.take(path);
    assert_eq!(header(&request, "Authorization"), Some("Bearer jwt"));
    let timer = window(&h, from, 20_000.).expect("the private read has no deadline");
    h.now += 20_000.;
    h.e.handle(h.now, Event::TimerFired { timer });
    h.collect();
    let error = failed(&h, id).expect("the private read never ended");
    assert_eq!(error.outcome, None);
}

/// Past the JWT refresh point the read waits; a refused `/auth` ends it with the
/// account's typed credential refusal, unsent.
fn auth_refusal_fails_it_typed(command: Command, id: u32, path: &str) {
    let mut h = private();
    h.now += 180_000.;
    h.command(command);
    assert!(!h.pending.iter().any(|r| r.url.contains(path)));
    let auth = h.take("/auth/");
    h.respond(
        auth,
        401,
        r#"{"error":"INVALID_SIGNATURE","message":"unknown subkey"}"#,
    );
    let error = failed(&h, id).expect("the waiting read never ended");
    assert_eq!(error.kind, Some(ErrorKind::AuthInvalid));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert!(h.pending.is_empty(), "{:?}", h.pending);
}

#[test]
fn a_history_page_that_never_answers_fails_at_its_window() {
    ends_at_its_window(order_history(81), 81, "/orders-history");
}

/// The walk has one window from admission; a later page does not renew it.
#[test]
fn a_later_history_page_that_never_answers_ends_at_the_walk_window() {
    let mut h = private();
    let from = h.actions.len();
    h.command(order_history(82));
    let first = h.take("/orders-history");
    let deadline = h.now + 20_000.;
    window(&h, from, 20_000.).expect("the history walk has no deadline");
    h.now += 5_000.;
    let page = h.actions.len();
    h.respond(first, 200, r#"{"next":"page-2","results":[]}"#);
    let second = h.take("cursor=page-2");
    assert_eq!(header(&second, "Authorization"), Some("Bearer jwt"));
    let timer = window(&h, page, deadline - h.now).expect("the second page has no deadline");
    h.now = deadline;
    h.e.handle(h.now, Event::TimerFired { timer });
    h.collect();
    let error = failed(&h, 82).expect("the walk outlived its window");
    assert_eq!(error.outcome, None);
}

#[test]
fn an_auth_refusal_fails_a_waiting_history_query_typed() {
    auth_refusal_fails_it_typed(order_history(83), 83, "/orders-history");
}

#[test]
fn a_leverage_read_that_never_answers_fails_at_its_window() {
    ends_at_its_window(fetch_leverage(84), 84, "/account/margin");
}

/// PRESERVATION: the legacy trading refusal already carries the typed error.
#[test]
fn an_auth_refusal_fails_a_waiting_leverage_read_typed() {
    auth_refusal_fails_it_typed(fetch_leverage(85), 85, "/account/margin");
}
