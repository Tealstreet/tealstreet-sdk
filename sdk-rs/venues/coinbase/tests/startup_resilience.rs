use safe_cex_engine::{Action, Engine, Event, Notice, Response};
use safe_cex_venue_coinbase::{Coinbase, Options};

// A transport failure must yield to the host clock; malformed individual rows
// cannot discard valid neighbours from the subsequent successful catalogue.
#[test]
fn public_discovery_retries_with_backoff_and_skips_one_malformed_row() {
    let mut engine = Engine::new(
        Coinbase::new(Options {
            public_data_only: true,
            ..Options::default()
        })
        .unwrap(),
    );
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
    engine.handle(now, Event::HttpResponse { request: request.id, response: Response { status: 200, headers: "", body: r###"[{"symbol":"BTC-PERP","type":"PERP","trading_state":"TRADING","base_asset_name":"BTC","quote_asset_name":"USDC","base_increment":"0.0001","quote_increment":"0.1","min_quantity":"0.0001","qty_24hr":"20","notional_24hr":"1200000","open_interest":"100","funding_interval":"3600000000","quote":{"best_bid_price":"59999","best_ask_price":"60001","trade_price":"60000","mark_price":"60002","index_price":"60003","predicted_funding":"0.0001"}},{"broken":{"unexpected":true}}]"### } });
    assert!(engine.store().loaded().markets);
    assert_eq!(engine.store().markets().len(), 1);
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::Log { message, .. }) if message.contains("Skipped malformed public discovery row"))));
}
