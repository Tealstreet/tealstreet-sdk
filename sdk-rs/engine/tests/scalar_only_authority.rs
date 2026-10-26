#![cfg(all(
    feature = "address-nonce",
    feature = "serial-plans",
    feature = "test-support"
))]
//! A `ScalarOnly` primary's request authority (docs/plans/RUST_CREDENTIAL_ONLY_SETUP.md):
//! signing-only on every path, PublicAddress beside Sign, nothing presented.
use safe_cex_engine::book::Book;
use safe_cex_engine::credential_selection::{CredentialRole, SlotPolicy, Slots};
use safe_cex_engine::nonce::{HYPERLIQUID_AGENT, NonceClaim};
use safe_cex_engine::request_context::plan::{SerialCall, SerialPolicy};
use safe_cex_engine::request_context::{
    Claim, Context, Credential, HttpAction, Operation as RequestOperation, Policy, PreflightPolicy,
    PreflightPurpose, Prepared, ReconciliationData, RetryPolicy, Scope,
};
use safe_cex_engine::signing::*;
use safe_cex_engine::*;

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    /// PublicAddress + Secp256k1Digest under the genuine address claim.
    Genuine,
    /// The claimed recipe also presents the signing ref.
    ClaimedPresent,
    /// A non-nonce signed request presents the signing ref.
    PlainPresent,
    /// A non-nonce signed request interpolates the signing ref.
    PlainInterpolate,
}

struct Probe {
    mode: Mode,
    empty_credential_admitted: Option<bool>,
}
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
        // The absent credential itself never forms a request authority.
        let identity = cx.address_nonce_identities().primary.clone().unwrap();
        let empty = Credential {
            slot: CredentialSlot::Primary,
            reference: identity.credential.clone(),
        };
        self.empty_credential_admitted =
            Some(cx.http_context(Scope::Background, read(empty)).is_ok());
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Context>, _: Command) {
        let ids = cx.address_nonce_identities();
        let slots = Slots::new(ids.primary.clone(), None, SlotPolicy::default());
        let (selection, identity, claim) = slots
            .select_with_nonce(CredentialRole::Mutation, &HYPERLIQUID_AGENT)
            .unwrap();
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
        let claim = matches!(self.mode, Mode::Genuine | Mode::ClaimedPresent).then_some(claim);
        cx.request_signed_managed(
            context.clone(),
            None,
            Method::Post,
            "https://scalar.test/exchange".into(),
            None,
            Box::new(Recipe {
                mode: self.mode,
                claim,
                signing: identity.signing_ref().unwrap().clone(),
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

fn read(credential: Credential) -> Policy {
    Policy {
        operation: RequestOperation::Read,
        credential: Some(credential),
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}

struct Recipe {
    mode: Mode,
    claim: Option<NonceClaim>,
    signing: KeyRef,
}
impl SignStep for Recipe {
    fn nonce_claim(&self) -> Option<&NonceClaim> {
        self.claim.as_ref()
    }
    fn prepare(
        &mut self,
        action: &mut Action,
        cx: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let nonce = match &self.claim {
            Some(claim) => Some(
                cx.reserve_nonce(claim)
                    .map_err(|_| SignError::Unavailable)?,
            ),
            None => None,
        };
        let Action::Http(packet) = action else {
            return Err(SignError::InvalidRequest);
        };
        packet.body = nonce.map(|nonce| nonce.to_string());
        let key = self.signing.clone();
        let mut payload = vec![PayloadPart::Bytes {
            bytes: [vec![0; 24], nonce.unwrap_or(7).to_be_bytes().to_vec()].concat(),
        }];
        if self.mode == Mode::PlainInterpolate {
            payload.push(PayloadPart::Credential { key: key.clone() });
        }
        let mut operations = vec![
            Operation::PublicAddress { key: key.clone() },
            Operation::Sign {
                algorithm: Algorithm::Secp256k1Digest,
                key: key.clone(),
                payload,
            },
        ];
        if matches!(self.mode, Mode::ClaimedPresent | Mode::PlainPresent) {
            operations.push(Operation::Present { key });
        }
        Ok(SignRequest {
            receipt: SignReceipt {
                nonce,
                ..Default::default()
            },
            context: Default::default(),
            operations,
        })
    }
    fn apply(&mut self, _: &mut Action, _: &SignResult) -> Result<(), SignError> {
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        10_000.
    }
}

fn scalar_only_policy() -> CredentialSetupPolicy {
    CredentialSetupPolicy {
        primary: Some(IdentitySetupPolicy {
            signing_algorithm: Some(Algorithm::Secp256k1Digest),
            source: PrimarySource::ScalarOnly,
            ..Default::default()
        }),
        secondary: None,
    }
}

/// What trusted local setup installs for a scalar-only primary.
fn absent_identities() -> Identities {
    let mut ids = Identities {
        primary: Some(Identity::new(
            KeyRef(String::new()),
            Some(KeyRef("signing".into())),
        )),
        secondary: None,
    };
    scalar_only_policy()
        .install_scalar_only_primary(&mut ids)
        .unwrap();
    assert!(ids.primary.as_ref().unwrap().credential_absent());
    ids
}

/// Engine-owned address setup over the sealed identities, then `start`.
fn started(mode: Mode, identities: Identities) -> Engine<Probe> {
    let mut e = safe_cex_engine::test_support::seeded_engine(Probe {
        mode,
        empty_credential_admitted: None,
    });
    e.set_identities(identities).unwrap();
    e.set_address_setup_host_support(SetupHostSupport::OwnedPublicAddress1)
        .unwrap();
    e.initialize_address_nonce(100.).unwrap();
    let (attempt, work) = e
        .drain_actions()
        .find_map(|a| match a {
            Action::Sign { attempt, work, .. } => Some((attempt, work)),
            _ => None,
        })
        .expect("engine-owned address setup");
    assert!(
        matches!(work.operations.as_slice(), [Operation::PublicAddress { key }] if key.0 == "signing")
    );
    e.handle(
        101.,
        Event::Signed {
            attempt,
            result: Ok(SignResult::new(vec![vec![19; 20]], work.receipt)),
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
    e
}

/// Prepares the one managed request; returns the provider work, if any.
fn prepared_work(e: &mut Engine<Probe>) -> Option<(RequestId, SignRequest)> {
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
    e.drain_actions().find_map(|a| match a {
        Action::Sign { attempt, work, .. } => Some((attempt, work)),
        _ => None,
    })
}

#[test]
fn scalar_only_primary_signs_publicaddress_and_digest_through_the_genuine_managed_flow() {
    for absent in [false, true] {
        genuine_flow(absent);
    }
}

fn genuine_flow(absent: bool) {
    let identities = if absent {
        absent_identities()
    } else {
        Identities {
            primary: Some(Identity::new(
                KeyRef("signing-control".into()),
                Some(KeyRef("signing".into())),
            )),
            secondary: None,
        }
    };
    let mut e = started(Mode::Genuine, identities);
    assert_eq!(e.venue().empty_credential_admitted, Some(!absent));
    let (attempt, work) = prepared_work(&mut e).expect("real provider work");
    let nonce = work.receipt.nonce.unwrap();
    assert!(matches!(
        work.operations.as_slice(),
        [Operation::PublicAddress { key: proof }, Operation::Sign { algorithm: Algorithm::Secp256k1Digest, key, payload }]
            if proof.0 == "signing" && key.0 == "signing"
                && !payload.iter().any(|part| matches!(part, PayloadPart::Credential { .. }))
    ));
    e.handle(
        202.,
        Event::Signed {
            attempt,
            result: Ok(SignResult::new(
                vec![vec![19; 20], vec![0; 65]],
                work.receipt,
            )),
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
    let claim = e.claim_http(202., permit);
    assert!(
        matches!(claim, Ok(Claim::Released(_))),
        "absent={absent} {claim:?}"
    );
    assert!(e.claim_http(202., permit).is_err());
}

#[test]
fn the_signing_only_ref_is_never_presented_or_interpolated_on_any_path() {
    for mode in [
        Mode::ClaimedPresent,
        Mode::PlainPresent,
        Mode::PlainInterpolate,
    ] {
        let mut e = started(mode, absent_identities());
        assert!(
            prepared_work(&mut e).is_none(),
            "provider work for a presenting recipe"
        );
    }
    // Control: the engine passes the same plain recipes for an ordinary
    // identity (the provider then refuses a declared asymmetric ref itself).
    for mode in [Mode::PlainPresent, Mode::PlainInterpolate] {
        let ordinary = Identities {
            primary: Some(Identity::new(
                KeyRef("signing-control".into()),
                Some(KeyRef("signing".into())),
            )),
            secondary: None,
        };
        let mut e = started(mode, ordinary);
        assert!(prepared_work(&mut e).is_some(), "ordinary control");
    }
}

#[test]
fn injected_absence_never_selects_a_claim() {
    let injected = Identity::new(KeyRef(String::new()), Some(KeyRef("signing".into())));
    assert!(!injected.credential_absent());
    assert!(
        Slots::new(Some(injected), None, SlotPolicy::default())
            .select_with_nonce(CredentialRole::Mutation, &HYPERLIQUID_AGENT)
            .is_err()
    );
}

#[test]
fn ordinary_identities_keep_their_exact_credential_authority() {
    let ordinary = Identity::new(
        KeyRef("primary.credential".into()),
        Some(KeyRef("primary.signing".into())),
    );
    assert_eq!(ordinary.authority_ref(), &ordinary.credential);
    let slots = Slots::new(Some(ordinary.clone()), None, SlotPolicy::default());
    let (selection, identity) = slots.select(CredentialRole::Mutation).unwrap();
    assert_eq!(
        selection.authority(identity),
        selection.credential(ordinary.credential.clone())
    );
    // An ordinary identity reusing one ref for both roles is unchanged.
    let shared = Identity::new(KeyRef("same".into()), Some(KeyRef("same".into())));
    assert_eq!(shared.authority_ref(), &KeyRef("same".into()));
    assert!(
        Identities {
            primary: Some(shared),
            secondary: None
        }
        .signing_only_ref()
        .is_none()
    );
}

#[test]
fn a_scalar_only_signing_ref_may_not_collide_with_another_slot() {
    for other in [
        Identity::new(
            KeyRef("signing".into()),
            Some(KeyRef("read.signing".into())),
        ),
        Identity::new(KeyRef("read".into()), Some(KeyRef("signing".into()))),
    ] {
        let mut ids = Identities {
            primary: Some(Identity::new(
                KeyRef(String::new()),
                Some(KeyRef("signing".into())),
            )),
            secondary: Some(other),
        };
        assert_eq!(
            scalar_only_policy().install_scalar_only_primary(&mut ids),
            Err(SignError::InvalidRequest)
        );
        assert!(!ids.primary.unwrap().credential_absent());
    }
}
