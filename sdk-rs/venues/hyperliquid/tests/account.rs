//! Official clearinghouseState, frontendOpenOrders and WsUserFills payload shapes.
mod support;
use safe_cex_engine::{Action, Event};
use support::TestEngine;
use support::*;
fn reconnect_reads(
    e: &mut TestEngine<safe_cex_venue_hyperliquid::Hyperliquid>,
    hip: bool,
) -> Vec<safe_cex_engine::HttpRequest> {
    let mut requests = vec![
        e.wait_http(|r| {
            kind(r) == "clearinghouseState" && !r.body.as_ref().unwrap().contains("xyz")
        }),
        e.wait_http(|r| {
            kind(r) == "frontendOpenOrders" && !r.body.as_ref().unwrap().contains("xyz")
        }),
    ];
    if hip {
        requests.push(e.wait_http(|r| {
            kind(r) == "clearinghouseState" && r.body.as_ref().unwrap().contains("xyz")
        }));
        requests.push(e.wait_http(|r| {
            kind(r) == "frontendOpenOrders" && r.body.as_ref().unwrap().contains("xyz")
        }));
    }
    requests
}
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
    let requests = reconnect_reads(&mut e, false);
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
    let requests = reconnect_reads(&mut e, false);
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
    let requests = reconnect_reads(&mut e, true);
    message(
        &mut e,
        s,
        3000.0,
        r#"{"channel":"userFills","data":{"fills":[{"coin":"xyz:FOO","side":"B","px":"50","sz":"2","time":3000,"tid":11,"oid":91,"dir":"Open Long","startPosition":"0","closedPnl":"0","fee":"0.01"}]}}"#,
    );
    let refresh_at = e.now + 350.0;
    let timer = e
        .drain_actions()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                (at_ms == refresh_at).then_some(timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(refresh_at, Event::TimerFired { timer });
    let next = e
        .drain_actions()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                (at_ms == refresh_at + 350.0).then_some(timer)
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
    e.handle(refresh_at + 350.0, Event::TimerFired { timer: next });
    let refreshed = e
        .wait_http(|r| kind(r) == "clearinghouseState" && r.body.as_ref().unwrap().contains("xyz"));
    assert!(refreshed.body.as_ref().unwrap().contains("xyz"));
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
    let requests = reconnect_reads(&mut e, false);
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

#[test]
fn delayed_hip3_state_preserves_position_after_newer_fill() {
    let (mut e, s) = ready_with_dex("disabled", true);
    let state = r#"{"channel":"clearinghouseState","data":{"dex":"xyz","clearinghouseState":{"assetPositions":[{"position":{"coin":"xyz:FOO","szi":"2","entryPx":"50","positionValue":"100","unrealizedPnl":"0","leverage":{"type":"cross","value":10}}}],"time":1500}}}"#;
    message(&mut e, s, 1500.0, state);
    assert!(
        e.store()
            .positions()
            .iter()
            .any(|p| p.symbol == "XYZ|FOO-PERP")
    );
    message(
        &mut e,
        s,
        5000.0,
        r#"{"channel":"userFills","data":{"fills":[{"coin":"xyz:FOO","side":"B","px":"50","sz":"1","time":5000,"tid":11,"oid":91,"dir":"Open Long","startPosition":"2"}]}}"#,
    );
    message(
        &mut e,
        s,
        5100.0,
        r#"{"channel":"userFills","data":{"fills":[{"coin":"xyz:FOO","side":"B","px":"50","sz":"1","time":3000,"tid":12,"oid":92,"dir":"Open Long","startPosition":"1"}]}}"#,
    );
    message(
        &mut e,
        s,
        5200.0,
        &state
            .replace("1500", "4000")
            .replace("\"szi\":\"2\"", "\"szi\":\"1\""),
    );
    assert_eq!(
        e.store()
            .positions()
            .iter()
            .find(|p| p.symbol == "XYZ|FOO-PERP")
            .unwrap()
            .contracts,
        2.0
    );
}

#[test]
fn reconnect_retries_order_snapshot_after_unrelated_stream_event() {
    let (mut e, s) = ready("disabled");
    e.handle(2000.0, Event::WsOpened { socket: s });
    let requests = reconnect_reads(&mut e, false);
    message(
        &mut e,
        s,
        2100.0,
        r#"{"channel":"orderUpdates","data":[{"order":{"coin":"BTC","oid":9,"side":"B","limitPx":"90","origSz":"1","sz":"1","orderType":"Limit","tif":"Gtc"},"status":"open"}]}"#,
    );
    let snapshot = r#"[{"coin":"BTC","oid":8,"side":"B","limitPx":"80","origSz":"1","sz":"1","orderType":"Limit","tif":"Gtc"},{"coin":"BTC","oid":9,"side":"B","limitPx":"90","origSz":"1","sz":"1","orderType":"Limit","tif":"Gtc"}]"#;
    for r in requests {
        let body = if kind(&r) == "frontendOpenOrders" {
            snapshot
        } else {
            STATE
        };
        e.handle(
            2200.0,
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
    assert!(e.store().order("9").is_some());
    let retry_at = e.now + 500.0;
    let timer = e
        .drain_actions()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                (at_ms == retry_at).then_some(timer)
            } else {
                None
            }
        })
        .expect("invalidated reconnect snapshot must retry");
    e.handle(retry_at, Event::TimerFired { timer });
    let r = e.wait_http(|r| kind(r) == "frontendOpenOrders");
    e.handle(
        2800.0,
        Event::HttpResponse {
            request: r.id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body: snapshot,
            },
        },
    );
    assert!(e.store().order("8").is_some());
    assert!(e.store().order("9").is_some());
}

#[test]
fn streamed_snapshot_does_not_resurrect_acknowledged_cancel() {
    for finished_at in [2400.0, 40_000.0] {
        let (mut e, s) = ready("disabled");
        let row = r#"{"coin":"BTC","oid":9,"side":"B","limitPx":"90","origSz":"1","sz":"1","orderType":"Limit","tif":"Gtc"}"#;
        message(
            &mut e,
            s,
            2000.0,
            &format!(r#"{{"channel":"orderUpdates","data":[{{"order":{row},"status":"open"}}]}}"#),
        );
        e.handle(2100.0, Event::WsOpened { socket: s });
        let r = e.wait_http(|r| kind(r) == "frontendOpenOrders");
        e.handle(
            2200.0,
            Event::HttpStart {
                request: r.id,
                status: 200,
                headers: "",
            },
        );
        e.handle(
            2200.0,
            Event::HttpChunk {
                request: r.id,
                data: format!("[{row},").as_bytes(),
            },
        );
        e.handle(
            2300.0,
            Event::Command(safe_cex_engine::Command::CancelOrders {
                command: 1,
                ids: vec!["9".into()],
            }),
        );
        let cancel = e.wait_http(|r| r.url.ends_with("/exchange"));
        e.handle(2301.0, Event::HttpResponse {
        request: cancel.id,
        response: safe_cex_engine::Response {status: 200, headers: "", body: r#"{"status":"ok","response":{"type":"cancel","data":{"statuses":["success"]}}}"#},
    });
        assert!(e.store().order("9").is_none());
        e.handle(finished_at, Event::HttpChunk {request: r.id, data: br#"{"coin":"BTC","oid":10,"side":"B","limitPx":"80","origSz":"1","sz":"1","orderType":"Limit","tif":"Gtc"}]"#});
        e.handle(finished_at, Event::HttpEnd { request: r.id });
        assert!(e.store().order("9").is_none());
        assert!(
            e.drain_actions()
                .any(|a| matches!(a, Action::SetTimer { at_ms, .. } if at_ms == e.now + 500.0))
        );
    }
}
