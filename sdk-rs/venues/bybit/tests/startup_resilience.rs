mod support;
use safe_cex_engine::Event;
use support::Options;
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
    // Skipped discovery rows log at Debug.
    h.engine.set_log_level(safe_cex_engine::Level::Debug);
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
        h.engine.set_log_level(safe_cex_engine::Level::Debug);
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

mod discovery_envelope_limits {
    use super::*;

    const LIMITED: &str = r#"{"retCode":10006,"retMsg":"Too many visits!","result":{}}"#;
    const MARKETS: &str = "/v5/market/instruments-info";
    const TICKERS_PATH: &str = "/v5/market/tickers";

    fn started() -> Harness {
        let mut h = Harness::started(Options {
            public_data_only: true,
            ..Default::default()
        });
        h.respond("status=PreLaunch", 200, PRELAUNCH);
        h
    }

    fn reply(h: &mut Harness, path: &str, status: u16, headers: &str, body: &str) {
        let request = h.take(path);
        h.event(Event::HttpResponse {
            request: request.id,
            response: safe_cex_engine::Response {
                status,
                headers,
                body,
            },
        });
    }

    #[test]
    fn catalogue_limit_keeps_reset_headers_with_or_without_a_list() {
        for body in [
            LIMITED,
            r#"{"retCode":10006,"retMsg":"Too many visits!","result":{"list":[]}}"#,
        ] {
            let mut h = started();
            let headers = format!("X-Bapi-Limit-Reset-Timestamp: {}", h.now + 120_000.0);
            reply(&mut h, MARKETS, 200, &headers, body);
            assert!(!h.engine.store().loaded().markets);
            assert_retry_waits(&mut h, MARKETS, 120_000.0);
            h.respond(MARKETS, 200, INSTRUMENTS);
            assert!(h.engine.store().loaded().markets);
        }
    }

    #[test]
    fn ticker_limit_uses_the_later_reset_or_retry_after_hint() {
        for (reset_ms, retry_seconds, wait_ms) in
            [(30_000.0, 90, 90_000.0), (120_000.0, 10, 120_000.0)]
        {
            let mut h = started();
            h.respond(MARKETS, 200, INSTRUMENTS);
            let headers = format!(
                "x-BAPI-limit-reset-timestamp: {}\r\nRetry-After: {retry_seconds}",
                h.now + reset_ms
            );
            reply(&mut h, TICKERS_PATH, 200, &headers, LIMITED);
            assert_retry_waits(&mut h, TICKERS_PATH, wait_ms);
        }
    }

    #[test]
    fn unusable_reset_hints_use_the_existing_ten_second_fallback() {
        for value in [
            None,
            Some("bad"),
            Some("NaN"),
            Some("inf"),
            Some("-inf"),
            Some("0"),
            Some("1000"),
        ] {
            let mut h = started();
            let headers = value.map_or_else(String::new, |value| {
                format!("X-Bapi-Limit-Reset-Timestamp: {value}")
            });
            reply(&mut h, MARKETS, 200, &headers, LIMITED);
            assert_retry_waits(&mut h, MARKETS, 10_000.0);
        }
    }

    #[test]
    fn valid_short_hints_do_not_inherit_the_missing_hint_fallback() {
        for reset in [true, false] {
            let mut h = started();
            let headers = if reset {
                format!("X-Bapi-Limit-Reset-Timestamp: {}", h.now + 2000.0)
            } else {
                "Retry-After: 2".into()
            };
            reply(&mut h, MARKETS, 200, &headers, LIMITED);
            assert_retry_waits(&mut h, MARKETS, 2000.0);
        }
    }

    #[test]
    fn absurd_finite_reset_is_bounded_to_the_shared_one_hour_cap() {
        let mut h = started();
        reply(
            &mut h,
            MARKETS,
            200,
            "X-Bapi-Limit-Reset-Timestamp: 1e100",
            LIMITED,
        );
        assert_retry_waits(&mut h, MARKETS, 3_600_000.0);
    }

    #[test]
    fn reset_header_on_success_or_other_business_error_does_not_pause_discovery() {
        for success in [true, false] {
            let mut h = started();
            let headers = format!("X-Bapi-Limit-Reset-Timestamp: {}", h.now + 120_000.0);
            if success {
                reply(&mut h, MARKETS, 200, &headers, INSTRUMENTS);
                let read = h.take(TICKERS_PATH);
                h.event(Event::HttpFailed {
                    request: read.id,
                    error: "scripted outage",
                });
            } else {
                reply(
                    &mut h,
                    MARKETS,
                    200,
                    &headers,
                    r#"{"retCode":10001,"retMsg":"bad parameter","result":{}}"#,
                );
            }
            h.advance(1000.0);
            assert!(h.has_request(if success { TICKERS_PATH } else { MARKETS }));
        }
    }

    #[test]
    fn envelope_reset_cannot_shorten_the_http403_floor() {
        let mut h = started();
        let headers = format!("X-Bapi-Limit-Reset-Timestamp: {}", h.now + 2000.0);
        reply(&mut h, MARKETS, 403, &headers, LIMITED);
        assert_retry_waits(&mut h, MARKETS, 600_000.0);
    }

    #[test]
    fn a_shorter_later_hint_does_not_replace_an_existing_window() {
        let mut h = started();
        let headers = format!("X-Bapi-Limit-Reset-Timestamp: {}", h.now + 120_000.0);
        reply(&mut h, MARKETS, 200, &headers, LIMITED);
        let headers = format!("X-Bapi-Limit-Reset-Timestamp: {}", h.now + 30_000.0);
        reply(&mut h, TICKERS_PATH, 200, &headers, LIMITED);
        assert_retry_waits(&mut h, MARKETS, 120_000.0);
    }

    fn take_category(h: &mut Harness, category: &str) -> safe_cex_engine::HttpRequest {
        let index = h
            .requests
            .iter()
            .position(|r| {
                r.url.contains(MARKETS) && r.url.contains(&format!("category={category}"))
            })
            .expect("category catalogue request");
        h.requests.remove(index)
    }

    fn has_category(h: &Harness, category: &str) -> bool {
        h.requests
            .iter()
            .any(|r| r.url.contains(MARKETS) && r.url.contains(&format!("category={category}")))
    }

    #[test]
    fn another_categorys_limit_extends_an_armed_retry_without_increasing_attempts() {
        let mut h = Harness::started(Options {
            public_data_only: true,
            enable_inverse_markets: true,
            ..Default::default()
        });
        for _ in 0..2 {
            h.respond("status=PreLaunch", 200, PRELAUNCH);
        }
        let linear = take_category(&mut h, "linear");
        h.event(Event::HttpFailed {
            request: linear.id,
            error: "scripted outage",
        });
        let old_timer = h
            .timers
            .iter()
            .find(|(_, at)| *at <= h.now + 1000.0)
            .unwrap()
            .0;
        h.advance(250.0);
        let inverse = take_category(&mut h, "inverse");
        let headers = format!("X-Bapi-Limit-Reset-Timestamp: {}", h.now + 60_000.0);
        h.event(Event::HttpResponse {
            request: inverse.id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: &headers,
                body: LIMITED,
            },
        });
        assert!(!h.timers.iter().any(|(timer, _)| *timer == old_timer));
        h.advance(59_999.0);
        assert!(!has_category(&h, "linear"));
        assert!(!has_category(&h, "inverse"));
        h.advance(1.0);
        assert!(has_category(&h, "inverse"));
        let linear = take_category(&mut h, "linear");
        h.event(Event::HttpFailed {
            request: linear.id,
            error: "scripted outage",
        });
        // Second failure still uses the second 1.5-2 s jittered backoff step.
        h.advance(1499.0);
        assert!(!has_category(&h, "linear"));
        h.advance(501.0);
        assert!(has_category(&h, "linear"));
    }
}
