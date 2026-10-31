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
fn delivered_fills(h: &mut Harness) -> Vec<(f64, f64)> {
    let Some(frame) = h.engine.take_frame(h.now) else {
        return Vec::new();
    };
    let Some((_, payload)) = safe_cex_engine::wire::sections(frame).find(|(kind, _)| *kind == 11)
    else {
        return Vec::new();
    };
    let count = u32::from_le_bytes(payload[..4].try_into().unwrap()) as usize;
    let offset = (4 + count * 12 + 7) & !7;
    (0..count)
        .map(|index| {
            let at = offset + index * 16;
            (
                f64::from_le_bytes(payload[at..at + 8].try_into().unwrap()),
                f64::from_le_bytes(payload[at + 8..at + 16].try_into().unwrap()),
            )
        })
        .collect()
}

#[test]
fn cumulative_preparation_classic_rejects_negative_price_before_execution_dedup() {
    let mut h = Harness::classic();
    delivered_fills(&mut h);
    let invalid = r#"{"arg":{"channel":"orders"},"action":"update","data":[{"orderId":"1","symbol":"BTCUSDT","size":"2","baseVolume":"0.4","accBaseVolume":"0.4","fillPrice":"-1","priceAvg":"99","price":"90","side":"buy","orderType":"limit","status":"partially_filled","tradeId":"correctable-price"}]}"#;
    h.private_message(invalid);
    assert!(delivered_fills(&mut h).is_empty());
    let corrected = invalid.replace("\"fillPrice\":\"-1\"", "\"fillPrice\":\"99\"");
    h.private_message(&corrected);
    assert_eq!(delivered_fills(&mut h), vec![(99.0, 0.4)]);
    h.private_message(&corrected);
    assert!(delivered_fills(&mut h).is_empty());
}

#[test]
fn cumulative_preparation_classic_correction_uses_pre_observation_delivered_seed() {
    let mut h = Harness::classic();
    delivered_fills(&mut h);
    let unpriced = r#"{"arg":{"channel":"orders"},"action":"update","data":[{"orderId":"1","symbol":"BTCUSDT","size":"2","accBaseVolume":"0.4","fillPrice":"0","price":"0","side":"buy","orderType":"market","status":"partially_filled","tradeId":"correctable-cumulative"}]}"#;
    h.private_message(unpriced);
    // A display observation is not evidence that a priced Fill was delivered.
    assert_eq!(h.engine.store().order("1").unwrap().filled, 0.4);
    delivered_fills(&mut h);
    let corrected = unpriced.replace("\"fillPrice\":\"0\"", "\"fillPrice\":\"99\"");
    h.private_message(&corrected);
    assert_eq!(delivered_fills(&mut h), vec![(99.0, 0.4)]);
    h.private_message(&corrected);
    assert!(delivered_fills(&mut h).is_empty());
}

#[test]
fn cumulative_preparation_classic_late_direct_fill_does_not_rewind_prefix() {
    let mut h = Harness::classic();
    delivered_fills(&mut h);
    let execution = r#"{"arg":{"channel":"orders"},"action":"update","data":[{"orderId":"1","symbol":"BTCUSDT","size":"2","baseVolume":"0.4","accBaseVolume":"0.8","fillPrice":"99","price":"90","side":"buy","orderType":"limit","status":"partially_filled","tradeId":"newer"}]}"#;
    h.private_message(execution);
    assert_eq!(delivered_fills(&mut h), vec![(99.0, 0.4)]);
    let late = execution
        .replace("\"accBaseVolume\":\"0.8\"", "\"accBaseVolume\":\"0.4\"")
        .replace("newer", "older");
    h.private_message(&late);
    assert_eq!(delivered_fills(&mut h), vec![(99.0, 0.4)]);
    let caught_up = execution
        .replace("\"baseVolume\":\"0.4\",", "")
        .replace("newer", "cumulative-catchup");
    h.private_message(&caught_up);
    assert!(delivered_fills(&mut h).is_empty());
    h.private_message(&late);
    assert!(delivered_fills(&mut h).is_empty());
    let higher = caught_up
        .replace("\"accBaseVolume\":\"0.8\"", "\"accBaseVolume\":\"1.2\"")
        .replace("cumulative-catchup", "higher");
    h.private_message(&higher);
    assert_eq!(delivered_fills(&mut h), vec![(99.0, 1.2 - 0.8)]);
    h.private_message(&higher);
    assert!(delivered_fills(&mut h).is_empty());
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
    h.respond("type=trailing_stop", 200, r#"{"code":"00000","data":[]}"#);
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
    h.respond("type=trailing_stop", 200, r#"{"code":"00000","data":[]}"#);
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
    h.respond("type=trailing_stop", 200, r#"{"code":"00000","data":[]}"#);
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

fn row_engine(unified: bool) -> Harness {
    if unified {
        uta(r#"{"code":"00000","data":[]}"#)
    } else {
        Harness::classic()
    }
}
fn position_message(rows: serde_json::Value) -> String {
    serde_json::json!({"arg":{"channel":"positions","topic":"position"},"action":"snapshot","data":rows}).to_string()
}
fn hedge_row(side: &str, size: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"symbol":"BTCUSDT","marginCoin":"USDT","holdSide":side,"total":size,"openPriceAvg":"90","leverage":"20","posMode":"hedge_mode","marginMode":"crossed","uTime":"200"})
}
#[test]
fn malformed_ws_positions_retain_cached_leg_and_admit_peers_for_classic_and_uta() {
    for unified in [false, true] {
        for malformed in [serde_json::json!({"bad":1}), serde_json::json!("-1")] {
            let mut h = row_engine(unified);
            h.private_message(&position_message(serde_json::json!([
                hedge_row("long", serde_json::json!("2")),
                hedge_row("short", serde_json::json!("3"))
            ])));
            h.private_message(&position_message(serde_json::json!([
                hedge_row("long", malformed),
                hedge_row("short", serde_json::json!("5"))
            ])));
            let positions = h.engine.store().positions();
            assert_eq!(
                positions
                    .iter()
                    .find(|p| p.side == safe_cex_engine::model::PositionSide::Short)
                    .unwrap()
                    .contracts,
                5.0,
                "healthy position starved"
            );
            assert_eq!(
                positions
                    .iter()
                    .find(|p| p.side == safe_cex_engine::model::PositionSide::Long)
                    .expect("malformed cached leg erased")
                    .contracts,
                2.0
            );
            assert!(
                h.logs
                    .iter()
                    .any(|m| m.contains("skipped Bitget position stream record"))
            );
            h.private_message(&position_message(serde_json::json!([])));
            assert!(
                h.engine.store().positions().is_empty(),
                "authoritative empty follow-up clears retained leg"
            );
        }
    }
}
#[test]
fn unidentified_ws_position_row_holds_absence_while_known_peer_updates() {
    let mut h = row_engine(false);
    h.private_message(&position_message(serde_json::json!([
        hedge_row("long", serde_json::json!("2")),
        hedge_row("short", serde_json::json!("3"))
    ])));
    h.private_message(&position_message(serde_json::json!([
        {},
        hedge_row("short", serde_json::json!("5"))
    ])));
    assert_eq!(h.engine.store().positions().len(), 2);
    assert_eq!(
        h.engine
            .store()
            .positions()
            .iter()
            .find(|p| p.side == safe_cex_engine::model::PositionSide::Short)
            .unwrap()
            .contracts,
        5.0
    );
}
fn ordinary_row(id: &str) -> serde_json::Value {
    serde_json::json!({"category":"usdt-futures","orderId":id,"symbol":"BTCUSDT","qty":"2","size":"2","price":"90","side":"buy","orderType":"limit","status":"live","orderStatus":"new"})
}
#[test]
fn malformed_ws_order_admits_healthy_peer_for_classic_and_uta() {
    for unified in [false, true] {
        let mut h = row_engine(unified);
        h.private_message(&serde_json::json!({"arg":{"channel":"orders","topic":"order"},"data":[ordinary_row("cached")]}).to_string());
        let mut bad = ordinary_row("cached");
        bad["size"] = serde_json::json!({"bad":1});
        bad["qty"] = serde_json::json!({"bad":1});
        h.private_message(&serde_json::json!({"arg":{"channel":"orders","topic":"order"},"data":[bad,ordinary_row("healthy")]}).to_string());
        assert!(
            h.engine.store().order("healthy").is_some(),
            "healthy order starved"
        );
        assert!(h.engine.store().order("cached").is_some());
        assert!(
            h.logs
                .iter()
                .any(|m| m.contains("Bitget order stream record"))
        );
    }
}
#[test]
fn rejected_ws_order_protects_cached_identity_from_older_rest_omission() {
    let mut h = Harness::classic();
    let mut bad = ordinary_row("1");
    bad["size"] = serde_json::json!({"bad":1});
    h.private_message(&serde_json::json!({"arg":{"channel":"orders"},"data":[bad]}).to_string());
    h.orders(r#"{"code":"00000","data":{"entrustedList":[]}}"#);
    assert!(
        h.engine.store().order("1").is_some(),
        "older clean REST erased rejected WS identity"
    );
    h.timer_after(500.0);
    assert!(
        h.requests.iter().any(|r| r.url.contains("/orders-pending")),
        "bounded repair missing"
    );
}

#[test]
fn repeated_malformed_ws_snapshot_cannot_starve_fresh_rest_only_position_peer() {
    let mut h = Harness::classic();
    let seed = hedge_row("long", serde_json::json!("2"));
    h.private_message(&position_message(serde_json::json!([seed])));
    h.respond(
        "/all-position",
        200,
        &serde_json::json!({"code":"00000","data":[seed]}).to_string(),
    );
    h.timer_after(500.0);
    h.respond(
        "/all-position",
        200,
        &serde_json::json!({"code":"00000","data":[seed]}).to_string(),
    );
    h.now += 6_000.0;
    for size in [3, 4] {
        let bad = hedge_row("long", serde_json::json!({"bad":1}));
        h.private_message(&position_message(serde_json::json!([bad])));
        h.timer_after(500.0);
        let read = h.take("/all-position");
        h.now += 1.0;
        h.private_message(&position_message(serde_json::json!([bad])));
        let mut peer = hedge_row("short", serde_json::json!(size.to_string()));
        peer.as_object_mut().unwrap().remove("uTime");
        h.reply(read.id,200,&serde_json::json!({"code":"00000","data":[hedge_row("long",serde_json::json!("9")),peer]}).to_string());
        assert_eq!(
            h.engine
                .store()
                .positions()
                .iter()
                .find(|p| p.side == safe_cex_engine::model::PositionSide::Short)
                .expect("REST-only peer starved by fake WS full-snapshot authority")
                .contracts,
            size as f64
        );
        assert_eq!(
            h.engine
                .store()
                .positions()
                .iter()
                .find(|p| p.side == safe_cex_engine::model::PositionSide::Long)
                .unwrap()
                .contracts,
            2.0
        );
    }
}

#[test]
fn malformed_snapshot_cannot_erase_clean_empty_proof_for_an_older_rest_read() {
    let mut h = Harness::classic();
    let read = h.take("/all-position");
    h.private_message(&position_message(serde_json::json!([])));
    h.now += 1.0;
    h.private_message(&position_message(serde_json::json!([{}])));
    let peer = hedge_row("short", serde_json::json!("3"));
    h.reply(
        read.id,
        200,
        &serde_json::json!({"code":"00000","data":[peer]}).to_string(),
    );
    assert!(
        h.engine.store().positions().is_empty(),
        "bad snapshot erased earlier clean-empty read fence"
    );
    h.timer_after(499.0);
    h.respond(
        "/all-position",
        200,
        &serde_json::json!({"code":"00000","data":[peer]}).to_string(),
    );
    assert_eq!(
        h.engine.store().positions().len(),
        1,
        "fresh read after clean proof must heal normally"
    );
}
#[test]
fn rejected_ws_leg_protects_mode_coherence_through_an_older_rest_join() {
    let mut h = Harness::classic();
    let hedge = hedge_row("long", serde_json::json!("2"));
    h.private_message(&position_message(serde_json::json!([hedge])));
    h.respond(
        "/all-position",
        200,
        &serde_json::json!({"code":"00000","data":[hedge]}).to_string(),
    );
    h.timer_after(500.0);
    let old = h.take("/all-position");
    h.now += 1.0;
    h.private_message(&position_message(serde_json::json!([hedge_row(
        "long",
        serde_json::json!({"bad":1})
    )])));
    let mut changed = hedge_row("long", serde_json::json!("4"));
    changed["posMode"] = serde_json::json!("one_way_mode");
    h.reply(
        old.id,
        200,
        &serde_json::json!({"code":"00000","data":[changed]}).to_string(),
    );
    let positions = h.engine.store().positions();
    assert_eq!(
        positions.len(),
        1,
        "old read mixed aggregate and hedged position IDs"
    );
    assert_eq!(
        positions[0].position_mode,
        Some(safe_cex_engine::model::PositionMode::Hedged)
    );
    // Rejection must preserve the earlier valid WS financial freshness window.
    // Deliver the queued repair after that existing five-second window expires.
    let (timer, _) = h
        .timers
        .iter()
        .copied()
        .find(|(_, at)| (*at - h.now - 500.0).abs() < 0.01)
        .unwrap();
    h.timers.retain(|(id, _)| *id != timer);
    h.now += 5_001.0;
    h.event(Event::TimerFired { timer });
    h.respond(
        "/all-position",
        200,
        &serde_json::json!({"code":"00000","data":[changed]}).to_string(),
    );
    assert_eq!(h.engine.store().positions().len(), 1);
    assert_eq!(
        h.engine.store().positions()[0].position_mode,
        Some(safe_cex_engine::model::PositionMode::Oneway)
    );
}
#[test]
fn rejected_ws_protection_retains_cancel_route_against_older_rest_plan_metadata() {
    let mut h = Harness::classic();
    let live = serde_json::json!({"orderId":"protected","symbol":"BTCUSDT","size":"1","price":"80","triggerPrice":"80","side":"sell","orderType":"market","planType":"pos_loss","status":"live"});
    h.private_message(
        &serde_json::json!({"arg":{"channel":"orders-algo"},"data":[live]}).to_string(),
    );
    let mut bad = live.clone();
    bad["price"] = serde_json::json!({"bad":1});
    h.private_message(
        &serde_json::json!({"arg":{"channel":"orders-algo"},"data":[bad]}).to_string(),
    );
    h.respond(
        "/orders-pending",
        200,
        r#"{"code":"00000","data":{"entrustedList":[]}}"#,
    );
    let mut old = live.clone();
    old["planType"] = serde_json::json!("profit_plan");
    h.respond(
        "planType=profit_loss",
        200,
        &serde_json::json!({"code":"00000","data":{"entrustedList":[old]}}).to_string(),
    );
    h.respond("planType=track_plan", 200, r#"{"code":"00000","data":[]}"#);
    assert_eq!(
        h.engine.store().order("protected").unwrap().kind,
        safe_cex_engine::model::OrderType::StopLoss
    );
    h.command(safe_cex_engine::Command::CancelOrders {
        command: 88,
        ids: vec!["protected".into()],
    });
    let request = h.take("/cancel-plan-order");
    let body: serde_json::Value = serde_json::from_str(request.body.as_ref().unwrap()).unwrap();
    assert_eq!(
        body["planType"], "pos_loss",
        "stale route metadata disagrees with retained native protection"
    );
}

#[test]
fn rejected_ws_leg_does_not_erase_prior_valid_financial_timestamp() {
    let mut h = Harness::classic();
    let seed = hedge_row("long", serde_json::json!("2"));
    h.private_message(&position_message(serde_json::json!([seed])));
    h.respond(
        "/all-position",
        200,
        &serde_json::json!({"code":"00000","data":[seed]}).to_string(),
    );
    h.now += 1.0;
    h.private_message(&position_message(serde_json::json!([hedge_row(
        "long",
        serde_json::json!({"bad":1})
    )])));
    h.timer_after(499.0);
    let mut older = hedge_row("long", serde_json::json!("1"));
    older["uTime"] = serde_json::json!("100");
    h.respond(
        "/all-position",
        200,
        &serde_json::json!({"code":"00000","data":[older]}).to_string(),
    );
    assert_eq!(
        h.engine.store().positions()[0].contracts,
        2.0,
        "rejection erased real newer WS financial proof"
    );
    h.now += 1.0;
    h.private_message(&position_message(serde_json::json!([hedge_row(
        "long",
        serde_json::json!({"bad":1})
    )])));
    h.timer_after(500.0);
    let mut newer = hedge_row("long", serde_json::json!("3"));
    newer["uTime"] = serde_json::json!("300");
    h.respond(
        "/all-position",
        200,
        &serde_json::json!({"code":"00000","data":[newer]}).to_string(),
    );
    assert_eq!(
        h.engine.store().positions()[0].contracts,
        3.0,
        "fresh newer native proof must heal"
    );
}
fn two_market_rows() -> Harness {
    let mut h = Harness::classic();
    let mut markets = h.engine.store().markets().to_vec();
    let mut eth = markets[0].clone();
    eth.id = "ETHUSDT".into();
    eth.symbol = "ETHUSDT".into();
    eth.base = "ETH".into();
    markets.push(eth);
    h.engine.store_mut().set_markets(markets);
    h
}
#[test]
fn partial_and_mixed_ws_modes_cannot_flip_global_mode_across_symbols() {
    for reverse in [false, true] {
        for malformed in [false, true] {
            let mut h = two_market_rows();
            let mut btc = hedge_row("long", serde_json::json!("1"));
            btc["posMode"] = serde_json::json!("one_way_mode");
            let mut eth = btc.clone();
            eth["symbol"] = serde_json::json!("ETHUSDT");
            eth["total"] = serde_json::json!("2");
            h.private_message(&position_message(serde_json::json!([btc, eth])));
            btc["posMode"] = serde_json::json!("hedge_mode");
            btc["total"] = serde_json::json!("4");
            eth["total"] = if malformed {
                serde_json::json!({"bad":1})
            } else {
                serde_json::json!("3")
            };
            let rows = if reverse {
                vec![eth.clone(), btc.clone()]
            } else {
                vec![btc.clone(), eth.clone()]
            };
            h.private_message(&position_message(serde_json::json!(rows)));
            assert!(
                !h.engine.store().hedged(),
                "partial contradictory modes changed global routing"
            );
            assert!(
                h.engine
                    .store()
                    .positions()
                    .iter()
                    .all(|p| p.position_mode == Some(safe_cex_engine::model::PositionMode::Oneway))
            );
            // A complete consistent proof can still change account mode.
            eth["posMode"] = serde_json::json!("hedge_mode");
            eth["total"] = serde_json::json!("3");
            h.private_message(&position_message(serde_json::json!([btc, eth])));
            assert!(h.engine.store().hedged());
            assert!(
                h.engine
                    .store()
                    .positions()
                    .iter()
                    .all(|p| p.position_mode == Some(safe_cex_engine::model::PositionMode::Hedged))
            );
        }
    }
}
#[test]
fn initial_authenticated_malformed_snapshot_cannot_load_empty_positions() {
    let mut h = Harness::private_started();
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"40084","msg":"Classic Account mode","data":null}"#,
    );
    h.login();
    assert!(!h.engine.store().loaded().positions);
    h.private_message(&position_message(serde_json::json!([{}])));
    assert!(
        !h.engine.store().loaded().positions,
        "malformed initial data manufactured readiness"
    );
    assert!(h.engine.store().positions().is_empty());
    h.private_message(&position_message(serde_json::json!([])));
    assert!(
        h.engine.store().loaded().positions,
        "valid empty proof must establish readiness"
    );
}

#[test]
fn clean_position_delta_cannot_change_global_mode_while_other_symbol_is_retained() {
    let mut h = two_market_rows();
    let mut btc = hedge_row("long", serde_json::json!("1"));
    btc["posMode"] = serde_json::json!("one_way_mode");
    let mut eth = btc.clone();
    eth["symbol"] = serde_json::json!("ETHUSDT");
    h.private_message(&position_message(serde_json::json!([btc, eth])));
    btc["posMode"] = serde_json::json!("hedge_mode");
    h.private_message(
        &serde_json::json!({"arg":{"channel":"positions"},"action":"update","data":[btc]})
            .to_string(),
    );
    assert!(
        !h.engine.store().hedged(),
        "single-symbol delta flipped account mode with another symbol retained"
    );
    assert!(
        h.engine
            .store()
            .positions()
            .iter()
            .all(|p| p.position_mode == Some(safe_cex_engine::model::PositionMode::Oneway))
    );
}

fn fetch_leverage(h: &mut Harness, command: u32, symbol: &str) {
    h.command(safe_cex_engine::Command::FetchLeverage {
        command,
        symbol: symbol.into(),
    });
}

fn fetched(h: &Harness, command: u32) -> serde_json::Value {
    let (_, result) = h
        .done
        .iter()
        .find(|(id, _)| *id == command)
        .expect("the read must settle");
    serde_json::from_str(result.as_ref().expect("the read must succeed")).unwrap()
}

#[test]
fn a_classic_flat_symbol_reads_its_leverage_on_demand() {
    let mut h = Harness::classic();
    assert_eq!(
        h.engine.capabilities().leverage_query,
        safe_cex_engine::LeverageQuery::OnReadAndMetadata
    );
    fetch_leverage(&mut h, 80, "BTCUSDT");
    let request = h.take("/mix/account/account?symbol=BTCUSDT");
    assert!(request.url.contains("marginCoin=USDT"), "{}", request.url);
    h.reply(
        request.id,
        200,
        r#"{"code":"00000","data":{"marginMode":"crossed","posMode":"one_way_mode","crossedMarginLeverage":"33"}}"#,
    );
    let value = fetched(&h, 80);
    assert_eq!(value["buyLeverage"], 33.0);
    assert_eq!(value["sellLeverage"], 33.0);
    assert_eq!(h.engine.store().setting("BTCUSDT").unwrap().leverage, 33.0);
}

#[test]
fn a_classic_isolated_hedged_symbol_reads_each_side() {
    let mut h = Harness::classic();
    fetch_leverage(&mut h, 81, "BTCUSDT");
    let request = h.take("/mix/account/account?symbol=BTCUSDT");
    h.reply(
        request.id,
        200,
        r#"{"code":"00000","data":{"marginMode":"isolated","posMode":"hedge_mode","isolatedLongLever":"11","isolatedShortLever":"6"}}"#,
    );
    let value = fetched(&h, 81);
    assert_eq!(value["buyLeverage"], 11.0);
    assert_eq!(value["sellLeverage"], 6.0);
    assert_eq!(value["positionMode"], "hedged");
}

#[test]
fn a_failed_leverage_read_fails_its_command_and_writes_nothing() {
    let mut h = Harness::classic();
    let before = h.engine.store().setting("BTCUSDT");
    fetch_leverage(&mut h, 82, "BTCUSDT");
    let request = h.take("/mix/account/account?symbol=BTCUSDT");
    h.reply(
        request.id,
        200,
        r#"{"code":"40001","msg":"no permission","data":null}"#,
    );
    let (_, result) = h.done.iter().find(|(id, _)| *id == 82).unwrap();
    assert!(result.as_ref().unwrap_err().contains("no permission"));
    assert_eq!(h.engine.store().setting("BTCUSDT"), before);
}

#[test]
fn a_uta_symbol_absent_from_the_settings_list_fails_rather_than_guessing() {
    let mut h = uta(r#"{"code":"00000","data":{"list":[]}}"#);
    fetch_leverage(&mut h, 83, "BTCUSDT");
    let request = h.take("/account/settings");
    h.reply(
        request.id,
        200,
        r#"{"code":"00000","data":{"holdMode":"one_way_mode","symbolConfigList":[{"symbol":"BTCUSDT","category":"USDT-FUTURES","leverage":"44","marginMode":"crossed"}]}}"#,
    );
    assert_eq!(fetched(&h, 83)["buyLeverage"], 44.0);
    fetch_leverage(&mut h, 84, "BTCUSDT");
    let request = h.take("/account/settings");
    h.reply(
        request.id,
        200,
        r#"{"code":"00000","data":{"holdMode":"one_way_mode","symbolConfigList":[]}}"#,
    );
    let (_, result) = h.done.iter().find(|(id, _)| *id == 84).unwrap();
    assert!(result.as_ref().unwrap_err().contains("do not list"));
}

#[test]
fn an_unknown_symbol_is_refused_before_any_request() {
    let mut h = Harness::classic();
    fetch_leverage(&mut h, 85, "NOPEUSDT");
    let (_, result) = h.done.iter().find(|(id, _)| *id == 85).unwrap();
    assert!(result.as_ref().unwrap_err().contains("unknown market"));
}
