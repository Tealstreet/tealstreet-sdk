use super::*;

#[tokio::test]
async fn delegated_join_proves_its_key_before_upgrading_a_cosigned_resident() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let f = fixture_with_auth(true).await;
            let resident =
                crate::accounts::testing::trading_account(&f.registry, "alice", &init(None));
            let (mut session, _sent) = candidate(&f, "alice");
            let request = payload(&mut session, Some("upgrade"));
            session.init(request).await.unwrap();
            assert!(
                session.pending_verifications.contains_key("acct"),
                "mode change must enter isolated possession proof, not refuse the resident's mode"
            );
            assert!(session.joined.is_empty());
            assert!(resident.delegated().is_none());
            assert!(resident.handle.disposal_cause().is_none());
            assert_eq!(f.private_writes.load(Ordering::SeqCst), 0);
            session.close();
            f.registry
                .dispose(&resident, crate::accounts::Disposal::PageDisposed);
        })
        .await;
}

fn delegated_resident(f: &Fixture) -> Arc<Account> {
    let request = init(None);
    let running = Arc::new(crate::algos::Running::new(Arc::new(
        tokio::sync::watch::channel(0).0,
    )));
    let (mut account, _inbox) = crate::accounts::testing::account_on(
        crate::accounts::testing::HeldLoad {
            placed: Arc::default(),
            load_after_ms: Some(0.),
            ticker: true,
        },
        ("alice", "acct", "bybit"),
        Some(SignerLink::new(0, Arc::new(|_| false))),
        crate::delegated::fingerprint(&request),
        running,
    );
    crate::accounts::testing::bind_identity(&mut account, &request);
    let inner = Arc::get_mut(&mut account).unwrap();
    inner.link = None;
    inner
        .signing
        .set_test_resident(crate::delegated::ResidentCopy::test_provider(Arc::new(
            safe_cex_crypto::signer::LocalSigner::new(),
        )));
    crate::accounts::testing::register(&f.registry, &account);
    account
}

#[tokio::test]
async fn cosigned_join_proves_its_own_key_and_attaches_to_delegated_resident() {
    tokio::task::LocalSet::new().run_until(async {
    let f = fixture_with_auth(true).await;
    let resident = delegated_resident(&f);
    let (mut session, mut sent) = candidate(&f, "alice");
    let mut done = session.take_verified();
    session.init(init(Some("attach"))).await.unwrap();
    assert!(session.pending_verifications.contains_key("acct"), "resident mode must not replace the joining session's proof");
    let mut device = Device::default();
    let completion = complete(&mut session, &mut sent, &mut done, &mut device, Answer::Correct, Some("attach")).await;
    session.verification_completed(completion).await.unwrap();
    assert!(Arc::ptr_eq(&session.joined["acct"].account, &resident));
    assert!(resident.delegated().is_some());
    assert!(resident.link.is_none());
    let frames: Vec<Value> = std::iter::from_fn(|| sent.try_recv().ok()).map(|text| serde_json::from_str(&text).unwrap()).collect();
    let mode = frames.iter().find(|frame| frame["d"]["op"] == "account-signing-mode").expect("confirmed actual mode");
    assert_eq!(mode["d"]["instanceId"], "attach");
    assert_eq!(mode["d"]["data"], json!({"version":1,"requestedMode":"cosigned","actualMode":"delegated","attached":true,"reason":"delegated-enabled-elsewhere"}));
    assert!(f.private_reads.load(Ordering::SeqCst) > 0);
    assert_eq!(f.private_writes.load(Ordering::SeqCst), 0);
    session.close();
    f.registry.dispose(&resident, crate::accounts::Disposal::PageDisposed);
    }).await;
}

#[tokio::test]
async fn wrong_cosigned_secret_cannot_borrow_the_delegated_resident_signer() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let f = fixture_with_auth(true).await;
            let resident = delegated_resident(&f);
            let (mut session, mut sent) = candidate(&f, "alice");
            let mut done = session.take_verified();
            session.init(init(Some("wrong-secret"))).await.unwrap();
            assert!(session.pending_verifications.contains_key("acct"));
            let mut device = Device::default();
            let completion = complete(
                &mut session,
                &mut sent,
                &mut done,
                &mut device,
                Answer::OtherKey,
                Some("wrong-secret"),
            )
            .await;
            session.verification_completed(completion).await.unwrap();
            assert!(session.joined.is_empty());
            assert!(resident.delegated().is_some());
            assert!(resident.handle.disposal_cause().is_none());
            assert!(f.rejected.load(Ordering::SeqCst) > 0);
            assert_eq!(f.private_writes.load(Ordering::SeqCst), 0);
            let frames: Vec<Value> = std::iter::from_fn(|| sent.try_recv().ok())
                .map(|text| serde_json::from_str(&text).unwrap())
                .collect();
            assert!(frames.iter().any(|frame| frame["d"]["op"] == "error"
                && frame["d"]["data"][1]["kind"] == "unverified"
                && frame["d"]["data"][1]["transient"] == false));
            assert!(
                !frames
                    .iter()
                    .any(|frame| frame["d"]["op"] == "account-signing-mode")
            );
            session.close();
            f.registry
                .dispose(&resident, crate::accounts::Disposal::PageDisposed);
        })
        .await;
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn proved_delegated_upgrade_keeps_the_actor_and_announces_both_registrations() {
    let f = fixture_with_auth(true).await;
    let mut device = Device::default();
    let mut original = Resident::start(&f, &mut device).await;
    let before_generation = original.account.twap_owner.snapshot();
    let (mut joining, mut sent) = candidate(&f, "alice");
    let mut done = joining.take_verified();
    let request = payload(&mut joining, Some("upgrade"));
    joining.init(request).await.unwrap();
    let completion = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            tokio::select! {
                completion = done.recv() => break completion.unwrap(),
                Some(text) = original._sent.recv() => {
                    let frame: Value = serde_json::from_str(&text).unwrap();
                    if frame["t"] == "sign-req" { device.answer(original.session.as_mut().unwrap(), frame, Answer::Correct).await; }
                }
            }
        }
    }).await.unwrap();
    assert!(completion.probe.as_ref().unwrap().permit.is_ok());
    joining.verification_completed(completion).await.unwrap();
    assert!(Arc::ptr_eq(
        &joining.joined["acct"].account,
        &original.account
    ));
    assert!(original.account.delegated().is_some());
    assert!(original.account.handle.disposal_cause().is_none());
    assert_eq!(original.account.running.counts(), original.counts);
    assert_eq!(original.account.twap_owner.snapshot(), before_generation);
    assert_eq!(
        original.session.as_ref().unwrap().joined["acct"].subscriptions,
        original.subscriptions
    );
    let frames: Vec<Value> = std::iter::from_fn(|| original._sent.try_recv().ok())
        .map(|text| serde_json::from_str(&text).unwrap())
        .collect();
    assert!(
        frames
            .iter()
            .any(|frame| frame["d"]["op"] == "account-signing-mode"
                && frame["d"]["instanceId"] == "resident"
                && frame["d"]["data"]["actualMode"] == "delegated"
                && frame["d"]["data"]["reason"] == "delegated-enabled-elsewhere")
    );
    let frames: Vec<Value> = std::iter::from_fn(|| sent.try_recv().ok())
        .map(|text| serde_json::from_str(&text).unwrap())
        .collect();
    assert!(
        frames
            .iter()
            .any(|frame| frame["d"]["op"] == "account-signing-mode"
                && frame["d"]["instanceId"] == "upgrade"
                && frame["d"]["data"]["actualMode"] == "delegated"
                && frame["d"]["data"]["reason"] == "requested")
    );
    assert_eq!(f.private_writes.load(Ordering::SeqCst), 0);
    joining.close();
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn delegated_upgrade_with_a_mismatched_secret_keeps_cosigned_residency_untouched() {
    let f = fixture_with_auth(true).await;
    let mut device = Device::default();
    let original = Resident::start(&f, &mut device).await;
    let (mut joining, mut sent) = candidate(&f, "alice");
    let mut done = joining.take_verified();
    let offer = joining
        .handovers
        .issue("acct", "bybit", crate::accounts::now_ms())
        .unwrap();
    let request = InitPayload {
        signing_mode: Some("delegated".into()),
        sealed: Some(crate::delegated::tests::seal(
            &offer,
            "bybit",
            KEY,
            br#"{"key":"synthetic-key","secret":"synthetic-other-secret"}"#,
        )),
        ..init(Some("wrong-key"))
    };
    joining.init(request).await.unwrap();
    finish(&mut joining, &mut done).await;
    refused(&f, &joining, &mut sent, Some("wrong-key"));
    assert!(
        f.rejected.load(Ordering::SeqCst) > 0,
        "the candidate must fail its own private-key proof"
    );
    original.untouched(&f);
    assert_eq!(f.private_writes.load(Ordering::SeqCst), 0);
    joining.close();
}
