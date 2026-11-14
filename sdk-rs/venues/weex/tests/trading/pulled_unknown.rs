use super::*;
use safe_cex_engine::write_plan::ElementResult;
use safe_cex_engine::{ErrorKind, Outcome, UNKNOWN_OUTCOME_TEXT};

fn native_failure(maximum: f64, code: &str) -> (Vec<Action>, String) {
    let mut e = engine();
    e.store_mut().update_markets(|market| {
        market.precision.amount = 1.0;
        market.limits.amount.max = maximum;
    });
    let mut first = opts("");
    first.amount = 26.0;
    let mut suffix = first.clone();
    suffix.kind = OrderType::Conditional;
    suffix.trigger_price = Some(68000.0);
    // TS weex.exchange.ts:1041,1112 sends the rounded oversized payload once.
    // KNOWN-ISSUE: TS sends above-max; venue refuses.
    let actions = command(
        &mut e,
        Command::PlaceOrders {
            command: 1,
            orders: vec![first, suffix],
        },
    );
    let requests = writes(&actions);
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert!(request.url.ends_with("/capi/v3/batchOrders"));
    let payload = body(request);
    let rows = payload["batchOrders"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["quantity"], "26");
    let client = rows[0]["newClientOrderId"].as_str().unwrap().to_owned();
    let actions = response(
        &mut e,
        request,
        json!([{
            "clientOrderId": client, "success": false,
            "errorCode": code, "errorMessage": "synthetic native refusal"
        }]),
    );
    assert!(
        writes(&actions).is_empty(),
        "no retry or unsent suffix write"
    );
    assert!(!actions.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone { command: 1, .. })
    )));
    (actions, client)
}

fn assert_journal(actions: &[Action], client: &str, unknown: bool) {
    let error = failure(actions);
    let elements = error.elements.as_ref().expect("indexed outcome journal");
    assert_eq!(elements.len(), 2);
    assert_eq!(elements[0].element.index, 0);
    assert_eq!(elements[0].element.client_order_id.as_deref(), Some(client));
    assert_eq!(elements[1].element.index, 1);
    assert!(elements[1].element.client_order_id.is_some());
    if unknown {
        assert!(
            matches!(&elements[0].result, ElementResult::Unknown { error }
            if error.outcome == Some(Outcome::Unknown)
                && error.kind == Some(ErrorKind::RateLimited)
                && error.message.contains(UNKNOWN_OUTCOME_TEXT)
                && error.venue_code.as_deref() == Some("429"))
        );
    } else {
        assert!(
            matches!(&elements[0].result, ElementResult::NotSent { error: Some(error) }
            if error.outcome == Some(Outcome::Rejected)
                && error.venue_code.as_deref() == Some("42002"))
        );
    }
    assert!(matches!(
        elements[1].result,
        ElementResult::NotSent { error: None }
    ));
}

#[test]
fn oversized_native_unknown_uses_required_literal() {
    let (actions, _) = native_failure(5.0, "429");
    assert_eq!(failure(&actions).outcome, Some(Outcome::Unknown));
    assert!(failure(&actions).message.contains(UNKNOWN_OUTCOME_TEXT));
    assert_eq!(failure(&actions).kind, Some(ErrorKind::RateLimited));
    assert_eq!(failure(&actions).venue_code.as_deref(), Some("429"));
    assert_eq!(
        failure(&actions).venue_message.as_deref(),
        Some("synthetic native refusal")
    );
}

#[test]
fn within_max_native_unknown_uses_required_literal() {
    let (actions, _) = native_failure(100.0, "429");
    assert_eq!(failure(&actions).outcome, Some(Outcome::Unknown));
    assert!(failure(&actions).message.contains(UNKNOWN_OUTCOME_TEXT));
}

#[test]
fn native_unknown_journals_first_order_and_unsent_suffix() {
    for maximum in [5.0, 100.0] {
        let (actions, client) = native_failure(maximum, "429");
        assert_journal(&actions, &client, true);
    }
}

#[test]
fn native_unknown_emits_one_correlated_diagnostic() {
    for maximum in [5.0, 100.0] {
        let (actions, _) = native_failure(maximum, "429");
        let diagnostics = actions
            .iter()
            .filter_map(|action| match action {
                Action::Notice(Notice::CommandError {
                    command: 1,
                    message,
                }) => Some(message),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].contains(UNKNOWN_OUTCOME_TEXT));
    }
}

#[test]
fn native_unknown_emits_one_outcome_event() {
    for maximum in [5.0, 100.0] {
        let (actions, _) = native_failure(maximum, "429");
        let events = actions
            .iter()
            .filter_map(|action| match action {
                Action::Notice(Notice::Event(event)) if event.code == "order.outcome_unknown" => {
                    Some(event)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].level, safe_cex_engine::Level::Warn);
        assert!(events[0].message.contains(UNKNOWN_OUTCOME_TEXT));
        assert!(events[0].fields.iter().any(|(key, value)| *key == "command"
            && matches!(value, safe_cex_engine::log::Value::Num(1.0))));
    }
}

#[test]
fn definite_oversized_refusal_keeps_native_outcome_without_unknown_reporting() {
    let (actions, client) = native_failure(5.0, "42002");
    assert_eq!(failure(&actions).outcome, Some(Outcome::Rejected));
    assert!(!failure(&actions).message.contains(UNKNOWN_OUTCOME_TEXT));
    assert!(
        command_errors(&actions)
            .iter()
            .all(|message| !message.contains(UNKNOWN_OUTCOME_TEXT))
    );
    assert!(!actions.iter().any(|action| matches!(action,
        Action::Notice(Notice::Event(event)) if event.code == "order.outcome_unknown")));
    assert_journal(&actions, &client, false);
}

#[test]
fn mixed_batch_journal_correlates_native_rows_and_retains_accepted_ids() {
    let mut e = engine();
    e.store_mut().update_markets(|market| {
        market.precision.amount = 1.0;
        market.limits.amount.max = 5.0;
    });
    let mut first = opts("");
    first.amount = 26.0;
    let mut suffix = first.clone();
    suffix.kind = OrderType::Conditional;
    suffix.trigger_price = Some(68000.0);
    let actions = command(
        &mut e,
        Command::PlaceOrders {
            command: 1,
            orders: vec![first.clone(), first, suffix],
        },
    );
    let requests = writes(&actions);
    assert_eq!(requests.len(), 1);
    let payload = body(&requests[0]);
    let rows = payload["batchOrders"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row["quantity"] == "26"));
    let actions = response(
        &mut e,
        &requests[0],
        json!([
            {"clientOrderId":rows[1]["newClientOrderId"], "success":false,
                "errorCode":"429", "errorMessage":"Too many requests"},
            {"clientOrderId":rows[0]["newClientOrderId"], "success":true, "orderId":"100"}
        ]),
    );
    assert!(writes(&actions).is_empty());
    let error = failure(&actions);
    assert!(error.message.contains(UNKNOWN_OUTCOME_TEXT));
    assert_eq!(error.partial.as_ref().unwrap().order_ids, ["100"]);
    let elements = error.elements.as_ref().unwrap();
    assert_eq!(elements.len(), 3);
    assert_eq!(
        elements[0].element.client_order_id.as_deref(),
        rows[0]["newClientOrderId"].as_str()
    );
    assert!(
        matches!(&elements[0].result, ElementResult::Placed { order_ids } if order_ids == &["100"])
    );
    assert!(
        matches!(&elements[1].result, ElementResult::Unknown { error } if error.venue_code.as_deref() == Some("429"))
    );
    assert!(matches!(
        elements[2].result,
        ElementResult::NotSent { error: None }
    ));
}

#[test]
fn unbatched_oversized_unknowns_stop_without_dispatching_the_next_order() {
    for kind in [
        OrderType::Market,
        OrderType::Conditional,
        OrderType::StopLoss,
        OrderType::TakeProfit,
    ] {
        let mut e = engine();
        e.store_mut().update_markets(|market| {
            market.precision.amount = 1.0;
            market.limits.amount.max = 5.0;
        });
        let mut first = opts("");
        first.amount = 26.0;
        first.kind = kind;
        first.reduce_only = Some(kind != OrderType::Conditional);
        if kind == OrderType::Conditional {
            first.trigger_price = Some(68000.0);
        }
        let mut suffix = opts("");
        suffix.amount = 26.0;
        let actions = command(
            &mut e,
            Command::PlaceOrders {
                command: 1,
                orders: vec![first, suffix],
            },
        );
        let requests = writes(&actions);
        assert_eq!(requests.len(), 1, "{kind:?}");
        let payload = body(&requests[0]);
        assert_eq!(payload["quantity"], "26");
        let client = e
            .store()
            .orders()
            .iter()
            .find_map(|order| order.client_order_id.clone())
            .unwrap();
        let actions = response(
            &mut e,
            &requests[0],
            json!({
                "success":false, "errorCode":"429", "errorMessage":"Too many requests"
            }),
        );
        assert!(writes(&actions).is_empty(), "{kind:?}");
        assert!(
            failure(&actions).message.contains(UNKNOWN_OUTCOME_TEXT),
            "{kind:?}"
        );
        assert_eq!(command_errors(&actions), [UNKNOWN_OUTCOME_TEXT]);
        assert_journal(&actions, &client, true);
    }
}

#[test]
fn earlier_accepted_step_survives_in_unknown_journal() {
    let mut e = engine();
    e.store_mut().update_markets(|market| {
        market.precision.amount = 1.0;
        market.limits.amount.max = 5.0;
    });
    let mut regular = opts("");
    regular.amount = 26.0;
    let mut conditional = regular.clone();
    conditional.kind = OrderType::Conditional;
    conditional.trigger_price = Some(68000.0);
    let actions = command(
        &mut e,
        Command::PlaceOrders {
            command: 1,
            orders: vec![conditional.clone(), regular, conditional],
        },
    );
    let requests = writes(&actions);
    assert_eq!(requests.len(), 1);
    assert!(requests[0].url.ends_with("/capi/v3/algoOrder"));
    let actions = response(
        &mut e,
        &requests[0],
        json!({"success":true,"orderId":"100"}),
    );
    let requests = writes(&actions);
    assert_eq!(requests.len(), 1);
    let payload = body(&requests[0]);
    let actions = response(
        &mut e,
        &requests[0],
        json!([{
            "clientOrderId":payload["batchOrders"][0]["newClientOrderId"],
            "success":false,"errorCode":"429","errorMessage":"Too many requests"
        }]),
    );
    assert!(writes(&actions).is_empty());
    let error = failure(&actions);
    assert!(error.message.contains(UNKNOWN_OUTCOME_TEXT));
    assert_eq!(error.partial.as_ref().unwrap().order_ids, ["100"]);
    let elements = error.elements.as_ref().unwrap();
    assert_eq!(elements.len(), 3);
    assert!(
        matches!(&elements[0].result, ElementResult::Placed { order_ids } if order_ids == &["100"])
    );
    assert!(
        matches!(&elements[1].result, ElementResult::Unknown { error } if error.venue_code.as_deref() == Some("429"))
    );
    assert!(matches!(
        elements[2].result,
        ElementResult::NotSent { error: None }
    ));
}
