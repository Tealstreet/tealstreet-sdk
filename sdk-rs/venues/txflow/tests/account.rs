//! Account shapes and semantics from the current official TxFlow API docs.
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
    assert!(actions.iter().any(|a|matches!(a,safe_cex_engine::Action::Notice(safe_cex_engine::Notice::CommandDone {command:992,result:Err(message)})if message.contains("hedge leverage"))));
}
