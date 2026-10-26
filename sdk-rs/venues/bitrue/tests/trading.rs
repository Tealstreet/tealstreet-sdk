//! Scripted orders from Bitrue's documented v2 futures payloads.
mod support;
use safe_cex_engine::model::*;
use safe_cex_engine::{Command, Event, HttpRequest, Method, Notice, Response};
use support::trading::*;
#[test]
fn normal_amount_stays_base_and_caller_id_is_verbatim() {
    let mut h = Harness::new();
    h.place(r#"[{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.003,"price":2000.04,"clientOrderId":"space / punctuation allowed"}]"#);
    let request = h.take(Method::Post, "/fapi/v2/order");
    let row = body(&request);
    assert_eq!(row["volume"], "0.003");
    assert_eq!(row["price"], 2000.0);
    assert_eq!(row["clientOrderId"], "space / punctuation allowed");
    assert_eq!(row["leverage"], 20.0);
    assert_eq!(row["positionType"], 1);
    assert_eq!(row["open"], "OPEN");
    h.reply(
        request,
        r#"{"code":"0","data":{"orderId":2334151155877155281}}"#,
    );
    assert_eq!(h.result(), Some(r#"["2334151155877155281"]"#));
}
#[test]
fn market_open_is_quote_with_step_buffer_and_minimum_but_reduce_is_base() {
    let mut h = Harness::new();
    h.place(r#"[{"symbol":"E-BTC-USDT","type":"market","side":"buy","amount":0.001}]"#);
    let request = h.take(Method::Post, "/fapi/v2/order");
    let row = body(&request);
    assert_eq!(row["amount"], 5.0);
    assert!(row.get("volume").is_none());
    h.reply(request, r#"{"code":"0","data":{"orderId":1}}"#);
    h.place(r#"[{"symbol":"E-BTC-USDT","type":"market","side":"sell","amount":0.001,"reduceOnly":true}]"#);
    let row = body(&h.take(Method::Post, "/fapi/v2/order"));
    assert_eq!(row["volume"], "0.001");
    assert_eq!(row["open"], "CLOSE");
    assert!(row.get("amount").is_none());
}
#[test]
fn attached_protection_is_atomic_with_independent_child_ids() {
    let mut h = Harness::new();
    h.place(r#"[{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.003,"price":2000,"stopLoss":1800,"takeProfit":2200,"slTriggerType":"mark_price","tpTriggerType":"last_price"}]"#);
    assert_eq!(h.writes(), 1);
    let row = body(&h.take(Method::Post, "/fapi/v2/order"));
    assert!(row.get("conditionOrder").is_none());
    assert_eq!(row["triggerOrderCreateParams"][0]["triggerType"], 1);
    assert_eq!(row["triggerOrderCreateParams"][0]["triggerPriceType"], 2);
    assert_eq!(row["triggerOrderCreateParams"][1]["triggerPriceType"], 1);
    assert_ne!(
        row["triggerOrderCreateParams"][0]["clientOrderId"],
        row["triggerOrderCreateParams"][1]["clientOrderId"]
    );
}
#[test]
fn invalid_final_batch_member_prevents_every_write() {
    for tail in [
        r#"{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.003,"price":2000,"clientOrderId":"bad\nID"}"#,
        r#"{"symbol":"E-BTC-USDT","type":"conditional","side":"buy","amount":0.003,"triggerPrice":2000}"#,
        r#"{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.003,"price":2000,"timeInForce":"ImmediateOrCancel"}"#,
        r#"{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.02,"price":2000,"clientOrderId":"cannotSplit"}"#,
        r#"{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.003,"price":2000,"clientOrderId":"cannotAttach","stopLoss":1800}"#,
    ] {
        let mut h = Harness::new();
        h.place(&format!(r#"[{{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.003,"price":2000}},{tail}]"#));
        assert!(h.failed(), "accepted {tail}");
        assert_eq!(h.writes(), 0);
    }
}

fn assert_bybit_control_rejected_before_batch_io(control: &str) {
    let mut h = Harness::new();
    let requests_before = h.requests.len();
    h.place(&format!(
        r#"[{{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.003,"price":2000}},{{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.003,"price":2000,{control}}}]"#
    ));
    assert_eq!(
        h.requests.len(),
        requests_before,
        "batch preflight must perform no HTTP"
    );
    assert!(
        h.failed(),
        "Bitrue cannot honor a Bybit-specific order control"
    );
    assert!(
        h.e.store().orders().is_empty(),
        "no creating orders may leak from preflight"
    );
}

#[test]
fn bybit_bbo_on_final_batch_member_prevents_every_http_request() {
    assert_bybit_control_rejected_before_batch_io(r#""bybitBbo":{"sideType":"Queue","level":1}"#);
}

#[test]
fn bybit_ticks_on_final_batch_member_prevents_every_http_request() {
    assert_bybit_control_rejected_before_batch_io(r#""bybitSlippageToleranceTicks":2"#);
}

#[test]
fn splits_conserve_step_units_and_each_size_stays_below_maximum() {
    let mut h = Harness::new();
    h.place(r#"[{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.025,"price":2000}]"#);
    let mut units = 0;
    let mut ids = std::collections::HashSet::new();
    for id in 1..=3 {
        let request = h.take(Method::Post, "/fapi/v2/order");
        let row = body(&request);
        let amount = row["volume"].as_str().unwrap().parse::<f64>().unwrap();
        assert!(amount <= 0.01);
        units += (amount / 0.001).round() as i32;
        assert!(ids.insert(row["clientOrderId"].as_str().unwrap().to_owned()));
        h.reply(
            request,
            &format!(r#"{{"code":"0","data":{{"orderId":{id}}}}}"#),
        );
    }
    assert_eq!(units, 25);
    assert_eq!(h.result(), Some(r#"["1","2","3"]"#));
}
#[test]
fn protective_orders_require_a_position_and_opposite_side() {
    let mut h = Harness::new();
    h.place(r#"[{"symbol":"E-BTC-USDT","type":"stop_market","side":"sell","amount":0.001,"price":1800}]"#);
    assert!(h.failed());
    assert_eq!(h.writes(), 0);
    h.notices.clear();
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
    h.place(r#"[{"symbol":"E-BTC-USDT","type":"stop_market","side":"sell","amount":0.001,"price":1800}]"#);
    let request = h.take(Method::Post, "/fapi/v2/order");
    let row = body(&request);
    assert_eq!(row["conditionOrder"], true);
    assert_eq!(row["open"], "CLOSE");
    assert_eq!(row["triggerType"], 1);
    assert_eq!(row["triggerPriceType"], 2);
    let client = row["clientOrderId"].as_str().unwrap();
    h.reply(request, r#"{"code":"0","data":{"orderId":123}}"#);
    assert_eq!(h.result(), None);
    let census = post_write_census(&mut h);
    h.reply(census, &protective_census(client));
    assert_eq!(h.result(), Some("[]"));
}
#[test]
fn lost_placement_response_never_retries_a_write() {
    let mut h = Harness::new();
    h.place(r#"[{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.003,"price":2000}]"#);
    let request = h.take(Method::Post, "/fapi/v2/order");
    h.handle(
        110.0,
        Event::HttpFailed {
            request: request.id,
            error: "response lost after acceptance",
        },
    );
    h.collect();
    assert!(h.failed());
    assert_eq!(h.writes(), 0);
}
#[test]
fn amend_reads_then_cancels_then_verifies_before_replacing_remaining_with_new_ids() {
    let mut h = Harness::new();
    h.order(0.004);
    h.amend();
    assert_eq!(h.writes(), 0);
    let request = h.take(Method::Get, "/fapi/v2/order?");
    h.reply(request, &detail("PARTIALLY_FILLED", 0.004, r#", "childOrders":[{"orderId":"child","origQty":0.01,"executedQty":0,"status":"INIT","type":"MARKET","triggerType":1,"triggerPrice":1800,"triggerPriceType":2}]"#));
    let cancel = h.take(Method::Post, "/fapi/v2/cancel");
    assert_eq!(body(&cancel)["conditionOrder"], false);
    h.reply(
        cancel,
        r#"{"code":"0","data":{"orderId":2334151155877155281}}"#,
    );
    assert_eq!(h.writes(), 0);
    let request = h.take(Method::Get, "/fapi/v2/order?");
    h.reply(request, &detail("CANCELLED", 0.004, ""));
    let request = h.take(Method::Post, "/fapi/v2/order");
    let row = body(&request);
    assert_eq!(placement_receipts(&h, &request).len(), 2);
    assert_eq!(row["volume"], "0.006");
    assert_eq!(row["price"], 1990.0);
    assert_ne!(row["clientOrderId"], "callerOld");
    assert_eq!(row["triggerOrderCreateParams"][0]["triggerPrice"], 1800.0);
    assert_eq!(row["triggerOrderCreateParams"][0]["triggerPriceType"], 2);
}
#[test]
fn amend_final_fill_or_missing_protection_provenance_never_places() {
    for final_reply in [
        detail("CANCELLED", 0.005, ""),
        detail("FILLED", 0.01, ""),
        detail("INIT", 0.004, ""),
    ] {
        let mut h = Harness::new();
        h.order(0.004);
        h.amend();
        let request = h.take(Method::Get, "/fapi/v2/order?");
        h.reply(request, &detail("PARTIALLY_FILLED", 0.004, ""));
        let cancel = h.take(Method::Post, "/fapi/v2/cancel");
        h.reply(
            cancel,
            r#"{"code":"0","data":{"orderId":2334151155877155281}}"#,
        );
        let request = h.take(Method::Get, "/fapi/v2/order?");
        h.reply(request, &final_reply);
        assert!(
            !h.notices
                .iter()
                .any(|n| matches!(n, Notice::CommandProgress { .. }))
        );
        assert!(h.failed());
        assert_eq!(h.writes(), 0);
    }
    let mut h = Harness::new();
    h.order(0.004);
    h.amend();
    let request = h.take(Method::Get, "/fapi/v2/order?");
    h.reply(request,&detail("PARTIALLY_FILLED",0.004,r#", "childOrders":[{"orderId":"child","status":"INIT","type":"MARKET","triggerType":1,"triggerPrice":1800}]"#));
    assert!(h.failed());
    assert_eq!(h.writes(), 0);
}
#[test]
fn unrelated_cancel_identity_cannot_start_replacement() {
    let mut h = Harness::new();
    h.order(0.004);
    h.amend();
    let request = h.take(Method::Get, "/fapi/v2/order?");
    h.reply(request, &detail("INIT", 0.004, ""));
    let cancel = h.take(Method::Post, "/fapi/v2/cancel");
    h.reply(cancel, r#"{"code":"0","data":{"orderId":999}}"#);
    assert!(h.failed());
    assert_eq!(h.writes(), 0);
}
#[test]
fn incomplete_authoritative_children_cannot_drop_known_protection() {
    for children in ["", r#", "childOrders":[]"#] {
        let mut h = Harness::new();
        h.order(0.0);
        let mut order = h.e.store().order("2334151155877155281").unwrap().clone();
        order.stop_loss = Some(1800.0);
        h.e.store_mut().upsert_order(order);
        h.amend();
        let request = h.take(Method::Get, "/fapi/v2/order?");
        h.reply(request, &detail("INIT", 0.0, children));
        assert!(h.failed());
        assert_eq!(h.writes(), 0);
    }
}
#[test]
fn settings_commit_only_after_success_and_unsupported_modes_never_write() {
    let mut h = Harness::new();
    h.command(Command::SetLeverage {
        command: 7,
        symbol: "E-BTC-USDT".into(),
        leverage: 30.0,
        side: None,
    });
    let request = h.take(Method::Post, "/fapi/v2/level_edit");
    h.reply(request, r#"{"code":"-1","msg":"denied"}"#);
    assert!(h.failed());
    assert_eq!(h.e.store().setting("E-BTC-USDT").unwrap().leverage, 20.0);
    h.notices.clear();
    h.command(Command::SetMarginType {
        command: 7,
        symbol: "E-BTC-USDT".into(),
        isolated: true,
    });
    assert!(h.failed());
    h.notices.clear();
    h.command(Command::SetPositionMode {
        command: 7,
        hedged: true,
        symbol: None,
    });
    assert!(h.failed());
}

#[test]
fn cancel_symbol_includes_ack_before_rest_or_private_new() {
    let mut h = Harness::new();
    h.place(r#"[{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.003,"price":2000,"clientOrderId":"pending"}]"#);
    let create = h.take(Method::Post, "/fapi/v2/order");
    h.reply(
        create,
        r#"{"code":"0","data":{"orderId":2334151155877155281}}"#,
    );
    assert!(h.e.store().orders().is_empty());
    h.command(Command::CancelSymbolOrders {
        command: 8,
        symbol: "E-BTC-USDT".into(),
    });
    let cancel = h.take(Method::Post, "/fapi/v2/cancel");
    assert_eq!(body(&cancel)["orderId"], "2334151155877155281");
    h.reply(
        cancel,
        r#"{"code":"0","data":{"orderId":2334151155877155281}}"#,
    );
    assert!(h.notices.iter().any(|n| matches!(
        n,
        Notice::CommandDone {
            command: 8,
            result: _
        }
    )));
}

fn protective_census(client: &str) -> String {
    format!(
        r#"{{"code":"0","data":[{{"orderId":456,"clientOrderId":"{client}","contractName":"E-BTC-USDT","type":"MARKET","side":"SELL","status":"INIT","action":"CLOSE","origQty":0.001,"executedQty":0,"triggerPrice":1800,"triggerType":1,"triggerPriceType":2,"conditionOrder":true}}]}}"#
    )
}
fn post_write_census(h: &mut Harness) -> HttpRequest {
    // Startup's census predates the write. It cannot certify its outcome.
    let stale = h.take(Method::Get, "/fapi/v2/openOrders");
    h.reply(stale, r#"{"code":"0","data":[]}"#);
    // The failed census retries ORDER_RETRY's 500 ms, jittered, after the reply.
    let (timer, due) = *h
        .timers
        .iter()
        .rev()
        .find(|(_, at)| (h.now + 375.0..=h.now + 500.0).contains(at))
        .unwrap();
    h.handle(due, Event::TimerFired { timer });
    h.collect();
    h.take(Method::Get, "/fapi/v2/openOrders")
}
fn protective_ack() -> (Harness, String) {
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
    h.place(r#"[{"symbol":"E-BTC-USDT","type":"stop_market","side":"sell","amount":0.001,"price":1800}]"#);
    let create = h.take(Method::Post, "/fapi/v2/order");
    let client = body(&create)["clientOrderId"].as_str().unwrap().to_owned();
    // Bitrue returns incorrect protective create IDs (Bitrue CLAUDE.md).
    h.reply(create, r#"{"code":"0","data":{"orderId":123}}"#);
    (h, client)
}
#[test]
fn conditional_cancel_all_waits_for_real_native_identity_in_a_post_write_census() {
    let (mut h, client) = protective_ack();
    h.command(Command::CancelSymbolOrders {
        command: 8,
        symbol: "E-BTC-USDT".into(),
    });
    assert_eq!(h.result(), None);
    assert_eq!(
        h.writes(),
        0,
        "the unreliable ACK must not authorize cancel123"
    );
    let census = post_write_census(&mut h);
    assert_eq!(
        h.result(),
        None,
        "a pre-write census cannot resolve placement"
    );
    assert_eq!(h.writes(), 0);
    h.reply(census, &protective_census(&client));
    assert_eq!(h.result(), Some("[]"));
    let cancel = h.take(Method::Post, "/fapi/v2/cancel");
    assert_eq!(body(&cancel)["orderId"], "456");
    assert_eq!(body(&cancel)["conditionOrder"], true);
    h.reply(cancel, r#"{"code":"0","data":{"orderId":456}}"#);
    assert!(h.notices.iter().any(|n| matches!(
        n,
        Notice::CommandDone {
            command: 8,
            result: _
        }
    )));
    assert_eq!(h.writes(), 0);
}
#[test]
fn conditional_census_requires_complete_success() {
    for scenario in ["malformed", "business_error", "incomplete"] {
        let (mut h, _) = protective_ack();
        h.command(Command::CancelSymbolOrders {
            command: 8,
            symbol: "E-BTC-USDT".into(),
        });
        let census = post_write_census(&mut h);
        let response = match scenario {
            "malformed" => r#"{"code":"0","data":{}}"#.to_owned(),
            "incomplete" => r#"{"code":"0","data":[{"orderId":456,"status":"INIT"}]}"#.to_owned(),
            _ => r#"{"code":"-1","msg":"denied"}"#.to_owned(),
        };
        h.reply(census, &response);
        assert!(
            h.failed(),
            "{scenario} must not confirm protective placement"
        );
        assert_eq!(
            h.writes(),
            0,
            "{scenario} cannot authorize a guessed cancellation"
        );
        assert!(h.notices.iter().any(|n| matches!(n, Notice::CommandFailed { command: 8, error } if error.legacy_text().contains("uncertain"))));
        for command in [9, 10] {
            h.command(Command::CancelSymbolOrders {
                command,
                symbol: "E-BTC-USDT".into(),
            });
            assert!(h.notices.iter().any(|n| matches!(n, Notice::CommandFailed { command: id, error } if *id == command && error.legacy_text().contains("uncertain"))));
        }
        assert_eq!(h.writes(), 0);
    }
}

#[test]
fn conditional_census_superseded_by_stream_cannot_authorize_cancel_all() {
    let (mut h, client) = protective_ack();
    h.command(Command::CancelSymbolOrders {
        command: 8,
        symbol: "E-BTC-USDT".into(),
    });
    let census = post_write_census(&mut h);
    let listen = h.take(Method::Post, "/user_stream/api/v1/listenKey");
    h.reply(listen, r#"{"code":"0","data":{"listenKey":"replay-key"}}"#);
    let socket = h
        .sockets
        .iter()
        .find(|(_, url)| url.contains("fapiws.bitrue.com"))
        .unwrap()
        .0;
    h.handle(700.0, Event::WsMessage { socket, text: r#"{"e":"ORDER_TRADE_UPDATE","o":{"s":"E_BTCUSDT","i":999,"S":"BUY","o":"LIMIT","x":"NEW","X":"NEW","q":"1","z":"0","p":"2000"}}"# });
    h.collect();
    h.reply(census, &protective_census(&client));
    assert!(h.failed());
    assert_eq!(h.writes(), 0);
    assert!(h.notices.iter().any(|n| matches!(n, Notice::CommandFailed { command: 8, error } if error.legacy_text().contains("uncertain"))));
}
#[test]
fn conditional_census_transport_retries_do_not_complete_or_send_guessed_cancel() {
    let (mut h, _) = protective_ack();
    h.command(Command::CancelSymbolOrders {
        command: 8,
        symbol: "E-BTC-USDT".into(),
    });
    let mut census = post_write_census(&mut h);
    for attempt in 0..4 {
        h.handle(
            700.0 + f64::from(attempt),
            Event::HttpFailed {
                request: census.id,
                error: "census transport unavailable",
            },
        );
        h.collect();
        assert_eq!(h.writes(), 0);
        if attempt < 3 {
            assert_eq!(h.result(), None);
            assert!(!h.failed());
            census = h.take(Method::Get, "/fapi/v2/openOrders");
        }
    }
    assert!(h.failed());
    assert!(h.notices.iter().any(|n| matches!(n, Notice::CommandFailed { command: 8, error } if error.legacy_text().contains("uncertain"))));
}
#[test]
fn attached_protection_census_owns_the_native_open_set_even_when_one_leg_is_gone() {
    for missing_take_profit in [false, true] {
        let mut h = Harness::new();
        h.place(r#"[{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.003,"price":2000,"stopLoss":1800,"takeProfit":2200}]"#);
        let create = h.take(Method::Post, "/fapi/v2/order");
        let payload = body(&create);
        h.reply(create, r#"{"code":"0","data":{"orderId":100}}"#);
        assert_eq!(h.result(), None);
        let census = post_write_census(&mut h);
        let children = payload["triggerOrderCreateParams"].as_array().unwrap().iter().filter(|child| !missing_take_profit || child["triggerType"] != 2).enumerate().map(|(index, child)| serde_json::json!({
            "orderId": 456 + index, "clientOrderId": child["clientOrderId"],
            "type": "MARKET", "side": "SELL", "status": "INIT", "action": "CLOSE",
            "origQty": 0.003, "executedQty": 0, "triggerPrice": child["triggerPrice"],
            "triggerType": child["triggerType"], "triggerPriceType": 2, "conditionOrder": true,
        })).collect::<Vec<_>>();
        let response = serde_json::json!({"code":"0","data":[{
            "orderId":100,"clientOrderId":payload["clientOrderId"],"contractName":"E-BTC-USDT",
            "type":"LIMIT","side":"BUY","status":"INIT","action":"OPEN","price":2000,
            "origQty":0.003,"executedQty":0,"childOrders":children,
        }]})
        .to_string();
        h.reply(census, &response);
        assert_eq!(h.result(), Some("[]"));
    }
}

#[test]
fn conditional_census_accepts_rewritten_zero_and_missing_client_ids_without_trusting_ack() {
    for client in ["0", "server-rewritten", ""] {
        let (mut h, _) = protective_ack();
        h.command(Command::CancelSymbolOrders {
            command: 8,
            symbol: "E-BTC-USDT".into(),
        });
        let census = post_write_census(&mut h);
        h.reply(census, &protective_census(client));
        assert_eq!(h.result(), Some("[]"));
        let cancel = h.take(Method::Post, "/fapi/v2/cancel");
        assert_eq!(body(&cancel)["orderId"], "456");
        assert_eq!(body(&cancel)["conditionOrder"], true);
    }
}
/// B1 interim (fe7lgm): a census delivery completes nothing inside itself; its
/// result lands exactly once on the next tick, with native's GET count.
#[test]
fn a_census_completes_its_placement_once_on_the_tick_after_its_delivery() {
    let (mut h, _) = protective_ack();
    let census = post_write_census(&mut h);
    let gets = |h: &Harness| {
        h.requests
            .iter()
            .filter(|r| r.method == Method::Get && r.url.contains("/fapi/v2/openOrders"))
            .count()
    };
    let before = gets(&h);
    let armed = h.timers.len();
    let now = h.now;
    h.e.handle(
        now,
        Event::HttpResponse {
            request: census.id,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"code":"0","data":[]}"#,
            },
        },
    );
    h.collect_once();
    assert_eq!(h.result(), None, "nothing completes inside the delivery");
    let ticks: Vec<_> = h.timers[armed..]
        .iter()
        .filter(|(_, at)| *at <= now)
        .copied()
        .collect();
    assert_eq!(ticks.len(), 1, "one deferred census tick");
    h.timers.retain(|tick| *tick != ticks[0]);
    h.handle(now, Event::TimerFired { timer: ticks[0].0 });
    h.collect();
    assert_eq!(h.result(), Some("[]"));
    let done = h
        .notices
        .iter()
        .filter(|n| matches!(n, Notice::CommandDone { command: 7, .. }))
        .count();
    assert_eq!(done, 1, "exactly once");
    h.collect();
    assert_eq!(gets(&h), before, "no extra open-order read");
}
#[test]
fn complete_post_write_empty_census_proves_no_open_protection_and_releases_cancel_all() {
    let (mut h, _) = protective_ack();
    h.command(Command::CancelSymbolOrders {
        command: 8,
        symbol: "E-BTC-USDT".into(),
    });
    let census = post_write_census(&mut h);
    assert_eq!(h.result(), None);
    h.reply(census, r#"{"code":"0","data":[]}"#);
    assert_eq!(h.result(), Some("[]"));
    assert_eq!(h.writes(), 0);
    assert!(h.notices.iter().any(|n| matches!(
        n,
        Notice::CommandDone {
            command: 8,
            result: _
        }
    )));
}

#[test]
fn protective_amend_failed_census_keeps_native_cancel_all_uncertain_until_a_fresh_read() {
    let (mut h, client) = protective_ack();
    let census = post_write_census(&mut h);
    h.reply(census, &protective_census(&client));
    h.command(Command::AmendOrder {
        command: 11,
        id: "456".into(),
        price: Some(1790.0),
        amount: None,
    });
    let detail = |status| {
        format!(
            r#"{{"code":"0","data":[{{"orderId":456,"contractName":"E-BTC-USDT","status":"{status}","origQty":0.001,"executedQty":0,"side":"SELL","type":"MARKET","action":"CLOSE","conditionOrder":true,"triggerType":1,"triggerPriceType":2}}]}}"#
        )
    };
    let inspect = h.take(Method::Get, "/fapi/v2/order?");
    h.reply(inspect, &detail("INIT"));
    let cancel = h.take(Method::Post, "/fapi/v2/cancel");
    h.reply(cancel, r#"{"code":"0","data":{"orderId":456}}"#);
    let verify = h.take(Method::Get, "/fapi/v2/order?");
    h.reply(verify, &detail("CANCELED"));
    let create = h.take(Method::Post, "/fapi/v2/order");
    h.reply(
        create,
        r#"{"code":"0","data":{"orderId":"unreliable-replacement"}}"#,
    );
    let census = h.take(Method::Get, "/fapi/v2/openOrders");
    h.reply(census, r#"{"code":"-1","msg":"denied"}"#);
    assert!(h.notices.iter().any(|n| matches!(n, Notice::CommandFailed { command: 11, error } if error.legacy_text().contains("uncertain"))));
    h.command(Command::CancelSymbolOrders {
        command: 12,
        symbol: "E-BTC-USDT".into(),
    });
    assert_eq!(h.writes(), 0);
    assert!(h.notices.iter().any(|n| matches!(n, Notice::CommandFailed { command: 12, error } if error.legacy_text().contains("uncertain"))));
    // The failed census retries ORDER_RETRY's 500 ms, jittered, after the reply.
    let (timer, due) = *h
        .timers
        .iter()
        .rev()
        .find(|(_, at)| (h.now + 375.0..=h.now + 500.0).contains(at))
        .unwrap();
    h.handle(due, Event::TimerFired { timer });
    h.collect();
    let census = h.take(Method::Get, "/fapi/v2/openOrders");
    h.reply(census, r#"{"code":"0","data":[]}"#);
    h.command(Command::CancelSymbolOrders {
        command: 13,
        symbol: "E-BTC-USDT".into(),
    });
    assert!(h.notices.iter().any(|n| matches!(
        n,
        Notice::CommandDone {
            command: 13,
            result: _
        }
    )));
    assert_eq!(h.writes(), 0);
}

fn protective_amend_create() -> (Harness, HttpRequest) {
    let (mut h, client) = protective_ack();
    let census = post_write_census(&mut h);
    h.reply(census, &protective_census(&client));
    h.command(Command::AmendOrder {
        command: 11,
        id: "456".into(),
        price: Some(1790.0),
        amount: None,
    });
    let detail = |status| {
        format!(
            r#"{{"code":"0","data":[{{"orderId":456,"contractName":"E-BTC-USDT","status":"{status}","origQty":0.001,"executedQty":0,"side":"SELL","type":"MARKET","action":"CLOSE","conditionOrder":true,"triggerType":1,"triggerPriceType":2}}]}}"#
        )
    };
    let inspect = h.take(Method::Get, "/fapi/v2/order?");
    h.reply(inspect, &detail("INIT"));
    let cancel = h.take(Method::Post, "/fapi/v2/cancel");
    h.reply(cancel, r#"{"code":"0","data":{"orderId":456}}"#);
    let verify = h.take(Method::Get, "/fapi/v2/order?");
    h.reply(verify, &detail("CANCELED"));
    let create = h.take(Method::Post, "/fapi/v2/order");
    (h, create)
}

#[test]
fn protective_amend_lost_or_malformed_create_ack_fences_cancel_until_fresh_native_census() {
    for outcome in [
        "lost",
        "malformed",
        "missing_id",
        "native_timeout",
        "server_error",
    ] {
        let (mut h, create) = protective_amend_create();
        if outcome == "lost" {
            h.handle(
                700.0,
                Event::HttpFailed {
                    request: create.id,
                    error: "replacement accepted but response lost",
                },
            );
            h.collect();
        } else if outcome == "server_error" {
            h.handle(
                700.0,
                Event::HttpResponse {
                    request: create.id,
                    response: Response {
                        status: 504,
                        headers: "",
                        body: r#"{"code":"-1102","msg":"proxy response is not a rejection proof"}"#,
                    },
                },
            );
            h.collect();
        } else {
            h.reply(
                create,
                match outcome {
                    "malformed" => "truncated",
                    "missing_id" => r#"{"code":"0","data":{}}"#,
                    _ => r#"{"code":"-1007","msg":"execution status unknown"}"#,
                },
            );
        }
        assert!(
            h.notices
                .iter()
                .any(|n| matches!(n, Notice::CommandFailed { command: 11, .. }))
        );
        let census = h.take(Method::Get, "/fapi/v2/openOrders");
        for command in [12, 13] {
            h.command(Command::CancelSymbolOrders {
                command,
                symbol: "E-BTC-USDT".into(),
            });
            assert!(h.notices.iter().any(|n| matches!(n, Notice::CommandFailed { command: id, error } if *id == command && error.legacy_text().contains("uncertain"))), "{outcome}: held census must prevent empty cancel success");
            assert_eq!(
                h.writes(),
                0,
                "{outcome}: no guessed cancel or create replay"
            );
        }
        h.reply(census, &protective_census("0").replace("456", "789"));
        h.command(Command::CancelSymbolOrders {
            command: 14,
            symbol: "E-BTC-USDT".into(),
        });
        let cancel = h.take(Method::Post, "/fapi/v2/cancel");
        assert_eq!(body(&cancel)["orderId"], "789");
        assert_eq!(body(&cancel)["conditionOrder"], true);
        h.reply(cancel, r#"{"code":"0","data":{"orderId":789}}"#);
        assert!(h.notices.iter().any(|n| matches!(
            n,
            Notice::CommandDone {
                command: 14,
                result: _
            }
        )));
    }
}

#[test]
fn protective_amend_definite_parameter_rejection_does_not_fence_cancel_all() {
    let (mut h, create) = protective_amend_create();
    h.reply(
        create,
        r#"{"code":"-1102","msg":"Required parameter missing"}"#,
    );
    assert!(
        h.notices
            .iter()
            .any(|n| matches!(n, Notice::CommandFailed { command: 11, .. }))
    );
    let _held = h.take(Method::Get, "/fapi/v2/openOrders");
    h.command(Command::CancelSymbolOrders {
        command: 12,
        symbol: "E-BTC-USDT".into(),
    });
    assert!(h.notices.iter().any(|n| matches!(
        n,
        Notice::CommandDone {
            command: 12,
            result: _
        }
    )));
    assert_eq!(h.writes(), 0);
}

#[test]
fn protective_amend_preflight_failure_keeps_existing_protection_cancelable() {
    let (mut h, client) = protective_ack();
    let census = post_write_census(&mut h);
    h.reply(census, &protective_census(&client));
    h.command(Command::AmendOrder {
        command: 11,
        id: "456".into(),
        price: Some(0.0),
        amount: None,
    });
    assert!(
        h.notices
            .iter()
            .any(|n| matches!(n, Notice::CommandFailed { command: 11, .. }))
    );
    assert_eq!(h.writes(), 0);
    h.command(Command::CancelSymbolOrders {
        command: 12,
        symbol: "E-BTC-USDT".into(),
    });
    let cancel = h.take(Method::Post, "/fapi/v2/cancel");
    assert_eq!(body(&cancel)["orderId"], "456");
}

#[test]
fn protective_amend_rejection_preserves_an_earlier_lost_replacement_fence() {
    let (mut h, create) = protective_amend_create();
    h.handle(
        700.0,
        Event::HttpFailed {
            request: create.id,
            error: "earlier replacement response lost",
        },
    );
    h.collect();
    let held = h.take(Method::Get, "/fapi/v2/openOrders");
    let mut other = h.e.store().order("456").unwrap().clone();
    other.id = "555".into();
    other.status = OrderStatus::Open;
    h.e.store_mut().upsert_order(other);
    h.command(Command::AmendOrder {
        command: 15,
        id: "555".into(),
        price: Some(1780.0),
        amount: None,
    });
    let detail = |status| {
        format!(
            r#"{{"code":"0","data":[{{"orderId":555,"contractName":"E-BTC-USDT","status":"{status}","origQty":0.001,"executedQty":0,"side":"SELL","type":"MARKET","action":"CLOSE","conditionOrder":true,"triggerType":1,"triggerPriceType":2}}]}}"#
        )
    };
    let inspect = h.take(Method::Get, "/fapi/v2/order?");
    h.reply(inspect, &detail("INIT"));
    let cancel = h.take(Method::Post, "/fapi/v2/cancel");
    h.reply(cancel, r#"{"code":"0","data":{"orderId":555}}"#);
    let verify = h.take(Method::Get, "/fapi/v2/order?");
    h.reply(verify, &detail("CANCELED"));
    let create = h.take(Method::Post, "/fapi/v2/order");
    h.reply(
        create,
        r#"{"code":"-1102","msg":"Required parameter missing"}"#,
    );
    assert!(
        h.notices
            .iter()
            .any(|n| matches!(n, Notice::CommandFailed { command: 15, .. }))
    );
    // This read predates the second write and cannot resolve the older unknown order.
    h.reply(held, r#"{"code":"0","data":[]}"#);
    h.command(Command::CancelSymbolOrders {
        command: 16,
        symbol: "E-BTC-USDT".into(),
    });
    assert!(h.notices.iter().any(|n| matches!(n, Notice::CommandFailed { command: 16, error } if error.legacy_text().contains("uncertain"))));
    assert_eq!(h.writes(), 0);
}

#[test]
fn missing_leverage_and_invalid_caller_null_fail_before_writes() {
    let mut h = Harness::new();
    h.e.store_mut().set_settings([]);
    h.place(r#"[{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.003,"price":2000}]"#);
    assert!(h.failed());
    assert_eq!(h.writes(), 0);
    assert!(serde_json::from_str::<PlaceOrderOpts>(r#"{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.003,"price":2000,"clientOrderId":null}"#).is_err());
}

fn placement_receipts(h: &Harness, request: &HttpRequest) -> Vec<OrderIdentityReceipt> {
    let row = body(request);
    let mut ids = vec![row["clientOrderId"].as_str().unwrap().to_owned()];
    if let Some(children) = row["triggerOrderCreateParams"].as_array() {
        ids.extend(
            children
                .iter()
                .map(|child| child["clientOrderId"].as_str().unwrap().to_owned()),
        );
    }
    let expected: Vec<_> = ids
        .into_iter()
        .map(|identity| OrderIdentityReceipt {
            symbol: "E-BTC-USDT".into(),
            kind: OrderIdentityKind::ClientOrderId,
            identity,
        })
        .collect();
    let write = h
        .trace
        .iter()
        .position(|event| matches!(event, Trace::Http(id) if *id == request.id))
        .unwrap();
    assert!(write > 0);
    match &h.trace[write - 1] {
        Trace::Progress(command, receipts) => {
            assert_eq!(*command, 7);
            assert_eq!(receipts, &expected);
        }
        _ => panic!("placement must be preceded by its typed receipts"),
    }
    expected
}

#[test]
fn receipts_cover_final_split_parents_and_embedded_children_before_each_write() {
    let mut h = Harness::new();
    h.place(r#"[{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.025,"price":2000,"stopLoss":1800,"takeProfit":2200}]"#);
    let mut ids = std::collections::HashSet::new();
    for index in 0..3 {
        let request = h.take(Method::Post, "/fapi/v2/order");
        let receipts = placement_receipts(&h, &request);
        assert_eq!(receipts.len(), 3);
        for receipt in receipts {
            assert!(ids.insert(receipt.identity));
        }
        h.reply(
            request,
            &format!(r#"{{"code":"0","data":{{"orderId":"parent-{index}"}}}}"#),
        );
    }
    assert_eq!(ids.len(), 9);
}

#[test]
fn receipts_preserve_a_caller_id_without_minting_another_namespace() {
    let mut h = Harness::new();
    h.place(r#"[{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.005,"price":2000,"clientOrderId":"caller-exact"}]"#);
    let request = h.take(Method::Post, "/fapi/v2/order");
    assert_eq!(placement_receipts(&h, &request)[0].identity, "caller-exact");
}

#[test]
fn missing_cancel_identity_is_reported_without_blocking_known_ids() {
    let mut h = Harness::new();
    h.order(0.0);
    h.command(Command::CancelOrders {
        command: 7,
        ids: vec!["missing".into(), "2334151155877155281".into()],
    });
    let cancel = h.take(Method::Post, "/fapi/v2/cancel");
    assert_eq!(body(&cancel)["orderId"], "2334151155877155281");
    h.reply(
        cancel,
        r#"{"code":"0","data":{"orderId":2334151155877155281}}"#,
    );
    let error = h
        .notices
        .iter()
        .find_map(|n| match n {
            Notice::CommandFailed { command: 7, error } => Some(error),
            _ => None,
        })
        .expect("partial cancellation error");
    let error: serde_json::Value = serde_json::from_str(&error.legacy_text()).unwrap();
    assert_eq!(error["failedOrders"][0]["id"], "missing");
    assert_eq!(error["canceledOrderIds"][0], "2334151155877155281");
    assert_eq!(
        h.e.store().order("2334151155877155281").unwrap().status,
        OrderStatus::Canceled
    );
}

#[test]
fn cancel_batch_continues_after_native_and_transport_and_identity_failures() {
    for (failure, by_symbol) in [
        ("native", false),
        ("transport", false),
        ("identity", false),
        ("native", true),
    ] {
        let mut h = Harness::new();
        h.order(0.0);
        let mut second = h.e.store().order("2334151155877155281").unwrap().clone();
        second.id = "2334151155877155282".into();
        h.e.store_mut().upsert_order(second);
        h.command(if by_symbol {
            Command::CancelSymbolOrders {
                command: 7,
                symbol: "E-BTC-USDT".into(),
            }
        } else {
            Command::CancelOrders {
                command: 7,
                ids: vec!["2334151155877155281".into(), "2334151155877155282".into()],
            }
        });
        let first = h.take(Method::Post, "/fapi/v2/cancel");
        match failure {
            "transport" => {
                h.handle(
                    101.0,
                    Event::HttpFailed {
                        request: first.id,
                        error: "connection lost",
                    },
                );
                h.collect();
            }
            "native" => h.reply(first, r#"{"code":"-1","msg":"denied"}"#),
            _ => h.reply(first, r#"{"code":"0","data":{"orderId":"different"}}"#),
        }
        assert_eq!(
            h.e.store().order("2334151155877155281").unwrap().status,
            OrderStatus::Open
        );
        let next = h.take(Method::Post, "/fapi/v2/cancel");
        assert_eq!(body(&next)["orderId"], "2334151155877155282");
        h.reply(
            next,
            r#"{"code":"0","data":{"orderId":2334151155877155282}}"#,
        );
        assert!(h.failed());
        let error = h
            .notices
            .iter()
            .find_map(|n| match n {
                Notice::CommandFailed { command: 7, error } => Some(error),
                _ => None,
            })
            .unwrap();
        let error: serde_json::Value = serde_json::from_str(&error.legacy_text()).unwrap();
        assert_eq!(error["failedOrders"][0]["id"], "2334151155877155281");
        assert_eq!(error["canceledOrderIds"][0], "2334151155877155282");
        assert_eq!(
            h.e.store().order("2334151155877155282").unwrap().status,
            OrderStatus::Canceled
        );
    }
}

#[test]
fn fresh_account_trades_only_after_explicit_leverage_ack_and_defaults_to_cross() {
    let mut h = Harness::new();
    h.e.store_mut().set_settings([]);
    h.command(Command::SetLeverage {
        command: 6,
        symbol: "E-BTC-USDT".into(),
        leverage: 7.0,
        side: None,
    });
    let setting = h.take(Method::Post, "/fapi/v2/level_edit");
    assert!(h.e.store().setting("E-BTC-USDT").is_none());
    h.reply(setting, r#"{"code":"0","data":{}}"#);
    // An old account read is retried after the acknowledgement; even a fresh
    // authoritative empty position list must retain the explicitly set leverage.
    let empty = r#"{"code":"0","data":{"account":[{"marginCoin":"USDT","totalEquity":100,"accountNormal":100,"positionVos":[]}]}}"#;
    let account = h.take(Method::Get, "/fapi/v2/account");
    h.reply(account, empty);
    let account = h.take(Method::Get, "/fapi/v2/account");
    h.reply(account, empty);
    h.place(r#"[{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.003,"price":2000}]"#);
    let order = h.take(Method::Post, "/fapi/v2/order");
    assert_eq!(body(&order)["leverage"], 7.0);
    assert_eq!(body(&order)["positionType"], 1);
}

#[test]
fn exact_leverage_is_not_silently_clamped_to_conservative_startup_limit() {
    let mut h = Harness::new();
    h.command(Command::SetLeverage {
        command: 7,
        symbol: "E-BTC-USDT".into(),
        leverage: 125.0,
        side: None,
    });
    let request = h.take(Method::Post, "/fapi/v2/level_edit");
    assert_eq!(body(&request)["leverage"], 125.0);
    h.reply(
        request,
        r#"{"code":"-1","msg":"Maximum leverage does not exceed 50"}"#,
    );
    assert!(h.failed());
    assert_eq!(
        h.e.store()
            .market("E-BTC-USDT")
            .unwrap()
            .limits
            .leverage
            .max,
        50.0
    );
    assert_eq!(h.e.store().setting("E-BTC-USDT").unwrap().leverage, 20.0);
    h.notices.clear();
    h.command(Command::SetLeverage {
        command: 7,
        symbol: "E-BTC-USDT".into(),
        leverage: 51.0,
        side: None,
    });
    assert!(h.failed());
    assert!(
        !h.requests
            .iter()
            .any(|r| r.url.ends_with("/fapi/v2/level_edit"))
    );
}

#[test]
fn invalid_leverage_never_writes_and_failed_ack_does_not_invent_fresh_settings() {
    let mut h = Harness::new();
    h.e.store_mut().set_settings([]);
    for leverage in [0.0, -1.0, 1.5, 126.0, f64::NAN, f64::INFINITY] {
        h.notices.clear();
        h.command(Command::SetLeverage {
            command: 7,
            symbol: "E-BTC-USDT".into(),
            leverage,
            side: None,
        });
        assert!(h.failed());
        assert!(!h.requests.iter().any(|r| r.url.ends_with("/level_edit")));
        assert!(h.e.store().setting("E-BTC-USDT").is_none());
    }
    h.command(Command::SetLeverage {
        command: 7,
        symbol: "E-BTC-USDT".into(),
        leverage: 10.0,
        side: None,
    });
    let request = h.take(Method::Post, "/level_edit");
    h.reply(request, r#"{"code":"-1","msg":"denied"}"#);
    assert!(h.e.store().setting("E-BTC-USDT").is_none());
}

#[test]
fn child_cancel_remains_a_fallback_when_parent_is_missing_or_its_cancel_fails() {
    for (parent_state, by_symbol) in [
        ("missing", false),
        ("unmapped", false),
        ("denied", false),
        ("denied", true),
    ] {
        let mut h = Harness::new();
        h.order(0.0);
        let mut child = h.e.store().order("2334151155877155281").unwrap().clone();
        child.id = "child".into();
        child.parent_id = Some("2334151155877155281".into());
        child.kind = OrderType::StopLoss;
        h.e.store_mut().upsert_order(child);
        if parent_state == "missing" {
            h.e.store_mut().remove_orders(&["2334151155877155281"]);
        } else if parent_state == "unmapped" {
            let mut parent = h.e.store().order("2334151155877155281").unwrap().clone();
            parent.symbol = "unloaded".into();
            h.e.store_mut().upsert_order(parent);
        }
        h.command(if by_symbol {
            Command::CancelSymbolOrders {
                command: 7,
                symbol: "E-BTC-USDT".into(),
            }
        } else {
            Command::CancelOrders {
                command: 7,
                ids: vec!["2334151155877155281".into(), "child".into()],
            }
        });
        if parent_state == "denied" {
            let parent = h.take(Method::Post, "/fapi/v2/cancel");
            assert_eq!(body(&parent)["orderId"], "2334151155877155281");
            h.reply(
                parent,
                r#"{"code":"-1","msg":"parent cancellation denied"}"#,
            );
        }
        let child = h.take(Method::Post, "/fapi/v2/cancel");
        assert_eq!(body(&child)["orderId"], "child");
        assert_eq!(body(&child)["conditionOrder"], true);
        h.reply(child, r#"{"code":"0","data":{"orderId":"child"}}"#);
        assert_eq!(
            h.e.store().order("child").unwrap().status,
            OrderStatus::Canceled
        );
        let error = h
            .notices
            .iter()
            .find_map(|n| match n {
                Notice::CommandFailed { command: 7, error } => Some(error),
                _ => None,
            })
            .unwrap();
        let error: serde_json::Value = serde_json::from_str(&error.legacy_text()).unwrap();
        assert_eq!(error["failedOrders"][0]["id"], "2334151155877155281");
        assert_eq!(error["canceledOrderIds"][0], "child");
    }
}

#[test]
fn confirmed_parent_cancel_deduplicates_child_only_after_success() {
    let mut h = Harness::new();
    h.order(0.0);
    let mut child = h.e.store().order("2334151155877155281").unwrap().clone();
    child.id = "child".into();
    child.parent_id = Some("2334151155877155281".into());
    child.kind = OrderType::StopLoss;
    h.e.store_mut().upsert_order(child);
    h.command(Command::CancelOrders {
        command: 7,
        ids: vec!["2334151155877155281".into(), "child".into()],
    });
    let parent = h.take(Method::Post, "/fapi/v2/cancel");
    h.reply(
        parent,
        r#"{"code":"0","data":{"orderId":2334151155877155281}}"#,
    );
    assert_eq!(
        h.e.store().order("child").unwrap().status,
        OrderStatus::Canceled
    );
    assert!(!h.requests.iter().any(|r| r.url.ends_with("/cancel")));
    assert_eq!(h.result(), Some("[]"));
}

#[test]
fn cancellation_missing_from_store_reports_error_and_attempts_known_mixed_targets() {
    for mixed in [false, true] {
        let mut h = Harness::new();
        h.order(0.0);
        h.command(Command::CancelOrders {
            command: 991,
            ids: if mixed {
                vec!["2334151155877155281".into(), "missing".into()]
            } else {
                vec!["missing".into()]
            },
        });
        if mixed {
            let cancel = h.take(Method::Post, "/fapi/v2/cancel");
            assert_eq!(body(&cancel)["orderId"], "2334151155877155281");
            h.reply(
                cancel,
                r#"{"code":"0","data":{"orderId":2334151155877155281}}"#,
            );
        }
        assert_eq!(h.writes(), 0);
        assert!(h.notices.iter().any(|notice| matches!(notice,
            Notice::CommandFailed { command: 991, error }
                if error.legacy_text().contains("missing") && error.legacy_text().contains("not found")
        )));
    }
}

#[test]
fn mixed_unknown_cancel_attempts_exact_ack_route_and_failed_ack_target_remains_retryable() {
    for cached_first in [false, true] {
        for rejected in [false, true] {
            let mut h = Harness::new();
            h.place(r#"[{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.003,"price":2000,"clientOrderId":"pending"}]"#);
            let create = h.take(Method::Post, "/fapi/v2/order");
            h.reply(
                create,
                r#"{"code":"0","data":{"orderId":2334151155877155281}}"#,
            );
            assert!(h.e.store().orders().is_empty());
            let cached = "2334151155877155281".to_string();
            h.command(Command::CancelOrders {
                command: 991,
                ids: if cached_first {
                    vec![cached.clone(), "missing".into()]
                } else {
                    vec!["missing".into(), cached.clone()]
                },
            });
            let cancel = h.take(Method::Post, "/fapi/v2/cancel");
            assert_eq!(body(&cancel)["orderId"], cached);
            assert_eq!(body(&cancel)["contractName"], "E-BTC-USDT");
            assert_eq!(body(&cancel)["conditionOrder"], false);
            h.reply(
                cancel,
                if rejected {
                    r#"{"code":"-1","msg":"denied"}"#
                } else {
                    r#"{"code":"0","data":{"orderId":2334151155877155281}}"#
                },
            );
            assert_eq!(h.writes(), 0);
            let error = h
                .notices
                .iter()
                .find_map(|notice| match notice {
                    Notice::CommandFailed {
                        command: 991,
                        error,
                    } => Some(error),
                    _ => None,
                })
                .expect("mixed cancellation remains visibly partial");
            let error: serde_json::Value = serde_json::from_str(&error.legacy_text()).unwrap();
            assert!(
                error["failedOrders"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|f| f["id"] == "missing")
            );
            if rejected {
                h.command(Command::CancelOrders {
                    command: 992,
                    ids: vec![cached],
                });
                let retry = h.take(Method::Post, "/fapi/v2/cancel");
                assert_eq!(body(&retry)["orderId"], "2334151155877155281");
                h.reply(
                    retry,
                    r#"{"code":"0","data":{"orderId":2334151155877155281}}"#,
                );
                assert!(h.notices.iter().any(|notice| matches!(
                    notice,
                    Notice::CommandDone {
                        command: 992,
                        result: _
                    }
                )));
            } else {
                assert_eq!(error["canceledOrderIds"][0], "2334151155877155281");
            }
        }
    }
}
#[test]
fn app_protective_payload_with_matching_trigger_price_places_like_price_only() {
    let protected = || {
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
        h
    };
    let app = |kind: &str, price: f64, trigger: &str| {
        format!(
            r#"[{{"symbol":"E-BTC-USDT","side":"sell","type":"{kind}","notional":2,"timeInForce":"GoodTillCancel","reduceOnly":true,"price":{price}{trigger},"amount":0.001}}]"#
        )
    };
    for (kind, price) in [("stop_market", 1800.0), ("take_profit_market", 2200.0)] {
        let mut sent = vec![];
        for trigger in [format!(r#","triggerPrice":{price}"#), String::new()] {
            let mut h = protected();
            h.place(&app(kind, price, &trigger));
            assert!(!h.failed(), "{kind}");
            let mut row = body(&h.take(Method::Post, "/fapi/v2/order"));
            row.as_object_mut().unwrap().remove("clientOrderId");
            sent.push(row);
        }
        assert_eq!(sent[0]["triggerPrice"], price, "{kind}");
        assert_eq!(sent[0], sent[1], "{kind}");
    }
    let mut h = protected();
    h.place(&app("stop_market", 1800.0, r#","triggerPrice":1790"#));
    assert!(h.failed());
    assert_eq!(h.writes(), 0);
}

#[test]
fn failed_protective_census_read_reports_once_through_the_placement() {
    let (mut h, _) = protective_ack();
    let mut census = post_write_census(&mut h);
    h.notices.clear();
    // Reads may retry; exhaust every attempt of the post-write census.
    for _ in 0..10 {
        h.handle(
            100.0,
            Event::HttpFailed {
                request: census.id,
                error: "census transport failed",
            },
        );
        h.collect();
        match h
            .requests
            .iter()
            .position(|r| r.method == Method::Get && r.url.contains("/fapi/v2/openOrders"))
        {
            Some(index) => census = h.requests.remove(index),
            None => break,
        }
    }
    assert!(h.failed(), "an uncertain protective placement fails");
    let error = h
        .notices
        .iter()
        .position(|n| matches!(n, Notice::CommandError { command: 7, message } if message == "Bitrue order read for E-BTC-USDT failed"));
    // Transport text stays behind the private diagnostic boundary.
    assert!(
        !format!("{:?}", h.notices).contains("census transport failed"),
        "{:?}",
        h.notices
    );
    let done = h
        .notices
        .iter()
        .position(|n| matches!(n, Notice::CommandFailed { command: 7, .. }));
    assert!(error.is_some() && error < done, "{:?}", h.notices);
    // An uncorrelated Error log would surface as a second toast beside Done Err.
    assert!(!h.notices.iter().any(|n| matches!(
        n,
        Notice::Event(safe_cex_engine::LogEvent {
            level: safe_cex_engine::Level::Error,
            ..
        })
    )));
}

#[test]
fn write_failures_are_classified_without_changing_their_text() {
    use safe_cex_engine::{ErrorKind, Outcome, Response};
    let unknown = (None, Some(Outcome::Unknown));
    for (status, body, classified) in [
        (
            200,
            r#"{"code":"-2017","msg":"insufficient balance","data":null}"#,
            (Some(ErrorKind::Rejected), Some(Outcome::Rejected)),
        ),
        // An undocumented code does not prove a refusal.
        (200, r#"{"code":"-9999","msg":"new"}"#, (None, None)),
        (
            429,
            r#"{"code":"-1003","msg":"too many"}"#,
            (Some(ErrorKind::RateLimited), Some(Outcome::Rejected)),
        ),
        (503, "busy", unknown),
        (200, r#"{"code":"0","data":{}}"#, unknown),
    ] {
        let mut h = Harness::new();
        h.place(
            r#"[{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.003,"price":2000}]"#,
        );
        let request = h.take(Method::Post, "/fapi/v2/order");
        h.handle(
            110.0,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
        h.collect();
        let [error] = std::mem::take(&mut h.failures).try_into().unwrap();
        assert_eq!(
            (error.kind, error.outcome),
            classified,
            "{status} {body}: {}",
            error.message
        );
        assert!(h.notices.iter().any(|n| matches!(
            n,
            Notice::CommandFailed { error: text, .. } if text.legacy_text() == error.legacy_text()
        )));
    }
    // Transport loss on a write is Unknown.
    let mut h = Harness::new();
    h.place(r#"[{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.003,"price":2000}]"#);
    let request = h.take(Method::Post, "/fapi/v2/order");
    h.handle(
        110.0,
        Event::HttpFailed {
            request: request.id,
            error: "reset",
        },
    );
    h.collect();
    assert_eq!(
        (h.failures[0].kind, h.failures[0].outcome),
        (Some(ErrorKind::Network), Some(Outcome::Unknown))
    );
}
