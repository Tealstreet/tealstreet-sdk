use super::*;

fn init() -> InitPayload {
    InitPayload {
        account_name: "display label".into(),
        exchange_name: "bybit".into(),
        key: "synthetic-key-A".into(),
        ..Default::default()
    }
}

#[test]
fn bybit_settings_fingerprint_excludes_primary_credentials() {
    let first = init();
    let second = InitPayload {
        key: "synthetic-key-B".into(),
        passphrase: Some("synthetic-passphrase".into()),
        ..first.clone()
    };
    assert_eq!(first.fingerprint(), second.fingerprint());
    let fields: Value = serde_json::from_str(&first.fingerprint()).unwrap();
    assert!(fields.get("key").is_none());
    assert!(fields.get("passphrase").is_none());
}

#[test]
fn bybit_settings_fingerprint_keeps_connection_settings_and_ignores_labels() {
    let first = init();
    let renamed = InitPayload {
        account_name: "another label".into(),
        instance_id: Some("another registration".into()),
        ..first.clone()
    };
    assert_eq!(first.fingerprint(), renamed.fingerprint());
    let changed = InitPayload {
        enable_spot_markets: Some(true),
        ..first.clone()
    };
    assert_ne!(first.fingerprint(), changed.fingerprint());
}

#[test]
fn identity_digest_vector_and_component_boundaries() {
    let first = init();
    let digest = identity::digest("u", &first).unwrap().unwrap();
    assert_eq!(
        digest,
        "16b4dec30a65d36e34c43a9f5c927abb5b0fb639e71006fa942f6cde06b2b01f"
    );
    let renamed = InitPayload {
        account_name: "renamed".into(),
        ..first.clone()
    };
    assert_eq!(
        identity::digest("u", &renamed).unwrap(),
        Some(digest.clone())
    );
    for changed in [
        InitPayload {
            key: "synthetic-key-B".into(),
            ..first.clone()
        },
        InitPayload {
            testnet: Some(true),
            ..first.clone()
        },
        InitPayload {
            bybit_site_id: Some("BRA_BTL".into()),
            ..first.clone()
        },
    ] {
        assert_ne!(
            identity::digest("u", &changed).unwrap(),
            Some(digest.clone())
        );
    }
    assert_ne!(identity::digest("v", &first).unwrap(), Some(digest));
    assert!(identity::digest("", &first).is_err());
    assert!(
        identity::digest(
            "u",
            &InitPayload {
                key: String::new(),
                ..first
            }
        )
        .is_err()
    );
}

#[test]
fn bybit_key_id_preserves_case_and_its_declared_no_trim_policy() {
    let first = init();
    let padded = InitPayload {
        key: format!(" {} ", first.key),
        ..first.clone()
    };
    let other_case = InitPayload {
        key: first.key.to_uppercase(),
        ..first.clone()
    };
    let original = identity::digest("u", &first).unwrap();
    assert_ne!(identity::digest("u", &padded).unwrap(), original);
    assert_ne!(identity::digest("u", &other_case).unwrap(), original);
}

#[test]
fn identity_outputs_keep_twap_labels_local_and_preserve_owner_ids() {
    for (op, data, path) in [
        (
            "store",
            serde_json::json!({"activeTWAPs":[{"accountName":"first","symbol":"BTCUSDT"}]}),
            "/d/data/activeTWAPs/0/accountName",
        ),
        (
            "twap:stateUpdate",
            serde_json::json!({"twap-1":{"options":{"accountName":"first"},"state":{"accountName":"first"},"ownerId":"digest"}}),
            "/d/data/twap-1/options/accountName",
        ),
    ] {
        let output = crate::envelope::with_account_label(
            crate::envelope::msg("first", op, data.clone()),
            "second",
        );
        let output: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(output["d"]["accountName"], "second");
        assert_eq!(output.pointer(path).unwrap(), "second");
        if op == "twap:stateUpdate" {
            assert_eq!(output["d"]["data"]["twap-1"]["ownerId"], "digest");
            assert_eq!(
                output["d"]["data"]["twap-1"]["state"]["accountName"],
                "second"
            );
        }
        let unchanged = crate::envelope::msg("first", op, data);
        assert_eq!(
            crate::envelope::with_account_label(unchanged.clone(), "first"),
            unchanged
        );
    }
}

#[test]
fn unflagged_and_public_accounts_keep_their_existing_identity_path() {
    let private = InitPayload {
        exchange_name: "bybitinverse".into(),
        ..init()
    };
    let another_key = InitPayload {
        key: "synthetic-key-B".into(),
        ..private.clone()
    };
    assert!(identity::digest("u", &private).unwrap().is_none());
    assert_ne!(private.fingerprint(), another_key.fingerprint());
    let public = InitPayload {
        public_only: Some(true),
        ..init()
    };
    assert!(identity::digest("", &public).unwrap().is_none());
    let pseudo = InitPayload {
        account_name: PUBLIC_ACCOUNT.into(),
        ..init()
    };
    assert!(identity::digest("", &pseudo).unwrap().is_none());
}

#[test]
fn probe_permit_remains_bound_to_identity_when_settings_match() {
    let registry = tests::registry();
    let first = init();
    let second = InitPayload {
        key: "synthetic-key-B".into(),
        ..first.clone()
    };
    assert_eq!(first.fingerprint(), second.fingerprint());
    let permit = registry.probe_permit("u", &first).unwrap();
    assert!(permit.check("u", &first).is_ok());
    assert!(permit.check("u", &second).is_err());
    assert!(permit.check("v", &first).is_err());
}

#[tokio::test(start_paused = true)]
async fn expired_router_lease_removes_the_identity_keyed_actor() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = Arc::new(
                Registry::new(
                    EngineThreads::start(1).unwrap(),
                    Duration::from_millis(1),
                    Duration::from_secs(60),
                )
                .with_router_lease(crate::router_lease::Config {
                    box_id: Some("01".into()),
                    enforce: true,
                    ..Default::default()
                }),
            );
            let now = now_ms();
            registry
                .router_leases
                .accept(&crate::router_lease::Heartbeat {
                    r#box: "01".into(),
                    issued_at: now,
                    users: vec![crate::router_lease::Assertion {
                        user_id: "u".into(),
                        owner_epoch: 1,
                        sequence: 1,
                        not_after: now + 1000,
                    }],
                })
                .unwrap();
            let account = account(&registry, "u", &init());
            assert!(account.running.io_allowed());
            assert_eq!(registry.account_count(), 1);
            tokio::time::advance(Duration::from_secs(2)).await;
            assert!(!account.running.io_allowed());
            registry.park_expired_router_leases();
            assert!(account.running.parked_by_lease());
            assert_eq!(registry.account_count(), 0);
        })
        .await;
}

#[tokio::test]
async fn resident_copy_counts_without_a_live_or_parked_holder() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = tests::registry();
            let mut original = init();
            original.signing_mode = Some("delegated".into());
            let account = account(&registry, "u", &original);
            assert!(account.holders.lock().unwrap().by_id.is_empty());
            let changed = InitPayload {
                enable_spot_markets: Some(true),
                ..original.clone()
            };
            assert!(
                registry
                    .rebuild_refusal_for_mode("u", &changed, false, 7, true)
                    .is_some()
            );
            assert!(Arc::ptr_eq(
                &registry.accounts.lock().unwrap()[&("u".into(), account.registry_name().into())],
                &account
            ));
        })
        .await;
}

fn account(registry: &Registry, user: &str, init: &InitPayload) -> Arc<Account> {
    let (mut account, _inbox) = testing::account_on(
        testing::HeldLoad {
            placed: Arc::default(),
            load_after_ms: Some(0.),
            ticker: true,
        },
        (user, &init.account_name, &init.exchange_name),
        Some(SignerLink::new(0, Arc::new(|_| true))),
        init.fingerprint(),
        Arc::new(crate::algos::Running::new(registry.algos_ended.clone())),
    );
    testing::bind_identity(&mut account, init);
    let inner = Arc::get_mut(&mut account).unwrap();
    if init.signing_mode.as_deref() == Some("delegated") {
        let prepared = prepare_delegated(
            init,
            DelegatedPair::new(init.key.clone(), "synthetic-secret".into(), None),
        )
        .unwrap();
        inner.delegated = Some(prepared.resident);
        inner.link = None;
        inner.fingerprint = crate::delegated::fingerprint(init);
    }
    testing::register(registry, &account);
    account
}

#[tokio::test]
async fn labels_share_one_identity_and_release_independently() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = tests::registry();
            let original = init();
            let account = account(&registry, "u", &original);
            let renamed = InitPayload {
                account_name: "second label".into(),
                ..original.clone()
            };
            let a = registry
                .obtain("u", &original, false, 7, None, Arc::new(|_| true))
                .await
                .unwrap();
            let b = registry
                .obtain("u", &renamed, false, 7, None, Arc::new(|_| true))
                .await
                .unwrap();
            assert!(Arc::ptr_eq(&a, &b) && Arc::ptr_eq(&b, &account));
            assert_eq!(registry.account_count(), 1);
            registry.release_label(&account, 7, &original.account_name, Disposal::Orphaned);
            assert!(registry.held("u", &original.account_name, 7).is_none());
            assert!(registry.held("u", &renamed.account_name, 7).is_some());
            assert!(registry.holds(&account, 7));
        })
        .await;
}

#[tokio::test]
async fn identity_broadcast_reaches_each_label_once_and_counts_one_socket() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = tests::registry();
            let first = init();
            let account = account(&registry, "u", &first);
            let second = InitPayload {
                account_name: "second label".into(),
                ..first.clone()
            };
            let (a, mut a_sent, _) = crate::out::Out::channel(16);
            let a = a
                .with_account_label(&first.account_name)
                .with_instance_id(Some("first-registration"));
            let (b, mut b_sent, _) = crate::out::Out::channel(16);
            let b = b
                .with_account_label(&second.account_name)
                .with_instance_id(Some("second-registration"));
            for (init, out) in [(&first, a), (&second, b)] {
                let joined = registry
                    .obtain("u", init, false, 7, None, out.outbox())
                    .await
                    .unwrap();
                assert!(Arc::ptr_eq(&joined, &account));
            }
            let update = crate::envelope::msg(&account.name, "store", json!({"activeChasers":[]}));
            let holding = account.reach_live(Some(update.clone()));
            assert_eq!(holding.live, [7]);
            assert!(!holding.parked);
            for (sent, label, id) in [
                (
                    &mut a_sent,
                    first.account_name.as_str(),
                    "first-registration",
                ),
                (
                    &mut b_sent,
                    second.account_name.as_str(),
                    "second-registration",
                ),
            ] {
                let frame: Value = serde_json::from_str(&sent.try_recv().unwrap()).unwrap();
                assert_eq!(frame["d"]["accountName"], label);
                assert_eq!(frame["d"]["instanceId"], id);
                assert!(sent.try_recv().is_err());
            }
            registry.release_label(&account, 7, &second.account_name, Disposal::PageDisposed);
            assert_eq!(account.reach_live(Some(update)).live, [7]);
            assert!(a_sent.try_recv().is_ok());
            assert!(b_sent.try_recv().is_err());
        })
        .await;
}

#[tokio::test]
async fn identity_cache_keys_do_not_override_another_accounts_wire_label() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = tests::registry();
            let original = init();
            let first = account(&registry, "u", &original);
            let other = InitPayload {
                account_name: "other account".into(),
                key: "synthetic-key-B".into(),
                ..original.clone()
            };
            let second = account(&registry, "u", &other);
            let alias = InitPayload {
                account_name: second.registry_name().to_owned(),
                ..original
            };
            for init in [&alias, &other] {
                registry
                    .obtain("u", init, false, 7, None, Arc::new(|_| true))
                    .await
                    .unwrap();
            }
            assert!(Arc::ptr_eq(
                &registry.held("u", &alias.account_name, 7).unwrap(),
                &first
            ));
            registry.release_label(&first, 7, &alias.account_name, Disposal::PageDisposed);
            assert!(registry.held("u", &alias.account_name, 7).is_none());
            assert!(Arc::ptr_eq(
                &registry.held("u", &other.account_name, 7).unwrap(),
                &second
            ));
        })
        .await;
}

#[tokio::test]
async fn mixed_rollout_name_collisions_refuse_without_rebuilding_either_mode() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for identified_first in [true, false] {
                let registry = tests::registry();
                let identified = init();
                let legacy = InitPayload {
                    account_name: identity::digest("u", &identified).unwrap().unwrap(),
                    exchange_name: "bybitinverse".into(),
                    ..identified.clone()
                };
                let (existing, proposed) = if identified_first {
                    (&identified, &legacy)
                } else {
                    (&legacy, &identified)
                };
                let retained = account(&registry, "u", existing);
                assert!(registry.rebuild_refusal("u", proposed, false, 7).is_some());
                assert!(retained.handle.disposal_cause().is_none());
                assert_eq!(registry.account_count(), 1);
            }
        })
        .await;
}

#[tokio::test]
async fn equal_labels_keep_distinct_keys_and_users_separate() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = tests::registry();
            let first = init();
            let second = InitPayload {
                key: "synthetic-key-B".into(),
                ..first.clone()
            };
            let a = account(&registry, "u", &first);
            let b = account(&registry, "u", &second);
            let other_user = account(&registry, "v", &first);
            for (user, init, expected, holder) in [
                ("u", &first, &a, 7),
                ("u", &second, &b, 8),
                ("v", &first, &other_user, 9),
            ] {
                let attached = registry
                    .obtain(user, init, false, holder, None, Arc::new(|_| true))
                    .await
                    .unwrap();
                assert!(Arc::ptr_eq(&attached, expected));
            }
            assert_eq!(registry.account_count(), 3);
            assert!(!registry.holds(&a, 8) && !registry.holds(&a, 9));
            assert!(!registry.holds(&b, 7));
        })
        .await;
}

#[tokio::test]
async fn parked_holder_prevents_implicit_settings_changes_and_renamed_resume_keeps_identity() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = tests::registry();
            let first = init();
            let account = account(&registry, "u", &first);
            let renamed = InitPayload {
                account_name: "phone label".into(),
                ..first.clone()
            };
            registry
                .obtain("u", &renamed, false, 7, None, Arc::new(|_| true))
                .await
                .unwrap();
            registry.park("resume".into(), "u", vec![(account.clone(), 7)]);
            let changed = InitPayload {
                enable_spot_markets: Some(true),
                ..first.clone()
            };
            assert!(registry.rebuild_refusal("u", &changed, false, 8).is_some());
            assert_eq!(registry.account_count(), 1);
            let frames = Arc::new(Mutex::new(Vec::new()));
            let captured = frames.clone();
            let outbox: Outbox = Arc::new(move |text| {
                captured.lock().unwrap().push(text);
                true
            });
            let resumed = registry.resume(
                "resume",
                "u",
                9,
                ("synthetic-token".into(), now_ms() + 60_000),
                &|_| outbox.clone(),
            );
            assert_eq!(resumed.len(), 1);
            assert!(Arc::ptr_eq(&resumed[0], &account));
            assert_eq!(account.labels(9), [renamed.account_name]);
            account.notify(crate::envelope::msg(&account.name, "test", Value::Null));
            let frame: Value = serde_json::from_str(&frames.lock().unwrap()[0]).unwrap();
            assert_eq!(frame["d"]["accountName"], "phone label");
        })
        .await;
}

#[tokio::test]
async fn label_lookup_selects_the_requested_holder_across_identity_modes() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = tests::registry();
            let identified = init();
            let identity_account = account(&registry, "u", &identified);
            let legacy = InitPayload {
                exchange_name: "bybitinverse".into(),
                ..identified.clone()
            };
            let legacy_account = account(&registry, "u", &legacy);
            for (init, holder) in [(&identified, 7), (&legacy, 8)] {
                registry
                    .obtain("u", init, false, holder, None, Arc::new(|_| true))
                    .await
                    .unwrap();
            }
            assert!(Arc::ptr_eq(
                &registry.held("u", &legacy.account_name, 8).unwrap(),
                &legacy_account
            ));
            assert!(registry.holds(&identity_account, 7));
            assert!(Arc::ptr_eq(
                &registry.held("u", &identified.account_name, 7).unwrap(),
                &identity_account
            ));
            assert!(registry.held("u", &identified.account_name, 9).is_none());
            assert!(
                registry
                    .held("other-user", &identified.account_name, 7)
                    .is_none()
            );
        })
        .await;
}

#[tokio::test]
async fn fixround_fatal_signer_notices_reach_each_live_alias() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for switch_user in [false, true] {
                let registry = tests::registry();
                let first = init();
                let (mut account, mut algos) = testing::account_on(
                    testing::HeldLoad {
                        placed: Arc::default(),
                        load_after_ms: Some(0.),
                        ticker: true,
                    },
                    ("u", &first.account_name, &first.exchange_name),
                    Some(SignerLink::new(0, Arc::new(|_| true))),
                    first.fingerprint(),
                    Arc::new(crate::algos::Running::new(registry.algos_ended.clone())),
                );
                testing::bind_identity(&mut account, &first);
                testing::register(&registry, &account);
                let mut outputs = Vec::new();
                for (holder, label) in [(7, "first"), (7, "second"), (8, "parked"), (9, "legacy")] {
                    let (out, sent, _) = crate::out::Out::channel(16);
                    let out = out
                        .account(label)
                        .with_account_label(label)
                        .with_instance_id(Some(label));
                    account.hold_registration(holder, out.outbox(), None, label);
                    outputs.push((label, out, sent));
                }
                account.holders.lock().unwrap().labels.remove(&9);
                registry.park("parked-resume".into(), "u", vec![(account.clone(), 8)]);
                registry.signer_failed(&account);
                let crate::algos::AlgoInput::Dispose {
                    stopped: Some(stopped),
                    ..
                } = algos.recv().await.unwrap()
                else {
                    panic!("fatal signing supplies a captured stop callback");
                };
                if switch_user {
                    let other = self::account(&registry, "v", &first);
                    let (next, mut next_sent, _) = crate::out::Out::channel(16);
                    let next = next.account("first").with_account_label("first");
                    other.hold_registration(7, next.outbox(), None, "first");
                    for (label, out, _) in &outputs {
                        out.retire_account(label);
                    }
                    stopped(crate::algos::Kept::from("synthetic algo stopped"));
                    for (_, _, sent) in &mut outputs {
                        assert!(sent.try_recv().is_err(), "retired user output is dropped");
                    }
                    assert!(
                        next_sent.try_recv().is_err(),
                        "the new user receives no old stop notice"
                    );
                    assert!(next.send(crate::envelope::error(Some("first"), "fresh user control")));
                    assert!(next_sent.try_recv().unwrap().contains("fresh user control"));
                    registry.dispose(&other, Disposal::PageDisposed);
                    continue;
                }
                stopped(crate::algos::Kept::from("synthetic algo stopped"));
                for (label, _, mut sent) in outputs {
                    if label == "parked" {
                        assert!(sent.try_recv().is_err());
                        continue;
                    }
                    for expected in ["Remote signing failed repeatedly", "synthetic algo stopped"] {
                        let frame: Value = serde_json::from_str(&sent.try_recv().unwrap()).unwrap();
                        assert_eq!(frame["d"]["accountName"], label);
                        assert_eq!(frame["d"]["instanceId"], label);
                        assert!(frame["d"]["data"][0].as_str().unwrap().contains(expected));
                    }
                    assert!(sent.try_recv().is_err());
                }
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn fixround_superseded_settings_keep_the_resident_account() {
    use std::future::Future;
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = tests::registry();
            let mut original = init();
            original.signing_mode = Some("delegated".into());
            let account = account(&registry, "u", &original);
            account.hold_registration(7, Arc::new(|_| true), None, &original.account_name);
            registry.pacer.admit().await;
            let changed = InitPayload {
                enable_spot_markets: Some(true),
                ..original.clone()
            };
            let apply = registry.apply_settings(
                "u",
                &changed,
                account.clone(),
                7,
                Some(("synthetic-token".into(), now_ms() + 60_000)),
                Arc::new(|_| true),
            );
            tokio::pin!(apply);
            assert!(
                std::future::poll_fn(|cx| std::task::Poll::Ready(
                    apply.as_mut().poll(cx).is_pending()
                ))
                .await
            );
            let superseding = registry
                .begin_init(
                    "u",
                    account.registry_name(),
                    false,
                    &account.fingerprint,
                    None,
                )
                .unwrap();
            tokio::time::advance(Duration::from_secs(1)).await;
            assert!(matches!(apply.await, Err(error) if error.contains("superseded")));
            assert!(account.handle.disposal_cause().is_none());
            assert!(registry.held("u", &original.account_name, 7).is_some());
            assert_eq!(registry.account_count(), 1);
            drop(superseding);
            registry.dispose(&account, Disposal::PageDisposed);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn public_fallback_cannot_reset_identity_settings_through_a_label() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for parked in [false, true] {
                let registry = tests::registry();
                let original = init();
                let account = account(&registry, "u", &original);
                let alias = InitPayload {
                    account_name: "phone label".into(),
                    ..original.clone()
                };
                for init in [&original, &alias] {
                    registry
                        .obtain("u", init, false, 7, None, Arc::new(|_| true))
                        .await
                        .unwrap();
                }
                if parked {
                    registry.park("resume".into(), "u", vec![(account.clone(), 7)]);
                }
                for label in [&original.account_name, &alias.account_name] {
                    let public = InitPayload {
                        account_name: label.clone(),
                        public_only: Some(true),
                        ..original.clone()
                    };
                    for holder in [7, 8] {
                        assert!(
                            registry.rebuild_refusal("u", &public, true, holder).is_some(),
                            "public fallback must not bypass explicit settings: parked={parked}, holder={holder}, alias={}",
                            label == &alias.account_name,
                        );
                        assert!(
                            registry.obtain("u", &public, true, holder, None, Arc::new(|_| true)).await.is_err(),
                            "trusted construction must retain the same settings refusal"
                        );
                    }
                    assert!(registry.rebuild_refusal("other", &public, true, 7).is_none());
                }
                assert!(account.handle.disposal_cause().is_none());
                assert_eq!(registry.account_count(), 1);
                registry.dispose(&account, Disposal::PageDisposed);
            }
        })
        .await;
}

#[test]
fn twap_binding_tracks_verified_identity_across_labels_and_key_changes() {
    let first = init();
    let same = InitPayload {
        account_name: "renamed".into(),
        ..first.clone()
    };
    let other = InitPayload {
        key: "synthetic-key-B".into(),
        ..first.clone()
    };
    let a = identity::digest("u", &first).unwrap().unwrap();
    let same = identity::digest("u", &same).unwrap().unwrap();
    let b = identity::digest("u", &other).unwrap().unwrap();
    let binding = crate::twap_owner::Binding::new(&a);
    let original = binding.snapshot();
    assert_eq!(original.owner_id, a);
    assert!(!binding.rebind_owner(&same));
    assert_eq!(binding.snapshot(), original);
    assert_ne!(crate::twap_owner::Binding::new(&b).snapshot(), original);
    assert!(binding.rebind_owner(&b));
    assert!(binding.rebind_owner(&a));
    assert_ne!(binding.snapshot().generation, original.generation);
}
