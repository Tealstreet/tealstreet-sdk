mod support;
use safe_cex_engine::{Action, Command, Event, Notice, Venue};
use safe_cex_venue_mexc::{MexcFutures, Options};
use support::*;

#[test]
fn plan_reduce_only_comes_from_the_native_flag_or_closing_side() {
    let (mut e, socket, reads, _) = private_started();
    let plan = |id: u32, side: u32, reduce: bool| {
        format!(
            r#"{{"id":{id},"symbol":"BTC_USDT","side":{side},"orderType":1,"state":1,"vol":30,"triggerPrice":65000,"triggerType":1,"trend":1,"reduceOnly":{reduce},"updateTime":1700000000000}}"#
        )
    };
    let rows = (1..=4)
        .map(|side| plan(100 + side, side, false))
        .collect::<Vec<_>>()
        .join(",");
    for request in reads {
        let data = if request.url.contains("planorder") {
            format!("[{rows}]")
        } else {
            "[]".into()
        };
        response(
            &mut e,
            &request,
            &format!(r#"{{"success":true,"code":0,"data":{data}}}"#),
        );
    }
    for side in 1..=4 {
        let order = e.store().order(&(100 + side).to_string()).unwrap();
        assert_eq!(order.reduce_only, matches!(side, 2 | 4), "REST side {side}");
        assert_eq!(order.amount, 0.003);
        ws(
            &mut e,
            socket,
            &format!(
                r#"{{"channel":"push.personal.plan.order","data":{}}}"#,
                plan(200 + side, side, false)
            ),
        );
        let order = e.store().order(&(200 + side).to_string()).unwrap();
        assert_eq!(order.reduce_only, matches!(side, 2 | 4), "WS side {side}");
    }
    ws(
        &mut e,
        socket,
        &format!(
            r#"{{"channel":"push.personal.plan.order","data":{}}}"#,
            plan(301, 1, true)
        ),
    );
    assert!(e.store().order("301").unwrap().reduce_only);
    ws(
        &mut e,
        socket,
        r#"{"channel":"push.personal.stop.planorder","data":{"id":302,"symbol":"BTC_USDT","positionType":1,"side":1,"state":1,"vol":30,"stopLossPrice":59000}}"#,
    );
    assert!(e.store().order("302").unwrap().reduce_only);
}

fn book_response(e: &mut TestEngine, r: &safe_cex_engine::HttpRequest, body: &str) {
    book_response_at(e, r, body, 300.0);
}
fn book_response_at(e: &mut TestEngine, r: &safe_cex_engine::HttpRequest, body: &str, now: f64) {
    e.handle(
        now,
        Event::HttpResponse {
            request: r.id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
fn book_query(
    e: &mut TestEngine,
    command: u32,
    limit: Option<u32>,
) -> safe_cex_engine::HttpRequest {
    e.handle(
        300.0,
        Event::Command(Command::FetchOrderBook {
            command,
            symbol: "BTCUSDT".into(),
            limit,
        }),
    );
    let out = safe_cex_engine::test_support::prepared_actions(e, 300.0);
    assert!(!out.iter().any(|a| matches!(a, Action::WsSend { .. })));
    let deadlines: Vec<_> = out
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { at_ms, .. } => Some(*at_ms),
            _ => None,
        })
        .collect();
    assert_eq!(
        deadlines,
        [20_300.0],
        "one engine deadline and no retry timer"
    );
    out.iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r.clone()),
            _ => None,
        })
        .expect("snapshot query request")
}
fn completed(e: &mut TestEngine, command: u32) -> serde_json::Value {
    e.drain_actions()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone {
                command: id,
                result: json,
            }) if id == command => Some(serde_json::from_str(&json).unwrap()),
            _ => None,
        })
        .expect("successful query completion")
}

#[test]
fn independent_book_query_sorts_base_totals_and_preserves_large_version() {
    assert!(
        MexcFutures::new(Options::default())
            .capabilities()
            .order_book_query
    );
    let (mut e, _) = started();
    e.drain_actions().for_each(drop);
    let request = book_query(&mut e, 80, Some(2));
    assert!(
        request
            .url
            .ends_with("/api/v1/contract/depth/BTC_USDT?limit=2")
    );
    book_response(
        &mut e,
        &request,
        r#"{"success":true,"code":0,"data":{"version":9007199254740993,"bids":[[59999,"20"],[60000,10,1],[59998,40]],"asks":[[60002,30],[60001,0]]}}"#,
    );
    let book = completed(&mut e, 80);
    assert_eq!(book["lastUpdateId"], "9007199254740993");
    assert_eq!(
        book["bids"],
        serde_json::json!([
            {"price":60000.0,"amount":0.001,"total":0.001},
            {"price":59999.0,"amount":0.002,"total":0.003}
        ])
    );
    assert_eq!(
        book["asks"],
        serde_json::json!([
            {"price":60001.0,"amount":0.0,"total":0.0},
            {"price":60002.0,"amount":0.003,"total":0.003}
        ])
    );
    assert!(e.book("BTCUSDT").is_none());
}

fn failure(e: &mut TestEngine, command: u32) -> safe_cex_engine::VenueError {
    let failures = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: id, error }) if id == command => {
                Some(error)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(failures.len(), 1, "query must settle once");
    failures.into_iter().next().unwrap()
}
const EMPTY: &str =
    r#"{"success":true,"code":0,"data":{"version":"9007199254740993","bids":[],"asks":[]}}"#;

#[test]
fn book_queries_validate_input_and_capture_their_own_scale_and_limit() {
    let (mut e, _) = started();
    e.drain_actions().for_each(drop);
    for (symbol, limit) in [
        ("MISSING", None),
        ("BTCUSDT", Some(0)),
        ("BTCUSDT", Some(1001)),
    ] {
        e.handle(
            300.0,
            Event::Command(Command::FetchOrderBook {
                command: 81,
                symbol: symbol.into(),
                limit,
            }),
        );
        let out = e.drain_actions().collect::<Vec<_>>();
        assert!(!out.iter().any(|a| matches!(
            a,
            Action::Http(_)
                | Action::WsSend { .. }
                | Action::SetTimer { .. }
                | Action::ManagedHttp(_)
        )));
        assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command:81, error }) if error.kind == Some(safe_cex_engine::ErrorKind::InvalidInput))));
    }
    let first = book_query(&mut e, 82, None);
    assert!(first.url.ends_with("limit=100"));
    e.store_mut()
        .update_markets(|m| m.contract_size = Some(0.01));
    let second = book_query(&mut e, 83, Some(1000));
    assert!(second.url.ends_with("limit=1000"));
    e.store_mut().set_markets(vec![]);
    let body = r#"{"success":true,"code":0,"data":{"version":"9007199254740993","bids":[[60000,10]],"asks":[]}}"#;
    book_response(&mut e, &second, body);
    let result = completed(&mut e, 83);
    assert_eq!(result["bids"][0]["amount"].as_f64(), Some(0.1));
    assert_eq!(result["asks"], serde_json::json!([]));
    book_response(&mut e, &first, body);
    assert_eq!(
        completed(&mut e, 82)["bids"][0]["amount"].as_f64(),
        Some(0.001)
    );
}

#[test]
fn query_response_cannot_replace_a_newer_subscribed_book() {
    let (mut e, socket) = started();
    e.drain_actions().for_each(drop);
    e.handle(
        150.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    let subscription = requests(&mut e).pop().unwrap();
    book_response(
        &mut e,
        &subscription,
        r#"{"success":true,"code":0,"data":{"version":10,"bids":[[60000,10]],"asks":[[60001,40]]}}"#,
    );
    e.drain_actions().for_each(drop);
    let query = book_query(&mut e, 84, Some(1));
    e.handle(301.0, Event::WsMessage {
        socket,
        text: r#"{"channel":"push.depth","symbol":"BTC_USDT","data":{"version":11,"bids":[[60000,30]],"asks":[]}}"#,
    });
    e.drain_actions().for_each(drop);
    book_response_at(&mut e, &query, EMPTY, 302.0);
    assert_eq!(completed(&mut e, 84)["bids"], serde_json::json!([]));
    assert_eq!(e.book("BTCUSDT").unwrap().bids(), [(60000.0, 0.003)]);
    assert_eq!(e.book("BTCUSDT").unwrap().asks(), [(60001.0, 0.004)]);
}

#[test]
fn malformed_books_and_overflow_reject_the_whole_query() {
    let (mut e, _) = started();
    e.drain_actions().for_each(drop);
    for data in [
        r#"{"bids":[],"asks":[]}"#,
        r#"{"version":1,"bids":[]}"#,
        r#"{"version":"bad","bids":[],"asks":[]}"#,
        r#"{"version":1.5,"bids":[],"asks":[]}"#,
        r#"{"version":-1,"bids":[],"asks":[]}"#,
        r#"{"version":"18446744073709551616","bids":[],"asks":[]}"#,
        r#"{"version":1,"bids":[[0,1]],"asks":[]}"#,
        r#"{"version":1,"bids":[[1,-1]],"asks":[]}"#,
        r#"{"version":1,"bids":[[1,"NaN"]],"asks":[]}"#,
        r#"{"version":1,"bids":[[1,null]],"asks":[]}"#,
    ] {
        let query = book_query(&mut e, 85, Some(1));
        book_response(
            &mut e,
            &query,
            &format!(r#"{{"success":true,"code":0,"data":{data}}}"#),
        );
        assert_eq!(
            failure(&mut e, 85).kind,
            Some(safe_cex_engine::ErrorKind::Internal)
        );
    }
    for size in [f64::INFINITY, 0.0, -1.0] {
        e.store_mut()
            .update_markets(|m| m.contract_size = Some(size));
        e.handle(
            300.0,
            Event::Command(Command::FetchOrderBook {
                command: 86,
                symbol: "BTCUSDT".into(),
                limit: None,
            }),
        );
        let out = e.drain_actions().collect::<Vec<_>>();
        assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(
            out.iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 86, .. })))
        );
    }
    for (size, rows) in [(1e308, "[[1,1e308]]"), (1.0, "[[2,1e308],[1,1e308]]")] {
        e.store_mut()
            .update_markets(|m| m.contract_size = Some(size));
        let query = book_query(&mut e, 87, Some(1));
        book_response(
            &mut e,
            &query,
            &format!(
                r#"{{"success":true,"code":0,"data":{{"version":1,"bids":{rows},"asks":[]}}}}"#
            ),
        );
        assert_eq!(
            failure(&mut e, 87).kind,
            Some(safe_cex_engine::ErrorKind::Internal)
        );
    }
}

#[test]
fn query_failures_are_classified_and_rate_limits_pause_without_retrying() {
    use safe_cex_engine::{ErrorKind, Response};
    for (status, body, expected) in [
        (503, "{}", ErrorKind::VenueUnavailable),
        (429, "{}", ErrorKind::RateLimited),
        (
            200,
            r#"{"success":false,"code":510,"message":"Slow down"}"#,
            ErrorKind::RateLimited,
        ),
        (
            200,
            r#"{"success":false,"code":600,"message":"Bad query"}"#,
            ErrorKind::VenueUnavailable,
        ),
    ] {
        let (mut e, _) = started();
        e.drain_actions().for_each(drop);
        let query = book_query(&mut e, 88, None);
        e.handle(
            400.0,
            Event::HttpResponse {
                request: query.id,
                response: Response {
                    status,
                    headers: "retry-after: NaN",
                    body,
                },
            },
        );
        let error = failure(&mut e, 88);
        assert_eq!(error.kind, Some(expected));
        if expected == ErrorKind::RateLimited {
            assert_eq!(error.retry_after_ms, Some(10000.0));
            e.handle(
                500.0,
                Event::Command(Command::FetchOrderBook {
                    command: 89,
                    symbol: "BTCUSDT".into(),
                    limit: None,
                }),
            );
            let out = e.drain_actions().collect::<Vec<_>>();
            assert!(!out.iter().any(|a| matches!(
                a,
                Action::Http(_)
                    | Action::ManagedHttp(_)
                    | Action::SetTimer { .. }
                    | Action::WsSend { .. }
            )));
            assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command:89, error }) if error.kind == Some(ErrorKind::RateLimited))));
        }
    }
    let (mut e, _) = started();
    e.drain_actions().for_each(drop);
    let query = book_query(&mut e, 90, None);
    e.handle(
        400.0,
        Event::HttpFailed {
            request: query.id,
            error: "scripted connection failure",
        },
    );
    assert_eq!(failure(&mut e, 90).kind, Some(ErrorKind::Network));
}

#[test]
fn public_refusals_do_not_damage_private_auth_or_state() {
    use safe_cex_engine::{ErrorKind, Response};
    let (mut e, socket, reads, _) = private_started();
    for request in reads {
        response(&mut e, &request, r#"{"success":true,"code":0,"data":[]}"#);
    }
    ws(
        &mut e,
        socket,
        r#"{"channel":"push.personal.plan.order","data":{"id":901,"symbol":"BTC_USDT","side":1,"orderType":1,"state":1,"vol":30,"triggerPrice":65000,"triggerType":1,"trend":1}}"#,
    );
    assert!(e.store().order("901").is_some());
    let auth_failures = e.health().auth_failures;
    e.drain_actions().for_each(drop);
    for status in [401, 403, 200] {
        let query = book_query(&mut e, 91, None);
        assert!(
            !query
                .headers
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case("ApiKey"))
        );
        e.handle(400.0, Event::HttpResponse {
            request: query.id,
            response: Response {
                status, headers: "",
                body: r#"{"success":false,"code":602,"message":"invalid api key: permission denied"}"#,
            },
        });
        let out = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(out.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command:91, error }) if error.kind == Some(ErrorKind::VenueUnavailable) && error.message.contains("invalid api key"))).count(), 1);
        assert!(!out.iter().any(|a| matches!(
            a,
            Action::Notice(
                Notice::Deactivate | Notice::StartupFailed { .. } | Notice::AccountAuth { .. }
            )
        )));
        assert_eq!(e.health().auth_failures, auth_failures);
        assert!(e.store().order("901").is_some());
    }
}

#[test]
fn depth_query_resumes_after_the_reported_cooldown() {
    use safe_cex_engine::Response;
    let (mut e, _) = started();
    e.drain_actions().for_each(drop);
    let query = book_query(&mut e, 92, None);
    e.handle(
        400.0,
        Event::HttpResponse {
            request: query.id,
            response: Response {
                status: 429,
                headers: "Retry-After: 2",
                body: "{}",
            },
        },
    );
    assert_eq!(failure(&mut e, 92).retry_after_ms, Some(2000.0));
    e.handle(
        2400.0,
        Event::Command(Command::FetchOrderBook {
            command: 93,
            symbol: "BTCUSDT".into(),
            limit: None,
        }),
    );
    let resumed = safe_cex_engine::test_support::prepared_actions(&mut e, 2400.0)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .expect("new query after cooldown");
    book_response_at(&mut e, &resumed, EMPTY, 2400.0);
    assert_eq!(completed(&mut e, 93)["asks"], serde_json::json!([]));
}

#[test]
fn standalone_json_book_query_reaches_unsigned_rest_and_preserves_native_result() {
    let (mut e, _) = started();
    e.drain_actions().for_each(drop);
    let command = safe_cex_engine::command::decode_command(
        r#"{"op":"fetchOrderBook","command":91,"symbol":"BTCUSDT","limit":1}"#,
    )
    .expect("standalone MEXC advertises JSON REST-book queries");
    e.handle(100.0, Event::Command(command));
    let reads: Vec<_> = safe_cex_engine::test_support::prepared_actions(&mut e, 100.0)
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    assert_eq!(reads.len(), 1);
    let request = &reads[0];
    assert_eq!(request.method, safe_cex_engine::Method::Get);
    assert!(
        request
            .url
            .ends_with("/api/v1/contract/depth/BTC_USDT?limit=1")
    );
    assert!(request.headers.is_empty());
    assert_eq!(request.body, None);
    book_response(
        &mut e,
        request,
        r#"{"success":true,"code":0,"data":{"version":"9007199254740993","bids":[[60000,20,1]],"asks":[[60002,40,1]]}}"#,
    );
    let book = completed(&mut e, 91);
    assert_eq!(book["lastUpdateId"], "9007199254740993");
    assert_eq!(
        book["bids"],
        serde_json::json!([{"price":60000.0,"amount":0.002,"total":0.002}])
    );
    assert_eq!(
        book["asks"],
        serde_json::json!([{"price":60002.0,"amount":0.004,"total":0.004}])
    );
    assert!(e.venue().book("BTCUSDT").is_none());
}
