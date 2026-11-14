mod support;
use safe_cex_engine::model::{OrderIdentityKind, OrderStatus};
use safe_cex_engine::{Action, Command, Event, Method, Notice};
use support::managed::*;
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
    let actions = actions(&mut e);
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
    let actions = actions(&mut e);
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
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:991-1025.
#[test]
fn limit_bracket_waits_for_entry_and_halts_after_unknown_child()
-> Result<(), Box<dyn std::error::Error>> {
    let (mut e, _) = ready();
    place(
        &mut e,
        1,
        r#"[{"symbol":"SOL_USDC_PERP","type":"limit","side":"buy","amount":1,"price":140,"takeProfit":150,"stopLoss":130}]"#,
    );
    let entry = requests(&mut e).pop().ok_or("entry missing")?;
    assert!(
        !entry
            .body
            .as_deref()
            .ok_or("entry body missing")?
            .contains("TriggerPrice")
    );
    reply(&mut e, &entry, 200, r#"{"id":"entry"}"#);
    let tp = requests(&mut e).pop().ok_or("first child missing")?;
    let body = tp.body.as_deref().ok_or("first child body missing")?;
    assert!(body.contains("\"side\":\"Ask\""));
    assert!(body.contains("\"triggerPrice\":\"150.00\""));
    assert!(body.contains("\"triggerBy\":\"MarkPrice\""));
    reply(&mut e, &tp, 400, r#"{"message":"rejected"}"#);
    let out = actions(&mut e);
    assert!(!out.iter().any(|action| matches!(action, Action::Http(_))));
    let error = failure(&out);
    assert!(
        error
            .message
            .contains("The outcome is unknown. Check Open Orders")
    );
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    assert_eq!(
        error
            .partial
            .as_ref()
            .ok_or("prior accepted ID missing")?
            .order_ids,
        ["entry"]
    );
    Ok(())
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
    e.handle(1000.0, Event::WsOpened { socket });
    safe_cex_engine::test_support::prepared_actions(&mut e, 1000.0);
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
    e.handle(1000.0, Event::WsOpened { socket });
    safe_cex_engine::test_support::prepared_actions(&mut e, 1000.0);
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

    use safe_cex_venue_backpack::Options;
    let mut e = Driver::new(Options {
        read_identity: None,
        read_credentials_status: None,
        read_credentials: None,
        public_data_only: false,
        key: Some("public-test-key".into()),
        secret: Some(STANDARD.encode([7u8; 32])),
        recv_window: None,
        identity: None,
    });
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
fn documented_empty_leverage_reply_applies_the_leverage() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};

    use safe_cex_venue_backpack::Options;
    let mut e = Driver::new(Options {
        read_identity: None,
        read_credentials_status: None,
        read_credentials: None,
        public_data_only: false,
        key: Some("public-test-key".into()),
        secret: Some(STANDARD.encode([7u8; 32])),
        recv_window: None,
        identity: None,
    });
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
    // Docs: PATCH /api/v1/account answers 200 "Success." with no content.
    reply(&mut e, &patch, 200, "");
    reply(&mut e, &info, 200, r#"{"leverageLimit":"10"}"#);
    assert_eq!(e.store().setting(SYMBOL).unwrap().leverage, 20.0);
}

#[test]
fn legacy_trigger_limit_amend_uses_the_edited_price_as_a_plain_limit() {
    let (mut e, socket) = ready();
    e.handle(1000.0, Event::WsOpened { socket });
    safe_cex_engine::test_support::prepared_actions(&mut e, 1000.0);
    ws(
        &mut e,
        socket,
        "account.orderUpdate",
        r#"{"e":"orderAccepted","i":"trigger-limit","E":2000000,"X":"New","s":"SOL_USDC_PERP","o":"Limit","S":"Bid","p":"145","q":"5","z":"0","f":"GTC","P":"150","Y":"5","B":"LastPrice"}"#,
    );
    e.drain_actions().for_each(drop);
    fresh(&mut e, 140.0);
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
    assert!(body.contains("\"price\":\"151.00\""), "{body}");
    assert!(!body.contains("triggerPrice"), "{body}");
    assert!(!body.contains("triggerBy"), "{body}");
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
        e.handle(1000.0, Event::WsOpened { socket });
        safe_cex_engine::test_support::prepared_actions(&mut e, 1000.0);
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
                OrderStatus::Canceling
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
        e.handle(1000.0, Event::WsOpened { socket });
        safe_cex_engine::test_support::prepared_actions(&mut e, 1000.0);
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
        e.handle(2100.0, Event::WsClosed { socket, code: 1006 });
        let (timer, at) = e
            .drain_actions()
            .find_map(|action| match action {
                Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
                _ => None,
            })
            .expect("actual private reconnect timer");
        e.handle(at, Event::TimerFired { timer });
        let replacement = e
            .drain_actions()
            .find_map(|action| match action {
                Action::WsConnect {
                    socket,
                    channel: Some(safe_cex_engine::SocketChannel::Private),
                    ..
                } => Some(socket),
                _ => None,
            })
            .expect("actual replacement private socket");
        e.handle(
            at,
            Event::WsOpened {
                socket: replacement,
            },
        );
        let read = safe_cex_engine::test_support::prepared_actions(&mut e, at)
            .into_iter()
            .find_map(|action| match action {
                Action::Http(request) if request.url.ends_with("/orders") => Some(request),
                _ => None,
            })
            .unwrap();
        e.handle(
            at,
            Event::HttpResponse {
                request: read.id,
                response: safe_cex_engine::Response {
                    status: 200,
                    headers: "",
                    body: &format!("[{ORDER}]"),
                },
            },
        );
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
fn legacy_amend_keeps_the_requested_total_when_stream_fills_change()
-> Result<(), Box<dyn std::error::Error>> {
    for (later, requested, expected) in [
        ("2", None, "5.00"),
        ("3", None, "5.00"),
        ("3", Some(4.), "4.00"),
        ("5", None, "5.00"),
    ] {
        let old = ORDER.replace(r#""executedQuantity":"0""#, r#""executedQuantity":"2""#);
        let (mut e, socket) = ready_orders(&format!("[{old}]"));
        e.handle(1000., Event::WsOpened { socket });
        actions(&mut e);
        e.handle(
            2000.,
            Event::Command(Command::AmendOrder {
                command: 1,
                id: "123".into(),
                price: Some(142.),
                amount: requested,
            }),
        );
        let cancel = requests(&mut e).pop().ok_or("cancel missing")?;
        ws(
            &mut e,
            socket,
            "account.orderUpdate",
            &format!(
                r#"{{"e":"orderFill","i":"123","E":3000000,"X":"PartiallyFilled","s":"SOL_USDC_PERP","o":"Limit","S":"Bid","p":"140","q":"5","z":"{later}","l":"1","L":"140","t":999,"f":"GTC"}}"#
            ),
        );
        actions(&mut e);
        reply(
            &mut e,
            &cancel,
            200,
            &format!(r#"{{"id":"123","status":"Cancelled","executedQuantity":"{later}"}}"#),
        );
        let replacement = requests(&mut e).pop().ok_or("legacy replacement missing")?;
        let row: serde_json::Value =
            serde_json::from_str(replacement.body.as_deref().ok_or("missing body")?)?;
        assert_eq!(row["quantity"], expected);
        assert_eq!(row["price"], "142.00");
    }
    Ok(())
}

#[test]
fn legacy_amend_continues_after_successful_cancel_http_response()
-> Result<(), Box<dyn std::error::Error>> {
    for ack in [
        "{}",
        r#"{"id":"other","status":"Cancelled","executedQuantity":"0"}"#,
        r#"{"id":"123","status":"New","executedQuantity":"0"}"#,
    ] {
        let (mut e, _) = ready_orders(&format!("[{ORDER}]"));
        e.handle(
            2000.,
            Event::Command(Command::AmendOrder {
                command: 1,
                id: "123".into(),
                price: Some(142.),
                amount: None,
            }),
        );
        let cancel = requests(&mut e).pop().ok_or("cancel missing")?;
        reply(&mut e, &cancel, 200, ack);
        let replacement = requests(&mut e).pop().ok_or("replacement missing")?;
        assert_eq!(replacement.method, Method::Post);
        let row: serde_json::Value =
            serde_json::from_str(replacement.body.as_deref().ok_or("missing body")?)?;
        assert_eq!(row["quantity"], "5.00");
        assert_eq!(row["price"], "142.00");
    }
    Ok(())
}

#[test]
fn an_unclassified_limit_replacement_failure_warns_its_outcome_is_unknown()
-> Result<(), Box<dyn std::error::Error>> {
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
    let cancel = requests(&mut e).pop().ok_or("cancel missing")?;
    reply(
        &mut e,
        &cancel,
        200,
        r#"{"id":"123","status":"Cancelled","executedQuantity":"0"}"#,
    );
    let place = actions(&mut e)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.method == Method::Post => Some(r),
            _ => None,
        })
        .ok_or("replacement missing")?;
    reply(
        &mut e,
        &place,
        400,
        r#"{"code":"INVALID_ORDER","message":"bad"}"#,
    );
    let out = actions(&mut e);
    assert!(!out.iter().any(|action| matches!(action, Action::Http(_))));
    let error = out
        .into_iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
            _ => None,
        })
        .ok_or("failure missing")?;
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    assert_eq!(error.venue_code.as_deref(), Some("INVALID_ORDER"));
    assert_eq!(error.venue_message.as_deref(), Some("bad"));
    assert!(
        error
            .message
            .contains("Your SOL_USDC_PERP order may be gone")
            && error.message.contains("Check Open Orders"),
        "{}",
        error.message
    );
    Ok(())
}

/// Prices observed now, as a trigger edit's final check needs.
fn fresh(e: &mut Driver, price: f64) {
    e.store_mut().update_ticker(SYMBOL, |t| {
        t.last = price;
        t.mark = price;
        t.index = price;
    });
}
/// A reduce-only sell stop at 130 guarding the fixture's long, the mark at 140.
fn resting_stop() -> Driver {
    resting_stop_with_socket().0
}
fn resting_stop_with_socket() -> (Driver, safe_cex_engine::SocketId) {
    let stop = ORDER
        .replace("\"orderType\":\"Limit\"", "\"orderType\":\"Market\",\"triggerPrice\":\"130\",\"triggerQuantity\":\"5\",\"reduceOnly\":true")
        .replace("\"side\":\"Bid\"", "\"side\":\"Ask\"");
    let (mut e, socket) = ready_orders(&format!("[{stop}]"));
    e.drain_actions().for_each(drop);
    assert_eq!(
        e.store().order("123").unwrap().kind,
        safe_cex_engine::model::OrderType::StopLoss
    );
    fresh(&mut e, 140.0);
    (e, socket)
}
fn amend_stop(e: &mut Driver, price: f64) {
    e.handle(
        2000.0,
        Event::Command(Command::AmendOrder {
            command: 1,
            id: "123".into(),
            price: Some(price),
            amount: None,
        }),
    );
}
fn posts(out: &[Action]) -> Vec<safe_cex_engine::HttpRequest> {
    out.iter()
        .filter_map(|a| match a {
            Action::Http(r) if r.method != Method::Get => Some(r.clone()),
            _ => None,
        })
        .collect()
}
fn failure(out: &[Action]) -> safe_cex_engine::VenueError {
    out.iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 1, error }) => Some(error.clone()),
            _ => None,
        })
        .expect("a failure")
}
/// The stop's cancel lands untouched; its replacement is the next write.
fn to_replacement(e: &mut Driver) -> safe_cex_engine::HttpRequest {
    amend_stop(e, 125.0);
    let cancel = requests(e).pop().unwrap();
    assert_eq!(cancel.method, Method::Delete);
    reply(
        e,
        &cancel,
        200,
        r#"{"id":"123","status":"Cancelled","executedQuantity":"0"}"#,
    );
    let place = posts(&actions(e)).pop().expect("the replacement");
    assert!(
        place
            .body
            .as_deref()
            .unwrap()
            .contains("\"triggerPrice\":\"125")
    );
    place
}
const REFUSED: &str = r#"{"code":"INVALID_ORDER","message":"bad"}"#;
fn assert_failed_stop(out: &[Action]) -> Result<(), Box<dyn std::error::Error>> {
    use safe_cex_engine::write_plan::{AppliedEffect, ElementResult};
    let error = failure(out);
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    let elements = error.elements.as_ref().ok_or("missing stop outcomes")?;
    assert_eq!(elements.len(), 2);
    assert!(
        matches!(&elements[0].result, ElementResult::Applied { effect: AppliedEffect::Cancel, target_order_id: Some(id), .. } if id == "123")
    );
    assert!(
        matches!(&elements[1].result, ElementResult::Unknown { error } if error.outcome == Some(safe_cex_engine::Outcome::Unknown))
    );
    assert!(
        error
            .partial
            .as_ref()
            .is_none_or(|partial| partial.order_ids.is_empty())
    );
    assert!(error.message.contains("Check Open Orders"));
    assert!(!out.iter().any(|action| matches!(action, Action::Http(_))));
    Ok(())
}

#[test]
fn an_unclassified_stop_failure_stops_without_proof_or_put_back()
-> Result<(), Box<dyn std::error::Error>> {
    let mut e = resting_stop();
    let replacement = to_replacement(&mut e);
    reply(&mut e, &replacement, 200, REFUSED);
    assert_failed_stop(&actions(&mut e))?;
    assert!(requests(&mut e).is_empty());
    Ok(())
}

#[test]
fn a_late_success_after_stop_failure_cannot_restart_the_plan()
-> Result<(), Box<dyn std::error::Error>> {
    let mut e = resting_stop();
    let replacement = to_replacement(&mut e);
    reply(&mut e, &replacement, 200, REFUSED);
    assert_failed_stop(&actions(&mut e))?;
    reply(&mut e, &replacement, 200, r#"{"id":"late"}"#);
    let out = actions(&mut e);
    assert!(!out.iter().any(|action| matches!(
        action,
        Action::Http(_)
            | Action::Notice(
                Notice::CommandDone { command: 1, .. } | Notice::CommandFailed { command: 1, .. }
            )
    )));
    Ok(())
}

#[test]
fn a_resting_stream_row_after_stop_failure_cannot_trigger_put_back()
-> Result<(), Box<dyn std::error::Error>> {
    let (mut e, socket) = resting_stop_with_socket();
    let replacement = to_replacement(&mut e);
    reply(&mut e, &replacement, 200, REFUSED);
    assert_failed_stop(&actions(&mut e))?;
    ws(
        &mut e,
        socket,
        "account.orderUpdate",
        r#"{"e":"orderAccepted","i":"new","E":3000000,"X":"New","s":"SOL_USDC_PERP","o":"Market","S":"Ask","P":"125","Y":"5","B":"MarkPrice","r":true}"#,
    );
    assert!(posts(&actions(&mut e)).is_empty());
    Ok(())
}

#[test]
fn a_late_transport_failure_after_stop_failure_is_inert() -> Result<(), Box<dyn std::error::Error>>
{
    let mut e = resting_stop();
    let replacement = to_replacement(&mut e);
    reply(&mut e, &replacement, 200, REFUSED);
    assert_failed_stop(&actions(&mut e))?;
    e.handle(
        2000.,
        Event::HttpFailed {
            request: replacement.id,
            error: "late loss",
        },
    );
    let out = actions(&mut e);
    assert!(!out.iter().any(|action| matches!(
        action,
        Action::Http(_)
            | Action::Notice(
                Notice::CommandDone { command: 1, .. } | Notice::CommandFailed { command: 1, .. }
            )
    )));
    Ok(())
}

#[test]
fn an_unknown_replacement_is_never_read_or_put_back() {
    for status in [500, 408, 409, 429] {
        let mut e = resting_stop();
        let place = to_replacement(&mut e);
        reply(&mut e, &place, status, REFUSED);
        let out = actions(&mut e);
        let error = failure(&out);
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
        assert!(error.message.contains("outcome is unknown"));
        assert!(error.message.contains("Check Open Orders"));
        assert!(!error.message.to_ascii_lowercase().contains("rejected"));
        assert_eq!(error.protection, None, "{status}");
        assert!(
            !out.iter().any(|a| matches!(a, Action::Http(r) if r.method == Method::Get && r.url.contains("/api/v1/orders?"))),
            "{status}: no proof read"
        );
        assert!(posts(&out).is_empty(), "{status}");
    }
}

#[test]
fn legacy_stop_amend_keeps_its_quantity_after_a_filled_cancel_reply()
-> Result<(), Box<dyn std::error::Error>> {
    let mut e = resting_stop();
    amend_stop(&mut e, 125.);
    let cancel = requests(&mut e).pop().ok_or("cancel missing")?;
    reply(
        &mut e,
        &cancel,
        200,
        r#"{"id":"123","status":"Filled","executedQuantity":"5"}"#,
    );
    let replacement = requests(&mut e).pop().ok_or("legacy replacement missing")?;
    let row: serde_json::Value =
        serde_json::from_str(replacement.body.as_deref().ok_or("missing body")?)?;
    assert_eq!(row["triggerQuantity"], "5.00");
    assert_eq!(row["triggerPrice"], "125.00");
    Ok(())
}

#[test]
fn legacy_stop_amend_has_no_extra_final_reference_or_position_gate()
-> Result<(), Box<dyn std::error::Error>> {
    for case in ["stale", "crossed", "changed", "flat"] {
        let mut e = resting_stop();
        amend_stop(&mut e, 125.);
        let at = match case {
            "stale" => 8000.,
            "crossed" => {
                e.store_mut().update_ticker(SYMBOL, |t| t.mark = 124.);
                2000.
            }
            "changed" => {
                e.store_mut().update_order("123", |o| o.filled = 1.);
                2000.
            }
            _ => {
                e.store_mut().set_positions(vec![]);
                2000.
            }
        };
        let out = safe_cex_engine::test_support::prepared_actions(&mut e, at);
        let writes = posts(&out);
        assert_eq!(writes.len(), 1, "{case}");
        assert_eq!(writes[0].method, Method::Delete, "{case}");
        let row: serde_json::Value =
            serde_json::from_str(writes[0].body.as_deref().ok_or("missing cancel")?)?;
        assert_eq!(row["orderId"], "123", "{case}");
        assert!(!out.iter().any(|action| matches!(
            action,
            Action::Notice(Notice::CommandFailed { command: 1, .. })
        )));
    }
    Ok(())
}

#[test]
fn a_trigger_veto_takes_its_direction_from_planning_not_last_price() {
    // A row with no triggerBy is classified on last but triggers on mark: last
    // moving through the new stop after admission does not refuse its cancel.
    let mut e = resting_stop();
    amend_stop(&mut e, 125.0);
    e.store_mut().update_ticker(SYMBOL, |t| t.last = 124.0);
    let out = safe_cex_engine::test_support::prepared_actions(&mut e, 2000.0);
    assert!(
        out.iter()
            .any(|a| matches!(a, Action::Http(r) if r.method == Method::Delete)),
        "the cancel is checked on the mark it triggers on"
    );
}

#[test]
fn write_failures_keep_classification_and_unknown_guidance() {
    use safe_cex_engine::{ErrorKind, Outcome, Response};
    let market = r#"[{"symbol":"SOL_USDC_PERP","type":"market","side":"buy","amount":1}]"#;
    for (status, body, kind, outcome) in [
        (
            503,
            "unavailable",
            Some(ErrorKind::VenueUnavailable),
            Some(Outcome::Unknown),
        ),
        (
            400,
            r#"{"code":"INVALID_ORDER","message":"bad"}"#,
            None,
            Some(Outcome::Unknown),
        ),
        (
            200,
            r#"{"code":"INSUFFICIENT_MARGIN","message":"margin"}"#,
            None,
            Some(Outcome::Unknown),
        ),
        (
            200,
            r#"{"id":""}"#,
            Some(ErrorKind::Internal),
            Some(Outcome::Unknown),
        ),
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
        assert_eq!(error.message, "The outcome is unknown. Check Open Orders");
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
        let out = actions(&mut e);
        let Some(error) = out.iter().find_map(|action| match action {
            Action::Notice(Notice::CommandFailed {
                command: 701,
                error,
            }) => Some(error),
            _ => None,
        }) else {
            panic!("missing uncertain batch failure");
        };
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
        assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::Internal));
        assert!(
            error
                .message
                .contains("The outcome is unknown. Check Open Orders")
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

#[test]
#[ignore = "Backpack account/background Plan and signer adoption (codex-hcmq3f), after #1946"]
fn legacy_account_repair_must_adopt_dispatch_signing_before_account_cutover() {
    let (mut e, socket) = ready();
    e.handle(1000., Event::WsOpened { socket });
    let read = requests(&mut e)
        .into_iter()
        .find(|request| request.url.ends_with("/orders"))
        .unwrap();
    let old = ORDER.replace(
        "\"orderType\":\"Limit\"",
        "\"orderType\":\"Market\",\"triggerPrice\":\"130\",\"triggerQuantity\":\"5\"",
    );
    reply(&mut e, &read, 200, &format!("[{old}]"));
    let out = e.drain_actions().collect::<Vec<_>>();
    assert!(out.iter().any(|action| matches!(action,
        Action::ManagedHttp(action) if matches!(action.as_ref(),
            safe_cex_engine::request_context::HttpAction::Prepare { request, .. }
            if request.method == Method::Delete
                && request.url == "https://api.backpack.exchange/api/v1/order"
                && request.body.as_deref() == Some(r#"{"orderId":"123","symbol":"SOL_USDC_PERP"}"#)))),
        "account repair must use shared dispatch signing before its cutover");
    assert!(
        !out.iter()
            .any(|action| matches!(action, Action::Http(request)
        if request.method == Method::Delete))
    );
}

/// Unlisted native codes remain unknown on ordinary and trigger replacements.
#[test]
fn unlisted_ordinary_4xx_and_trigger_200_errors_stay_unknown()
-> Result<(), Box<dyn std::error::Error>> {
    let (mut e, _) = ready();
    place(
        &mut e,
        1,
        r#"[{"symbol":"SOL_USDC_PERP","type":"limit","side":"buy","amount":1,"price":140}]"#,
    );
    let post = posts(&actions(&mut e)).pop().ok_or("placement missing")?;
    reply(&mut e, &post, 400, REFUSED);
    assert_eq!(
        failure(&actions(&mut e)).outcome,
        Some(safe_cex_engine::Outcome::Unknown)
    );
    let mut e = resting_stop();
    let replacement = to_replacement(&mut e);
    reply(&mut e, &replacement, 200, REFUSED);
    assert_failed_stop(&actions(&mut e))?;
    Ok(())
}

/// Concurrent order updates cannot replay or retire an admitted replacement.
#[test]
fn repair_never_retires_an_admitted_replacement_in_flight() -> Result<(), Box<dyn std::error::Error>>
{
    let (mut e, socket) = resting_stop_with_socket();
    let replacement = to_replacement(&mut e);
    for update in [
        r#"{"e":"orderCancelled","i":"123","E":3000000,"X":"Cancelled","s":"SOL_USDC_PERP","o":"Market","S":"Ask","q":"5","z":"0","P":"130","Y":"5","r":true}"#,
        r#"{"e":"orderAccepted","i":"other","E":3000001,"X":"New","s":"SOL_USDC_PERP","o":"Limit","S":"Bid","p":"100","q":"1","z":"0","f":"GTC"}"#,
    ] {
        ws(&mut e, socket, "account.orderUpdate", update);
    }
    assert!(posts(&actions(&mut e)).is_empty());
    reply(
        &mut e,
        &replacement,
        200,
        r#"{"id":"777","status":"New","orderType":"Market","side":"Ask"}"#,
    );
    let out = actions(&mut e);
    assert!(out.iter().any(|action| matches!(action, Action::Notice(Notice::CommandDone { command: 1, result }) if result == r#"["777"]"#)));
    assert!(posts(&out).is_empty());
    Ok(())
}

/// New fills veto the immutable replacement before signing or transport.
#[test]
fn legacy_stop_amend_preserves_its_requested_quantity_after_cancel_fills()
-> Result<(), Box<dyn std::error::Error>> {
    let mut e = resting_stop();
    amend_stop(&mut e, 125.);
    let cancel = requests(&mut e).pop().ok_or("cancel missing")?;
    reply(
        &mut e,
        &cancel,
        200,
        r#"{"id":"123","status":"Cancelled","executedQuantity":"1"}"#,
    );
    let replacement = requests(&mut e).pop().ok_or("legacy replacement missing")?;
    let row: serde_json::Value =
        serde_json::from_str(replacement.body.as_deref().ok_or("missing body")?)?;
    assert_eq!(row["triggerQuantity"], "5.00");
    assert_eq!(row["triggerPrice"], "125.00");
    Ok(())
}

/// Lost and unproven replacement replies remain unknown and terminal.
#[test]
fn an_unknown_stop_replacement_reports_applied_cancel_and_unknown_creation()
-> Result<(), Box<dyn std::error::Error>> {
    for status in [None, Some(429), Some(408), Some(500)] {
        let mut e = resting_stop();
        let replacement = to_replacement(&mut e);
        match status {
            Some(status) => reply(&mut e, &replacement, status, ""),
            None => e.handle(
                2000.,
                Event::HttpFailed {
                    request: replacement.id,
                    error: "lost reply",
                },
            ),
        }
        assert_failed_stop(&actions(&mut e))?;
        assert!(requests(&mut e).is_empty());
    }
    Ok(())
}

fn refused_then_read(e: &mut Driver) -> safe_cex_engine::HttpRequest {
    let place = to_replacement(e);
    reply(e, &place, 400, REFUSED);
    let out = actions(e);
    assert!(posts(&out).is_empty(), "no put-back before the read");
    out.into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.method == Method::Get && r.url.contains("/api/v1/orders") => {
                Some(r)
            }
            _ => None,
        })
        .expect("an open-orders proof read")
}

#[test]
fn a_refused_stop_replacement_with_a_clean_read_is_put_back_once() {
    use safe_cex_engine::error::Protection;
    let mut e = resting_stop();
    let read = refused_then_read(&mut e);
    reply(&mut e, &read, 200, "[]");
    let restore = posts(&actions(&mut e)).pop().expect("the put-back");
    let body = restore.body.as_deref().unwrap();
    for expected in [
        r#""triggerPrice":"130.00"#,
        r#""triggerQuantity":"5.00"#,
        r#""side":"Ask""#,
        r#""reduceOnly":true"#,
        r#""triggerBy":"MarkPrice""#,
    ] {
        assert!(
            body.contains(expected),
            "the original, not the edit: {body}"
        );
    }
    reply(
        &mut e,
        &restore,
        200,
        r#"{"id":"777","status":"New","orderType":"Market","side":"Ask"}"#,
    );
    let out = actions(&mut e);
    let error = failure(&out);
    assert!(
        error.message.contains("is unchanged") && error.message.contains("put back"),
        "{}",
        error.message
    );
    assert_eq!(error.protection, Some(Protection::Restored));
    assert!(posts(&out).is_empty(), "one put-back only");
}

#[test]
fn a_refused_put_back_is_unprotected_and_never_retried() {
    use safe_cex_engine::error::Protection;
    let mut e = resting_stop();
    let read = refused_then_read(&mut e);
    reply(&mut e, &read, 200, "[]");
    let restore = posts(&actions(&mut e)).pop().expect("the put-back");
    reply(&mut e, &restore, 400, REFUSED);
    let out = actions(&mut e);
    let error = failure(&out);
    assert!(error.message.contains("both refused"), "{}", error.message);
    assert_eq!(error.protection, Some(Protection::Unprotected));
    assert!(posts(&out).is_empty());
}

#[test]
fn a_refused_replacement_still_resting_is_never_put_back() {
    let mut e = resting_stop();
    let read = refused_then_read(&mut e);
    reply(
        &mut e,
        &read,
        200,
        r#"[{"id":"9","symbol":"SOL_USDC_PERP","side":"Ask","orderType":"Market","triggerPrice":"125","triggerQuantity":"5","status":"TriggerPending"}]"#,
    );
    let out = actions(&mut e);
    let error = failure(&out);
    assert!(
        error
            .message
            .contains("Your SOL_USDC_PERP long position may have no stop")
            && error.message.contains("Check Open Orders"),
        "{}",
        error.message
    );
    assert_eq!(error.protection, None);
    assert!(
        posts(&out).is_empty(),
        "no put-back over a resting replacement"
    );
}

#[test]
fn a_lost_proof_read_is_unprotected_with_no_put_back() {
    use safe_cex_engine::error::Protection;
    let mut e = resting_stop();
    let read = refused_then_read(&mut e);
    e.handle(
        1000.0,
        Event::HttpFailed {
            request: read.id,
            error: "lost proof read",
        },
    );
    let out = actions(&mut e);
    let error = failure(&out);
    assert!(
        error.message.contains("was removed") && error.message.contains("please re-place it"),
        "{}",
        error.message
    );
    assert_eq!(error.protection, Some(Protection::Unprotected));
    assert!(posts(&out).is_empty());
}

#[test]
fn a_stop_that_filled_during_its_cancel_is_not_put_back() {
    use safe_cex_engine::error::Protection;
    let mut e = resting_stop();
    amend_stop(&mut e, 125.0);
    let cancel = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &cancel,
        200,
        r#"{"id":"123","status":"Cancelled","executedQuantity":"1"}"#,
    );
    let place = posts(&actions(&mut e)).pop().expect("the replacement");
    reply(&mut e, &place, 400, REFUSED);
    let out = actions(&mut e);
    let error = failure(&out);
    assert!(
        error.message.contains("was removed") && error.message.contains("please re-place it"),
        "{}",
        error.message
    );
    assert_eq!(error.protection, Some(Protection::Unprotected));
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::Http(r) if r.method == Method::Get)),
        "no proof read"
    );
    assert!(posts(&out).is_empty());
}

#[test]
fn an_unknown_put_back_says_check_open_orders_never_re_place() {
    for case in ["429", "408", "500", "lost"] {
        let mut e = resting_stop();
        let read = refused_then_read(&mut e);
        reply(&mut e, &read, 200, "[]");
        let restore = posts(&actions(&mut e)).pop().expect("the put-back");
        match case {
            "408" => reply(&mut e, &restore, 408, ""),
            "500" => reply(
                &mut e,
                &restore,
                500,
                r#"{"code":"INTERNAL_ERROR","message":"oops"}"#,
            ),
            "429" => reply(
                &mut e,
                &restore,
                429,
                r#"{"code":"TOO_MANY_REQUESTS","message":"slow"}"#,
            ),
            _ => e.handle(
                1000.0,
                Event::HttpFailed {
                    request: restore.id,
                    error: "lost put-back response",
                },
            ),
        }
        let out = actions(&mut e);
        let error = failure(&out);
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
        assert!(
            error.message.contains("put-back's outcome is unknown")
                && error.message.contains("Check Open Orders")
                && !error.message.contains("re-place")
                && !error.message.contains("INVALID_ORDER"),
            "{case}: {}",
            error.message
        );
        assert_eq!(error.protection, None, "{case}");
        assert!(posts(&out).is_empty(), "{case}: never resent");
    }
}

#[test]
fn repair_never_retires_a_put_back_in_flight() {
    use safe_cex_engine::error::Protection;
    let (mut e, socket) = resting_stop_with_socket();
    let read = refused_then_read(&mut e);
    reply(&mut e, &read, 200, "[]");
    let restore = posts(&actions(&mut e)).pop().expect("the put-back");
    // The original's cancel and an unrelated order's update both run repair.
    for update in [
        r#"{"e":"orderCancelled","i":"123","E":3000000,"X":"Cancelled","s":"SOL_USDC_PERP","o":"Market","S":"Ask","q":"5","z":"0","P":"130","Y":"5","r":true}"#,
        r#"{"e":"orderAccepted","i":"other","E":3000001,"X":"New","s":"SOL_USDC_PERP","o":"Limit","S":"Bid","p":"100","q":"1","z":"0","f":"GTC"}"#,
    ] {
        ws(&mut e, socket, "account.orderUpdate", update);
    }
    let out = actions(&mut e);
    assert!(posts(&out).is_empty(), "nothing re-sent or cancelled");
    reply(
        &mut e,
        &restore,
        200,
        r#"{"id":"777","status":"New","orderType":"Market","side":"Ask"}"#,
    );
    let error = failure(&actions(&mut e));
    assert_eq!(
        error.protection,
        Some(Protection::Restored),
        "the put-back's answer still lands: {}",
        error.message
    );
}

#[test]
fn rounded_original_quantity_does_not_gate_guarded_amend_or_put_back()
-> Result<(), Box<dyn std::error::Error>> {
    // Keep the existing guarded put-back; TS reference/backpack/backpack.exchange.ts:918-925
    // formats quantities without rechecking rounded bounds.
    let mut e = resting_stop();
    e.store_mut().update_markets(|market| {
        market.precision.amount = 0.1;
        market.limits.amount.max = 0.95;
    });
    let mut original = e.store().order("123").cloned().ok_or("missing original")?;
    original.amount = 0.95;
    original.remaining = 0.95;
    e.store_mut().upsert_order(original);
    e.handle(
        2000.,
        Event::Command(Command::AmendOrder {
            command: 1,
            id: "123".into(),
            price: Some(125.),
            amount: Some(0.4),
        }),
    );
    let cancel = requests(&mut e).pop().ok_or("missing guarded cancel")?;
    assert_eq!(cancel.method, Method::Delete);
    reply(
        &mut e,
        &cancel,
        200,
        r#"{"id":"123","status":"Cancelled","executedQuantity":"0"}"#,
    );
    let replacement = posts(&actions(&mut e)).pop().ok_or("missing replacement")?;
    let row: serde_json::Value =
        serde_json::from_str(replacement.body.as_deref().ok_or("missing body")?)?;
    assert_eq!(row["triggerQuantity"], "0.4");
    reply(&mut e, &replacement, 400, REFUSED);
    let read = requests(&mut e)
        .into_iter()
        .find(|r| r.method == Method::Get && r.url.contains("/api/v1/orders"))
        .ok_or("missing proof read")?;
    reply(&mut e, &read, 200, "[]");
    let restore = posts(&actions(&mut e)).pop().ok_or("missing put-back")?;
    let row: serde_json::Value =
        serde_json::from_str(restore.body.as_deref().ok_or("missing body")?)?;
    assert_eq!(row["triggerQuantity"], "1.0");
    assert_eq!(row["triggerPrice"], "130.00");
    reply(
        &mut e,
        &restore,
        200,
        r#"{"id":"777","status":"New","orderType":"Market","side":"Ask"}"#,
    );
    let out = actions(&mut e);
    assert_eq!(
        failure(&out).protection,
        Some(safe_cex_engine::error::Protection::Restored)
    );
    assert!(posts(&out).is_empty());
    Ok(())
}
