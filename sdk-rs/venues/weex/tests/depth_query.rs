//! REST queries must work without subscription/cached account state.
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Method, Notice, Outcome, Response,
    Venue,
};
mod support;
use safe_cex_venue_weex::WeexFutures;
use support::setup::Options;
fn engine() -> Engine<WeexFutures> {
    support::setup::engine(Options::default())
}
fn ask(e: &mut Engine<WeexFutures>, command: u32, symbol: &str, limit: Option<u32>) -> HttpRequest {
    ask_at(e, 1000., command, symbol, limit)
}
fn ask_at(
    e: &mut Engine<WeexFutures>,
    now: f64,
    command: u32,
    symbol: &str,
    limit: Option<u32>,
) -> HttpRequest {
    e.handle(
        now,
        Event::Command(Command::FetchOrderBook {
            command,
            symbol: symbol.into(),
            limit,
        }),
    );
    let actions = safe_cex_engine::test_support::prepared_actions(e, now);
    assert_eq!(
        actions.len(),
        3,
        "only root timer, child timer and unsigned HTTP"
    );
    let mut deadlines: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::SetTimer { at_ms, .. } => Some(*at_ms),
            _ => None,
        })
        .collect();
    deadlines.sort_by(f64::total_cmp);
    assert_eq!(deadlines, vec![now + 20_000., now + 120_000.]);
    let mut requests: Vec<_> = actions
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 1, "one unsigned managed REST request");
    requests.remove(0)
}
fn reply(
    e: &mut Engine<WeexFutures>,
    request: &HttpRequest,
    status: u16,
    body: &str,
) -> Vec<Action> {
    reply_with_headers(e, request, status, "", body)
}
fn reply_with_headers(
    e: &mut Engine<WeexFutures>,
    request: &HttpRequest,
    status: u16,
    headers: &str,
    body: &str,
) -> Vec<Action> {
    reply_at(e, request, 1001., status, headers, body)
}
fn reply_at(
    e: &mut Engine<WeexFutures>,
    request: &HttpRequest,
    now: f64,
    status: u16,
    headers: &str,
    body: &str,
) -> Vec<Action> {
    e.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status,
                headers,
                body,
            },
        },
    );
    e.drain_actions().collect()
}
fn done(actions: &[Action], owner: u32) -> serde_json::Value {
    let results: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone { command, result }) => {
                assert_eq!(*command, owner);
                Some(result)
            }
            _ => None,
        })
        .collect();
    assert_eq!(results.len(), 1, "exactly one completion");
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
    );
    serde_json::from_str(results[0]).unwrap()
}
fn failed(actions: &[Action], owner: u32, kind: ErrorKind) {
    let errors: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command, error }) => {
                assert_eq!(*command, owner);
                Some(error)
            }
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 1, "exactly one owned failure");
    assert_eq!(errors[0].kind, Some(kind));
    assert_eq!(
        errors[0].outcome,
        if kind == ErrorKind::Network {
            None
        } else {
            Some(Outcome::NotSent)
        }
    );
    assert!(!actions.iter().any(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.code==safe_cex_engine::log::code::AUTH_REJECTED)));
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::Notice(Notice::CommandDone { .. } | Notice::AccountAuth { .. })
    )));
}

const BOOK: &str = r#"{"lastUpdateId":900719925474099312345678901234567890,"bids":[["1.20","2.5"],["1.2","0","ignored"],["1.9","4",{"ignored":true}]],"asks":[["2.00","3"]]}"#;
#[test]
fn native_capability_admits_book_queries() {
    assert!(
        support::setup::Options::default()
            .build()
            .0
            .capabilities()
            .order_book_query
    );
}
#[test]
fn standalone_json_decoder_admits_real_book_command() {
    assert!(matches!(
        safe_cex_engine::command::decode_command(
            r#"{"op":"fetchOrderBook","command":7,"symbol":"BTCUSDT","limit":1}"#
        )
        .unwrap(),
        Command::FetchOrderBook { command: 7, .. }
    ));
}
#[test]
fn unsigned_unseeded_gets_forward_raw_symbol_and_limit_with_exact_base_totals_and_version() {
    for (limit, expected) in [
        (None, 200),
        (Some(0), 0),
        (Some(1), 1),
        (Some(u32::MAX), u32::MAX),
    ] {
        let mut e = engine();
        let r = ask(&mut e, 7, "BTC /&USDT", limit);
        assert_eq!(r.method, Method::Get);
        assert!(r.headers.is_empty());
        assert_eq!(r.body, None);
        assert_eq!(r.route, None);
        assert_eq!(
            r.url,
            format!(
                "https://api-contract.weex.com/capi/v3/market/depth?symbol=BTC%20%2F%26USDT&limit={expected}"
            )
        );
        assert_eq!(
            done(&reply(&mut e, &r, 200, BOOK), 7),
            serde_json::json!({"bids":[{"price":1.2,"amount":2.5,"total":2.5},{"price":1.2,"amount":0.0,"total":2.5},{"price":1.9,"amount":4.0,"total":6.5}],"asks":[{"price":2.0,"amount":3.0,"total":3.0}],"lastUpdateId":"900719925474099312345678901234567890"})
        );
        assert!(e.store().markets().is_empty());
        assert!(e.venue().book("BTC /&USDT").is_none());
    }
}
#[test]
fn alias_precedence_empty_sides_and_exact_string_or_missing_version_match_ts() {
    for (body, id, bids) in [
        (r#"{}"#, "", 0),
        (r#"{"bids":null,"asks":null,"lastUpdateId":null}"#, "", 0),
        (
            r#"{"b":[["1","2"]],"a":[],"lastUpdateId":"00091"}"#,
            "00091",
            1,
        ),
        (
            r#"{"bids":[],"b":[["1","2"]],"asks":[],"lastUpdateId":91}"#,
            "91",
            0,
        ),
        (r#"{"bids":null,"b":[["1","2"]],"asks":[]}"#, "", 1),
    ] {
        let mut e = engine();
        let r = ask(&mut e, 7, "UNKNOWN", None);
        let result = done(&reply(&mut e, &r, 200, body), 7);
        assert_eq!(result["lastUpdateId"], id);
        assert_eq!(result["bids"].as_array().unwrap().len(), bids);
    }
}
#[test]
fn malformed_shapes_financial_fallbacks_and_total_overflow_fail_atomically() {
    for body in [
        "null",
        "[]",
        r#"{"bids":{}}"#,
        r#"{"bids":[["1"]]}"#,
        r#"{"bids":[["1","2"],["NaN","1"]]}"#,
        r#"{"bids":[["0","1"]]}"#,
        r#"{"bids":[["1","-1"]]}"#,
        r#"{"bids":[["1","Infinity"]]}"#,
        r#"{"bids":[["1x","2"]]}"#,
        r#"{"bids":[["1","1e308"],["1","1e308"]]}"#,
        r#"{"lastUpdateId":true,"bids":[]}"#,
    ] {
        let mut e = engine();
        let r = ask(&mut e, 7, "BTCUSDT", None);
        failed(&reply(&mut e, &r, 200, body), 7, ErrorKind::Internal);
    }
}
#[test]
fn native_success_codes_and_owned_public_refusals_do_not_invalidate_account_keys() {
    for code in ["0", "200", r#""0""#, r#""200""#] {
        let mut e = engine();
        let r = ask(&mut e, 7, "BTCUSDT", None);
        assert_eq!(
            done(
                &reply(
                    &mut e,
                    &r,
                    200,
                    &format!(r#"{{"code":{code},"bids":[],"asks":[]}}"#)
                ),
                7
            )["bids"],
            serde_json::json!([])
        );
    }
    for (status, body, kind) in [
        (
            200,
            r#"{"code":40006,"msg":"invalid api key","bids":[],"asks":[]}"#,
            ErrorKind::VenueUnavailable,
        ),
        (401, "auth required", ErrorKind::VenueUnavailable),
        (403, "denied", ErrorKind::VenueUnavailable),
        (429, "bad JSON", ErrorKind::RateLimited),
        (504, "timeout", ErrorKind::Timeout),
        (503, "down", ErrorKind::VenueUnavailable),
    ] {
        let mut e = engine();
        let r = ask(&mut e, 7, "BTCUSDT", None);
        let actions = reply(&mut e, &r, status, body);
        failed(&actions, 7, kind);
        if status == 200 {
            let error = actions
                .iter()
                .find_map(|a| match a {
                    Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
                    _ => None,
                })
                .unwrap();
            assert_eq!(error.venue_code.as_deref(), Some("40006"));
            assert_eq!(error.venue_message.as_deref(), Some("invalid api key"));
        }
    }
}
#[test]
fn shared_rate_pause_defers_new_query_without_retrying_the_failed_owner() {
    let mut e = engine();
    let r = ask(&mut e, 7, "BTCUSDT", None);
    let actions = reply_with_headers(
        &mut e,
        &r,
        429,
        "Retry-After: 3\r\nX-REMAINING-WEIGHT-1M: 0",
        "malformed",
    );
    failed(&actions, 7, ErrorKind::RateLimited);
    let error = actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
            _ => None,
        })
        .unwrap();
    assert_eq!(error.retry_after_ms, Some(3000.0));
    e.handle(
        1002.0,
        Event::Command(Command::FetchOrderBook {
            command: 8,
            symbol: "ETHUSDT".into(),
            limit: Some(1),
        }),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert_eq!(
        actions.len(),
        2,
        "only original root timer and native pause timer"
    );
    assert!(
        actions
            .iter()
            .all(|action| matches!(action, Action::SetTimer { .. }))
    );
    let (timer, at_ms) = actions
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == 4001. => Some((*timer, *at_ms)),
            _ => None,
        })
        .expect("native shared pause admission");
    e.handle(at_ms, Event::TimerFired { timer });
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, at_ms);
    assert_eq!(
        actions.len(),
        2,
        "only child timer and deferred unsigned HTTP"
    );
    assert_eq!(
        actions
            .iter()
            .filter(|action| matches!(action, Action::SetTimer { at_ms, .. } if *at_ms == 24_001.))
            .count(),
        1
    );
    let mut requests: Vec<_> = actions
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 1);
    let request = requests.remove(0);
    assert!(request.url.ends_with("symbol=ETHUSDT&limit=1"));
    done(&reply_at(&mut e, &request, 4002., 200, "", BOOK), 8);
    assert!(reply_at(&mut e, &r, 4003., 200, "", BOOK).is_empty());
}
#[test]
fn reversed_concurrent_queries_and_transport_failure_keep_owners_once() {
    let mut e = engine();
    let a = ask(&mut e, 7, "BTCUSDT", None);
    let b = ask(&mut e, 8, "ETHUSDT", None);
    done(&reply(&mut e, &b, 200, BOOK), 8);
    done(&reply(&mut e, &a, 200, BOOK), 7);
    assert!(reply(&mut e, &a, 200, BOOK).is_empty());
    let a = ask_at(&mut e, 1002., 9, "BTCUSDT", None);
    let b = ask_at(&mut e, 1002., 10, "ETHUSDT", None);
    e.handle(
        1003.0,
        Event::HttpFailed {
            request: a.id,
            error: "network failure",
        },
    );
    failed(
        &e.drain_actions().collect::<Vec<_>>(),
        9,
        ErrorKind::Network,
    );
    done(&reply_at(&mut e, &b, 1004., 200, "", BOOK), 10);
}
