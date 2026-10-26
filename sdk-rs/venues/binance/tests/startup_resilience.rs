//! Scripted discovery regressions; no network or account credentials.
use support::Options;
mod support;
use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{Action, Engine, Event, Notice, Response};
use safe_cex_venue_binance::BinanceFutures;
use support::Harness;

/// Retry delays are jittered into `[0.75, 1.0] * nominal`.
fn jittered(actual: f64, nominal: f64) -> bool {
    (0.75 * nominal..=nominal).contains(&actual)
}
const CATALOGUE: &str = r###"{"symbols":[{"broken":true},{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","marginAsset":"USDT","contractType":"PERPETUAL","filters":[{"filterType":"PRICE_FILTER","tickSize":"0.10"},{"filterType":"LOT_SIZE","stepSize":"0.001","minQty":"0.001","maxQty":"1000"},{"filterType":"MARKET_LOT_SIZE","stepSize":"0.001","minQty":"0.001","maxQty":"120"}]}]}"###;
fn public_engine() -> Engine<BinanceFutures> {
    support::engine(Options {
        public_data_only: true,
        ..Default::default()
    })
}
#[test]
fn catalogue_network_failures_back_off_then_keep_valid_rows() {
    let mut engine = public_engine();
    engine.start(0.0);
    let mut request = prepared_actions(&mut engine, 0.0)
        .into_iter()
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
        let actions = prepared_actions(&mut engine, now)
            .into_iter()
            .collect::<Vec<_>>();
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
                Action::Http(request) if request.url.contains("exchangeInfo") => Some(request),
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
    assert!(prepared_actions(&mut engine, now).into_iter().any(|action| matches!(action,
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
    let catalogue = prepared_actions(&mut engine, 0.0)
        .into_iter()
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
    let reads = prepared_actions(&mut engine, 0.0)
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
fn entirely_malformed_ticker_source_retries_without_publishing_an_empty_seed() {
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
                status: 200,
                headers: "",
                body: r###"[{"broken":true}]"###,
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
    assert!(!engine.store().loaded().tickers);
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

fn public_harness() -> Harness {
    Harness::started(Options {
        public_data_only: true,
        ..Default::default()
    })
}

/// No retry of `path` until `wait_ms` has passed, then exactly on time.
fn assert_retry_waits(h: &mut Harness, path: &str, wait_ms: f64) {
    h.advance(wait_ms - 1.0);
    assert!(!h.has_request(path), "{path} retried inside the ban window");
    h.advance(1.0);
    assert!(h.has_request(path), "{path} retried once the window passed");
}

#[test]
fn catalogue_rate_limit_retry_waits_for_retry_after() {
    let mut h = public_harness();
    let request = h.take("/fapi/v1/exchangeInfo").id;
    h.reply(request, 429, "Retry-After: 120", "{}");
    assert_retry_waits(&mut h, "/fapi/v1/exchangeInfo", 120_000.0);
}

#[test]
fn ticker_rate_limit_retry_waits_for_retry_after() {
    let mut h = public_harness();
    h.respond("/fapi/v1/exchangeInfo", 200, support::EXCHANGE_INFO);
    let request = h.take("/fapi/v1/ticker/24hr").id;
    h.reply(request, 429, "Retry-After: 120", "{}");
    assert_retry_waits(&mut h, "/fapi/v1/ticker/24hr", 120_000.0);
}

#[test]
fn an_overflowing_retry_after_resumes_after_a_day_instead_of_never() {
    let mut h = public_harness();
    let request = h.take("/fapi/v1/exchangeInfo").id;
    h.reply(request, 418, "Retry-After: 1e308", "{}");
    assert_retry_waits(&mut h, "/fapi/v1/exchangeInfo", 86_400_000.0);
}

#[test]
fn catalogue_ip_ban_without_retry_after_waits_the_weight_pause() {
    let mut h = public_harness();
    let request = h.take("/fapi/v1/exchangeInfo").id;
    h.reply(request, 418, "", "{}");
    assert_retry_waits(&mut h, "/fapi/v1/exchangeInfo", 60_000.0);
}

#[test]
fn a_market_without_a_positive_step_is_skipped_as_malformed() {
    let valid = r#"{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","marginAsset":"USDT","contractType":"PERPETUAL","filters":[{"filterType":"PRICE_FILTER","tickSize":"0.10"},{"filterType":"LOT_SIZE","stepSize":"0.001","minQty":"0.001","maxQty":"1000"}]}"#;
    for (field, bad) in [
        ("\"tickSize\":\"0.10\"", "\"tickSize\":\"0\""),
        ("\"stepSize\":\"0.001\"", "\"stepSize\":\"-1\""),
        (
            "{\"filterType\":\"PRICE_FILTER\",\"tickSize\":\"0.10\"},",
            "",
        ),
    ] {
        let broken = valid
            .replace("BTCUSDT", "ETHUSDT")
            .replace("\"BTC\"", "\"ETH\"")
            .replacen(field, bad, 1);
        let body = format!(r#"{{"symbols":[{valid},{broken}]}}"#);
        let mut engine = public_engine();
        engine.start(0.0);
        let request = prepared_actions(&mut engine, 0.0)
            .into_iter()
            .find_map(|action| match action {
                Action::Http(request) if request.url.contains("exchangeInfo") => Some(request),
                _ => None,
            })
            .expect("catalogue request");
        engine.set_log_level(safe_cex_engine::Level::Debug);
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
        assert!(engine.store().market("BTCUSDT").is_some());
        assert!(engine.store().market("ETHUSDT").is_none(), "{bad}");
        assert!(prepared_actions(&mut engine, 0.0).into_iter().any(|action| matches!(action,
            Action::Notice(Notice::Event(safe_cex_engine::LogEvent { message, ..})) if message.contains("Skipped malformed public discovery row"))));
    }
}
