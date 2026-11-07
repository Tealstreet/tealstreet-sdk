mod support;

use safe_cex_engine::model::{MarginType, OrderSide, OrderStatus, PositionMode, PositionSide};
use safe_cex_engine::{Command, Event};
use serde_json::{Value, json};
use support::*;

#[test]
fn hedge_close_cancel_replace_preserves_close_intent_for_both_sides_and_aliases() {
    for (venue_side, execution_side, suffix) in [
        ("buy", OrderSide::Sell, "long"),
        ("sell", OrderSide::Buy, "short"),
    ] {
        for prefix in [
            "",
            "close_",
            "reduce_close_",
            "offset_close_",
            "burst_close_",
            "delivery_close_",
        ] {
            let trade_side = if prefix.is_empty() {
                "close".to_owned()
            } else {
                format!("{prefix}{suffix}")
            };
            for reduce_only in [None, Some("NO")] {
                let mut h = Harness::classic();
                h.engine.store_mut().set_hedged(true);
                let mut row = json!({
                    "orderId": "review-close", "symbol": "BTCUSDT", "size": "2",
                    "baseVolume": "0", "price": "110", "side": venue_side,
                    "tradeSide": trade_side, "posMode": "hedge_mode",
                    "orderType": "limit", "status": "live"
                });
                if let Some(reduce_only) = reduce_only {
                    row["reduceOnly"] = json!(reduce_only);
                }
                h.private_message(&json!({"arg":{"channel":"orders"},"data":[row]}).to_string());
                let order = h.engine.store().order("review-close").unwrap();
                assert_eq!(order.side, execution_side, "{trade_side}, {reduce_only:?}");
                assert!(order.reduce_only, "{trade_side}, {reduce_only:?}");

                h.command(Command::AmendOrder {
                    command: 1,
                    id: "review-close".into(),
                    price: Some(111.0),
                    amount: None,
                });
                h.respond(
                    "/modify-order",
                    200,
                    r#"{"code":"40017","msg":"cannot modify","data":null}"#,
                );
                h.respond("/batch-cancel-orders", 200, r#"{"code":"00000","data":{"successList":[{"orderId":"review-close"}],"failureList":[]}}"#);
                let request = h.take("/batch-place-order");
                let body: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
                let replacement = &body["orderList"][0];
                assert_eq!(
                    replacement["tradeSide"], "close",
                    "{trade_side}, {reduce_only:?}"
                );
                assert_eq!(
                    replacement["side"], venue_side,
                    "{trade_side}, {reduce_only:?}"
                );
            }
        }
    }
}

#[test]
fn full_empty_snapshot_preserves_ws_tpsl_omitted_by_bitget() {
    let mut h = Harness::classic();
    // Finish the socket-open reconciliation before introducing the stop, so
    // the tested snapshot really starts after the most recent private event.
    h.orders(OPEN_ORDER);
    h.private_message(r#"{"arg":{"channel":"orders-algo"},"data":[{"orderId":"review-stop","symbol":"BTCUSDT","size":"1","triggerPrice":"90","holdSide":"long","planType":"loss_plan","status":"live"}]}"#);
    assert_eq!(
        h.engine.store().order("review-stop").unwrap().status,
        OrderStatus::Open
    );
    let timer = h
        .timers
        .iter()
        .find(|(_, at)| (*at - h.now - 500.0).abs() < 0.01)
        .unwrap()
        .0;
    h.now += 6_000.0;
    h.timers.retain(|(id, _)| *id != timer);
    h.event(Event::TimerFired { timer });
    h.orders(r#"{"code":"00000","data":{"entrustedList":[]}}"#);
    // Bitget's orders-plan-pending can omit valid place-tpsl-order stops.
    // See the venue CLAUDE.md, Order Reconciliation, algo preservation rule.
    assert_eq!(
        h.engine.store().order("review-stop").unwrap().status,
        OrderStatus::Open,
        "REST omission must not remove a WS TPSL without terminal proof"
    );
}

#[test]
fn older_rest_positions_preserve_side_specific_isolated_hedge_leverage() {
    for uta_account in [false, true] {
        let mut h = if uta_account {
            let mut h = Harness::private_started();
            h.respond(
                "/account/settings",
                200,
                r#"{"code":"00000","data":{"holdMode":"hedge_mode"}}"#,
            );
            h.respond(
                "/current-position",
                200,
                &POSITION
                    .replace("\"data\":[", "\"data\":{\"list\":[")
                    .replace("]}", "]}}"),
            );
            h.respond("/account/assets", 200, r#"{"code":"00000","data":{"accountEquity":"1000","effEquity":"900","unrealisedPnl":"0"}}"#);
            h.respond(
                "/unfilled-orders",
                200,
                r#"{"code":"00000","data":{"list":[]}}"#,
            );
            h.respond(
                "/unfilled-strategy-orders",
                200,
                r#"{"code":"00000","data":[]}"#,
            );
            h.respond("type=trailing_stop", 200, r#"{"code":"00000","data":[]}"#);
            h.login();
            h
        } else {
            Harness::classic()
        };
        h.engine.store_mut().set_hedged(true);
        h.engine.store_mut().update_setting("BTCUSDT", |setting| {
            // This fixture replaces the startup 20x one-way positions with
            // two isolated 10x legs; seed their authoritative settings too.
            setting.leverage = 10.0;
            setting.buy_leverage = Some(10.0);
            setting.sell_leverage = Some(10.0);
            setting.margin_type = Some(MarginType::Isolated);
        });
        let mut long = h.engine.store().positions()[0].clone();
        long.id = "review-long".into();
        long.side = PositionSide::Long;
        long.position_mode = Some(PositionMode::Hedged);
        long.margin_type = Some(MarginType::Isolated);
        long.leverage = 10.0;
        let mut short = long.clone();
        short.id = "review-short".into();
        short.side = PositionSide::Short;
        h.engine.store_mut().set_positions(vec![long, short]);
        // The socket-open read predates the long-only write acknowledgment.
        let old_read = h.take(if uta_account {
            "/current-position"
        } else {
            "/all-position"
        });
        h.now += 10.0;
        h.command(Command::SetLeverage {
            command: 1,
            symbol: "BTCUSDT".into(),
            leverage: 20.0,
            side: None,
        });
        let write = h.take("/set-leverage");
        let body: Value = serde_json::from_str(write.body.as_deref().unwrap()).unwrap();
        assert_eq!(
            body[if uta_account { "posSide" } else { "holdSide" }],
            "long"
        );
        h.reply(write.id, 200, r#"{"code":"00000","data":{}}"#);
        let rows = ["long", "short"].map(|side| {
            json!({
                "symbol": "BTCUSDT", "holdSide": side, "total": "1",
                "openPriceAvg": "90", "markPrice": "100", "leverage": "10",
                "marginMode": "isolated", "posMode": "hedge_mode", "uTime": "100"
            })
        });
        let response = if uta_account {
            json!({"code":"00000","data":{"list":rows}})
        } else {
            json!({"code":"00000","data":rows})
        };
        h.now += 1.0;
        h.reply(old_read.id, 200, &response.to_string());
        for (side, expected) in [(PositionSide::Long, 20.0), (PositionSide::Short, 10.0)] {
            let position = h
                .engine
                .store()
                .positions()
                .iter()
                .find(|p| p.side == side)
                .unwrap();
            assert_eq!(
                position.leverage, expected,
                "UTA={uta_account}, side={side:?}"
            );
        }
    }
}
