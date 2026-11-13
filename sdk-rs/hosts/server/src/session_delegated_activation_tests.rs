//! Partial activation permits place/reduce-only close, never the later action set.
use super::*;

async fn refuses_before_native_handoff(method: &str, args: Value) {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let tape = Tape::default();
            let account = delegated_account(&registry, "A", "bybit", tape.clone());
            let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
            call(&mut session, "unsupported-action", method, args).await;
            let result = reply(&mut sent, "unsupported-action").await;
            assert_eq!(result["success"], false, "{method}: {result}");
            assert_eq!(result["result"]["name"], "DelegatedActionUnavailable");
            assert!(
                result["result"]["message"]
                    .as_str()
                    .unwrap()
                    .contains(method)
            );
            assert!(
                tape.writes.lock().unwrap().is_empty(),
                "{method} reached native I/O"
            );
            assert!(
                account.link.is_none(),
                "a refusal must never select COSIGNED"
            );
            session.close();
            registry.dispose(&account, Disposal::PageDisposed);
        })
        .await;
}

#[tokio::test]
async fn partial_activation_amend_is_visibly_refused_before_handoff() {
    refuses_before_native_handoff(
        "updateOrder",
        json!([{"order":{"id":"resting-order","symbol":"BTCUSDT"},"update":{"price":51000}}]),
    )
    .await;
}

#[tokio::test]
async fn partial_activation_cancel_is_visibly_refused_before_handoff() {
    refuses_before_native_handoff(
        "cancelOrders",
        json!([[{"id":"resting-order","symbol":"BTCUSDT"}]]),
    )
    .await;
}

#[tokio::test]
async fn partial_activation_cancel_all_is_visibly_refused_before_handoff() {
    refuses_before_native_handoff("cancelAllOrders", json!([])).await;
}

#[tokio::test]
async fn partial_activation_cancel_all_refuses_without_waiting_for_a_busy_store() {
    use std::future::Future as _;

    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let tape = Tape::default();
            let account = delegated_account(&registry, "A", "bybit", tape.clone());
            let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
            let handle = account.handle.clone();
            // Poll both before the local actor can answer the first read.
            // A legacy refusal needs no store result.
            let (read_done, completed) = tokio::join!(biased;
                handle.read(|_| true),
                async {
                    let work = call(&mut session, "busy-legacy", "cancelAllOrders", json!([]));
                    tokio::pin!(work);
                    std::future::poll_fn(|cx| std::task::Poll::Ready(work.as_mut().poll(cx).is_ready()))
                        .await
                }
            );
            assert_eq!(read_done, Some(true));
            if completed {
                let result = reply(&mut sent, "busy-legacy").await;
                assert_eq!(result["success"], false);
                assert_eq!(result["result"]["name"], "DelegatedActionUnavailable");
            }
            assert!(tape.writes.lock().unwrap().is_empty());
            session.close();
            registry.dispose(&account, Disposal::PageDisposed);
            assert!(
                completed,
                "legacy refusal waited on the blocked order store"
            );
        })
        .await;
}

#[tokio::test]
async fn partial_activation_nuke_is_visibly_refused_before_handoff() {
    refuses_before_native_handoff("nuke", json!([])).await;
}

#[tokio::test]
async fn partial_activation_cannot_bypass_the_action_gate_through_exec_or_algo_start() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let tape = Tape::default();
            let account = delegated_account(&registry, "A", "bybit", tape.clone());
            let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
            for op in ["exec", "chaser:start", "twap:start", "twap:resume"] {
                session
                    .handle(
                        &json!({"t":"msg","d":{
                            "accountName":"synthetic-main","op":op,
                            "data":{"reqId":"synthetic-exec","command":"nuke"}
                        }})
                        .to_string(),
                    )
                    .await
                    .unwrap();
                let frame = tokio::time::timeout(Duration::from_secs(2), async {
                    while let Some(text) = sent.recv().await {
                        let frame: Value = serde_json::from_str(&text).unwrap();
                        if frame["d"]["op"] == "exec-result" || frame["d"]["op"] == "error" {
                            return frame;
                        }
                    }
                    panic!("missing visible refusal");
                })
                .await
                .unwrap();
                if op == "exec" {
                    assert_eq!(frame["d"]["op"], "exec-result");
                    assert_eq!(frame["d"]["data"]["reqId"], "synthetic-exec");
                    assert_eq!(frame["d"]["data"]["success"], false);
                    assert!(
                        frame["d"]["data"]["error"]
                            .as_str()
                            .unwrap()
                            .contains("DelegatedActionUnavailable")
                    );
                } else {
                    assert!(
                        frame["d"]["data"]
                            .to_string()
                            .contains("DelegatedActionUnavailable")
                    );
                }
                assert!(tape.writes.lock().unwrap().is_empty());
            }
            session.close();
            registry.dispose(&account, Disposal::PageDisposed);
        })
        .await;
}

fn restored_twap(id: &str, status: &str, chaser: bool) -> Value {
    let market = json!({"symbol":"BTCUSDT"});
    json!({
        "id": id,
        "options": {
            "accountName":"synthetic-main", "activeMarket":market,
            "size":0.002, "duration":10, "ordersCount":2, "side":"buy",
            "chaserDelay":0, "useChaserOrders":chaser,
        },
        "state": {
            "accountName":"synthetic-main", "activeMarket":market,
            "size":0.002, "ordersSizes":[0.001,0.001], "side":"buy", "status":status,
            "ordersCount":2, "ordersExecuted":0, "nextOrderAt":crate::accounts::now_ms(),
        },
    })
}

async fn send_algo(session: &mut Session, op: &str, data: Value) {
    session
        .handle(
            &json!({"t":"msg","d":{
                "accountName":"synthetic-main", "op":op, "data":data,
            }})
            .to_string(),
        )
        .await
        .unwrap();
}

async fn restore_is_refused(rows: Value) {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let tape = Tape::default();
            let (account, mut inbox) =
                delegated_account_with_algo_inbox(&registry, "A", "bybit", tape.clone());
            let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
            while inbox.try_recv().is_ok() {}
            send_algo(&mut session, "twap:state", rows).await;
            let forwarded = inbox.try_recv().is_ok();
            let mut refused = false;
            while let Ok(text) = sent.try_recv() {
                let frame: Value = serde_json::from_str(&text).unwrap();
                refused |= frame["d"]["op"] == "error"
                    && frame["d"]["data"]
                        .to_string()
                        .contains("DelegatedActionUnavailable");
            }
            session.close();
            drop(inbox);
            registry.dispose(&account, Disposal::PageDisposed);
            assert!(
                !forwarded,
                "running restore must be refused before algo handoff"
            );
            assert!(
                refused,
                "running restore must return a visible named refusal"
            );
            assert!(tape.writes.lock().unwrap().is_empty());
        })
        .await;
}

#[tokio::test]
async fn partial_activation_running_restore_is_visibly_refused() {
    restore_is_refused(json!([restored_twap("running", "running", false)])).await;
}

#[tokio::test]
async fn partial_activation_mixed_restore_is_visibly_refused() {
    restore_is_refused(json!([
        restored_twap("paused", "paused", false),
        restored_twap("running", "running", true),
    ]))
    .await;
}

#[tokio::test]
async fn partial_activation_paused_restore_and_cleanup_preserve_dispatch() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let tape = Tape::default();
            let (account, mut inbox) =
                delegated_account_with_algo_inbox(&registry, "A", "bybit", tape.clone());
            let (mut session, _sent) = joined(registry.clone(), "A", account.clone()).await;
            while inbox.try_recv().is_ok() {}
            for (op, data) in [
                (
                    "twap:state",
                    json!([restored_twap("paused", "paused", false)]),
                ),
                ("twap:state", json!([])),
                ("twap:pause", json!("paused")),
                ("twap:stop", json!("paused")),
                ("twap:cleanup", json!("paused")),
                ("chaser:terminate", json!("chaser")),
                ("chaser:terminateAll", Value::Null),
            ] {
                send_algo(&mut session, op, data.clone()).await;
                let Some(crate::algos::AlgoInput::Op {
                    op: received,
                    data: payload,
                    origin,
                    twap_owner,
                    action: None,
                }) = inbox.recv().await
                else {
                    panic!("missing safe algo dispatch");
                };
                assert_eq!(received, op);
                assert_eq!(payload, data.to_string());
                assert_eq!(origin, format!("socket:{}", session.socket));
                assert_eq!(twap_owner, account.twap_owner.snapshot());
            }
            assert!(tape.writes.lock().unwrap().is_empty());
            session.close();
            drop(inbox);
            registry.dispose(&account, Disposal::PageDisposed);
        })
        .await;
}

#[tokio::test]
async fn partial_activation_cosigned_running_restore_preserves_dispatch() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let (account, mut inbox) = testing::account_on(
                testing::HeldLoad {
                    placed: Arc::default(),
                    load_after_ms: Some(0.),
                    ticker: true,
                },
                ("A", "synthetic-main", "bybit"),
                Some(SignerLink::new(0, Arc::new(|_| false))),
                init("bybit").fingerprint(),
                Arc::new(crate::algos::Running::new(Arc::new(
                    tokio::sync::watch::channel(0).0,
                ))),
            );
            testing::register(&registry, &account);
            let (mut session, _sent) = joined(registry.clone(), "A", account.clone()).await;
            while inbox.try_recv().is_ok() {}
            let rows = json!([restored_twap("cosigned-running", "running", true)]);
            send_algo(&mut session, "twap:state", rows.clone()).await;
            let Some(crate::algos::AlgoInput::Op {
                op,
                data,
                origin,
                twap_owner,
                action: None,
            }) = inbox.recv().await
            else {
                panic!("missing COSIGNED restore dispatch");
            };
            assert_eq!(op, "twap:state");
            assert_eq!(data, rows.to_string());
            assert_eq!(origin, format!("socket:{}", session.socket));
            assert_eq!(twap_owner, account.twap_owner.snapshot());
            assert!(account.delegated.is_none());
            assert!(account.link.is_some());
            session.close();
            drop(inbox);
            registry.dispose(&account, Disposal::PageDisposed);
        })
        .await;
}

async fn native_restore_is_refused(chaser: bool) {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let tape = Tape::default();
            let (account, inbox) =
                delegated_account_with_algo_inbox(&registry, "A", "bybit", tape.clone());
            let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
            let out = session.out.clone();
            let runtime = tokio::task::spawn_local(crate::algos::run(
                account.handle.clone(),
                1_000_000,
                crate::audit::Who {
                    user: account.user.clone(),
                    account: account.name.clone(),
                    venue: account.venue.clone(),
                },
                Box::new(move |frame| {
                    out.send(frame);
                }),
                Box::new(|_| crate::algos::Holding {
                    live: vec![1],
                    parked: false,
                }),
                None,
                account.placed.clone(),
                account.held.clone(),
                account.running.clone(),
                account.twap_owner.clone(),
                inbox,
            ));
            send_algo(
                &mut session,
                "twap:state",
                json!([restored_twap("native-running", "running", chaser)]),
            )
            .await;
            let refused = tokio::time::timeout(Duration::from_secs(3), async {
                loop {
                    while let Ok(text) = sent.try_recv() {
                        let frame: Value = serde_json::from_str(&text).unwrap();
                        if frame["d"]["op"] == "error"
                            && frame["d"]["data"]
                                .to_string()
                                .contains("DelegatedActionUnavailable")
                        {
                            return true;
                        }
                    }
                    if !tape.writes.lock().unwrap().is_empty() {
                        return false;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .expect("native restore produces an admission outcome");
            let writes = tape.writes.lock().unwrap().clone();
            session.close();
            registry.dispose(&account, Disposal::PageDisposed);
            runtime.abort();
            assert!(
                refused,
                "running restore must receive its named admission refusal"
            );
            assert!(
                writes.is_empty(),
                "refused restore must not produce native writes"
            );
        })
        .await;
}

#[tokio::test]
async fn partial_activation_native_market_restore_is_refused() {
    native_restore_is_refused(false).await;
}

#[tokio::test]
async fn partial_activation_native_chaser_restore_is_refused() {
    native_restore_is_refused(true).await;
}

fn decoded_restored_twap(
    id: &str,
    status: Value,
    expected: safe_cex_algo_twap::TwapStatus,
) -> Value {
    let mut row = restored_twap(id, "running", false);
    row["state"]["status"] = status;
    let parsed: safe_cex_algo_twap::PersistedTwap =
        serde_json::from_value(row.clone()).expect("valid complete runtime restore row");
    assert_eq!(parsed.state.status, expected);
    row
}

#[tokio::test]
async fn partial_activation_decoded_running_restore_is_visibly_refused() {
    for status in [json!("running"), json!({"running":null})] {
        let row = decoded_restored_twap(
            "decoded-running",
            status,
            safe_cex_algo_twap::TwapStatus::Running,
        );
        restore_is_refused(json!([row])).await;
    }
}

#[tokio::test]
async fn partial_activation_mixed_decoded_restore_is_visibly_refused() {
    restore_is_refused(json!([
        decoded_restored_twap(
            "decoded-paused",
            json!({"paused":null}),
            safe_cex_algo_twap::TwapStatus::Paused
        ),
        decoded_restored_twap(
            "decoded-running",
            json!({"running":null}),
            safe_cex_algo_twap::TwapStatus::Running
        ),
    ]))
    .await;
}

#[tokio::test]
async fn partial_activation_decoded_paused_restore_preserves_dispatch() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let tape = Tape::default();
            let (account, mut inbox) =
                delegated_account_with_algo_inbox(&registry, "A", "bybit", tape.clone());
            let (mut session, _sent) = joined(registry.clone(), "A", account.clone()).await;
            while inbox.try_recv().is_ok() {}
            let rows = json!([decoded_restored_twap(
                "decoded-paused",
                json!({"paused":null}),
                safe_cex_algo_twap::TwapStatus::Paused
            )]);
            send_algo(&mut session, "twap:state", rows.clone()).await;
            let Some(crate::algos::AlgoInput::Op {
                op,
                data,
                origin,
                twap_owner,
                action: None,
            }) = inbox.recv().await
            else {
                panic!("missing paused restore dispatch");
            };
            assert_eq!(op, "twap:state");
            assert_eq!(data, rows.to_string());
            assert_eq!(origin, format!("socket:{}", session.socket));
            assert_eq!(twap_owner, account.twap_owner.snapshot());
            assert!(tape.writes.lock().unwrap().is_empty());
            session.close();
            drop(inbox);
            registry.dispose(&account, Disposal::PageDisposed);
        })
        .await;
}

#[tokio::test]
async fn partial_activation_decoded_row_restore_is_visibly_refused() {
    let row = decoded_restored_twap(
        "decoded-row",
        json!("running"),
        safe_cex_algo_twap::TwapStatus::Running,
    );
    let row = json!([row["id"], null, null, row["options"], row["state"]]);
    let parsed: safe_cex_algo_twap::PersistedTwap =
        serde_json::from_value(row.clone()).expect("valid complete runtime restore row");
    assert_eq!(parsed.owner_id, None);
    assert_eq!(parsed.owner_generation, None);
    assert_eq!(parsed.state.status, safe_cex_algo_twap::TwapStatus::Running);
    restore_is_refused(json!([row])).await;
}

#[tokio::test]
async fn partial_activation_cosigned_decoded_running_restore_preserves_dispatch() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let (account, mut inbox) = testing::account_on(
                testing::HeldLoad {
                    placed: Arc::default(),
                    load_after_ms: Some(0.),
                    ticker: true,
                },
                ("A", "synthetic-main", "bybit"),
                Some(SignerLink::new(0, Arc::new(|_| false))),
                init("bybit").fingerprint(),
                Arc::new(crate::algos::Running::new(Arc::new(
                    tokio::sync::watch::channel(0).0,
                ))),
            );
            testing::register(&registry, &account);
            let (mut session, _sent) = joined(registry.clone(), "A", account.clone()).await;
            while inbox.try_recv().is_ok() {}
            let rows = json!([decoded_restored_twap(
                "cosigned-decoded-running",
                json!({"running":null}),
                safe_cex_algo_twap::TwapStatus::Running
            )]);
            send_algo(&mut session, "twap:state", rows.clone()).await;
            let Some(crate::algos::AlgoInput::Op {
                op,
                data,
                origin,
                twap_owner,
                action: None,
            }) = inbox.recv().await
            else {
                panic!("missing COSIGNED restore dispatch");
            };
            assert_eq!(op, "twap:state");
            assert_eq!(data, rows.to_string());
            assert_eq!(origin, format!("socket:{}", session.socket));
            assert_eq!(twap_owner, account.twap_owner.snapshot());
            assert!(account.delegated.is_none());
            assert!(account.link.is_some());
            session.close();
            drop(inbox);
            registry.dispose(&account, Disposal::PageDisposed);
        })
        .await;
}
