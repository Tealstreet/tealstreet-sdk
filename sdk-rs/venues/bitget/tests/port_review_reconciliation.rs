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
fn unmappable_rest_position_and_order_rows_are_logged_without_discarding_valid_rows() {
    let mut h = Harness::classic();
    let positions: Value = serde_json::from_str(POSITION).unwrap();
    let mut unknown = positions["data"][0].clone();
    unknown["symbol"] = json!("UNKNOWNUSDT");
    let mut invalid_known = positions["data"][0].clone();
    invalid_known["total"] = json!("-1");
    let mut valid = positions["data"][0].clone();
    valid["total"] = json!("3");
    h.respond(
        "/all-position",
        200,
        &json!({"code":"00000","data":[unknown,invalid_known,valid]}).to_string(),
    );
    assert_eq!(h.engine.store().positions().len(), 1);
    assert_eq!(h.engine.store().positions()[0].contracts, 3.0);
    let orders: Value = serde_json::from_str(OPEN_ORDER).unwrap();
    let mut unknown = orders["data"]["entrustedList"][0].clone();
    unknown["symbol"] = json!("UNKNOWNUSDT");
    let mut invalid_known = orders["data"]["entrustedList"][0].clone();
    invalid_known["status"] = json!("future_unsupported_status");
    let mut valid = orders["data"]["entrustedList"][0].clone();
    valid["orderId"] = json!("valid-new");
    h.orders(
        &json!({"code":"00000","data":{"entrustedList":[unknown,invalid_known,valid]}}).to_string(),
    );
    assert!(h.engine.store().order("valid-new").is_some());
    // Order 1's own row could not map, so its stored copy is kept.
    assert_eq!(h.engine.store().order("1").unwrap().amount, 2.0);
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

#[test]
fn unmapped_owned_rest_rows_keep_their_own_position_and_protection_while_peers_update() {
    use safe_cex_engine::model::OrderType;

    for unknown_first in [true, false] {
        let mut h = Harness::classic();
        let mut unknown_position = h.engine.store().positions()[0].clone();
        unknown_position.id = unknown_position.id.replace("BTCUSDT", "UNKNOWNUSDT");
        unknown_position.symbol = "UNKNOWNUSDT".into();
        let mut positions = h.engine.store().positions().to_vec();
        positions.push(unknown_position.clone());
        h.engine.store_mut().set_positions(positions);
        let mut orders = h.engine.store().orders().to_vec();
        let mut unknown_orders = vec![];
        for (id, parent, kind) in [
            ("unknown-parent", None, OrderType::Limit),
            (
                "unknown-parent__stop_loss",
                Some("unknown-parent"),
                OrderType::StopLoss,
            ),
            (
                "unknown-parent__take_profit",
                Some("unknown-parent"),
                OrderType::TakeProfit,
            ),
            ("unknown-owned-sl", None, OrderType::StopLoss),
        ] {
            let mut order = orders[0].clone();
            order.id = id.into();
            order.parent_id = parent.map(Into::into);
            order.symbol = "UNKNOWNUSDT".into();
            order.kind = kind;
            order.reduce_only = kind != OrderType::Limit;
            unknown_orders.push(order);
        }
        orders.extend(unknown_orders.iter().cloned());
        h.engine.store_mut().set_orders(orders);

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
        let store = h.engine.store();
        assert_eq!(store.positions().len(), 2);
        assert_eq!(
            store.position("BTCUSDT:Long:Oneway").unwrap().contracts,
            3.0
        );
        assert_eq!(
            store.position(&unknown_position.id),
            Some(&unknown_position)
        );

        let open: Value = serde_json::from_str(OPEN_ORDER).unwrap();
        let mut normal = open["data"]["entrustedList"][0].clone();
        normal["orderId"] = json!("valid-new");
        let mut unknown_normal = normal.clone();
        unknown_normal["orderId"] = json!("unknown-parent");
        unknown_normal["symbol"] = json!("UNKNOWNUSDT");
        unknown_normal["presetStopSurplusPrice"] = json!("120");
        unknown_normal["presetStopLossPrice"] = json!("80");
        let known = json!({"orderId":"valid-new-sl","symbol":"BTCUSDT","size":"1",
            "triggerPrice":"80","holdSide":"long","planType":"loss_plan","status":"live"});
        let mut unknown = known.clone();
        unknown["orderId"] = json!("unknown-owned-sl");
        unknown["symbol"] = json!("UNKNOWNUSDT");
        let (normal_rows, plan_rows) = if unknown_first {
            (vec![unknown_normal, normal], vec![unknown, known])
        } else {
            (vec![normal, unknown_normal], vec![known, unknown])
        };
        h.respond(
            "/orders-pending",
            200,
            &json!({"code":"00000","data":{"entrustedList":normal_rows}}).to_string(),
        );
        h.no_partial_fills();
        h.respond(
            "planType=profit_loss",
            200,
            &json!({"code":"00000","data":{"entrustedList":plan_rows}}).to_string(),
        );
        h.respond("planType=track_plan", 200, r#"{"code":"00000","data":[]}"#);
        let store = h.engine.store();
        assert!(store.order("valid-new").is_some());
        assert!(store.order("valid-new-sl").is_some());
        // Order 1 is absent from an otherwise valid snapshot, so it is removed.
        assert!(store.order("1").is_none());
        for order in &unknown_orders {
            assert_eq!(store.order(&order.id), Some(order), "{}", order.id);
        }
        for expected in [
            "skipped Bitget position record UNKNOWNUSDT: invalid Bitget position symbol/size",
            "skipped Bitget order record unknown-parent: unknown Bitget order market",
            "skipped Bitget order record unknown-owned-sl: unknown Bitget order market",
        ] {
            assert!(h.logs.iter().any(|log| log == expected), "{expected}");
        }
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
fn unmappable_snapshot_rows_protect_only_their_identity_but_empty_lists_clear_it() {
    let mut h = Harness::classic();
    h.respond(
        "/all-position",
        200,
        r#"{"code":"00000","data":[{"symbol":"BTCUSDT","total":"-1"}]}"#,
    );
    h.orders(r#"{"code":"00000","data":{"entrustedList":[{"orderId":"bad","symbol":"BTCUSDT","size":"2","price":"90","side":"buy","orderType":"new_unsupported_type","status":"live"}]}}"#);
    assert_eq!(h.engine.store().positions().len(), 1);
    // The skipped row names only order "bad"; order 1 is absent from a valid list.
    assert!(h.engine.store().order("1").is_none());
    assert!(h.engine.store().order("bad").is_none());
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

#[test]
fn skipped_rest_rows_keep_their_stored_copy_until_a_valid_snapshot_omits_them() {
    let mut h = Harness::classic();
    let long = r#"{"symbol":"BTCUSDT","holdSide":"long","total":"TOTAL","openPriceAvg":"90","posMode":"hedge_mode","marginMode":"crossed","uTime":"100"}"#;
    let short = r#"{"symbol":"BTCUSDT","holdSide":"short","total":"2","openPriceAvg":"110","posMode":"hedge_mode","marginMode":MARGIN,"uTime":"100"}"#;
    let order = |id: &str, price: &str, status: &str, extra: &str| {
        format!(
            r#"{{"orderId":"{id}","symbol":SYMBOL,"size":"2","price":"{price}","side":"buy","orderType":"limit","status":"{status}"{extra}}}"#
        )
    };
    let presets = r#","presetStopSurplusPrice":"120","presetStopLossPrice":"80""#;
    let positions = |rows: &[String]| format!(r#"{{"code":"00000","data":[{}]}}"#, rows.join(","));
    let orders = |rows: &[String]| {
        format!(
            r#"{{"code":"00000","data":{{"entrustedList":[{}]}}}}"#,
            rows.join(",").replace("SYMBOL", r#""BTCUSDT""#)
        )
    };
    let valid_short = short.replace("MARGIN", r#""crossed""#);
    h.respond(
        "/all-position",
        200,
        &positions(&[long.replace("TOTAL", "1"), valid_short.clone()]),
    );
    h.respond("/account/accounts", 200, ACCOUNT);
    h.orders(&orders(&[
        order("1", "90", "live", ""),
        order("2", "91", "live", ""),
        order("3", "92", "live", presets),
    ]));
    let stored_short = h.engine.store().positions()[1].clone();
    let stored: Vec<_> = ["1", "3", "3__stop_loss", "3__take_profit"]
        .map(|id| h.engine.store().order(id).unwrap().clone())
        .into();
    assert_eq!(
        stored_short.side,
        safe_cex_engine::model::PositionSide::Short
    );

    h.login();
    h.respond(
        "/all-position",
        200,
        &positions(&[
            long.replace("TOTAL", "3"),
            short.replace("MARGIN", r#"{"nested":1}"#),
        ]),
    );
    h.orders(&orders(&[
        order("1", "90", "live", "").replace("SYMBOL", "7"),
        order("2", "95", "live", ""),
        order("3", "92", "secret_new_status", presets),
    ]));
    let store = h.engine.store();
    assert_eq!(store.positions().len(), 2);
    assert_eq!(store.positions()[0].contracts, 3.0);
    assert_eq!(store.positions()[1], stored_short);
    assert_eq!(store.order("2").unwrap().price, 95.0);
    for row in &stored {
        assert_eq!(store.order(&row.id), Some(row));
    }
    for expected in [
        "skipped Bitget position record BTCUSDT: malformed record",
        "skipped Bitget order record 1: malformed record",
        "skipped Bitget order record 3: unknown Bitget order status",
    ] {
        assert!(h.logs.iter().any(|log| log == expected), "{expected}");
    }
    assert!(
        !h.logs
            .iter()
            .any(|log| log.contains("nested") || log.contains("secret_new_status"))
    );

    h.login();
    h.respond(
        "/all-position",
        200,
        &positions(&[long.replace("TOTAL", "3")]),
    );
    h.orders(&orders(&[order("2", "95", "live", "")]));
    let store = h.engine.store();
    assert_eq!(store.positions().len(), 1);
    assert!(store.order("1").is_none() && store.order("3").is_none());
    assert!(store.order("2").is_some());
    // Classic polls keep any open SL/TP absent from REST (pre-existing), so the
    // removed parent's preset legs remain until the venue reports them closed.
    assert!(store.order("3__stop_loss").is_some() && store.order("3__take_profit").is_some());
}

#[test]
fn skipped_rest_order_without_identity_keeps_every_stored_order() {
    let mut h = Harness::classic();
    h.respond("/all-position", 200, POSITION);
    h.respond("/account/accounts", 200, ACCOUNT);
    h.orders(r#"{"code":"00000","data":{"entrustedList":[{"orderId":2.5,"symbol":"BTCUSDT"},{"orderId":"2","symbol":"BTCUSDT","size":"1","price":"89","side":"buy","orderType":"limit","status":"live"}]}}"#);
    assert!(h.engine.store().order("1").is_some());
    assert!(h.engine.store().order("2").is_some());
    assert!(
        h.logs
            .iter()
            .any(|log| log == "skipped Bitget order record without id: malformed record")
    );
}
