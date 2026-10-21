mod support;
use safe_cex_engine::Event;
use safe_cex_venue_bybit::Options;
use support::*;

#[test]
fn ticker_failure_cannot_block_authenticated_account_snapshot() {
    let mut h = Harness::started(Options {
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Default::default()
    });
    h.respond("/v5/account/fee-rate", 200, FEE_RATES);
    h.respond("/v5/account/info", 200, ACCOUNT_INFO);
    h.respond("status=PreLaunch", 200, PRELAUNCH);
    let mut body: serde_json::Value = serde_json::from_str(INSTRUMENTS).unwrap();
    body["result"]["list"]
        .as_array_mut()
        .unwrap()
        .insert(0, serde_json::json!({"broken":true}));
    h.respond("/v5/market/instruments-info", 200, &body.to_string());
    assert!(h.engine.store().loaded().markets);
    assert!(!h.engine.store().loaded().tickers);
    assert!(h.has_request("/v5/order/realtime"));
    assert!(h.has_request("/v5/position/list"));
    assert!(h.has_request("/v5/account/wallet-balance"));
    assert!(
        h.logs
            .iter()
            .any(|line| line.contains("Skipped malformed public discovery row"))
    );
    let ticker = h.take("/v5/market/tickers");
    h.event(Event::HttpFailed {
        request: ticker.id,
        error: "scripted outage",
    });
    assert!(h.has_request("/v5/account/wallet-balance"));
    h.advance(1000.0);
    assert!(h.has_request("/v5/market/tickers"));
}

#[test]
fn catalogue_failure_uses_shared_exponential_backoff() {
    let mut h = Harness::started(Options {
        public_data_only: true,
        ..Default::default()
    });
    h.respond("status=PreLaunch", 200, PRELAUNCH);
    for delay in [1000.0, 2000.0, 4000.0] {
        let read = h.take("/v5/market/instruments-info");
        h.event(Event::HttpFailed {
            request: read.id,
            error: "scripted outage",
        });
        assert!(!h.has_request("/v5/market/instruments-info"));
        h.advance(delay);
        assert!(h.has_request("/v5/market/instruments-info"));
    }
    h.respond("/v5/market/instruments-info", 200, INSTRUMENTS);
    assert!(h.engine.store().loaded().markets);
}

#[test]
fn wholly_malformed_required_trading_catalogue_retries_and_never_replaces_prior_markets() {
    for cached in [false, true] {
        let prior = if cached {
            Harness::new().engine.store().markets().to_vec()
        } else {
            vec![]
        };
        let mut h = Harness::started(Options {
            public_data_only: true,
            ..Default::default()
        });
        if cached {
            h.engine.store_mut().set_markets(prior.clone());
        }
        h.respond("status=PreLaunch", 200, PRELAUNCH);
        h.respond("/v5/market/instruments-info", 200,
            r#"{"retCode":0,"retMsg":"OK","result":{"category":"linear","list":[{"broken":true}],"nextPageCursor":""}}"#);
        assert_eq!(h.engine.store().markets(), prior);
        assert_eq!(h.engine.store().loaded().markets, cached);
        h.advance(1000.0);
        assert!(h.has_request("/v5/market/instruments-info"));
        h.respond("/v5/market/instruments-info", 200, INSTRUMENTS);
        assert!(h.engine.store().loaded().markets);
        assert!(h.engine.store().market("BTCUSDT").is_some());
    }
}

fn reply_with_headers(h: &mut Harness, path: &str, status: u16, headers: &str) {
    let request = h.take(path).id;
    h.event(Event::HttpResponse {
        request,
        response: safe_cex_engine::Response {
            status,
            headers,
            body: "",
        },
    });
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
    let mut h = Harness::started(Options {
        public_data_only: true,
        ..Default::default()
    });
    h.respond("status=PreLaunch", 200, PRELAUNCH);
    reply_with_headers(
        &mut h,
        "/v5/market/instruments-info",
        429,
        "Retry-After: 120",
    );
    assert_retry_waits(&mut h, "/v5/market/instruments-info", 120_000.0);
}

#[test]
fn ticker_ip_ban_retry_waits_ten_minutes() {
    let mut h = Harness::started(Options {
        public_data_only: true,
        ..Default::default()
    });
    h.respond("status=PreLaunch", 200, PRELAUNCH);
    h.respond("/v5/market/instruments-info", 200, INSTRUMENTS);
    reply_with_headers(&mut h, "/v5/market/tickers", 403, "");
    assert_retry_waits(&mut h, "/v5/market/tickers", 600_000.0);
}

#[test]
fn a_market_without_a_positive_step_is_skipped_as_malformed() {
    for (filter, field, bad) in [
        ("priceFilter", "tickSize", "0"),
        ("lotSizeFilter", "qtyStep", "-0.01"),
        ("priceFilter", "tickSize", ""),
    ] {
        let mut h = Harness::started(Options {
            public_data_only: true,
            ..Default::default()
        });
        h.respond("status=PreLaunch", 200, PRELAUNCH);
        let mut body: serde_json::Value = serde_json::from_str(INSTRUMENTS).unwrap();
        let eth = body["result"]["list"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|row| row["symbol"] == "ETHUSDT")
            .unwrap();
        eth[filter][field] = bad.into();
        h.respond("/v5/market/instruments-info", 200, &body.to_string());
        assert!(h.engine.store().market("BTCUSDT").is_some());
        assert!(
            h.engine.store().market("ETHUSDT").is_none(),
            "{field}={bad:?}"
        );
        assert!(
            h.logs
                .iter()
                .any(|line| line.contains("Skipped malformed public discovery row"))
        );
    }
}
