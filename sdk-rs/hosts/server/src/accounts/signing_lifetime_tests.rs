use super::*;
use safe_cex_engine::signing::{ProtectedProvider, ProtectedSigner};
use std::sync::atomic::{AtomicUsize, Ordering};

struct ValueFreeSigner(Arc<AtomicUsize>);
impl Drop for ValueFreeSigner {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
impl ProtectedSigner for ValueFreeSigner {}
impl Signer for ValueFreeSigner {
    fn immediate(&self) -> bool {
        true
    }
    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        if key.0 == "new/signing" && algorithm == Algorithm::HmacSha256 {
            Capability::Supported
        } else {
            Capability::Unsupported
        }
    }
    fn sign_ready(&self, _: &SignRequest) -> Option<Result<SignResult, SignError>> {
        Some(Err(SignError::Refused))
    }
    fn sign(&self, _: SignRequest) -> SignFuture {
        Box::pin(std::future::ready(Err(SignError::Refused)))
    }
}
fn resident(released: Arc<AtomicUsize>) -> Arc<ResidentCopy> {
    let protection = ProtectedProvider::new(Arc::new(ValueFreeSigner(released))).unwrap();
    let identities = Identities {
        primary: Some(Identity::new(
            KeyRef("new/credential".into()),
            Some(KeyRef("new/signing".into())),
        )),
        secondary: None,
    };
    ResidentCopy::test_protected_provider(protection, identities)
}
fn init() -> InitPayload {
    InitPayload {
        account_name: "acct".into(),
        exchange_name: "bybit".into(),
        key: "synthetic-key".into(),
        ..Default::default()
    }
}
fn account() -> Arc<Account> {
    let (account, _inbox) = testing::account_on(
        testing::HeldLoad {
            placed: Arc::default(),
            load_after_ms: Some(0.),
            ticker: true,
        },
        ("alice", "acct", "bybit"),
        Some(SignerLink::new(0, Arc::new(|_| false))),
        init().fingerprint(),
        Arc::new(crate::algos::Running::new(Arc::new(
            tokio::sync::watch::channel(0).0,
        ))),
    );
    account
}
fn cosigned() -> (Arc<Account>, safe_cex_engine::signing::Provider, KeyRef) {
    let mut account = account();
    let link = account.link.clone();
    let remote = prepare(&init(), false, link.clone())
        .unwrap()
        .remote
        .unwrap();
    let options: Value = serde_json::from_str(&remote.options_json).unwrap();
    let identities: Identities = serde_json::from_value(options["signing"].clone()).unwrap();
    let key = identities
        .primary
        .as_ref()
        .unwrap()
        .signing_ref()
        .unwrap()
        .clone();
    Arc::get_mut(&mut account).unwrap().signing =
        AccountSigner::new(Some(remote.provider), identities, link, None);
    let engine = account.signing.provider();
    (account, engine, key)
}
fn delegated(
    resident: Arc<ResidentCopy>,
) -> (Arc<Account>, safe_cex_engine::signing::Provider, KeyRef) {
    let mut account = account();
    let key = resident
        .identities()
        .primary
        .as_ref()
        .unwrap()
        .signing_ref()
        .unwrap()
        .clone();
    let provider = resident.provider();
    let inner = Arc::get_mut(&mut account).unwrap();
    inner.link = None;
    inner.signing = AccountSigner::new(
        Some(provider.clone()),
        resident.identities().clone(),
        None,
        Some(resident),
    );
    (account, provider, key)
}
fn request(key: KeyRef) -> SignRequest {
    SignRequest {
        receipt: Default::default(),
        context: Default::default(),
        operations: vec![Operation::Sign {
            key,
            algorithm: Algorithm::HmacSha256,
            payload: vec![PayloadPart::Bytes { bytes: vec![] }],
        }],
    }
}

#[tokio::test]
async fn direct_delegated_account_drop_erases_retained_provider_handles() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let released = Arc::new(AtomicUsize::new(0));
            let (account, engine, key) = delegated(resident(released.clone()));
            assert_eq!(
                engine.can_sign(&key, Algorithm::HmacSha256),
                Capability::Supported
            );
            drop(account);
            assert_eq!(released.load(Ordering::SeqCst), 1);
            assert_eq!(
                engine.can_sign(&key, Algorithm::HmacSha256),
                Capability::Unsupported
            );
        })
        .await;
}
#[tokio::test]
async fn upgraded_account_drop_erases_retained_engine_and_provider_handles() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let released = Arc::new(AtomicUsize::new(0));
            let (account, engine, key) = cosigned();
            account.signing.upgrade(resident(released.clone())).unwrap();
            let retained = account.delegated().unwrap().provider();
            assert_eq!(
                engine.can_sign(&key, Algorithm::HmacSha256),
                Capability::Supported
            );
            drop(account);
            assert_eq!(
                released.load(Ordering::SeqCst),
                1,
                "engine handles cannot retain account erasure authority"
            );
            assert_eq!(
                engine.can_sign(&key, Algorithm::HmacSha256),
                Capability::Unsupported
            );
            assert_eq!(
                retained.can_sign(&KeyRef("new/signing".into()), Algorithm::HmacSha256),
                Capability::Unsupported
            );
        })
        .await;
}
#[tokio::test]
async fn unpolled_engine_future_cannot_extend_account_key_authority() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let released = Arc::new(AtomicUsize::new(0));
            let (account, engine, key) = cosigned();
            account.signing.upgrade(resident(released.clone())).unwrap();
            let work = engine.sign(request(key));
            drop(engine);
            drop(account);
            assert_eq!(released.load(Ordering::SeqCst), 1);
            assert_eq!(work.await.unwrap_err(), SignError::Unavailable);
        })
        .await;
}
#[tokio::test]
async fn reconfigure_keeps_the_shared_owner_until_the_last_account_drops() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let released = Arc::new(AtomicUsize::new(0));
            let (original, old_engine, old_key) = cosigned();
            original
                .signing
                .upgrade(resident(released.clone()))
                .unwrap();
            let prepared = original.delegated().unwrap().reconfigure(&init()).unwrap();
            let (replacement, new_engine, new_key) = delegated(prepared.resident);
            assert_eq!(
                old_engine.can_sign(&old_key, Algorithm::HmacSha256),
                Capability::Supported
            );
            drop(original);
            assert_eq!(
                released.load(Ordering::SeqCst),
                0,
                "dropping the old account must preserve its replacement's shared provider"
            );
            assert_eq!(
                new_engine.can_sign(&new_key, Algorithm::HmacSha256),
                Capability::Supported
            );
            drop(replacement);
            assert_eq!(
                released.load(Ordering::SeqCst),
                1,
                "retained engine handles cannot extend the final account owner"
            );
            assert_eq!(
                old_engine.can_sign(&old_key, Algorithm::HmacSha256),
                Capability::Unsupported
            );
            assert_eq!(
                new_engine.can_sign(&new_key, Algorithm::HmacSha256),
                Capability::Unsupported
            );
        })
        .await;
}
