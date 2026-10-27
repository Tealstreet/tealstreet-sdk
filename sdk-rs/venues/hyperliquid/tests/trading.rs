//! Command contract cases derived from the official exchange endpoint shapes.
mod support;
use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Response};
use safe_cex_venue_hyperliquid::Hyperliquid;
use serde_json::Value;
use support::prepared_now;
use support::*;
const CLOID: &str = "0x00000000000000000000000000000001";
#[test]
fn all_actual_bracket_cloids_are_receipted_before_the_signed_write() {
    let (mut e, _) = ready("disabled");
    e.handle(2000., Event::Command(Command::PlaceOrders { command: 91, orders: vec![opts(&format!(r#"{{"symbol":"BTC-PERP","type":"limit","side":"buy","price":90,"amount":2,"stopLoss":80,"takeProfit":120,"clientOrderId":"{CLOID}"}}"#))] }));
    let actions = e.drain_plain().collect::<Vec<_>>();
    let (at, receipts) = actions
        .iter()
        .enumerate()
        .find_map(|(i, action)| match action {
            Action::Notice(Notice::CommandProgress {
                command: 91,
                receipts,
            }) => Some((i, receipts)),
            _ => None,
        })
        .unwrap();
    let (write_at, request) = actions
        .iter()
        .enumerate()
        .find_map(|(i, action)| match action {
            Action::Http(request) if request.url.ends_with("/exchange") => Some((i, request)),
            _ => None,
        })
        .unwrap();
    assert!(at < write_at);
    let payload = body(request);
    let wires = payload["action"]["orders"].as_array().unwrap();
    assert_eq!(receipts.len(), 3);
    for (receipt, wire) in receipts.iter().zip(wires) {
        assert_eq!(receipt.symbol, "BTC-PERP");
        assert_eq!(
            receipt.kind,
            safe_cex_engine::OrderIdentityKind::ClientOrderId
        );
        assert_eq!(receipt.identity, wire["c"].as_str().unwrap());
    }
    assert_eq!(receipts[0].identity, CLOID);
    assert_ne!(receipts[1].identity, receipts[2].identity);
}
fn opts(text: &str) -> PlaceOrderOpts {
    serde_json::from_str(text).unwrap()
}
fn write(e: &mut Engine<Hyperliquid>) -> HttpRequest {
    fire_timers(e)
        .into_iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                (r.url.ends_with("/exchange")).then_some(r)
            } else {
                None
            }
        })
        .unwrap()
}
fn body(r: &HttpRequest) -> Value {
    serde_json::from_str(r.body.as_deref().unwrap()).unwrap()
}
#[test]
fn bracket_honors_caller_id_and_mints_unique_leg_ids() {
    let (mut e, _) = ready("disabled");
    e.handle(2000.0,Event::Command(Command::PlaceOrders{command:1,orders:vec![opts(&format!(r#"{{"symbol":"BTC-PERP","type":"limit","side":"buy","price":90,"amount":2,"stopLoss":80,"takeProfit":120,"clientOrderId":"{CLOID}"}}"#))]}));
    let r = write(&mut e);
    let b = body(&r);
    let a = &b["action"];
    assert_eq!(a["grouping"], "normalTpsl");
    let wires = a["orders"].as_array().unwrap();
    assert_eq!(wires.len(), 3);
    assert_eq!(wires[0]["c"], CLOID);
    assert_ne!(wires[1]["c"], wires[0]["c"]);
    assert_ne!(wires[2]["c"], wires[1]["c"]);
    assert_eq!(a["builder"]["f"], 10);
    assert_eq!(
        a["builder"]["b"],
        "0xea629e1e8534b82f704a818643f2daf38cb7fd4d"
    );
    let raw = r.body.unwrap();
    assert!(raw.find("\"t\"").unwrap() < raw.find("\"c\"").unwrap());
}
#[test]
fn invalid_or_duplicate_caller_id_rejects_entire_batch_before_a_write() {
    for id in ["0x0000000000000000000000000000000F", CLOID] {
        let (mut e, _) = ready("disabled");
        let make = |cloid: &str| {
            opts(&format!(
                r#"{{"symbol":"BTC-PERP","type":"limit","side":"buy","price":90,"amount":1,"clientOrderId":"{cloid}"}}"#
            ))
        };
        e.handle(
            2000.0,
            Event::Command(Command::PlaceOrders {
                command: 1,
                orders: vec![make(CLOID), make(id)],
            }),
        );
        let actions = e.drain_plain().collect::<Vec<_>>();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
        );
    }
}
#[test]
fn bybit_controls_reject_the_whole_batch_before_any_write() {
    for (field, value) in [
        (
            "bybitBbo",
            serde_json::json!({"sideType":"Queue","level":1}),
        ),
        ("bybitSlippageToleranceTicks", serde_json::json!(2)),
    ] {
        let (mut e, _) = ready("disabled");
        let good =
            opts(r#"{"symbol":"BTC-PERP","type":"limit","side":"buy","price":90,"amount":1}"#);
        let mut unsupported: Value = serde_json::to_value(&good).unwrap();
        unsupported[field] = value;
        e.handle(
            2000.0,
            Event::Command(Command::PlaceOrders {
                command: 1,
                orders: vec![good, serde_json::from_value(unsupported).unwrap()],
            }),
        );
        let actions = e.drain_plain().collect::<Vec<_>>();
        assert!(
            !actions.iter().any(|a| matches!(a, Action::Http(_))),
            "{field} emitted a write"
        );
        assert!(actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandFailed { command: 1, error })
                if error.legacy_text().contains("Bybit")
        )));
    }
}

#[test]
fn full_position_stop_without_oid_is_an_accepted_setting() {
    let (mut e, _) = ready("disabled");
    e.handle(2000.0,Event::Command(Command::PlaceOrders{command:1,orders:vec![opts(&format!(r#"{{"symbol":"BTC-PERP","type":"stop_market","side":"sell","price":80,"amount":2,"reduceOnly":true,"clientOrderId":"{CLOID}"}}"#))]}));
    let r = write(&mut e);
    let b = body(&r);
    assert_eq!(b["action"]["grouping"], "positionTpsl");
    assert_eq!(b["action"]["orders"][0]["s"], "0");
    assert_eq!(b["action"]["orders"][0]["p"], "95");
    reply(
        &mut e,
        &r,
        200,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":["waitingForTrigger"]}}}"#,
    );
    let actions = e.drain_plain().collect::<Vec<_>>();
    assert!(actions.iter().any(|a| matches!(a,
        Action::Notice(Notice::CommandDone { command: 1, result }) if result == "[]")));
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")))
    );
}
#[test]
fn own_bracket_leg_is_inferred_from_cloid_before_rest_returns() {
    let (mut e, s) = ready("disabled");
    e.handle(2000.0,Event::Command(Command::PlaceOrders{command:1,orders:vec![opts(r#"{"symbol":"BTC-PERP","type":"limit","side":"buy","price":90,"amount":2,"stopLoss":80}"#)]}));
    let r = write(&mut e);
    let b = body(&r);
    let c = b["action"]["orders"][1]["c"].as_str().unwrap();
    message(
        &mut e,
        s,
        2001.0,
        &format!(
            r#"{{"channel":"orderUpdates","data":[{{"order":{{"coin":"BTC","oid":9,"cloid":"{c}","side":"A","limitPx":"90","origSz":"2","sz":"2","reduceOnly":true}},"status":"open"}}]}}"#
        ),
    );
    let order = e.store().order("9").unwrap();
    assert_eq!(order.kind, safe_cex_engine::model::OrderType::StopLoss);
    assert_eq!(order.price, 80.0);
    assert_eq!(order.client_order_id.as_deref(), Some(c));
}
#[test]
fn lost_placement_response_reconciles_and_never_resends_write() {
    let (mut e, _) = ready("disabled");
    e.handle(
        2000.0,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![opts(
                r#"{"symbol":"BTC-PERP","type":"market","side":"buy","amount":1}"#,
            )],
        }),
    );
    let r = write(&mut e);
    assert_eq!(body(&r)["action"]["orders"][0]["p"], "105");
    e.handle(
        2001.0,
        Event::HttpFailed {
            request: r.id,
            error: "response lost",
        },
    );
    let actions = e.drain_plain().collect::<Vec<_>>();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a,Action::Http(r)if r.url.ends_with("/exchange")))
    );
    assert!(actions.iter().any(|a| matches!(a, Action::SetTimer { .. })));
}
#[test]
fn amend_cancels_then_replaces_with_fresh_cloid_and_tp_to_sl_only() {
    let (mut e, s) = ready("disabled");
    message(
        &mut e,
        s,
        2000.0,
        &format!(
            r#"{{"channel":"orderUpdates","data":[{{"order":{{"coin":"BTC","oid":9,"cloid":"{CLOID}","side":"A","limitPx":"95","origSz":"2","sz":"2","reduceOnly":true,"orderType":"Take Profit Market","triggerPx":"120"}},"status":"open"}}]}}"#
        ),
    );
    e.handle(
        2001.0,
        Event::Command(Command::AmendOrder {
            command: 2,
            id: "9".into(),
            price: Some(80.0),
            amount: None,
        }),
    );
    let r = write(&mut e);
    assert_eq!(body(&r)["action"]["type"], "cancel");
    reply(
        &mut e,
        &r,
        200,
        r#"{"status":"ok","response":{"type":"cancel","data":{"statuses":["success"]}}}"#,
    );
    let r = write(&mut e);
    let b = body(&r);
    let wire = &b["action"]["orders"][0];
    assert_ne!(wire["c"], CLOID);
    assert_eq!(wire["t"]["trigger"]["tpsl"], "sl");
    assert_eq!(wire["t"]["trigger"]["triggerPx"], "80");
    reply(
        &mut e,
        &r,
        200,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":["waitingForTrigger"]}}}"#,
    );
    assert_eq!(b["action"]["grouping"], "positionTpsl");
    assert_eq!(wire["s"], "0");
    let actions = e.drain_plain().collect::<Vec<_>>();
    let result = actions
        .iter()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandDone { command: 2, result }) => {
                Some(serde_json::from_str::<Value>(result).unwrap())
            }
            _ => None,
        })
        .expect("confirmed full-position replacement succeeds");
    assert_eq!(result["orderIds"], serde_json::json!([]));
    assert_eq!(result["clientOrderId"], wire["c"]);
    assert!(e.store().order("9").is_none());
    assert!(!actions.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandFailed { .. }) | Action::Http(_)
    )));
}
#[test]
fn malformed_cancel_ack_stops_replacement_and_keeps_original() {
    let (mut e, s) = ready("disabled");
    message(
        &mut e,
        s,
        2000.0,
        r#"{"channel":"orderUpdates","data":[{"order":{"coin":"BTC","oid":9,"side":"B","limitPx":"90","origSz":"1","sz":"1","orderType":"Limit","tif":"Gtc"},"status":"open"}]}"#,
    );
    e.handle(
        2001.0,
        Event::Command(Command::AmendOrder {
            command: 2,
            id: "9".into(),
            price: Some(85.0),
            amount: None,
        }),
    );
    let r = write(&mut e);
    reply(
        &mut e,
        &r,
        200,
        r#"{"status":"ok","response":{"type":"default"}}"#,
    );
    let actions = e.drain_plain().collect::<Vec<_>>();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a,Action::Http(r)if r.url.ends_with("/exchange")))
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
    );
    assert_eq!(
        e.store().order("9").unwrap().status,
        safe_cex_engine::model::OrderStatus::Open
    );
}
#[test]
fn queued_leverage_uses_margin_mode_from_preceding_command() {
    let (mut e, _) = ready("disabled");
    e.handle(
        2000.0,
        Event::Command(Command::SetMarginType {
            command: 1,
            symbol: "BTC-PERP".into(),
            isolated: true,
        }),
    );
    let first = write(&mut e);
    e.handle(
        2001.0,
        Event::Command(Command::SetLeverage {
            command: 2,
            symbol: "BTC-PERP".into(),
            leverage: 5.0,
            side: None,
        }),
    );
    reply(
        &mut e,
        &first,
        200,
        r#"{"status":"ok","response":{"type":"default"}}"#,
    );
    let second = write(&mut e);
    let b = body(&second);
    assert_eq!(b["action"]["isCross"], false);
    assert_eq!(b["action"]["leverage"], 5);
    assert!(!second.body.as_ref().unwrap().contains("5.0"));
}
#[test]
fn configured_builder_pair_requires_the_configured_approval_rate() {
    for (fee, expected) in [(30, true), (50, false)] {
        let (mut e, _) = ready_custom(
            "disabled",
            false,
            Some(safe_cex_venue_hyperliquid::Builder {
                b: "0x95d699952Fd9B110ef1218369def8538CCbB094B".into(),
                f: fee,
            }),
            "30",
        );
        e.handle(
            2000.0,
            Event::Command(Command::PlaceOrders {
                command: 1,
                orders: vec![opts(
                    r#"{"symbol":"BTC-PERP","type":"limit","side":"buy","price":90,"amount":1}"#,
                )],
            }),
        );
        let r = write(&mut e);
        let b = body(&r);
        assert_eq!(!b["action"]["builder"].is_null(), expected);
        if expected {
            assert_eq!(b["action"]["builder"]["f"], fee);
        }
    }
}

#[test]
fn uncertain_margin_write_blocks_dependent_settings_until_fresh_account_state() {
    for failure in [0, 1, 2] {
        let (mut e, socket) = ready("disabled");
        e.handle(
            2000.0,
            Event::Command(Command::SetMarginType {
                command: 1,
                symbol: "BTC-PERP".into(),
                isolated: true,
            }),
        );
        let first = write(&mut e);
        e.handle(
            2001.0,
            Event::Command(Command::SetLeverage {
                command: 2,
                symbol: "BTC-PERP".into(),
                leverage: 5.0,
                side: None,
            }),
        );
        if failure == 1 {
            e.handle(2002.0, Event::HttpResponse { request: first.id, response: Response {
                status: 200, headers: "", body: r#"{"status":"ok","response":{"type":"order","data":{"statuses":[]}}}"#,
            }});
        } else if failure == 2 {
            e.handle(
                2002.0,
                Event::HttpResponse {
                    request: first.id,
                    response: Response {
                        status: 503,
                        headers: "",
                        body: "unavailable",
                    },
                },
            );
        } else {
            e.handle(
                2002.0,
                Event::HttpFailed {
                    request: first.id,
                    error: "response lost",
                },
            );
        }
        let mut actions = e.drain_plain().collect::<Vec<_>>();
        let mut timers = actions
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                _ => None,
            })
            .collect::<Vec<_>>();
        timers.sort_by(|a, b| a.1.total_cmp(&b.1));
        for (timer, at_ms) in timers {
            e.handle(at_ms, Event::TimerFired { timer });
            actions.extend(e.drain_plain());
        }
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")))
        );
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 2, error }) if error.legacy_text().contains("settings are uncertain")))
        );
        message(
            &mut e,
            socket,
            2900.0,
            &format!(r#"{{"channel":"clearinghouseState","data":{STATE}}}"#),
        );
        e.handle(
            3000.0,
            Event::Command(Command::SetMarginType {
                command: 3,
                symbol: "BTC-PERP".into(),
                isolated: false,
            }),
        );
        assert!(
            e.drain_plain()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 3, ref error }) if error.legacy_text().contains("settings are uncertain")))
        );

        let fresh = STATE.replace("cross", "isolated").replace("1000}", "3500}");
        message(
            &mut e,
            socket,
            3500.0,
            &format!(r#"{{"channel":"clearinghouseState","data":{fresh}}}"#),
        );
        e.handle(
            3501.0,
            Event::Command(Command::SetLeverage {
                command: 4,
                symbol: "BTC-PERP".into(),
                leverage: 5.0,
                side: None,
            }),
        );
        let recovered = write(&mut e);
        assert_eq!(body(&recovered)["action"]["isCross"], false);
        assert_eq!(body(&recovered)["action"]["leverage"], 5);
    }
}

#[test]
fn stale_account_snapshot_cannot_reverse_acknowledged_margin_change() {
    let (mut e, socket) = ready("disabled");
    e.handle(
        2000.0,
        Event::Command(Command::SetMarginType {
            command: 1,
            symbol: "BTC-PERP".into(),
            isolated: true,
        }),
    );
    let first = write(&mut e);
    e.handle(
        2001.0,
        Event::Command(Command::SetLeverage {
            command: 2,
            symbol: "BTC-PERP".into(),
            leverage: 5.0,
            side: None,
        }),
    );
    e.handle(
        2002.0,
        Event::HttpResponse {
            request: first.id,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"status":"ok","response":{"type":"default"}}"#,
            },
        },
    );
    let pending = e.drain_plain().collect::<Vec<_>>();
    message(
        &mut e,
        socket,
        2003.0,
        &format!(
            r#"{{"channel":"clearinghouseState","data":{}}}"#,
            STATE
                .replace("1000}", "1500}")
                .replace(r#""szi":"2""#, r#""szi":"3""#)
        ),
    );
    assert_eq!(e.store().positions()[0].contracts, 3.0);
    assert_eq!(
        e.store().setting("BTC-PERP").unwrap().margin_type,
        Some(safe_cex_engine::model::MarginType::Isolated)
    );
    assert_eq!(
        e.store().positions()[0].margin_type,
        Some(safe_cex_engine::model::MarginType::Isolated)
    );
    let actions = fire_actions(&mut e, pending);
    let second = action_write(&actions);
    assert_eq!(body(second)["action"]["isCross"], false);
}

#[test]
fn missing_installed_signer_refuses_cancel_without_changing_order_status() {
    let wallet = "0x14791697260e4c9a71f18484c9f997b308e59325";
    let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(
        safe_cex_venue_hyperliquid::Options {
            key: Some(wallet.into()),
            application_id: Some(wallet.into()),
            credentials: None,
            ..Default::default()
        },
    ));
    e.start(1000.0);
    for _ in 0..20 {
        let requests = e
            .drain_plain()
            .filter_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .collect::<Vec<_>>();
        if requests.is_empty() {
            break;
        }
        for request in requests {
            let fixture = match kind(&request).as_str() {
                "metaAndAssetCtxs" => META,
                "perpDexs" => "[null]",
                "userAbstraction" => r#""disabled""#,
                "maxBuilderFee" => "30",
                "userFees" => r#"{"userCrossRate":"0.00045","userAddRate":"0.00015"}"#,
                "clearinghouseState" => STATE,
                "frontendOpenOrders" => {
                    r#"[{"coin":"BTC","oid":9,"side":"B","limitPx":"90","origSz":"1","sz":"1","orderType":"Limit","tif":"Gtc"}]"#
                }
                other => panic!("unexpected read {other}"),
            };
            reply(&mut e, &request, 200, fixture);
        }
    }
    assert_eq!(
        e.store().order("9").unwrap().status,
        safe_cex_engine::model::OrderStatus::Open
    );
    e.handle(
        2000.0,
        Event::Command(Command::CancelOrders {
            command: 1,
            ids: vec!["9".into()],
        }),
    );
    let actions = e.drain_plain().collect::<Vec<_>>();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 1, .. })))
    );
    assert_eq!(
        e.store().order("9").unwrap().status,
        safe_cex_engine::model::OrderStatus::Open
    );
}

fn reply_at(e: &mut Engine<Hyperliquid>, request: &HttpRequest, now: f64, raw: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: raw,
            },
        },
    );
}
// Follow only emitted readiness/pacing timers, never the live 20-second call timeout.
fn fire_timers(e: &mut Engine<Hyperliquid>) -> Vec<Action> {
    let actions = e.drain_plain().collect();
    fire_actions(e, actions)
}
fn fire_actions(e: &mut Engine<Hyperliquid>, actions: Vec<Action>) -> Vec<Action> {
    let end = safe_cex_engine::test_support::dispatch_now(e) + 334.0;
    support::advance_packet_timers(e, actions, end)
}
fn action_write(actions: &[Action]) -> &HttpRequest {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with("/exchange") => Some(r),
            _ => None,
        })
        .unwrap()
}
fn open_order(e: &mut Engine<Hyperliquid>, socket: safe_cex_engine::SocketId, now: f64) {
    message(
        e,
        socket,
        now,
        r#"{"channel":"orderUpdates","data":[{"order":{"coin":"BTC","oid":9,"side":"B","limitPx":"90","origSz":"1","sz":"1","orderType":"Limit","tif":"Gtc"},"status":"open"}]}"#,
    );
}

#[test]
fn queued_amend_reserves_source_and_rechecks_terminal_order_before_cancel() {
    for terminal_before_cancel in [false, true] {
        let (mut e, socket) = ready("disabled");
        open_order(&mut e, socket, 1900.0);
        e.handle(
            2000.0,
            Event::Command(Command::SetLeverage {
                command: 1,
                symbol: "ETH-PERP".into(),
                leverage: 5.0,
                side: None,
            }),
        );
        let earlier = write(&mut e);
        e.handle(
            2001.0,
            Event::Command(Command::AmendOrder {
                command: 2,
                id: "9".into(),
                price: Some(85.0),
                amount: None,
            }),
        );
        if terminal_before_cancel {
            message(
                &mut e,
                socket,
                2002.0,
                r#"{"channel":"orderUpdates","data":[{"order":{"coin":"BTC","oid":9,"side":"B","limitPx":"90","origSz":"1","sz":"0"},"status":"canceled"}]}"#,
            );
        }
        e.handle(
            2003.0,
            Event::Command(Command::AmendOrder {
                command: 3,
                id: "9".into(),
                price: Some(80.0),
                amount: None,
            }),
        );
        let rejected = e.drain_plain().collect::<Vec<_>>();
        assert!(rejected.iter().any(|a| matches!(a,
            Action::Notice(Notice::CommandFailed { command: 3, error })
            if error.legacy_text() == "Order amendment or cancellation is already in flight")));
        assert!(!rejected.iter().any(|a| matches!(a, Action::Http(_))));
        reply_at(
            &mut e,
            &earlier,
            2004.0,
            r#"{"status":"ok","response":{"type":"default"}}"#,
        );
        let actions = fire_timers(&mut e);
        if terminal_before_cancel {
            assert!(
                !actions
                    .iter()
                    .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")))
            );
            assert!(actions.iter().any(|a| matches!(a,
                Action::Notice(Notice::CommandFailed { command: 2, error })
                if error.legacy_text().contains("no longer open"))));
        } else {
            let cancel = action_write(&actions);
            assert_eq!(body(cancel)["action"]["cancels"][0]["o"], 9);
            reply_at(
                &mut e,
                cancel,
                2400.0,
                r#"{"status":"ok","response":{"type":"cancel","data":{"statuses":["success"]}}}"#,
            );
            let actions = fire_timers(&mut e);
            let replacement = action_write(&actions);
            assert_eq!(body(replacement)["action"]["orders"][0]["p"], "85");
            reply_at(
                &mut e,
                replacement,
                2800.0,
                r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":10}}]}}}"#,
            );
            assert!(
                !e.drain_plain()
                    .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")))
            );
        }
    }
}

#[test]
fn resting_core_ack_can_be_canceled_before_websocket_update() {
    let (mut e, socket) = ready("disabled");
    let (_replacement, _) = reconnect(&mut e, socket, 1900.0);
    let older_orders = e
        .drain_plain()
        .find_map(|a| match a {
            Action::Http(r) if kind(&r) == "frontendOpenOrders" => Some(r),
            _ => None,
        })
        .unwrap();
    e.handle(
        3000.0,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![opts(
                r#"{"symbol":"BTC-PERP","type":"limit","side":"buy","price":90,"amount":1}"#,
            )],
        }),
    );
    let placed = write(&mut e);
    let cloid = body(&placed)["action"]["orders"][0]["c"]
        .as_str()
        .unwrap()
        .to_owned();
    reply_at(
        &mut e,
        &placed,
        3001.0,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":9}}]}}}"#,
    );
    reply_at(&mut e, &older_orders, 3001.5, "[]");
    let acknowledged = e.store().order("9").unwrap();
    assert_eq!(
        acknowledged.status,
        safe_cex_engine::model::OrderStatus::Open
    );
    assert_eq!(acknowledged.symbol, "BTC-PERP");
    assert_eq!(
        acknowledged.client_order_id.as_deref(),
        Some(cloid.as_str())
    );
    assert_eq!(acknowledged.price, 90.0);
    e.drain_plain().for_each(drop);
    e.handle(
        3002.0,
        Event::Command(Command::CancelOrders {
            command: 2,
            ids: vec!["9".into()],
        }),
    );
    let actions = fire_timers(&mut e);
    let cancel = action_write(&actions);
    assert_eq!(body(cancel)["action"]["type"], "cancel");
    assert_eq!(
        body(cancel)["action"]["cancels"][0],
        serde_json::json!({"a":0,"o":9})
    );
}

#[test]
fn unlisted_settings_error_fences_the_next_queued_command() {
    let (mut e, _) = ready("disabled");
    e.handle(
        2000.0,
        Event::Command(Command::SetMarginType {
            command: 1,
            symbol: "BTC-PERP".into(),
            isolated: true,
        }),
    );
    let first = write(&mut e);
    e.handle(
        2001.0,
        Event::Command(Command::SetLeverage {
            command: 2,
            symbol: "BTC-PERP".into(),
            leverage: 5.0,
            side: None,
        }),
    );
    reply_at(
        &mut e,
        &first,
        2002.0,
        r#"{"status":"err","response":"Cannot switch margin mode with open orders"}"#,
    );
    let actions = fire_timers(&mut e);
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 1, error }) if error.legacy_text() == "Cannot switch margin mode with open orders"))
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")))
    );
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 2, error }) if error.legacy_text().contains("settings are uncertain"))));
    assert!(actions.iter().any(|a| matches!(a, Action::Http(r) if kind(r) == "activeAssetData" || kind(r) == "clearinghouseState")));
}

#[test]
fn uncertain_flat_settings_ignore_untimed_stream_until_targeted_rest_recovers() {
    let (mut e, socket) = ready("disabled");
    e.handle(
        2000.0,
        Event::Command(Command::SetMarginType {
            command: 1,
            symbol: "ETH-PERP".into(),
            isolated: true,
        }),
    );
    let first = write(&mut e);
    e.handle(
        2001.0,
        Event::HttpFailed {
            request: first.id,
            error: "lost after execution",
        },
    );
    let actions = e.drain_plain().collect::<Vec<_>>();
    let asset = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if kind(r) == "activeAssetData" => Some(r.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(body(&asset)["coin"], "ETH");
    message(
        &mut e,
        socket,
        2400.0,
        r#"{"channel":"activeAssetData","data":{"coin":"ETH","leverage":{"type":"cross","value":20}}}"#,
    );
    e.handle(
        2401.0,
        Event::Command(Command::SetLeverage {
            command: 2,
            symbol: "ETH-PERP".into(),
            leverage: 5.0,
            side: None,
        }),
    );
    assert!(
        e.drain_plain()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 2, ref error }) if error.legacy_text().contains("settings are uncertain")))
    );
    reply_at(
        &mut e,
        &asset,
        2500.0,
        r#"{"coin":"ETH","leverage":{"type":"isolated","value":20}}"#,
    );
    e.handle(
        2501.0,
        Event::Command(Command::SetLeverage {
            command: 3,
            symbol: "ETH-PERP".into(),
            leverage: 5.0,
            side: None,
        }),
    );
    let recovered = write(&mut e);
    assert_eq!(body(&recovered)["action"]["isCross"], false);
    assert_eq!(body(&recovered)["action"]["leverage"], 5);
}

#[test]
fn prewrite_rest_revision_blocks_future_clock_settings_but_updates_position_size() {
    let (mut e, socket) = ready("disabled");
    let (_replacement, _) = reconnect(&mut e, socket, 1900.0);
    let snapshot = e
        .drain_plain()
        .find_map(|a| match a {
            Action::Http(r) if kind(&r) == "clearinghouseState" => Some(r),
            _ => None,
        })
        .unwrap();
    e.handle(
        3000.0,
        Event::Command(Command::SetMarginType {
            command: 1,
            symbol: "BTC-PERP".into(),
            isolated: true,
        }),
    );
    let first = write(&mut e);
    reply_at(
        &mut e,
        &first,
        3002.0,
        r#"{"status":"ok","response":{"type":"default"}}"#,
    );
    let future = STATE
        .replace("1000}", "4000}")
        .replace(r#""szi":"2""#, r#""szi":"3""#);
    reply_at(&mut e, &snapshot, 3003.0, &future);
    assert_eq!(e.store().positions()[0].contracts, 3.0);
    assert_eq!(
        e.store().setting("BTC-PERP").unwrap().margin_type,
        Some(safe_cex_engine::model::MarginType::Isolated)
    );
    assert_eq!(
        e.store().positions()[0].margin_type,
        Some(safe_cex_engine::model::MarginType::Isolated)
    );
}

#[test]
fn terminal_cancel_miss_aborts_amend_but_regular_cancel_remains_idempotent() {
    for amend in [false, true] {
        for raw in [
            r#"{"status":"err","response":"Order was never placed, already canceled, or filled. asset=0"}"#,
            r#"{"status":"ok","response":{"type":"cancel","data":{"statuses":[{"error":"Order was never placed, already canceled, or filled. asset=0"}]}}}"#,
        ] {
            let (mut e, socket) = ready("disabled");
            open_order(&mut e, socket, 1900.0);
            let command = if amend {
                Command::AmendOrder {
                    command: 1,
                    id: "9".into(),
                    price: Some(85.0),
                    amount: None,
                }
            } else {
                Command::CancelOrders {
                    command: 1,
                    ids: vec!["9".into()],
                }
            };
            e.handle(2000.0, Event::Command(command));
            let cancel = write(&mut e);
            reply_at(&mut e, &cancel, 2001.0, raw);
            let actions = fire_timers(&mut e);
            assert!(
                !actions
                    .iter()
                    .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")))
            );
            assert!(actions.iter().any(|a| matches!(a,
                Action::Notice(Notice::CommandFailed { command: 1, .. }) if amend)
                || !amend && matches!(a, Action::Notice(Notice::CommandDone { command: 1, .. }))));
        }
    }
}

#[test]
fn filled_source_aborts_inflight_amend_but_own_cancel_frame_allows_replacement() {
    for terminal in ["filled", "canceled"] {
        let (mut e, socket) = ready("disabled");
        open_order(&mut e, socket, 1900.0);
        e.handle(
            2000.0,
            Event::Command(Command::AmendOrder {
                command: 1,
                id: "9".into(),
                price: Some(85.0),
                amount: None,
            }),
        );
        let cancel = write(&mut e);
        message(
            &mut e,
            socket,
            2001.0,
            &format!(
                r#"{{"channel":"orderUpdates","data":[{{"order":{{"coin":"BTC","oid":9,"side":"B","limitPx":"90","origSz":"1","sz":"0"}},"status":"{terminal}"}}]}}"#
            ),
        );
        reply_at(
            &mut e,
            &cancel,
            2002.0,
            r#"{"status":"ok","response":{"type":"cancel","data":{"statuses":["success"]}}}"#,
        );
        let actions = fire_timers(&mut e);
        if terminal == "filled" {
            assert!(
                !actions
                    .iter()
                    .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")))
            );
            assert!(actions.iter().any(|a| matches!(a,
                Action::Notice(Notice::CommandFailed { command: 1, error })
                if error.legacy_text().contains("source was filled"))));
        } else {
            let replacement = action_write(&actions);
            assert_eq!(body(replacement)["action"]["orders"][0]["p"], "85");
        }
    }
}

#[test]
fn delayed_source_fill_after_replacement_dispatch_preserves_confirmed_replacement_ids() {
    let (mut e, socket) = ready("disabled");
    open_order(&mut e, socket, 1900.0);
    e.handle(
        2000.0,
        Event::Command(Command::AmendOrder {
            command: 1,
            id: "9".into(),
            price: Some(85.0),
            amount: None,
        }),
    );
    let cancel = write(&mut e);
    reply_at(
        &mut e,
        &cancel,
        2001.0,
        r#"{"status":"ok","response":{"type":"cancel","data":{"statuses":["success"]}}}"#,
    );
    let actions = fire_timers(&mut e);
    let replacement = action_write(&actions);
    let cloid = body(replacement)["action"]["orders"][0]["c"]
        .as_str()
        .unwrap()
        .to_owned();
    message(
        &mut e,
        socket,
        2400.0,
        r#"{"channel":"orderUpdates","data":[{"order":{"coin":"BTC","oid":9,"side":"B","limitPx":"90","origSz":"1","sz":"0"},"status":"filled"}]}"#,
    );
    reply_at(
        &mut e,
        replacement,
        2401.0,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":10}}]}}}"#,
    );
    let terminal = e.drain_plain().collect::<Vec<_>>();
    let result = terminal
        .into_iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone { command: 1, result }) => Some(result),
            _ => None,
        })
        .unwrap();
    let result: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(result["orderIds"], serde_json::json!(["10"]));
    assert_eq!(result["clientOrderId"], cloid);
    assert!(e.store().order("10").is_some());
}

#[test]
fn same_signer_engines_and_vaults_share_one_action_nonce_sequence() {
    let (mut a, _) = ready("disabled");
    let (mut b, _) = ready_identity(
        "disabled",
        false,
        None,
        "30",
        Some("0x2222222222222222222222222222222222222222".into()),
    );
    for e in [&mut a, &mut b] {
        e.handle(
            2000.0,
            Event::Command(Command::PlaceOrders {
                command: 901,
                orders: vec![opts(
                    r#"{"symbol":"BTC-PERP","type":"limit","side":"buy","amount":1,"price":100}"#,
                )],
            }),
        );
    }
    let first = body(&write(&mut a))["nonce"].as_u64().unwrap();
    let second = body(&write(&mut b))["nonce"].as_u64().unwrap();
    assert!(second > first);
}

#[test]
fn cancellation_missing_from_store_errors_before_any_write_even_in_a_mixed_batch() {
    use safe_cex_engine::model::{Order, OrderSide, OrderStatus, OrderType};
    use safe_cex_engine::{Action, Notice};
    for mixed in [false, true] {
        let (mut e, _) = ready("disabled");
        e.store_mut().set_orders(vec![Order {
            id: "123".into(),
            parent_id: None,
            status: OrderStatus::Open,
            symbol: "BTC-PERP".into(),
            kind: OrderType::Limit,
            side: OrderSide::Buy,
            price: 90.0,
            amount: 1.0,
            filled: 0.0,
            remaining: 1.0,
            reduce_only: false,
            close: None,
            client_order_id: None,
            time_in_force: None,
            last_update: 0.0,
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: None,
        }]);
        e.drain_plain().for_each(drop);
        e.handle(
            2000.0,
            Event::Command(Command::CancelOrders {
                command: 991,
                ids: if mixed {
                    vec!["123".into(), "456".into()]
                } else {
                    vec!["456".into()]
                },
            }),
        );
        let actions = e.drain_plain().collect::<Vec<_>>();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(
            actions.iter().any(|a| matches!(
                a,
                Action::Notice(Notice::CommandFailed { command: 991, error })
                    if error.legacy_text().contains("456") && error.legacy_text().contains("not found")
            )),
            "missing cancellation error: {actions:?}"
        );
        assert_eq!(e.store().order("123").unwrap().status, OrderStatus::Open);
    }
}
#[test]
fn flat_isolated_only_hip3_leverage_write_is_isolated_like_ts() {
    let (mut e, _) = ready_with_dex("disabled", true);
    e.handle(
        2000.0,
        Event::Command(Command::SetLeverage {
            command: 1,
            symbol: "XYZ|FOO-PERP".into(),
            leverage: 3.0,
            side: None,
        }),
    );
    let b = body(&write(&mut e));
    assert_eq!(b["action"]["type"], "updateLeverage");
    assert_eq!(b["action"]["isCross"], false);
    assert_eq!(b["action"]["leverage"], 3);
}
const CROSS_HIP3_META: &str = r#"[{"universe":[{"name":"xyz:FOO","szDecimals":2,"maxLeverage":10}]},[{"midPx":"50","markPx":"50","oraclePx":"50"}]]"#;
#[test]
fn flat_cross_capable_hip3_is_seeded_and_written_isolated_like_ts() {
    use safe_cex_engine::model::MarginType;
    let (mut e, _) = ready_with_hip3_meta(CROSS_HIP3_META);
    let seed = e.store().setting("XYZ|FOO-PERP").unwrap();
    assert_eq!(seed.margin_type, Some(MarginType::Isolated));
    assert_eq!(seed.leverage, 10.0);
    e.handle(
        2000.0,
        Event::Command(Command::SetLeverage {
            command: 1,
            symbol: "XYZ|FOO-PERP".into(),
            leverage: 3.0,
            side: None,
        }),
    );
    let b = body(&write(&mut e));
    assert_eq!(b["action"]["type"], "updateLeverage");
    assert_eq!(b["action"]["isCross"], false);
    assert_eq!(b["action"]["leverage"], 3);
}
#[test]
fn venue_cross_hip3_position_settings_win_over_the_isolated_seed() {
    use safe_cex_engine::model::MarginType;
    let (mut e, s) = ready_with_hip3_meta(CROSS_HIP3_META);
    message(
        &mut e,
        s,
        1500.0,
        r#"{"channel":"clearinghouseState","data":{"dex":"xyz","clearinghouseState":{"assetPositions":[{"position":{"coin":"xyz:FOO","szi":"2","entryPx":"50","positionValue":"100","unrealizedPnl":"0","leverage":{"type":"cross","value":7}}}],"time":1500}}}"#,
    );
    let setting = e.store().setting("XYZ|FOO-PERP").unwrap();
    assert_eq!(setting.margin_type, Some(MarginType::Cross));
    assert_eq!(setting.leverage, 7.0);
    e.handle(
        2000.0,
        Event::Command(Command::SetLeverage {
            command: 1,
            symbol: "XYZ|FOO-PERP".into(),
            leverage: 3.0,
            side: None,
        }),
    );
    assert_eq!(body(&write(&mut e))["action"]["isCross"], true);
}
#[test]
fn flat_hip3_margin_change_uses_a_leverage_within_the_market_maximum() {
    let (mut e, _) = ready_with_dex("disabled", true);
    e.handle(
        2000.0,
        Event::Command(Command::SetMarginType {
            command: 1,
            symbol: "XYZ|FOO-PERP".into(),
            isolated: true,
        }),
    );
    let b = body(&write(&mut e));
    assert_eq!(b["action"]["isCross"], false);
    assert_eq!(b["action"]["leverage"], 10);
}
#[test]
fn cross_on_an_isolated_only_market_is_rejected_before_signing() {
    let (mut e, _) = ready_with_dex("disabled", true);
    e.handle(
        2000.0,
        Event::Command(Command::SetMarginType {
            command: 1,
            symbol: "XYZ|FOO-PERP".into(),
            isolated: false,
        }),
    );
    let out: Vec<_> = e.drain_plain().collect();
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")))
    );
    assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { error: m, .. }) if m.legacy_text().contains("isolated margin only"))));
}

#[test]
fn write_failures_are_classified_without_changing_their_text() {
    use safe_cex_engine::{ErrorKind, Outcome};
    let rejected = (Some(ErrorKind::Rejected), Some(Outcome::Unknown));
    let unknown = (Some(ErrorKind::Internal), Some(Outcome::Unknown));
    let limit = r#"{"symbol":"BTC-PERP","type":"limit","side":"buy","price":90,"amount":2}"#;
    for (status, body, classified) in [
        (
            200,
            r#"{"status":"err","response":"Insufficient margin"}"#,
            unknown,
        ),
        (
            200,
            r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"error":"Order price too far"}]}}}"#,
            rejected,
        ),
        (
            200,
            r#"{"status":"ok","response":{"type":"order","data":{"statuses":["mystery"]}}}"#,
            rejected,
        ),
        (200, "<html>", unknown),
        (
            502,
            "bad gateway",
            (Some(ErrorKind::VenueUnavailable), Some(Outcome::Unknown)),
        ),
        (
            429,
            "rate limited",
            (Some(ErrorKind::RateLimited), Some(Outcome::Unknown)),
        ),
    ] {
        let (mut e, _) = ready("disabled");
        e.handle(
            2000.,
            Event::Command(Command::PlaceOrders {
                command: 1,
                orders: vec![opts(limit)],
            }),
        );
        let r = write(&mut e);
        reply(&mut e, &r, status, body);
        let [error] = failures(&mut e).try_into().unwrap();
        assert_eq!(
            (error.kind, error.outcome),
            classified,
            "{status} {body}: {}",
            error.message
        );
    }
    // The second order is refused after the first rested: nothing to claim.
    let (mut e, _) = ready("disabled");
    e.handle(
        2000.,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![opts(limit), opts(limit)],
        }),
    );
    let r = write(&mut e);
    reply(
        &mut e,
        &r,
        200,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":1}}]}}}"#,
    );
    let timer = pace_timer(&mut e);
    e.handle(3000., Event::TimerFired { timer });
    let r = write(&mut e);
    reply(
        &mut e,
        &r,
        200,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"error":"Order price too far"}]}}}"#,
    );
    let [error] = failures(&mut e).try_into().unwrap();
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    assert_eq!(error.partial.as_ref().unwrap().order_ids, ["1"]);
    // Transport loss on a write is Unknown.
    let (mut e, _) = ready("disabled");
    e.handle(
        2000.,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![opts(limit)],
        }),
    );
    let r = write(&mut e);
    e.handle(
        2100.,
        Event::HttpFailed {
            request: r.id,
            error: "reset",
        },
    );
    let [error] = failures(&mut e).try_into().unwrap();
    assert_eq!(
        (error.kind, error.outcome),
        (Some(ErrorKind::Network), Some(Outcome::Unknown))
    );
}

fn pace_timer(e: &mut Engine<Hyperliquid>) -> safe_cex_engine::TimerId {
    e.drain_plain()
        .filter_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .next_back()
        .expect("pacing timer")
}

#[test]
fn perp_zero_bracket_prices_disable_only_the_requested_child() {
    for (stop, take, children) in [
        ("0", "0", Vec::<&str>::new()),
        ("0", "120", vec!["tp"]),
        ("80", "0", vec!["sl"]),
    ] {
        let (mut e, _) = ready("disabled");
        e.handle(2000., Event::Command(Command::PlaceOrders { command: 91, orders: vec![opts(&format!(r#"{{"symbol":"BTC-PERP","type":"limit","side":"buy","price":90,"amount":2,"stopLoss":{stop},"takeProfit":{take}}}"#))] }));
        let payload = body(&write(&mut e));
        let wires = payload["action"]["orders"].as_array().unwrap();
        assert_eq!(wires.len(), 1 + children.len(), "sl {stop} tp {take}");
        assert_eq!(
            payload["action"]["grouping"],
            if children.is_empty() {
                "na"
            } else {
                "normalTpsl"
            }
        );
        for (wire, kind) in wires.iter().skip(1).zip(children) {
            assert_eq!(wire["t"]["trigger"]["tpsl"], kind);
        }
    }
}

#[test]
fn schedule_control_shares_ordinary_write_queue_and_nonce_without_settings_mutation() {
    let (mut e, _) = ready("disabled");
    e.handle(
        2000.0,
        Event::Command(Command::SetLeverage {
            command: 71,
            symbol: "BTC-PERP".into(),
            leverage: 5.0,
            side: None,
        }),
    );
    let first = write(&mut e);
    let nonce = body(&first)["nonce"].as_u64().unwrap();
    let schedule =
        safe_cex_engine::command::decode_command(r#"{"op":"scheduleCancel","command":72}"#)
            .unwrap();
    e.handle(2001.0, Event::Command(schedule));
    assert!(
        !prepared_now(&mut e)
            .into_iter()
            .any(|a| matches!(a, Action::Http(_)))
    );
    reply(
        &mut e,
        &first,
        200,
        r#"{"status":"ok","response":{"type":"default"}}"#,
    );
    let control = write(&mut e);
    let control_body = body(&control);
    assert_eq!(
        control_body["action"],
        serde_json::json!({"type":"scheduleCancel"})
    );
    assert!(control_body["nonce"].as_u64().unwrap() > nonce);
    let positions = serde_json::to_value(e.store().positions()).unwrap();
    reply(
        &mut e,
        &control,
        200,
        r#"{"status":"ok","response":{"type":"default"}}"#,
    );
    let actions: Vec<_> = e.drain_plain().collect();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 72, .. })))
    );
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert_eq!(
        serde_json::to_value(e.store().positions()).unwrap(),
        positions
    );
}

#[path = "support/beta_batch_outcomes.rs"]
mod beta_batch_outcomes;

#[test]
fn settings_engine_seal_joins_old_read_but_only_a_post_write_read_recovers() {
    let (mut e, socket) = ready("disabled");
    let set = |e: &mut Engine<Hyperliquid>, command, now| {
        e.handle(
            now,
            Event::Command(Command::SetLeverage {
                command,
                symbol: "ETH-PERP".into(),
                leverage: 5.0,
                side: None,
            }),
        );
    };
    set(&mut e, 101, 2000.0);
    let first = write(&mut e);
    e.handle(
        2001.0,
        Event::HttpFailed {
            request: first.id,
            error: "lost",
        },
    );
    let actions = e.drain_plain().collect::<Vec<_>>();
    let old = actions
        .iter()
        .find_map(|action| match action {
            Action::Http(r) if kind(r) == "activeAssetData" => Some(r.clone()),
            _ => None,
        })
        .expect("real targeted settings read");
    let fresh = STATE.replace("BTC", "ETH").replace("1000}", "2100}");
    message(
        &mut e,
        socket,
        2100.0,
        &format!(r#"{{"channel":"clearinghouseState","data":{fresh}}}"#),
    );
    e.drain_plain().for_each(drop);
    set(&mut e, 102, 2500.0);
    let second = write(&mut e);
    e.handle(
        2501.0,
        Event::HttpResponse {
            request: second.id,
            response: Response {
                status: 503,
                headers: "",
                body: r#"{"status":"err","response":"unavailable"}"#,
            },
        },
    );
    let actions = e.drain_plain().collect::<Vec<_>>();
    assert!(actions.iter().any(|action| matches!(action, Action::Notice(Notice::CommandFailed { command: 102, error }) if error.outcome == Some(safe_cex_engine::Outcome::Unknown))));
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(r) if kind(r) == "activeAssetData")),
        "join the real in-flight read"
    );
    reply_at(
        &mut e,
        &old,
        2600.0,
        r#"{"coin":"ETH","leverage":{"type":"cross","value":20}}"#,
    );
    let actions = e.drain_plain().collect::<Vec<_>>();
    let reads = actions
        .iter()
        .filter_map(|action| match action {
            Action::Http(r) if kind(r) == "activeAssetData" => Some(r.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        reads.len(),
        1,
        "stale completion starts one fresh successor"
    );
    assert_eq!(e.store().setting("ETH-PERP").unwrap().leverage, 10.0);
    set(&mut e, 103, 2601.0);
    let actions = e.drain_plain().collect::<Vec<_>>();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")))
    );
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 103, error }) if error.legacy_text().contains("settings are uncertain"))));
    reply_at(
        &mut e,
        &reads[0],
        2700.0,
        r#"{"coin":"ETH","leverage":{"type":"isolated","value":5}}"#,
    );
    e.drain_plain().for_each(drop);
    set(&mut e, 104, 2900.0);
    let recovered = write(&mut e);
    assert_eq!(body(&recovered)["action"]["leverage"], 5);
    assert_eq!(body(&recovered)["action"]["isCross"], false);
}
