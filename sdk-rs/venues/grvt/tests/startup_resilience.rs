use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{Action, Event, Response};
use safe_cex_venue_grvt::{Grvt, Options};

/// A jittered retry delay lands in `[0.75, 1.0] * nominal`.
fn jittered(actual: f64, nominal: f64) -> bool {
    (0.75 * nominal..=nominal).contains(&actual)
}

// A transport failure must yield to the host clock; malformed individual rows
// cannot discard valid neighbours from the subsequent successful catalogue.
#[test]
fn public_discovery_retries_with_backoff_and_skips_one_malformed_row() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Grvt::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    engine.start(0.0);
    let mut request = prepared_actions(&mut engine, 0.0)
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
        request = prepared_actions(&mut engine, now)
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
    engine.handle(now, Event::HttpResponse { request: request.id, response: Response { status: 200, headers: "", body: r###"{"r":[{"i":"BTC_USDT_Perp","ih":"0x030501","b":"BTC","q":"USDT","k":"PERPETUAL","ts":"0.1","ms":"0.001","mp":"1000","bd":9,"qd":6},{"i":"ETH_USDT_Perp","ih":"0x030502","b":"ETH","q":"USDT","k":"PERPETUAL","ts":"0.01","ms":"0.01","bd":9,"qd":6},{"broken":{"unexpected":true}}]}"### } });
    assert!(engine.store().loaded().markets);
    assert_eq!(engine.store().markets().len(), 2);
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains("Skipped malformed public discovery row"))));
}

#[test]
fn empty_or_all_malformed_catalogue_retains_current_state_and_recovers_after_backoff() {
    const GOOD: &str = r#"{"r":[{"i":"BTC_USDT_Perp","ih":"0x030501","b":"BTC","q":"USDT","k":"PERPETUAL","ts":"0.1","ms":"0.001","mp":"1000","bd":9,"qd":6}]}"#;
    for seeded in [false, true] {
        for bad in [
            r#"{"r":[]}"#,
            r#"{"r":[{"broken":true},{"i":"BTC_USDT_Perp","ih":"0xinvalid","k":"PERPETUAL","ts":"invalid","ms":"0.001"}]}"#,
        ] {
            let mut engine = safe_cex_engine::test_support::seeded_engine(Grvt::new(Options {
                public_data_only: true,
                ..Options::default()
            }));
            engine.start(0.0);
            let mut request = prepared_actions(&mut engine, 0.0)
                .into_iter()
                .find_map(|action| match action {
                    Action::Http(request) => Some(request),
                    _ => None,
                })
                .unwrap();
            let now = if seeded {
                engine.handle(
                    0.0,
                    Event::HttpResponse {
                        request: request.id,
                        response: Response {
                            status: 200,
                            headers: "",
                            body: GOOD,
                        },
                    },
                );
                engine
                    .store_mut()
                    .update_ticker("BTC_USDT_PERP", |ticker| ticker.last = 123.0);
                let refresh = engine
                    .drain_actions()
                    .find_map(|action| match action {
                        Action::SetTimer {
                            timer,
                            at_ms: 300_000.0,
                        } => Some(timer),
                        _ => None,
                    })
                    .unwrap();
                engine.handle(300_000.0, Event::TimerFired { timer: refresh });
                request = prepared_actions(&mut engine, 300_000.0)
                    .into_iter()
                    .find_map(|action| match action {
                        Action::Http(request) => Some(request),
                        _ => None,
                    })
                    .unwrap();
                300_000.0
            } else {
                0.0
            };
            let before = engine.snapshot_frame(0.0).to_vec();
            engine.handle(
                now,
                Event::HttpResponse {
                    request: request.id,
                    response: Response {
                        status: 200,
                        headers: "",
                        body: bad,
                    },
                },
            );
            assert_eq!(engine.snapshot_frame(0.0), before);
            assert_eq!(engine.store().loaded().markets, seeded);
            let actions: Vec<_> = engine.drain_actions().collect();
            assert!(
                !actions
                    .iter()
                    .any(|action| matches!(action, Action::Http(_)))
            );
            assert!(actions.iter().any(|action| matches!(action, Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains("Invalid or empty GRVT instrument catalog"))));
            let retry = actions
                .iter()
                .find_map(|action| match action {
                    Action::SetTimer { timer, at_ms } if jittered(*at_ms - now, 1000.0) => {
                        Some(*timer)
                    }
                    _ => None,
                })
                .unwrap();
            engine.handle(now + 1000.0, Event::TimerFired { timer: retry });
            let recovered = prepared_actions(&mut engine, now + 1000.0)
                .into_iter()
                .find_map(|action| match action {
                    Action::Http(request) => Some(request),
                    _ => None,
                })
                .unwrap();
            assert_eq!(recovered.url, request.url);
            engine.handle(
                now + 1000.0,
                Event::HttpResponse {
                    request: recovered.id,
                    response: Response {
                        status: 200,
                        headers: "",
                        body: GOOD,
                    },
                },
            );
            assert_eq!(engine.store().markets().len(), 1);
            assert!(engine.store().loaded().markets);
            if seeded {
                assert_eq!(engine.store().ticker("BTC_USDT_PERP").unwrap().last, 123.0);
            }
        }
    }
}
