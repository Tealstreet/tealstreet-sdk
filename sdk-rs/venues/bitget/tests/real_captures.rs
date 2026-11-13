//! Bodies captured from real Classic and UTA keys; symbols moved onto the fixture market.
mod support;

use safe_cex_engine::Command;
use safe_cex_engine::model::*;
use serde_json::Value;
use support::{ACCOUNT, Harness, POSITION};

const NULL_ENTRUSTED: &str = r#"{"code":"00000","msg":"success","requestTime":1793543514194,"data":{"entrustedList":null,"endId":null}}"#;
const NOT_UTA: &str = r#"{"code":"40084","msg":"You are in Classic Account mode, and the Unified Account API is not supported at this time","data":null}"#;
const CLASSIC_STOP: &str = r#"{"planType":"pos_loss","symbol":"BTCUSDT","size":"0","orderId":"1500798416221888512","price":"","executePrice":"","callbackRatio":"","triggerPrice":"1.1271","triggerType":"mark_price","planStatus":"live","side":"sell","posSide":"net","marginCoin":"USDT","marginMode":"crossed","tradeSide":"sell_single","posMode":"one_way_mode","orderType":"market","stopSurplusTriggerPrice":null}"#;
const UTA_CLOSE: &str = r#"{"orderId":"1500796226646327296","clientOid":"d1dd3e13-85da-458c-a349-bf49ce4452a6","category":"USDT-FUTURES","symbol":"BTCUSDT","orderType":"limit","side":"sell","price":"0.32636","qty":"18","amount":"0","cumExecQty":"0","cumExecValue":"0","avgPrice":"0","timeInForce":"gtc","orderStatus":"new","posSide":"long","holdMode":"hedge_mode","reduceOnly":"NO","feeDetail":[{"feeCoin":"","fee":""}],"createdTime":"1793543624555","updatedTime":"1793543624556","stpMode":"none","tpTriggerBy":null,"slTriggerBy":null}"#;

fn uta(hedged: bool, positions: &str, orders: &str) -> Harness {
    let mut h = Harness::private_started();
    let settings = format!(
        r#"{{"code":"00000","data":{{"holdMode":"{}"}}}}"#,
        if hedged { "hedge_mode" } else { "one_way_mode" }
    );
    h.respond("/account/settings", 200, &settings);
    h.respond("/current-position", 200, positions);
    h.respond(
        "/account/assets",
        200,
        r#"{"code":"00000","data":{"accountEquity":"1000","effEquity":"1000","unrealisedPnl":"0"}}"#,
    );
    h.respond("/unfilled-orders", 200, orders);
    while h
        .requests
        .iter()
        .any(|r| r.url.contains("/unfilled-strategy-orders"))
    {
        h.respond(
            "/unfilled-strategy-orders",
            200,
            r#"{"code":"00000","data":{"list":null}}"#,
        );
    }
    h.login();
    // The login's own catch-up reads.
    for (path, reply) in [
        ("/current-position", positions),
        ("/unfilled-orders", orders),
        (
            "/unfilled-strategy-orders",
            r#"{"code":"00000","data":{"list":null}}"#,
        ),
        (
            "/account/assets",
            r#"{"code":"00000","data":{"accountEquity":"1000","effEquity":"1000","unrealisedPnl":"0"}}"#,
        ),
    ] {
        while h.requests.iter().any(|r| r.url.contains(path)) {
            h.respond(path, 200, reply);
        }
    }
    h.engine.store_mut().set_hedged(hedged);
    h.engine.store_mut().update_setting("BTCUSDT", |s| {
        s.leverage = 10.0;
        s.margin_type = Some(MarginType::Cross);
    });
    h
}
const NO_POSITIONS: &str = r#"{"code":"00000","data":{"list":[]}}"#;
const NO_ORDERS: &str = r#"{"code":"00000","data":{"list":[]}}"#;

fn settle_settings(h: &mut Harness, hedged: bool) {
    while h
        .requests
        .iter()
        .any(|r| r.url.contains("/account/settings"))
    {
        let settings = format!(
            r#"{{"code":"00000","data":{{"holdMode":"{}"}}}}"#,
            if hedged { "hedge_mode" } else { "one_way_mode" }
        );
        h.respond("/account/settings", 200, &settings);
    }
}

fn body(request: &safe_cex_engine::HttpRequest) -> Value {
    serde_json::from_str(request.body.as_deref().unwrap()).unwrap()
}

// (1) A null list is an empty one: the chain reaches its stops and commits.
#[test]
fn classic_null_lists_commit_and_the_stop_shows() {
    let mut h = Harness::private_started();
    h.respond("/account/settings", 200, NOT_UTA);
    h.respond("/all-position", 200, POSITION);
    h.respond("/account/accounts", 200, ACCOUNT);
    h.respond("/orders-pending", 200, NULL_ENTRUSTED);
    h.respond(
        "planType=profit_loss",
        200,
        &format!(r#"{{"code":"00000","msg":"success","data":{{"entrustedList":[{CLASSIC_STOP}],"endId":"1500798416221888512"}}}}"#),
    );
    while h
        .requests
        .iter()
        .any(|r| r.url.contains("planType=profit_loss"))
    {
        h.respond("planType=profit_loss", 200, NULL_ENTRUSTED);
    }
    h.respond("planType=track_plan", 200, NULL_ENTRUSTED);
    assert!(h.engine.store().loaded().orders);
    assert!(h.engine.store().order("1500798416221888512").is_some());
}

#[test]
fn classic_market_stop_with_blank_execution_can_be_modified_as_market() {
    let mut h = Harness::private_started();
    h.respond("/account/settings", 200, NOT_UTA);
    h.respond("/all-position", 200, POSITION);
    h.respond("/account/accounts", 200, ACCOUNT);
    h.respond("/orders-pending", 200, NULL_ENTRUSTED);
    h.respond(
        "planType=profit_loss",
        200,
        &format!(r#"{{"code":"00000","data":{{"entrustedList":[{CLASSIC_STOP}],"endId":""}}}}"#),
    );
    h.respond("planType=track_plan", 200, NULL_ENTRUSTED);
    h.engine.store_mut().update_ticker("BTCUSDT", |t| {
        t.last = 1.2;
        t.mark = 1.2;
    });
    h.command(Command::AmendOrder {
        command: 71,
        id: "1500798416221888512".into(),
        price: Some(1.1),
        amount: None,
    });
    assert!(h.failures.is_empty(), "{:?}", h.failures);
    let request = h.take("/modify-tpsl-order");
    let payload = body(&request);
    assert_eq!(payload["executePrice"], "0");
    assert_eq!(payload["triggerPrice"], "1.1");
    assert_eq!(payload["triggerType"], "mark_price");
    assert!(!h.requests.iter().any(|r| r.url.contains("cancel")));
}

// (2) and (5): the row is read, and its amend closes the long instead of opening a short.
#[test]
fn uta_hedge_close_with_nulls_is_kept_and_its_amend_stays_a_close() {
    let orders = format!(r#"{{"code":"00000","data":{{"list":[{UTA_CLOSE}],"cursor":""}}}}"#);
    let mut h = uta(true, NO_POSITIONS, &orders);
    let id = "1500796226646327296";
    let order = h.engine.store().order(id).expect("resting order shows");
    assert!(order.reduce_only);
    assert_eq!(order.side, OrderSide::Sell);
    h.command(Command::AmendOrder {
        command: 7,
        id: id.into(),
        price: Some(90.0),
        amount: None,
    });
    settle_settings(&mut h, true);
    h.respond(
        "/trade/cancel-order",
        200,
        &format!(r#"{{"code":"00000","data":{{"orderId":"{id}"}}}}"#),
    );
    settle_settings(&mut h, true);
    let placed = body(&h.take("/trade/place-order"));
    assert_eq!(placed["side"], "sell");
    assert_eq!(placed["posSide"], "long", "{placed}");
}

// (4) A modify's real success names no order; the detail read does.
#[test]
fn classic_modify_with_null_order_id_succeeds_and_moves_to_the_new_id() {
    let mut h = Harness::classic();
    h.command(Command::AmendOrder {
        command: 1,
        id: "1".into(),
        price: Some(95.0),
        amount: Some(3.0),
    });
    h.respond(
        "/modify-order",
        200,
        r#"{"code":"00000","msg":"success","requestTime":1793543637512,"data":{"orderId":null,"clientOid":null}}"#,
    );
    h.respond(
        "/order/detail",
        200,
        r#"{"code":"00000","data":{"orderId":"2","status":"live","baseVolume":"0"}}"#,
    );
    assert!(
        h.done.iter().any(|(id, result)| *id == 1 && result.is_ok()),
        "{:?}",
        h.done
    );
    assert!(h.engine.store().order("2").is_some());
}

// (6)
#[test]
fn uta_flat_null_position_list_loads() {
    let h = uta(
        false,
        r#"{"code":"00000","msg":"success","data":{"list":null}}"#,
        NO_ORDERS,
    );
    assert!(h.engine.store().loaded().positions);
}

// (7)
#[test]
fn a_success_with_text_data_is_a_success() {
    let mut h = uta(false, NO_POSITIONS, NO_ORDERS);
    h.command(Command::SetPositionMode {
        command: 3,
        hedged: true,
        symbol: None,
    });
    h.respond(
        "/set-hold-mode",
        200,
        r#"{"code":"00000","msg":"success","requestTime":1793543637512,"data":"success"}"#,
    );
    assert!(
        h.done.iter().any(|(id, result)| *id == 3 && result.is_ok()),
        "{:?}",
        h.done
    );
    assert!(h.engine.store().hedged());
}

fn stop(side: &str) -> PlaceOrderOpts {
    serde_json::from_str(&format!(
        r#"{{"symbol":"BTCUSDT","type":"stop_market","side":"{side}","amount":0.01,"price":90}}"#
    ))
    .unwrap()
}

// (8) and (12)
#[test]
fn uta_one_way_stop_sends_pos_side_and_its_null_cancel_succeeds() {
    let mut h = uta(false, NO_POSITIONS, NO_ORDERS);
    h.command(Command::PlaceOrders {
        command: 4,
        orders: vec![stop("sell")],
    });
    settle_settings(&mut h, false);
    h.respond("/current-position", 200, NO_POSITIONS);
    settle_settings(&mut h, false);
    let request = h.take("/place-strategy-order");
    assert_eq!(body(&request)["posSide"], "long");
    h.reply(
        request.id,
        200,
        r#"{"code":"00000","data":{"orderId":"55"}}"#,
    );
    h.command(Command::CancelOrders {
        command: 5,
        ids: vec!["55".into()],
    });
    settle_settings(&mut h, false);
    h.respond(
        "/cancel-strategy-order",
        200,
        r#"{"code":"00000","msg":"success","requestTime":1793543630443,"data":null}"#,
    );
    assert!(
        h.done.iter().any(|(id, result)| *id == 5 && result.is_ok()),
        "{:?}",
        h.done
    );
}

// (9) One-way, the strategy row's side says what it closes.
#[test]
fn uta_one_way_strategy_row_keeps_its_own_side() {
    let mut h = uta(false, NO_POSITIONS, NO_ORDERS);
    h.private_message(
        r#"{"action":"snapshot","arg":{"instType":"UTA","topic":"strategy-order","symbol":"default"},"data":[{"category":"usdt-futures","symbol":"BTCUSDT","orderId":"66","qty":"0.01","side":"sell","posSide":"short","reduceOnly":"no","type":"tpsl","status":"pending","slTriggerBy":"market","stopLoss":"90000","slOrderType":"market","createdTime":"1730186725663","updatedTime":"1730186725691"}],"ts":1730186725700}"#,
    );
    assert_eq!(h.engine.store().order("66").unwrap().side, OrderSide::Sell);
}

// (10) A one-way or cross UTA key gets 40085 from v2.
#[test]
fn uta_cross_leverage_goes_to_v3_with_its_coin() {
    let mut h = uta(false, NO_POSITIONS, NO_ORDERS);
    h.command(Command::SetLeverage {
        command: 6,
        symbol: "BTCUSDT".into(),
        leverage: 5.0,
        side: None,
    });
    settle_settings(&mut h, false);
    let request = h.take("/api/v3/account/set-leverage");
    assert_eq!(body(&request)["coin"], "USDT");
}

// (13) An empty success from a cancel-plan proves nothing: the stop may be live.
#[test]
fn classic_cancel_plan_empty_success_does_not_mark_canceled() {
    let mut h = Harness::classic();
    let mut order = h.engine.store().order("1").unwrap().clone();
    order.id = "77".into();
    order.kind = OrderType::StopLoss;
    h.engine.store_mut().upsert_order(order);
    h.command(Command::CancelOrders {
        command: 8,
        ids: vec!["77".into()],
    });
    h.respond(
        "/cancel-plan-order",
        200,
        r#"{"code":"00000","data":{"successList":[],"failureList":[]}}"#,
    );
    assert_ne!(
        h.engine.store().order("77").map(|o| o.status),
        Some(OrderStatus::Canceled)
    );
    // The confirming read still lists it.
    h.respond(
        "orderId=77",
        200,
        r#"{"code":"00000","data":{"entrustedList":[{"orderId":"77","symbol":"BTCUSDT","planType":"loss_plan","planStatus":"live"}],"endId":"77"}}"#,
    );
    assert_ne!(
        h.engine.store().order("77").map(|o| o.status),
        Some(OrderStatus::Canceled)
    );
    assert!(
        h.done
            .iter()
            .any(|(id, result)| *id == 8 && result.is_err()),
        "{:?}",
        h.done
    );
}

// (15)
#[test]
fn a_post_only_cancel_reason_is_read() {
    let mut h = Harness::classic();
    h.private_message(
        r#"{"arg":{"channel":"orders"},"data":[{"orderId":"1","symbol":"BTCUSDT","size":"2","baseVolume":"0","price":"90","side":"buy","orderType":"limit","status":"canceled","cancelReason":"post_only_cancel"}]}"#,
    );
    let order = h.engine.store().order("1").unwrap();
    assert_eq!(
        order.extra.as_ref().and_then(|extra| extra.reject_reason),
        Some(RejectReason::PostOnly)
    );
}

fn edit_bracket_of_limit_filled(filled: f64, uta_mode: bool) -> String {
    let mut h = Harness::classic();
    let mut limit = h.engine.store().order("1").unwrap().clone();
    if uta_mode {
        let positions = POSITION
            .replace("\"data\":[", "\"data\":{\"list\":[")
            .replace("]}", "]}}");
        h = uta(false, &positions, NO_ORDERS);
    }
    limit.filled = filled;
    let mut stop = limit.clone();
    stop.id = "1__stop_loss".into();
    stop.parent_id = Some("1".into());
    stop.kind = OrderType::StopLoss;
    stop.side = limit.side.opposite();
    stop.price = 80.0;
    stop.extra
        .get_or_insert_with(OrderExtra::default)
        .sl_trigger_type = Some(StopTriggerType::LastPrice);
    h.engine.store_mut().upsert_order(limit);
    h.engine.store_mut().upsert_order(stop);
    h.command(Command::AmendOrder {
        command: 9,
        id: "1__stop_loss".into(),
        price: Some(70.0),
        amount: None,
    });
    if !uta_mode && filled > 0.0 {
        assert!(
            !h.requests
                .iter()
                .any(|r| r.method == safe_cex_engine::Method::Post)
        );
        assert_eq!(
            h.engine.store().order("1").unwrap().status,
            OrderStatus::Open
        );
    }
    for _ in 0..8 {
        if h.done.iter().any(|(id, _)| *id == 9) {
            break;
        }
        let Some(request) = h
            .requests
            .iter()
            .find(|r| r.method == safe_cex_engine::Method::Post || r.url.contains("/position/"))
            .map(|r| (r.id, r.url.clone(), r.body.clone()))
        else {
            break;
        };
        let body = if request.1.contains("cancel") {
            r#"{"code":"00000","data":{"successList":[{"orderId":"1"}],"failureList":[],"orderId":"1"}}"#
        } else if request.1.contains("place") || request.1.contains("modify-order") {
            r#"{"code":"40808","msg":"refused","data":null}"#
        } else {
            r#"{"code":"00000","data":[]}"#
        };
        h.requests.retain(|r| r.id != request.0);
        h.reply(request.0, 200, body);
    }
    h.done
        .iter()
        .find(|(id, _)| *id == 9)
        .expect("the edit ended")
        .1
        .clone()
        .unwrap_err()
}

#[test]
fn a_partly_filled_uta_limit_losing_its_bracket_says_the_position_is_unprotected() {
    let error = edit_bracket_of_limit_filled(0.5, true);
    assert!(error.contains("position has no stop"), "{error}");
}

#[test]
fn an_unfilled_limit_losing_its_bracket_does_not() {
    let error = edit_bracket_of_limit_filled(0.0, false);
    assert!(!error.contains("has no stop"), "{error}");
}

#[test]
fn a_partly_filled_classic_preset_refuses_replacement_before_cancel() {
    let error = edit_bracket_of_limit_filled(0.5, false);
    assert!(
        error.contains("Classic preset default is unproven"),
        "{error}"
    );
    assert!(!error.contains("has no stop"), "{error}");
}

#[test]
fn a_price_amend_of_a_partly_filled_uta_order_places_what_is_left() {
    let row = UTA_CLOSE
        .replace(r#""cumExecQty":"0""#, r#""cumExecQty":"8""#)
        .replace(
            r#""orderStatus":"new""#,
            r#""orderStatus":"partially_filled""#,
        );
    let orders = format!(r#"{{"code":"00000","data":{{"list":[{row}],"cursor":""}}}}"#);
    let mut h = uta(true, NO_POSITIONS, &orders);
    let id = "1500796226646327296";
    assert_eq!(h.engine.store().order(id).unwrap().filled, 8.0);
    h.command(Command::AmendOrder {
        command: 7,
        id: id.into(),
        price: Some(90.0),
        amount: None,
    });
    settle_settings(&mut h, true);
    h.respond(
        "/trade/cancel-order",
        200,
        &format!(r#"{{"code":"00000","data":{{"orderId":"{id}"}}}}"#),
    );
    settle_settings(&mut h, true);
    let placed = body(&h.take("/trade/place-order"));
    assert_eq!(placed["qty"], "10", "{placed}");
}
