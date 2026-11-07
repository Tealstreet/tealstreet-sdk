//! Unsigned candle authority must survive preparation and asynchronous delivery.
mod support;
use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{
    Claim, Error, HttpAction, Preparation, Prepared, RoutingIntent,
};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Notice, Outcome, Response, TimerId,
    VenueError,
};
use safe_cex_venue_phemex::PhemexPerpetuals;
use support::Options;
type TestEngine = Engine<PhemexPerpetuals>;
const ROWS: &str = r#"{"code":0,"data":{"rows":[[120,60,0,"105","115","95","110","3",0],[60,60,0,"100","110","90","105","2",0]]}}"#;

fn engine(options: Options) -> TestEngine {
    let mut e = support::engine(options);
    e.start(0.);
    e.drain_actions().for_each(drop);
    e
}
fn public() -> TestEngine {
    engine(Options {
        public_data_only: true,
        ..Options::default()
    })
}
fn command(e: &mut TestEngine, now: f64, owner: u32) {
    e.handle(
        now,
        Event::Command(Command::FetchCandles {
            command: owner,
            symbol: "BTC/USDT+P".into(),
            interval: Interval::M1,
            limit: Some(2),
            from_ms: Some(0.),
            to_ms: Some(180_000.),
        }),
    );
}
fn parked(e: &mut TestEngine, owner: u32) -> (Preparation, HttpRequest, TimerId) {
    command(e, 1_000., owner);
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::Sign { .. })),
        "public HTTP must await preparation"
    );
    let (preparation, request) = actions
        .iter()
        .find_map(|a| match a {
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    intent,
                    request,
                } => {
                    assert_eq!(*intent, RoutingIntent::Public);
                    Some((*preparation, request.clone()))
                }
                _ => None,
            },
            _ => None,
        })
        .expect("one unsigned read awaits route preparation");
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer {
                timer,
                at_ms: 21_000.,
            } => Some(*timer),
            _ => None,
        })
        .expect("the original twenty-second deadline covers preparation");
    assert_eq!(request.timeout_ms, Some(20_000));
    assert!(request.headers.is_empty() && request.body.is_none());
    (preparation, request, timer)
}
fn released(e: &mut TestEngine, now: f64, owner: u32) -> HttpRequest {
    command(e, now, owner);
    safe_cex_engine::test_support::prepared_actions(e, now)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .expect("one physical read")
}
fn reply(e: &mut TestEngine, now: f64, request: &HttpRequest) -> Vec<Action> {
    e.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: ROWS,
            },
        },
    );
    e.drain_actions().collect()
}
fn failure(actions: &[Action], owner: u32, kind: ErrorKind) {
    let errors: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command, error }) if *command == owner => {
                Some(error)
            }
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].kind, Some(kind));
    assert_eq!(errors[0].outcome, Some(Outcome::NotSent));
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Http(_)
            | Action::Sign { .. }
            | Action::Notice(Notice::CommandDone { .. } | Notice::AccountAuth { .. })
    )));
}

#[test]
fn unsigned_read_waits_for_binding_and_one_claim_then_preserves_native_rows() {
    let mut e = public();
    let (preparation, request, _) = parked(&mut e, 7);
    assert_eq!(
        request.url,
        "https://api.phemex.com/exchange/public/md/v2/kline/list?symbol=BTC%2FUSDT%2BP&resolution=60&from=60&to=180"
    );
    assert!(
        reply(&mut e, 1_001., &request).is_empty(),
        "unreleased replies have no authority"
    );
    e.http_prepared(
        1_002.,
        preparation,
        Prepared {
            id: request.id.0,
            expires_at: None,
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    let permit = actions
        .into_iter()
        .find_map(|a| match a {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Bind {
                    permit,
                    request: bound,
                    ..
                } => {
                    assert_eq!(bound.id, request.id);
                    assert_eq!(bound.url, request.url);
                    Some(permit)
                }
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    assert!(matches!(
        e.claim_http(1_003., permit),
        Ok(Claim::Released(_))
    ));
    assert!(e.claim_http(1_004., permit).is_err());
    e.drain_actions().for_each(drop);
    let actions = reply(&mut e, 1_005., &request);
    let results: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone { command: 7, result }) => Some(result),
            _ => None,
        })
        .collect();
    assert_eq!(results.len(), 1);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(results[0]).unwrap(),
        serde_json::json!([
            {"timestamp":60.0,"open":100.0,"high":110.0,"low":90.0,"close":105.0,"volume":2.0},
            {"timestamp":120.0,"open":105.0,"high":115.0,"low":95.0,"close":110.0,"volume":3.0}
        ])
    );
    assert!(reply(&mut e, 1_006., &request).is_empty());
    assert!(e.store().markets().is_empty() && e.store().orders().is_empty());
}

#[test]
fn parked_read_expiry_rejects_late_readiness_and_reply_once() {
    let mut e = public();
    let (preparation, request, timer) = parked(&mut e, 8);
    e.handle(21_000., Event::TimerFired { timer });
    failure(
        &e.drain_actions().collect::<Vec<_>>(),
        8,
        ErrorKind::Timeout,
    );
    assert!(!e.http_preparation_pending(preparation));
    e.http_prepared(
        21_001.,
        preparation,
        Prepared {
            id: request.id.0,
            expires_at: None,
        },
    );
    assert!(e.drain_actions().next().is_none());
    assert!(reply(&mut e, 21_002., &request).is_empty());
}

#[test]
fn released_expiry_cannot_complete_a_reused_command_id() {
    let mut e = public();
    let first = released(&mut e, 1_000., 9);
    failure(&reply(&mut e, 21_000., &first), 9, ErrorKind::Timeout);
    let second = released(&mut e, 21_001., 9);
    assert_ne!(first.id, second.id);
    assert!(reply(&mut e, 21_002., &first).is_empty());
    let actions = reply(&mut e, 21_003., &second);
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 9, .. })))
            .count(),
        1
    );
    assert!(reply(&mut e, 21_004., &second).is_empty());
}

#[test]
fn setup_refusal_preserves_typed_metadata_without_auth_evidence() {
    let mut e = public();
    let (preparation, request, _) = parked(&mut e, 10);
    e.http_preparation_failed(
        1_001.,
        preparation,
        Error::State,
        VenueError::new("public route refused")
            .kind(ErrorKind::AuthPermission)
            .venue_code("route"),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    failure(&actions, 10, ErrorKind::AuthPermission);
    assert!(actions.iter().any(|a| matches!(a,
        Action::Notice(Notice::CommandFailed { error, .. })
        if error.message == "public route refused" && error.venue_code.as_deref() == Some("route"))));
    assert!(reply(&mut e, 1_002., &request).is_empty());
}

#[test]
fn released_network_failure_is_typed_and_never_retries() {
    let mut e = public();
    let request = released(&mut e, 1_000., 11);
    e.handle(
        1_001.,
        Event::HttpFailed {
            request: request.id,
            error: "wire failed",
        },
    );
    failure(
        &e.drain_actions().collect::<Vec<_>>(),
        11,
        ErrorKind::Network,
    );
    assert!(reply(&mut e, 1_002., &request).is_empty());
}

#[test]
fn invalid_initial_time_rejects_before_transport_without_a_write_outcome() {
    let mut e = public();
    command(&mut e, f64::NAN, 12);
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
    );
    let errors: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 12, error }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].kind, Some(ErrorKind::InvalidInput));
    assert_eq!(errors[0].outcome, None);
    let packet = released(&mut e, 1_000., 12);
    assert!(
        reply(&mut e, 1_001., &packet)
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 12, .. })))
    );
}

#[test]
fn primary_secondary_and_dual_credentials_keep_the_existing_signed_branch() {
    for (primary, secondary, expected) in [
        (true, false, "primary"),
        (false, true, "secondary"),
        (true, true, "secondary"),
    ] {
        let mut e = engine(Options {
            public_data_only: true,
            key: primary.then(|| "primary".into()),
            secret: primary.then(|| "replay-secret".into()),
            read_credentials: secondary.then(|| safe_cex_engine::ReadCredentials {
                key: "secondary".into(),
                secret: "replay-read-secret".into(),
                passphrase: None,
            }),
            ..Options::default()
        });
        command(&mut e, 1_000., 13);
        let actions: Vec<_> = e.drain_actions().collect();
        let preparation = actions
            .iter()
            .find_map(|a| match a {
                Action::ManagedHttp(action) => match action.as_ref() {
                    HttpAction::Prepare {
                        preparation,
                        intent,
                        ..
                    } => {
                        assert_eq!(
                            *intent,
                            if secondary {
                                RoutingIntent::Secondary
                            } else {
                                RoutingIntent::Primary
                            }
                        );
                        Some(*preparation)
                    }
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        e.http_prepared(
            1_000.,
            preparation,
            Prepared {
                id: preparation.attempt().request.0,
                expires_at: None,
            },
        );
        let actions: Vec<_> = e.drain_actions().collect();
        let request = actions
            .into_iter()
            .find_map(|action| match action {
                Action::ManagedHttp(action) => match *action {
                    HttpAction::Bind {
                        permit, request, ..
                    } => {
                        assert!(matches!(
                            e.claim_http(1_000., permit),
                            Ok(Claim::Released(_))
                        ));
                        Some(request)
                    }
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        let actions = [Action::Http(request)];
        let request = actions
            .iter()
            .find_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .unwrap();
        assert!(
            request
                .headers
                .iter()
                .any(|(k, v)| *k == "X-PHEMEX-ACCESS-TOKEN" && v == expected)
        );
        assert!(
            request
                .headers
                .iter()
                .any(|(k, v)| *k == "X-PHEMEX-REQUEST-EXPIRY" && v == "61")
        );
        assert!(
            request
                .headers
                .iter()
                .any(|(k, v)| *k == "X-PHEMEX-REQUEST-SIGNATURE" && v.len() == 64)
        );
    }
}

#[test]
fn native_errors_malformed_rows_and_missing_lists_match_the_signed_read() {
    for (status, body) in [
        (
            200,
            r#"{"code":10002,"msg":"venue refused","data":{"rows":[]}}"#,
        ),
        (200, r#"{"code":10002,"msg":"venue refused"}"#),
        (
            200,
            r#"{"code":0,"data":{"rows":[[60,60,0,"100","110","90","105","2",0],["bad"]]}}"#,
        ),
        (200, r#"{"code":0,"data":{}}"#),
        (503, "<html>busy</html>"),
    ] {
        let mut errors = vec![];
        for signed in [false, true] {
            let mut e = engine(Options {
                public_data_only: true,
                key: signed.then(|| "primary".into()),
                secret: signed.then(|| "replay-secret".into()),
                ..Options::default()
            });
            let packet = released(&mut e, 1_000., 14);
            e.handle(
                1_001.,
                Event::HttpResponse {
                    request: packet.id,
                    response: Response {
                        status,
                        headers: "",
                        body,
                    },
                },
            );
            let actions: Vec<_> = e.drain_actions().collect();
            let failures: Vec<_> = actions
                .iter()
                .filter_map(|a| match a {
                    Action::Notice(Notice::CommandFailed { command: 14, error }) => Some(error),
                    _ => None,
                })
                .collect();
            assert_eq!(failures.len(), 1, "{signed} {status} {body}");
            errors.push(failures[0].message.clone());
            assert!(!actions.iter().any(|a| matches!(
                a,
                Action::Http(_)
                    | Action::Sign { .. }
                    | Action::Notice(Notice::CommandDone { .. } | Notice::AccountAuth { .. })
            )));
            assert!(!actions.iter().any(|a| matches!(a,
                Action::ManagedHttp(action) if matches!(action.as_ref(),
                    HttpAction::Prepare { .. } | HttpAction::Bind { .. }))));
            assert!(reply(&mut e, 1_002., &packet).is_empty());
        }
        assert_eq!(errors[0], errors[1], "{status} {body}");
    }
}

#[test]
fn expired_rate_limit_reply_cannot_pause_a_new_read() {
    let mut e = public();
    let packet = released(&mut e, 1_000., 15);
    e.handle(
        21_000.,
        Event::HttpResponse {
            request: packet.id,
            response: Response {
                status: 429,
                headers: "x-ratelimit-retry-after-contract: 60",
                body: r#"{"code":10002,"msg":"limited"}"#,
            },
        },
    );
    failure(
        &e.drain_actions().collect::<Vec<_>>(),
        15,
        ErrorKind::Timeout,
    );
    let next = released(&mut e, 21_001., 16);
    assert!(
        reply(&mut e, 21_002., &next)
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 16, .. })))
    );
}

/// Live REST rows carry the symbol as a tenth column; socket `kline_p` rows stop at nine.
#[test]
fn rows_with_and_without_the_symbol_column_decode_on_both_read_paths() {
    let body = r#"{"code":0,"msg":"OK","data":{"total":-1,"rows":[[1793399940,60,"76923.7","76915.6","76915.8","76915.6","76915.7","0.144","11075.8629","BTCUSDT"],[1793399880,60,"76900","76901","76902","76899","76900.5","0.2","15380"]]}}"#;
    for signed in [false, true] {
        let mut e = engine(Options {
            public_data_only: true,
            key: signed.then(|| "primary".into()),
            secret: signed.then(|| "replay-secret".into()),
            ..Options::default()
        });
        let packet = released(&mut e, 1_000., 15);
        e.handle(
            1_001.,
            Event::HttpResponse {
                request: packet.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
        let actions: Vec<_> = e.drain_actions().collect();
        let result = actions
            .iter()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandDone {
                    command: 15,
                    result,
                }) => Some(result),
                _ => None,
            })
            .unwrap_or_else(|| panic!("signed={signed}: no candles"));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(result).unwrap(),
            serde_json::json!([
                {"timestamp":1793399880.0,"open":76901.0,"high":76902.0,"low":76899.0,"close":76900.5,"volume":0.2},
                {"timestamp":1793399940.0,"open":76915.6,"high":76915.8,"low":76915.6,"close":76915.7,"volume":0.144}
            ]),
            "signed={signed}"
        );
    }
}
