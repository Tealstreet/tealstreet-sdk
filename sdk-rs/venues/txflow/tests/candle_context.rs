//! Original-source controls for the public command candle Read boundary.
mod support;

use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{Claim, Error, HttpAction, Prepared};
use safe_cex_engine::{
    Action, Command, CommandId, Engine, ErrorKind, Event, HttpRequest, Method, Notice,
    RequestRoute, Response, VenueError,
};
use safe_cex_venue_txflow::{Options, TxFlow};

type Driver = Engine<TxFlow>;
const NOW: f64 = 2_000.;
const ROWS: &str = r#"{"success":true,"code":0,"data":[{"t":60000,"s":"1","i":"1m","o":"2","h":"3","l":"1","c":"2.5","v":"4"}]}"#;

fn reply(e: &mut Driver, now: f64, request: &HttpRequest, status: u16, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
}

fn ready() -> Driver {
    let mut e = safe_cex_engine::test_support::seeded_engine(TxFlow::new(Options {
        testnet: true,
        public_data_only: true,
        ..Default::default()
    }));
    e.start(1_000.);
    let requests = safe_cex_engine::test_support::prepared_actions(&mut e, 1_000.);
    let metadata = requests
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .expect("native metadata seed");
    reply(&mut e, 1_001., &metadata, 200, support::META);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, 1_001.);
    let ticker = actions
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .expect("native ticker seed");
    reply(&mut e, 1_002., &ticker, 200, support::TICKER);
    let _ = safe_cex_engine::test_support::prepared_actions(&mut e, 1_002.);
    assert_eq!(e.store().markets()[0].symbol, "BTC-USDC");
    e
}

fn command(id: CommandId) -> Command {
    Command::FetchCandles {
        command: id,
        symbol: "BTC-USDC".into(),
        interval: Interval::M1,
        limit: Some(2),
        from_ms: Some(0.),
        to_ms: Some(120_000.),
    }
}

fn issued(e: &mut Driver, now: f64, id: CommandId) -> Vec<Action> {
    e.handle(now, Event::Command(command(id)));
    e.drain_actions().collect()
}

fn released(e: &mut Driver, id: CommandId, wait: Option<u32>) -> HttpRequest {
    e.handle_command(NOW, command(id), wait).unwrap();
    let actions = safe_cex_engine::test_support::prepared_actions(e, NOW);
    let mut requests = actions.into_iter().filter_map(|action| match action {
        Action::Http(request) => Some(request),
        _ => None,
    });
    let request = requests.next().expect("one released candle request");
    assert!(requests.next().is_none(), "no retry or sibling request");
    request
}

fn failure(actions: &[Action], command: CommandId) -> &VenueError {
    let mut errors = actions.iter().filter_map(|action| match action {
        Action::Notice(Notice::CommandFailed { command: id, error }) if *id == command => {
            Some(error)
        }
        _ => None,
    });
    let error = errors.next().expect("one contextual failure");
    assert!(errors.next().is_none(), "exactly one terminal");
    assert!(!actions.iter().any(|action| matches!(action,
        Action::Notice(Notice::CommandDone { command: id, .. }) if *id == command)));
    error
}

fn result(actions: &[Action], command: CommandId) -> String {
    let mut results = actions.iter().filter_map(|action| match action {
        Action::Notice(Notice::CommandDone {
            command: id,
            result,
        }) if *id == command => Some(result.clone()),
        _ => None,
    });
    let result = results.next().expect("native candle result");
    assert!(results.next().is_none());
    assert!(!actions.iter().any(|action| matches!(action,
        Action::Notice(Notice::CommandFailed { command: id, .. }) if *id == command)));
    result
}

fn no_new_http(actions: &[Action]) {
    assert!(!actions.iter().any(|action| match action {
        Action::Http(_) => true,
        Action::ManagedHttp(action) => {
            matches!(
                action.as_ref(),
                HttpAction::Prepare { .. } | HttpAction::Bind { .. }
            )
        }
        _ => false,
    }));
}

#[test]
fn a_valid_candle_command_waits_for_prepare_before_physical_http() {
    let mut e = ready();
    let actions = issued(&mut e, NOW, 7);
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_))),
        "the command must not eagerly release raw HTTP"
    );
    assert_eq!(
        actions
            .iter()
            .filter(|action| matches!(action,
        Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Prepare { .. })))
            .count(),
        1
    );
}

#[test]
fn native_wire_header_order_and_all_candle_fields_are_preserved() {
    let mut e = ready();
    let request = released(&mut e, 7, None);
    assert_eq!(request.method, Method::Post);
    assert_eq!(request.url, "https://rpc.txflow-testnet.xyz/info");
    assert_eq!(request.route, Some(RequestRoute::PublicCors));
    assert_eq!(
        request.headers,
        [
            ("Content-Type", "application/json".into()),
            ("x-api-version", "2026-08-16".into()),
        ]
    );
    assert_eq!(
        request.body.as_deref(),
        Some(
            r#"{"type":"candleSnapshot","req":{"coin":"1","interval":"1m","startTime":0,"endTime":120000}}"#
        )
    );
    reply(&mut e, NOW + 1., &request, 200, ROWS);
    let actions = e.drain_actions().collect::<Vec<_>>();
    let rows: serde_json::Value = serde_json::from_str(&result(&actions, 7)).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    for (field, expected) in [
        ("timestamp", 60.),
        ("open", 2.),
        ("high", 3.),
        ("low", 1.),
        ("close", 2.5),
        ("volume", 4.),
    ] {
        assert_eq!(rows[0][field].as_f64(), Some(expected), "{field}");
    }
    assert_eq!(e.store().markets()[0].symbol, "BTC-USDC");
    no_new_http(&actions);
}

#[test]
fn finite_fractional_bounds_keep_native_flooring() {
    let mut e = ready();
    let mut c = command(7);
    if let Command::FetchCandles { from_ms, to_ms, .. } = &mut c {
        *from_ms = Some(1.9);
        *to_ms = Some(120_000.9);
    }
    e.handle(NOW, Event::Command(c));
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    let request = actions
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    let body: serde_json::Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["req"]["startTime"], 1);
    assert_eq!(body["req"]["endTime"], 120_000);
    reply(
        &mut e,
        NOW + 1.,
        &request,
        200,
        r#"{"success":true,"code":0,"data":[]}"#,
    );
    assert_eq!(result(&e.drain_actions().collect::<Vec<_>>(), 7), "[]");
}

#[test]
fn streamed_data_at_the_new_twenty_second_bound_has_one_timeout() {
    let mut e = ready();
    let request = released(&mut e, 7, None);
    e.handle(
        NOW + 19_999.,
        Event::HttpStart {
            request: request.id,
            status: 200,
            headers: "",
        },
    );
    e.handle(
        NOW + 19_999.,
        Event::HttpChunk {
            request: request.id,
            data: ROWS.as_bytes(),
        },
    );
    assert!(e.drain_actions().all(|action| !matches!(
        action,
        Action::Notice(Notice::CommandDone { command: 7, .. })
    )));
    e.handle(
        NOW + 20_000.,
        Event::HttpEnd {
            request: request.id,
        },
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(failure(&actions, 7).kind, Some(ErrorKind::Timeout));
    reply(&mut e, NOW + 20_001., &request, 200, ROWS);
    assert!(e.drain_actions().all(|action| !matches!(
        action,
        Action::Notice(
            Notice::CommandDone { command: 7, .. } | Notice::CommandFailed { command: 7, .. }
        )
    )));
}

#[test]
fn the_original_short_caller_receipt_caps_the_candle_child() {
    let mut e = ready();
    let request = released(&mut e, 7, Some(37));
    reply(&mut e, NOW + 37., &request, 200, ROWS);
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(failure(&actions, 7).kind, Some(ErrorKind::Timeout));
}

#[test]
fn a_stalled_prepare_has_a_twenty_second_timer_and_cannot_bind_after_it() {
    let mut e = ready();
    let actions = issued(&mut e, NOW, 7);
    let timers = actions
        .iter()
        .filter_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 20_000. => Some(*timer),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        !timers.is_empty(),
        "the bound starts before the host prepares a route"
    );
    for timer in timers {
        e.handle(NOW + 20_000., Event::TimerFired { timer });
    }
    let outcome = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(failure(&outcome, 7).kind, Some(ErrorKind::Timeout));
    for action in actions {
        if let Action::ManagedHttp(action) = action
            && let HttpAction::Prepare { preparation, .. } = *action
        {
            assert!(!e.http_preparation_pending(preparation));
            e.http_prepared(
                NOW + 20_001.,
                preparation,
                Prepared {
                    id: 7,
                    expires_at: None,
                },
            );
        }
    }
    no_new_http(&e.drain_actions().collect::<Vec<_>>());
}

#[test]
fn invalid_clock_cannot_release_an_unsigned_candle_request() {
    let mut e = ready();
    let actions = issued(&mut e, f64::NAN, 7);
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
    assert_eq!(failure(&actions, 7).kind, Some(ErrorKind::InvalidInput));
}

#[test]
fn shared_capacity_refuses_before_http_and_a_completed_owner_releases_credits() {
    let mut e = ready();
    let mut held = Vec::new();
    for id in 1_000..1_128 {
        held.extend(issued(&mut e, NOW, id));
    }
    let refused = issued(&mut e, NOW, 2_000);
    assert_eq!(failure(&refused, 2_000).kind, Some(ErrorKind::Internal));
    no_new_http(&refused);
    let preparation = held
        .into_iter()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .expect("a real parked owner");
    e.http_prepared(
        NOW + 1.,
        preparation,
        Prepared {
            id: 99,
            expires_at: None,
        },
    );
    let bound = e
        .drain_actions()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Bind {
                    permit, request, ..
                } => Some((permit, request)),
                _ => None,
            },
            _ => None,
        })
        .expect("the original owner binds");
    assert!(matches!(
        e.claim_http(NOW + 1., bound.0),
        Ok(Claim::Released(_))
    ));
    reply(&mut e, NOW + 2., &bound.1, 200, ROWS);
    assert!(!result(&e.drain_actions().collect::<Vec<_>>(), 1_000).is_empty());
    let recovered = issued(&mut e, NOW + 3., 2_000);
    assert_eq!(
        recovered
            .iter()
            .filter(|action| matches!(action,
        Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Prepare { .. })))
            .count(),
        1
    );
}

#[test]
fn a_duplicate_command_cannot_replace_the_original_candle_owner() {
    let mut e = ready();
    let first = released(&mut e, 7, None);
    let duplicate = issued(&mut e, NOW + 1., 7);
    no_new_http(&duplicate);
    reply(&mut e, NOW + 2., &first, 200, ROWS);
    assert!(!result(&e.drain_actions().collect::<Vec<_>>(), 7).is_empty());
}

#[test]
fn released_transport_failure_keeps_network_metadata_and_does_not_retry() {
    let mut e = ready();
    let request = released(&mut e, 7, None);
    e.handle(
        NOW + 1.,
        Event::HttpFailed {
            request: request.id,
            error: "candle transport closed",
        },
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    let error = failure(&actions, 7);
    assert_eq!(error.message, "candle transport closed");
    assert_eq!(error.kind, Some(ErrorKind::Network));
    assert_eq!(error.outcome, None);
    no_new_http(&actions);
}

#[test]
fn native_codec_and_envelope_failures_remain_strict_without_retries() {
    for (status, body) in [
        (503, "offline"),
        (200, r#"{"success":false,"code":9,"data":[]}"#),
        (200, r#"{"success":true,"code":0,"data":[{}]}"#),
    ] {
        let mut e = ready();
        let request = released(&mut e, 7, None);
        reply(&mut e, NOW + 1., &request, status, body);
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert!(!failure(&actions, 7).message.is_empty());
        no_new_http(&actions);
    }
}

#[test]
fn native_initial_validation_precedence_stays_outside_the_physical_slice() {
    for (symbol, interval, from, expected) in [
        ("MISSING", Interval::S1, f64::NAN, "Market not found"),
        (
            "BTC-USDC",
            Interval::S1,
            0.,
            "Unsupported TxFlow interval: 1s",
        ),
        ("BTC-USDC", Interval::M1, -1., "Invalid candle time range"),
    ] {
        let mut e = ready();
        let mut c = command(7);
        if let Command::FetchCandles {
            symbol: s,
            interval: i,
            from_ms,
            ..
        } = &mut c
        {
            *s = symbol.into();
            *i = interval;
            *from_ms = Some(from);
        }
        e.handle(NOW, Event::Command(c));
        let actions = e.drain_actions().collect::<Vec<_>>();
        let expected = if interval == Interval::S1 && symbol == "BTC-USDC" {
            interval.unsupported("TxFlow")
        } else {
            expected.into()
        };
        assert_eq!(failure(&actions, 7).message, expected);
        no_new_http(&actions);
    }
}

#[test]
fn a_delivered_request_cannot_publish_a_second_numeric_terminal() {
    let mut e = ready();
    let request = released(&mut e, 7, None);
    reply(&mut e, NOW + 1., &request, 200, ROWS);
    let _ = result(&e.drain_actions().collect::<Vec<_>>(), 7);
    reply(&mut e, NOW + 2., &request, 200, ROWS);
    assert!(e.drain_actions().all(|action| !matches!(
        action,
        Action::Notice(
            Notice::CommandDone { command: 7, .. } | Notice::CommandFailed { command: 7, .. }
        )
    )));
}

#[test]
fn a_host_preparation_refusal_is_typed_not_sent_without_http_or_retry() {
    let mut e = ready();
    let actions = issued(&mut e, NOW, 7);
    let preparations = actions
        .into_iter()
        .filter_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        preparations.len(),
        1,
        "preparation refusal needs real managed custody"
    );
    e.http_preparation_failed(
        NOW + 1.,
        preparations[0],
        Error::Invalid,
        VenueError::new("route refused"),
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    let error = failure(&actions, 7);
    assert_eq!(error.message, "route refused");
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::NotSent));
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
}

fn prepared_packet(
    actions: Vec<Action>,
) -> (
    safe_cex_engine::request_context::Preparation,
    HttpRequest,
    safe_cex_engine::request_context::RoutingIntent,
) {
    let mut packets = actions.into_iter().filter_map(|action| match action {
        Action::ManagedHttp(action) => match *action {
            HttpAction::Prepare {
                preparation,
                request,
                intent,
            } => Some((preparation, request, intent)),
            _ => None,
        },
        _ => None,
    });
    let packet = packets.next().expect("one genuine Prepare");
    assert!(packets.next().is_none());
    packet
}

fn bind(
    e: &mut Driver,
    now: f64,
    preparation: safe_cex_engine::request_context::Preparation,
) -> (safe_cex_engine::request_context::Permit, HttpRequest) {
    e.http_prepared(
        now,
        preparation,
        Prepared {
            id: preparation.attempt().request.0,
            expires_at: None,
        },
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
    let mut packets = actions.into_iter().filter_map(|action| match action {
        Action::ManagedHttp(action) => match *action {
            HttpAction::Bind {
                permit, request, ..
            } => Some((permit, request)),
            _ => None,
        },
        _ => None,
    });
    let packet = packets.next().expect("one genuine Bind");
    assert!(packets.next().is_none());
    packet
}

#[test]
fn public_cors_is_captured_on_the_real_prepare_and_bind_packets() {
    let mut e = ready();
    let (preparation, prepared, intent) = prepared_packet(issued(&mut e, NOW, 7));
    assert_eq!(
        intent,
        safe_cex_engine::request_context::RoutingIntent::Public
    );
    assert_eq!(prepared.route, Some(RequestRoute::PublicCors));
    assert_eq!(prepared.timeout_ms, Some(20_000));
    assert_eq!(
        prepared.headers,
        [
            ("Content-Type", "application/json".into()),
            ("x-api-version", "2026-08-16".into()),
        ]
    );
    let (permit, packet) = bind(&mut e, NOW + 3., preparation);
    assert_eq!(packet.route, Some(RequestRoute::PublicCors));
    assert_eq!(packet.body, prepared.body);
    let Ok(Claim::Released(release)) = e.claim_http(NOW + 3., permit) else {
        panic!("live unsigned public Read releases once");
    };
    assert_eq!(release.deadline, NOW + 20_000.);
    reply(&mut e, NOW + 4., &packet, 200, ROWS);
    let _ = result(&e.drain_actions().collect::<Vec<_>>(), 7);
}

#[test]
fn fractional_caller_receipt_is_exact_at_claim_and_data_equality() {
    let start = NOW + 0.25;
    for elapsed in [36.75, 37.] {
        let mut e = ready();
        e.handle_command(start, command(7), Some(37)).unwrap();
        let actions = e.drain_actions().collect();
        let (preparation, prepared, _) = prepared_packet(actions);
        assert_eq!(prepared.timeout_ms, Some(37));
        let (permit, packet) = bind(&mut e, start + 1., preparation);
        let Ok(Claim::Released(release)) = e.claim_http(start + 2., permit) else {
            panic!("the exact capped leg is live before equality");
        };
        assert_eq!(release.deadline, start + 37.);
        reply(&mut e, start + elapsed, &packet, 200, ROWS);
        let actions = e.drain_actions().collect::<Vec<_>>();
        if elapsed < 37. {
            let _ = result(&actions, 7);
        } else {
            assert_eq!(failure(&actions, 7).kind, Some(ErrorKind::Timeout));
        }
    }
}

#[test]
fn an_unclaimed_bind_expires_without_renewing_the_original_bound() {
    let mut e = ready();
    let (preparation, _, _) = prepared_packet(issued(&mut e, NOW, 7));
    let (permit, request) = bind(&mut e, NOW + 19_999., preparation);
    assert!(e.claim_http(NOW + 20_000., permit).is_err());
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(failure(&actions, 7).kind, Some(ErrorKind::Timeout));
    no_new_http(&actions);
    reply(&mut e, NOW + 20_001., &request, 200, ROWS);
    assert!(e.drain_actions().all(|action| !matches!(
        action,
        Action::Notice(
            Notice::CommandDone { command: 7, .. } | Notice::CommandFailed { command: 7, .. }
        )
    )));
}

#[test]
fn stale_preparation_and_data_cannot_retire_a_reused_command_generation() {
    let mut e = ready();
    let (old, old_request, _) = prepared_packet(issued(&mut e, NOW, 7));
    e.http_preparation_failed(
        NOW + 1.,
        old,
        Error::Invalid,
        VenueError::new("old route refused"),
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(failure(&actions, 7).message, "old route refused");
    let (current, _, _) = prepared_packet(issued(&mut e, NOW + 2., 7));
    assert_ne!(old.attempt(), current.attempt());
    e.http_prepared(
        NOW + 3.,
        old,
        Prepared {
            id: 99,
            expires_at: None,
        },
    );
    e.http_preparation_failed(
        NOW + 3.,
        old,
        Error::TimedOut,
        VenueError::new("stale failure"),
    );
    reply(&mut e, NOW + 3., &old_request, 503, "stale body");
    let stale = e.drain_actions().collect::<Vec<_>>();
    assert!(
        stale.is_empty(),
        "stale ingress emits no cleanup or terminal"
    );
    assert!(e.http_preparation_pending(current));
    let (permit, request) = bind(&mut e, NOW + 4., current);
    assert!(matches!(
        e.claim_http(NOW + 4., permit),
        Ok(Claim::Released(_))
    ));
    reply(&mut e, NOW + 5., &request, 200, ROWS);
    let _ = result(&e.drain_actions().collect::<Vec<_>>(), 7);
}

#[test]
fn completing_every_parked_read_releases_all_root_and_child_credits() {
    let mut e = ready();
    let mut held = Vec::new();
    for id in 1_000..1_128 {
        held.push(prepared_packet(issued(&mut e, NOW, id)).0);
    }
    assert_eq!(
        failure(&issued(&mut e, NOW, 2_000), 2_000).kind,
        Some(ErrorKind::Internal)
    );
    let mut completed = std::collections::HashSet::new();
    for preparation in held {
        let (permit, request) = bind(&mut e, NOW + 1., preparation);
        assert!(matches!(
            e.claim_http(NOW + 1., permit),
            Ok(Claim::Released(_))
        ));
        reply(&mut e, NOW + 2., &request, 200, ROWS);
        for action in e.drain_actions() {
            if let Action::Notice(Notice::CommandDone { command, .. }) = action {
                assert!(completed.insert(command));
            }
        }
    }
    assert_eq!(completed.len(), 128);
    assert!((1_000..1_128).all(|id| completed.contains(&id)));
    for id in 1_000..1_128 {
        let (_, request, _) = prepared_packet(issued(&mut e, NOW + 3., id));
        assert_eq!(request.route, Some(RequestRoute::PublicCors));
    }
    assert_eq!(
        failure(&issued(&mut e, NOW + 3., 2_000), 2_000).kind,
        Some(ErrorKind::Internal)
    );
}
