//! Regressions for the Bitget-family port review in RUST_AUDITS.md.
mod support;

use safe_cex_engine::{Command, Event, Query};
use serde_json::{Value, json};
use support::*;

fn query(h: &mut Harness, query: Query) {
    h.command(Command::Query { command: 42, query });
}
fn result(h: &Harness) -> Value {
    let result = &h
        .done
        .iter()
        .find(|(id, _)| *id == 42)
        .expect("query completed")
        .1;
    serde_json::from_str(result.as_ref().expect("query succeeded")).unwrap()
}
fn fills() -> Query {
    Query::Fills {
        symbol: None,
        order_id: None,
        from_ms: None,
        to_ms: None,
        limit: None,
    }
}

#[test]
fn classic_forced_closes_keep_direction_and_default_absent_profit_to_zero() {
    for (trade, leg, side) in [
        ("burst_close_long", "long", "sell"),
        ("burst_close_short", "short", "buy"),
        ("reduce_close_long", "long", "sell"),
        ("dte_sys_adl_close_short", "short", "buy"),
    ] {
        let mut h = Harness::classic();
        query(&mut h, fills());
        h.respond(
            "/order/fills",
            200,
            &json!({"code":"00000","data":{"fillList":[{
                "tradeId":"fill", "orderId":"order", "symbol":"BTCUSDT", "price":"100",
                "baseVolume":"1", "side":"buy", "tradeSide":trade, "cTime":"1000"
            }]}})
            .to_string(),
        );
        let rows = result(&h);
        assert_eq!(rows[0]["positionSide"], leg, "{trade}");
        assert_eq!(rows[0]["side"], side, "{trade}");
        assert_eq!(rows[0]["realizedPnl"], 0.0);
        assert_eq!(rows[0]["commission"], 0.0);
    }
}

#[test]
fn classic_forced_close_order_history_marks_reduce_only() {
    for (trade, leg, side) in [
        ("burst_close_long", "long", "sell"),
        ("dte_sys_adl_close_short", "short", "buy"),
    ] {
        let mut h = Harness::classic();
        query(
            &mut h,
            Query::OrderHistory {
                symbol: None,
                from_ms: None,
                to_ms: None,
                limit: None,
            },
        );
        h.respond("/orders-history", 200, &json!({"code":"00000","data":{"entrustedList":[{
            "orderId":"order", "symbol":"BTCUSDT", "size":"1", "baseVolume":"1",
            "side":"buy", "tradeSide":trade, "orderType":"market", "status":"filled", "cTime":"1000"
        }]}}).to_string());
        let rows = result(&h);
        assert_eq!(rows[0]["positionSide"], leg);
        assert_eq!(rows[0]["side"], side);
        assert_eq!(rows[0]["reduceOnly"], true);
    }
}

#[test]
fn uta_oneway_fill_retains_inferred_or_explicit_leg() {
    for (side, trade, explicit, expected) in [
        ("buy", "open", "", "long"),
        ("sell", "open", "", "short"),
        ("buy", "close", "", "short"),
        ("sell", "close", "", "long"),
        ("buy", "", "short", "short"),
    ] {
        let mut h = Harness::private_started();
        h.respond(
            "/account/settings",
            200,
            r#"{"code":"00000","data":{"holdMode":"one_way_mode","symbolConfigList":[]}}"#,
        );
        query(&mut h, fills());
        h.respond(
            "/api/v3/trade/fills",
            200,
            &json!({"code":"00000","data":{"list":[{
                "execId":"fill", "orderId":"order", "symbol":"BTCUSDT", "execPrice":"100",
                "execQty":"1", "side":side, "tradeSide":trade, "posSide":explicit,
                "holdMode":"one_way_mode", "createdTime":"1000"
            }]}})
            .to_string(),
        );
        let rows = result(&h);
        assert_eq!(rows[0]["positionSide"], expected);
        assert_eq!(rows[0]["side"], side);
    }
}

#[test]
fn mixed_unmapped_owned_rest_rows_preserve_the_whole_position_and_protection_scope() {
    use safe_cex_engine::model::OrderType;

    for unknown_first in [true, false] {
        let mut h = Harness::classic();
        let previous_positions = h.engine.store().positions().to_vec();
        let mut previous_orders = h.engine.store().orders().to_vec();
        for (id, kind, trigger) in [
            ("prior-sl", OrderType::StopLoss, 80.0),
            ("prior-tp", OrderType::TakeProfit, 120.0),
        ] {
            let mut protection = previous_orders[0].clone();
            protection.id = id.into();
            protection.kind = kind;
            protection.reduce_only = true;
            protection.price = trigger;
            previous_orders.push(protection);
        }
        h.engine.store_mut().set_orders(previous_orders.clone());
        let positions: Value = serde_json::from_str(POSITION).unwrap();
        let mut unknown = positions["data"][0].clone();
        unknown["symbol"] = json!("UNKNOWNUSDT");
        unknown["total"] = json!("4");
        let mut valid = positions["data"][0].clone();
        valid["total"] = json!("3");
        let rows = if unknown_first {
            vec![unknown, valid]
        } else {
            vec![valid, unknown]
        };
        h.respond(
            "/all-position",
            200,
            &json!({"code":"00000","data":rows}).to_string(),
        );
        assert_eq!(h.engine.store().positions(), previous_positions);
        // A valid normal source cannot commit before the later protection
        // source proves complete; a missing catalogue peer is not a flat leg.
        let orders: Value = serde_json::from_str(OPEN_ORDER).unwrap();
        let mut normal = orders["data"]["entrustedList"][0].clone();
        normal["orderId"] = json!("valid-new");
        h.respond(
            "/orders-pending",
            200,
            &json!({"code":"00000","data":{"entrustedList":[normal]}}).to_string(),
        );
        let known = json!({"orderId":"valid-new-sl","symbol":"BTCUSDT","size":"1",
            "triggerPrice":"80","holdSide":"long","planType":"loss_plan","status":"live"});
        let mut unknown = known.clone();
        unknown["orderId"] = json!("unknown-owned-sl");
        unknown["symbol"] = json!("UNKNOWNUSDT");
        let rows = if unknown_first {
            vec![unknown, known]
        } else {
            vec![known, unknown]
        };
        h.respond(
            "planType=profit_loss",
            200,
            &json!({"code":"00000","data":{"entrustedList":rows}}).to_string(),
        );
        assert_eq!(h.engine.store().orders(), previous_orders);
        assert!(h.engine.store().order("valid-new").is_none());
        assert!(h.engine.store().order("valid-new-sl").is_none());
        assert!(
            h.logs
                .iter()
                .any(|log| log.contains("skipped Bitget position record"))
        );
        assert!(
            h.logs
                .iter()
                .any(|log| log.contains("skipped Bitget order record"))
        );
    }
}

#[test]
fn unmappable_ws_position_row_does_not_discard_valid_snapshot_row() {
    let mut h = Harness::classic();
    h.private_message(r#"{"arg":{"channel":"positions"},"action":"snapshot","data":[{"symbol":"UNKNOWNUSDT","total":"1"},{"symbol":"BTCUSDT","holdSide":"short","total":"2","openPriceAvg":"100","posMode":"one_way_mode"}]}"#);
    assert_eq!(h.engine.store().positions().len(), 1);
    assert_eq!(h.engine.store().positions()[0].contracts, 2.0);
    assert_eq!(
        h.engine.store().positions()[0].side,
        safe_cex_engine::model::PositionSide::Short
    );
    assert!(
        h.logs
            .iter()
            .any(|log| log.contains("skipped Bitget position stream record"))
    );
}

#[test]
fn ws_order_refresh_retries_after_busy_classic_snapshot() {
    let mut h = Harness::classic();
    h.private_message(r#"{"arg":{"channel":"orders"},"data":[{"orderId":"1","symbol":"BTCUSDT","size":"2","price":"90","side":"buy","orderType":"limit","status":"live"}]}"#);
    h.timer_after(500.0);
    assert_eq!(
        h.requests
            .iter()
            .filter(|r| r.url.contains("/orders-pending"))
            .count(),
        1
    );
    h.orders(OPEN_ORDER);
    h.timer_after(100.0);
    assert_eq!(
        h.requests
            .iter()
            .filter(|r| r.url.contains("/orders-pending"))
            .count(),
        1
    );
}

#[test]
fn ws_position_and_balance_refreshes_retry_while_prior_reads_are_busy() {
    let mut h = Harness::classic();
    h.private_message(r#"{"arg":{"channel":"positions"},"action":"update","data":[{"symbol":"BTCUSDT","total":"2","openPriceAvg":"100"}]}"#);
    let mut due: Vec<_> = h
        .timers
        .iter()
        .copied()
        .filter(|(_, at)| *at == h.now + 300.0 || *at == h.now + 500.0)
        .collect();
    due.sort_by(|a, b| a.1.total_cmp(&b.1));
    for (timer, at) in due {
        h.timers.retain(|(other, _)| *other != timer);
        h.now = at;
        h.event(Event::TimerFired { timer });
    }
    h.respond("/all-position", 200, POSITION);
    h.respond("/account/accounts", 200, ACCOUNT);
    let mut retries: Vec<_> = h
        .timers
        .iter()
        .copied()
        .filter(|(_, at)| *at <= h.now + 100.0)
        .collect();
    retries.sort_by(|a, b| a.1.total_cmp(&b.1));
    for (timer, at) in retries {
        h.timers.retain(|(other, _)| *other != timer);
        h.now = h.now.max(at);
        h.event(Event::TimerFired { timer });
    }
    assert!(h.requests.iter().any(|r| r.url.contains("/all-position")));
    assert!(
        h.requests
            .iter()
            .any(|r| r.url.contains("/account/accounts"))
    );
}

#[test]
fn classic_hedged_position_snapshot_populates_each_leg_setting() {
    let mut h = Harness::classic();
    h.respond("/all-position", 200, r#"{"code":"00000","data":[{"symbol":"BTCUSDT","holdSide":"long","total":"1","leverage":"10","marginMode":"isolated","posMode":"hedge_mode"},{"symbol":"BTCUSDT","holdSide":"short","total":"1","leverage":"11","marginMode":"isolated","posMode":"hedge_mode"}]}"#);
    let setting = h.engine.store().setting("BTCUSDT").unwrap();
    assert_eq!(setting.buy_leverage, Some(10.0));
    assert_eq!(setting.sell_leverage, Some(11.0));
    h.private_message(r#"{"arg":{"channel":"positions"},"action":"update","data":[{"symbol":"BTCUSDT","holdSide":"long","total":"1","leverage":"12","marginMode":"isolated","posMode":"hedge_mode"}]}"#);
    let setting = h.engine.store().setting("BTCUSDT").unwrap();
    assert_eq!(setting.buy_leverage, Some(12.0));
    assert_eq!(setting.sell_leverage, Some(11.0));
}

#[test]
fn uta_settings_populate_both_legs_and_scalar_refresh_preserves_known_leg_values() {
    use safe_cex_engine::model::PositionSide;
    let mut h = Harness::private_started();
    h.respond("/account/settings", 200, r#"{"code":"00000","data":{"holdMode":"hedge_mode","symbolConfigList":[{"symbol":"BTCUSDT","category":"USDT-FUTURES","marginMode":"isolated","longLeverage":"10","shortLeverage":"11"}]}}"#);
    let setting = h.engine.store().setting("BTCUSDT").unwrap();
    assert_eq!(
        (setting.buy_leverage, setting.sell_leverage),
        (Some(10.0), Some(11.0))
    );
    // No positions are available, so trading requests a fresh typed settings row.
    h.command(Command::SetLeverage {
        command: 90,
        symbol: "BTCUSDT".into(),
        leverage: 25.0,
        side: Some(PositionSide::Short),
    });
    h.respond("/account/settings", 200, r#"{"code":"00000","data":{"holdMode":"hedge_mode","symbolConfigList":[{"symbol":"BTCUSDT","category":"USDT-FUTURES","marginMode":"isolated","leverage":"3"}]}}"#);
    let setting = h.engine.store().setting("BTCUSDT").unwrap();
    assert_eq!(
        (setting.buy_leverage, setting.sell_leverage),
        (Some(10.0), Some(11.0))
    );
    h.respond("/set-leverage", 200, r#"{"code":"00000","data":{}}"#);
    let setting = h.engine.store().setting("BTCUSDT").unwrap();
    assert_eq!(
        (setting.buy_leverage, setting.sell_leverage),
        (Some(10.0), Some(25.0))
    );
}

#[test]
fn stale_rest_after_side_write_uses_matching_leg_setting_even_without_position() {
    use safe_cex_engine::model::{MarginType, PositionSide};
    let mut h = Harness::classic();
    let old = h.take("/all-position");
    h.engine.store_mut().set_hedged(true);
    h.engine.store_mut().update_setting("BTCUSDT", |s| {
        s.margin_type = Some(MarginType::Isolated);
        s.leverage = 20.0;
        s.buy_leverage = Some(20.0);
        s.sell_leverage = Some(11.0);
    });
    h.now += 1.0;
    h.command(Command::SetLeverage {
        command: 91,
        symbol: "BTCUSDT".into(),
        leverage: 25.0,
        side: Some(PositionSide::Short),
    });
    h.respond("/set-leverage", 200, r#"{"code":"00000","data":{}}"#);
    // The other leg's stream update must not replace the written short setting.
    h.now += 1.0;
    h.private_message(r#"{"arg":{"channel":"positions"},"action":"update","data":[{"symbol":"BTCUSDT","holdSide":"long","total":"1","leverage":"22","marginMode":"isolated","posMode":"hedge_mode"}]}"#);
    h.reply(old.id, 200, r#"{"code":"00000","data":[{"symbol":"BTCUSDT","holdSide":"short","total":"1","leverage":"11","marginMode":"isolated","posMode":"hedge_mode"}]}"#);
    let short = h
        .engine
        .store()
        .positions()
        .iter()
        .find(|p| p.side == PositionSide::Short)
        .unwrap();
    assert_eq!(short.leverage, 25.0);
    let setting = h.engine.store().setting("BTCUSDT").unwrap();
    assert_eq!(
        (setting.buy_leverage, setting.sell_leverage),
        (Some(22.0), Some(25.0))
    );
}

#[test]
fn entirely_unmappable_snapshots_preserve_state_but_successful_empty_lists_clear_it() {
    let mut h = Harness::classic();
    h.respond(
        "/all-position",
        200,
        r#"{"code":"00000","data":[{"symbol":"BTCUSDT","total":"-1"}]}"#,
    );
    h.respond("/orders-pending", 200, r#"{"code":"00000","data":{"entrustedList":[{"orderId":"bad","symbol":"BTCUSDT","size":"2","price":"90","side":"buy","orderType":"new_unsupported_type","status":"live"}]}}"#);
    assert_eq!(h.engine.store().positions().len(), 1);
    assert!(h.engine.store().order("1").is_some());
    h.private_message(r#"{"arg":{"channel":"positions"},"action":"snapshot","data":[{"symbol":"BTCUSDT","total":"-1"}]}"#);
    assert_eq!(h.engine.store().positions().len(), 1);
    h.private_message(r#"{"arg":{"channel":"positions"},"action":"snapshot","data":[]}"#);
    assert!(h.engine.store().positions().is_empty());
    let mut h = Harness::classic();
    h.respond("/all-position", 200, r#"{"code":"00000","data":[]}"#);
    h.orders(r#"{"code":"00000","data":{"entrustedList":[]}}"#);
    assert!(h.engine.store().positions().is_empty());
    assert!(h.engine.store().orders().is_empty());
}
