use super::*;

fn init(instance_id: Option<&str>) -> InitPayload {
    let mut value = serde_json::to_value(acct_init()).unwrap();
    if let Some(id) = instance_id {
        value["instanceId"] = id.into();
    }
    serde_json::from_value(value).unwrap()
}

async fn ready() {
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
}

fn without_id(frame: &str) -> Value {
    let mut value: Value = serde_json::from_str(frame).unwrap();
    value["d"].as_object_mut().unwrap().remove("instanceId");
    value
}

async fn matching(sent: &mut tokio::sync::mpsc::Receiver<String>, expected: &str) -> String {
    let expected = without_id(expected);
    for _ in 0..100 {
        while let Ok(frame) = sent.try_recv() {
            if without_id(&frame) == expected {
                return frame;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("missing expected account reply: {expected}");
}

async fn fixture(id: Option<&str>) -> (Arc<Account>, Session, tokio::sync::mpsc::Receiver<String>) {
    let registry = crate::accounts::tests::registry();
    let payload = init(id);
    let account = crate::accounts::testing::trading_account(&registry, "u", &payload);
    for _ in 0..100 {
        if account_loaded(&account.handle).await {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        account_loaded(&account.handle).await,
        "the fixture account is loaded before joining"
    );
    let (mut session, sent) = user_session(registry, "u");
    session.init(payload).await.unwrap();
    ready().await;
    (account, session, sent)
}

async fn account_reply(frame: String) {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (account, session, mut sent) = fixture(Some("registration-A")).await;
            while sent.try_recv().is_ok() {}
            session.send(frame.clone());
            let reply = matching(&mut sent, &frame).await;
            let value: Value = serde_json::from_str(&reply).unwrap();
            assert_eq!(value["d"]["instanceId"], "registration-A");
            assert!(value.get("instanceId").is_none());
            assert_eq!(without_id(&reply), without_id(&frame));
            assert!(reply.starts_with(r#"{"t":"msg","d":"#));
            session.close();
            account.handle.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn frame_echoes_instance_id() {
    account_reply(envelope::frame("acct", 9, false, b"probe")).await;
}

#[tokio::test(start_paused = true)]
async fn store_echoes_instance_id() {
    account_reply(envelope::msg("acct", "store", json!({"probe":1}))).await;
}

#[tokio::test(start_paused = true)]
async fn exec_result_echoes_instance_id() {
    account_reply(envelope::msg(
        "acct",
        "exec-result",
        json!({"reqId":"probe"}),
    ))
    .await;
}

#[tokio::test(start_paused = true)]
async fn error_echoes_instance_id() {
    account_reply(envelope::error(Some("acct"), "probe")).await;
}

#[tokio::test(start_paused = true)]
async fn algo_error_echoes_instance_id() {
    account_reply(envelope::algo_error("acct", json!("probe"))).await;
}

#[tokio::test(start_paused = true)]
async fn stop_notice_echoes_instance_id() {
    account_reply(envelope::algo_notice(
        "acct",
        "probe",
        Some(&crate::algos::AlgoRef {
            kind: "chase",
            id: "probe".into(),
        }),
    ))
    .await;
}

#[tokio::test(start_paused = true)]
async fn moving_error_echoes_instance_id() {
    account_reply(envelope::moving("acct", "probe")).await;
}

#[tokio::test(start_paused = true)]
async fn method_reply_echoes_instance_id() {
    account_reply(envelope::msg(
        "acct",
        "callMethod:done",
        json!({"requestId":"probe","success":true}),
    ))
    .await;
}

#[tokio::test(start_paused = true)]
async fn absent_and_empty_instance_ids_preserve_master_bytes() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for id in [None, Some("")] {
                let (account, session, mut sent) = fixture(id).await;
                let frames = [
                    envelope::frame("acct", 4, true, b"probe"),
                    envelope::msg("acct", "store", json!({"probe":1})),
                    envelope::msg("acct", "exec-result", json!({"reqId":"probe"})),
                    envelope::error(Some("acct"), "probe"),
                    envelope::algo_error("acct", json!("probe")),
                    envelope::algo_notice("acct", "probe", None),
                    envelope::moving("acct", "probe"),
                    envelope::init_refused("acct", "probe"),
                    envelope::init_unsupported("acct", "bitmex"),
                    r#"{"t":"sign-req","d":{"accountName":"acct","id":"probe"}}"#.into(),
                ];
                for frame in frames {
                    session.send(frame.clone());
                    assert_eq!(matching(&mut sent, &frame).await, frame);
                }
                session.close();
                account.handle.dispose();
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn init_errors_echo_the_attempts_instance_id() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let (mut session, mut sent) = user_session(registry, "u");
            let mut payload = init(Some("refused-registration"));
            payload.exchange_name = "bitmex".into();
            session.init(payload).await.unwrap();
            let frame = matching(&mut sent, &envelope::init_unsupported("acct", "bitmex")).await;
            let value: Value = serde_json::from_str(&frame).unwrap();
            assert_eq!(value["d"]["instanceId"], "refused-registration");
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn actual_snapshot_and_resnapshot_echo_instance_id() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (account, mut session, mut sent) = fixture(Some("snapshot-registration")).await;
            for resnapshot in [false, true] {
                if resnapshot {
                    assert!(session.resnapshot("acct").await);
                    ready().await;
                }
                let mut snapshots = Vec::new();
                while let Ok(frame) = sent.try_recv() {
                    let value: Value = serde_json::from_str(&frame).unwrap();
                    if value["d"]["op"] == "frame" && value["d"]["snapshot"] == true {
                        snapshots.push(value);
                    }
                }
                assert!(
                    !snapshots.is_empty(),
                    "the native pump must emit an actual snapshot"
                );
                for frame in snapshots {
                    assert_eq!(frame["d"]["instanceId"], "snapshot-registration");
                }
            }
            session.close();
            account.handle.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn two_devices_receive_their_own_broadcast_instance_ids() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (account, first, mut first_sent) = fixture(Some("device-A")).await;
            let (mut second, mut second_sent) = user_session(first.registry.clone(), "u");
            second.init(init(Some("device-B"))).await.unwrap();
            assert!(Arc::ptr_eq(
                &first.joined["acct"].account,
                &second.joined["acct"].account
            ));
            ready().await;
            while first_sent.try_recv().is_ok() {}
            while second_sent.try_recv().is_ok() {}
            crate::accounts::testing::algo_op(&account, "chaser:start", "invalid");
            ready().await;
            for (sent, id) in [
                (&mut first_sent, "device-A"),
                (&mut second_sent, "device-B"),
            ] {
                let mut errors = Vec::new();
                while let Ok(frame) = sent.try_recv() {
                    let value: Value = serde_json::from_str(&frame).unwrap();
                    if value["d"]["op"] == "error" {
                        errors.push(value);
                    }
                }
                assert!(
                    !errors.is_empty(),
                    "real algo broadcast must reach each holder"
                );
                for frame in errors {
                    assert_eq!(frame["d"]["instanceId"], id);
                }
            }
            first.close();
            second.close();
            account.handle.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn repeated_init_updates_id_without_rebuilding_or_retagging_queued_output() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (account, mut session, mut sent) = fixture(Some("registration-A")).await;
            while sent.try_recv().is_ok() {}
            let held = envelope::msg("acct", "exec-result", json!({"reqId":"held-A"}));
            session.send(held.clone());
            session.init(init(Some("registration-B"))).await.unwrap();
            assert!(Arc::ptr_eq(&account, &session.joined["acct"].account));
            let queued: Value = serde_json::from_str(&matching(&mut sent, &held).await).unwrap();
            assert_eq!(queued["d"]["instanceId"], "registration-A");
            let current = envelope::msg("acct", "exec-result", json!({"reqId":"current-B"}));
            session.send(current.clone());
            let current: Value =
                serde_json::from_str(&matching(&mut sent, &current).await).unwrap();
            assert_eq!(current["d"]["instanceId"], "registration-B");
            session.close();
            account.handle.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn same_owner_resume_updates_the_connection_id() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (account, session, mut old_sent) = fixture(Some("before-resume")).await;
            let registry = session.registry.clone();
            let resume = session.resume_id.clone();
            session.close();
            while old_sent.try_recv().is_ok() {}
            drop(old_sent);
            let (mut next, mut sent) = user_session(registry, "u");
            next.identity.resume_from = Some(resume);
            next.open().await;
            next.init(init(Some("after-resume"))).await.unwrap();
            assert!(Arc::ptr_eq(&account, &next.joined["acct"].account));
            let frame = envelope::msg("acct", "exec-result", json!({"reqId":"resumed"}));
            account.notify(frame.clone());
            let frame: Value = serde_json::from_str(&matching(&mut sent, &frame).await).unwrap();
            assert_eq!(frame["d"]["instanceId"], "after-resume");
            next.close();
            account.handle.dispose();
        })
        .await;
}

#[test]
fn instance_id_is_a_string_and_never_changes_engine_identity() {
    let plain = init(None);
    for id in ["registration-A", "registration-B", ""] {
        let tagged = init(Some(id));
        assert_eq!(tagged.fingerprint(), plain.fingerprint());
        assert_eq!(
            crate::accounts::engine_options(&tagged, false),
            crate::accounts::engine_options(&plain, false)
        );
    }
    let mut value = serde_json::to_value(plain).unwrap();
    value["instanceId"] = json!(42);
    assert!(
        serde_json::from_value::<InitPayload>(value).is_err(),
        "registration IDs are strings, never numeric coercions"
    );
}

#[tokio::test(start_paused = true)]
async fn public_account_reinit_updates_notices_without_changing_the_signing_holder() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let mut payload = init(Some("public-A"));
            payload.public_only = Some(true);
            let (account, _) =
                crate::accounts::tests::held_account_with(None, payload.fingerprint());
            crate::accounts::tests::register(&registry, &account);
            let (mut session, mut sent) = user_session(registry, "u");
            session.init(payload.clone()).await.unwrap();
            let holder = crate::accounts::tests::signing_holder(&account);
            payload.instance_id = Some("public-B".into());
            session.init(payload).await.unwrap();
            assert!(Arc::ptr_eq(&account, &session.joined["acct"].account));
            assert_eq!(crate::accounts::tests::signing_holder(&account), holder);
            let notice = envelope::error(Some("acct"), "public notice");
            account.notify(notice.clone());
            let frame: Value = serde_json::from_str(&matching(&mut sent, &notice).await).unwrap();
            assert_eq!(frame["d"]["instanceId"], "public-B");
            session.close();
            account.handle.dispose();
        })
        .await;
}

fn signing(
    account: &Account,
) -> tokio::task::JoinHandle<
    Result<safe_cex_engine::signing::SignResult, safe_cex_engine::signing::SignError>,
> {
    use safe_cex_engine::signing::{
        Algorithm, KeyRef, Operation, PayloadPart, SignReceipt, SignRequest,
    };
    let remote = safe_cex_remote_signer::remote_account(
        account.link.as_ref().unwrap().clone(),
        "bybit",
        Some("acct".into()),
        r#"{"key":"synthetic-public-key"}"#,
    )
    .unwrap();
    let options: Value = serde_json::from_str(&remote.options_json).unwrap();
    let key = KeyRef(
        options["signing"]["primary"]["signing"]
            .as_str()
            .unwrap()
            .into(),
    );
    tokio::spawn(async move {
        remote
            .provider
            .sign(SignRequest {
                receipt: SignReceipt {
                    expiry_ms: Some(1_700_000_000_000.0),
                    ..Default::default()
                },
                operations: vec![Operation::Sign {
                    algorithm: Algorithm::HmacSha256,
                    key,
                    payload: vec![PayloadPart::Bytes {
                        bytes: b"GET/realtime1700000000000".to_vec(),
                    }],
                }],
                context: Default::default(),
            })
            .await
    })
}

async fn sign_request(sent: &mut tokio::sync::mpsc::Receiver<String>) -> Value {
    for _ in 0..100 {
        while let Ok(frame) = sent.try_recv() {
            if frame.starts_with(r#"{"t":"sign-req","d":"#) {
                return serde_json::from_str(&frame).unwrap();
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("the real remote signer must emit sign-req");
}

#[tokio::test(start_paused = true)]
async fn signer_requests_echo_the_signing_holders_instance_id() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (account, first, _) = fixture(Some("device-A")).await;
            let (mut second, mut sent) = user_session(first.registry.clone(), "u");
            second.init(init(Some("device-B"))).await.unwrap();
            let work = signing(&account);
            let frame = sign_request(&mut sent).await;
            assert_eq!(frame["d"]["instanceId"], "device-B");
            assert!(frame.get("instanceId").is_none());
            assert_eq!(frame["d"]["wsAuth"]["toSign"], "GET/realtime1700000000000");
            assert!(account.link.as_ref().unwrap().resolve(
                second.socket,
                json!({"id":frame["d"]["id"],"wsAuthSignature":"07".repeat(32)})
            ));
            assert!(work.await.unwrap().is_ok());
            first.close();
            second.close();
            account.handle.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn resumed_pending_sign_request_uses_the_new_registration() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (account, first, mut first_sent) = fixture(Some("before-resume")).await;
            let work = signing(&account);
            let before = sign_request(&mut first_sent).await;
            let registry = first.registry.clone();
            let resume = first.resume_id.clone();
            first.close();
            let (mut next, mut sent) = user_session(registry, "u");
            next.identity.resume_from = Some(resume);
            next.open().await;
            next.init(init(Some("after-resume"))).await.unwrap();
            let after = sign_request(&mut sent).await;
            assert_eq!(
                after["d"]["id"], before["d"]["id"],
                "the same in-flight request resumes"
            );
            assert_eq!(after["d"]["instanceId"], "after-resume");
            assert!(account.link.as_ref().unwrap().resolve(
                next.socket,
                json!({"id":after["d"]["id"],"wsAuthSignature":"07".repeat(32)})
            ));
            assert!(work.await.unwrap().is_ok());
            next.close();
            account.handle.dispose();
        })
        .await;
}

async fn method_reply(sent: &mut tokio::sync::mpsc::Receiver<String>, request: &str) -> Value {
    for _ in 0..100 {
        while let Ok(frame) = sent.try_recv() {
            let frame: Value = serde_json::from_str(&frame).unwrap();
            if frame["d"]["op"] == "callMethod:done" && frame["d"]["data"]["requestId"] == request {
                return frame;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("missing method reply {request}");
}

#[tokio::test(start_paused = true)]
async fn deferred_read_keeps_original_registration_during_account_replacement() {
    tokio::task::LocalSet::new().run_until(async {
        let registry = crate::accounts::tests::registry();
        let link = safe_cex_remote_signer::SignerLink::new(7, Arc::new(|_|true));
        let (old, _) = crate::accounts::tests::held_load_account(link, None);
        crate::accounts::tests::register(&registry, &old);
        let (mut session, mut sent) = user_session(registry.clone(), "u");
        session.socket = 7; // The held-load fixture's existing holder.
        session.join(old.clone(),session.out.with_instance_id(Some("registration-A"))).await;
        session.handle(r#"{"t":"msg","d":{"op":"callMethod","accountName":"acct","data":{"requestId":"deferred-A","method":"fetchOrders","args":[]}}}"#).await.unwrap();
        assert_eq!(old.frame_reads.available_permits(),crate::accounts::MAX_FRAME_READS-1,
            "A's detached read is in flight before replacement");
        let replacement = crate::accounts::testing::trading_account(&registry,"u",&init(Some("registration-B")));
        session.init(init(Some("registration-B"))).await.unwrap();
        assert!(Arc::ptr_eq(&session.joined["acct"].account,&replacement));
        tokio::time::sleep(std::time::Duration::from_secs(31)).await;
        let stale = method_reply(&mut sent,"deferred-A").await;
        assert_eq!(stale["d"]["instanceId"],"registration-A","never relabel A's awaited read as B");
        session.handle(r#"{"t":"msg","d":{"op":"callMethod","accountName":"acct","data":{"requestId":"current-B","method":"fetchOrders","args":[]}}}"#).await.unwrap();
        let current = method_reply(&mut sent,"current-B").await;
        assert_eq!(current["d"]["instanceId"],"registration-B");
        assert_eq!(current["d"]["data"]["success"],true);
        session.close(); old.handle.dispose(); replacement.handle.dispose();
    }).await;
}

#[tokio::test(start_paused = true)]
async fn retained_notices_only_replay_to_the_addressed_user_with_its_new_id() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (account, first, _) = fixture(Some("old-device")).await;
            let registry = first.registry.clone();
            let keep = registry.keep_notice(&account, &first.resume_id);
            first.close();
            let mut kept: crate::algos::Kept = "retained probe".into();
            let end = envelope::msg("acct", "chaser:end", json!({"id":"retained-probe"}));
            kept.ends.push(end.clone());
            let (release, wait) = tokio::sync::oneshot::channel();
            let held = tokio::spawn(async move {
                wait.await.unwrap();
                keep(kept);
            });
            let (mut foreign, mut foreign_sent) = user_session(registry.clone(), "foreign");
            let foreign_account = crate::accounts::testing::trading_account(
                &registry,
                "foreign",
                &init(Some("foreign-device")),
            );
            foreign.init(init(Some("foreign-device"))).await.unwrap();
            release.send(()).unwrap();
            held.await.unwrap();
            ready().await;
            while let Ok(frame) = foreign_sent.try_recv() {
                assert!(!frame.contains("retained probe") && !frame.contains("retained-probe"));
            }
            let (mut next, mut sent) = user_session(registry, "u");
            next.init(init(Some("new-device"))).await.unwrap();
            for frame in [envelope::algo_notice("acct", "retained probe", None), end] {
                let reply: Value =
                    serde_json::from_str(&matching(&mut sent, &frame).await).unwrap();
                assert_eq!(reply["d"]["instanceId"], "new-device");
            }
            next.close();
            foreign.close();
            account.handle.dispose();
            foreign_account.handle.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn retained_old_account_notices_never_take_a_replacements_instance_id() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for unregister_b in [false, true] {
                let (a, first, _) = fixture(Some("registration-A")).await;
                let registry = first.registry.clone();
                let keep_a = registry.keep_notice(&a, &first.resume_id);
                let (release, wait) = tokio::sync::oneshot::channel();
                let held = tokio::spawn(async move {
                    wait.await.unwrap();
                    keep_a("private-A".into());
                });
                first.close();
                let mut b_init = init(Some("registration-B"));
                b_init.key = "synthetic-key-B".into();
                let b = crate::accounts::testing::trading_account(&registry, "u", &b_init);
                let (mut current, _) = user_session(registry.clone(), "u");
                current.init(b_init.clone()).await.unwrap();
                (registry.keep_notice(&b, &current.resume_id))("owed-B".into());
                if unregister_b {
                    current
                        .handle(r#"{"t":"dispose","d":{"accountName":"acct"}}"#)
                        .await
                        .unwrap();
                }
                release.send(()).unwrap();
                held.await.unwrap();
                let rebuilt = unregister_b
                    .then(|| crate::accounts::testing::trading_account(&registry, "u", &b_init));
                let (mut next, mut sent) = user_session(registry.clone(), "u");
                b_init.instance_id = Some("registration-C".into());
                next.init(b_init.clone()).await.unwrap();
                ready().await;
                let mut owed = 0;
                while let Ok(frame) = sent.try_recv() {
                    assert!(
                        !frame.contains("private-A"),
                        "never relabel old credentials as C: {frame}"
                    );
                    if frame.contains("owed-B") {
                        let frame: Value = serde_json::from_str(&frame).unwrap();
                        assert_eq!(frame["d"]["instanceId"], "registration-C");
                        owed += 1;
                    }
                }
                assert_eq!(
                    owed, 1,
                    "B's own notice survives unregister_b={unregister_b}"
                );
                let (mut another, mut sent) = user_session(registry, "u");
                b_init.instance_id = Some("registration-D".into());
                another.init(b_init).await.unwrap();
                ready().await;
                while let Ok(frame) = sent.try_recv() {
                    assert!(
                        !frame.contains("private-A") && !frame.contains("owed-B"),
                        "no wrong-owner or duplicate replay"
                    );
                }
                current.close();
                next.close();
                another.close();
                a.handle.dispose();
                b.handle.dispose();
                if let Some(rebuilt) = rebuilt {
                    rebuilt.handle.dispose();
                }
            }
        })
        .await;
}
