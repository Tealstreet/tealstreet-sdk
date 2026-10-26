//! Standalone public depth reads use the same prepared release as hosted reads.
mod support;
use safe_cex_engine::request_context::{
    Claim, Error, HttpAction, Preparation, Prepared, RoutingIntent,
};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Notice, Outcome, Response, TimerId,
    VenueError,
};
use safe_cex_venue_cryptocom::Cryptocom;
use support::fixture::Options;

const BOOK: &str =
    r#"{"code":0,"result":{"data":[{"bids":[["001.20","2.5"]],"asks":[["2.00","3"]]}]}}"#;

fn engine() -> Engine<Cryptocom> {
    support::fixture::engine(Options::default())
}

fn command(e: &mut Engine<Cryptocom>, now: f64, owner: u32) {
    e.handle(
        now,
        Event::Command(Command::FetchOrderBook {
            command: owner,
            symbol: "BTCUSD-PERP".into(),
            limit: None,
        }),
    );
}

fn parked(
    e: &mut Engine<Cryptocom>,
    now: f64,
    owner: u32,
) -> (Preparation, HttpRequest, TimerId, f64) {
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

fn reply(e: &mut Engine<Cryptocom>, now: f64, packet: &HttpRequest) -> Vec<Action> {
    e.handle(
        now,
        Event::HttpResponse {
            request: packet.id,
            response: Response {
                status: 200,
                headers: "",
                body: BOOK,
            },
        },
    );
    e.drain_actions().collect()
}

#[test]
fn depth_waits_for_route_binding_and_final_claim_without_startup_or_signing() {
    let mut e = engine();
    let (token, unsigned, _, _) = parked(&mut e, 1_000., 7);
    assert!(unsigned.headers.is_empty());
    assert!(unsigned.body.is_none());
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
    let book: serde_json::Value = serde_json::from_str(results[0]).unwrap();
    assert_eq!(book["bids"][0]["id"], "001.20");
    assert_eq!(book["bids"][0]["total"], 0.0);
    assert!(e.store().markets().is_empty());
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
