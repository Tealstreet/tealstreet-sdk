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
