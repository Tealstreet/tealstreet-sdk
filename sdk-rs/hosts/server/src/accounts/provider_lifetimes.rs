use super::*;
use safe_cex_crypto::{
    admission::{Family, PublicKey, Statement},
    signer::LocalSigner,
};
use safe_cex_engine::signing::Provider;

fn provider(family: Family, prefix: &str, scalar: u8) -> (Provider, Identities) {
    let mut material = vec![0; if family == Family::Schnorr { 40 } else { 32 }];
    if family == Family::Schnorr {
        material[0] = scalar;
    } else {
        material[31] = scalar;
    }
    let mut local = LocalSigner::new();
    let credential = KeyRef(format!("{prefix}/credential"));
    let signing = KeyRef(format!("{prefix}/signing"));
    local.insert(credential.clone(), b"synthetic-public-identity".to_vec());
    local
        .insert_declared(signing.clone(), material, family.algorithm())
        .unwrap();
    (
        Arc::new(local),
        Identities {
            primary: Some(Identity::new(credential, Some(signing))),
            secondary: None,
        },
    )
}

fn handover(family: Family) {
    let (original, old_ids) = provider(family, "original", 7);
    let old_key = old_ids
        .primary
        .as_ref()
        .unwrap()
        .signing_ref()
        .unwrap()
        .clone();
    let expected = PublicKey::from_provider(family, &original, &old_key).unwrap();
    let (local, new_ids) = provider(family, "resident", 7);
    let new_key = new_ids
        .primary
        .as_ref()
        .unwrap()
        .signing_ref()
        .unwrap()
        .clone();
    assert!(expected.matches_provider(&local, &new_key).unwrap());
    let link = SignerLink::new(
        1,
        Arc::new(|_| panic!("upgrade must not ask the device to sign")),
    );
    let signer = AccountSigner::new(Some(original.clone()), old_ids, Some(link.clone()), None);
    let front = signer.provider();
    let statement = Statement::new("provider-lifetime", [2; 32], [3; 32], 1000).unwrap();
    let request = statement.sign_request(family, &old_key, &[9; 32]).unwrap();
    let before = original.sign_ready(&request).unwrap().unwrap();
    assert_eq!(
        signer.upgrade(ResidentCopy::test_provider_with_identities(local, new_ids)),
        Ok(true)
    );
    assert!(
        front.incarnation().is_none(),
        "a wrapper must never counterfeit a provider seal"
    );
    assert!(!link.is_available());
    assert_eq!(
        front.can_sign(&old_key, family.algorithm()),
        Capability::Supported
    );
    let after = front.sign_ready(&request).unwrap().unwrap();
    assert_eq!(after.receipt, before.receipt);
    assert_eq!(after.values, before.values);
    assert_eq!(request.operations.len(), 1);
    assert!(matches!(&request.operations[0], Operation::Sign { key, .. } if key == &old_key));
}

#[test]
fn ethereum_provider_lifetime_survives_handover() {
    handover(Family::Ethereum);
}
#[test]
fn stark_provider_lifetime_survives_handover() {
    handover(Family::Stark);
}
#[test]
fn schnorr_provider_lifetime_survives_handover() {
    handover(Family::Schnorr);
}
#[test]
fn ed25519_provider_lifetime_survives_handover() {
    handover(Family::Ed25519);
}

#[test]
fn another_public_key_never_retires_the_original_provider() {
    for family in [
        Family::Ethereum,
        Family::Stark,
        Family::Schnorr,
        Family::Ed25519,
    ] {
        let (original, ids) = provider(family, "original", 7);
        let old_key = ids.primary.as_ref().unwrap().signing_ref().unwrap().clone();
        let (wrong, wrong_ids) = provider(family, "candidate", 8);
        let link = SignerLink::new(1, Arc::new(|_| false));
        let signer = AccountSigner::new(Some(original), ids, Some(link.clone()), None);
        assert_eq!(
            signer.upgrade(ResidentCopy::test_provider_with_identities(
                wrong, wrong_ids
            )),
            Err(ModeTransitionRefusal::Unavailable)
        );
        assert!(link.is_available());
        assert!(signer.resident().is_none());
        assert_eq!(
            signer.provider().can_sign(&old_key, family.algorithm()),
            Capability::Supported
        );
    }
}

#[test]
fn unpolled_public_key_work_blocks_handover_without_cancellation() {
    for family in [
        Family::Ethereum,
        Family::Stark,
        Family::Schnorr,
        Family::Ed25519,
    ] {
        let (original, ids) = provider(family, "original", 7);
        let old_key = ids.primary.as_ref().unwrap().signing_ref().unwrap().clone();
        let (local, new_ids) = provider(family, "resident", 7);
        let link = SignerLink::new(1, Arc::new(|_| false));
        let signer = AccountSigner::new(Some(original), ids, Some(link.clone()), None);
        let request = Statement::new("busy", [2; 32], [3; 32], 1000)
            .unwrap()
            .sign_request(family, &old_key, &[9; 32])
            .unwrap();
        let work = signer.provider().sign(request);
        assert_eq!(
            signer.upgrade(ResidentCopy::test_provider_with_identities(local, new_ids)),
            Err(ModeTransitionRefusal::Busy)
        );
        assert!(link.is_available());
        drop(work);
    }
}

struct SetupProvider {
    provider: Provider,
    joined: Arc<AtomicBool>,
    cancellations: Arc<std::sync::atomic::AtomicUsize>,
}
impl Signer for SetupProvider {
    fn immediate(&self) -> bool {
        self.provider.immediate()
    }
    fn sign(&self, request: SignRequest) -> SignFuture {
        self.provider.sign(request)
    }
    fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        self.provider.sign_ready(request)
    }
    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        self.provider.can_sign(key, algorithm)
    }
    fn owned_public_address_capability(&self) -> Capability {
        Capability::Supported
    }
    fn create_public_address_setup(
        &self,
        request: SignRequest,
    ) -> Option<Box<dyn OwnedSetupExecution>> {
        Some(Box::new(JoiningSetup {
            result: self.provider.sign_ready(&request),
            joined: self.joined.clone(),
            cancellations: self.cancellations.clone(),
        }))
    }
}
struct JoiningSetup {
    result: Option<Result<SignResult, SignError>>,
    joined: Arc<AtomicBool>,
    cancellations: Arc<std::sync::atomic::AtomicUsize>,
}
impl OwnedSetupExecution for JoiningSetup {
    fn start(&mut self) -> Result<(), SignError> {
        Ok(())
    }
    fn cancel(&mut self) -> Result<(), SignError> {
        self.cancellations.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn poll_result(&mut self, _: &mut Context<'_>) -> Poll<Result<SignResult, SignError>> {
        self.result.take().map_or(Poll::Pending, Poll::Ready)
    }
    fn poll_joined(&mut self, _: &mut Context<'_>) -> Poll<Result<(), SignError>> {
        if self.joined.load(Ordering::SeqCst) {
            Poll::Ready(Ok(()))
        } else {
            Poll::Pending
        }
    }
}

#[tokio::test]
async fn completed_address_result_stays_busy_until_owned_work_actually_joins() {
    let (original, ids) = provider(Family::Ethereum, "original", 7);
    let key = ids.primary.as_ref().unwrap().signing_ref().unwrap().clone();
    let joined = Arc::new(AtomicBool::new(false));
    let cancellations = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let original: Provider = Arc::new(SetupProvider {
        provider: original,
        joined: joined.clone(),
        cancellations: cancellations.clone(),
    });
    let signer = AccountSigner::new(
        Some(original),
        ids,
        Some(SignerLink::new(1, Arc::new(|_| false))),
        None,
    );
    let mut setup = signer
        .provider()
        .create_public_address_setup(SignRequest {
            receipt: Default::default(),
            context: Default::default(),
            operations: vec![Operation::PublicAddress { key }],
        })
        .unwrap();
    setup.start().unwrap();
    let (local, ids) = provider(Family::Ethereum, "resident", 7);
    assert_eq!(
        signer.upgrade(ResidentCopy::test_provider_with_identities(
            local.clone(),
            ids.clone()
        )),
        Err(ModeTransitionRefusal::Busy)
    );
    std::future::poll_fn(|cx| setup.poll_result(cx))
        .await
        .unwrap();
    assert_eq!(
        signer.upgrade(ResidentCopy::test_provider_with_identities(
            local.clone(),
            ids.clone()
        )),
        Err(ModeTransitionRefusal::Busy)
    );
    assert!(
        std::future::poll_fn(|cx| Poll::Ready(setup.poll_joined(cx)))
            .await
            .is_pending()
    );
    joined.store(true, Ordering::SeqCst);
    std::future::poll_fn(|cx| setup.poll_joined(cx))
        .await
        .unwrap();
    assert_eq!(
        signer.upgrade(ResidentCopy::test_provider_with_identities(local, ids)),
        Ok(true)
    );
    assert_eq!(cancellations.load(Ordering::SeqCst), 0);
}

#[test]
fn hmac_control_keeps_its_existing_handover_and_exact_bytes() {
    fn hmac(prefix: &str) -> (Provider, Identities) {
        let mut provider = LocalSigner::new();
        let credential = KeyRef(format!("{prefix}/credential"));
        let signing = KeyRef(format!("{prefix}/signing"));
        provider.insert(credential.clone(), b"synthetic-public-identity".to_vec());
        provider.insert(signing.clone(), vec![7; 32]);
        (
            Arc::new(provider),
            Identities {
                primary: Some(Identity::new(credential, Some(signing))),
                secondary: None,
            },
        )
    }
    let (original, ids) = hmac("original");
    let key = ids.primary.as_ref().unwrap().signing_ref().unwrap().clone();
    let request = SignRequest {
        receipt: Default::default(),
        context: Default::default(),
        operations: vec![Operation::Sign {
            key,
            algorithm: Algorithm::HmacSha256,
            payload: vec![PayloadPart::Bytes {
                bytes: b"original/signing literal, not a ref".to_vec(),
            }],
        }],
    };
    let expected = original.sign_ready(&request).unwrap().unwrap();
    let signer = AccountSigner::new(
        Some(original),
        ids,
        Some(SignerLink::new(1, Arc::new(|_| false))),
        None,
    );
    let (local, ids) = hmac("resident");
    assert_eq!(
        signer.upgrade(ResidentCopy::test_provider_with_identities(local, ids)),
        Ok(true)
    );
    let actual = signer.provider().sign_ready(&request).unwrap().unwrap();
    assert_eq!(actual.values, expected.values);
    assert_eq!(actual.receipt, expected.receipt);
}

async fn remote_handover(venue: &str, family: Family) {
    use base64::Engine as _;
    let (local, new_ids) = provider(family, "resident", 7);
    let new_key = new_ids
        .primary
        .as_ref()
        .unwrap()
        .signing_ref()
        .unwrap()
        .clone();
    let public = PublicKey::from_provider(family, &local, &new_key).unwrap();
    let public_bytes = public.as_bytes().to_vec();
    let public_text = if family == Family::Ed25519 {
        base64::engine::general_purpose::STANDARD.encode(&public_bytes)
    } else {
        format!("0x{}", safe_cex_crypto::hex(&public_bytes))
    };
    let (send, mut receive) = mpsc::unbounded_channel::<String>();
    let link = SignerLink::new(1, Arc::new(move |frame| send.send(frame).is_ok()));
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let (weak, count, answer) = (Arc::downgrade(&link), calls.clone(), public_text.clone());
    tokio::spawn(async move {
        while let Some(frame) = receive.recv().await {
            let request: Value = serde_json::from_str(&frame).unwrap();
            let format = request["d"]["wsAuth"]["outputFormat"].as_str().unwrap();
            assert!(matches!(format, "hyperliquid-address" | "paradex-public"));
            count.fetch_add(1, Ordering::SeqCst);
            if let Some(link) = weak.upgrade() {
                link.resolve(
                    1,
                    json!({"id":request["d"]["id"], "wsAuthSignature":answer}),
                );
            }
        }
    });
    let metadata = json!({"key":if venue == "backpack" { public_text.clone() } else { "synthetic-public-account".into() }, "grvtSignerAddress":public_text});
    let remote = safe_cex_remote_signer::remote_account(
        link.clone(),
        venue,
        Some("synthetic-account".into()),
        &metadata.to_string(),
    )
    .unwrap();
    let options: Value = serde_json::from_str(&remote.options_json).unwrap();
    let ids: Identities = serde_json::from_value(options["signing"].clone()).unwrap();
    let key = ids.primary.as_ref().unwrap().signing_ref().unwrap().clone();
    let signer = AccountSigner::new_bound(
        Some(remote.provider.clone()),
        ids,
        Some(link.clone()),
        None,
        remote.public_binding_source(),
    );
    if matches!(venue, "hyperliquid" | "paradex") {
        let operation = if family == Family::Ethereum {
            Operation::PublicAddress { key: key.clone() }
        } else {
            Operation::PublicKey {
                key: key.clone(),
                algorithm: family.algorithm(),
            }
        };
        let mut setup = signer
            .provider()
            .create_public_address_setup(SignRequest {
                receipt: Default::default(),
                context: Default::default(),
                operations: vec![operation],
            })
            .unwrap();
        setup.start().unwrap();
        assert_eq!(
            signer.upgrade(ResidentCopy::test_provider_with_identities(
                local.clone(),
                new_ids.clone()
            )),
            Err(ModeTransitionRefusal::Busy)
        );
        let result = std::future::poll_fn(|cx| setup.poll_result(cx))
            .await
            .unwrap();
        assert_eq!(result.values, vec![public_bytes]);
        std::future::poll_fn(|cx| setup.poll_joined(cx))
            .await
            .unwrap();
    }
    let (wrong, wrong_ids) = provider(family, "wrong-candidate", 8);
    assert_eq!(
        signer.upgrade(ResidentCopy::test_provider_with_identities(
            wrong, wrong_ids
        )),
        Err(ModeTransitionRefusal::Unavailable)
    );
    assert!(link.is_available());
    assert!(signer.resident().is_none());
    assert_eq!(
        signer.upgrade(ResidentCopy::test_provider_with_identities(local, new_ids)),
        Ok(true)
    );
    let before = calls.load(Ordering::SeqCst);
    let statement = Statement::new("remote-lifetime", [2; 32], [3; 32], 1000).unwrap();
    let request = statement.sign_request(family, &key, &[9; 32]).unwrap();
    let signed = signer.provider().sign_ready(&request).unwrap().unwrap();
    assert!(public.verify(&statement, &signed.values[0]));
    assert_eq!(calls.load(Ordering::SeqCst), before);
    assert!(!link.is_available());
}

#[tokio::test]
async fn completed_hyperliquid_remote_address_survives_handover() {
    remote_handover("hyperliquid", Family::Ethereum).await;
}
#[tokio::test]
async fn completed_paradex_remote_key_survives_handover() {
    remote_handover("paradex", Family::Stark).await;
}
#[tokio::test]
async fn grvt_remote_identity_survives_handover() {
    remote_handover("grvt", Family::Ethereum).await;
}
#[tokio::test]
async fn backpack_remote_identity_survives_handover() {
    remote_handover("backpack", Family::Ed25519).await;
}
