mod support;
use safe_cex_engine::{Action, Engine, Event, Response};
use safe_cex_venue_bitrue::Bitrue;
use support::Options;

// A transport failure must yield to the host clock; malformed individual rows
// cannot discard valid neighbours from the subsequent successful catalogue.
#[test]
fn public_discovery_retries_with_backoff_and_skips_one_malformed_row() {
    let mut engine = support::engine(Options {
        public_data_only: true,
        ..Options::default()
    });
    engine.start(0.0);
    let mut request = drain(&mut engine, 0.0)
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
        let actions = drain(&mut engine, now).collect::<Vec<_>>();
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
                Action::SetTimer { timer, at_ms } if jittered(*at_ms - now, delay) => {
                    Some((*timer, *at_ms))
                }
                _ => None,
            })
            .expect("bounded public discovery backoff");
        let (timer, due) = timer;
        now = due;
        engine.handle(now, Event::TimerFired { timer });
        request = drain(&mut engine, now)
            .find_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .unwrap();
        assert_eq!(request.url, original_url);
    }
    // Skipped discovery rows log at Debug.
    engine.set_log_level(safe_cex_engine::Level::Debug);
    engine.handle(now, Event::HttpResponse { request: request.id, response: Response { status: 200, headers: "", body: r###"[{"symbol":"E-BTC-USDT","pricePrecision":2,"multiplier":0.001,"minOrderVolume":1,"maxMarketVolume":1000,"maxLimitVolume":2000,"minOrderMoney":"","status":1},{"symbol":"E-ETH-USDT","pricePrecision":2,"multiplier":0.01,"minOrderVolume":1,"maxMarketVolume":1000,"status":1},{"broken":{"unexpected":true}}]"### } });
    assert!(engine.store().loaded().markets);
    assert_eq!(engine.store().markets().len(), 2);
    let actions = drain(&mut engine, now).collect::<Vec<_>>();
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains("Skipped malformed public discovery row"))));
}

/// Managed reads are released only through engine preparation.
fn drain(engine: &mut Engine<Bitrue>, now: f64) -> std::vec::IntoIter<Action> {
    safe_cex_engine::test_support::prepared_actions(engine, now).into_iter()
}
/// Retry delays are jittered into `[0.75, 1.0] * nominal`.
fn jittered(actual: f64, nominal: f64) -> bool {
    (0.75 * nominal..=nominal).contains(&actual)
}
