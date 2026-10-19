//! Documented account/position/order/TPSL envelopes and repository Bitunix contracts.
mod support;
use safe_cex_engine::model::{MarginType, OrderSide, OrderStatus};
use safe_cex_engine::{Action, Event, Notice};
use support::*;
#[test]
fn startup_keeps_reported_net_pnl_and_handles_null_tpsl_and_account_arrays() {
    let (e, _) = startup();
    let p = &e.store().positions()[0];
    assert_eq!(p.realized_pnl, Some(-3.5));
    assert_eq!(p.contracts, 0.01);
    assert_eq!(p.entry_price, 95000.0);
    assert_eq!(e.store().balance().total, 1003.0);
    assert_eq!(e.store().balance().upnl, 12.0);
    assert!(e.store().hedged());
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 20.0);
    assert_eq!(
        e.store().setting("BTCUSDT").unwrap().margin_type,
        Some(MarginType::Isolated)
    );
    assert_eq!(
        e.store().order("protect__position-1__tp").unwrap().side,
        OrderSide::Buy
    );
    assert!(e.store().order("protect__position-1__sl").is_some());
}
#[test]
fn parent_updates_do_not_corrupt_virtual_trigger_prices() {
    let (mut e, s) = startup();
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"one","side":"Sell","price":"105","qty":"3","dealAmount":"2","averagePrice":"100","mtime":"2025-04-05T16:05:39.369077000Z"}}"#,
    );
    assert_eq!(e.store().order("one").unwrap().price, 105.0);
    assert_eq!(e.store().order("one").unwrap().last_update, 1743869139369.0);
    assert_eq!(e.store().order("one__tp").unwrap().price, 80.0);
    assert_eq!(e.store().order("one__sl").unwrap().price, 110.0);
}
#[test]
fn terminal_events_cannot_be_undone_by_delayed_creates_or_cached_rest() {
    let (mut e, s) = startup();
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"CLOSE","symbol":"BTCUSDT","orderId":"one","orderStatus":"FILLED","dealAmount":"3"}}"#,
    );
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"CREATE","symbol":"BTCUSDT","orderId":"one","orderType":"LIMIT","side":"SELL","qty":"3","price":"100"}}"#,
    );
    assert_eq!(e.store().order("one").unwrap().status, OrderStatus::Closed);
    let timer = e
        .drain_actions()
        .filter_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                if at_ms == 1100.0 { Some(timer) } else { None }
            } else {
                None
            }
        })
        .next_back()
        .unwrap();
    e.handle(1100.0, Event::TimerFired { timer });
    let reads = requests(&mut e);
    for r in reads {
        reply(
            &mut e,
            r.id,
            if r.url.contains("/tpsl/") {
                TPSL
            } else {
                ORDERS
            },
        );
    }
    assert!(
        !e.store()
            .orders()
            .iter()
            .any(|o| o.id == "one" && o.status == OrderStatus::Open)
    );
}
#[test]
fn signature_headers_cover_sorted_query_and_exact_body() {
    let mut e = safe_cex_engine::Engine::new(safe_cex_venue_bitunix::BitunixFutures::new(
        safe_cex_venue_bitunix::Options {
            key: Some("key".into()),
            secret: Some("secret".into()),
            ..Default::default()
        },
    ));
    e.start(0.0);
    let r = requests(&mut e).remove(0);
    reply(&mut e, r.id, PAIRS);
    let r = requests(&mut e).remove(0);
    reply(&mut e, r.id, TICKERS);
    let r = requests(&mut e)
        .into_iter()
        .find(|r| r.url.contains("marginCoin"))
        .unwrap();
    let h = |name| {
        r.headers
            .iter()
            .find(|(n, _)| *n == name)
            .unwrap()
            .1
            .as_str()
    };
    let digest = safe_cex_crypto::hex(&safe_cex_crypto::sha256(
        format!("{}{}keymarginCoinUSDT", h("nonce"), h("timestamp")).as_bytes(),
    ));
    assert_eq!(
        h("sign"),
        safe_cex_crypto::hex(&safe_cex_crypto::sha256(
            format!("{digest}secret").as_bytes()
        ))
    );
    assert!(r.body.is_none());
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { .. })))
    );
}

fn balance_request(e: &mut TestEngine, s: safe_cex_engine::SocketId) -> safe_cex_engine::RequestId {
    event(e, s, r#"{"ch":"balance","data":{"available":"900"}}"#);
    let timer = e
        .drain_actions()
        .filter_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                if at_ms == 1100.0 { Some(timer) } else { None }
            } else {
                None
            }
        })
        .next_back()
        .unwrap();
    e.handle(1100.0, Event::TimerFired { timer });
    requests(e).remove(0).id
}
#[test]
fn newer_socket_and_overlapping_reads_cannot_restore_old_balance() {
    let (mut e, s) = startup();
    let a = balance_request(&mut e, s);
    let b = balance_request(&mut e, s);
    reply(
        &mut e,
        b,
        r#"{"code":0,"data":{"data":[{"marginCoin":"USDT","available":"700","margin":"10","crossUnrealizedPNL":"0","isolationUnrealizedPNL":"0","unrealizedPNL":"999"}]}}"#,
    );
    assert_eq!(e.store().balance().free, 700.0);
    assert_eq!(e.store().balance().upnl, 0.0);
    reply(&mut e, a, ACCOUNT);
    assert_eq!(e.store().balance().free, 700.0);
    let a = balance_request(&mut e, s);
    event(&mut e, s, r#"{"ch":"balance","data":{}}"#);
    reply(&mut e, a, ACCOUNT);
    assert_eq!(e.store().balance().free, 700.0);
    let a = balance_request(&mut e, s);
    reply(
        &mut e,
        a,
        r#"{"code":10001,"msg":"request too frequently","data":[]}"#,
    );
    assert_eq!(e.store().balance().free, 700.0);
    let a = balance_request(&mut e, s);
    reply(&mut e, a, r#"{"code":0,"data":{"unexpected":1}}"#);
    assert_eq!(e.store().balance().free, 700.0);
}
#[test]
fn private_heartbeat_preserves_legacy_silent_sessions() {
    let (mut e, s) = startup();
    e.handle(
        1.0,
        Event::WsClosed {
            socket: s,
            code: 1006,
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    let socket = actions
        .iter()
        .find_map(|a| {
            if let Action::WsConnect { socket, .. } = a {
                Some(*socket)
            } else {
                None
            }
        })
        .unwrap();
    let timer = actions
        .iter()
        .find_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(*timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(1.0, Event::WsOpened { socket });
    e.drain_actions().for_each(drop);
    e.handle(50_000.0, Event::TimerFired { timer });
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a,Action::WsClose {socket:s} if s==socket))
    );
}

#[test]
fn incremental_fills_use_rest_baseline_and_dedupe_repeated_updates() {
    let (mut e, s) = startup();
    e.take_frame(0.0);
    let update = r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"one","side":"Sell","qty":"3","dealAmount":"2","averagePrice":"150"}}"#;
    event(&mut e, s, update);
    event(&mut e, s, update);
    let bytes = e.take_market_frame(100.0).unwrap();
    let mut at = 16;
    let mut found = 0;
    while at < bytes.len() {
        let kind = u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap());
        let size = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        let payload = &bytes[at + 8..at + 8 + size];
        if kind == 11 {
            assert_eq!(u32::from_le_bytes(payload[..4].try_into().unwrap()), 1);
            assert_eq!(
                f64::from_le_bytes(payload[16..24].try_into().unwrap()),
                200.0
            );
            assert_eq!(f64::from_le_bytes(payload[24..32].try_into().unwrap()), 1.0);
            found += 1;
        }
        at += 8 + size;
    }
    assert_eq!(found, 1);
}

#[test]
fn malformed_order_rows_preserve_previous_snapshot() {
    let (mut e, s) = startup();
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"one","dealAmount":"1"}}"#,
    );
    let timer = e
        .drain_actions()
        .filter_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                if at_ms == 1100.0 { Some(timer) } else { None }
            } else {
                None
            }
        })
        .next_back()
        .unwrap();
    e.handle(1100.0, Event::TimerFired { timer });
    for r in requests(&mut e) {
        reply(
            &mut e,
            r.id,
            if r.url.contains("/tpsl/") {
                TPSL
            } else {
                r#"{"code":0,"data":{"orderList":[{}]}}"#
            },
        );
    }
    assert!(e.store().order("one").is_some());
    assert!(e.store().order("").is_none());
}
