//! Stop and take-profit edits BloFin makes in place: only the edited field is
//! sent, a refusal leaves the order as it was, and an accepted edit must read
//! back exactly as asked.
mod support;
use safe_cex_engine::model::OrderType;
use safe_cex_engine::{Action, Command, Event, Notice};
use support::*;

fn failure(h: &Harness) -> Option<safe_cex_engine::VenueError> {
    h.actions.iter().find_map(|a| match a {
        Action::Notice(Notice::CommandFailed { error, .. }) => Some(error.clone()),
        _ => None,
    })
}
fn done(h: &Harness) -> bool {
    h.actions
        .iter()
        .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. })))
}
fn body(r: &safe_cex_engine::HttpRequest) -> serde_json::Value {
    serde_json::from_str(r.body.as_deref().unwrap()).unwrap()
}
fn amend(h: &mut Harness, kind: OrderType, price: Option<f64>, amount: Option<f64>) {
    let id =
        h.e.store()
            .orders()
            .iter()
            .find(|o| o.kind == kind)
            .unwrap()
            .id
            .clone();
    h.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 7,
            id,
            price,
            amount,
        }),
    );
    h.collect();
}
fn destructive(h: &Harness) -> bool {
    h.pending.iter().any(|r| {
        r.url.contains("cancel") || r.url.contains("order-tpsl") || r.url.contains("batch-orders")
    })
}

/// A two-leg tpsl order; its stop loss is edited.
fn bracket(h: &mut Harness) {
    h.open_private();
    h.seed();
    h.ws(r#"{"arg":{"channel":"orders-algo"},"data":[{"instId":"BTC-USDT","tpslId":"500","side":"sell","size":"4","marginMode":"cross","positionSide":"net","tpTriggerPrice":"65000","tpOrderPrice":"-1","tpTriggerPriceType":"last","slTriggerPrice":"55000","slOrderPrice":"-1","slTriggerPriceType":"last","state":"live"}]}"#);
}

#[test]
fn a_tpsl_stop_edit_sends_only_its_trigger_and_must_read_back_both_legs() {
    let mut h = Harness::new();
    bracket(&mut h);
    amend(&mut h, OrderType::StopLoss, Some(56000.0), None);
    let r = h.take("amend-tpsl");
    assert_eq!(
        body(&r),
        serde_json::json!({"instId":"BTC-USDT","tpslId":"500","newSlTriggerPrice":"56000"})
    );
    assert!(!destructive(&h));
    h.respond(
        r,
        r#"{"code":"0","msg":"success","data":{"tpslId":"500","code":"0","msg":null}}"#,
    );
    let read = h.take("orders-tpsl-pending");
    assert!(
        read.url.ends_with("instId=BTC-USDT&tpslId=500"),
        "{}",
        read.url
    );
    h.respond(read, r#"{"code":"0","data":[{"tpslId":"500","tpTriggerPrice":"65000","slTriggerPrice":"56000","size":"4","state":"live"}]}"#);
    assert!(done(&h), "{:?}", failure(&h).map(|e| e.message));
}

#[test]
fn an_accepted_tpsl_edit_that_drops_the_other_leg_is_a_typed_error() {
    let mut h = Harness::new();
    bracket(&mut h);
    amend(&mut h, OrderType::StopLoss, Some(56000.0), None);
    h.reply(
        "amend-tpsl",
        r#"{"code":"0","msg":"success","data":{"tpslId":"500","code":"0","msg":null}}"#,
    );
    h.reply("orders-tpsl-pending", r#"{"code":"0","data":[{"tpslId":"500","tpTriggerPrice":null,"slTriggerPrice":"56000","size":"4","state":"live"}]}"#);
    let error = failure(&h).expect("the read-back disagrees");
    assert!(
        error.message.contains("does not read as edited")
            && error.message.contains("tpTriggerPrice"),
        "{}",
        error.message
    );
    assert!(
        !destructive(&h),
        "no cancel or re-place follows a disagreeing read"
    );
    // The edit took effect (its accepted write leaves no NotSent or Rejected
    // outcome); what it left does not read as asked.
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::Internal));
    assert_eq!(error.outcome, None);
    let outcomes = error.elements.as_ref().expect("applied edit outcome");
    assert_eq!(outcomes.len(), 1);
    assert!(
        matches!(&outcomes[0].result, safe_cex_engine::write_plan::ElementResult::Applied {
        effect: safe_cex_engine::write_plan::AppliedEffect::Amend, target_order_id: Some(id), ..
    } if id == "500")
    );
    assert!(
        error
            .partial
            .as_ref()
            .is_none_or(|partial| partial.order_ids.is_empty())
    );
}

#[test]
fn a_failed_read_back_is_typed_apart_from_a_mismatch() {
    let mut h = Harness::new();
    bracket(&mut h);
    amend(&mut h, OrderType::StopLoss, Some(56000.0), None);
    h.reply(
        "amend-tpsl",
        r#"{"code":"0","msg":"success","data":{"tpslId":"500","code":"0","msg":null}}"#,
    );
    let read = h.take("orders-tpsl-pending");
    h.respond_status(read, 503, "");
    let error = failure(&h).expect("unconfirmed");
    assert!(
        error.message.contains("could not be read back"),
        "{}",
        error.message
    );
    assert_eq!(
        error.kind,
        Some(safe_cex_engine::ErrorKind::VenueUnavailable)
    );
    assert_eq!(error.outcome, None, "the accepted edit still took effect");
}

#[test]
fn a_tpsl_size_edit_sends_only_the_new_size() {
    let mut h = Harness::new();
    bracket(&mut h);
    amend(&mut h, OrderType::StopLoss, None, Some(3.0));
    let r = h.take("amend-tpsl");
    let sent = body(&r);
    assert_eq!(sent["tpslId"], "500");
    assert!(sent.get("newSize").is_some());
    assert!(sent.get("newSlTriggerPrice").is_none() && sent.get("newTpTriggerPrice").is_none());
}

/// A limit entry carrying both attached legs, from the open-orders census.
fn attached(h: &mut Harness, filled: &str, state: &str) {
    h.open_private();
    h.modes();
    h.reply(
        "account/positions",
        &format!("{{\"code\":\"0\",\"data\":[{POSITION}]}}"),
    );
    h.reply(
        "balances",
        r#"{"code":"0","data":[{"currency":"USDT","balance":"1010","available":"1000","frozen":"10"}]}"#,
    );
    h.reply("orders-pending", &format!(r#"{{"code":"0","data":[{{"instId":"BTC-USDT","orderCategory":"normal","orderId":"12","orderType":"limit","side":"buy","marginMode":"cross","positionSide":"net","size":"10","filledSize":"{filled}","price":"60000","state":"{state}","tpTriggerPrice":"65000","tpOrderPrice":"-1","slTriggerPrice":"55000","slOrderPrice":"-1"}}]}}"#));
    // The census pages on until an empty page.
    h.reply("orders-pending", r#"{"code":"0","data":[]}"#);
    for path in ["orders-tpsl-pending", "orders-algo-pending"] {
        h.reply(path, r#"{"code":"0","data":[]}"#);
    }
}

#[test]
fn an_attached_stop_on_an_open_limit_is_edited_in_place_and_read_back() {
    let mut h = Harness::new();
    attached(&mut h, "0", "live");
    amend(&mut h, OrderType::StopLoss, Some(56000.0), None);
    let r = h.take("amend-order");
    assert_eq!(
        body(&r),
        serde_json::json!({"instId":"BTC-USDT","orderId":"12","newSlTriggerPrice":"56000"})
    );
    assert!(!destructive(&h), "the entry is never cancelled");
    h.respond(
        r,
        r#"{"code":"0","msg":"success","data":{"orderId":"12","code":"0","msg":null}}"#,
    );
    let read = h.take("order-detail");
    assert!(
        read.url.ends_with("instId=BTC-USDT&orderId=12"),
        "{}",
        read.url
    );
    h.respond(read, r#"{"code":"0","data":{"orderId":"12","price":"60000","size":"10","tpTriggerPrice":"65000","slTriggerPrice":"56000","state":"live"}}"#);
    assert!(done(&h), "{:?}", failure(&h).map(|e| e.message));
}

#[test]
fn an_attached_edit_that_moved_the_entry_is_a_typed_error() {
    let mut h = Harness::new();
    attached(&mut h, "0", "live");
    amend(&mut h, OrderType::TakeProfit, Some(66000.0), None);
    let r = h.take("amend-order");
    assert_eq!(
        body(&r),
        serde_json::json!({"instId":"BTC-USDT","orderId":"12","newTpTriggerPrice":"66000"})
    );
    h.respond(
        r,
        r#"{"code":"0","msg":"success","data":{"orderId":"12","code":"0","msg":null}}"#,
    );
    h.reply("order-detail", r#"{"code":"0","data":{"orderId":"12","price":"59000","size":"10","tpTriggerPrice":"66000","slTriggerPrice":"55000","state":"live"}}"#);
    let error = failure(&h).expect("the entry moved");
    assert!(
        error.message.contains("price reads 59000"),
        "{}",
        error.message
    );
}

#[test]
fn an_attached_stop_on_a_partly_filled_entry_is_not_edited_in_place() {
    let mut h = Harness::new();
    attached(&mut h, "2", "partially_filled");
    amend(&mut h, OrderType::StopLoss, Some(56000.0), None);
    assert!(
        !h.pending.iter().any(|r| r.url.contains("amend-order")),
        "a partly filled entry keeps the replacement path"
    );
}

#[test]
fn a_trigger_order_without_legs_amends_its_trigger_in_place() {
    let mut h = Harness::new();
    h.open_private();
    h.seed();
    h.ws(r#"{"arg":{"channel":"orders-algo"},"data":[{"instId":"BTC-USDT","algoId":"78","orderType":"trigger","side":"buy","size":"3","triggerPrice":"61000","orderPrice":"60900","triggerPriceType":"mark","marginMode":"isolated","positionSide":"long","state":"live"}]}"#);
    amend(&mut h, OrderType::Conditional, Some(61500.0), None);
    let r = h.take("amend-algo");
    assert_eq!(
        body(&r),
        serde_json::json!({"instId":"BTC-USDT","algoId":"78","newTriggerPrice":"61500"})
    );
    h.respond(
        r,
        r#"{"code":"0","msg":"success","data":{"algoId":"78","code":"0","msg":null}}"#,
    );
    let read = h.take("orders-algo-pending");
    assert!(
        read.url.ends_with("instId=BTC-USDT&algoId=78"),
        "{}",
        read.url
    );
    h.respond(read, r#"{"code":"0","data":[{"algoId":"78","triggerPrice":"61500","orderPrice":"60900","size":"3","state":"live"}]}"#);
    assert!(done(&h), "{:?}", failure(&h).map(|e| e.message));
}
