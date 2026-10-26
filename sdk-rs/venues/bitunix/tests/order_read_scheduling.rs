mod support;
use safe_cex_engine::{Action, Event, HttpRequest, Response, SocketId, TimerId};
use support::*;
const BASE: f64 = 2000.0;
fn ready() -> (TestEngine, SocketId) {
    startup()
}
fn ready_with_signer(
    fail: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
) -> (TestEngine, SocketId) {
    let mut e = support::engine(support::Options {
        key: Some("replay-key".into()),
        secret: Some("replay-secret".into()),
        ..support::Options::default()
    });
    if let Some(fail) = fail {
        e.set_signing_entropy(Box::new(move |bytes| {
            if fail.load(std::sync::atomic::Ordering::SeqCst) {
                return Err("controlled signing unavailable".into());
            }
            bytes.fill(7);
            Ok(())
        }))
        .unwrap();
    }
    startup_with_engine(e)
}
const UPDATE: &str = r#"{"ch":"order","data":{"event":"UPDATE","orderId":"one","symbol":"BTCUSDT","qty":"3","dealAmount":"1"}}"#;
fn notify(e: &mut TestEngine, socket: SocketId, at: f64) -> Vec<Action> {
    e.handle(
        BASE + at,
        Event::WsMessage {
            socket,
            text: UPDATE,
        },
    );
    safe_cex_engine::test_support::prepared_now(e)
        .into_iter()
        .collect()
}
fn timer_at(actions: &[Action], at: f64) -> TimerId {
    actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == BASE + at => Some(*timer),
            _ => None,
        })
        .expect("scheduled deadline")
}
fn fire(e: &mut TestEngine, timer: TimerId, at: f64) -> Vec<Action> {
    e.handle(BASE + at, Event::TimerFired { timer });
    safe_cex_engine::test_support::prepared_now(e)
        .into_iter()
        .collect()
}
fn order_requests(actions: Vec<Action>) -> Vec<HttpRequest> {
    actions
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(r) if r.url.contains("get_pending_orders") => Some(r),
            _ => None,
        })
        .collect()
}
fn answer(
    e: &mut TestEngine,
    request: safe_cex_engine::RequestId,
    at: f64,
    body: &str,
) -> Vec<Action> {
    e.handle(
        BASE + at,
        Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
    safe_cex_engine::test_support::prepared_now(e)
        .into_iter()
        .collect()
}
fn begin(e: &mut TestEngine, socket: SocketId) -> Vec<Action> {
    let first = notify(e, socket, 100.0);
    fire(e, timer_at(&first, 1100.0), 1100.0)
}
#[test]
fn continuous_notifications_keep_the_first_order_read_deadline() {
    let (mut e, socket) = ready();
    let first = notify(&mut e, socket, 100.0);
    let timer = timer_at(&first, 1100.0);
    for at in [200.0, 500.0, 1000.0] {
        let actions = notify(&mut e, socket, at);
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a,Action::ClearTimer{timer:old} if *old==timer))
        );
    }
    assert_eq!(order_requests(fire(&mut e, timer, 1100.0)).len(), 2);
}
#[test]
fn ordinary_refresh_cannot_supersede_slow_regular_or_tpsl_reads() {
    let (mut e, socket) = ready();
    let first = begin(&mut e, socket);
    assert_eq!(
        first
            .iter()
            .filter(|a| matches!(a,Action::Http(r) if r.url.contains("get_pending_orders")))
            .count(),
        2
    );
    let notification = notify(&mut e, socket, 1200.0);
    let actions = fire(&mut e, timer_at(&notification, 2200.0), 2200.0);
    assert!(
        order_requests(actions).is_empty(),
        "each slow endpoint keeps its serial"
    );
}
#[test]
fn lost_endpoint_replacement_and_late_callbacks_leave_the_other_deadline_intact() {
    for late_failure in [false, true] {
        let (mut e, socket) = ready();
        let actions = begin(&mut e, socket);
        let deadlines: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == BASE + 11100.0 => Some(*timer),
                _ => None,
            })
            .collect();
        assert_eq!(deadlines.len(), 2, "each endpoint owns a deadline");
        let mut reads = order_requests(actions);
        let regular = reads.remove(
            reads
                .iter()
                .position(|r| r.url.contains("/trade/"))
                .unwrap(),
        );
        let tpsl = reads.remove(0);
        answer(&mut e, tpsl.id, 2000.0, TPSL);
        let notification = notify(&mut e, socket, 2500.0);
        let refreshed = fire(&mut e, timer_at(&notification, 3500.0), 3500.0);
        let fresh_tpsl_deadline = timer_at(&refreshed, 13500.0);
        let refreshed = order_requests(refreshed);
        assert_eq!(refreshed.len(), 1);
        assert!(refreshed[0].url.contains("/tpsl/"));
        let mut replacements = Vec::new();
        for timer in deadlines {
            replacements.extend(order_requests(fire(&mut e, timer, 11100.0)));
        }
        assert_eq!(replacements.len(), 1);
        assert!(replacements[0].url.contains("/trade/"));
        if late_failure {
            e.handle(
                BASE + 11200.0,
                Event::HttpFailed {
                    request: regular.id,
                    error: "late old read",
                },
            );
            safe_cex_engine::test_support::prepared_now(&mut e)
                .into_iter()
                .for_each(drop);
        } else {
            answer(&mut e, regular.id, 11200.0, ORDERS);
        }
        let next = order_requests(fire(&mut e, fresh_tpsl_deadline, 13500.0));
        assert_eq!(next.len(), 1);
        assert!(next[0].url.contains("/tpsl/"));
    }
}
fn failed_retry_timers(actions: &[Action], at: f64) -> Vec<(TimerId, f64)> {
    let mut timers: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms }
                if *at_ms >= BASE + at + 750.0 && *at_ms <= BASE + at + 1000.0 =>
            {
                Some((*timer, *at_ms - BASE))
            }
            _ => None,
        })
        .collect();
    timers.sort_by(|a, b| a.1.total_cmp(&b.1));
    timers
}
#[test]
fn failed_reads_release_both_tokens_for_independent_bounded_repair() {
    for failure in ["transport", "native", "missing", "malformed"] {
        let (mut e, socket) = ready();
        let reads = order_requests(begin(&mut e, socket));
        let mut actions = Vec::new();
        for r in reads {
            if failure == "transport" {
                e.handle(
                    BASE + 2000.0,
                    Event::HttpFailed {
                        request: r.id,
                        error: "read unavailable",
                    },
                );
                actions.extend(safe_cex_engine::test_support::prepared_now(&mut e));
            } else {
                actions.extend(answer(
                    &mut e,
                    r.id,
                    2000.0,
                    match failure {
                        "native" => r#"{"code":10001,"data":[]}"#,
                        "missing" => r#"{"code":0,"data":{}}"#,
                        _ => r#"{"code":0,"data":["#,
                    },
                ));
            }
        }
        let repairs = failed_retry_timers(&actions, 2000.0);
        assert_eq!(
            repairs.len(),
            2,
            "one bounded retry per endpoint: {failure}"
        );
        let mut next = Vec::new();
        for (timer, at) in repairs {
            next.extend(order_requests(fire(&mut e, timer, at)));
        }
        assert_eq!(next.len(), 2, "{failure}");
        assert_eq!(next.iter().filter(|r| r.url.contains("/trade/")).count(), 1);
        assert_eq!(next.iter().filter(|r| r.url.contains("/tpsl/")).count(), 1);
    }
}

#[test]
fn synchronous_signing_failure_releases_each_endpoint_before_repair() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let unavailable = Arc::new(AtomicBool::new(false));
    let (mut e, socket) = ready_with_signer(Some(unavailable.clone()));
    unavailable.store(true, Ordering::SeqCst);
    let actions = begin(&mut e, socket);
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    let repairs = failed_retry_timers(&actions, 1100.0);
    assert_eq!(
        repairs.len(),
        2,
        "both signing failures must release their token and retry"
    );
    unavailable.store(false, Ordering::SeqCst);
    let mut next = Vec::new();
    for (timer, at) in repairs {
        next.extend(order_requests(fire(&mut e, timer, at)));
    }
    assert_eq!(next.len(), 2);
    assert_eq!(next.iter().filter(|r| r.url.contains("/trade/")).count(), 1);
    assert_eq!(next.iter().filter(|r| r.url.contains("/tpsl/")).count(), 1);
}
#[test]
fn ordinary_refresh_cannot_supersede_owned_order_preflight_or_relax_its_generation_fence() {
    use safe_cex_engine::{Command, Method, Notice};
    let (mut e, socket) = ready();
    let order=serde_json::from_str(r#"{"symbol":"BTCUSDT","type":"take_profit_market","side":"buy","price":85000,"amount":0,"tpTriggerType":"mark_price"}"#).unwrap();
    e.handle(
        BASE + 100.0,
        Event::Command(Command::PlaceOrders {
            command: 73,
            orders: vec![order],
        }),
    );
    let positions = requests(&mut e)
        .into_iter()
        .find(|r| r.url.contains("get_pending_positions"))
        .unwrap();
    let actions = answer(&mut e, positions.id, 200.0, POSITIONS);
    let owned = order_requests(actions)
        .into_iter()
        .find(|r| r.url.contains("/trade/"))
        .unwrap();
    let actions = notify(&mut e, socket, 300.0);
    let background = fire(&mut e, timer_at(&actions, 1300.0), 1300.0);
    assert!(order_requests(background).is_empty());
    let actions = answer(&mut e, owned.id, 2000.0, ORDERS);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a,Action::Http(r) if r.method==Method::Post))
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 73, .. })))
    );
}
#[test]
fn an_early_stall_event_rearms_only_its_remaining_deadline() {
    let (mut e, socket) = ready();
    let actions = begin(&mut e, socket);
    let deadline = timer_at(&actions, 11100.0);
    let early = fire(&mut e, deadline, 11099.5);
    assert!(!early.iter().any(|a| matches!(a, Action::Http(_))));
    let rearmed = timer_at(&early, 11100.0);
    let replaced = order_requests(fire(&mut e, rearmed, 11100.0));
    assert_eq!(replaced.len(), 1);
}

#[test]
fn exhausted_admission_retires_both_stall_owners_and_does_not_rearm_them() {
    let (mut e, socket) = ready();
    let issued = begin(&mut e, socket);
    let deadlines: Vec<_> = issued
        .iter()
        .filter_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == BASE + 11100.0 => Some(*timer),
            _ => None,
        })
        .collect();
    assert_eq!(deadlines.len(), 2);
    let queued = notify(&mut e, socket, 1200.0);
    let debounce = timer_at(&queued, 2200.0);
    safe_cex_engine::test_support::exhaust_order_snapshots(e.store_mut());
    let retired = fire(&mut e, deadlines[0], 11100.0);
    assert!(
        !retired
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
    // The engine already consumed the fired timer; the sibling must be canceled.
    for timer in [deadlines[1], debounce] {
        assert!(retired.iter().any(|action| matches!(action,
            Action::ClearTimer { timer: cleared } if *cleared == timer)));
    }
    // Duplicate and sibling expired events have no stale timer ownership to rearm.
    for timer in [deadlines[0], deadlines[1], debounce] {
        let actions = fire(&mut e, timer, 11101.0);
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::Http(_) | Action::SetTimer { .. }))
        );
    }
    assert!(e.store().loaded().orders);
    assert!(e.store().order("one").is_some());
}

#[test]
fn exhausted_order_authority_settles_owned_preflight_once_before_any_write() {
    use safe_cex_engine::{Command, Method, Notice};
    let (mut e, _) = ready();
    safe_cex_engine::test_support::exhaust_order_snapshots(e.store_mut());
    for (command, at) in [(91, 100.0), (92, 300.0)] {
        let order = serde_json::from_str(r#"{"symbol":"BTCUSDT","type":"take_profit_market","side":"buy","price":85000,"amount":0,"tpTriggerType":"mark_price"}"#).unwrap();
        e.handle(
            BASE + at,
            Event::Command(Command::PlaceOrders {
                command,
                orders: vec![order],
            }),
        );
        let positions = requests(&mut e)
            .into_iter()
            .find(|r| r.url.contains("get_pending_positions"))
            .unwrap();
        let actions = answer(&mut e, positions.id, at + 100.0, POSITIONS);
        assert_eq!(
            actions
                .iter()
                .filter(|action| matches!(action,
            Action::Notice(Notice::CommandFailed { command: failed, .. }) if *failed == command))
                .count(),
            1
        );
        assert!(!actions.iter().any(|action| matches!(action,
            Action::Http(r) if r.method == Method::Post || r.url.contains("get_pending_orders"))));
        assert!(!actions.iter().any(|action| matches!(action,
            Action::Notice(Notice::CommandDone { command: done, .. }) if *done == command)));
    }
}
