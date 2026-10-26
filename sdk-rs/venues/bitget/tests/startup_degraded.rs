mod support;
use safe_cex_engine::{Action, Engine, Event, Level, Notice, RequestId, Response};
use safe_cex_venue_bitget::{BitgetFutures, Options};

fn request(out: &[Action], path: &str) -> RequestId {
    out.iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains(path) => Some(request.id),
            _ => None,
        })
        .expect("expected discovery request")
}
fn reply(e: &mut Engine<BitgetFutures>, request: RequestId, now: f64, body: &str) -> Vec<Action> {
    e.handle(
        now,
        Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
    safe_cex_engine::test_support::prepared_actions(e, now)
}
fn events(out: &[Action], code: &str) -> usize {
    out.iter()
        .filter(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.code == code))
        .count()
}

#[test]
fn public_startup_degrades_once_keeps_retrying_and_recovers() {
    for fail_tickers in [false, true] {
        let mut e = safe_cex_engine::test_support::seeded_engine(BitgetFutures::new(Options {
            public_data_only: true,
            ..Default::default()
        }));
        e.start(0.0);
        let out: Vec<_> = safe_cex_engine::test_support::prepared_actions(&mut e, 0.0);
        let mut pending = request(&out, "/contracts");
        let mut now = 0.0;
        if fail_tickers {
            let out = reply(&mut e, pending, now, support::CONTRACTS);
            pending = request(&out, "/tickers");
        }
        for (index, nominal) in [1000., 2000., 4000., 8000., 16000., 30000., 30000., 30000.]
            .into_iter()
            .enumerate()
        {
            let cause = format!("scripted outage {}", index + 1);
            e.handle(
                now,
                Event::HttpFailed {
                    request: pending,
                    error: &cause,
                },
            );
            let out: Vec<_> = safe_cex_engine::test_support::prepared_actions(&mut e, now);
            assert_eq!(
                events(&out, "request.failed"),
                1,
                "one coded retry per failure"
            );
            assert_eq!(
                events(&out, "startup.degraded"),
                usize::from(index == 5),
                "one threshold notice"
            );
            assert_eq!(events(&out, "startup.recovered"), 0);
            assert!(
                !out.iter()
                    .any(|a| matches!(a, Action::Notice(Notice::StartupFailed { .. })))
            );
            if index == 5 {
                let event = out
                    .iter()
                    .find_map(|a| match a {
                        Action::Notice(Notice::Event(event))
                            if event.code == "startup.degraded" =>
                        {
                            Some(event)
                        }
                        _ => None,
                    })
                    .unwrap();
                assert_eq!(event.level, Level::Warn);
                assert!(event.message.contains(&cause));
                assert!(event.fields.contains(&("kind", "network".into())));
                assert!(event.fields.contains(&(
                    "read",
                    if fail_tickers { "tickers" } else { "markets" }.into()
                )));
                assert!(event.fields.contains(&("attempts", 6u32.into())));
            }
            let (timer, at) = out
                .iter()
                .find_map(|a| match a {
                    Action::SetTimer { timer, at_ms }
                        if (0.75 * nominal..=nominal).contains(&(at_ms - now)) =>
                    {
                        Some((*timer, *at_ms))
                    }
                    _ => None,
                })
                .expect("continuing jittered capped retry");
            now = at;
            e.handle(now, Event::TimerFired { timer });
            let out: Vec<_> = safe_cex_engine::test_support::prepared_actions(&mut e, now);
            pending = request(
                &out,
                if fail_tickers {
                    "/tickers"
                } else {
                    "/contracts"
                },
            );
        }
        let out = if fail_tickers {
            reply(&mut e, pending, now, support::TICKERS)
        } else {
            let out = reply(&mut e, pending, now, support::CONTRACTS);
            assert_eq!(
                events(&out, "startup.recovered"),
                0,
                "tickers still required"
            );
            let pending = request(&out, "/tickers");
            reply(&mut e, pending, now, support::TICKERS)
        };
        assert!(e.store().loaded().markets && e.store().loaded().tickers);
        assert_eq!(events(&out, "startup.recovered"), 1);
        assert!(
            !out.iter()
                .any(|a| matches!(a, Action::Notice(Notice::StartupFailed { .. })))
        );
    }
}

#[test]
fn venue_rejection_reports_latest_parsed_cause_without_raw_body() {
    let mut e = safe_cex_engine::test_support::seeded_engine(BitgetFutures::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    e.start(0.0);
    let out: Vec<_> = safe_cex_engine::test_support::prepared_actions(&mut e, 0.0);
    let mut pending = request(&out, "/contracts");
    let mut now = 0.0;
    for attempt in 1..=6 {
        // Both normal streamed completion and the missing-list callback keep the cause.
        let data = if attempt % 2 == 0 { "{}" } else { "[]" };
        let body = format!(
            r#"{{"code":"40010","msg":"maintenance {attempt}","data":{data},"opaque":"RAW_BODY_CANARY"}}"#
        );
        let out = reply(&mut e, pending, now, &body);
        if attempt == 6 {
            let event = out
                .iter()
                .find_map(|a| match a {
                    Action::Notice(Notice::Event(e)) if e.code == "startup.degraded" => Some(e),
                    _ => None,
                })
                .unwrap();
            assert!(event.message.contains("maintenance 6"));
            assert!(!event.message.contains("RAW_BODY_CANARY"));
            assert!(event.fields.contains(&("venueCode", "40010".into())));
            assert!(
                event
                    .fields
                    .contains(&("venueMessage", "maintenance 6".into()))
            );
            assert!(event.fields.contains(&("kind", "venueUnavailable".into())));
        }
        let (timer, at) = out
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                _ => None,
            })
            .expect("retry remains armed");
        now = at;
        e.handle(now, Event::TimerFired { timer });
        let out: Vec<_> = safe_cex_engine::test_support::prepared_actions(&mut e, now);
        pending = request(&out, "/contracts");
    }
}

#[test]
fn malformed_body_does_not_leak_preview_into_degradation() {
    let mut e = safe_cex_engine::test_support::seeded_engine(BitgetFutures::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    e.start(0.0);
    let out: Vec<_> = safe_cex_engine::test_support::prepared_actions(&mut e, 0.0);
    let mut pending = request(&out, "/contracts");
    let mut now = 0.0;
    for attempt in 1..=6 {
        let out = reply(
            &mut e,
            pending,
            now,
            r#"{"opaque":"RAW_BODY_CANARY","data":["#,
        );
        if attempt == 6 {
            let event = out
                .iter()
                .find_map(|a| match a {
                    Action::Notice(Notice::Event(e)) if e.code == "startup.degraded" => Some(e),
                    _ => None,
                })
                .unwrap();
            assert!(!event.message.contains("RAW_BODY_CANARY"));
            assert!(event.fields.contains(&("kind", "venueUnavailable".into())));
        }
        let (timer, at) = out
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                _ => None,
            })
            .expect("retry remains armed");
        now = at;
        e.handle(now, Event::TimerFired { timer });
        let out: Vec<_> = safe_cex_engine::test_support::prepared_actions(&mut e, now);
        pending = request(&out, "/contracts");
    }
}
