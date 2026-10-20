use safe_cex_engine::{Action, Engine, Event, Notice, Response};
use safe_cex_venue_paradex::{Options, Paradex};

// A transport failure must yield to the host clock; malformed individual rows
// cannot discard valid neighbours from the subsequent successful catalogue.
#[test]
fn public_discovery_retries_with_backoff_and_skips_one_malformed_row() {
    let mut engine = Engine::new(Paradex::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    engine.start(0.0);
    let mut request = engine
        .drain_actions()
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
            !actions.iter().any(|a| matches!(a, Action::Http(_))),
            "read retried without backoff: {actions:?}"
        );
        let timer = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == now + delay => Some(*timer),
                _ => None,
            })
            .expect("bounded public discovery backoff");
        now += delay;
        engine.handle(now, Event::TimerFired { timer });
        request = engine
            .drain_actions()
            .find_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .unwrap();
        assert_eq!(request.url, original_url);
    }
    engine.handle(now, Event::HttpResponse { request: request.id, response: Response { status: 200, headers: "", body: r###"{"results":[{"symbol":"BTC-USD-PERP","base_currency":"BTC","quote_currency":"USD","asset_kind":"PERP","order_size_increment":"0.00001","price_tick_size":"0.1","min_notional":"10","max_order_size":"100","delta1_cross_margin_params":{"imf_base":"0.02"},"maker_fee":""},{"symbol":"ETH-USD-PERP","base_currency":"ETH","quote_currency":"USD","asset_kind":"PERP","order_size_increment":"0.001","price_tick_size":"0.01","min_notional":"10","max_order_size":"1000","initial_margin_fraction":"0.05"},{"symbol":"BTC-USD-250101-100000-C","asset_kind":"OPTION"},{"symbol":"USDC-USD-SPOT","asset_kind":"SPOT"},{"symbol":"OLD-USD-PERP","asset_kind":"PERP","status":"INACTIVE"},{"symbol":"INVALID-USD-PERP","step_size":"malformed"}]}"### } });
    assert!(engine.store().loaded().markets);
    assert_eq!(engine.store().markets().len(), 2);
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::Log { message, .. }) if message.contains("Skipped malformed public discovery row"))));
}

#[test]
fn private_start_is_independent_of_ticker_seed_responses() {
    let mut engine = Engine::new(Paradex::new(Options {
        key: Some(format!("0x{}1", "0".repeat(63))),
        paradex_address: Some("0x0123".into()),
        subkey_address: Some("0x456".into()),
        ..Options::default()
    }));
    engine.start(0.0);
    let request = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    engine.handle(0.0, Event::HttpResponse { request: request.id, response: Response { status: 200, headers: "", body: r###"{"results":[{"symbol":"BTC-USD-PERP","base_currency":"BTC","quote_currency":"USD","asset_kind":"PERP","order_size_increment":"0.00001","price_tick_size":"0.1","min_notional":"10","max_order_size":"100","delta1_cross_margin_params":{"imf_base":"0.02"},"maker_fee":""},{"symbol":"ETH-USD-PERP","base_currency":"ETH","quote_currency":"USD","asset_kind":"PERP","order_size_increment":"0.001","price_tick_size":"0.01","min_notional":"10","max_order_size":"1000","initial_margin_fraction":"0.05"},{"symbol":"BTC-USD-250101-100000-C","asset_kind":"OPTION"},{"symbol":"USDC-USD-SPOT","asset_kind":"SPOT"},{"symbol":"OLD-USD-PERP","asset_kind":"PERP","status":"INACTIVE"},{"symbol":"INVALID-USD-PERP","step_size":"malformed"}]}"### } });
    assert!(engine.store().loaded().markets);
    assert!(!engine.store().loaded().tickers);
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Http(request) if request.url.ends_with("/system/config"))),
        "private snapshot did not start: {actions:?}"
    );
}
