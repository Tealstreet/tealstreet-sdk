use safe_cex_engine::{Action, Engine, Event, Notice, Response};
use safe_cex_venue_apexomni::{ApexOmni, Options};

// A transport failure must yield to the host clock; malformed individual rows
// cannot discard valid neighbours from the subsequent successful catalogue.
#[test]
fn public_discovery_retries_with_backoff_and_skips_one_malformed_row() {
    let mut engine = Engine::new(ApexOmni::new(Options {
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
    engine.handle(now, Event::HttpResponse { request: request.id, response: Response { status: 200, headers: "", body: r###"{"code":0,"data":{"contractConfig":{"perpetualContract":[{"symbol":"BTC-USDT","crossSymbolName":"BTCUSDT","baseTokenId":"BTC","settleAssetId":"USDT","enableTrade":true,"stepSize":"0.001","tickSize":"0.1","minOrderSize":"0.001","maxOrderSize":"1000","displayMinLeverage":"1","displayMaxLeverage":"100","l2PairId":"50000","initialMarginRate":"","defaultInitialMarginRate":"0.05","maxMarketPriceRange":"0.045"},{"enableTrade":true,"tickSize":"malformed"}]}}}"### } });
    assert!(engine.store().loaded().markets);
    assert_eq!(engine.store().markets().len(), 1);
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::Log { message, .. }) if message.contains("Skipped malformed public discovery row"))));
}

// A contract without a positive default margin rate has no leverage fallback.
#[test]
fn contract_without_positive_default_margin_rate_is_malformed() {
    let mut engine = Engine::new(ApexOmni::new(Options {
        public_data_only: true,
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
    let row = |name: &str, rate: &str| {
        format!(
            r#"{{"symbol":"{name}-USDT","crossSymbolName":"{name}USDT","baseTokenId":"{name}","settleAssetId":"USDT","enableTrade":true,"stepSize":"0.001","tickSize":"0.1","minOrderSize":"0.001","maxOrderSize":"1000","displayMinLeverage":"1","displayMaxLeverage":"100","l2PairId":"50000","initialMarginRate":"0.01"{rate},"maxMarketPriceRange":"0.045"}}"#
        )
    };
    let body = format!(
        r#"{{"code":0,"data":{{"contractConfig":{{"perpetualContract":[{},{},{},{}]}}}}}}"#,
        row("BTC", r#","defaultInitialMarginRate":"0.05""#),
        row("ETH", r#","defaultInitialMarginRate":"""#),
        row("SOL", r#","defaultInitialMarginRate":"0""#),
        row("XRP", ""),
    );
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
    let symbols = engine
        .store()
        .markets()
        .iter()
        .map(|m| m.symbol.as_str())
        .collect::<Vec<_>>();
    assert_eq!(symbols, ["BTCUSDT"]);
}
