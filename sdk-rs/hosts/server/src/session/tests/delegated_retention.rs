use super::*;
use safe_cex_engine::signing::{Algorithm, Capability, KeyRef};

#[tokio::test]
async fn restart_notice_precedes_handover_and_echoes_only_the_current_registration() {
    for id in [Some("restart-registration"), None, Some("")] {
        let registry = crate::accounts::tests::registry();
        let now = crate::accounts::now_ms();
        registry
            .interruptions
            .record(
                "alice",
                crate::interruption::Marker::parse(
                    &json!({"reason":"waiting-for-key","fromBox":"01","at":now}).to_string(),
                    now,
                )
                .unwrap(),
            )
            .unwrap();
        let (mut session, mut sent) = user_session(registry.clone(), "alice");
        session
            .init(InitPayload {
                account_name: "acct".into(),
                exchange_name: "bybit".into(),
                key: "synthetic-key".into(),
                signing_mode: Some("delegated".into()),
                instance_id: id.map(str::to_owned),
                ..Default::default()
            })
            .await
            .unwrap();
        let first = sent
            .try_recv()
            .expect("waiting notice before another key is supplied");
        let value: Value = serde_json::from_str(&first).unwrap();
        assert!(
            value["d"]["data"][0]
                .as_str()
                .unwrap()
                .contains("waiting for your key")
        );
        let expected = envelope::algo_notice(
            "acct",
            "Your algos are waiting for your key after the server restarted. Provide your key again, then check Open Orders before explicitly resuming TWAPs or starting new chases.",
            None,
        );
        let expected = id.filter(|id| !id.is_empty()).map_or_else(
            || expected.clone(),
            |id| envelope::with_instance_id(expected.clone(), id),
        );
        assert_eq!(first, expected);
        let second: Value = serde_json::from_str(&sent.try_recv().unwrap()).unwrap();
        assert_eq!(second["d"]["data"][1]["kind"], "unverified");
        assert_eq!(registry.account_count(), 0);
        session.close();
        let (mut bob, mut bob_sent) = user_session(registry.clone(), "bob");
        bob.init(InitPayload {
            account_name: "acct".into(),
            exchange_name: "bybit".into(),
            signing_mode: Some("delegated".into()),
            ..Default::default()
        })
        .await
        .unwrap();
        assert!(
            !bob_sent
                .try_recv()
                .unwrap()
                .contains("waiting for your key")
        );
        bob.close();
    }
}

#[tokio::test]
async fn a_new_authenticated_socket_can_remove_its_retained_account_but_another_user_cannot() {
    let f = fixture().await;
    let (mut alice, _sent) = user_session(f.registry.clone(), "alice");
    let mut ready = alice.take_verified();
    let init = payload(&mut alice, Some("retained-registration"));
    alice.init(init).await.unwrap();
    finish(&mut alice, &mut ready).await;
    let account = f.registry.held("alice", "acct", alice.socket).unwrap();
    let provider = account.delegated.as_ref().unwrap().provider();
    alice.close();
    assert_eq!(f.registry.account_count(), 1);
    assert_eq!(
        provider.can_sign(&KeyRef("primary.signing".into()), Algorithm::HmacSha256),
        Capability::Supported
    );
    let (mut bob, _) = user_session(f.registry.clone(), "bob");
    bob.handle(r#"{"t":"dispose","d":{"accountName":"acct"}}"#)
        .await
        .unwrap();
    assert_eq!(f.registry.account_count(), 1);
    bob.close();
    let (mut returning, _) = user_session(f.registry.clone(), "alice");
    returning
        .handle(r#"{"t":"dispose","d":{"accountName":"acct"}}"#)
        .await
        .unwrap();
    assert_eq!(f.registry.account_count(), 0);
    assert_eq!(
        provider.can_sign(&KeyRef("primary.signing".into()), Algorithm::HmacSha256),
        Capability::Unsupported
    );
    returning.close();
}

#[tokio::test]
async fn switching_login_during_retention_keeps_the_expiry_on_the_disconnected_user() {
    let f = fixture().await;
    let (mut alice, _alice_sent) = user_session(f.registry.clone(), "alice");
    let mut a_ready = alice.take_verified();
    let init = payload(&mut alice, Some("alice-registration"));
    alice.init(init).await.unwrap();
    finish(&mut alice, &mut a_ready).await;
    let a = f.registry.held("alice", "acct", alice.socket).unwrap();
    let a_provider = a.delegated.as_ref().unwrap().provider();
    alice.close();

    // A's captured expiry is pending throughout the login switch and B's proof await.
    let (mut bob, _bob_sent) = user_session(f.registry.clone(), "bob");
    let mut b_ready = bob.take_verified();
    let init = payload(&mut bob, Some("bob-registration"));
    bob.init(init).await.unwrap();
    finish(&mut bob, &mut b_ready).await;
    let b = f.registry.held("bob", "acct", bob.socket).unwrap();
    let b_provider = b.delegated.as_ref().unwrap().provider();
    assert_eq!(f.registry.account_count(), 2);
    tokio::time::pause();
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(48 * 60 * 60)).await;
    for _ in 0..5 {
        tokio::task::yield_now().await;
    }
    assert_eq!(f.registry.account_count(), 1);
    assert_eq!(
        a_provider.can_sign(&KeyRef("primary.signing".into()), Algorithm::HmacSha256),
        Capability::Unsupported
    );
    assert!(b.handle.disposal_cause().is_none());
    assert!(f.registry.held("bob", "acct", bob.socket).is_some());
    assert_eq!(
        b_provider.can_sign(&KeyRef("primary.signing".into()), Algorithm::HmacSha256),
        Capability::Supported
    );
    bob.handle(r#"{"t":"dispose","d":{"accountName":"acct"}}"#)
        .await
        .unwrap();
    bob.close();
}
