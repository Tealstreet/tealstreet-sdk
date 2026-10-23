#![allow(dead_code)]
mod support;
use crate::support::FixtureOptions as Options;
use safe_cex_engine::{Action, Engine, Event, Level, Notice, RequestId, Response};
use safe_cex_venue_okx::Okx;

fn request(out: &[Action], path: &str) -> RequestId {
    out.iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains(path) => Some(request.id),
            _ => None,
        })
        .expect("expected discovery request")
}
fn reply(e: &mut Engine<Okx>, request: RequestId, now: f64, body: &str) -> Vec<Action> {
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
    e.drain_actions().collect()
}
fn events(out: &[Action], code: &str) -> usize {
    out.iter()
        .filter(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.code == code))
        .count()
}

#[test]
fn public_startup_degrades_once_keeps_retrying_and_recovers() {
    for fail_tickers in [false, true] {
        let mut e = crate::support::engine(Options {
            public_data_only: true,
            ..Default::default()
        });
        e.start(0.0);
        let out: Vec<_> = e.drain_actions().collect();
        let mut pending = request(&out, "public/instruments");
        let mut now = 0.0;
        if fail_tickers {
            let out = reply(
                &mut e,
                pending,
                now,
                &crate::support::envelope(crate::support::MARKET),
            );
            pending = request(&out, "market/tickers");
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
            let out: Vec<_> = e.drain_actions().collect();
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
            let out: Vec<_> = e.drain_actions().collect();
            pending = request(
                &out,
                if fail_tickers {
                    "market/tickers"
                } else {
                    "public/instruments"
                },
            );
        }
        let out = if fail_tickers {
            reply(
                &mut e,
                pending,
                now,
                &crate::support::envelope(crate::support::TICKER),
            )
        } else {
            let out = reply(
                &mut e,
                pending,
                now,
                &crate::support::envelope(crate::support::MARKET),
            );
            assert_eq!(
                events(&out, "startup.recovered"),
                0,
                "tickers still required"
            );
            let pending = request(&out, "market/tickers");
            reply(
                &mut e,
                pending,
                now,
                &crate::support::envelope(crate::support::TICKER),
            )
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
fn empty_aggregate_catalogue_keeps_startup_retrying() {
    let mut e = crate::support::engine(Options {
        public_data_only: true,
        ..Default::default()
    });
    e.start(0.0);
    let out: Vec<_> = e.drain_actions().collect();
    let pending = request(&out, "public/instruments");
    let out = reply(&mut e, pending, 0.0, &crate::support::envelope(""));
    assert!(!e.store().loaded().markets);
    assert_eq!(events(&out, "request.failed"), 1);
    let timer = out
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(*timer),
            _ => None,
        })
        .expect("empty catalogue requires retry");
    e.handle(1000.0, Event::TimerFired { timer });
    let out: Vec<_> = e.drain_actions().collect();
    let pending = request(&out, "public/instruments");
    reply(
        &mut e,
        pending,
        1000.0,
        &crate::support::envelope(crate::support::MARKET),
    );
    assert!(e.store().loaded().markets);
}

#[test]
fn venue_rejection_reports_latest_parsed_cause_without_raw_body() {
    let mut e = crate::support::engine(Options {
        public_data_only: true,
        ..Default::default()
    });
    e.start(0.0);
    let out: Vec<_> = e.drain_actions().collect();
    let mut pending = request(&out, "public/instruments");
    let mut now = 0.0;
    for attempt in 1..=6 {
        // Both normal streamed completion and the missing-list callback keep the cause.
        let data = if attempt % 2 == 0 { "{}" } else { "[]" };
        let body = format!(
            r#"{{"code":"50011","msg":"maintenance {attempt}","data":{data},"opaque":"RAW_BODY_CANARY"}}"#
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
            assert!(event.fields.contains(&("venueCode", "50011".into())));
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
        let out: Vec<_> = e.drain_actions().collect();
        pending = request(&out, "public/instruments");
    }
}

#[test]
fn malformed_body_does_not_leak_preview_into_degradation() {
    let mut e = crate::support::engine(Options {
        public_data_only: true,
        ..Default::default()
    });
    e.start(0.0);
    let out: Vec<_> = e.drain_actions().collect();
    let mut pending = request(&out, "public/instruments");
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
        let out: Vec<_> = e.drain_actions().collect();
        pending = request(&out, "public/instruments");
    }
}
