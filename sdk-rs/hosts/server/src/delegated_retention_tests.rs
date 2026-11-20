use super::*;
use crate::delegated::{DelegatedPair, prepare_delegated};
use safe_cex_engine::signing::{Algorithm, Capability, KeyRef};

const RETAIN: Duration = Duration::from_secs(48 * 60 * 60);

fn resident(
    registry: &Registry,
    user: &str,
) -> (
    Arc<Account>,
    mpsc::UnboundedReceiver<crate::algos::AlgoInput>,
) {
    resident_with_identity(registry, user, Some("synthetic-key"))
}

fn resident_with_identity(
    registry: &Registry,
    user: &str,
    key: Option<&str>,
) -> (
    Arc<Account>,
    mpsc::UnboundedReceiver<crate::algos::AlgoInput>,
) {
    resident_with_authority(registry, user, key, false)
}

fn resident_with_authority(
    registry: &Registry,
    user: &str,
    key: Option<&str>,
    proxy: bool,
) -> (
    Arc<Account>,
    mpsc::UnboundedReceiver<crate::algos::AlgoInput>,
) {
    resident_from(registry, user, key, proxy, |init| {
        prepare_delegated(
            init,
            DelegatedPair::new(init.key.clone(), "synthetic-secret".into(), None),
        )
        .unwrap()
        .resident
    })
}

fn resident_from(
    registry: &Registry,
    user: &str,
    key: Option<&str>,
    proxy: bool,
    resident: impl FnOnce(&InitPayload) -> Arc<ResidentCopy>,
) -> (
    Arc<Account>,
    mpsc::UnboundedReceiver<crate::algos::AlgoInput>,
) {
    let init = InitPayload {
        account_name: "synthetic-main".into(),
        exchange_name: "bybit".into(),
        key: key.unwrap_or("synthetic-key").into(),
        ..Default::default()
    };
    let running = Arc::new(crate::algos::Running::new(registry.algos_ended.clone()));
    let (mut account, inbox) = testing::account_on(
        testing::HeldLoad {
            placed: Arc::default(),
            load_after_ms: Some(0.),
            ticker: true,
        },
        (user, &init.account_name, &init.exchange_name),
        None,
        crate::delegated::fingerprint(&init),
        running,
    );
    let inner = Arc::get_mut(&mut account).unwrap();
    inner.public_only = false;
    inner.proxy_authority_required = proxy;
    if key.is_some() {
        inner.identity = identity::digest(user, &init).unwrap();
    }
    inner.signing.set_test_resident(resident(&init));
    testing::register(registry, &account);
    hold(&account, 7);
    (account, inbox)
}

fn hold(account: &Account, id: u64) {
    let registration = format!("registration-{id}");
    hold_requesting(account, id, Delegated, &registration);
}

use crate::envelope::AccountSigningMode::{Cosigned, Delegated};

/// A live holder whose join confirmed `requested` for a client registration.
fn hold_requesting(
    account: &Account,
    id: u64,
    requested: crate::envelope::AccountSigningMode,
    registration: &str,
) {
    account.hold(
        id,
        Live {
            outbox: Arc::new(|_| true),
            token: None,
        },
    );
    account.confirm_signing_mode(
        id,
        &account.name,
        requested,
        Arc::new(|_| true),
        Some(registration),
    );
}

async fn advance(duration: Duration) {
    tokio::task::yield_now().await;
    tokio::time::advance(duration).await;
    for _ in 0..5 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test(start_paused = true)]
async fn delegated_retention_outlives_resume_grace_then_stops_and_revokes_existing_provider_clones()
{
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = tests::registry();
            let (account, mut inbox) = resident(&registry, "A");
            let provider = account.delegated().as_ref().unwrap().provider();
            let key = KeyRef("primary.signing".into());
            assert_eq!(
                provider.can_sign(&key, Algorithm::HmacSha256),
                Capability::Supported
            );
            registry.park("a-resume".into(), "A", vec![(account.clone(), 7)]);
            advance(Duration::from_secs(61)).await;
            assert!(
                inbox.try_recv().is_err(),
                "DELEGATED keeps running after the resume grace"
            );
            assert_eq!(registry.account_count(), 1);
            advance(RETAIN - Duration::from_secs(62)).await;
            assert!(inbox.try_recv().is_err());
            advance(Duration::from_secs(1)).await;
            match inbox.try_recv() {
                Ok(crate::algos::AlgoInput::Dispose { reason, stopped }) => {
                    assert_eq!(reason, "no client for 48 h");
                    assert!(stopped.is_some());
                }
                _ => panic!("retention must stop the algo host at 48 h"),
            }
            assert_eq!(registry.account_count(), 0);
            assert_eq!(
                provider.can_sign(&key, Algorithm::HmacSha256),
                Capability::Unsupported
            );
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn delegated_retention_also_covers_nonresumable_disconnects() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = tests::registry();
            let (account, mut inbox) = resident(&registry, "A");
            registry.release(&account, 7, Disposal::SocketClosed);
            advance(Duration::from_secs(61)).await;
            assert_eq!(registry.account_count(), 1);
            assert!(inbox.try_recv().is_err());
            advance(RETAIN).await;
            assert!(matches!(
                inbox.try_recv(),
                Ok(crate::algos::AlgoInput::Dispose { .. })
            ));
            assert_eq!(registry.account_count(), 0);
        })
        .await;
}

// AGT-79 replaced `delegated_removal_is_immediate_even_with_another_live_holder`:
// a page dispose (or orphan) now detaches only its own holder, and the account stops
// with the last live holder. The hard stops below still stop every holder.
#[tokio::test(start_paused = true)]
async fn a_page_dispose_detaches_only_its_holder_until_the_last_live_holder_leaves() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for disposal in [Disposal::PageDisposed, Disposal::Orphaned] {
                let registry = tests::registry();
                let (account, mut inbox) = resident(&registry, "A");
                hold(&account, 8);
                let provider = account.delegated().as_ref().unwrap().provider();
                let key = KeyRef("primary.signing".into());
                registry.release(&account, 7, disposal);
                assert_eq!(registry.account_count(), 1, "{disposal:?}");
                assert!(
                    inbox.try_recv().is_err(),
                    "{disposal:?}: algos keep running"
                );
                assert!(account.handle.disposal_cause().is_none());
                assert_eq!(
                    provider.can_sign(&key, Algorithm::HmacSha256),
                    Capability::Supported,
                    "{disposal:?}: the resident signer stays for the other holder"
                );
                assert!(!registry.holds(&account, 7));
                assert!(registry.holds(&account, 8));
                assert!(account.holders.lock().unwrap().retention.is_none());

                registry.release(&account, 8, disposal);
                assert_eq!(registry.account_count(), 0, "{disposal:?}");
                assert!(matches!(
                    inbox.try_recv(),
                    Ok(crate::algos::AlgoInput::Dispose { .. })
                ));
                assert_eq!(
                    provider.can_sign(&key, Algorithm::HmacSha256),
                    Capability::Unsupported,
                    "{disposal:?}: the last holder's dispose erases the key"
                );
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_parked_holder_never_keeps_a_disposed_account_resident() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = tests::registry();
            let (account, mut inbox) = resident(&registry, "A");
            hold(&account, 8);
            registry.park("stale-socket".into(), "A", vec![(account.clone(), 8)]);
            let provider = account.delegated().as_ref().unwrap().provider();
            registry.release(&account, 7, Disposal::PageDisposed);
            assert_eq!(registry.account_count(), 0);
            assert!(matches!(
                inbox.try_recv(),
                Ok(crate::algos::AlgoInput::Dispose { .. })
            ));
            assert_eq!(
                provider.can_sign(&KeyRef("primary.signing".into()), Algorithm::HmacSha256),
                Capability::Unsupported
            );
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_cosigned_holder_never_keeps_the_delegated_key_resident() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for unjoined in [false, true] {
                let registry = tests::registry();
                let (account, mut inbox) = resident(&registry, "A");
                hold_requesting(&account, 8, Cosigned, "registration-8");
                let provider = account.delegated().as_ref().unwrap().provider();
                if unjoined {
                    registry.release(&account, 7, Disposal::SocketClosed);
                    registry.dispose_delegated("A", "synthetic-main");
                } else {
                    registry.release(&account, 7, Disposal::PageDisposed);
                }
                assert_eq!(registry.account_count(), 0, "unjoined={unjoined}");
                assert!(matches!(
                    inbox.try_recv(),
                    Ok(crate::algos::AlgoInput::Dispose { .. })
                ));
                assert_eq!(
                    provider.can_sign(&KeyRef("primary.signing".into()), Algorithm::HmacSha256),
                    Capability::Unsupported,
                    "unjoined={unjoined}: the last DELEGATED holder's dispose erases the key"
                );
                assert!(!registry.holds(&account, 8));
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn hard_stops_still_stop_every_holder() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for hard_stop in ["dispose", "signer"] {
                let registry = tests::registry();
                let (account, mut inbox) = resident(&registry, "A");
                hold(&account, 8);
                let provider = account.delegated().as_ref().unwrap().provider();
                match hard_stop {
                    "dispose" => registry.dispose(&account, Disposal::PageDisposed),
                    _ => registry.signer_failed(&account),
                }
                assert_eq!(registry.account_count(), 0, "{hard_stop}");
                assert!(matches!(
                    inbox.try_recv(),
                    Ok(crate::algos::AlgoInput::Dispose { .. })
                ));
                assert_eq!(
                    provider.can_sign(&KeyRef("primary.signing".into()), Algorithm::HmacSha256),
                    Capability::Unsupported,
                    "{hard_stop}"
                );
                assert!(!registry.holds(&account, 7));
                assert!(!registry.holds(&account, 8));
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_dispose_never_counts_its_own_registrations_stale_socket() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for other_device in [false, true] {
                let registry = tests::registry();
                let (account, mut inbox) = resident(&registry, "A");
                // Socket 8 is socket 7's half-open predecessor: same registration.
                hold_requesting(&account, 8, Delegated, "registration-7");
                if other_device {
                    hold(&account, 9);
                }
                let provider = account.delegated().as_ref().unwrap().provider();
                let key = KeyRef("primary.signing".into());
                registry.release(&account, 7, Disposal::PageDisposed);
                assert!(!registry.holds(&account, 8), "other_device={other_device}");
                if !other_device {
                    assert_eq!(registry.account_count(), 0);
                    assert_eq!(
                        provider.can_sign(&key, Algorithm::HmacSha256),
                        Capability::Unsupported,
                        "the stale socket never keeps a removed key resident"
                    );
                    continue;
                }
                assert_eq!(registry.account_count(), 1);
                assert!(inbox.try_recv().is_err());
                assert!(registry.holds(&account, 9));
                registry.release(&account, 9, Disposal::PageDisposed);
                assert_eq!(registry.account_count(), 0);
                assert_eq!(
                    provider.can_sign(&key, Algorithm::HmacSha256),
                    Capability::Unsupported
                );
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn an_unjoined_dispose_still_stops_the_account_it_cannot_attribute() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = tests::registry();
            let (account, mut inbox) = resident(&registry, "A");
            let provider = account.delegated().as_ref().unwrap().provider();
            registry.dispose_delegated("A", "synthetic-main");
            assert_eq!(registry.account_count(), 0);
            assert!(matches!(
                inbox.try_recv(),
                Ok(crate::algos::AlgoInput::Dispose { .. })
            ));
            assert_eq!(
                provider.can_sign(&KeyRef("primary.signing".into()), Algorithm::HmacSha256),
                Capability::Unsupported
            );
            assert!(!registry.holds(&account, 7));
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn admitted_reconnect_resets_retention_and_another_user_does_not() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = tests::registry();
            let (a, mut a_inbox) = resident(&registry, "A");
            registry.park("a-old".into(), "A", vec![(a.clone(), 7)]);
            advance(Duration::from_secs(30)).await;
            let (b, mut b_inbox) = resident(&registry, "B");
            let outbox: Outbox = Arc::new(|_| true);
            assert!(
                registry
                    .resume(
                        "a-old",
                        "B",
                        8,
                        ("synthetic-token".into(), u64::MAX),
                        &|_| outbox.clone()
                    )
                    .is_empty()
            );
            // Login B remains active while A's original timer is pending.
            let resumed = registry.resume(
                "a-old",
                "A",
                9,
                ("synthetic-token".into(), u64::MAX),
                &|_| outbox.clone(),
            );
            assert!(resumed.is_empty(), "DELEGATED requires fresh admission");
            let init = InitPayload {
                account_name: a.name.clone(),
                exchange_name: "bybit".into(),
                key: "synthetic-key".into(),
                ..Default::default()
            };
            let admitted = registry
                .obtain_delegated(
                    "A",
                    &init,
                    9,
                    Some(("synthetic-token".into(), u64::MAX)),
                    outbox,
                    DelegatedPair::new("synthetic-key".into(), "synthetic-secret".into(), None),
                )
                .await
                .unwrap();
            assert!(Arc::ptr_eq(&a, &admitted));
            advance(RETAIN).await;
            assert!(
                a_inbox.try_recv().is_err(),
                "A's stale timer cannot stop an admitted reconnect"
            );
            assert!(b_inbox.try_recv().is_err());
            registry.release(&a, 9, Disposal::SocketClosed);
            advance(RETAIN - Duration::from_secs(1)).await;
            assert!(a_inbox.try_recv().is_err());
            advance(Duration::from_secs(1)).await;
            assert!(matches!(
                a_inbox.try_recv(),
                Ok(crate::algos::AlgoInput::Dispose { .. })
            ));
            assert!(
                b_inbox.try_recv().is_err(),
                "A's callback never disposes B's same-name account"
            );
            assert!(registry.held("B", &b.name, 7).is_some());
            registry.dispose(&b, Disposal::PageDisposed);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn retained_jobs_still_refuse_idle_release_after_resume_grace() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = tests::registry();
            let (account, mut inbox) = resident(&registry, "A");
            registry.user_drains.track(
                "A",
                &account.name,
                &account.running,
                Arc::downgrade(&account),
            );
            account.running.refresh(
                &[],
                &[json!({"id":"synthetic-schedule","status":"running"})],
                0,
            );
            registry.park("a-old".into(), "A", vec![(account.clone(), 7)]);
            advance(Duration::from_secs(61)).await;
            assert!(inbox.try_recv().is_err());
            let crate::user_drain::IdleReleaseError::Refused(refusal) = registry
                .user_drains
                .idle_release(
                    "A",
                    "01934fd2-a111-7111-8111-111111111111",
                    Duration::from_secs(20),
                )
                .unwrap_err()
            else {
                panic!("retained jobs refuse idle release")
            };
            assert_eq!(refusal["reason"], "parked-jobs");
            registry.dispose(&account, Disposal::PageDisposed);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn last_live_holder_starts_the_clock_and_stale_account_work_is_dropped() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = tests::registry();
            let (old, mut old_inbox) = resident(&registry, "A");
            hold(&old, 8);
            registry.park("a-first".into(), "A", vec![(old.clone(), 7)]);
            advance(RETAIN).await;
            assert!(
                old_inbox.try_recv().is_err(),
                "another live holder prevents retention expiry"
            );
            registry.release(&old, 8, Disposal::SocketClosed);
            advance(Duration::from_secs(30)).await;
            registry.dispose(&old, Disposal::PageDisposed);
            assert!(matches!(
                old_inbox.try_recv(),
                Ok(crate::algos::AlgoInput::Dispose { .. })
            ));
            let (current, mut current_inbox) = resident(&registry, "A");
            // Delayed disconnect work still carries the retired account incarnation.
            registry.park("a-stale".into(), "A", vec![(old, 8)]);
            advance(RETAIN).await;
            assert!(current_inbox.try_recv().is_err());
            assert!(registry.held("A", &current.name, 7).is_some());
            registry.dispose(&current, Disposal::PageDisposed);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn fresh_verified_admission_after_resume_grace_resets_retention_but_a_pending_init_does_not()
{
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = tests::registry();
            let (account, mut inbox) = resident(&registry, "A");
            registry.park("old-resume".into(), "A", vec![(account.clone(), 7)]);
            advance(Duration::from_secs(61)).await;
            let init = InitPayload {
                account_name: account.name.clone(),
                exchange_name: "bybit".into(),
                key: "synthetic-key".into(),
                ..Default::default()
            };
            let joined = registry
                .obtain_delegated(
                    "A",
                    &init,
                    8,
                    None,
                    Arc::new(|_| true),
                    DelegatedPair::new("synthetic-key".into(), "synthetic-secret".into(), None),
                )
                .await
                .unwrap();
            assert!(Arc::ptr_eq(&account, &joined));
            advance(RETAIN).await;
            assert!(inbox.try_recv().is_err());
            registry.release(&account, 8, Disposal::SocketClosed);
            advance(RETAIN - Duration::from_secs(1)).await;
            let pending = registry
                .begin_init(
                    "A",
                    account.registry_name(),
                    false,
                    &crate::delegated::fingerprint(&init),
                    None,
                )
                .unwrap();
            assert!(pending.authority.check().is_ok());
            advance(Duration::from_secs(1)).await;
            assert!(matches!(
                inbox.try_recv(),
                Ok(crate::algos::AlgoInput::Dispose { .. })
            ));
            assert_eq!(registry.account_count(), 0);
            assert_eq!(Disposal::NoClient48h.as_str(), "no_client_48h");
        })
        .await;
}

#[tokio::test]
async fn restart_marker_reports_waiting_for_key_and_keeps_restores_paused() {
    let now = now_ms();
    let marker = crate::interruption::Marker::parse(
        &json!({"reason":"waiting-for-key", "fromBox":"01", "at":now}).to_string(),
        now,
    )
    .expect("restart marker is recognized");
    let notices = Arc::new(crate::interruption::Notices::default());
    notices.record("A", marker).unwrap();
    let guard = notices.account("A", "synthetic-main");
    assert!(guard.must_pause("synthetic-twap", guard.epoch(None)));
    for id in [Some("current-registration"), None] {
        let (out, mut messages, _) = crate::out::Out::channel(4);
        // A fresh process receives the router-owned marker again.
        let notices = crate::interruption::Notices::default();
        notices
            .record(
                "A",
                crate::interruption::Marker::parse(
                    &json!({"reason":"waiting-for-key", "fromBox":"01", "at":now}).to_string(),
                    now,
                )
                .unwrap(),
            )
            .unwrap();
        assert!(notices.deliver("A", "synthetic-main", &out.with_instance_id(id)));
        let text = messages.recv().await.unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        assert!(
            value["d"]["data"][0]
                .as_str()
                .unwrap()
                .contains("waiting for your key")
        );
        assert_eq!(value["d"].get("instanceId").and_then(Value::as_str), id);
        assert!(!notices.deliver("B", "synthetic-main", &out));
    }
}

#[tokio::test(start_paused = true)]
async fn delegated_cleanup_does_not_leave_a_copy_behind_an_old_resume_holder() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for disposal in [Disposal::Moving, Disposal::JoinFailed] {
                let registry = tests::registry();
                let (account, mut inbox) = resident(&registry, "A");
                let provider = account.delegated().as_ref().unwrap().provider();
                registry.park("old-resume".into(), "A", vec![(account.clone(), 7)]);
                hold(&account, 8);
                hold(&account, 9);
                registry.release(&account, 8, disposal);
                assert_eq!(
                    registry.account_count(),
                    1,
                    "another admitted client still holds it"
                );
                assert!(inbox.try_recv().is_err());
                registry.release(&account, 9, disposal);
                assert_eq!(
                    registry.account_count(),
                    0,
                    "a resume holder cannot retain a moved or failed account"
                );
                assert!(matches!(
                    inbox.try_recv(),
                    Ok(crate::algos::AlgoInput::Dispose { .. })
                ));
                assert_eq!(
                    provider.can_sign(&KeyRef("primary.signing".into()), Algorithm::HmacSha256),
                    Capability::Unsupported
                );
                advance(RETAIN).await;
                assert!(
                    inbox.try_recv().is_err(),
                    "old resume cleanup cannot dispose it again"
                );
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn delegated_stopped_accounts_retire_without_rearming_retention() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for live in [false, true] {
                let registry = tests::registry();
                let (account, _inbox) = resident(&registry, "A");
                if !live {
                    registry.release(&account, 7, Disposal::SocketClosed);
                }
                registry.stop(
                    &account,
                    "the server is restarting",
                    None,
                    DisposalCause::Stopped,
                );
                if live {
                    assert_eq!(
                        registry.account_count(),
                        1,
                        "live holders remain for stop notices"
                    );
                    registry.release(&account, 7, Disposal::SocketClosed);
                }
                assert_eq!(
                    registry.account_count(),
                    0,
                    "a stopped account must leave the registry"
                );
                advance(RETAIN).await;
                assert_eq!(registry.account_count(), 0);
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn stopped_delegated_resumable_holders_release_only_their_own_incarnation() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = tests::registry();
            let (account, _inbox) = resident(&registry, "A");
            hold(&account, 8);
            registry.stop(&account, "stopped", None, DisposalCause::Stopped);

            registry.park("wrong-owner".into(), "B", vec![(account.clone(), 7)]);
            assert!(registry.holds(&account, 7));
            registry.park("first-close".into(), "A", vec![(account.clone(), 7)]);
            assert!(!registry.holds(&account, 7));
            assert!(registry.holds(&account, 8));
            assert_eq!(registry.account_count(), 1, "keep the other live holder");
            assert!(account.holders.lock().unwrap().retention.is_none());

            registry.park("last-close".into(), "A", vec![(account.clone(), 8)]);
            assert!(!registry.holds(&account, 8));
            assert_eq!(registry.account_count(), 0);
            assert!(registry.parked.lock().unwrap().is_empty());

            let (replacement, _inbox) = resident(&registry, "A");
            registry.park("late-close".into(), "A", vec![(account.clone(), 7)]);
            advance(RETAIN + Duration::from_secs(60 * 60)).await;
            assert_eq!(registry.account_count(), 1);
            assert!(Arc::ptr_eq(
                &registry.held("A", &replacement.name, 7).unwrap(),
                &replacement
            ));
            assert!(replacement.handle.disposal_cause().is_none());
            registry.dispose(&replacement, Disposal::PageDisposed);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn retention_and_explicit_disposal_use_the_identity_key_after_alias_grace() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = tests::registry();
            let (account, _inbox) = resident_with_identity(&registry, "A", Some("synthetic-key"));
            assert_ne!(account.registry_name(), account.name);
            let provider = account.delegated().as_ref().unwrap().provider();
            assert!(account.hold_label(
                8,
                Live {
                    outbox: Arc::new(|_| true),
                    token: None
                },
                "phone-label"
            ));
            registry.release_label(&account, 7, &account.name, Disposal::SocketClosed);
            assert!(account.holders.lock().unwrap().retention.is_none());
            registry.release_label(&account, 8, "phone-label", Disposal::SocketClosed);
            assert!(account.holders.lock().unwrap().retention.is_some());
            registry.dispose_delegated("B", &account.name);
            assert_eq!(registry.account_count(), 1);
            assert!(account.hold_label(
                9,
                Live {
                    outbox: Arc::new(|_| true),
                    token: None
                },
                "phone-label"
            ));
            assert!(account.holders.lock().unwrap().retention.is_none());
            registry.release_label(&account, 9, "phone-label", Disposal::SocketClosed);
            advance(Duration::from_secs(61)).await;
            let public = InitPayload {
                account_name: "phone-label".into(),
                exchange_name: "bybit".into(),
                public_only: Some(true),
                ..Default::default()
            };
            assert!(
                registry
                    .signing_mode_refusal("A", &public, true, false)
                    .is_some()
            );
            assert!(registry.rebuild_refusal("A", &public, true, 10).is_some());
            assert!(
                registry
                    .signing_mode_refusal("B", &public, true, false)
                    .is_none()
            );
            registry.dispose_delegated("B", "phone-label");
            assert_eq!(registry.account_count(), 1);
            registry.dispose_delegated("A", "phone-label");
            assert_eq!(registry.account_count(), 0);
            assert_eq!(
                provider.can_sign(&KeyRef("primary.signing".into()), Algorithm::HmacSha256),
                Capability::Unsupported
            );
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn retiring_old_settings_preserves_only_the_published_successors_resident() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = tests::registry();
            let (account, _inbox) = resident_with_identity(&registry, "A", Some("synthetic-key"));
            let resident = account.delegated().as_ref().unwrap().clone();
            let provider = resident.provider();
            let running = Arc::new(crate::algos::Running::new(registry.algos_ended.clone()));
            let (mut successor, _successor_inbox) = testing::account_on(
                testing::HeldLoad {
                    placed: Arc::default(),
                    load_after_ms: Some(0.),
                    ticker: true,
                },
                ("A", &account.name, &account.venue),
                None,
                "changed-settings".into(),
                running,
            );
            let inner = Arc::get_mut(&mut successor).unwrap();
            inner.identity = account.identity.clone();
            inner.public_only = false;
            inner.signing.set_test_resident(resident);
            testing::register(&registry, &successor);
            hold(&successor, 8);
            registry.stop_for_new_settings(&account, Arc::new(|_| true), Disposal::SettingsChanged);
            assert_eq!(
                provider.can_sign(&KeyRef("primary.signing".into()), Algorithm::HmacSha256),
                Capability::Supported
            );
            assert!(registry.holds(&successor, 8));
            registry.dispose(&successor, Disposal::PageDisposed);
            assert_eq!(registry.account_count(), 0);
            assert_eq!(
                provider.can_sign(&KeyRef("primary.signing".into()), Algorithm::HmacSha256),
                Capability::Unsupported
            );
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn an_ambiguous_unjoined_label_never_disposes_a_guessed_identity() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = tests::registry();
            let (first, _first_inbox) =
                resident_with_identity(&registry, "A", Some("synthetic-key-A"));
            let (second, _second_inbox) =
                resident_with_identity(&registry, "A", Some("synthetic-key-B"));
            hold(&second, 8);
            registry.release(&second, 7, Disposal::SocketClosed);
            registry.release(&first, 7, Disposal::SocketClosed);
            registry.release(&second, 8, Disposal::SocketClosed);
            let first_provider = first.delegated().as_ref().unwrap().provider();
            let second_provider = second.delegated().as_ref().unwrap().provider();
            registry.dispose_delegated("A", &first.name);
            assert_eq!(registry.account_count(), 2);
            let key = KeyRef("primary.signing".into());
            assert_eq!(
                first_provider.can_sign(&key, Algorithm::HmacSha256),
                Capability::Supported
            );
            assert_eq!(
                second_provider.can_sign(&key, Algorithm::HmacSha256),
                Capability::Supported
            );
            registry.dispose(&first, Disposal::PageDisposed);
            assert_eq!(registry.account_count(), 1);
            registry.dispose_delegated("A", &second.name);
            assert_eq!(registry.account_count(), 0);
            assert_eq!(
                first_provider.can_sign(&key, Algorithm::HmacSha256),
                Capability::Unsupported
            );
            assert_eq!(
                second_provider.can_sign(&key, Algorithm::HmacSha256),
                Capability::Unsupported
            );
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn earlier_disconnected_alias_still_revokes_resident_and_does_not_touch_another_user() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for resumable in [false, true] {
                let registry = tests::registry();
                let (account, mut inbox) = resident(&registry, "A");
                let (other, mut other_inbox) = resident(&registry, "B");
                let provider = account.delegated().as_ref().unwrap().provider();
                for (id, label) in [(8, "phone"), (9, "tablet")] {
                    assert!(account.hold_label(
                        id,
                        Live {
                            outbox: Arc::new(|_| true),
                            token: None
                        },
                        label
                    ));
                }
                if resumable {
                    registry.park("earlier-phone".into(), "A", vec![(account.clone(), 8)]);
                    advance(Duration::from_secs(61)).await;
                } else {
                    registry.release(&account, 8, Disposal::SocketClosed);
                }
                registry.release(&account, 7, Disposal::SocketClosed);
                registry.release(&account, 9, Disposal::SocketClosed);
                advance(Duration::from_secs(61)).await;
                registry.dispose_delegated("A", "phone");
                assert_eq!(
                    registry.account_count(),
                    1,
                    "the earlier label must still reach its own retained account"
                );
                assert_eq!(
                    provider.can_sign(&KeyRef("primary.signing".into()), Algorithm::HmacSha256),
                    Capability::Unsupported
                );
                assert!(matches!(
                    inbox.try_recv(),
                    Ok(crate::algos::AlgoInput::Dispose { .. })
                ));
                assert!(other_inbox.try_recv().is_err());
                assert!(registry.holds(&other, 7));
                registry.dispose(&other, Disposal::PageDisposed);
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn disconnected_alias_on_a_still_live_socket_keeps_revocation_lookup() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = tests::registry();
            let (account, _inbox) = resident(&registry, "A");
            assert!(account.hold_label(
                7,
                Live {
                    outbox: Arc::new(|_| true),
                    token: None
                },
                "phone"
            ));
            let provider = account.delegated().as_ref().unwrap().provider();
            registry.release_label(&account, 7, "phone", Disposal::SocketClosed);
            assert!(registry.holds(&account, 7));
            registry.release(&account, 7, Disposal::SocketClosed);
            advance(Duration::from_secs(61)).await;
            registry.dispose_delegated("A", "phone");
            assert_eq!(registry.account_count(), 0);
            assert_eq!(
                provider.can_sign(&KeyRef("primary.signing".into()), Algorithm::HmacSha256),
                Capability::Unsupported
            );
        })
        .await;
}

// Interim authority fence; the maintainer keeps renewal versus long-term early
// stop undecided. A selected proxy cannot extend its socket-issued authority.
#[tokio::test(start_paused = true)]
async fn retained_proxy_expiry_stops_and_erases_with_a_recorded_reason() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = tests::registry();
            let (account, mut inbox) =
                resident_with_authority(&registry, "proxy-expiry", Some("synthetic-key"), true);
            let provider = account.delegated().unwrap().provider();
            account
                .token
                .set(Some(("synthetic-token".into(), now_ms() + 10_000)));
            assert!(account.proxy_authority_required());
            registry.release(&account, 7, Disposal::SocketClosed);
            advance(Duration::from_secs(11)).await;
            match inbox.try_recv() {
                Ok(crate::algos::AlgoInput::Dispose { reason, stopped }) => {
                    assert_eq!(reason, "proxy-token-expired");
                    assert!(stopped.is_some());
                }
                _ => panic!("expired proxy authority must stop retained algos visibly"),
            }
            assert_eq!(registry.account_count(), 0);
            assert_eq!(
                provider.can_sign(&KeyRef("primary.signing".into()), Algorithm::HmacSha256),
                Capability::Unsupported
            );
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn direct_retention_does_not_inherit_the_socket_proxy_expiry() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = tests::registry();
            let (account, mut inbox) = resident(&registry, "direct-control");
            let provider = account.delegated().unwrap().provider();
            account
                .token
                .set(Some(("synthetic-token".into(), now_ms() + 10_000)));
            assert!(!account.proxy_authority_required());
            registry.release(&account, 7, Disposal::SocketClosed);
            advance(Duration::from_secs(11)).await;
            assert!(inbox.try_recv().is_err());
            assert_eq!(registry.account_count(), 1);
            assert_eq!(
                provider.can_sign(&KeyRef("primary.signing".into()), Algorithm::HmacSha256),
                Capability::Supported
            );
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn admitted_proxy_reconnect_replaces_the_old_expiry_without_affecting_another_user() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = tests::registry();
            let (account, mut inbox) =
                resident_with_authority(&registry, "reconnect-proxy", Some("synthetic-key"), true);
            let (peer, mut peer_inbox) = resident(&registry, "other-user");
            let provider = account.delegated().unwrap().provider();
            account
                .token
                .set(Some(("old-token".into(), now_ms() + 10_000)));
            registry.release(&account, 7, Disposal::SocketClosed);
            registry.release(&peer, 7, Disposal::SocketClosed);
            advance(Duration::from_secs(5)).await;
            assert!(account.hold(
                9,
                Live {
                    outbox: Arc::new(|_| true),
                    token: Some(("new-token".into(), now_ms() + 60_000)),
                }
            ));
            advance(Duration::from_secs(11)).await;
            assert!(
                inbox.try_recv().is_err(),
                "old token timer cannot stop a fresh admitted holder"
            );
            registry.release(&account, 9, Disposal::SocketClosed);
            advance(Duration::from_secs(61)).await;
            match inbox.try_recv() {
                Ok(crate::algos::AlgoInput::Dispose { reason, stopped }) => {
                    assert_eq!(reason, "proxy-token-expired");
                    assert!(stopped.is_some());
                }
                _ => panic!("new proxy authority must still expire"),
            }
            assert!(peer_inbox.try_recv().is_err());
            assert_eq!(registry.account_count(), 1);
            assert_eq!(
                provider.can_sign(&KeyRef("primary.signing".into()), Algorithm::HmacSha256),
                Capability::Unsupported
            );
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn retained_proxy_without_transport_authority_stops_and_erases_immediately() {
    tokio::task::LocalSet::new().run_until(async {
        let registry = tests::registry();
        let (account, mut inbox) = resident_with_authority(&registry, "missing-proxy-authority", Some("synthetic-key"), true);
        let provider = account.delegated().unwrap().provider();
        registry.release(&account, 7, Disposal::SocketClosed);
        advance(Duration::ZERO).await;
        assert!(matches!(inbox.try_recv(), Ok(crate::algos::AlgoInput::Dispose { reason, .. }) if reason == "proxy-token-expired"));
        assert_eq!(registry.account_count(), 0);
        assert_eq!(provider.can_sign(&KeyRef("primary.signing".into()), Algorithm::HmacSha256), Capability::Unsupported);
    }).await;
}

async fn successor_publication_control(published: bool) {
    let registry = tests::registry();
    let (account, _inbox) = resident(&registry, "unpublished-owner");
    let owner = account.delegated().unwrap();
    let provider = owner.provider();
    let request = safe_cex_engine::signing::SignRequest {
        receipt: Default::default(),
        context: Default::default(),
        operations: vec![safe_cex_engine::signing::Operation::Sign {
            algorithm: Algorithm::HmacSha256,
            key: KeyRef("primary.signing".into()),
            payload: vec![safe_cex_engine::signing::PayloadPart::Bytes { bytes: vec![7] }],
        }],
    };
    assert!(provider.sign_ready(&request).unwrap().is_ok());
    let deferred = provider.sign(request.clone());
    let running = Arc::new(crate::algos::Running::new(registry.algos_ended.clone()));
    let (mut successor, _successor_inbox) = testing::account_on(
        testing::HeldLoad {
            placed: Arc::default(),
            load_after_ms: Some(0.),
            ticker: true,
        },
        (&account.user, &account.name, &account.venue),
        None,
        "pending-settings".into(),
        running,
    );
    let inner = Arc::get_mut(&mut successor).unwrap();
    inner.identity = account.identity.clone();
    inner.public_only = false;
    inner.signing.set_test_resident(owner.clone());
    let mut attempt = registry
        .begin_init(
            &account.user,
            account.registry_name(),
            false,
            "pending-settings",
            None,
        )
        .unwrap();
    registry.release(&account, 7, Disposal::SocketClosed);
    // Delay only registry cleanup: escaped handles must enforce their deadline.
    account
        .holders
        .lock()
        .unwrap()
        .retention
        .as_ref()
        .unwrap()
        .abort_cleanup_for_test();
    assert!(successor.hold_before_publication(
        8,
        Live {
            outbox: Arc::new(|_| true),
            token: None
        },
        &successor.name
    ));
    if published {
        assert!(
            attempt
                .publish(&registry.accounts, &successor)
                .unwrap()
                .is_some()
        );
    } else {
        let _newer = registry
            .begin_init(
                &account.user,
                account.registry_name(),
                false,
                "superseding-settings",
                None,
            )
            .unwrap();
        assert!(attempt.publish(&registry.accounts, &successor).is_err());
    }
    let current = registry.accounts.lock().unwrap()
        [&(account.user.clone(), account.registry_name().into())]
        .clone();
    assert!(Arc::ptr_eq(
        &current,
        if published { &successor } else { &account }
    ));
    advance(RETAIN).await;
    if published {
        assert_eq!(
            provider.can_sign(&KeyRef("primary.signing".into()), Algorithm::HmacSha256),
            Capability::Supported
        );
        assert!(provider.sign_ready(&request).unwrap().is_ok());
        assert!(deferred.await.is_ok());
    } else {
        assert_eq!(
            provider.can_sign(&KeyRef("primary.signing".into()), Algorithm::HmacSha256),
            Capability::Unsupported
        );
        assert_eq!(
            provider.sign_ready(&request),
            Some(Err(safe_cex_engine::signing::SignError::Unavailable))
        );
        assert_eq!(
            deferred.await,
            Err(safe_cex_engine::signing::SignError::Unavailable)
        );
    }
}

#[tokio::test(start_paused = true)]
async fn unpublished_settings_successor_cannot_clear_the_residents_retention_deadline() {
    tokio::task::LocalSet::new()
        .run_until(successor_publication_control(false))
        .await;
}

#[tokio::test(start_paused = true)]
async fn published_settings_successor_cancels_the_residents_old_retention_deadline() {
    tokio::task::LocalSet::new()
        .run_until(successor_publication_control(true))
        .await;
}

/// Owns no key material, so these tests also run where protected key setup
/// is unsupported (protected allocation is Linux-only).
struct KeylessSigner;
impl safe_cex_engine::signing::ProtectedSigner for KeylessSigner {}
impl safe_cex_engine::signing::Signer for KeylessSigner {
    fn immediate(&self) -> bool {
        true
    }
    fn can_sign(&self, _: &KeyRef, _: Algorithm) -> Capability {
        Capability::Unsupported
    }
    fn sign_ready(
        &self,
        _: &safe_cex_engine::signing::SignRequest,
    ) -> Option<Result<safe_cex_engine::signing::SignResult, safe_cex_engine::signing::SignError>>
    {
        Some(Err(safe_cex_engine::signing::SignError::Refused))
    }
    fn sign(
        &self,
        _: safe_cex_engine::signing::SignRequest,
    ) -> safe_cex_engine::signing::SignFuture {
        Box::pin(std::future::ready(Err(
            safe_cex_engine::signing::SignError::Refused,
        )))
    }
}

fn keyless_resident(
    registry: &Registry,
    user: &str,
    proxy: bool,
) -> (
    Arc<Account>,
    mpsc::UnboundedReceiver<crate::algos::AlgoInput>,
) {
    resident_from(registry, user, Some("synthetic-key"), proxy, |_| {
        ResidentCopy::test_protected_provider(
            safe_cex_engine::signing::ProtectedProvider::new(Arc::new(KeylessSigner)).unwrap(),
            safe_cex_engine::signing::Identities::default(),
        )
    })
}

fn notifying_registry() -> (Arc<Registry>, crate::host_notify::HostNotifier) {
    let notifier = crate::host_notify::HostNotifier::unstarted();
    let mut registry = Registry::new(
        EngineThreads::start(1).unwrap(),
        Duration::from_millis(1),
        Duration::from_secs(60),
    )
    .with_host_notify(Some(notifier.clone()));
    testing::set_join_admission(&mut registry, false);
    (Arc::new(registry), notifier)
}

fn notified(notifier: &crate::host_notify::HostNotifier) -> Vec<serde_json::Value> {
    notifier
        .queued()
        .iter()
        .map(|notification| serde_json::to_value(notification).unwrap())
        .collect()
}

const LEAD: Duration = crate::host_notify::WARNING_LEAD;

#[tokio::test(start_paused = true)]
async fn shutdown_warning_fires_one_hour_before_retention_expiry_then_shutdown() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (registry, notifier) = notifying_registry();
            let (account, mut inbox) = keyless_resident(&registry, "warned", false);
            advance(RETAIN).await;
            assert!(notified(&notifier).is_empty(), "a held account arms nothing");
            registry.release(&account, 7, Disposal::SocketClosed);
            advance(RETAIN - LEAD - Duration::from_secs(1)).await;
            assert!(notified(&notifier).is_empty());
            advance(Duration::from_secs(1)).await;
            let queued = notified(&notifier);
            assert_eq!(queued.len(), 1);
            let warning = &queued[0];
            assert_eq!(warning["kind"], "delegated.shutdownWarning");
            assert_eq!(warning["severity"], "warning");
            assert_eq!(warning["user"], "warned");
            assert_eq!(warning["venue"], "bybit");
            assert_eq!(warning["clientPresent"], false);
            assert_eq!(warning["fields"], serde_json::json!({"cause": "noClient48h"}));
            // The wall clock does not move under paused Tokio time.
            let lead = warning["deadline"].as_u64().unwrap() - warning["occurredAt"].as_u64().unwrap();
            let retain_ms = RETAIN.as_millis() as u64;
            assert!((retain_ms - 5_000..=retain_ms).contains(&lead), "{lead}");
            assert!(inbox.try_recv().is_err(), "the warning stops nothing");
            advance(LEAD).await;
            assert!(matches!(
                inbox.try_recv(),
                Ok(crate::algos::AlgoInput::Dispose { reason, .. }) if reason == "no client for 48 h"
            ));
            let queued = notified(&notifier);
            assert_eq!(queued.len(), 2);
            let shutdown = &queued[1];
            assert_eq!(shutdown["kind"], "delegated.shutdown");
            assert_eq!(shutdown["severity"], "critical");
            assert_eq!(shutdown["clientPresent"], false);
            assert_eq!(shutdown["fields"], serde_json::json!({"cause": "noClient48h"}));
            assert!(shutdown.get("deadline").is_none());
            assert_eq!(shutdown["accountDigest"], warning["accountDigest"]);
            assert_ne!(shutdown["id"], warning["id"]);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn admitted_reconnect_cancels_the_pending_warning_and_rearms_with_the_next_interval() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (registry, notifier) = notifying_registry();
            let (account, mut inbox) = keyless_resident(&registry, "returning", false);
            registry.release(&account, 7, Disposal::SocketClosed);
            advance(Duration::from_secs(10)).await;
            hold(&account, 8);
            advance(RETAIN).await;
            assert!(
                notified(&notifier).is_empty(),
                "retention cancel aborts the warning"
            );
            assert!(inbox.try_recv().is_err());
            registry.release(&account, 8, Disposal::SocketClosed);
            advance(RETAIN - LEAD).await;
            let queued = notified(&notifier);
            assert_eq!(queued.len(), 1);
            assert_eq!(queued[0]["kind"], "delegated.shutdownWarning");
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn short_proxy_interval_skips_the_warning_and_reports_its_cause() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (registry, notifier) = notifying_registry();
            let (account, mut inbox) =
                keyless_resident(&registry, "short-proxy", true);
            account
                .token
                .set(Some(("synthetic-token".into(), now_ms() + 30 * 60 * 1000)));
            registry.release(&account, 7, Disposal::SocketClosed);
            advance(Duration::from_secs(31 * 60)).await;
            assert!(matches!(
                inbox.try_recv(),
                Ok(crate::algos::AlgoInput::Dispose { reason, .. }) if reason == "proxy-token-expired"
            ));
            let queued = notified(&notifier);
            assert_eq!(queued.len(), 1, "an interval under one hour has no warning");
            assert_eq!(queued[0]["kind"], "delegated.shutdown");
            assert_eq!(
                queued[0]["fields"],
                serde_json::json!({"cause": "proxyTokenExpired"})
            );
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn drain_reports_server_restart_for_delegated_residents_with_client_presence() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (registry, notifier) = notifying_registry();
            let (held, _held_inbox) = keyless_resident(&registry, "held-user", false);
            let (retained, _retained_inbox) = keyless_resident(&registry, "retained-user", false);
            registry.release(&retained, 7, Disposal::SocketClosed);
            let started = tokio::time::Instant::now();
            registry.drain(Duration::from_millis(100)).await;
            // The unstarted outbox never empties: the flush bound ends the wait.
            assert!(
                started.elapsed() <= crate::host_notify::RESTART_FLUSH + Duration::from_millis(100)
            );
            let queued = notified(&notifier);
            assert_eq!(queued.len(), 2);
            for notification in &queued {
                assert_eq!(notification["kind"], "delegated.shutdown");
                assert_eq!(
                    notification["fields"],
                    serde_json::json!({"cause": "serverRestart"})
                );
                let present = notification["user"] == "held-user";
                assert_eq!(notification["clientPresent"], present, "{notification}");
            }
            assert_eq!(
                queued.iter().filter(|n| n["user"] == "held-user").count(),
                1
            );
            drop(held);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn without_a_notifier_retention_arms_no_warning_and_queues_nothing() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = tests::registry();
            let (account, mut inbox) = keyless_resident(&registry, "off", false);
            assert!(registry.host_notify.is_none());
            registry.release(&account, 7, Disposal::SocketClosed);
            advance(RETAIN).await;
            assert!(matches!(
                inbox.try_recv(),
                Ok(crate::algos::AlgoInput::Dispose { .. })
            ));
        })
        .await;
}
