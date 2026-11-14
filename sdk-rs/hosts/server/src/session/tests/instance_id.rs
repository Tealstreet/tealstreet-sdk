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

async fn matching(sent: &mut crate::out::Receiver, expected: &str) -> String {
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

async fn fixture(id: Option<&str>) -> (Arc<Account>, Session, crate::out::Receiver) {
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

async fn sign_request(sent: &mut crate::out::Receiver) -> Value {
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

async fn method_reply(sent: &mut crate::out::Receiver, request: &str) -> Value {
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
async fn deferred_read_drops_retired_generation_without_relabelling_registration() {
    tokio::task::LocalSet::new().run_until(async {
        let registry = crate::accounts::tests::registry();
        let link = safe_cex_remote_signer::SignerLink::new(7, Arc::new(|_|true));
        let (old, _) = crate::accounts::tests::held_load_account(link, None);
        crate::accounts::tests::register(&registry, &old);
        let (mut session, mut sent) = user_session(registry.clone(), "u");
        session.socket = 7; // The held-load fixture's existing holder.
        session.join(old.clone(),session.out.account("acct").with_instance_id(Some("registration-A"))).await;
        session.handle(r#"{"t":"msg","d":{"op":"callMethod","accountName":"acct","data":{"requestId":"deferred-A","method":"fetchOrders","args":[]}}}"#).await.unwrap();
        assert_eq!(old.frame_reads.available_permits(),crate::accounts::MAX_FRAME_READS-1,
            "A's detached read is in flight before replacement");
        let replacement = crate::accounts::testing::trading_account(&registry,"u",&init(Some("registration-B")));
        session.init(init(Some("registration-B"))).await.unwrap();
        assert!(Arc::ptr_eq(&session.joined["acct"].account,&replacement));
        tokio::time::sleep(std::time::Duration::from_secs(31)).await;
        assert_eq!(old.frame_reads.available_permits(), crate::accounts::MAX_FRAME_READS,
            "the old read has settled after replacement");
        while let Ok(frame) = sent.try_recv() {
            let frame: Value = serde_json::from_str(&frame).unwrap();
            assert_ne!(frame["d"]["data"]["requestId"], "deferred-A",
                "retired output must be dropped, never stamped as B");
        }
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

async fn stopped_reply(
    session: &mut Session,
    sent: &mut crate::out::Receiver,
    op: &str,
    instance_id: Option<&str>,
) {
    session
        .handle(
            &json!({"t":"msg","d":{"accountName":"acct","op":op,
            "data":{"requestId":"stopped-probe","method":"fetchOrders","args":[]}}})
            .to_string(),
        )
        .await
        .unwrap();
    assert!(
        !session.joined.contains_key("acct"),
        "the fallback must not rejoin"
    );
    let expected = match op {
        "callMethod" => envelope::msg(
            "acct",
            "callMethod:done",
            json!({
            "requestId":"stopped-probe","success":false,"result":{
                "name":"Error","message":"this account stopped and is restarting; reload if it does not return",
                "outcome":"notSent"}}),
        ),
        "updatePositions" => envelope::msg(
            "acct",
            "updatePositions:done",
            json!({"requestId":"stopped-probe","success":true}),
        ),
        _ => envelope::error(
            Some("acct"),
            &format!(
                "{op} was not run: this account is not connected yet. Try again once it has loaded"
            ),
        ),
    };
    let frame = matching(sent, &expected).await;
    let value: Value = serde_json::from_str(&frame).unwrap();
    match instance_id.filter(|id| !id.is_empty()) {
        Some(id) => assert_eq!(value["d"]["instanceId"], id, "{op}: {frame}"),
        None => assert_eq!(frame, expected, "legacy fallback bytes: {op}"),
    }
    assert!(value.get("instanceId").is_none());
}

async fn signer_stopped_reply(op: &str) {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (account, mut session, mut sent) = fixture(Some("failed-signer")).await;
            crate::accounts::tests::fail_signer(&session.registry, &account);
            assert!(!session.registry.holds(&account, session.socket));
            stopped_reply(&mut session, &mut sent, op, Some("failed-signer")).await;
            session.close();
            account.handle.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn stopped_method_reply_keeps_instance_id_after_signer_failure() {
    signer_stopped_reply("callMethod").await;
}

#[tokio::test(start_paused = true)]
async fn stopped_update_positions_reply_keeps_instance_id_after_signer_failure() {
    signer_stopped_reply("updatePositions").await;
}

#[tokio::test(start_paused = true)]
async fn stopped_algo_refusals_keep_instance_id_after_signer_failure() {
    for op in ["chaser:start", "twap:start"] {
        signer_stopped_reply(op).await;
    }
}

#[tokio::test(start_paused = true)]
async fn stopped_replies_keep_instance_id_after_ping_forgets_the_account() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (account, mut session, mut sent) = fixture(Some("before-ping")).await;
            session.registry.dispose(&account, Disposal::Replaced);
            session.handle(r#"{"t":"ping"}"#).await.unwrap();
            assert!(!session.joined.contains_key("acct"));
            for op in [
                "callMethod",
                "updatePositions",
                "chaser:start",
                "twap:start",
            ] {
                stopped_reply(&mut session, &mut sent, op, Some("before-ping")).await;
            }
            session.close();
            account.handle.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn stopped_replies_keep_instance_id_after_another_init_forgets_the_account() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (account, mut session, mut sent) = fixture(Some("before-other-init")).await;
            session
                .registry
                .dispose(&account, Disposal::SettingsChanged);
            let mut payload = init(Some("other-registration"));
            payload.account_name = "other".into();
            let other = crate::accounts::testing::trading_account(&session.registry, "u", &payload);
            session.init(payload).await.unwrap();
            assert!(!session.joined.contains_key("acct"));
            for op in [
                "callMethod",
                "updatePositions",
                "chaser:start",
                "twap:start",
            ] {
                stopped_reply(&mut session, &mut sent, op, Some("before-other-init")).await;
            }
            session.close();
            account.handle.dispose();
            other.handle.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn stopped_replies_without_instance_ids_preserve_legacy_bytes() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for id in [None, Some("")] {
                let (account, mut session, mut sent) = fixture(id).await;
                crate::accounts::tests::fail_signer(&session.registry, &account);
                session.handle(r#"{"t":"ping"}"#).await.unwrap();
                for op in [
                    "callMethod",
                    "updatePositions",
                    "chaser:start",
                    "twap:start",
                ] {
                    stopped_reply(&mut session, &mut sent, op, id).await;
                }
                session.close();
                account.handle.dispose();
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn stopped_reply_context_is_replaced_on_reinit_and_removed_on_explicit_dispose() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (a, mut session, mut sent) = fixture(Some("registration-A")).await;
            let old_out = session.account_out("acct");
            session.registry.dispose(&a, Disposal::SettingsChanged);
            session.handle(r#"{"t":"ping"}"#).await.unwrap();
            let mut payload = init(Some("registration-B"));
            payload.key = "synthetic-key-B".into();
            let b = crate::accounts::testing::trading_account(&session.registry, "u", &payload);
            session.init(payload).await.unwrap();
            assert!(Arc::ptr_eq(&session.joined["acct"].account, &b));
            let late = envelope::error(Some("acct"), "late-A");
            assert!(
                !old_out.send(late),
                "accepted replacement retires A's output"
            );
            while let Ok(frame) = sent.try_recv() {
                assert!(
                    !frame.contains("late-A"),
                    "retired callback leaked: {frame}"
                );
            }
            session.registry.dispose(&b, Disposal::Replaced);
            stopped_reply(
                &mut session,
                &mut sent,
                "callMethod",
                Some("registration-B"),
            )
            .await;
            session
                .handle(r#"{"t":"dispose","d":{"accountName":"acct"}}"#)
                .await
                .unwrap();
            stopped_reply(&mut session, &mut sent, "callMethod", None).await;
            session.close();
            a.handle.dispose();
            b.handle.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn stopped_replies_remain_per_connection_during_deferred_disposal_and_user_switch() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (a, mut first, mut first_sent) = fixture(Some("device-A1")).await;
            let registry = first.registry.clone();
            let (mut second, mut second_sent) = user_session(registry.clone(), "u");
            second.init(init(Some("device-A2"))).await.unwrap();
            let (release, wait) = tokio::sync::oneshot::channel();
            let source = a.clone();
            let disposal_registry = registry.clone();
            let disposal = tokio::spawn(async move {
                wait.await.unwrap();
                crate::accounts::tests::fail_signer(&disposal_registry, &source);
            });
            let (mut foreign, mut foreign_sent) = user_session(registry.clone(), "foreign");
            let foreign_account = crate::accounts::testing::trading_account(
                &registry, "foreign", &init(Some("foreign-B")));
            foreign.init(init(Some("foreign-B"))).await.unwrap();
            let mut replacement = init(Some("replacement-B"));
            replacement.key = "synthetic-key-B".into();
            let b = crate::accounts::testing::trading_account(&registry, "u", &replacement);
            let (mut current, mut current_sent) = user_session(registry.clone(), "u");
            current.init(replacement).await.unwrap();
            while foreign_sent.try_recv().is_ok() {}
            while current_sent.try_recv().is_ok() {}
            release.send(()).unwrap();
            disposal.await.unwrap();
            assert!(registry.holds(&b, current.socket), "late A disposal cannot remove B");
            assert!(registry.holds(&foreign_account, foreign.socket));
            for sent in [&mut foreign_sent, &mut current_sent] {
                while let Ok(frame) = sent.try_recv() {
                    assert!(!frame.contains("Remote signing failed repeatedly"));
                }
            }
            first.handle(r#"{"t":"ping"}"#).await.unwrap();
            second.handle(r#"{"t":"ping"}"#).await.unwrap();
            for op in ["callMethod", "updatePositions", "chaser:start", "twap:start"] {
                stopped_reply(&mut first, &mut first_sent, op, Some("device-A1")).await;
                stopped_reply(&mut second, &mut second_sent, op, Some("device-A2")).await;
            }
            for (session, sent, id) in [
                (&mut current, &mut current_sent, "replacement-B"),
                (&mut foreign, &mut foreign_sent, "foreign-B"),
            ] {
                session.handle(r#"{"t":"msg","d":{"op":"callMethod","accountName":"acct","data":{"requestId":"live-control","method":"fetchOrders","args":[]}}}"#).await.unwrap();
                let frame = method_reply(sent, "live-control").await;
                assert_eq!(frame["d"]["instanceId"], id);
                assert_eq!(frame["d"]["data"]["success"], true);
            }
            first.close(); second.close(); current.close(); foreign.close();
            a.handle.dispose(); b.handle.dispose(); foreign_account.handle.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn reviewer_fresh_registration_receives_its_own_rebuild_refusal() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (account, owner, _) = fixture(Some("existing-owner")).await;
            let (mut attempt, mut sent) = user_session(owner.registry.clone(), "u");
            let mut payload = init(Some("registration-A"));
            payload.key = "synthetic-different-credentials".into();
            attempt.init(payload.clone()).await.unwrap();
            let mut replies = Vec::new();
            while let Ok(frame) = sent.try_recv() {
                replies.push(serde_json::from_str::<Value>(&frame).unwrap());
            }
            assert!(
                replies
                    .iter()
                    .any(|v| v["d"]["op"] == "error" && v["d"]["instanceId"] == "registration-A"),
                "first refusal control: {replies:?}"
            );
            payload.instance_id = Some("registration-B".into());
            attempt.init(payload).await.unwrap();
            let mut replies = Vec::new();
            while let Ok(frame) = sent.try_recv() {
                replies.push(serde_json::from_str::<Value>(&frame).unwrap());
            }
            assert!(
                replies
                    .iter()
                    .any(|v| v["d"]["op"] == "error" && v["d"]["instanceId"] == "registration-B"),
                "fresh registration must receive its own refusal: {replies:?}"
            );
            attempt.close();
            owner.close();
            account.handle.dispose();
        })
        .await;
}

async fn refused_registration(
    attempt: &mut Session,
    sent: &mut crate::out::Receiver,
    payload: &InitPayload,
    notify: bool,
) {
    let reason = attempt
        .registry
        .rebuild_refusal("u", payload, false, attempt.socket)
        .expect("another live holder keeps its different settings");
    let mut expected = envelope::init_refused("acct", &reason);
    let id = payload.instance_id.as_deref().filter(|id| !id.is_empty());
    if let Some(id) = id {
        expected = envelope::with_instance_id(expected, id);
    }
    attempt.init(payload.clone()).await.unwrap();
    let replies: Vec<_> = std::iter::from_fn(|| sent.try_recv().ok()).collect();
    if notify {
        assert_eq!(
            replies,
            vec![expected],
            "one stamped refusal per registration click"
        );
    } else {
        assert!(
            replies.is_empty(),
            "same registration retry must be silent: {replies:?}"
        );
    }
    assert!(
        !attempt.joined.contains_key("acct"),
        "a refusal must never join"
    );
}

#[tokio::test(start_paused = true)]
async fn same_registration_rebuild_refusal_retries_stay_silent() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (account, owner, _) = fixture(Some("owner")).await;
            let (mut attempt, mut sent) = user_session(owner.registry.clone(), "u");
            let mut payload = init(Some("one-click"));
            payload.key = "synthetic-other-key".into();
            refused_registration(&mut attempt, &mut sent, &payload, true).await;
            for _ in 0..4 {
                refused_registration(&mut attempt, &mut sent, &payload, false).await;
            }
            assert!(owner.registry.holds(&account, owner.socket));
            attempt.close();
            owner.close();
            account.handle.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn interleaved_refused_registrations_each_receive_exactly_one_error() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (account, owner, _) = fixture(Some("owner")).await;
            let (mut attempt, mut sent) = user_session(owner.registry.clone(), "u");
            let mut payload = init(None);
            payload.key = "synthetic-other-key".into();
            for (id, notify) in [
                (None, true),
                (Some("A"), true),
                (Some("A"), false),
                (Some("B"), true),
                (Some("B"), false),
                (Some("A"), false),
                (None, false),
                (Some(""), false),
                (Some("B"), false),
            ] {
                payload.instance_id = id.map(str::to_owned);
                refused_registration(&mut attempt, &mut sent, &payload, notify).await;
            }
            assert!(owner.registry.holds(&account, owner.socket));
            assert!(Arc::ptr_eq(&account, &owner.joined["acct"].account));
            attempt.close();
            owner.close();
            account.handle.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn legacy_rebuild_refusal_suppression_and_changed_reasons_are_byte_exact() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (account, owner, _) = fixture(None).await;
            let (mut attempt, mut sent) = user_session(owner.registry.clone(), "u");
            let mut payload = init(None);
            payload.key = "synthetic-other-key".into();
            refused_registration(&mut attempt, &mut sent, &payload, true).await;
            refused_registration(&mut attempt, &mut sent, &payload, false).await;
            payload.testnet = Some(true);
            refused_registration(&mut attempt, &mut sent, &payload, true).await;
            refused_registration(&mut attempt, &mut sent, &payload, false).await;
            payload.testnet = None;
            refused_registration(&mut attempt, &mut sent, &payload, true).await;
            payload.instance_id = Some("".into());
            refused_registration(&mut attempt, &mut sent, &payload, false).await;
            assert!(owner.registry.holds(&account, owner.socket));
            attempt.close();
            owner.close();
            account.handle.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn reviewer_refused_fresh_registration_does_not_reuse_a_stopped_registrations_id() {
    tokio::task::LocalSet::new().run_until(async {
        let (a, mut original, mut sent) = fixture(Some("old-registration-A")).await;
        let registry = original.registry.clone();
        let captured_a = original.account_out("acct");
        crate::accounts::tests::fail_signer(&registry, &a);
        original.handle(r#"{"t":"ping"}"#).await.unwrap();
        assert!(!original.joined.contains_key("acct"));
        let mut replacement = init(Some("replacement-B"));
        replacement.key = "synthetic-key-B".into();
        let b = crate::accounts::testing::trading_account(&registry, "u", &replacement);
        let (mut other, mut other_sent) = user_session(registry, "u");
        other.init(replacement).await.unwrap();
        while sent.try_recv().is_ok() {}
        let fresh = init(Some("fresh-registration-C"));
        refused_registration(&mut original, &mut sent, &fresh, true).await;
        for op in ["callMethod", "updatePositions", "chaser:start", "twap:start"] {
            stopped_reply(&mut original, &mut sent, op, Some("fresh-registration-C")).await;
        }
        refused_registration(&mut original, &mut sent, &fresh, false).await;
        stopped_reply(&mut original, &mut sent, "callMethod", Some("fresh-registration-C")).await;
        let old_reply = envelope::error(Some("acct"), "already-captured-A");
        captured_a.send(old_reply.clone());
        let frame: Value = serde_json::from_str(&matching(&mut sent, &old_reply).await).unwrap();
        assert_eq!(frame["d"]["instanceId"], "old-registration-A");
        assert!(other.registry.holds(&b, other.socket));
        other.handle(r#"{"t":"msg","d":{"op":"callMethod","accountName":"acct","data":{"requestId":"live-B","method":"fetchOrders","args":[]}}}"#).await.unwrap();
        let frame = method_reply(&mut other_sent, "live-B").await;
        assert_eq!(frame["d"]["data"]["success"], true);
        assert_eq!(frame["d"]["instanceId"], "replacement-B");
        original.close(); other.close(); a.handle.dispose(); b.handle.dispose();
    }).await;
}

#[tokio::test(start_paused = true)]
async fn unsupported_fresh_registration_replaces_stopped_fallback_provenance() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (account, mut session, mut sent) = fixture(Some("old-A")).await;
            crate::accounts::tests::fail_signer(&session.registry, &account);
            session.handle(r#"{"t":"ping"}"#).await.unwrap();
            let mut payload = init(Some("unsupported-C"));
            payload.exchange_name = "bitmex".into();
            session.init(payload).await.unwrap();
            let frame: Value = serde_json::from_str(
                &matching(&mut sent, &envelope::init_unsupported("acct", "bitmex")).await,
            )
            .unwrap();
            assert_eq!(frame["d"]["instanceId"], "unsupported-C");
            for op in [
                "callMethod",
                "updatePositions",
                "chaser:start",
                "twap:start",
            ] {
                stopped_reply(&mut session, &mut sent, op, Some("unsupported-C")).await;
            }
            session.close();
            account.handle.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn refused_registration_without_id_removes_the_old_stopped_reply_id() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for id in [None, Some("")] {
                let (a, mut session, mut sent) = fixture(Some("old-A")).await;
                let registry = session.registry.clone();
                crate::accounts::tests::fail_signer(&registry, &a);
                session.handle(r#"{"t":"ping"}"#).await.unwrap();
                let mut replacement = init(Some("replacement-B"));
                replacement.key = "synthetic-key-B".into();
                let b = crate::accounts::testing::trading_account(&registry, "u", &replacement);
                let (mut other, _) = user_session(registry, "u");
                other.init(replacement).await.unwrap();
                while sent.try_recv().is_ok() {}
                refused_registration(&mut session, &mut sent, &init(id), true).await;
                for op in [
                    "callMethod",
                    "updatePositions",
                    "chaser:start",
                    "twap:start",
                ] {
                    stopped_reply(&mut session, &mut sent, op, id).await;
                }
                session.close();
                other.close();
                a.handle.dispose();
                b.handle.dispose();
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn disposal_after_a_refused_registration_cannot_restore_the_old_reply_id() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (account, mut original, mut sent) = fixture(Some("old-A")).await;
            let (mut other, _) = user_session(original.registry.clone(), "u");
            other.init(init(Some("holder-B"))).await.unwrap();
            let mut fresh = init(Some("fresh-C"));
            fresh.key = "synthetic-other-key".into();
            let expected = envelope::init_refused(
                "acct",
                &original
                    .registry
                    .rebuild_refusal("u", &fresh, false, original.socket)
                    .unwrap(),
            );
            original.init(fresh).await.unwrap();
            let refusal: Value =
                serde_json::from_str(&matching(&mut sent, &expected).await).unwrap();
            assert_eq!(refusal["d"]["instanceId"], "fresh-C");
            crate::accounts::tests::fail_signer(&original.registry, &account);
            original.handle(r#"{"t":"ping"}"#).await.unwrap();
            for op in [
                "callMethod",
                "updatePositions",
                "chaser:start",
                "twap:start",
            ] {
                stopped_reply(&mut original, &mut sent, op, Some("fresh-C")).await;
            }
            original.close();
            other.close();
            account.handle.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn orphaned_generation_drops_old_output_but_fresh_stopped_reply_keeps_registration() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (account, mut session, mut sent) = fixture(Some("orphan-registration")).await;
            let previous = session.account_out("acct");
            while sent.try_recv().is_ok() {}
            assert!(previous.send(envelope::error(Some("acct"), "queued-before-orphan")));
            session
                .handle(r#"{"t":"ping","d":{"accounts":["other"]}}"#)
                .await
                .unwrap();
            assert!(!session.joined.contains_key("acct"));
            assert!(!previous.send(envelope::error(Some("acct"), "late-after-orphan")));
            while let Ok(frame) = sent.try_recv() {
                assert!(
                    !frame.contains("queued-before-orphan") && !frame.contains("late-after-orphan")
                );
            }
            stopped_reply(
                &mut session,
                &mut sent,
                "callMethod",
                Some("orphan-registration"),
            )
            .await;
            session.close();
            account.handle.dispose();
        })
        .await;
}

async fn delegated_mode_refusal_reply(
    joined: bool,
    previous_id: Option<&str>,
    requested_id: Option<&str>,
) {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let mut payload = init(requested_id);
            payload.exchange_name = "bybit".into();
            let pair = || {
                crate::delegated::DelegatedPair::new(
                    payload.key.clone(),
                    "synthetic-registration-fixture".into(),
                    None,
                )
            };
            let prepared = crate::delegated::prepare_delegated(&payload, pair())
                .unwrap_or_else(|_| panic!("synthetic fixture prepares"));
            let (mut account, _inbox) = crate::accounts::testing::account_on(
                crate::accounts::testing::HeldLoad {
                    placed: Arc::default(),
                    load_after_ms: Some(0.),
                    ticker: true,
                },
                ("u", "acct", "bybit"),
                Some(safe_cex_remote_signer::SignerLink::new(
                    0,
                    Arc::new(|_| true),
                )),
                crate::delegated::fingerprint(&payload),
                Arc::default(),
            );
            crate::accounts::testing::bind_identity(&mut account, &payload);
            let inner = Arc::get_mut(&mut account).unwrap();
            inner.link = None;
            inner.delegated = Some(prepared.resident);
            crate::accounts::testing::register(&registry, &account);
            let (mut session, mut sent) = user_session(registry.clone(), "u");
            if joined {
                let previous = session.out.with_instance_id(previous_id);
                let held = registry
                    .obtain_delegated(
                        "u",
                        &payload,
                        session.socket,
                        None,
                        previous.outbox(),
                        pair(),
                    )
                    .await
                    .unwrap_or_else(|_| panic!("same-mode hold succeeds"));
                session.join(held, previous).await;
                assert!(session.joined.contains_key("acct"));
                ready().await;
                while sent.try_recv().is_ok() {}
            }
            session.init(payload).await.unwrap();
            let expected = envelope::init_refused(
                "acct",
                "Account signing mode differs; account not attached",
            );
            let actual = matching(&mut sent, &expected).await;
            let expected = match requested_id.filter(|id| !id.is_empty()) {
                Some(id) => envelope::with_instance_id(expected, id),
                None => expected,
            };
            session.close();
            registry.dispose(&account, Disposal::PageDisposed);
            assert_eq!(
                actual, expected,
                "mode refusal belongs to the current registration, including legacy bytes"
            );
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn delegated_mode_refusal_uses_the_requested_registration_after_join() {
    delegated_mode_refusal_reply(
        true,
        Some("prior-registration"),
        Some("current-registration"),
    )
    .await;
}

#[tokio::test(start_paused = true)]
async fn delegated_mode_refusal_preserves_no_id_bytes_after_a_tagged_join() {
    delegated_mode_refusal_reply(true, Some("prior-registration"), None).await;
}

#[tokio::test(start_paused = true)]
async fn delegated_mode_refusal_uses_a_fresh_registration() {
    delegated_mode_refusal_reply(false, None, Some("current-registration")).await;
}

#[tokio::test(start_paused = true)]
async fn delegated_mode_refusal_preserves_legacy_bytes() {
    delegated_mode_refusal_reply(true, None, None).await;
}

#[tokio::test(start_paused = true)]
async fn delegated_mode_refusal_preserves_empty_id_bytes_after_a_tagged_join() {
    delegated_mode_refusal_reply(true, Some("prior-registration"), Some("")).await;
}

#[tokio::test(start_paused = true)]
async fn delegated_mode_refusal_preserves_fresh_legacy_bytes() {
    delegated_mode_refusal_reply(false, None, None).await;
}

async fn queued_mode_refusal_retired_queue(
    previous_id: Option<&str>,
    requested_id: Option<&str>,
    switch_user: bool,
) {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let payload = InitPayload {
                exchange_name: "bybit".into(),
                ..init(requested_id)
            };
            let pair = || {
                crate::delegated::DelegatedPair::new(
                    payload.key.clone(),
                    "synthetic-registration-fixture".into(),
                    None,
                )
            };
            let prepared = crate::delegated::prepare_delegated(&payload, pair())
                .unwrap_or_else(|_| panic!("synthetic fixture prepares"));
            let (mut account, _inbox) = crate::accounts::testing::account_on(
                crate::accounts::testing::HeldLoad {
                    placed: Arc::default(),
                    load_after_ms: Some(0.),
                    ticker: true,
                },
                ("u", "acct", "bybit"),
                Some(safe_cex_remote_signer::SignerLink::new(
                    0,
                    Arc::new(|_| true),
                )),
                crate::delegated::fingerprint(&payload),
                Arc::default(),
            );
            let inner = Arc::get_mut(&mut account).unwrap();
            inner.link = None;
            inner.delegated = Some(prepared.resident);
            crate::accounts::testing::bind_identity(&mut account, &payload);
            crate::accounts::testing::register(&registry, &account);
            let (mut session, mut sent) = user_session(registry.clone(), "u");
            {
                let previous = session.out.account("acct").with_instance_id(previous_id);
                let held = registry
                    .obtain_delegated(
                        "u",
                        &payload,
                        session.socket,
                        None,
                        previous.outbox(),
                        pair(),
                    )
                    .await
                    .unwrap_or_else(|_| panic!("same-mode hold succeeds"));
                session.join(held, previous).await;
                assert!(session.joined.contains_key("acct"));
                ready().await;
                while sent.try_recv().is_ok() {}
            }
            session.init(payload).await.unwrap();
            let stale = envelope::init_refused(
                "acct",
                "Account signing mode differs; account not attached",
            );
            let switched = switch_user.then(|| user_session(registry.clone(), "v"));
            session
                .handle(r#"{"t":"dispose","d":{"accountName":"acct"}}"#)
                .await
                .unwrap();
            let mut retired_sent = None;
            let (mut session, mut sent) = if let Some(next) = switched {
                session.close();
                retired_sent = Some(sent);
                next
            } else {
                (session, sent)
            };
            let user = if switch_user { "v" } else { "u" };
            let replacement_id = requested_id.map(|_| "replacement-B");
            let replacement = InitPayload {
                exchange_name: "bybit".into(),
                ..init(replacement_id)
            };
            let current =
                crate::accounts::testing::trading_account(&registry, user, &replacement);
            session.init(replacement).await.unwrap();
            assert!(Arc::ptr_eq(&session.joined["acct"].account, &current));
            let control = envelope::error(Some("acct"), "current-generation-control");
            current.notify(control.clone());
            let mut delivered = false;
            while let Ok(frame) = sent.try_recv() {
                assert_ne!(
                    without_id(&frame), without_id(&stale),
                    "mode refusal queued before dispose must not reach the replacement account generation: {frame}",
                );
                if without_id(&frame) == without_id(&control) {
                    let value: Value = serde_json::from_str(&frame).unwrap();
                    assert_eq!(value["d"]["instanceId"].as_str(), replacement_id);
                    delivered = true;
                }
            }
            assert!(delivered, "current generation delivery control");
            if let Some(mut retired) = retired_sent {
                while let Ok(frame) = retired.try_recv() {
                    assert_ne!(
                        without_id(&frame), without_id(&stale),
                        "the previous user's queued refusal must retire during login switch: {frame}",
                    );
                    assert_ne!(
                        without_id(&frame), without_id(&control),
                        "the replacement user's output must stay on its socket: {frame}",
                    );
                }
            }
            session.close();
            registry.dispose(&current, Disposal::PageDisposed);
            registry.dispose(&account, Disposal::PageDisposed);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn queued_mode_refusal_retires_before_legacy_replacement() {
    queued_mode_refusal_retired_queue(None, None, false).await;
}

#[tokio::test(start_paused = true)]
async fn queued_mode_refusal_retires_before_tagged_replacement() {
    queued_mode_refusal_retired_queue(Some("prior"), Some("refused-current"), false).await;
}

#[tokio::test(start_paused = true)]
async fn queued_mode_refusal_retires_during_user_switch() {
    queued_mode_refusal_retired_queue(Some("prior"), Some("refused-current"), true).await;
}
