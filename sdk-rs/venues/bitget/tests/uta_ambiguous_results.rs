//! Bitget answers that name a busy server or a per-order failure: never read as a refusal or a success.
mod support;
use safe_cex_engine::model::*;
use safe_cex_engine::{Command, Method};
use serde_json::json;
use support::*;

fn uta(hedged: bool) -> Harness {
    let mut h = Harness::private_started();
    h.respond(
        "/account/settings",
        200,
        &json!({"code":"00000","data":{"holdMode":if hedged {"hedge_mode"}else{"one_way_mode"}}})
            .to_string(),
    );
    h.respond("/current-position", 200, r#"{"code":"00000","data":{"list":[{"symbol":"BTCUSDT","category":"USDT-FUTURES","marginCoin":"USDT","posSide":"long","total":"0.01","avgPrice":"100","marginMode":"crossed","leverage":"10","holdMode":"one_way_mode"}]}}"#);
    h.respond("/account/assets", 200, r#"{"code":"00000","data":{"accountEquity":"1000","effEquity":"1000","unrealisedPnl":"0"}}"#);
    h.respond(
        "/unfilled-orders",
        200,
        r#"{"code":"00000","data":{"list":[]}}"#,
    );
    while h
        .requests
        .iter()
        .any(|r| r.url.contains("/unfilled-strategy-orders"))
    {
        h.respond(
            "/unfilled-strategy-orders",
            200,
            r#"{"code":"00000","data":[]}"#,
        );
    }
    h.login();
    h.engine.store_mut().set_hedged(hedged);
    h.engine.store_mut().update_setting("BTCUSDT", |s| {
        s.leverage = 10.0;
        s.margin_type = Some(MarginType::Cross);
    });
    h
}
fn settle_settings(h: &mut Harness) {
    while h
        .requests
        .iter()
        .any(|r| r.url.contains("/account/settings"))
    {
        h.respond(
            "/account/settings",
            200,
            r#"{"code":"00000","data":{"holdMode":"one_way_mode"}}"#,
        );
    }
}

#[test]
fn a_busy_or_timeout_answer_to_a_moved_stop_says_it_may_be_live() {
    for code in ["50008", "40015", "40200", "40604", "40010"] {
        let mut h = uta(false);
        h.private_message(r#"{"action":"snapshot","arg":{"instType":"UTA","topic":"strategy-order","symbol":"default"},"data":[{"category":"usdt-futures","symbol":"BTCUSDT","orderId":"66","qty":"0.01","side":"sell","posSide":"long","reduceOnly":"yes","type":"tpsl","status":"pending","slTriggerBy":"market","stopLoss":"90","slOrderType":"market","createdTime":"1730186725663","updatedTime":"1730186725691"}],"ts":1730186725700}"#);
        let o = h.engine.store().order("66").cloned().expect("stop row");
        assert_eq!(o.kind, OrderType::StopLoss);
        h.command(Command::AmendOrder {
            command: 9,
            id: "66".into(),
            price: Some(85.0),
            amount: Some(0.0),
        });
        settle_settings(&mut h);
        let cancel = h.take("/cancel-strategy-order");
        h.reply(cancel.id, 200, r#"{"code":"00000","data":null}"#);
        let replace = h.take("/place-strategy-order");
        h.reply(
            replace.id,
            200,
            &format!(r#"{{"code":"{code}","msg":"Request timeout","data":null}}"#),
        );
        let failure = h.failures.last().expect("the move fails visibly");
        assert!(
            failure.message.contains("may have no stop"),
            "{code}: {}",
            failure.message
        );
        assert!(
            !failure.message.contains("replacement refused"),
            "{code}: {}",
            failure.message
        );
        assert!(
            !h.requests
                .iter()
                .any(|r| r.url.contains("/place-strategy-order") && r.method == Method::Post),
            "{code}: no rollback send"
        );
    }
}

#[test]
fn an_order_bitget_would_not_cancel_fails_the_account_cancel() {
    let mut h = uta(false);
    h.command(Command::Venue {
        command: 5,
        action: safe_cex_engine::VenueAction::CancelAccountOrders,
    });
    let r = h.take("/cancel-symbol-order");
    h.reply(r.id, 200, r#"{"code":"00000","msg":"success","data":{"list":[{"orderId":"1","clientOid":"a","code":"24056","msg":"notExisted"},{"orderId":"2","code":"25204","msg":"cannot cancel"}]}}"#);
    let result = h
        .done
        .iter()
        .find(|(id, _)| *id == 5)
        .map(|(_, r)| r.clone());
    assert!(matches!(result, Some(Err(_))), "{result:?}");
}
