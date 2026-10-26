//! REST queries must work without subscription/cached account state.
mod support;
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Method, Notice, Outcome, Response,
    Venue,
};
use safe_cex_venue_cryptocom::Cryptocom;
use support::fixture::Options;
fn engine(testnet: bool) -> Engine<Cryptocom> {
    support::fixture::engine(Options {
        testnet,
        ..Default::default()
    })
}
fn ask(e: &mut Engine<Cryptocom>, command: u32, symbol: &str, limit: Option<u32>) -> HttpRequest {
    e.handle(
        1000.0,
        Event::Command(Command::FetchOrderBook {
            command,
            symbol: symbol.into(),
            limit,
        }),
    );
    let actions = safe_cex_engine::test_support::prepared_actions(e, 1000.0);
    let requests: Vec<_> = actions
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(packet) => Some(packet),
            Action::SetTimer { .. } | Action::ClearTimer { .. } => None,
            other => panic!("unexpected public depth action: {other:?}"),
        })
        .collect();
    assert_eq!(
        requests.len(),
        1,
        "one unsigned REST request, no startup/stream actions"
    );
    requests.into_iter().next().unwrap()
}
fn reply(e: &mut Engine<Cryptocom>, request: &HttpRequest, status: u16, body: &str) -> Vec<Action> {
    reply_with_headers(e, request, status, "", body)
}
fn reply_with_headers(
    e: &mut Engine<Cryptocom>,
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
    assert!(!actions.iter().any(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::AUTH_REJECTED)));
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::Notice(Notice::CommandDone { .. } | Notice::AccountAuth { .. })
    )));
}

const BOOK: &str = r#"{"code":0,"result":{"data":[{"bids":[["001.20","2.5","7"],["1.2","0","0"],["1.9","4",{"ignored":true}]],"asks":[["2.00","3"]]},{"bids":"later row ignored"}]}}"#;
#[test]
fn capability_and_standalone_json_admit_real_rest_queries() {
    assert!(
        support::fixture::venue(Options::default())
            .capabilities()
            .order_book_query
    );
}
#[test]
fn standalone_json_ingress_decodes_rest_book_command() {
    let command = safe_cex_engine::command::decode_command(
        r#"{"op":"fetchOrderBook","command":7,"symbol":"BTCUSD-PERP","limit":1}"#,
    )
    .expect("standalone Crypto.com JSON command");
    assert!(matches!(
        command,
        Command::FetchOrderBook { command: 7, .. }
    ));
}
#[test]
fn live_and_uat_unsigned_unseeded_gets_keep_fixed_depth_and_ts_financial_mapping() {
    for (testnet, root) in [
        (false, "https://api.crypto.com/exchange/v1"),
        (true, "https://uat-api.3ona.co/exchange/v1"),
    ] {
        for limit in [None, Some(0), Some(1), Some(u32::MAX)] {
            let mut e = engine(testnet);
            let r = ask(&mut e, 7, "BTC &/USD-PERP", limit);
            assert_eq!(r.method, Method::Get);
            assert_eq!(r.body, None);
            assert!(r.headers.is_empty());
            assert_eq!(r.route, None);
            assert_eq!(
                r.url,
                format!("{root}/public/get-book?instrument_name=BTC%20%26%2FUSD-PERP&depth=50")
            );
            let result = done(&reply(&mut e, &r, 200, BOOK), 7);
            assert_eq!(
                result,
                serde_json::json!({"bids":[{"id":"001.20","price":1.2,"amount":2.5,"total":0.0},{"id":"1.2","price":1.2,"amount":0.0,"total":0.0},{"id":"1.9","price":1.9,"amount":4.0,"total":0.0}],"asks":[{"id":"2.00","price":2.0,"amount":3.0,"total":0.0}]})
            );
            assert!(e.store().markets().is_empty());
            assert!(e.venue().book("BTC &/USD-PERP").is_none());
        }
    }
}
#[test]
fn ts_empty_data_missing_data_and_first_null_are_empty_but_explicit_null_result_is_not() {
    for body in [
        r#"{"code":0,"result":{"data":[]}}"#,
        r#"{"code":0,"result":{"data":null}}"#,
        r#"{"code":0,"result":{}}"#,
        r#"{"code":0}"#,
        r#"{"code":0,"data":[]}"#,
        r#"{"code":0,"result":{"data":[null,{"ignored":true}]}}"#,
    ] {
        let mut e = engine(false);
        let r = ask(&mut e, 7, "BTCUSD-PERP", None);
        assert_eq!(
            done(&reply(&mut e, &r, 200, body), 7),
            serde_json::json!({"bids":[],"asks":[]})
        );
    }
    let mut e = engine(false);
    let r = ask(&mut e, 7, "BTCUSD-PERP", None);
    failed(
        &reply(&mut e, &r, 200, r#"{"code":0,"result":null}"#),
        7,
        ErrorKind::Internal,
    );
}
#[test]
fn missing_rest_code_malformed_structures_and_bad_financial_levels_reject_atomically() {
    for body in [
        "null",
        r#"{"result":{"data":[]}}"#,
        r#"{"code":"0","result":{"data":[]}}"#,
        r#"{"code":0,"result":{"data":{}}}"#,
        r#"{"code":0,"result":{"data":[{"bids":null,"asks":[]}]}}"#,
        r#"{"code":0,"result":{"data":[{"bids":[]}]}}"#,
        r#"{"code":0,"result":{"data":[{"bids":[["1","2"],["NaN","1"]],"asks":[]}]}}"#,
        r#"{"code":0,"result":{"data":[{"bids":[["0","1"]],"asks":[]}]}}"#,
        r#"{"code":0,"result":{"data":[{"bids":[["1","-1"]],"asks":[]}]}}"#,
        r#"{"code":0,"result":{"data":[{"bids":[["1","Infinity"]],"asks":[]}]}}"#,
        r#"{"code":0,"result":{"data":[{"bids":[["1"]],"asks":[]}]}}"#,
    ] {
        let mut e = engine(false);
        let r = ask(&mut e, 7, "BTCUSD-PERP", None);
        failed(&reply(&mut e, &r, 200, body), 7, ErrorKind::Internal);
    }
}
#[test]
fn public_native_auth_looking_errors_and_http_failures_never_invalidate_account_keys() {
    for (status, body, kind) in [
        (
            200,
            r#"{"code":40101,"message":"invalid api key","result":{"data":[]}}"#,
            ErrorKind::VenueUnavailable,
        ),
        (401, "not authenticated", ErrorKind::VenueUnavailable),
        (403, "permission denied", ErrorKind::VenueUnavailable),
        (429, r#"{"code":true}"#, ErrorKind::RateLimited),
        (408, "timeout", ErrorKind::Timeout),
        (503, "unavailable", ErrorKind::VenueUnavailable),
        (
            200,
            r#"{"code":42901,"message":"too many requests"}"#,
            ErrorKind::RateLimited,
        ),
    ] {
        let mut e = engine(false);
        let r = ask(&mut e, 7, "BTCUSD-PERP", None);
        let actions = reply(&mut e, &r, status, body);
        failed(&actions, 7, kind);
        if status == 200 {
            assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed {error,..}) if error.venue_code.is_some() && error.venue_message.is_some())));
        }
    }
}
#[test]
fn concurrent_reverse_replies_and_transport_failure_keep_their_owners() {
    let mut e = engine(false);
    let a = ask(&mut e, 7, "BTCUSD-PERP", None);
    let b = ask(&mut e, 8, "ETHUSD-PERP", None);
    done(&reply(&mut e, &b, 200, BOOK), 8);
    done(&reply(&mut e, &a, 200, BOOK), 7);
    assert!(reply(&mut e, &a, 200, BOOK).is_empty());
    let a = ask(&mut e, 9, "BTCUSD-PERP", None);
    let b = ask(&mut e, 10, "ETHUSD-PERP", None);
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

#[test]
fn missing_result_uses_outer_data_but_present_result_takes_precedence() {
    let mut e = engine(false);
    let r = ask(&mut e, 7, "BTCUSD-PERP", None);
    let result = done(
        &reply(
            &mut e,
            &r,
            200,
            r#"{"code":0,"data":[{"bids":[["001.00","2"]],"asks":[]}]}"#,
        ),
        7,
    );
    assert_eq!(result["bids"][0]["id"], "001.00");
    let r = ask(&mut e, 8, "BTCUSD-PERP", None);
    assert_eq!(
        done(
            &reply(
                &mut e,
                &r,
                200,
                r#"{"code":0,"result":{"data":[]},"data":[{"bids":[["1","2"]],"asks":[]}]}"#
            ),
            8
        ),
        serde_json::json!({"bids":[],"asks":[]})
    );
}
#[test]
fn public_rate_hints_are_preserved_bounded_and_do_not_retry_or_invalidate_auth() {
    for (status, body) in [
        (429, "not JSON"),
        (200, r#"{"code":42901,"message":"slow down"}"#),
    ] {
        for (hint, expected) in [
            ("120", Some(120000.0)),
            (
                "9999999",
                Some(safe_cex_engine::startup::MAX_RATE_LIMIT_PAUSE_MS),
            ),
            ("NaN", None),
            ("Infinity", None),
            ("-1", None),
            ("bad", None),
        ] {
            let mut e = engine(false);
            let r = ask(&mut e, 7, "BTCUSD-PERP", None);
            let actions = reply_with_headers(
                &mut e,
                &r,
                status,
                &format!("Retry-After: {hint}\r\n"),
                body,
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
}

#[test]
fn arrays_cannot_impersonate_named_rest_objects() {
    for body in [
        r#"[0]"#,
        r#"[0,null,{"data":[{"bids":[["1","2"]],"asks":[]}]},null]"#,
        r#"{"code":0,"result":[[{"bids":[["1","2"]],"asks":[]}]]}"#,
        r#"{"code":0,"result":{"data":[[[["1","2"]],[]]]}}"#,
    ] {
        let mut e = engine(false);
        let request = ask(&mut e, 7, "BTCUSD-PERP", None);
        failed(&reply(&mut e, &request, 200, body), 7, ErrorKind::Internal);
    }
}
