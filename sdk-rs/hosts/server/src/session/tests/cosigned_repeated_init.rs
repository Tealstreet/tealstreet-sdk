use super::*;

async fn assert_snapshot_repeat(session: &mut Session, account: &Arc<Account>, f: &Fixture) {
    let client = session.joined["acct"].client.id;
    let verification = session.next_verification;
    let generation = account.twap_owner.snapshot();
    let request = init(Some("repeat"));
    let fingerprint_mismatch = account.fingerprint != request.fingerprint();
    let resident_delegated = account.delegated().is_some();
    let identity_matches = account.matches_init(session.user(), &request);
    session.init(request).await.unwrap();
    assert!(
        session.pending_verifications.is_empty(),
        "admitted COSIGNED repeat started proof: fingerprint_mismatch={fingerprint_mismatch}, resident_delegated={resident_delegated}, identity_matches={identity_matches}"
    );
    assert_eq!(session.next_verification, verification);
    assert!(Arc::ptr_eq(&session.joined["acct"].account, account));
    assert_ne!(
        session.joined["acct"].client.id, client,
        "fresh snapshot client"
    );
    assert_eq!(account.twap_owner.snapshot(), generation);
    assert!(account.handle.disposal_cause().is_none());
    assert_eq!(f.private_writes.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn admitted_cosigned_repeat_on_delegated_resident_only_resnapshots() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let f = fixture_with_auth(true).await;
            let account = delegated_resident(&f);
            let fingerprint: Value = serde_json::from_str(&account.fingerprint).unwrap();
            assert_eq!(fingerprint["hostedSigningMode"], "delegated");
            assert_ne!(account.fingerprint, init(Some("repeat")).fingerprint());
            let (mut session, mut sent) = candidate(&f, "alice");
            let mut done = session.take_verified();
            session.init(init(Some("admitted"))).await.unwrap();
            assert!(session.pending_verifications.contains_key("acct"));
            assert!(session.joined.is_empty(), "a new socket still needs proof");
            let completion = complete(
                &mut session,
                &mut sent,
                &mut done,
                &mut Device::default(),
                Answer::Correct,
                Some("admitted"),
            )
            .await;
            assert!(completion.probe.as_ref().unwrap().permit.is_ok());
            session.verification_completed(completion).await.unwrap();
            assert!(Arc::ptr_eq(&session.joined["acct"].account, &account));
            assert!(account.delegated().is_some());
            assert_snapshot_repeat(&mut session, &account, &f).await;
            session.close();
            f.registry
                .dispose(&account, crate::accounts::Disposal::PageDisposed);
        })
        .await;
}

#[tokio::test]
async fn admitted_cosigned_repeat_after_resident_upgrade_only_resnapshots() {
    let f = fixture_with_auth(true).await;
    let mut resident = Resident::start(&f, &mut Device::default()).await;
    let fingerprint: Value = serde_json::from_str(&resident.account.fingerprint).unwrap();
    assert!(fingerprint.get("hostedSigningMode").is_none());
    assert_eq!(
        resident.account.fingerprint,
        init(Some("repeat")).fingerprint()
    );
    // Model the routing-visible mode change; real protected handover is
    // covered by the Linux mode-transition tests.
    resident
        .account
        .signing
        .set_test_resident(crate::delegated::ResidentCopy::test_provider(Arc::new(
            safe_cex_crypto::signer::LocalSigner::new(),
        )));
    assert_snapshot_repeat(resident.session.as_mut().unwrap(), &resident.account, &f).await;
}

#[tokio::test]
async fn admitted_cosigned_repeat_without_mode_change_remains_snapshot_only() {
    let f = fixture_with_auth(true).await;
    let mut resident = Resident::start(&f, &mut Device::default()).await;
    assert_snapshot_repeat(resident.session.as_mut().unwrap(), &resident.account, &f).await;
}

#[tokio::test]
async fn admitted_cosigned_mode_change_still_requires_a_new_proof() {
    let f = fixture_with_auth(true).await;
    let mut resident = Resident::start(&f, &mut Device::default()).await;
    let session = resident.session.as_mut().unwrap();
    let client = session.joined["acct"].client.id;
    let verification = session.next_verification;
    let request = payload(session, Some("mode-change"));
    assert_eq!(request.signing_mode.as_deref(), Some("delegated"));
    session.init(request).await.unwrap();
    assert!(session.pending_verifications.contains_key("acct"));
    assert!(session.next_verification > verification);
    assert_eq!(session.joined["acct"].client.id, client);
    assert!(Arc::ptr_eq(
        &session.joined["acct"].account,
        &resident.account
    ));
    assert!(resident.account.delegated().is_none());
    assert_eq!(f.private_writes.load(Ordering::SeqCst), 0);
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn admitted_delegated_to_cosigned_mode_change_still_requires_a_new_proof() {
    let f = fixture_with_auth(true).await;
    let (mut session, mut sent) = candidate(&f, "alice");
    let mut done = session.take_verified();
    let request = payload(&mut session, Some("delegated-admitted"));
    session.init(request).await.unwrap();
    finish(&mut session, &mut done).await;
    let account = session.joined["acct"].account.clone();
    assert!(account.delegated().is_some());
    let client = session.joined["acct"].client.id;
    let verification = session.next_verification;
    while sent.try_recv().is_ok() {}
    session
        .init(init(Some("cosigned-requested")))
        .await
        .unwrap();
    assert!(session.pending_verifications.contains_key("acct"));
    assert!(
        session.pending_verifications["acct"]
            .cosigned_link
            .is_some()
    );
    assert!(session.next_verification > verification);
    assert_eq!(session.joined["acct"].client.id, client);
    assert!(Arc::ptr_eq(&session.joined["acct"].account, &account));
    let completion = complete(
        &mut session,
        &mut sent,
        &mut done,
        &mut Device::default(),
        Answer::OtherKey,
        Some("cosigned-requested"),
    )
    .await;
    assert!(completion.probe.as_ref().unwrap().permit.is_err());
    session.verification_completed(completion).await.unwrap();
    assert!(account.delegated().is_some());
    assert!(session.pending_verifications.is_empty());
    assert!(f.rejected.load(Ordering::SeqCst) > 0);
    session
        .init(init(Some("cosigned-after-refusal")))
        .await
        .unwrap();
    assert!(
        session.pending_verifications.contains_key("acct"),
        "a refused COSIGNED proof cannot change the prior DELEGATED admission"
    );
    assert!(
        session.pending_verifications["acct"]
            .cosigned_link
            .is_some()
    );
    assert_eq!(f.private_writes.load(Ordering::SeqCst), 0);
    session.close();
}

#[tokio::test]
async fn resumed_cosigned_socket_keeps_snapshot_path_after_resident_upgrade() {
    let f = fixture_with_auth(true).await;
    let mut resident = Resident::start(&f, &mut Device::default()).await;
    let mut previous = resident.session.take().unwrap();
    previous.identity.resumable = true;
    let from = previous.resume_id.clone();
    previous.close();
    let (mut session, _sent) = candidate(&f, "alice");
    session.identity.resumable = true;
    session.identity.resume_from = Some(from);
    session.open().await;
    assert!(Arc::ptr_eq(
        &session.joined["acct"].account,
        &resident.account
    ));
    resident
        .account
        .signing
        .set_test_resident(crate::delegated::ResidentCopy::test_provider(Arc::new(
            safe_cex_crypto::signer::LocalSigner::new(),
        )));
    assert_snapshot_repeat(&mut session, &resident.account, &f).await;
    session.identity.resumable = false;
    session.close();
}

#[tokio::test]
async fn refused_mode_change_preserves_original_cosigned_admission() {
    let f = fixture_with_auth(true).await;
    let (mut session, mut sent) = candidate(&f, "alice");
    let mut done = session.take_verified();
    session.init(init(Some("original"))).await.unwrap();
    let completion = complete(
        &mut session,
        &mut sent,
        &mut done,
        &mut Device::default(),
        Answer::Correct,
        Some("original"),
    )
    .await;
    session.verification_completed(completion).await.unwrap();
    let account = session.joined["acct"].account.clone();
    let mut request = payload(&mut session, Some("refused-change"));
    request.sealed.as_mut().unwrap().ciphertext.push('!');
    session.init(request).await.unwrap();
    assert!(session.pending_verifications.contains_key("acct"));
    let completion = tokio::time::timeout(Duration::from_secs(5), done.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(completion.candidate.is_err());
    session.verification_completed(completion).await.unwrap();
    assert!(session.pending_verifications.is_empty());
    assert!(account.delegated().is_none());
    assert_snapshot_repeat(&mut session, &account, &f).await;
    session.close();
}

#[tokio::test]
async fn admitted_cosigned_changed_settings_and_identity_still_require_proof() {
    let f = fixture_with_auth(true).await;
    let mut resident = Resident::start(&f, &mut Device::default()).await;
    resident
        .account
        .signing
        .set_test_resident(crate::delegated::ResidentCopy::test_provider(Arc::new(
            safe_cex_crypto::signer::LocalSigner::new(),
        )));
    let session = resident.session.as_mut().unwrap();
    for change_identity in [false, true] {
        let mut request = init(Some("changed-settings"));
        if change_identity {
            request.key = "synthetic-other-key".into();
        } else {
            request.enable_spot_markets = Some(true);
            assert_ne!(resident.account.fingerprint, request.fingerprint());
        }
        let verification = session.next_verification;
        let client = session.joined["acct"].client.id;
        session.init(request).await.unwrap();
        assert!(session.pending_verifications.contains_key("acct"));
        assert!(
            session.pending_verifications["acct"]
                .cosigned_link
                .is_some()
        );
        assert!(session.next_verification > verification);
        assert_eq!(session.joined["acct"].client.id, client);
        assert!(Arc::ptr_eq(
            &session.joined["acct"].account,
            &resident.account
        ));
    }
    assert_eq!(f.private_writes.load(Ordering::SeqCst), 0);
}
