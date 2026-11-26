//! AGT-70: a whole-position TP/SL (`pos_profit`/`pos_loss`) closes whatever the position is when it
//! triggers. It holds no fixed size, so the store row carries amount 0 even when the venue row
//! reports the current position size; a sized plan keeps its size.
mod support;
use support::*;

fn algo_push(plan: &str, size: &str) -> String {
    serde_json::json!({"arg":{"channel":"orders-algo"},"data":[{
        "orderId":"plan","symbol":"BTCUSDT","size":size,"triggerPrice":"80","side":"sell",
        "holdSide":"long","orderType":"market","planType":plan,"status":"live"
    }]})
    .to_string()
}

#[test]
fn whole_position_plan_reported_with_the_position_size_holds_no_amount() {
    for plan in ["pos_loss", "pos_profit", "psl", "ptp"] {
        let mut h = Harness::classic();
        h.private_message(&algo_push(plan, "0.02"));
        let order = h.engine.store().order("plan").expect(plan);
        assert_eq!(order.amount, 0.0, "{plan}");
        assert_eq!(order.remaining, 0.0, "{plan}");
    }
}

#[test]
fn whole_position_plan_read_over_rest_with_the_position_size_holds_no_amount() {
    let mut h = Harness::classic();
    h.command(safe_cex_engine::Command::Venue {
        command: 7,
        action: safe_cex_engine::VenueAction::RefreshOrders { symbols: None },
    });
    h.respond(
        "/orders-pending",
        200,
        r#"{"code":"00000","data":{"entrustedList":[]}}"#,
    );
    h.respond(
        "planType=profit_loss",
        200,
        r#"{"code":"00000","data":{"entrustedList":[{"orderId":"plan","symbol":"BTCUSDT","size":"0.02","triggerPrice":"80","side":"sell","holdSide":"long","orderType":"market","planType":"pos_loss","planStatus":"live"}]}}"#,
    );
    h.respond("planType=track_plan", 200, r#"{"code":"00000","data":[]}"#);
    assert_eq!(h.engine.store().order("plan").unwrap().amount, 0.0);
}

#[test]
fn sized_plan_keeps_its_size() {
    for plan in ["loss_plan", "profit_plan"] {
        let mut h = Harness::classic();
        h.private_message(&algo_push(plan, "0.02"));
        assert_eq!(
            h.engine.store().order("plan").unwrap().amount,
            0.02,
            "{plan}"
        );
    }
}
