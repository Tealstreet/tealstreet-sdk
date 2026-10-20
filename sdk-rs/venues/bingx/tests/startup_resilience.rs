//! Scripted discovery regressions; no network or account credentials.
use safe_cex_engine::{Action, Engine, Event, Notice, Response};
use safe_cex_venue_bingx::{Bingx, Options};
const CATALOGUE: &str = r###"{"code":0,"data":[{"broken":true},{"symbol":"BTC-USDT","asset":"BTC","currency":"USDT","status":1,"quantityPrecision":4,"pricePrecision":1,"tradeMinLimit":"0.0001","maxLongLeverage":125}]}"###;
fn public_engine() -> Engine<Bingx> {
    Engine::new(Bingx::new(Options {
        public_data_only: true,
        ..Default::default()
    }))
}
#[test]
fn catalogue_network_failures_back_off_then_keep_valid_rows() {
    let mut engine = public_engine();
    engine.start(0.0);
    let mut request = engine
        .drain_actions()
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
                Action::SetTimer { timer, at_ms } if at_ms == now + delay => Some(timer),
                _ => None,
            })
            .expect("shared exponential retry timer");
        now += delay;
        engine.handle(now, Event::TimerFired { timer });
        request = engine
            .drain_actions()
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
    assert!(engine.drain_actions().any(|action| matches!(action,
        Action::Notice(Notice::Log { message, .. }) if message.contains("Skipped malformed public discovery row"))));
}

#[test]
fn private_snapshot_starts_while_ticker_response_is_still_pending() {
    let mut engine = Engine::new(Bingx::new(Options {
        key: Some("scripted-key".into()),
        secret: Some("scripted-secret".into()),
        ..Default::default()
    }));
    engine.start(0.0);
    let catalogue = engine
        .drain_actions()
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
    let reads = engine
        .drain_actions()
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
    let catalogue = engine
        .drain_actions()
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
    let ticker = engine
        .drain_actions()
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
            Action::SetTimer {
                timer,
                at_ms: 1000.0,
            } => Some(timer),
            _ => None,
        })
        .expect("failed public ticker read uses shared retry backoff");
    assert!(engine.store().loaded().markets);
    engine.handle(1000.0, Event::TimerFired { timer });
    let retry = engine
        .drain_actions()
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
    let catalogue = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    engine.handle(0.0, Event::HttpResponse { request: catalogue.id, response: Response { status: 200, headers: "", body: r###"{"code":0,"data":[{"broken":true},{"symbol":"BTC-USDT","asset":"BTC","currency":"USDT","status":1,"quantityPrecision":4,"pricePrecision":1,"tradeMinLimit":"0.0001","maxLongLeverage":125},{"symbol":"ETH-USDT","asset":"ETH","currency":"USDT","status":1,"quantityPrecision":4,"pricePrecision":1,"tradeMinLimit":"0.0001","maxLongLeverage":125}]}"### }});
    let mut actions = engine.drain_actions().collect::<Vec<_>>();
    let ticker = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r.clone()),
            _ => None,
        })
        .unwrap();
    engine.handle(0.0, Event::HttpResponse { request: ticker.id, response: Response { status: 200, headers: "", body: r###"{"code":0,"data":[{"symbol":"BTC-USDT","lastPrice":"100","priceChangePercent":"1","volume":"1","quoteVolume":"100"}]}"### }});
    actions.extend(engine.drain_actions());
    let premium = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains("premiumIndex") => Some(r.clone()),
            _ => None,
        })
        .unwrap();
    engine.handle(0.0, Event::HttpResponse { request: premium.id, response: Response { status: 200, headers: "", body: r#"{"code":0,"data":[{"symbol":"BTC-USDT","markPrice":"100","indexPrice":"100","lastFundingRate":"0.01","nextFundingTime":0}]}"# }});
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
    engine.handle(1.0, Event::WsMessage { socket, text: r###"{"dataType":"market.contracts","data":{"contracts":[{"symbol":"ETH-USDT","tradePrice":"201","fairPrice":"202","indexPrice":"203","fundingRate":"0.01"},{"symbol":"UNKNOWN","tradePrice":"999","fairPrice":"999","indexPrice":"999","fundingRate":"0"}]}}"### });
    assert_eq!(engine.store().ticker("ETHUSDT").unwrap().last, 201.0);
    assert!(engine.store().ticker("UNKNOWN").is_none());
    engine.handle(2.0, Event::WsMessage { socket, text: r###"{"dataType":"market.contracts","data":{"contracts":[{"symbol":"ETH-USDT","tradePrice":"201","fairPrice":"202","indexPrice":"203","fundingRate":"0.01"},{"symbol":"UNKNOWN","tradePrice":"999","fairPrice":"999","indexPrice":"999","fundingRate":"0"}]}}"### });
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
fn entirely_malformed_ticker_source_retries_without_publishing_an_empty_seed() {
    let mut engine = public_engine();
    engine.start(0.0);
    let catalogue = engine
        .drain_actions()
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
    let ticker = engine
        .drain_actions()
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
    let timer = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer {
                timer,
                at_ms: 1000.0,
            } => Some(timer),
            _ => None,
        })
        .expect("failed public ticker read uses shared retry backoff");
    assert!(engine.store().loaded().markets);
    assert!(!engine.store().loaded().tickers);
    engine.handle(1000.0, Event::TimerFired { timer });
    let retry = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::Http(r) if r.url == ticker.url => Some(r),
            _ => None,
        })
        .expect("ticker source retried independently");
    assert_ne!(retry.id, ticker.id);
}
