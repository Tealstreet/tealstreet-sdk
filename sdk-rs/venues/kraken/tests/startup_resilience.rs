use safe_cex_engine::{Action, Event, Response};
use safe_cex_venue_kraken::{Kraken, Options};

/// A failed-read retry delay lies in the engine's jittered `[0.75, 1.0] * nominal`.
fn jittered(actual: f64, nominal: f64) -> bool {
    (0.75 * nominal..=nominal).contains(&actual)
}

// A transport failure must yield to the host clock; malformed individual rows
// cannot discard valid neighbours from the subsequent successful catalogue.
#[test]
fn public_discovery_retries_with_backoff_and_skips_one_malformed_row() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Kraken::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    let mut now = 0.0;
    engine.start(now);
    let mut request = safe_cex_engine::test_support::prepared_actions(&mut engine, now)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    for delay in [1000.0, 2000.0] {
        let original_url = request.url.clone();
        engine.handle(
            now,
            Event::HttpFailed {
                request: request.id,
                error: "scripted public failure",
            },
        );
        let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, now);
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
    engine.handle(now, Event::HttpResponse { request: request.id, response: Response { status: 200, headers: "", body: r###"{"result":"success","instruments":[{"symbol":"PF_XBTUSD","base":"BTC","quote":"USD","tradeable":true,"tickSize":0.5,"contractSize":1,"contractValueTradePrecision":3,"marginLevels":[{"initialMargin":0.02}]},{"symbol":"PF_ETHUSD","base":"ETH","quote":"USD","tradeable":true,"tickSize":0.1,"contractSize":1,"contractValueTradePrecision":2,"marginLevels":[{"initialMargin":0.05}]},{"symbol":"PF_INVALIDUSD","contractValueTradePrecision":"malformed"}]}"### } });
    assert!(engine.store().loaded().markets);
    assert_eq!(engine.store().markets().len(), 2);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, now);
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains("Skipped malformed public discovery row"))));
}

#[test]
fn private_start_is_independent_of_ticker_seed_responses() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Kraken::new(Options {
        key: Some("replay-key".into()),
        secret: Some("cmVwbGF5LXNlY3JldA==".into()),
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
    engine.handle(0.0, Event::HttpResponse { request: request.id, response: Response { status: 200, headers: "", body: r###"{"result":"success","instruments":[{"symbol":"PF_XBTUSD","base":"BTC","quote":"USD","tradeable":true,"tickSize":0.5,"contractSize":1,"contractValueTradePrecision":3,"marginLevels":[{"initialMargin":0.02}]},{"symbol":"PF_ETHUSD","base":"ETH","quote":"USD","tradeable":true,"tickSize":0.1,"contractSize":1,"contractValueTradePrecision":2,"marginLevels":[{"initialMargin":0.05}]},{"symbol":"PF_INVALIDUSD","contractValueTradePrecision":"malformed"}]}"### } });
    assert!(engine.store().loaded().markets);
    assert!(!engine.store().loaded().tickers);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.0);
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Http(request) if request.url.contains("/accounts"))),
        "private snapshot did not start: {actions:?}"
    );
}
