#![cfg(feature = "spot")]
//! Scripted discovery regressions; no network or account credentials.
mod support;
use safe_cex_engine::{Action, Engine, Event, Notice, Response};
use safe_cex_venue_kucoin::spot::KucoinSpot;
use support::credentials::Options;
use support::jittered;
const CATALOGUE: &str = r###"{"code":"200000","data":[{"broken":true},{"symbol":"BTC-USDT","baseCurrency":"BTC","quoteCurrency":"USDT","enableTrading":true,"baseIncrement":"0.001","priceIncrement":"0.1","baseMinSize":"0.001","baseMaxSize":"100"},{"symbol":"ETH-USDT","baseCurrency":"ETH","quoteCurrency":"USDT","enableTrading":true,"baseIncrement":"0.01","priceIncrement":"0.01","baseMinSize":"0.01","baseMaxSize":"1000"}]}"###;
fn public_engine() -> Engine<KucoinSpot> {
    support::credentials::engine(
        Options {
            public_data_only: true,
            ..Default::default()
        },
        KucoinSpot::new,
    )
}
#[test]
fn catalogue_network_failures_back_off_then_keep_valid_rows() {
    let mut engine = public_engine();
    engine.start(0.0);
    let mut request = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.0)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("/api/v2/symbols") => Some(request),
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
                Action::Http(request) if request.url.contains("/api/v2/symbols") => Some(request),
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
    let mut engine = support::credentials::engine(
        Options {
            key: Some("scripted-key".into()),
            secret: Some("scripted-secret".into()),
            passphrase: Some("scripted-passphrase".into()),
            ..Default::default()
        },
        KucoinSpot::new,
    );
    engine.start(0.0);
    let catalogue = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.0)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("/api/v2/symbols") => Some(request),
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
fn unusable_ticker_lists_preserve_existing_quotes_retry_and_recover_valid_peers() {
    for body in [
        r###"{"code":"200000","data":{"ticker":[{"broken":true}]}}"###,
        r###"{"code":"200000","data":{"ticker":[]}}"###,
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
                    Action::Http(request) if request.url.contains("/market/allTickers") => {
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
                response: Response { status: 200, headers: "", body: r###"{"code":"200000","data":{"ticker":[{"broken":true},{"symbol":"BTC-USDT","last":"100"}]}}"### } });
            assert!(engine.store().loaded().tickers);
            assert_eq!(engine.store().tickers().len(), 1);
            assert_eq!(engine.store().tickers()[0].last, 100.0);
        }
    }
}

fn reply(engine: &mut Engine<KucoinSpot>, request: safe_cex_engine::HttpRequest, body: &str) {
    engine.handle(
        0.0,
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

#[test]
fn funded_wallet_usd_balance_waits_for_its_quote() {
    let mut engine = support::credentials::engine(
        Options {
            key: Some("scripted-key".into()),
            secret: Some("scripted-secret".into()),
            passphrase: Some("scripted-passphrase".into()),
            ..Default::default()
        },
        KucoinSpot::new,
    );
    engine.start(0.0);
    let catalogue = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.0)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("/api/v2/symbols") => Some(request),
            _ => None,
        })
        .unwrap();
    reply(&mut engine, catalogue, CATALOGUE);
    let mut reads = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.0)
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect::<Vec<_>>();
    let take = |reads: &mut Vec<safe_cex_engine::HttpRequest>, part: &str| {
        let index = reads.iter().position(|r| r.url.contains(part)).unwrap();
        reads.remove(index)
    };
    let tickers = take(&mut reads, "/market/allTickers");
    let wallet = r###"{"code":"200000","data":[{"currency":"BTC","available":"1","holds":"0","balance":"1"},{"currency":"USDT","available":"5","holds":"0","balance":"5"}]}"###;
    reply(&mut engine, take(&mut reads, "/api/v1/accounts"), wallet);
    assert!(
        !engine.store().loaded().balance,
        "BTC is unpriced; no USD balance may be published"
    );
    assert_eq!(engine.store().currency_balances().unwrap().len(), 2);
    reply(
        &mut engine,
        tickers,
        r###"{"code":"200000","data":{"ticker":[{"symbol":"BTC-USDT","last":"100"}]}}"###,
    );
    let balance = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.0)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("/api/v1/accounts") => Some(request),
            _ => None,
        })
        .expect("wallet revalued once its quote arrives");
    reply(&mut engine, balance, wallet);
    assert!(engine.store().loaded().balance);
    assert_eq!(engine.store().balance().total, 105.0);
}

struct Private {
    engine: Engine<KucoinSpot>,
    reads: Vec<safe_cex_engine::HttpRequest>,
    poll: safe_cex_engine::TimerId,
}
fn private_with_catalogue(catalogue: &str) -> Private {
    let mut engine = support::credentials::engine(
        Options {
            key: Some("scripted-key".into()),
            secret: Some("scripted-secret".into()),
            passphrase: Some("scripted-passphrase".into()),
            ..Default::default()
        },
        KucoinSpot::new,
    );
    engine.start(0.0);
    let request = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.0)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("/api/v2/symbols") => Some(request),
            _ => None,
        })
        .unwrap();
    reply(&mut engine, request, catalogue);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.0);
    let poll = actions
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == 5000.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    let reads = actions
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect();
    Private {
        engine,
        reads,
        poll,
    }
}
fn take(reads: &mut Vec<safe_cex_engine::HttpRequest>, part: &str) -> safe_cex_engine::HttpRequest {
    let index = reads.iter().position(|r| r.url.contains(part)).unwrap();
    reads.remove(index)
}
fn wallet_read(actions: &[Action]) -> safe_cex_engine::HttpRequest {
    actions
        .iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("/api/v1/accounts") => {
                Some(request.clone())
            }
            _ => None,
        })
        .expect("wallet read")
}
fn count(actions: &[Action], text: &str) -> usize {
    actions
        .iter()
        .filter(|action| matches!(action, Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains(text)))
        .count()
}

#[test]
fn funded_wallet_on_inactive_pair_publishes_without_waiting_for_a_quote() {
    let catalogue = CATALOGUE.replace(
        "]}",
        r###",{"symbol":"NEW-USDT","baseCurrency":"NEW","quoteCurrency":"USDT","enableTrading":false,"baseIncrement":"1","priceIncrement":"0.0001","baseMinSize":"1","baseMaxSize":"1000000"}]}"###,
    );
    let Private {
        mut engine,
        mut reads,
        ..
    } = private_with_catalogue(&catalogue);
    let wallet = r###"{"code":"200000","data":[{"currency":"NEW","available":"50","holds":"0","balance":"50"},{"currency":"USDT","available":"5","holds":"0","balance":"5"}]}"###;
    reply(&mut engine, take(&mut reads, "/api/v1/accounts"), wallet);
    assert!(
        engine.store().loaded().balance,
        "a pre-listing pair never quotes; it must not hold the USD balance"
    );
    assert_eq!(engine.store().balance().total, 5.0);
    assert_eq!(engine.store().coin_balances().unwrap()[0].notional, 0.0);
}

#[test]
fn listed_pair_missing_from_the_ticker_snapshot_is_valued_at_zero_with_one_warning() {
    let Private {
        mut engine,
        mut reads,
        poll,
    } = private_with_catalogue(CATALOGUE);
    let tickers = take(&mut reads, "/market/allTickers");
    let wallet = r###"{"code":"200000","data":[{"currency":"ETH","available":"2","holds":"0","balance":"2"},{"currency":"USDT","available":"5","holds":"0","balance":"5"}]}"###;
    reply(&mut engine, take(&mut reads, "/api/v1/accounts"), wallet);
    assert!(
        !engine.store().loaded().balance,
        "held until the first ticker snapshot"
    );
    reply(
        &mut engine,
        tickers,
        r###"{"code":"200000","data":{"ticker":[{"symbol":"BTC-USDT","last":"100"}]}}"###,
    );
    let balance = wallet_read(&engine.drain_actions().collect::<Vec<_>>());
    reply(&mut engine, balance, wallet);
    assert!(engine.store().loaded().balance);
    assert_eq!(engine.store().balance().total, 5.0);
    let mut warned = count(&engine.drain_actions().collect::<Vec<_>>(), "valued at 0");
    engine.handle(5000.0, Event::TimerFired { timer: poll });
    let balance = wallet_read(&engine.drain_actions().collect::<Vec<_>>());
    reply(&mut engine, balance, wallet);
    warned += count(&engine.drain_actions().collect::<Vec<_>>(), "valued at 0");
    assert_eq!(warned, 1);
}

#[test]
fn a_market_without_a_positive_step_is_skipped_as_malformed() {
    for (valid, bad) in [
        (r#""priceIncrement":"0.01""#, r#""priceIncrement":"0""#),
        (r#""baseIncrement":"0.01""#, r#""baseIncrement":"-1""#),
        (r#""priceIncrement":"0.01""#, r#""priceIncrement":"""#),
    ] {
        let body = CATALOGUE.replace(valid, bad);
        assert_ne!(body, CATALOGUE);
        let mut engine = public_engine();
        engine.start(0.0);
        let request = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.0)
            .into_iter()
            .find_map(|action| match action {
                Action::Http(request) if request.url.contains("/api/v2/symbols") => Some(request),
                _ => None,
            })
            .expect("catalogue request");
        engine.handle(
            0.0,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: &body,
                },
            },
        );
        assert!(engine.store().market("BTC-USDT").is_some());
        assert!(engine.store().market("ETH-USDT").is_none(), "{bad}");
        assert!(engine.drain_actions().any(|action| matches!(action,
            Action::Notice(Notice::Event(safe_cex_engine::LogEvent { message, ..})) if message.contains("Skipped malformed public discovery row"))));
    }
}
