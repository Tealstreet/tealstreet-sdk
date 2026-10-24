use safe_cex_engine::{Action, Event, Response};
use safe_cex_venue_polymarket::{Options, Polymarket};

fn released(
    engine: &mut safe_cex_engine::Engine<Polymarket>,
    now: f64,
) -> std::vec::IntoIter<Action> {
    safe_cex_engine::test_support::prepared_actions(engine, now).into_iter()
}

/// A jittered retry delay lands in `[0.75, 1.0] * nominal`.
fn jittered(actual: f64, nominal: f64) -> bool {
    (0.75 * nominal..=nominal).contains(&actual)
}

// A transport failure must yield to the host clock; malformed individual rows
// cannot discard valid neighbours from the subsequent successful catalogue.
#[test]
fn public_discovery_retries_with_backoff_and_skips_one_malformed_row() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Polymarket::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    engine.start(0.0);
    let mut request = released(&mut engine, 0.)
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
        let actions = released(&mut engine, now).collect::<Vec<_>>();
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
        request = released(&mut engine, now)
            .find_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .unwrap();
        assert_eq!(request.url, original_url);
    }
    engine.handle(now, Event::HttpResponse { request: request.id, response: Response { status: 200, headers: "", body: r###"{"markets":[{"conditionId":"0x123","slug":"election","question":"Who wins?","description":"Test outcome","endDate":"2026-12-31","clobTokenIds":"[\"107505882767731489358349912513945399560393482969656700824895970500493757150417\",\"2\"]","outcomes":"[\"Yes\",\"No\"]","orderPriceMinTickSize":0.01,"orderMinSize":"","active":true,"closed":false,"negRisk":true},{"broken":{"unexpected":true}}]}"### } });
    assert!(engine.store().loaded().markets);
    assert_eq!(engine.store().markets().len(), 2);
    let actions = released(&mut engine, now).collect::<Vec<_>>();
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains("Skipped malformed public discovery row"))));
}

#[test]
fn private_start_is_independent_of_ticker_seed_responses() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Polymarket::new(Options {
        key: Some("0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80".into()),
        ..Options::default()
    }));
    engine.start(0.0);
    let request = released(&mut engine, 0.)
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    engine.handle(0.0, Event::HttpResponse { request: request.id, response: Response { status: 200, headers: "", body: r###"{"markets":[{"conditionId":"0x123","slug":"election","question":"Who wins?","description":"Test outcome","endDate":"2026-12-31","clobTokenIds":"[\"107505882767731489358349912513945399560393482969656700824895970500493757150417\",\"2\"]","outcomes":"[\"Yes\",\"No\"]","orderPriceMinTickSize":0.01,"orderMinSize":"","active":true,"closed":false,"negRisk":true},{"broken":{"unexpected":true}}]}"### } });
    assert!(engine.store().loaded().markets);
    assert!(!engine.store().loaded().tickers);
    let actions = released(&mut engine, 0.).collect::<Vec<_>>();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Http(request) if request.url.ends_with("/time"))),
        "private snapshot did not start: {actions:?}"
    );
}

#[test]
fn all_rejected_catalogue_keeps_current_store_and_recovers_after_backoff() {
    const GOOD: &str = r###"{"markets":[{"conditionId":"0x123","slug":"election","question":"Who wins?","description":"Test outcome","endDate":"2026-12-31","clobTokenIds":"[\"107505882767731489358349912513945399560393482969656700824895970500493757150417\",\"2\"]","outcomes":"[\"Yes\",\"No\"]","orderPriceMinTickSize":0.01,"orderMinSize":"","active":true,"closed":false,"negRisk":true},{"broken":{"unexpected":true}}]}"###;
    const BAD: &str = r###"{"markets":[{"conditionId":"x","slug":"bad","clobTokenIds":"[\"1\",\"2\"]","outcomes":"[\"Yes\"]"}]}"###;
    for seeded in [false, true] {
        let options = || Options {
            public_data_only: true,
            ..Options::default()
        };
        let mut previous = safe_cex_engine::test_support::seeded_engine(Polymarket::new(options()));
        previous.start(0.0);
        let seed = released(&mut previous, 0.)
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
        let mut engine = safe_cex_engine::test_support::seeded_engine(Polymarket::new(options()));
        if seeded {
            engine
                .store_mut()
                .set_markets(previous.store().markets().to_vec());
            engine
                .store_mut()
                .set_tickers(previous.store().tickers().to_vec());
            engine
                .store_mut()
                .set_prediction_markets(previous.store().prediction_markets().unwrap().to_vec());
        }
        engine.start(0.0);
        let request = released(&mut engine, 0.)
            .find_map(|action| match action {
                Action::Http(request) => Some(request),
                _ => None,
            })
            .unwrap();
        let before = engine.snapshot_frame(0.0).to_vec();
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
        assert_eq!(engine.snapshot_frame(0.0), before);
        assert_eq!(engine.store().loaded().markets, seeded);
        let actions: Vec<_> = released(&mut engine, 0.).collect();
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
        let recovered = released(&mut engine, 1000.)
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
