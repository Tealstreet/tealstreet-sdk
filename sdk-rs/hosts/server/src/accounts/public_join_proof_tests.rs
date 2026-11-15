use super::*;
use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::signing::Identities;
use std::{future::Future, task::Poll};

fn candidate(scalar: u8) -> ResidentCopy {
    let mut bytes = vec![0; 40];
    bytes[0] = scalar;
    let (provider, identities) = LocalSigner::from_json_with_policy(
        json!({"primary":{"key":"synthetic-public-account", "secret":safe_cex_crypto::hex(&bytes)}}).to_string(),
        serde_json::from_value(json!({"primary":{"secretEncoding":"hex", "signingAlgorithm":"schnorrDigest"}})).unwrap(),
    ).unwrap();
    ResidentCopy::test_provider_with_identities(Arc::new(provider), identities)
}

struct Fixture {
    registry: Registry,
    account: Arc<Account>,
    init: InitPayload,
    candidate: ResidentCopy,
    gate: Arc<tokio::sync::Semaphore>,
    observed: Arc<Mutex<crate::delegated_e2e_io::Observed>>,
}
fn fixture() -> Fixture {
    let candidate = candidate(7);
    let key = candidate
        .identities()
        .primary
        .as_ref()
        .unwrap()
        .signing_ref()
        .unwrap();
    let public = PublicKey::from_provider(Family::Schnorr, &candidate.provider(), key).unwrap();
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let io = crate::delegated_e2e_io::MockVenue::registered_key(
        json!({"code":200,"api_keys":[{"account_index":42,"api_key_index":1,"public_key":safe_cex_crypto::hex(public.as_bytes())}]}),
        gate.clone(),
    );
    let mut registry = Registry::new(
        EngineThreads::start(1).unwrap(),
        Duration::from_millis(1),
        Duration::from_secs(60),
    );
    let observed = io.observed.clone();
    registry.e2e_venue = Some(io);
    let mut init = InitPayload {
        account_name: "synthetic-lighter".into(),
        exchange_name: "lighter".into(),
        key: "synthetic-public-account".into(),
        application_id: Some("42".into()),
        api_key_index: Some("1".into()),
        signing_mode: Some("cosigned".into()),
        ..Default::default()
    };
    let link = SignerLink::new(
        1,
        Arc::new(|_| panic!("local key proof must not borrow a device signer")),
    );
    let (mut account, _) = testing::account_on(
        testing::HeldLoad {
            placed: Arc::default(),
            load_after_ms: Some(0.),
            ticker: true,
        },
        ("alice", &init.account_name, "lighter"),
        Some(link.clone()),
        init.fingerprint(),
        Arc::new(crate::algos::Running::new(Arc::new(
            tokio::sync::watch::channel(0).0,
        ))),
    );
    let remote = remote_account(
        link.clone(),
        "lighter",
        Some(init.account_name.clone()),
        &engine_options(&init, false).to_string(),
    )
    .unwrap();
    let options: Value = serde_json::from_str(&remote.options_json).unwrap();
    let identities: Identities = serde_json::from_value(options["signing"].clone()).unwrap();
    Arc::get_mut(&mut account).unwrap().signing = AccountSigner::new_bound(
        Some(remote.provider.clone()),
        identities,
        Some(link),
        None,
        remote.public_binding_source(),
    );
    registry
        .accounts
        .lock()
        .unwrap()
        .insert(("alice".into(), init.account_name.clone()), account.clone());
    init.signing_mode = Some("delegated".into());
    Fixture {
        registry,
        account,
        init,
        candidate,
        gate,
        observed,
    }
}
fn permit(f: &Fixture) -> ProbePermit {
    let mut permit = f.registry.probe_permit("alice", &f.init).unwrap();
    permit.token_expires_at = Some(now_ms() + 60_000);
    permit
}

#[tokio::test]
async fn registered_key_proof_uses_the_real_candidate_and_keeps_one_actor() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let f = fixture();
            let permit = permit(&f);
            let owners = Arc::strong_count(&f.account);
            f.gate.add_permits(1);
            let result = f
                .registry
                .prove_public_join(
                    &f.init,
                    &f.candidate,
                    &RoutingPlan::default(),
                    None,
                    &permit,
                )
                .await
                .unwrap()
                .unwrap();
            assert!(result.account.ptr_eq(&Arc::downgrade(&f.account)));
            assert_eq!(
                Arc::strong_count(&f.account),
                owners,
                "proof retains no account owner"
            );
            assert!(f.account.delegated().is_none());
            assert_eq!(f.registry.account_count(), 1);
            assert_eq!(f.observed.lock().unwrap().reads, 1);
            assert!(f.observed.lock().unwrap().writes.is_empty());
            f.account.handle.dispose();
        })
        .await;
}

#[tokio::test]
async fn wrong_registered_key_never_changes_the_resident_or_its_link() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let f = fixture();
            let permit = permit(&f);
            f.gate.add_permits(1);
            let wrong = candidate(8);
            let Err(refusal) = f
                .registry
                .prove_public_join(&f.init, &wrong, &RoutingPlan::default(), None, &permit)
                .await
            else {
                panic!("another Schnorr key must not attach");
            };
            assert!(!refusal.transient);
            assert!(f.account.delegated().is_none());
            assert!(f.account.link.as_ref().unwrap().is_available());
            assert!(f.observed.lock().unwrap().writes.is_empty());
            f.account.handle.dispose();
        })
        .await;
}

#[tokio::test]
async fn retiring_the_init_during_preparation_stays_retired_after_a_return_to_the_same_settings() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for return_to_original in [false, true] {
                let f = fixture();
                let old = permit(&f);
                let plan = RoutingPlan::default();
                let future = f
                    .registry
                    .prove_public_join(&f.init, &f.candidate, &plan, None, &old);
                tokio::pin!(future);
                assert!(
                    std::future::poll_fn(|cx| Poll::Ready(future.as_mut().poll(cx)))
                        .await
                        .is_pending()
                );
                assert_eq!(f.observed.lock().unwrap().preparations, 1);
                old.current.store(false, Ordering::Release);
                if return_to_original {
                    let current = permit(&f);
                    assert!(current.current.load(Ordering::Acquire));
                    assert!(!Arc::ptr_eq(&old.current, &current.current));
                }
                f.gate.add_permits(1);
                let Err(refusal) = future.await else {
                    panic!("retired proof cannot attach after the await");
                };
                assert!(refusal.transient);
                assert_eq!(f.observed.lock().unwrap().requests, 0);
                assert!(f.account.delegated().is_none());
                assert!(f.account.link.as_ref().unwrap().is_available());
                f.account.handle.dispose();
            }
        })
        .await;
}

#[tokio::test]
async fn replacing_the_actor_during_preparation_cannot_transfer_its_key_proof() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let f = fixture();
            let old = permit(&f);
            let plan = RoutingPlan::default();
            let future = f
                .registry
                .prove_public_join(&f.init, &f.candidate, &plan, None, &old);
            tokio::pin!(future);
            assert!(
                std::future::poll_fn(|cx| Poll::Ready(future.as_mut().poll(cx)))
                    .await
                    .is_pending()
            );
            f.registry
                .accounts
                .lock()
                .unwrap()
                .remove(&("alice".into(), f.init.account_name.clone()));
            f.gate.add_permits(1);
            let Err(refusal) = future.await else {
                panic!("removed actor proof cannot create a replacement");
            };
            assert!(refusal.transient);
            assert!(f.account.delegated().is_none());
            assert_eq!(f.registry.account_count(), 0);
            f.account.handle.dispose();
        })
        .await;
}

#[tokio::test]
async fn a_completed_proof_cannot_rebuild_an_actor_removed_before_attachment() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let f = fixture();
            let mut permit = permit(&f);
            f.gate.add_permits(1);
            permit.public_join = f
                .registry
                .prove_public_join(
                    &f.init,
                    &f.candidate,
                    &RoutingPlan::default(),
                    None,
                    &permit,
                )
                .await
                .unwrap();
            let registry = Arc::new(f.registry);
            registry
                .accounts
                .lock()
                .unwrap()
                .remove(&("alice".into(), f.init.account_name.clone()));
            let pair =
                DelegatedPair::new(f.init.key.clone(), format!("07{}", "00".repeat(39)), None);
            let result = registry
                .obtain_with_pair(
                    "alice",
                    &f.init,
                    false,
                    2,
                    Some(("synthetic-token".into(), now_ms() + 60_000)),
                    Arc::new(|_| panic!("retired proof must publish nothing")),
                    Some(pair),
                    Some(permit),
                )
                .await;
            let Err(error) = result else {
                panic!("proof from the removed actor must not build a replacement");
            };
            assert_eq!(error, "Account verification expired");
            assert_eq!(registry.account_count(), 0);
            assert!(f.account.delegated().is_none());
            assert!(f.account.handle.disposal_cause().is_none());
            f.account.handle.dispose();
        })
        .await;
}
