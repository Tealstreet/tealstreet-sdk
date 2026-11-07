//! Standalone book reads keep their command authority through prepared public HTTP.
mod support;
use safe_cex_engine::request_context::{
    Claim, Error, HttpAction, Preparation, Prepared, RoutingIntent,
};
use safe_cex_engine::{
    Action, Command, ErrorKind, Event, HttpRequest, Notice, Outcome, Response, TimerId, VenueError,
};
use support::TestEngine;

const DEPTH: &str = r#"{"success":true,"code":0,"data":{"version":9007199254740993,"bids":[[59999,20],[60000,10]],"asks":[[60002,30],[60001,0]]}}"#;

fn engine() -> TestEngine {
    let (mut e, _) = support::started();
    e.drain_actions().for_each(drop);
    e
}

fn command(e: &mut TestEngine, now: f64, owner: u32) {
    e.handle(
        now,
        Event::Command(Command::FetchOrderBook {
            command: owner,
            symbol: "BTCUSDT".into(),
            limit: Some(1),
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
                body: DEPTH,
            },
        },
    );
    e.drain_actions().collect()
}

#[test]
fn book_waits_for_route_binding_and_final_claim_without_signing() {
    let mut e = engine();
    let (token, unsigned, _, _) = parked(&mut e, 1_000., 7);
    assert!(unsigned.headers.is_empty());
    assert!(unsigned.body.is_none());
    assert_eq!(
        unsigned.url,
        "https://api.mexc.com/api/v1/contract/depth/BTC_USDT?limit=1"
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
        serde_json::json!({"lastUpdateId":"9007199254740993",
            "bids":[{"price":60000.0,"amount":0.001,"total":0.001}],
            "asks":[{"price":60001.0,"amount":0.0,"total":0.0}]})
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
fn invalid_initial_time_is_terminal_before_transport_without_write_outcome() {
    let mut e = engine();
    command(&mut e, f64::NAN, 12);
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
    );
    let failures: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 12, error }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].kind, Some(ErrorKind::InvalidInput));
    assert_eq!(failures[0].outcome, None);
    command(&mut e, 1_000., 12);
    let packet = safe_cex_engine::test_support::prepared_actions(&mut e, 1_000.)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    assert!(
        reply(&mut e, 1_001., &packet)
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 12, .. })))
    );
}

#[test]
fn expired_public_refusal_cannot_pause_depth_or_account_reads() {
    let mut e = engine();
    command(&mut e, 1_000., 13);
    let packet = safe_cex_engine::test_support::prepared_actions(&mut e, 1_000.)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    e.handle(
        21_000.,
        Event::HttpResponse {
            request: packet.id,
            response: Response {
                status: 429,
                headers: "retry-after: 60",
                body: r#"{"success":false,"code":602,"message":"invalid api key"}"#,
            },
        },
    );
    failed(
        &e.drain_actions().collect::<Vec<_>>(),
        13,
        ErrorKind::Timeout,
    );
    command(&mut e, 21_001., 14);
    let packet = safe_cex_engine::test_support::prepared_actions(&mut e, 21_001.)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .expect("late refusal must not pause depth");
    assert!(
        reply(&mut e, 21_002., &packet)
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 14, .. })))
    );
    e.handle(
        21_003.,
        Event::Command(Command::FetchLeverage {
            command: 15,
            symbol: "BTCUSDT".into(),
        }),
    );
    assert!(e.drain_actions().any(|a| matches!(a,
        Action::Notice(Notice::CommandFailed { command: 15, error })
        if error.message == "MEXC leverage requires credentials")));
}
