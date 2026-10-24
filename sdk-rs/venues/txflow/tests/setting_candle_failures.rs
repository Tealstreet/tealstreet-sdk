//! Account-setting progression and candle failure custody in one genuine engine.
mod support;
use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{Error, HttpAction};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Notice, Outcome, Response, VenueError,
};
use safe_cex_venue_txflow::{Options, TxFlow};

type Driver = Engine<TxFlow>;
const NOW: f64 = 1050.;
fn is_setting(request: &HttpRequest) -> bool {
    request.body.as_deref().unwrap().contains("activeAssetData")
}
fn is_candle(request: &HttpRequest) -> bool {
    request.body.as_deref().unwrap().contains("candleSnapshot")
}
fn reply(engine: &mut Driver, request: &HttpRequest, now: f64, body: &str) {
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
}
fn pending_pair() -> Driver {
    let mut engine = safe_cex_engine::test_support::seeded_engine(TxFlow::new(Options {
        testnet: true,
        application_id: Some("0x0000000000000000000000000000000000000001".into()),
        secret: Some(format!("0x{:064x}", 1)),
        ..Default::default()
    }));
    engine.start(1000.);
    let metadata = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .unwrap();
    let mut body: serde_json::Value = serde_json::from_str(support::META).unwrap();
    let mut second = body["data"]["universe"][0].clone();
    second["index"] = 2.into();
    second["name"] = "ETH-USDC".into();
    second["baseCurrency"] = "ETH".into();
    body["data"]["universe"]
        .as_array_mut()
        .unwrap()
        .push(second);
    reply(&mut engine, &metadata, 1000., &body.to_string());
    for (now, asset) in [(1000., 1), (1025., 2)] {
        let ticker = safe_cex_engine::test_support::prepared_actions(&mut engine, now)
            .into_iter()
            .find_map(|action| match action {
                Action::Http(request)
                    if request.body.as_deref().unwrap().contains("activeAssetCtx") =>
                {
                    Some(request)
                }
                _ => None,
            })
            .unwrap();
        reply(
            &mut engine,
            &ticker,
            now,
            &support::TICKER.replace("\"instrumentId\":1", &format!("\"instrumentId\":{asset}")),
        );
        let cadence = engine
            .drain_actions()
            .find_map(|action| match action {
                Action::SetTimer { timer, at_ms } if at_ms == now + 25. => Some(timer),
                _ => None,
            })
            .unwrap();
        engine.handle(now + 25., Event::TimerFired { timer: cadence });
    }
    assert!(engine.store().loaded().tickers);
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
fn assert_failure_and_next_setting(engine: &mut Driver, kind: ErrorKind, outcome: Option<Outcome>) {
    let actions = engine.drain_actions().collect::<Vec<_>>();
    let errors = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].message, "candle refused");
    assert_eq!(errors[0].kind, Some(kind));
    assert_eq!(errors[0].outcome, outcome);
    assert!(!actions.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone { command: 7, .. }) | Action::Http(_)
    )));
    assert!(!actions.iter().any(|action| matches!(action,
        Action::ManagedHttp(action) if !matches!(action.as_ref(), HttpAction::Cancel { .. }))));
    let timers = actions
        .iter()
        .filter_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 100. => Some(*timer),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        timers.len(),
        1,
        "setting failure alone advances the native queue once"
    );
    engine.handle(NOW + 100., Event::TimerFired { timer: timers[0] });
    let actions = safe_cex_engine::test_support::prepared_actions(engine, NOW + 100.);
    let requests = actions
        .iter()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        requests.len(),
        1,
        "no candle retry or repeated first setting"
    );
    assert!(is_setting(requests[0]));
    let body: serde_json::Value =
        serde_json::from_str(requests[0].body.as_deref().unwrap()).unwrap();
    assert_eq!(body["coin"], "2");
    assert!(requests[0].route.is_none());
    assert!(!actions.iter().any(|action| matches!(
        action,
        Action::Notice(
            Notice::CommandDone { command: 7, .. } | Notice::CommandFailed { command: 7, .. }
        )
    )));
}
#[test]
fn paired_setting_candle_preparation_refusals_keep_both_cleanup_paths() {
    let mut engine = pending_pair();
    let packets = engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } if is_setting(&request) || is_candle(&request) => Some((preparation, request)),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(packets.len(), 2);
    assert_eq!(
        packets
            .iter()
            .filter(|(_, request)| is_setting(request))
            .count(),
        1
    );
    for (preparation, request) in packets {
        engine.http_preparation_failed(
            NOW,
            preparation,
            Error::Invalid,
            VenueError::new(if is_setting(&request) {
                "setting refused"
            } else {
                "candle refused"
            }),
        );
    }
    assert_failure_and_next_setting(&mut engine, ErrorKind::InvalidInput, Some(Outcome::NotSent));
}
#[test]
fn paired_setting_candle_released_failures_keep_both_cleanup_paths() {
    let mut engine = pending_pair();
    let requests = safe_cex_engine::test_support::prepared_actions(&mut engine, NOW)
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(request) if is_setting(&request) || is_candle(&request) => Some(request),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests
            .iter()
            .filter(|request| is_setting(request))
            .count(),
        1
    );
    for request in requests {
        engine.handle(
            NOW,
            Event::HttpFailed {
                request: request.id,
                error: if is_setting(&request) {
                    "setting refused"
                } else {
                    "candle refused"
                },
            },
        );
    }
    assert_failure_and_next_setting(&mut engine, ErrorKind::Network, None);
}
