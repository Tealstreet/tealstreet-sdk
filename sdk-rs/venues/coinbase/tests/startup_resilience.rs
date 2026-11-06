use safe_cex_engine::{Action, Event, Notice, Response};
use safe_cex_venue_coinbase::{Coinbase, Options};

/// A jittered retry delay lands in `[0.75, 1.0] * nominal`.
fn jittered(actual: f64, nominal: f64) -> bool {
    (0.75 * nominal..=nominal).contains(&actual)
}

// A transport failure must yield to the host clock; malformed individual rows
// cannot discard valid neighbours from the subsequent successful catalogue.
#[test]
fn public_discovery_retries_with_backoff_and_skips_one_malformed_row() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(
        Coinbase::new(Options {
            public_data_only: true,
            spot_only: true,
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
                Action::Notice(Notice::Event(safe_cex_engine::LogEvent {
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
        request = engine
            .drain_actions()
            .find_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .unwrap();
        assert_eq!(request.url, original_url);
    }
    // Skipped discovery rows log at Debug.
    engine.set_log_level(safe_cex_engine::Level::Debug);
    engine.handle(now, Event::HttpResponse { request: request.id, response: Response { status: 200, headers: "", body: r###"{"products":[{"product_type":"SPOT","product_id":"BTC-USD","base_currency_id":"BTC","quote_currency_id":"USD","status":"TRADING","base_increment":"0.0001","price_increment":"0.1","base_min_size":"0.0001","base_max_size":"10000","price":"60000","volume_24h":"20","price_percentage_change_24h":"1"},{"broken":{"unexpected":true}}],"pagination":{"has_next":false}}"### } });
    assert!(engine.store().loaded().markets);
    assert_eq!(engine.store().markets().len(), 1);
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains("Skipped malformed public discovery row"))));
}
