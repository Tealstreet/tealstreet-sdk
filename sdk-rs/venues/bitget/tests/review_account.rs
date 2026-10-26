mod support;

use safe_cex_engine::{Command, Event, Query};
use support::*;

/// Native cadence of the venue's recurring account poll; policy scales it.
const POLL_MS: f64 = 2_000.0;

fn late_uta_query(h: &mut Harness) {
    h.command(Command::Query {
        command: 97,
        query: Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    });
    h.respond(
        "/order/fills",
        200,
        r#"{"code":"40085","msg":"Classic Account API is not supported","data":null}"#,
    );
}

fn fire_next_timer(h: &mut Harness) {
    let (timer, at) = h
        .timers
        .iter()
        .copied()
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .expect("pending query has a timer");
    h.timers.retain(|(id, _)| *id != timer);
    h.now = at;
    h.event(Event::TimerFired { timer });
}

#[test]
fn confirmed_uta_survives_settings_transport_and_html_server_failures() {
    for transport_failure in [true, false] {
        let mut h = Harness::classic();
        late_uta_query(&mut h);
        let probe = h.take("/account/settings");
        let old_requests = h.requests.len();
        if transport_failure {
            h.event(Event::HttpFailed {
                request: probe.id,
                error: "temporary connection reset",
            });
        } else {
            h.reply(probe.id, 503, "<html>temporary gateway failure</html>");
        }
        assert!(
            h.requests[old_requests..]
                .iter()
                .all(|r| !r.url.contains("/api/v2/")),
            "confirmed UTA was downgraded after a transient settings failure: {:?}",
            h.requests
        );
        for _ in 0..20 {
            if h.requests
                .iter()
                .any(|r| r.url.contains("/account/settings"))
            {
                break;
            }
            fire_next_timer(&mut h);
        }
        h.respond(
            "/account/settings",
            200,
            r#"{"code":"00000","data":{"holdMode":"hedge_mode"}}"#,
        );
        for _ in 0..20 {
            if h.requests.iter().any(|r| r.url.contains("/v3/trade/fills")) {
                break;
            }
            fire_next_timer(&mut h);
        }
        h.respond(
            "/v3/trade/fills",
            200,
            r#"{"code":"00000","data":{"list":[],"cursor":""}}"#,
        );
        assert_eq!(
            h.done.iter().find(|(id, _)| *id == 97).unwrap().1,
            Ok("[]".into())
        );
    }
}

#[test]
fn late_uta_detection_that_never_responds_completes_with_a_bounded_error() {
    let mut h = Harness::classic();
    late_uta_query(&mut h);
    for _ in 0..250 {
        if h.done.iter().any(|(id, _)| *id == 97) {
            break;
        }
        fire_next_timer(&mut h);
    }
    let result = &h.done.iter().find(|(id, _)| *id == 97).unwrap().1;
    assert!(result.as_ref().unwrap_err().contains("account detection"));
    assert!(h.now <= 1_010_100.0);
}

#[test]
fn repeated_confirmed_uta_probe_failures_cannot_restart_classic_detection() {
    let mut h = Harness::classic();
    late_uta_query(&mut h);
    for _ in 0..4 {
        for _ in 0..200 {
            if h.requests
                .iter()
                .any(|r| r.url.contains("/account/settings"))
            {
                break;
            }
            fire_next_timer(&mut h);
        }
        let probe = h.take("/account/settings");
        let old_requests = h.requests.len();
        h.event(Event::HttpFailed {
            request: probe.id,
            error: "settings temporarily unavailable",
        });
        assert!(
            h.requests[old_requests..]
                .iter()
                .all(|r| !r.url.contains("/api/v2/"))
        );
    }
    for _ in 0..250 {
        if h.done.iter().any(|(id, _)| *id == 97) {
            break;
        }
        fire_next_timer(&mut h);
    }
    let result = &h.done.iter().find(|(id, _)| *id == 97).unwrap().1;
    assert!(result.as_ref().unwrap_err().contains("account detection"));
    assert!(h.now <= 1_010_100.0);
}

#[test]
fn repeated_nonempty_account_order_cursor_preserves_the_previous_snapshot() {
    let mut h = Harness::classic();
    let first = h.take("/orders-pending");
    let page = OPEN_ORDER
        .replace("\"orderId\":\"1\"", "\"orderId\":\"2\"")
        .replace("\"endId\":\"\"", "\"endId\":\"repeated\"");
    h.reply(first.id, 200, &page);
    h.respond("idLessThan=repeated", 200, &page);
    for path in ["planType=profit_loss", "planType=track_plan"] {
        if h.requests.iter().any(|r| r.url.contains(path)) {
            h.respond(path, 200, r#"{"code":"00000","data":{"entrustedList":[]}}"#);
        }
    }
    assert!(h.engine.store().order("1").is_some());
    assert!(h.engine.store().order("2").is_none());
    assert!(h.logs.iter().any(|message| message.contains("cursor")));
}

// Drive the actual account loop through slow/fast updates, a parked stale callback,
// and a resumed read. Other timers are deliberately not fired.
fn policy_poll_read<V: safe_cex_engine::Venue>(
    engine: &mut safe_cex_engine::Engine<V>,
    now: f64,
    endpoint: &str,
) -> (f64, Vec<safe_cex_engine::HttpRequest>) {
    use safe_cex_engine::test_support::prepared_actions;
    use safe_cex_engine::{Action, Event};
    engine.drain_actions().for_each(drop);
    engine.handle(
        now,
        Event::PollScaleChanged {
            scale: 60_000.0 / POLL_MS,
        },
    );
    let slow: Vec<_> = prepared_actions(engine, now)
        .into_iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            Action::Http(_) => panic!("policy updates must not issue account reads"),
            _ => None,
        })
        .collect();
    assert_eq!(slow.len(), 1, "exactly one recurring account poll");
    assert_eq!(slow[0].1, now + 60_000.0);
    engine.handle(
        now + 1.0,
        Event::PollScaleChanged {
            scale: 1_500.0 / POLL_MS,
        },
    );
    let fast: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .collect();
    assert_eq!(fast.len(), 1);
    assert_eq!(fast[0].1, now + 1_501.0);
    engine.handle(
        now + 2.0,
        Event::PollScaleChanged {
            scale: f64::INFINITY,
        },
    );
    let paused: Vec<_> = engine.drain_actions().collect();
    assert!(
        paused
            .iter()
            .any(|a| matches!(a, Action::ClearTimer { timer } if *timer == fast[0].0))
    );
    assert!(!paused.iter().any(|a| matches!(a, Action::SetTimer { .. })));
    engine.handle(now + 1_501.0, Event::TimerFired { timer: fast[0].0 });
    assert!(
        !prepared_actions(engine, now + 1_501.0)
            .iter()
            .any(|a| matches!(a, Action::Http(_)))
    );
    engine.handle(
        now + 90_000.0,
        Event::PollScaleChanged {
            scale: 1_500.0 / POLL_MS,
        },
    );
    let resumed: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .collect();
    assert_eq!(resumed.len(), 1);
    assert_eq!(resumed[0].1, now + 91_500.0);
    engine.handle(
        now + 91_500.0,
        Event::TimerFired {
            timer: resumed[0].0,
        },
    );
    let reads: Vec<_> = prepared_actions(engine, now + 91_500.0)
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect();
    assert!(
        reads.iter().any(|r| r.url.contains(endpoint)),
        "actual resumed account HTTP: {reads:?}"
    );
    let completed_at = now + 91_501.0;
    engine.handle(
        completed_at,
        Event::PollScaleChanged {
            scale: f64::INFINITY,
        },
    );
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::SetTimer { .. }))
    );
    (completed_at, reads)
}

fn policy_poll_after_completion<V: safe_cex_engine::Venue>(
    engine: &mut safe_cex_engine::Engine<V>,
    now: f64,
) {
    use safe_cex_engine::{Action, Event};
    // A response may schedule an ordinary repair/deadline, but the parked poll
    // itself must still be registered and adopt this newest policy.
    engine.drain_actions().for_each(drop);
    engine.handle(
        now,
        Event::PollScaleChanged {
            scale: 30_000.0 / POLL_MS,
        },
    );
    let times: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { at_ms, .. } => Some(at_ms),
            _ => None,
        })
        .collect();
    assert_eq!(times, vec![now + 30_000.0]);
}

#[test]
fn recurring_account_poll_obeys_live_policy_and_completion() {
    let mut h = Harness::classic();
    // Opening the private socket admits one more startup reconciliation.
    h.respond("/all-position", 200, POSITION);
    h.respond("/account/accounts", 200, ACCOUNT);
    h.orders(OPEN_ORDER);
    let now = h.now;
    let engine = &mut h.engine;
    let (now, mut reads) = policy_poll_read(engine, now, "/all-position");
    for _ in 0..12 {
        if reads.is_empty() {
            break;
        }
        for r in reads {
            let body = if r.url.contains("all-position") {
                POSITION
            } else if r.url.contains("account/accounts") {
                ACCOUNT
            } else {
                r#"{"code":"00000","data":[]}"#
            };
            engine.handle(
                now,
                safe_cex_engine::Event::HttpResponse {
                    request: r.id,
                    response: safe_cex_engine::Response {
                        status: 200,
                        headers: "",
                        body,
                    },
                },
            );
        }
        reads = safe_cex_engine::test_support::prepared_actions(engine, now)
            .into_iter()
            .filter_map(|a| match a {
                safe_cex_engine::Action::Http(r) => Some(r),
                _ => None,
            })
            .collect();
    }
    assert!(reads.is_empty(), "account cycle must complete");
    policy_poll_after_completion(engine, now);
}
