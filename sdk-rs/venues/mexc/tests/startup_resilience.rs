//! Scripted discovery regressions; no network or account credentials.
mod support;
use safe_cex_engine::{Action, Engine, Event, Response};
use safe_cex_venue_mexc::{MexcFutures, Options};
use support::jittered;
const CATALOGUE: &str = r###"{"success":true,"code":0,"data":[{"broken":true},{"symbol":"BTC_USDT","baseCoin":"BTC","quoteCoin":"USDT","settleCoin":"USDT","state":0,"apiAllowed":true,"contractSize":0.0001,"volUnit":1,"priceUnit":0.1,"minVol":1,"maxVol":400000,"minLeverage":1,"maxLeverage":500,"makerFeeRate":0,"takerFeeRate":0.0002}]}"###;
fn public_engine() -> Engine<MexcFutures> {
    safe_cex_engine::test_support::seeded_engine(MexcFutures::new(Options {
        public_data_only: true,
        ..Default::default()
    }))
}
#[test]
fn catalogue_network_failures_back_off_then_keep_valid_rows() {
    let mut engine = public_engine();
    engine.start(0.0);
    let mut request = support::prepared(&mut engine, 0.)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("contract/detail") => Some(request),
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
        request = support::prepared(&mut engine, now)
            .into_iter()
            .find_map(|action| match action {
                Action::Http(request) if request.url.contains("contract/detail") => Some(request),
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
    let mut engine = safe_cex_engine::test_support::seeded_engine(MexcFutures::new(Options {
        key: Some("scripted-key".into()),
        secret: Some("scripted-secret".into()),
        ..Default::default()
    }));
    engine.start(0.0);
    let catalogue = support::prepared(&mut engine, 0.)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("contract/detail") => Some(request),
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
    let reads = support::prepared(&mut engine, 0.)
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
fn public_blank_ticker_numbers_follow_ts_zero_and_last_price_fallbacks() {
    let mut engine = public_engine();
    engine.start(0.0);
    let catalogue = support::prepared(&mut engine, 0.)
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
    let ticker = support::prepared(&mut engine, 0.)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains("contract/ticker") => Some(r),
            _ => None,
        })
        .unwrap();
    engine.handle(0.0, Event::HttpResponse { request: ticker.id, response: Response { status: 200, headers: "", body: r#"{"success":true,"code":0,"data":[{"symbol":"BTC_USDT","lastPrice":"100","fairPrice":"","indexPrice":"","bid1":"","ask1":"","riseFallRate":"","holdVol":"","fundingRate":"","fundingTime":"","volume24":"","amount24":""}]}"# }});
    let ticker = engine.store().ticker("BTCUSDT").unwrap();
    assert_eq!(
        (
            ticker.bid,
            ticker.ask,
            ticker.last,
            ticker.mark,
            ticker.index
        ),
        (100.0, 100.0, 100.0, 100.0, 100.0)
    );
    assert_eq!(
        (ticker.volume, ticker.quote_volume, ticker.funding_rate),
        (0.0, 0.0, 0.0)
    );
}

#[test]
fn ticker_http_failure_retries_without_reloading_the_catalogue() {
    let mut engine = public_engine();
    engine.start(0.0);
    let catalogue = support::prepared(&mut engine, 0.)
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
    let ticker = support::prepared(&mut engine, 0.)
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
    let retry = support::prepared(&mut engine, 1000.)
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
        r###"{"success":true,"code":0,"data":[{"broken":true}]}"###,
        r###"{"success":true,"code":0,"data":[]}"###,
    ] {
        for cached in [false, true] {
            let mut engine = public_engine();
            engine.start(0.0);
            let catalogue = support::prepared(&mut engine, 0.)
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
            let ticker = support::prepared(&mut engine, 0.)
                .into_iter()
                .find_map(|action| match action {
                    Action::Http(request) if request.url.contains("/contract/ticker") => {
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
            let retry = support::prepared(&mut engine, 1000.)
                .into_iter()
                .find_map(|action| match action {
                    Action::Http(request) if request.url == ticker.url => Some(request),
                    _ => None,
                })
                .expect("fresh ticker-only retry");
            engine.handle(1000.0, Event::HttpResponse { request: retry.id,
                response: Response { status: 200, headers: "", body: r###"{"success":true,"code":0,"data":[{"broken":true},{"symbol":"BTC_USDT","lastPrice":100}]}"### } });
            assert!(engine.store().loaded().tickers);
            assert_eq!(engine.store().tickers().len(), 1);
            assert_eq!(engine.store().tickers()[0].last, 100.0);
        }
    }
}
