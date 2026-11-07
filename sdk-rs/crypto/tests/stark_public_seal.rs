#![cfg(feature = "stark")]
//! A Stark public key sealed by trusted local setup reads only under the exact
//! provider incarnation that sealed it, whatever the installation order.
use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::book::Book;
use safe_cex_engine::signing::{
    Algorithm, CredentialSetupPolicy, Identities, Provider, SignError, SignFuture, SignRequest,
    Signer,
};
use safe_cex_engine::{Command, CredentialSlot, Cx, Engine, Response, SocketId, TimerId, Venue};
use std::sync::Arc;

const SCALAR: &str = "000000000000000000000000000000000000000000000000000000000000002a";

struct Quiet;
impl Venue for Quiet {
    type Pending = ();
    fn start(&mut self, _: &mut Cx<'_, ()>) {}
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn policy() -> CredentialSetupPolicy {
    serde_json::from_str(
        r#"{"primary":{"secretEncoding":"hex","signingAlgorithm":"starkDigest","source":"scalarOnly"}}"#,
    )
    .unwrap()
}

fn setup(scalar: &str) -> (Provider, Identities) {
    let (signer, identities) = LocalSigner::from_json_with_policy(
        serde_json::json!({"primary": {"secret": scalar}}).to_string(),
        policy(),
    )
    .unwrap();
    (Arc::new(signer), identities)
}

fn engine(signer: Option<Provider>, identities: Identities, signer_first: bool) -> Engine<Quiet> {
    let mut e = safe_cex_engine::test_support::seeded_engine(Quiet);
    if signer_first {
        e.set_signer(signer).unwrap();
        e.set_identities(identities).unwrap();
    } else {
        e.set_identities(identities).unwrap();
        e.set_signer(signer).unwrap();
    }
    e
}

fn read(e: &Engine<Quiet>) -> Result<[u8; 32], SignError> {
    e.verified_public_key(CredentialSlot::Primary, Algorithm::StarkDigest)
}

fn expected(scalar: &str) -> [u8; 32] {
    let key = starknet_crypto::Felt::from_hex(scalar).unwrap();
    safe_cex_crypto::starknet::public_key(&key).to_bytes_be()
}

#[test]
fn the_sealing_provider_reads_its_key_in_either_installation_order() {
    for signer_first in [true, false] {
        let (a, ids) = setup(SCALAR);
        assert_eq!(
            read(&engine(Some(a), ids, signer_first)),
            Ok(expected(SCALAR))
        );
    }
}

#[test]
fn another_provider_under_the_same_ref_text_never_reads_the_seal() {
    let (_, sealed) = setup(SCALAR);
    // Identical material and refs, freshly built: a different incarnation.
    let (rebuilt, _) = setup(SCALAR);
    let (other, _) = setup(&format!("{}2b", "0".repeat(62)));
    for provider in [rebuilt, other] {
        for signer_first in [true, false] {
            assert_eq!(
                read(&engine(
                    Some(provider.clone()),
                    sealed.clone(),
                    signer_first
                )),
                Err(SignError::Unsupported)
            );
        }
    }
    assert_eq!(
        read(&engine(None, sealed, true)),
        Err(SignError::Unsupported)
    );
}

/// An external provider able to produce the sealing key's signatures.
struct Forwarding(Provider);
impl Signer for Forwarding {
    fn sign(&self, request: SignRequest) -> SignFuture {
        self.0.sign(request)
    }
}

#[test]
fn an_external_provider_and_a_replaced_provider_refuse_but_the_same_arc_restores() {
    let (a, sealed) = setup(SCALAR);
    assert_eq!(
        read(&engine(
            Some(Arc::new(Forwarding(a.clone()))),
            sealed.clone(),
            true
        )),
        Err(SignError::Unsupported)
    );
    let (b, _) = setup(SCALAR);
    let mut e = engine(Some(a.clone()), sealed, true);
    e.set_signer(Some(b)).unwrap();
    assert_eq!(read(&e), Err(SignError::Unsupported));
    // Restoring the unchanged provider is the same incarnation.
    e.set_signer(Some(a)).unwrap();
    assert_eq!(read(&e), Ok(expected(SCALAR)));
}

#[test]
fn a_transported_identity_or_mutated_provider_confers_nothing() {
    let (a, sealed) = setup(SCALAR);
    let transported: Identities =
        serde_json::from_str(&serde_json::to_string(&sealed).unwrap()).unwrap();
    assert_eq!(
        read(&engine(Some(a), transported, true)),
        Err(SignError::Unsupported)
    );
    let (signer, sealed) = LocalSigner::from_json_with_policy(
        serde_json::json!({"primary": {"secret": SCALAR}}).to_string(),
        policy(),
    )
    .unwrap();
    let mut signer = signer;
    let signing = sealed.primary.as_ref().unwrap().authority_ref().clone();
    signer.insert(signing, hex(SCALAR));
    assert_eq!(
        read(&engine(Some(Arc::new(signer)), sealed, true)),
        Err(SignError::Unsupported)
    );
}

#[test]
fn a_key_with_no_canonical_public_form_refuses_setup() {
    for scalar in [
        "0".repeat(64),
        "0800000000000010ffffffffffffffffb781126dcae7b2321e66a241adc64d2f".into(),
    ] {
        assert!(
            LocalSigner::from_json_with_policy(
                serde_json::json!({"primary": {"secret": scalar}}).to_string(),
                policy(),
            )
            .is_err()
        );
    }
}

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}
