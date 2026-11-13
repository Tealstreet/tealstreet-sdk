use super::*;
use safe_cex_engine::{Command, Cx, Response, SocketId, TimerId, Venue};
use std::sync::atomic::AtomicUsize;

/// Full startup readiness, with a count at the actual venue command boundary.
struct ReadyLater {
    commands: Arc<AtomicUsize>,
    ready: Arc<AtomicBool>,
}

impl Venue for ReadyLater {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        cx.set_timer(5.);
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, cx: &mut Cx<'_, ()>, timer: TimerId) {
        if !self.ready.load(Ordering::SeqCst) {
            cx.set_timer(5.);
            return;
        }
        crate::accounts::testing::HeldLoad {
            placed: Arc::default(),
            load_after_ms: None,
            ticker: false,
        }
        .on_timer(cx, timer);
        cx.store.set_balance(Default::default());
    }
    fn on_command(&mut self, cx: &mut Cx<'_, ()>, command: Command) {
        if let Command::CancelSymbolOrders { command, .. } = command {
            self.commands.fetch_add(1, Ordering::SeqCst);
            cx.done(command, "[]".to_owned());
        }
    }
    fn book(&self, _: &str) -> Option<&safe_cex_engine::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn expire(session: &mut Session) {
    session.identity.token.as_mut().unwrap().1 = now_ms();
}

fn expired_reply(sent: &mut crate::out::Receiver) {
    let frames: Vec<_> = std::iter::from_fn(|| sent.try_recv().ok()).collect();
    assert!(
        frames
            .iter()
            .any(|frame| frame.contains("Tealstreet proxy token expired")),
        "expiry refusal: {frames:?}"
    );
}

#[tokio::test]
async fn token_expiry_refuses_initial_admission_in_both_modes() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for mode in ["cosigned", "delegated"] {
                let registry = crate::accounts::tests::registry();
                let mut init = acct_init();
                init.signing_mode = Some(mode.into());
                crate::accounts::testing::trading_account(&registry, "alice", &init);
                let (mut session, mut sent) = user_session(registry.clone(), "alice");
                expire(&mut session);
                session.init(init).await.unwrap();
                expired_reply(&mut sent);
                assert!(session.joined.is_empty());
                assert!(session.pending_verifications.is_empty());
                assert_eq!(registry.account_count(), 1, "resident account stays intact");
                session.close();
            }
        })
        .await;
}

#[tokio::test]
async fn token_expiry_refuses_reinit_and_automatic_rejoin_without_changing_the_holder() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let init = acct_init();
            let account = crate::accounts::testing::trading_account(&registry, "alice", &init);
            let (mut session, mut sent) = user_session(registry.clone(), "alice");
            session.init(init.clone()).await.unwrap();
            let client = session.joined["acct"].client.id;
            while sent.try_recv().is_ok() {}
            expire(&mut session);
            session.init(init).await.unwrap();
            assert_eq!(
                session.joined["acct"].client.id, client,
                "no re-init snapshot"
            );
            expired_reply(&mut sent);
            session.client_dropped("acct", client).await;
            session.account_loaded("acct", client).await;
            assert_eq!(
                session.joined["acct"].client.id, client,
                "no deferred rejoin"
            );
            assert!(registry.holds(&account, session.socket));
            session.close();
        })
        .await;
}

#[tokio::test]
async fn token_expiry_does_not_consume_a_parked_resume() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let init = acct_init();
            let account = crate::accounts::testing::trading_account(&registry, "alice", &init);
            let (mut first, _sent) = user_session(registry.clone(), "alice");
            first.init(init).await.unwrap();
            let resume = first.resume_id.clone();
            first.close();
            let (mut expired, _sent) = user_session(registry.clone(), "alice");
            expired.identity.resume_from = Some(resume.clone());
            expire(&mut expired);
            expired.open().await;
            assert!(expired.joined.is_empty(), "expired socket cannot resume");
            assert!(!registry.holds(&account, expired.socket));
            let (mut fresh, _sent) = user_session(registry.clone(), "alice");
            fresh.identity.resume_from = Some(resume);
            fresh.open().await;
            assert!(
                fresh.joined.contains_key("acct"),
                "valid resume remains available"
            );
            expired.close();
            fresh.close();
        })
        .await;
}

#[tokio::test]
async fn token_expiry_is_rechecked_after_waiting_for_resume_takeover() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let init = acct_init();
            crate::accounts::testing::trading_account(&registry, "alice", &init);
            let (mut first, _sent) = user_session(registry.clone(), "alice");
            first.open().await;
            first.init(init).await.unwrap();
            let resume = first.resume_id.clone();
            let taken_over = first.taken_over();
            let (mut next, _sent) = user_session(registry.clone(), "alice");
            next.identity.resume_from = Some(resume.clone());
            next.identity.token.as_mut().unwrap().1 = now_ms() + 100;
            {
                let open = next.open();
                tokio::pin!(open);
                tokio::select! {
                    _ = &mut open => panic!("resume must wait for the first socket"),
                    _ = taken_over.notified() => {}
                }
                tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                first.close();
                open.await;
            }
            assert!(
                next.joined.is_empty(),
                "expiry during takeover cannot admit"
            );
            let (mut fresh, _sent) = user_session(registry, "alice");
            fresh.identity.resume_from = Some(resume);
            fresh.open().await;
            assert!(fresh.joined.contains_key("acct"));
            next.close();
            fresh.close();
        })
        .await;
}

#[tokio::test]
async fn token_expiry_is_checked_by_registry_before_reusing_an_account() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let init = acct_init();
            let original = crate::accounts::testing::trading_account(&registry, "alice", &init);
            let result = registry
                .obtain(
                    "alice",
                    &init,
                    false,
                    9,
                    Some(("synthetic".into(), now_ms())),
                    Arc::new(|_| true),
                )
                .await;
            assert!(result.is_err(), "expired init cannot install a holder");
            assert!(!registry.holds(&original, 9));
        })
        .await;
}

#[tokio::test]
async fn token_expiry_is_rechecked_after_verification_and_keeps_the_owner() {
    let registry = crate::accounts::tests::registry();
    let (mut alice, mut alice_sent) = user_session(registry.clone(), "alice");
    let mut results = alice.take_verified();
    let (release, wait) = tokio::sync::oneshot::channel();
    alice.start_verification("acct".into(), async move {
        wait.await.unwrap();
        Ok(synthetic_candidate())
    });
    let (mut bob, mut bob_sent) = user_session(registry, "bob");
    bob.start_verification("acct".into(), std::future::pending());
    expire(&mut alice);
    release.send(()).unwrap();
    let completion = results.recv().await.unwrap();
    alice.verification_completed(completion).await.unwrap();
    expired_reply(&mut alice_sent);
    assert!(alice.pending_verifications.is_empty());
    assert!(alice.joined.is_empty());
    assert!(bob.pending_verifications.contains_key("acct"));
    assert!(bob_sent.try_recv().is_err());
    alice.close();
    bob.close();
}

#[tokio::test]
async fn token_expiry_stops_a_nuke_waiting_for_startup() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let link = safe_cex_remote_signer::SignerLink::new(7, Arc::new(|_| true));
            let (account, placed) = crate::accounts::tests::held_load_account(link, Some(500.));
            crate::accounts::tests::register(&registry, &account);
            registry.park("r1".into(), "u", vec![(account.clone(), 7)]);
            let (mut session, mut sent) = sign_silent_session(registry);
            session.open().await;
            session.identity.token.as_mut().unwrap().1 = now_ms() + 100;
            session.handle(NUKE_CALL).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
            assert_eq!(
                placed.load(Ordering::SeqCst),
                0,
                "no dispatch after the startup wait"
            );
            expired_reply(&mut sent);
            session.close();
        })
        .await;
}

#[tokio::test]
async fn token_expiry_stops_a_command_waiting_for_startup() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let ready = Arc::new(AtomicBool::new(false));
            let mut waiting = Vec::new();
            // Bob logs in while Alice's command still waits for startup. Neither
            // the reply nor its captured expiry may be donated to Bob's session.
            for (user, expired) in [("alice", true), ("bob", false)] {
                let commands = Arc::new(AtomicUsize::new(0));
                let init = acct_init();
                let (account, _inbox) = crate::accounts::testing::account_on(
                    ReadyLater {
                        commands: commands.clone(),
                        ready: ready.clone(),
                    },
                    (user, "acct", "bybit"),
                    Some(safe_cex_remote_signer::SignerLink::new(
                        7,
                        Arc::new(|_| true),
                    )),
                    init.fingerprint(),
                    Arc::default(),
                );
                crate::accounts::tests::register(&registry, &account);
                let (mut session, sent) = user_session(registry.clone(), user);
                session.init(init).await.unwrap();
                assert!(
                    !account
                        .handle
                        .read(|store| store.loaded().balance)
                        .await
                        .unwrap()
                );
                if expired {
                    session.identity.token.as_mut().unwrap().1 = now_ms() + 100;
                }
                session.send_command(
                    "acct",
                    &json!(user),
                    "cancelAllOrders",
                    &None,
                    json!({"op":"cancelSymbolOrders","symbol":"BTCUSDT"}),
                    methods::Reply::Unit,
                    true,
                    None,
                );
                waiting.push((expired, account, commands, session, sent));
            }
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            ready.store(true, Ordering::SeqCst);
            for (expired, account, commands, session, mut sent) in waiting {
                tokio::time::timeout(std::time::Duration::from_secs(2), async {
                    loop {
                        if commands.load(Ordering::SeqCst) > 0
                            || session.joined["acct"]
                                .client
                                .pending
                                .lock()
                                .unwrap()
                                .is_empty()
                        {
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                    }
                })
                .await
                .expect("dispatch or refusal after startup");
                assert!(
                    account
                        .handle
                        .read(|store| {
                            let loaded = store.loaded();
                            loaded.markets && loaded.balance && loaded.orders && loaded.positions
                        })
                        .await
                        .unwrap(),
                    "startup really completed"
                );
                assert_eq!(
                    commands.load(Ordering::SeqCst),
                    usize::from(!expired),
                    "venue dispatch, expired={expired}"
                );
                if expired {
                    expired_reply(&mut sent);
                    assert!(
                        session.joined["acct"]
                            .client
                            .pending
                            .lock()
                            .unwrap()
                            .is_empty()
                    );
                } else {
                    assert!(
                        std::iter::from_fn(|| sent.try_recv().ok())
                            .all(|frame| !frame.contains("Tealstreet proxy token expired"))
                    );
                }
                session.close();
            }
        })
        .await;
}

#[tokio::test]
async fn token_expiry_stops_an_action_nuke_waiting_for_startup() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let link = safe_cex_remote_signer::SignerLink::new(7, Arc::new(|_| true));
            let (account, placed) = crate::accounts::tests::held_load_account(link, Some(500.));
            crate::accounts::tests::register(&registry, &account);
            registry.park("r1".into(), "u", vec![(account.clone(), 7)]);
            let (mut session, mut sent) = sign_silent_session(registry);
            session.open().await;
            session.joined.get_mut("acct").unwrap().out = session.out.account("acct");
            assert!(session.account_out("acct").action_active());
            let timestamp = format!("{:012x}", now_ms());
            let intent = format!(
                "{}-{}-7000-8000-000000000001",
                &timestamp[..8],
                &timestamp[8..]
            );
            let deadline =
                crate::order_actions::ActionDeadline::new(&intent, 0., std::time::Instant::now())
                    .unwrap();
            session.identity.token.as_mut().unwrap().1 = now_ms() + 100;
            session.send_action_nuke(
                "acct",
                &json!(intent),
                OrderReceipt::Action {
                    seq: 1,
                    kind: "nuke",
                    deadline,
                },
                "all",
            );
            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
            assert_eq!(
                placed.load(Ordering::SeqCst),
                0,
                "no action dispatch after the startup wait"
            );
            expired_reply(&mut sent);
            session.close();
        })
        .await;
}

#[tokio::test]
async fn token_expiry_does_not_fence_another_valid_holder_of_the_same_account() {
    tokio::task::LocalSet::new().run_until(async {
        let registry = crate::accounts::tests::registry();
        let init = acct_init();
        let account = crate::accounts::testing::trading_account(&registry, "alice", &init);
        let (mut old, _sent) = user_session(registry.clone(), "alice");
        let (mut fresh, mut sent) = user_session(registry.clone(), "alice");
        old.init(init.clone()).await.unwrap();
        fresh.init(init).await.unwrap();
        expire(&mut old);
        while sent.try_recv().is_ok() {}
        fresh.handle(r#"{"t":"msg","d":{"op":"subscribeOrderBook","accountName":"acct","data":{"symbol":"BTCUSDT"}}}"#).await.unwrap();
        assert_eq!(fresh.joined["acct"].subscriptions.len(), 1);
        assert!(account.running.io_allowed());
        assert!(Arc::ptr_eq(&old.joined["acct"].account, &fresh.joined["acct"].account));
        assert!(std::iter::from_fn(|| sent.try_recv().ok()).all(|frame| !frame.contains("Tealstreet proxy token expired")));
        old.close();
        fresh.close();
    }).await;
}

#[tokio::test]
async fn token_expiry_refuses_every_envelope_without_closing_during_grace() {
    tokio::task::LocalSet::new().run_until(async {
        let registry = crate::accounts::tests::registry();
        let init = acct_init();
        let account = crate::accounts::testing::trading_account(&registry, "alice", &init);
        let (mut session, mut sent) = user_session(registry.clone(), "alice");
        session.init(init).await.unwrap();
        while sent.try_recv().is_ok() {}
        expire(&mut session);
        let mut frames = vec![
            json!({"t":"sign-res","d":{"id":"unknown"}}),
            json!({"t":"echo-res","d":{}}),
            json!({"t":"dispose","d":{"accountName":"acct"}}),
            json!({"t":"ping","d":{"accounts":["other"]}}),
            json!({"t":"perf","d":{}}),
            json!({"t":"verbose","d":true}),
            json!({"t":"init","d":acct_init()}),
        ];
        for (op, data) in [
            ("delegated-key-req", json!({})),
            ("subscribeOrderBook", json!({"symbol":"BTCUSDT"})),
            ("sendSnapshot", json!({})),
            ("callMethod", json!({"requestId":"expired","method":"cancelAllOrders","args":[]})),
            ("chaser:start", json!({})),
            ("twap:state", json!({})),
            ("exec", json!({})),
        ] {
            frames.push(json!({"t":"msg","d":{"op":op,"accountName":"acct","data":data}}));
        }
        for kind in ["order.submit", "order.cancelQueued", "order.action", "order.actionCancelQueued"] {
            let mut data = json!({"accountName":"acct","intentId":"019a5634-0000-7000-8000-000000000001","seq":1,"fingerprint":"a".repeat(64)});
            if kind == "order.submit" {
                data["params"] = json!({});
                data["ageMs"] = json!(0);
            } else if kind.starts_with("order.action") {
                data["version"] = json!(1);
                if kind == "order.action" {
                    data["ageMs"] = json!(0);
                    data["action"] = json!({"kind":"cancelAll"});
                }
            }
            frames.push(json!({"t":kind,"d":data}));
        }
        let client = session.joined["acct"].client.id;
        for frame in frames {
            let wire = frame.to_string();
            assert!(serde_json::from_str::<Inbound>(&wire).is_ok(), "test envelope: {wire}");
            assert_eq!(session.handle(&wire).await, Ok(()), "keep the existing grace: {wire}");
            expired_reply(&mut sent);
            assert_eq!(session.joined["acct"].client.id, client, "{wire}");
            assert!(session.joined["acct"].subscriptions.is_empty(), "{wire}");
            assert_eq!(session.joined["acct"].next_command, 0, "{wire}");
            assert!(registry.holds(&account, session.socket), "{wire}");
        }
        session.close();
    }).await;
}
