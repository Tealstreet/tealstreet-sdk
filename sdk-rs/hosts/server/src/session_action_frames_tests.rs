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

fn native_request_rows(tape: &Tape, suffix: &str) -> Vec<Value> {
    tape.requests
        .lock()
        .unwrap()
        .iter()
        .filter(|r| {
            r["path"].as_str().is_some_and(|path| {
                path.ends_with(&format!("/{suffix}")) || path.ends_with(&format!("/{suffix}-batch"))
            })
        })
        .flat_map(|r| {
            r["body"]["request"]
                .as_array()
                .cloned()
                .unwrap_or_else(|| vec![r["body"].clone()])
        })
        .collect()
}

#[tokio::test]
async fn dedicated_actions_coexist_with_partial_activation_on_the_same_session() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let tape = Tape::default();
            let account = delegated_account(&registry, "A", "bybit", tape.clone());
            let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
            call(
                &mut session,
                "legacy-cancel",
                "cancelOrders",
                json!([[{"id":"resting-order","symbol":"BTCUSDT"}]]),
            )
            .await;
            let refused = reply(&mut sent, "legacy-cancel").await;
            assert_eq!(refused["success"], false);
            assert_eq!(refused["result"]["name"], "DelegatedActionUnavailable");
            assert!(tape.writes.lock().unwrap().is_empty());
            let id = action(
                &mut session,
                json!({"kind":"cancel","order":{"id":"resting-order","symbol":"BTCUSDT"}}),
                1,
                false,
            )
            .await;
            let result = action_frame(&mut sent, "order.actionOutcome").await;
            assert_eq!(result["d"]["intentId"], id);
            assert_eq!(result["d"]["outcome"], "applied");
            assert_eq!(tape.writes.lock().unwrap().as_slice(), &["cancel"]);
            session.close();
            registry.dispose(&account, Disposal::PageDisposed);
        })
        .await;
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
            let account = delegated_account(&registry,"A",if case == "mode" {"hyrotrader"} else {"bybit"},tape.clone());
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

#[tokio::test]
async fn action_fix_invalid_queue_cancel_does_not_invent_a_not_sent_proof() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let tape = Tape::default();
            let account = delegated_account(&registry, "A", "bybit", tape.clone());
            let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
            let payload = json!({"kind":"cancel","order":{"id":"resting-order","symbol":"BTCUSDT"}});
            let id = action(&mut session, payload.clone(), 1, false).await;
            let applied = action_frame(&mut sent, "order.actionOutcome").await;
            assert_eq!(applied["d"]["outcome"], "applied");
            let fingerprint = crate::order_contract::action_fingerprint("synthetic-main", &id, &payload, 1).unwrap();
            let cancel = json!({"t":"order.actionCancelQueued","d":{"version":1,"accountName":"synthetic-main","intentId":id,"seq":1,"fingerprint":fingerprint}});
            session.handle(&cancel.to_string()).await.unwrap();
            let valid = action_frame(&mut sent, "order.actionOutcome").await;
            assert_eq!(valid["d"]["outcome"], "unknown");

            // Losing action admission after dispatch proves nothing about the
            // original action. A still-bound queue cancellation stays unknown.
            session.start_verification("synthetic-main".into(), std::future::pending());
            session.handle(&cancel.to_string()).await.unwrap();
            let pending = action_frame(&mut sent, "order.actionOutcome").await;
            assert_eq!(pending["d"]["outcome"], "unknown");
            for (field, value) in [
                ("version", json!(2)), ("fingerprint", json!("bad")),
                ("instanceId", json!("stale")), ("seq", json!(0)),
            ] {
                let mut invalid = cancel.clone();
                invalid["d"][field] = value;
                session.handle(&invalid.to_string()).await.unwrap();
                let frames: Vec<Value> = std::iter::from_fn(|| sent.try_recv().ok())
                    .map(|text| serde_json::from_str(&text).unwrap()).collect();
                assert!(frames.iter().any(|frame| frame["t"] == "msg" && frame["d"]["op"] == "error"), "{field}: {frames:?}");
                assert!(frames.iter().all(|frame| frame["t"] != "order.actionAck" && frame["t"] != "order.actionOutcome"), "no business receipt for invalid {field}: {frames:?}");
            }
            session.handle(&cancel.to_string()).await.unwrap();
            let after = action_frame(&mut sent, "order.actionOutcome").await;
            assert_eq!(after["d"]["outcome"], "unknown");
            assert_eq!(after["d"]["outcomeSeq"].as_u64(), pending["d"]["outcomeSeq"].as_u64().map(|seq| seq + 1));
            assert_eq!(*tape.writes.lock().unwrap(), vec!["cancel"]);
            session.close();
            registry.dispose(&account, Disposal::PageDisposed);
        })
        .await;
}

#[tokio::test]
async fn action_frames_use_the_captured_startup_runtime_gate() {
    const PROBE: &str = "TEALSTREET_TEST_ACTION_RUNTIME";
    if std::env::var(PROBE).is_err() {
        for value in ["0", "1"] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "session::delegated_writes_tests::action_frames::action_frames_use_the_captured_startup_runtime_gate", "--nocapture"])
                .env("TEALSTREET_DELEGATED_ORDER_FRAMES", value).env(PROBE, value).output().unwrap();
            assert!(
                output.status.success(),
                "startup {value}: {}",
                String::from_utf8_lossy(&output.stdout)
            );
            assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
        }
        return;
    }
    let enabled = std::env::var(PROBE).as_deref() == Ok("1");
    tokio::task::LocalSet::new().run_until(async {
        for payload in [
            json!({"kind":"amend","order":{"id":"resting-order","symbol":"BTCUSDT"},"update":{"price":51000}}),
            json!({"kind":"cancel","order":{"id":"resting-order","symbol":"BTCUSDT"}}),
            json!({"kind":"cancelAll"}), json!({"kind":"nuke"}),
        ] {
            let registry = crate::accounts::tests::registry(); let tape = Tape::default();
            let account = delegated_account_with_algos(&registry, "A", "bybit", tape.clone(), true);
            let (mut session, mut sent) = joined_without_runtime_opt_in(registry.clone(), "A", account.clone()).await;
            assert_eq!(session.order_frames_enabled, enabled);
            let caps = crate::http::capabilities_json();
            assert_eq!(caps["transports"]["accountWorkerWebSocket"]["delegatedOrderActionsV1"], enabled);
            action(&mut session, payload, 1, false).await;
            let ack = action_frame(&mut sent, "order.actionAck").await;
            assert_eq!(ack["d"]["received"], enabled);
            let result = action_frame(&mut sent, "order.actionOutcome").await;
            assert_eq!(result["d"]["outcome"], if enabled {"applied"} else {"refused"});
            if !enabled { assert_eq!(result["d"]["error"]["outcome"], "notSent"); assert!(tape.writes.lock().unwrap().is_empty()); }
            assert_eq!(session.order_frames_enabled, enabled);
            session.close(); registry.dispose(&account, Disposal::PageDisposed);
        }
    }).await;
}

// R1 exercises actual Out::send stamping, including router-only cancellation
// and a pre-dispatch refusal, rather than only the unregistered fixture shapes.
#[tokio::test]
async fn action_fix_registered_replies_carry_the_captured_binding() {
    tokio::task::LocalSet::new().run_until(async {
        for payload in [
            json!({"kind":"amend","order":{"id":"resting-order","symbol":"BTCUSDT"},"update":{"price":51000}}),
            json!({"kind":"cancel","order":{"id":"resting-order","symbol":"BTCUSDT"}}),
            json!({"kind":"cancelAll"}), json!({"kind":"nuke"}),
        ] {
            let registry = crate::accounts::tests::registry();
            let tape = Tape::default();
            let account = delegated_account_with_algos(&registry,"A","bybit",tape.clone(),true);
            let (mut session,mut sent) = joined(registry.clone(),"A",account.clone()).await;
            let bound = session.account_out("synthetic-main").with_instance_id(Some("reg-A"));
            session.join(account.clone(),bound).await;
            let id = action_key();
            let fp = crate::order_contract::action_fingerprint("synthetic-main",&id,&payload,1).unwrap();
            session.handle(&json!({"t":"order.action","d":{"version":1,"accountName":"synthetic-main","instanceId":"reg-A","intentId":id,"seq":1,"ageMs":0,"fingerprint":fp,"action":payload}}).to_string()).await.unwrap();
            let ack = action_frame(&mut sent,"order.actionAck").await;
            let outcome = action_frame(&mut sent,"order.actionOutcome").await;
            assert_eq!(ack["d"]["received"],true);
            assert_eq!(outcome["d"]["outcome"],"applied");
            let mut cancel = json!({"version":1,"accountName":"synthetic-main","instanceId":"reg-A","intentId":id,"seq":1,"fingerprint":fp});
            session.handle(&json!({"t":"order.actionCancelQueued","d":cancel}).to_string()).await.unwrap();
            let canceled = action_frame(&mut sent,"order.actionOutcome").await;
            assert_eq!(canceled["d"]["outcome"],"unknown");
            cancel["ageMs"] = json!(0); cancel["action"] = payload; cancel["version"] = json!(2);
            session.handle(&json!({"t":"order.action","d":cancel}).to_string()).await.unwrap();
            let refused_ack = action_frame(&mut sent,"order.actionAck").await;
            let refused = action_frame(&mut sent,"order.actionOutcome").await;
            assert_eq!(refused_ack["d"]["received"],false);
            assert_eq!(refused["d"]["outcome"],"refused");
            session.close(); registry.dispose(&account,Disposal::PageDisposed);
            for reply in [ack,outcome,canceled,refused_ack,refused] {
                assert_eq!(reply["d"]["instanceId"],"reg-A","{reply}");
            }
        }
    }).await;
}

// Independent UUID-wall and forwarded-monotonic age controls. Both requests
// are admitted while valid; private startup consumes their remaining budget.
#[tokio::test]
async fn action_fix_deferred_nuke_expires_without_any_physical_write() {
    tokio::task::LocalSet::new().run_until(async {
        let mut observed = Vec::new();
        for clock in ["uuid","age","fresh"] {
            let registry = crate::accounts::tests::registry(); let tape = Tape::default();
            tape.defer_positions.store(true,std::sync::atomic::Ordering::SeqCst);
            let account = delegated_account_with_algos(&registry,"A","bybit",tape.clone(),true);
            let (out,mut sent,_) = Out::channel(256);
            let mut session = Session::new(registry.clone(),Arc::new(Metrics::default()),Identity {
                user:Some("A".into()),chat_handle:None,token:None,resume_from:None,resumable:false,
            },out.clone());
            session.order_frames_enabled = crate::order_contract::enabled();
            crate::accounts::tests::hold_live(&account,session.socket,out.outbox());
            session.join(account.clone(),out.with_instance_id(None)).await;
            tokio::time::timeout(Duration::from_secs(3),async {
                while tape.held_positions.lock().unwrap().is_empty() { tokio::task::yield_now().await; }
            }).await.unwrap();
            let hex = format!("{:012x}",crate::accounts::now_ms()-if clock=="uuid" {9500}else{0});
            let id = format!("{}-{}-7101-8101-010101010101",&hex[..8],&hex[8..]);
            let payload = json!({"kind":"nuke"});
            let fp = crate::order_contract::action_fingerprint("synthetic-main",&id,&payload,1).unwrap();
            session.handle(&json!({"t":"order.action","d":{"version":1,"accountName":"synthetic-main","intentId":id,"seq":1,"ageMs":if clock=="age" {9500}else{0},"fingerprint":fp,"action":payload}}).to_string()).await.unwrap();
            assert_eq!(action_frame(&mut sent,"order.actionAck").await["d"]["received"],true);
            assert!(tape.writes.lock().unwrap().is_empty());
            tokio::time::sleep(Duration::from_millis(650)).await;
            tape.release_positions();
            let result = action_frame(&mut sent,"order.actionOutcome").await;
            let writes = tape.writes.lock().unwrap().clone();
            observed.push((clock,result,writes));
            session.close(); registry.dispose(&account,Disposal::PageDisposed);
        }
        for (clock,result,writes) in observed {
            if clock=="fresh" {
                assert_eq!(result["d"]["outcome"],"applied","{result}");
                assert_eq!(writes,vec!["cancel","create"]);
            } else {
                assert!(writes.is_empty(),"{clock}: expired deferred nuke dispatched {writes:?}; {result}");
                assert_eq!(result["d"]["outcome"],"unknown","{result}");
                assert_eq!(result["d"]["error"]["outcome"],"unknown");
                assert!(result["d"]["error"]["message"].as_str().unwrap().contains("expired"));
                assert!(result["d"]["result"].is_null());
            }
        }
    }).await;
}

#[tokio::test]
async fn action_fix_nuke_expiry_is_checked_between_each_native_leg() {
    tokio::task::LocalSet::new().run_until(async {
        let mut observed = Vec::new();
        for clock in ["uuid","age","fresh"] {
            let registry = crate::accounts::tests::registry();
            let tape = Tape { hold:true,..Default::default() };
            let account = delegated_account_with_algos(&registry,"A","bybit",tape.clone(),true);
            let (mut session,mut sent) = joined(registry.clone(),"A",account.clone()).await;
            let hex = format!("{:012x}",crate::accounts::now_ms()-if clock=="uuid" {9500}else{0});
            let id = format!("{}-{}-7101-8101-010101010101",&hex[..8],&hex[8..]);
            let payload = json!({"kind":"nuke"});
            let fp = crate::order_contract::action_fingerprint("synthetic-main",&id,&payload,1).unwrap();
            session.handle(&json!({"t":"order.action","d":{"version":1,"accountName":"synthetic-main","intentId":id,"seq":1,"ageMs":if clock=="age" {9500}else{0},"fingerprint":fp,"action":payload}}).to_string()).await.unwrap();
            assert_eq!(action_frame(&mut sent,"order.actionAck").await["d"]["received"],true);
            tokio::time::timeout(Duration::from_secs(3),async {
                while tape.waiting.lock().unwrap().is_empty() { tokio::task::yield_now().await; }
            }).await.expect("first cancel before expiry");
            assert_eq!(*tape.writes.lock().unwrap(),vec!["cancel"]);
            tokio::time::sleep(Duration::from_millis(650)).await;
            tape.finish_unknown();
            // A broken implementation emits another physical command here.
            // Only the fresh control is allowed to reach the held close.
            let close = tokio::time::timeout(Duration::from_millis(500),async {
                while tape.waiting.lock().unwrap().is_empty() { tokio::task::yield_now().await; }
            }).await.is_ok();
            let writes = tape.writes.lock().unwrap().clone();
            if !close && clock != "fresh" {
                let result = action_frame(&mut sent,"order.actionOutcome").await;
                assert_eq!(result["d"]["outcome"],"unknown","{result}");
                assert_eq!(result["d"]["error"]["outcome"],"unknown");
                assert!(result["d"]["error"]["message"].as_str().unwrap().contains("expired"));
                assert!(result["d"]["result"].is_null());
            }
            observed.push((clock,close,writes));
            session.close(); registry.dispose(&account,Disposal::PageDisposed);
        }
        for (clock,close,writes) in observed {
            assert_eq!(close,clock=="fresh","{clock}: {writes:?}");
            assert_eq!(writes,if clock=="fresh" {vec!["cancel","create"]}else{vec!["cancel"]});
        }
    }).await;
}

#[tokio::test]
async fn action_fix_cancel_all_expires_during_its_symbol_store_read() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let mut observed = Vec::new();
            for clock in ["fresh", "uuid", "age"] {
                let registry = crate::accounts::tests::registry();
                let tape = Tape::default();
                let account = delegated_account(&registry, "A", "bybit", tape.clone());
                let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
                let hex = format!(
                    "{:012x}",
                    crate::accounts::now_ms() - if clock == "uuid" { 9500 } else { 0 }
                );
                let id = format!("{}-{}-7101-8101-010101010101", &hex[..8], &hex[8..]);
                let payload = json!({"kind":"cancelAll"});
                let fp = crate::order_contract::action_fingerprint("synthetic-main", &id, &payload, 1).unwrap();
                let wire = json!({"t":"order.action","d":{"version":1,"accountName":"synthetic-main",
                    "intentId":id,"seq":1,"ageMs":if clock=="age" {9500}else{0},"fingerprint":fp,"action":payload}}).to_string();
                let handle = account.handle.clone();
                // Poll both operations before yielding to the engine. The first
                // actor read defers cancelAll's symbol read after admission.
                let (delay_done, handled) = tokio::join!(biased;
                    handle.read(|_| { std::thread::sleep(Duration::from_millis(650)); true }),
                    session.handle(&wire)
                );
                assert_eq!(delay_done, Some(true));
                handled.unwrap();
                let ack = action_frame(&mut sent, "order.actionAck").await;
                assert_eq!(ack["d"]["received"], true, "{clock}: {ack}");
                let result = action_frame(&mut sent, "order.actionOutcome").await;
                let writes = tape.writes.lock().unwrap().clone();
                observed.push((clock, result, writes));
                session.close();
                registry.dispose(&account, Disposal::PageDisposed);
            }
            for (clock, result, writes) in &observed {
                if *clock == "fresh" {
                    assert_eq!(result["d"]["outcome"], "applied", "{result}");
                    assert_eq!(writes, &vec!["cancel"]);
                } else {
                    assert!(writes.is_empty(), "expired {clock} cancelAll wrote: {observed:?}");
                    assert_eq!(result["d"]["outcome"], "unknown", "{result}");
                    assert_eq!(result["d"]["error"]["outcome"], "unknown");
                    assert!(result["d"]["error"]["message"].as_str().unwrap().contains("expired"));
                    assert!(result["d"]["result"].is_null());
                }
            }
        })
        .await;
}

#[tokio::test]
async fn action_fix_each_deferred_cancel_leg_keeps_deadline_and_owner() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for mode in ["fresh", "uuid", "age", "retired"] {
                let registry = crate::accounts::tests::registry();
                let tape = Tape::default();
                tape.defer_positions
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                let account = delegated_account(&registry, "A", "bybit", tape.clone());
                let (out, mut sent, _) = Out::channel(256);
                let mut session = Session::new(
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
                session.order_frames_enabled = crate::order_contract::enabled();
                crate::accounts::tests::hold_live(&account, session.socket, out.outbox());
                session
                    .join(account.clone(), out.with_instance_id(None))
                    .await;
                tokio::time::timeout(Duration::from_secs(3), async {
                    while tape.held_positions.lock().unwrap().is_empty() {
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .unwrap();
                let hex = format!(
                    "{:012x}",
                    crate::accounts::now_ms() - if mode == "uuid" { 9500 } else { 0 }
                );
                let id = json!(format!(
                    "{}-{}-7101-8101-010101010101",
                    &hex[..8],
                    &hex[8..]
                ));
                let deadline = crate::order_actions::ActionDeadline::new(
                    id.as_str().unwrap(),
                    if mode == "age" { 9500.0 } else { 0.0 },
                    std::time::Instant::now(),
                )
                .unwrap();
                let receipt = Some(OrderReceipt::Action {
                    seq: 1,
                    kind: "cancelAll",
                    deadline,
                });
                let group = Some(Arc::new(Mutex::new(Group {
                    remaining: 2,
                    failures: Vec::new(),
                })));
                // Exercise the actual per-command startup continuation with two
                // legs from one original receipt, not just call_inner's read fence.
                for symbol in ["BTCUSDT", "ETHUSDT"] {
                    session.send_command(
                        "synthetic-main",
                        &id,
                        "cancelAllOrders",
                        &group,
                        json!({"op":"cancelSymbolOrders","symbol":symbol}),
                        methods::Reply::Action {
                            seq: 1,
                            kind: "cancelAll",
                            unit: true,
                            idless_protection: false,
                        },
                        true,
                        receipt,
                    );
                }
                let pending = session.joined["synthetic-main"].client.pending.clone();
                assert_eq!(pending.lock().unwrap().len(), 2);
                assert!(tape.writes.lock().unwrap().is_empty());
                tokio::time::sleep(Duration::from_millis(650)).await;
                let mut session = Some(session);
                let (keeper, _keeper_rx, _) = Out::channel(256);
                if mode == "retired" {
                    // A remains a live native account after its originating
                    // registration retires; B completes a real action meanwhile.
                    crate::accounts::tests::hold_live(&account, 9999, keeper.outbox());
                    session.take().unwrap().close();
                    let b_tape = Tape::default();
                    let b = delegated_account(&registry, "B", "bybit", b_tape.clone());
                    let (mut bob, mut b_sent) = joined(registry.clone(), "B", b.clone()).await;
                    action(&mut bob, json!({"kind":"cancelAll"}), 1, false).await;
                    assert_eq!(
                        action_frame(&mut b_sent, "order.actionOutcome").await["d"]["outcome"],
                        "applied"
                    );
                    assert_eq!(*b_tape.writes.lock().unwrap(), vec!["cancel"]);
                    bob.close();
                    registry.dispose(&b, Disposal::PageDisposed);
                    assert!(
                        !deadline.expired(),
                        "retirement must be tested independently of expiry"
                    );
                }
                tape.release_positions();
                tokio::time::timeout(Duration::from_secs(3), async {
                    while !pending.lock().unwrap().is_empty() {
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .expect("both captured command continuations settle");
                if mode == "retired" {
                    assert!(tape.writes.lock().unwrap().is_empty());
                    while let Ok(text) = sent.try_recv() {
                        let frame: Value = serde_json::from_str(&text).unwrap();
                        assert_ne!(
                            frame["t"], "order.actionOutcome",
                            "stale A outcome: {frame}"
                        );
                    }
                } else {
                    let result = action_frame(&mut sent, "order.actionOutcome").await;
                    if mode == "fresh" {
                        assert_eq!(result["d"]["outcome"], "applied", "{result}");
                        assert_eq!(*tape.writes.lock().unwrap(), vec!["cancel", "cancel"]);
                    } else {
                        assert!(tape.writes.lock().unwrap().is_empty(), "{mode}: {result}");
                        assert_eq!(result["d"]["outcome"], "unknown", "{result}");
                        assert_eq!(result["d"]["error"]["outcome"], "unknown");
                        assert!(
                            result["d"]["error"]["message"]
                                .as_str()
                                .unwrap()
                                .contains("expired")
                        );
                        assert!(result["d"]["result"].is_null());
                    }
                }
                if let Some(session) = session {
                    session.close();
                }
                registry.dispose(&account, Disposal::PageDisposed);
            }
        })
        .await;
}

#[tokio::test]
async fn action_fix_cancel_all_partial_handoff_settles_failed_legs_without_refusal() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = Arc::new(
                Registry::new(
                    crate::threads::EngineThreads::start(1).unwrap(),
                    Duration::from_millis(1),
                    Duration::from_secs(60),
                )
                .with_router_lease(crate::router_lease::Config {
                    enforce: true,
                    box_id: Some("test-box".into()),
                    ..Default::default()
                }),
            );
            let grant = |sequence, ttl| {
                let now = crate::accounts::now_ms();
                registry
                    .router_leases
                    .accept(&crate::router_lease::Heartbeat {
                        r#box: "test-box".into(),
                        issued_at: now,
                        users: vec![crate::router_lease::Assertion {
                            user_id: "A".into(),
                            owner_epoch: 1,
                            sequence,
                            not_after: now + ttl,
                        }],
                    })
                    .unwrap();
            };
            grant(1, 45_000);
            let tape = Tape {
                hold: true,
                ..Default::default()
            };
            let account = delegated_account(&registry, "A", "bybit", tape.clone());
            let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
            let id = json!(action_key());
            let deadline = crate::order_actions::ActionDeadline::new(
                id.as_str().unwrap(),
                0.0,
                std::time::Instant::now(),
            )
            .unwrap();
            let receipt = Some(OrderReceipt::Action {
                seq: 1,
                kind: "cancelAll",
                deadline,
            });
            let group = Some(Arc::new(Mutex::new(Group {
                remaining: 2,
                failures: Vec::new(),
            })));
            session.send_command(
                "synthetic-main",
                &id,
                "cancelAllOrders",
                &group,
                json!({"op":"cancelSymbolOrders","symbol":"BTCUSDT"}),
                methods::Reply::Action {
                    seq: 1,
                    kind: "cancelAll",
                    unit: true,
                    idless_protection: false,
                },
                false,
                receipt,
            );
            tokio::time::timeout(Duration::from_secs(3), async {
                while tape.waiting.lock().unwrap().is_empty() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert_eq!(*tape.writes.lock().unwrap(), vec!["cancel"]);
            // The first leg has crossed physical IO. Fence the account before the
            // second leg: its definite local failure proves nothing about the first.
            grant(2, 50);
            tokio::time::sleep(Duration::from_millis(60)).await;
            assert!(!account.running.io_allowed());
            session.send_command(
                "synthetic-main",
                &id,
                "cancelAllOrders",
                &group,
                json!({"op":"cancelSymbolOrders","symbol":"ETHUSDT"}),
                methods::Reply::Action {
                    seq: 1,
                    kind: "cancelAll",
                    unit: true,
                    idless_protection: false,
                },
                false,
                receipt,
            );
            assert_eq!(
                group.as_ref().unwrap().lock().unwrap().remaining,
                1,
                "the unsent leg must settle through the group, awaiting the first leg"
            );
            while let Ok(text) = sent.try_recv() {
                let frame: Value = serde_json::from_str(&text).unwrap();
                assert!(
                    !matches!(
                        frame["t"].as_str(),
                        Some("order.actionAck" | "order.actionOutcome")
                    ),
                    "one unsent leg cannot refuse the whole in-flight action: {frame}"
                );
            }
            tape.finish_unknown();
            let outcome = action_frame(&mut sent, "order.actionOutcome").await;
            assert_eq!(outcome["d"]["outcome"], "unknown", "{outcome}");
            assert_eq!(outcome["d"]["error"]["outcome"], "unknown");
            assert_eq!(group.as_ref().unwrap().lock().unwrap().remaining, 0);
            assert_eq!(*tape.writes.lock().unwrap(), vec!["cancel"]);
            session.close();
            registry.dispose(&account, Disposal::PageDisposed);
        })
        .await;
}

#[tokio::test]
async fn full_writes_sign_and_send_each_supported_operation_without_browser_signing() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let cases: Vec<Value> =
                serde_json::from_str(include_str!("../tests/data/delegated-full-writes-v1.json"))
                    .unwrap();
            let mut failures = Vec::new();
            for case in cases {
                let kind = case["action"]["kind"].as_str().unwrap();
                let registry = crate::accounts::tests::registry();
                // Bybit's LOCAL-compatible cancellation uses store identities.
                // Publish both orders before asserting that the batch signs/sends.
                let tape = Tape {
                    two_orders: kind == "cancelOrders",
                    ..Default::default()
                };
                let account =
                    delegated_account_with_algos(&registry, "A", "bybit", tape.clone(), true);
                assert!(
                    account.link.is_none(),
                    "DELEGATED must never ask a browser to sign"
                );
                let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
                let mut payload = case["action"].clone();
                if payload["op"] == "twap:state" {
                    for row in payload["data"].as_array_mut().unwrap() {
                        row["ownerId"] = account.registry_name().into();
                        row["ownerGeneration"] = account.twap_owner.snapshot().generation.into();
                    }
                }
                action(&mut session, payload, 1, false).await;
                let result = action_frame(&mut sent, "order.actionOutcome").await;
                let unsupported =
                    matches!(kind, "scheduleCancel" | "setStpMode" | "setAccountTier")
                        || case["refused"] == true;
                let expected = if unsupported {
                    "refused"
                } else if kind == "algo" {
                    "unknown"
                } else {
                    "applied"
                };
                if result["d"]["outcome"] != expected {
                    failures.push(format!(
                        "{kind} {}: expected {expected}, got {result}",
                        case["action"]["op"]
                    ));
                    session.close();
                    registry.dispose(&account, Disposal::PageDisposed);
                    continue;
                }
                if unsupported {
                    assert_eq!(result["d"]["error"]["outcome"], "notSent");
                    assert!(tape.writes.lock().unwrap().is_empty(), "{kind}");
                } else if kind == "algo" {
                    assert!(result["d"]["result"].is_null());
                    if case["action"]["op"] == "twap:state"
                        && !account
                            .running
                            .jobs()
                            .0
                            .iter()
                            .any(|job| job.id == "restored-typed-job" && job.parked)
                    {
                        failures.push("twap:state did not restore the paused job".into());
                    }
                    if matches!(
                        case["action"]["op"].as_str(),
                        Some("exec" | "chaser:start" | "twap:start")
                    ) && tokio::time::timeout(Duration::from_secs(2), async {
                        while tape.writes.lock().unwrap().is_empty() {
                            tokio::task::yield_now().await;
                        }
                    })
                    .await
                    .is_err()
                    {
                        failures.push(format!(
                            "{} did not reach a real signed native write",
                            case["action"]["op"]
                        ));
                    }
                } else {
                    assert_eq!(result["d"]["result"]["kind"], kind);
                    if matches!(kind, "place" | "cancelOrders") {
                        let suffix = if kind == "place" { "create" } else { "cancel" };
                        let rows = native_request_rows(&tape, suffix);
                        assert_eq!(
                            rows.len(),
                            2,
                            "both {kind} legs must reach the signed transport"
                        );
                        if kind == "cancelOrders" {
                            let ids: std::collections::BTreeSet<_> = rows
                                .iter()
                                .map(|r| r["orderId"].as_str().unwrap())
                                .collect();
                            assert_eq!(
                                ids,
                                std::collections::BTreeSet::from([
                                    "resting-order",
                                    "second-resting-order"
                                ])
                            );
                        } else {
                            let prices: Vec<_> = rows
                                .iter()
                                .map(|r| r["price"].as_str().unwrap().parse::<f64>().unwrap())
                                .collect();
                            assert_eq!(prices, vec![50000.0, 50001.0]);
                        }
                    }
                    let writes = tape.writes.lock().unwrap();
                    if kind != "stopAlgos" {
                        assert!(!writes.is_empty(), "{kind}");
                    }
                    if kind == "closeAll" {
                        assert!(!writes.iter().any(|op| op == "cancel"));
                    }
                    if matches!(kind, "cancelOrders" | "cancelSymbol" | "nukeOrders") {
                        assert!(!writes.iter().any(|op| op == "create"));
                    }
                    if kind == "close" {
                        let rows = native_request_rows(&tape, "create");
                        assert_eq!(rows.len(), 1);
                        assert_eq!(rows[0]["reduceOnly"], true);
                        assert_eq!(rows[0]["symbol"], "BTCUSDT");
                        assert_eq!(rows[0]["side"], "Sell");
                        assert_eq!(
                            rows[0]["qty"].as_str().unwrap().parse::<f64>().unwrap(),
                            0.001
                        );
                    }
                }
                session.close();
                registry.dispose(&account, Disposal::PageDisposed);
            }
            assert!(
                failures.is_empty(),
                "native full-write failures:\n{}",
                failures.join("\n")
            );
        })
        .await;
}

#[tokio::test]
async fn full_write_ambiguous_setting_or_batch_is_never_assumed_applied() {
    tokio::task::LocalSet::new().run_until(async {
        for payload in [
            json!({"kind":"setLeverage","symbol":"BTCUSDT","leverage":5}),
            json!({"kind":"place","orders":[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.001,"price":50000}]}),
        ] {
            let registry = crate::accounts::tests::registry();
            let tape = Tape { hold: true, ..Default::default() };
            let account = delegated_account(&registry, "A", "bybit", tape.clone());
            let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
            action(&mut session, payload, 1, false).await;
            action_frame(&mut sent, "order.actionAck").await;
            tokio::time::timeout(Duration::from_secs(2), async {
                while tape.waiting.lock().unwrap().is_empty() { tokio::task::yield_now().await; }
            }).await.unwrap();
            // Definitively discard the response after physical handoff. No retry.
            for (id, events) in tape.waiting.lock().unwrap().drain(..) {
                events.send(HostEvent::HttpFailed(id, "synthetic disconnected after handoff".into())).unwrap();
            }
            let result = action_frame(&mut sent, "order.actionOutcome").await;
            assert_eq!(result["d"]["outcome"], "unknown", "{result}");
            assert!(result["d"]["result"].is_null());
            assert_eq!(tape.writes.lock().unwrap().len(), 1);
            session.close(); registry.dispose(&account, Disposal::PageDisposed);
        }
    }).await;
}

#[tokio::test]
async fn full_write_protection_shapes_reach_native_signing_without_a_forced_caller_id() {
    tokio::task::LocalSet::new().run_until(async {
        for order in [
            json!({"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":0.001,"price":49000,"reduceOnly":true}),
            json!({"symbol":"BTCUSDT","type":"take_profit_market","side":"sell","amount":0.001,"price":51000,"reduceOnly":true}),
            json!({"symbol":"BTCUSDT","type":"conditional","side":"buy","amount":0.001,"triggerPrice":51000}),
            json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.001,"price":50000,"stopLoss":49000,"takeProfit":51000}),
        ] {
            let registry = crate::accounts::tests::registry(); let tape = Tape::default();
            let account = delegated_account(&registry, "A", "bybit", tape.clone());
            let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
            action(&mut session, json!({"kind":"place","orders":[order.clone()]}), 1, false).await;
            let result = action_frame(&mut sent, "order.actionOutcome").await;
            assert_eq!(result["d"]["outcome"], "applied", "{order}: {result}");
            assert!(!tape.writes.lock().unwrap().is_empty(), "{order}");
            if matches!(order["type"].as_str(), Some("stop_market" | "take_profit_market" | "trailing_stop_market")) {
                assert_eq!(result["d"]["result"]["protectionApplied"], true);
                assert_eq!(result["d"]["result"]["orderIds"], json!([]));
                assert!(tape.requests.lock().unwrap().iter().any(|r| r["path"].as_str().is_some_and(|p| p.ends_with("/position/trading-stop"))));
            }
            assert!(account.link.is_none());
            session.close(); registry.dispose(&account, Disposal::PageDisposed);
        }
    }).await;
}

#[tokio::test]
async fn full_write_leverage_fanout_cannot_refuse_after_another_leg_completed() {
    let (out, mut sent, _) = Out::channel(8);
    let group = Arc::new(Mutex::new(Group {
        remaining: 2,
        failures: Vec::new(),
    }));
    let pending = || Pending {
        request_id: json!("same-action"),
        method: "setAllLeverage".into(),
        reply: methods::Reply::Action {
            kind: "setAllLeverage",
            seq: 1,
            unit: true,
            idless_protection: false,
        },
        group: Some(group.clone()),
        incomplete: false,
        audit: None,
    };
    settle(&out, "main", pending(), Ok(Value::Null));
    assert!(sent.try_recv().is_err());
    settle(
        &out,
        "main",
        pending(),
        Err(json!({"message":"second leg refused","outcome":"notSent"})),
    );
    let result: Value = serde_json::from_str(&sent.recv().await.unwrap()).unwrap();
    assert_eq!(result["d"]["outcome"], "unknown");
    assert_eq!(result["d"]["error"]["outcome"], "unknown");
}

#[tokio::test]
async fn full_write_algo_queued_for_alice_is_dropped_when_bob_replaces_the_login() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let a_tape = Tape::default();
            let b_tape = Tape::default();
            let a = delegated_account_with_algos(&registry, "A", "bybit", a_tape.clone(), true);
            let b = delegated_account_with_algos(&registry, "B", "bybit", b_tape.clone(), true);
            let (mut alice, mut a_sent) = joined(registry.clone(), "A", a.clone()).await;
            let (mut bob, mut b_sent) = joined(registry.clone(), "B", b.clone()).await;
            let (keeper, _kept, _) = Out::channel(256);
            crate::accounts::tests::hold_live(&a, 9999, keeper.outbox());
            // This fixture's actors share the current LocalSet. The typed algo
            // admission path contains no pending await: it enqueues the Op and
            // returns before that actor can execute. Retire A before yielding.
            action(
                &mut alice,
                json!({"kind":"algo","op":"exec","data":{"command":"buy $100","symbol":"BTCUSDT"}}),
                1,
                false,
            )
            .await;
            assert!(a_tape.writes.lock().unwrap().is_empty());
            alice.close();
            tokio::task::yield_now().await;
            action(
                &mut bob,
                json!({"kind":"algo","op":"exec","data":{"command":"buy $100","symbol":"BTCUSDT"}}),
                1,
                false,
            )
            .await;
            assert_eq!(
                action_frame(&mut b_sent, "order.actionOutcome").await["d"]["outcome"],
                "unknown"
            );
            tokio::time::timeout(Duration::from_secs(2), async {
                while b_tape.writes.lock().unwrap().is_empty() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert!(
                a_tape.writes.lock().unwrap().is_empty(),
                "retired A admission must not execute on the still-live account"
            );
            while let Ok(text) = a_sent.try_recv() {
                let frame: Value = serde_json::from_str(&text).unwrap();
                assert_ne!(frame["t"], "order.actionOutcome");
            }
            bob.close();
            registry.dispose(&a, Disposal::PageDisposed);
            registry.dispose(&b, Disposal::PageDisposed);
        })
        .await;
}

#[tokio::test]
async fn full_write_execution_constraints_are_never_silently_dropped_or_partially_sent() {
    tokio::task::LocalSet::new().run_until(async {
        for (field, value) in [("stpMode", json!("EXPIRE_TAKER")), ("impliedVolatility", json!(0.5))] {
            let registry = crate::accounts::tests::registry(); let tape = Tape::default();
            let account = delegated_account(&registry, "A", "bybit", tape.clone());
            let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
            let good = json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.001,"price":50000});
            let mut unsupported = good.clone(); unsupported[field] = value.clone();
            action(&mut session, json!({"kind":"place","orders":[good,unsupported.clone()]}), 1, false).await;
            let result = action_frame(&mut sent, "order.actionOutcome").await;
            assert_eq!(result["d"]["outcome"], "refused", "{field}: {result}");
            assert_eq!(result["d"]["error"]["outcome"], "notSent");
            assert!(tape.writes.lock().unwrap().is_empty(), "whole batch refuses before first write");
            // The existing single-placement family has the same LOCAL guard.
            let id = action_key();
            let fp = crate::order_contract::fingerprint("synthetic-main", &id, &unsupported, 1).unwrap();
            session.handle(&json!({"t":"order.submit","d":{"accountName":"synthetic-main","intentId":id,"seq":1,"ageMs":0,"fingerprint":fp,"params":unsupported}}).to_string()).await.unwrap();
            let result = action_frame(&mut sent, "order.outcome").await;
            assert_eq!(result["d"]["outcome"], "refused", "{field}: {result}");
            assert!(tape.writes.lock().unwrap().is_empty());
            session.close(); registry.dispose(&account, Disposal::PageDisposed);
        }
    }).await;
}

#[tokio::test]
async fn full_write_keeps_local_unit_reply_semantics_after_native_completion() {
    let (out, mut sent, _) = Out::channel(8);
    let pending = |unit| Pending {
        request_id: json!("setting"),
        method: "setLeverage".into(),
        reply: methods::Reply::Action {
            kind: "setLeverage",
            seq: 1,
            unit,
            idless_protection: false,
        },
        group: None,
        incomplete: false,
        audit: None,
    };
    // Bybit's complete trading plan returns an empty ID array for a setting.
    // The existing method translator maps that native completion to unit.
    settle(&out, "main", pending(true), Ok(json!([])));
    assert_eq!(
        action_frame(&mut sent, "order.actionOutcome").await["d"]["outcome"],
        "applied"
    );
    settle(
        &out,
        "main",
        pending(true),
        Err(json!({"message":"interrupted","outcome":"unknown"})),
    );
    assert_eq!(
        action_frame(&mut sent, "order.actionOutcome").await["d"]["outcome"],
        "unknown"
    );
    settle(&out, "main", pending(false), Ok(json!([])));
    assert_eq!(
        action_frame(&mut sent, "order.actionOutcome").await["d"]["outcome"],
        "unknown"
    );
}

// Forward-compatible v1 additions must never execute a recognized subset.
#[tokio::test]
async fn v1_unknown_operations_and_fields_return_typed_unsupported_without_io() {
    tokio::task::LocalSet::new().run_until(async {
        let registry = crate::accounts::tests::registry();
        let tape = Tape::default();
        let account = delegated_account_with_algos(&registry, "A", "bybit", tape.clone(), true);
        let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
        let bound = session.account_out("synthetic-main").with_instance_id(Some("reg-v1"));
        session.join(account.clone(), bound).await;
        for case in ["kind", "duplicateKind", "actionField", "opVersion", "targetField", "updateField", "orderField", "bboField", "algoOp", "algoData", "twapMarket", "requestField", "envelopeField"] {
            let mut payload = match case {
                "kind" => json!({"kind":"futureWrite"}),
                "targetField" => json!({"kind":"cancel","order":{"id":"resting-order","symbol":"BTCUSDT","future":true}}),
                "updateField" => json!({"kind":"amend","order":{"id":"resting-order","symbol":"BTCUSDT"},"update":{"price":51000,"future":true}}),
                "orderField" => json!({"kind":"place","orders":[{"symbol":"BTCUSDT","type":"market","side":"buy","amount":0.001,"future":true}]}),
                "bboField" => json!({"kind":"place","orders":[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.001,"bybitBbo":{"sideType":"Queue","level":1,"future":true}}]}),
                "algoOp" => json!({"kind":"algo","op":"future:start","data":{}}),
                "algoData" => json!({"kind":"algo","op":"exec","data":{"command":"buy $100","future":true}}),
                "twapMarket" => {
                    let cases: Value = serde_json::from_str(include_str!("../tests/data/delegated-full-writes-v1.json")).unwrap();
                    let mut action = cases.as_array().unwrap().iter().find(|v| v["action"]["op"] == "twap:start").unwrap()["action"].clone();
                    action["data"]["activeMarket"]["future"] = json!(true);
                    action
                }
                _ => json!({"kind":"cancelAll"}),
            };
            if case == "actionField" { payload["future"] = json!(true); }
            if case == "opVersion" { payload["version"] = json!(2); }
            let id = action_key();
            // A newer client can hash its unknown schema. These ASCII test values
            // have the same sorted JSON/number spelling as the v1 canonicalizer.
            let canonical = format!("tealstreet/delegated-action/v1\0{}", json!({"accountName":"synthetic-main","action":payload,"intentId":id,"seq":1,"version":1}));
            let fp: String = safe_cex_crypto::sha256(canonical.as_bytes()).iter().map(|v| format!("{v:02x}")).collect();
            let mut frame = json!({"t":"order.action","d":{"version":1,"accountName":"synthetic-main","instanceId":"reg-v1","intentId":id,"seq":1,"ageMs":0,"fingerprint":fp,"action":payload}});
            if case == "requestField" { frame["d"]["future"] = json!(true); }
            if case == "envelopeField" { frame["future"] = json!(true); }
            let wire = frame.to_string();
            let wire = if case == "duplicateKind" {
                wire.replace(r#""action":{"kind":"cancelAll"}"#, r#""action":{"kind":"futureWrite","kind":"cancelAll"}"#)
            } else { wire };
            session.handle(&wire).await.unwrap();
            // Rejection is synchronous; do not hide a generic error behind a timeout.
            let replies: Vec<Value> = std::iter::from_fn(|| sent.try_recv().ok()).map(|v| serde_json::from_str(&v).unwrap()).collect();
            let result = replies.iter().find(|v| v["t"] == "order.actionOutcome").unwrap_or_else(|| panic!("{case}: missing typed unsupported: {replies:?}"));
            assert_eq!(result["d"]["version"], 1, "{case}");
            assert_eq!(result["d"]["instanceId"], "reg-v1", "{case}");
            assert_eq!(result["d"]["intentId"], id, "{case}");
            assert_eq!(result["d"]["outcome"], "unsupported", "{case}: {result}");
            assert_eq!(result["d"]["result"], Value::Null);
            assert_eq!(result["d"]["error"]["outcome"], "unsupported");
            assert!(replies.iter().all(|v| v["t"] != "order.actionAck" || v["d"]["received"] != true));
            assert!(tape.writes.lock().unwrap().is_empty(), "{case}");
        }

        // An unsupported payload is not permission to bypass admission, hashing
        // or age bounds, nor to stamp a rejection on another registration.
        for control in ["binding", "fingerprint", "age", "sequence"] {
            let id = action_key();
            let payload = json!({"kind":"futureWrite"});
            let canonical = format!("tealstreet/delegated-action/v1\0{}", json!({"accountName":"synthetic-main","action":payload,"intentId":id,"seq":1,"version":1}));
            let fp: String = safe_cex_crypto::sha256(canonical.as_bytes()).iter().map(|v| format!("{v:02x}")).collect();
            let mut frame = json!({"t":"order.action","d":{"version":1,"accountName":"synthetic-main","instanceId":"reg-v1","intentId":id,"seq":1,"ageMs":0,"fingerprint":fp,"action":payload}});
            match control {
                "binding" => frame["d"]["instanceId"] = json!("someone-else"),
                "fingerprint" => frame["d"]["fingerprint"] = json!("0".repeat(64)),
                "age" => frame["d"]["ageMs"] = json!(10_000),
                _ => frame["d"]["seq"] = json!(0),
            }
            session.handle(&frame.to_string()).await.unwrap();
            let result = action_frame(&mut sent, "order.actionOutcome").await;
            assert_eq!(result["d"]["outcome"], "refused", "{control}: {result}");
            assert_eq!(result["d"]["error"]["outcome"], "notSent", "{control}");
            assert_eq!(result["d"]["instanceId"], "reg-v1");
            assert!(tape.writes.lock().unwrap().is_empty());
        }
        session.close(); registry.dispose(&account, Disposal::PageDisposed);
    }).await;
}

async fn exec_review_result(sent: &mut crate::out::Receiver, id: &str) -> Value {
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(text) = sent.recv().await {
            let frame: Value = serde_json::from_str(&text).unwrap();
            if frame["t"] == "msg"
                && frame["d"]["op"] == "exec-result"
                && frame["d"]["data"]["reqId"] == id
            {
                return frame["d"]["data"].clone();
            }
        }
        panic!("missing EXEC result for {id}")
    })
    .await
    .expect("bounded EXEC factory seed result")
}

#[tokio::test]
async fn exec_review_f2_keeps_factory_identity_across_legacy_and_overlapping_typed_calls() {
    tokio::task::LocalSet::new().run_until(async {
        for prior_legacy in [true, false] {
            let registry = crate::accounts::tests::registry();
            let tape = Tape { hold: true, ..Default::default() };
            let account = delegated_account_with_algos(&registry, "A", "bybit", tape.clone(), true);
            let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
            if prior_legacy {
                // Master intentionally refuses this legacy DELEGATED WS op.
                // A refusal must not masquerade as a seeded native factory.
                session.handle(&json!({"t":"msg","d":{"accountName":"synthetic-main","op":"exec","data":{"command":"tasks","reqId":"legacy-ws-refused","symbol":"BTCUSDT"}}}).to_string()).await.unwrap();
                let refused = exec_review_result(&mut sent, "legacy-ws-refused").await;
                assert_eq!(refused["success"], false);
                assert!(refused["error"].as_str().unwrap().contains("DelegatedActionUnavailable"));
                assert!(tape.writes.lock().unwrap().is_empty());
                // Exercise the resident factory's actual legacy entry (used by
                // COSIGNED/native callers), without reopening the refused WS API.
                account.algos.as_ref().unwrap().send(crate::algos::AlgoInput::Op {
                    op: "exec".into(),
                    data: json!({"command":"tasks","reqId":"legacy-before-typed","symbol":"BTCUSDT"}).to_string(),
                    origin: "native-legacy-identity-control".into(),
                    twap_owner: account.twap_owner.snapshot(),
                    action: None,
                }).unwrap();
                let seed = exec_review_result(&mut sent, "legacy-before-typed").await;
                assert_eq!(seed["success"], true, "native legacy factory seed never ran: {seed}");
            } else {
                action(&mut session, json!({"kind":"algo","op":"exec","data":{"command":"buy $100","symbol":"BTCUSDT"}}), 1, false).await;
                action_frame(&mut sent, "order.actionOutcome").await;
                tokio::time::timeout(Duration::from_secs(2), async {
                    while tape.waiting.lock().unwrap().is_empty() { tokio::task::yield_now().await; }
                }).await.expect("first typed EXEC must remain in flight");
            }
            action(&mut session, json!({"kind":"algo","op":"exec","data":{"command":"buy $100","symbol":"BTCUSDT"}}), 1, false).await;
            action_frame(&mut sent, "order.actionOutcome").await;
            let expected = if prior_legacy { 1 } else { 2 };
            let passed = tokio::time::timeout(Duration::from_secs(2), async {
                while tape.writes.lock().unwrap().len() < expected { tokio::task::yield_now().await; }
            }).await.is_ok();
            let writes = tape.writes.lock().unwrap().clone();
            let replies: Vec<String> = std::iter::from_fn(|| sent.try_recv().ok()).collect();
            tape.finish_unknown();
            session.close(); registry.dispose(&account, Disposal::PageDisposed);
            assert!(passed, "legacy={prior_legacy}: expected {expected} signed writes, got {writes:?}; {replies:?}");
            assert!(!replies.iter().any(|v| v.contains("ExecFactory must be account-scoped")), "{replies:?}");
        }
    }).await;
}

#[tokio::test]
async fn exec_review_f1_fences_deferred_shared_factory_handoffs_without_stopping_accepted_jobs() {
    tokio::task::LocalSet::new().run_until(async {
        for command in ["chase buy $100", "twap buy $1000 into 2 over 1"] {
            for control in ["retired", "expired", "live"] {
                let registry = crate::accounts::tests::registry();
                let tape = Tape::default();
                let account = delegated_account_with_algos(&registry, "A", "bybit", tape.clone(), true);
                let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
                let (keeper, _kept, _) = Out::channel(256);
                crate::accounts::tests::hold_live(&account, 9999, keeper.outbox());
                let delay = if control == "live" {100} else {1500};
                let payload = json!({"kind":"algo","op":"exec","data":{"command":format!("sleep {delay}ms; {command}"),"symbol":"BTCUSDT"}});
                let id = action_key();
                let fp = crate::order_contract::action_fingerprint("synthetic-main", &id, &payload, 1).unwrap();
                session.handle(&json!({"t":"order.action","d":{"version":1,"accountName":"synthetic-main","intentId":id,"seq":1,"ageMs":if control != "retired" {9000}else{0},"fingerprint":fp,"action":payload}}).to_string()).await.unwrap();
                let accepted = action_frame(&mut sent, "order.actionOutcome").await;
                assert_eq!(accepted["d"]["outcome"], "unknown");
                assert!(account.running.jobs().1 > 0, "EXEC must reach its sleep deferral before retirement/expiry");
                assert!(tape.writes.lock().unwrap().is_empty());
                let mut remaining_session = Some(session);
                if control == "retired" { remaining_session.take().unwrap().close(); }
                assert!(account.running.io_allowed(), "keep the account live so only the action fence can stop its continuation");
                // A fresh owner's independent account continues DURING A's
                // sleep deferral; A must never acquire Bob's authority.
                if control == "retired" {
                    let b_tape = Tape::default();
                    let b = delegated_account_with_algos(&registry, "B", "bybit", b_tape.clone(), true);
                    let (mut bob, mut b_sent) = joined(registry.clone(), "B", b.clone()).await;
                    action(&mut bob, json!({"kind":"algo","op":"exec","data":{"command":"buy $100","symbol":"BTCUSDT"}}), 1, false).await;
                    action_frame(&mut b_sent, "order.actionOutcome").await;
                    tokio::time::timeout(Duration::from_secs(2), async {
                        while b_tape.writes.lock().unwrap().is_empty() { tokio::task::yield_now().await; }
                    }).await.expect("Bob's independent EXEC still signs and sends");
                    bob.close(); registry.dispose(&b, Disposal::PageDisposed);
                }
                tokio::time::sleep(Duration::from_millis(1800)).await;
                let writes = tape.writes.lock().unwrap().clone();
                let jobs = account.running.jobs();
                if let Some(session) = remaining_session { session.close(); }
                registry.dispose(&account, Disposal::PageDisposed);
                if control == "live" {
                    assert!(!writes.is_empty(), "{command}: positive handoff must sign and send");
                    assert!(!jobs.0.is_empty(), "accepted long-lived jobs keep account scope");
                } else {
                    assert!(writes.is_empty(), "{control} {command}: deferred admission sent {writes:?}");
                    assert!(jobs.0.is_empty(), "{control} {command}: deferred admission created jobs {:?}", jobs.0);
                }
            }
        }
    }).await;
}

// Exercise the actual authenticated v1 boundary, not only native serde parsing.
async fn j6_chaser_schema_rejection(mutation: &str) {
    tokio::task::LocalSet::new().run_until(async {
        for valid in [true, false] {
            let registry = crate::accounts::tests::registry();
            let tape = Tape::default();
            let account = delegated_account_with_algos(&registry, "A", "bybit", tape.clone(), true);
            let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
            let mut data = json!({"symbol":"BTCUSDT","size":0.001,"side":"buy","maxAttempts":3,"delay":500,
                "terminate":{"type":"price","value":1_000_000,"anchorPrice":50000}});
            if !valid {
                match mutation {
                    "terminate" => data["terminate"]["future"] = json!(true),
                    "delay" => data["delay"] = json!("invalid"),
                    _ => unreachable!(),
                }
            }
            let payload = json!({"kind":"algo","op":"chaser:start","data":data});
            let id = action_key();
            // Hash the exact newer-client payload even if the local schema rejects it.
            let fp = crate::order_contract::action_wire_fingerprint("synthetic-main", &id, &payload, 1).unwrap();
            session.handle(&json!({"t":"order.action","d":{"version":1,"accountName":"synthetic-main",
                "intentId":id,"seq":1,"ageMs":0,"fingerprint":fp,"action":payload}}).to_string()).await.unwrap();
            let (result, replies) = tokio::time::timeout(Duration::from_secs(5), async {
                let mut replies = Vec::new();
                while let Some(text) = sent.recv().await {
                    let frame: Value = serde_json::from_str(&text).unwrap();
                    replies.push(frame.clone());
                    if frame["t"] == "order.actionOutcome" { return (frame, replies); }
                }
                panic!("missing chaser outcome");
            }).await.expect("bounded chaser schema outcome");
            if valid || result["d"]["outcome"] != "unsupported" {
                tokio::time::timeout(Duration::from_secs(5), async {
                    while tape.writes.lock().unwrap().is_empty() { tokio::task::yield_now().await; }
                }).await.expect("valid control or pre-fix regression must reach native signing/transport");
            }
            let writes = tape.writes.lock().unwrap().clone();
            let jobs = account.running.jobs();
            session.close(); registry.dispose(&account, Disposal::PageDisposed);
            assert_eq!(result["d"]["version"], 1);
            assert_eq!(result["d"]["intentId"], id);
            if valid {
                assert_eq!(result["d"]["outcome"], "unknown", "valid chaser: {result}");
                assert!(writes.iter().any(|v| v == "create"), "valid control must sign and send");
                assert!(replies.iter().any(|v| v["t"] == "order.actionAck" && v["d"]["received"] == true));
            } else {
                assert_eq!(result["d"]["outcome"], "unsupported", "{mutation}: {result}, writes={writes:?}");
                assert_eq!(result["d"]["result"], Value::Null);
                assert_eq!(result["d"]["error"]["outcome"], "unsupported");
                assert!(replies.iter().all(|v| v["t"] != "order.actionAck" || v["d"]["received"] != true));
                assert!(writes.is_empty(), "{mutation}: native write escaped");
                assert!(jobs.0.is_empty(), "{mutation}: job admitted");
            }
        }
    }).await;
}

#[tokio::test]
async fn j6_chaser_rejects_unknown_termination_fields_without_ack_job_or_write() {
    j6_chaser_schema_rejection("terminate").await;
}

#[tokio::test]
async fn j6_chaser_rejects_invalid_delay_without_ack_job_or_write() {
    j6_chaser_schema_rejection("delay").await;
}

#[tokio::test]
async fn j6b_settings_and_lifecycle_refuse_each_unadmitted_connection() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let cases: Vec<Value> =
                serde_json::from_str(include_str!("../tests/data/delegated-full-writes-v1.json"))
                    .unwrap();
            for case in cases {
                let payload = &case["action"];
                if !matches!(
                    payload["kind"].as_str(),
                    Some(
                        "setMarginType"
                            | "setPositionMode"
                            | "scheduleCancel"
                            | "setStpMode"
                            | "setAccountTier"
                            | "stopAlgos"
                            | "algo"
                    )
                ) {
                    continue;
                }
                for admission in ["unjoined", "pending", "disabled"] {
                    let registry = crate::accounts::tests::registry();
                    let tape = Tape::default();
                    let account =
                        delegated_account_with_algos(&registry, "A", "bybit", tape.clone(), true);
                    let (mut session, mut sent) =
                        joined(registry.clone(), "A", account.clone()).await;
                    match admission {
                        "unjoined" => {
                            session.joined.clear();
                        }
                        "pending" => {
                            session.start_verification(
                                "synthetic-main".into(),
                                std::future::pending(),
                            );
                        }
                        "disabled" => {
                            session.order_frames_enabled = false;
                        }
                        _ => unreachable!(),
                    }
                    j6b_raw_action(&mut session, payload.clone(), 0).await;
                    let outcome = action_frame(&mut sent, "order.actionOutcome").await;
                    assert_eq!(
                        outcome["d"]["outcome"], "refused",
                        "{admission}: {payload}: {outcome}"
                    );
                    assert_eq!(outcome["d"]["error"]["outcome"], "notSent");
                    tokio::task::yield_now().await;
                    assert!(
                        tape.writes.lock().unwrap().is_empty(),
                        "{admission}: {payload}"
                    );
                    assert_eq!(account.running.counts(), (0, 0), "{admission}: {payload}");
                    session.close();
                    registry.dispose(&account, Disposal::PageDisposed);
                }
            }
        })
        .await;
}

async fn j6b_raw_action(session: &mut Session, payload: Value, age: u64) {
    let id = action_key();
    let fp =
        crate::order_contract::action_wire_fingerprint("synthetic-main", &id, &payload, 1).unwrap();
    session.handle(&json!({"t":"order.action","d":{"version":1,"accountName":"synthetic-main","intentId":id,"seq":1,"ageMs":age,"fingerprint":fp,"action":payload}}).to_string()).await.unwrap();
}
fn j6b_options(family: &str) -> Value {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("../tests/data/delegated-full-writes-v1.json")).unwrap();
    cases
        .into_iter()
        .find(|c| c["action"]["op"] == format!("{family}:start"))
        .unwrap()["action"]["data"]
        .clone()
}
async fn j6b_wait_writes(tape: &Tape, count: usize) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while native_request_rows(tape, "create").len() < count {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("native signed placements");
}

async fn j6b_wait_idle(account: &Account) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while account.running.any() || account.running.jobs().1 != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("existing lifecycle retires resident jobs");
}

#[tokio::test]
async fn j6b_chaser_start_and_each_stop_use_the_resident_signed_factory() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for stop in ["chaser:terminate", "chaser:terminateAll", "stopAlgos"] {
                let registry = crate::accounts::tests::registry();
                let tape = Tape::default();
                let account =
                    delegated_account_with_algos(&registry, "A", "bybit", tape.clone(), true);
                let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
                action(
                    &mut session,
                    json!({"kind":"algo","op":"chaser:start","data":j6b_options("chaser")}),
                    1,
                    false,
                )
                .await;
                assert_eq!(
                    action_frame(&mut sent, "order.actionOutcome").await["d"]["outcome"],
                    "unknown"
                );
                j6b_wait_writes(&tape, 1).await;
                let jobs = account.running.jobs().0;
                assert_eq!(jobs.len(), 1);
                assert_eq!(jobs[0].kind, "chase");
                let payload = match stop {
                    "stopAlgos" => json!({"kind":"stopAlgos"}),
                    "chaser:terminateAll" => json!({"kind":"algo","op":stop,"data":{}}),
                    _ => json!({"kind":"algo","op":stop,"data":jobs[0].id}),
                };
                action(&mut session, payload, 1, false).await;
                let outcome = action_frame(&mut sent, "order.actionOutcome").await;
                assert_eq!(
                    outcome["d"]["outcome"],
                    if stop == "stopAlgos" {
                        "applied"
                    } else {
                        "unknown"
                    },
                    "{stop}: {outcome}"
                );
                j6b_wait_idle(&account).await;
                assert_eq!(
                    native_request_rows(&tape, "create").len(),
                    1,
                    "{stop}: no replacement"
                );
                assert!(
                    account.link.is_none(),
                    "hosted signing has no browser signer"
                );
                session.close();
                registry.dispose(&account, Disposal::PageDisposed);
            }
        })
        .await;
}

#[tokio::test]
async fn j6b_twap_pause_resume_and_stop_preserve_the_existing_job() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let tape = Tape::default();
            let account = delegated_account_with_algos(&registry, "A", "bybit", tape.clone(), true);
            let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
            action(
                &mut session,
                json!({"kind":"algo","op":"twap:start","data":j6b_options("twap")}),
                1,
                false,
            )
            .await;
            assert_eq!(
                action_frame(&mut sent, "order.actionOutcome").await["d"]["outcome"],
                "unknown"
            );
            j6b_wait_writes(&tape, 1).await;
            let id = account.running.jobs().0[0].id.clone();
            for (op, parked) in [("twap:pause", true), ("twap:resume", false)] {
                action(
                    &mut session,
                    json!({"kind":"algo","op":op,"data":id}),
                    1,
                    false,
                )
                .await;
                assert_eq!(
                    action_frame(&mut sent, "order.actionOutcome").await["d"]["outcome"],
                    "unknown"
                );
                let jobs = account.running.jobs().0;
                assert_eq!(jobs.len(), 1);
                assert_eq!(jobs[0].id, id);
                assert_eq!(jobs[0].parked, parked, "{op}");
                assert_eq!(
                    native_request_rows(&tape, "create").len(),
                    1,
                    "{op}: no duplicate first slice"
                );
            }
            action(
                &mut session,
                json!({"kind":"algo","op":"twap:stop","data":id}),
                1,
                false,
            )
            .await;
            assert_eq!(
                action_frame(&mut sent, "order.actionOutcome").await["d"]["outcome"],
                "unknown"
            );
            j6b_wait_idle(&account).await;
            action(
                &mut session,
                json!({"kind":"algo","op":"twap:cleanup","data":id}),
                1,
                false,
            )
            .await;
            assert_eq!(
                action_frame(&mut sent, "order.actionOutcome").await["d"]["outcome"],
                "unknown"
            );
            assert_eq!(native_request_rows(&tape, "create").len(), 1);
            assert!(account.link.is_none());
            session.close();
            registry.dispose(&account, Disposal::PageDisposed);
        })
        .await;
}

#[tokio::test]
async fn j6b_restored_twap_stays_paused_until_explicit_resume_and_account_stop() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let tape = Tape::default();
            let account = delegated_account_with_algos(&registry, "A", "bybit", tape.clone(), true);
            let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
            let cases: Vec<Value> =
                serde_json::from_str(include_str!("../tests/data/delegated-full-writes-v1.json"))
                    .unwrap();
            let mut restore = cases
                .into_iter()
                .find(|c| c["action"]["op"] == "twap:state")
                .unwrap()["action"]
                .clone();
            for row in restore["data"].as_array_mut().unwrap() {
                row["ownerId"] = account.registry_name().into();
                row["ownerGeneration"] = account.twap_owner.snapshot().generation.into();
            }
            action(&mut session, restore, 1, false).await;
            assert_eq!(
                action_frame(&mut sent, "order.actionOutcome").await["d"]["outcome"],
                "unknown"
            );
            let id = account.running.jobs().0[0].id.clone();
            assert!(account.running.jobs().0[0].parked);
            assert!(tape.writes.lock().unwrap().is_empty());
            action(
                &mut session,
                json!({"kind":"algo","op":"twap:resume","data":id}),
                1,
                false,
            )
            .await;
            assert_eq!(
                action_frame(&mut sent, "order.actionOutcome").await["d"]["outcome"],
                "unknown"
            );
            j6b_wait_writes(&tape, 1).await;
            assert_eq!(account.running.jobs().0[0].id, id);
            assert!(!account.running.jobs().0[0].parked);
            action(&mut session, json!({"kind":"stopAlgos"}), 1, false).await;
            assert_eq!(
                action_frame(&mut sent, "order.actionOutcome").await["d"]["outcome"],
                "applied"
            );
            j6b_wait_idle(&account).await;
            assert_eq!(native_request_rows(&tape, "create").len(), 1);
            session.close();
            registry.dispose(&account, Disposal::PageDisposed);
        })
        .await;
}

#[tokio::test]
async fn j6b_parameter_amendments_remain_unsupported_without_mutating_running_jobs() {
    tokio::task::LocalSet::new().run_until(async {
        for family in ["chaser","twap"] {
            let registry=crate::accounts::tests::registry(); let tape=Tape::default();
            let account=delegated_account_with_algos(&registry,"A","bybit",tape.clone(),true);
            let (mut session,mut sent)=joined(registry.clone(),"A",account.clone()).await;
            let mut options=j6b_options(family);
            action(&mut session,json!({"kind":"algo","op":format!("{family}:start"),"data":options}),1,false).await;
            assert_eq!(action_frame(&mut sent,"order.actionOutcome").await["d"]["outcome"],"unknown");
            j6b_wait_writes(&tape,1).await;
            let id=account.running.jobs().0[0].id.clone(); options["size"]=0.006.into();
            j6b_raw_action(&mut session,json!({"kind":"algo","op":format!("{family}:amend"),"data":{"id":id,"options":options}}),0).await;
            let outcome=action_frame(&mut sent,"order.actionOutcome").await;
            assert_eq!(outcome["d"]["outcome"],"unsupported","{family}: {outcome}");
            assert_eq!(outcome["d"]["error"]["outcome"],"unsupported");
            assert_eq!(outcome["d"]["result"],Value::Null);
            assert_eq!(account.running.jobs().0[0].id,id);
            assert_eq!(native_request_rows(&tape,"create").len(),1);
            session.close(); registry.dispose(&account,Disposal::PageDisposed);
        }
    }).await;
}

#[tokio::test]
async fn j6b_duplicate_algo_fields_are_typed_unsupported_before_ack_or_native_io() {
    tokio::task::LocalSet::new().run_until(async {
        for family in ["chaser","twap"] {
            for duplicate in ["size", "object", "array"] {
                let registry=crate::accounts::tests::registry(); let tape=Tape::default();
                let account=delegated_account_with_algos(&registry,"A","bybit",tape.clone(),true);
                let (mut session,mut sent)=joined(registry.clone(),"A",account.clone()).await;
                let id=action_key();
                let mut payload=json!({"kind":"algo","op":format!("{family}:start"),"data":j6b_options(family)});
                if duplicate=="object" && family=="chaser" {
                    payload["data"]["terminate"]=json!({"type":"price","value":51000});
                }
                if duplicate=="array" {
                    let cases: Vec<Value>=serde_json::from_str(include_str!("../tests/data/delegated-full-writes-v1.json")).unwrap();
                    payload=cases.into_iter().find(|c| c["action"]["op"]=="twap:state").unwrap()["action"].clone();
                }
                let fp=crate::order_contract::action_wire_fingerprint("synthetic-main",&id,&payload,1).unwrap();
                let wire=json!({"t":"order.action","d":{"version":1,"accountName":"synthetic-main","intentId":id,"seq":1,"ageMs":0,"fingerprint":fp,"action":payload}}).to_string();
                let wire=match duplicate {
                    "object" if family=="twap"=>wire.replacen("\"activeMarket\":","\"activeMarket\":{\"future\":true},\"activeMarket\":",1),
                    "object"=>wire.replacen("\"terminate\":","\"terminate\":{\"future\":true},\"terminate\":",1),
                    _=>wire.replacen("\"size\":","\"size\":{},\"size\":",1),
                };
                assert!(crate::order_actions::decode_request(&wire).is_err(),"{family}/{duplicate}: ambiguous raw schema accepted");
                session.handle(&wire).await.unwrap();
                let replies: Vec<Value>=std::iter::from_fn(||sent.try_recv().ok()).map(|s|serde_json::from_str(&s).unwrap()).collect();
                let outcome=replies.iter().find(|v|v["t"]=="order.actionOutcome").unwrap_or_else(||panic!("{family}/{duplicate}: missing typed unsupported: {replies:?}"));
                assert_eq!(outcome["d"]["intentId"],id);
                assert_eq!(outcome["d"]["outcome"],"unsupported");
                assert_eq!(outcome["d"]["error"]["outcome"],"unsupported");
                assert!(replies.iter().all(|v|v["t"]!="order.actionAck"||v["d"]["received"]!=true));
                assert!(tape.writes.lock().unwrap().is_empty()); assert!(account.running.jobs().0.is_empty());
                session.close(); registry.dispose(&account,Disposal::PageDisposed);
            }
        }
    }).await;
}

#[tokio::test]
async fn typed_twap_start_keeps_connection_labels_on_a_shared_identity() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let cases: Vec<Value> =
                serde_json::from_str(include_str!("../tests/data/delegated-full-writes-v1.json"))
                    .unwrap();
            let start = cases
                .iter()
                .find(|case| case["action"]["op"] == "twap:start")
                .unwrap()["action"]
                .clone();
            for label in ["synthetic-main", "phone-label"] {
                let registry = crate::accounts::tests::registry();
                let tape = Tape::default();
                let account =
                    delegated_account_with_algos(&registry, "A", "bybit", tape.clone(), true);
                let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
                let instance = (label != account.name).then_some("phone-registration");
                if instance.is_some() {
                    let out = session
                        .out
                        .account(label)
                        .with_account_label(label)
                        .with_instance_id(instance);
                    account.hold_registration(session.socket, out.outbox(), None, label);
                    session.join(account.clone(), out).await;
                    assert!(Arc::ptr_eq(
                        &account,
                        &registry.held("A", label, session.socket).unwrap()
                    ));
                }
                let mut payload = start.clone();
                payload["data"]["accountName"] = label.into();
                let id = action_key();
                let fingerprint =
                    crate::order_contract::action_fingerprint(label, &id, &payload, 1).unwrap();
                session
                    .handle(
                        &json!({"t":"order.action","d":{"version":1,
                        "accountName":label,"instanceId":instance,"intentId":id,
                        "seq":1,"ageMs":0,"fingerprint":fingerprint,"action":payload}})
                        .to_string(),
                    )
                    .await
                    .unwrap();
                let ack = action_frame(&mut sent, "order.actionAck").await;
                assert_eq!(ack["d"]["received"], true);
                assert_eq!(ack["d"]["accountName"], label);
                assert_eq!(ack["d"]["instanceId"].as_str(), instance);
                let outcome = action_frame(&mut sent, "order.actionOutcome").await;
                assert_eq!(outcome["d"]["outcome"], "unknown");
                assert_eq!(outcome["d"]["accountName"], label);
                assert_eq!(outcome["d"]["instanceId"].as_str(), instance);
                tokio::time::timeout(Duration::from_secs(3), async {
                    while !tape.writes.lock().unwrap().iter().any(|op| op == "create") {
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .unwrap_or_else(|_| {
                    panic!("typed TWAP through {label} must place on its identity")
                });
                assert!(registry.held("B", label, session.socket).is_none());
                session.close();
                registry.dispose(&account, Disposal::PageDisposed);
            }
        })
        .await;
}

#[tokio::test]
async fn typed_algo_admission_keeps_the_initiating_socket_token_expiry() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let cases: Vec<Value> =
                serde_json::from_str(include_str!("../tests/data/delegated-full-writes-v1.json"))
                    .unwrap();
            for op in ["chaser:start", "twap:start"] {
                for expired in [false, true] {
                    let registry = crate::accounts::tests::registry();
                    let tape = Tape::default();
                    let (account, inbox) =
                        delegated_account_with_algo_inbox(&registry, "A", "bybit", tape.clone());
                    let (mut session, mut sent) =
                        joined(registry.clone(), "A", account.clone()).await;
                    let (keeper, _kept, _) = Out::channel(256);
                    account.hold_registration(
                        9999,
                        keeper.outbox(),
                        Some(("valid-keeper".into(), crate::accounts::now_ms() + 60_000)),
                        "synthetic-main",
                    );
                    let expires = crate::accounts::now_ms() + if expired { 500 } else { 60_000 };
                    session.identity.token = Some(("initiating-token".into(), expires));
                    let payload = cases
                        .iter()
                        .find(|case| case["action"]["op"] == op)
                        .unwrap()["action"]
                        .clone();
                    action(&mut session, payload, 1, false).await;
                    assert_eq!(
                        action_frame(&mut sent, "order.actionAck").await["d"]["received"],
                        true
                    );
                    // The actor does not consume its inbox until after this deferral.
                    tokio::time::sleep(Duration::from_millis(650)).await;
                    assert_eq!(session.token_expired(), expired);
                    assert!(session.account_out("synthetic-main").action_active());
                    assert!(account.running.io_allowed());
                    let account =
                        testing::run_algos(account, SignerLink::new(0, Arc::new(|_| false)), inbox);
                    let outcome = action_frame(&mut sent, "order.actionOutcome").await;
                    if !expired {
                        tokio::time::timeout(Duration::from_secs(3), async {
                            while tape.writes.lock().unwrap().is_empty() {
                                tokio::task::yield_now().await;
                            }
                        })
                        .await
                        .expect("valid token must admit the native job and write");
                    }
                    let writes = tape.writes.lock().unwrap().clone();
                    let jobs = account.running.jobs().0;
                    session.close();
                    registry.dispose(&account, Disposal::PageDisposed);
                    if expired {
                        assert!(
                            writes.is_empty(),
                            "{op}: expired queued admission wrote {writes:?}"
                        );
                        assert!(
                            jobs.is_empty(),
                            "{op}: expired queued admission created {jobs:?}"
                        );
                        assert_eq!(outcome["d"]["outcome"], "refused", "{outcome}");
                        assert!(
                            outcome["d"]["error"]["message"]
                                .as_str()
                                .unwrap()
                                .contains("expired"),
                            "{outcome}"
                        );
                    } else {
                        assert!(
                            !jobs.is_empty(),
                            "{op}: positive control did not create a job"
                        );
                        assert_eq!(outcome["d"]["outcome"], "unknown");
                    }
                }
            }
        })
        .await;
}

#[tokio::test]
async fn typed_exec_continuations_keep_socket_expiry_without_expiring_accepted_jobs() {
    tokio::task::LocalSet::new().run_until(async {
        for command in ["chase buy $100", "twap buy $1000 into 2 over 1"] {
            for control in ["expired", "valid", "accepted-job"] {
                let registry = crate::accounts::tests::registry();
                let tape = Tape::default();
                let account = delegated_account_with_algos(&registry, "A", "bybit", tape.clone(), true);
                let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
                let (keeper, _kept, _) = Out::channel(256);
                account.hold_registration(9999, keeper.outbox(), Some(("valid-keeper".into(), crate::accounts::now_ms() + 60_000)), "synthetic-main");
                session.identity.token = Some(("initiating-token".into(), crate::accounts::now_ms() + if control == "valid" { 60_000 } else { 500 }));
                let delay = if control == "expired" { 800 } else { 50 };
                let payload = json!({"kind":"algo","op":"exec","data":{"command":format!("sleep {delay}ms; {command}"),"symbol":"BTCUSDT"}});
                action(&mut session, payload, 1, false).await;
                let outcome = action_frame(&mut sent, "order.actionOutcome").await;
                assert_eq!(outcome["d"]["outcome"], "unknown");
                assert!(account.running.jobs().1 > 0, "EXEC must enter its sleep deferral");
                assert!(tape.writes.lock().unwrap().is_empty());
                tokio::time::sleep(Duration::from_millis(1_100)).await;
                assert_eq!(session.token_expired(), control != "valid");
                assert!(session.account_out("synthetic-main").action_active());
                assert!(account.running.io_allowed(), "the valid holder keeps account IO live");
                let writes = tape.writes.lock().unwrap().clone();
                let jobs = account.running.jobs().0;
                session.close();
                registry.dispose(&account, Disposal::PageDisposed);
                if control == "expired" {
                    assert!(writes.is_empty(), "{command}: expired EXEC continuation wrote {writes:?}");
                    assert!(jobs.is_empty(), "{command}: expired EXEC continuation created {jobs:?}");
                } else {
                    assert!(!writes.is_empty(), "{control} {command}: positive control did not write");
                    assert!(!jobs.is_empty(), "{control} {command}: accepted jobs keep account lifetime");
                }
            }
        }
    }).await;
}

async fn duplicate_order_payload_is_unsupported(mutation: &str) {
    tokio::task::LocalSet::new().run_until(async {
        for unique in [true, false] {
            let registry = crate::accounts::tests::registry();
            let tape = Tape::default();
            let account = delegated_account_with_algos(&registry, "A", "bybit", tape.clone(), true);
            let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
            let mut order = json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.001,"price":50000});
            let payload = if mutation == "reduceOnly" {
                order["side"] = "sell".into();
                order["reduceOnly"] = true.into();
                json!({"kind":"close","order":order})
            } else {
                if matches!(mutation, "bybitBbo" | "level") {
                    order.as_object_mut().unwrap().remove("price");
                    order["bybitBbo"] = json!({"sideType":"Queue","level":1});
                }
                json!({"kind":"place","orders":[order]})
            };
            let id = action_key();
            let fingerprint = crate::order_contract::action_wire_fingerprint("synthetic-main", &id, &payload, 1).unwrap();
            let wire = json!({"t":"order.action","d":{"version":1,"accountName":"synthetic-main",
                "intentId":id,"seq":1,"ageMs":0,"fingerprint":fingerprint,"action":payload}}).to_string();
            let wire = if unique { wire } else {
                let (field, duplicate) = match mutation {
                    "price" => ("\"price\":", "\"price\":0,\"price\":"),
                    "reduceOnly" => ("\"reduceOnly\":", "\"reduceOnly\":false,\"reduceOnly\":"),
                    "bybitBbo" => ("\"bybitBbo\":", "\"bybitBbo\":{\"future\":true},\"bybitBbo\":"),
                    "level" => ("\"level\":", "\"level\":0,\"level\":"),
                    _ => unreachable!(),
                };
                assert!(wire.contains(field));
                wire.replacen(field, duplicate, 1)
            };
            session.handle(&wire).await.unwrap();
            let mut replies = Vec::new();
            let outcome = tokio::time::timeout(Duration::from_secs(5), async {
                while let Some(text) = sent.recv().await {
                    let frame: Value = serde_json::from_str(&text).unwrap();
                    replies.push(frame.clone());
                    if frame["t"] == "order.actionOutcome" { return frame; }
                }
                panic!("missing placement/close outcome");
            }).await.expect("bounded placement/close outcome");
            let writes = tape.writes.lock().unwrap().clone();
            let jobs = account.running.jobs();
            session.close();
            registry.dispose(&account, Disposal::PageDisposed);
            assert_eq!(outcome["d"]["intentId"], id);
            assert_eq!(outcome["d"]["version"], 1);
            if unique {
                assert_eq!(outcome["d"]["outcome"], "applied", "{mutation}: {outcome}");
                assert_eq!(writes, vec!["create"], "{mutation}: unique-key control must sign and send");
                assert!(replies.iter().any(|frame| frame["t"] == "order.actionAck" && frame["d"]["received"] == true));
            } else {
                assert_eq!(outcome["d"]["outcome"], "unsupported", "{mutation}: {outcome}, writes={writes:?}, replies={replies:?}");
                assert_eq!(outcome["d"]["error"]["outcome"], "unsupported");
                assert!(outcome["d"]["result"].is_null());
                assert!(replies.iter().all(|frame| frame["t"] != "order.actionAck"));
                assert!(writes.is_empty(), "{mutation}: duplicate payload wrote {writes:?}");
                assert!(jobs.0.is_empty() && jobs.1 == 0, "{mutation}: duplicate payload admitted native work");
            }
        }
    }).await;
}

#[tokio::test]
async fn typed_place_rejects_duplicate_price_without_ack_job_or_write() {
    duplicate_order_payload_is_unsupported("price").await;
}

#[tokio::test]
async fn typed_close_rejects_duplicate_reduce_only_without_ack_job_or_write() {
    duplicate_order_payload_is_unsupported("reduceOnly").await;
}

#[tokio::test]
async fn typed_place_rejects_shadowed_bbo_without_ack_job_or_write() {
    duplicate_order_payload_is_unsupported("bybitBbo").await;
}

#[tokio::test]
async fn typed_place_rejects_duplicate_bbo_level_without_ack_job_or_write() {
    duplicate_order_payload_is_unsupported("level").await;
}
