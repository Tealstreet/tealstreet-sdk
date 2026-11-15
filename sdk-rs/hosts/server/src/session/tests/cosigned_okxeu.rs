//! EEA startup remains bound to the joining socket, init, token and lease.
use super::*;

async fn deferred(
    session: &mut Session,
    sent: &mut crate::out::Receiver,
) -> (tokio::sync::mpsc::UnboundedReceiver<JoinCompletion>, Value) {
    let done = session.take_verified();
    session.init(payload("okxeu")).await.unwrap();
    let frame = tokio::time::timeout(Duration::from_secs(3), sent.recv())
        .await
        .unwrap()
        .unwrap();
    let frame: Value = serde_json::from_str(&frame).unwrap();
    assert_eq!(frame["t"], "sign-req");
    assert!(
        frame["d"]["wsAuth"]["toSign"]
            .as_str()
            .unwrap()
            .contains("GET/api/v5/account/config")
    );
    assert!(session.joined.is_empty());
    (done, frame)
}

async fn complete(
    session: &mut Session,
    sent: &mut crate::out::Receiver,
    done: &mut tokio::sync::mpsc::UnboundedReceiver<JoinCompletion>,
    first: Value,
) -> JoinCompletion {
    let mut device = Device::default();
    device
        .answer("okxeu", session, first, Answer::Correct)
        .await;
    tokio::time::timeout(Duration::from_secs(3), async {
        loop { tokio::select! {
            completed = done.recv() => return completed.unwrap(),
            text = sent.recv() => {
                let frame: Value = serde_json::from_str(&text.unwrap()).unwrap();
                if frame["t"] == "sign-req" { device.answer("okxeu", session, frame, Answer::Correct).await; }
                else { assert_ne!(frame["d"]["op"], "frame"); }
            }
        }}
    }).await.unwrap()
}

#[tokio::test]
async fn cosigned_okxeu_owner_aba_cannot_reuse_deferred_config() {
    let (io, _) = MockVenue::for_admission("okxeu");
    let registry = crate::accounts::testing::e2e_registry("alice", io);
    let (mut old, mut sent) = user_session(registry.clone(), "alice");
    old.identity.resumable = false;
    let (mut retired_done, first) = deferred(&mut old, &mut sent).await;
    let link = old.pending_verifications["acct"]
        .cosigned_link
        .clone()
        .unwrap();
    let socket = old.socket;
    old.close();
    for user in ["bob", "alice"] {
        let (mut current, mut output) = user_session(registry.clone(), user);
        current.identity.resumable = false;
        let mut device = Device::default();
        proof(
            "okxeu",
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
async fn cosigned_okxeu_expired_token_cannot_attach_completed_proof() {
    let (io, _) = MockVenue::for_admission("okxeu");
    let registry = crate::accounts::testing::e2e_registry("alice", io);
    let (mut session, mut sent) = user_session(registry.clone(), "alice");
    session.identity.resumable = false;
    let (mut done, first) = deferred(&mut session, &mut sent).await;
    let result = complete(&mut session, &mut sent, &mut done, first).await;
    assert!(result.probe.as_ref().unwrap().permit.is_ok());
    session.identity.token = Some(("synthetic-expired".into(), 0));
    session.verification_completed(result).await.unwrap();
    assert!(session.joined.is_empty());
    assert!(registry.held("alice", "acct", session.socket).is_none());
    session.close();
}

#[tokio::test]
async fn cosigned_okxeu_replaced_init_cannot_attach_old_completion() {
    let (io, _) = MockVenue::for_admission("okxeu");
    let registry = crate::accounts::testing::e2e_registry("alice", io);
    let (mut session, mut sent) = user_session(registry.clone(), "alice");
    session.identity.resumable = false;
    let (mut done, first) = deferred(&mut session, &mut sent).await;
    let result = complete(&mut session, &mut sent, &mut done, first).await;
    let mut replacement = payload("okxeu");
    replacement.instance_id = Some("successor".into());
    replacement.key = "synthetic-other-key".into();
    session.init(replacement).await.unwrap();
    session.verification_completed(result).await.unwrap();
    assert!(session.joined.is_empty());
    assert!(registry.held("alice", "acct", session.socket).is_none());
    session.close();
}

#[tokio::test]
async fn cosigned_okxeu_retired_lease_cannot_attach_completed_proof() {
    let (io, _) = MockVenue::for_admission("okxeu");
    let mut registry = crate::accounts::testing::e2e_registry("alice", io);
    Arc::get_mut(&mut registry).unwrap().router_leases = Arc::new(crate::router_lease::Users::new(
        crate::router_lease::Config {
            enforce: true,
            box_id: Some("01".into()),
            enable_grace_until: Some(crate::accounts::now_ms() + 10_000),
            ..Default::default()
        },
    ));
    let (mut session, mut sent) = user_session(registry.clone(), "alice");
    session.identity.resumable = false;
    let (mut done, first) = deferred(&mut session, &mut sent).await;
    let result = complete(&mut session, &mut sent, &mut done, first).await;
    assert!(result.probe.as_ref().unwrap().permit.is_ok());
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(11)).await;
    assert!(registry.user_lease_fenced("alice"));
    assert_eq!(
        session.verification_completed(result).await,
        Err(Close::Moved)
    );
    assert!(session.joined.is_empty());
    assert!(registry.held("alice", "acct", session.socket).is_none());
    session.close();
}

#[tokio::test]
async fn cosigned_okxeu_wrong_passphrase_and_absent_signature_never_attach() {
    for mode in [Answer::WrongPassphrase, Answer::MissingSignature] {
        let (io, seen) = MockVenue::for_admission("okxeu");
        let registry = crate::accounts::testing::e2e_registry("alice", io);
        let (mut session, mut sent) = user_session(registry.clone(), "alice");
        let mut device = Device::default();
        proof("okxeu", &mut session, &mut sent, &mut device, mode).await;
        assert!(session.joined.is_empty());
        assert!(registry.held("alice", "acct", session.socket).is_none());
        if matches!(mode, Answer::WrongPassphrase) {
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
async fn cosigned_okxeu_first_config_signature_refusal_can_recover() {
    let (io, seen) = MockVenue::for_admission("okxeu");
    seen.lock().unwrap().config_refusals_remaining = 1;
    let registry = crate::accounts::testing::e2e_registry("alice", io);
    let (mut session, mut sent) = user_session(registry.clone(), "alice");
    session.identity.resumable = false;
    let mut device = Device::default();
    proof(
        "okxeu",
        &mut session,
        &mut sent,
        &mut device,
        Answer::Correct,
    )
    .await;
    assert!(session.joined.contains_key("acct"));
    let seen = seen.lock().unwrap();
    assert_eq!(seen.refused, 1);
    assert_eq!(seen.config_refusals_remaining, 0);
    drop(seen);
    session.close();
}

#[tokio::test]
async fn cosigned_okxeu_authority_retired_while_config_signing_is_paused() {
    for case in ["token", "lease", "init"] {
        let (io, _) = MockVenue::for_admission("okxeu");
        let mut registry = crate::accounts::testing::e2e_registry("alice", io);
        if case == "lease" {
            Arc::get_mut(&mut registry).unwrap().router_leases = Arc::new(
                crate::router_lease::Users::new(crate::router_lease::Config {
                    enforce: true,
                    box_id: Some("01".into()),
                    enable_grace_until: Some(crate::accounts::now_ms() + 10_000),
                    ..Default::default()
                }),
            );
        }
        let (mut session, mut sent) = user_session(registry.clone(), "alice");
        session.identity.resumable = false;
        let (mut done, first) = deferred(&mut session, &mut sent).await;
        let link = session.pending_verifications["acct"]
            .cosigned_link
            .clone()
            .unwrap();
        let preimage = first["d"]["wsAuth"]["toSign"].as_str().unwrap();
        let answer = json!({"id":first["d"]["id"], "wsAuthSignature":safe_cex_crypto::hmac_sha256(MATERIAL.as_bytes(), preimage.as_bytes(), safe_cex_crypto::Encoding::Base64)});
        match case {
            "token" => session.identity.token = Some(("synthetic-expired".into(), 0)),
            "lease" => {
                tokio::time::pause();
                tokio::time::advance(Duration::from_secs(11)).await;
                assert!(registry.user_lease_fenced("alice"));
                tokio::time::resume();
            }
            "init" => {
                let mut replacement = payload("okxeu");
                replacement.key = "synthetic-other-key".into();
                replacement.instance_id = Some("successor".into());
                session.init(replacement).await.unwrap();
                assert!(!link.is_available());
            }
            _ => unreachable!(),
        }
        session
            .handle(&json!({"t":"sign-res", "d":answer}).to_string())
            .await
            .unwrap();
        if case == "lease" {
            let result = tokio::time::timeout(Duration::from_secs(3), done.recv())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                session.verification_completed(result).await,
                Err(Close::Moved)
            );
        }
        assert!(session.joined.is_empty(), "{case}");
        assert!(registry.held("alice", "acct", session.socket).is_none());
        session.close();
        assert!(!link.is_available());
    }
}

#[tokio::test]
async fn cosigned_okxeu_rate_limited_config_is_transient_not_bad_credentials() {
    let (io, seen) = MockVenue::for_admission("okxeu");
    {
        let mut seen = seen.lock().unwrap();
        seen.config_refusals_remaining = 1;
        seen.config_refusal_status = Some(429);
    }
    let registry = crate::accounts::testing::e2e_registry("alice", io);
    let (mut session, mut sent) = user_session(registry.clone(), "alice");
    let mut device = Device::default();
    proof(
        "okxeu",
        &mut session,
        &mut sent,
        &mut device,
        Answer::Correct,
    )
    .await;
    assert!(session.joined.is_empty());
    assert!(registry.held("alice", "acct", session.socket).is_none());
    assert_eq!(seen.lock().unwrap().refused, 1);
    let frames: Vec<Value> = std::iter::from_fn(|| sent.try_recv().ok())
        .map(|text| serde_json::from_str(&text).unwrap())
        .collect();
    let refusal = frames
        .iter()
        .find_map(|frame| {
            let data = &frame["d"]["data"][1];
            (data["initRefused"] == true).then_some(data)
        })
        .expect("the private read must refuse without attaching");
    assert_eq!(refusal["transient"], true, "{refusal:?}");
    assert!(!frames.iter().any(|frame| frame["d"]["op"] == "frame"));
    session.close();
}
