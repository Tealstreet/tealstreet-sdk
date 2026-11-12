use super::*;

fn expected(frame: String, id: Option<&str>) -> String {
    match id.filter(|id| !id.is_empty()) {
        Some(id) => envelope::with_instance_id(frame, id),
        None => frame,
    }
}

#[tokio::test(start_paused = true)]
async fn handover_replies_use_current_registration_and_preserve_legacy_bytes() {
    tokio::task::LocalSet::new().run_until(async {
        let registry = crate::accounts::tests::registry();
        let mut original = acct_init();
        original.instance_id = Some("live-account".into());
        let account = crate::accounts::testing::trading_account(&registry, "u", &original);
        for _ in 0..100 {
            if account_loaded(&account.handle).await { break; }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(account_loaded(&account.handle).await);
        let (mut session, mut sent) = user_session(registry, "u");
        let mut completed = session.take_verified();
        session.init(original).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        while sent.try_recv().is_ok() {}
        for id in [Some("handover-A"), Some("handover-B"), None, Some("")] {
            let mut init = acct_init();
            init.instance_id = id.map(str::to_owned);
            init.signing_mode = Some("delegated".into());
            session.init(init).await.unwrap();
            assert_eq!(sent.try_recv().unwrap(), expected(envelope::init_unverified("acct", false), id));
            assert!(Arc::ptr_eq(&session.joined["acct"].account, &account));

            // Existing live-account output retains master's registration routing.
            let live = envelope::error(Some("acct"), "live account");
            session.send(live.clone());
            assert_eq!(sent.try_recv().unwrap(), expected(live, Some("live-account")));

            session.issue_handover("acct", "bybit");
            let actual = sent.try_recv().unwrap();
            let value: Value = serde_json::from_str(&actual).unwrap();
            let offer = crate::delegated::KeyOffer {
                attempt_id: value["d"]["attemptId"].as_str().unwrap().to_owned(),
                public_key: value["d"]["publicKey"].as_str().unwrap().to_owned(),
                boot_id: crate::state::boot_id(),
                expires_at_ms: value["d"]["expiresAtMs"].as_u64().unwrap(),
            };
            assert_eq!(actual, expected(envelope::delegated_key("acct", &offer), id));

            let (release, wait) = tokio::sync::oneshot::channel();
            session.start_verification("acct".into(), async move {
                wait.await.unwrap();
                Ok(synthetic_candidate())
            });
            session.handle(r#"{"t":"msg","d":{"op":"callMethod","accountName":"acct","data":{"requestId":"pending","method":"cancelAllOrders","args":[]}}}"#).await.unwrap();
            assert_eq!(sent.try_recv().unwrap(), expected(envelope::msg("acct", "callMethod:done", json!({"requestId":"pending", "success":false, "result":{"name":"Error", "message":"verifying", "outcome":"notSent"}})), id));
            for op in ["subscribeOrderBook", "chaser:start"] {
                session.handle(&json!({"t":"msg","d":{"op":op,"accountName":"acct","data":{}}}).to_string()).await.unwrap();
                assert_eq!(sent.try_recv().unwrap(), expected(envelope::error(Some("acct"), "verifying"), id));
            }
            release.send(()).unwrap();
            session.verification_completed(completed.recv().await.unwrap()).await.unwrap();
            assert_eq!(sent.try_recv().unwrap(), expected(envelope::init_unverified("acct", true), id));
        }
        session.close();
        account.handle.dispose();
    }).await;
}

#[tokio::test]
async fn unsupported_reinit_retires_queued_handover_completion() {
    let (mut session, mut sent) = user_session(crate::accounts::tests::registry(), "u");
    let mut completed = session.take_verified();
    session.start_verification("acct".into(), async { Ok(synthetic_candidate()) });
    let stale = completed.recv().await.unwrap();
    let mut init = acct_init();
    init.exchange_name = "bitmex".into();
    init.instance_id = Some("replacement".into());
    session.init(init).await.unwrap();
    assert_eq!(
        sent.try_recv().unwrap(),
        expected(
            envelope::init_unsupported("acct", "bitmex"),
            Some("replacement")
        )
    );
    session.verification_completed(stale).await.unwrap();
    assert!(sent.try_recv().is_err());
    session.close();
}
