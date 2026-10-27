//! Documented spot metadata, balances and spot asset ID payloads.
//! https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/info-endpoint/spot
mod support;
use safe_cex_engine::model::*;
use safe_cex_engine::test_support::prepared_now;
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, PublicFeedPolicy, Query, SocketChannel,
    SocketId, Venue,
};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options};
use support::Plain;
/// Native cadence of the venue's recurring account poll; policy scales it.
const POLL_MS: f64 = 10_000.0;

const META: &str = r#"[{"tokens":[{"name":"USDC","index":0,"szDecimals":8},{"name":"PURR","index":1,"szDecimals":0},{"name":"HFUN","index":2,"szDecimals":2}],"universe":[{"name":"PURR/USDC","tokens":[1,0],"index":0},{"name":"@1","tokens":[2,0],"index":1}]},[{"coin":"@1","midPx":"0.00093722","markPx":"0.00094","prevDayPx":"0.0009","dayNtlVlm":"937.22"},{"coin":"PURR/USDC","midPx":"0.209265","markPx":"0.14","prevDayPx":"0.20432","dayNtlVlm":"8906.0"}]]"#;
fn ready(private: bool) -> (Engine<Hyperliquid>, SocketId) {
    ready_with_policy(private, PublicFeedPolicy::Direct)
}
fn ready_with_policy(private: bool, policy: PublicFeedPolicy) -> (Engine<Hyperliquid>, SocketId) {
    let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new_spot(Options {
        public_data_only: !private,
        key: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
        secret: Some("0x0123456789012345678901234567890123456789012345678901234567890123".into()),
        application_id: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
        ..Options::default()
    }));
    e.set_public_feed_policy(policy).unwrap();
    e.start(1000.0);
    let r = take_http(&mut e);
    assert_eq!(support::kind(&r), "spotMetaAndAssetCtxs");
    support::reply(&mut e, &r, 200, META);
    let mut socket = None;
    for a in e.drain_plain().collect::<Vec<_>>() {
        match a {
            Action::WsConnect {
                socket: s, channel, ..
            } => {
                assert_eq!(
                    channel,
                    Some(if private {
                        SocketChannel::Private
                    } else {
                        SocketChannel::Public
                    })
                );
                socket = Some(s);
            }
            Action::Http(r) if support::kind(&r) == "frontendOpenOrders" => {
                support::reply(&mut e, &r, 200, "[]");
            }
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
    e.drain_plain()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap()
}
fn take_prepared_http(e: &mut Engine<Hyperliquid>, now: f64) -> HttpRequest {
    safe_cex_engine::test_support::prepared_actions(e, now)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .expect("prepared native candle HTTP")
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
        .drain_plain()
        .filter_map(|a| {
            if let Action::WsSend { text, .. } = a {
                Some(text)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let mut types = messages
        .iter()
        .map(|message| {
            serde_json::from_str::<serde_json::Value>(message).unwrap()["subscription"]["type"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect::<Vec<_>>();
    types.sort();
    assert_eq!(
        types,
        [
            "allMids",
            "notification",
            "spotState",
            "userFills",
            "userNonFundingLedgerUpdates"
        ]
    );
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
fn spot_receipt_keeps_canonical_symbol_and_actual_shifted_asset_cloid() {
    let (mut e, _) = ready(true);
    e.drain_plain().for_each(drop);
    e.handle(
        2000.,
        Event::Command(Command::PlaceOrders {
            command: 91,
            orders: vec![order("HFUN/USDC")],
        }),
    );
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
    let payload: serde_json::Value =
        serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert!(at < write_at);
    assert_eq!(payload["action"]["orders"][0]["a"], 10001);
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].symbol, "HFUN/USDC");
    assert_eq!(receipts[0].kind, OrderIdentityKind::ClientOrderId);
    assert_eq!(
        receipts[0].identity,
        payload["action"]["orders"][0]["c"].as_str().unwrap()
    );
}
#[test]
fn signed_spot_order_uses_shifted_asset_and_eight_decimal_cap() {
    let (mut e, _) = ready(true);
    e.drain_plain().for_each(drop);
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
    e.drain_plain().for_each(drop);
    let mut o = order("HFUN/USDC");
    o.kind = OrderType::StopLoss;
    e.handle(
        2000.0,
        Event::Command(Command::PlaceOrders {
            command: 10,
            orders: vec![o],
        }),
    );
    assert!(!e.drain_plain().any(|a| matches!(a, Action::Http(_))));
}

#[test]
fn documented_contexts_without_coin_follow_metadata_order() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new_spot(Options {
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
    e.drain_plain().for_each(drop);
    e.handle(
        2000.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "PURR/USDC".into(),
        }),
    );
    let subscriptions = e
        .drain_plain()
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
    e.drain_plain().for_each(drop);
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
fn spot_zero_bracket_prices_disable_only_the_requested_child() {
    for (stop, take, children) in [
        (0.0, 0.0, Vec::<&str>::new()),
        (0.0, 0.3, vec!["tp"]),
        (0.18, 0.0, vec!["sl"]),
    ] {
        let (mut e, _) = ready(true);
        e.drain_plain().for_each(drop);
        let mut o = order("PURR/USDC");
        o.stop_loss = Some(stop);
        o.take_profit = Some(take);
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
        assert_eq!(wires.len(), 1 + children.len());
        assert_eq!(
            p["action"]["grouping"],
            if children.is_empty() {
                "na"
            } else {
                "normalTpsl"
            }
        );
        for (wire, kind) in wires.iter().skip(1).zip(children) {
            assert_eq!(wire["t"]["trigger"]["tpsl"], kind);
            assert!(
                wire["t"]["trigger"]["triggerPx"]
                    .as_str()
                    .unwrap()
                    .parse::<f64>()
                    .unwrap()
                    > 0.0
            );
        }
    }
}
#[test]
fn spot_invalid_bracket_prices_still_reject_the_batch_before_writes() {
    for trigger in [-0.1, f64::NAN, f64::INFINITY] {
        let (mut e, _) = ready(true);
        e.drain_plain().for_each(drop);
        let mut bad = order("PURR/USDC");
        bad.stop_loss = Some(trigger);
        e.handle(
            3000.0,
            Event::Command(Command::PlaceOrders {
                command: 1,
                orders: vec![order("PURR/USDC"), bad],
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
fn spot_market_order_uses_last_mid_with_five_percent_gtc_envelope() {
    let (mut e, _) = ready(true);
    e.drain_plain().for_each(drop);
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
    e.drain_plain().for_each(drop);
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
        let a = e.drain_plain().collect::<Vec<_>>();
        assert!(!a.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(
            a.iter()
                .any(|a| matches!(a,Action::Notice(Notice::CommandDone{result:s,..})if s=="[]"))
        );
    }
}
#[test]
fn spot_fixed_settings_do_not_issue_perpetual_leverage_actions() {
    let (mut e, _) = ready(true);
    e.drain_plain().for_each(drop);
    for command in [
        Command::SetLeverage {
            command: 1,
            symbol: "PURR/USDC".into(),
            leverage: 2.0,
            side: None,
        },
        Command::SetMarginType {
            command: 1,
            symbol: "PURR/USDC".into(),
            isolated: true,
        },
    ] {
        e.handle(3000.0, Event::Command(command));
        let a = e.drain_plain().collect::<Vec<_>>();
        assert!(!a.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(
            a.iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
        );
    }
    assert_eq!(e.store().setting("PURR/USDC").unwrap().leverage, 1.0);
}
#[test]
fn spot_candles_use_native_at_coin_and_documented_rest_shape() {
    let (mut e, _) = ready(false);
    e.drain_plain().for_each(drop);
    e.handle(
        10_000.0,
        Event::Command(Command::FetchCandles {
            command: 1,
            symbol: "HFUN/USDC".into(),
            interval: safe_cex_engine::model::Interval::M1,
            limit: Some(5),
            from_ms: None,
            to_ms: Some(10_000.0),
        }),
    );
    let r = take_prepared_http(&mut e, 10_000.);
    let p: serde_json::Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    assert_eq!(p["req"]["coin"], "@1");
    assert_eq!(p["type"], "candleSnapshot");
    support::reply(
        &mut e,
        &r,
        200,
        r#"[{"t":1000,"o":"1","h":"2","l":"0.5","c":"1.5","v":"10"}]"#,
    );
    assert!(
        e.drain_plain().any(
            |a| matches!(a,Action::Notice(Notice::CommandDone{result:s,..})if s.contains("1.5"))
        )
    );
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
    e.drain_plain().for_each(drop);
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
    e.drain_plain().for_each(drop);
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
        .drain_plain()
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
    let a = e.drain_plain().collect::<Vec<_>>();
    assert!(
        !a.iter()
            .any(|a| matches!(a,Action::Http(r)if r.url.ends_with("/exchange")))
    );
    assert!(
        a.iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 2, .. })))
    );
    assert!(e.store().positions().iter().all(|p| p.contracts == 0.0));
}
#[test]
fn spot_metadata_failures_keep_retrying_without_committing_partial_markets() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new_spot(Options {
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
        let timer = e.drain_plain().find_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(timer)
            } else {
                None
            }
        });
        e.handle(
            3000.0 + f64::from(attempt) * 30000.0,
            Event::TimerFired {
                timer: timer.unwrap(),
            },
        );
    }
}

#[test]
fn fills_before_resting_ack_preserve_native_executed_quantity() {
    for (sizes, remaining) in [(vec![6208.0], 0.0), (vec![1000.0, 2000.0], 3208.0)] {
        let (mut e, s) = ready(true);
        e.drain_plain().for_each(drop);
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
    let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new_spot(Options {
        public_data_only: true,
        ..Options::default()
    }));
    e.start(1000.0);
    let r = take_http(&mut e);
    support::reply(&mut e, &r, 200, META);
    let socket = e
        .drain_plain()
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
    e.drain_plain().for_each(drop);
    e.handle(
        2000.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "PURR/USDC".into(),
        }),
    );
    let messages = e
        .drain_plain()
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
        let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new_spot(Options {
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
    let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new_spot(Options {
        key: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
        secret: Some("0x0123456789012345678901234567890123456789012345678901234567890123".into()),
        application_id: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
        ..Options::default()
    }));
    e.start(1000.0);
    let r = take_http(&mut e);
    support::reply(&mut e, &r, 200, meta);
    let socket = e
        .drain_plain()
        .find_map(|a| {
            if let Action::WsConnect { socket, .. } = a {
                Some(socket)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(1000.0, Event::WsOpened { socket });
    e.drain_plain().for_each(drop);
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
            .drain_plain()
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
                interval: safe_cex_engine::model::Interval::M1,
            }),
        );
        assert!(e.drain_plain().any(|a|matches!(a,Action::WsSend{text,..}if serde_json::from_str::<serde_json::Value>(&text).unwrap()["subscription"]["coin"]==coin)));
        e.handle(
            2000.0,
            Event::Command(Command::FetchCandles {
                command: 1,
                symbol: symbol.clone(),
                interval: safe_cex_engine::model::Interval::M1,
                limit: Some(1),
                from_ms: None,
                to_ms: None,
            }),
        );
        let r = take_prepared_http(&mut e, 2000.);
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

const TEST_SECRET: &str = "0x0123456789012345678901234567890123456789012345678901234567890123";
const TEST_WALLET: &str = "0x14791697260e4c9a71f18484c9f997b308e59325";
#[test]
fn invalid_spot_local_credentials_reject_before_io_and_loaded_state() {
    for (secret, key) in [
        ("not-a-private-key", TEST_WALLET),
        (TEST_SECRET, "0x0000000000000000000000000000000000000001"),
        (TEST_SECRET, "invalid-wallet"),
        (" ", TEST_WALLET),
    ] {
        let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new_spot(Options {
            secret: Some(secret.into()),
            key: Some(key.into()),
            application_id: Some(TEST_WALLET.into()),
            ..Options::default()
        }));
        e.start(1000.0);
        let actions = e.drain_plain().collect::<Vec<_>>();
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::Deactivate)))
        );
        assert!(actions.iter().any(|a|matches!(a,Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { level: safe_cex_engine::Level::Error, message, .. }))if message.contains("wallet address pair"))));
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Http(_) | Action::WsConnect { .. }))
        );
        assert!(!e.store().loaded().balance);
        assert!(!e.store().loaded().markets);
    }
}
#[test]
fn spot_local_signer_identity_is_separate_from_main_wallet_and_vault() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new_spot(Options {
        secret: Some(TEST_SECRET.into()),
        key: Some(TEST_WALLET.to_uppercase()),
        application_id: Some("0x0000000000000000000000000000000000000002".into()),
        vault: Some("0x0000000000000000000000000000000000000003".into()),
        ..Options::default()
    }));
    e.start(1000.0);
    let request = take_http(&mut e);
    support::reply(&mut e, &request, 200, META);
    let actions = e.drain_plain().collect::<Vec<_>>();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::Deactivate)))
    );
    let r = actions
        .iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    let read: serde_json::Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    assert_eq!(read["type"], "spotClearinghouseState");
    assert_eq!(read["user"], "0x0000000000000000000000000000000000000003");
}
#[test]
fn absent_spot_local_credentials_and_explicit_public_mode_stay_public() {
    for options in [
        Options::default(),
        Options {
            key: Some(TEST_WALLET.into()),
            secret: Some(String::new()),
            application_id: Some(TEST_WALLET.into()),
            ..Options::default()
        },
        Options {
            public_data_only: true,
            key: Some("bad-wallet".into()),
            secret: Some("bad-secret".into()),
            application_id: Some(TEST_WALLET.into()),
            ..Options::default()
        },
    ] {
        let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new_spot(options));
        e.start(1000.0);
        let r = take_http(&mut e);
        support::reply(&mut e, &r, 200, META);
        let actions = e.drain_plain().collect::<Vec<_>>();
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::Deactivate) | Action::Http(_)))
        );
        assert!(actions.iter().any(|a| matches!(
            a,
            Action::WsConnect {
                channel: Some(safe_cex_engine::SocketChannel::Public),
                ..
            }
        )));
        assert!(e.store().loaded().balance);
    }
}

#[test]
fn spot_fill_frames_keep_native_order_identity_and_each_distinct_execution() {
    let (mut e, s) = ready(true);
    e.take_frame(1000.0);
    let fills = r#"{"channel":"userFills","data":{"isSnapshot":false,"fills":[{"coin":"PURR/USDC","side":"B","px":"0.2","sz":"0.5","time":2000,"tid":12345,"oid":900},{"coin":"PURR/USDC","side":"B","px":"0.2","sz":"0.25","time":2001,"tid":12346,"oid":900}]}}"#;
    support::message(&mut e, s, 2001.0, fills);
    support::message(&mut e, s, 2001.0, fills);
    let frame = e.take_frame(2001.0).unwrap();
    let read =
        |offset: usize| u32::from_le_bytes(frame[offset..offset + 4].try_into().unwrap()) as usize;
    let mut offset = 16;
    let mut fills_payload = None;
    let mut strings_payload = None;
    while offset + 8 <= frame.len() {
        let kind = u16::from_le_bytes(frame[offset..offset + 2].try_into().unwrap());
        let size = read(offset + 4);
        match kind {
            11 => fills_payload = Some(offset + 8),
            1 => strings_payload = Some(offset + 8),
            _ => {}
        }
        offset += 8 + size;
    }
    let fills = fills_payload.unwrap();
    assert_eq!(read(fills), 2);
    let strings = strings_payload.unwrap();
    let text = strings + 4 + (read(strings) + 1) * 4;
    for i in 0..2 {
        let index = read(fills + 4 + i * 4);
        let start = read(strings + 4 + index * 4);
        let end = read(strings + 8 + index * 4);
        assert_eq!(&frame[text + start..text + end], b"900");
    }
    // Three u32 columns follow count; the numeric rows begin at the next 8-byte boundary.
    let numbers = (fills + 4 + 2 * 3 * 4).next_multiple_of(8);
    assert_eq!(
        u16::from_le_bytes(frame[fills - 6..fills - 4].try_into().unwrap()),
        2
    );
    for (i, expected) in ["12345", "12346"].iter().enumerate() {
        let index = read(numbers + 2 * 20 + i * 4);
        let start = read(strings + 4 + index * 4);
        let end = read(strings + 8 + index * 4);
        assert_eq!(&frame[text + start..text + end], expected.as_bytes());
    }
    let amount = |i: usize| {
        f64::from_le_bytes(
            frame[numbers + i * 16 + 8..numbers + i * 16 + 16]
                .try_into()
                .unwrap(),
        )
    };
    assert_eq!(amount(0), 0.5);
    assert_eq!(amount(1), 0.25);
}

#[test]
fn non_direct_spot_account_socket_keeps_private_topics_ping_and_reconnect_only() {
    for policy in [PublicFeedPolicy::Disabled, PublicFeedPolicy::Shared] {
        let (mut e, socket) = ready_with_policy(true, policy);
        let actions: Vec<_> = e.drain_plain().collect();
        let assert_topics = |actions: &[Action]| {
            let mut topics: Vec<String> = actions
                .iter()
                .filter_map(|a| match a {
                    Action::WsSend { text, .. } => serde_json::from_str::<serde_json::Value>(text)
                        .ok()
                        .and_then(|v| v["subscription"]["type"].as_str().map(str::to_owned)),
                    _ => None,
                })
                .collect();
            topics.sort();
            assert_eq!(
                topics,
                [
                    "notification",
                    "spotState",
                    "userFills",
                    "userNonFundingLedgerUpdates"
                ]
            );
        };
        assert_topics(&actions);
        let mut timers: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                _ => None,
            })
            .collect();
        for command in [
            Command::SubscribeOrderBook {
                symbol: "PURR/USDC".into(),
            },
            Command::SubscribeCandles {
                symbol: "PURR/USDC".into(),
                interval: safe_cex_engine::model::Interval::M1,
            },
        ] {
            e.handle(2000., Event::Command(command));
        }
        let actions: Vec<_> = e.drain_plain().collect();
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::WsSend { .. } | Action::WsConnect { .. }))
        );
        timers.extend(actions.iter().filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
            _ => None,
        }));
        let ticker = e.store().ticker("PURR/USDC").unwrap().clone();
        support::message(
            &mut e,
            socket,
            2100.,
            r#"{"channel":"allMids","data":{"mids":{"PURR/USDC":"999"}}}"#,
        );
        assert_eq!(e.store().ticker("PURR/USDC").unwrap(), &ticker);
        let mut pinged = false;
        while let Some(index) = timers
            .iter()
            .enumerate()
            .filter(|(_, (_, at))| *at <= 16000.)
            .min_by(|a, b| a.1.1.total_cmp(&b.1.1))
            .map(|(index, _)| index)
        {
            let (timer, at) = timers.swap_remove(index);
            e.handle(at, Event::TimerFired { timer });
            for a in e.drain_plain() {
                match a {
                    Action::SetTimer { timer, at_ms } => timers.push((timer, at_ms)),
                    Action::ClearTimer { timer } => timers.retain(|(t, _)| *t != timer),
                    Action::WsSend { text, .. } => pinged |= text == r#"{"method":"ping"}"#,
                    _ => {}
                }
            }
        }
        assert!(pinged);
        e.handle(17000., Event::WsClosed { socket, code: 1006 });
        let (reconnect, at) = e
            .drain_plain()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if (17750. ..=18000.).contains(&at_ms) => {
                    Some((timer, at_ms))
                }
                _ => None,
            })
            .unwrap();
        e.handle(at, Event::TimerFired { timer: reconnect });
        let next = e
            .drain_plain()
            .find_map(|a| match a {
                Action::WsConnect {
                    socket,
                    channel: Some(SocketChannel::Private),
                    ..
                } => Some(socket),
                Action::WsConnect { .. } => panic!("spot reconnect opened a public feed"),
                _ => None,
            })
            .unwrap();
        e.handle(at, Event::WsOpened { socket: next });
        assert_topics(&e.drain_plain().collect::<Vec<_>>());
    }
}

#[test]
fn shared_spot_prices_preserve_direct_private_valuation_cadence_and_raw_holdings() {
    let state = r#"{"channel":"spotState","data":{"spotState":{"balances":[{"coin":"USDC","token":0,"hold":"3","total":"20"},{"coin":"UPURR","token":1,"hold":"1","total":"10"}]}}}"#;
    for policy in [PublicFeedPolicy::Direct, PublicFeedPolicy::Shared] {
        let (mut e, socket) = ready_with_policy(true, policy);
        e.drain_plain().for_each(drop);
        let balance = e.store().balance().clone();
        let raw = e.store().currency_balances().unwrap().to_vec();
        if policy == PublicFeedPolicy::Direct {
            support::message(
                &mut e,
                socket,
                2000.,
                r#"{"channel":"allMids","data":{"mids":{"PURR/USDC":"0.3"}}}"#,
            );
        } else {
            e.apply_public_feed(
                2000.,
                serde_json::from_str(
                    r#"{"tickers":[{"symbol":"PURR/USDC","last":0.3,"mark":0.14}]}"#,
                )
                .unwrap(),
            )
            .unwrap();
        }
        assert_eq!(
            e.store().balance(),
            &balance,
            "spot quotes do not run perpetual PnL math"
        );
        assert_eq!(e.store().currency_balances().unwrap(), raw);
        support::message(&mut e, socket, 2100., state);
        assert!((e.store().balance().total - 21.4).abs() < 1e-12);
        assert_eq!(e.store().currency_balances().unwrap()[1].balance.total, 10.);
        assert!((e.store().coin_balances().unwrap()[1].notional - 1.4).abs() < 1e-12);
        if policy == PublicFeedPolicy::Shared {
            e.apply_public_feed(
                2200.,
                serde_json::from_str(
                    r#"{"tickers":[{"symbol":"PURR/USDC","last":0.3,"mark":0.3}]}"#,
                )
                .unwrap(),
            )
            .unwrap();
            assert!((e.store().balance().total - 21.4).abs() < 1e-12);
            // Model a late local metadata quote write and let the engine restore
            // provider ownership at the next ordinary dispatch boundary.
            e.store_mut().update_ticker("PURR/USDC", |t| {
                t.mark = 999.;
                t.last = 999.;
            });
            support::message(&mut e, socket, 2300., r#"{"channel":"pong","data":{}}"#);
            assert_eq!(e.store().ticker("PURR/USDC").unwrap().mark, 0.3);
            assert!((e.store().balance().total - 21.4).abs() < 1e-12);
            support::message(&mut e, socket, 2400., state);
            assert!((e.store().balance().total - 23.).abs() < 1e-12);
            assert!((e.store().coin_balances().unwrap()[1].notional - 3.).abs() < 1e-12);
            assert_eq!(e.store().currency_balances().unwrap()[1].currency, "UPURR");
            assert_eq!(e.store().currency_balances().unwrap()[1].balance.total, 10.);
            assert_eq!(e.store().balance().upnl, 0.);
        }
    }
}

#[test]
fn periodic_account_policy_changes_preserve_inflight_reads_and_fence_stale_timers() {
    let (mut e, _) = ready(true);
    let now = 2000.;
    prepared_now(&mut e).into_iter().for_each(drop);
    e.handle(
        now,
        Event::PollScaleChanged {
            scale: f64::INFINITY,
        },
    );
    let stale: Vec<_> = prepared_now(&mut e)
        .into_iter()
        .filter_map(|a| match a {
            Action::ClearTimer { timer } => Some(timer),
            _ => None,
        })
        .collect();
    assert_eq!(stale.len(), 1, "one recurring account registration");
    let mut obsolete = stale;
    let mut current = None;
    for interval in [100_000_f64, 1_500.] {
        e.handle(
            now,
            Event::PollScaleChanged {
                scale: interval / POLL_MS,
            },
        );
        let actions: Vec<_> = prepared_now(&mut e).into_iter().collect();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        let timers: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                _ => None,
            })
            .collect();
        assert_eq!(timers.len(), 1);
        assert_eq!(timers[0].1, now + interval.max(10000.0));
        if let Some((timer, _)) = current.replace(timers[0]) {
            obsolete.push(timer);
        }
    }
    for timer in obsolete {
        e.handle(now, Event::TimerFired { timer });
        assert!(
            !prepared_now(&mut e)
                .into_iter()
                .any(|a| matches!(a, Action::Http(_)))
        );
    }
    let (timer, due) = current.unwrap();
    e.handle(due, Event::TimerFired { timer });
    let mut pending: Vec<_> = prepared_now(&mut e)
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    assert!(
        pending
            .iter()
            .any(|r| support::kind(r) == "spotClearinghouseState"),
        "actual native account request"
    );
    // Pause while REST is in flight. Completion may finish the current read
    // chain, but must not recreate an active recurring host timer.
    e.handle(
        due,
        Event::PollScaleChanged {
            scale: f64::INFINITY,
        },
    );
    prepared_now(&mut e).into_iter().for_each(drop);
    for _ in 0..12 {
        if pending.is_empty() {
            break;
        }
        for r in std::mem::take(&mut pending) {
            let body = r#"{"balances":[{"coin":"USDC","token":0,"hold":"0","total":"1000"}]}"#;
            e.handle(
                due + 1.,
                Event::HttpResponse {
                    request: r.id,
                    response: safe_cex_engine::Response {
                        status: 200,
                        headers: "",
                        body,
                    },
                },
            );
            pending.extend(prepared_now(&mut e).into_iter().filter_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            }));
        }
    }
    assert!(pending.is_empty(), "all fixture account reads complete");
    e.handle(
        due + 2.,
        Event::PollScaleChanged {
            scale: 10_000. / POLL_MS,
        },
    );
    let resumed: Vec<_> = prepared_now(&mut e)
        .into_iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .collect();
    assert_eq!(
        resumed.len(),
        1,
        "completion retains one paused registration"
    );
    assert_eq!(resumed[0].1, due + 2. + 10_000_f64.max(10000.0));
}
#[test]
fn spot_open_orders_are_loaded_and_externally_canceled_orders_are_removed() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new_spot(Options {
        key: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
        secret: Some("0x0123456789012345678901234567890123456789012345678901234567890123".into()),
        application_id: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
        ..Options::default()
    }));
    e.start(1000.0);
    let r = take_http(&mut e);
    support::reply(&mut e, &r, 200, META);
    let rows = r#"[{"coin":"@1","side":"B","limitPx":"0.0009","sz":"5","oid":77,"timestamp":1000,"origSz":"5","orderType":"Limit","tif":"Gtc","reduceOnly":false,"isTrigger":false,"triggerPx":"0","isPositionTpsl":false}]"#;
    let mut sync = None;
    for a in e.drain_plain().collect::<Vec<_>>() {
        match a {
            Action::Http(r) if support::kind(&r) == "frontendOpenOrders" => {
                support::reply(&mut e, &r, 200, rows);
            }
            Action::Http(r) => support::reply(&mut e, &r, 200, r#"{"balances":[]}"#),
            Action::SetTimer {
                timer,
                at_ms: 11_000.0,
            } => sync = Some(timer),
            _ => {}
        }
    }
    let order = e.store().order("77").expect("resting spot order loaded");
    assert_eq!(order.symbol, "HFUN/USDC");
    assert_eq!(order.amount, 5.0);
    e.handle(
        11_000.0,
        Event::TimerFired {
            timer: sync.expect("spot sync timer"),
        },
    );
    let reads: Vec<_> = e
        .drain_plain()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    let orders = reads
        .iter()
        .find(|r| support::kind(r) == "frontendOpenOrders")
        .expect("periodic spot open-order read");
    support::reply(&mut e, orders, 200, "[]");
    assert!(
        e.store().order("77").is_none(),
        "order canceled elsewhere stayed open"
    );
}
#[test]
fn open_order_read_sent_before_a_full_fill_cannot_restore_the_order() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new_spot(Options {
        key: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
        secret: Some("0x0123456789012345678901234567890123456789012345678901234567890123".into()),
        application_id: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
        ..Options::default()
    }));
    e.start(1000.0);
    let r = take_http(&mut e);
    support::reply(&mut e, &r, 200, META);
    let rows = r#"[{"coin":"@1","side":"B","limitPx":"0.0009","sz":"5","oid":78,"timestamp":1000,"origSz":"5","orderType":"Limit","tif":"Gtc","reduceOnly":false,"isTrigger":false,"triggerPx":"0","isPositionTpsl":false}]"#;
    let (mut sync, mut socket) = (None, None);
    for a in e.drain_plain().collect::<Vec<_>>() {
        match a {
            Action::Http(r) if support::kind(&r) == "frontendOpenOrders" => {
                support::reply(&mut e, &r, 200, rows);
            }
            Action::Http(r) => support::reply(&mut e, &r, 200, r#"{"balances":[]}"#),
            Action::SetTimer {
                timer,
                at_ms: 11_000.0,
            } => sync = Some(timer),
            Action::WsConnect { socket: s, .. } => socket = Some(s),
            _ => {}
        }
    }
    let socket = socket.unwrap();
    e.handle(1000.0, Event::WsOpened { socket });
    for a in e.drain_plain().collect::<Vec<_>>() {
        match a {
            Action::Http(r) if support::kind(&r) == "frontendOpenOrders" => {
                support::reply(&mut e, &r, 200, rows);
            }
            Action::Http(r) => support::reply(&mut e, &r, 200, r#"{"balances":[]}"#),
            _ => {}
        }
    }
    assert!(e.store().order("78").is_some());
    e.handle(
        11_000.0,
        Event::TimerFired {
            timer: sync.unwrap(),
        },
    );
    let stale = e
        .drain_plain()
        .find_map(|a| match a {
            Action::Http(r) if support::kind(&r) == "frontendOpenOrders" => Some(r),
            _ => None,
        })
        .expect("periodic read in flight");
    support::message(
        &mut e,
        socket,
        11_001.0,
        r#"{"channel":"userFills","data":{"isSnapshot":false,"fills":[{"coin":"@1","side":"B","px":"0.0009","sz":"5","time":11001,"tid":31,"oid":78}]}}"#,
    );
    assert!(e.store().order("78").is_none());
    support::reply(&mut e, &stale, 200, rows);
    assert!(
        e.store().order("78").is_none(),
        "stale read restored a filled order"
    );
}

#[test]
fn a_spot_market_without_a_positive_step_is_skipped_as_malformed() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new_spot(Options {
        public_data_only: true,
        ..Options::default()
    }));
    e.start(1000.0);
    let r = take_http(&mut e);
    support::reply(
        &mut e,
        &r,
        200,
        &META.replace(
            r#""name":"HFUN","index":2,"szDecimals":2"#,
            r#""name":"HFUN","index":2,"szDecimals":400"#,
        ),
    );
    assert!(e.store().market("PURR/USDC").is_some());
    assert!(e.store().market("HFUN/USDC").is_none());
}

mod owned_nonpageable {
    use super::*;
    use safe_cex_engine::{Action, ErrorKind, Event, Level, Notice, Outcome, Value, log::code};

    fn queries() -> [Query; 3] {
        [
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
        ]
    }
    fn incomplete(output: &[Action], command: u32) {
        let [
            Action::Notice(Notice::Event(marker)),
            Action::Notice(Notice::CommandDone {
                command: owner,
                result,
            }),
        ] = output
        else {
            panic!("one coverage marker before one result, and no I/O: {output:?}");
        };
        assert_eq!(*owner, command);
        assert_eq!(result, "[]");
        assert_eq!(marker.code, code::HISTORY_INCOMPLETE);
        assert_eq!(marker.level, Level::Warn);
        assert!(marker.message.contains("INCOMPLETE"));
        assert!(marker.message.contains("Hyperliquid spot"));
        assert!(
            matches!(marker.fields.as_slice(), [("command", Value::Num(owner))] if *owner == f64::from(command))
        );
    }

    #[test]
    fn history_has_owned_incomplete_coverage_even_with_cached_orders_and_error_log_level() {
        let (mut e, _) = placed();

        prepared_now(&mut e).into_iter().for_each(drop);
        let before = e.store().orders().to_vec();
        assert!(
            !before.is_empty(),
            "live orders do not establish history completeness"
        );
        let balance = e.store().balance().clone();
        e.set_log_level(Level::Error);
        for (index, query) in queries().into_iter().enumerate() {
            let command = 100 + index as u32;
            e.handle(5000., Event::Command(Command::Query { command, query }));
            incomplete(
                &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
                command,
            );
            assert_eq!(e.store().orders(), before);
            assert_eq!(e.store().balance(), &balance);
        }
    }

    #[test]
    fn public_trade_refusal_is_typed_not_sent_with_original_text_and_no_coverage() {
        let (mut e, _) = placed();
        prepared_now(&mut e).into_iter().for_each(drop);
        e.handle(
            5000.,
            Event::Command(Command::Query {
                command: 200,
                query: Query::PublicTrades {
                    symbol: "HFUN/USDC".into(),
                    limit: Some(10),
                },
            }),
        );
        let output: Vec<_> = prepared_now(&mut e).into_iter().collect();
        let [
            Action::Notice(Notice::CommandFailed {
                command: 200,
                error,
            }),
        ] = output.as_slice()
        else {
            panic!("one refusal, no I/O or unrelated coverage: {output:?}");
        };
        assert_eq!(
            error.message,
            "Hyperliquid spot public trades query is not supported"
        );
        assert_eq!(error.kind, Some(ErrorKind::Unsupported));
        assert_eq!(error.outcome, Some(Outcome::NotSent));
    }

    #[test]
    fn lookup_miss_remains_nondefinitive_without_a_history_marker_or_io() {
        let (mut e, _) = placed();
        prepared_now(&mut e).into_iter().for_each(drop);
        e.handle(
            5000.,
            Event::Command(Command::Query {
                command: 300,
                query: Query::OrderByClientId {
                    symbol: "HFUN/USDC".into(),
                    client_order_id: "0x11111111111111111111111111111111".into(),
                },
            }),
        );
        let output: Vec<_> = prepared_now(&mut e).into_iter().collect();
        let [
            Action::Notice(Notice::CommandDone {
                command: 300,
                result,
            }),
        ] = output.as_slice()
        else {
            panic!("one explicit nondefinitive lookup, no history marker or I/O: {output:?}");
        };
        assert_eq!(result, r#"{"found":false,"definitive":false}"#);
    }
    #[test]
    fn existing_local_lookup_hit_stays_separate_from_unavailable_history() {
        let (mut e, _) = placed();
        prepared_now(&mut e).into_iter().for_each(drop);
        let cached = e.store().orders()[0].clone();
        let client_order_id = cached
            .client_order_id
            .clone()
            .expect("placement retains its client identity");
        e.handle(
            5000.,
            Event::Command(Command::Query {
                command: 400,
                query: Query::OrderByClientId {
                    symbol: cached.symbol.clone(),
                    client_order_id,
                },
            }),
        );
        let output: Vec<_> = prepared_now(&mut e).into_iter().collect();
        let [
            Action::Notice(Notice::CommandDone {
                command: 400,
                result,
            }),
        ] = output.as_slice()
        else {
            panic!(
                "cache lookup retains its own positive contract, no history marker/I/O: {output:?}"
            );
        };
        let result: serde_json::Value = serde_json::from_str(result).unwrap();
        assert_eq!(result["found"], true);
        assert_eq!(result["order"]["id"], cached.id);
        assert!(result.get("definitive").is_none());
        assert_eq!(e.store().orders(), &[cached]);
    }
}
