//! Scripted discovery regressions; no network or account credentials.
use safe_cex_engine::{Action, Engine, Event, Response};
use safe_cex_venue_backpack::{Backpack, Options};

/// Retry delays are jittered into `[0.75, 1.0] * nominal`.
fn jittered(actual: f64, nominal: f64) -> bool {
    (0.75 * nominal..=nominal).contains(&actual)
}
const CATALOGUE: &str = r###"[{"broken":true},{"symbol":"SOL_USDC_PERP","marketType":"PERP","quantityIncrement":"0.01","priceIncrement":"0.01"}]"###;
fn public_engine() -> Engine<Backpack> {
    safe_cex_engine::test_support::seeded_engine(Backpack::new(Options {
        public_data_only: true,
        ..Default::default()
    }))
}
#[test]
fn catalogue_network_failures_back_off_then_keep_valid_rows() {
    let mut engine = public_engine();
    engine.start(0.0);
    let mut request = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.0)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("/markets") => Some(request),
            _ => None,
        })
        .expect("catalogue request");
    let mut now = 0.0;
    for delay in [1000.0, 2000.0, 4000.0] {
        engine.handle(
            now,
            Event::HttpFailed {
                request: request.id,
                error: "scripted network failure",
            },
        );
        let actions = engine.drain_actions().collect::<Vec<_>>();
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::Http(_))),
            "no immediate retry"
        );
        let timer = actions
            .into_iter()
            .find_map(|action| match action {
                Action::SetTimer { timer, at_ms } if jittered(at_ms - now, delay) => Some(timer),
                _ => None,
            })
            .expect("shared exponential retry timer");
        now += delay;
        engine.handle(now, Event::TimerFired { timer });
        request = safe_cex_engine::test_support::prepared_actions(&mut engine, now)
            .into_iter()
            .find_map(|action| match action {
                Action::Http(request) if request.url.contains("/markets") => Some(request),
                _ => None,
            })
            .expect("fresh catalogue request");
    }
    engine.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: CATALOGUE,
            },
        },
    );
    assert!(engine.store().loaded().markets, "valid peers must commit");
    assert!(!engine.store().markets().is_empty());
    assert!(engine.drain_actions().any(|action| matches!(action,
        Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains("Skipped malformed public discovery row"))));
}

#[test]
fn private_snapshot_starts_while_ticker_response_is_still_pending() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Backpack::new(Options {
        key: Some("scripted-key".into()),
        secret: Some("BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc=".into()),
        ..Default::default()
    }));
    engine.start(0.0);
    let catalogue = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.0)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("/markets") => Some(request),
            _ => None,
        })
        .unwrap();
    engine.handle(
        0.0,
        Event::HttpResponse {
            request: catalogue.id,
            response: Response {
                status: 200,
                headers: "",
                body: CATALOGUE,
            },
        },
    );
    assert!(engine.store().loaded().markets);
    assert!(!engine.store().loaded().tickers);
    let reads = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.0)
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        reads.iter().any(|request| [
            "account",
            "position",
            "private/",
            "user/",
            "listenKey",
            "collateral"
        ]
        .iter()
        .any(|part| request
            .url
            .to_ascii_lowercase()
            .contains(&part.to_ascii_lowercase()))),
        "private REST reads must not wait for ticker success or a socket welcome: {:?}",
        reads.iter().map(|r| &r.url).collect::<Vec<_>>()
    );
}

#[test]
fn ticker_http_failure_retries_without_reloading_the_catalogue() {
    let mut engine = public_engine();
    engine.start(0.0);
    let catalogue = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.0)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    engine.handle(
        0.0,
        Event::HttpResponse {
            request: catalogue.id,
            response: Response {
                status: 200,
                headers: "",
                body: CATALOGUE,
            },
        },
    );
    let ticker = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.0)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    engine.handle(
        0.0,
        Event::HttpResponse {
            request: ticker.id,
            response: Response {
                status: 503,
                headers: "",
                body: "{}",
            },
        },
    );
    let timer = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if jittered(at_ms, 1000.0) => Some(timer),
            _ => None,
        })
        .expect("failed public ticker read uses shared retry backoff");
    assert!(engine.store().loaded().markets);
    engine.handle(1000.0, Event::TimerFired { timer });
    let retry = safe_cex_engine::test_support::prepared_actions(&mut engine, 1000.0)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url == ticker.url => Some(r),
            _ => None,
        })
        .expect("ticker source retried independently");
    assert_ne!(retry.id, ticker.id);
}

#[test]
fn entirely_malformed_ticker_source_retries_without_publishing_an_empty_seed() {
    let mut engine = public_engine();
    engine.start(0.0);
    let catalogue = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.0)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    engine.handle(
        0.0,
        Event::HttpResponse {
            request: catalogue.id,
            response: Response {
                status: 200,
                headers: "",
                body: CATALOGUE,
            },
        },
    );
    let ticker = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.0)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    engine.handle(
        0.0,
        Event::HttpResponse {
            request: ticker.id,
            response: Response {
                status: 200,
                headers: "",
                body: r###"[{"broken":true}]"###,
            },
        },
    );
    let timer = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if jittered(at_ms, 1000.0) => Some(timer),
            _ => None,
        })
        .expect("failed public ticker read uses shared retry backoff");
    assert!(engine.store().loaded().markets);
    assert!(!engine.store().loaded().tickers);
    engine.handle(1000.0, Event::TimerFired { timer });
    let retry = safe_cex_engine::test_support::prepared_actions(&mut engine, 1000.0)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url == ticker.url => Some(r),
            _ => None,
        })
        .expect("ticker source retried independently");
    assert_ne!(retry.id, ticker.id);
}

#[test]
fn late_ticker_seed_preserves_supplied_live_fields_and_uses_one_public_session() {
    for live_trade in [false, true] {
        let mut engine = public_engine();
        engine.start(0.0);
        let catalogue = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.0)
            .into_iter()
            .find_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .unwrap();
        engine.handle(
            0.0,
            Event::HttpResponse {
                request: catalogue.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: CATALOGUE,
                },
            },
        );
        let early = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.0);
        assert_eq!(
            early
                .iter()
                .filter(|a| matches!(a, Action::WsConnect { .. }))
                .count(),
            1
        );
        let socket = early
            .iter()
            .find_map(|a| match a {
                Action::WsConnect { socket, .. } => Some(*socket),
                _ => None,
            })
            .unwrap();
        engine.handle(0.0, Event::WsOpened { socket });
        engine.handle(1.0, Event::WsMessage { socket, text: r#"{"stream":"bookTicker.SOL_USDC_PERP","data":{"s":"SOL_USDC_PERP","b":"120","a":"121"}}"# });
        if live_trade {
            engine.handle(1.0, Event::WsMessage { socket, text: r#"{"stream":"trade.SOL_USDC_PERP","data":{"s":"SOL_USDC_PERP","p":"200"}}"# });
        }
        assert!(!engine.store().loaded().tickers);
        for action in early {
            if let Action::Http(request) = action {
                let body = if request.url.contains("/tickers") {
                    r#"[{"symbol":"SOL_USDC_PERP","lastPrice":"100","priceChangePercent":"0.05","volume":"10","quoteVolume":"1000"}]"#
                } else {
                    r#"[{"symbol":"SOL_USDC_PERP","markPrice":"99","indexPrice":"98","fundingRate":"0.001","nextFundingTimestamp":1000}]"#
                };
                engine.handle(
                    2.0,
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
        }
        let ticker = engine.store().ticker("SOL_USDC_PERP").unwrap();
        assert_eq!((ticker.bid, ticker.ask, ticker.mark), (120.0, 121.0, 99.0));
        assert_eq!(ticker.last, if live_trade { 200.0 } else { 100.0 });
        assert!(engine.store().loaded().tickers);
        assert!(
            !engine
                .drain_actions()
                .any(|a| matches!(a, Action::WsConnect { .. }))
        );
    }
}
