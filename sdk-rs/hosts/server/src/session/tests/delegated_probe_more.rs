use super::*;

#[tokio::test]
async fn refused_candidates_leave_the_running_account_and_its_provider_untouched() {
    for (code, transient) in [(10003, false), (10006, true)] {
        let f = fixture().await;
        let (mut resident, _resident_sent) = user_session(f.registry.clone(), "alice");
        let mut ready = resident.take_verified();
        let init = payload(&mut resident, None);
        resident.init(init).await.unwrap();
        finish(&mut resident, &mut ready).await;
        let original = f.registry.held("alice", "acct", resident.socket).unwrap();
        let provider = original.delegated.as_ref().unwrap().provider();
        f.code.store(code, Ordering::SeqCst);
        let (mut candidate, mut sent) = user_session(f.registry.clone(), "alice");
        let mut done = candidate.take_verified();
        let init = payload(&mut candidate, Some("candidate"));
        candidate.init(init).await.unwrap();
        finish(&mut candidate, &mut done).await;
        assert_eq!(
            sent.try_recv().unwrap(),
            envelope::with_instance_id(envelope::init_unverified("acct", transient), "candidate")
        );
        assert!(f.registry.held("alice", "acct", candidate.socket).is_none());
        assert!(Arc::ptr_eq(
            &original,
            &f.registry.held("alice", "acct", resident.socket).unwrap()
        ));
        assert!(Arc::ptr_eq(
            &provider,
            &original.delegated.as_ref().unwrap().provider()
        ));
        assert!(original.handle.disposal_cause().is_none());
        candidate.close();
        resident.close();
    }
}

#[tokio::test]
async fn a_valid_pair_joins_without_comparing_or_replacing_resident_material() {
    let f = fixture().await;
    let (mut resident, _sent) = user_session(f.registry.clone(), "alice");
    let mut ready = resident.take_verified();
    let init = payload(&mut resident, None);
    resident.init(init).await.unwrap();
    finish(&mut resident, &mut ready).await;
    let original = f.registry.held("alice", "acct", resident.socket).unwrap();
    let provider = original.delegated.as_ref().unwrap().provider();
    let (mut candidate, _sent) = user_session(f.registry.clone(), "alice");
    let mut done = candidate.take_verified();
    let mut init = payload(&mut candidate, Some("second"));
    let offer = candidate
        .handovers
        .issue("acct", "bybit", crate::accounts::now_ms())
        .unwrap();
    init.sealed = Some(crate::delegated::tests::seal(
        &offer,
        "bybit",
        "synthetic-key",
        br#"{"key":"synthetic-key","secret":"synthetic-other-secret"}"#,
    ));
    candidate.init(init).await.unwrap();
    finish(&mut candidate, &mut done).await;
    assert!(Arc::ptr_eq(
        &original,
        &f.registry.held("alice", "acct", candidate.socket).unwrap()
    ));
    assert!(Arc::ptr_eq(
        &provider,
        &original.delegated.as_ref().unwrap().provider()
    ));
    assert!(original.handle.disposal_cause().is_none());
    candidate.close();
    resident.close();
}

#[tokio::test]
async fn a_refused_different_key_cannot_replace_a_running_delegated_account() {
    let f = fixture().await;
    let (mut resident, _sent) = user_session(f.registry.clone(), "alice");
    let mut ready = resident.take_verified();
    let init = payload(&mut resident, None);
    resident.init(init).await.unwrap();
    finish(&mut resident, &mut ready).await;
    let original = f.registry.held("alice", "acct", resident.socket).unwrap();
    f.code.store(10003, Ordering::SeqCst);
    let (mut candidate, mut sent) = user_session(f.registry.clone(), "alice");
    let mut done = candidate.take_verified();
    let mut init = payload(&mut candidate, None);
    let offer = candidate
        .handovers
        .issue("acct", "bybit", crate::accounts::now_ms())
        .unwrap();
    init.key = "synthetic-other-key".into();
    init.sealed = Some(crate::delegated::tests::seal(
        &offer,
        "bybit",
        &init.key,
        br#"{"key":"synthetic-other-key","secret":"synthetic-secret"}"#,
    ));
    candidate.init(init).await.unwrap();
    finish(&mut candidate, &mut done).await;
    assert_eq!(
        sent.try_recv().unwrap(),
        envelope::init_unverified("acct", false)
    );
    assert!(f.registry.held("alice", "acct", candidate.socket).is_none());
    assert!(Arc::ptr_eq(
        &original,
        &f.registry.held("alice", "acct", resident.socket).unwrap()
    ));
    assert!(original.handle.disposal_cause().is_none());
    candidate.close();
    resident.close();
}

#[tokio::test]
async fn two_users_switch_while_native_private_reads_are_deferred() {
    let f = fixture().await;
    f.gate.forget_permits(1000);
    let (mut alice, mut alice_sent) = user_session(f.registry.clone(), "alice");
    let alice_socket = alice.socket;
    let mut a_done = alice.take_verified();
    let init = payload(&mut alice, Some("alice-registration"));
    alice.init(init).await.unwrap();
    wait_reads(&f).await;
    alice.handle(r#"{"t":"ping"}"#).await.unwrap();
    let pong: Value = serde_json::from_str(&alice_sent.try_recv().unwrap()).unwrap();
    assert_eq!(pong["t"], "pong");
    assert_eq!(pong["d"]["accounts"], json!([]));
    let (mut bob, _bob_sent) = user_session(f.registry.clone(), "bob");
    let mut b_done = bob.take_verified();
    let init = payload(&mut bob, Some("bob-registration"));
    bob.init(init).await.unwrap();
    alice.close();
    f.gate.add_permits(1000);
    finish(&mut bob, &mut b_done).await;
    assert!(f.registry.held("bob", "acct", bob.socket).is_some());
    assert!(f.registry.held("alice", "acct", alice_socket).is_none());
    assert!(a_done.try_recv().is_err());
    assert!(alice_sent.try_recv().is_err());
    bob.close();
}

#[tokio::test]
async fn a_lease_generation_changed_during_probe_cannot_be_recaptured() {
    let mut f = fixture().await;
    Arc::get_mut(&mut f.registry).unwrap().router_leases = Arc::new(
        crate::router_lease::Users::new(crate::router_lease::Config {
            box_id: Some("probe-box".into()),
            enforce: true,
            ..Default::default()
        }),
    );
    let grant = |epoch| {
        let now = crate::accounts::now_ms();
        f.registry
            .router_leases
            .accept(&crate::router_lease::Heartbeat {
                r#box: "probe-box".into(),
                issued_at: now,
                users: vec![crate::router_lease::Assertion {
                    user_id: "alice".into(),
                    owner_epoch: epoch,
                    sequence: epoch,
                    not_after: now + 45_000,
                }],
            })
            .unwrap();
    };
    grant(1);
    f.gate.forget_permits(1000);
    let (mut session, mut sent) = user_session(f.registry.clone(), "alice");
    let mut done = session.take_verified();
    let init = payload(&mut session, None);
    session.init(init).await.unwrap();
    wait_reads(&f).await;
    grant(2);
    f.gate.add_permits(1000);
    finish(&mut session, &mut done).await;
    assert_eq!(
        sent.try_recv().unwrap(),
        envelope::init_unverified("acct", true)
    );
    assert!(f.registry.held("alice", "acct", session.socket).is_none());
    session.close();
}

#[tokio::test]
async fn native_probe_deadline_refuses_and_releases_its_engine() {
    let f = fixture().await;
    f.gate.forget_permits(1000);
    let (mut session, mut sent) = user_session(f.registry.clone(), "alice");
    let mut done = session.take_verified();
    let init = payload(&mut session, None);
    session.init(init).await.unwrap();
    wait_reads(&f).await;
    tokio::time::pause();
    let budget = safe_cex_remote_signer::SIGN_TIMEOUT
        + Duration::from_millis(u64::from(
            safe_cex_engine::request_budget::RequestClass::Read.budget_ms(),
        ));
    tokio::time::advance(budget + Duration::from_millis(1)).await;
    let completion = done.recv().await.unwrap();
    tokio::time::resume();
    session.verification_completed(completion).await.unwrap();
    assert_eq!(
        sent.try_recv().unwrap(),
        envelope::init_unverified("acct", true)
    );
    assert!(f.registry.held("alice", "acct", session.socket).is_none());
    tokio::time::timeout(Duration::from_secs(5), async {
        while crate::accounts::testing::engine_count(&f.registry) != 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("probe actor disposed");
    session.close();
}

#[tokio::test]
async fn probe_authority_is_captured_before_the_spawned_task_first_runs() {
    let mut f = fixture().await;
    Arc::get_mut(&mut f.registry).unwrap().router_leases = Arc::new(
        crate::router_lease::Users::new(crate::router_lease::Config {
            box_id: Some("probe-box".into()),
            enforce: true,
            ..Default::default()
        }),
    );
    let grant = |epoch| {
        let now = crate::accounts::now_ms();
        f.registry
            .router_leases
            .accept(&crate::router_lease::Heartbeat {
                r#box: "probe-box".into(),
                issued_at: now,
                users: vec![crate::router_lease::Assertion {
                    user_id: "alice".into(),
                    owner_epoch: epoch,
                    sequence: epoch,
                    not_after: now + 45_000,
                }],
            })
            .unwrap();
    };
    grant(1);
    let (mut session, mut sent) = user_session(f.registry.clone(), "alice");
    let mut done = session.take_verified();
    let init = payload(&mut session, None);
    session.init(init).await.unwrap();
    // This current-thread runtime has not yielded to the verification task.
    grant(2);
    finish(&mut session, &mut done).await;
    assert_eq!(
        sent.try_recv().unwrap(),
        envelope::init_unverified("acct", true)
    );
    assert_eq!(f.private_reads.load(Ordering::SeqCst), 0);
    assert!(f.registry.held("alice", "acct", session.socket).is_none());
    session.close();
}

#[tokio::test]
async fn verified_rebuild_retires_previous_output_generation() {
    let f = fixture().await;
    let (mut session, mut sent) = user_session(f.registry.clone(), "alice");
    let mut completed = session.take_verified();
    let init = payload(&mut session, Some("verified-owner-A"));
    session.init(init).await.unwrap();
    finish(&mut session, &mut completed).await;
    let a = session.joined["acct"].account.clone();
    let previous = session.account_out("acct");
    while sent.try_recv().is_ok() {}
    assert!(previous.send(envelope::error(Some("acct"), "queued-verified-A")));
    f.registry.dispose(&a, Disposal::Replaced);
    let init = payload(&mut session, Some("verified-owner-B"));
    session.init(init).await.unwrap();
    finish(&mut session, &mut completed).await;
    assert!(!Arc::ptr_eq(&session.joined["acct"].account, &a));
    assert!(
        !previous.send(envelope::error(Some("acct"), "late-verified-A")),
        "a verified rebuild must retire the old account's output"
    );
    while let Ok(frame) = sent.try_recv() {
        assert!(
            !frame.contains("queued-verified-A") && !frame.contains("late-verified-A"),
            "retired verified output survived: {frame}"
        );
    }
    assert!(
        session
            .account_out("acct")
            .send(envelope::error(Some("acct"), "current-verified-B"))
    );
    let current = std::iter::from_fn(|| sent.try_recv().ok())
        .find(|frame| frame.contains("current-verified-B"))
        .expect("the current verified owner still receives output");
    let current: Value = serde_json::from_str(&current).unwrap();
    assert_eq!(current["d"]["instanceId"], "verified-owner-B");
    session.close();
}
