#[allow(dead_code)]
mod support;

use safe_cex_engine::write_plan::ElementResult;
use safe_cex_engine::{Action, Command, Event, Method, Notice, Outcome};
use serde_json::json;
use support::*;

fn answer(envelope: &str) -> Harness {
    let mut h = Harness::new();
    h.open_private();
    h.seed();
    h.requests.clear();
    let orders = (0..21)
        .map(|index| json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":60000,"clientOrderId":format!("caller{index}")}))
        .collect::<Vec<_>>();
    h.handle(
        1.0,
        Event::Command(Command::PlaceOrders {
            command: 7,
            orders: serde_json::from_value(json!(orders)).unwrap(),
        }),
    );
    h.collect();
    let rows = (0..20)
        .map(|index| {
            json!({"orderId":(100+index).to_string(),"code":if index==19 {"102014"} else {"0"},"msg":if index==19 {"refused"} else {"success"}})
        })
        .collect::<Vec<_>>();
    h.reply(
        "batch-orders",
        &json!({"code":envelope,"msg":"envelope diagnostic","data":rows}).to_string(),
    );
    h
}

#[test]
fn unknown_envelopes_keep_the_journal_and_emit_outcome_unknown_once() {
    for envelope in ["103013", "999999", "", "429"] {
        let h = answer(envelope);
        let error = h.failures.last().expect("uncertain batch failed");
        assert_eq!(error.outcome, Some(Outcome::Unknown));
        assert_eq!(error.message, "The outcome is unknown. Check Open Orders");
        assert!(
            error
                .partial
                .as_ref()
                .is_none_or(|partial| partial.order_ids.is_empty())
        );
        let rows = error.elements.as_ref().expect("full logical journal");
        assert_eq!(rows.len(), 21);
        assert!(
            rows[..20]
                .iter()
                .all(|row| matches!(&row.result, ElementResult::Unknown { error }
            if error.message == "The outcome is unknown. Check Open Orders"))
        );
        assert!(matches!(
            rows[20].result,
            ElementResult::NotSent { error: None }
        ));
        assert!(
            h.e.store().order("100").is_none(),
            "no row effects under an unknown envelope"
        );
        let events = h
            .actions
            .iter()
            .filter(|action| {
                matches!(action,
                    Action::Notice(Notice::Event(event)) if event.code == "order.outcome_unknown"
                        && event.message == "The outcome is unknown. Check Open Orders"
                && event.fields.iter().any(|(key, value)| *key == "outcome" && value.as_str() == Some("unknown"))
                )
            })
            .count();
        assert_eq!(events, 1, "one outcome event for {envelope}");
        assert_eq!(
            h.requests
                .iter()
                .filter(|(method, _)| *method == Method::Post)
                .count(),
            1
        );
    }
}

#[test]
fn success_and_aggregate_envelopes_keep_their_row_facts() {
    for envelope in ["0", "1", "2", "102014"] {
        let h = answer(envelope);
        let error = h.failures.last().expect("native refusal stops this batch");
        assert_ne!(error.outcome, Some(Outcome::Unknown));
        let rows = error.elements.as_ref().expect("full logical journal");
        assert!(
            rows[..19]
                .iter()
                .all(|row| matches!(row.result, ElementResult::Placed { .. }))
        );
        assert!(
            matches!(&rows[19].result, ElementResult::NotSent { error: Some(error) }
            if error.outcome == Some(Outcome::Rejected))
        );
        assert!(matches!(
            rows[20].result,
            ElementResult::NotSent { error: None }
        ));
        assert_eq!(error.partial.as_ref().unwrap().order_ids.len(), 19);
        assert!(!h.actions.iter().any(|action| matches!(action,
            Action::Notice(Notice::Event(event)) if event.code == "order.outcome_unknown"
        )));
    }
}
