use safe_cex_engine::{Action, Engine, Event, Notice, Response};
use safe_cex_venue_kraken::{Kraken, Options};

// A transport failure must yield to the host clock; malformed individual rows
// cannot discard valid neighbours from the subsequent successful catalogue.
#[test]
fn public_discovery_retries_with_backoff_and_skips_one_malformed_row() {
    let mut engine = Engine::new(Kraken::new(Options {
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
            !actions.iter().any(|action| matches!(
                action,
                Action::Notice(Notice::Log {
                    level: safe_cex_engine::LogLevel::Error,
                    ..
                })
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
    engine.handle(now, Event::HttpResponse { request: request.id, response: Response { status: 200, headers: "", body: r###"{"result":"success","instruments":[{"symbol":"PF_XBTUSD","base":"BTC","quote":"USD","tradeable":true,"tickSize":0.5,"contractSize":1,"contractValueTradePrecision":3,"marginLevels":[{"initialMargin":0.02}]},{"symbol":"PF_ETHUSD","base":"ETH","quote":"USD","tradeable":true,"tickSize":0.1,"contractSize":1,"contractValueTradePrecision":2,"marginLevels":[{"initialMargin":0.05}]},{"symbol":"PF_INVALIDUSD","contractValueTradePrecision":"malformed"}]}"### } });
    assert!(engine.store().loaded().markets);
    assert_eq!(engine.store().markets().len(), 2);
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::Log { message, .. }) if message.contains("Skipped malformed public discovery row"))));
}

#[test]
fn private_start_is_independent_of_ticker_seed_responses() {
    let mut engine = Engine::new(Kraken::new(Options {
        key: Some("replay-key".into()),
        secret: Some("cmVwbGF5LXNlY3JldA==".into()),
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
    engine.handle(0.0, Event::HttpResponse { request: request.id, response: Response { status: 200, headers: "", body: r###"{"result":"success","instruments":[{"symbol":"PF_XBTUSD","base":"BTC","quote":"USD","tradeable":true,"tickSize":0.5,"contractSize":1,"contractValueTradePrecision":3,"marginLevels":[{"initialMargin":0.02}]},{"symbol":"PF_ETHUSD","base":"ETH","quote":"USD","tradeable":true,"tickSize":0.1,"contractSize":1,"contractValueTradePrecision":2,"marginLevels":[{"initialMargin":0.05}]},{"symbol":"PF_INVALIDUSD","contractValueTradePrecision":"malformed"}]}"### } });
    assert!(engine.store().loaded().markets);
    assert!(!engine.store().loaded().tickers);
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Http(request) if request.url.contains("/accounts"))),
        "private snapshot did not start: {actions:?}"
    );
}
