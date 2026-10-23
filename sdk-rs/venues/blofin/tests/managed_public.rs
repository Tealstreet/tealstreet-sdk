use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{Error, HttpAction, Preparation, RoutingIntent};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, Notice, Outcome, RequestId, Response, VenueError,
};
use safe_cex_venue_blofin::{Blofin, Options};

const INSTRUMENT: &str = r#"{"code":"0","data":[{"instId":"BTC-USDT","instType":"SWAP","contractType":"linear","baseCurrency":"BTC","quoteCurrency":"USDT","state":"live","contractValue":"0.001","minSize":"0.1","lotSize":"0.1","tickSize":"0.1","maxLimitSize":"1000","maxMarketSize":"500","maxLeverage":"150"}]}"#;

fn engine() -> Engine<Blofin> {
    safe_cex_engine::test_support::seeded_engine(Blofin::new(Options {
        public_data_only: true,
        ..Options::default()
    }))
}

fn reply(e: &mut Engine<Blofin>, now: f64, request: RequestId, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}

fn preparation(actions: &[Action]) -> Preparation {
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_))),
        "public HTTP must wait for prepared transport admission"
    );
    actions
        .iter()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    intent,
                    request,
                } => {
                    assert_eq!(*intent, RoutingIntent::Public);
                    assert!(request.headers.is_empty());
                    assert!(request.signer_headers.is_empty());
                    Some(*preparation)
                }
                _ => None,
            },
            _ => None,
        })
        .expect("a public managed preparation")
}

fn ready() -> Engine<Blofin> {
    let mut e = engine();
    e.start(0.);
    for body in [INSTRUMENT, r#"{"code":"0","data":[]}"#] {
        let request = safe_cex_engine::test_support::prepared_actions(&mut e, 0.)
            .into_iter()
            .find_map(|action| match action {
                Action::Http(request) => Some(request.id),
                _ => None,
            })
            .expect("startup HTTP");
        reply(&mut e, 0., request, body);
    }
    e.drain_actions().for_each(drop);
    e
}

fn candles(e: &mut Engine<Blofin>, now: f64, command: u32, symbol: &str, interval: Interval) {
    e.handle(
        now,
        Event::Command(Command::FetchCandles {
            command,
            symbol: symbol.into(),
            interval,
            limit: Some(2),
            from_ms: None,
            to_ms: None,
        }),
    );
}

#[test]
fn discovery_waits_for_preparation_and_ignores_unreleased_response() {
    let mut e = engine();
    e.start(100.);
    let actions = e.drain_actions().collect::<Vec<_>>();
    let token = preparation(&actions);
    reply(&mut e, 101., token.attempt().request, INSTRUMENT);
    assert!(e.store().markets().is_empty());
    assert!(!e.store().loaded().markets);
    assert!(
        !e.drain_actions()
            .any(|action| matches!(action, Action::Http(_)))
    );
}

#[test]
fn unclaimed_catalogue_response_cannot_change_the_store() {
    let mut e = engine();
    e.start(100.);
    let request = e
        .drain_actions()
        .find_map(|action| match action {
            Action::Http(request) => Some(request.id),
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation.attempt().request),
                _ => None,
            },
            _ => None,
        })
        .expect("catalogue request identity");
    reply(&mut e, 101., request, INSTRUMENT);
    assert!(
        e.store().markets().is_empty(),
        "an identity is not a release permit"
    );
    assert!(!e.store().loaded().markets);
}

#[test]
fn parked_candle_preparation_expires_and_late_success_cannot_publish() {
    let mut e = ready();
    candles(&mut e, 100., 71, "BTCUSDT", Interval::M1);
    let actions = e.drain_actions().collect::<Vec<_>>();
    let token = preparation(&actions);
    let timer = actions
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == 20_100. => Some(*timer),
            _ => None,
        })
        .expect("original preparation-inclusive deadline");
    e.handle(20_100., Event::TimerFired { timer });
    let failures = e
        .drain_actions()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: 71, error }) => Some(error),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].kind, Some(ErrorKind::Timeout));
    assert_eq!(failures[0].outcome, Some(Outcome::NotSent));
    assert!(!e.http_preparation_pending(token));
    reply(
        &mut e,
        20_101.,
        token.attempt().request,
        r#"{"code":"0","data":[[1000,1,2,1,2,4,0,4]]}"#,
    );
    assert!(!e.drain_actions().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone { command: 71, .. })
            | Action::Notice(Notice::CommandFailed { command: 71, .. })
    )));
}

#[test]
fn unsupported_prepared_transport_keeps_typed_candle_failure() {
    let mut e = ready();
    candles(&mut e, 100., 72, "BTCUSDT", Interval::M1);
    let token = preparation(&e.drain_actions().collect::<Vec<_>>());
    e.http_preparation_failed(
        101.,
        token,
        Error::Invalid,
        VenueError::new("prepared HTTP unsupported").kind(ErrorKind::Unsupported),
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
    let failures = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: 72, error }) => Some(error),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].kind, Some(ErrorKind::Unsupported));
    assert_eq!(failures[0].outcome, Some(Outcome::NotSent));
}

#[test]
fn candle_validation_is_typed_without_io_and_keeps_market_precedence() {
    let mut e = ready();
    for (command, symbol, kind, message) in [
        (
            73,
            "MISSING",
            ErrorKind::InvalidInput,
            "Unknown market MISSING",
        ),
        (
            74,
            "BTCUSDT",
            ErrorKind::Unsupported,
            "BloFin does not support 1M candles",
        ),
    ] {
        candles(&mut e, 100., command, symbol, Interval::Mo1);
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::Http(_) | Action::ManagedHttp(_)))
        );
        let error = actions
            .iter()
            .find_map(|action| match action {
                Action::Notice(Notice::CommandFailed { command: id, error }) if *id == command => {
                    Some(error)
                }
                _ => None,
            })
            .expect("one terminal validation failure");
        assert_eq!(error.kind, Some(kind));
        assert_eq!(error.outcome, Some(Outcome::NotSent));
        assert_eq!(error.message, message);
    }
}

#[test]
fn candle_success_keeps_sorting_and_reused_command_rejects_old_response() {
    let mut e = ready();
    candles(&mut e, 100., 75, "BTCUSDT", Interval::M1);
    let first = safe_cex_engine::test_support::prepared_actions(&mut e, 100.)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) => Some(request.id),
            _ => None,
        })
        .expect("released candle request");
    let body = r#"{"code":"0","data":[[2000,2,3,1,2,4,0,5],[1000,1,2,1,2,4,0,6]]}"#;
    reply(&mut e, 101., first, body);
    let results = e
        .drain_actions()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandDone {
                command: 75,
                result,
            }) => Some(result),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(results.len(), 1);
    let rows: serde_json::Value = serde_json::from_str(&results[0]).unwrap();
    assert_eq!(rows[0]["timestamp"], 1.);
    assert_eq!(rows[1]["timestamp"], 2.);
    assert_eq!(rows[0]["volume"], 6.);
    candles(&mut e, 102., 75, "BTCUSDT", Interval::M1);
    let second = safe_cex_engine::test_support::prepared_actions(&mut e, 102.)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) => Some(request.id),
            _ => None,
        })
        .expect("new generation's candle request");
    assert_ne!(first, second);
    reply(&mut e, 103., first, body);
    assert!(!e.drain_actions().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone { command: 75, .. })
            | Action::Notice(Notice::CommandFailed { command: 75, .. })
    )));
    reply(&mut e, 104., second, r#"{"code":"0","data":[]}"#);
    assert_eq!(
        e.drain_actions()
            .filter(|action| matches!(
                action,
                Action::Notice(Notice::CommandDone { command: 75, result }) if result == "[]"
            ))
            .count(),
        1
    );
}
