//! Credential-only setup custody (docs/plans/RUST_CREDENTIAL_ONLY_SETUP.md C4, C4p):
//! a signing call whose ref is absent refuses in preparation, before any provider
//! work, and releases everything it held.
#![cfg(all(feature = "test-support", feature = "http-plans"))]
use safe_cex_engine::book::Book;
use safe_cex_engine::classify::WriteFact;
use safe_cex_engine::request_context::{
    Context, Credential, Error, HttpOwnerRetirement, Operation, Plan, PlanPolicy, Policy,
    PreflightPolicy, PreflightPurpose, ReconciliationData, RetryPolicy, Scope,
    plan::{CallBound, RetryWindow, Slot, StepBound},
};
use safe_cex_engine::signing::{
    Identities, Identity, KeyRef, Operation as SignOperation, SignError, SignFuture, SignReceipt,
    SignRequest, SignResult, Signer,
};
use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{
    Action, Command, CredentialSlot, Cx, DispatchContext, Engine, ErrorKind, Event, HttpRequest,
    LogEvent, Method, Notice, Outcome, Response, SignStep, SocketId, TimerId, Venue, VenueError,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Takes its key through `Identity::signing_ref()`: an absent ref refuses in
/// preparation, before any provider work.
struct Recipe(Identity);
impl SignStep for Recipe {
    fn freshness_ms(&self) -> f64 {
        3_000.
    }
    fn prepare(
        &mut self,
        _: &mut Action,
        cx: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let key = self.0.signing_ref()?.clone();
        Ok(SignRequest {
            receipt: SignReceipt {
                stamp_ms: Some(cx.stamp_ms()),
                expiry_ms: Some(cx.stamp_ms() + 3_000.),
                ..Default::default()
            },
            context: Default::default(),
            operations: vec![SignOperation::Sign {
                algorithm: safe_cex_engine::signing::Algorithm::HmacSha256,
                key,
                payload: vec![safe_cex_engine::signing::PayloadPart::Bytes {
                    bytes: b"preimage".to_vec(),
                }],
            }],
        })
    }
    fn apply(&mut self, _: &mut Action, _: &SignResult) -> Result<(), SignError> {
        Ok(())
    }
}

fn credential() -> Credential {
    Credential {
        slot: CredentialSlot::Primary,
        reference: KeyRef("primary.credential".into()),
    }
}
fn signing(signed: bool) -> Identity {
    Identity::new(
        KeyRef("primary.credential".into()),
        signed.then(|| KeyRef("primary.signing".into())),
    )
}

#[derive(Default)]
struct Probe {
    /// Commands whose single signed Mutation has a signing ref.
    signed: Vec<u32>,
    /// C4p: a two-step Mutation Plan whose second call has no signing ref.
    plan: Option<(Plan, Slot)>,
    plan_mode: bool,
    failures: Vec<(Error, VenueError)>,
    /// C11: the installed identity a recipe without a signing ref carries
    /// (an Optional-flagged slot); defaults to an unflagged absent ref.
    absent: Option<Identity>,
    /// C11: commands whose single Mutation is unsigned (no credential).
    unsigned: Vec<u32>,
}
impl Probe {
    fn identity(&self, signed: bool) -> Identity {
        match (signed, &self.absent) {
            (false, Some(absent)) => absent.clone(),
            _ => signing(signed),
        }
    }
    fn plan_call(&mut self, cx: &mut Cx<'_, Context>, signed: bool) {
        let plan = self.plan.as_ref().unwrap().0.clone();
        let slot = cx.plan_slots(&plan).unwrap().remove(0);
        let context = cx.plan_leg_context(&plan, &slot).unwrap();
        cx.request_signed_managed(
            context.clone(),
            None,
            Method::Post,
            "https://venue.test/write".into(),
            None,
            Box::new(Recipe(self.identity(signed))),
            context,
        )
        .map_err(|(_, error)| error)
        .unwrap();
        self.plan = Some((plan, slot));
    }
}
impl Venue for Probe {
    type Pending = Context;
    fn start(&mut self, _: &mut Cx<'_, Context>) {}
    fn on_command(&mut self, cx: &mut Cx<'_, Context>, command: Command) {
        let Command::AmendOrder { command, .. } = command else {
            panic!("fixture issues amends only")
        };
        if self.plan_mode {
            let preflight = cx
                .begin_preflight(PreflightPolicy {
                    purpose: PreflightPurpose::WriteAfterReads,
                    read_credential: None,
                    write_credential: Some(credential()),
                })
                .unwrap();
            let calls = [CallBound {
                timeout_ms: 20_000,
                attempts: 1,
                retry_window: RetryWindow::Shared,
                after_refusal: &[],
            }];
            let steps = [
                StepBound {
                    wait_ms: 0,
                    calls: &calls,
                },
                StepBound {
                    wait_ms: 0,
                    calls: &calls,
                },
            ];
            let plan = cx
                .admit_plan(
                    &preflight,
                    &steps,
                    PlanPolicy {
                        independent_calls: false,
                        operation: Operation::Mutation,
                        credential: Some(credential()),
                        pacing: None,
                        retry: RetryPolicy::Never,
                    },
                )
                .unwrap();
            let slot = cx.plan_slots(&plan).unwrap().remove(0);
            self.plan = Some((plan, slot));
            self.plan_call(cx, true);
            return;
        }
        let context = cx
            .http_context(
                Scope::Command(command),
                Policy {
                    operation: Operation::Mutation,
                    credential: (!self.unsigned.contains(&command)).then(credential),
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            )
            .unwrap();
        let url = format!("https://venue.test/amend-{command}");
        if self.unsigned.contains(&command) {
            cx.request_managed(context.clone(), None, Method::Post, url, None, context)
        } else {
            cx.request_signed_managed(
                context.clone(),
                None,
                Method::Post,
                url,
                None,
                Box::new(Recipe(self.identity(self.signed.contains(&command)))),
                context,
            )
        }
        .map_err(|(_, error)| error)
        .unwrap();
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Context>,
        pending: Context,
        reason: Error,
        error: VenueError,
    ) {
        self.failures.push((reason, error.clone()));
        match &self.plan {
            Some((plan, _)) => cx.fail_plan(plan, error).unwrap(),
            None => cx.fail_context(&pending, error).unwrap(),
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Context>, context: Context, response: Response<'_>) {
        assert_eq!(response.status, 200);
        let (plan, slot) = self
            .plan
            .clone()
            .expect("only the Plan's first call is answered");
        cx.validate_plan_delivery(&plan, &context).unwrap();
        cx.http_write_fact(
            &context,
            WriteFact::Accepted {
                order_ids: vec![response.body.into()],
            },
            None,
        )
        .unwrap();
        cx.validate_http_write_effects(&context).unwrap();
        cx.finish_plan_call(&plan, &slot, &context).unwrap();
        self.plan_call(cx, false);
    }
    fn capture_http_reconciliation(&self, _: &Context) -> Result<ReconciliationData, Error> {
        ReconciliationData::new("custody-native".into())
    }
    fn on_http_owner_retired(&mut self, _: &HttpOwnerRetirement) {}
    fn on_sign_error(&mut self, _: &mut Cx<'_, Context>, _: Context, error: VenueError) {
        panic!("managed preparation failures use their own hook: {error:?}")
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, error: &str) {
        panic!("typed fixture failure required: {error}")
    }
    fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {
        panic!("no fixture socket")
    }
    fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {
        panic!("no fixture socket")
    }
    fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {
        panic!("no fixture socket")
    }
    fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

const NOW: f64 = 1_000.;

/// Immediate provider; counts every request it is asked to sign.
struct Provider(Arc<AtomicUsize>);
impl Signer for Provider {
    fn sign(&self, _: SignRequest) -> SignFuture {
        panic!("immediate fixture")
    }
    fn immediate(&self) -> bool {
        true
    }
    fn sign_ready(&self, work: &SignRequest) -> Option<Result<SignResult, SignError>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Some(Ok(SignResult::new(vec![vec![7; 32]], work.receipt)))
    }
}

/// An installed ref whose material the provider lacks: an inline MissingKey.
struct MissingProvider;
impl Signer for MissingProvider {
    fn sign(&self, _: SignRequest) -> SignFuture {
        panic!("immediate fixture")
    }
    fn immediate(&self) -> bool {
        true
    }
    fn sign_ready(&self, _: &SignRequest) -> Option<Result<SignResult, SignError>> {
        Some(Err(SignError::MissingKey))
    }
}

fn engine_missing(probe: Probe, identities: Identities) -> Engine<Probe> {
    let mut engine = safe_cex_engine::test_support::seeded_engine(probe);
    engine.set_identities(identities).unwrap();
    engine.set_signer(Some(Arc::new(MissingProvider))).unwrap();
    engine.start(NOW);
    engine.drain_actions().for_each(drop);
    engine
}

fn engine(probe: Probe) -> (Engine<Probe>, Arc<AtomicUsize>) {
    engine_with(probe, Identities::default())
}
fn engine_with(probe: Probe, identities: Identities) -> (Engine<Probe>, Arc<AtomicUsize>) {
    let provided = Arc::new(AtomicUsize::new(0));
    let mut engine = safe_cex_engine::test_support::seeded_engine(probe);
    engine.set_identities(identities).unwrap();
    engine
        .set_signer(Some(Arc::new(Provider(provided.clone()))))
        .unwrap();
    engine.start(NOW);
    engine.drain_actions().for_each(drop);
    (engine, provided)
}
fn amend(engine: &mut Engine<Probe>, at: f64, command: u32) -> Vec<Action> {
    engine.handle(
        at,
        Event::Command(Command::AmendOrder {
            command,
            id: "native".into(),
            price: Some(1.),
            amount: None,
        }),
    );
    prepared_actions(engine, at)
}
fn http(actions: &[Action]) -> Vec<&HttpRequest> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect()
}
fn failed(actions: &[Action], command: u32) -> Option<VenueError> {
    actions.iter().find_map(|a| match a {
        Action::Notice(Notice::CommandFailed { command: id, error }) if *id == command => {
            Some(error.clone())
        }
        _ => None,
    })
}

/// Installed primary and secondary that both sign.
fn signing_slots() -> Identities {
    Identities {
        primary: Some(signing(true)),
        secondary: Some(Identity::new(
            KeyRef("secondary.credential".into()),
            Some(KeyRef("secondary.signing".into())),
        )),
    }
}

fn config_invalid(actions: &[Action]) -> usize {
    actions
        .iter()
        .filter(|a| {
            matches!(a, Action::Notice(Notice::Event(LogEvent { code, .. }))
                if *code == safe_cex_engine::log::code::CONFIG_INVALID)
        })
        .count()
}

/// C4: the refused prepare reaches no provider and releases no HTTP, fails its
/// command InvalidInput / NotSent with MissingKey, and releases its reservation
/// and Context: a peer signed command afterwards still signs and releases. The
/// installed primary signs and a secondary exists, so the refusal is not
/// primary material and nothing latches (C4l is the primary case).
#[test]
fn c4_absent_signing_ref_refuses_in_preparation_and_releases_custody() {
    let (mut e, provided) = engine_with(
        Probe {
            signed: vec![2],
            ..Default::default()
        },
        signing_slots(),
    );
    let out = amend(&mut e, NOW, 1);
    assert_eq!(provided.load(Ordering::SeqCst), 0, "no provider work");
    assert!(http(&out).is_empty(), "no HTTP for an absent ref: {out:?}");
    let error = failed(&out, 1).expect("command 1 terminal");
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput), "{error:?}");
    assert_eq!(error.outcome, Some(Outcome::NotSent), "{error:?}");
    assert_eq!(e.venue().failures.len(), 1);
    assert_eq!(e.venue().failures[0].1, SignError::MissingKey.venue_error());

    assert_eq!(config_invalid(&out), 0, "not primary material: {out:?}");
    let peer = amend(&mut e, NOW + 1., 2);
    assert_eq!(
        provided.load(Ordering::SeqCst),
        1,
        "the peer reaches the provider"
    );
    assert_eq!(http(&peer).len(), 1, "the peer releases: {peer:?}");
    assert_eq!(failed(&peer, 2), None);
}

/// C4p: in a sequential Mutation Plan, the first signed call settles Accepted,
/// then a second call without a signing ref refuses NotSent in preparation. The
/// command projection keeps the settled call's facts: not NotSent, not Rejected.
#[test]
fn c4p_plan_keeps_a_settled_call_when_a_later_call_lacks_a_signing_ref() {
    let (mut e, provided) = engine(Probe {
        plan_mode: true,
        ..Default::default()
    });
    let out = amend(&mut e, NOW, 1);
    let [request] = http(&out)[..] else {
        panic!("the first call releases: {out:?}")
    };
    let request = request.clone();
    assert_eq!(provided.load(Ordering::SeqCst), 1);
    e.handle(
        NOW + 1.,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: "settled-native-id",
            },
        },
    );
    let out = prepared_actions(&mut e, NOW + 1.);
    assert_eq!(
        provided.load(Ordering::SeqCst),
        1,
        "no provider work for an absent ref"
    );
    assert!(http(&out).is_empty(), "no HTTP for an absent ref: {out:?}");
    assert_eq!(e.venue().failures.len(), 1);
    assert_eq!(e.venue().failures[0].1, SignError::MissingKey.venue_error());
    let error = failed(&out, 1).expect("command 1 terminal");
    assert_ne!(
        error.outcome,
        Some(Outcome::NotSent),
        "a settled Accepted call is not NotSent: {error:?}"
    );
    assert_ne!(
        error.outcome,
        Some(Outcome::Rejected),
        "a settled Accepted call is not Rejected: {error:?}"
    );
    assert_eq!(
        error.partial.as_ref().map(|p| p.order_ids.clone()),
        Some(vec!["settled-native-id".to_string()]),
        "the settled call's native id survives: {error:?}"
    );
}

/// C4l: with a secondary installed, an HTTP refusal for the credential-only
/// primary's absent signing ref is still primary material and latches once.
#[test]
fn c4l_a_credential_only_primary_latches_beside_a_secondary() {
    let mut identities = signing_slots();
    identities.primary = Some(signing(false));
    let (mut e, provided) = engine_with(Probe::default(), identities);
    let out = amend(&mut e, NOW, 1);
    assert_eq!(provided.load(Ordering::SeqCst), 0);
    assert_eq!(config_invalid(&out), 1, "{out:?}");
    assert_eq!(e.venue().failures[0].1, SignError::MissingKey.venue_error());
}

/// The Optional family's trusted setup: a credential-only primary whose
/// absence is intentional (D6), derived by `apply_to_identities`.
fn optional_identities() -> Identities {
    let mut ids = Identities {
        primary: Some(signing(false)),
        secondary: None,
    };
    let policy: safe_cex_engine::signing::CredentialSetupPolicy =
        serde_json::from_str(r#"{"primary":{"signing":"optional"}}"#).unwrap();
    policy.apply_to_identities(&mut ids, false).unwrap();
    ids
}

fn startup_failed(actions: &[Action]) -> bool {
    actions
        .iter()
        .any(|a| matches!(a, Action::Notice(Notice::StartupFailed { .. })))
}

/// C11: an Optional family's intentional absence refuses only that signed
/// operation, typed NotSent: no provider work, no latch, no config.invalid or
/// StartupFailed, and later unsigned work still releases.
#[test]
fn c11_optional_absence_refuses_the_operation_without_a_latch() {
    let ids = optional_identities();
    let (mut e, provided) = engine_with(
        Probe {
            absent: ids.primary.clone(),
            unsigned: vec![2],
            ..Default::default()
        },
        ids,
    );
    let out = amend(&mut e, NOW, 1);
    assert_eq!(provided.load(Ordering::SeqCst), 0);
    assert!(http(&out).is_empty(), "{out:?}");
    assert_eq!(
        e.venue().failures[0].1,
        SignError::SigningNotConfigured.venue_error()
    );
    assert_eq!(config_invalid(&out), 0, "no latch: {out:?}");
    assert!(!startup_failed(&out), "{out:?}");
    let peer = amend(&mut e, NOW + 1., 2);
    assert_eq!(
        http(&peer).len(),
        1,
        "unsigned work still releases: {peer:?}"
    );
    assert_eq!(config_invalid(&peer), 0);
}

/// C11p: in a sequential Mutation Plan the absent-ref call fails typed while
/// the settled call's facts survive; there is no account latch.
#[test]
fn c11p_plan_keeps_settled_facts_when_an_optional_call_lacks_signing() {
    let ids = optional_identities();
    let (mut e, provided) = engine_with(
        Probe {
            plan_mode: true,
            absent: ids.primary.clone(),
            ..Default::default()
        },
        ids,
    );
    let out = amend(&mut e, NOW, 1);
    let [request] = http(&out)[..] else {
        panic!("the first call releases: {out:?}")
    };
    let request = request.clone();
    e.handle(
        NOW + 1.,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: "settled-native-id",
            },
        },
    );
    let out = prepared_actions(&mut e, NOW + 1.);
    assert_eq!(provided.load(Ordering::SeqCst), 1);
    assert_eq!(
        e.venue().failures[0].1,
        SignError::SigningNotConfigured.venue_error()
    );
    assert_eq!(config_invalid(&out), 0, "no latch: {out:?}");
    let error = failed(&out, 1).expect("command 1 terminal");
    assert_eq!(
        error.partial.as_ref().map(|p| p.order_ids.clone()),
        Some(vec!["settled-native-id".to_string()])
    );
}

/// C11i (control): the same absent ref under Required policy keeps #2109's
/// MissingKey latch.
#[test]
fn c11i_required_absence_still_latches_missing_key() {
    let ids = Identities {
        primary: Some(signing(false)),
        secondary: None,
    };
    let (mut e, _) = engine_with(
        Probe {
            absent: ids.primary.clone(),
            ..Default::default()
        },
        ids,
    );
    let out = amend(&mut e, NOW, 1);
    assert_eq!(e.venue().failures[0].1, SignError::MissingKey.venue_error());
    assert_eq!(config_invalid(&out), 1, "{out:?}");
}

/// C11i (Optional arm): an Optional family whose primary has an INSTALLED ref
/// still latches on a real inline provider MissingKey (#2119), so the typed
/// Optional refusal never swallows a provider failure.
#[test]
fn c11i_optional_installed_ref_still_latches_on_provider_missing_key() {
    let mut ids = Identities {
        primary: Some(signing(true)),
        secondary: None,
    };
    let policy: safe_cex_engine::signing::CredentialSetupPolicy =
        serde_json::from_str(r#"{"primary":{"signing":"optional"}}"#).unwrap();
    policy.apply_to_identities(&mut ids, false).unwrap();
    assert!(ids.primary.as_ref().unwrap().signing_ref().is_ok());
    let mut e = engine_missing(
        Probe {
            signed: vec![1],
            ..Default::default()
        },
        ids,
    );
    let out = amend(&mut e, NOW, 1);
    assert_eq!(config_invalid(&out), 1, "{out:?}");
    assert_eq!(
        failed(&out, 1).map(|error| error.message),
        Some(SignError::MissingKey.venue_error().message)
    );
}
