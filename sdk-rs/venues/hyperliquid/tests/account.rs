//! Official clearinghouseState, frontendOpenOrders and WsUserFills payload shapes.
mod support;
use safe_cex_engine::{Action, Event};
use support::*;
#[test]
fn private_bootstrap_separates_positions_and_market_settings() {
    let (e, _) = ready("disabled");
    assert_eq!(e.store().positions().len(), 1);
    assert_eq!(e.store().positions()[0].id, "BTC-PERP");
    assert_eq!(e.store().setting("BTC-PERP").unwrap().leverage, 10.0);
    assert_eq!(e.store().setting("ETH-PERP").unwrap().leverage, 20.0);
    assert_eq!(e.store().balance().total, 1000.0);
    assert_eq!(e.store().balance().free, 980.0);
}
#[test]
fn newer_stream_state_wins_over_inflight_rest_and_failed_reads_preserve_state() {
    let (mut e, s) = ready("disabled");
    e.handle(2000.0, Event::WsOpened { socket: s });
    let requests = e
        .drain_actions()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    message(
        &mut e,
        s,
        3000.0,
        r#"{"channel":"clearinghouseState","data":{"user":"x","dex":"","clearinghouseState":{"assetPositions":[{"position":{"coin":"BTC","szi":"3","entryPx":"100","positionValue":"300","unrealizedPnl":"0","liquidationPx":null,"marginUsed":"30","leverage":{"type":"isolated","value":10}}}],"marginSummary":{"accountValue":"1000","totalMarginUsed":"30"},"time":3000}}}"#,
    );
    for r in requests {
        if kind(&r) == "clearinghouseState" {
            reply(&mut e, &r, 200, STATE);
        } else {
            reply(&mut e, &r, 503, "temporarily unavailable");
        }
    }
    assert_eq!(e.store().positions()[0].contracts, 3.0);
    assert_eq!(e.store().balance().used, 30.0);
}
#[test]
fn live_mids_preserve_wallet_total_and_update_available_margin() {
    let (mut e, s) = ready("disabled");
    message(
        &mut e,
        s,
        2000.0,
        r#"{"channel":"allMids","data":{"mids":{"BTC":"110"}}}"#,
    );
    assert_eq!(e.store().balance().total, 1000.0);
    assert_eq!(e.store().balance().upnl, 20.0);
    assert_eq!(e.store().balance().free, 1000.0);
}
#[test]
fn authoritative_unified_spot_balance_is_stable_between_spot_frames() {
    let (mut e, s) = ready("portfolioMargin");
    assert_eq!(e.store().balance().free, 1050.0);
    message(
        &mut e,
        s,
        2000.0,
        r#"{"channel":"allMids","data":{"mids":{"BTC":"110"}}}"#,
    );
    assert_eq!(e.store().balance().total, 1100.0);
    assert_eq!(e.store().balance().used, 50.0);
    assert_eq!(e.store().balance().free, 1050.0);
    assert_eq!(e.store().balance().upnl, 20.0);
    message(
        &mut e,
        s,
        3000.0,
        r#"{"channel":"spotState","data":{"user":"x","spotState":{"balances":[],"tokenToAvailableAfterMaintenance":[[0,"0"]]}}}"#,
    );
    assert_eq!(e.store().balance().free, 0.0);
}
#[test]
fn snapshot_fills_do_not_emit_and_duplicate_live_executions_emit_once() {
    let (mut e, s) = ready("disabled");
    let fill = r#"{"coin":"BTC","side":"B","px":"100","sz":"0.5","time":2000,"tid":7,"oid":99,"dir":"Open Long","startPosition":"2","closedPnl":"0","fee":"0.01"}"#;
    message(
        &mut e,
        s,
        2000.0,
        &format!(r#"{{"channel":"userFills","data":{{"isSnapshot":true,"fills":[{fill}]}}}}"#),
    );
    assert_eq!(fill_count(e.take_frame(2000.0).unwrap()), 0);
    for _ in 0..2 {
        message(
            &mut e,
            s,
            2000.0,
            &format!(r#"{{"channel":"userFills","data":{{"isSnapshot":false,"fills":[{fill}]}}}}"#),
        );
    }
    assert_eq!(e.store().positions()[0].contracts, 2.5);
    let frame = e.take_frame(2000.0).unwrap();
    assert_eq!(fill_count(frame), 1);
}
#[test]
fn explicit_type_without_tif_preserves_existing_post_only() {
    let (mut e, s) = ready("disabled");
    message(
        &mut e,
        s,
        2000.0,
        r#"{"channel":"orderUpdates","data":[{"order":{"coin":"BTC","oid":9,"side":"B","limitPx":"90","origSz":"1","sz":"1","orderType":"Limit","tif":"Alo"},"status":"open"}]}"#,
    );
    message(
        &mut e,
        s,
        2100.0,
        r#"{"channel":"orderUpdates","data":[{"order":{"coin":"BTC","oid":9,"side":"B","limitPx":"90","origSz":"1","sz":"0.5","orderType":"Limit"},"status":"open"}]}"#,
    );
    assert_eq!(
        e.store().order("9").unwrap().time_in_force,
        Some(safe_cex_engine::model::TimeInForce::PostOnly)
    );
}
#[test]
fn user_event_order_objects_and_raw_arrays_remove_terminal_orders() {
    for (event, terminal) in [
        (
            r#"{"orders":[{"order":{"coin":"BTC","oid":9,"side":"B","limitPx":"90","origSz":"1","sz":"1"},"status":"open"}]}"#,
            r#"{"orders":[{"order":{"coin":"BTC","oid":9,"side":"B","origSz":"1","sz":"0"},"status":"filled"}]}"#,
        ),
        (
            r#"[{"order":{"coin":"BTC","oid":9,"side":"B","limitPx":"90","origSz":"1","sz":"1"},"status":"open"}]"#,
            r#"[{"order":{"coin":"BTC","oid":9,"side":"B","origSz":"1","sz":"0"},"status":"canceled"}]"#,
        ),
    ] {
        let (mut e, s) = ready("disabled");
        message(
            &mut e,
            s,
            2000.0,
            &format!(r#"{{"channel":"user","data":{event}}}"#),
        );
        assert!(e.store().order("9").is_some());
        message(
            &mut e,
            s,
            3000.0,
            &format!(r#"{{"channel":"user","data":{terminal}}}"#),
        );
        assert!(e.store().order("9").is_none());
    }
}
#[test]
fn newer_authoritative_rest_closes_positions_after_partial_stream() {
    let (mut e, s) = ready("disabled");
    e.handle(2000.0, Event::WsOpened { socket: s });
    let requests = e
        .drain_actions()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let partial = r#"{"assetPositions":[{"position":{"coin":"ETH","szi":"1","entryPx":"200","positionValue":"200","unrealizedPnl":"0","leverage":{"type":"cross","value":20}}}],"marginSummary":{"accountValue":"1000","totalMarginUsed":"10"},"time":3000}"#;
    message(
        &mut e,
        s,
        3000.0,
        &format!(r#"{{"channel":"clearinghouseState","data":{partial}}}"#),
    );
    assert_eq!(e.store().positions().len(), 2);
    for r in requests {
        if kind(&r) == "clearinghouseState" {
            reply(&mut e, &r, 200, &partial.replace("3000", "4000"));
        } else {
            reply(&mut e, &r, 200, "[]");
        }
    }
    assert_eq!(e.store().positions().len(), 1);
    assert_eq!(e.store().positions()[0].symbol, "ETH-PERP");
}
#[test]
fn partial_stream_for_another_coin_does_not_suppress_delayed_fill() {
    let (mut e, s) = ready("disabled");
    message(
        &mut e,
        s,
        3000.0,
        r#"{"channel":"clearinghouseState","data":{"assetPositions":[{"position":{"coin":"ETH","szi":"1","entryPx":"200","positionValue":"200","unrealizedPnl":"0","leverage":{"type":"cross","value":20}}}],"time":3000}}"#,
    );
    message(
        &mut e,
        s,
        3100.0,
        r#"{"channel":"userFills","data":{"fills":[{"coin":"BTC","side":"A","px":"100","sz":"2","time":2500,"tid":8,"oid":90,"dir":"Close Long","startPosition":"2","closedPnl":"0","fee":"0.01"}]}}"#,
    );
    assert!(!e.store().positions().iter().any(|p| p.symbol == "BTC-PERP"));
}
#[test]
fn direct_server_time_guards_older_account_snapshots() {
    let (mut e, s) = ready("disabled");
    let newer = STATE
        .replace("\"time\":1000", "\"serverTime\":3000")
        .replace("\"szi\":\"2\"", "\"szi\":\"3\"");
    message(
        &mut e,
        s,
        3000.0,
        &format!(r#"{{"channel":"clearinghouseState","data":{newer}}}"#),
    );
    let older = STATE.replace("\"time\":1000", "\"serverTime\":2000");
    message(
        &mut e,
        s,
        3100.0,
        &format!(r#"{{"channel":"clearinghouseState","data":{older}}}"#),
    );
    assert_eq!(e.store().positions()[0].contracts, 3.0);
}

fn fill_count(frame: &[u8]) -> u32 {
    let mut offset = 16;
    while offset + 8 <= frame.len() {
        let kind = u16::from_le_bytes(frame[offset..offset + 2].try_into().unwrap());
        let size = u32::from_le_bytes(frame[offset + 4..offset + 8].try_into().unwrap()) as usize;
        if kind == 11 {
            return u32::from_le_bytes(frame[offset + 8..offset + 12].try_into().unwrap());
        }
        offset += 8 + size;
    }
    0
}
#[test]
fn hip3_fill_keeps_a_refresh_pending_behind_inflight_state() {
    let (mut e, s) = ready_with_dex("disabled", true);
    e.handle(2000.0, Event::WsOpened { socket: s });
    let requests = e
        .drain_actions()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    message(
        &mut e,
        s,
        3000.0,
        r#"{"channel":"userFills","data":{"fills":[{"coin":"xyz:FOO","side":"B","px":"50","sz":"2","time":3000,"tid":11,"oid":91,"dir":"Open Long","startPosition":"0","closedPnl":"0","fee":"0.01"}]}}"#,
    );
    let timer = e
        .drain_actions()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                (at_ms == 3350.0).then_some(timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(3350.0, Event::TimerFired { timer });
    let next = e
        .drain_actions()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                (at_ms == 3700.0).then_some(timer)
            } else {
                None
            }
        })
        .unwrap();
    for r in requests {
        let body = if kind(&r) == "clearinghouseState" {
            if r.body.as_ref().unwrap().contains("xyz") {
                r#"{"assetPositions":[],"time":1000}"#
            } else {
                STATE
            }
        } else {
            "[]"
        };
        reply(&mut e, &r, 200, body);
    }
    e.handle(3700.0, Event::TimerFired { timer: next });
    assert!(e.drain_actions().any(|a|matches!(a,Action::Http(r) if kind(&r)=="clearinghouseState" && r.body.as_ref().unwrap().contains("xyz"))));
}
#[test]
fn rest_after_cancel_trust_window_reenables_live_order_updates() {
    let (mut e, s) = ready("disabled");
    message(
        &mut e,
        s,
        2000.0,
        r#"{"channel":"orderUpdates","data":[{"order":{"coin":"BTC","oid":9,"side":"B","origSz":"1","sz":"0"},"status":"canceled"}]}"#,
    );
    e.handle(40000.0, Event::WsOpened { socket: s });
    let requests = e
        .drain_actions()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    for r in requests {
        let body = if kind(&r) == "frontendOpenOrders" {
            r#"[{"coin":"BTC","oid":9,"side":"B","limitPx":"90","origSz":"1","sz":"1","orderType":"Limit","tif":"Gtc"}]"#
        } else {
            STATE
        };
        e.handle(
            40001.0,
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
    message(
        &mut e,
        s,
        40002.0,
        r#"{"channel":"orderUpdates","data":[{"order":{"coin":"BTC","oid":9,"side":"B","limitPx":"90","origSz":"1","sz":"0.5"},"status":"open"}]}"#,
    );
    assert_eq!(e.store().order("9").unwrap().remaining, 0.5);
}
#[test]
fn same_millisecond_partial_executions_both_update_position() {
    let (mut e, s) = ready("disabled");
    message(
        &mut e,
        s,
        2000.0,
        r#"{"channel":"userFills","data":{"fills":[{"coin":"BTC","side":"B","px":"100","sz":"0.5","time":2000,"tid":7,"oid":99,"startPosition":"2"},{"coin":"BTC","side":"B","px":"100","sz":"0.5","time":2000,"tid":8,"oid":99,"startPosition":"2.5"}]}}"#,
    );
    assert_eq!(e.store().positions()[0].contracts, 3.0);
    assert_eq!(fill_count(e.take_frame(2000.0).unwrap()), 2);
}
