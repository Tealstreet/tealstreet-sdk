use super::*;

#[tokio::test]
async fn runtime_order_frames_are_off_for_an_admitted_session_by_default() {
    const EXPECT: &str = "TEALSTREET_TEST_ORDER_SESSION_ENABLED";
    if std::env::var(EXPECT).is_err() {
        for value in ["0", "1"] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "session::delegated_writes_tests::order_frames::runtime_order_frames_are_off_for_an_admitted_session_by_default",
                    "--nocapture",
                ])
                .env("TEALSTREET_DELEGATED_ORDER_FRAMES", value)
                .env(EXPECT, value)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "runtime session input {value:?}:\n{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );
            assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
        }
        return;
    }
    let enabled = std::env::var(EXPECT).as_deref() == Ok("1");
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let tape = Tape::default();
            let account = delegated_account(&registry, "A", "bybit", tape.clone());
            let (mut session, mut sent) =
                joined_without_runtime_opt_in(registry.clone(), "A", account.clone()).await;
            submit(&mut session, 0, None).await;
            let ack = order_frame(&mut sent, "order.ack").await;
            assert_eq!(ack["d"]["received"], enabled);
            if enabled {
                let outcome = order_frame(&mut sent, "order.outcome").await;
                assert_eq!(outcome["d"]["outcome"], "placed");
                assert_eq!(*tape.writes.lock().unwrap(), vec!["create"]);
            } else {
                assert!(tape.writes.lock().unwrap().is_empty());
            }
            session.close();
            registry.dispose(&account, Disposal::PageDisposed);
        })
        .await;
}

fn intent(age: u64) -> String {
    let epoch = crate::accounts::now_ms().saturating_sub(age);
    let hex = format!("{epoch:012x}");
    format!("{}-{}-7101-8101-010101010101", &hex[..8], &hex[8..])
}

async fn submit(session: &mut Session, age: u64, registration: Option<&str>) -> String {
    let id = intent(age);
    let params = placement();
    let fingerprint =
        crate::order_contract::fingerprint("synthetic-main", &id, &params, 1).unwrap();
    session.handle(&json!({"t":"order.submit","d":{"accountName":"synthetic-main","instanceId":registration,
        "intentId":id,"params":params,"seq":1,"ageMs":age,"fingerprint":fingerprint}}).to_string()).await.unwrap();
    id
}

async fn order_frame(sent: &mut crate::out::Receiver, op: &str) -> Value {
    tokio::time::timeout(Duration::from_secs(2), async {
        while let Some(text) = sent.recv().await {
            let frame: Value = serde_json::from_str(&text).unwrap();
            if frame["t"] == op {
                return frame;
            }
        }
        panic!("missing order frame")
    })
    .await
    .expect("bounded order reply")
}

#[tokio::test]
async fn order_frames_use_the_existing_native_call_and_registration() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for id in [Some("current-registration"), None] {
                let registry = crate::accounts::tests::registry();
                let tape = Tape::default();
                let account = delegated_account(&registry, "A", "bybit", tape.clone());
                let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
                session.joined.get_mut("synthetic-main").unwrap().out =
                    session.out.with_instance_id(id);
                assert!(session.resnapshot("synthetic-main").await);
                let key = submit(&mut session, 0, id).await;
                {
                    let pending = session.joined["synthetic-main"]
                        .client
                        .pending
                        .lock()
                        .unwrap();
                    let command = pending
                        .values()
                        .next()
                        .expect("existing call pipeline records command");
                    assert_eq!(command.request_id, key);
                    assert!(command.audit.is_some());
                }
                let ack = order_frame(&mut sent, "order.ack").await;
                assert_eq!(ack["d"]["intentId"], key);
                assert_eq!(ack["d"]["received"], true);
                assert_eq!(ack["d"]["seq"], 1);
                let outcome = order_frame(&mut sent, "order.outcome").await;
                assert_eq!(outcome["d"]["intentId"], key);
                assert_eq!(outcome["d"]["outcome"], "placed");
                assert_eq!(outcome["d"]["venueOrderId"], "native-one");
                assert_eq!(outcome["d"]["clientOrderId"], key);
                for frame in [ack, outcome] {
                    assert_eq!(frame["d"]["instanceId"].as_str(), id);
                }
                assert_eq!(*tape.writes.lock().unwrap(), vec!["create"]);
                assert_eq!(*tape.client_order_ids.lock().unwrap(), vec![key]);
                session.close();
                registry.dispose(&account, Disposal::PageDisposed);
            }
        })
        .await;
}

#[tokio::test]
async fn order_frames_refuse_expired_or_unjoined_submits_without_a_write() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let tape = Tape::default();
            let account = delegated_account(&registry, "A", "bybit", tape.clone());
            let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
            submit(&mut session, 10_000, None).await;
            let refused = order_frame(&mut sent, "order.outcome").await;
            assert_eq!(refused["d"]["outcome"], "refused");
            assert_eq!(refused["d"]["error"]["outcome"], "notSent");
            assert!(tape.writes.lock().unwrap().is_empty());
            session.close();
            registry.dispose(&account, Disposal::PageDisposed);
        })
        .await;
}

#[tokio::test]
async fn order_frames_refuse_pending_missing_modes_and_mismatched_client_ids() {
    tokio::task::LocalSet::new().run_until(async {
        for case in ["pending", "missing", "resume", "flag", "client-id", "intent", "sequence", "binding", "fingerprint", "key-age"] {
            let registry = crate::accounts::tests::registry();
            let tape = Tape::default();
            let venue = if case == "flag" { "hyrotrader" } else { "bybit" };
            let account = delegated_account(&registry, "A", venue, tape.clone());
            let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
            if case == "pending" {
                session.registration_outputs.insert("synthetic-main".into(), session.out.with_instance_id(Some("pending-registration")));
                session.start_verification("synthetic-main".into(), std::future::pending());
            }
            if case == "missing" {
                let old = session.joined.remove("synthetic-main").unwrap();
                Session::leave_engine(old);
            }
            if case == "resume" { session.joined.get_mut("synthetic-main").unwrap().out = session.out.awaiting_instance_id(); }
            let key = intent(0);
            let mut data = json!({"intentId":key,"params":placement(),"seq":1,"ageMs":0,"accountName":"synthetic-main","instanceId":if case == "pending" {Some("pending-registration")} else {None},"fingerprint":crate::order_contract::fingerprint("synthetic-main",&key,&placement(),1).unwrap()});
            if case == "client-id" { data["params"]["clientOrderId"] = "another-client-id".into(); }
            if case == "intent" { data["intentId"] = "invalid".into(); }
            if case == "sequence" { data["seq"] = 0.into(); }
            if case == "binding" { data["instanceId"] = "another-registration".into(); }
            if case == "fingerprint" { data["params"]["amount"] = json!(0.002); }
            if case == "key-age" {
                let old = intent(10_001);
                data["intentId"] = old.clone().into();
                data["fingerprint"] = crate::order_contract::fingerprint("synthetic-main", &old, &placement(), 1).unwrap().into();
                data["ageMs"] = 0.into();
            }
            session.handle(&json!({"t":"order.submit","d":data}).to_string()).await.unwrap();
            let ack = order_frame(&mut sent, "order.ack").await;
            assert_eq!(ack["d"]["received"], false);
            let refusal = order_frame(&mut sent, "order.outcome").await;
            assert_eq!(refusal["d"]["error"]["outcome"], "notSent");
            if case == "key-age" { assert_eq!(refusal["d"]["error"]["message"], "not sent: expired"); }
            if case == "pending" { assert_eq!(refusal["d"]["instanceId"], "pending-registration"); }
            assert!(tape.writes.lock().unwrap().is_empty());
            session.close();
            registry.dispose(&account, Disposal::PageDisposed);
        }
    }).await;
}

#[tokio::test]
async fn order_frames_delayed_unknown_remains_on_its_original_users_socket() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let a_tape = Tape {
                hold: true,
                ..Default::default()
            };
            let a = delegated_account(&registry, "A", "bybit", a_tape.clone());
            let (mut alice, mut a_sent) = joined(registry.clone(), "A", a.clone()).await;
            alice.joined.get_mut("synthetic-main").unwrap().out =
                alice.out.with_instance_id(Some("alice"));
            assert!(alice.resnapshot("synthetic-main").await);
            submit(&mut alice, 0, Some("alice")).await;
            let _ = order_frame(&mut a_sent, "order.ack").await;
            tokio::time::timeout(Duration::from_secs(2), async {
                while a_tape.waiting.lock().unwrap().is_empty() {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            })
            .await
            .unwrap();
            let b_tape = Tape::default();
            let b = delegated_account(&registry, "B", "bybit", b_tape.clone());
            let (mut bob, mut b_sent) = joined(registry.clone(), "B", b.clone()).await;
            bob.joined.get_mut("synthetic-main").unwrap().out =
                bob.out.with_instance_id(Some("bob"));
            assert!(bob.resnapshot("synthetic-main").await);
            a_tape.finish_unknown();
            let unknown = order_frame(&mut a_sent, "order.outcome").await;
            assert_eq!(unknown["d"]["outcome"], "unknown");
            assert_eq!(unknown["d"]["instanceId"], "alice");
            submit(&mut bob, 0, Some("bob")).await;
            assert_eq!(
                order_frame(&mut b_sent, "order.ack").await["d"]["instanceId"],
                "bob"
            );
            let placed = order_frame(&mut b_sent, "order.outcome").await;
            assert_eq!(placed["d"]["instanceId"], "bob");
            assert_eq!(placed["d"]["outcome"], "placed");
            assert_eq!(*a_tape.writes.lock().unwrap(), vec!["create"]);
            assert_eq!(*b_tape.writes.lock().unwrap(), vec!["create"]);
            alice.close();
            bob.close();
            registry.dispose(&a, Disposal::PageDisposed);
            registry.dispose(&b, Disposal::PageDisposed);
        })
        .await;
}

#[tokio::test]
async fn order_frames_retired_registration_drops_its_deferred_completion() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let a_tape = Tape {
                hold: true,
                ..Default::default()
            };
            let a = delegated_account(&registry, "A", "bybit", a_tape.clone());
            let (mut alice, mut a_sent) = joined(registry.clone(), "A", a.clone()).await;
            alice.joined.get_mut("synthetic-main").unwrap().out =
                alice.out.with_instance_id(Some("alice-old"));
            assert!(alice.resnapshot("synthetic-main").await);
            submit(&mut alice, 0, Some("alice-old")).await;
            order_frame(&mut a_sent, "order.ack").await;
            let pending = alice.joined["synthetic-main"].client.pending.clone();
            tokio::time::timeout(Duration::from_secs(2), async {
                while a_tape.waiting.lock().unwrap().is_empty() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            let replacement = alice.out.with_instance_id(Some("alice-new"));
            let previous = std::mem::replace(
                &mut alice.joined.get_mut("synthetic-main").unwrap().out,
                replacement,
            );
            previous.retire_registration();
            assert!(alice.resnapshot("synthetic-main").await);
            let b_tape = Tape::default();
            let b = delegated_account(&registry, "B", "bybit", b_tape.clone());
            let (mut bob, mut b_sent) = joined(registry.clone(), "B", b.clone()).await;
            bob.joined.get_mut("synthetic-main").unwrap().out =
                bob.out.with_instance_id(Some("bob"));
            assert!(bob.resnapshot("synthetic-main").await);
            a_tape.finish_unknown();
            tokio::time::timeout(Duration::from_secs(2), async {
                while !pending.lock().unwrap().is_empty() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            let b_id = submit(&mut bob, 0, Some("bob")).await;
            order_frame(&mut b_sent, "order.ack").await;
            let placed = order_frame(&mut b_sent, "order.outcome").await;
            assert_eq!(placed["d"]["instanceId"], "bob");
            assert_eq!(placed["d"]["intentId"], b_id);
            assert_eq!(placed["d"]["outcome"], "placed");
            while let Ok(text) = a_sent.try_recv() {
                let frame: Value = serde_json::from_str(&text).unwrap();
                assert_ne!(
                    frame["t"], "order.outcome",
                    "retired registration has no late receipt"
                );
            }
            while let Ok(text) = b_sent.try_recv() {
                let frame: Value = serde_json::from_str(&text).unwrap();
                assert_ne!(frame["t"], "order.outcome", "B has only its own completion");
            }
            assert_eq!(*a_tape.writes.lock().unwrap(), vec!["create"]);
            assert_eq!(*b_tape.writes.lock().unwrap(), vec!["create"]);
            alice.close();
            bob.close();
            registry.dispose(&a, Disposal::PageDisposed);
            registry.dispose(&b, Disposal::PageDisposed);
        })
        .await;
}

#[tokio::test]
async fn order_frames_cancel_queued_never_cancels_a_venue_order() {
    tokio::task::LocalSet::new().run_until(async {
        let registry = crate::accounts::tests::registry();
        let tape = Tape::default();
        let account = delegated_account(&registry, "A", "bybit", tape.clone());
        let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
        session.handle(&json!({"t":"order.cancelQueued","d":{"accountName":"synthetic-main","intentId":intent(0),"seq":1,"fingerprint":"a".repeat(64)}}).to_string()).await.unwrap();
        assert_eq!(order_frame(&mut sent, "order.outcome").await["d"]["outcome"], "unknown");
        assert!(tape.writes.lock().unwrap().is_empty());
        session.close();
        registry.dispose(&account, Disposal::PageDisposed);
    }).await;
}

#[tokio::test]
async fn order_frames_successful_reverification_drops_previous_deferred_completion() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let a_tape = Tape {
                hold: true,
                ..Default::default()
            };
            let a = delegated_account(&registry, "A", "bybit", a_tape.clone());
            let (mut alice, mut a_sent) = joined(registry.clone(), "A", a.clone()).await;
            // Re-verification uses the authenticated admission boundary.
            alice.identity.token = Some(("synthetic-valid-token".into(), u64::MAX));
            alice.joined.get_mut("synthetic-main").unwrap().out =
                alice.out.with_instance_id(Some("alice-old"));
            assert!(alice.resnapshot("synthetic-main").await);
            submit(&mut alice, 0, Some("alice-old")).await;
            order_frame(&mut a_sent, "order.ack").await;
            let pending = alice.joined["synthetic-main"].client.pending.clone();
            tokio::time::timeout(Duration::from_secs(2), async {
                while a_tape.waiting.lock().unwrap().is_empty() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            let mut admitted = init("bybit");
            admitted.key = "synthetic-key-A".into();
            admitted.instance_id = Some("alice-new".into());
            admitted.signing_mode = Some("delegated".into());
            assert_eq!(crate::delegated::fingerprint(&admitted), a.fingerprint);
            let permit = registry.probe_permit("A", &admitted).unwrap();
            let candidate: crate::delegated::Credentials = serde_json::from_str(
                r#"{"key":"synthetic-key-A","secret":"synthetic-signing-fixture-A"}"#,
            )
            .unwrap();
            alice.registration_outputs.insert(
                "synthetic-main".into(),
                alice.out.with_instance_id(Some("alice-new")),
            );
            let mut verified = alice.take_verified();
            alice.start_probe_verification("synthetic-main".into(), None, async move {
                (
                    Ok(candidate),
                    Some(ProbedInit {
                        diagnostic: None,
                        init: admitted,
                        permit: Ok(permit),
                    }),
                )
            });
            let completion = tokio::time::timeout(Duration::from_secs(2), verified.recv())
                .await
                .unwrap()
                .unwrap();
            alice.verification_completed(completion).await.unwrap();
            assert!(Arc::ptr_eq(&alice.joined["synthetic-main"].account, &a));
            assert!(
                alice.joined["synthetic-main"]
                    .out
                    .order_binding(Some("alice-new"))
            );
            let b_tape = Tape::default();
            let b = delegated_account(&registry, "B", "bybit", b_tape.clone());
            let (mut bob, mut b_sent) = joined(registry.clone(), "B", b.clone()).await;
            bob.joined.get_mut("synthetic-main").unwrap().out =
                bob.out.with_instance_id(Some("bob"));
            assert!(bob.resnapshot("synthetic-main").await);
            a_tape.finish_unknown();
            tokio::time::timeout(Duration::from_secs(2), async {
                while !pending.lock().unwrap().is_empty() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            let b_id = submit(&mut bob, 0, Some("bob")).await;
            order_frame(&mut b_sent, "order.ack").await;
            let placed = order_frame(&mut b_sent, "order.outcome").await;
            assert_eq!(placed["d"]["instanceId"], "bob");
            assert_eq!(placed["d"]["intentId"], b_id);
            assert_eq!(placed["d"]["outcome"], "placed");
            while let Ok(text) = a_sent.try_recv() {
                let frame: Value = serde_json::from_str(&text).unwrap();
                assert_ne!(
                    frame["t"], "order.outcome",
                    "retired registration has no late receipt"
                );
            }
            while let Ok(text) = b_sent.try_recv() {
                let frame: Value = serde_json::from_str(&text).unwrap();
                assert_ne!(frame["t"], "order.outcome", "B has only its own completion");
            }
            assert_eq!(*a_tape.writes.lock().unwrap(), vec!["create"]);
            assert_eq!(*b_tape.writes.lock().unwrap(), vec!["create"]);
            alice.close();
            bob.close();
            registry.dispose(&a, Disposal::PageDisposed);
            registry.dispose(&b, Disposal::PageDisposed);
        })
        .await;
}

#[tokio::test]
async fn unsupported_init_preserves_live_delegated_receipts_and_pending_completion() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let tape = Tape {
                hold: true,
                ..Default::default()
            };
            let account = delegated_account(&registry, "A", "bybit", tape.clone());
            let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
            let live = session.out.with_instance_id(Some("live"));
            session.joined.get_mut("synthetic-main").unwrap().out = live.clone();
            session
                .registration_outputs
                .insert("synthetic-main".into(), live);
            assert!(session.resnapshot("synthetic-main").await);
            let first_id = submit(&mut session, 0, Some("live")).await;
            assert_eq!(
                order_frame(&mut sent, "order.ack").await["d"]["instanceId"],
                "live"
            );
            tokio::time::timeout(Duration::from_secs(2), async {
                while tape.waiting.lock().unwrap().is_empty() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();

            // #3502 restores Bitrue admission; Weex remains outside the beta roster.
            // Keep this init unsupported so the live registration and receipts survive.
            let mut refused = init("weex");
            refused.instance_id = Some("refused".into());
            session.init(refused).await.unwrap();
            let reply = tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    let text = sent.recv().await.unwrap();
                    let frame: Value = serde_json::from_str(&text).unwrap();
                    if frame["d"]["op"] == "error" {
                        break frame;
                    }
                }
            })
            .await
            .expect("bounded unsupported-init reply");
            assert_eq!(reply["d"]["instanceId"], "refused");
            assert_eq!(reply["d"]["data"][1]["kind"], "unsupported");
            assert!(Arc::ptr_eq(
                &session.joined["synthetic-main"].account,
                &account
            ));

            // The rejected registration cannot dispatch, and keeps its own refusal output.
            submit(&mut session, 0, Some("refused")).await;
            let ack = order_frame(&mut sent, "order.ack").await;
            assert_eq!(ack["d"]["instanceId"], "refused");
            assert_eq!(ack["d"]["received"], false);
            let denied = order_frame(&mut sent, "order.outcome").await;
            assert_eq!(denied["d"]["instanceId"], "refused");
            assert_eq!(denied["d"]["outcomeSeq"], 1);
            assert_eq!(denied["d"]["outcome"], "refused");
            assert_eq!(*tape.writes.lock().unwrap(), vec!["create"]);

            // Another authenticated user's session becomes active while A's native work waits.
            let b_tape = Tape::default();
            let b = delegated_account(&registry, "B", "bybit", b_tape.clone());
            let (mut bob, mut b_sent) = joined(registry.clone(), "B", b.clone()).await;
            bob.joined.get_mut("synthetic-main").unwrap().out = bob.out.with_instance_id(Some("B"));
            assert!(bob.resnapshot("synthetic-main").await);
            for (id, events) in tape.waiting.lock().unwrap().drain(..) {
                answer(
                    &events,
                    id,
                    json!({
                        "retCode":0,"retMsg":"OK",
                        "result":{"list":[{"orderId":"native-before-refusal",
                            "orderLinkId":first_id,"symbol":"BTCUSDT"}]},
                        "retExtInfo":{"list":[{"code":0,"msg":"OK"}]}
                    })
                    .to_string(),
                );
            }
            let completion = order_frame(&mut sent, "order.outcome").await;
            assert_eq!(completion["d"]["instanceId"], "live");
            assert_eq!(completion["d"]["outcomeSeq"], 1);
            assert_eq!(completion["d"]["outcome"], "placed");
            // A new accepted placement must select the same output as admission.
            submit(&mut session, 0, Some("live")).await;
            let ack = order_frame(&mut sent, "order.ack").await;
            assert_eq!(ack["d"]["instanceId"], "live");
            assert_eq!(ack["d"]["received"], true);
            tokio::time::timeout(Duration::from_secs(2), async {
                while tape.waiting.lock().unwrap().is_empty() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            tape.finish_unknown();
            let completion = order_frame(&mut sent, "order.outcome").await;
            assert_eq!(completion["d"]["instanceId"], "live");
            assert_eq!(completion["d"]["outcomeSeq"], 2);
            assert_eq!(completion["d"]["outcome"], "unknown");
            session
                .handle(
                    &json!({"t":"order.cancelQueued","d":{
                        "accountName":"synthetic-main","instanceId":"live","intentId":intent(0),
                        "seq":2,"fingerprint":"a".repeat(64)
                    }})
                    .to_string(),
                )
                .await
                .unwrap();
            let receipt = order_frame(&mut sent, "order.outcome").await;
            assert_eq!(receipt["d"]["instanceId"], "live");
            assert_eq!(receipt["d"]["outcomeSeq"], 3);
            assert_eq!(receipt["d"]["outcome"], "unknown");
            assert_eq!(*tape.writes.lock().unwrap(), vec!["create", "create"]);
            assert!(b_tape.writes.lock().unwrap().is_empty());
            while let Ok(text) = b_sent.try_recv() {
                let frame: Value = serde_json::from_str(&text).unwrap();
                assert!(
                    !frame["t"]
                        .as_str()
                        .is_some_and(|kind| kind.starts_with("order.")),
                    "A's receipts must never reach B: {frame}"
                );
            }
            submit(&mut bob, 0, Some("B")).await;
            assert_eq!(
                order_frame(&mut b_sent, "order.ack").await["d"]["instanceId"],
                "B"
            );
            let placed = order_frame(&mut b_sent, "order.outcome").await;
            assert_eq!(placed["d"]["instanceId"], "B");
            assert_eq!(placed["d"]["outcomeSeq"], 1);
            assert_eq!(placed["d"]["outcome"], "placed");
            session.close();
            bob.close();
            registry.dispose(&account, Disposal::PageDisposed);
            registry.dispose(&b, Disposal::PageDisposed);
        })
        .await;
}
