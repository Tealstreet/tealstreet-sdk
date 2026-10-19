mod support;
use safe_cex_engine::Event;
use support::*;

#[test]
fn classic_startup_signs_and_structural_login_subscribes_all_channels() {
    let mut h = Harness::private_started();
    let req = h.take("/account/settings");
    let headers: std::collections::HashMap<_, _> = req.headers.into_iter().collect();
    assert_eq!(headers["ACCESS-TIMESTAMP"], "1000000");
    assert_eq!(headers["ACCESS-PASSPHRASE"], "pass");
    assert_eq!(headers["X-CHANNEL-API-CODE"], "Tealstreet");
    assert_eq!(
        headers["ACCESS-SIGN"],
        "rFwHGJEzlKfEoWaV9jUV5EhIZQy29SIQpDzIix+yHBY="
    );
    h.reply(
        req.id,
        200,
        r#"{"code":"40084","msg":"Classic Account mode","data":null}"#,
    );
    h.respond("/all-position", 200, POSITION);
    h.respond("/account/accounts", 200, ACCOUNT);
    h.orders(OPEN_ORDER);
    h.login();
    assert!(
        h.sent
            .iter()
            .any(|(_, text)| text.contains("4oq2NH54iUZJ50BHqQXAzE1sOFIHoHwHlRpNn6Izxj8="))
    );
    let subscribe = h
        .sent
        .iter()
        .find(|(_, t)| t.contains("orders-algo"))
        .unwrap();
    assert!(subscribe.1.contains("USDT-FUTURES"));
    assert!(subscribe.1.contains("\"coin\":\"default\""));
    let loaded = h.engine.store().loaded();
    assert!(loaded.balance && loaded.positions && loaded.orders);
    let balance = h.engine.store().balance();
    assert_eq!(
        (balance.used, balance.free, balance.total, balance.upnl),
        (110.0, 900.0, 1000.0, 10.0)
    );
    assert_eq!(h.engine.store().setting("BTCUSDT").unwrap().leverage, 20.0);
}

#[test]
fn failed_and_malformed_snapshots_never_clear_account() {
    let mut h = Harness::classic();
    // Opening the private socket requested a new authoritative snapshot.
    h.respond(
        "/all-position",
        200,
        r#"{"code":"40001","msg":"bad key","data":[]}"#,
    );
    h.respond(
        "/account/accounts",
        500,
        r#"{"code":"500","msg":"error","data":[]}"#,
    );
    h.respond(
        "/orders-pending",
        200,
        r#"{"code":"00000","data":{"entrustedList":[{}]}}"#,
    );
    assert_eq!(h.engine.store().positions().len(), 1);
    assert_eq!(h.engine.store().orders().len(), 1);
    assert_eq!(h.engine.store().balance().total, 1000.0);
    assert!(h.logs.iter().any(|e| e.contains("40001")));
}

#[test]
fn empty_position_snapshot_blocks_older_rest_and_zero_updates_cannot_reopen() {
    let mut h = Harness::classic();
    let request = h.take("/all-position");
    h.private_message(r#"{"arg":{"channel":"positions"},"action":"snapshot","data":[]}"#);
    h.reply(request.id, 200, POSITION);
    assert!(h.engine.store().positions().is_empty());
    h.private_message(r#"{"arg":{"channel":"positions"},"action":"update","data":[{"symbol":"BTCUSDT","holdSide":"long","total":"1","openPriceAvg":"90","posMode":"one_way_mode","marginMode":"crossed","uTime":"200"}]}"#);
    h.timer_after(500.0);
    let request = h.take("/all-position");
    h.now += 1.0;
    h.private_message(r#"{"arg":{"channel":"positions"},"action":"update","data":[{"symbol":"BTCUSDT","holdSide":"long","total":"0","posMode":"one_way_mode","marginMode":"crossed","uTime":"300"}]}"#);
    h.reply(request.id, 200, POSITION);
    assert!(h.engine.store().positions().is_empty());
}

#[test]
fn newer_position_liquidation_survives_blank_and_explicit_zero_is_authoritative() {
    let mut h = Harness::classic();
    let payload = r#"{"arg":{"channel":"positions"},"action":"update","data":[{"symbol":"BTCUSDT","total":"1","openPriceAvg":"90","posMode":"one_way_mode","marginMode":"crossed","liquidationPrice":"VALUE","uTime":"200"}]}"#;
    h.private_message(&payload.replace("VALUE", ""));
    assert_eq!(h.engine.store().positions()[0].liquidation_price, 50.0);
    h.private_message(&payload.replace("VALUE", "0"));
    assert_eq!(h.engine.store().positions()[0].liquidation_price, 0.0);
}

#[test]
fn balance_rest_cannot_overwrite_newer_stream_balance() {
    let mut h = Harness::classic();
    let request = h.take("/account/accounts");
    h.private_message(r#"{"arg":{"channel":"account"},"data":[{"marginCoin":"USDT","unionTotalMargin":"2000","unionAvailable":"1800","unrealizedPL":"10"}]}"#);
    h.reply(request.id, 200, ACCOUNT);
    assert_eq!(h.engine.store().balance().free, 1800.0);
}

#[test]
fn terminal_order_tombstone_and_ws_open_grace_guard_rest() {
    let mut h = Harness::classic();
    let normal = h.take("/orders-pending");
    h.private_message(r#"{"arg":{"channel":"orders"},"data":[{"orderId":"1","symbol":"BTCUSDT","size":"2","price":"90","side":"buy","orderType":"limit","status":"canceled"}]}"#);
    h.private_message(r#"{"arg":{"channel":"orders"},"data":[{"orderId":"2","symbol":"BTCUSDT","size":"1","price":"89","side":"buy","orderType":"limit","status":"live"}]}"#);
    h.reply(normal.id, 200, OPEN_ORDER);
    h.respond(
        "planType=profit_loss",
        200,
        r#"{"code":"00000","data":{"entrustedList":[]}}"#,
    );
    h.respond("planType=track_plan", 200, r#"{"code":"00000","data":[]}"#);
    assert!(h.engine.store().order("1").is_none());
    assert!(h.engine.store().order("2").is_some());
}

#[test]
fn partial_executions_emit_once_and_do_not_fill_on_algo_trigger_ack() {
    let mut h = Harness::classic();
    h.engine.take_frame(h.now);
    let execution = r#"{"arg":{"channel":"orders"},"action":"snapshot","data":[{"orderId":"1","symbol":"BTCUSDT","size":"2","baseVolume":"0.4","accBaseVolume":"0.4","priceAvg":"99","price":"90","side":"buy","orderType":"limit","status":"partially_filled","tradeId":"9007199254740993"}]}"#;
    h.private_message(execution);
    h.private_message(execution);
    let frame = h.engine.take_frame(h.now).unwrap().to_vec();
    // FILLS section starts after the order/status/string sections, decode its row count.
    assert_eq!(fill_count(&frame), 1);
    h.private_message(r#"{"arg":{"channel":"orders-algo"},"data":[{"orderId":"3","symbol":"BTCUSDT","size":"1","actualSize":"1","triggerPrice":"90","holdSide":"long","planType":"loss_plan","status":"triggered"}]}"#);
    assert_eq!(h.engine.take_frame(h.now).map_or(0, fill_count), 0);
}
fn fill_count(frame: &[u8]) -> u32 {
    let mut at = 16;
    let mut count = 0;
    while at + 8 <= frame.len() {
        let kind = u16::from_le_bytes(frame[at..at + 2].try_into().unwrap());
        let bytes = u32::from_le_bytes(frame[at + 4..at + 8].try_into().unwrap()) as usize;
        if kind == 11 {
            count += u32::from_le_bytes(frame[at + 8..at + 12].try_into().unwrap());
        }
        at += 8 + bytes;
    }
    count
}

#[test]
fn uta_detection_uses_v3_topics_and_open_order_pagination() {
    let mut h = Harness::private_started();
    h.respond("/account/settings", 200, r#"{"code":"00000","data":{"holdMode":"hedge_mode","symbolConfigList":[{"symbol":"BTCUSDT","category":"USDT-FUTURES","leverage":"10","marginMode":"crossed"}]}}"#);
    assert!(h.sockets.iter().any(|(_, u)| u.contains("/v3/ws/private")));
    assert!(h.requests.iter().all(|r| !r.url.contains("/api/v2/")));
    h.respond(
        "/current-position",
        200,
        r#"{"code":"00000","data":{"list":[]}}"#,
    );
    h.respond(
        "/account/assets",
        200,
        r#"{"code":"00000","data":{"accountEquity":"1000","effEquity":"900","unrealisedPnl":"0"}}"#,
    );
    h.respond("/unfilled-orders", 200, r#"{"code":"00000","data":{"list":[{"orderId":"1","symbol":"BTCUSDT","qty":"2","price":"90","side":"buy","orderType":"limit","orderStatus":"live"}],"cursor":"next"}}"#);
    let req = h.take("cursor=next");
    h.reply(
        req.id,
        200,
        r#"{"code":"00000","data":{"list":[],"cursor":""}}"#,
    );
    h.respond(
        "/unfilled-strategy-orders",
        200,
        r#"{"code":"00000","data":[]}"#,
    );
    h.login();
    assert!(h.engine.store().hedged());
    assert!(
        h.sent.iter().any(
            |(_, t)| t.contains("\"instType\":\"UTA\"") && t.contains("\"topic\":\"position\"")
        )
    );
    assert_eq!(h.engine.store().orders().len(), 1);
}

#[test]
fn probe_auth_error_does_not_fall_back_to_classic() {
    let mut h = Harness::private_started();
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"40009","msg":"bad signature","data":null}"#,
    );
    assert!(!h.sockets.iter().any(|(_, u)| u.ends_with("/private")));
    assert!(h.requests.is_empty());
}

#[test]
fn private_socket_ignores_payload_before_login_and_watchdog_covers_it() {
    let mut h = Harness::private_started();
    h.respond("/account/settings", 200, r#"{"code":"40084","data":null}"#);
    h.private_message(r#"{"arg":{"channel":"positions"},"action":"snapshot","data":[]}"#);
    assert!(!h.engine.store().loaded().positions);
    let socket = h
        .sockets
        .iter()
        .find(|(_, u)| u.ends_with("/private"))
        .unwrap()
        .0;
    h.event(Event::WsOpened { socket });
    h.now += 25_000.0;
    for (timer, _) in h.timers.clone() {
        h.event(Event::TimerFired { timer });
    }
    assert!(h.closed.contains(&socket));
}

fn uta(strategy: &str) -> Harness {
    let mut h = Harness::private_started();
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"00000","data":{"holdMode":"hedge_mode"}}"#,
    );
    h.respond(
        "/current-position",
        200,
        r#"{"code":"00000","data":{"list":[]}}"#,
    );
    h.respond(
        "/account/assets",
        200,
        r#"{"code":"00000","data":{"accountEquity":"1000","effEquity":"900","unrealisedPnl":"0"}}"#,
    );
    h.respond(
        "/unfilled-orders",
        200,
        r#"{"code":"00000","data":{"list":[]}}"#,
    );
    h.respond("/unfilled-strategy-orders", 200, strategy);
    h.login();
    h
}

#[test]
fn uta_statusless_combined_strategy_keeps_both_native_legs() {
    let h = uta(
        r#"{"code":"00000","data":[{"orderId":"strategy","symbol":"BTCUSDT","qty":"1","side":"sell","posSide":"long","takeProfit":"120","stopLoss":"80","createdTime":"100","updatedTime":"200"}]}"#,
    );
    assert_eq!(h.engine.store().orders().len(), 2, "{:?}", h.logs);
    assert!(h.engine.store().order("strategy__stop_loss").is_some());
    assert!(h.engine.store().order("strategy__take_profit").is_some());
}

#[test]
fn uta_mixed_product_streams_filter_foreign_orders_and_positions() {
    let mut h = uta(r#"{"code":"00000","data":[]}"#);
    h.private_message(r#"{"arg":{"topic":"order"},"action":"snapshot","data":[{"category":"spot","orderId":"spot","symbol":"BTCUSDT","qty":"1","price":"90","side":"buy","orderType":"limit","orderStatus":"new"}]}"#);
    assert!(h.engine.store().orders().is_empty());
    h.private_message(r#"{"arg":{"topic":"position"},"action":"snapshot","data":[{"symbol":"ETHUSDC","marginCoin":"USDC","total":"1"},{"symbol":"BTCUSDT","marginCoin":"USDT","holdSide":"long","total":"1","avgPrice":"90","markPrice":"100","leverage":"5","marginMode":"crossed","totalFundingFee":"2","curRealisedPnl":"1"}]}"#);
    assert_eq!(h.engine.store().positions().len(), 1, "{:?}", h.logs);
    assert_eq!(h.engine.store().positions()[0].realized_pnl, Some(3.0));
    assert!(h.engine.store().hedged());
}

#[test]
fn uta_fill_topic_emits_exact_executions_and_order_snapshot_does_not_duplicate() {
    let mut h = uta(r#"{"code":"00000","data":[]}"#);
    h.engine.take_frame(h.now);
    h.private_message(r#"{"arg":{"topic":"order"},"action":"snapshot","data":[{"category":"usdt-futures","orderId":"1","symbol":"BTCUSDT","qty":"2","side":"buy","orderType":"market","orderStatus":"filled","cumExecQty":"2","avgPrice":"150"}]}"#);
    let fill = r#"{"arg":{"topic":"fill"},"action":"snapshot","data":[{"category":"usdt-futures","symbol":"BTCUSDT","orderId":"1","execId":"exec","execPrice":"200","execQty":"1","side":"buy"}]}"#;
    h.private_message(fill);
    h.private_message(fill);
    assert_eq!(fill_count(h.engine.take_frame(h.now).unwrap()), 1);
    assert!(h.sent.iter().any(|(_, t)| t.contains("\"topic\":\"fill\"")));
}

#[test]
fn classic_40085_without_array_promotes_route_and_ignores_old_http_generation() {
    let mut h = Harness::classic();
    let old_position = h.take("/all-position");
    let old_socket = h
        .sockets
        .iter()
        .find(|(_, u)| u.contains("/v2/ws/private"))
        .unwrap()
        .0;
    h.respond(
        "/account/accounts",
        200,
        r#"{"code":"40085","msg":"Unified Account mode","data":null}"#,
    );
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"00000","data":{"holdMode":"hedge_mode"}}"#,
    );
    assert!(h.closed.contains(&old_socket));
    assert!(h.sockets.iter().any(|(_, u)| u.contains("/v3/ws/private")));
    h.respond(
        "/current-position",
        200,
        r#"{"code":"00000","data":{"list":[]}}"#,
    );
    h.reply(old_position.id, 200, POSITION);
    assert!(h.engine.store().positions().is_empty());
}

#[test]
fn uta_close_side_stays_execution_direction_and_virtual_refresh_keeps_strategies() {
    let mut h = uta(
        r#"{"code":"00000","data":[{"orderId":"strategy","symbol":"BTCUSDT","qty":"1","side":"sell","posSide":"long","takeProfit":"120","stopLoss":"80"}]}"#,
    );
    h.private_message(r#"{"arg":{"topic":"order"},"action":"snapshot","data":[{"category":"usdt-futures","orderId":"close","symbol":"BTCUSDT","qty":"1","price":"110","holdMode":"hedge_mode","tradeSide":"close","side":"sell","posSide":"long","reduceOnly":"yes","orderType":"limit","orderStatus":"new"}]}"#);
    assert_eq!(
        h.engine.store().order("close").unwrap().side,
        safe_cex_engine::model::OrderSide::Sell
    );
    // Finish reconnect's full refresh, then drive the fourth polling tick.
    h.respond(
        "/unfilled-orders",
        200,
        r#"{"code":"00000","data":{"list":[]}}"#,
    );
    h.respond("/unfilled-strategy-orders", 200, r#"{"code":"00000","data":[{"orderId":"strategy","symbol":"BTCUSDT","qty":"1","side":"sell","posSide":"long","takeProfit":"120","stopLoss":"80"}]}"#);
    for _ in 0..4 {
        h.timer_after(2000.0);
    }
    h.respond(
        "/unfilled-orders",
        200,
        r#"{"code":"00000","data":{"list":[]}}"#,
    );
    assert!(h.engine.store().order("strategy__stop_loss").is_some());
    assert!(h.engine.store().order("strategy__take_profit").is_some());
}
