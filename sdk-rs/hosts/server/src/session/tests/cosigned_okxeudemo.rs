//! Demo admission retains EEA startup and the joining connection's authority.
use super::*;

#[tokio::test]
async fn cosigned_okxeudemo_owner_aba_during_config_signing_is_isolated() {
    let (io, _) = MockVenue::for_admission("okxeudemo");
    let registry = crate::accounts::testing::e2e_registry("alice", io);
    let (mut old, mut sent) = user_session(registry.clone(), "alice");
    old.identity.resumable = false;
    let mut retired_done = old.take_verified();
    old.init(payload("okxeudemo")).await.unwrap();
    let first: Value = serde_json::from_str(
        &tokio::time::timeout(Duration::from_secs(3), sent.recv())
            .await
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(first["t"], "sign-req");
    assert!(
        first["d"]["wsAuth"]["toSign"]
            .as_str()
            .unwrap()
            .contains("GET/api/v5/account/config")
    );
    let link = old.pending_verifications["acct"]
        .cosigned_link
        .clone()
        .unwrap();
    let socket = old.socket;
    assert!(old.joined.is_empty());
    old.close();
    for user in ["bob", "alice"] {
        let (mut current, mut output) = user_session(registry.clone(), user);
        current.identity.resumable = false;
        let mut device = Device::default();
        proof(
            "okxeudemo",
            &mut current,
            &mut output,
            &mut device,
            Answer::Correct,
        )
        .await;
        assert_eq!(current.joined["acct"].account.user, user);
        assert!(!link.resolve(
            socket,
            json!({"id":first["d"]["id"],"wsAuthSignature":"retired-synthetic-answer"})
        ));
        assert!(registry.held("alice", "acct", socket).is_none());
        assert!(retired_done.try_recv().is_err());
        current.close();
    }
}

#[tokio::test]
async fn cosigned_okxeudemo_wrong_passphrase_and_absent_signature_never_attach() {
    for answer in [Answer::WrongPassphrase, Answer::MissingSignature] {
        let (io, seen) = MockVenue::for_admission("okxeudemo");
        let registry = crate::accounts::testing::e2e_registry("alice", io);
        let (mut session, mut sent) = user_session(registry.clone(), "alice");
        let mut device = Device::default();
        proof("okxeudemo", &mut session, &mut sent, &mut device, answer).await;
        assert!(session.joined.is_empty());
        assert!(registry.held("alice", "acct", session.socket).is_none());
        if matches!(answer, Answer::WrongPassphrase) {
            assert_eq!(seen.lock().unwrap().refused, 2);
        }
        let frames: Vec<Value> = std::iter::from_fn(|| sent.try_recv().ok())
            .map(|s| serde_json::from_str(&s).unwrap())
            .collect();
        assert!(
            frames
                .iter()
                .any(|f| f["d"]["data"][1]["initRefused"] == true)
        );
        assert!(!frames.iter().any(|f| f["d"]["op"] == "frame"));
        session.close();
    }
}

#[tokio::test]
async fn cosigned_okxeudemo_config_retry_is_bounded_and_can_recover() {
    for refusals in [1, 2] {
        let (io, seen) = MockVenue::for_admission("okxeudemo");
        seen.lock().unwrap().config_refusals_remaining = refusals;
        let registry = crate::accounts::testing::e2e_registry("alice", io);
        let (mut session, mut sent) = user_session(registry.clone(), "alice");
        session.identity.resumable = false;
        let mut device = Device::default();
        proof(
            "okxeudemo",
            &mut session,
            &mut sent,
            &mut device,
            Answer::Correct,
        )
        .await;
        assert_eq!(session.joined.contains_key("acct"), refusals == 1);
        let seen = seen.lock().unwrap();
        assert_eq!(seen.refused, refusals);
        assert_eq!(seen.config_refusals_remaining, 0);
        if refusals == 2 {
            assert!(registry.held("alice", "acct", session.socket).is_none());
            assert!(
                seen.urls
                    .iter()
                    .all(|url| url.ends_with("/api/v5/account/config"))
            );
            let frames: Vec<Value> = std::iter::from_fn(|| sent.try_recv().ok())
                .map(|s| serde_json::from_str(&s).unwrap())
                .collect();
            assert!(
                frames
                    .iter()
                    .any(|f| f["d"]["data"][1]["kind"] == "unverified"
                        && f["d"]["data"][1]["transient"] == false)
            );
            assert!(!frames.iter().any(|f| f["d"]["op"] == "frame"));
        }
        drop(seen);
        session.close();
    }
}

#[tokio::test]
async fn cosigned_okxeudemo_config_success_is_not_complete_private_startup() {
    let (io, seen) = MockVenue::for_admission("okxeudemo");
    seen.lock().unwrap().refused_private_path = Some("/api/v5/account/balance");
    let registry = crate::accounts::testing::e2e_registry("alice", io);
    let (mut session, mut sent) = user_session(registry.clone(), "alice");
    let mut device = Device::default();
    proof(
        "okxeudemo",
        &mut session,
        &mut sent,
        &mut device,
        Answer::Correct,
    )
    .await;
    assert!(session.joined.is_empty());
    assert!(registry.held("alice", "acct", session.socket).is_none());
    let seen = seen.lock().unwrap();
    assert_variant_startup("okxeudemo", &seen);
    assert!(
        seen.urls
            .iter()
            .any(|url| url.ends_with("/api/v5/account/balance"))
    );
    assert!(seen.refused > 0);
    let frames: Vec<Value> = std::iter::from_fn(|| sent.try_recv().ok())
        .map(|s| serde_json::from_str(&s).unwrap())
        .collect();
    assert!(
        frames
            .iter()
            .any(|f| f["d"]["data"][1]["kind"] == "unverified"
                && f["d"]["data"][1]["transient"] == false)
    );
    assert!(!frames.iter().any(|f| f["d"]["op"] == "frame"));
    drop(seen);
    session.close();
}
