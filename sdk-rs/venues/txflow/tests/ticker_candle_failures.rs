//! Ticker recovery and typed candle failures share the same genuine engine.
mod support;

use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{Error, HttpAction};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Notice, Outcome, TimerId, VenueError,
};
use safe_cex_venue_txflow::{Options, TxFlow};

type Driver = Engine<TxFlow>;
const NOW: f64 = 1000.;

fn with_pending_ticker_and_candle() -> Driver {
    let mut engine = safe_cex_engine::test_support::seeded_engine(TxFlow::new(Options {
        testnet: true,
        public_data_only: true,
        ..Default::default()
    }));
    engine.start(NOW);
    let metadata = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .expect("native metadata request");
    support::reply(&mut engine, &metadata, 200, support::META);
    engine
        .handle_command(
            NOW,
            Command::FetchCandles {
                command: 7,
                symbol: "BTC-USDC".into(),
                interval: Interval::M1,
                limit: Some(2),
                from_ms: Some(0.),
                to_ms: Some(120_000.),
            },
            None,
        )
        .unwrap();
    engine
}

fn assert_failures(
    actions: &[Action],
    kind: ErrorKind,
    outcome: Option<Outcome>,
) -> (TimerId, f64) {
    let failures = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].message, "candle refused");
    assert_eq!(failures[0].kind, Some(kind));
    assert_eq!(failures[0].outcome, outcome);
    assert!(!actions.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone { command: 7, .. }) | Action::Http(_)
    )));
    assert!(!actions.iter().any(|action| matches!(action,
        Action::ManagedHttp(action) if !matches!(action.as_ref(), HttpAction::Cancel { .. }))));
    let next = actions
        .iter()
        .filter(|action| matches!(action, Action::SetTimer { at_ms, .. } if *at_ms == NOW + 25.))
        .count();
    assert_eq!(
        next, 1,
        "only ticker failure schedules the native continuation"
    );
    let retry = actions
        .iter()
        .filter_map(|action| match action {
            Action::SetTimer { timer, at_ms } if support::jittered(*at_ms - NOW, 1000.) => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(retry.len(), 1, "only the failed ticker key backs off");
    retry[0]
}

fn recover_ticker(engine: &mut Driver, retry: (TimerId, f64)) {
    engine.handle(retry.1, Event::TimerFired { timer: retry.0 });
    let requests = safe_cex_engine::test_support::prepared_actions(engine, retry.1);
    let requests = requests
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect::<Vec<HttpRequest>>();
    assert_eq!(
        requests.len(),
        1,
        "ticker retries; the failed candle never retries"
    );
    assert!(
        requests[0]
            .body
            .as_deref()
            .unwrap()
            .contains("activeAssetCtx")
    );
    engine.handle(
        retry.1,
        Event::HttpResponse {
            request: requests[0].id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body: support::TICKER,
            },
        },
    );
    let next = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if at_ms == retry.1 + 25. => Some(timer),
            _ => None,
        })
        .expect("successful retry retains the native continuation");
    engine.handle(retry.1 + 25., Event::TimerFired { timer: next });
    assert!(engine.store().loaded().tickers);
    assert_eq!(engine.store().ticker("BTC-USDC").unwrap().mark, 65001.);
    assert!(!engine.drain_actions().any(|action| matches!(
        action,
        Action::Notice(
            Notice::CommandDone { command: 7, .. } | Notice::CommandFailed { command: 7, .. }
        )
    )));
}

#[test]
fn mixed_preparation_refusals_preserve_ticker_recovery_and_typed_candle_failure() {
    let mut engine = with_pending_ticker_and_candle();
    let packets = engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => Some((preparation, request)),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(packets.len(), 2);
    for (preparation, request) in &packets {
        let message = if request.body.as_deref().unwrap().contains("activeAssetCtx") {
            "ticker refused"
        } else {
            assert!(request.body.as_deref().unwrap().contains("candleSnapshot"));
            "candle refused"
        };
        engine.http_preparation_failed(NOW, *preparation, Error::Invalid, VenueError::new(message));
    }
    let actions = engine.drain_actions().collect::<Vec<_>>();
    let retry = assert_failures(&actions, ErrorKind::InvalidInput, Some(Outcome::NotSent));
    for (preparation, _) in packets {
        engine.http_preparation_failed(
            NOW,
            preparation,
            Error::Invalid,
            VenueError::new("duplicate"),
        );
    }
    assert!(
        engine.drain_actions().next().is_none(),
        "duplicate refusals are inert"
    );
    recover_ticker(&mut engine, retry);
}

#[test]
fn mixed_released_failures_preserve_ticker_recovery_and_typed_candle_failure() {
    let mut engine = with_pending_ticker_and_candle();
    let requests = safe_cex_engine::test_support::prepared_actions(&mut engine, NOW)
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        let error = if request.body.as_deref().unwrap().contains("activeAssetCtx") {
            "ticker refused"
        } else {
            assert!(request.body.as_deref().unwrap().contains("candleSnapshot"));
            "candle refused"
        };
        engine.handle(
            NOW,
            Event::HttpFailed {
                request: request.id,
                error,
            },
        );
    }
    let actions = engine.drain_actions().collect::<Vec<_>>();
    let retry = assert_failures(&actions, ErrorKind::Network, None);
    for request in requests {
        engine.handle(
            NOW,
            Event::HttpFailed {
                request: request.id,
                error: "duplicate",
            },
        );
    }
    assert!(
        engine.drain_actions().next().is_none(),
        "duplicate failures are inert"
    );
    recover_ticker(&mut engine, retry);
}
