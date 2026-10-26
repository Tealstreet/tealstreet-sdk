#![cfg(feature = "address-nonce")]
//! Producer to release for a `ScalarOnly` primary: trusted `LocalSigner`
//! setup, engine-owned address setup over its refs, the sealed identity's
//! nonce claim, and the managed Prepare/Bind/Claim signed by that provider.
use alloy_primitives::{Address, B256, Signature};
use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::book::Book;
use safe_cex_engine::credential_selection::{CredentialRole, SlotPolicy, Slots};
use safe_cex_engine::nonce::{HYPERLIQUID_AGENT, NonceClaim};
use safe_cex_engine::request_context::plan::{CallAuth, SerialCall, SerialPolicy};
use safe_cex_engine::request_context::{
    Claim, Context, HttpAction, Operation as RequestOperation, PreflightPolicy, PreflightPurpose,
    Prepared, ReconciliationData,
};
use safe_cex_engine::signing::*;
use safe_cex_engine::*;
use std::sync::{Arc, Mutex};

/// Arbitrary in-range scalar; never a vector or real key.
const SCALAR: &str = "1111111111111111111111111111111111111111111111111111111111111111";

#[derive(Default)]
struct Observed {
    /// The payload digest and the provider's [PublicAddress, Sign] values.
    signed: Option<([u8; 32], Vec<Vec<u8>>)>,
    sealed: Option<[u8; 20]>,
}

struct Probe(Arc<Mutex<Observed>>);
impl Venue for Probe {
    type Pending = Context;
    fn address_nonce_domain(&self) -> Option<&'static safe_cex_engine::nonce::NonceDomain> {
        Some(&HYPERLIQUID_AGENT)
    }
    fn public_address_slots(&self) -> AddressSetupSlots {
        AddressSetupSlots::Primary
    }
    fn dispatch_nonce() -> bool {
        true
    }
    fn capture_http_reconciliation(
        &self,
        _: &Context,
    ) -> Result<ReconciliationData, safe_cex_engine::request_context::Error> {
        ReconciliationData::new("hl-cloid-1".into())
    }
    fn http_release() -> HttpRelease<Self> {
        HttpRelease::NonceUnscheduled(|_, _, _| Ok(()))
    }
    fn start(&mut self, cx: &mut Cx<'_, Context>) {
        assert!(cx.observe_protocol_time(9000.75));
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Context>, _: Command) {
        let ids = cx.address_nonce_identities();
        let slots = Slots::new(ids.primary.clone(), None, SlotPolicy::default());
        let (selection, identity, claim) = slots
            .select_with_nonce(CredentialRole::Mutation, &HYPERLIQUID_AGENT)
            .unwrap();
        self.0.lock().unwrap().sealed = Some(identity.verified_signing_address().unwrap());
        let authority = selection.authority(identity);
        let preflight = cx
            .begin_preflight(PreflightPolicy {
                purpose: PreflightPurpose::WriteAfterReads,
                read_credential: None,
                write_credential: Some(authority.clone()),
            })
            .unwrap();
        let (plan, slot) = cx
            .admit_serial_plan(
                &preflight,
                SerialCall {
                    auth: CallAuth::Preflight,
                    operation: RequestOperation::Mutation,
                    timeout_ms: 20_000,
                    attempts: 1,
                },
                SerialPolicy {
                    credential: Some(authority),
                    pacing: None,
                    refusals_complete: false,
                },
            )
            .unwrap();
        let context = cx.plan_leg_context(&plan, &slot).unwrap();
        cx.request_signed_managed(
            context.clone(),
            None,
            Method::Post,
            "https://scalar.test/exchange".into(),
            None,
            Box::new(Recipe {
                claim,
                signing: identity.signing_ref().unwrap().clone(),
                digest: [0; 32],
                observed: self.0.clone(),
            }),
            context,
        )
        .unwrap();
    }
    fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, Context>, _: Context, _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

struct Recipe {
    claim: NonceClaim,
    signing: KeyRef,
    digest: [u8; 32],
    observed: Arc<Mutex<Observed>>,
}
impl SignStep for Recipe {
    fn nonce_claim(&self) -> Option<&NonceClaim> {
        Some(&self.claim)
    }
    fn prepare(
        &mut self,
        action: &mut Action,
        cx: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let nonce = cx
            .reserve_nonce(&self.claim)
            .map_err(|_| SignError::Unavailable)?;
        let Action::Http(packet) = action else {
            return Err(SignError::InvalidRequest);
        };
        packet.body = Some(nonce.to_string());
        self.digest = [0; 32];
        self.digest[24..].copy_from_slice(&nonce.to_be_bytes());
        Ok(SignRequest {
            receipt: SignReceipt {
                nonce: Some(nonce),
                ..Default::default()
            },
            context: Default::default(),
            operations: vec![
                Operation::PublicAddress {
                    key: self.signing.clone(),
                },
                Operation::Sign {
                    algorithm: Algorithm::Secp256k1Digest,
                    key: self.signing.clone(),
                    payload: vec![PayloadPart::Bytes {
                        bytes: self.digest.to_vec(),
                    }],
                },
            ],
        })
    }
    fn apply(&mut self, _: &mut Action, result: &SignResult) -> Result<(), SignError> {
        self.observed.lock().unwrap().signed = Some((self.digest, result.values.clone()));
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        10_000.
    }
}

fn next_sign(e: &mut Engine<Probe>) -> (RequestId, SignRequest) {
    e.drain_actions()
        .find_map(|a| match a {
            Action::Sign { attempt, work, .. } => Some((attempt, work)),
            _ => None,
        })
        .expect("provider work")
}

#[test]
fn trusted_scalar_only_setup_signs_and_releases_through_the_managed_flow() {
    let policy: CredentialSetupPolicy = serde_json::from_str(
        r#"{"primary":{"trimCredential":true,"secretEncoding":"hex",
            "signingAlgorithm":"secp256k1Digest","source":"scalarOnly"}}"#,
    )
    .unwrap();
    let (signer, identities, facts) = LocalSigner::from_json_with_facts(
        serde_json::json!({"primary": {"secret": SCALAR}}).to_string(),
        policy,
        false,
    )
    .unwrap();
    assert_eq!(facts.primary_key(), KeyPresence::Absent);
    assert!(identities.primary.as_ref().unwrap().credential_absent());

    let observed = Arc::new(Mutex::new(Observed::default()));
    let mut e = safe_cex_engine::test_support::seeded_engine(Probe(observed.clone()));
    e.set_identities(identities).unwrap();
    e.set_address_setup_host_support(SetupHostSupport::OwnedPublicAddress1)
        .unwrap();
    // Engine-owned address setup, answered by the same provider's refs.
    e.initialize_address_nonce(100.).unwrap();
    let (attempt, work) = next_sign(&mut e);
    e.handle(
        101.,
        Event::Signed {
            attempt,
            result: signer.sign_now(&work),
        },
    );
    let token = e
        .drain_actions()
        .find_map(|a| match a {
            Action::RetireAddressSetup { token, .. } => Some(token),
            _ => None,
        })
        .expect("setup retirement");
    assert!(e.address_setup_retired(token, 102.));
    e.start(200.);
    e.handle(
        200.5,
        Event::Command(Command::CancelOrders {
            command: 41,
            ids: vec!["o-1".into()],
        }),
    );

    let preparation = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .expect("managed preparation");
    e.http_prepared(
        201.,
        preparation,
        Prepared {
            id: preparation.attempt().request.0,
            expires_at: None,
        },
    );
    let (attempt, work) = next_sign(&mut e);
    assert!(
        !work
            .operations
            .iter()
            .any(|op| matches!(op, Operation::Present { .. }))
    );
    let nonce = work.receipt.nonce.unwrap();
    e.handle(
        202.,
        Event::Signed {
            attempt,
            result: signer.sign_now(&work),
        },
    );
    let permit = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind {
                    permit, request, ..
                } => {
                    assert_eq!(request.body, Some(nonce.to_string()));
                    assert!(request.headers.is_empty());
                    Some(permit)
                }
                _ => None,
            },
            _ => None,
        })
        .expect("bound exchange packet");
    assert!(matches!(
        e.claim_http(202., permit),
        Ok(Claim::Released(release)) if release.signature.nonce == Some(nonce)
    ));

    // The provider's public proof equals the engine-sealed address, and its
    // signature over the released nonce's digest recovers to that address.
    let observed = observed.lock().unwrap();
    let sealed = observed.sealed.unwrap();
    let (digest, values) = observed.signed.clone().unwrap();
    assert_eq!(values[0], sealed.to_vec());
    let signature = Signature::from_raw(&values[1]).unwrap();
    let recovered = signature
        .recover_address_from_prehash(&B256::from(digest))
        .unwrap();
    assert_eq!(recovered, Address::from(sealed));
}
