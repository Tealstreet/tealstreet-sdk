mod support;
use safe_cex_engine::{Action, Event, HttpRequest, Response, SocketId, TimerId};
use support::*;
const UPDATE: &str = r#"{"ch":"position","data":{"event":"UPDATE","symbol":"BTCUSDT","positionId":"new-ws-id","side":"SELL"}}"#;
fn notify(e: &mut TestEngine, socket: SocketId, now: f64) -> Vec<Action> {
    e.handle(
        now,
        Event::WsMessage {
            socket,
            text: UPDATE,
        },
    );
    safe_cex_engine::test_support::prepared_now(e)
        .into_iter()
        .collect()
}
fn timer(actions: &[Action]) -> (TimerId, f64) {
    actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
            _ => None,
        })
        .unwrap()
}
fn read(e: &mut TestEngine, socket: SocketId) -> (HttpRequest, Vec<Action>) {
    let actions = notify(e, socket, 100.0);
    let (timer, at) = timer(&actions);
    e.handle(at, Event::TimerFired { timer });
    let mut actions: Vec<_> = safe_cex_engine::test_support::prepared_now(e)
        .into_iter()
        .collect();
    let at = actions
        .iter()
        .position(|a| matches!(a, Action::Http(r) if r.url.contains("get_pending_positions")))
        .unwrap();
    let Action::Http(request) = actions.remove(at) else {
        unreachable!()
    };
    (request, actions)
}
fn answer(e: &mut TestEngine, id: safe_cex_engine::RequestId, now: f64, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request: id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
#[test]
fn continuous_position_notifications_do_not_postpone_the_first_read() {
    let (mut e, socket) = startup();
    let actions = notify(&mut e, socket, 100.0);
    let (first, at) = timer(&actions);
    assert_eq!(at, 1100.0);
    for now in [200.0, 500.0, 1000.0] {
        let actions = notify(&mut e, socket, now);
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::ClearTimer { timer } if *timer == first)),
            "continuous activity cannot postpone reconciliation"
        );
    }
    e.handle(at, Event::TimerFired { timer: first });
    assert!(
        safe_cex_engine::test_support::prepared_now(&mut e)
            .into_iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("get_pending_positions")))
    );
}
#[test]
fn slow_position_snapshot_advances_financial_state_during_continuous_notifications() {
    for empty in [false, true] {
        let (mut e, socket) = startup();
        let (request, _) = read(&mut e, socket);
        for now in [1200.0, 1500.0, 2000.0, 2500.0] {
            let actions = notify(&mut e, socket, now);
            // Fire notification timers too: a slow request must not be superseded.
            for (timer, at) in actions.iter().filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                _ => None,
            }) {
                e.handle(at, Event::TimerFired { timer });
                assert!(
                    !safe_cex_engine::test_support::prepared_now(&mut e).into_iter().any(
                        |a| matches!(a, Action::Http(r) if r.url.contains("get_pending_positions"))
                    ),
                    "a pending position read must keep its serial"
                );
            }
        }
        let body = if empty {
            r#"{"code":0,"data":{"positionList":[]}}"#.into()
        } else {
            POSITIONS.replace("0.01", "0.02")
        };
        answer(&mut e, request.id, 5000.0, &body);
        if empty {
            assert!(e.store().positions().is_empty());
        } else {
            assert_eq!(e.store().positions()[0].contracts, 0.02);
        }
        let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
            .into_iter()
            .collect();
        let (followup, at) = timer(&actions);
        assert_eq!(at, 6000.0);
        e.handle(at, Event::TimerFired { timer: followup });
        assert!(
            safe_cex_engine::test_support::prepared_now(&mut e)
                .into_iter()
                .any(|a| matches!(a, Action::Http(r) if r.url.contains("get_pending_positions")))
        );
    }
}

#[test]
fn lost_position_read_is_replaced_at_a_bound_and_late_reply_or_failure_cannot_retire_it() {
    for late_failure in [false, true] {
        let (mut e, socket) = startup();
        let (old, actions) = read(&mut e, socket);
        let (deadline, at) = timer(&actions);
        assert_eq!(at, 11_100.0);
        notify(&mut e, socket, 2000.0);
        e.handle(at, Event::TimerFired { timer: deadline });
        let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
            .into_iter()
            .collect();
        let replacement = actions
            .iter()
            .find_map(|a| match a {
                Action::Http(r) if r.url.contains("get_pending_positions") => Some(r.id),
                _ => None,
            })
            .unwrap();
        let (new_watch, new_at) = timer(&actions);
        assert_eq!(new_at, at + 10_000.0);
        if late_failure {
            e.handle(
                at + 1.0,
                Event::HttpFailed {
                    request: old.id,
                    error: "late old failure",
                },
            );
        } else {
            answer(&mut e, old.id, at + 1.0, &POSITIONS.replace("0.01", "0.09"));
        }
        assert_eq!(e.store().positions()[0].contracts, 0.01);
        assert!(
            !safe_cex_engine::test_support::prepared_now(&mut e)
                .into_iter()
                .any(
                    |a| matches!(a, Action::ClearTimer { timer } if timer == new_watch)
                        || matches!(a, Action::SetTimer { .. })
                )
        );
        answer(
            &mut e,
            replacement,
            at + 2.0,
            &POSITIONS.replace("0.01", "0.02"),
        );
        assert_eq!(e.store().positions()[0].contracts, 0.02);
        assert!(
            safe_cex_engine::test_support::prepared_now(&mut e)
                .into_iter()
                .any(|a| matches!(a, Action::ClearTimer { timer } if timer == new_watch))
        );
    }
}

#[test]
fn failed_position_reads_release_the_matching_token_keep_state_and_retry() {
    for failure in ["transport", "native", "missing", "truncated"] {
        let (mut e, socket) = startup();
        let (request, _) = read(&mut e, socket);
        notify(&mut e, socket, 1200.0);
        match failure {
            "transport" => e.handle(
                2000.0,
                Event::HttpFailed {
                    request: request.id,
                    error: "read unavailable",
                },
            ),
            "native" => answer(&mut e, request.id, 2000.0, r#"{"code":10001,"data":[]}"#),
            "missing" => answer(&mut e, request.id, 2000.0, r#"{"code":0,"data":{}}"#),
            _ => answer(
                &mut e,
                request.id,
                2000.0,
                r#"{"code":0,"data":{"positionList":[{"symbol":"BTCUSDT"}"#,
            ),
        }
        assert_eq!(e.store().positions()[0].contracts, 0.01, "{failure}");
        let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
            .into_iter()
            .collect();
        let (timer, at) = actions
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms >= 2750.0 && *at_ms <= 3000.0 => {
                    Some((*timer, *at_ms))
                }
                _ => None,
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .expect("actual jittered failed-position retry");
        e.handle(at, Event::TimerFired { timer });
        assert!(
            safe_cex_engine::test_support::prepared_now(&mut e)
                .into_iter()
                .any(|a| matches!(a, Action::Http(r) if r.url.contains("get_pending_positions")))
        );
    }
}

#[test]
fn background_notifications_cannot_supersede_owned_preflight_or_approve_it_after_ws_change() {
    use safe_cex_engine::{Command, Method, Notice};
    let (mut e, socket) = startup();
    let order = serde_json::from_str(r#"{"symbol":"BTCUSDT","type":"take_profit_market","side":"buy","price":85000,"amount":0,"tpTriggerType":"mark_price"}"#).unwrap();
    e.handle(
        100.0,
        Event::Command(Command::PlaceOrders {
            command: 61,
            orders: vec![order],
        }),
    );
    let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .collect();
    let owned = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains("get_pending_positions") => Some(r.id),
            _ => None,
        })
        .unwrap();
    let (deadline, at) = timer(&actions);
    assert_eq!(at, 10_100.0);
    let actions = notify(&mut e, socket, 200.0);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::ClearTimer { timer } if *timer == deadline))
    );
    answer(&mut e, owned, 2000.0, POSITIONS);
    let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .collect();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.method == Method::Post))
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 61, .. })))
    );
    assert_eq!(e.store().positions()[0].contracts, 0.01);
    let (repair, at) = timer(&actions);
    e.handle(at, Event::TimerFired { timer: repair });
    assert!(
        safe_cex_engine::test_support::prepared_now(&mut e)
            .into_iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("get_pending_positions")))
    );
}
