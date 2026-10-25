//! Scripted discovery regressions; no network or account credentials.
use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{Action, Engine, Event, Response};
use safe_cex_venue_orderly::{Options, Orderly};

/// A jittered retry delay lands in `[0.75, 1.0] * nominal`.
fn jittered(actual: f64, nominal: f64) -> bool {
    (0.75 * nominal..=nominal).contains(&actual)
}
const CATALOGUE: &str = r###"{"success":true,"data":{"rows":[{"broken":true},{"symbol":"PERP_BTC_USDC","base_tick":0.001,"quote_tick":0.1,"base_min":0.001,"base_max":20,"base_imr":0.01},{"symbol":"PERP_ETH_USDC","base_tick":0.001,"quote_tick":0.01,"base_min":0.001,"base_max":100,"base_imr":0.02}]}}"###;
fn public_engine() -> Engine<Orderly> {
    safe_cex_engine::test_support::seeded_engine(Orderly::new(Options {
        public_data_only: true,
        ..Default::default()
    }))
}
#[test]
fn catalogue_network_failures_back_off_then_keep_valid_rows() {
    let mut engine = public_engine();
    let mut now = 0.0;
    engine.start(0.0);
    let mut request = prepared_actions(&mut engine, now)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("/public/info") => Some(request),
            _ => None,
        })
        .expect("catalogue request");
    for delay in [1000.0, 2000.0, 4000.0] {
        engine.handle(
            now,
            Event::HttpFailed {
                request: request.id,
                error: "scripted network failure",
            },
        );
        let actions = prepared_actions(&mut engine, now);
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
        request = prepared_actions(&mut engine, now)
            .into_iter()
            .find_map(|action| match action {
                Action::Http(request) if request.url.contains("/public/info") => Some(request),
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
    assert!(prepared_actions(&mut engine, now).into_iter().any(|action| matches!(action,
        Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains("Skipped malformed public discovery row"))));
}

#[test]
fn authenticated_discovery_starts_private_reads_before_tickers() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Orderly::new(Options {
        key: Some("ed25519:11111111111111111111111111111111".into()),
        secret: Some(bs58::encode([7; 32]).into_string()),
        application_id: Some("scripted-account".into()),
        ..Default::default()
    }));
    engine.start(0.0);
    let info = prepared_actions(&mut engine, 0.0)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    engine.handle(
        0.0,
        Event::HttpResponse {
            request: info.id,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"success":true,"data":{"max_leverage":20}}"#,
            },
        },
    );
    let catalogue = prepared_actions(&mut engine, 0.0)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with("/public/info") => Some(r),
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
    assert!(
        prepared_actions(&mut engine, 0.0)
            .into_iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("/client/margin_modes")))
    );
}

#[test]
fn ticker_http_failure_retries_without_reloading_the_catalogue() {
    let mut engine = public_engine();
    engine.start(0.0);
    let catalogue = prepared_actions(&mut engine, 0.0)
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
    let ticker = prepared_actions(&mut engine, 0.0)
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
    let timer = prepared_actions(&mut engine, 0.0)
        .into_iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if jittered(at_ms, 1000.0) => Some(timer),
            _ => None,
        })
        .expect("failed public ticker read uses shared retry backoff");
    assert!(engine.store().loaded().markets);
    engine.handle(1000.0, Event::TimerFired { timer });
    let retry = prepared_actions(&mut engine, 1000.0)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url == ticker.url => Some(r),
            _ => None,
        })
        .expect("ticker source retried independently");
    assert_ne!(retry.id, ticker.id);
}

#[test]
fn unusable_ticker_lists_preserve_existing_quotes_retry_and_recover_valid_peers() {
    for body in [
        r###"{"success":true,"data":{"rows":[{"broken":true}]}}"###,
        r###"{"success":true,"data":{"rows":[]}}"###,
    ] {
        for cached in [false, true] {
            let mut engine = public_engine();
            engine.start(0.0);
            let catalogue = prepared_actions(&mut engine, 0.0)
                .into_iter()
                .find_map(|action| match action {
                    Action::Http(request) => Some(request),
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
            let ticker = prepared_actions(&mut engine, 0.0)
                .into_iter()
                .find_map(|action| match action {
                    Action::Http(request) if request.url.contains("/public/futures") => {
                        Some(request)
                    }
                    _ => None,
                })
                .expect("public ticker seed");
            let market = engine.store().markets()[0].clone();
            if cached {
                engine
                    .store_mut()
                    .set_tickers(vec![safe_cex_engine::model::Ticker {
                        id: market.id.clone(),
                        symbol: market.symbol.clone(),
                        last: 321.0,
                        ..Default::default()
                    }]);
            }
            let before = engine.store().tickers().to_vec();
            engine.handle(
                0.0,
                Event::HttpResponse {
                    request: ticker.id,
                    response: Response {
                        status: 200,
                        headers: "",
                        body,
                    },
                },
            );
            assert_eq!(engine.store().tickers(), before);
            assert_eq!(engine.store().loaded().tickers, cached);
            let timer = prepared_actions(&mut engine, 0.0)
                .into_iter()
                .find_map(|action| match action {
                    Action::SetTimer { timer, at_ms } if jittered(at_ms, 1000.0) => Some(timer),
                    _ => None,
                })
                .expect("unusable successful HTTP body must back off");
            engine.handle(1000.0, Event::TimerFired { timer });
            let retry = prepared_actions(&mut engine, 1000.0)
                .into_iter()
                .find_map(|action| match action {
                    Action::Http(request) if request.url == ticker.url => Some(request),
                    _ => None,
                })
                .expect("fresh ticker-only retry");
            engine.handle(1000.0, Event::HttpResponse { request: retry.id,
                response: Response { status: 200, headers: "", body: r###"{"success":true,"data":{"rows":[{"broken":true},{"symbol":"PERP_BTC_USDC","24h_open":90,"24h_close":100}]}}"### } });
            assert!(engine.store().loaded().tickers);
            assert_eq!(engine.store().tickers().len(), 1);
            assert_eq!(engine.store().tickers()[0].last, 100.0);
        }
    }
}
