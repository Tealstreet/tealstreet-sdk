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
