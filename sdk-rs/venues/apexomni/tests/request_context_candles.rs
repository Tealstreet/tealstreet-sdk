use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{
    Claim, Error, HttpAction, Preparation, Prepared, RoutingIntent,
};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Notice, Response, TimerId, VenueError,
};
use safe_cex_venue_apexomni::{ApexOmni, Options};

type TestEngine = Engine<ApexOmni>;
const EMPTY: &str = r#"{"code":0,"data":{"BTCUSDT":[]}}"#;

fn engine() -> TestEngine {
    safe_cex_engine::test_support::seeded_engine(ApexOmni::new(Options {
        public_data_only: true,
        ..Default::default()
    }))
}

fn query(engine: &mut TestEngine, command: u32, now: f64) -> Vec<Action> {
    engine.handle(
        now,
        Event::Command(Command::FetchCandles {
            command,
            symbol: "BTCUSDT".into(),
            interval: Interval::M1,
            limit: None,
            from_ms: Some(0.),
            to_ms: Some(120_000.),
        }),
    );
    engine.drain_actions().collect()
}

fn preparation(actions: &[Action]) -> (Preparation, HttpRequest) {
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_))),
        "candle HTTP must await preparation and final claim"
    );
    actions
        .iter()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match &**action {
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
        .expect("managed public candle preparation")
}

fn release(engine: &mut TestEngine, token: Preparation, now: f64) -> HttpRequest {
    engine.http_prepared(
        now,
        token,
        Prepared {
            id: token.attempt().request.0,
            expires_at: None,
        },
    );
    let (permit, packet) = engine
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
        .expect("prepared candle binds its exact packet");
    assert!(matches!(
        engine.claim_http(now, permit),
        Ok(Claim::Released(_))
    ));
    packet
}

fn response(engine: &mut TestEngine, request: &HttpRequest, now: f64, body: &str) -> Vec<Action> {
    response_status(engine, request, now, 200, body)
}

fn response_status(
    engine: &mut TestEngine,
    request: &HttpRequest,
    now: f64,
    status: u16,
    body: &str,
) -> Vec<Action> {
    engine.handle(
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
    engine.drain_actions().collect()
}

fn failure(actions: &[Action], command: u32) -> &VenueError {
    let failures: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: id, error }) if *id == command => {
                Some(error)
            }
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1, "one contextual terminal failure");
    assert!(!actions.iter().any(|action| matches!(action, Action::Notice(Notice::CommandDone { command: id, .. }) if *id == command)));
    failures[0]
}

fn deadline(actions: &[Action]) -> TimerId {
    actions
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == 20_100. => Some(*timer),
            _ => None,
        })
        .expect("original raw twenty-second deadline")
}

#[test]
fn public_candles_prepare_before_exact_unsigned_handoff() {
    let mut engine = engine();
    let actions = query(&mut engine, 7, 100.);
    let (token, unsigned) = preparation(&actions);
    deadline(&actions);
    assert!(unsigned.headers.is_empty());
    assert!(unsigned.body.is_none());
    assert_eq!(
        unsigned.url,
        "https://omni.apex.exchange/api/v3/klines?end=120&interval=1&start=0&symbol=BTCUSDT"
    );
    let packet = release(&mut engine, token, 500.);
    assert_eq!(packet.url, unsigned.url);
    assert!(packet.headers.is_empty());
    let actions = response(&mut engine, &packet, 501., EMPTY);
    assert_eq!(actions.iter().filter(|action| matches!(action, Action::Notice(Notice::CommandDone { command: 7, result }) if result == "[]")).count(), 1);
    assert!(response(&mut engine, &packet, 502., EMPTY).is_empty());
}

#[test]
fn preparation_refusal_keeps_typed_failure_and_never_retries_as_network() {
    let mut engine = engine();
    let actions = query(&mut engine, 7, 100.);
    let (token, _) = preparation(&actions);
    engine.http_preparation_failed(
        101.,
        token,
        Error::Invalid,
        VenueError::new("custom route unsupported")
            .kind(ErrorKind::Unsupported)
            .venue_code("route.test"),
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    let error = failure(&actions, 7);
    assert_eq!(error.kind, Some(ErrorKind::Unsupported));
    assert_eq!(error.venue_code.as_deref(), Some("route.test"));
    assert!(!actions.iter().any(|action| matches!(action, Action::ManagedHttp(action) if matches!(&**action, HttpAction::Prepare { .. }))));
    engine.http_prepared(
        102.,
        token,
        Prepared {
            id: token.attempt().request.0,
            expires_at: None,
        },
    );
    assert!(engine.drain_actions().next().is_none());
}

#[test]
fn parked_route_expires_once_at_the_original_deadline() {
    let mut engine = engine();
    let actions = query(&mut engine, 7, 100.);
    let (token, _) = preparation(&actions);
    engine.handle(
        20_100.,
        Event::TimerFired {
            timer: deadline(&actions),
        },
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    assert_eq!(failure(&actions, 7).kind, Some(ErrorKind::Timeout));
    engine.http_prepared(
        20_101.,
        token,
        Prepared {
            id: token.attempt().request.0,
            expires_at: None,
        },
    );
    assert!(engine.drain_actions().next().is_none());
}

#[test]
fn exact_deadline_response_cannot_publish_success_or_row_warning() {
    let mut engine = engine();
    let initial = query(&mut engine, 7, 100.);
    let (token, _) = preparation(&initial);
    let packet = release(&mut engine, token, 101.);
    let mut actions = response(
        &mut engine,
        &packet,
        20_100.,
        r#"{"code":0,"data":{"BTCUSDT":[{"t":1000,"o":"0","h":"0","l":"0","c":"0","v":"0"}]}}"#,
    );
    engine.handle(
        20_100.,
        Event::TimerFired {
            timer: deadline(&initial),
        },
    );
    actions.extend(engine.drain_actions());
    assert_eq!(failure(&actions, 7).kind, Some(ErrorKind::Timeout));
    assert!(!actions.iter().any(|action| matches!(action, Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::ROW_SKIPPED)));
}

#[test]
fn signature_and_transport_read_retries_have_separate_budgets_and_one_deadline() {
    let mut engine = engine();
    let mut actions = query(&mut engine, 7, 100.);
    let mut packets = Vec::new();
    for attempt in 0..5 {
        let (token, _) = preparation(&actions);
        deadline(&actions);
        let now = 200. + f64::from(attempt) * 100.;
        let packet = release(&mut engine, token, now);
        assert!(packet.headers.is_empty());
        packets.push(packet.id);
        actions = if attempt == 2 {
            response(
                &mut engine,
                &packet,
                now + 1.,
                r#"{"code":20016,"msg":"bad signature"}"#,
            )
        } else {
            engine.handle(
                now + 1.,
                Event::HttpFailed {
                    request: packet.id,
                    error: "network lost",
                },
            );
            engine.drain_actions().collect()
        };
    }
    assert_eq!(
        packets
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        5
    );
    assert_eq!(failure(&actions, 7).kind, Some(ErrorKind::Network));
    assert!(!actions.iter().any(|action| matches!(action, Action::ManagedHttp(action) if matches!(&**action, HttpAction::Prepare { .. }))));
}

#[test]
fn undecodable_http_replies_keep_typed_internal_after_read_retry_exhaustion() {
    let mut engine = engine();
    let mut actions = query(&mut engine, 7, 100.);
    for attempt in 0..4 {
        let (token, _) = preparation(&actions);
        deadline(&actions);
        let now = 200. + f64::from(attempt) * 100.;
        let packet = release(&mut engine, token, now);
        engine.handle(
            now + 1.,
            Event::HttpStart {
                request: packet.id,
                status: 200,
                headers: "",
            },
        );
        engine.handle(
            now + 2.,
            Event::HttpChunk {
                request: packet.id,
                data: &[0xff],
            },
        );
        engine.handle(now + 3., Event::HttpEnd { request: packet.id });
        actions = engine.drain_actions().collect();
    }
    let error = failure(&actions, 7);
    assert_eq!(error.kind, Some(ErrorKind::Internal));
    assert_eq!(error.outcome, None);
    assert!(!actions.iter().any(|action| matches!(action, Action::ManagedHttp(action) if matches!(&**action, HttpAction::Prepare { .. }))));
}

#[test]
fn retry_cannot_renew_its_root_or_complete_a_reused_command() {
    let mut engine = engine();
    let initial = query(&mut engine, 7, 100.);
    let (token, _) = preparation(&initial);
    let old = release(&mut engine, token, 101.);
    engine.handle(
        200.,
        Event::HttpFailed {
            request: old.id,
            error: "read failed",
        },
    );
    let retry: Vec<_> = engine.drain_actions().collect();
    deadline(&retry);
    let (retry_token, _) = preparation(&retry);
    engine.handle(
        20_100.,
        Event::TimerFired {
            timer: deadline(&retry),
        },
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    assert_eq!(failure(&actions, 7).kind, Some(ErrorKind::Timeout));
    let fresh = query(&mut engine, 7, 20_101.);
    let (fresh_token, _) = preparation(&fresh);
    engine.http_prepared(
        20_102.,
        retry_token,
        Prepared {
            id: retry_token.attempt().request.0,
            expires_at: None,
        },
    );
    assert!(engine.drain_actions().next().is_none());
    assert!(response(&mut engine, &old, 20_103., EMPTY).is_empty());
    let fresh = release(&mut engine, fresh_token, 20_104.);
    assert!(
        response(&mut engine, &fresh, 20_105., EMPTY)
            .iter()
            .any(|action| matches!(
                action,
                Action::Notice(Notice::CommandDone { command: 7, .. })
            ))
    );
}

#[test]
fn duplicate_command_cannot_replace_parked_candle_authority() {
    let mut engine = engine();
    let first = query(&mut engine, 7, 100.);
    let (token, _) = preparation(&first);
    let duplicate = query(&mut engine, 7, 101.);
    assert!(
        !duplicate
            .iter()
            .any(|action| matches!(action, Action::ManagedHttp(_) | Action::Http(_)))
    );
    assert!(duplicate.iter().any(|action| matches!(action, Action::Notice(Notice::Event(event)) if event.code == "command.duplicate")));
    let packet = release(&mut engine, token, 102.);
    let actions = response(&mut engine, &packet, 103., EMPTY);
    assert_eq!(
        actions
            .iter()
            .filter(|action| matches!(
                action,
                Action::Notice(Notice::CommandDone { command: 7, .. })
            ))
            .count(),
        1
    );
}

#[test]
fn initial_invalid_clock_is_one_read_failure_without_authority_or_write_outcome() {
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut engine = engine();
        let actions = query(&mut engine, 7, now);
        let error = failure(&actions, 7);
        assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
        assert_eq!(error.outcome, None);
        assert!(
            actions
                .iter()
                .all(|action| matches!(action, Action::Notice(_)))
        );

        let fresh = query(&mut engine, 7, 100.);
        let (token, _) = preparation(&fresh);
        let packet = release(&mut engine, token, 101.);
        let actions = response(&mut engine, &packet, 102., EMPTY);
        assert_eq!(
            actions
                .iter()
                .filter(|action| matches!(
                    action,
                    Action::Notice(Notice::CommandDone { command: 7, .. })
                ))
                .count(),
            1
        );
    }
}

#[test]
fn initial_capacity_rejection_keeps_parked_owner_and_reuses_only_freed_capacity() {
    let mut engine = engine();
    let mut first = None;
    for command in 1..=256 {
        let actions = query(&mut engine, command, 100.);
        let (token, _) = preparation(&actions);
        if command == 1 {
            first = Some(token);
        }
    }
    let actions = query(&mut engine, 257, 101.);
    let error = failure(&actions, 257);
    assert_eq!(error.kind, Some(ErrorKind::Internal));
    assert_eq!(error.outcome, None);
    assert!(
        actions
            .iter()
            .all(|action| matches!(action, Action::Notice(_)))
    );

    let first = release(&mut engine, first.unwrap(), 102.);
    let actions = response(&mut engine, &first, 103., EMPTY);
    assert_eq!(
        actions
            .iter()
            .filter(|action| matches!(
                action,
                Action::Notice(Notice::CommandDone { command: 1, .. })
            ))
            .count(),
        1
    );
    let fresh = query(&mut engine, 257, 104.);
    let (token, _) = preparation(&fresh);
    let fresh = release(&mut engine, token, 105.);
    let actions = response(&mut engine, &fresh, 106., EMPTY);
    assert_eq!(
        actions
            .iter()
            .filter(|action| matches!(
                action,
                Action::Notice(Notice::CommandDone { command: 257, .. })
            ))
            .count(),
        1
    );
}

#[test]
fn native_candle_failures_keep_read_messages_without_write_outcomes() {
    for (status, body, signature_retry, expected) in [
        (
            503,
            r#"{"code":0,"msg":"gateway"}"#,
            false,
            "ApeX Omni request failed [0]: gateway",
        ),
        (
            200,
            r#"{"code":"#,
            false,
            "ApeX Omni response contains malformed error metadata",
        ),
        (
            200,
            r#"{"code":20016,"msg":"bad signature"}"#,
            true,
            "ApeX Omni request failed [20016]: bad signature",
        ),
    ] {
        let mut engine = engine();
        let actions = query(&mut engine, 7, 100.);
        let (token, _) = preparation(&actions);
        let packet = release(&mut engine, token, 101.);
        let mut actions = response_status(&mut engine, &packet, 102., status, body);
        if signature_retry {
            let (token, _) = preparation(&actions);
            let packet = release(&mut engine, token, 103.);
            actions = response_status(&mut engine, &packet, 104., status, body);
        }
        let error = failure(&actions, 7);
        assert_eq!(error.message, expected);
        assert_eq!(error.outcome, None);
        assert_eq!(error.kind, None);
        assert_eq!(error.venue_code, None);
    }
}
