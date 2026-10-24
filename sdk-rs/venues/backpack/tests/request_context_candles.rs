use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{
    Claim, Error, HttpAction, Preparation, Prepared, RoutingIntent,
};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Method, Notice, Outcome, Response,
    TimerId, VenueError,
};
use safe_cex_venue_backpack::{Backpack, Options};

type TestEngine = Engine<Backpack>;
const EMPTY: &str = "[]";

fn engine() -> TestEngine {
    safe_cex_engine::test_support::seeded_engine(Backpack::new(Options::default()))
}

fn query(engine: &mut TestEngine, command: u32, now: f64) -> Vec<Action> {
    engine.handle(
        now,
        Event::Command(Command::FetchCandles {
            command,
            symbol: "SOL &/USDC_PERP".into(),
            interval: Interval::M1,
            limit: Some(0),
            from_ms: Some(1001.9),
            to_ms: Some(120999.9),
        }),
    );
    engine.drain_actions().collect()
}

fn no_opaque_http(actions: &[Action]) {
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_))),
        "candle HTTP must await managed preparation and final claim"
    );
}

fn native_packet(request: &HttpRequest) {
    assert_eq!(request.method, Method::Get);
    assert_eq!(
        request.url,
        "https://api.backpack.exchange/api/v1/klines?endTime=120&interval=1m&startTime=1&symbol=SOL%20%26%2FUSDC_PERP"
    );
    assert_eq!(request.headers, vec![("X-Broker-Id", "1200".into())]);
    assert_eq!(request.body, None);
    assert_eq!(request.route, None);
}

fn preparation(actions: &[Action]) -> (Preparation, HttpRequest) {
    no_opaque_http(actions);
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
                    native_packet(request);
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
    native_packet(&packet);
    assert!(matches!(
        engine.claim_http(now, permit),
        Ok(Claim::Released(_))
    ));
    packet
}

fn response(engine: &mut TestEngine, request: &HttpRequest, now: f64, body: &str) -> Vec<Action> {
    engine.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
    engine.drain_actions().collect()
}

fn failure(actions: &[Action], command: u32) -> &VenueError {
    let errors: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: id, error }) if *id == command => {
                Some(error)
            }
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 1, "one contextual terminal failure");
    assert!(!actions.iter().any(|action| matches!(action, Action::Notice(Notice::CommandDone { command: id, .. }) if *id == command)));
    errors[0]
}

fn done(actions: &[Action], command: u32) {
    assert_eq!(actions.iter().filter(|action| matches!(action, Action::Notice(Notice::CommandDone { command: id, result }) if *id == command && result == EMPTY)).count(), 1);
    assert!(!actions.iter().any(|action| matches!(action, Action::Notice(Notice::CommandFailed { command: id, .. }) if *id == command)));
}

fn no_new_work(actions: &[Action]) {
    no_opaque_http(actions);
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::SetTimer { .. }))
    );
    assert!(!actions.iter().any(|action| matches!(action, Action::ManagedHttp(action) if matches!(&**action, HttpAction::Prepare { .. } | HttpAction::Bind { .. }))));
}

fn deadline(actions: &[Action]) -> TimerId {
    actions
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == 20_100. => Some(*timer),
            _ => None,
        })
        .expect("original twenty-second candle deadline")
}

#[test]
fn native_static_headers_survive_prepare_bind_and_exact_unsigned_handoff() {
    let mut engine = engine();
    let actions = query(&mut engine, 7, 100.);
    let (token, unsigned) = preparation(&actions);
    deadline(&actions);
    assert_eq!(unsigned.timeout_ms, Some(20_000));
    let packet = release(&mut engine, token, 101.);
    done(&response(&mut engine, &packet, 102., EMPTY), 7);
    assert!(response(&mut engine, &packet, 103., EMPTY).is_empty());
    assert!(engine.store().markets().is_empty());
}

#[test]
fn stale_route_reprepares_same_headers_without_extending_the_root() {
    let mut engine = engine();
    let initial = query(&mut engine, 7, 100.);
    let (old, _) = preparation(&initial);
    deadline(&initial);
    engine.http_prepared(
        101.,
        old,
        Prepared {
            id: old.attempt().request.0,
            expires_at: None,
        },
    );
    let old_permit = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Bind {
                    permit, request, ..
                } => {
                    native_packet(&request);
                    Some(permit)
                }
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    engine.http_preparation_failed(150., old, Error::Stale, VenueError::new("route expired"));
    let actions: Vec<_> = engine.drain_actions().collect();
    let (replacement, request) = preparation(&actions);
    assert_ne!(replacement, old);
    assert_eq!(request.timeout_ms, Some(19_950));
    engine.http_preparation_failed(151., old, Error::Invalid, VenueError::new("old failure"));
    assert!(engine.drain_actions().next().is_none());
    assert_eq!(engine.claim_http(152., old_permit), Err(Error::Revoked));
    let packet = release(&mut engine, replacement, 153.);
    done(&response(&mut engine, &packet, 154., EMPTY), 7);
}

#[test]
fn preparation_refusal_keeps_typed_metadata_and_local_unsent_fact_without_retry() {
    let mut engine = engine();
    let actions = query(&mut engine, 7, 100.);
    let (token, _) = preparation(&actions);
    engine.http_preparation_failed(
        101.,
        token,
        Error::Invalid,
        VenueError::new("custom route unsupported")
            .kind(ErrorKind::Unsupported)
            .venue_code("route.test")
            .venue_message("route detail")
            .retry_after_ms(1234.),
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    let error = failure(&actions, 7);
    assert_eq!(error.kind, Some(ErrorKind::Unsupported));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert_eq!(error.venue_code.as_deref(), Some("route.test"));
    assert_eq!(error.venue_message.as_deref(), Some("route detail"));
    assert_eq!(error.retry_after_ms, Some(1234.));
    no_new_work(&actions);
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
fn parked_route_expires_once_and_late_preparation_has_no_rights() {
    let mut engine = engine();
    let initial = query(&mut engine, 7, 100.);
    let (token, _) = preparation(&initial);
    engine.handle(
        20_100.,
        Event::TimerFired {
            timer: deadline(&initial),
        },
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    let error = failure(&actions, 7);
    assert_eq!(error.kind, Some(ErrorKind::Timeout));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    no_new_work(&actions);
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
fn released_reply_at_deadline_cannot_complete_or_steal_a_reused_command() {
    let mut engine = engine();
    let initial = query(&mut engine, 7, 100.);
    let (token, _) = preparation(&initial);
    let old = release(&mut engine, token, 101.);
    let mut actions = response(&mut engine, &old, 20_100., EMPTY);
    engine.handle(
        20_100.,
        Event::TimerFired {
            timer: deadline(&initial),
        },
    );
    actions.extend(engine.drain_actions());
    assert_eq!(failure(&actions, 7).kind, Some(ErrorKind::Timeout));
    no_new_work(&actions);
    let fresh = query(&mut engine, 7, 20_101.);
    let (token, _) = preparation(&fresh);
    assert!(response(&mut engine, &old, 20_102., EMPTY).is_empty());
    let fresh = release(&mut engine, token, 20_103.);
    done(&response(&mut engine, &fresh, 20_104., EMPTY), 7);
}

#[test]
fn transport_and_integrity_failures_keep_engine_kind_and_never_retry() {
    for integrity in [false, true] {
        let mut engine = engine();
        let actions = query(&mut engine, 7, 100.);
        let (token, _) = preparation(&actions);
        let packet = release(&mut engine, token, 101.);
        if integrity {
            engine.handle(
                102.,
                Event::HttpStart {
                    request: packet.id,
                    status: 200,
                    headers: "",
                },
            );
            engine.handle(
                103.,
                Event::HttpChunk {
                    request: packet.id,
                    data: &[0xff],
                },
            );
            engine.handle(104., Event::HttpEnd { request: packet.id });
        } else {
            engine.handle(
                102.,
                Event::HttpFailed {
                    request: packet.id,
                    error: "network lost",
                },
            );
        }
        let actions: Vec<_> = engine.drain_actions().collect();
        let error = failure(&actions, 7);
        assert_eq!(
            error.kind,
            Some(if integrity {
                ErrorKind::Internal
            } else {
                ErrorKind::Network
            })
        );
        assert_eq!(error.outcome, None);
        no_new_work(&actions);
    }
}

#[test]
fn duplicate_command_preserves_parked_authority_and_concurrent_owners() {
    let mut engine = engine();
    let first = query(&mut engine, 7, 100.);
    let (first, _) = preparation(&first);
    let second = query(&mut engine, 8, 101.);
    let (second, _) = preparation(&second);
    let duplicate = query(&mut engine, 7, 102.);
    no_new_work(&duplicate);
    assert!(duplicate.iter().any(|action| matches!(action, Action::Notice(Notice::Event(event)) if event.code == "command.duplicate")));
    let second = release(&mut engine, second, 103.);
    done(&response(&mut engine, &second, 104., EMPTY), 8);
    let first = release(&mut engine, first, 105.);
    done(&response(&mut engine, &first, 106., EMPTY), 7);
    assert!(response(&mut engine, &second, 107., EMPTY).is_empty());
}

#[test]
fn invalid_initial_clock_has_no_read_outcome_or_authority_and_allows_reuse() {
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut engine = engine();
        let actions = query(&mut engine, 7, now);
        no_opaque_http(&actions);
        assert_eq!(failure(&actions, 7).kind, Some(ErrorKind::InvalidInput));
        assert_eq!(failure(&actions, 7).outcome, None);
        no_new_work(&actions);
        let fresh = query(&mut engine, 7, 100.);
        let (token, _) = preparation(&fresh);
        let fresh = release(&mut engine, token, 101.);
        done(&response(&mut engine, &fresh, 102., EMPTY), 7);
    }
}

#[test]
fn initial_capacity_refusal_preserves_old_owner_and_reuses_freed_capacity() {
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
    assert_eq!(failure(&actions, 257).kind, Some(ErrorKind::Internal));
    assert_eq!(failure(&actions, 257).outcome, None);
    no_new_work(&actions);
    let first = release(&mut engine, first.unwrap(), 102.);
    done(&response(&mut engine, &first, 103., EMPTY), 1);
    let fresh = query(&mut engine, 257, 104.);
    let (token, _) = preparation(&fresh);
    let fresh = release(&mut engine, token, 105.);
    done(&response(&mut engine, &fresh, 106., EMPTY), 257);
}

#[test]
fn native_rows_keep_order_duplicates_and_ignored_limit_without_local_bound_filtering() {
    let mut engine = engine();
    let actions = query(&mut engine, 7, 100.);
    let (token, _) = preparation(&actions);
    let packet = release(&mut engine, token, 101.);
    let body = r#"[{"start":"2","open":"2","high":"3","low":"1","close":"2","volume":"-3"},{"start":"1","open":0,"high":0,"low":0,"close":0,"volume":0},{"start":"1","open":1,"high":2,"low":0,"close":1,"volume":4}]"#;
    let actions = response(&mut engine, &packet, 102., body);
    let results: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandDone { command: 7, result }) => {
                Some(serde_json::from_str::<serde_json::Value>(result).unwrap())
            }
            _ => None,
        })
        .collect();
    assert_eq!(results.len(), 1);
    let rows = results[0].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows.iter()
            .map(|row| row["timestamp"].as_f64().unwrap())
            .collect::<Vec<_>>(),
        vec![2000., 1000., 1000.]
    );
    assert_eq!(
        rows.iter()
            .map(|row| row["volume"].as_f64().unwrap())
            .collect::<Vec<_>>(),
        vec![-3., 0., 4.]
    );
    assert_eq!(rows[1]["open"], 0.);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, .. })))
    );
}

#[test]
fn malformed_row_discards_valid_prefix_with_the_existing_native_diagnostic() {
    let mut engine = engine();
    let actions = query(&mut engine, 7, 100.);
    let (token, _) = preparation(&actions);
    let packet = release(&mut engine, token, 101.);
    let actions = response(
        &mut engine,
        &packet,
        102.,
        r#"[{"start":"1","open":1,"high":2,"low":0,"close":1,"volume":3},{"start":"2","open":"bad","high":2,"low":0,"close":1,"volume":3}]"#,
    );
    let error = failure(&actions, 7);
    assert_eq!(error.message, "Backpack invalid candles");
    assert_eq!(error.kind, None);
    assert_eq!(error.outcome, None);
    assert_eq!(actions.iter().filter(|a| matches!(a,Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::STREAM_MALFORMED)).count(),1);
    no_new_work(&actions);
}

#[test]
fn expired_stream_cannot_parse_malformed_rows_or_publish_a_prefix() {
    let mut engine = engine();
    let initial = query(&mut engine, 7, 100.);
    let (token, _) = preparation(&initial);
    let packet = release(&mut engine, token, 101.);
    engine.handle(
        102.,
        Event::HttpStart {
            request: packet.id,
            status: 200,
            headers: "",
        },
    );
    engine.handle(
        20_100.,
        Event::HttpChunk {
            request: packet.id,
            data: br#"[{"start":"1","open":"bad"}]"#,
        },
    );
    let mut actions: Vec<_> = engine.drain_actions().collect();
    engine.handle(
        20_100.,
        Event::TimerFired {
            timer: deadline(&initial),
        },
    );
    actions.extend(engine.drain_actions());
    assert_eq!(failure(&actions, 7).kind, Some(ErrorKind::Timeout));
    assert!(!actions.iter().any(|a| matches!(a,Action::Notice(Notice::Event(event)) if event.code==safe_cex_engine::log::code::STREAM_MALFORMED)));
    no_new_work(&actions);
    engine.handle(20_101., Event::HttpEnd { request: packet.id });
    assert!(engine.drain_actions().next().is_none());
}

#[test]
fn non_success_status_never_completes_its_valid_stream_prefix() {
    let mut engine = engine();
    let actions = query(&mut engine, 7, 100.);
    let (token, _) = preparation(&actions);
    let packet = release(&mut engine, token, 101.);
    engine.handle(
        102.,
        Event::HttpResponse {
            request: packet.id,
            response: Response {
                status: 429,
                headers: "",
                body: r#"[{"start":"1","open":1,"high":2,"low":0,"close":1,"volume":3}]"#,
            },
        },
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    let error = failure(&actions, 7);
    assert!(error.message.starts_with("Backpack HTTP 429:"));
    assert_eq!(error.kind, None);
    assert_eq!(error.outcome, None);
    no_new_work(&actions);
}
