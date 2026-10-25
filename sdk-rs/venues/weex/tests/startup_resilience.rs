//! Scripted discovery regressions; no network or account credentials.
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response};
mod support;
use safe_cex_venue_weex::WeexFutures;
use support::PreparedActions;
use support::setup::Options;

/// A jittered retry delay lands in `[0.75, 1.0] * nominal`.
fn jittered(actual: f64, nominal: f64) -> bool {
    (0.75 * nominal..=nominal).contains(&actual)
}
const CATALOGUE: &str = r###"{"symbols":[{"broken":true},{"symbol":"BTCUSDT","baseAsset":"BTC","quoteAsset":"USDT","marginAsset":"USDT","pricePrecision":1,"quantityPrecision":6,"contractVal":1e-06,"minOrderSize":0.0001,"maxOrderSize":10000,"maxLeverage":408}]}"###;
fn public_engine() -> Engine<WeexFutures> {
    support::setup::engine(Options {
        public_data_only: true,
        ..Default::default()
    })
}
#[test]
fn catalogue_network_failures_back_off_then_keep_valid_rows() {
    let mut engine = public_engine();
    engine.start(0.0);
    let mut request = engine
        .prepared_actions(0.0)
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("exchangeInfo") => Some(request),
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
        request = engine
            .prepared_actions(now)
            .find_map(|action| match action {
                Action::Http(request) if request.url.contains("exchangeInfo") => Some(request),
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
fn private_snapshot_starts_when_a_pending_ticker_read_fails() {
    let mut engine = support::setup::engine(Options {
        key: Some("scripted-key".into()),
        secret: Some("scripted-secret".into()),
        passphrase: Some("scripted-passphrase".into()),
        ..Default::default()
    });
    engine.start(0.0);
    let catalogue = engine
        .prepared_actions(0.0)
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("exchangeInfo") => Some(request),
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
    let private = |request: &HttpRequest| {
        ["account", "position", "order"]
            .iter()
            .any(|part| request.url.to_ascii_lowercase().contains(part))
    };
    let seed = engine
        .prepared_actions(0.0)
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect::<Vec<_>>();
    // Account rows map against seeded prices, so a pending seed is awaited...
    assert!(!seed.iter().any(private));
    let daily = seed.iter().find(|r| r.url.contains("24hr")).unwrap();
    engine.handle(
        1.0,
        Event::HttpFailed {
            request: daily.id,
            error: "timed out",
        },
    );
    // ...but its first failure starts the account; retrying public reads never gate it.
    let reads = engine
        .prepared_actions(1.0)
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        reads.iter().any(private),
        "private REST reads must start on the first ticker failure: {:?}",
        reads.iter().map(|r| &r.url).collect::<Vec<_>>()
    );
}

#[test]
fn blank_public_values_keep_ts_zero_and_fallback_semantics_in_either_response_order() {
    for book_first in [false, true] {
        let mut engine = public_engine();
        engine.start(0.0);
        let catalogue = engine
            .prepared_actions(0.0)
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
        let requests = engine
            .prepared_actions(0.0)
            .filter_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .collect::<Vec<_>>();
        let daily = requests.iter().find(|r| r.url.contains("24hr")).unwrap();
        let book = requests
            .iter()
            .find(|r| r.url.contains("bookTicker"))
            .unwrap();
        let rows = [
            (
                daily.id,
                r#"[{"symbol":"BTCUSDT","lastPrice":"100","openPrice":"","markPrice":"","indexPrice":"","priceChangePercent":"","volume":"","quoteVolume":"","fundingRate":""}]"#,
            ),
            (
                book.id,
                r#"[{"symbol":"BTCUSDT","bidPrice":"","askPrice":""}]"#,
            ),
        ];
        for index in if book_first { [1, 0] } else { [0, 1] } {
            engine.handle(
                0.0,
                Event::HttpResponse {
                    request: rows[index].0,
                    response: Response {
                        status: 200,
                        headers: "",
                        body: rows[index].1,
                    },
                },
            );
        }
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
}

#[test]
fn ticker_http_failure_retries_without_reloading_the_catalogue() {
    let mut engine = public_engine();
    engine.start(0.0);
    let catalogue = engine
        .prepared_actions(0.0)
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
        .prepared_actions(0.0)
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
    let retry = engine
        .prepared_actions(1000.0)
        .find_map(|a| match a {
            Action::Http(r) if r.url == ticker.url => Some(r),
            _ => None,
        })
        .expect("ticker source retried independently");
    assert_ne!(retry.id, ticker.id);
}

#[test]
fn entirely_unusable_daily_and_book_sources_retry_without_replacing_prior_quotes() {
    for cached in [false, true] {
        let mut engine = public_engine();
        engine.start(0.0);
        let catalogue = engine
            .prepared_actions(0.0)
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
        let reads = engine
            .prepared_actions(0.0)
            .filter_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .collect::<Vec<_>>();
        let market = engine.store().markets()[0].clone();
        if cached {
            engine
                .store_mut()
                .set_tickers(vec![safe_cex_engine::model::Ticker {
                    id: market.id,
                    symbol: market.symbol,
                    last: 321.0,
                    ..Default::default()
                }]);
        }
        let before = engine.store().tickers().to_vec();
        for read in reads {
            engine.handle(
                0.0,
                Event::HttpResponse {
                    request: read.id,
                    response: Response {
                        status: 200,
                        headers: "",
                        body: r#"[{"broken":true}]"#,
                    },
                },
            );
        }
        assert_eq!(engine.store().tickers(), before);
        assert_eq!(engine.store().loaded().tickers, cached);
        let timer = engine
            .drain_actions()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if jittered(at_ms, 1000.0) => Some(timer),
                _ => None,
            })
            .expect("empty seed retries");
        engine.handle(1000.0, Event::TimerFired { timer });
        assert!(
            engine
                .prepared_actions(1000.0)
                .any(|a| matches!(a, Action::Http(r) if r.url.contains("24hr")))
        );
    }
}
