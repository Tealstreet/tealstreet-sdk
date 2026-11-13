use super::*;
use crate::delegated::{DelegatedPair, prepare_delegated};
use safe_cex_crypto::signer::Secret;
use safe_cex_engine::signing::{Algorithm, Capability, KeyRef};

const RETAIN: Duration = Duration::from_secs(48 * 60 * 60);

fn resident(
    registry: &Registry,
    user: &str,
) -> (
    Arc<Account>,
    mpsc::UnboundedReceiver<crate::algos::AlgoInput>,
) {
    let init = InitPayload {
        account_name: "synthetic-main".into(),
        exchange_name: "bybit".into(),
        key: "synthetic-key".into(),
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
    inner.delegated = Some(
        prepare_delegated(
            &init,
            DelegatedPair::new(
                init.key.clone(),
                Secret::new("synthetic-secret".into()),
                None,
            ),
        )
        .unwrap()
        .resident,
    );
    testing::register(registry, &account);
    hold(&account, 7);
    (account, inbox)
}

fn hold(account: &Account, id: u64) {
    account.hold(
        id,
        Live {
            outbox: Arc::new(|_| true),
            token: None,
        },
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
            let provider = account.delegated.as_ref().unwrap().provider();
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

#[tokio::test(start_paused = true)]
async fn delegated_removal_is_immediate_even_with_another_live_holder() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = tests::registry();
            let (account, mut inbox) = resident(&registry, "A");
            hold(&account, 8);
            let provider = account.delegated.as_ref().unwrap().provider();
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
            assert!(!registry.holds(&account, 8));
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
                    DelegatedPair::new(
                        "synthetic-key".into(),
                        Secret::new("synthetic-secret".into()),
                        None,
                    ),
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
                    DelegatedPair::new(
                        "synthetic-key".into(),
                        Secret::new("synthetic-secret".into()),
                        None,
                    ),
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
                    &account.name,
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
                let provider = account.delegated.as_ref().unwrap().provider();
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
