//! Scripted discovery regressions; no network or account credentials.
mod support;
use safe_cex_engine::{Action, Engine, Event, Response};
use safe_cex_venue_woo::{Options, Woo};
use support::jittered;

const CATALOGUE: &str = r###"{"success":true,"rows":[{"broken":true},{"symbol":"SPOT_BTC_USDT"},{"symbol":"PERP_BTC_USDT","base_tick":0.001,"quote_tick":0.1,"base_min":0.001,"base_max":100,"base_imr":0.02,"price_range":0.01}]}"###;
fn public_engine() -> Engine<Woo> {
    safe_cex_engine::test_support::seeded_engine(Woo::new(Options {
        public_data_only: true,
        ..Default::default()
    }))
}
#[test]
fn catalogue_network_failures_back_off_then_keep_valid_rows() {
    let mut engine = public_engine();
    engine.start(0.0);
    let mut request = support::prepared_seed_actions(&mut engine, 0.0)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("/public/info") => Some(request),
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
        request = support::prepared_seed_actions(&mut engine, now)
            .into_iter()
            .find_map(|action| match action {
                Action::Http(request) if request.url.contains("/public/info") => Some(request),
                _ => None,
            })
            .expect("fresh catalogue request");
    }
    // Skipped discovery rows log at Debug.
    engine.set_log_level(safe_cex_engine::Level::Debug);
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
    let mut engine = safe_cex_engine::test_support::seeded_engine(Woo::new(Options {
        key: Some("scripted-key".into()),
        secret: Some("scripted-secret".into()),
        application_id: Some("scripted-app".into()),
        ..Default::default()
    }));
    engine.start(0.0);
    let catalogue = support::prepared_seed_actions(&mut engine, 0.0)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("/public/info") => Some(request),
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
    let reads = support::prepared_seed_actions(&mut engine, 0.0)
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
    let catalogue = support::prepared_seed_actions(&mut engine, 0.0)
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
    let ticker = support::prepared_seed_actions(&mut engine, 0.0)
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
    let retry = support::prepared_seed_actions(&mut engine, 1000.0)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url == ticker.url => Some(r),
            _ => None,
        })
        .expect("ticker source retried independently");
    assert_ne!(retry.id, ticker.id);
}

#[test]
fn live_ticker_never_creates_a_catalogue_market_missing_from_the_rest_seed() {
    let mut engine = public_engine();
    engine.start(0.0);
    let catalogue = support::prepared_seed_actions(&mut engine, 0.0)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    engine.handle(0.0, Event::HttpResponse { request: catalogue.id, response: Response { status: 200, headers: "", body: r###"{"success":true,"rows":[{"broken":true},{"symbol":"SPOT_BTC_USDT"},{"symbol":"PERP_BTC_USDT","base_tick":0.001,"quote_tick":0.1,"base_min":0.001,"base_max":100,"base_imr":0.02,"price_range":0.01},{"symbol":"PERP_ETH_USDT","base_tick":0.001,"quote_tick":0.1,"base_min":0.001,"base_max":100,"base_imr":0.02,"price_range":0.01}]}"### }});
    let mut actions = support::prepared_seed_actions(&mut engine, 0.0);
    let ticker = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r.clone()),
            _ => None,
        })
        .unwrap();
    engine.handle(0.0, Event::HttpResponse { request: ticker.id, response: Response { status: 200, headers: "", body: r###"{"success":true,"rows":[{"symbol":"PERP_UNKNOWN_USDT"},{"symbol":"PERP_BTC_USDT","24h_open":50000,"24h_close":60000,"24h_volume":4,"24h_amount":240000,"index_price":59999,"mark_price":60001,"last_funding_rate":0.0001,"next_funding_time":1700028800000,"open_interest":10000}]}"### }});
    actions.extend(engine.drain_actions());

    assert!(engine.store().market("ETHUSDT").is_some());
    assert!(engine.store().ticker("ETHUSDT").is_none());
    let socket = actions
        .into_iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .unwrap();
    engine.handle(0.0, Event::WsOpened { socket });
    engine.drain_actions().for_each(drop);
    engine.handle(1.0, Event::WsMessage { socket, text: r###"{"topic":"tickers","data":[{"symbol":"PERP_ETH_USDT","close":201},{"symbol":"PERP_BTC_USDT","close":60001},{"symbol":"PERP_UNKNOWN_USDT","close":999}]}"### });
    // Like TS, live data only updates rows the REST ticker seed listed.
    assert!(engine.store().ticker("ETHUSDT").is_none());
    assert!(engine.store().ticker("UNKNOWN").is_none());
    assert_eq!(engine.store().ticker("BTCUSDT").unwrap().last, 60001.0);
    assert_eq!(engine.store().tickers().len(), 1);
}

#[test]
fn unusable_ticker_lists_preserve_existing_quotes_retry_and_recover_valid_peers() {
    for body in [
        r###"{"success":true,"rows":[{"broken":true}]}"###,
        r###"{"success":true,"rows":[]}"###,
    ] {
        for cached in [false, true] {
            let mut engine = public_engine();
            engine.start(0.0);
            let catalogue = support::prepared_seed_actions(&mut engine, 0.0)
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
            let ticker = support::prepared_seed_actions(&mut engine, 0.0)
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
            let timer = engine
                .drain_actions()
                .find_map(|action| match action {
                    Action::SetTimer { timer, at_ms } if jittered(at_ms, 1000.0) => Some(timer),
                    _ => None,
                })
                .expect("unusable successful HTTP body must back off");
            engine.handle(1000.0, Event::TimerFired { timer });
            let retry = support::prepared_seed_actions(&mut engine, 1000.0)
                .into_iter()
                .find_map(|action| match action {
                    Action::Http(request) if request.url == ticker.url => Some(request),
                    _ => None,
                })
                .expect("fresh ticker-only retry");
            engine.handle(1000.0, Event::HttpResponse { request: retry.id,
                response: Response { status: 200, headers: "", body: r###"{"success":true,"rows":[{"broken":true},{"symbol":"PERP_BTC_USDT","24h_open":90,"24h_close":100,"24h_volume":1,"24h_amount":100,"index_price":100,"mark_price":100,"last_funding_rate":0,"next_funding_time":0,"open_interest":1}]}"### } });
            assert!(engine.store().loaded().tickers);
            assert_eq!(engine.store().tickers().len(), 1);
            assert_eq!(engine.store().tickers()[0].last, 100.0);
        }
    }
}
