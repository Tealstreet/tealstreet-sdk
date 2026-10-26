//! Official clearinghouseState, frontendOpenOrders and WsUserFills payload shapes.
mod support;
use safe_cex_engine::{Action, Event};
use support::*;
/// Native cadence of the venue's recurring account poll; policy scales it.
const POLL_MS: f64 = 120_000.0;

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
    let (s, _) = reconnect(&mut e, s, 2000.0);
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
    let (s, _) = reconnect(&mut e, s, 2000.0);
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
    let (s, _) = reconnect(&mut e, s, 2000.0);
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
        4000.0,
        r#"{"channel":"userFills","data":{"fills":[{"coin":"xyz:FOO","side":"B","px":"50","sz":"2","time":3000,"tid":11,"oid":91,"dir":"Open Long","startPosition":"0","closedPnl":"0","fee":"0.01"}]}}"#,
    );
    let timer = e
        .drain_actions()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                (at_ms == 4350.0).then_some(timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(4350.0, Event::TimerFired { timer });
    let next = e
        .drain_actions()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                (at_ms == 4700.0).then_some(timer)
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
    e.handle(4700.0, Event::TimerFired { timer: next });
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
    let (s, _) = reconnect(&mut e, s, 40000.0);
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
            41001.0,
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
        41002.0,
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
    let (s, _) = reconnect(&mut e, s, 2000.0);
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
        3100.0,
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
            3200.0,
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
    let timer = e
        .drain_actions()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                (at_ms == 3700.0).then_some(timer)
            } else {
                None
            }
        })
        .expect("invalidated reconnect snapshot must retry");
    e.handle(3700.0, Event::TimerFired { timer });
    let r = e
        .drain_actions()
        .find_map(|a| {
            if let Action::Http(r) = a {
                (kind(&r) == "frontendOpenOrders").then_some(r)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(
        3800.0,
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
    for finished_at in [3400.0, 41_000.0] {
        let (mut e, s) = ready("disabled");
        let row = r#"{"coin":"BTC","oid":9,"side":"B","limitPx":"90","origSz":"1","sz":"1","orderType":"Limit","tif":"Gtc"}"#;
        message(
            &mut e,
            s,
            2000.0,
            &format!(r#"{{"channel":"orderUpdates","data":[{{"order":{row},"status":"open"}}]}}"#),
        );
        let (_replacement, _) = reconnect(&mut e, s, 2100.0);
        let r = e
            .drain_actions()
            .find_map(|a| {
                if let Action::Http(r) = a {
                    (kind(&r) == "frontendOpenOrders").then_some(r)
                } else {
                    None
                }
            })
            .unwrap();
        e.handle(
            3200.0,
            Event::HttpStart {
                request: r.id,
                status: 200,
                headers: "",
            },
        );
        e.handle(
            3200.0,
            Event::HttpChunk {
                request: r.id,
                data: format!("[{row},").as_bytes(),
            },
        );
        e.handle(
            3300.0,
            Event::Command(safe_cex_engine::Command::CancelOrders {
                command: 1,
                ids: vec!["9".into()],
            }),
        );
        let cancel = e
            .drain_actions()
            .find_map(|a| {
                if let Action::Http(r) = a {
                    r.url.ends_with("/exchange").then_some(r)
                } else {
                    None
                }
            })
            .unwrap();
        e.handle(3301.0, Event::HttpResponse {
        request: cancel.id,
        response: safe_cex_engine::Response {status: 200, headers: "", body: r#"{"status":"ok","response":{"type":"cancel","data":{"statuses":["success"]}}}"#},
    });
        assert!(e.store().order("9").is_none());
        e.handle(finished_at, Event::HttpChunk {request: r.id, data: br#"{"coin":"BTC","oid":10,"side":"B","limitPx":"80","origSz":"1","sz":"1","orderType":"Limit","tif":"Gtc"}]"#});
        e.handle(finished_at, Event::HttpEnd { request: r.id });
        assert!(e.store().order("9").is_none());
        assert!(
            e.drain_actions().any(
                |a| matches!(a, Action::SetTimer { at_ms, .. } if at_ms == finished_at + 500.0)
            )
        );
    }
}

#[test]
fn periodic_account_policy_changes_preserve_inflight_reads_and_fence_stale_timers() {
    let (mut e, _) = ready_with_dex("disabled", true);
    let now = 2000.;
    e.drain_actions().for_each(drop);
    e.handle(
        now,
        Event::PollScaleChanged {
            scale: f64::INFINITY,
        },
    );
    let stale: Vec<_> = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::ClearTimer { timer } => Some(timer),
            _ => None,
        })
        .collect();
    assert_eq!(stale.len(), 1, "one recurring account registration");
    let obsolete = stale;
    let mut current = None;
    // The 120s native cadence is both the venue minimum and the host policy cap,
    // so every finite policy resolves to 120s: only resuming from pause re-arms.
    for interval in [1_500_f64, 100_000.] {
        e.handle(
            now,
            Event::PollScaleChanged {
                scale: interval / POLL_MS,
            },
        );
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        let timers: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                _ => None,
            })
            .collect();
        if current.is_some() {
            assert!(timers.is_empty(), "unchanged 120s cadence keeps its timer");
            continue;
        }
        assert_eq!(timers.len(), 1);
        assert_eq!(timers[0].1, now + 120_000.0);
        current = Some(timers[0]);
    }
    for timer in obsolete {
        e.handle(now, Event::TimerFired { timer });
        assert!(!e.drain_actions().any(|a| matches!(a, Action::Http(_))));
    }
    let (timer, due) = current.unwrap();
    e.handle(due, Event::TimerFired { timer });
    let mut pending: Vec<_> = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    assert!(
        pending.iter().any(|r| kind(r) == "clearinghouseState"),
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
    e.drain_actions().for_each(drop);
    for _ in 0..12 {
        if pending.is_empty() {
            break;
        }
        for r in std::mem::take(&mut pending) {
            let body = r#"{"assetPositions":[],"marginSummary":{"accountValue":"1000","totalMarginUsed":"0"},"time":1000}"#;
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
            pending.extend(e.drain_actions().filter_map(|a| match a {
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
    let resumed: Vec<_> = e
        .drain_actions()
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
    assert_eq!(resumed[0].1, due + 2. + 10_000_f64.max(120000.0));
}

fn execution_ids(frame: &[u8]) -> Vec<&str> {
    use safe_cex_engine::wire::{section, sections};
    let fills = sections(frame)
        .find(|(kind, _)| *kind == section::FILLS)
        .unwrap()
        .1;
    let strings = sections(frame)
        .find(|(kind, _)| *kind == section::STRINGS)
        .unwrap()
        .1;
    let read = |data: &[u8], at| u32::from_le_bytes(data[at..at + 4].try_into().unwrap()) as usize;
    let count = read(fills, 0);
    let ids = (4 + count * 12).next_multiple_of(8) + count * 20;
    let text = 4 + (read(strings, 0) + 1) * 4;
    (0..count)
        .map(|i| {
            let id = read(fills, ids + i * 4);
            let start = read(strings, 4 + id * 4);
            let end = read(strings, 8 + id * 4);
            std::str::from_utf8(&strings[text + start..text + end]).unwrap()
        })
        .collect()
}

#[test]
fn live_close_fill_financials_are_net_and_do_not_invent_opening_values() {
    let (mut e, socket) = ready("disabled");
    let fills = r#"{"channel":"userFills","data":{"isSnapshot":false,"fills":[{"coin":"BTC","side":"B","px":"100","sz":"0.1","time":2000,"tid":71,"oid":9007199254740995,"dir":"Open Long","startPosition":"2","closedPnl":"99","fee":"99"},{"coin":"BTC","side":"A","px":"101","sz":"0.1","time":2001,"tid":72,"oid":9007199254740996,"dir":"Close Long","startPosition":"2.1","closedPnl":"4","fee":"0.25"},{"coin":"BTC","side":"B","px":"102","sz":"0.1","time":2002,"tid":73,"oid":9007199254740997,"dir":"Close Short","startPosition":"-0.1","closedPnl":"0","fee":"0"}]}}"#;
    message(&mut e, socket, 2100., fills);
    message(&mut e, socket, 2100., fills);
    let frame = e.take_frame(2100.).unwrap();
    let payload = safe_cex_engine::wire::sections(frame)
        .find(|(kind, _)| *kind == safe_cex_engine::wire::section::FILLS)
        .unwrap()
        .1;
    assert_eq!(u32::from_le_bytes(payload[..4].try_into().unwrap()), 3);
    assert_eq!(payload.len(), 160);
    assert_eq!(execution_ids(frame), ["71", "72", "73"]);
    let symbol_refs: Vec<_> = payload[88..100]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|value| u32::from_le_bytes(*value))
        .collect();
    assert_eq!(symbol_refs.len(), 3);
    assert!(
        symbol_refs
            .iter()
            .all(|value| *value != u32::MAX && *value == symbol_refs[0])
    );
    let value = |at| f64::from_le_bytes(payload[at..at + 8].try_into().unwrap());
    assert_eq!([value(40), value(56), value(72)], [100., 101., 102.]);
    assert!(
        value(112).is_nan(),
        "opening executions do not carry close PnL"
    );
    assert_eq!([value(120), value(128)], [3.75, 0.]);
    assert!(
        value(136).is_nan(),
        "opening executions do not carry close fees"
    );
    assert_eq!([value(144), value(152)], [0.25, 0.]);
    assert!(e.take_market_frame(2100.).is_none());
}

#[test]
fn close_fill_missing_financial_fields_remain_absent_instead_of_becoming_zero() {
    for (fields, has_fee) in [
        ("", false),
        (r#","closedPnl":"4""#, false),
        (r#","fee":"0""#, true),
    ] {
        let (mut e, socket) = ready("disabled");
        let fill = format!(
            r#"{{"channel":"userFills","data":{{"isSnapshot":false,"fills":[{{"coin":"BTC","side":"A","px":"100","sz":"0.1","time":2000,"tid":81,"oid":99,"dir":"Close Long","startPosition":"2"{fields}}}]}}}}"#
        );
        message(&mut e, socket, 2100., &fill);
        let frame = e.take_frame(2100.).unwrap();
        let payload = safe_cex_engine::wire::sections(frame)
            .find(|(kind, _)| *kind == safe_cex_engine::wire::section::FILLS)
            .unwrap()
            .1;
        assert_eq!(u32::from_le_bytes(payload[..4].try_into().unwrap()), 1);
        assert_eq!(payload.len(), 56);
        assert_ne!(
            u32::from_le_bytes(payload[32..36].try_into().unwrap()),
            u32::MAX
        );
        assert_ne!(
            u32::from_le_bytes(payload[36..40].try_into().unwrap()),
            u32::MAX
        );
        if has_fee {
            assert!(f64::from_le_bytes(payload[40..48].try_into().unwrap()).is_nan());
            assert_eq!(f64::from_le_bytes(payload[48..56].try_into().unwrap()), 0.);
        }
    }
}

struct ResyncSocket {
    socket: safe_cex_engine::SocketId,
    now: f64,
}

/// Reconnects and answers each account read by kind; returns the warnings logged.
fn resync(
    e: &mut safe_cex_engine::Engine<safe_cex_venue_hyperliquid::Hyperliquid>,
    s: &mut ResyncSocket,
    now: f64,
    state: &str,
    orders: &str,
) -> Vec<String> {
    let (replacement, at) = reconnect(e, s.socket, now.max(s.now));
    s.socket = replacement;
    s.now = at;
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
        let body = match kind(&r).as_str() {
            "clearinghouseState" => state,
            "frontendOpenOrders" => orders,
            _ => "[]",
        };
        e.handle(
            at,
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
    e.drain_actions()
        .filter_map(|a| match a {
            Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
                message,
                ..
            })) => Some(message),
            _ => None,
        })
        .collect()
}

#[test]
fn bad_order_rows_keep_their_stored_copies_until_a_valid_snapshot_omits_them() {
    let (mut e, socket) = ready("disabled");
    let mut s = ResyncSocket {
        socket,
        now: 1000.0,
    };
    let row = |oid: u32, coin: &str, px: &str| {
        format!(
            r#"{{"coin":"{coin}","oid":{oid},"side":"B","limitPx":"{px}","origSz":"1","sz":"1","orderType":"Limit","tif":"Gtc"}}"#
        )
    };
    let seeded = format!(
        "[{},{},{}]",
        row(8, "BTC", "80"),
        row(9, "BTC", "90"),
        row(10, "BTC", "70")
    );
    resync(&mut e, &mut s, 2000.0, STATE, &seeded);
    assert_eq!(e.store().orders().len(), 3);
    let malformed = row(9, "BTC", "91").replace(r#""sz":"1""#, r#""sz":"bad""#);
    let logs = resync(
        &mut e,
        &mut s,
        3000.0,
        STATE,
        &format!(
            "[{},{malformed},{}]",
            row(8, "BTC", "81"),
            row(11, "@107", "1")
        ),
    );
    assert_eq!(e.store().order("8").unwrap().price, 81.0);
    assert_eq!(e.store().order("9").unwrap().price, 90.0);
    assert!(e.store().order("10").is_none());
    assert!(e.store().order("11").is_none());
    assert!(logs.contains(&"Hyperliquid skipped malformed order 9".to_owned()));
    assert!(
        !logs
            .iter()
            .any(|m| m.contains("bad") || m.contains("@107") || m.contains("11"))
    );
    resync(
        &mut e,
        &mut s,
        4000.0,
        STATE,
        &format!("[{}]", row(8, "BTC", "81")),
    );
    assert!(e.store().order("9").is_none());
    assert_eq!(e.store().orders().len(), 1);
}

#[test]
fn malformed_order_row_without_an_id_keeps_every_stored_order() {
    let (mut e, socket) = ready("disabled");
    let mut s = ResyncSocket {
        socket,
        now: 1000.0,
    };
    let row = |oid: u32, px: &str| {
        format!(
            r#"{{"coin":"BTC","oid":{oid},"side":"B","limitPx":"{px}","origSz":"1","sz":"1","orderType":"Limit","tif":"Gtc"}}"#
        )
    };
    resync(
        &mut e,
        &mut s,
        2000.0,
        STATE,
        &format!("[{},{}]", row(8, "80"), row(9, "90")),
    );
    let logs = resync(
        &mut e,
        &mut s,
        3000.0,
        STATE,
        &format!(r#"[{},{{"coin":"BTC","oid":"bad"}}]"#, row(8, "81")),
    );
    assert_eq!(e.store().order("8").unwrap().price, 81.0);
    assert_eq!(e.store().order("9").unwrap().price, 90.0);
    assert!(logs.contains(&"Hyperliquid skipped malformed order without an id".to_owned()));
}

#[test]
fn bad_position_rows_keep_their_stored_copies_until_a_valid_snapshot_omits_them() {
    let (mut e, socket) = ready("disabled");
    let mut s = ResyncSocket {
        socket,
        now: 1000.0,
    };
    let position = |coin: &str, szi: &str| {
        format!(
            r#"{{"position":{{"coin":"{coin}","szi":"{szi}","entryPx":"100","positionValue":"200","unrealizedPnl":"0","marginUsed":"20","leverage":{{"type":"cross","value":10}}}}}}"#
        )
    };
    let state = |time: u32, used: u32, rows: &[String]| {
        format!(
            r#"{{"assetPositions":[{}],"marginSummary":{{"accountValue":"1000","totalMarginUsed":"{used}"}},"time":{time}}}"#,
            rows.join(",")
        )
    };
    resync(
        &mut e,
        &mut s,
        2000.0,
        &state(2000, 20, &[position("BTC", "2"), position("ETH", "1")]),
        "[]",
    );
    assert_eq!(e.store().positions().len(), 2);
    let logs = resync(
        &mut e,
        &mut s,
        3000.0,
        &state(3000, 40, &[position("BTC", "3"), position("ETH", "bad")]),
        "[]",
    );
    let contracts = |e: &safe_cex_engine::Engine<_>, symbol: &str| {
        e.store()
            .positions()
            .iter()
            .find(|p| p.symbol == symbol)
            .map(|p| p.contracts)
    };
    assert_eq!(contracts(&e, "BTC-PERP"), Some(3.0));
    assert_eq!(contracts(&e, "ETH-PERP"), Some(1.0));
    assert!(logs.contains(&"Hyperliquid skipped malformed position eth".to_owned()));
    assert!(!logs.iter().any(|m| m.contains("bad")));
    assert_eq!(e.store().balance().used, 40.0);
    resync(
        &mut e,
        &mut s,
        4000.0,
        &state(4000, 20, &[position("BTC", "3")]),
        "[]",
    );
    assert_eq!(contracts(&e, "ETH-PERP"), None);
    assert_eq!(e.store().positions().len(), 1);
}

#[test]
fn owned_position_on_a_market_gone_from_the_catalogue_keeps_its_stored_copy() {
    use safe_cex_engine::model::{MarginType, Position, PositionMode, PositionSide};
    let (mut e, socket) = ready("disabled");
    let mut s = ResyncSocket {
        socket,
        now: 1000.0,
    };
    let mut positions = e.store().positions().to_vec();
    positions.push(Position {
        id: "SOL-PERP".into(),
        symbol: "SOL-PERP".into(),
        side: PositionSide::Long,
        entry_price: 20.0,
        notional: 60.0,
        leverage: 10.0,
        margin_type: Some(MarginType::Cross),
        position_mode: Some(PositionMode::Oneway),
        unrealized_pnl: 50.0,
        realized_pnl: None,
        contracts: 3.0,
        liquidation_price: 0.0,
        margin_size: Some(6.0),
        last_update: None,
    });
    e.store_mut().set_positions(positions);
    let state = r#"{"assetPositions":[{"position":{"coin":"SOL","szi":"3","entryPx":"20","positionValue":"90","unrealizedPnl":"10","marginUsed":"6","leverage":{"type":"cross","value":10}}},{"position":{"coin":"BTC","szi":"2","entryPx":"100","positionValue":"200","unrealizedPnl":"0","marginUsed":"20","leverage":{"type":"cross","value":10}}}],"marginSummary":{"accountValue":"1000","totalMarginUsed":"26"},"time":2000}"#;
    let logs = resync(&mut e, &mut s, 2000.0, state, "[]");
    assert!(logs.contains(&"Hyperliquid skipped position sol on an unknown market".to_owned()));
    assert_eq!(e.store().position("SOL-PERP").unwrap().contracts, 3.0);
    assert_eq!(e.store().position("BTC-PERP").unwrap().contracts, 2.0);
    // The wallet total nets the native row PnL (10), not the stale stored copy (50).
    assert_eq!(e.store().balance().total, 990.0);
    let logs = resync(
        &mut e,
        &mut s,
        3000.0,
        &state.replace("2000}", "3000}"),
        "[]",
    );
    assert!(!logs.iter().any(|m| m.contains("unknown market")));
    assert_eq!(e.store().position("SOL-PERP").unwrap().contracts, 3.0);
}

#[test]
fn malformed_position_row_holds_the_wallet_total() {
    let (mut e, socket) = ready("disabled");
    let mut s = ResyncSocket {
        socket,
        now: 1000.0,
    };
    assert_eq!(e.store().balance().total, 1000.0);
    resync(
        &mut e,
        &mut s,
        2000.0,
        r#"{"assetPositions":[{"position":{"coin":"BTC","szi":"2","entryPx":"100","positionValue":"260","unrealizedPnl":"30","marginUsed":"20","leverage":{"type":"cross","value":10}}},{"position":{"coin":"ETH","szi":"bad"}}],"marginSummary":{"accountValue":"1100","totalMarginUsed":"40"},"time":2000}"#,
        "[]",
    );
    assert_eq!(e.store().position("BTC-PERP").unwrap().notional, 260.0);
    assert_eq!(e.store().balance().total, 1000.0);
    assert_eq!(e.store().balance().used, 40.0);
}

#[test]
fn unified_spot_state_publishes_spot_holdings_as_coins() {
    let (mut e, s) = ready("unifiedAccount");
    message(
        &mut e,
        s,
        2000.0,
        r#"{"channel":"allMids","data":{"mids":{"BTC":"110"}}}"#,
    );
    message(
        &mut e,
        s,
        3000.0,
        r#"{"channel":"spotState","data":{"user":"x","spotState":{"balances":[{"coin":"USDC","token":0,"hold":"0","total":"900"},{"coin":"BTC","token":1,"hold":"0","total":"0.5"},{"coin":"PURR","token":2,"hold":"0","total":"0"}],"tokenToAvailableAfterMaintenance":[[0,"800"]]}}}"#,
    );
    let coins = e.store().coin_balances().expect("coins published");
    assert_eq!(coins.len(), 2, "zero holdings are left out");
    let usdc = coins.iter().find(|c| c.base == "USDC").unwrap();
    assert_eq!((usdc.amount, usdc.notional), (900.0, 900.0));
    let btc = coins.iter().find(|c| c.base == "BTC").unwrap();
    assert_eq!(btc.amount, 0.5);
    // Valued at mark (100), as collateral is, not the mid that allMids moved to 110.
    assert_eq!(btc.notional, 50.0);
}
