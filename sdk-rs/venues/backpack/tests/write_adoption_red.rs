//! Original-source contract regressions for managed foreground write adoption.
mod support;
use safe_cex_engine::{Action, Command, Event, Method, Notice, Outcome};
use serde_json::json;
use support::managed::*;

#[test]
fn p0_write_all_failed_explicit_cancels_are_not_success() {
    let (mut e, _) = ready_orders(&format!("[{ORDER}]"));
    e.handle(
        2000.0,
        Event::Command(Command::CancelOrders {
            command: 711,
            ids: vec!["123".into()],
        }),
    );
    let request = requests(&mut e).pop().unwrap();
    assert_eq!(request.method, Method::Delete);
    reply(&mut e, &request, 400, r#"{"message":"denied"}"#);
    let out: Vec<_> = e.drain_actions().collect();
    assert!(!out.iter().any(|action| matches!(action, Action::Http(next)
        if next.method == request.method && next.url == request.url)));
    assert_eq!(
        out.iter()
            .filter(|action| matches!(
                action,
                Action::Notice(Notice::CommandFailed { command: 711, .. })
            ))
            .count(),
        1,
        "requested cancellation failure must reach its caller"
    );
    assert!(!out.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone { command: 711, .. })
    )));
}

#[test]
fn p0_write_pair_failed_row_preserves_accepted_id() {
    pair(false);
}

#[test]
fn p0_write_pair_missing_row_preserves_evidence_without_counts() {
    pair(true);
}

fn pair(missing: bool) {
    let (mut e, _) = ready();
    e.store_mut()
        .update_markets(|market| market.limits.amount.max = 1000.0);
    let orders = serde_json::from_str(r#"[{"symbol":"SOL_USDC_PERP","type":"limit","side":"buy","amount":1,"price":140},{"symbol":"SOL_USDC_PERP","type":"limit","side":"sell","amount":1,"price":150}]"#).unwrap();
    e.handle(
        2000.0,
        Event::Command(Command::PlaceOrders {
            command: 712,
            orders,
        }),
    );
    let request = requests(&mut e).pop().unwrap();
    assert!(request.url.ends_with("/orders"));
    let sent: Vec<serde_json::Value> =
        serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert_eq!(sent.len(), 2);
    let row = |index: usize, operation: &str, id: &str| {
        json!({
            "operation": operation, "id": id, "clientId": sent[index]["clientId"],
            "symbol": sent[index]["symbol"], "side": sent[index]["side"],
            "orderType": sent[index]["orderType"],
        })
    };
    let mut rows = vec![row(0, "Ok", "placed-one")];
    if !missing {
        rows.push(row(1, "Error", ""));
    }
    reply(
        &mut e,
        &request,
        200,
        &serde_json::to_string(&rows).unwrap(),
    );
    let out: Vec<_> = e.drain_actions().collect();
    assert!(!out.iter().any(|action| matches!(action, Action::Http(next)
        if next.method == request.method && next.url == request.url)));
    let failures: Vec<_> = out
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed {
                command: 712,
                error,
            }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].outcome, Some(Outcome::Unknown));
    let partial = failures[0]
        .partial
        .as_ref()
        .expect("accepted native ID retained");
    assert_eq!(partial.order_ids, ["placed-one"]);
    if missing {
        assert_eq!(partial.total, None);
        assert_eq!(partial.failed_index, None);
    }
    assert!(!out.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone { command: 712, .. })
    )));
}

#[test]
fn p0_write_optional_child_failure_keeps_both_ids_and_fails_once() {
    let (mut e, _) = ready();
    e.handle(2000.0, Event::Command(Command::PlaceOrders {
        command: 713,
        orders: serde_json::from_str(r#"[{"symbol":"SOL_USDC_PERP","type":"limit","side":"buy","amount":1,"price":140,"takeProfit":150,"stopLoss":130}]"#).unwrap(),
    }));
    let entry = requests(&mut e).pop().unwrap();
    assert!(!entry.body.as_deref().unwrap().contains("TriggerPrice"));
    reply(&mut e, &entry, 200, r#"{"id":"entry"}"#);
    let tp = requests(&mut e).pop().unwrap();
    let body = tp.body.as_deref().unwrap();
    assert!(body.contains("\"side\":\"Ask\""));
    assert!(body.contains("\"triggerPrice\":\"150.00\""));
    assert!(body.contains("\"triggerBy\":\"MarkPrice\""));
    reply(&mut e, &tp, 400, r#"{"message":"rejected"}"#);
    let after_failure = actions(&mut e);
    assert!(after_failure.iter().any(|action| matches!(action,
        Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::CHILD_LEG_FAILED
    )));
    let sl = after_failure
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .expect("native optional continuation still sends SL");
    assert!(
        sl.body
            .as_deref()
            .unwrap()
            .contains("\"triggerPrice\":\"130.00\"")
    );
    reply(&mut e, &sl, 200, r#"{"id":"stop"}"#);
    let out: Vec<_> = e.drain_actions().collect();
    let failures: Vec<_> = out
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed {
                command: 713,
                error,
            }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(
        failures[0].partial.as_ref().unwrap().order_ids,
        ["entry", "stop"]
    );
    assert!(!out.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone { command: 713, .. }) | Action::Http(_)
    )));
}
