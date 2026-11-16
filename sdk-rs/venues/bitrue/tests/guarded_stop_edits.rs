//! Stop and take-profit edits Bitrue makes by cancel and replace: the cancel
//! is checked again at its final claim, proven terminal before the
//! replacement, and a definitely refused replacement puts the original back once.
mod support;
use safe_cex_engine::error::Protection;
use safe_cex_engine::model::*;
use safe_cex_engine::{Command, Event, HttpRequest, Method, Notice, Outcome, VenueError};
use support::trading::*;

const REFUSED: &str = r#"{"code":"-1102","msg":"Required parameter missing"}"#;

/// The job's own census: the newest open-order read (D4).
fn post_write_census(h: &mut Harness) -> HttpRequest {
    let newest = h
        .requests
        .iter()
        .filter(|r| r.url.contains("/fapi/v2/openOrders"))
        .map(|r| r.id)
        .max_by_key(|id| id.0)
        .expect("the job's census");
    let index = h.requests.iter().position(|r| r.id == newest).unwrap();
    h.requests.remove(index)
}
fn failure(h: &Harness) -> VenueError {
    h.notices
        .iter()
        .find_map(|n| match n {
            Notice::CommandFailed { command: 7, error } => Some(error.clone()),
            _ => None,
        })
        .expect("a failure")
}
fn order_posts(h: &Harness) -> usize {
    h.requests
        .iter()
        .filter(|r| r.method == Method::Post && r.url.ends_with("/fapi/v2/order"))
        .count()
}
fn cancels(h: &Harness) -> usize {
    h.requests
        .iter()
        .filter(|r| r.method == Method::Post && r.url.ends_with("/fapi/v2/cancel"))
        .count()
}
fn body(request: &HttpRequest) -> serde_json::Value {
    serde_json::from_str(request.body.as_deref().unwrap()).unwrap()
}
/// A reduce-only sell stop at 1800 on the mark, guarding a long; mark and last 2000.
fn guarded_stop() -> Harness {
    let mut h = Harness::new();
    h.e.store_mut().set_positions(vec![Position {
        id: "E-BTC-USDT".into(),
        symbol: "E-BTC-USDT".into(),
        side: PositionSide::Long,
        entry_price: 2000.0,
        notional: 20.0,
        leverage: 20.0,
        margin_type: Some(MarginType::Cross),
        position_mode: Some(PositionMode::Oneway),
        unrealized_pnl: 0.0,
        realized_pnl: None,
        contracts: 0.01,
        liquidation_price: 0.0,
        margin_size: None,
        last_update: None,
    }]);
    h.e.store_mut().upsert_order(Order {
        id: "456".into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "E-BTC-USDT".into(),
        kind: OrderType::StopLoss,
        side: OrderSide::Sell,
        price: 1800.0,
        amount: 0.001,
        filled: 0.0,
        remaining: 0.001,
        reduce_only: true,
        close: None,
        client_order_id: None,
        time_in_force: Some(TimeInForce::GoodTillCancel),
        last_update: 0.0,
        trigger_type: Some(StopTriggerType::MarkPrice),
        take_profit: None,
        stop_loss: None,
        extra: None,
    });
    h
}
fn detail(status: &str, filled: f64) -> String {
    format!(
        r#"{{"code":"0","data":[{{"orderId":456,"contractName":"E-BTC-USDT","status":"{status}","origQty":0.001,"executedQty":{filled},"side":"SELL","type":"MARKET","action":"CLOSE","conditionOrder":true,"triggerType":1,"triggerPriceType":2,"triggerPrice":1800}}]}}"#
    )
}
fn amend(h: &mut Harness) {
    h.command(Command::AmendOrder {
        command: 7,
        id: "456".into(),
        price: Some(1790.0),
        amount: None,
    });
}
/// Inspect read, then the cancel ready for its final claim.
fn to_cancel(h: &mut Harness) -> HttpRequest {
    amend(h);
    let inspect = h.take(Method::Get, "/fapi/v2/order?");
    h.reply(inspect, &detail("INIT", 0.0));
    h.take(Method::Post, "/fapi/v2/cancel")
}
/// The cancel lands and is proven: the replacement is next.
fn to_replacement(h: &mut Harness) -> HttpRequest {
    let cancel = to_cancel(h);
    h.reply(cancel, r#"{"code":"0","data":{"orderId":456}}"#);
    let verify = h.take(Method::Get, "/fapi/v2/order?");
    h.reply(verify, &detail("CANCELED", 0.0));
    let create = h.take(Method::Post, "/fapi/v2/order");
    assert_eq!(body(&create)["triggerPrice"], 1790.0);
    create
}

#[test]
fn a_stale_crossed_changed_or_flat_stop_at_the_final_claim_cancels_nothing() {
    for (case, why) in [
        ("stale", "mark price reference is stale"),
        ("crossed", "already crossed the mark price"),
        ("changed", "changed or filled before cancellation"),
        ("flat", "no position for it to protect"),
    ] {
        let mut h = guarded_stop();
        amend(&mut h);
        let inspect = h.take(Method::Get, "/fapi/v2/order?");
        h.answer_only(inspect, &detail("INIT", 0.0));
        // Between the Inspect read and the cancel's final claim.
        match case {
            "stale" => h.now += 6_000.0,
            "crossed" => {
                h.e.store_mut()
                    .update_ticker("E-BTC-USDT", |t| t.mark = 1785.0)
            }
            "changed" => {
                h.e.store_mut().update_order("456", |o| o.filled = 0.0005);
            }
            _ => h.e.store_mut().set_positions(vec![]),
        }
        h.collect();
        assert_eq!(cancels(&h), 0, "{case}: no cancel");
        let error = failure(&h);
        assert!(
            error.message.contains("was not changed") && error.message.contains(why),
            "{case}: {}",
            error.message
        );
        assert_eq!(error.outcome, Some(Outcome::NotSent), "{case}");
        assert_eq!(error.protection, None, "{case}: never Unprotected");
        assert_ne!(
            h.e.store().order("456").unwrap().status,
            OrderStatus::Canceling,
            "{case}: the stop is left as it was"
        );
    }
}

#[test]
fn a_re_signed_cancel_is_checked_again_at_its_own_claim() {
    let mut h = guarded_stop();
    let cancel = to_cancel(&mut h);
    h.e.store_mut()
        .update_ticker("E-BTC-USDT", |t| t.mark = 1785.0);
    h.reply(
        cancel,
        r#"{"code":"-1022","msg":"Signature for this request is not valid."}"#,
    );
    assert_eq!(cancels(&h), 0, "no re-signed cancel");
    let error = failure(&h);
    assert!(
        error.message.contains("already crossed the mark price"),
        "{}",
        error.message
    );
    assert_eq!(error.protection, None);
}

#[test]
fn a_cancel_still_processing_is_re_read_until_proven() {
    let mut h = guarded_stop();
    let cancel = to_cancel(&mut h);
    h.reply(cancel, r#"{"code":"0","data":{"orderId":456}}"#);
    let verify = h.take(Method::Get, "/fapi/v2/order?");
    h.reply(verify, &detail("PENDING_CANCEL", 0.0));
    assert_eq!(
        order_posts(&h),
        0,
        "no replacement while the cancel is live"
    );
    h.elapse();
    let verify = h.take(Method::Get, "/fapi/v2/order?");
    h.reply(verify, &detail("CANCELED", 0.0));
    assert_eq!(order_posts(&h), 1, "the replacement follows the proof");
}

#[test]
fn a_cancel_never_proven_is_unknown_with_no_replacement() {
    for lost in [false, true] {
        let mut h = guarded_stop();
        let cancel = to_cancel(&mut h);
        h.reply(cancel, r#"{"code":"0","data":{"orderId":456}}"#);
        if lost {
            while let Some(index) = h
                .requests
                .iter()
                .position(|r| r.method == Method::Get && r.url.contains("/fapi/v2/order?"))
            {
                let verify = h.requests.remove(index);
                h.handle(
                    h.now,
                    Event::HttpFailed {
                        request: verify.id,
                        error: "lost read",
                    },
                );
                h.elapse();
            }
        } else {
            h.verify_until_unproven(&detail("NEW", 0.0));
        }
        let error = failure(&h);
        assert!(
            error.message.contains("could not be confirmed")
                && error.message.contains("Check Open Orders"),
            "lost {lost}: {}",
            error.message
        );
        assert_eq!(error.outcome, Some(Outcome::Unknown), "lost {lost}");
        assert_eq!(error.protection, None);
        assert_eq!(
            order_posts(&h),
            0,
            "lost {lost}: no replacement or put-back"
        );
    }
}

#[test]
fn a_stop_that_fired_during_its_cancel_reports_the_fill() {
    let mut h = guarded_stop();
    let cancel = to_cancel(&mut h);
    h.reply(cancel, r#"{"code":"0","data":{"orderId":456}}"#);
    let verify = h.take(Method::Get, "/fapi/v2/order?");
    h.reply(verify, &detail("FILLED", 0.001));
    let error = failure(&h);
    assert!(
        error.message.contains("filled before its cancel landed"),
        "{}",
        error.message
    );
    assert_eq!(
        error.protection, None,
        "the fill is reported, not Unprotected"
    );
    assert_eq!(order_posts(&h), 0);
}

#[test]
fn a_definitely_refused_replacement_puts_the_original_back_once() {
    let mut h = guarded_stop();
    let create = to_replacement(&mut h);
    h.reply(create, REFUSED);
    let restore = h.take(Method::Post, "/fapi/v2/order");
    let original = body(&restore);
    assert_eq!(
        original["triggerPrice"], 1800.0,
        "the original, not the edit"
    );
    assert_eq!(original["triggerType"], 1);
    assert_eq!(original["triggerPriceType"], 2);
    assert_eq!(original["type"], "MARKET");
    let client = original["clientOrderId"].as_str().unwrap().to_owned();
    h.reply(
        restore,
        &format!(r#"{{"code":"0","data":{{"orderId":789,"clientOrderId":"{client}"}}}}"#),
    );
    // The job's own census confirms the put-back rests (D4).
    let census = post_write_census(&mut h);
    h.reply(
        census,
        &format!(
            r#"{{"code":"0","data":[{{"orderId":789,"clientOrderId":"{client}","contractName":"E-BTC-USDT","type":"MARKET","side":"SELL","status":"INIT","action":"CLOSE","origQty":0.001,"executedQty":0,"triggerPrice":1800,"triggerType":1,"triggerPriceType":2,"conditionOrder":true}}]}}"#
        ),
    );
    let error = failure(&h);
    assert!(
        error.message.contains("is unchanged") && error.message.contains("put back"),
        "{}",
        error.message
    );
    assert_eq!(error.protection, Some(Protection::Restored));
    assert_eq!(order_posts(&h), 0, "one put-back only");
}

#[test]
fn a_refused_put_back_is_unprotected_and_never_retried() {
    let mut h = guarded_stop();
    let create = to_replacement(&mut h);
    h.reply(create, REFUSED);
    let restore = h.take(Method::Post, "/fapi/v2/order");
    h.reply(restore, REFUSED);
    let error = failure(&h);
    assert!(error.message.contains("both refused"), "{}", error.message);
    assert_eq!(error.protection, Some(Protection::Unprotected));
    assert_eq!(order_posts(&h), 0);
}

#[test]
fn a_rate_limited_or_unknown_replacement_is_never_put_back() {
    for (status, reply) in [
        (429, r#"{"code":"429","msg":"Too many requests"}"#),
        (503, ""),
        (408, ""),
    ] {
        let mut h = guarded_stop();
        let create = to_replacement(&mut h);
        h.reply_status(create, status, reply);
        let error = failure(&h);
        assert_eq!(error.protection, None, "{status}");
        assert_eq!(order_posts(&h), 0, "{status}: no put-back");
    }
}

#[test]
fn a_put_back_sealed_on_a_5xx_says_its_outcome_is_unknown() {
    for status in [408, 503] {
        let mut h = guarded_stop();
        let create = to_replacement(&mut h);
        h.reply(create, REFUSED);
        let restore = h.take(Method::Post, "/fapi/v2/order");
        h.reply_status(restore, status, "");
        let error = failure(&h);
        assert!(
            error.message.contains("put-back's outcome is unknown")
                && error.message.contains("Check Open Orders")
                && !error.message.contains("re-place"),
            "{status}: {}",
            error.message
        );
        assert_eq!(error.outcome, Some(Outcome::Unknown), "{status}");
        assert_eq!(error.protection, None, "{status}");
        assert_eq!(order_posts(&h), 0, "{status}: never resent");
    }
}

#[test]
fn a_stop_that_partly_filled_during_its_cancel_is_not_replaced() {
    let mut h = guarded_stop();
    let cancel = to_cancel(&mut h);
    h.reply(cancel, r#"{"code":"0","data":{"orderId":456}}"#);
    let verify = h.take(Method::Get, "/fapi/v2/order?");
    h.reply(verify, &detail("CANCELED", 0.0005));
    let error = failure(&h);
    assert!(
        error.message.contains("was removed") && error.message.contains("please re-place it"),
        "{}",
        error.message
    );
    assert_eq!(error.protection, Some(Protection::Unprotected));
    assert_eq!(order_posts(&h), 0);
}

/// A size edit's put-back carries the original's size from the Inspect read,
/// never the edited size.
#[test]
fn a_size_edit_put_back_restores_the_original_volume() {
    let mut h = guarded_stop();
    h.command(Command::AmendOrder {
        command: 7,
        id: "456".into(),
        price: Some(1790.0),
        amount: Some(0.002),
    });
    let inspect = h.take(Method::Get, "/fapi/v2/order?");
    h.reply(inspect, &detail("INIT", 0.0));
    let cancel = h.take(Method::Post, "/fapi/v2/cancel");
    h.reply(cancel, r#"{"code":"0","data":{"orderId":456}}"#);
    let verify = h.take(Method::Get, "/fapi/v2/order?");
    h.reply(verify, &detail("CANCELED", 0.0));
    let create = h.take(Method::Post, "/fapi/v2/order");
    let edited = body(&create);
    h.reply(create, REFUSED);
    let restore = h.take(Method::Post, "/fapi/v2/order");
    let original = body(&restore);
    assert_eq!(edited["volume"], "0.002", "the edit");
    assert_eq!(
        original["volume"], "0.001",
        "the original's size, not the edit's"
    );
    assert_eq!(original["triggerPrice"], 1800.0);
}

#[test]
fn documented_protective_refusals_restore_the_exact_original_once() {
    for code in [
        -1111, -1136, -1137, -1138, -1139, -1140, -1154, -1155, -1156, -1158, -1159, -1160, -1161,
        -1162, -1164, -1165, -1166,
    ] {
        let mut h = guarded_stop();
        let create = to_replacement(&mut h);
        h.reply(
            create,
            &format!(r#"{{"code":"{code}","msg":"documented placement refusal"}}"#),
        );
        assert_eq!(order_posts(&h), 1, "{code}: one exact put-back");
        let restore = h.take(Method::Post, "/fapi/v2/order");
        let original = body(&restore);
        assert_eq!(original["triggerPrice"], 1800.0, "{code}");
        assert_eq!(original["volume"], "0.001", "{code}");
        assert_eq!(original["type"], "MARKET", "{code}");
        assert_eq!(original["triggerType"], 1, "{code}");
        // Refusal of that one put-back is explicit, not a third placement.
        h.reply(restore, REFUSED);
        assert_eq!(
            failure(&h).protection,
            Some(Protection::Unprotected),
            "{code}"
        );
        assert!(failure(&h).message.contains("re-place"), "{code}");
        assert_eq!(order_posts(&h), 0, "{code}: never retried");
    }
}

#[test]
fn ambiguous_protective_answers_are_unknown_and_never_restore() {
    for (status, reply) in [
        (418, r#"{"code":"-1111","msg":"precision"}"#),
        (429, r#"{"code":"-1111","msg":"precision"}"#),
        (503, r#"{"code":"-1111","msg":"precision"}"#),
        (408, r#"{"code":"-1111","msg":"precision"}"#),
        (200, r#"{"code":"-1003","msg":"request rate"}"#),
        (200, r#"{"code":"-5999","msg":"unlisted"}"#),
        (200, r#"{"code":"-1007","msg":"backend timeout"}"#),
        (200, "not JSON"),
        (200, r#"{"code":"0","data":{}}"#),
        (
            200,
            r#"{"code":"-1111","msg":"precision","data":{"orderId":999}}"#,
        ),
    ] {
        let mut h = guarded_stop();
        let create = to_replacement(&mut h);
        h.reply_status(create, status, reply);
        let error = failure(&h);
        assert_eq!(
            error.outcome,
            Some(Outcome::Unknown),
            "{status} {reply}: {error:?}"
        );
        assert_eq!(error.protection, None);
        assert_eq!(order_posts(&h), 0);
    }
}

#[test]
fn directly_edited_attached_legs_are_refused_before_cancel() {
    for (kind, trigger, edited, native_kind) in [
        (OrderType::StopLoss, 1800., 1790., 1),
        (OrderType::TakeProfit, 2200., 2210., 2),
    ] {
        let mut h = guarded_stop();
        h.e.store_mut().update_order("456", |order| {
            order.parent_id = Some("parent".into());
            order.kind = kind;
            order.price = trigger;
        });
        h.command(Command::AmendOrder {
            command: 7,
            id: "456".into(),
            price: Some(edited),
            amount: None,
        });
        // Drive any Inspect emitted by the old code, so it reaches its cancel.
        if let Some(index) = h
            .requests
            .iter()
            .position(|r| r.method == Method::Get && r.url.contains("/fapi/v2/order?"))
        {
            let inspect = h.requests.remove(index);
            let mut row: serde_json::Value = serde_json::from_str(&detail("INIT", 0.)).unwrap();
            row["data"][0]["triggerType"] = serde_json::json!(native_kind);
            row["data"][0]["triggerPrice"] = serde_json::json!(trigger);
            h.reply(inspect, &row.to_string());
        }
        assert_eq!(cancels(&h), 0, "{kind:?}: attached order stays untouched");
        assert_eq!(order_posts(&h), 0);
        let error = failure(&h);
        assert_eq!(error.outcome, Some(Outcome::NotSent));
        assert_eq!(error.protection, None);
        assert!(
            error.message.contains("attached") && error.message.contains("re-place"),
            "{error:?}"
        );
        assert_eq!(h.e.store().order("456").unwrap().status, OrderStatus::Open);
    }
}

#[test]
fn ordinary_amend_refusal_wire_and_outcome_match_master() {
    // Recorded before this fix on master 06a9b1326ac, using only synthetic keys.
    // Ambiguous-status rows reflect #3245's unproven-write rule; all other rows retain master's result.
    let mut expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ordinary-amend-master.json")).unwrap();
    // Shared HTTP/WS lifecycle diagnostics now omit the transport name. Keep
    // the recorded master fixture intact and pin this one presentation change.
    for case in expected["cases"].as_array_mut().unwrap() {
        if let Some(message) = case["error"]["message"].as_str()
            && message
                == "HTTP response does not prove the write outcome. The outcome is unknown. Check Open Orders"
        {
            case["error"]["message"] = serde_json::json!(
                "Response does not prove the write outcome. The outcome is unknown. Check Open Orders"
            );
        }
    }
    let mut records = Vec::new();
    for (status, code) in [
        (200, -1111),
        (200, -1136),
        (400, -1155),
        (200, -2017),
        (429, -1111),
        (503, -1111),
        (200, -5999),
        (200, -1003),
    ] {
        let mut h = Harness::new();
        h.order(0.);
        h.amend();
        let inspect = h.take(Method::Get, "/fapi/v2/order?");
        h.reply(inspect, &support::trading::detail("INIT", 0., ""));
        let cancel = h.take(Method::Post, "/fapi/v2/cancel");
        let cancel_wire =
            serde_json::json!({"url":cancel.url,"body":cancel.body,"headers":cancel.headers});
        h.reply(
            cancel,
            r#"{"code":"0","data":{"orderId":"2334151155877155281"}}"#,
        );
        let verify = h.take(Method::Get, "/fapi/v2/order?");
        h.reply(verify, &support::trading::detail("CANCELED", 0., ""));
        let create = h.take(Method::Post, "/fapi/v2/order");
        let create_wire =
            serde_json::json!({"url":create.url,"body":create.body,"headers":create.headers});
        assert_eq!(cancel_wire, expected["cancel"]);
        assert_eq!(create_wire, expected["create"]);
        h.reply_status(
            create,
            status,
            &format!(r#"{{"code":"{code}","msg":"ordinary pin"}}"#),
        );
        records.push(serde_json::json!({"status":status,"code":code,"error":failure(&h),"additionalOrderPosts":order_posts(&h)}));
    }
    assert_eq!(serde_json::json!(records), expected["cases"]);
}

#[test]
fn an_inexact_or_split_original_is_never_guessed_back() {
    for (amount, trigger) in [(0.0015, 1800.), (0.025, 1800.), (0.001, 1800.05)] {
        let mut h = guarded_stop();
        h.e.store_mut().update_order("456", |order| {
            order.amount = amount;
            order.remaining = amount;
            order.price = trigger;
        });
        h.command(Command::AmendOrder {
            command: 7,
            id: "456".into(),
            price: Some(1790.),
            amount: Some(0.001),
        });
        let native = |status| {
            let mut row: serde_json::Value = serde_json::from_str(&detail(status, 0.)).unwrap();
            row["data"][0]["origQty"] = serde_json::json!(amount);
            row["data"][0]["triggerPrice"] = serde_json::json!(trigger);
            row.to_string()
        };
        let inspect = h.take(Method::Get, "/fapi/v2/order?");
        h.reply(inspect, &native("INIT"));
        let cancel = h.take(Method::Post, "/fapi/v2/cancel");
        h.reply(cancel, r#"{"code":"0","data":{"orderId":456}}"#);
        let verify = h.take(Method::Get, "/fapi/v2/order?");
        h.reply(verify, &native("CANCELED"));
        let create = h.take(Method::Post, "/fapi/v2/order");
        h.reply(create, REFUSED);
        assert_eq!(
            order_posts(&h),
            0,
            "{amount}/{trigger}: no resized, rounded or split restore"
        );
        let error = failure(&h);
        assert_eq!(error.protection, Some(Protection::Unprotected));
        assert!(error.message.contains("re-place"), "{error:?}");
    }
}
