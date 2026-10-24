mod support;
use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{
    Claim, Error, HttpAction, Preparation, Prepared, RoutingIntent,
};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Notice, Response, TimerId, VenueError,
};
use safe_cex_venue_orderly::{Options, Orderly};

type TestEngine = Engine<Orderly>;
const EMPTY: &str = r#"{"s":"no_data"}"#;
const CURRENT_EMPTY: &str = r#"{"success":true,"data":{"rows":[]}}"#;

fn engine() -> TestEngine {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Orderly::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    engine.start(0.);
    let catalogue = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .expect("legacy startup catalogue");
    engine.handle(
        0.,
        Event::HttpResponse {
            request: catalogue.id,
            response: Response {
                status: 200,
                headers: "",
                body: support::MARKETS,
            },
        },
    );
    let tickers = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.ends_with("/v1/public/futures") => Some(request),
            _ => None,
        })
        .expect("complete startup before saturating command reads");
    response(&mut engine, &tickers, 0., support::TICKERS);
    engine
}

fn query(engine: &mut TestEngine, command: u32, now: f64) -> Vec<Action> {
    query_kind(engine, command, now, true)
}
fn query_kind(engine: &mut TestEngine, command: u32, now: f64, bounded: bool) -> Vec<Action> {
    engine.handle(
        now,
        Event::Command(Command::FetchCandles {
            command,
            symbol: "BTCUSDC".into(),
            interval: Interval::M1,
            limit: Some(2),
            from_ms: bounded.then_some(1001.),
            to_ms: bounded.then_some(120_999.),
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
fn both_endpoints_prepare_before_exact_unsigned_handoff() {
    for bounded in [false, true] {
        let mut engine = engine();
        let initial = query_kind(&mut engine, 7, 100., bounded);
        let (token, declaration) = preparation(&initial);
        deadline(&initial);
        assert!(declaration.headers.is_empty());
        assert!(declaration.body.is_none());
        let suffix = if bounded {
            "/v1/tv/kline_history?symbol=PERP_BTC_USDC&resolution=1m&limit=2&from=1&to=120"
        } else {
            "/v1/kline?symbol=PERP_BTC_USDC&type=1m&limit=2"
        };
        assert!(declaration.url.ends_with(suffix), "{}", declaration.url);
        let packet = release(&mut engine, token, 101.);
        assert_eq!(packet.url, declaration.url);
        let actions = response(
            &mut engine,
            &packet,
            102.,
            if bounded { EMPTY } else { CURRENT_EMPTY },
        );
        assert_eq!(actions.iter().filter(|action| matches!(action, Action::Notice(Notice::CommandDone { command: 7, result }) if result == "[]")).count(), 1);
        assert!(response(&mut engine, &packet, 103., EMPTY).is_empty());
    }
}

#[test]
fn preparation_refusal_preserves_typed_metadata_without_retry() {
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
}

#[test]
fn parked_preparation_expires_once_and_cannot_bind_late() {
    let mut engine = engine();
    let initial = query(&mut engine, 7, 100.);
    let (token, _) = preparation(&initial);
    engine.handle(
        20_100.,
        Event::TimerFired {
            timer: deadline(&initial),
        },
    );
    assert_eq!(
        failure(&engine.drain_actions().collect::<Vec<_>>(), 7).kind,
        Some(ErrorKind::Timeout)
    );
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
fn late_stream_rows_cannot_publish_malformed_diagnostics() {
    let mut engine = engine();
    let initial = query_kind(&mut engine, 7, 100., false);
    let (token, _) = preparation(&initial);
    let packet = release(&mut engine, token, 101.);
    let mut actions = response(
        &mut engine,
        &packet,
        20_100.,
        r#"{"success":true,"data":{"rows":[{"broken":true}]}}"#,
    );
    engine.handle(
        20_100.,
        Event::TimerFired {
            timer: deadline(&initial),
        },
    );
    actions.extend(engine.drain_actions());
    assert_eq!(failure(&actions, 7).kind, Some(ErrorKind::Timeout));
    assert!(!actions.iter().any(|action| matches!(action, Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::STREAM_MALFORMED)));
}

#[test]
fn neither_candle_endpoint_adds_automatic_transport_retries() {
    for bounded in [false, true] {
        let mut engine = engine();
        let initial = query_kind(&mut engine, 7, 100., bounded);
        let (token, _) = preparation(&initial);
        let packet = release(&mut engine, token, 101.);
        engine.handle(
            102.,
            Event::HttpFailed {
                request: packet.id,
                error: "network lost",
            },
        );
        let actions: Vec<_> = engine.drain_actions().collect();
        assert_eq!(failure(&actions, 7).kind, Some(ErrorKind::Network));
        assert!(!actions.iter().any(|action| matches!(action, Action::ManagedHttp(action) if matches!(&**action, HttpAction::Prepare { .. }))));
    }
}

#[test]
fn expired_callbacks_cannot_complete_a_reused_command() {
    let mut engine = engine();
    let initial = query(&mut engine, 7, 100.);
    let (token, _) = preparation(&initial);
    let old = release(&mut engine, token, 101.);
    engine.handle(
        20_100.,
        Event::TimerFired {
            timer: deadline(&initial),
        },
    );
    assert_eq!(
        failure(&engine.drain_actions().collect::<Vec<_>>(), 7).kind,
        Some(ErrorKind::Timeout)
    );
    let fresh = query(&mut engine, 7, 20_101.);
    let (token, _) = preparation(&fresh);
    assert!(response(&mut engine, &old, 20_102., EMPTY).is_empty());
    let packet = release(&mut engine, token, 20_103.);
    assert!(
        response(&mut engine, &packet, 20_104., EMPTY)
            .iter()
            .any(|action| matches!(
                action,
                Action::Notice(Notice::CommandDone { command: 7, .. })
            ))
    );
}

#[test]
fn duplicate_command_preserves_the_original_managed_owner() {
    let mut engine = engine();
    let first = query(&mut engine, 7, 100.);
    let (token, _) = preparation(&first);
    let duplicate = query(&mut engine, 7, 101.);
    assert!(
        !duplicate
            .iter()
            .any(|action| matches!(action, Action::Http(_) | Action::ManagedHttp(_)))
    );
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
fn failed_stream_does_not_publish_a_valid_prefix_or_write_outcome() {
    let mut engine = engine();
    let initial = query_kind(&mut engine, 7, 100., false);
    let (token, _) = preparation(&initial);
    let packet = release(&mut engine, token, 101.);
    let actions = response(
        &mut engine,
        &packet,
        102.,
        r#"{"success":false,"data":{"rows":[{"start_timestamp":1000,"open":1,"high":2,"low":1,"close":2,"volume":3}]},"message":"native refusal"}"#,
    );
    assert_eq!(failure(&actions, 7).outcome, None);
}

fn check_initial_invalid_clock(bounded: bool) {
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut engine = engine();
        let actions = query_kind(&mut engine, 7, now, bounded);
        let error = failure(&actions, 7);
        assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
        assert_eq!(error.outcome, None);
        assert!(
            actions
                .iter()
                .all(|action| matches!(action, Action::Notice(_)))
        );

        let fresh = query_kind(&mut engine, 7, 100., bounded);
        let (token, _) = preparation(&fresh);
        let packet = release(&mut engine, token, 101.);
        let actions = response(
            &mut engine,
            &packet,
            102.,
            if bounded { EMPTY } else { CURRENT_EMPTY },
        );
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

fn check_initial_capacity_rejection(bounded: bool) {
    let mut engine = engine();
    let mut first = None;
    for command in 1..=256 {
        let actions = query_kind(&mut engine, command, 100., bounded);
        let (token, _) = preparation(&actions);
        if command == 1 {
            first = Some(token);
        }
    }
    let actions = query_kind(&mut engine, 257, 101., bounded);
    let error = failure(&actions, 257);
    assert_eq!(error.kind, Some(ErrorKind::Internal));
    assert_eq!(error.outcome, None);
    assert!(
        actions
            .iter()
            .all(|action| matches!(action, Action::Notice(_)))
    );

    let first = release(&mut engine, first.unwrap(), 102.);
    let actions = response(
        &mut engine,
        &first,
        103.,
        if bounded { EMPTY } else { CURRENT_EMPTY },
    );
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
    let fresh = query_kind(&mut engine, 257, 104., bounded);
    let (token, _) = preparation(&fresh);
    let fresh = release(&mut engine, token, 105.);
    let actions = response(
        &mut engine,
        &fresh,
        106.,
        if bounded { EMPTY } else { CURRENT_EMPTY },
    );
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
fn initial_bounded_clock_rejection_has_no_write_outcome() {
    check_initial_invalid_clock(true);
}

#[test]
fn initial_current_clock_rejection_has_no_write_outcome() {
    check_initial_invalid_clock(false);
}

#[test]
fn initial_bounded_capacity_rejection_keeps_existing_owner() {
    check_initial_capacity_rejection(true);
}

#[test]
fn initial_current_capacity_rejection_keeps_existing_owner() {
    check_initial_capacity_rejection(false);
}
