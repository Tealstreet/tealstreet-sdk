//! Original caller intent bounds the first physical release, not its accepted effects.
mod support;
use safe_cex_engine::request_context::{Claim, HttpAction, Prepared};
use safe_cex_engine::{Action, Command, Engine, ErrorKind, Event, HttpRequest, Notice, Outcome};
use safe_cex_venue_hyperliquid::Hyperliquid;
use serde_json::json;
use support::*;

const NOW: f64 = 2000.;
const ACK: &str =
    r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":900}}]}}}"#;

fn place(command: u32) -> Command {
    Command::PlaceOrders {
        command,
        orders: vec![
            serde_json::from_value(json!({
                "symbol":"BTC-PERP","type":"limit","side":"buy","price":90,"amount":1,
                "clientOrderId":format!("0x{command:032x}")
            }))
            .unwrap(),
        ],
    }
}
fn writes(actions: &[Action]) -> Vec<&HttpRequest> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) if r.url.ends_with("/exchange") => Some(r),
            _ => None,
        })
        .collect()
}
fn timeout(actions: &[Action], command: u32) {
    let errors: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: id, error }) if *id == command => {
                Some(error)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        errors.len(),
        1,
        "one visible terminal for the expired intent"
    );
    assert_eq!(errors[0].kind, Some(ErrorKind::Timeout));
    assert_eq!(errors[0].outcome, Some(Outcome::NotSent));
}
fn first(e: &mut Engine<Hyperliquid>) -> HttpRequest {
    e.handle_command(NOW, place(1), Some(20_000)).unwrap();
    let actions = prepared_now(e);
    assert_eq!(writes(&actions).len(), 1);
    writes(&actions)[0].clone()
}

#[test]
fn a_queued_intent_cannot_send_after_its_caller_deadline_even_before_the_timer() {
    let (mut e, _) = ready("disabled");
    let leader = first(&mut e);
    e.handle_command(NOW + 1., place(2), Some(10)).unwrap();
    let queued = prepared_now(&mut e);
    assert!(writes(&queued).is_empty());
    e.handle(
        NOW + 500.,
        Event::HttpResponse {
            request: leader.id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body: ACK,
            },
        },
    );
    let actions = prepared_now(&mut e);
    assert!(
        writes(&actions).is_empty(),
        "expired FIFO work must never be released"
    );
    timeout(&actions, 2);
}

#[test]
fn queued_expiry_is_driven_by_its_own_captured_timer_and_leaves_the_leader_live() {
    let (mut e, _) = ready("disabled");
    let leader = first(&mut e);
    e.handle_command(NOW + 1., place(2), Some(10)).unwrap();
    let actions = prepared_now(&mut e);
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 11. => Some(*timer),
            _ => None,
        })
        .expect("queued intent has an original-deadline timer");
    e.handle(NOW + 11., Event::TimerFired { timer });
    let actions = prepared_now(&mut e);
    timeout(&actions, 2);
    assert!(writes(&actions).is_empty());
    reply(&mut e, &leader, 200, ACK);
    let actions = prepared_now(&mut e);
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 1, .. })))
    );
    e.handle(NOW + 12., Event::TimerFired { timer });
    assert!(
        prepared_now(&mut e)
            .iter()
            .all(|a| !matches!(a, Action::Notice(Notice::CommandFailed { command: 2, .. })))
    );
}

#[test]
fn a_prepared_first_write_checks_the_exclusive_original_deadline_at_claim() {
    for (at, allowed) in [(NOW + 9., true), (NOW + 10., false), (NOW + 11., false)] {
        let (mut e, _) = ready("disabled");
        e.handle_command(NOW, place(1), Some(10)).unwrap();
        let preparation = e
            .drain_actions()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match *a {
                    HttpAction::Prepare {
                        preparation,
                        request,
                        ..
                    } if request.url.ends_with("/exchange") => Some(preparation),
                    _ => None,
                },
                _ => None,
            })
            .expect("genuine first-write preparation");
        e.http_prepared(
            NOW,
            preparation,
            Prepared {
                id: preparation.attempt().request.0,
                expires_at: None,
            },
        );
        let permit = e
            .drain_actions()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match *a {
                    HttpAction::Bind { permit, .. } => Some(permit),
                    _ => None,
                },
                _ => None,
            })
            .expect("signed first-write binding");
        let result = e.claim_http(at, permit);
        assert_eq!(
            matches!(result, Ok(Claim::Released(_))),
            allowed,
            "claim at {at}"
        );
        if !allowed {
            timeout(&prepared_now(&mut e), 1);
        }
    }
}

#[test]
fn an_already_released_write_keeps_its_original_response_after_the_caller_deadline() {
    let (mut e, _) = ready("disabled");
    e.handle_command(NOW, place(1), Some(10)).unwrap();
    let actions = prepared_now(&mut e);
    let write = writes(&actions)[0].clone();
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 10. => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(NOW + 10., Event::TimerFired { timer });
    assert!(
        !prepared_now(&mut e)
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 1, .. })))
    );
    e.handle(
        NOW + 20.,
        Event::HttpResponse {
            request: write.id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body: ACK,
            },
        },
    );
    let actions = prepared_now(&mut e);
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 1, .. })))
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 1, .. })))
    );
}

fn next_after_queue(cancel_symbol: &str) -> Vec<Action> {
    let (mut e, socket) = ready("disabled");
    message(
        &mut e,
        socket,
        1500.,
        r#"{"channel":"orderUpdates","data":[{"order":{"coin":"BTC","oid":9,"side":"B","limitPx":"90","origSz":"1","sz":"1","orderType":"Limit","tif":"Gtc"},"status":"open"},{"order":{"coin":"ETH","oid":19,"side":"B","limitPx":"190","origSz":"1","sz":"1","orderType":"Limit","tif":"Gtc"},"status":"open"}]}"#,
    );
    e.drain_actions().for_each(drop);
    let leader = first(&mut e);
    e.handle_command(NOW + 1., place(2), Some(20_000)).unwrap();
    prepared_now(&mut e);
    e.handle_command(
        NOW + 2.,
        Command::CancelOrders {
            command: 3,
            ids: vec![if cancel_symbol == "ETH" { "19" } else { "9" }.into()],
        },
        Some(20_000),
    )
    .unwrap();
    prepared_now(&mut e);
    e.handle(
        NOW + 500.,
        Event::HttpResponse {
            request: leader.id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body: ACK,
            },
        },
    );
    prepared_now(&mut e)
}

#[test]
fn an_independent_cancel_passes_queued_placements_without_changing_mutation_pacing() {
    let actions = next_after_queue("ETH");
    let writes = writes(&actions);
    assert_eq!(writes.len(), 1);
    let body: serde_json::Value = serde_json::from_str(writes[0].body.as_deref().unwrap()).unwrap();
    assert_eq!(body["action"]["type"], "cancel");
    assert_eq!(body["action"]["cancels"][0]["o"], 19);
}

#[test]
fn a_cancel_cannot_pass_an_older_placement_on_the_same_market() {
    let actions = next_after_queue("BTC");
    let writes = writes(&actions);
    assert_eq!(writes.len(), 1);
    let body: serde_json::Value = serde_json::from_str(writes[0].body.as_deref().unwrap()).unwrap();
    assert_eq!(body["action"]["type"], "order");
}

#[test]
fn a_replacement_after_a_cancel_ack_is_not_expired_with_the_original_unsent_intent() {
    let (mut e, socket) = ready("disabled");
    message(
        &mut e,
        socket,
        1500.,
        r#"{"channel":"orderUpdates","data":[{"order":{"coin":"BTC","oid":9,"side":"B","limitPx":"90","origSz":"1","sz":"1","orderType":"Limit","tif":"Gtc"},"status":"open"}]}"#,
    );
    e.drain_actions().for_each(drop);
    e.handle_command(
        NOW,
        Command::AmendOrder {
            command: 1,
            id: "9".into(),
            price: Some(85.),
            amount: None,
        },
        Some(10),
    )
    .unwrap();
    let actions = prepared_now(&mut e);
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 10. => Some(*timer),
            _ => None,
        })
        .unwrap();
    let sizing = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if kind(r) == "orderStatus" => Some(r),
            _ => None,
        })
        .expect("fresh sizing still precedes cancel");
    answer_order_status(&mut e, sizing);
    let actions = prepared_now(&mut e);
    let cancel = writes(&actions)[0].clone();
    let ack = r#"{"status":"ok","response":{"type":"cancel","data":{"statuses":["success"]}}}"#;
    e.handle(NOW + 10., Event::TimerFired { timer });
    assert!(
        !prepared_now(&mut e)
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 1, .. })))
    );
    e.handle(
        NOW + 20.,
        Event::HttpResponse {
            request: cancel.id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body: ack,
            },
        },
    );
    let actions = prepared_now(&mut e);
    let actions = advance_packet_timers(&mut e, actions, NOW + 334.);
    let replacement = writes(&actions);
    assert_eq!(
        replacement.len(),
        1,
        "never leave the cancelled order unreplaced"
    );
    let body: serde_json::Value =
        serde_json::from_str(replacement[0].body.as_deref().unwrap()).unwrap();
    assert_eq!(body["action"]["type"], "order");
    assert_eq!(body["action"]["orders"][0]["p"], "85");
}

#[test]
fn independent_protection_passes_bulk_but_cannot_preempt_a_released_owner() {
    let (mut e, _) = ready("disabled");
    let leader = first(&mut e);
    e.handle_command(NOW + 1., place(2), Some(20_000)).unwrap();
    prepared_now(&mut e);
    let protection = Command::PlaceOrders { command:3, orders:vec![serde_json::from_value(json!({
        "symbol":"ETH-PERP","type":"stop_market","side":"sell","price":180,"amount":1,"reduceOnly":true,
        "clientOrderId":"0x00000000000000000000000000000003"
    })).unwrap()] };
    e.handle_command(NOW + 2., protection, Some(20_000))
        .unwrap();
    assert!(
        writes(&prepared_now(&mut e)).is_empty(),
        "current physical owner is never preempted"
    );
    e.handle(
        NOW + 500.,
        Event::HttpResponse {
            request: leader.id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body: ACK,
            },
        },
    );
    let actions = prepared_now(&mut e);
    let selected = writes(&actions);
    assert_eq!(selected.len(), 1);
    let body: serde_json::Value =
        serde_json::from_str(selected[0].body.as_deref().unwrap()).unwrap();
    assert_eq!(
        body["action"]["orders"][0]["a"], 1,
        "ETH protection precedes queued BTC placement"
    );
    assert_eq!(body["action"]["orders"][0]["t"]["trigger"]["tpsl"], "sl");
}

#[test]
fn original_timeout_callback_revokes_a_held_binding_even_if_the_wall_clock_rolled_back() {
    let (mut e, _) = ready("disabled");
    e.handle_command(NOW, place(1), Some(10)).unwrap();
    let actions: Vec<_> = e.drain_actions().collect();
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 10. => Some(*timer),
            _ => None,
        })
        .expect("original caller timer exists even while the first route is preparing");
    let preparation = actions
        .iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match &**a {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } if request.url.ends_with("/exchange") => Some(*preparation),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    e.http_prepared(
        NOW,
        preparation,
        Prepared {
            id: preparation.attempt().request.0,
            expires_at: None,
        },
    );
    let permit = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind { permit, .. } => Some(permit),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    e.handle(NOW + 1., Event::TimerFired { timer });
    assert!(
        e.claim_http(NOW + 2., permit).is_err(),
        "fired original timer is authoritative despite raw clock rollback"
    );
    timeout(&prepared_now(&mut e), 1);
}

#[test]
fn expiring_an_amend_read_cannot_leave_its_wait_or_old_timer_on_the_next_owner() {
    let (mut e, socket) = ready("disabled");
    message(
        &mut e,
        socket,
        1500.,
        r#"{"channel":"orderUpdates","data":[{"order":{"coin":"BTC","oid":9,"side":"B","limitPx":"90","origSz":"1","sz":"1","orderType":"Limit","tif":"Gtc"},"status":"open"}]}"#,
    );
    e.drain_actions().for_each(drop);
    e.handle_command(
        NOW,
        Command::AmendOrder {
            command: 1,
            id: "9".into(),
            price: Some(85.),
            amount: None,
        },
        Some(10),
    )
    .unwrap();
    let actions = prepared_now(&mut e);
    let timer_at = |at| {
        actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == at => Some(*timer),
                _ => None,
            })
            .unwrap()
    };
    let caller_timer = timer_at(NOW + 10.);
    let old_read_timer = timer_at(NOW + 5000.);
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if kind(r) == "orderStatus"))
    );
    e.handle_command(NOW + 1., place(3), Some(20_000)).unwrap();
    assert!(writes(&prepared_now(&mut e)).is_empty());
    e.handle(
        NOW + 10.,
        Event::TimerFired {
            timer: caller_timer,
        },
    );
    let actions = prepared_now(&mut e);
    timeout(&actions, 1);
    assert_eq!(
        writes(&actions).len(),
        1,
        "successor must not inherit an expired amend's wait"
    );
    let successor = writes(&actions)[0].clone();
    e.handle(
        NOW + 5000.,
        Event::TimerFired {
            timer: old_read_timer,
        },
    );
    assert!(
        !prepared_now(&mut e)
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 3, .. })))
    );
    reply(&mut e, &successor, 200, ACK);
    assert!(
        prepared_now(&mut e)
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 3, .. })))
    );
}
