//! Candle reads keep their command authority through prepared public HTTP.
mod support;
use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{
    Claim, Error, HttpAction, Preparation, Prepared, RoutingIntent,
};
use safe_cex_engine::{
    Action, Command, ErrorKind, Event, HttpRequest, Notice, Outcome, Response, TimerId, VenueError,
};
use support::TestEngine;

const KLINES: &str = r#"{"success":true,"code":0,"data":{"time":[1700000000,1700000060000],"open":[1,2],"high":[3,4],"low":[0.5,1.5],"close":[2,3],"vol":[10,20]}}"#;

fn engine() -> TestEngine {
    let (mut e, _) = support::started();
    e.drain_actions().for_each(drop);
    e
}

fn command(e: &mut TestEngine, now: f64, owner: u32) {
    e.handle(
        now,
        Event::Command(Command::FetchCandles {
            command: owner,
            symbol: "BTCUSDT".into(),
            interval: Interval::M1,
            limit: Some(1),
            from_ms: Some(1700000000123.),
            to_ms: Some(1700000060999.),
        }),
    );
}

fn parked(e: &mut TestEngine, now: f64, owner: u32) -> (Preparation, HttpRequest, TimerId, f64) {
    command(e, now, owner);
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::Sign { .. })),
        "route preparation must precede public HTTP"
    );
    let mut preparations = actions.iter().filter_map(|a| match a {
        Action::ManagedHttp(a) => match a.as_ref() {
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
    });
    let (token, packet) = preparations.next().expect("one read awaits preparation");
    assert!(preparations.next().is_none());
    let (timer, deadline) = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
            _ => None,
        })
        .expect("original deadline is owned before route preparation");
    assert_eq!(deadline, now + 20_000.);
    assert_eq!(packet.timeout_ms, Some(20_000));
    (token, packet, timer, deadline)
}

fn failed(actions: &[Action], owner: u32, kind: ErrorKind) {
    let failures: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command, error }) => {
                assert_eq!(*command, owner);
                Some(error)
            }
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].kind, Some(kind));
    assert_eq!(failures[0].outcome, Some(Outcome::NotSent));
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::Notice(Notice::CommandDone { .. } | Notice::AccountAuth { .. })
    )));
}

fn reply(e: &mut TestEngine, now: f64, packet: &HttpRequest) -> Vec<Action> {
    e.handle(
        now,
        Event::HttpResponse {
            request: packet.id,
            response: Response {
                status: 200,
                headers: "",
                body: KLINES,
            },
        },
    );
    e.drain_actions().collect()
}

#[test]
fn candles_wait_for_route_binding_and_final_claim_without_signing() {
    let mut e = engine();
    let (token, unsigned, _, _) = parked(&mut e, 1_000., 7);
    assert!(unsigned.headers.is_empty());
    assert!(unsigned.body.is_none());
    assert_eq!(
        unsigned.url,
        "https://api.mexc.com/api/v1/contract/kline/BTC_USDT?interval=Min1&start=1700000000&end=1700000060"
    );
    e.http_prepared(
        1_500.,
        token,
        Prepared {
            id: unsigned.id.0,
            expires_at: None,
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    let (permit, bound) = actions
        .into_iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind {
                    permit, request, ..
                } => Some((permit, request)),
                _ => None,
            },
            _ => None,
        })
        .expect("prepared route must bind before claim");
    assert_eq!(bound.url, unsigned.url);
    assert_eq!(bound.id, unsigned.id);
    assert!(matches!(
        e.claim_http(1_501., permit),
        Ok(Claim::Released(_))
    ));
    assert!(e.claim_http(1_502., permit).is_err());
    assert!(e.drain_actions().next().is_none());
    let actions = reply(&mut e, 1_503., &bound);
    let results: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone { command: 7, result }) => Some(result),
            _ => None,
        })
        .collect();
    assert_eq!(results.len(), 1);
    let rows: serde_json::Value = serde_json::from_str(results[0]).unwrap();
    assert_eq!(
        rows,
        serde_json::json!([{"timestamp":1700000060.0,"open":2.0,"high":4.0,"low":1.5,"close":3.0,"volume":20.0}])
    );
    assert!(reply(&mut e, 1_504., &bound).is_empty());
}

#[test]
fn route_refusal_keeps_its_typed_cause_and_late_readiness_has_no_authority() {
    for kind in [ErrorKind::Unsupported, ErrorKind::AuthPermission] {
        let mut e = engine();
        let (token, packet, _, _) = parked(&mut e, 1_000., 8);
        e.http_preparation_failed(
            1_001.,
            token,
            Error::State,
            VenueError::new("prepared route refused")
                .kind(kind)
                .venue_code("route-refused"),
        );
        let actions: Vec<_> = e.drain_actions().collect();
        failed(&actions, 8, kind);
        assert!(actions.iter().any(|a| matches!(a,
            Action::Notice(Notice::CommandFailed { error, .. })
                if error.message == "prepared route refused"
                    && error.venue_code.as_deref() == Some("route-refused")
        )));
        e.http_prepared(
            1_002.,
            token,
            Prepared {
                id: packet.id.0,
                expires_at: None,
            },
        );
        assert!(e.drain_actions().next().is_none());
        assert!(reply(&mut e, 1_003., &packet).is_empty());
    }
}

#[test]
fn parked_route_expires_once_at_the_original_read_deadline() {
    let mut e = engine();
    let (token, packet, timer, deadline) = parked(&mut e, 1_000., 9);
    e.handle(deadline, Event::TimerFired { timer });
    failed(
        &e.drain_actions().collect::<Vec<_>>(),
        9,
        ErrorKind::Timeout,
    );
    assert!(!e.http_preparation_pending(token));
    e.http_prepared(
        deadline + 1.,
        token,
        Prepared {
            id: packet.id.0,
            expires_at: None,
        },
    );
    assert!(e.drain_actions().next().is_none());
    assert!(reply(&mut e, deadline + 2., &packet).is_empty());
}

#[test]
fn readiness_near_expiry_cannot_extend_the_bound_packets_claim_window() {
    let mut e = engine();
    let (token, packet, _, deadline) = parked(&mut e, 1_000., 10);
    e.http_prepared(
        deadline - 1.,
        token,
        Prepared {
            id: packet.id.0,
            expires_at: None,
        },
    );
    let permit = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind { permit, .. } => Some(permit),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    assert!(e.claim_http(deadline, permit).is_err());
    failed(
        &e.drain_actions().collect::<Vec<_>>(),
        10,
        ErrorKind::Timeout,
    );
    assert!(reply(&mut e, deadline + 1., &packet).is_empty());
}

#[test]
fn released_late_reply_cannot_complete_a_reused_numeric_command() {
    let mut e = engine();
    command(&mut e, 1_000., 11);
    let first = safe_cex_engine::test_support::prepared_actions(&mut e, 1_000.)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(packet) => Some(packet),
            _ => None,
        })
        .unwrap();
    failed(&reply(&mut e, 21_000., &first), 11, ErrorKind::Timeout);
    command(&mut e, 21_001., 11);
    let second = safe_cex_engine::test_support::prepared_actions(&mut e, 21_001.)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(packet) => Some(packet),
            _ => None,
        })
        .unwrap();
    assert_ne!(first.id, second.id);
    assert!(reply(&mut e, 21_002., &first).is_empty());
    let actions = reply(&mut e, 21_003., &second);
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 11, .. })))
            .count(),
        1
    );
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandFailed { .. } | Notice::AccountAuth { .. })
    )));
    assert!(reply(&mut e, 21_004., &second).is_empty());
}

#[test]
fn native_limits_and_malformed_columns_remain_atomic() {
    for (limit, expected) in [(None, 2), (Some(0), 0), (Some(1), 1)] {
        let mut e = engine();
        e.handle(
            1_000.,
            Event::Command(Command::FetchCandles {
                command: 12,
                symbol: "BTCUSDT".into(),
                interval: Interval::M1,
                limit,
                from_ms: None,
                to_ms: None,
            }),
        );
        let packet = safe_cex_engine::test_support::prepared_actions(&mut e, 1_000.)
            .into_iter()
            .find_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .unwrap();
        let actions = reply(&mut e, 1_001., &packet);
        let rows = actions
            .iter()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandDone {
                    command: 12,
                    result,
                }) => Some(result),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Vec<serde_json::Value>>(rows)
                .unwrap()
                .len(),
            expected
        );
    }
    let mut e = engine();
    let (_, packet, _, _) = parked(&mut e, 1_000., 13);
    // A response cannot bypass route preparation even when its body looks valid.
    assert!(reply(&mut e, 1_001., &packet).is_empty());
    let packet = {
        command(&mut e, 1_002., 14);
        safe_cex_engine::test_support::prepared_actions(&mut e, 1_002.)
            .into_iter()
            .find_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .unwrap()
    };
    e.handle(1_003., Event::HttpResponse { request: packet.id, response: Response {
        status: 200, headers: "", body: r#"{"success":true,"code":0,"data":{"time":[1,2],"open":[1],"high":[3,4],"low":[0,0],"close":[2,3],"vol":[10,20]}}"#,
    }});
    let actions: Vec<_> = e.drain_actions().collect();
    assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command:14, error }) if error.message == "Malformed MEXC klines")).count(), 1);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { .. })))
    );
}

#[test]
fn public_429_preserves_pause_and_late_429_has_no_pause_authority() {
    for late in [false, true] {
        let mut e = engine();
        command(&mut e, 1_000., 15);
        let packet = safe_cex_engine::test_support::prepared_actions(&mut e, 1_000.)
            .into_iter()
            .find_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .unwrap();
        let now = if late { 21_000. } else { 1_001. };
        e.handle(
            now,
            Event::HttpResponse {
                request: packet.id,
                response: Response {
                    status: 429,
                    headers: "retry-after: 60\r\n",
                    body: r#"{"success":false,"code":401,"message":"bad key"}"#,
                },
            },
        );
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::AccountAuth { .. })))
        );
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 15, .. })))
                .count(),
            1
        );
        e.handle(
            now + 1.,
            Event::Command(Command::FetchOrderBook {
                command: 16,
                symbol: "BTCUSDT".into(),
                limit: None,
            }),
        );
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, now + 1.);
        assert_eq!(actions.iter().any(|a| matches!(a, Action::Http(_))), late);
        assert_eq!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command:16, error }) if error.kind == Some(ErrorKind::RateLimited))), !late);
        e.handle(
            now + 2.,
            Event::Command(Command::FetchLeverage {
                command: 18,
                symbol: "BTCUSDT".into(),
            }),
        );
        let actions: Vec<_> = e.drain_actions().collect();
        let expected = if late {
            "MEXC leverage requires credentials"
        } else {
            "MEXC account reads are rate limited"
        };
        assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command:18, error }) if error.message == expected)).count(), 1);
        assert!(!actions.iter().any(|a| matches!(
            a,
            Action::Http(_) | Action::Notice(Notice::AccountAuth { .. })
        )));
    }
}

#[test]
fn immediate_native_validation_still_never_sends_http() {
    for (symbol, interval, message) in [
        ("BTCUSDT", Interval::S1, Interval::S1.unsupported("MEXC")),
        ("MISSING", Interval::M1, "Unknown MEXC market".to_owned()),
    ] {
        let mut e = engine();
        e.handle(
            1_000.,
            Event::Command(Command::FetchCandles {
                command: 17,
                symbol: symbol.into(),
                interval,
                limit: None,
                from_ms: None,
                to_ms: None,
            }),
        );
        let actions: Vec<_> = e.drain_actions().collect();
        assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command:17, error }) if error.message == message)).count(), 1);
        assert!(!actions.iter().any(|a| matches!(
            a,
            Action::Http(_) | Action::ManagedHttp(_) | Action::Sign { .. }
        )));
    }
}

#[test]
fn released_transport_failure_stays_typed_and_native_failures_do_not_retry() {
    for (status, body, message) in [
        (0, "", "transport lost"),
        (
            200,
            r#"{"success":false,"code":401,"message":"native refusal"}"#,
            "MEXC 401: native refusal",
        ),
        (503, "", "MEXC HTTP 503"),
    ] {
        let mut e = engine();
        command(&mut e, 1_000., 19);
        let packet = safe_cex_engine::test_support::prepared_actions(&mut e, 1_000.)
            .into_iter()
            .find_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .unwrap();
        if status == 0 {
            e.handle(
                1_001.,
                Event::HttpFailed {
                    request: packet.id,
                    error: message,
                },
            );
        } else {
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
        }
        let actions: Vec<_> = e.drain_actions().collect();
        if status == 0 {
            failed(&actions, 19, ErrorKind::Network);
        }
        assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command:19, error }) if error.message == message)).count(), 1);
        assert!(!actions.iter().any(|a| matches!(
            a,
            Action::Http(_)
                | Action::Sign { .. }
                | Action::SetTimer { .. }
                | Action::Notice(Notice::CommandDone { .. } | Notice::AccountAuth { .. })
        )));
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::ManagedHttp(action)
            if matches!(action.as_ref(), HttpAction::Prepare { .. } | HttpAction::Bind { .. })))
        );
        assert!(reply(&mut e, 1_002., &packet).is_empty());
    }
}

fn initial_refusal(actions: &[Action], owner: u32, kind: ErrorKind) {
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
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].kind, Some(kind));
    assert_eq!(
        errors[0].outcome, None,
        "an unissued read has no write outcome"
    );
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Http(_)
            | Action::ManagedHttp(_)
            | Action::Sign { .. }
            | Action::Notice(Notice::CommandDone { .. } | Notice::AccountAuth { .. })
    )));
}

#[test]
fn initial_invalid_clock_is_owned_by_engine_once_and_id_can_be_reused() {
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut e = engine();
        command(&mut e, now, 20);
        initial_refusal(
            &e.drain_actions().collect::<Vec<_>>(),
            20,
            ErrorKind::InvalidInput,
        );
        command(&mut e, 1_000., 20);
        let packet = safe_cex_engine::test_support::prepared_actions(&mut e, 1_000.)
            .into_iter()
            .find_map(|a| match a {
                Action::Http(packet) => Some(packet),
                _ => None,
            })
            .unwrap();
        let actions = reply(&mut e, 1_001., &packet);
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 20, .. })))
                .count(),
            1
        );
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
        );
    }
}

#[test]
fn initial_owner_capacity_refusal_does_not_send_or_steal_an_existing_read() {
    let mut e = engine();
    let (first_token, first, _, _) = parked(&mut e, 1_000., 100);
    for owner in 101..356 {
        let _ = parked(&mut e, 1_000., owner);
    }
    command(&mut e, 1_000., 356);
    initial_refusal(
        &e.drain_actions().collect::<Vec<_>>(),
        356,
        ErrorKind::Internal,
    );
    assert!(e.http_preparation_pending(first_token));
    e.http_prepared(
        1_001.,
        first_token,
        Prepared {
            id: first.id.0,
            expires_at: None,
        },
    );
    let permit = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Bind { permit, .. } => Some(permit),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    assert!(matches!(
        e.claim_http(1_002., permit),
        Ok(Claim::Released(_))
    ));
    let actions = reply(&mut e, 1_003., &first);
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 100, .. })))
            .count(),
        1
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
    );
}
