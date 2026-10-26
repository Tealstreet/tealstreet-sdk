use safe_cex_engine::{Action, Event, Response};
use safe_cex_venue_paradex::{Options, Paradex};

/// A jittered retry delay lands in `[0.75, 1.0] * nominal`.
fn jittered(actual: f64, nominal: f64) -> bool {
    (0.75 * nominal..=nominal).contains(&actual)
}

// A transport failure must yield to the host clock; malformed individual rows
// cannot discard valid neighbours from the subsequent successful catalogue.
#[test]
fn public_discovery_retries_with_backoff_and_skips_one_malformed_row() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Paradex::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    engine.start(0.0);
    let mut request = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.0)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    let mut now = 0.0;
    for delay in [1000.0, 2000.0] {
        let original_url = request.url.clone();
        engine.handle(
            now,
            Event::HttpFailed {
                request: request.id,
                error: "scripted public failure",
            },
        );
        let actions = engine.drain_actions().collect::<Vec<_>>();
        assert!(
            !actions.iter().any(|action| matches!(
                action,
                Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
                    level: safe_cex_engine::Level::Error,
                    ..
                }))
            )),
            "public retry must not invalidate account credentials: {actions:?}"
        );
        assert!(
            !actions.iter().any(|a| matches!(a, Action::Http(_))),
            "read retried without backoff: {actions:?}"
        );
        let timer = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if jittered(*at_ms - now, delay) => Some(*timer),
                _ => None,
            })
            .expect("bounded public discovery backoff");
        now += delay;
        engine.handle(now, Event::TimerFired { timer });
        request = safe_cex_engine::test_support::prepared_actions(&mut engine, now)
            .into_iter()
            .find_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .unwrap();
        assert_eq!(request.url, original_url);
    }
    // Skipped discovery rows log at Debug.
    engine.set_log_level(safe_cex_engine::Level::Debug);
    engine.handle(now, Event::HttpResponse { request: request.id, response: Response { status: 200, headers: "", body: r###"{"results":[{"symbol":"BTC-USD-PERP","base_currency":"BTC","quote_currency":"USD","asset_kind":"PERP","order_size_increment":"0.00001","price_tick_size":"0.1","min_notional":"10","max_order_size":"100","delta1_cross_margin_params":{"imf_base":"0.02"},"maker_fee":""},{"symbol":"ETH-USD-PERP","base_currency":"ETH","quote_currency":"USD","asset_kind":"PERP","order_size_increment":"0.001","price_tick_size":"0.01","min_notional":"10","max_order_size":"1000","initial_margin_fraction":"0.05"},{"symbol":"BTC-USD-250101-100000-C","asset_kind":"OPTION"},{"symbol":"USDC-USD-SPOT","asset_kind":"SPOT"},{"symbol":"OLD-USD-PERP","asset_kind":"PERP","status":"INACTIVE"},{"symbol":"INVALID-USD-PERP","step_size":"malformed"}]}"### } });
    assert!(engine.store().loaded().markets);
    assert_eq!(engine.store().markets().len(), 2);
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains("Skipped malformed public discovery row"))));
}

#[test]
fn private_start_is_independent_of_ticker_seed_responses() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Paradex::new(Options {
        key: Some(format!("0x{}1", "0".repeat(63))),
        paradex_address: Some("0x0123".into()),
        subkey_address: Some("0x456".into()),
        ..Options::default()
    }));
    engine.start(0.0);
    let request = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.0)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    engine.handle(0.0, Event::HttpResponse { request: request.id, response: Response { status: 200, headers: "", body: r###"{"results":[{"symbol":"BTC-USD-PERP","base_currency":"BTC","quote_currency":"USD","asset_kind":"PERP","order_size_increment":"0.00001","price_tick_size":"0.1","min_notional":"10","max_order_size":"100","delta1_cross_margin_params":{"imf_base":"0.02"},"maker_fee":""},{"symbol":"ETH-USD-PERP","base_currency":"ETH","quote_currency":"USD","asset_kind":"PERP","order_size_increment":"0.001","price_tick_size":"0.01","min_notional":"10","max_order_size":"1000","initial_margin_fraction":"0.05"},{"symbol":"BTC-USD-250101-100000-C","asset_kind":"OPTION"},{"symbol":"USDC-USD-SPOT","asset_kind":"SPOT"},{"symbol":"OLD-USD-PERP","asset_kind":"PERP","status":"INACTIVE"},{"symbol":"INVALID-USD-PERP","step_size":"malformed"}]}"### } });
    assert!(engine.store().loaded().markets);
    assert!(!engine.store().loaded().tickers);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.0);
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Http(request) if request.url.ends_with("/system/config"))),
        "private snapshot did not start: {actions:?}"
    );
}

#[test]
fn all_rejected_catalogue_keeps_current_store_and_recovers_after_backoff() {
    const GOOD: &str = r###"{"results":[{"symbol":"BTC-USD-PERP","base_currency":"BTC","quote_currency":"USD","asset_kind":"PERP","order_size_increment":"0.00001","price_tick_size":"0.1","min_notional":"10","max_order_size":"100","delta1_cross_margin_params":{"imf_base":"0.02"},"maker_fee":""},{"symbol":"ETH-USD-PERP","base_currency":"ETH","quote_currency":"USD","asset_kind":"PERP","order_size_increment":"0.001","price_tick_size":"0.01","min_notional":"10","max_order_size":"1000","initial_margin_fraction":"0.05"},{"symbol":"BTC-USD-250101-100000-C","asset_kind":"OPTION"},{"symbol":"USDC-USD-SPOT","asset_kind":"SPOT"},{"symbol":"OLD-USD-PERP","asset_kind":"PERP","status":"INACTIVE"},{"symbol":"INVALID-USD-PERP","step_size":"malformed"}]}"###;
    const BAD: &str = r###"{"results":[{"symbol":"BAD-PERP","step_size":"malformed"}]}"###;
    for seeded in [false, true] {
        let options = || Options {
            public_data_only: true,
            ..Options::default()
        };
        let mut previous = safe_cex_engine::test_support::seeded_engine(Paradex::new(options()));
        previous.start(0.0);
        let seed = safe_cex_engine::test_support::prepared_actions(&mut previous, 0.0)
            .into_iter()
            .find_map(|action| match action {
                Action::Http(request) => Some(request),
                _ => None,
            })
            .unwrap();
        previous.handle(
            0.0,
            Event::HttpResponse {
                request: seed.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: GOOD,
                },
            },
        );
        let mut engine = safe_cex_engine::test_support::seeded_engine(Paradex::new(options()));
        if seeded {
            engine
                .store_mut()
                .set_markets(previous.store().markets().to_vec());
            engine
                .store_mut()
                .set_tickers(previous.store().tickers().to_vec());
        }
        engine.start(0.0);
        let request = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.0)
            .into_iter()
            .find_map(|action| match action {
                Action::Http(request) => Some(request),
                _ => None,
            })
            .unwrap();
        let before = serde_json::to_value(engine.store().markets()).unwrap();
        let tickers = serde_json::to_value(engine.store().tickers()).unwrap();
        engine.handle(
            0.0,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: BAD,
                },
            },
        );
        assert_eq!(
            serde_json::to_value(engine.store().markets()).unwrap(),
            before
        );
        assert_eq!(
            serde_json::to_value(engine.store().tickers()).unwrap(),
            tickers
        );
        assert_eq!(engine.store().loaded().markets, seeded);
        let actions: Vec<_> = engine.drain_actions().collect();
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::Http(_)))
        );
        assert!(actions.iter().any(|action| matches!(
            action,
            Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
                level: safe_cex_engine::Level::Warn,
                ..
            }))
        )));
        let timer = actions
            .iter()
            .find_map(|action| match action {
                Action::SetTimer { timer, at_ms } if jittered(*at_ms, 1000.0) => Some(*timer),
                _ => None,
            })
            .unwrap();
        engine.handle(1000.0, Event::TimerFired { timer });
        let recovered = safe_cex_engine::test_support::prepared_actions(&mut engine, 1000.0)
            .into_iter()
            .find_map(|action| match action {
                Action::Http(request) => Some(request),
                _ => None,
            })
            .unwrap();
        assert_eq!(recovered.url, request.url);
        engine.handle(
            1000.0,
            Event::HttpResponse {
                request: recovered.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: GOOD,
                },
            },
        );
        assert!(engine.store().loaded().markets);
        assert_eq!(engine.store().markets().len(), 2);
    }
}
