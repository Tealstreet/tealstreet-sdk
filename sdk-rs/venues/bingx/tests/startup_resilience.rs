//! Scripted discovery regressions; no network or account credentials.
use safe_cex_engine::{Action, Engine, Event, Notice, Response};
use safe_cex_venue_bingx::Bingx;
mod support;
use support::Options;

/// Managed reads are released only through engine preparation.
fn drain(engine: &mut Engine<Bingx>, now: f64) -> std::vec::IntoIter<Action> {
    safe_cex_engine::test_support::prepared_actions(engine, now).into_iter()
}
/// Retry delays are jittered into `[0.75, 1.0] * nominal`.
fn jittered(actual: f64, nominal: f64) -> bool {
    (0.75 * nominal..=nominal).contains(&actual)
}
const CATALOGUE: &str = r###"{"code":0,"data":[{"broken":true},{"symbol":"BTC-USDT","asset":"BTC","currency":"USDT","status":1,"quantityPrecision":4,"pricePrecision":1,"tradeMinLimit":"0.0001","maxLongLeverage":125}]}"###;
fn public_engine() -> Engine<Bingx> {
    support::engine(Options {
        public_data_only: true,
        ..Default::default()
    })
}
#[test]
fn catalogue_network_failures_back_off_then_keep_valid_rows() {
    let mut engine = public_engine();
    engine.start(0.0);
    let mut request = drain(&mut engine, 0.)
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
        let actions = drain(&mut engine, now).collect::<Vec<_>>();
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
        request = drain(&mut engine, now)
            .find_map(|action| match action {
                Action::Http(request) if request.url.contains("/contracts") => Some(request),
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
    assert!(drain(&mut engine, now).any(|action| matches!(action,
        Action::Notice(Notice::Event(safe_cex_engine::LogEvent { message, ..})) if message.contains("Skipped malformed public discovery row"))));
}

#[test]
fn private_snapshot_starts_while_ticker_response_is_still_pending() {
    let mut engine = support::engine(Options {
        key: Some("scripted-key".into()),
        secret: Some("scripted-secret".into()),
        ..Default::default()
    });
    engine.start(0.0);
    let catalogue = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.)
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
    let reads = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.)
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
    let catalogue = drain(&mut engine, 0.)
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
    let ticker = drain(&mut engine, 0.)
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
    let timer = drain(&mut engine, 0.)
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if jittered(at_ms, 1000.0) => Some(timer),
            _ => None,
        })
        .expect("failed public ticker read uses shared retry backoff");
    assert!(engine.store().loaded().markets);
    engine.handle(1000.0, Event::TimerFired { timer });
    let retry = drain(&mut engine, 1000.)
        .find_map(|a| match a {
            Action::Http(r) if r.url == ticker.url => Some(r),
            _ => None,
        })
        .expect("ticker source retried independently");
    assert_ne!(retry.id, ticker.id);
}

#[test]
fn live_ticker_after_the_seed_does_not_create_a_market_the_rest_seed_omitted() {
    let mut engine = public_engine();
    engine.start(0.0);
    let catalogue = drain(&mut engine, 0.)
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    engine.handle(0.0, Event::HttpResponse { request: catalogue.id, response: Response { status: 200, headers: "", body: r###"{"code":0,"data":[{"broken":true},{"symbol":"BTC-USDT","asset":"BTC","currency":"USDT","status":1,"quantityPrecision":4,"pricePrecision":1,"tradeMinLimit":"0.0001","maxLongLeverage":125},{"symbol":"ETH-USDT","asset":"ETH","currency":"USDT","status":1,"quantityPrecision":4,"pricePrecision":1,"tradeMinLimit":"0.0001","maxLongLeverage":125}]}"### }});
    let mut actions = drain(&mut engine, 0.).collect::<Vec<_>>();
    let ticker = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r.clone()),
            _ => None,
        })
        .unwrap();
    engine.handle(0.0, Event::HttpResponse { request: ticker.id, response: Response { status: 200, headers: "", body: r###"{"code":0,"data":[{"symbol":"BTC-USDT","lastPrice":"100","priceChangePercent":"1","volume":"1","quoteVolume":"100"}]}"### }});
    actions.extend(drain(&mut engine, 0.));
    let premium = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains("premiumIndex") => Some(r.clone()),
            _ => None,
        })
        .unwrap();
    engine.handle(0.0, Event::HttpResponse { request: premium.id, response: Response { status: 200, headers: "", body: r#"{"code":0,"data":[{"symbol":"BTC-USDT","markPrice":"100","indexPrice":"100","lastFundingRate":"0.01","nextFundingTime":0}]}"# }});
    actions.extend(drain(&mut engine, 0.));
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
    drain(&mut engine, 0.).for_each(drop);
    engine.handle(1.0, Event::WsMessage { socket, text: r###"{"dataType":"market.contracts","data":{"contracts":[{"symbol":"ETH-USDT","tradePrice":"201","fairPrice":"202","indexPrice":"203","fundingRate":"0.01"},{"symbol":"UNKNOWN","tradePrice":"999","fairPrice":"999","indexPrice":"999","fundingRate":"0"}]}}"### });
    // TS only updates seeded rows (e.g. the NC* markets absent from the daily seed).
    assert!(engine.store().ticker("ETHUSDT").is_none());
    assert!(engine.store().ticker("UNKNOWN").is_none());
    assert_eq!(engine.store().ticker("BTCUSDT").unwrap().last, 100.0);
    assert_eq!(engine.store().tickers().len(), 1);
}

#[test]
fn entirely_malformed_ticker_source_retries_without_publishing_an_empty_seed() {
    let mut engine = public_engine();
    engine.start(0.0);
    let catalogue = drain(&mut engine, 0.)
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
    let ticker = drain(&mut engine, 0.)
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
                body: r###"{"code":0,"data":[{"broken":true}]}"###,
            },
        },
    );
    let timer = drain(&mut engine, 0.)
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if jittered(at_ms, 1000.0) => Some(timer),
            _ => None,
        })
        .expect("failed public ticker read uses shared retry backoff");
    assert!(engine.store().loaded().markets);
    assert!(!engine.store().loaded().tickers);
    engine.handle(1000.0, Event::TimerFired { timer });
    let retry = drain(&mut engine, 1000.)
        .find_map(|a| match a {
            Action::Http(r) if r.url == ticker.url => Some(r),
            _ => None,
        })
        .expect("ticker source retried independently");
    assert_ne!(retry.id, ticker.id);
}
