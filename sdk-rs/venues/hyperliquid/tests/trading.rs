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
        &format!(r#"{{"channel":"clearinghouseState","data":{STATE}}}"#),
    );
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
