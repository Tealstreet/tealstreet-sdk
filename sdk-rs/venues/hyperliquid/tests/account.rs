//! Official clearinghouseState, frontendOpenOrders and WsUserFills payload shapes.
mod support;
use safe_cex_engine::test_support::prepared_now;
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
    let requests = prepared_now(&mut e)
        .into_iter()
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
    let requests = prepared_now(&mut e)
        .into_iter()
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
    let requests = prepared_now(&mut e)
        .into_iter()
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
    let timer = prepared_now(&mut e)
        .into_iter()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                (at_ms == 4350.0).then_some(timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(4350.0, Event::TimerFired { timer });
    let next = prepared_now(&mut e)
        .into_iter()
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
    assert!(prepared_now(&mut e).into_iter().any(|a|matches!(a,Action::Http(r) if kind(&r)=="clearinghouseState" && r.body.as_ref().unwrap().contains("xyz"))));
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
    let requests = prepared_now(&mut e)
        .into_iter()
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
    let requests = prepared_now(&mut e)
        .into_iter()
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
    let timer = prepared_now(&mut e)
        .into_iter()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                (at_ms == 3700.0).then_some(timer)
            } else {
                None
            }
        })
        .expect("invalidated reconnect snapshot must retry");
    e.handle(3700.0, Event::TimerFired { timer });
    let r = prepared_now(&mut e)
        .into_iter()
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
        let r = prepared_now(&mut e)
            .into_iter()
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
        let cancel = prepared_now(&mut e)
            .into_iter()
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
        let actions = prepared_now(&mut e);
        if finished_at < 22_100.0 {
            assert!(actions.into_iter().any(
                |a| matches!(a, Action::SetTimer { at_ms, .. } if at_ms == finished_at + 500.0)
            ));
        } else {
            assert!(
                e.store().order("10").is_none(),
                "expired stream publishes no rows"
            );
        }
    }
}

#[test]
fn periodic_account_policy_changes_preserve_inflight_reads_and_fence_stale_timers() {
    let (mut e, _) = ready_with_dex("disabled", true);
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
        let actions: Vec<_> = prepared_now(&mut e).into_iter().collect();
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
    prepared_now(&mut e).into_iter().for_each(drop);
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
    let requests = prepared_now(e)
        .into_iter()
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
    prepared_now(e)
        .into_iter()
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

#[test]
fn a_real_trigger_row_with_a_null_tif_stays_visible() {
    let (mut e, s) = ready("disabled");
    let row = r#"{"coin":"BTC","oid":77,"side":"A","limitPx":"1920.0","triggerPx":"1929.0","isTrigger":true,"isPositionTpsl":false,"reduceOnly":true,"orderType":"Stop Market","origSz":"0.005","sz":"0.005","tif":null,"cloid":null,"timestamp":1793547192000}"#;
    let (_, _) = reconnect(&mut e, s, 40000.0);
    let requests = prepared_now(&mut e)
        .into_iter()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let orders = format!("[{row}]");
    for r in requests {
        let body = if kind(&r) == "frontendOpenOrders" {
            orders.as_str()
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
    let order = e.store().order("77").expect("the stop shows");
    assert_eq!(order.kind, safe_cex_engine::model::OrderType::StopLoss);
    assert_eq!(order.amount, 0.005);
}

#[test]
fn a_triggered_stop_does_not_show_price_zero() {
    let (mut e, s) = ready("disabled");
    message(
        &mut e,
        s,
        2000.0,
        r#"{"channel":"orderUpdates","data":[{"order":{"coin":"BTC","oid":78,"side":"A","limitPx":"88.5","triggerPx":"0.0","orderType":"Stop Market","origSz":"1","sz":"1","tif":"Gtc"},"status":"triggered"}]}"#,
    );
    assert_eq!(e.store().order("78").unwrap().price, 88.5);
}

#[test]
fn a_fill_arms_an_order_read_because_partial_fills_send_no_update() {
    let (mut e, s) = ready("disabled");
    e.drain_actions().for_each(drop);
    message(
        &mut e,
        s,
        2000.0,
        r#"{"channel":"userFills","data":{"user":"0x14791697260e4c9a71f18484c9f997b308e59325","fills":[{"coin":"BTC","px":"90","sz":"0.1","side":"B","time":2000,"startPosition":"0","dir":"Open Long","closedPnl":"0","hash":"0x0","oid":9,"crossed":true,"fee":"0","tid":5,"feeToken":"USDC"}]}}"#,
    );
    let armed = e
        .drain_actions()
        .any(|a| matches!(a, Action::SetTimer { at_ms, .. } if at_ms < 3001.0));
    assert!(armed);
}

// Live, ZRO: 160 closing in five partial fills; the feed froze at 49.3 and later frames omit the coin.
#[test]
fn a_state_frame_that_omits_a_coin_closes_its_position() {
    let (mut e, s) = ready("disabled");
    let frame = |time: u64, rows: &str| {
        format!(
            r#"{{"channel":"clearinghouseState","data":{{"user":"x","dex":"","clearinghouseState":{{"assetPositions":[{rows}],"marginSummary":{{"accountValue":"1000","totalMarginUsed":"0","totalNtlPos":"0","totalRawUsd":"1000"}},"withdrawable":"1000","time":{time}}}}}}}"#
        )
    };
    let row = |coin: &str, szi: &str| {
        format!(
            r#"{{"position":{{"coin":"{coin}","szi":"{szi}","entryPx":"100","positionValue":"100","unrealizedPnl":"0","liquidationPx":null,"marginUsed":"1","leverage":{{"type":"cross","value":10}}}}}}"#
        )
    };
    let both = format!("{},{}", row("BTC", "49.3"), row("ETH", "2"));
    message(&mut e, s, 3000.0, &frame(3000, &both));
    assert!(
        e.store()
            .positions()
            .iter()
            .any(|p| p.symbol == "BTC-PERP" && p.contracts == 49.3)
    );
    message(&mut e, s, 4000.0, &frame(4000, &row("ETH", "2")));
    let symbols: Vec<_> = e
        .store()
        .positions()
        .iter()
        .map(|p| p.symbol.as_str())
        .collect();
    assert_eq!(symbols, ["ETH-PERP"]);
}

#[test]
fn failed_hip3_orders_retry_after_peer_scope_loaded_and_honor_retry_after() {
    let (mut e, socket) = ready_with_dex("disabled", true);
    assert!(e.store().loaded().orders);
    let (_socket, _) = reconnect(&mut e, socket, 2000.0);
    let mut requests = prepared_now(&mut e)
        .into_iter()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let mut failed = false;
    let mut retained = Vec::new();
    for _ in 0..20 {
        for request in requests {
            let body: serde_json::Value =
                serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
            if kind(&request) == "frontendOpenOrders" && body["dex"] == "xyz" {
                failed = true;
                e.handle(
                    3000.0,
                    Event::HttpResponse {
                        request: request.id,
                        response: safe_cex_engine::Response {
                            status: 429,
                            headers: "Retry-After: 12\r\n",
                            body: "Too many requests",
                        },
                    },
                );
            } else if kind(&request) == "frontendOpenOrders" {
                reply(&mut e, &request, 200, "[]");
            } else if kind(&request) == "clearinghouseState" {
                reply(&mut e, &request, 200, STATE);
            }
        }
        requests = Vec::new();
        for action in prepared_now(&mut e) {
            if let Action::Http(request) = action {
                requests.push(request);
            } else {
                retained.push(action);
            }
        }
        if requests.is_empty() {
            break;
        }
    }
    assert!(failed, "the failed read must belong to the HIP-3 scope");
    let actions = retained;
    let timer = actions
        .into_iter()
        .find_map(|action| {
            if let Action::SetTimer { timer, at_ms } = action {
                (at_ms == 15_000.0).then_some(timer)
            } else {
                None
            }
        })
        .expect("an already-loaded peer scope cannot suppress the 12-second retry");
    e.handle(15_000.0, Event::TimerFired { timer });
    let retried = prepared_now(&mut e)
        .into_iter()
        .filter_map(|action| {
            if let Action::Http(request) = action {
                Some(request)
            } else {
                None
            }
        })
        .find(|request| kind(request) == "frontendOpenOrders")
        .expect("the failed orders scope must be retried");
    let body: serde_json::Value = serde_json::from_str(retried.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["dex"], "xyz");
    reply(&mut e, &retried, 200, "[]");
    assert!(e.store().loaded().orders);
}

#[test]
fn public_startup_diagnostics_never_report_private_order_reads() {
    use safe_cex_engine::Venue;
    use safe_cex_venue_hyperliquid::{Hyperliquid, Options};
    let venue = Hyperliquid::new(Options {
        public_data_only: true,
        ..Options::default()
    });
    let (ready, _) = ready("disabled");
    let mut store = safe_cex_engine::Store::default();
    store.set_markets(ready.store().markets().to_vec());
    assert!(!store.loaded().orders);
    assert!(
        venue
            .startup_diagnostics(&store)
            .contains(&("stage", "tickers".into()))
    );
}

#[test]
fn startup_watchdog_exposes_stage_original_deadline_and_rate_limit_count() {
    let (mut e, socket) = ready("disabled");
    e.set_account_startup(true);
    let (_socket, _) = reconnect(&mut e, socket, 2000.0);
    for action in prepared_now(&mut e) {
        if let Action::Http(request) = action {
            if kind(&request) == "frontendOpenOrders" {
                reply(&mut e, &request, 429, "Too many requests");
            } else if kind(&request) == "clearinghouseState" {
                reply(&mut e, &request, 200, STATE);
            }
        }
    }
    e.store_mut().reset_account();
    e.store_mut().set_balance(Default::default());
    e.store_mut().set_positions(vec![]);
    prepared_now(&mut e);
    e.handle(61_000.0, Event::PollScaleChanged { scale: 1.0 });
    let notice = e
        .drain_actions()
        .find_map(|action| match action {
            Action::Notice(safe_cex_engine::Notice::Event(event))
                if event.code == "startup.incomplete" =>
            {
                Some(event)
            }
            _ => None,
        })
        .expect("the startup watchdog must report the unloaded orders section");
    assert!(
        notice
            .fields
            .contains(&("stage", "frontendOpenOrders".into()))
    );
    assert!(notice.fields.contains(&("elapsedMs", 60_000.0.into())));
    assert!(notice.fields.contains(&("deadlineMs", 60_000.0.into())));
    assert!(notice.fields.contains(&("readTimeoutMs", 20_000.0.into())));
    assert!(notice.fields.contains(&("http429Count", 1.0.into())));
    e.handle(361_000.0, Event::PollScaleChanged { scale: 1.0 });
    let repeated = e
        .drain_actions()
        .find_map(|action| match action {
            Action::Notice(safe_cex_engine::Notice::Event(event))
                if event.code == "startup.incomplete" =>
            {
                Some(event)
            }
            _ => None,
        })
        .expect("a repeated notice must retain the original startup deadline");
    assert!(repeated.fields.contains(&("elapsedMs", 360_000.0.into())));
    assert!(repeated.fields.contains(&("deadlineMs", 60_000.0.into())));
}

fn subscription_receipts(frame: &[u8]) -> Vec<(String, u32, f64)> {
    use safe_cex_engine::wire::{section, sections};
    let stats = sections(frame)
        .find(|(kind, _)| *kind == section::MESSAGE_HANDLER_STATS)
        .unwrap()
        .1;
    let strings = sections(frame)
        .find(|(kind, _)| *kind == section::STRINGS)
        .unwrap()
        .1;
    let count = u32::from_le_bytes(strings[..4].try_into().unwrap()) as usize;
    let text = |index: usize| {
        let start =
            u32::from_le_bytes(strings[4 + index * 4..8 + index * 4].try_into().unwrap()) as usize;
        let end =
            u32::from_le_bytes(strings[8 + index * 4..12 + index * 4].try_into().unwrap()) as usize;
        let base = 4 + (count + 1) * 4;
        std::str::from_utf8(&strings[base + start..base + end])
            .unwrap()
            .to_owned()
    };
    let rows: Vec<_> = stats[8..]
        .as_chunks::<24>()
        .0
        .iter()
        .map(|row| {
            (
                text(u32::from_le_bytes(row[4..8].try_into().unwrap()) as usize),
                u32::from_le_bytes(row[8..12].try_into().unwrap()),
                f64::from_le_bytes(row[16..24].try_into().unwrap()),
            )
        })
        .collect();
    assert_eq!(
        rows.len(),
        u32::from_le_bytes(stats[..4].try_into().unwrap()) as usize
    );
    rows
}

#[test]
fn subscription_receipt_is_scoped_to_the_decoded_clearinghouse_dex() {
    let (mut e, socket) = ready_with_dex("disabled", true);
    let receipt = |rows: &[(String, u32, f64)], dex: &str| {
        let row = rows
            .iter()
            .find(|(topic, _, _)| {
                let (_, payload) = topic.split_once('.').unwrap();
                let value: serde_json::Value = serde_json::from_str(payload).unwrap();
                value["subscription"]["type"] == "clearinghouseState"
                    && value["subscription"]["dex"].as_str().unwrap_or("") == dex
            })
            .unwrap();
        (row.1, row.2)
    };
    message(
        &mut e,
        socket,
        2000.,
        r#"{"channel":"clearinghouseState","data":{"dex":"xyz","clearinghouseState":{"assetPositions":[],"time":2000}}}"#,
    );
    let rows = subscription_receipts(e.snapshot_frame(2001.));
    assert_eq!(receipt(&rows, "xyz"), (3, 1.));
    let untouched = receipt(&rows, "");
    assert_eq!(untouched.0, 1);
    assert!(
        untouched.1.is_nan(),
        "a sibling dex has not delivered a frame"
    );

    message(
        &mut e,
        socket,
        3000.,
        r#"{"channel":"clearinghouseState","data":{"dex":"","clearinghouseState":{"assetPositions":[],"time":3000}}}"#,
    );
    let rows = subscription_receipts(e.snapshot_frame(3001.));
    assert_eq!(receipt(&rows, ""), (3, 1.));
    assert_eq!(receipt(&rows, "xyz"), (3, 1001.));

    message(
        &mut e,
        socket,
        4000.,
        r#"{"channel":"clearinghouseState","data":{"dex":"unknown","clearinghouseState":{"assetPositions":[],"time":4000}}}"#,
    );
    let rows = subscription_receipts(e.snapshot_frame(4001.));
    assert_eq!(receipt(&rows, ""), (3, 1001.));
    assert_eq!(receipt(&rows, "xyz"), (3, 2001.));
}
