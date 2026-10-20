mod support;
use safe_cex_engine::{Command, Event};
use support::*;

#[test]
fn stale_join_cannot_unfence_lost_placement_but_fresh_complete_read_can() {
    let mut h = Harness::classic();
    // This join started before the mutation, and its normal source is already accepted.
    h.respond(
        "/orders-pending",
        200,
        r#"{"code":"00000","data":{"entrustedList":[]}}"#,
    );
    let held_protection = h.take("planType=profit_loss");
    h.command(Command::PlaceOrders {
        command: 31,
        orders: vec![
            serde_json::from_str(
                r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.002,"price":90}"#,
            )
            .unwrap(),
        ],
    });
    let write = h.take("/batch-place-order");
    h.event(Event::HttpFailed {
        request: write.id,
        error: "response lost after acceptance",
    });
    assert!(
        h.done
            .iter()
            .any(|(id, result)| *id == 31 && result.is_err())
    );
    h.command(Command::CancelSymbolOrders {
        command: 32,
        symbol: "BTCUSDT".into(),
    });
    assert!(
        h.done
            .iter()
            .any(|(id, result)| *id == 32 && result.is_err())
    );
    // Even though the last page would be dispatched after failure, this whole join is stale.
    h.reply(
        held_protection.id,
        200,
        r#"{"code":"00000","data":{"entrustedList":[]}}"#,
    );
    assert!(
        h.requests.iter().any(|r| r.url.contains("/orders-pending")),
        "stale join must restart from its normal source"
    );
    h.command(Command::CancelSymbolOrders {
        command: 33,
        symbol: "BTCUSDT".into(),
    });
    assert!(
        h.done
            .iter()
            .any(|(id, result)| *id == 33 && result.is_err())
    );
    h.orders(r#"{"code":"00000","data":{"entrustedList":[]}}"#);
    h.command(Command::CancelSymbolOrders {
        command: 34,
        symbol: "BTCUSDT".into(),
    });
    assert!(
        h.done
            .iter()
            .any(|(id, result)| *id == 34 && result.is_ok()),
        "accepted fresh complete order census must clear uncertainty"
    );
}
