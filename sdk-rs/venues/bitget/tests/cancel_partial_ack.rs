//! A batch cancel Bitget acknowledges for only some orders: the rest may still rest.
mod support;
use safe_cex_engine::Command;
use safe_cex_engine::model::OrderStatus;
use support::*;

const TWO_ORDERS: &str = r#"{"code":"00000","data":{"entrustedList":[{"orderId":"1","symbol":"BTCUSDT","size":"2","baseVolume":"0","price":"90","side":"buy","orderType":"limit","status":"live","clientOid":"one"},{"orderId":"2","symbol":"BTCUSDT","size":"1","baseVolume":"0","price":"110","side":"sell","orderType":"limit","status":"live","clientOid":"two"}],"endId":""}}"#;

fn started() -> Harness {
    let mut h = Harness::private_started();
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"40084","msg":"Classic Account mode","data":null}"#,
    );
    h.respond("/all-position", 200, POSITION);
    h.respond("/account/accounts", 200, ACCOUNT);
    h.orders(TWO_ORDERS);
    h.login();
    h
}

#[test]
fn a_partly_acknowledged_cancel_names_the_rest_and_reads_orders_again() {
    let mut h = started();
    h.command(Command::CancelOrders {
        command: 7,
        ids: vec!["1".into(), "2".into()],
    });
    let cancel = h.take("/batch-cancel-orders");
    // A read already in flight predates the cancel, so it cannot settle what still rests.
    let stale = h.requests.iter().any(|r| r.url.contains("/orders-pending"));
    h.reply(
        cancel.id,
        200,
        r#"{"code":"00000","data":{"successList":[{"orderId":"1"}],"failureList":[]}}"#,
    );
    if stale {
        h.orders(TWO_ORDERS);
    }
    assert!(
        h.engine
            .store()
            .order("1")
            .is_none_or(|o| o.status == OrderStatus::Canceled)
    );
    assert_eq!(
        h.engine.store().order("2").unwrap().status,
        OrderStatus::Open
    );
    assert!(
        h.requests.iter().any(|r| r.url.contains("/orders-pending")),
        "the open orders are read again: {:?}",
        h.requests.iter().map(|r| &r.url).collect::<Vec<_>>()
    );
    let error = h.failures.last().expect("the cancel fails").legacy_text();
    assert!(error.contains("BTCUSDT sell limit 2"), "{error}");
    assert!(error.contains("may still be open"), "{error}");
}
