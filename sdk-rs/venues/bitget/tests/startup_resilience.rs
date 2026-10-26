//! Scripted discovery regressions; no network or account credentials.
use crate::support::FixtureOptions as Options;
mod support;
use safe_cex_engine::{Action, Engine, Event, Notice, Response};
use safe_cex_venue_bitget::BitgetFutures;

/// Retry delays are jittered into `[0.75, 1.0] * nominal`.
fn jittered(actual: f64, nominal: f64) -> bool {
    (0.75 * nominal..=nominal).contains(&actual)
}
const CATALOGUE: &str = r###"{"code":"00000","data":[{"broken":true},{"symbol":"BTCUSDT","baseCoin":"BTC","quoteCoin":"USDT","symbolStatus":"normal","pricePlace":"1","priceEndStep":"1","sizeMultiplier":"0.001","minTradeNum":"0.001","maxOrderQty":"120","maxLever":"125"}]}"###;
fn public_engine() -> Engine<BitgetFutures> {
    crate::support::engine(Options {
        public_data_only: true,
        ..Default::default()
    })
}
#[test]
fn catalogue_network_failures_back_off_then_keep_valid_rows() {
    let mut engine = public_engine();
    engine.start(0.0);
    let mut request = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.0)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("/contracts") => Some(request),
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
        let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, now);
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
                Action::Http(request) if request.url.contains("/contracts") => Some(request),
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
    assert!(safe_cex_engine::test_support::prepared_actions(&mut engine, now).into_iter().any(|action| matches!(action,
        Action::Notice(Notice::Event(safe_cex_engine::LogEvent { message, ..})) if message.contains("Skipped malformed public discovery row"))));
}

#[test]
fn private_snapshot_starts_while_ticker_response_is_still_pending() {
    let mut engine = crate::support::engine(Options {
        key: Some("scripted-key".into()),
        secret: Some("scripted-secret".into()),
        passphrase: Some("scripted-passphrase".into()),
        ..Default::default()
    });
    engine.start(0.0);
    let catalogue = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.0)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("/contracts") => Some(request),
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
    let timer = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.0)
        .into_iter()
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
fn valid_live_ticker_recovers_a_catalogue_market_missing_from_the_rest_seed() {
    let mut engine = public_engine();
    engine.start(0.0);
    let catalogue = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.0)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    engine.handle(0.0, Event::HttpResponse { request: catalogue.id, response: Response { status: 200, headers: "", body: r###"{"code":"00000","data":[{"broken":true},{"symbol":"BTCUSDT","baseCoin":"BTC","quoteCoin":"USDT","symbolStatus":"normal","pricePlace":"1","priceEndStep":"1","sizeMultiplier":"0.001","minTradeNum":"0.001","maxOrderQty":"120","maxLever":"125"},{"symbol":"ETHUSDT","baseCoin":"ETH","quoteCoin":"USDT","symbolStatus":"normal","pricePlace":"1","priceEndStep":"1","sizeMultiplier":"0.001","minTradeNum":"0.001","maxOrderQty":"120","maxLever":"125"}]}"### }});
    let mut actions = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.0);
    let ticker = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r.clone()),
            _ => None,
        })
        .unwrap();
    engine.handle(0.0, Event::HttpResponse { request: ticker.id, response: Response { status: 200, headers: "", body: r###"{"code":"00000","data":[{"symbol":"BTCUSDT","bidPr":"99.9","askPr":"100.1","lastPr":"100","indexPrice":"99","changeUtc24h":"0.01","baseVolume":"1000","quoteVolume":"100000","fundingRate":"0.0001"}]}"### }});
    actions.extend(safe_cex_engine::test_support::prepared_actions(
        &mut engine,
        0.0,
    ));

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
    safe_cex_engine::test_support::prepared_actions(&mut engine, 0.0)
        .into_iter()
        .for_each(drop);
    engine.handle(1.0, Event::WsMessage { socket, text: r###"{"arg":{"channel":"ticker"},"data":[{"instId":"ETHUSDT","lastPr":"201","bidPr":"200","askPr":"202"},{"instId":"UNKNOWN","lastPr":"999"}]}"### });
    assert_eq!(engine.store().ticker("ETHUSDT").unwrap().last, 201.0);
    assert!(engine.store().ticker("UNKNOWN").is_none());
    engine.handle(2.0, Event::WsMessage { socket, text: r###"{"arg":{"channel":"ticker"},"data":[{"instId":"ETHUSDT","lastPr":"201","bidPr":"200","askPr":"202"},{"instId":"UNKNOWN","lastPr":"999"}]}"### });
    assert_eq!(
        engine
            .store()
            .tickers()
            .iter()
            .filter(|t| t.symbol == "ETHUSDT")
            .count(),
        1
    );
}

#[test]
fn unusable_ticker_lists_preserve_existing_quotes_retry_and_recover_valid_peers() {
    for body in [
        r###"{"code":"00000","data":[{"broken":true}]}"###,
        r###"{"code":"00000","data":[]}"###,
    ] {
        for cached in [false, true] {
            let mut engine = public_engine();
            engine.start(0.0);
            let catalogue = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.0)
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
            let ticker = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.0)
                .into_iter()
                .find_map(|action| match action {
                    Action::Http(request) if request.url.contains("/tickers") => Some(request),
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
            let timer = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.0)
                .into_iter()
                .find_map(|action| match action {
                    Action::SetTimer { timer, at_ms } if jittered(at_ms, 1000.0) => Some(timer),
                    _ => None,
                })
                .expect("unusable successful HTTP body must back off");
            engine.handle(1000.0, Event::TimerFired { timer });
            let retry = safe_cex_engine::test_support::prepared_actions(&mut engine, 1000.0)
                .into_iter()
                .find_map(|action| match action {
                    Action::Http(request) if request.url == ticker.url => Some(request),
                    _ => None,
                })
                .expect("fresh ticker-only retry");
            engine.handle(1000.0, Event::HttpResponse { request: retry.id,
                response: Response { status: 200, headers: "", body: r###"{"code":"00000","data":[{"broken":true},{"symbol":"BTCUSDT","lastPr":"100"}]}"### } });
            assert!(engine.store().loaded().tickers);
            assert_eq!(engine.store().tickers().len(), 1);
            assert_eq!(engine.store().tickers()[0].last, 100.0);
        }
    }
}
