use super::*;

fn action_key() -> String {
    let hex = format!("{:012x}", crate::accounts::now_ms());
    format!("{}-{}-7101-8101-010101010101", &hex[..8], &hex[8..])
}
async fn action(session: &mut Session, action: Value, version: u8, corrupt: bool) -> String {
    let id = action_key();
    let fp = crate::order_contract::action_fingerprint("synthetic-main", &id, &action, 1).unwrap();
    session.handle(&json!({"t":"order.action","d":{"version":version,"accountName":"synthetic-main",
        "intentId":id,"seq":1,"ageMs":0,"fingerprint":if corrupt {"0".repeat(64)} else {fp},"action":action}}).to_string()).await.unwrap();
    id
}
async fn action_frame(sent: &mut crate::out::Receiver, kind: &str) -> Value {
    tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(text) = sent.recv().await {
            let frame: Value = serde_json::from_str(&text).unwrap();
            if frame["t"] == kind {
                return frame;
            }
        }
        panic!("missing action receipt")
    })
    .await
    .expect("bounded native action receipt")
}

#[tokio::test]
async fn action_frames_use_real_native_amend_cancel_cancel_all_and_nuke_paths() {
    tokio::task::LocalSet::new().run_until(async {
        for (kind, payload, expected) in [
            ("amend", json!({"kind":"amend","order":{"id":"resting-order","symbol":"BTCUSDT"},"update":{"price":51000}}), vec!["amend"]),
            ("cancel", json!({"kind":"cancel","order":{"id":"resting-order","symbol":"BTCUSDT"}}), vec!["cancel"]),
            ("cancelAll", json!({"kind":"cancelAll"}), vec!["cancel"]),
            ("nuke", json!({"kind":"nuke"}), vec!["cancel","create"]),
        ] {
            let registry = crate::accounts::tests::registry(); let tape = Tape::default();
            let account = delegated_account_with_algos(&registry,"A","bybit",tape.clone(),true);
            let (mut session,mut sent) = joined(registry.clone(),"A",account.clone()).await;
            let id = action(&mut session,payload,1,false).await;
            let ack = action_frame(&mut sent,"order.actionAck").await;
            assert_eq!(ack["d"]["intentId"],id); assert_eq!(ack["d"]["received"],true);
            let result = action_frame(&mut sent,"order.actionOutcome").await;
            assert_eq!(result["d"]["version"],1);
            assert_eq!(result["d"]["intentId"],id);
            assert_eq!(result["d"]["outcome"],"applied","{kind}: {result}");
            assert_eq!(result["d"]["result"]["kind"],kind);
            let writes = tape.writes.lock().unwrap().clone();
            for expected in expected { assert!(writes.iter().any(|w| w == expected),"{kind}: {writes:?}"); }
            if kind != "nuke" { assert!(!writes.iter().any(|w| w == "create")); }
            session.close(); registry.dispose(&account,Disposal::PageDisposed);
        }
    }).await;
}

#[tokio::test]
async fn action_frames_refuse_wrong_version_fingerprint_registration_mode_and_expired_keys() {
    tokio::task::LocalSet::new().run_until(async {
        for case in ["version","fingerprint","binding","mode","expired","pending"] {
            let registry = crate::accounts::tests::registry(); let tape = Tape::default();
            let account = delegated_account(&registry,"A",if case == "mode" {"bybitspot"} else {"bybit"},tape.clone());
            let (mut session,mut sent) = joined(registry.clone(),"A",account.clone()).await;
            let payload = json!({"kind":"cancelAll"});
            let mut id = action_key();
            if case == "expired" { let hex=format!("{:012x}",crate::accounts::now_ms()-10_001); id=format!("{}-{}-7101-8101-010101010101",&hex[..8],&hex[8..]); }
            let fp = crate::order_contract::action_fingerprint("synthetic-main",&id,&payload,1).unwrap();
            if case == "pending" { session.start_verification("synthetic-main".into(),std::future::pending()); }
            session.handle(&json!({"t":"order.action","d":{"version":if case=="version"{2}else{1},"accountName":"synthetic-main","instanceId":if case=="binding"{Some("other")}else{None},"intentId":id,"seq":1,"ageMs":0,"fingerprint":if case=="fingerprint"{"0".repeat(64)}else{fp},"action":payload}}).to_string()).await.unwrap();
            let reply=action_frame(&mut sent,"order.actionOutcome").await;
            assert_eq!(reply["d"]["outcome"],"refused"); assert_eq!(reply["d"]["error"]["outcome"],"notSent");
            assert!(tape.writes.lock().unwrap().is_empty());
            session.close(); registry.dispose(&account,Disposal::PageDisposed);
        }
    }).await;
}

#[tokio::test]
async fn deferred_action_completion_cannot_cross_from_alice_to_bob() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let a_tape = Tape {
                hold: true,
                ..Default::default()
            };
            let a = delegated_account(&registry, "A", "bybit", a_tape.clone());
            let (mut alice, mut a_sent) = joined(registry.clone(), "A", a.clone()).await;
            action(
                &mut alice,
                json!({"kind":"cancel","order":{"id":"resting-order","symbol":"BTCUSDT"}}),
                1,
                false,
            )
            .await;
            action_frame(&mut a_sent, "order.actionAck").await;
            tokio::time::timeout(Duration::from_secs(2), async {
                while a_tape.waiting.lock().unwrap().is_empty() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            alice.close();
            let b_tape = Tape::default();
            let b = delegated_account(&registry, "B", "bybit", b_tape.clone());
            let (mut bob, mut b_sent) = joined(registry.clone(), "B", b.clone()).await;
            let b_id = action(
                &mut bob,
                json!({"kind":"cancel","order":{"id":"resting-order","symbol":"BTCUSDT"}}),
                1,
                false,
            )
            .await;
            a_tape.finish_unknown();
            let reply = action_frame(&mut b_sent, "order.actionOutcome").await;
            assert_eq!(reply["d"]["intentId"], b_id);
            assert_eq!(reply["d"]["outcome"], "applied");
            assert_eq!(*b_tape.writes.lock().unwrap(), vec!["cancel"]);
            let stale = tokio::time::timeout(Duration::from_millis(30), async {
                while let Some(text) = a_sent.recv().await {
                    let frame: Value = serde_json::from_str(&text).unwrap();
                    if frame["t"] == "order.actionOutcome" {
                        return Some(frame);
                    }
                }
                None
            })
            .await;
            assert!(!matches!(stale, Ok(Some(_))));
            bob.close();
            registry.dispose(&a, Disposal::PageDisposed);
            registry.dispose(&b, Disposal::PageDisposed);
        })
        .await;
}

#[tokio::test]
async fn nuke_checks_the_captured_owner_after_the_private_load_deferral() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let tape = Tape::default();
            tape.defer_positions
                .store(true, std::sync::atomic::Ordering::SeqCst);
            let a = delegated_account_with_algos(&registry, "A", "bybit", tape.clone(), true);
            let (out, mut a_sent, _) = Out::channel(256);
            let mut alice = Session::new(
                registry.clone(),
                Arc::new(Metrics::default()),
                Identity {
                    user: Some("A".into()),
                    chat_handle: None,
                    token: None,
                    resume_from: None,
                    resumable: false,
                },
                out.clone(),
            );
            crate::accounts::tests::hold_live(&a, alice.socket, out.outbox());
            alice.join(a.clone(), out.with_instance_id(None)).await;
            action(&mut alice, json!({"kind":"nuke"}), 1, false).await;
            action_frame(&mut a_sent, "order.actionAck").await;
            tokio::time::timeout(Duration::from_secs(2), async {
                while tape.held_positions.lock().unwrap().is_empty() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert_eq!(
                a.handle.read(|store| store.loaded().positions).await,
                Some(false)
            );
            assert!(tape.writes.lock().unwrap().is_empty());
            // Keep A resident through another holder: the stale task could still
            // reach a live A engine after load, so registration retirement must fence it.
            let (kept_a, _kept_rx, _) = Out::channel(256);
            crate::accounts::tests::hold_live(&a, 9999, kept_a.outbox());
            alice.close();
            let b_tape = Tape::default();
            let b = delegated_account(&registry, "B", "bybit", b_tape.clone());
            let (mut bob, mut b_sent) = joined(registry.clone(), "B", b.clone()).await;
            tape.release_positions();
            tokio::time::timeout(Duration::from_secs(2), async {
                while a.handle.read(|store| store.loaded().positions).await != Some(true) {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;
            assert!(
                tape.writes.lock().unwrap().is_empty(),
                "retired A nuke must not dispatch after its load wait"
            );
            action(&mut bob, json!({"kind":"cancelAll"}), 1, false).await;
            assert_eq!(
                action_frame(&mut b_sent, "order.actionOutcome").await["d"]["outcome"],
                "applied"
            );
            assert_eq!(*b_tape.writes.lock().unwrap(), vec!["cancel"]);
            bob.close();
            registry.dispose(&a, Disposal::PageDisposed);
            registry.dispose(&b, Disposal::PageDisposed);
        })
        .await;
}

#[tokio::test]
async fn a_partial_native_nuke_is_unknown_after_its_cancel_leg_completed() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let tape = Tape {
                close_unknown: true,
                ..Default::default()
            };
            let account = delegated_account_with_algos(&registry, "A", "bybit", tape.clone(), true);
            let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
            action(&mut session, json!({"kind":"nuke"}), 1, false).await;
            let reply = action_frame(&mut sent, "order.actionOutcome").await;
            assert_eq!(reply["d"]["outcome"], "unknown");
            assert_eq!(reply["d"]["error"]["outcome"], "unknown");
            assert!(reply["d"]["result"].is_null());
            let writes = tape.writes.lock().unwrap().clone();
            assert!(writes.iter().any(|op| op == "cancel"));
            assert!(writes.iter().any(|op| op == "create"));
            session.close();
            registry.dispose(&account, Disposal::PageDisposed);
        })
        .await;
}

#[tokio::test]
async fn action_fix_nuke_owner_is_checked_between_native_legs() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for retire in [false, true] {
                let registry = crate::accounts::tests::registry();
                let tape = Tape {
                    hold: true,
                    ..Default::default()
                };
                let a = delegated_account_with_algos(&registry, "A", "bybit", tape.clone(), true);
                let (alice, mut a_sent) = joined(registry.clone(), "A", a.clone()).await;
                let mut alice = Some(alice);
                let (keeper, _keeper_rx, _) = Out::channel(256);
                crate::accounts::tests::hold_live(&a, 9999, keeper.outbox());
                action(alice.as_mut().unwrap(), json!({"kind":"nuke"}), 1, false).await;
                action_frame(&mut a_sent, "order.actionAck").await;
                tokio::time::timeout(Duration::from_secs(3), async {
                    while tape.waiting.lock().unwrap().is_empty() {
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .expect("first native cancel reached controlled transport");
                assert_eq!(*tape.writes.lock().unwrap(), vec!["cancel"]);
                if retire {
                    alice.take().unwrap().close();
                }
                // A separate owner's real native action remains a working positive control.
                let b_tape = Tape::default();
                let b = delegated_account(&registry, "B", "bybit", b_tape.clone());
                let (mut bob, mut b_sent) = joined(registry.clone(), "B", b.clone()).await;
                action(&mut bob, json!({"kind":"cancelAll"}), 1, false).await;
                let b_out = action_frame(&mut b_sent, "order.actionOutcome").await;
                assert_eq!(b_out["d"]["outcome"], "applied");
                assert_eq!(*b_tape.writes.lock().unwrap(), vec!["cancel"]);
                tape.finish_unknown();
                let crossed = tokio::time::timeout(Duration::from_millis(750), async {
                    loop {
                        if tape.writes.lock().unwrap().iter().any(|op| op == "create") {
                            break;
                        }
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .is_ok();
                let writes = tape.writes.lock().unwrap().clone();
                bob.close();
                if let Some(alice) = alice.take() {
                    alice.close();
                }
                registry.dispose(&a, Disposal::PageDisposed);
                registry.dispose(&b, Disposal::PageDisposed);
                assert_eq!(
                    crossed, !retire,
                    "retire={retire}; native A writes={writes:?}"
                );
            }
        })
        .await;
}
