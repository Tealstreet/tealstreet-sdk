//! REST queries must work without subscription/cached account state.
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Method, Notice, Outcome, Response,
    Venue,
};
use safe_cex_venue_grvt::{Grvt, Options};

#[path = "../../../engine/tests/support/public_managed_headers.rs"]
mod public_managed_headers;
fn engine(testnet: bool) -> Engine<Grvt> {
    let mut e = safe_cex_engine::test_support::seeded_engine(Grvt::new(Options {
        testnet,
        public_data_only: true,
        ..Default::default()
    }));
    e.start(0.0);
    let request = safe_cex_engine::test_support::prepared_actions(&mut e, 0.)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    e.handle(1.0,Event::HttpResponse {request:request.id,response:Response {status:200,headers:"",body:r#"{"r":[{"i":"BTC_USDT_Perp","ih":"0x030501","b":"BTC","q":"USDT","k":"PERPETUAL","ts":"0.1","ms":"0.001","mp":"1000","bd":9,"qd":6}]}"#}});
    e.drain_actions().for_each(drop);
    e
}
fn ask(e: &mut Engine<Grvt>, command: u32, symbol: &str, limit: Option<u32>) -> HttpRequest {
    e.handle(
        1000.0,
        Event::Command(Command::FetchOrderBook {
            command,
            symbol: symbol.into(),
            limit,
        }),
    );
    let actions = safe_cex_engine::test_support::prepared_actions(e, 1000.)
        .into_iter()
        .filter(|action| !matches!(action, Action::SetTimer { .. } | Action::ClearTimer { .. }))
        .collect::<Vec<_>>();
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
fn reply(e: &mut Engine<Grvt>, request: &HttpRequest, status: u16, body: &str) -> Vec<Action> {
    reply_with_headers(e, request, status, "", body)
}
fn reply_with_headers(
    e: &mut Engine<Grvt>,
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

const BOOK: &str = r#"{"r":{"bids":[{"price":"1.20","size":"2.5"},{"price":"1.2","size":"0"},{"price":"1.9","size":"4"}],"asks":[{"price":"2.00","size":"3"},{"price":"1.99","size":"2"}],"sequence":"9007199254740993"}}"#;
#[test]
fn native_capability_and_actual_standalone_json_decoder_admit_books() {
    assert!(
        Grvt::new(Options::default())
            .capabilities()
            .order_book_query
    );
}
#[test]
fn actual_standalone_json_decoder_admits_books() {
    assert!(matches!(
        safe_cex_engine::command::decode_command(
            r#"{"op":"fetchOrderBook","command":7,"symbol":"BTC_USDT_PERP","limit":1}"#
        )
        .unwrap(),
        Command::FetchOrderBook { command: 7, .. }
    ));
}
#[test]
fn live_testnet_public_posts_use_catalog_native_id_and_ignore_limit() {
    for (testnet, root) in [
        (false, "https://market-data.grvt.io"),
        (true, "https://market-data.testnet.grvt.io"),
    ] {
        for limit in [None, Some(0), Some(1), Some(u32::MAX)] {
            let mut e = engine(testnet);
            let r = ask(&mut e, 7, "BTC_USDT_PERP", limit);
            assert_eq!(r.method, Method::Post);
            assert_eq!(r.url, format!("{root}/lite/v1/book"));
            assert_eq!(r.route, None);
            assert_eq!(r.headers, vec![("Content-Type", "application/json".into())]);
            public_managed_headers::assert_native_public_intent_survives_managed_release(&r);
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(r.body.as_ref().unwrap()).unwrap(),
                serde_json::json!({"i":"BTC_USDT_Perp","d":50})
            );
            assert_eq!(
                done(&reply(&mut e, &r, 200, BOOK), 7),
                serde_json::json!({"bids":[{"price":1.2,"amount":2.5,"total":2.5},{"price":1.2,"amount":0.0,"total":2.5},{"price":1.9,"amount":4.0,"total":6.5}],"asks":[{"price":2.0,"amount":3.0,"total":3.0},{"price":1.99,"amount":2.0,"total":5.0}]})
            );
            assert!(e.venue().book("BTC_USDT_PERP").is_none());
        }
    }
}
#[test]
fn unknown_market_returns_owned_empty_without_http_or_auth() {
    let mut e = engine(false);
    for (owner, symbol) in [(7, "UNKNOWN"), (8, "BTC_USDT_Perp")] {
        e.handle(
            2.0,
            Event::Command(Command::FetchOrderBook {
                command: owner,
                symbol: symbol.into(),
                limit: None,
            }),
        );
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert_eq!(
            done(&actions, owner),
            serde_json::json!({"bids":[],"asks":[]})
        );
    }
}
#[test]
fn lite_book_fields_and_missing_null_result_or_sides_keep_empty_semantics() {
    for body in [
        r#"{}"#,
        r#"{"r":null}"#,
        r#"{"r":{}}"#,
        r#"{"r":{"bids":null,"asks":null}}"#,
        r#"{"r":{"b":[],"a":[]}}"#,
    ] {
        let mut e = engine(false);
        let r = ask(&mut e, 7, "BTC_USDT_PERP", None);
        assert_eq!(
            done(&reply(&mut e, &r, 200, body), 7),
            serde_json::json!({"bids":[],"asks":[]})
        );
    }
    let mut e = engine(false);
    let r = ask(&mut e, 7, "BTC_USDT_PERP", None);
    assert_eq!(
        done(
            &reply(
                &mut e,
                &r,
                200,
                r#"{"r":{"b":[{"p":"1","s":"2"},{"p":2,"s":3}],"a":[]}}"#
            ),
            7
        ),
        serde_json::json!({"bids":[{"price":1.0,"amount":2.0,"total":2.0},{"price":2.0,"amount":3.0,"total":5.0}],"asks":[]})
    );
}
#[test]
fn malformed_objects_and_nonfinite_or_bad_financial_rows_fail_atomically() {
    for body in [
        "null",
        "[]",
        r#"{"r":[]}"#,
        r#"{"r":{"bids":{}}}"#,
        r#"{"r":{"bids":[["1","2"]]}}"#,
        r#"{"r":{"bids":[{"price":"1","size":"2"},{"price":"NaN","size":"1"}]}}"#,
        r#"{"r":{"bids":[{"price":"0","size":"1"}]}}"#,
        r#"{"r":{"bids":[{"price":"1","size":"-1"}]}}"#,
        r#"{"r":{"bids":[{"price":"1","size":"Infinity"}]}}"#,
        r#"{"r":{"bids":[{"price":"1","size":"1e308"},{"price":"1","size":"1e308"}]}}"#,
    ] {
        let mut e = engine(false);
        let r = ask(&mut e, 7, "BTC_USDT_PERP", None);
        failed(&reply(&mut e, &r, 200, body), 7, ErrorKind::Internal);
    }
}
#[test]
fn native1000_refusal_never_refreshes_session_and_http_failure_precedes_malformed_body() {
    for (status, body, kind) in [
        (
            200,
            r#"{"c":1000,"m":"invalid api key","r":{"bids":[],"asks":[]}}"#,
            ErrorKind::VenueUnavailable,
        ),
        (
            200,
            r#"{"error":{"code":"1000","message":"invalid session"},"r":null}"#,
            ErrorKind::VenueUnavailable,
        ),
        (401, "auth required", ErrorKind::VenueUnavailable),
        (403, "denied", ErrorKind::VenueUnavailable),
        (429, "not json", ErrorKind::RateLimited),
        (504, "not json", ErrorKind::Timeout),
        (503, "not json", ErrorKind::VenueUnavailable),
    ] {
        let mut e = engine(false);
        let r = ask(&mut e, 7, "BTC_USDT_PERP", None);
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
            assert_eq!(error.venue_code.as_deref(), Some("1000"));
            assert!(error.venue_message.is_some());
        }
    }
}
#[test]
fn concurrent_queries_keep_owners_and_read_network_retries_are_bounded_without_auth() {
    let mut e = engine(false);
    let a = ask(&mut e, 7, "BTC_USDT_PERP", None);
    let b = ask(&mut e, 8, "BTC_USDT_PERP", None);
    done(&reply(&mut e, &b, 200, BOOK), 8);
    done(&reply(&mut e, &a, 200, BOOK), 7);
    assert!(reply(&mut e, &a, 200, BOOK).is_empty());
    let mut r = ask(&mut e, 9, "BTC_USDT_PERP", None);
    for attempt in 0..=5 {
        e.handle(
            1001.0 + f64::from(attempt),
            Event::HttpFailed {
                request: r.id,
                error: "network read failure",
            },
        );
        let actions =
            safe_cex_engine::test_support::prepared_actions(&mut e, 1001. + f64::from(attempt))
                .into_iter()
                .filter(|action| {
                    !matches!(action, Action::SetTimer { .. } | Action::ClearTimer { .. })
                })
                .collect::<Vec<_>>();
        if attempt == 5 {
            failed(&actions, 9, ErrorKind::Network);
        } else {
            assert_eq!(actions.len(), 1);
            let Action::Http(next) = actions.into_iter().next().unwrap() else {
                panic!("bounded public retry")
            };
            assert_eq!(next.url, r.url);
            assert_eq!(next.body, r.body);
            assert_eq!(next.headers, r.headers);
            r = next;
        }
    }
}

#[test]
fn full_wrapper_alias_and_native_rate_hints_preserve_protocol_evidence() {
    let mut e = engine(false);
    let r = ask(&mut e, 7, "BTC_USDT_PERP", None);
    assert_eq!(
        done(
            &reply(
                &mut e,
                &r,
                200,
                r#"{"result":{"bids":[{"price":"65038.01","size":"3456.78"}],"asks":[]}}"#
            ),
            7
        ),
        serde_json::json!({"bids":[{"price":65038.01,"amount":3456.78,"total":3456.78}],"asks":[]})
    );
    for (status, body) in [
        (429, "not JSON"),
        (200, r#"{"c":1006,"m":"rate limit","r":null}"#),
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
        ] {
            let r = ask(&mut e, 8, "BTC_USDT_PERP", None);
            let actions = reply_with_headers(
                &mut e,
                &r,
                status,
                &format!("Retry-After: {hint}\r\n"),
                body,
            );
            failed(&actions, 8, ErrorKind::RateLimited);
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
fn malformed_nested_refusal_metadata_cannot_erase_root_evidence_or_poison_a_peer() {
    for body in [
        r#"{"c":1006,"m":"slow down","e":false,"r":null}"#,
        r#"{"c":1006,"m":"slow down","e":{"c":{}},"r":null}"#,
        r#"{"c":1006,"m":"slow down","e":{"c":1006,"m":""},"r":null}"#,
    ] {
        let mut e = engine(false);
        let refused = ask(&mut e, 7, "BTC_USDT_PERP", None);
        let peer = ask(&mut e, 8, "BTC_USDT_PERP", None);
        let actions = reply_with_headers(&mut e, &refused, 200, "Retry-After: 120\r\n", body);
        failed(&actions, 7, ErrorKind::RateLimited);
        let error = actions
            .iter()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
                _ => None,
            })
            .unwrap();
        assert_eq!(error.venue_code.as_deref(), Some("1006"));
        assert_eq!(error.venue_message.as_deref(), Some("slow down"));
        assert_eq!(error.retry_after_ms, Some(120000.0));
        done(&reply(&mut e, &peer, 200, BOOK), 8);
    }
}

#[test]
fn explicitly_null_native_code_cannot_become_success() {
    for body in [
        r#"{"c":null,"r":{"b":[{"p":"1","s":"2"}],"a":[]}}"#,
        r#"{"code":null,"r":null}"#,
    ] {
        let mut e = engine(false);
        let r = ask(&mut e, 7, "BTC_USDT_PERP", None);
        failed(&reply(&mut e, &r, 200, body), 7, ErrorKind::Internal);
    }
}
