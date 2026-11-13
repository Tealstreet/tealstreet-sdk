//! Restart recovery uses a router interruption, not persisted server algo/key state.
use super::*;

fn record_restart(f: &Fixture, user: &str) {
    // Existing routers already emit this reason when ownership cannot continue.
    f.registry
        .interruptions
        .record(
            user,
            crate::interruption::Marker {
                reason: crate::interruption::Reason::LeaseExpired,
                from_box: "01".into(),
                at: crate::accounts::now_ms(),
            },
        )
        .unwrap();
}

fn statuses(sent: &mut crate::out::Receiver) -> Vec<Value> {
    std::iter::from_fn(|| sent.try_recv().ok())
        .map(|frame| serde_json::from_str::<Value>(&frame).unwrap())
        .filter(|frame| frame["d"]["op"] == "delegated:status")
        .collect()
}

#[tokio::test]
async fn delegated_restart_ambiguous_unjoined_request_requires_an_explicit_venue() {
    crate::delegated::with_test_declaration(
        json!({"venues": {
            "bybit": {"remoteSigning": {"enabled": true}},
            "binance": {"remoteSigning": {"enabled": true}}
        }}),
        async {
            let f = fixture().await;
            record_restart(&f, "alice");
            let (mut session, mut sent) = user_session(f.registry.clone(), "alice");
            let epoch = f.registry.interruptions.account("alice", "acct").epoch(None);
            assert!(epoch != crate::interruption::RestoreEpoch::default());
            assert!(session.handover_venue("acct", &Value::Null).is_none());

            session
                .handle(
                    r#"{"t":"msg","d":{"op":"delegated-key-req","accountName":"acct","data":null}}"#,
                )
                .await
                .unwrap();
            let refused: Vec<Value> = std::iter::from_fn(|| sent.try_recv().ok())
                .map(|frame| serde_json::from_str(&frame).unwrap())
                .collect();
            assert_eq!(
                refused,
                vec![serde_json::from_str::<Value>(&envelope::init_unverified("acct", false)).unwrap()],
                "an ambiguous target must not receive a key offer or waiting status"
            );
            assert!(session.joined.is_empty());
            assert!(session.pending_verifications.is_empty());
            assert_eq!(f.private_reads.load(Ordering::SeqCst), 0);
            assert!(f.registry.interruptions.account("alice", "acct").epoch(None) == epoch);

            session
                .handle(
                    r#"{"t":"msg","d":{"op":"delegated-key-req","accountName":"acct","data":{"exchangeName":"bybit"}}}"#,
                )
                .await
                .unwrap();
            let offered: Vec<Value> = std::iter::from_fn(|| sent.try_recv().ok())
                .map(|frame| serde_json::from_str(&frame).unwrap())
                .collect();
            assert_eq!(offered.len(), 2, "the explicit target restores the normal offer/status sequence");
            assert_eq!(offered[0]["d"]["op"], "delegated-key");
            assert_eq!(offered[1]["d"]["op"], "delegated:status");
            assert_eq!(offered[1]["d"]["data"]["status"], "waiting-for-key");
            assert!(session.joined.is_empty(), "an offer is not admission");
            assert!(session.pending_verifications.is_empty());
            assert_eq!(f.private_reads.load(Ordering::SeqCst), 0);
            assert!(f.registry.interruptions.account("alice", "acct").epoch(None) == epoch);
            session.close();
        },
    )
    .await;
}

#[tokio::test]
async fn delegated_restart_key_availability_does_not_require_a_retained_marker() {
    let f = fixture().await;
    let (mut session, mut sent) = user_session(f.registry.clone(), "alice");
    let mut done = session.take_verified();
    let init = payload(&mut session, Some("admitted-without-marker"));
    session.init(init).await.unwrap();
    finish(&mut session, &mut done).await;
    let ready = statuses(&mut sent);
    assert_eq!(
        ready.len(),
        1,
        "an expired or absent marker cannot leave key status stuck waiting"
    );
    assert_eq!(ready[0]["d"]["data"]["status"], "key-available");
    session.close();
}

#[tokio::test]
async fn delegated_restart_waits_then_clears_only_after_admitted_handover() {
    let f = fixture().await;
    record_restart(&f, "alice");
    let (mut session, mut sent) = user_session(f.registry.clone(), "alice");
    let mut done = session.take_verified();
    session.issue_handover("acct", "bybit");
    let waiting = statuses(&mut sent);
    assert_eq!(
        waiting.len(),
        1,
        "interruption must not disappear into a key offer"
    );
    assert_eq!(waiting[0]["d"]["data"]["status"], "waiting-for-key");
    assert_eq!(waiting[0]["d"]["data"]["reason"], "waiting-for-key");
    assert_eq!(waiting[0]["d"]["data"]["message"], "waiting for your key");
    assert!(f.registry.held("alice", "acct", session.socket).is_none());

    // A real deferred venue read, not just a completion constructed by the test.
    f.gate.forget_permits(1000);
    let init = payload(&mut session, Some("returned-key"));
    session.init(init).await.unwrap();
    wait_reads(&f).await;
    assert!(
        statuses(&mut sent).is_empty(),
        "pending proof cannot clear the wait"
    );
    assert!(session.joined.is_empty());
    f.gate.add_permits(1000);
    finish(&mut session, &mut done).await;
    let ready = statuses(&mut sent);
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0]["d"]["data"]["status"], "key-available");
    assert_eq!(ready[0]["d"]["instanceId"], "returned-key");
    assert!(f.registry.held("alice", "acct", session.socket).is_some());
    let restore_guard = f.registry.interruptions.account("alice", "acct");
    let restore_epoch = restore_guard.epoch(None);
    assert!(
        restore_epoch != crate::interruption::RestoreEpoch::default(),
        "key admission must not erase the interruption's restore fence"
    );
    assert_eq!(
        session.joined["acct"].account.running.counts(),
        (0, 0),
        "restart cannot fabricate lost chases or TWAP schedules"
    );

    let identity = session.joined["acct"].account.registry_name().to_owned();
    let options: Value = serde_json::from_str(WEB_TWAP).unwrap();
    let now = crate::accounts::now_ms();
    let mut state = json!({
        "accountName":"acct", "activeMarket":options["activeMarket"],
        "size":0.1, "ordersSizes":[0.01,0.01,0.01], "side":"buy",
        "status":"running", "ordersCount":10, "ordersExecuted":7,
        "nextOrderAt":now+60_000, "origin":"web", "createdAt":now-180_000,
        "startedAt":now-180_000
    });
    // Synthetic saved evidence comes from the admitted account, never the received row.
    let owner = session.joined["acct"].account.twap_owner.snapshot();
    assert_eq!(
        owner.owner_id, identity,
        "restore authority follows the verified identity"
    );
    session
        .msg(AccountMsg {
            op: "twap:state".into(),
            account_name: Some("acct".into()),
            data: json!([{
                "id":"client-held-twap", "options":options, "state":state,
                "ownerId":owner.owner_id, "ownerGeneration":owner.generation
            }]),
        })
        .await;
    assert_partial_action_refusal(&mut sent, "twap:state");
    assert_eq!(session.joined["acct"].account.running.counts(), (0, 0));

    state["status"] = json!("paused");
    session
        .msg(AccountMsg {
            op: "twap:state".into(),
            account_name: Some("acct".into()),
            data: json!([{
                "id":"client-held-twap", "options":options, "state":state,
                "ownerId":owner.owner_id, "ownerGeneration":owner.generation
            }]),
        })
        .await;
    let restored = twap_row(&mut sent, "paused").await;
    assert_eq!(
        restored["status"], "paused",
        "key admission never auto-replays a job"
    );
    assert_eq!(restored["ordersExecuted"], 7);
    assert_eq!(restored["id"], "client-held-twap");
    session
        .msg(AccountMsg {
            op: "twap:resume".into(),
            account_name: Some("acct".into()),
            data: json!("client-held-twap"),
        })
        .await;
    assert_partial_action_refusal(&mut sent, "twap:resume");
    assert!(
        restore_guard.epoch(None) == restore_epoch
            && restore_guard.must_pause("client-held-twap", restore_epoch),
        "paused restoration and refused resume must keep the interruption fence"
    );
    let account = &session.joined["acct"].account;
    assert_eq!(
        account.running.counts(),
        (0, 1),
        "the paused schedule stays in the retained job inventory"
    );
    assert!(Arc::ptr_eq(
        account,
        &f.registry.held("alice", "acct", session.socket).unwrap()
    ));
    account
        .algos
        .as_ref()
        .unwrap()
        .send(crate::algos::AlgoInput::Joined)
        .unwrap();
    let still_paused = twap_row(&mut sent, "paused").await;
    assert_eq!(
        still_paused, restored,
        "refused resume must retain the paused schedule and its progress"
    );
    session.close();
}

#[tokio::test]
async fn delegated_restart_refuses_untagged_client_held_twap() {
    let f = fixture().await;
    record_restart(&f, "alice");
    let (mut session, mut sent) = user_session(f.registry.clone(), "alice");
    let mut done = session.take_verified();
    let init = payload(&mut session, Some("returned-key"));
    session.init(init).await.unwrap();
    finish(&mut session, &mut done).await;
    let options: Value = serde_json::from_str(WEB_TWAP).unwrap();
    let now = crate::accounts::now_ms();
    let state = json!({
        "accountName":"acct", "activeMarket":options["activeMarket"],
        "size":0.1, "ordersSizes":[0.01,0.01,0.01], "side":"buy",
        "status":"paused", "ordersCount":10, "ordersExecuted":7,
        "nextOrderAt":now+60_000, "origin":"web", "createdAt":now-180_000,
        "startedAt":now-180_000
    });
    session
        .msg(AccountMsg {
            op: "twap:state".into(),
            account_name: Some("acct".into()),
            data: json!([{"id":"untagged-client-held-twap", "options":options, "state":state}]),
        })
        .await;
    let expected_refusal =
        "TWAP BTCUSDT not restored: persisted TWAP owner or generation does not match";
    let refusal = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(frame) = sent.recv().await {
            let frame: Value = serde_json::from_str(&frame).unwrap();
            if frame["d"]["op"] == "error" && frame["d"]["data"][0] == expected_refusal {
                return frame["d"]["data"][0].clone();
            }
        }
        panic!("account output ended before the restore refusal");
    })
    .await
    .expect("untagged restore must visibly refuse");
    assert_eq!(refusal, expected_refusal);
    assert_eq!(session.joined["acct"].account.running.counts(), (0, 0));
    assert!(
        f.registry
            .interruptions
            .account("alice", "acct")
            .epoch(None)
            != crate::interruption::RestoreEpoch::default(),
        "an untagged row cannot clear the interruption fence"
    );
    session.close();
}

fn assert_partial_action_refusal(sent: &mut crate::out::Receiver, op: &str) {
    let errors: Vec<Value> = std::iter::from_fn(|| sent.try_recv().ok())
        .map(|frame| serde_json::from_str::<Value>(&frame).unwrap())
        .filter(|frame| frame["d"]["op"] == "error")
        .collect();
    let expected = envelope::with_instance_id(
        envelope::error(
            Some("acct"),
            &format!(
                "DelegatedActionUnavailable: DELEGATED {op} is unavailable; only place and reduce-only close are enabled"
            ),
        ),
        "returned-key",
    );
    assert_eq!(
        errors,
        vec![serde_json::from_str::<Value>(&expected).unwrap()]
    );
}

async fn twap_row(sent: &mut crate::out::Receiver, status: &str) -> Value {
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(frame) = sent.recv().await {
            let frame: Value = serde_json::from_str(&frame).unwrap();
            assert_ne!(
                frame["d"]["op"], "error",
                "unexpected algo refusal: {frame}"
            );
            if frame["d"]["op"] == "store"
                && let Some(row) = frame["d"]["data"]["activeTWAPs"]
                    .as_array()
                    .and_then(|rows| {
                        rows.iter()
                            .find(|row| row["id"] == "client-held-twap" && row["status"] == status)
                    })
            {
                return row.clone();
            }
        }
        panic!("account output ended before the TWAP row");
    })
    .await
    .expect("native algo reply")
}

#[tokio::test]
async fn delegated_restart_refusal_does_not_clear_wait_or_consume_another_users_marker() {
    let f = fixture().await;
    record_restart(&f, "alice");
    f.code.store(10003, Ordering::SeqCst);
    let (mut alice, mut sent) = user_session(f.registry.clone(), "alice");
    let mut done = alice.take_verified();
    let init = payload(&mut alice, Some("refused"));
    alice.init(init).await.unwrap();
    finish(&mut alice, &mut done).await;
    assert!(
        statuses(&mut sent).is_empty(),
        "refused keys never report available"
    );
    alice.issue_handover("acct", "bybit");
    assert_eq!(
        statuses(&mut sent)[0]["d"]["data"]["status"],
        "waiting-for-key"
    );
    let (mut bob, mut bob_sent) = user_session(f.registry.clone(), "bob");
    bob.issue_handover("acct", "bybit");
    assert!(
        statuses(&mut bob_sent).is_empty(),
        "another user's marker is private"
    );
    alice.close();
    bob.close();
}

#[tokio::test]
async fn delegated_restart_owner_switch_during_proof_cannot_clear_the_old_wait() {
    let f = fixture().await;
    record_restart(&f, "alice");
    record_restart(&f, "bob");
    f.gate.forget_permits(1000);
    let (mut alice, mut a_sent) = user_session(f.registry.clone(), "alice");
    let mut a_done = alice.take_verified();
    let init = payload(&mut alice, Some("alice-old"));
    alice.init(init).await.unwrap();
    wait_reads(&f).await;
    let (mut bob, mut b_sent) = user_session(f.registry.clone(), "bob");
    let mut b_done = bob.take_verified();
    let init = payload(&mut bob, Some("bob-current"));
    bob.init(init).await.unwrap();
    alice.close();
    f.gate.add_permits(1000);
    finish(&mut bob, &mut b_done).await;
    assert!(statuses(&mut a_sent).is_empty());
    assert!(a_done.try_recv().is_err());
    let ready = statuses(&mut b_sent);
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0]["d"]["instanceId"], "bob-current");
    let (mut returned, mut returned_sent) = user_session(f.registry.clone(), "alice");
    returned.issue_handover("acct", "bybit");
    assert_eq!(
        statuses(&mut returned_sent)[0]["d"]["data"]["status"],
        "waiting-for-key"
    );
    returned.close();
    bob.close();
}
