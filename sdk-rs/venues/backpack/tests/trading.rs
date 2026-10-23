mod support;
use safe_cex_engine::model::{OrderIdentityKind, OrderStatus};
use safe_cex_engine::{Action, Command, Event, Method, Notice};
use support::*;
fn place(e: &mut Driver, id: u32, orders: &str) {
    let orders = serde_json::from_str(orders).unwrap();
    e.handle(
        2000.0,
        Event::Command(Command::PlaceOrders {
            command: id,
            orders,
        }),
    );
}
fn done(e: &mut Driver) -> Result<String, String> {
    e.drain_actions()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone { result, .. }) => Some(Ok(result)),
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(Err(error.legacy_text())),
            _ => None,
        })
        .expect("command completed")
}

#[test]
fn caller_and_generated_pair_identities_precede_the_signed_write() {
    caller_or_generated_pair_identities(false);
    caller_or_generated_pair_identities(true);
}

fn caller_or_generated_pair_identities(generated: bool) {
    let (mut e, _) = ready();
    e.store_mut()
        .update_markets(|market| market.limits.amount.max = 1000.0);
    let orders = if generated {
        r#"[{"symbol":"SOL_USDC_PERP","type":"limit","side":"buy","amount":1,"price":140},{"symbol":"SOL_USDC_PERP","type":"limit","side":"sell","amount":1,"price":150}]"#
    } else {
        r#"[{"symbol":"SOL_USDC_PERP","type":"limit","side":"buy","amount":1,"price":140,"clientOrderId":"4294967295"}]"#
    };
    place(&mut e, 91, orders);
    let actions = e.drain_actions().collect::<Vec<_>>();
    let (progress_at, receipts) = actions
        .iter()
        .enumerate()
        .find_map(|(index, action)| match action {
            Action::Notice(Notice::CommandProgress {
                command: 91,
                receipts,
            }) => Some((index, receipts)),
            _ => None,
        })
        .expect("native client identities");
    let (write_at, request) = actions
        .iter()
        .enumerate()
        .find_map(|(index, action)| match action {
            Action::Http(request) => Some((index, request)),
            _ => None,
        })
        .unwrap();
    assert!(progress_at < write_at);
    #[derive(serde::Deserialize)]
    struct Row {
        #[serde(rename = "clientId")]
        client_id: u32,
    }
    let body = request.body.as_deref().unwrap();
    let rows: Vec<Row> = if generated {
        serde_json::from_str(body).unwrap()
    } else {
        vec![serde_json::from_str(body).unwrap()]
    };
    assert_eq!(receipts.len(), rows.len());
    if generated {
        assert_eq!(rows.len(), 2);
        assert!(rows[0].client_id > 0 && rows[1].client_id > 0);
        assert_ne!(rows[0].client_id, rows[1].client_id);
    } else {
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].client_id, u32::MAX);
    }
    for (receipt, row) in receipts.iter().zip(rows) {
        assert_eq!(receipt.symbol, "SOL_USDC_PERP");
        assert_eq!(receipt.kind, OrderIdentityKind::ClientOrderId);
        assert_eq!(receipt.identity, row.client_id.to_string());
    }
}

#[test]
fn a_single_native_order_without_client_id_does_not_invent_a_receipt() {
    let (mut e, _) = ready();
    place(
        &mut e,
        92,
        r#"[{"symbol":"SOL_USDC_PERP","type":"limit","side":"buy","amount":1,"price":140}]"#,
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(
        actions
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Notice(Notice::CommandProgress { .. })))
    );
}
#[test]
fn bybit_execution_controls_reject_entire_batch_before_writes() {
    for (field, value) in [
        (
            "bybitBbo",
            serde_json::json!({"sideType":"Queue","level":1}),
        ),
        ("bybitSlippageToleranceTicks", serde_json::json!(2)),
    ] {
        let (mut e, _) = ready();
        let mut orders = serde_json::json!([
            {"symbol":"SOL_USDC_PERP","type":"limit","side":"buy","amount":1,"price":140,"clientOrderId":"42"},
            {"symbol":"SOL_USDC_PERP","type":"limit","side":"buy","amount":1,"price":140,"clientOrderId":"43"}
        ]);
        orders[1][field] = value;
        place(&mut e, 1, &orders.to_string());
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandFailed { error, .. }) if error.message.contains("Bybit")
        )));
    }
}

#[test]
fn caller_uint32_is_numeric_on_wire_and_bad_batch_ids_reject_before_any_write() {
    let (mut e, _) = ready();
    for id in ["0", "01", "4294967296", "-1", "1.0", "test"] {
        place(
            &mut e,
            1,
            &format!(
                r#"[{{"symbol":"SOL_USDC_PERP","type":"limit","side":"buy","amount":1,"price":140}},{{"symbol":"SOL_USDC_PERP","type":"limit","side":"buy","amount":1,"price":140,"clientOrderId":"{id}"}}]"#
            ),
        );
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
        );
    }
    place(
        &mut e,
        2,
        r#"[{"symbol":"SOL_USDC_PERP","type":"limit","side":"buy","amount":1.234,"price":140.123,"clientOrderId":"4294967295"}]"#,
    );
    let r = requests(&mut e).pop().unwrap();
    let body = r.body.as_deref().unwrap();
    assert!(body.contains("\"clientId\":4294967295"));
    assert!(body.contains("\"quantity\":\"1.23\""));
    assert!(body.contains("\"price\":\"140.12\""));
    assert!(r.url.ends_with("/order"));
    reply(&mut e, &r, 200, r#"{"id":"new"}"#);
    assert_eq!(done(&mut e).unwrap(), r#"["new"]"#);
}
#[test]
fn duplicate_client_ids_and_bracket_client_id_fail_closed() {
    let (mut e, _) = ready();
    for orders in [
        r#"[{"symbol":"SOL_USDC_PERP","type":"limit","side":"buy","amount":1,"price":140,"clientOrderId":"42"},{"symbol":"SOL_USDC_PERP","type":"limit","side":"buy","amount":1,"price":140,"clientOrderId":"42"}]"#,
        r#"[{"symbol":"SOL_USDC_PERP","type":"limit","side":"buy","amount":1,"price":140,"takeProfit":150,"clientOrderId":"42"}]"#,
    ] {
        place(&mut e, 1, orders);
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
        );
    }
}
#[test]
fn limit_bracket_waits_for_entry_and_keeps_a_successful_leg_if_other_leg_fails() {
    let (mut e, _) = ready();
    place(
        &mut e,
        1,
        r#"[{"symbol":"SOL_USDC_PERP","type":"limit","side":"buy","amount":1,"price":140,"takeProfit":150,"stopLoss":130}]"#,
    );
    let r = requests(&mut e).pop().unwrap();
    assert!(!r.body.as_deref().unwrap().contains("TriggerPrice"));
    reply(&mut e, &r, 200, r#"{"id":"entry"}"#);
    let tp = requests(&mut e).pop().unwrap();
    let body = tp.body.as_deref().unwrap();
    assert!(body.contains("\"side\":\"Ask\""));
    assert!(body.contains("\"triggerPrice\":\"150.00\""));
    assert!(body.contains("\"triggerBy\":\"MarkPrice\""));
    reply(&mut e, &tp, 400, r#"{"message":"rejected"}"#);
    let sl = requests(&mut e).pop().unwrap();
    assert!(
        sl.body
            .as_deref()
            .unwrap()
            .contains("\"triggerPrice\":\"130.00\"")
    );
    reply(&mut e, &sl, 200, r#"{"id":"stop"}"#);
    assert_eq!(done(&mut e).unwrap(), r#"["entry","stop"]"#);
}
#[test]
fn market_attached_brackets_and_standalone_trigger_payloads() {
    let (mut e, _) = ready();
    place(
        &mut e,
        1,
        r#"[{"symbol":"SOL_USDC_PERP","type":"market","side":"buy","amount":1,"takeProfit":150,"stopLoss":130}]"#,
    );
    let r = requests(&mut e).pop().unwrap();
    let body = r.body.as_deref().unwrap();
    assert!(body.contains("\"takeProfitTriggerPrice\":\"150.00\""));
    assert!(body.contains("\"stopLossTriggerPrice\":\"130.00\""));
    assert!(!body.contains("LimitPrice"));
    assert!(!body.contains("timeInForce"));
    reply(&mut e, &r, 200, r#"{"id":"market"}"#);
    e.drain_actions().for_each(drop);
    place(
        &mut e,
        2,
        r#"[{"symbol":"SOL_USDC_PERP","type":"stop_market","side":"sell","amount":1,"price":130,"slTriggerType":"last_price"}]"#,
    );
    let r = requests(&mut e).pop().unwrap();
    let body = r.body.as_deref().unwrap();
    assert!(body.contains("\"triggerQuantity\":\"1.00\""));
    assert!(body.contains("\"triggerBy\":\"LastPrice\""));
    assert!(body.contains("\"reduceOnly\":true"));
    assert!(!body.contains("\"quantity\""));
}
#[test]
fn amend_validates_before_cancel_and_never_reuses_the_original_caller_id() {
    // Seed the caller's order through the real private stream path.
    let (mut e, socket) = ready();
    ws(
        &mut e,
        socket,
        "account.orderUpdate",
        r#"{"e":"orderAccepted","i":"123","c":42,"E":2000000,"X":"New","s":"SOL_USDC_PERP","o":"Limit","S":"Bid","p":"140","q":"5","z":"0","f":"GTC"}"#,
    );
    e.drain_actions().for_each(drop);
    e.handle(
        2000.0,
        Event::Command(Command::AmendOrder {
            command: 1,
            id: "123".into(),
            price: Some(0.0),
            amount: None,
        }),
    );
    assert!(done(&mut e).is_err());
    e.handle(
        2000.0,
        Event::Command(Command::AmendOrder {
            command: 2,
            id: "123".into(),
            price: Some(142.0),
            amount: Some(3.0),
        }),
    );
    let cancel = requests(&mut e).pop().unwrap();
    assert_eq!(cancel.method, Method::Delete);
    reply(
        &mut e,
        &cancel,
        200,
        r#"{"id":"123","status":"Cancelled","executedQuantity":"0"}"#,
    );
    let replace = requests(&mut e).pop().unwrap();
    assert!(!replace.body.as_deref().unwrap().contains("clientId"));
    reply(&mut e, &replace, 200, r#"{"id":"replacement"}"#);
    assert_eq!(done(&mut e).unwrap(), r#"["replacement"]"#);
    assert_eq!(
        e.store().order("123").unwrap().client_order_id.as_deref(),
        Some("42")
    );
}
#[test]
fn no_write_retry_after_lost_response_and_leverage_is_global_patch() {
    let (mut e, _) = ready();
    place(
        &mut e,
        1,
        r#"[{"symbol":"SOL_USDC_PERP","type":"market","side":"buy","amount":1}]"#,
    );
    let r = requests(&mut e).pop().unwrap();
    e.handle(
        2000.0,
        Event::HttpFailed {
            request: r.id,
            error: "response lost after acceptance",
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
    );
    e.handle(
        2000.0,
        Event::Command(Command::SetLeverage {
            command: 2,
            symbol: SYMBOL.into(),
            leverage: 20.0,
            side: None,
        }),
    );
    let r = requests(&mut e).pop().unwrap();
    assert_eq!(r.method, Method::Patch);
    reply(&mut e, &r, 200, "{}");
    assert_eq!(e.store().setting("BTC_USDC_PERP").unwrap().leverage, 20.0);
    assert_eq!(e.store().setting(SYMBOL).unwrap().leverage, 20.0);
}

#[test]
fn delayed_cancel_responses_preserve_new_fills_and_new_orders() {
    let (mut e, socket) = ready();
    let event = |id: &str, time: u32, status: &str| {
        format!(
            r#"{{"e":"orderAccepted","i":"{id}","E":{time},"X":"{status}","s":"SOL_USDC_PERP","o":"Limit","S":"Bid","p":"140","q":"5","z":"0","f":"GTC"}}"#
        )
    };
    ws(
        &mut e,
        socket,
        "account.orderUpdate",
        &event("123", 2000000, "New"),
    );
    e.drain_actions().for_each(drop);
    e.handle(
        2000.0,
        Event::Command(Command::CancelOrders {
            command: 1,
            ids: vec!["123".into()],
        }),
    );
    let r = requests(&mut e).pop().unwrap();
    ws(
        &mut e,
        socket,
        "account.orderUpdate",
        &event("123", 3000000, "Filled"),
    );
    reply(&mut e, &r, 200, "{}");
    assert_eq!(
        e.store().order("123").unwrap().status,
        safe_cex_engine::model::OrderStatus::Closed
    );
    e.drain_actions().for_each(drop);
    ws(
        &mut e,
        socket,
        "account.orderUpdate",
        &event("456", 4000000, "New"),
    );
    e.drain_actions().for_each(drop);
    e.handle(
        2000.0,
        Event::Command(Command::CancelSymbolOrders {
            command: 2,
            symbol: SYMBOL.into(),
        }),
    );
    let r = requests(&mut e).pop().unwrap();
    ws(
        &mut e,
        socket,
        "account.orderUpdate",
        &event("789", 5000000, "New"),
    );
    reply(&mut e, &r, 200, "{}");
    assert_eq!(
        e.store().order("789").unwrap().status,
        safe_cex_engine::model::OrderStatus::Open
    );
    assert_eq!(
        e.store().order("123").unwrap().status,
        safe_cex_engine::model::OrderStatus::Closed
    );
    assert_eq!(
        e.store().order("456").unwrap().status,
        safe_cex_engine::model::OrderStatus::Canceled
    );
}
#[test]
fn native_pair_correlates_reordered_responses_and_rejects_ambiguous_ids() {
    let (mut e, _) = ready();
    e.store_mut()
        .update_markets(|m| m.limits.amount.max = 1000.0);
    place(
        &mut e,
        1,
        r#"[{"symbol":"SOL_USDC_PERP","type":"limit","side":"buy","amount":1,"price":140},{"symbol":"SOL_USDC_PERP","type":"limit","side":"sell","amount":2,"price":150}]"#,
    );
    let r = requests(&mut e).pop().unwrap();
    assert!(r.url.ends_with("/orders"));
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Row {
        client_id: u32,
        side: String,
    }
    let rows: Vec<Row> = serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
    assert_ne!(rows[0].client_id, rows[1].client_id);
    let row = |index: usize, id: &str| {
        format!(
            r#"{{"operation":"Ok","id":"{id}","clientId":{},"symbol":"SOL_USDC_PERP","side":"{}","orderType":"Limit"}}"#,
            rows[index].client_id, rows[index].side
        )
    };
    reply(
        &mut e,
        &r,
        200,
        &format!("[{},{}]", row(1, "second"), row(0, "first")),
    );
    assert_eq!(done(&mut e).unwrap(), r#"["first","second"]"#);
}
#[test]
fn missing_trigger_by_is_repaired_and_does_not_reopen_a_replacement_filled_before_response() {
    for quantity in ["present", "null", "missing"] {
        let (mut e, socket) = ready();
        e.handle(1000.0, Event::WsOpened { socket });
        let r = requests(&mut e)
            .into_iter()
            .find(|r| r.url.ends_with("/orders"))
            .unwrap();
        let old = ORDER.replace(
            "\"orderType\":\"Limit\"",
            "\"orderType\":\"Market\",\"triggerPrice\":\"130\",\"triggerQuantity\":\"5\"",
        );
        let old = match quantity {
            "null" => old.replace("\"quantity\":\"5\"", "\"quantity\":null"),
            "missing" => old.replace(",\"quantity\":\"5\"", ""),
            _ => old,
        };
        reply(&mut e, &r, 200, &format!("[{old}]"));
        let cancel = requests(&mut e).pop().unwrap();
        assert_eq!(cancel.method, Method::Delete);
        reply(
            &mut e,
            &cancel,
            200,
            r#"{"id":"123","status":"Cancelled","executedQuantity":"0"}"#,
        );
        let new = requests(&mut e).pop().unwrap();
        assert!(
            new.body
                .as_deref()
                .unwrap()
                .contains("\"triggerBy\":\"MarkPrice\"")
        );
        assert!(!new.body.as_deref().unwrap().contains("clientId"));
        ws(
            &mut e,
            socket,
            "account.orderUpdate",
            r#"{"e":"orderFill","i":"replacement","E":4000000,"X":"Filled","s":"SOL_USDC_PERP","o":"Market","S":"Ask","P":"130","Y":"5","q":"0","z":"5","l":"5","L":"129.5","t":999}"#,
        );
        let replacement = old
            .replace("\"id\":\"123\"", "\"id\":\"replacement\"")
            .replace(
                "\"triggerPrice\":\"130\"",
                "\"triggerPrice\":\"130\",\"triggerBy\":\"MarkPrice\"",
            );
        reply(&mut e, &new, 200, &replacement);
        assert!(e.store().order("123").is_none());
        assert_eq!(
            e.store().order("replacement").unwrap().status,
            safe_cex_engine::model::OrderStatus::Closed
        );
    }
}

#[test]
fn delayed_account_info_cannot_restore_the_previous_leverage_after_patch() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};

    use safe_cex_venue_backpack::{Backpack, Options};
    let mut e = safe_cex_engine::test_support::seeded_engine(Backpack::new(Options {
        identity: None,
        read_credentials: None,
        public_data_only: false,
        key: Some("public-test-key".into()),
        secret: Some(STANDARD.encode([7u8; 32])),
        recv_window: None,
    }));
    e.start(1000.0);
    let r = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &r,
        200,
        r#"[{"symbol":"SOL_USDC_PERP","marketType":"PERP","quantityIncrement":"0.01","priceIncrement":"0.01"}]"#,
    );
    let pending = requests(&mut e);
    let info = pending
        .iter()
        .find(|r| r.url.ends_with("/account"))
        .unwrap()
        .clone();
    for r in pending
        .iter()
        .filter(|r| r.url.contains("/tickers") || r.url.contains("/markPrices"))
    {
        reply(&mut e, r, 200, "[]");
    }
    assert!(info.url.ends_with("/account"));
    e.handle(
        2000.0,
        Event::Command(Command::SetLeverage {
            command: 1,
            symbol: SYMBOL.into(),
            leverage: 20.0,
            side: None,
        }),
    );
    let patch = requests(&mut e).pop().unwrap();
    reply(&mut e, &patch, 200, "{}");
    reply(&mut e, &info, 200, r#"{"leverageLimit":"10"}"#);
    assert_eq!(e.store().setting(SYMBOL).unwrap().leverage, 20.0);
}

#[test]
fn amend_trigger_limit_preserves_execution_price_and_trigger_source() {
    let (mut e, socket) = ready();
    ws(
        &mut e,
        socket,
        "account.orderUpdate",
        r#"{"e":"orderAccepted","i":"trigger-limit","E":2000000,"X":"New","s":"SOL_USDC_PERP","o":"Limit","S":"Bid","p":"145","q":"5","z":"0","f":"GTC","P":"150","Y":"5","B":"LastPrice"}"#,
    );
    e.drain_actions().for_each(drop);
    e.handle(
        2000.0,
        Event::Command(Command::AmendOrder {
            command: 1,
            id: "trigger-limit".into(),
            price: Some(151.0),
            amount: None,
        }),
    );
    let cancel = requests(&mut e).pop().unwrap();
    assert_eq!(cancel.method, Method::Delete);
    reply(
        &mut e,
        &cancel,
        200,
        r#"{"id":"trigger-limit","status":"Cancelled","executedQuantity":"0"}"#,
    );
    let replace = requests(&mut e).pop().unwrap();
    let body = replace.body.as_deref().unwrap();
    assert!(body.contains("\"price\":\"145.00\""), "{body}");
    assert!(body.contains("\"triggerPrice\":\"151.00\""), "{body}");
    assert!(body.contains("\"triggerBy\":\"LastPrice\""), "{body}");
}

#[test]
fn rest_and_stream_post_only_orders_remain_post_only_when_amended() {
    let snapshot = format!(
        "[{}]",
        ORDER.replace(
            "\"timeInForce\":\"GTC\"",
            "\"timeInForce\":\"GTC\",\"postOnly\":true"
        )
    );
    for rest in [false, true] {
        let (mut e, socket) = if rest {
            ready_orders(&snapshot)
        } else {
            ready()
        };
        if !rest {
            ws(
                &mut e,
                socket,
                "account.orderUpdate",
                r#"{"e":"orderAccepted","i":"123","E":2000000,"X":"New","s":"SOL_USDC_PERP","o":"Limit","S":"Bid","p":"140","q":"5","z":"0","f":"GTC","y":true}"#,
            );
        }
        assert_eq!(
            e.store().order("123").unwrap().time_in_force,
            Some(safe_cex_engine::model::TimeInForce::PostOnly)
        );
        e.drain_actions().for_each(drop);
        e.handle(
            2000.0,
            Event::Command(Command::AmendOrder {
                command: 1,
                id: "123".into(),
                price: Some(142.0),
                amount: None,
            }),
        );
        let cancel = requests(&mut e).pop().unwrap();
        reply(
            &mut e,
            &cancel,
            200,
            r#"{"id":"123","status":"Cancelled","executedQuantity":"0"}"#,
        );
        let replacement = requests(&mut e).pop().unwrap();
        assert!(
            replacement
                .body
                .as_deref()
                .unwrap()
                .contains("\"postOnly\":true")
        );
    }
}

#[test]
fn concurrent_cancel_is_rejected_until_the_first_command_completes() {
    for status in [200, 503] {
        let (mut e, _) = ready_orders(&format!("[{ORDER}]"));
        e.drain_actions().for_each(drop);
        e.handle(
            2000.0,
            Event::Command(Command::CancelOrders {
                command: 1,
                ids: vec!["123".into()],
            }),
        );
        let one = requests(&mut e).pop().unwrap();
        e.handle(
            2000.0,
            Event::Command(Command::CancelOrders {
                command: 2,
                ids: vec!["123".into()],
            }),
        );
        let out: Vec<_> = e.drain_actions().collect();
        assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(
            out.iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 2, .. })))
        );
        reply(&mut e, &one, status, "{}");
        assert_eq!(
            e.store().order("123").unwrap().status,
            if status == 200 {
                OrderStatus::Canceled
            } else {
                OrderStatus::Open
            }
        );
    }
}

#[test]
fn trigger_limit_repairs_preserve_post_only_time_in_force_and_reduce_only() {
    for (post_only, tif) in [(true, "GTC"), (false, "IOC"), (false, "FOK")] {
        let (mut e, socket) = ready();
        e.handle(1000.0, Event::WsOpened { socket });
        let read = requests(&mut e)
            .into_iter()
            .find(|r| r.url.ends_with("/orders"))
            .unwrap();
        let old = ORDER.replace("\"orderType\":\"Limit\"", "\"orderType\":\"Limit\",\"triggerPrice\":\"130\",\"triggerQuantity\":\"5\",\"reduceOnly\":false")
            .replace("\"timeInForce\":\"GTC\"", &format!("\"timeInForce\":\"{tif}\",\"postOnly\":{post_only}"));
        reply(&mut e, &read, 200, &format!("[{old}]"));
        let cancel = requests(&mut e).pop().unwrap();
        reply(
            &mut e,
            &cancel,
            200,
            r#"{"id":"123","status":"Cancelled","executedQuantity":"0"}"#,
        );
        let replacement = requests(&mut e).pop().unwrap();
        let body = replacement.body.as_deref().unwrap();
        assert!(
            body.contains(&format!("\"postOnly\":{post_only}")),
            "{body}"
        );
        assert!(
            body.contains(&format!("\"timeInForce\":\"{tif}\"")),
            "{body}"
        );
        assert!(body.contains("\"reduceOnly\":false"), "{body}");
        assert!(body.contains("\"price\":\"140.00\""), "{body}");
    }
}

#[test]
fn reconciliation_created_at_cannot_invalidate_pending_cancel_acknowledgements() {
    for all in [false, true] {
        let (mut e, socket) = ready();
        ws(
            &mut e,
            socket,
            "account.orderUpdate",
            r#"{"e":"orderAccepted","i":"123","E":2000000,"X":"New","s":"SOL_USDC_PERP","o":"Limit","S":"Bid","p":"140","q":"5","z":"0","f":"GTC"}"#,
        );
        e.drain_actions().for_each(drop);
        let command = if all {
            Command::CancelSymbolOrders {
                command: 1,
                symbol: SYMBOL.into(),
            }
        } else {
            Command::CancelOrders {
                command: 1,
                ids: vec!["123".into()],
            }
        };
        e.handle(2000.0, Event::Command(command));
        let cancel = requests(&mut e).pop().unwrap();
        e.handle(2100.0, Event::WsOpened { socket });
        let read = requests(&mut e)
            .into_iter()
            .find(|r| r.url.ends_with("/orders"))
            .unwrap();
        reply(&mut e, &read, 200, &format!("[{ORDER}]"));
        assert_eq!(e.store().order("123").unwrap().last_update, 1000.0);
        reply(
            &mut e,
            &cancel,
            200,
            r#"{"id":"123","status":"Cancelled","executedQuantity":"0"}"#,
        );
        assert_eq!(
            e.store().order("123").unwrap().status,
            OrderStatus::Canceled
        );
    }
}

#[test]
fn amend_replaces_only_the_unfilled_total_including_fills_during_cancel() {
    for (initial, later, requested, expected) in [
        ("2", "2", None, Some("3.00")),
        ("2", "3", None, Some("2")),
        ("2", "3", Some(4.0), Some("1")),
        ("2", "5", None, None),
    ] {
        let (mut e, socket) = ready_orders(&format!(
            "[{}]",
            ORDER.replace(
                "\"executedQuantity\":\"0\"",
                &format!("\"executedQuantity\":\"{initial}\"")
            )
        ));
        e.drain_actions().for_each(drop);
        e.handle(
            2000.0,
            Event::Command(Command::AmendOrder {
                command: 1,
                id: "123".into(),
                price: Some(142.0),
                amount: requested,
            }),
        );
        let cancel = requests(&mut e).pop().unwrap();
        // A stream update can beat the definitive HTTP cancellation reply.
        ws(
            &mut e,
            socket,
            "account.orderUpdate",
            &format!(
                r#"{{"e":"orderFill","i":"123","E":3000000,"X":"PartiallyFilled","s":"SOL_USDC_PERP","o":"Limit","S":"Bid","p":"140","q":"5","z":"{later}","l":"1","L":"140","t":999,"f":"GTC"}}"#
            ),
        );
        requests(&mut e);
        reply(
            &mut e,
            &cancel,
            200,
            &format!(r#"{{"id":"123","status":"Cancelled","executedQuantity":"{later}"}}"#),
        );
        let out: Vec<_> = e.drain_actions().collect();
        let writes: Vec<_> = out
            .iter()
            .filter_map(|a| match a {
                Action::Http(r) if r.method == Method::Post => Some(r),
                _ => None,
            })
            .collect();
        if let Some(expected) = expected {
            assert_eq!(writes.len(), 1);
            let value: serde_json::Value =
                serde_json::from_str(writes[0].body.as_deref().unwrap()).unwrap();
            assert_eq!(value["quantity"], expected);
        } else {
            assert!(
                writes.is_empty(),
                "a fully executed order must not be replaced"
            );
            assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandDone { result: value, .. }) if value == "[]")));
        }
    }
}

#[test]
fn amend_does_not_replace_after_an_ambiguous_cancel_ack() {
    for ack in [
        "{}",
        r#"{"id":"other","status":"Cancelled","executedQuantity":"0"}"#,
        r#"{"id":"123","status":"New","executedQuantity":"0"}"#,
    ] {
        let (mut e, _) = ready_orders(&format!("[{ORDER}]"));
        e.drain_actions().for_each(drop);
        e.handle(
            2000.0,
            Event::Command(Command::AmendOrder {
                command: 1,
                id: "123".into(),
                price: Some(142.0),
                amount: None,
            }),
        );
        let cancel = requests(&mut e).pop().unwrap();
        reply(&mut e, &cancel, 200, ack);
        let out: Vec<_> = e.drain_actions().collect();
        assert!(
            !out.iter()
                .any(|a| matches!(a, Action::Http(r) if r.method == Method::Post))
        );
        assert!(
            out.iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
        );
    }
}

#[test]
fn write_failures_are_classified_without_changing_their_text() {
    use safe_cex_engine::{ErrorKind, Outcome, Response};
    let market = r#"[{"symbol":"SOL_USDC_PERP","type":"market","side":"buy","amount":1}]"#;
    for (status, body, kind, outcome) in [
        (503, "unavailable", None, Some(Outcome::Unknown)),
        (
            400,
            r#"{"code":"INVALID_ORDER","message":"bad"}"#,
            Some(ErrorKind::Rejected),
            Some(Outcome::Rejected),
        ),
        (
            200,
            r#"{"code":"INSUFFICIENT_MARGIN","message":"margin"}"#,
            None,
            None,
        ),
        (200, r#"{"id":""}"#, None, Some(Outcome::Unknown)),
    ] {
        let (mut e, _) = ready();
        place(&mut e, 1, market);
        let r = requests(&mut e).pop().unwrap();
        e.handle(
            2000.0,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
        let error = e
            .drain_actions()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{status} {body}"));
        assert_eq!(
            (error.kind, error.outcome),
            (kind, outcome),
            "{status} {body}: {}",
            error.message
        );
    }
}

#[test]
fn malformed_repair_replacement_quantity_does_not_remove_original_identity() {
    let (mut e, socket) = ready();
    e.handle(1000.0, Event::WsOpened { socket });
    let read = requests(&mut e)
        .into_iter()
        .find(|r| r.url.ends_with("/orders"))
        .unwrap();
    let mut row: serde_json::Value = serde_json::from_str(ORDER).unwrap();
    row["quantity"] = serde_json::Value::Null;
    row["triggerPrice"] = "130".into();
    row["triggerQuantity"] = "5".into();
    reply(&mut e, &read, 200, &format!("[{row}]"));
    let cancel = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &cancel,
        200,
        r#"{"id":"123","status":"Cancelled","executedQuantity":"0"}"#,
    );
    let replacement = requests(&mut e).pop().unwrap();
    row["id"] = "replacement".into();
    row["triggerBy"] = "MarkPrice".into();
    row.as_object_mut().unwrap().remove("triggerQuantity");
    reply(&mut e, &replacement, 200, &row.to_string());
    assert!(e.store().order("123").is_some());
    assert!(e.store().order("replacement").is_none());
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a, Action::Http(r) if r.method != Method::Get))
    );
}

#[test]
fn snapshot_accepted_pair_ack_invalidates_an_older_order_read() {
    snapshot_pair_ack_fence(true, true);
}

#[test]
fn snapshot_rejected_pair_identity_does_not_invalidate_an_order_read() {
    snapshot_pair_ack_fence(false, true);
}

#[test]
fn snapshot_accepted_pair_ack_fences_native_only_missing_stored_rows() {
    snapshot_pair_ack_fence(true, false);
}

fn snapshot_pair_ack_fence(accepted: bool, stored: bool) {
    let initial = if stored {
        format!("[{ORDER}]")
    } else {
        "[]".into()
    };
    let (mut e, socket) = ready_orders(&initial);
    e.store_mut()
        .update_markets(|m| m.limits.amount.max = 1000.0);
    e.handle(1000.0, Event::WsOpened { socket });
    let old = requests(&mut e)
        .into_iter()
        .find(|r| r.url.ends_with("/orders"))
        .unwrap();
    place(
        &mut e,
        701,
        r#"[{"symbol":"SOL_USDC_PERP","type":"limit","side":"buy","amount":1,"price":140},{"symbol":"SOL_USDC_PERP","type":"limit","side":"sell","amount":1,"price":150}]"#,
    );
    let write = requests(&mut e).pop().unwrap();
    assert_eq!(write.method, Method::Post);
    assert!(write.url.ends_with("/orders"));
    #[derive(serde::Deserialize)]
    struct PairRow {
        #[serde(rename = "clientId")]
        client_id: u32,
    }
    let rows: Vec<PairRow> = serde_json::from_str(write.body.as_deref().unwrap()).unwrap();
    assert_eq!(rows.len(), 2);
    let response = format!(
        r#"[{{"operation":"Ok","id":"new-bid","clientId":{},"symbol":"SOL_USDC_PERP","side":"Bid","orderType":"Limit"}},{{"operation":"Ok","id":"new-ask","clientId":{},"symbol":"SOL_USDC_PERP","side":"{}","orderType":"Limit"}}]"#,
        rows[0].client_id,
        rows[1].client_id,
        if accepted { "Ask" } else { "Bid" }
    );
    reply(&mut e, &write, 200, &response);
    if accepted {
        assert_eq!(done(&mut e).unwrap(), r#"["new-bid","new-ask"]"#);
    } else {
        assert!(
            done(&mut e)
                .unwrap_err()
                .contains("batch response identity")
        );
    }
    assert_eq!(e.store().order("123").is_some(), stored);
    let late = if stored {
        "[]".into()
    } else {
        format!("[{ORDER}]")
    };
    reply(&mut e, &old, 200, &late);
    assert_eq!(
        e.store().order("123").is_some(),
        if accepted { stored } else { !stored },
        "only a fully accepted pair ACK supersedes the old read"
    );
}
