use safe_cex_exchange_server::{
    accounts::{Disposal, Registry, testing},
    envelope::InitPayload,
    out::{Out, Receiver},
    session::{Identity, Session},
    state::Metrics,
    threads::EngineThreads,
};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

async fn frame(sent: &mut Receiver, kind: &str) -> Value {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let text = sent.recv().await.expect("session output remains open");
            let value: Value = serde_json::from_str(&text).unwrap();
            if value["t"] == kind || (value["t"] == "msg" && value["d"]["op"] == kind) {
                return value;
            }
        }
    })
    .await
    .expect("bounded session reply")
}

async fn cancellation_binding(pending: bool) {
    tokio::task::LocalSet::new()
        .run_until(async move {
            let mut registry = Registry::new(
                EngineThreads::start(1).unwrap(),
                Duration::from_millis(60),
                Duration::from_secs(1),
            );
            testing::set_join_admission(&mut registry, true);
            let registry = Arc::new(registry);
            let init = InitPayload {
                account_name: "fixture-main".into(),
                exchange_name: "bybit".into(),
                key: "synthetic-key".into(),
                instance_id: Some("live-registration".into()),
                ..Default::default()
            };
            let account = testing::trading_account(&registry, "fixture-owner", &init);
            registry.park(
                "fixture-resume".into(),
                "fixture-owner",
                vec![(account.clone(), 999)],
            );
            let (out, mut sent, _) = Out::channel(1024);
            let identity = Identity {
                user: Some("fixture-owner".into()),
                chat_handle: None,
                token: Some(("synthetic-proxy-token".into(), u64::MAX / 1000)),
                resume_from: Some("fixture-resume".into()),
                resumable: true,
            };
            let mut session = Session::new(
                registry.clone(),
                Arc::new(Metrics::default()),
                identity,
                out,
            );
            let _verification_completions = session.take_verified();
            session.open().await;
            session
                .handle(&json!({"t":"init","d":init}).to_string())
                .await
                .unwrap();
            session.handle(r#"{"t":"ping"}"#).await.unwrap();
            assert_eq!(frame(&mut sent, "pong").await["d"]["accounts"], json!(["fixture-main"]));

            let cancel = json!({"t":"order.actionCancelQueued","d":{
                "version":1,"accountName":"fixture-main","instanceId":"live-registration",
                "intentId":"019a7fff-ffff-7101-8101-010101010101","seq":1,"fingerprint":"a".repeat(64)
            }});
            session.handle(&cancel.to_string()).await.unwrap();
            let control = frame(&mut sent, "order.actionOutcome").await;
            assert_eq!(control["d"]["instanceId"], "live-registration");
            assert_eq!(control["d"]["outcome"], "unknown");
            assert_eq!(control["d"]["outcomeSeq"], 1);

            if pending {
                let replacement = InitPayload {
                    key: "synthetic-replacement-key".into(),
                    instance_id: Some("pending-registration".into()),
                    signing_mode: Some("cosigned".into()),
                    ..init
                };
                session
                    .handle(&json!({"t":"init","d":replacement}).to_string())
                    .await
                    .unwrap();
                // Completion delivery stays paused; the real init leaves the live row joined.
                session.handle(r#"{"t":"msg","d":{"op":"callMethod","accountName":"fixture-main","data":{"requestId":"pending-control","method":"fetchOrders","args":[]}}}"#).await.unwrap();
                let reply = frame(&mut sent, "callMethod:done").await;
                assert_eq!(reply["d"]["instanceId"], "pending-registration");
                assert_eq!(reply["d"]["data"]["requestId"], "pending-control");
                assert_eq!(reply["d"]["data"]["result"]["message"], "verifying");
            }

            for sequence in [2, 3] {
                session.handle(&cancel.to_string()).await.unwrap();
                let receipt = frame(&mut sent, "order.actionOutcome").await;
                assert_eq!(receipt["d"]["instanceId"], "live-registration");
                assert_eq!(receipt["d"]["outcome"], "unknown");
                assert_eq!(receipt["d"]["outcomeSeq"], sequence);
            }

            let mut mismatched = cancel.clone();
            mismatched["d"]["instanceId"] = "other-registration".into();
            session.handle(&mismatched.to_string()).await.unwrap();
            let error = frame(&mut sent, "error").await;
            assert_eq!(error["d"]["op"], "error");
            assert_eq!(error["d"]["instanceId"], if pending { "pending-registration" } else { "live-registration" });

            session.handle(&cancel.to_string()).await.unwrap();
            let after_mismatch = frame(&mut sent, "order.actionOutcome").await;
            assert_eq!(after_mismatch["d"]["instanceId"], "live-registration");
            assert_eq!(after_mismatch["d"]["outcomeSeq"], 4);

            registry.dispose(&account, Disposal::PageDisposed);
            session.handle(&cancel.to_string()).await.unwrap();
            let error = frame(&mut sent, "error").await;
            assert_eq!(error["d"]["op"], "error");
            assert_eq!(error["d"]["data"], json!(["Invalid action queue cancellation"]));
            session.close();
        })
        .await;
}

#[tokio::test]
async fn queued_action_cancellation_uses_live_registration_control() {
    cancellation_binding(false).await;
}

#[tokio::test]
async fn queued_action_cancellation_keeps_live_registration_during_verification() {
    cancellation_binding(true).await;
}
