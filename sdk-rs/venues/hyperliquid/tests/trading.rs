//! Command contract cases derived from the official exchange endpoint shapes.
mod support;
use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Response};
use safe_cex_venue_hyperliquid::Hyperliquid;
use serde_json::Value;
use support::*;
const CLOID: &str = "0x00000000000000000000000000000001";
fn opts(text: &str) -> PlaceOrderOpts {
    serde_json::from_str(text).unwrap()
}
fn write(e: &mut Engine<Hyperliquid>) -> HttpRequest {
    e.drain_actions()
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
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandDone { result: Err(_), .. })
        )));
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
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert!(
            !actions.iter().any(|a| matches!(a, Action::Http(_))),
            "{field} emitted a write"
        );
        assert!(actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandDone { command: 1, result: Err(error) })
                if error.contains("Bybit")
        )));
    }
}

#[test]
fn full_position_stop_accepts_waiting_without_oid() {
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
    assert!(e.drain_actions().any(
        |a| matches!(a,Action::Notice(Notice::CommandDone{result:Ok(ref ids),..})if ids=="[]")
    ));
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
    let actions = e.drain_actions().collect::<Vec<_>>();
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
    let timer = e
        .drain_actions()
        .find_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(2500.0, Event::TimerFired { timer });
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
    let result = e
        .drain_actions()
        .find_map(|a| {
            if let Action::Notice(Notice::CommandDone { result: Ok(s), .. }) = a {
                Some(s)
            } else {
                None
            }
        })
        .unwrap();
    let result: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(result["clientOrderId"], wire["c"]);
    assert_eq!(result["orderIds"], serde_json::json!([]));
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
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a,Action::Http(r)if r.url.ends_with("/exchange")))
    );
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { result: Err(_), .. })
    )));
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
        }),
    );
    reply(
        &mut e,
        &first,
        200,
        r#"{"status":"ok","response":{"type":"default"}}"#,
    );
    let timer = e
        .drain_actions()
        .find_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(2500.0, Event::TimerFired { timer });
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
    for malformed_reply in [false, true] {
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
            }),
        );
        if malformed_reply {
            e.handle(2002.0, Event::HttpResponse { request: first.id, response: Response {
                status: 200, headers: "", body: r#"{"status":"ok","response":{"type":"order","data":{"statuses":[]}}}"#,
            }});
        } else {
            e.handle(
                2002.0,
                Event::HttpFailed {
                    request: first.id,
                    error: "response lost",
                },
            );
        }
        let mut actions = e.drain_actions().collect::<Vec<_>>();
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
            actions.extend(e.drain_actions());
        }
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")))
        );
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandDone {
            command: 2, result: Err(error),
        }) if error.contains("settings are uncertain")))
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
            e.drain_actions()
                .any(|a| matches!(a, Action::Notice(Notice::CommandDone {
            command: 3, result: Err(ref error),
        }) if error.contains("settings are uncertain")))
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
    let (timer, at_ms) = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .unwrap();
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
    e.handle(at_ms, Event::TimerFired { timer });
    let second = write(&mut e);
    assert_eq!(body(&second)["action"]["isCross"], false);
}

#[test]
fn signing_failure_restores_cancel_status_without_sending_a_request() {
    let wallet = "0x14791697260e4c9a71f18484c9f997b308e59325";
    let mut e = Engine::new(Hyperliquid::new(safe_cex_venue_hyperliquid::Options {
        key: Some(wallet.into()),
        application_id: Some(wallet.into()),
        secret: Some("0x00".into()),
        ..Default::default()
    }));
    e.start(1000.0);
    for _ in 0..20 {
        let requests = e
            .drain_actions()
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
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone {
            command: 1,
            result: Err(_),
        })
    )));
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
fn fire_timers(e: &mut Engine<Hyperliquid>) -> Vec<Action> {
    let mut actions = e.drain_actions().collect::<Vec<_>>();
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
        actions.extend(e.drain_actions());
    }
    actions
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
        let rejected = e.drain_actions().collect::<Vec<_>>();
        assert!(rejected.iter().any(|a| matches!(a,
            Action::Notice(Notice::CommandDone { command: 3, result: Err(error) })
            if error.contains("amendment pending"))));
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
                Action::Notice(Notice::CommandDone { command: 2, result: Err(error) })
                if error.contains("no longer open"))));
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
                !e.drain_actions()
                    .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")))
            );
        }
    }
}

#[test]
fn resting_core_ack_can_be_canceled_before_websocket_update() {
    let (mut e, socket) = ready("disabled");
    e.handle(1900.0, Event::WsOpened { socket });
    let older_orders = e
        .drain_actions()
        .find_map(|a| match a {
            Action::Http(r) if kind(&r) == "frontendOpenOrders" => Some(r),
            _ => None,
        })
        .unwrap();
    e.handle(
        2000.0,
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
        2001.0,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":9}}]}}}"#,
    );
    reply_at(&mut e, &older_orders, 2001.5, "[]");
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
    e.drain_actions().for_each(drop);
    e.handle(
        2002.0,
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
fn definite_settings_rejection_keeps_next_queued_command_usable() {
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
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone {
        command: 1, result: Err(error),
    }) if error == "Cannot switch margin mode with open orders"))
    );
    let second = action_write(&actions);
    assert_eq!(body(second)["action"]["isCross"], true);
    assert_eq!(body(second)["action"]["leverage"], 5);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if kind(r) == "activeAssetData"))
    );
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
    let actions = e.drain_actions().collect::<Vec<_>>();
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
        }),
    );
    assert!(
        e.drain_actions()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone {
        command: 2, result: Err(ref error),
    }) if error.contains("settings are uncertain")))
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
        }),
    );
    let recovered = write(&mut e);
    assert_eq!(body(&recovered)["action"]["isCross"], false);
    assert_eq!(body(&recovered)["action"]["leverage"], 5);
}

#[test]
fn prewrite_rest_revision_blocks_future_clock_settings_but_updates_position_size() {
    let (mut e, socket) = ready("disabled");
    e.handle(1900.0, Event::WsOpened { socket });
    let snapshot = e
        .drain_actions()
        .find_map(|a| match a {
            Action::Http(r) if kind(&r) == "clearinghouseState" => Some(r),
            _ => None,
        })
        .unwrap();
    e.handle(
        2000.0,
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
        2002.0,
        r#"{"status":"ok","response":{"type":"default"}}"#,
    );
    let future = STATE
        .replace("1000}", "4000}")
        .replace(r#""szi":"2""#, r#""szi":"3""#);
    reply_at(&mut e, &snapshot, 2003.0, &future);
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
                Action::Notice(Notice::CommandDone { command: 1, result }) if result.is_err() == amend)));
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
                Action::Notice(Notice::CommandDone { command: 1, result: Err(error) })
                if error.contains("source was filled"))));
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
    let result = e
        .drain_actions()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone { command: 1, result }) => Some(result.unwrap()),
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
