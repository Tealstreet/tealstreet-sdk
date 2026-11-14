mod support;
use safe_cex_engine::model::*;
use safe_cex_engine::request_context::{Claim, HttpAction, Prepared};
use safe_cex_engine::{Action, Command, Event, Method, Outcome};
use serde_json::Value;
use support::*;

fn uta() -> Harness {
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
    while h
        .requests
        .iter()
        .any(|r| r.url.contains("/unfilled-strategy-orders"))
    {
        h.respond(
            "/unfilled-strategy-orders",
            200,
            r#"{"code":"00000","data":[]}"#,
        );
    }
    h.login();
    h
}
fn edit(id: &str, price: Option<f64>, amount: Option<f64>) -> Command {
    Command::AmendOrder {
        command: 801,
        id: id.into(),
        price,
        amount,
    }
}
fn parsed(h: &mut Harness, path: &str) -> (safe_cex_engine::RequestId, Value) {
    let r = h.take(path);
    (
        r.id,
        serde_json::from_str(r.body.as_deref().unwrap()).unwrap(),
    )
}
fn full_stop(h: &mut Harness) {
    h.private_message(r#"{"arg":{"topic":"strategy-order"},"data":[{"category":"USDT-FUTURES","orderId":"full","symbol":"BTCUSDT","tpslMode":"full","qty":"0","side":"sell","posSide":"long","stopLoss":"80","slTriggerBy":"market","slOrderType":"market","status":"pending"}]}"#);
}
fn trailing(h: &mut Harness) {
    h.private_message(r#"{"arg":{"topic":"strategy-order"},"data":[{"category":"USDT-FUTURES","type":"trailing_stop","orderId":"trail","symbol":"BTCUSDT","qty":"0.01","side":"sell","posSide":"long","reduceOnly":"yes","status":"pending","trailingStopParams":{"activationPrice":"95","activationType":"market","trailType":"distance","trailVariance":"5","preOrderType":"market"}}]}"#);
}
fn held(h: &mut Harness) -> safe_cex_engine::request_context::Permit {
    held_call(h, "cancel").0
}
fn held_call(
    h: &mut Harness,
    path: &str,
) -> (
    safe_cex_engine::request_context::Permit,
    safe_cex_engine::HttpRequest,
) {
    for _ in 0..8 {
        h.engine.prepare_dispatch(h.now);
        let actions: Vec<_> = h.engine.drain_actions().collect();
        assert!(!actions.is_empty(), "real cancel preparation is reached");
        for action in actions {
            match action {
                Action::ManagedHttp(a) => match *a {
                    HttpAction::Prepare {
                        preparation,
                        request,
                        ..
                    } => {
                        assert!(request.url.contains(path));
                        h.engine.http_prepared(
                            h.now,
                            preparation,
                            Prepared {
                                id: request.id.0,
                                expires_at: None,
                            },
                        );
                    }
                    HttpAction::Bind {
                        permit, request, ..
                    } => {
                        assert!(request.url.contains(path));
                        return (permit, request);
                    }
                    _ => {}
                },
                action => h.absorb(vec![action]),
            }
        }
    }
    panic!("no held {path} binding")
}

fn native_amend_rechecks_sibling(uta_mode: bool) {
    for update in ["unchanged", "receipt", "sibling"] {
        let mut h = if uta_mode { uta() } else { Harness::classic() };
        let row = if uta_mode {
            r#"{"arg":{"topic":"strategy-order"},"data":[{"category":"USDT-FUTURES","orderId":"pair","symbol":"BTCUSDT","tpslMode":"partial","qty":"0.5","side":"sell","posSide":"long","stopLoss":"80","slTriggerBy":"market","slOrderType":"market","takeProfit":"120","tpTriggerBy":"market","tpOrderType":"market","status":"pending"}]}"#
        } else {
            r#"{"arg":{"channel":"orders"},"data":[{"orderId":"1","symbol":"BTCUSDT","size":"2","baseVolume":"0","price":"90","side":"buy","orderType":"limit","status":"live","presetStopLossPrice":"80","presetStopSurplusPrice":"120"}]}"#
        };
        h.private_message(row);
        let (target, sibling, path, field) = if uta_mode {
            (
                "pair__stop_loss",
                "pair__take_profit",
                "/modify-strategy-order",
                "takeProfit",
            )
        } else {
            (
                "1__stop_loss",
                "1__take_profit",
                "/modify-order",
                "newPresetStopSurplusPrice",
            )
        };
        h.engine
            .handle(h.now, Event::Command(edit(target, Some(85.0), None)));
        let (permit, request) = held_call(&mut h, path);
        let payload: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        assert_eq!(payload[field], "120");
        if update != "unchanged" {
            h.now += 1.0;
            let text = if update == "sibling" {
                row.replace("\"120\"", "\"130\"")
            } else {
                row.to_owned()
            };
            let socket = h
                .sockets
                .iter()
                .find(|(_, url)| url.ends_with("/private"))
                .unwrap()
                .0;
            h.engine.handle(
                h.now,
                Event::WsMessage {
                    socket,
                    text: &text,
                },
            );
        }
        let sibling_price = if update == "sibling" { 130.0 } else { 120.0 };
        assert_eq!(
            h.engine.store().order(sibling).unwrap().price,
            sibling_price
        );
        let claim = h.engine.claim_http(h.now, permit);
        assert_eq!(
            matches!(claim, Ok(Claim::Released(_))),
            update != "sibling",
            "uta={uta_mode}, {update}: {claim:?}"
        );
        if update == "sibling" {
            h.collect();
            let error = h
                .failures
                .last()
                .expect("stale amendment refused before sending");
            assert_eq!(error.outcome, Some(Outcome::NotSent));
            assert_eq!(error.protection, None);
            assert!(error.legacy_text().contains("no amendment sent"));
            assert!(!h.requests.iter().any(|r| r.method == Method::Post));
            assert_eq!(h.engine.store().order(sibling).unwrap().price, 130.0);
        }
    }
}

#[test]
fn native_uta_amend_rechecks_sibling_at_final_claim() {
    native_amend_rechecks_sibling(true);
}

#[test]
fn classic_preset_amend_rechecks_sibling_at_final_claim() {
    native_amend_rechecks_sibling(false);
}

#[test]
fn classic_attached_edit_modifies_presets_without_canceling_parent_or_sibling() {
    let mut h = Harness::classic();
    h.private_message(r#"{"arg":{"channel":"orders"},"data":[{"orderId":"1","symbol":"BTCUSDT","size":"2","baseVolume":"0","price":"90","side":"buy","orderType":"limit","status":"live","presetStopLossPrice":"80","presetStopSurplusPrice":"120"}]}"#);
    h.command(edit("1__stop_loss", Some(85.0), None));
    let (id, p) = parsed(&mut h, "/modify-order");
    assert_eq!(p["orderId"], "1");
    assert_eq!(p["newPresetStopLossPrice"], "85");
    assert_eq!(p["newPresetStopSurplusPrice"], "120");
    assert!(p.get("newSize").is_none());
    assert!(p.get("newPrice").is_none());
    assert!(!h.requests.iter().any(|r| r.url.contains("cancel")));
    h.reply(
        id,
        200,
        r#"{"code":"40017","msg":"invalid preset","data":null}"#,
    );
    assert!(!h.requests.iter().any(|r| r.method == Method::Post));
    for id in ["1", "1__stop_loss", "1__take_profit"] {
        assert_eq!(
            h.engine.store().order(id).unwrap().status,
            OrderStatus::Open
        );
    }
    assert!(h.failures[0].message.contains("unchanged"));
}

#[test]
fn uta_partial_pair_uses_native_modify_preserving_both_execution_types_and_sources() {
    let mut h = uta();
    h.private_message(r#"{"arg":{"topic":"strategy-order"},"data":[{"category":"USDT-FUTURES","orderId":"pair","symbol":"BTCUSDT","tpslMode":"partial","qty":"0.5","side":"sell","posSide":"long","stopLoss":"80","slTriggerBy":"mark","slOrderType":"limit","slLimitPrice":"79","takeProfit":"120","tpTriggerBy":"market","tpOrderType":"limit","tpLimitPrice":"121","status":"pending"}]}"#);
    h.command(edit("pair__stop_loss", Some(85.0), Some(0.6)));
    let (id, p) = parsed(&mut h, "/modify-strategy-order");
    assert_eq!(p["orderId"], "pair");
    assert_eq!(p["qty"], "0.6");
    assert_eq!(p["stopLoss"], "85");
    assert_eq!(p["takeProfit"], "120");
    assert_eq!(p["slTriggerBy"], "mark");
    assert_eq!(p["tpTriggerBy"], "market");
    assert_eq!(p["slOrderType"], "limit");
    assert_eq!(p["slLimitPrice"], "79");
    assert_eq!(p["tpOrderType"], "limit");
    assert_eq!(p["tpLimitPrice"], "121");
    h.reply(id, 200, r#"{"code":"00000","data":{"orderId":"pair"}}"#);
    assert!(h.done.last().unwrap().1.is_ok());
    assert_eq!(
        h.engine.store().order("pair__stop_loss").unwrap().price,
        85.0
    );
    assert_eq!(
        h.engine.store().order("pair__take_profit").unwrap().price,
        120.0
    );
    assert!(!h.requests.iter().any(|r| r.url.contains("cancel")));
}

#[test]
fn equal_venue_ticker_receipts_refresh_only_their_reported_sources() {
    let mut h = Harness::classic();
    h.message(r#"{"arg":{"channel":"ticker","instId":"BTCUSDT"},"data":[{"instId":"BTCUSDT","lastPr":"100","markPrice":"100"}]}"#);
    let before = h.now;
    h.now += 6000.0;
    h.message(r#"{"arg":{"channel":"ticker","instId":"BTCUSDT"},"data":[{"instId":"BTCUSDT","lastPr":"100"}]}"#);
    assert_eq!(
        h.engine
            .store()
            .reference_seen_at("BTCUSDT", StopTriggerType::LastPrice),
        Some(h.now)
    );
    assert_eq!(
        h.engine
            .store()
            .reference_seen_at("BTCUSDT", StopTriggerType::MarkPrice),
        Some(before)
    );
}

#[test]
fn final_cancel_veto_checks_source_freshness_and_new_trigger_at_the_claim() {
    for case in ["working", "stale", "crossed", "filled", "terminal"] {
        let mut h = uta();
        full_stop(&mut h);
        h.engine
            .handle(h.now, Event::Command(edit("full", Some(85.0), None)));
        h.now += if case == "stale" { 5001.0 } else { 5000.0 };
        let permit = held(&mut h);
        let now = h.now;
        match case {
            "crossed" => h.engine.handle(h.now, Event::WsMessage { socket: h.sockets[0].0, text: r#"{"arg":{"channel":"ticker","instId":"BTCUSDT"},"data":[{"instId":"BTCUSDT","lastPr":"84"}]}"# }),
            "filled" => {
                let mut row = h.engine.store().order("full").unwrap().clone();
                row.filled = 0.1;
                h.engine.store_mut().upsert_order(row);
            }
            "terminal" => {
                h.engine
                    .store_mut()
                    .set_order_status("full", OrderStatus::Closed);
            }
            _ => {}
        }
        let claim = h.engine.claim_http(now, permit);
        let released = matches!(claim, Ok(Claim::Released(_)));
        assert_eq!(released, case == "working", "{case}: {claim:?}");
        if !released {
            h.now = now;
            h.collect();
            assert_eq!(
                h.failures.last().unwrap().outcome,
                Some(Outcome::NotSent),
                "{case}"
            );
            assert!(!h.requests.iter().any(|r| r.method == Method::Post));
        }
    }
}

#[test]
fn signature_resend_cannot_bypass_the_final_cancel_veto() {
    let mut h = uta();
    full_stop(&mut h);
    h.command(edit("full", Some(85.0), None));
    let cancel = h.take("/cancel-strategy-order");
    h.engine.handle(
        h.now,
        Event::HttpResponse {
            request: cancel.id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body: r#"{"code":"40009","msg":"sign signature error","data":null}"#,
            },
        },
    );
    let permit = held(&mut h);
    h.engine.handle(h.now, Event::WsMessage { socket: h.sockets[0].0, text: r#"{"arg":{"channel":"ticker","instId":"BTCUSDT"},"data":[{"instId":"BTCUSDT","lastPr":"84"}]}"# });
    assert!(!matches!(
        h.engine.claim_http(h.now, permit),
        Ok(Claim::Released(_))
    ));
    h.collect();
    assert!(!h.requests.iter().any(|r| r.method == Method::Post));
}

#[test]
fn trailing_restore_is_one_shot_exact_and_typed_for_all_terminal_outcomes() {
    for (response, protection) in [
        (
            r#"{"code":"00000","data":{"orderId":"restored"}}"#,
            Some(safe_cex_engine::error::Protection::Restored),
        ),
        (
            r#"{"code":"40017","msg":"restore refused","data":null}"#,
            Some(safe_cex_engine::error::Protection::Unprotected),
        ),
        (r#"{"code":"25001","msg":"timeout","data":null}"#, None),
    ] {
        let mut h = uta();
        trailing(&mut h);
        h.command(edit("trail", Some(96.0), Some(0.02)));
        h.respond(
            "/cancel-strategy-order",
            200,
            r#"{"code":"00000","data":null}"#,
        );
        h.respond(
            "/place-strategy-order",
            200,
            r#"{"code":"40017","msg":"replace refused","data":null}"#,
        );
        let (id, p) = parsed(&mut h, "/place-strategy-order");
        assert_eq!(p["qty"], "0.01");
        assert_eq!(p["trailingStopParams"][0]["activationPrice"], "95");
        assert_eq!(p["trailingStopParams"][0]["trailVariance"], "5");
        h.reply(id, 200, response);
        let error = h.failures.last().unwrap();
        assert_eq!(error.protection, protection, "{error:?}");
        assert!(!h.requests.iter().any(|r| r.method == Method::Post));
        if protection == Some(safe_cex_engine::error::Protection::Unprotected) {
            assert!(error.message.contains("re-place"), "{error:?}");
        }
    }
}

#[test]
fn engine_sealed_put_back_keeps_registered_unknown_protection_words() {
    let mut h = uta();
    trailing(&mut h);
    h.command(edit("trail", Some(96.0), None));
    h.respond(
        "/cancel-strategy-order",
        200,
        r#"{"code":"00000","data":null}"#,
    );
    h.respond(
        "/place-strategy-order",
        200,
        r#"{"code":"40017","msg":"replace refused","data":null}"#,
    );
    h.respond(
        "/place-strategy-order",
        503,
        r#"{"code":"25000","msg":"service unavailable","data":null}"#,
    );
    let error = h.failures.last().unwrap();
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(error.protection, None);
    assert!(
        error.message.contains("put-back") && error.message.contains("check Open Orders"),
        "{error:?}"
    );
}

#[test]
fn size_limits_and_position_mode_are_rechecked_on_the_destructive_claim() {
    for case in ["working", "size", "mode"] {
        let mut h = uta();
        trailing(&mut h);
        h.engine
            .handle(h.now, Event::Command(edit("trail", Some(96.0), Some(0.02))));
        let permit = held(&mut h);
        if case == "size" {
            let mut markets = h.engine.store().markets().to_vec();
            markets[0].limits.amount.max = 0.015;
            h.engine.store_mut().set_markets(markets);
        }
        if case == "mode" {
            let hedged = h.engine.store().hedged();
            h.engine.store_mut().set_hedged(!hedged);
        }
        let claim = h.engine.claim_http(h.now, permit);
        assert_eq!(
            matches!(claim, Ok(Claim::Released(_))),
            case == "working",
            "{case}: {claim:?}"
        );
    }
}

#[test]
fn native_strategy_amend_requires_the_exact_id_and_never_falls_back_on_unknown() {
    for (status, body) in [
        (200, r#"{"code":"00000","data":{"orderId":"foreign"}}"#),
        (200, r#"{"code":"00000","data":null}"#),
        (200, r#"{"code":"25001","msg":"timeout","data":null}"#),
        (429, r#"{"code":"429","msg":"rate limited","data":null}"#),
    ] {
        let mut h = uta();
        h.private_message(r#"{"arg":{"topic":"strategy-order"},"data":[{"category":"USDT-FUTURES","orderId":"partial","symbol":"BTCUSDT","tpslMode":"partial","qty":"0.5","side":"sell","posSide":"long","stopLoss":"80","slTriggerBy":"market","slOrderType":"market","status":"pending"}]}"#);
        h.command(edit("partial", Some(85.0), None));
        h.respond("/modify-strategy-order", status, body);
        let error = h.failures.last().expect("unproven amend is surfaced");
        assert_eq!(error.outcome, Some(Outcome::Unknown));
        assert_eq!(error.protection, None);
        assert_eq!(h.engine.store().order("partial").unwrap().price, 80.0);
        assert!(!h.requests.iter().any(|r| r.method == Method::Post));
    }
}

#[test]
fn a_newer_streamed_strategy_row_wins_over_the_native_amend_ack() {
    let mut h = uta();
    let row = r#"{"arg":{"topic":"strategy-order"},"data":[{"category":"USDT-FUTURES","orderId":"partial","symbol":"BTCUSDT","tpslMode":"partial","qty":"0.5","side":"sell","posSide":"long","stopLoss":"80","slTriggerBy":"market","slOrderType":"market","status":"pending"}]}"#;
    h.private_message(row);
    h.command(edit("partial", Some(85.0), None));
    let request = h.take("/modify-strategy-order");
    h.private_message(&row.replace("\"80\"", "\"82\""));
    h.reply(
        request.id,
        200,
        r#"{"code":"00000","data":{"orderId":"partial"}}"#,
    );
    assert_eq!(h.engine.store().order("partial").unwrap().price, 82.0);
}

#[test]
fn unknown_execution_refuses_before_any_destructive_cancel() {
    let mut h = uta();
    h.private_message(r#"{"arg":{"topic":"strategy-order"},"data":[{"category":"USDT-FUTURES","orderId":"unknown","symbol":"BTCUSDT","tpslMode":"full","qty":"0","side":"sell","posSide":"long","stopLoss":"80","slTriggerBy":"market","status":"pending"}]}"#);
    h.command(edit("unknown", Some(85.0), None));
    assert_eq!(h.failures.last().unwrap().outcome, Some(Outcome::NotSent));
    assert!(
        h.failures
            .last()
            .unwrap()
            .message
            .contains("execution price is unavailable")
    );
    assert!(!h.requests.iter().any(|r| r.method == Method::Post));
}

#[test]
fn attached_size_edit_uses_veto_and_reports_unprotected_without_guessed_restore() {
    for status in [200, 429] {
        let mut h = uta();
        h.private_message(r#"{"arg":{"topic":"order"},"data":[{"category":"USDT-FUTURES","orderId":"entry","symbol":"BTCUSDT","qty":"1","cumExecQty":"0","price":"90","side":"buy","posSide":"long","orderType":"limit","orderStatus":"live","stopLoss":"80","takeProfit":"120","slTriggerBy":"market","tpTriggerBy":"market"}]}"#);
        h.command(edit("entry__stop_loss", Some(85.0), Some(0.5)));
        h.respond(
            "/cancel-order",
            200,
            r#"{"code":"00000","data":{"orderId":"entry"}}"#,
        );
        h.respond(
            "/place-order",
            status,
            if status == 200 {
                r#"{"code":"40017","msg":"replace refused","data":null}"#
            } else {
                r#"{"code":"429","msg":"rate limited","data":null}"#
            },
        );
        let error = h.failures.last().unwrap();
        assert_eq!(
            error.protection,
            if status == 200 {
                Some(safe_cex_engine::error::Protection::Unprotected)
            } else {
                None
            }
        );
        assert!(!h.requests.iter().any(|r| r.method == Method::Post));
        if status == 200 {
            assert!(error.message.contains("re-place"));
        }
    }
}

#[test]
fn trailing_put_back_keeps_original_venue_size_even_after_position_shrinks() {
    let mut h = uta();
    trailing(&mut h);
    let mut positions = h.engine.store().positions().to_vec();
    positions[0].contracts = 0.005;
    h.engine.store_mut().set_positions(positions);
    h.command(edit("trail", Some(96.0), Some(0.002)));
    h.respond(
        "/cancel-strategy-order",
        200,
        r#"{"code":"00000","data":null}"#,
    );
    h.respond(
        "/place-strategy-order",
        200,
        r#"{"code":"40017","msg":"replace refused","data":null}"#,
    );
    let (_, p) = parsed(&mut h, "/place-strategy-order");
    assert_eq!(p["qty"], "0.01");
}

#[test]
fn frame_fed_cancel_needs_a_genuine_symbol_quote_not_a_cached_frame_stamp() {
    for case in [
        "working",
        "stale",
        "wrong-symbol",
        "conflicting-symbol",
        "malformed",
    ] {
        let mut source = uta();
        full_stop(&mut source);
        let frame = source.engine.snapshot_frame(source.now).to_vec();
        let mut h = Harness::started_configure(
            FixtureOptions {
                key: Some("key".into()),
                secret: Some("secret".into()),
                passphrase: Some("pass".into()),
                ..FixtureOptions::default()
            },
            false,
            |e| e.set_frame_fed().unwrap(),
        );
        h.respond("/contracts", 200, CONTRACTS);
        h.now += 60_000.0;
        h.engine.apply_frame(h.now, &frame, true).unwrap();
        h.collect();
        h.command(edit("full", Some(85.0), None));
        assert!(!h.requests.iter().any(|r| r.method == Method::Post));
        let body = match case {
            "wrong-symbol" => TICKERS.replace("BTCUSDT", "ETHUSDT"),
            "conflicting-symbol" => TICKERS.replace(
                "\"symbol\":\"BTCUSDT\"",
                "\"symbol\":\"BTCUSDT\",\"instId\":\"ETHUSDT\"",
            ),
            "malformed" => "not-json".into(),
            _ => TICKERS.into(),
        };
        let request = h.take("/tickers");
        h.engine.handle(
            h.now,
            Event::HttpResponse {
                request: request.id,
                response: safe_cex_engine::Response {
                    status: 200,
                    headers: "",
                    body: &body,
                },
            },
        );
        if matches!(case, "wrong-symbol" | "conflicting-symbol" | "malformed") {
            h.collect();
            assert_eq!(h.failures.last().unwrap().outcome, Some(Outcome::NotSent));
        } else {
            if case == "stale" {
                h.now += 5001.0;
            }
            let permit = held(&mut h);
            let claim = h.engine.claim_http(h.now, permit);
            assert_eq!(
                matches!(claim, Ok(Claim::Released(_))),
                case == "working",
                "{case}: {claim:?}"
            );
        }
    }
}

#[test]
fn a_store_only_refresh_cannot_lend_its_clock_to_an_old_price() {
    let mut h = uta();
    full_stop(&mut h);
    h.engine
        .handle(h.now, Event::Command(edit("full", Some(85.0), None)));
    h.now += 6000.0;
    h.engine.handle(
        h.now,
        Event::WsMessage {
            socket: h.sockets[0].0,
            text: "pong",
        },
    );
    h.engine
        .store_mut()
        .update_ticker("BTCUSDT", |ticker| ticker.last = 101.0);
    assert_eq!(
        h.engine
            .store()
            .reference_seen_at("BTCUSDT", StopTriggerType::LastPrice),
        Some(h.now)
    );
    let permit = held(&mut h);
    assert!(!matches!(
        h.engine.claim_http(h.now, permit),
        Ok(Claim::Released(_))
    ));
    h.collect();
    assert!(!h.requests.iter().any(|r| r.method == Method::Post));
    assert_eq!(h.failures.last().unwrap().outcome, Some(Outcome::NotSent));
}

#[test]
fn attached_notional_is_checked_at_the_entry_price_not_the_stop_trigger() {
    let mut h = uta();
    h.private_message(r#"{"arg":{"topic":"order"},"data":[{"category":"USDT-FUTURES","orderId":"entry","symbol":"BTCUSDT","qty":"1","cumExecQty":"0","price":"90","side":"buy","posSide":"long","orderType":"limit","orderStatus":"live","stopLoss":"80","takeProfit":"120","slTriggerBy":"market","tpTriggerBy":"market"}]}"#);
    h.command(edit("entry__stop_loss", Some(70.0), Some(0.058)));
    let (_, payload) = parsed(&mut h, "/cancel-order");
    assert_eq!(payload["orderId"], "entry");
    assert!(h.failures.is_empty());
}

#[test]
fn a_last_or_bid_receipt_cannot_refresh_a_mark_referenced_cancel() {
    for field in ["markPrice", "lastPr", "bidPr"] {
        let mut h = uta();
        h.private_message(r#"{"arg":{"topic":"strategy-order"},"data":[{"category":"USDT-FUTURES","orderId":"full","symbol":"BTCUSDT","tpslMode":"full","qty":"0","side":"sell","posSide":"long","stopLoss":"80","slTriggerBy":"mark","slOrderType":"market","status":"pending"}]}"#);
        h.message(r#"{"arg":{"channel":"ticker","instId":"BTCUSDT"},"data":[{"instId":"BTCUSDT","markPrice":"100"}]}"#);
        h.engine
            .handle(h.now, Event::Command(edit("full", Some(85.0), None)));
        h.now += 6000.0;
        h.engine.handle(h.now, Event::WsMessage {socket:h.sockets[0].0, text:&format!(r#"{{"arg":{{"channel":"ticker","instId":"BTCUSDT"}},"data":[{{"instId":"BTCUSDT","{field}":"100"}}]}}"#)});
        let permit = held(&mut h);
        assert_eq!(
            matches!(h.engine.claim_http(h.now, permit), Ok(Claim::Released(_))),
            field == "markPrice",
            "{field}"
        );
    }
}

#[test]
fn a_concurrent_user_edit_cannot_replace_the_stop_while_its_put_back_is_live() {
    let mut h = uta();
    trailing(&mut h);
    h.command(edit("trail", Some(96.0), None));
    h.respond(
        "/cancel-strategy-order",
        200,
        r#"{"code":"00000","data":null}"#,
    );
    h.respond(
        "/place-strategy-order",
        200,
        r#"{"code":"40017","msg":"replace refused","data":null}"#,
    );
    let restore = h.take("/place-strategy-order");
    h.command(Command::AmendOrder {
        command: 802,
        id: "trail".into(),
        price: Some(97.0),
        amount: None,
    });
    assert!(
        h.done
            .iter()
            .any(|(id, result)| *id == 802 && result.is_err())
    );
    assert!(!h.requests.iter().any(|r| r.method == Method::Post));
    h.reply(
        restore.id,
        200,
        r#"{"code":"00000","data":{"orderId":"restored"}}"#,
    );
    assert_eq!(
        h.failures.last().unwrap().protection,
        Some(safe_cex_engine::error::Protection::Restored)
    );
    assert!(!h.requests.iter().any(|r| r.method == Method::Post));
}

#[test]
fn a_changed_native_trailing_range_revokes_an_already_bound_cancel() {
    let mut h = uta();
    trailing(&mut h);
    h.engine
        .handle(h.now, Event::Command(edit("trail", Some(96.0), None)));
    let permit = held(&mut h);
    let socket = h
        .sockets
        .iter()
        .find(|(_, url)| url.ends_with("/private"))
        .unwrap()
        .0;
    h.engine.handle(h.now, Event::WsMessage {socket,text:r#"{"arg":{"topic":"strategy-order"},"data":[{"category":"USDT-FUTURES","type":"trailing_stop","orderId":"trail","symbol":"BTCUSDT","qty":"0.01","side":"sell","posSide":"long","reduceOnly":"yes","status":"pending","trailingStopParams":{"activationPrice":"95","activationType":"market","trailType":"distance","trailVariance":"7","preOrderType":"market"}}]}"#});
    assert!(!matches!(
        h.engine.claim_http(h.now, permit),
        Ok(Claim::Released(_))
    ));
    h.collect();
    assert_eq!(h.failures.last().unwrap().outcome, Some(Outcome::NotSent));
    assert!(!h.requests.iter().any(|r| r.method == Method::Post));
}

#[test]
fn an_unknown_guarded_cancel_never_sends_a_replacement_or_claims_lost_protection() {
    for (status, code) in [(429, 429), (200, 429), (200, 25000), (200, 25001)] {
        let mut h = uta();
        full_stop(&mut h);
        h.command(edit("full", Some(85.0), None));
        h.respond(
            "/cancel-strategy-order",
            status,
            &format!(r#"{{"code":"{code}","msg":"busy","data":null}}"#),
        );
        let error = h.failures.last().unwrap();
        assert_eq!(error.outcome, Some(Outcome::Unknown));
        assert_eq!(error.protection, None);
        assert_eq!(
            h.engine.store().order("full").unwrap().status,
            OrderStatus::Open
        );
        assert!(!h.requests.iter().any(|r| r.method == Method::Post));
    }
}

#[test]
fn a_client_only_or_conflicting_put_back_ack_cannot_claim_protection_restored() {
    for (full, reply) in [
        (false, r#"{"code":"00000","data":{"clientOid":"foreign"}}"#),
        (
            true,
            r#"{"code":"00000","data":{"orderId":"restored","clientOid":"foreign"}}"#,
        ),
    ] {
        let mut h = uta();
        if full {
            full_stop(&mut h);
        } else {
            trailing(&mut h);
        }
        h.command(edit(
            if full { "full" } else { "trail" },
            Some(if full { 85.0 } else { 96.0 }),
            None,
        ));
        h.respond(
            "/cancel-strategy-order",
            200,
            r#"{"code":"00000","data":null}"#,
        );
        h.respond(
            "/place-strategy-order",
            200,
            r#"{"code":"40017","msg":"replace refused","data":null}"#,
        );
        h.respond("/place-strategy-order", 200, reply);
        let error = h.failures.last().unwrap();
        assert_eq!(error.outcome, Some(Outcome::Unknown));
        assert_eq!(error.protection, None);
        assert!(!h.requests.iter().any(|r| r.method == Method::Post));
    }
}

#[test]
fn ambiguous_replies_never_retry_any_protective_edit_leg_or_claim_protection() {
    for leg in ["cancel", "replacement", "put-back", "native-amend"] {
        for (status, code) in [
            (429, "00000"),
            (429, "40009"),
            (429, "40017"),
            (408, "40017"),
            (503, "40009"),
            (200, "429"),
            (200, "25000"),
            (200, "25001"),
        ] {
            let mut h = uta();
            let native = leg == "native-amend";
            if native {
                h.private_message(r#"{"arg":{"topic":"strategy-order"},"data":[{"category":"USDT-FUTURES","orderId":"partial","symbol":"BTCUSDT","tpslMode":"partial","qty":"0.5","side":"sell","posSide":"long","stopLoss":"80","slTriggerBy":"market","slOrderType":"market","status":"pending"}]}"#);
            } else {
                full_stop(&mut h);
            }
            h.command(edit(
                if native { "partial" } else { "full" },
                Some(85.0),
                None,
            ));
            if matches!(leg, "replacement" | "put-back") {
                h.respond(
                    "/cancel-strategy-order",
                    200,
                    r#"{"code":"00000","data":null}"#,
                );
            }
            if leg == "put-back" {
                h.respond(
                    "/place-strategy-order",
                    200,
                    r#"{"code":"40017","msg":"replacement refused","data":null}"#,
                );
            }
            let path = match leg {
                "cancel" => "/cancel-strategy-order",
                "native-amend" => "/modify-strategy-order",
                _ => "/place-strategy-order",
            };
            // A success or signature-refusal body cannot override an ambiguous HTTP status.
            h.respond(
                path,
                status,
                &format!(r#"{{"code":"{code}","msg":"ambiguous","data":{{"orderId":"partial"}}}}"#),
            );
            let error = h.failures.last().expect("terminal protective edit failure");
            assert_eq!(
                error.outcome,
                Some(Outcome::Unknown),
                "{leg}/{status}/{code}: {error:?}"
            );
            assert_eq!(error.protection, None, "{leg}/{status}/{code}: {error:?}");
            assert!(
                !error.message.contains("original protection restored"),
                "{error:?}"
            );
            assert!(
                !error.message.contains("protection is not in place"),
                "{error:?}"
            );
            assert!(
                !h.requests.iter().any(|r| r.method == Method::Post),
                "{leg}/{status}/{code}: no resend, replacement or put-back after uncertainty"
            );
        }
    }
}

#[test]
fn protective_quote_age_stays_monotonic_across_protocol_utc_jumps() {
    const UTC: f64 = 1_800_000_000_000.0;
    for elapsed in [5000.0, 5001.0] {
        for correction in [-3_600_000.0, 3_600_000.0] {
            let mut h = uta();
            full_stop(&mut h);
            assert!(h.engine.sync_host_time(h.now, UTC));
            h.engine
                .handle(h.now, Event::Command(edit("full", Some(85.0), None)));
            h.now += elapsed;
            assert!(h.engine.sync_host_time(h.now, UTC + elapsed + correction));
            let permit = held(&mut h);
            let claim = h.engine.claim_http(h.now, permit);
            assert_eq!(
                matches!(claim, Ok(Claim::Released(_))),
                elapsed <= 5000.0,
                "quote age {elapsed}, UTC correction {correction}: {claim:?}"
            );
            if elapsed > 5000.0 {
                h.collect();
                assert_eq!(h.failures.last().unwrap().outcome, Some(Outcome::NotSent));
                assert!(!h.requests.iter().any(|r| r.method == Method::Post));
            }
        }
    }
}
