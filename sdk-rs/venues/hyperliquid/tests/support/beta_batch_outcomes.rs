use super::*;
use safe_cex_engine::{Outcome, VenueError};

fn failed(actions: &[Action]) -> &VenueError {
    actions
        .iter()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: 81, error }) => Some(error),
            _ => None,
        })
        .expect("one failed batch")
}
fn pair(e: &mut Engine<Hyperliquid>) -> HttpRequest {
    let mut order =
        opts(r#"{"symbol":"BTC-PERP","type":"limit","side":"buy","price":90,"amount":1}"#);
    order.stop_loss = Some(80.);
    e.handle(
        2000.,
        Event::Command(Command::PlaceOrders {
            command: 81,
            orders: vec![order],
        }),
    );
    let request = write(e);
    assert_eq!(
        body(&request)["action"]["orders"].as_array().unwrap().len(),
        2
    );
    request
}

#[test]
fn beta_mixed_place_preserves_every_accepted_oid_in_either_row_order() {
    for accepted in [r#"{"resting":{"oid":101}}"#, r#"{"filled":{"oid":101}}"#] {
        for accepted_first in [true, false] {
            let (mut e, _) = ready("disabled");
            let request = pair(&mut e);
            let error = r#"{"error":"second order rejected"}"#;
            let rows = if accepted_first {
                format!("{accepted},{error}")
            } else {
                format!("{error},{accepted}")
            };
            reply_at(
                &mut e,
                &request,
                2001.,
                &format!(
                    r#"{{"status":"ok","response":{{"type":"order","data":{{"statuses":[{rows}]}}}}}}"#
                ),
            );
            let actions = e.drain_plain().collect::<Vec<_>>();
            let error = failed(&actions);
            assert_eq!(
                error.partial.as_ref().map(|p| p.order_ids.as_slice()),
                Some(["101".to_owned()].as_slice())
            );
            assert_eq!(error.outcome, Some(Outcome::Unknown));
            assert_eq!(
                e.store().order("101").is_some(),
                accepted.contains("resting")
            );
            assert!(
                !actions
                    .iter()
                    .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")))
            );
        }
    }
}

#[test]
fn beta_mixed_cancel_removes_only_successful_legs_in_either_row_order() {
    for accepted in [
        r#""success""#,
        r#"{"error":"Order was never placed, already canceled, or filled. asset=0"}"#,
    ] {
        for success_first in [true, false] {
            let (mut e, socket) = ready("disabled");
            open_order(&mut e, socket, 1500.);
            message(
                &mut e,
                socket,
                1501.,
                r#"{"channel":"orderUpdates","data":[{"order":{"coin":"BTC","oid":10,"side":"B","limitPx":"90","origSz":"1","sz":"1","orderType":"Limit","tif":"Gtc"},"status":"open"}]}"#,
            );
            e.drain_plain().for_each(drop);
            e.handle(
                2000.,
                Event::Command(Command::CancelOrders {
                    command: 81,
                    ids: vec!["9".into(), "10".into()],
                }),
            );
            let request = write(&mut e);
            let refusal = r#"{"error":"cancel refused"}"#;
            let rows = if success_first {
                format!("{accepted},{refusal}")
            } else {
                format!("{refusal},{accepted}")
            };
            reply_at(
                &mut e,
                &request,
                2001.,
                &format!(
                    r#"{{"status":"ok","response":{{"type":"cancel","data":{{"statuses":[{rows}]}}}}}}"#
                ),
            );
            let actions = e.drain_plain().collect::<Vec<_>>();
            assert_eq!(failed(&actions).outcome, Some(Outcome::Unknown));
            let (cancelled, refused) = if success_first {
                ("9", "10")
            } else {
                ("10", "9")
            };
            assert!(e.store().order(cancelled).is_none());
            assert_eq!(
                e.store().order(refused).unwrap().status,
                safe_cex_engine::model::OrderStatus::Open
            );
            assert!(
                !actions
                    .iter()
                    .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")))
            );
        }
    }
}

fn unknown_cancel_batch(unknown_index: usize) -> Vec<Action> {
    let (mut e, socket) = ready("disabled");
    open_order(&mut e, socket, 1500.);
    message(
        &mut e,
        socket,
        1501.,
        r#"{"channel":"orderUpdates","data":[{"order":{"coin":"BTC","oid":10,"cloid":"0x11111111111111111111111111111111","side":"B","limitPx":"90","origSz":"1","sz":"1","orderType":"Limit","tif":"Gtc"},"status":"open"},{"order":{"coin":"BTC","oid":11,"side":"B","limitPx":"90","origSz":"1","sz":"1","orderType":"Limit","tif":"Gtc"},"status":"open"}]}"#,
    );
    e.drain_plain().for_each(drop);
    e.handle(
        2000.,
        Event::Command(Command::CancelOrders {
            command: 81,
            ids: vec!["9".into(), "10".into(), "11".into()],
        }),
    );
    let request = write(&mut e);
    assert_eq!(
        body(&request)["action"],
        serde_json::json!({"type":"cancel","cancels":[
            {"a":0,"o":9},{"a":0,"o":10},{"a":0,"o":11}
        ]})
    );
    let mut statuses = vec![serde_json::json!("success"); 3];
    statuses[unknown_index] = serde_json::json!({"error":"unclassified cancel result"});
    reply_at(
        &mut e,
        &request,
        2001.,
        &serde_json::json!({"status":"ok","response":{
            "type":"cancel","data":{"statuses":statuses}
        }})
        .to_string(),
    );
    let actions = e.drain_plain().collect::<Vec<_>>();
    let error = failed(&actions);
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    for (index, id) in ["9", "10", "11"].into_iter().enumerate() {
        if index == unknown_index {
            assert_eq!(
                e.store().order(id).unwrap().status,
                safe_cex_engine::model::OrderStatus::Open
            );
        } else {
            assert!(e.store().order(id).is_none(), "confirmed cancellation {id}");
        }
    }
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange"))),
        "one cancellation batch, no retry or suffix write"
    );
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::Rejected));
    let partial = error.partial.as_ref().expect("classified batch detail");
    assert_eq!(partial.total, Some(3));
    assert_eq!(partial.failed_index, Some(unknown_index));
    assert!(
        partial.order_ids.is_empty(),
        "cancellation targets are not created-order evidence: {error:?}"
    );
    actions
}

#[test]
fn cancel_batch_with_unknown_row_retains_applied_and_unknown_journal() {
    use safe_cex_engine::write_plan::{AppliedEffect, ElementResult};
    for unknown_index in [1, 0, 2] {
        let actions = unknown_cancel_batch(unknown_index);
        let rows = failed(&actions).elements.as_ref().expect("cancel journal");
        assert_eq!(rows.len(), 3);
        for (index, (row, id)) in rows.iter().zip(["9", "10", "11"]).enumerate() {
            assert_eq!(row.element.index, index);
            assert!(row.element.client_order_id.is_none());
            if index == unknown_index {
                let ElementResult::Unknown { error } = &row.result else {
                    panic!("unconfirmed cancellation must stay Unknown: {row:?}");
                };
                assert_eq!(error.outcome, Some(Outcome::Unknown));
                assert_eq!(error.message, "The outcome is unknown. Check Open Orders");
                assert_eq!(
                    error.venue_message.as_deref(),
                    Some("unclassified cancel result")
                );
            } else {
                assert_eq!(
                    row.result,
                    ElementResult::Applied {
                        effect: AppliedEffect::Cancel,
                        target_order_id: Some(id.into()),
                        position_key: None,
                    }
                );
            }
        }
    }
}

#[test]
fn cancel_batch_with_unknown_row_publishes_guidance_and_native_diagnostic() {
    for unknown_index in [1, 0, 2] {
        let actions = unknown_cancel_batch(unknown_index);
        let error = failed(&actions);
        assert_eq!(error.message, "The outcome is unknown. Check Open Orders");
        assert_eq!(
            error.venue_message.as_deref(),
            Some("unclassified cancel result")
        );
    }
}

#[test]
fn cancel_batch_with_unknown_row_emits_one_independent_correlated_diagnostic() {
    for unknown_index in [1, 0, 2] {
        let actions = unknown_cancel_batch(unknown_index);
        let events: Vec<_> = actions
            .iter()
            .filter_map(|action| match action {
                Action::Notice(Notice::Event(event)) if event.code == "order.outcome_unknown" => {
                    Some(event)
                }
                _ => None,
            })
            .collect();
        assert_eq!(events.len(), 1, "independent Unknown diagnostic");
        assert!(
            events[0]
                .message
                .contains("The outcome is unknown. Check Open Orders")
        );
        assert!(
            events[0]
                .fields
                .iter()
                .any(|(key, value)| *key == "commandId"
                    && *value == safe_cex_engine::log::Value::Num(81.0))
        );
        assert!(
            events[0]
                .fields
                .iter()
                .any(|(key, value)| *key == "outcome" && value.as_str() == Some("unknown"))
        );
        assert!(events[0].fields.iter().any(|(key, value)| {
            *key == "unknownIndices" && value.as_str() == Some(unknown_index.to_string().as_str())
        }));
        assert!(
            events[0]
                .fields
                .iter()
                .any(|(key, value)| { *key == "placedOrderIds" && value.as_str() == Some("") })
        );
        assert_eq!(
            actions
                .iter()
                .filter(|action| matches!(
                    action,
                    Action::Notice(Notice::CommandFailed { command: 81, .. })
                ))
                .count(),
            1
        );
    }
}

#[test]
fn confirmed_cancel_does_not_emit_unknown_guidance_or_diagnostics() {
    for status in [
        serde_json::json!("success"),
        serde_json::json!({"error":"Order was never placed, already canceled, or filled. asset=0"}),
    ] {
        let (mut e, socket) = ready("disabled");
        open_order(&mut e, socket, 1500.);
        e.drain_plain().for_each(drop);
        e.handle(
            2000.,
            Event::Command(Command::CancelOrders {
                command: 81,
                ids: vec!["9".into()],
            }),
        );
        let request = write(&mut e);
        reply_at(
            &mut e,
            &request,
            2001.,
            &serde_json::json!({
                "status":"ok", "response":{"type":"cancel","data":{"statuses":[status]}}
            })
            .to_string(),
        );
        let actions = e.drain_plain().collect::<Vec<_>>();
        assert!(e.store().order("9").is_none());
        assert_eq!(
            actions
                .iter()
                .filter(|action| matches!(
                    action,
                    Action::Notice(Notice::CommandDone { command: 81, .. })
                ))
                .count(),
            1
        );
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::Notice(Notice::CommandFailed { .. })))
        );
        assert!(!actions.iter().any(|action| matches!(action,
            Action::Notice(Notice::Event(event)) if event.code == "order.outcome_unknown"
        )));
    }
}

#[test]
fn beta_all_refused_rows_have_no_partial_ids_or_published_orders() {
    let (mut e, _) = ready("disabled");
    let request = pair(&mut e);
    reply_at(
        &mut e,
        &request,
        2001.,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"error":"first"},{"error":"second"}]}}}"#,
    );
    let actions = e.drain_plain().collect::<Vec<_>>();
    let error = failed(&actions);
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert!(error.partial.is_none());
    assert!(e.store().orders().is_empty());
}

#[test]
fn beta_prior_accepted_call_then_refusal_keeps_ids_and_unknown() {
    let (mut e, _) = ready("disabled");
    let order = opts(r#"{"symbol":"BTC-PERP","type":"limit","side":"buy","price":90,"amount":1}"#);
    e.handle(
        2000.,
        Event::Command(Command::PlaceOrders {
            command: 81,
            orders: vec![order.clone(), order],
        }),
    );
    let request = write(&mut e);
    reply_at(
        &mut e,
        &request,
        2001.,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":101}}]}}}"#,
    );
    let timer = pace_timer(&mut e);
    e.handle(3000., Event::TimerFired { timer });
    let second = write(&mut e);
    reply_at(
        &mut e,
        &second,
        3001.,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"error":"later call refused"}]}}}"#,
    );
    let actions = e.drain_plain().collect::<Vec<_>>();
    let error = failed(&actions);
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(error.partial.as_ref().unwrap().order_ids, ["101"]);
    assert!(e.store().order("101").is_some());
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")))
    );
}
