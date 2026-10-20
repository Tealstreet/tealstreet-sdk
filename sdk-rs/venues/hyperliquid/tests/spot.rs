//! Documented spot metadata, balances and spot asset ID payloads.
//! https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/info-endpoint/spot
mod support;
use safe_cex_engine::model::*;
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, Query, SocketId, Venue,
};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options};
const META: &str = r#"[{"tokens":[{"name":"USDC","index":0,"szDecimals":8},{"name":"PURR","index":1,"szDecimals":0},{"name":"HFUN","index":2,"szDecimals":2}],"universe":[{"name":"PURR/USDC","tokens":[1,0],"index":0},{"name":"@1","tokens":[2,0],"index":1}]},[{"coin":"@1","midPx":"0.00093722","markPx":"0.00094","prevDayPx":"0.0009","dayNtlVlm":"937.22"},{"coin":"PURR/USDC","midPx":"0.209265","markPx":"0.14","prevDayPx":"0.20432","dayNtlVlm":"8906.0"}]]"#;
fn ready(private: bool) -> (Engine<Hyperliquid>, SocketId) {
    let mut e = Engine::new(Hyperliquid::new_spot(Options {
        public_data_only: !private,
        key: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
        secret: Some("0x0123456789012345678901234567890123456789012345678901234567890123".into()),
        application_id: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
        ..Options::default()
    }));
    e.start(1000.0);
    let r = take_http(&mut e);
    assert_eq!(support::kind(&r), "spotMetaAndAssetCtxs");
    support::reply(&mut e, &r, 200, META);
    let mut socket = None;
    for a in e.drain_actions().collect::<Vec<_>>() {
        match a {
            Action::WsConnect { socket: s, .. } => socket = Some(s),
            Action::Http(r) => {
                assert_eq!(support::kind(&r), "spotClearinghouseState");
                support::reply(
                    &mut e,
                    &r,
                    200,
                    r#"{"balances":[{"coin":"USDC","token":0,"hold":"2","total":"14.625485"},{"coin":"PURR","token":1,"hold":"10","total":"2000"}]}"#,
                );
            }
            _ => {}
        }
    }
    let s = socket.unwrap();
    e.handle(1000.0, Event::WsOpened { socket: s });
    (e, s)
}
fn take_http(e: &mut Engine<Hyperliquid>) -> HttpRequest {
    e.drain_actions()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap()
}
#[test]
fn metadata_joins_by_coin_and_uses_token_size_precision() {
    let (e, _) = ready(false);
    let m = e.store().market("HFUN/USDC").unwrap();
    assert_eq!(m.id, "@1");
    assert_eq!(m.is_spot, Some(true));
    assert_eq!(m.precision.amount, 0.01);
    assert_eq!(m.precision.price, 0.000001);
    let t = e.store().ticker("HFUN/USDC").unwrap();
    assert!((t.volume - 1_000_000.0).abs() < 1e-6);
    assert!(e.store().positions().is_empty());
    assert!(
        e.store()
            .positions()
            .iter()
            .all(|p| p.contracts == 0.0 && p.leverage == 1.0 && p.side == PositionSide::Short)
    );
}
#[test]
fn spot_subscriptions_exclude_perpetual_channels() {
    let (mut e, _) = ready(true);
    let messages = e
        .drain_actions()
        .filter_map(|a| {
            if let Action::WsSend { text, .. } = a {
                Some(text)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert!(messages.iter().any(|s| s.contains("spotState")));
    for excluded in ["fastAssetCtxs", "clearinghouseState", "activeAssetData"] {
        assert!(!messages.iter().any(|s| s.contains(excluded)));
    }
}
#[test]
fn balances_keep_native_currency_units_and_mark_notional() {
    let (mut e, s) = ready(true);
    assert!((e.store().balance().total - (14.625485 + 280.0)).abs() < 1e-9);
    assert_eq!(
        e.store().currency_balances().unwrap()[1].balance.total,
        2000.0
    );
    support::message(
        &mut e,
        s,
        2000.0,
        r#"{"channel":"spotState","data":{"spotState":{"balances":[{"coin":"USDC","hold":"3","total":"20"}]}}}"#,
    );
    assert_eq!(e.store().balance().free, 17.0);
    assert_eq!(e.store().coin_balances().unwrap()[0].amount, 20.0);
}
#[test]
fn late_rest_balance_does_not_overwrite_newer_websocket() {
    let (mut e, s) = ready(true);
    support::message(
        &mut e,
        s,
        12_000.0,
        r#"{"channel":"userNonFundingLedgerUpdates","data":{}}"#,
    );
    let request = take_http(&mut e);
    support::message(
        &mut e,
        s,
        2000.0,
        r#"{"channel":"spotState","data":{"spotState":{"balances":[{"coin":"USDC","hold":"3","total":"20"}]}}}"#,
    );
    support::reply(
        &mut e,
        &request,
        200,
        r#"{"balances":[{"coin":"USDC","hold":"0","total":"1"}]}"#,
    );
    assert_eq!(e.store().balance().total, 20.0);
}
fn order(symbol: &str) -> PlaceOrderOpts {
    serde_json::from_str(&format!(r#"{{"symbol":"{symbol}","type":"limit","side":"buy","price":0.00093722,"amount":6208,"clientOrderId":"0x00000000000000000000000000000001"}}"#)).unwrap()
}
#[test]
fn signed_spot_order_uses_shifted_asset_and_eight_decimal_cap() {
    let (mut e, _) = ready(true);
    e.drain_actions().for_each(drop);
    e.handle(
        2000.0,
        Event::Command(Command::PlaceOrders {
            command: 9,
            orders: vec![order("HFUN/USDC")],
        }),
    );
    let r = take_http(&mut e);
    let payload: serde_json::Value = serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
    let w = &payload["action"]["orders"][0];
    assert_eq!(w["a"], 10001);
    assert_eq!(w["p"], "0.000937");
    assert_eq!(w["r"], false);
    assert!(payload["action"]["builder"].is_null());
    assert!(
        payload["signature"]["r"]
            .as_str()
            .unwrap()
            .starts_with("0x")
    );
}
#[test]
fn standalone_spot_triggers_are_rejected_before_write() {
    let (mut e, _) = ready(true);
    e.drain_actions().for_each(drop);
    let mut o = order("HFUN/USDC");
    o.kind = OrderType::StopLoss;
    e.handle(
        2000.0,
        Event::Command(Command::PlaceOrders {
            command: 10,
            orders: vec![o],
        }),
    );
    assert!(!e.drain_actions().any(|a| matches!(a, Action::Http(_))));
}

#[test]
fn documented_contexts_without_coin_follow_metadata_order() {
    let mut e = Engine::new(Hyperliquid::new_spot(Options {
        public_data_only: true,
        ..Options::default()
    }));
    e.start(1000.0);
    let r = take_http(&mut e);
    support::reply(
        &mut e,
        &r,
        200,
        r#"[{"tokens":[{"name":"USDC","index":0,"szDecimals":8},{"name":"PURR","index":1,"szDecimals":0}],"universe":[{"name":"PURR/USDC","tokens":[1,0],"index":0}]},[{"dayNtlVlm":"8906.0","markPx":"0.14","midPx":"0.209265","prevDayPx":"0.20432"}]]"#,
    );
    assert_eq!(e.store().markets()[0].symbol, "PURR/USDC");
    assert_eq!(e.store().tickers()[0].last, 0.209265);
}
#[test]
fn spot_books_subscribe_all_tiers_and_append_only_worse_levels() {
    let (mut e, s) = ready(false);
    e.drain_actions().for_each(drop);
    e.handle(
        2000.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "PURR/USDC".into(),
        }),
    );
    let subscriptions = e
        .drain_actions()
        .filter(|a| matches!(a, Action::WsSend { .. }))
        .count();
    assert_eq!(subscriptions, 5);
    support::message(
        &mut e,
        s,
        2100.0,
        r#"{"channel":"l2Book","data":{"coin":"PURR/USDC","time":2100,"levels":[[{"px":"0.2","sz":"2"},{"px":"0.19999","sz":"3"}],[{"px":"0.21","sz":"4"},{"px":"0.21001","sz":"5"}]]}}"#,
    );
    support::message(
        &mut e,
        s,
        2200.0,
        r#"{"channel":"l2Book","data":{"coin":"PURR/USDC","time":2200,"levels":[[{"px":"0.2","sz":"20"},{"px":"0.1999","sz":"30"}],[{"px":"0.21","sz":"40"},{"px":"0.2101","sz":"50"}]]}}"#,
    );
    let book = e
        .venue()
        .books()
        .find(|(s, _)| *s == "PURR/USDC")
        .unwrap()
        .1;
    assert_eq!(book.bids().len(), 3);
    assert_eq!(book.asks().len(), 3);
}
#[test]
fn malformed_balance_preserves_the_complete_previous_snapshot() {
    let (mut e, s) = ready(true);
    let before = e.store().balance().clone();
    support::message(
        &mut e,
        s,
        2000.0,
        r#"{"channel":"spotState","data":{"spotState":{"balances":[{"coin":"USDC","hold":"0","total":"100"},{"coin":"PURR","hold":"0","total":"NaN"}]}}}"#,
    );
    assert_eq!(*e.store().balance(), before);
    assert_eq!(e.store().currency_balances().unwrap().len(), 2);
}
#[test]
fn spot_currency_valuation_resolves_native_token_id_aliases() {
    let (mut e, s) = ready(true);
    support::message(
        &mut e,
        s,
        2000.0,
        r#"{"channel":"spotState","data":{"spotState":{"balances":[{"coin":"UPURR","token":1,"hold":"1","total":"10"}]}}}"#,
    );
    assert!((e.store().balance().total - 1.4).abs() < 1e-12);
    assert_eq!(e.store().currency_balances().unwrap()[0].currency, "UPURR");
}

#[test]
fn spot_limit_brackets_preserve_caller_cloid_and_never_reduce_only() {
    let (mut e, _) = ready(true);
    e.drain_actions().for_each(drop);
    let mut o = order("HFUN/USDC");
    o.reduce_only = Some(true);
    o.stop_loss = Some(0.0008);
    o.take_profit = Some(0.0012);
    o.time_in_force = Some(TimeInForce::PostOnly);
    e.handle(
        3000.0,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![o],
        }),
    );
    let r = take_http(&mut e);
    let p: serde_json::Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    let wires = p["action"]["orders"].as_array().unwrap();
    assert_eq!(p["action"]["grouping"], "normalTpsl");
    assert_eq!(wires.len(), 3);
    assert!(wires.iter().all(|w| w["r"] == false));
    assert_eq!(wires[0]["c"], "0x00000000000000000000000000000001");
    assert_ne!(wires[1]["c"], wires[2]["c"]);
    assert_eq!(wires[0]["t"]["limit"]["tif"], "Alo");
}
#[test]
fn spot_market_order_uses_last_mid_with_five_percent_gtc_envelope() {
    let (mut e, _) = ready(true);
    e.drain_actions().for_each(drop);
    let mut o = order("PURR/USDC");
    o.kind = OrderType::Market;
    o.amount = 10.0;
    o.price = None;
    e.handle(
        3000.0,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![o],
        }),
    );
    let r = take_http(&mut e);
    let p: serde_json::Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    assert_eq!(p["action"]["orders"][0]["p"], "0.21973");
    assert_eq!(p["action"]["orders"][0]["t"]["limit"]["tif"], "Gtc");
}
#[test]
fn spot_history_matches_inherited_empty_capabilities_without_private_reads() {
    let (mut e, _) = ready(true);
    e.drain_actions().for_each(drop);
    for query in [
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
        Query::OrderHistory {
            symbol: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
        Query::Income {
            symbol: None,
            kind: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    ] {
        e.handle(3000.0, Event::Command(Command::Query { command: 1, query }));
        let a = e.drain_actions().collect::<Vec<_>>();
        assert!(!a.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(
            a.iter().any(
                |a| matches!(a,Action::Notice(Notice::CommandDone{result:Ok(s),..})if s=="[]")
            )
        );
    }
}
#[test]
fn spot_fixed_settings_do_not_issue_perpetual_leverage_actions() {
    let (mut e, _) = ready(true);
    e.drain_actions().for_each(drop);
    for command in [
        Command::SetLeverage {
            command: 1,
            symbol: "PURR/USDC".into(),
            leverage: 2.0,
        },
        Command::SetMarginType {
            command: 1,
            symbol: "PURR/USDC".into(),
            isolated: true,
        },
    ] {
        e.handle(3000.0, Event::Command(command));
        let a = e.drain_actions().collect::<Vec<_>>();
        assert!(!a.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(a.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandDone { result: Err(_), .. })
        )));
    }
    assert_eq!(e.store().setting("PURR/USDC").unwrap().leverage, 1.0);
}
#[test]
fn spot_candles_use_native_at_coin_and_documented_rest_shape() {
    let (mut e, _) = ready(false);
    e.drain_actions().for_each(drop);
    e.handle(
        10_000.0,
        Event::Command(Command::FetchCandles {
            command: 1,
            symbol: "HFUN/USDC".into(),
            interval: "1m".into(),
            limit: Some(5),
            from_ms: None,
            to_ms: Some(10_000.0),
        }),
    );
    let r = take_http(&mut e);
    let p: serde_json::Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    assert_eq!(p["req"]["coin"], "@1");
    assert_eq!(p["type"], "candleSnapshot");
    support::reply(
        &mut e,
        &r,
        200,
        r#"[{"t":1000,"o":"1","h":"2","l":"0.5","c":"1.5","v":"10"}]"#,
    );
    assert!(e.drain_actions().any(
        |a| matches!(a,Action::Notice(Notice::CommandDone{result:Ok(s),..})if s.contains("1.5"))
    ));
}
fn acknowledge(e: &mut Engine<Hyperliquid>, r: &HttpRequest, now: f64, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request: r.id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
fn placed() -> (Engine<Hyperliquid>, SocketId) {
    let (mut e, s) = ready(true);
    e.drain_actions().for_each(drop);
    e.handle(
        3000.0,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![order("HFUN/USDC")],
        }),
    );
    let r = take_http(&mut e);
    acknowledge(
        &mut e,
        &r,
        3001.0,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":101}}]}}}"#,
    );
    e.drain_actions().for_each(drop);
    (e, s)
}
#[test]
fn cancel_uses_the_same_shifted_native_asset_id_and_removes_acknowledged_order() {
    let (mut e, _) = placed();
    assert!(e.store().order("101").is_some());
    e.handle(
        5000.0,
        Event::Command(Command::CancelOrders {
            command: 2,
            ids: vec!["101".into()],
        }),
    );
    let r = take_http(&mut e);
    let p: serde_json::Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    assert_eq!(p["action"]["cancels"][0]["a"], 10001);
    assert_eq!(p["action"]["cancels"][0]["o"], 101);
    acknowledge(
        &mut e,
        &r,
        5001.0,
        r#"{"status":"ok","response":{"type":"cancel","data":{"statuses":["success"]}}}"#,
    );
    assert!(e.store().order("101").is_none());
}
#[test]
fn amend_waits_for_cancel_then_places_with_new_client_id() {
    let (mut e, _) = placed();
    e.handle(
        5000.0,
        Event::Command(Command::AmendOrder {
            command: 2,
            id: "101".into(),
            price: Some(0.00095),
            amount: Some(6000.0),
        }),
    );
    let r = take_http(&mut e);
    acknowledge(
        &mut e,
        &r,
        5001.0,
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
    e.handle(5500.0, Event::TimerFired { timer });
    let r = take_http(&mut e);
    let p: serde_json::Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    let w = &p["action"]["orders"][0];
    assert_eq!(w["a"], 10001);
    assert_eq!(w["s"], "6000");
    assert_eq!(w["p"], "0.00095");
    assert_ne!(w["c"], "0x00000000000000000000000000000001");
}
#[test]
fn fill_during_amend_cancel_stops_the_replacement_and_deduplicates_executions() {
    let (mut e, s) = placed();
    e.handle(
        5000.0,
        Event::Command(Command::AmendOrder {
            command: 2,
            id: "101".into(),
            price: Some(0.00095),
            amount: None,
        }),
    );
    let r = take_http(&mut e);
    let fill = r#"{"channel":"userFills","data":{"isSnapshot":false,"fills":[{"coin":"@1","side":"B","px":"0.000937","sz":"1","time":5001,"tid":9,"oid":101}]}}"#;
    support::message(&mut e, s, 5001.0, fill);
    support::message(&mut e, s, 5001.0, fill);
    let frame = e.take_frame(5001.0).unwrap();
    let mut offset = 16;
    let mut fills = 0;
    while offset + 8 <= frame.len() {
        let kind = u16::from_le_bytes(frame[offset..offset + 2].try_into().unwrap());
        let size = u32::from_le_bytes(frame[offset + 4..offset + 8].try_into().unwrap()) as usize;
        if kind == 11 {
            fills = u32::from_le_bytes(frame[offset + 8..offset + 12].try_into().unwrap());
        }
        offset += 8 + size;
    }
    assert_eq!(fills, 1);
    acknowledge(
        &mut e,
        &r,
        5002.0,
        r#"{"status":"ok","response":{"type":"cancel","data":{"statuses":["success"]}}}"#,
    );
    let a = e.drain_actions().collect::<Vec<_>>();
    assert!(
        !a.iter()
            .any(|a| matches!(a,Action::Http(r)if r.url.ends_with("/exchange")))
    );
    assert!(a.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone {
            command: 2,
            result: Err(_)
        })
    )));
    assert!(e.store().positions().iter().all(|p| p.contracts == 0.0));
}
#[test]
fn spot_metadata_failures_retry_three_reads_without_committing_partial_markets() {
    let mut e = Engine::new(Hyperliquid::new_spot(Options {
        public_data_only: true,
        ..Options::default()
    }));
    e.start(1000.0);
    for attempt in 0..3 {
        let r = take_http(&mut e);
        e.handle(
            1000.0,
            Event::HttpResponse {
                request: r.id,
                response: safe_cex_engine::Response {
                    status: 503,
                    headers: "",
                    body: "unavailable",
                },
            },
        );
        assert!(e.store().markets().is_empty());
        let timer = e.drain_actions().find_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(timer)
            } else {
                None
            }
        });
        if attempt < 2 {
            e.handle(
                3000.0,
                Event::TimerFired {
                    timer: timer.unwrap(),
                },
            );
        } else {
            assert!(timer.is_none());
        }
    }
}

#[test]
fn fills_before_resting_ack_preserve_native_executed_quantity() {
    for (sizes, remaining) in [(vec![6208.0], 0.0), (vec![1000.0, 2000.0], 3208.0)] {
        let (mut e, s) = ready(true);
        e.drain_actions().for_each(drop);
        e.handle(
            3000.0,
            Event::Command(Command::PlaceOrders {
                command: 1,
                orders: vec![order("HFUN/USDC")],
            }),
        );
        let r = take_http(&mut e);
        for (index, size) in sizes.iter().enumerate() {
            let fill = format!(
                r#"{{"channel":"userFills","data":{{"isSnapshot":false,"fills":[{{"coin":"@1","side":"B","px":"0.000937","sz":"{size}","time":3001,"tid":{},"oid":101}}]}}}}"#,
                index + 100
            );
            support::message(&mut e, s, 3001.0, &fill);
            support::message(&mut e, s, 3001.0, &fill);
        }
        acknowledge(
            &mut e,
            &r,
            3002.0,
            r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":101}}]}}}"#,
        );
        if remaining == 0.0 {
            assert!(e.store().order("101").is_none());
        } else {
            let o = e.store().order("101").unwrap();
            assert_eq!(o.remaining, remaining);
            assert_eq!(o.filled, 3000.0);
        }
    }
}
#[test]
fn spot_book_protocol_uses_native_endpoint_and_only_documented_precision_values() {
    let mut e = Engine::new(Hyperliquid::new_spot(Options {
        public_data_only: true,
        ..Options::default()
    }));
    e.start(1000.0);
    let r = take_http(&mut e);
    support::reply(&mut e, &r, 200, META);
    let socket = e
        .drain_actions()
        .find_map(|a| {
            if let Action::WsConnect { socket, url, .. } = a {
                assert_eq!(url, "wss://api.hyperliquid.xyz/ws");
                Some(socket)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(1000.0, Event::WsOpened { socket });
    e.drain_actions().for_each(drop);
    e.handle(
        2000.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "PURR/USDC".into(),
        }),
    );
    let messages = e
        .drain_actions()
        .filter_map(|a| {
            if let Action::WsSend { text, .. } = a {
                Some(serde_json::from_str::<serde_json::Value>(&text).unwrap())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(messages.len(), 5);
    assert_eq!(
        messages
            .iter()
            .map(|m| m["subscription"]["nSigFigs"].clone())
            .collect::<Vec<_>>(),
        vec![
            serde_json::Value::Null,
            2.into(),
            3.into(),
            4.into(),
            5.into()
        ]
    );
}

#[test]
fn positional_context_fallback_requires_a_complete_homogeneous_documented_array() {
    for mixed in [true, false] {
        let mut payload: serde_json::Value = serde_json::from_str(META).unwrap();
        if mixed {
            payload[1][1].as_object_mut().unwrap().remove("coin");
        } else {
            for ctx in payload[1].as_array_mut().unwrap() {
                ctx.as_object_mut().unwrap().remove("coin");
            }
            payload[1]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!({"midPx":"999"}));
        }
        let mut e = Engine::new(Hyperliquid::new_spot(Options {
            public_data_only: true,
            ..Options::default()
        }));
        e.start(1000.0);
        let r = take_http(&mut e);
        support::reply(&mut e, &r, 200, &payload.to_string());
        if mixed {
            assert_eq!(e.store().markets().len(), 1);
            assert_eq!(e.store().markets()[0].symbol, "HFUN/USDC");
            assert_eq!(e.store().tickers()[0].last, 0.00093722);
        } else {
            assert!(e.store().markets().is_empty());
        }
    }
}
const MULTI_QUOTE_META: &str = r#"[{"tokens":[{"name":"USDC","index":0,"szDecimals":8},{"name":"HYPE","index":150,"szDecimals":2},{"name":"USDT0","index":268,"szDecimals":2},{"name":"USDH","index":360,"szDecimals":2},{"name":"USDE","index":235,"szDecimals":2}],"universe":[{"name":"@207","tokens":[150,268],"index":207},{"name":"@232","tokens":[150,360],"index":232},{"name":"@255","tokens":[150,235],"index":255},{"name":"@107","tokens":[150,0],"index":107},{"name":"@200","tokens":[268,0],"index":200}]},[{"coin":"@255","midPx":"13","markPx":"13"},{"coin":"@107","midPx":"40","markPx":"40"},{"coin":"@207","midPx":"11","markPx":"11"},{"coin":"@232","midPx":"12","markPx":"12"},{"coin":"@200","midPx":"0.98","markPx":"0.98"}]]"#;
fn custom_spot(meta: &str) -> (Engine<Hyperliquid>, SocketId) {
    let mut e = Engine::new(Hyperliquid::new_spot(Options {
        key: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
        secret: Some("0x0123456789012345678901234567890123456789012345678901234567890123".into()),
        application_id: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
        ..Options::default()
    }));
    e.start(1000.0);
    let r = take_http(&mut e);
    support::reply(&mut e, &r, 200, meta);
    let socket = e
        .drain_actions()
        .find_map(|a| {
            if let Action::WsConnect { socket, .. } = a {
                Some(socket)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(1000.0, Event::WsOpened { socket });
    e.drain_actions().for_each(drop);
    (e, socket)
}
#[test]
fn duplicate_base_tokens_keep_each_quote_symbol_and_native_action_identity() {
    for (quote, coin, index) in [
        ("USDC", "@107", 107),
        ("USDT0", "@207", 207),
        ("USDH", "@232", 232),
        ("USDE", "@255", 255),
    ] {
        let (mut e, _) = custom_spot(MULTI_QUOTE_META);
        let symbol = format!("HYPE/{quote}");
        assert_eq!(e.store().markets().len(), 5);
        assert_eq!(e.store().market(&symbol).unwrap().id, coin);
        assert_eq!(e.store().market(&symbol).unwrap().quote, quote);
        e.handle(
            2000.0,
            Event::Command(Command::SubscribeOrderBook {
                symbol: symbol.clone(),
            }),
        );
        let messages = e
            .drain_actions()
            .filter_map(|a| {
                if let Action::WsSend { text, .. } = a {
                    Some(serde_json::from_str::<serde_json::Value>(&text).unwrap())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(messages.len(), 5);
        assert!(messages.iter().all(|m| m["subscription"]["coin"] == coin));
        e.handle(
            2000.0,
            Event::Command(Command::SubscribeCandles {
                symbol: symbol.clone(),
                interval: "1m".into(),
            }),
        );
        assert!(e.drain_actions().any(|a|matches!(a,Action::WsSend{text,..}if serde_json::from_str::<serde_json::Value>(&text).unwrap()["subscription"]["coin"]==coin)));
        e.handle(
            2000.0,
            Event::Command(Command::FetchCandles {
                command: 1,
                symbol: symbol.clone(),
                interval: "1m".into(),
                limit: Some(1),
                from_ms: None,
                to_ms: None,
            }),
        );
        let r = take_http(&mut e);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(r.body.as_ref().unwrap()).unwrap()["req"]["coin"],
            coin
        );
        let mut o = order(&symbol);
        o.price = Some(10.0);
        o.amount = 1.0;
        e.handle(
            3000.0,
            Event::Command(Command::PlaceOrders {
                command: 2,
                orders: vec![o],
            }),
        );
        let r = take_http(&mut e);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(r.body.as_ref().unwrap()).unwrap()["action"]
                ["orders"][0]["a"],
            10_000 + index
        );
    }
}
#[test]
fn spot_mids_update_only_the_matching_native_quote_pair() {
    let (mut e, s) = custom_spot(MULTI_QUOTE_META);
    support::message(
        &mut e,
        s,
        2000.0,
        r#"{"channel":"allMids","data":{"mids":{"@107":"50","@207":"51","@232":"52","@255":"53"}}}"#,
    );
    for (quote, expected) in [
        ("USDC", 50.0),
        ("USDT0", 51.0),
        ("USDH", 52.0),
        ("USDE", 53.0),
    ] {
        assert_eq!(
            e.store().ticker(&format!("HYPE/{quote}")).unwrap().last,
            expected
        );
    }
    support::message(
        &mut e,
        s,
        2001.0,
        r#"{"channel":"allMids","data":{"mids":{"@107":"60"}}}"#,
    );
    assert_eq!(e.store().ticker("HYPE/USDC").unwrap().last, 60.0);
    assert_eq!(e.store().ticker("HYPE/USDT0").unwrap().last, 51.0);
}
#[test]
fn spot_token_valuation_prefers_usdc_and_converts_other_quotes_when_available() {
    let (mut e, s) = custom_spot(MULTI_QUOTE_META);
    let balances = r#"{"channel":"spotState","data":{"spotState":{"balances":[{"coin":"UHYPE","token":150,"total":"2","hold":"0.5"},{"coin":"USDT0","token":268,"total":"10","hold":"0"}]}}}"#;
    support::message(&mut e, s, 2000.0, balances);
    assert!((e.store().balance().total - 89.8).abs() < 1e-9);
    assert_eq!(e.store().coin_balances().unwrap()[0].notional, 80.0);
    let mut metadata: serde_json::Value = serde_json::from_str(MULTI_QUOTE_META).unwrap();
    metadata[0]["universe"]
        .as_array_mut()
        .unwrap()
        .retain(|m| m["name"] == "@207" || m["name"] == "@200");
    let (mut e, s) = custom_spot(&metadata.to_string());
    support::message(&mut e, s, 2000.0, balances);
    assert!((e.store().coin_balances().unwrap()[0].notional - 21.56).abs() < 1e-9);
    assert_eq!(e.store().currency_balances().unwrap()[0].balance.total, 2.0);
    metadata[0]["universe"].as_array_mut().unwrap()[0]["tokens"][1] = 235.into();
    let (mut e, s) = custom_spot(&metadata.to_string());
    support::message(&mut e, s, 2000.0, balances);
    assert_eq!(e.store().coin_balances().unwrap()[0].notional, 0.0);
    assert_eq!(e.store().coin_balances().unwrap()[0].amount, 2.0);
}

#[test]
fn cyclic_quote_paths_do_not_multiply_prices_or_block_independent_stable_exits() {
    let metadata = r#"[{"tokens":[{"name":"A","index":1,"szDecimals":2},{"name":"B","index":2,"szDecimals":2},{"name":"USDH","index":3,"szDecimals":2}],"universe":[{"name":"@1","tokens":[1,2],"index":1},{"name":"@2","tokens":[1,3],"index":2},{"name":"@3","tokens":[2,1],"index":3}]},[{"coin":"@1","midPx":"2"},{"coin":"@2","midPx":"3"},{"coin":"@3","midPx":"4"}]]"#;
    let balances = r#"{"channel":"spotState","data":{"spotState":{"balances":[{"coin":"A","token":1,"total":"1","hold":"0"},{"coin":"B","token":2,"total":"1","hold":"0"}]}}}"#;
    let (mut e, s) = custom_spot(metadata);
    support::message(&mut e, s, 2000.0, balances);
    let coins = e.store().coin_balances().unwrap();
    assert_eq!(coins[0].notional, 3.0);
    assert_eq!(coins[1].notional, 12.0);
    let alias_balances = balances.replace("\"coin\":\"A\"", "\"coin\":\"NATIVE_A_ALIAS\"");
    support::message(&mut e, s, 2001.0, &alias_balances);
    assert_eq!(e.store().coin_balances().unwrap()[0].notional, 3.0);
    assert_eq!(
        e.store().currency_balances().unwrap()[0].currency,
        "NATIVE_A_ALIAS"
    );
    let mut cyclic: serde_json::Value = serde_json::from_str(metadata).unwrap();
    cyclic[0]["universe"]
        .as_array_mut()
        .unwrap()
        .retain(|m| m["name"] != "@2");
    let (mut e, s) = custom_spot(&cyclic.to_string());
    support::message(&mut e, s, 2000.0, balances);
    assert_eq!(e.store().balance().total, 0.0);
    assert!(
        e.store()
            .coin_balances()
            .unwrap()
            .iter()
            .all(|coin| coin.amount == 1.0 && coin.notional == 0.0)
    );
    assert!(
        e.store()
            .currency_balances()
            .unwrap()
            .iter()
            .all(|currency| currency.balance.total == 1.0)
    );
}
