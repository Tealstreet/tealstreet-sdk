//! Unknown cancels keep their result; only a fresh native census proves open/absent.
mod support;
use safe_cex_engine::{Command, Outcome, VenueAction};
use safe_cex_engine::model::OrderStatus;
use support::*;

const EMPTY: &str = r#"{"code":"00000","data":[]}"#;
const PLANS: &str = r#"{"code":"00000","data":{"entrustedList":[{"orderId":"target","symbol":"BTCUSDT","size":"0","triggerPrice":"120","side":"buy","holdSide":"short","planType":"pos_loss","status":"live"},{"orderId":"sibling","symbol":"BTCUSDT","size":"0","triggerPrice":"80","side":"buy","holdSide":"short","planType":"pos_profit","status":"live"}]}}"#;
const SIBLING: &str = r#"{"code":"00000","data":{"entrustedList":[{"orderId":"sibling","symbol":"BTCUSDT","size":"0","triggerPrice":"80","side":"buy","holdSide":"short","planType":"pos_profit","status":"live"}]}}"#;

fn ready() -> Harness {
    let mut h = Harness::classic();
    while h.requests.iter().any(|r| r.url.contains("/orders-pending")) {
        h.orders(OPEN_ORDER);
    }
    h.private_message(r#"{"arg":{"channel":"orders-algo"},"data":[{"orderId":"target","symbol":"BTCUSDT","size":"0","triggerPrice":"120","side":"buy","holdSide":"short","planType":"pos_loss","status":"live"},{"orderId":"sibling","symbol":"BTCUSDT","size":"0","triggerPrice":"80","side":"buy","holdSide":"short","planType":"pos_profit","status":"live"}]}"#);
    assert_eq!(h.engine.store().order("target").unwrap().status, OrderStatus::Open);
    h
}

fn cancel_unknown(h: &mut Harness) {
    h.command(Command::CancelOrders { command: 90, ids: vec!["target".into()] });
    let request = h.take("/cancel-plan-order");
    assert!(request.body.as_deref().unwrap().contains("pos_loss"));
    h.reply(request.id, 200, r#"{"code":"00000","data":{"successList":[],"failureList":[]}}"#);
    assert_eq!(h.failures.last().unwrap().outcome, Some(Outcome::Unknown));
    assert_eq!(h.engine.store().order("target").unwrap().status, OrderStatus::Open);
}

fn census(h: &mut Harness, plans: &str) {
    h.respond("/orders-pending", 200, OPEN_ORDER);
    h.respond("planType=profit_loss", 200, plans);
    h.respond("planType=track_plan", 200, EMPTY);
}

fn confirmations(h: &Harness, result: &str) -> usize {
    h.events.iter().filter(|e| e.code == "bitget.cancel.confirmed" && e.fields.iter()
        .any(|(key,value)| *key == "result" && matches!(value, safe_cex_engine::Value::Str(s) if s == result))).count()
}

#[test]
fn clean_native_absence_beats_cached_algo_retention_without_claiming_canceled() {
    let mut h = ready();
    cancel_unknown(&mut h);
    census(&mut h, SIBLING);
    assert!(h.engine.store().order("target").is_none(), "fresh native absence must beat cached algo retention");
    assert_eq!(h.engine.store().order("sibling").unwrap().status, OrderStatus::Open);
    assert_eq!(confirmations(&h, "absent"), 1);
    assert_eq!(h.failures.len(), 1, "original Unknown is not rewritten");
    assert!(!h.requests.iter().any(|r| r.url.contains("/cancel-plan-order")));
}

#[test]
fn a_clean_native_present_target_is_reported_still_open() {
    let mut h = ready();
    cancel_unknown(&mut h);
    census(&mut h, PLANS);
    assert_eq!(h.engine.store().order("target").unwrap().status, OrderStatus::Open);
    assert_eq!(confirmations(&h, "present"), 1);
    assert_eq!(h.failures.len(), 1);
}

#[test]
fn a_pre_cancel_census_proves_nothing_and_owes_one_fresh_successor() {
    let mut h = ready();
    h.command(Command::Venue { command: 80, action: VenueAction::RefreshOrders { symbols: None } });
    cancel_unknown(&mut h);
    census(&mut h, SIBLING);
    assert_eq!(h.engine.store().order("target").unwrap().status, OrderStatus::Open);
    assert_eq!(confirmations(&h, "absent"), 0);
    assert_eq!(h.requests.iter().filter(|r| r.url.contains("/orders-pending")).count(), 1);
    census(&mut h, SIBLING);
    assert!(h.engine.store().order("target").is_none());
    assert_eq!(confirmations(&h, "absent"), 1);
}
