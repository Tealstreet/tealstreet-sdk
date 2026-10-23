//! Original-source contract failures; enable when shared write adoption lands.
mod support;
use safe_cex_engine::{Action, Command, Event, Method, Notice, Outcome};
use serde_json::json;
use support::*;

#[test]
#[ignore = "original RED: backpack adoption owner codex-hcmq3f, tracked in plan #1880"]
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
#[ignore = "original RED: backpack adoption owner codex-hcmq3f, tracked in plan #1880"]
fn p0_write_pair_failed_row_preserves_accepted_id() {
    pair(false);
}

#[test]
#[ignore = "original RED: backpack adoption owner codex-hcmq3f, tracked in plan #1880"]
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
