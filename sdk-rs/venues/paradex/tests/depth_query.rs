//! REST queries must work without subscription/cached account state.
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Method, Notice, Outcome, Response,
    Venue,
};
use safe_cex_venue_paradex::{Options, Paradex};
fn engine(testnet: bool) -> Engine<Paradex> {
    safe_cex_engine::test_support::seeded_engine(Paradex::new(Options {
        testnet,
        ..Default::default()
    }))
}
fn ask(e: &mut Engine<Paradex>, command: u32, symbol: &str, limit: Option<u32>) -> HttpRequest {
    e.handle(
        1000.0,
        Event::Command(Command::FetchOrderBook {
            command,
            symbol: symbol.into(),
            limit,
        }),
    );
    let mut deadline = None;
    let actions: Vec<_> = safe_cex_engine::test_support::prepared_actions(e, 1000.0)
        .into_iter()
        .filter(|action| {
            if let Action::SetTimer { timer, at_ms } = action {
                assert_eq!(*at_ms, 21_000.0);
                assert!(
                    deadline.replace(*timer).is_none(),
                    "one original read deadline"
                );
                false
            } else {
                true
            }
        })
        .collect();
    assert!(
        deadline.is_some(),
        "managed read retains its original deadline"
    );
    assert_eq!(
        actions.len(),
        1,
        "one unsigned REST request, no startup/stream actions"
    );
    let Action::Http(request) = actions.into_iter().next().unwrap() else {
        panic!("REST query must dispatch")
    };
    request
}
fn reply(e: &mut Engine<Paradex>, request: &HttpRequest, status: u16, body: &str) -> Vec<Action> {
    reply_with_headers(e, request, status, "", body)
}
fn reply_with_headers(
    e: &mut Engine<Paradex>,
    request: &HttpRequest,
    status: u16,
    headers: &str,
    body: &str,
) -> Vec<Action> {
    e.handle(
        1001.0,
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
    assert_eq!(errors[0].outcome, Some(Outcome::NotSent));
    assert!(!actions.iter().any(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.code==safe_cex_engine::log::code::AUTH_REJECTED)));
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::Notice(Notice::CommandDone { .. } | Notice::AccountAuth { .. })
    )));
}

const BOOK: &str = r#"{"bids":[["1.20","2.5"],["1.2","0","ignored"],["1.9","4",{"ignored":true}]],"asks":[["2.00","3"]],"seq_no":"9007199254740993","market":"ignored","last_updated_at":null}"#;
#[test]
fn native_capability_admits_public_rest_book_queries() {
    assert!(
        Paradex::new(Options::default())
            .capabilities()
            .order_book_query
    );
}
#[test]
fn standalone_json_decoder_admits_real_book_command() {
    assert!(matches!(
        safe_cex_engine::command::decode_command(
            r#"{"op":"fetchOrderBook","command":7,"symbol":"BTC-USD-PERP","limit":5}"#
        )
        .unwrap(),
        Command::FetchOrderBook { command: 7, .. }
    ));
}
#[test]
fn live_testnet_unsigned_unseeded_queries_forward_depth_and_encode_symbol() {
    for (testnet, root) in [
        (false, "https://api.prod.paradex.trade/v1"),
        (true, "https://api.testnet.paradex.trade/v1"),
    ] {
        for (limit, depth) in [
            (None, 20),
            (Some(0), 0),
            (Some(5), 5),
            (Some(u32::MAX), u32::MAX),
        ] {
            let mut e = engine(testnet);
            let r = ask(&mut e, 7, "BTC /&-USD-PERP", limit);
            assert_eq!(r.method, Method::Get);
            assert!(r.headers.is_empty());
            assert_eq!(r.body, None);
            assert_eq!(r.route, None);
            assert_eq!(
                r.url,
                format!("{root}/orderbook/BTC%20%2F%26-USD-PERP?depth={depth}")
            );
            let result = done(&reply(&mut e, &r, 200, BOOK), 7);
            assert_eq!(result["bids"].as_array().unwrap().len(), 3);
            assert_eq!(
                result["bids"][0],
                serde_json::json!({"price":1.2,"amount":2.5,"total":3.0})
            );
            assert_eq!(
                result["bids"][1],
                serde_json::json!({"price":1.2,"amount":0.0,"total":0.0})
            );
            assert_eq!(
                result["bids"][2],
                serde_json::json!({"price":1.9,"amount":4.0,"total":7.6})
            );
            assert_eq!(
                result["asks"],
                serde_json::json!([{"price":2.0,"amount":3.0,"total":6.0}])
            );
            assert_eq!(result.as_object().unwrap().len(), 2);
            assert!(e.store().markets().is_empty());
            assert!(e.venue().book("BTC /&-USD-PERP").is_none());
        }
    }
}
#[test]
fn empty_and_one_sided_books_succeed_without_metadata() {
    for body in [
        r#"{"bids":[],"asks":[]}"#,
        r#"{"bids":[["1","2"]],"asks":[]}"#,
    ] {
        let mut e = engine(false);
        let r = ask(&mut e, 7, "BTC-USD-PERP", None);
        assert_eq!(
            done(&reply(&mut e, &r, 200, body), 7)["asks"],
            serde_json::json!([])
        );
    }
}
#[test]
fn malformed_objects_required_sides_financial_levels_and_quote_overflow_fail_atomically() {
    for body in [
        "null",
        "[]",
        r#"{"bids":[],"asks":null}"#,
        r#"{"bids":[]}"#,
        r#"{"bids":{},"asks":[]}"#,
        r#"{"bids":[["1"]],"asks":[]}"#,
        r#"{"bids":[["1","2"],["NaN","1"]],"asks":[]}"#,
        r#"{"bids":[["0","1"]],"asks":[]}"#,
        r#"{"bids":[["1","-1"]],"asks":[]}"#,
        r#"{"bids":[["1e308","1e308"]],"asks":[]}"#,
    ] {
        let mut e = engine(false);
        let r = ask(&mut e, 7, "BTC-USD-PERP", None);
        failed(&reply(&mut e, &r, 200, body), 7, ErrorKind::Internal);
    }
}
#[test]
fn public_native_and_http_refusals_are_typed_non_auth_with_structured_reasons() {
    for (status, body, kind) in [
        (
            200,
            r#"{"error":"INVALID_API_KEY","message":"invalid api key","bids":[],"asks":[]}"#,
            ErrorKind::VenueUnavailable,
        ),
        (
            200,
            r#"{"error":{"code":"NOT_AUTHENTICATED","message":"permission denied"},"bids":[],"asks":[]}"#,
            ErrorKind::VenueUnavailable,
        ),
        (401, "raw upstream dump", ErrorKind::VenueUnavailable),
        (403, "denied", ErrorKind::VenueUnavailable),
        (429, "bad JSON", ErrorKind::RateLimited),
        (408, "timeout", ErrorKind::Timeout),
        (503, "down", ErrorKind::VenueUnavailable),
    ] {
        let mut e = engine(false);
        let r = ask(&mut e, 7, "BTC-USD-PERP", None);
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
            assert!(error.venue_code.is_some());
            assert!(error.venue_message.is_some());
        }
    }
}
#[test]
fn public_rate_hints_are_bounded_without_retry_or_auth_side_effects() {
    for (hint, expected) in [
        ("120", Some(120000.0)),
        (
            "9999999",
            Some(safe_cex_engine::startup::MAX_RATE_LIMIT_PAUSE_MS),
        ),
        ("NaN", None),
        ("Infinity", None),
        ("-1", None),
    ] {
        let mut e = engine(false);
        let r = ask(&mut e, 7, "BTC-USD-PERP", None);
        let actions = reply_with_headers(
            &mut e,
            &r,
            429,
            &format!("Retry-After: {hint}\r\n"),
            "unreadable",
        );
        failed(&actions, 7, ErrorKind::RateLimited);
        let error = actions
            .iter()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
                _ => None,
            })
            .unwrap();
        assert_eq!(error.retry_after_ms, expected);
    }
}
#[test]
fn concurrent_owners_and_transport_failure_settle_independently_once() {
    let mut e = engine(false);
    let a = ask(&mut e, 7, "BTC-USD-PERP", None);
    let b = ask(&mut e, 8, "ETH-USD-PERP", Some(5));
    done(&reply(&mut e, &b, 200, BOOK), 8);
    done(&reply(&mut e, &a, 200, BOOK), 7);
    assert!(reply(&mut e, &a, 200, BOOK).is_empty());
    let a = ask(&mut e, 9, "BTC-USD-PERP", None);
    let b = ask(&mut e, 10, "ETH-USD-PERP", None);
    e.handle(
        1001.0,
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
    done(&reply(&mut e, &b, 200, BOOK), 10);
}
