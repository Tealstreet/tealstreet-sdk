mod support;
use safe_cex_engine::model::Interval;
use safe_cex_engine::{Action, Command, ErrorKind, Event, Notice, Response};
use safe_cex_venue_coinbase::{Coinbase, Options};

fn fail_catalogue(mode: u8) {
    let mut engine = safe_cex_engine::test_support::seeded_engine(
        Coinbase::new(Options {
            public_data_only: true,
            spot_only: mode >= 4,
            ..Default::default()
        })
        .unwrap(),
    );
    let symbol = if mode >= 4 {
        "BTC-USD"
    } else {
        "BTC-PERP-INTX"
    };
    engine.start(0.);
    let catalogue = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .unwrap();
    for command in [7, 8] {
        engine.handle(
            10.,
            Event::Command(Command::FetchCandles {
                command,
                symbol: symbol.into(),
                interval: Interval::M1,
                limit: Some(1),
                from_ms: None,
                to_ms: None,
            }),
        );
    }
    engine.handle(
        10.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: symbol.into(),
        }),
    );
    assert!(!engine.drain_actions().any(|action| matches!(
        action,
        Action::Http(_) | Action::Notice(Notice::CommandFailed { .. } | Notice::CommandDone { .. })
    )));
    let (status, body) = match mode {
        1 | 4 => (503, "{}"),
        5 => (200, r#"{"products":[],"pagination":{"has_next":false}}"#),
        2 => (200, "[]"),
        _ => (200, "not-json"),
    };
    if mode == 0 {
        engine.handle(
            100.,
            Event::HttpFailed {
                request: catalogue.id,
                error: "catalogue reset",
            },
        );
    } else {
        engine.handle(
            100.,
            Event::HttpResponse {
                request: catalogue.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
    }
    let actions: Vec<_> = engine.drain_actions().collect();
    for command in [7, 8] {
        let errors: Vec<_> = actions
            .iter()
            .filter_map(|action| match action {
                Action::Notice(Notice::CommandFailed { command: id, error }) if *id == command => {
                    Some(error)
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            errors.len(),
            1,
            "each deferred candle must settle on catalogue failure"
        );
        assert_eq!(errors[0].kind, Some(ErrorKind::Internal));
        assert_eq!(errors[0].message, "Coinbase public catalogue unavailable");
        assert!(
            errors[0]
                .venue_message
                .as_deref()
                .is_some_and(|message| !message.is_empty())
        );
        assert_eq!(errors[0].outcome, None);
    }
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Notice(Notice::CommandDone { .. })))
    );
    let timer = actions
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms >= 850. && *at_ms <= 1100. => Some(*timer),
            _ => None,
        })
        .expect("existing discovery backoff is retained");
    engine.handle(1200., Event::TimerFired { timer });
    let retry = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .unwrap();
    assert_eq!(retry.url, catalogue.url);
    engine.handle(
        1201.,
        Event::HttpResponse {
            request: retry.id,
            response: Response {
                status: 200,
                headers: "",
                body: if mode >= 4 { r#"{"products":[{"product_type":"SPOT","product_id":"BTC-USD","base_currency_id":"BTC","quote_currency_id":"USD","status":"TRADING","base_increment":"0.001","price_increment":"0.01","base_min_size":"0.001","base_max_size":"100","price":"100","volume_24h":"1","price_percentage_change_24h":"0"}],"pagination":{"has_next":false}}"# } else { support::INSTRUMENTS },
            },
        },
    );
    let recovered: Vec<_> = engine.drain_actions().collect();
    assert!(engine.store().loaded().markets);
    assert!(!recovered.iter().any(|action| matches!(
        action,
        Action::Notice(
            Notice::CommandDone { command: 7 | 8, .. }
                | Notice::CommandFailed { command: 7 | 8, .. }
        )
    )));
    assert!(
        !recovered.iter().any(
            |action| matches!(action, Action::Http(request) if request.url.contains("/candles"))
        )
    );
    let socket = recovered
        .iter()
        .find_map(|action| match action {
            Action::WsConnect { socket, .. } => Some(*socket),
            _ => None,
        })
        .unwrap();
    engine.handle(1202., Event::WsOpened { socket });
    assert!(
        engine
            .drain_actions()
            .any(|action| matches!(action, Action::WsSend { text, .. } if text.contains("level2"))),
        "subscription intent survives catalogue recovery"
    );
}

#[test]
fn transport_failure_settles_deferred_candles_once() {
    fail_catalogue(0);
}
#[test]
fn http_failure_settles_deferred_candles_once() {
    fail_catalogue(1);
}
#[test]
fn empty_catalogue_settles_deferred_candles_once() {
    fail_catalogue(2);
}
#[test]
fn malformed_catalogue_settles_deferred_candles_once() {
    fail_catalogue(3);
}

#[test]
fn spot_product_failure_settles_deferred_candles_once() {
    fail_catalogue(4);
}
#[test]
fn empty_spot_catalogue_settles_deferred_candles_once() {
    fail_catalogue(5);
}
