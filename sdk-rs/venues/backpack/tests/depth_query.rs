//! REST queries must work without subscription/cached account state.
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Method, Notice, Outcome, Response,
    Venue,
};
use safe_cex_venue_backpack::{Backpack, Options};
fn engine() -> Engine<Backpack> {
    safe_cex_engine::test_support::seeded_engine(Backpack::new(Options::default()))
}
fn ask(e: &mut Engine<Backpack>, command: u32, symbol: &str, limit: Option<u32>) -> HttpRequest {
    e.handle(
        1000.0,
        Event::Command(Command::FetchOrderBook {
            command,
            symbol: symbol.into(),
            limit,
        }),
    );
    let actions: Vec<_> = safe_cex_engine::test_support::prepared_actions(e, 1000.0)
        .into_iter()
        .filter(|action| !matches!(action, Action::SetTimer { .. } | Action::ClearTimer { .. }))
        .collect();
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
fn reply(e: &mut Engine<Backpack>, request: &HttpRequest, status: u16, body: &str) -> Vec<Action> {
    e.handle(
        1001.0,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status,
                headers: "",
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
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::Notice(Notice::CommandDone { .. } | Notice::AccountAuth { .. })
    )));
}
const BOOK: &str = r#"{"bids":[["001.2300","2.5"],["1.23000","0"],["1.9","4"]],"asks":[["2.000","3"]],"lastUpdateId":9007199254740993}"#;
#[test]
fn native_capability_advertises_rest_queries() {
    assert!(
        Backpack::new(Options::default())
            .capabilities()
            .order_book_query
    );
}
#[test]
fn unsigned_unseeded_query_keeps_ts_ids_order_zero_sizes_totals_and_ignored_limits() {
    for limit in [None, Some(0), Some(1), Some(u32::MAX)] {
        let mut e = engine();
        let request = ask(&mut e, 7, "SOL &/USDC_PERP", limit);
        assert_eq!(request.method, Method::Get);
        assert_eq!(request.body, None);
        assert_eq!(request.route, None);
        assert_eq!(
            request.url,
            "https://api.backpack.exchange/api/v1/depth?symbol=SOL%20%26%2FUSDC_PERP"
        );
        assert_eq!(request.headers, vec![("X-Broker-Id", "1200".into())]);
        let actions = reply(&mut e, &request, 200, BOOK);
        assert_eq!(
            done(&actions, 7),
            serde_json::json!({"bids":[{"id":"001.2300","price":1.23,"amount":2.5,"total":0.0},{"id":"1.23000","price":1.23,"amount":0.0,"total":0.0},{"id":"1.9","price":1.9,"amount":4.0,"total":0.0}],"asks":[{"id":"2.000","price":2.0,"amount":3.0,"total":0.0}]})
        );
        assert!(e.venue().book("SOL &/USDC_PERP").is_none());
        assert!(e.store().markets().is_empty());
    }
}
#[test]
fn empty_sides_and_reverse_concurrent_responses_preserve_each_owner() {
    let mut e = engine();
    let a = ask(&mut e, 7, "SOL_USDC_PERP", None);
    let b = ask(&mut e, 8, "BTC_USDC_PERP", None);
    assert_eq!(
        done(&reply(&mut e, &b, 200, r#"{"bids":[],"asks":[]}"#), 8),
        serde_json::json!({"bids":[],"asks":[]})
    );
    let result = done(
        &reply(&mut e, &a, 200, r#"{"bids":[],"asks":[["2.00","1"]]}"#),
        7,
    );
    assert_eq!(result["asks"][0]["id"], "2.00");
    assert_eq!(result["bids"], serde_json::json!([]));
    assert!(
        reply(&mut e, &a, 200, BOOK).is_empty(),
        "duplicate callback is inert"
    );
}
#[test]
fn malformed_shapes_and_financial_rows_fail_whole_query_without_partial_book() {
    for body in [
        "null",
        "{}",
        r#"{"bids":null,"asks":[]}"#,
        r#"{"bids":[]}"#,
        r#"{"bids":[["1","2","3"]],"asks":[]}"#,
        r#"{"bids":[["1"]],"asks":[]}"#,
        r#"{"bids":[[1,"2"]],"asks":[]}"#,
        r#"{"bids":[],"asks":[["2",null]]}"#,
        r#"{"bids":[["1","2"],["NaN","1"]],"asks":[]}"#,
        r#"{"bids":[["Inf","1"]],"asks":[]}"#,
        r#"{"bids":[["0","1"]],"asks":[]}"#,
        r#"{"bids":[["1","-1"]],"asks":[]}"#,
        r#"{"bids":[["1","Infinity"]],"asks":[]}"#,
        r#"{"bids":[["","1"]],"asks":[]}"#,
    ] {
        let mut e = engine();
        let r = ask(&mut e, 7, "SOL_USDC_PERP", None);
        failed(&reply(&mut e, &r, 200, body), 7, ErrorKind::Internal);
        assert!(e.venue().book("SOL_USDC_PERP").is_none());
    }
}
#[test]
fn public_http_native_and_transport_failures_stay_owned_without_auth_or_retries() {
    for (status, body, kind) in [
        (
            401,
            r#"{"code":"INVALID_CLIENT_REQUEST","message":"invalid api key"}"#,
            ErrorKind::VenueUnavailable,
        ),
        (403, "permission denied", ErrorKind::VenueUnavailable),
        (429, "rate limit", ErrorKind::RateLimited),
        (408, "timeout", ErrorKind::Timeout),
        (503, "unavailable", ErrorKind::VenueUnavailable),
        (
            200,
            r#"{"error":{"msg":"venue refused"},"bids":[],"asks":[]}"#,
            ErrorKind::VenueUnavailable,
        ),
        (
            200,
            r#"{"code":123,"msg":"venue refused","bids":[],"asks":[]}"#,
            ErrorKind::VenueUnavailable,
        ),
    ] {
        let mut e = engine();
        let r = ask(&mut e, 7, "SOL_USDC_PERP", None);
        failed(&reply(&mut e, &r, status, body), 7, kind);
    }
    let mut e = engine();
    let a = ask(&mut e, 7, "SOL_USDC_PERP", None);
    let b = ask(&mut e, 8, "BTC_USDC_PERP", None);
    e.handle(
        1001.0,
        Event::HttpFailed {
            request: a.id,
            error: "network unavailable",
        },
    );
    failed(
        &e.drain_actions().collect::<Vec<_>>(),
        7,
        ErrorKind::Network,
    );
    done(&reply(&mut e, &b, 200, BOOK), 8);
}

#[test]
fn single_venue_json_ingress_decodes_the_advertised_book_command() {
    let command: Command = safe_cex_engine::command::decode_command(
        r#"{"op":"fetchOrderBook","command":7,"symbol":"SOL_USDC_PERP","limit":1}"#,
    )
    .expect("standalone advertised JSON command");
    assert!(matches!(
        command,
        Command::FetchOrderBook {
            command: 7,
            limit: Some(1),
            ..
        }
    ));
}

#[test]
fn native_error_message_and_malformed_peer_metadata_never_become_empty_success() {
    for rows in [r#""bids":[],"asks":[]"#, r#""bids":[["1","2"]],"asks":[]"#] {
        let mut e = engine();
        let bad = ask(&mut e, 7, "SOL_USDC_PERP", None);
        let good = ask(&mut e, 8, "BTC_USDC_PERP", None);
        let body = format!(r#"{{"code":"INVALID_SYMBOL","message":"venue refused",{rows}}}"#);
        let actions = reply(&mut e, &bad, 200, &body);
        failed(&actions, 7, ErrorKind::VenueUnavailable);
        assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed {error,..}) if error.venue_code.as_deref()==Some("INVALID_SYMBOL") && error.venue_message.as_deref()==Some("venue refused"))));
        done(&reply(&mut e, &good, 200, BOOK), 8);
    }
    for peer in [
        r#""code":{}"#,
        r#""code":true"#,
        r#""code":1.5"#,
        r#""msg":1"#,
    ] {
        let mut e = engine();
        let bad = ask(&mut e, 7, "SOL_USDC_PERP", None);
        let good = ask(&mut e, 8, "BTC_USDC_PERP", None);
        let body = format!(r#"{{"error":{{"msg":"venue refused"}},{peer},"bids":[],"asks":[]}}"#);
        failed(&reply(&mut e, &bad, 200, &body), 7, ErrorKind::Internal);
        done(&reply(&mut e, &good, 200, BOOK), 8);
    }
}
#[test]
fn rate_status_survives_malformed_body_and_preserves_native_cooldown_hint() {
    let mut e = engine();
    let r = ask(&mut e, 7, "SOL_USDC_PERP", None);
    e.handle(
        1001.0,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 429,
                headers: "Retry-After: 120\r\n",
                body: r#"{"code":true}"#,
            },
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    failed(&actions, 7, ErrorKind::RateLimited);
    assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed {error,..}) if error.retry_after_ms==Some(120_000.0))));
}
