use super::*;

fn request(instance: Option<&str>) -> String {
    let mut data = json!({"accountName":"acct"});
    if let Some(instance) = instance {
        data["instanceId"] = instance.into();
    }
    json!({"t":"apply-settings", "d":data}).to_string()
}

fn frames(sent: &mut crate::out::Receiver) -> Vec<Value> {
    std::iter::from_fn(|| sent.try_recv().ok())
        .map(|text| serde_json::from_str(&text).unwrap())
        .collect()
}

#[tokio::test(start_paused = true)]
async fn apply_settings_requires_an_admitted_client_and_records_the_stop_reason() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let init = acct_init();
            let (account, mut algos) = crate::accounts::testing::account_on(
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
                init.fingerprint(),
                Arc::default(),
            );
            crate::accounts::testing::register(&registry, &account);
            let (mut session, mut sent) = user_session(registry.clone(), "u");
            let mut init = init;
            init.instance_id = Some("current".into());
            session.init(init.clone()).await.unwrap();
            while algos.try_recv().is_ok() {}
            frames(&mut sent);
            let changed = InitPayload {
                enable_spot_markets: Some(true),
                ..init.clone()
            };
            assert!(
                registry
                    .rebuild_refusal("u", &changed, false, session.socket)
                    .is_some()
            );
            session.init(changed.clone()).await.unwrap();
            assert!(Arc::ptr_eq(&session.joined["acct"].account, &account));
            assert!(
                algos.try_recv().is_err(),
                "ordinary init must not stop algos"
            );
            assert!(
                frames(&mut sent)
                    .iter()
                    .any(|f| f["d"]["data"][1]["initRefused"] == true)
            );

            session.handle(&request(Some("current"))).await.unwrap();
            assert!(!registry.holds(&account, session.socket));
            assert!(!session.joined.contains_key("acct"));
            assert_eq!(registry.account_count(), 0);
            let crate::algos::AlgoInput::Dispose {
                reason,
                stopped: Some(stopped),
            } = algos.try_recv().expect("settings stop")
            else {
                panic!("expected the existing settings-disposal path");
            };
            assert_eq!(reason, "its key or connection settings changed");
            assert_eq!(
                account.handle.disposal_cause(),
                Some(safe_cex_native::DisposalCause::Stopped)
            );
            stopped(crate::algos::Kept {
                notices: vec![crate::algos::Notice {
                    text: format!("TWAP stopped: {reason}"),
                    algo: None,
                }],
                ends: vec![],
            });
            let replies = frames(&mut sent);
            assert!(
                replies.iter().any(|f| f["d"]["op"] == "applySettings:done"
                    && f["d"]["data"]["success"] == true
                    && f["d"]["instanceId"] == "current"),
                "{replies:?}"
            );
            assert!(
                replies
                    .iter()
                    .any(|f| f["d"]["data"][0] == format!("TWAP stopped: {reason}")),
                "recorded algo reason must reach the admitted client: {replies:?}"
            );
            assert!(
                registry
                    .rebuild_refusal("u", &changed, false, session.socket)
                    .is_none()
            );
            // The normal init/admission path remains responsible for the replacement.
            let replacement = crate::accounts::testing::trading_account(&registry, "u", &changed);
            session.init(changed.clone()).await.unwrap();
            assert!(Arc::ptr_eq(&session.joined["acct"].account, &replacement));
            let (release, deferred) = tokio::sync::oneshot::channel();
            let stop_notice = tokio::spawn(async move {
                deferred.await.unwrap();
                stopped(crate::algos::Kept {
                    notices: vec![
                        crate::algos::Notice {
                            text: format!("TWAP stopped: {reason}"),
                            algo: None,
                        },
                        crate::algos::Notice {
                            text: "late settings stop from old owner".into(),
                            algo: None,
                        },
                    ],
                    ends: vec![],
                });
            });
            // Switch users while the old account's stop callback is deferred.
            let bob_account = crate::accounts::testing::trading_account(&registry, "bob", &changed);
            let (mut bob, mut bob_sent) = user_session(registry.clone(), "bob");
            bob.init(changed).await.unwrap();
            release.send(()).unwrap();
            stop_notice.await.unwrap();
            let control = envelope::error(Some("acct"), "current bob output");
            assert!(bob.account_out("acct").send(control));
            let alice_frames = frames(&mut sent);
            let bob_frames = frames(&mut bob_sent);
            assert!(
                !alice_frames
                    .iter()
                    .chain(&bob_frames)
                    .any(|f| f["d"]["data"][0] == "late settings stop from old owner")
            );
            assert!(
                bob_frames
                    .iter()
                    .any(|f| f["d"]["data"][0] == "current bob output")
            );
            registry.dispose(&bob_account, Disposal::PageDisposed);
            bob.close();
            session.close();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn apply_settings_refuses_pending_missing_other_user_and_stale_registration() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for case in [
                "pending",
                "missing",
                "other-user",
                "stale-registration",
                "expired",
                "retired-account",
                "parked",
                "lease-fenced",
            ] {
                let mut registry = crate::accounts::tests::registry();
                if case == "lease-fenced" {
                    Arc::get_mut(&mut registry).unwrap().router_leases = Arc::new(
                        crate::router_lease::Users::new(crate::router_lease::Config {
                            enforce: true,
                            box_id: Some("01".into()),
                            enable_grace_until: Some(now_ms() + 10_000),
                            ..Default::default()
                        }),
                    );
                }
                let init = InitPayload {
                    instance_id: Some("current".into()),
                    ..acct_init()
                };
                let account = crate::accounts::testing::trading_account(&registry, "u", &init);
                let (mut owner, _) = user_session(registry.clone(), "u");
                owner.init(init.clone()).await.unwrap();
                let (mut attempt, mut sent) = user_session(
                    registry.clone(),
                    if case == "other-user" { "other" } else { "u" },
                );
                if !["missing", "other-user"].contains(&case) {
                    attempt.init(init.clone()).await.unwrap();
                }
                frames(&mut sent);
                if case == "pending" {
                    attempt.start_verification("acct".into(), std::future::pending());
                }
                if case == "expired" {
                    attempt.identity.token = Some(("expired".into(), 0));
                }
                if case == "parked" {
                    registry.park(
                        "parked".into(),
                        "u",
                        vec![(account.clone(), attempt.socket)],
                    );
                }
                if case == "lease-fenced" {
                    assert!(account.running.io_allowed(), "valid during explicit grace");
                    tokio::time::advance(std::time::Duration::from_secs(11)).await;
                    assert!(
                        !account.running.io_allowed(),
                        "lease fixture must really fence"
                    );
                    assert!(registry.user_lease_fenced("u"));
                }
                let replacement = if case == "retired-account" {
                    Some(crate::accounts::testing::trading_account(
                        &registry, "u", &init,
                    ))
                } else {
                    None
                };
                attempt
                    .handle(&request(Some(if case == "stale-registration" {
                        "old"
                    } else {
                        "current"
                    })))
                    .await
                    .unwrap();
                let replies = frames(&mut sent);
                let expected = if case == "pending" {
                    "verifying"
                } else if case == "expired" {
                    crate::auth::TOKEN_EXPIRED
                } else {
                    "apply settings refused: account not admitted"
                };
                assert!(
                    replies
                        .iter()
                        .any(|f| f["d"]["op"] == "error" && f["d"]["data"][0] == expected),
                    "{case}: {replies:?}"
                );
                assert!(account.handle.disposal_cause().is_none(), "{case}");
                if case == "expired" {
                    assert!(attempt.joined["acct"].out.order_binding(Some("current")));
                    assert!(registry.holds(&account, attempt.socket));
                }
                if let Some(replacement) = replacement {
                    assert!(replacement.handle.disposal_cause().is_none());
                }
                attempt.close();
                owner.close();
            }
        })
        .await;
}

#[tokio::test]
async fn apply_settings_rejects_an_unauthenticated_connection() {
    let registry = crate::accounts::tests::registry();
    let (mut session, _) = user_session(registry, "u");
    session.identity.user = None;
    assert!(matches!(
        session.handle(&request(None)).await,
        Err(Close::Policy(_))
    ));
}

#[tokio::test(start_paused = true)]
async fn apply_settings_requires_a_registration_binding_even_on_legacy_init() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let init = acct_init();
            let account = crate::accounts::testing::trading_account(&registry, "u", &init);
            let (mut session, mut sent) = user_session(registry.clone(), "u");
            session.init(init.clone()).await.unwrap();
            frames(&mut sent);
            session.handle(&request(None)).await.unwrap();
            let replies = frames(&mut sent);
            assert!(
                registry.holds(&account, session.socket),
                "an unbound reset cannot target a later registration"
            );
            assert!(account.handle.disposal_cause().is_none());
            assert!(
                replies.iter().any(|f| f["d"]["op"] == "error"
                    && f["d"]["data"][0] == "apply settings refused: account not admitted"),
                "{replies:?}"
            );
            session
                .init(InitPayload {
                    instance_id: Some("bound".into()),
                    ..init
                })
                .await
                .unwrap();
            session.handle(&request(Some("bound"))).await.unwrap();
            assert!(
                !registry.holds(&account, session.socket),
                "a current admitted registration is the working control"
            );
            let next = InitPayload {
                instance_id: Some("next".into()),
                ..acct_init()
            };
            let replacement = crate::accounts::testing::trading_account(&registry, "u", &next);
            session.init(next).await.unwrap();
            for stale in [None, Some("bound")] {
                session.handle(&request(stale)).await.unwrap();
                assert!(registry.holds(&replacement, session.socket));
                assert!(replacement.handle.disposal_cause().is_none());
            }
            session.handle(&request(Some("next"))).await.unwrap();
            assert!(!registry.holds(&replacement, session.socket));
            session.close();
        })
        .await;
}
