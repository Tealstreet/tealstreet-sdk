use super::*;
use safe_cex_crypto::{Encoding, hmac_sha256};

fn cosigned(name: &str, id: &str) -> InitPayload {
    InitPayload {
        account_name: name.into(),
        exchange_name: "bybit".into(),
        key: "synthetic-key".into(),
        signing_mode: Some("cosigned".into()),
        instance_id: Some(id.into()),
        ..Default::default()
    }
}

#[tokio::test]
async fn proof_queue_serializes_modes_across_sockets_and_labels() {
    let f = fixture().await;
    f.gate.forget_permits(1000);
    let (mut first, _first_sent) = user_session(f.registry.clone(), "alice");
    let mut first_done = first.take_verified();
    let init = payload(&mut first, Some("first"));
    first.init(init).await.unwrap();
    wait_reads(&f).await;

    let (mut second, mut sent) = user_session(f.registry.clone(), "alice");
    let mut done = second.take_verified();
    second.init(cosigned("renamed", "second")).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(400), sent.recv())
            .await
            .is_err(),
        "another socket and label must wait for the same credential's active proof"
    );
    assert!(second.joined.is_empty());
    f.gate.add_permits(1000);
    finish(&mut first, &mut first_done).await;
    let resident = first.joined["acct"].account.clone();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            tokio::select! {
                Some(text) = sent.recv() => {
                    let frame: Value = serde_json::from_str(&text).unwrap();
                    assert_eq!(frame["t"], "sign-req");
                    assert_eq!(frame["d"]["accountName"], "renamed");
                    assert_eq!(frame["d"]["instanceId"], "second");
                    let preimage = frame["d"]["wsAuth"]["toSign"].as_str().unwrap();
                    let signature = hmac_sha256(b"synthetic-secret", preimage.as_bytes(), Encoding::Hex);
                    second.handle(&json!({"t":"sign-res", "d":{
                        "id":frame["d"]["id"], "wsAuthSignature":signature
                    }}).to_string()).await.unwrap();
                }
                completion = done.recv() => {
                    let completion = completion.unwrap();
                    assert!(completion.probe.as_ref().unwrap().permit.is_ok());
                    second.verification_completed(completion).await.unwrap();
                    break;
                }
            }
        }
    }).await.expect("waiting proof starts and independently verifies");
    // Identity-keyed actors refuse cross-mode attachment even after a valid proof;
    // the queued COSIGNED proof cannot replace the existing DELEGATED resident.
    assert!(second.joined.is_empty());
    assert_eq!(
        sent.try_recv().unwrap(),
        envelope::with_instance_id(envelope::init_unverified("renamed", false), "second"),
    );
    assert!(Arc::ptr_eq(&first.joined["acct"].account, &resident));
    assert_eq!(f.registry.account_count(), 1);
    second.close();
    first.close();
}

#[tokio::test]
async fn proof_queue_capacity_is_shared_and_cancel_releases_reservations() {
    let f = fixture().await;
    let mut permits = Vec::new();
    for index in 0..5 {
        let mut init = cosigned(&format!("label-{index}"), "registration");
        if index % 2 == 0 {
            init.signing_mode = Some("delegated".into());
            init.enable_spot_markets = Some(true);
        }
        permits.push(f.registry.probe_permit("alice", &init).unwrap());
    }
    assert!(matches!(
        f.registry.probe_permit("alice", &cosigned("overflow", "overflow")),
        Err(refusal) if refusal.transient
    ));
    let mut alias = cosigned("variant-label", "variant");
    alias.exchange_name = "bybitspot".into();
    alias.bybit_site_id = Some("ignored-site".into());
    assert!(matches!(
        f.registry.probe_permit("alice", &alias),
        Err(refusal) if refusal.transient
    ));
    let mut other = cosigned("independent", "other");
    let bob = f.registry.probe_permit("bob", &other).unwrap();
    other.key = "different-credential".into();
    let another_key = f.registry.probe_permit("alice", &other).unwrap();
    other.key = "synthetic-key".into();
    other.testnet = Some(true);
    let another_network = f.registry.probe_permit("alice", &other).unwrap();
    other.testnet = None;
    other.bybit_site_id = Some("BRA_BTL".into());
    let another_site = f.registry.probe_permit("alice", &other).unwrap();
    drop(permits.pop());
    let replacement = f
        .registry
        .probe_permit("alice", &cosigned("replacement", "new"))
        .unwrap();
    assert!(matches!(
        f.registry.probe_permit("alice", &cosigned("overflow-again", "new")),
        Err(refusal) if refusal.transient
    ));
    drop((
        permits,
        replacement,
        bob,
        another_key,
        another_network,
        another_site,
    ));
    let recovered = f
        .registry
        .probe_permit("alice", &cosigned("recovered", "new"))
        .unwrap();
    drop(recovered);
}

#[tokio::test]
async fn proof_queue_cancelled_waiter_never_uses_another_users_socket() {
    let f = fixture().await;
    f.gate.forget_permits(1000);
    let (mut first, _first_sent) = user_session(f.registry.clone(), "alice");
    let init = payload(&mut first, None);
    first.init(init).await.unwrap();
    wait_reads(&f).await;
    let (mut stale, mut stale_sent) = user_session(f.registry.clone(), "alice");
    let mut stale_done = stale.take_verified();
    stale.init(cosigned("stale", "alice-stale")).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(300), stale_sent.recv())
            .await
            .is_err()
    );
    let stale_socket = stale.socket;
    stale.close();

    let (mut bob, mut bob_sent) = user_session(f.registry.clone(), "bob");
    bob.init(cosigned("stale", "bob-current")).await.unwrap();
    let text = tokio::time::timeout(Duration::from_secs(5), bob_sent.recv())
        .await
        .unwrap()
        .unwrap();
    let frame: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        frame["t"], "sign-req",
        "another user has an independent active slot"
    );
    assert_eq!(frame["d"]["instanceId"], "bob-current");
    first.close();
    f.gate.add_permits(1000);
    assert!(
        tokio::time::timeout(Duration::from_millis(300), stale_sent.recv())
            .await
            .ok()
            .flatten()
            .is_none()
    );
    assert!(stale_done.try_recv().is_err());
    assert!(f.registry.held("alice", "stale", stale_socket).is_none());
    bob.close();
}

#[tokio::test]
async fn proof_queue_rechecks_the_captured_expiry_after_waiting() {
    let f = fixture().await;
    f.gate.forget_permits(1000);
    let (mut first, _first_sent) = user_session(f.registry.clone(), "alice");
    let init = payload(&mut first, None);
    first.init(init).await.unwrap();
    wait_reads(&f).await;
    let (mut waiting, mut sent) = user_session(f.registry.clone(), "alice");
    let mut done = waiting.take_verified();
    waiting.identity.token.as_mut().unwrap().1 = now_ms() + 250;
    waiting
        .init(cosigned("expired-waiter", "expired"))
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(350), sent.recv())
            .await
            .is_err()
    );
    // Renewing the Session cannot renew authority already captured by the work.
    waiting.identity.token.as_mut().unwrap().1 = u64::MAX;
    first.close();
    f.gate.add_permits(1000);
    finish(&mut waiting, &mut done).await;
    assert_eq!(
        sent.try_recv().unwrap(),
        envelope::with_instance_id(
            envelope::init_unverified("expired-waiter", false),
            "expired"
        )
    );
    assert!(waiting.joined.is_empty());
    waiting.close();
}

#[tokio::test]
async fn proof_queue_session_capacity_refuses_transiently_in_both_modes() {
    for mode in ["cosigned", "delegated"] {
        let f = fixture().await;
        let (mut session, mut sent) = user_session(f.registry.clone(), "alice");
        for index in 0..64 {
            session.start_verification(format!("pending-{index}"), std::future::pending());
        }
        let mut init = payload(&mut session, Some("overflow-registration"));
        init.signing_mode = Some(mode.into());
        if mode == "cosigned" {
            init.sealed = None;
        }
        session.init(init).await.unwrap();
        assert_eq!(session.pending_verifications.len(), 64);
        assert!(session.joined.is_empty());
        assert_eq!(
            sent.try_recv().unwrap(),
            envelope::with_instance_id(
                envelope::init_unverified("acct", true),
                "overflow-registration"
            )
        );
        assert_eq!(f.private_reads.load(Ordering::SeqCst), 0);
        session.close();
    }
}

#[tokio::test]
async fn proof_queue_identity_overflow_keeps_registration_and_refuses_both_modes() {
    for mode in ["cosigned", "delegated"] {
        let f = fixture().await;
        let reservations: Vec<_> = (0..5)
            .map(|index| {
                f.registry
                    .probe_permit("alice", &cosigned(&format!("held-{index}"), "held"))
                    .unwrap()
            })
            .collect();
        let (mut session, mut sent) = user_session(f.registry.clone(), "alice");
        let mut done = session.take_verified();
        let mut init = payload(&mut session, Some("identity-overflow"));
        init.signing_mode = Some(mode.into());
        if mode == "cosigned" {
            init.sealed = None;
        }
        session.init(init).await.unwrap();
        if mode == "cosigned" {
            finish(&mut session, &mut done).await;
        }
        assert_eq!(
            sent.try_recv().unwrap(),
            envelope::with_instance_id(
                envelope::init_unverified("acct", true),
                "identity-overflow"
            )
        );
        assert!(session.pending_verifications.is_empty());
        assert!(session.joined.is_empty());
        assert_eq!(f.private_reads.load(Ordering::SeqCst), 0);
        session.close();
        drop(reservations);
    }
}
