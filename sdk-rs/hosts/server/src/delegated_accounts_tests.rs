use super::*;
use crate::delegated::{DelegatedPair, prepare_delegated};

fn init() -> InitPayload {
    InitPayload {
        account_name: "synthetic-main".into(),
        exchange_name: "bybit".into(),
        key: "synthetic-key".into(),
        ..Default::default()
    }
}
fn pair() -> DelegatedPair {
    DelegatedPair::new(
        "synthetic-key".into(),
        "synthetic-signing-fixture".into(),
        None,
    )
}

fn resident_account(registry: &Registry, user: &str) -> Arc<Account> {
    let init = init();
    let running = Arc::new(crate::algos::Running::new(registry.algos_ended.clone()));
    let (mut account, _inbox) = testing::account_on(
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
    let inner = Arc::get_mut(&mut account).expect("unshared fixture");
    inner.identity = identity::digest(user, &init).unwrap();
    inner.public_only = false;
    inner.link = None;
    inner.delegated = Some(
        prepare_delegated(&init, pair())
            .unwrap_or_else(|_| panic!("Bybit setup"))
            .resident,
    );
    inner.fingerprint = crate::delegated::fingerprint(&init);
    testing::register(registry, &account);
    account
}

#[tokio::test]
async fn apply_settings_is_required_even_when_only_the_delegated_copy_holds_the_account() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = tests::registry();
            let account = resident_account(&registry, "A");
            let changed = InitPayload {
                enable_spot_markets: Some(true),
                ..init()
            };
            assert!(account.holders.lock().unwrap().by_id.is_empty());
            assert!(
                registry
                    .rebuild_refusal_for_mode("A", &changed, false, 8, true)
                    .is_some()
            );
            let result = registry
                .obtain_delegated("A", &changed, 8, None, Arc::new(|_| true), pair())
                .await;
            assert!(
                result.is_err(),
                "the resident account must not be rebuilt implicitly"
            );
            assert!(account.handle.disposal_cause().is_none());
            assert!(
                registry
                    .accounts
                    .lock()
                    .unwrap()
                    .get(&("A".into(), account.registry_name().to_owned()))
                    .is_some_and(|current| Arc::ptr_eq(current, &account))
            );
            registry.dispose(&account, Disposal::PageDisposed);
        })
        .await;
}

#[tokio::test]
async fn cosigned_cannot_attach_to_or_rebuild_a_delegated_engine() {
    tokio::task::LocalSet::new().run_until(async {
        let registry = tests::registry(); let init = init(); let account = resident_account(&registry, "A");
        assert!(!account.is_public()); assert!(account.link.is_none());
        let original = account.handle.clone();
        assert!(registry.rebuild_refusal("A", &init, false, 8).is_some());
        let result = registry.obtain("A", &init, false, 8, None, Arc::new(|_| true)).await;
        assert!(matches!(result, Err(ref e) if e == "Account signing mode differs; account not attached"));
        assert!(registry.accounts.lock().unwrap().get(&("A".into(), account.registry_name().to_owned())).is_some_and(|a| Arc::ptr_eq(a, &account)));
        assert!(original.disposal_cause().is_none()); assert!(!registry.holds(&account, 8));
        registry.dispose(&account, Disposal::PageDisposed);
    }).await;
}

#[tokio::test]
async fn delegated_cannot_attach_to_or_rebuild_a_cosigned_engine() {
    tokio::task::LocalSet::new().run_until(async {
        let registry = tests::registry(); let init = init(); let account = testing::trading_account(&registry, "A", &init);
        assert!(registry.signing_mode_refusal("A", &init, false, true).is_some());
        let result = registry.obtain_delegated("A", &init, 8, None, Arc::new(|_| true), pair()).await;
        assert!(matches!(result, Err(ref e) if e == "Account signing mode differs; account not attached"));
        assert!(account.handle.disposal_cause().is_none()); assert!(!registry.holds(&account, 8));
        registry.dispose(&account, Disposal::PageDisposed);
    }).await;
}

#[tokio::test]
async fn delegated_reuses_the_same_users_matching_engine_without_a_signer_link() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = tests::registry();
            let account = resident_account(&registry, "A");
            let joined = registry
                .obtain_delegated("A", &init(), 8, None, Arc::new(|_| true), pair())
                .await
                .unwrap_or_else(|_| panic!("same-mode attach"));
            assert!(Arc::ptr_eq(&account, &joined));
            assert!(registry.holds(&account, 8));
            assert!(!account.is_public());
            assert!(account.link.is_none());
            assert!(account.delegated.is_some());
            assert!(
                registry
                    .signing_mode_refusal("B", &init(), false, false)
                    .is_none()
            );
            registry.dispose(&account, Disposal::PageDisposed);
        })
        .await;
}

#[tokio::test]
async fn mode_is_captured_before_the_init_pacing_deferral() {
    let registry = tests::registry();
    let init = init();
    let original = registry
        .begin_init("A", &init.account_name, false, &init.fingerprint(), None)
        .unwrap();
    let delegated = registry
        .begin_init(
            "A",
            &init.account_name,
            false,
            &crate::delegated::fingerprint(&init),
            None,
        )
        .unwrap();
    assert!(original.authority.check().is_err());
    assert!(delegated.authority.check().is_ok());
    let other_user = registry
        .begin_init("B", &init.account_name, false, &init.fingerprint(), None)
        .unwrap();
    assert!(delegated.authority.check().is_ok());
    assert!(other_user.authority.check().is_ok());
}

#[tokio::test]
async fn every_unhosted_venue_refuses_before_mutating_a_running_cosigned_account() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = tests::registry();
            let mut init = init();
            let account = testing::trading_account(&registry, "A", &init);
            for venue in ["hyrotrader", "binanceoptions", "kucoinspot", "unknown"] {
                assert!(
                    safe_cex_native::resolve::rest_base_url(venue, false).is_err(),
                    "{venue}: refusal control must lack a hosted origin"
                );
                init.exchange_name = venue.into();
                let result = registry
                    .obtain_delegated("A", &init, 8, None, Arc::new(|_| true), pair())
                    .await;
                assert!(
                    matches!(result, Err(ref e) if e == "DELEGATED is not enabled for this venue")
                );
                assert!(account.handle.disposal_cause().is_none());
                assert!(!registry.holds(&account, 8));
            }
            registry.dispose(&account, Disposal::PageDisposed);
        })
        .await;
}

#[tokio::test]
async fn cosigned_socket_receives_a_visible_mode_refusal_and_keeps_the_live_account() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = tests::registry();
            let account = resident_account(&registry, "A");
            let (out, mut sent, _) = crate::out::Out::channel(16);
            let identity = crate::session::Identity {
                user: Some("A".into()),
                chat_handle: None,
                token: None,
                resume_from: None,
                resumable: false,
            };
            let mut session = crate::session::Session::new(
                registry.clone(),
                Arc::new(crate::state::Metrics::default()),
                identity,
                out,
            );
            let text = json!({"t":"init","d":init()}).to_string();
            assert!(session.handle(&text).await.is_ok());
            let response = tokio::time::timeout(Duration::from_secs(1), sent.recv())
                .await
                .unwrap()
                .unwrap();
            let response: Value = serde_json::from_str(&response).unwrap();
            assert_eq!(response["d"]["op"], "error");
            assert_eq!(
                response["d"]["data"][0],
                "Account signing mode differs; account not attached"
            );
            assert!(account.handle.disposal_cause().is_none());
            registry.dispose(&account, Disposal::PageDisposed);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_deferred_delegated_build_keeps_its_original_user_and_init_generation() {
    use std::future::Future;
    use std::task::Poll;
    let registry = tests::registry();
    let init = init();
    registry.pacer.admit().await;
    let first = registry.obtain_delegated("A", &init, 8, None, Arc::new(|_| true), pair());
    tokio::pin!(first);
    let pending = std::future::poll_fn(|cx| Poll::Ready(first.as_mut().poll(cx))).await;
    assert!(pending.is_pending());
    // Another user with the same name remains independent while A is queued.
    let b = registry
        .begin_init(
            "B",
            &identity::registry_name("B", &init).unwrap(),
            false,
            &crate::delegated::fingerprint(&init),
            None,
        )
        .unwrap();
    let replacement = registry
        .begin_init(
            "A",
            &identity::registry_name("A", &init).unwrap(),
            false,
            &init.fingerprint(),
            None,
        )
        .unwrap();
    tokio::time::advance(Duration::from_secs(1)).await;
    assert!(
        matches!(first.await, Err(ref e) if e == "Account init superseded; initialization stopped")
    );
    assert!(b.authority.check().is_ok());
    assert!(replacement.authority.check().is_ok());
    assert_eq!(registry.account_count(), 0);
}

#[tokio::test]
async fn handed_over_key_is_checked_before_matching_delegated_reuse() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = tests::registry();
            let account = resident_account(&registry, "A");
            let wrong = DelegatedPair::new(
                "other-synthetic-key".into(),
                "synthetic-signing-fixture".into(),
                None,
            );
            let result = registry
                .obtain_delegated("A", &init(), 8, None, Arc::new(|_| true), wrong)
                .await;
            assert!(matches!(result, Err(ref e) if e == "Invalid DELEGATED account setup"));
            assert!(!registry.holds(&account, 8));
            assert!(account.handle.disposal_cause().is_none());
            registry.dispose(&account, Disposal::PageDisposed);
        })
        .await;
}

#[tokio::test]
async fn a_public_reinit_cannot_replace_a_running_delegated_account() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = tests::registry();
            let account = resident_account(&registry, "A");
            let mut public = init();
            public.public_only = Some(true);
            assert!(registry.rebuild_refusal("A", &public, true, 8).is_some());
            let (out, mut sent, _) = crate::out::Out::channel(16);
            let identity = crate::session::Identity {
                user: Some("A".into()),
                chat_handle: None,
                token: None,
                resume_from: None,
                resumable: false,
            };
            let mut session = crate::session::Session::new(
                registry.clone(),
                Arc::new(crate::state::Metrics::default()),
                identity,
                out,
            );
            assert!(
                session
                    .handle(&json!({"t":"init","d":public}).to_string())
                    .await
                    .is_ok()
            );
            let reply = tokio::time::timeout(Duration::from_secs(1), sent.recv())
                .await
                .unwrap()
                .unwrap();
            let reply: Value = serde_json::from_str(&reply).unwrap();
            assert_eq!(reply["d"]["op"], "error");
            assert_eq!(
                reply["d"]["data"][0],
                "Account signing mode differs; account not attached"
            );
            assert!(account.handle.disposal_cause().is_none());
            assert!(
                registry
                    .accounts
                    .lock()
                    .unwrap()
                    .get(&("A".into(), account.registry_name().to_owned()))
                    .is_some_and(|current| Arc::ptr_eq(current, &account))
            );
            registry.dispose(&account, Disposal::PageDisposed);
        })
        .await;
}
