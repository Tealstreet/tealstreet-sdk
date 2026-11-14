use super::*;

#[path = "delegated_identity_admission.rs"]
mod admission;

#[cfg(feature = "delegated-order-frames")]
#[path = "delegated_identity_orders.rs"]
mod orders;

fn named_payload(session: &mut Session, name: &str, key: &str, id: &str) -> InitPayload {
    let offer = session
        .handovers
        .issue(name, "bybit", crate::accounts::now_ms())
        .unwrap();
    let plaintext = json!({"key":key,"secret":"synthetic-secret"});
    InitPayload {
        account_name: name.into(),
        exchange_name: "bybit".into(),
        key: key.into(),
        signing_mode: Some("delegated".into()),
        instance_id: Some(id.into()),
        sealed: Some(crate::delegated::tests::seal(
            &offer,
            "bybit",
            key,
            plaintext.to_string().as_bytes(),
        )),
        ..Default::default()
    }
}

async fn admitted(
    session: &mut Session,
    name: &str,
    key: &str,
    id: &str,
    results: &mut tokio::sync::mpsc::UnboundedReceiver<JoinCompletion>,
) {
    let payload = named_payload(session, name, key, id);
    session.init(payload).await.unwrap();
    finish(session, results).await;
    assert!(session.joined.contains_key(name));
}

async fn frame(sent: &mut crate::out::Receiver, op: &str) -> Value {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let frame: Value = serde_json::from_str(&sent.recv().await.unwrap()).unwrap();
            if frame["t"] == "msg" && frame["d"]["op"] == op {
                return frame;
            }
        }
    })
    .await
    .expect("expected account frame")
}

async fn snapshot(session: &mut Session, sent: &mut crate::out::Receiver, label: &str) -> Value {
    let account = session.joined[label].account.clone();
    tokio::time::timeout(Duration::from_secs(5), async {
        while !crate::session::account_loaded(&account.handle).await {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("account loads before the socket releases its held snapshot");
    assert!(session.resnapshot(label).await);
    frame(sent, "frame").await
}

#[tokio::test]
async fn two_connections_share_identity_with_their_own_labels_and_registration_ids() {
    let f = fixture().await;
    let (mut a, mut a_sent) = user_session(f.registry.clone(), "alice");
    let (mut b, mut b_sent) = user_session(f.registry.clone(), "alice");
    let (mut a_results, mut b_results) = (a.take_verified(), b.take_verified());
    admitted(
        &mut a,
        "desktop",
        "synthetic-key",
        "desktop-id",
        &mut a_results,
    )
    .await;
    admitted(&mut b, "phone", "synthetic-key", "phone-id", &mut b_results).await;
    assert!(Arc::ptr_eq(
        &a.joined["desktop"].account,
        &b.joined["phone"].account
    ));
    assert_eq!(f.registry.account_count(), 1);
    let desktop = snapshot(&mut a, &mut a_sent, "desktop").await;
    let phone = snapshot(&mut b, &mut b_sent, "phone").await;
    assert_eq!(desktop["d"]["accountName"], "desktop");
    assert_eq!(desktop["d"]["instanceId"], "desktop-id");
    assert_eq!(phone["d"]["accountName"], "phone");
    assert_eq!(phone["d"]["instanceId"], "phone-id");
}

#[tokio::test]
async fn cosigned_labels_share_identity_and_a_different_key_keeps_the_previous_actor() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let first = InitPayload {
                account_name: "desktop".into(),
                exchange_name: "bybit".into(),
                key: "synthetic-key".into(),
                instance_id: Some("desktop-id".into()),
                ..Default::default()
            };
            let old = crate::accounts::testing::trading_account(&registry, "alice", &first);
            let (mut a, mut a_sent) = user_session(registry.clone(), "alice");
            let (mut b, mut b_sent) = user_session(registry.clone(), "alice");
            a.init(first.clone()).await.unwrap();
            b.init(InitPayload {
                account_name: "phone".into(),
                instance_id: Some("phone-id".into()),
                ..first.clone()
            })
            .await
            .unwrap();
            assert!(Arc::ptr_eq(&old, &a.joined["desktop"].account));
            assert!(Arc::ptr_eq(&old, &b.joined["phone"].account));
            let desktop = snapshot(&mut a, &mut a_sent, "desktop").await;
            let phone = snapshot(&mut b, &mut b_sent, "phone").await;
            assert_eq!(desktop["d"]["instanceId"], "desktop-id");
            assert_eq!(phone["d"]["instanceId"], "phone-id");
            assert_eq!(phone["d"]["accountName"], "phone");
            let changed = InitPayload {
                key: "synthetic-other-key".into(),
                ..first
            };
            let new = crate::accounts::testing::trading_account(&registry, "alice", &changed);
            a.init(changed).await.unwrap();
            assert!(Arc::ptr_eq(&new, &a.joined["desktop"].account));
            assert!(Arc::ptr_eq(&old, &b.joined["phone"].account));
            assert!(old.handle.disposal_cause().is_none());
            assert!(registry.holds(&old, b.socket));
            assert_eq!(registry.account_count(), 2);
            a.close();
            b.close();
        })
        .await;
}

#[tokio::test]
async fn one_connection_can_release_one_label_and_keep_the_other() {
    let f = fixture().await;
    let (mut session, _sent) = user_session(f.registry.clone(), "alice");
    let mut results = session.take_verified();
    admitted(
        &mut session,
        "first",
        "synthetic-key",
        "first-id",
        &mut results,
    )
    .await;
    admitted(
        &mut session,
        "second",
        "synthetic-key",
        "second-id",
        &mut results,
    )
    .await;
    let account = session.joined["first"].account.clone();
    assert!(Arc::ptr_eq(&account, &session.joined["second"].account));
    session
        .handle(r#"{"t":"dispose","d":{"accountName":"second"}}"#)
        .await
        .unwrap();
    assert!(f.registry.held("alice", "first", session.socket).is_some());
    assert!(f.registry.held("alice", "second", session.socket).is_none());
    assert!(f.registry.holds(&account, session.socket));
    assert_eq!(account.labels(session.socket), ["first"]);
}

#[tokio::test]
async fn separately_verified_delegated_keys_with_equal_labels_keep_separate_actors() {
    let f = fixture().await;
    let (mut first, _sent) = user_session(f.registry.clone(), "alice");
    let (mut second, _sent) = user_session(f.registry.clone(), "alice");
    let (mut first_done, mut second_done) = (first.take_verified(), second.take_verified());
    admitted(
        &mut first,
        "acct",
        "synthetic-key",
        "first",
        &mut first_done,
    )
    .await;
    let original = first.joined["acct"].account.clone();
    let provider = original.delegated.as_ref().unwrap().provider();
    admitted(
        &mut second,
        "acct",
        "synthetic-other-key",
        "second",
        &mut second_done,
    )
    .await;
    assert!(!Arc::ptr_eq(&original, &second.joined["acct"].account));
    assert!(Arc::ptr_eq(&original, &first.joined["acct"].account));
    assert!(Arc::ptr_eq(
        &provider,
        &original.delegated.as_ref().unwrap().provider()
    ));
    assert!(original.handle.disposal_cause().is_none());
    assert!(f.registry.holds(&original, first.socket));
    assert_eq!(f.registry.account_count(), 2);
}

#[tokio::test]
async fn parked_aliases_require_fresh_admission_under_the_same_verified_user() {
    let f = fixture().await;
    let (mut original, _sent) = user_session(f.registry.clone(), "alice");
    let mut results = original.take_verified();
    admitted(&mut original, "first", "synthetic-key", "one", &mut results).await;
    admitted(
        &mut original,
        "second",
        "synthetic-key",
        "two",
        &mut results,
    )
    .await;
    let account = original.joined["first"].account.clone();
    let resume = original.resume_id.clone();
    original.close();
    let (mut foreign, _sent) = user_session(f.registry.clone(), "bob");
    foreign.identity.resume_from = Some(resume.clone());
    foreign.open().await;
    assert!(foreign.joined.is_empty());
    let (mut resumed, _sent) = user_session(f.registry.clone(), "alice");
    resumed.identity.resume_from = Some(resume);
    resumed.open().await;
    assert!(
        resumed.joined.is_empty(),
        "DELEGATED cannot resume without a fresh handover"
    );
    assert!(
        account.has_parked_holders(),
        "the original grace holder remains"
    );
    let mut resumed_results = resumed.take_verified();
    for label in ["first", "second"] {
        let reads = f.private_reads.load(Ordering::SeqCst);
        admitted(
            &mut resumed,
            label,
            "synthetic-key",
            label,
            &mut resumed_results,
        )
        .await;
        assert!(
            f.private_reads.load(Ordering::SeqCst) > reads,
            "each alias needs fresh proof"
        );
        assert!(Arc::ptr_eq(&account, &resumed.joined[label].account));
        assert!(f.registry.held("alice", label, resumed.socket).is_some());
    }
    assert_eq!(resumed.joined.len(), 2);
    assert!(
        account.has_parked_holders(),
        "admission cannot replace the original grace timer"
    );
    assert!(foreign.joined.is_empty());
    assert_eq!(f.registry.account_count(), 1);
}

#[tokio::test]
async fn apply_settings_rebuilds_the_same_identity_using_the_existing_provider() {
    let f = fixture().await;
    let (mut session, mut sent) = user_session(f.registry.clone(), "alice");
    let mut results = session.take_verified();
    admitted(
        &mut session,
        "acct",
        "synthetic-key",
        "current",
        &mut results,
    )
    .await;
    let before = session.joined["acct"].account.clone();
    let provider = before.delegated.as_ref().unwrap().provider();
    let settings = InitPayload {
        account_name: "acct".into(),
        exchange_name: "bybit".into(),
        key: "synthetic-key".into(),
        signing_mode: Some("delegated".into()),
        enable_spot_markets: Some(true),
        ..Default::default()
    };
    session.handle(&json!({"t":"msg","d":{"op":"applySettings","accountName":"acct","data":{"requestId":7,"settings":settings}}}).to_string()).await.unwrap();
    let reply = frame(&mut sent, "applySettings:done").await;
    assert_eq!(reply["d"]["data"]["success"], true);
    assert_eq!(reply["d"]["instanceId"], "current");
    assert_eq!(reply["d"]["accountName"], "acct");
    let after = &session.joined["acct"].account;
    assert!(!Arc::ptr_eq(&before, after));
    assert_eq!(before.registry_name(), after.registry_name());
    assert_ne!(before.fingerprint, after.fingerprint);
    assert!(Arc::ptr_eq(
        &provider,
        &after.delegated.as_ref().unwrap().provider()
    ));
    assert_eq!(f.registry.account_count(), 1);
}

#[tokio::test]
async fn apply_settings_refuses_another_key_and_preserves_the_running_account() {
    let f = fixture().await;
    let (mut session, mut sent) = user_session(f.registry.clone(), "alice");
    let mut results = session.take_verified();
    admitted(
        &mut session,
        "acct",
        "synthetic-key",
        "current",
        &mut results,
    )
    .await;
    let before = session.joined["acct"].account.clone();
    let settings = InitPayload {
        account_name: "acct".into(),
        exchange_name: "bybit".into(),
        key: "synthetic-other-key".into(),
        signing_mode: Some("delegated".into()),
        enable_spot_markets: Some(false),
        ..Default::default()
    };
    session.handle(&json!({"t":"msg","d":{"op":"applySettings","accountName":"acct","data":{"requestId":8,"settings":settings}}}).to_string()).await.unwrap();
    let reply = frame(&mut sent, "applySettings:done").await;
    assert_eq!(reply["d"]["data"]["success"], false);
    assert_eq!(reply["d"]["instanceId"], "current");
    assert!(Arc::ptr_eq(&before, &session.joined["acct"].account));
    assert!(f.registry.holds(&before, session.socket));
    assert!(f.orders.lock().unwrap().is_empty());
}

#[tokio::test]
async fn apply_settings_without_admission_replies_to_the_current_registration() {
    let f = fixture().await;
    let (mut session, mut sent) = user_session(f.registry.clone(), "alice");
    session
        .registration_outputs
        .insert("acct".into(), session.out.with_instance_id(Some("current")));
    session.handle(&json!({"t":"msg","d":{"op":"applySettings","accountName":"acct","data":{"requestId":9,"settings":{}}}}).to_string()).await.unwrap();
    let reply = frame(&mut sent, "applySettings:done").await;
    assert_eq!(reply["d"]["data"]["success"], false);
    assert_eq!(reply["d"]["instanceId"], "current");
    assert_eq!(f.registry.account_count(), 0);
}

async fn replaced_identity_releases_only_its_label(keep_alias: bool, keep_other: bool) {
    let f = fixture().await;
    let (mut session, mut sent) = user_session(f.registry.clone(), "alice");
    let mut results = session.take_verified();
    admitted(&mut session, "acct", "synthetic-key", "old", &mut results).await;
    let previous = session.joined["acct"].account.clone();
    if keep_alias {
        admitted(
            &mut session,
            "alias",
            "synthetic-key",
            "alias-id",
            &mut results,
        )
        .await;
    }
    let (mut other, mut other_sent) = user_session(f.registry.clone(), "alice");
    let mut other_results = other.take_verified();
    if keep_other {
        admitted(
            &mut other,
            "other",
            "synthetic-key",
            "other-id",
            &mut other_results,
        )
        .await;
    }
    admitted(
        &mut session,
        "acct",
        "synthetic-next-key",
        "new",
        &mut results,
    )
    .await;
    let current = session.joined["acct"].account.clone();
    assert!(!Arc::ptr_eq(&previous, &current));
    assert!(Arc::ptr_eq(
        &current,
        &f.registry.held("alice", "acct", session.socket).unwrap()
    ));
    assert_eq!(
        previous.labels(session.socket),
        if keep_alias {
            vec!["alias".to_owned()]
        } else {
            vec![]
        }
    );
    assert_eq!(f.registry.holds(&previous, session.socket), keep_alias);
    assert_eq!(f.registry.holds(&previous, other.socket), keep_other);
    assert_eq!(
        previous.handle.disposal_cause().is_none(),
        keep_alias || keep_other
    );
    let notice = "synthetic previous account notice";
    previous.notify(envelope::error(Some("acct"), notice));
    for (sent, label, id, expected) in [
        (&mut sent, "alias", "alias-id", keep_alias && !keep_other),
        (&mut other_sent, "other", "other-id", keep_other),
    ] {
        let mut notices = 0;
        while let Ok(message) = sent.try_recv() {
            let message: Value = serde_json::from_str(&message).unwrap();
            if message["d"]["op"] == "error" && message["d"]["data"][0] == notice {
                notices += 1;
                assert_eq!(message["d"]["accountName"], label);
                assert_eq!(message["d"]["instanceId"], id);
            }
        }
        assert_eq!(notices, usize::from(expected));
    }
    session.identity.resumable = false;
    other.identity.resumable = false;
    let socket = session.socket;
    session.close();
    other.close();
    assert!(!f.registry.holds(&previous, socket));
    assert!(!f.registry.holds(&current, socket));
    assert_eq!(f.registry.account_count(), 0);
}

#[tokio::test]
async fn verified_identity_change_releases_the_replaced_registration() {
    replaced_identity_releases_only_its_label(false, false).await;
}

#[tokio::test]
async fn verified_identity_change_preserves_another_alias() {
    replaced_identity_releases_only_its_label(true, false).await;
}

#[tokio::test]
async fn verified_identity_change_preserves_another_holder() {
    replaced_identity_releases_only_its_label(false, true).await;
}

#[tokio::test]
async fn fixround_refused_alias_join_preserves_the_running_label() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for fill_clients in [false, true] {
                let registry = crate::accounts::tests::registry();
                let init = InitPayload {
                    account_name: "first".into(),
                    exchange_name: "bybit".into(),
                    key: "synthetic-key".into(),
                    ..Default::default()
                };
                let account = crate::accounts::testing::trading_account(&registry, "alice", &init);
                let (mut session, _sent) = user_session(registry.clone(), "alice");
                session.init(init.clone()).await.unwrap();
                let mut clients = Vec::new();
                if fill_clients {
                    for id in 10_000..10_256 {
                        match account.handle.join(id, false).await {
                            Ok(client) => clients.push(client),
                            Err(error) => {
                                assert_eq!(error, "account client limit reached");
                                break;
                            }
                        }
                    }
                    assert!(clients.len() < 256, "the native client limit was reached");
                }
                session
                    .init(InitPayload {
                        account_name: "second".into(),
                        ..init
                    })
                    .await
                    .unwrap();
                assert!(registry.held("alice", "first", session.socket).is_some());
                assert_eq!(session.joined.contains_key("second"), !fill_clients);
                assert_eq!(
                    registry.held("alice", "second", session.socket).is_some(),
                    !fill_clients
                );
                assert!(account.handle.disposal_cause().is_none());
                assert_eq!(registry.account_count(), 1);
                session.close();
                for (index, _) in clients.iter().enumerate() {
                    account.handle.leave(10_000 + index as u64);
                }
            }
        })
        .await;
}
