//! Account shapes and semantics from the current official TxFlow API docs.
/// Native cadence of the venue's recurring account poll; policy scales it.
const POLL_MS: f64 = 5_000.0;

#[path = "support/account.rs"]
mod support;
use safe_cex_engine::{Event, SocketId};
use support::*;
#[test]
fn main_wallet_reads_are_unsigned_and_settings_do_not_create_positions() {
    let (mut e, _, reads, _) = bootstrap();
    for r in &reads {
        assert_eq!(r.url, "https://rpc.txflow-testnet.xyz/info");
        assert!(
            r.headers
                .iter()
                .any(|(k, v)| *k == "x-api-version" && v == "2026-08-16")
        );
        assert!(
            !r.headers
                .iter()
                .any(|(k, _)| k.to_lowercase().contains("auth"))
        );
        let b = serde_json::from_str::<serde_json::Value>(r.body.as_ref().unwrap()).unwrap();
        assert_eq!(b["user"], WALLET);
    }
    answer(&mut e, reads);
    assert_eq!(e.store().positions().len(), 1);
    assert_eq!(e.store().positions()[0].contracts, 2.0);
    assert_eq!(e.store().balance().total, 1000.0);
    assert_eq!(e.store().balance().free, 980.0);
    assert_eq!(e.store().balance().upnl, 5.0);
    assert_eq!(e.store().setting("BTC-USDC").unwrap().leverage, 10.0);
}
#[test]
fn successful_empty_state_clears_positions_and_failure_preserves_last_known() {
    let (mut e, s, reads, t) = bootstrap();
    answer(&mut e, reads);
    message(
        &mut e,
        s,
        NOW + 1000.0,
        &format!(
            r#"{{"channel":"clearinghouseState","data":{}}}"#,
            serde_json::from_str::<serde_json::Value>(EMPTY_STATE).unwrap()["data"]
        ),
    );
    assert!(e.store().positions().is_empty());
    assert_eq!(e.store().setting("BTC-USDC").unwrap().leverage, 10.0);
    e.handle(NOW + 5025.0, Event::TimerFired { timer: t });
    let rs = requests(&mut e);
    for r in rs {
        reply_at(&mut e, &r, NOW + 5025.0, 503, "offline");
    }
    assert!(e.store().positions().is_empty());
    assert_eq!(e.store().balance().total, 1000.0);
}
#[test]
fn newer_push_fences_inflight_state_and_order_snapshots() {
    let (mut e, s, reads, _) = bootstrap();
    let data = serde_json::from_str::<serde_json::Value>(STATE).unwrap()["data"]
        .to_string()
        .replace("\"2\"", "\"3\"");
    message(
        &mut e,
        s,
        NOW + 100.0,
        &format!(r#"{{"channel":"clearinghouseState","data":{data}}}"#),
    );
    message(
        &mut e,
        s,
        NOW + 101.0,
        &format!(
            r#"{{"channel":"orderUpdates","data":[{{"order":{ORDER},"status":"Filled","statusTimestamp":1700000000101}}]}}"#
        ),
    );
    answer(&mut e, reads);
    assert_eq!(e.store().positions()[0].contracts, 3.0);
    assert!(e.store().orders().is_empty());
    assert_eq!(e.take_frame(NOW + 102.0).map_or(0, fill_count), 0);
}
#[test]
fn malformed_or_semantically_failed_responses_do_not_commit_partial_rows() {
    let (mut e, _, reads, t) = bootstrap();
    answer(&mut e, reads);
    e.handle(NOW + 5025.0, Event::TimerFired { timer: t });
    for r in requests(&mut e) {
        let body = match kind(&r).as_str() {
            "clearinghouseState" => STATE.replace("\"szi\":\"2\"", "\"szi\":\"NaN\""),
            "frontendOpenOrders" => format!(r#"{{"success":false,"code":1,"data":[{ORDER}]}}"#),
            _ => r#"{"success":false,"code":1,"message":"failed"}"#.into(),
        };
        reply(&mut e, &r, &body);
    }
    assert_eq!(e.store().positions()[0].contracts, 2.0);
    assert_eq!(e.store().orders().len(), 1);
    assert_eq!(e.store().balance().free, 980.0);
}
#[test]
fn disconnect_fences_old_http_and_late_old_socket_frames() {
    let (mut e, s, reads, _) = bootstrap();
    e.handle(
        NOW + 100.0,
        Event::WsClosed {
            socket: s,
            code: 1006,
        },
    );
    answer(&mut e, reads);
    assert!(e.store().positions().is_empty());
    message(
        &mut e,
        s,
        NOW + 101.0,
        &format!(
            r#"{{"channel":"orderUpdates","data":[{{"order":{ORDER},"status":"Open","statusTimestamp":1700000000101}}]}}"#
        ),
    );
    assert!(e.store().orders().is_empty());
}
#[test]
fn fills_are_native_executions_once_and_api_socket_history_is_ignored() {
    let (mut e, s, reads, t) = bootstrap();
    let f = fill(10, 101.0, NOW - 1000.0);
    for r in reads {
        if kind(&r) == "userFills" {
            reply(
                &mut e,
                &r,
                &format!(r#"{{"success":true,"code":0,"data":{{"data":[{f}],"hasMore":false}}}}"#),
            );
        } else {
            answer(&mut e, vec![r]);
        }
    }
    assert_eq!(e.take_frame(NOW).map_or(0, fill_count), 0);
    e.handle(NOW + 5025.0, Event::TimerFired { timer: t });
    let reads = requests(&mut e);
    let f = fill(11, 102.0, NOW + 1000.0);
    for r in reads {
        if kind(&r) == "userFills" {
            assert!(r.body.as_ref().unwrap().contains("startTimeMs"));
            reply(
                &mut e,
                &r,
                &format!(
                    r#"{{"success":true,"code":0,"data":{{"data":[{f},{f}],"hasMore":false}}}}"#
                ),
            );
        } else {
            answer(&mut e, vec![r]);
        }
    }
    assert_eq!(e.take_frame(NOW + 5025.0).map_or(0, fill_count), 1);
    message(
        &mut e,
        s,
        NOW + 6000.0,
        &format!(
            r#"{{"channel":"orderUpdates","data":[{{"order":{ORDER},"status":"PartialFilled","statusTimestamp":1700000006000}}]}}"#
        ),
    );
    for r in requests(&mut e) {
        if kind(&r) == "userFills" {
            reply(
                &mut e,
                &r,
                &format!(r#"{{"success":true,"code":0,"data":{{"data":[{f}],"hasMore":false}}}}"#),
            );
        } else {
            answer(&mut e, vec![r]);
        }
    }
    assert_eq!(e.take_frame(NOW + 6000.0).map_or(0, fill_count), 0);
}
#[test]
fn account_settings_push_without_positions_preserves_no_exposure() {
    let (mut e, s, _, _) = bootstrap();
    message(
        &mut e,
        s,
        NOW + 100.0,
        &format!(
            r#"{{"channel":"userAssetData","data":{{"user":"{WALLET}","coins":[{{"coin":"1","leverage":{{"type":"Isolated","value":7}}}}]}}}}"#
        ),
    );
    assert!(e.store().positions().is_empty());
    assert_eq!(e.store().setting("BTC-USDC").unwrap().leverage, 7.0);
}
#[test]
fn stale_order_events_cannot_reopen_terminal_orders() {
    let (mut e, s, reads, _) = bootstrap();
    answer(&mut e, reads);
    for (status, t) in [
        ("Filled", NOW + 2000.0),
        ("Open", NOW + 1000.0),
        ("Open", NOW + 3000.0),
    ] {
        message(
            &mut e,
            s,
            t,
            &format!(
                r#"{{"channel":"orderUpdates","data":[{{"order":{ORDER},"status":"{status}","statusTimestamp":{t}}}]}}"#
            ),
        );
    }
    assert!(e.store().orders().is_empty());
}
#[test]
fn api_fill_snapshots_never_become_execution_events() {
    let (mut e, s, reads, _) = bootstrap();
    answer(&mut e, reads);
    let _ = e.take_frame(NOW);
    let f = fill(50, 100.0, NOW + 2000.0);
    let api = SocketId(s.0 + 1);
    message(
        &mut e,
        api,
        NOW + 3000.0,
        &format!(r#"{{"channel":"userFills","data":{{"isSnapshot":true,"fills":[{f}]}}}}"#),
    );
    assert_eq!(e.take_frame(NOW + 3000.0).map_or(0, fill_count), 0);
}
#[test]
fn first_fill_response_keeps_executions_created_while_startup_read_was_inflight() {
    let (mut e, _, reads, _) = bootstrap();
    for r in reads {
        if kind(&r) == "userFills" {
            assert!(r.body.as_ref().unwrap().contains("startTimeMs"));
            let f = fill(99, 123.0, NOW + 50.0);
            reply(
                &mut e,
                &r,
                &format!(r#"{{"success":true,"code":0,"data":{{"data":[{f}],"hasMore":false}}}}"#),
            );
        } else {
            answer(&mut e, vec![r]);
        }
    }
    assert_eq!(e.take_frame(NOW + 100.0).map_or(0, fill_count), 1);
}
#[test]
fn flat_hedge_mode_is_confirmed_from_native_settings_without_placeholder_positions() {
    let (mut e, _, reads, _) = bootstrap();
    for r in reads {
        match kind(&r).as_str() {
            "clearinghouseState" => {
                assert!(
                    r.body
                        .as_ref()
                        .unwrap()
                        .contains("\"accountType\":\"perp\"")
                );
                reply(&mut e, &r, EMPTY_STATE);
            }
            "activeAssetData" => reply(
                &mut e,
                &r,
                &format!(
                    r#"{{"success":true,"code":0,"data":{{"coin":"1","user":"{WALLET}","leverage":{{"mode":"hedge","type":"Isolated","long_value":5,"short_value":8}}}}}}"#
                ),
            ),
            _ => answer(&mut e, vec![r]),
        }
    }
    assert!(e.store().positions().is_empty());
    assert!(e.store().hedged());
    let setting = e.store().setting("BTC-USDC").unwrap();
    assert_eq!(setting.buy_leverage, Some(5.0));
    assert_eq!(setting.sell_leverage, Some(8.0));
}
#[test]
fn account_subject_compares_wallet_hex_without_case_sensitivity() {
    let wallet = format!("0x{}", "AB".repeat(20));
    let (mut e, socket, _, _) = bootstrap_with_wallet(&wallet);
    let data = serde_json::from_str::<serde_json::Value>(STATE).unwrap()["data"].clone();
    let mut data = data.as_object().unwrap().clone();
    data.insert(
        "user".into(),
        serde_json::Value::String(wallet.to_lowercase()),
    );
    message(
        &mut e,
        socket,
        NOW + 100.0,
        &format!(
            r#"{{"channel":"clearinghouseState","data":{}}}"#,
            serde_json::Value::Object(data)
        ),
    );
    assert_eq!(e.store().positions()[0].contracts, 2.0);
}

fn hedge_state(legs: &[(f64, f64)], time: f64) -> String {
    let mut envelope: serde_json::Value = serde_json::from_str(STATE).unwrap();
    let template = envelope["data"]["assetPositions"][0].clone();
    envelope["data"]["assetPositions"] = legs
        .iter()
        .map(|(size, leverage)| {
            let mut row = template.clone();
            row["type"] = "hedge".into();
            row["position"]["szi"] = size.to_string().into();
            row["position"]["leverage"] = serde_json::json!({"type":"Isolated","value":leverage});
            row
        })
        .collect();
    envelope["data"]["time"] = time.into();
    envelope.to_string()
}
fn hedge_settings(long: f64, short: f64) -> String {
    serde_json::json!({"coin":"1","user":WALLET,"leverage":{"mode":"hedge","type":"Isolated","long_value":long,"short_value":short}}).to_string()
}
fn push_hedge_settings(
    e: &mut safe_cex_engine::Engine<safe_cex_venue_txflow::TxFlow>,
    socket: safe_cex_engine::SocketId,
    now: f64,
    long: f64,
    short: f64,
) {
    message(
        e,
        socket,
        now,
        &format!(
            r#"{{"channel":"activeAssetData","data":{}}}"#,
            hedge_settings(long, short)
        ),
    );
}
fn push_hedge_state(
    e: &mut safe_cex_engine::Engine<safe_cex_venue_txflow::TxFlow>,
    socket: safe_cex_engine::SocketId,
    now: f64,
    legs: &[(f64, f64)],
) {
    let state: serde_json::Value = serde_json::from_str(&hedge_state(legs, now)).unwrap();
    message(
        e,
        socket,
        now,
        &serde_json::json!({"channel":"clearinghouseState","data":state["data"]}).to_string(),
    );
}
fn assert_hedge_rails(
    e: &safe_cex_engine::Engine<safe_cex_venue_txflow::TxFlow>,
    long: f64,
    short: f64,
) {
    let setting = e.store().setting("BTC-USDC").unwrap();
    assert_eq!(setting.buy_leverage, Some(long));
    assert_eq!(setting.sell_leverage, Some(short));
    assert!(e.store().hedged());
}
#[test]
fn hedge_position_row_order_never_overwrites_configured_directional_leverage() {
    for legs in [[(2.0, 5.0), (-3.0, 8.0)], [(-3.0, 8.0), (2.0, 5.0)]] {
        let (mut e, s, reads, _) = bootstrap();
        answer(&mut e, reads);
        push_hedge_settings(&mut e, s, NOW + 100.0, 5.0, 8.0);
        push_hedge_state(&mut e, s, NOW + 200.0, &legs);
        assert_hedge_rails(&e, 5.0, 8.0);
        for (size, leverage) in legs {
            let side = if size > 0.0 {
                safe_cex_engine::model::PositionSide::Long
            } else {
                safe_cex_engine::model::PositionSide::Short
            };
            let position = e
                .store()
                .positions()
                .iter()
                .find(|p| p.side == side)
                .unwrap();
            assert_eq!(position.leverage, leverage);
            assert_eq!(
                position.margin_type,
                Some(safe_cex_engine::model::MarginType::Isolated)
            );
        }
    }
}
#[test]
fn a_single_hedge_leg_preserves_the_absent_legs_configured_leverage() {
    for leg in [(2.0, 5.0), (-3.0, 8.0)] {
        let (mut e, s, reads, _) = bootstrap();
        answer(&mut e, reads);
        push_hedge_settings(&mut e, s, NOW + 100.0, 5.0, 8.0);
        push_hedge_state(&mut e, s, NOW + 200.0, &[leg]);
        assert_hedge_rails(&e, 5.0, 8.0);
        assert_eq!(e.store().positions().len(), 1);
        assert_eq!(e.store().positions()[0].leverage, leg.1);
    }
}
#[test]
fn rest_hedge_reconciliation_uses_per_leg_effective_leverage_without_inventing_settings() {
    for legs in [
        vec![(2.0, 5.0), (-3.0, 8.0)],
        vec![(-3.0, 8.0), (2.0, 5.0)],
        vec![(2.0, 5.0)],
        vec![(-3.0, 8.0)],
    ] {
        let (mut e, _, reads, _) = bootstrap();
        let mut state = None;
        for r in reads {
            match kind(&r).as_str() {
                "clearinghouseState" => state = Some(r),
                "activeAssetData" => reply(
                    &mut e,
                    &r,
                    &format!(
                        r#"{{"success":true,"code":0,"data":{}}}"#,
                        hedge_settings(5.0, 8.0)
                    ),
                ),
                _ => answer(&mut e, vec![r]),
            }
        }
        reply(&mut e, &state.unwrap(), &hedge_state(&legs, NOW + 100.0));
        assert_hedge_rails(&e, 5.0, 8.0);
        assert_eq!(e.store().positions().len(), legs.len());
        for (size, leverage) in legs {
            assert!(
                e.store()
                    .positions()
                    .iter()
                    .any(|p| p.contracts == size.abs() && p.leverage == leverage)
            );
        }
    }
}
#[test]
fn settings_remain_authoritative_after_pushes_and_stale_or_malformed_state_reads() {
    let (mut e, s, reads, timer) = bootstrap();
    answer(&mut e, reads);
    push_hedge_settings(&mut e, s, NOW + 100.0, 5.0, 8.0);
    push_hedge_state(&mut e, s, NOW + 200.0, &[(2.0, 5.0), (-3.0, 8.0)]);
    e.drain_actions().for_each(drop);
    e.handle(NOW + 5025.0, Event::TimerFired { timer });
    let held = requests(&mut e)
        .into_iter()
        .find(|r| kind(r) == "clearinghouseState")
        .unwrap();
    message(&mut e,s,NOW+5100.0,&serde_json::json!({"channel":"userAssetData","data":{"user":WALLET,"coins":[serde_json::from_str::<serde_json::Value>(&hedge_settings(9.0,12.0)).unwrap()]}}).to_string());
    // An older state may refresh exposure but cannot roll back the account mode
    // established by the newer setting response, or either configured rail.
    let mut older_state: serde_json::Value = serde_json::from_str(STATE).unwrap();
    older_state["data"]["time"] = serde_json::json!(NOW + 5050.0);
    reply_at(&mut e, &held, NOW + 5200.0, 200, &older_state.to_string());
    assert_hedge_rails(&e, 9.0, 12.0);
    let before = e.store().positions().to_vec();
    let malformed = hedge_state(&[(4.0, 5.0), (-5.0, 0.0)], NOW + 5300.0);
    let malformed: serde_json::Value = serde_json::from_str(&malformed).unwrap();
    message(
        &mut e,
        s,
        NOW + 5300.0,
        &serde_json::json!({"channel":"clearinghouseState","data":malformed["data"]}).to_string(),
    );
    assert_eq!(e.store().positions(), before.as_slice());
    assert_hedge_rails(&e, 9.0, 12.0);
    push_hedge_state(&mut e, s, NOW + 5400.0, &[(-3.0, 8.0)]);
    assert_hedge_rails(&e, 9.0, 12.0);
    assert_eq!(e.store().positions()[0].leverage, 8.0);
}
#[test]
fn positions_cannot_manufacture_configured_settings_when_asset_data_is_unavailable() {
    let (mut e, _, reads, _) = bootstrap();
    for r in reads {
        match kind(&r).as_str() {
            "clearinghouseState" => reply(
                &mut e,
                &r,
                &hedge_state(&[(2.0, 5.0), (-3.0, 8.0)], NOW + 100.0),
            ),
            "activeAssetData" => reply_at(&mut e, &r, NOW + 100.0, 503, "settings unavailable"),
            _ => answer(&mut e, vec![r]),
        }
    }
    assert_eq!(e.store().positions().len(), 2);
    assert!(e.store().setting("BTC-USDC").is_none());
}
#[test]
fn an_accepted_setting_write_fences_held_state_without_losing_configured_hedge_rails() {
    let (mut e, s, reads, timer) = bootstrap();
    answer(&mut e, reads);
    push_hedge_settings(&mut e, s, NOW + 100.0, 5.0, 8.0);
    push_hedge_state(&mut e, s, NOW + 200.0, &[(2.0, 5.0), (-3.0, 8.0)]);
    e.drain_actions().for_each(drop);
    e.handle(NOW + 5025.0, Event::TimerFired { timer });
    let held = requests(&mut e)
        .into_iter()
        .find(|r| kind(r) == "clearinghouseState")
        .unwrap();
    e.handle(
        NOW + 5100.0,
        Event::Command(safe_cex_engine::Command::SetMarginType {
            command: 991,
            symbol: "BTC-USDC".into(),
            isolated: false,
        }),
    );
    let write = e
        .drain_actions()
        .find_map(|a| match a {
            safe_cex_engine::Action::Http(r) if r.url.ends_with("/exchange") => Some(r),
            _ => None,
        })
        .unwrap();
    reply_at(
        &mut e,
        &write,
        NOW + 5200.0,
        200,
        r#"{"status":"ok","confirm":"executed","code":0,"data":{"type":"UpdateMarginMode","data":{"statuses":["success"]}}}"#,
    );
    reply_at(
        &mut e,
        &held,
        NOW + 5300.0,
        200,
        &hedge_state(&[(9.0, 99.0)], NOW + 5050.0),
    );
    assert_eq!(e.store().positions().len(), 2);
    assert_hedge_rails(&e, 5.0, 8.0);
    assert_eq!(
        e.store().setting("BTC-USDC").unwrap().margin_type,
        Some(safe_cex_engine::model::MarginType::Cross)
    );
}
#[test]
fn a_successful_rest_setting_fences_older_state_mode_before_publishing_hedge_rails() {
    let (mut e, _, reads, _) = bootstrap();
    let mut held = None;
    for r in reads {
        match kind(&r).as_str() {
            "clearinghouseState" => held = Some(r),
            "activeAssetData" => reply(
                &mut e,
                &r,
                &format!(
                    r#"{{"success":true,"code":0,"data":{}}}"#,
                    hedge_settings(5.0, 8.0)
                ),
            ),
            _ => answer(&mut e, vec![r]),
        }
    }
    assert_hedge_rails(&e, 5.0, 8.0);
    reply_at(&mut e, &held.unwrap(), NOW + 100.0, 200, STATE);
    assert_eq!(e.store().positions().len(), 1);
    assert_hedge_rails(&e, 5.0, 8.0);
    e.drain_actions().for_each(drop);
    e.handle(
        NOW + 200.0,
        Event::Command(safe_cex_engine::Command::SetLeverage {
            command: 992,
            symbol: "BTC-USDC".into(),
            leverage: 10.0,
            side: None,
        }),
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a,safe_cex_engine::Action::Http(r)if r.url.ends_with("/exchange")))
    );
    assert!(actions.iter().any(|a|matches!(a,safe_cex_engine::Action::Notice(safe_cex_engine::Notice::CommandFailed {command:992,error})if error.message.contains("hedge leverage"))));
}

#[test]
fn periodic_account_policy_changes_preserve_inflight_reads_and_fence_stale_timers() {
    let (mut e, _, reads, _) = bootstrap();
    answer(&mut e, reads);
    let now = NOW + 1000.;
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
            safe_cex_engine::Action::ClearTimer { timer } => Some(timer),
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
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, safe_cex_engine::Action::Http(_)))
        );
        let timers: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                safe_cex_engine::Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                _ => None,
            })
            .collect();
        assert_eq!(timers.len(), 1);
        assert_eq!(timers[0].1, now + interval.max(0.0));
        if let Some((timer, _)) = current.replace(timers[0]) {
            obsolete.push(timer);
        }
    }
    for timer in obsolete {
        e.handle(now, Event::TimerFired { timer });
        assert!(
            !e.drain_actions()
                .any(|a| matches!(a, safe_cex_engine::Action::Http(_)))
        );
    }
    let (timer, due) = current.unwrap();
    e.handle(due, Event::TimerFired { timer });
    let mut pending: Vec<_> = e
        .drain_actions()
        .filter_map(|a| match a {
            safe_cex_engine::Action::Http(r) => Some(r),
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
            let body = STATE;
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
                safe_cex_engine::Action::Http(r) => Some(r),
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
            safe_cex_engine::Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .collect();
    assert_eq!(
        resumed.len(),
        1,
        "completion retains one paused registration"
    );
    assert_eq!(resumed[0].1, due + 2. + 10_000_f64.max(0.0));
}

#[test]
fn account_execution_window_opens_after_the_paced_ticker_seed() {
    let mut e = safe_cex_engine::test_support::seeded_engine(safe_cex_venue_txflow::TxFlow::new(
        safe_cex_venue_txflow::Options {
            testnet: true,
            application_id: Some(WALLET.into()),
            secret: Some(format!("0x{:064x}", 1)),
            ..Default::default()
        },
    ));
    e.start(NOW);
    let r = requests(&mut e).pop().unwrap();
    reply(&mut e, &r, META);
    let seed = requests(&mut e);
    assert_eq!(
        seed.iter().map(kind).collect::<Vec<_>>(),
        ["activeAssetCtx"],
        "account reads must wait for the ticker seed"
    );
    reply(
        &mut e,
        &seed[0],
        r#"{"success":true,"code":0,"data":{"instrumentId":1,"lastPrice":"100","markPx":"100","oraclePx":"100"}}"#,
    );
    let timer = e
        .drain_actions()
        .find_map(|a| match a {
            safe_cex_engine::Action::SetTimer { timer, at_ms } if at_ms == NOW + 25.0 => {
                Some(timer)
            }
            _ => None,
        })
        .unwrap();
    e.handle(NOW + 25.0, Event::TimerFired { timer });
    let fills = requests(&mut e)
        .into_iter()
        .find(|r| kind(r) == "userFills")
        .unwrap();
    let body = serde_json::from_str::<serde_json::Value>(fills.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["startTimeMs"], (NOW + 25.0) as u64);
}

#[test]
fn a_failing_ticker_seed_read_does_not_block_account_start() {
    let mut e = safe_cex_engine::test_support::seeded_engine(safe_cex_venue_txflow::TxFlow::new(
        safe_cex_venue_txflow::Options {
            testnet: true,
            application_id: Some(WALLET.into()),
            secret: Some(format!("0x{:064x}", 1)),
            ..Default::default()
        },
    ));
    e.start(NOW);
    let r = requests(&mut e).pop().unwrap();
    reply(&mut e, &r, META);
    let seed = requests(&mut e);
    reply(
        &mut e,
        &seed[0],
        r#"{"success":false,"code":1,"msg":"halted"}"#,
    );
    let timer = e
        .drain_actions()
        .find_map(|a| match a {
            safe_cex_engine::Action::SetTimer { timer, at_ms } if at_ms == NOW + 25.0 => {
                Some(timer)
            }
            _ => None,
        })
        .unwrap();
    e.handle(NOW + 25.0, Event::TimerFired { timer });
    assert!(
        requests(&mut e).iter().any(|r| kind(r) == "userFills"),
        "a market still retrying its seed must not block private reads"
    );
}
#[test]
fn one_malformed_rest_row_keeps_its_stored_copy_while_valid_peers_commit() {
    use safe_cex_engine::Action;
    let orders =
        |rows: &[String]| format!(r#"{{"success":true,"code":0,"data":[{}]}}"#, rows.join(","));
    let order = |oid: u64, side: &str, sz: &str| {
        ORDER
            .replace("\"oid\":42", &format!("\"oid\":{oid}"))
            .replace("\"side\":\"B\"", &format!("\"side\":\"{side}\""))
            .replace("\"sz\":\"1.5\"", &format!("\"sz\":\"{sz}\""))
    };
    let cycle = |e: &mut safe_cex_engine::Engine<safe_cex_venue_txflow::TxFlow>,
                 timer,
                 now: f64,
                 legs: &[(f64, f64)],
                 rows: &[String]| {
        e.handle(now, Event::TimerFired { timer });
        let mut next = None;
        let mut reads = vec![];
        for action in e.drain_actions() {
            match action {
                Action::Http(r) => reads.push(r),
                Action::SetTimer { timer, at_ms } if at_ms == now + 5000.0 => next = Some(timer),
                _ => {}
            }
        }
        for r in reads {
            match kind(&r).as_str() {
                "clearinghouseState" => reply_at(e, &r, now, 200, &hedge_state(legs, now)),
                "frontendOpenOrders" => reply_at(e, &r, now, 200, &orders(rows)),
                _ => {}
            }
        }
        let logs = e
            .drain_actions()
            .filter_map(|a| match a {
                Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
                    message,
                    ..
                })) => Some(message),
                _ => None,
            })
            .collect::<Vec<_>>();
        (next.unwrap(), logs)
    };
    let (mut e, _, reads, timer) = bootstrap();
    answer(&mut e, reads);
    let (timer, _) = cycle(
        &mut e,
        timer,
        NOW + 5025.0,
        &[(2.0, 5.0), (-3.0, 8.0)],
        &[order(42, "B", "1.5"), order(43, "A", "2")],
    );
    let contracts = |e: &safe_cex_engine::Engine<safe_cex_venue_txflow::TxFlow>, id: &str| {
        e.store().position(id).map(|p| p.contracts)
    };
    assert_eq!(contracts(&e, "BTC-USDC:short"), Some(3.0));
    assert!(e.store().order("43").is_some());

    // Zero leverage and an unknown side are unmappable rows with known identities.
    let (timer, logs) = cycle(
        &mut e,
        timer,
        NOW + 10_025.0,
        &[(4.0, 5.0), (-5.0, 0.0)],
        &[order(42, "B", "1"), order(43, "X", "1")],
    );
    assert_eq!(contracts(&e, "BTC-USDC:long"), Some(4.0));
    assert_eq!(contracts(&e, "BTC-USDC:short"), Some(3.0));
    assert_eq!(e.store().order("42").unwrap().remaining, 1.0);
    assert_eq!(e.store().order("43").unwrap().remaining, 2.0);
    assert!(logs.contains(&"TxFlow skipped malformed position BTC-USDC:short".to_owned()));
    assert!(logs.contains(&"TxFlow skipped malformed order 43".to_owned()));
    assert!(
        !logs
            .iter()
            .any(|l| l.contains('{') || l.contains("leverage"))
    );
    // The same skip on the next poll is not reported again.
    let (timer, logs) = cycle(
        &mut e,
        timer,
        NOW + 15_025.0,
        &[(4.0, 5.0), (-5.0, 0.0)],
        &[order(42, "B", "1"), order(43, "X", "1")],
    );
    assert!(
        !logs
            .iter()
            .any(|l| l.contains("skipped malformed position"))
    );

    cycle(
        &mut e,
        timer,
        NOW + 20_025.0,
        &[(4.0, 5.0)],
        &[order(42, "B", "1")],
    );
    assert_eq!(contracts(&e, "BTC-USDC:short"), None);
    assert!(e.store().order("43").is_none());
    assert_eq!(e.store().orders().len(), 1);
}
#[test]
fn owned_position_on_unlisted_asset_holds_until_discovery_settles() {
    use safe_cex_engine::{Action, Level, LogEvent, Notice};
    let unlisted = STATE.replace(r#""asset":"1""#, r#""asset":"7""#);
    let read = |meta: &str| {
        let (mut e, _, reads, _) = bootstrap_with_meta(WALLET, meta);
        let (state, rest): (Vec<_>, Vec<_>) = reads
            .into_iter()
            .partition(|r| kind(r) == "clearinghouseState");
        answer(&mut e, rest);
        e.drain_actions().for_each(drop);
        reply(&mut e, &state[0], &unlisted);
        let logs = e
            .drain_actions()
            .filter_map(|a| match a {
                Action::Notice(Notice::Event(LogEvent { level, message, .. })) => {
                    Some((level, message))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        (e, logs)
    };

    // The catalogue skipped a malformed row, so asset 7 may be listed: hold.
    let (e, logs) = read(&META.replace("]}}", r#",{"index":7}]}}"#));
    assert!(!e.store().loaded().positions);
    assert!(
        logs.iter().any(|(_, m)| m
            == "TxFlow owned position asset unavailable; preserving previous account state")
    );

    // A complete catalogue settles it: the row is skipped, visibly.
    let (e, logs) = read(META);
    assert!(e.store().loaded().positions);
    assert!(e.store().positions().is_empty());
    assert!(
        logs.iter().any(|(level, m)| *level == Level::Error
            && m == "TxFlow skipped position on unlisted asset 7")
    );
}

fn reporter_state_rows(rows: serde_json::Value, time: f64) -> String {
    let mut state: serde_json::Value = serde_json::from_str(STATE).unwrap();
    state["data"]["time"] = time.into();
    state["data"]["assetPositions"] = rows;
    state.to_string()
}
fn reporter_state_cycle(
    e: &mut safe_cex_engine::Engine<safe_cex_venue_txflow::TxFlow>,
    timer: &mut safe_cex_engine::TimerId,
    now: &mut f64,
    rows: serde_json::Value,
    stale: bool,
) -> Vec<(safe_cex_engine::Level, String)> {
    use safe_cex_engine::{Action, Notice};
    *now += 5_000.;
    e.handle(*now, Event::TimerFired { timer: *timer });
    let mut reads = vec![];
    let mut next = None;
    for action in e.drain_actions() {
        match action {
            Action::Http(r) => reads.push(r),
            Action::SetTimer { timer, at_ms } if at_ms == *now + 5_000. => next = Some(timer),
            _ => (),
        }
    }
    *timer = next.expect("native account refresh");
    for r in reads {
        match kind(&r).as_str() {
            "clearinghouseState" => reply_at(
                e,
                &r,
                *now,
                200,
                &reporter_state_rows(rows.clone(), if stale { NOW - 1. } else { *now }),
            ),
            "frontendOpenOrders" => {
                reply_at(e, &r, *now, 200, r#"{"success":true,"code":0,"data":[]}"#)
            }
            _ => (),
        }
    }
    e.drain_actions()
        .filter_map(|a| match a {
            Action::Notice(Notice::Event(event))
                if event.code == safe_cex_engine::log::code::ROW_SKIPPED =>
            {
                Some((event.level, event.message))
            }
            _ => None,
        })
        .collect()
}
fn reporter_position() -> serde_json::Value {
    serde_json::from_str::<serde_json::Value>(STATE).unwrap()["data"]["assetPositions"][0].clone()
}
#[test]
fn reporter_composite_and_unlisted_keys_survive_single_leg_mapping_until_clean_read() {
    let (mut e, _, reads, mut timer) = bootstrap();
    answer(&mut e, reads);
    e.drain_actions().for_each(drop);
    let mut now = NOW + 25.;
    let valid = reporter_position();
    let mut composite = valid.clone();
    composite["position"]["szi"] = "bad".into();
    let mut unlisted = valid.clone();
    unlisted["position"]["asset"] = "7".into();
    let bad = serde_json::json!([composite, {}, unlisted]);
    let first = reporter_state_cycle(&mut e, &mut timer, &mut now, bad.clone(), false);
    assert_eq!(first.len(), 3);
    assert_eq!(
        first[0].1,
        "TxFlow skipped malformed position BTC-USDC:long, BTC-USDC:short"
    );
    assert_eq!(
        first[1].1,
        "TxFlow skipped malformed position without an asset"
    );
    assert_eq!(first[2].1, "TxFlow skipped position on unlisted asset 7");
    assert!(
        reporter_state_cycle(
            &mut e,
            &mut timer,
            &mut now,
            serde_json::json!([valid, {}, unlisted]),
            false
        )
        .is_empty()
    );
    // Forgetting the mapped long ID does not split/forget the composite label.
    assert!(reporter_state_cycle(&mut e, &mut timer, &mut now, bad.clone(), false).is_empty());
    reporter_state_cycle(&mut e, &mut timer, &mut now, serde_json::json!([]), false);
    assert_eq!(
        reporter_state_cycle(&mut e, &mut timer, &mut now, bad, false).len(),
        3
    );
}
#[test]
fn reporter_rest_mapping_rearms_exact_leg_even_when_snapshot_time_is_stale() {
    let (mut e, _, reads, mut timer) = bootstrap();
    answer(&mut e, reads);
    e.drain_actions().for_each(drop);
    let mut now = NOW + 25.;
    let valid = reporter_position();
    let mut bad = valid.clone();
    bad["position"]["leverage"]["value"] = 0.into();
    let rejected = serde_json::json!([bad, {}]);
    assert_eq!(
        reporter_state_cycle(&mut e, &mut timer, &mut now, rejected.clone(), false).len(),
        2
    );
    let mut mapped = valid.clone();
    mapped["position"]["szi"] = "9".into();
    assert!(
        reporter_state_cycle(
            &mut e,
            &mut timer,
            &mut now,
            serde_json::json!([mapped, {}]),
            true
        )
        .is_empty()
    );
    assert_eq!(e.store().positions()[0].contracts, 2.);
    let logs = reporter_state_cycle(&mut e, &mut timer, &mut now, rejected, false);
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].1, "TxFlow skipped malformed position BTC-USDC:long");
}
#[test]
fn reporter_zero_quantity_does_not_forget_and_once_does_not_upgrade_severity() {
    let (mut e, _, reads, mut timer) = bootstrap();
    answer(&mut e, reads);
    e.drain_actions().for_each(drop);
    let mut now = NOW + 25.;
    let valid = reporter_position();
    let mut bad = valid.clone();
    bad["position"]["leverage"]["value"] = 0.into();
    let rejected = serde_json::json!([bad, {}]);
    let first = reporter_state_cycle(&mut e, &mut timer, &mut now, rejected.clone(), false);
    assert_eq!(first.len(), 2);
    assert!(
        first
            .iter()
            .all(|(level, _)| *level == safe_cex_engine::Level::Warn)
    );
    let mut zero = valid.clone();
    zero["position"]["szi"] = "0".into();
    assert!(
        reporter_state_cycle(
            &mut e,
            &mut timer,
            &mut now,
            serde_json::json!([zero, {}]),
            false
        )
        .is_empty()
    );
    e.store_mut().set_positions(vec![]);
    assert!(reporter_state_cycle(&mut e, &mut timer, &mut now, rejected, false).is_empty());
}
