#![cfg(feature = "test-support")]

use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{
    Claim, Credential, HttpAction, Operation as HttpOperation, Policy, Prepared, RetryPolicy, Scope,
};
use safe_cex_engine::signing::{
    Algorithm, Capability, Context, Identities, Identity, KeyRef, Operation, PayloadPart, Provider,
    SignError, SignFuture, SignRequest, SignResult, Signer,
};
use safe_cex_engine::{
    Action, Command, CredentialSlot, Cx, DispatchContext, Engine, Event, HttpRequest, Method,
    Outcome, RequestId, Response, SignStep, SocketId, TimerId, Venue, VenueError,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct Owner {
    marker: u8,
    immediate: bool,
    calls: Arc<AtomicUsize>,
}
impl Signer for Owner {
    fn immediate(&self) -> bool {
        self.immediate
    }
    fn can_sign(&self, _: &KeyRef, _: Algorithm) -> Capability {
        Capability::Supported
    }
    fn sign(&self, _: SignRequest) -> SignFuture {
        let marker = self.marker;
        Box::pin(async move { Ok(synthetic_result(marker)) })
    }
    fn sign_ready(&self, _: &SignRequest) -> Option<Result<SignResult, SignError>> {
        self.immediate.then(|| {
            self.calls.fetch_add(1, Ordering::Relaxed);
            Ok(synthetic_result(self.marker))
        })
    }
}
fn provider(marker: u8, immediate: bool) -> (Provider, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    (
        Arc::new(Owner {
            marker,
            immediate,
            calls: calls.clone(),
        }),
        calls,
    )
}
// Synthetic signature markers; no signing key or live credential is used.
fn synthetic_result(marker: u8) -> SignResult {
    SignResult::new(vec![vec![marker; 32]], Default::default())
}
struct Recipe;
impl SignStep for Recipe {
    fn freshness_ms(&self) -> f64 {
        5_000.
    }
    fn prepare(
        &mut self,
        action: &mut Action,
        _: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let Action::Http(request) = action else {
            return Err(SignError::InvalidRequest);
        };
        Ok(SignRequest {
            receipt: Default::default(),
            context: Context::default(),
            operations: vec![Operation::Sign {
                algorithm: Algorithm::HmacSha256,
                key: KeyRef("account/primary".into()),
                payload: vec![PayloadPart::Bytes {
                    bytes: request.body.as_ref().unwrap().as_bytes().to_vec(),
                }],
            }],
        })
    }
    fn apply(&mut self, action: &mut Action, result: &SignResult) -> Result<(), SignError> {
        let Action::Http(request) = action else {
            return Err(SignError::InvalidRequest);
        };
        request
            .headers
            .push(("test-owner", result.values[0][0].to_string()));
        Ok(())
    }
}
#[derive(Default)]
struct Probe {
    managed: bool,
    errors: Vec<(u32, VenueError)>,
}
impl Venue for Probe {
    type Pending = u32;
    fn capture_http_reconciliation(
        &self,
        command: &u32,
    ) -> Result<
        safe_cex_engine::request_context::ReconciliationData,
        safe_cex_engine::request_context::Error,
    > {
        safe_cex_engine::request_context::ReconciliationData::new(format!("owner-order-{command}"))
    }
    fn start(&mut self, _: &mut Cx<'_, u32>) {}
    fn on_open(&mut self, _: &mut Cx<'_, u32>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, u32>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, u32>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, u32>, _: u32, _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, u32>, _: u32, _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, u32>, _: TimerId) {}
    fn on_sign_error(&mut self, cx: &mut Cx<'_, u32>, command: u32, error: VenueError) {
        self.errors.push((command, error.clone()));
        if !self.managed {
            cx.fail(command, error);
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, u32>, command: Command) {
        let Command::PlaceOrders { command, .. } = command else {
            panic!("fixture placement")
        };
        let body = Some(format!("owner-order-{command}"));
        if self.managed {
            let context = cx
                .http_context(
                    Scope::Command(command),
                    Policy {
                        operation: HttpOperation::Mutation,
                        credential: Some(Credential {
                            slot: CredentialSlot::Primary,
                            reference: KeyRef("account/primary".into()),
                        }),
                        timeout_ms: 20_000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    },
                )
                .unwrap();
            cx.request_signed_managed(
                context,
                None,
                Method::Post,
                "https://venue.test/order".into(),
                body,
                Box::new(Recipe),
                command,
            )
            .unwrap_or_else(|(pending, error)| {
                panic!("fixture managed admission {pending}: {error:?}")
            });
        } else {
            let request = cx.request(
                Method::Post,
                "https://venue.test/order".into(),
                vec![],
                body,
                command,
            );
            cx.sign_http(request, Box::new(Recipe));
        }
    }
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn configured(provider: Provider, managed: bool) -> Engine<Probe> {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Probe {
        managed,
        ..Default::default()
    });
    engine
        .set_identities(Identities {
            primary: Some(Identity::new(
                KeyRef("account/primary".into()),
                Some(KeyRef("account/primary".into())),
            )),
            ..Default::default()
        })
        .unwrap();
    engine.set_signer(Some(provider)).unwrap();
    engine
}
fn queue(engine: &mut Engine<Probe>, command: u32) {
    engine.handle(
        1.,
        Event::Command(Command::PlaceOrders {
            command,
            orders: vec![],
        }),
    );
}
fn signed(engine: &mut Engine<Probe>, attempt: RequestId, result: Result<SignResult, SignError>) {
    engine.handle(2., Event::Signed { attempt, result });
}
fn signing(engine: &mut Engine<Probe>) -> RequestId {
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    let tasks: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Sign { attempt, .. } => Some(*attempt),
            _ => None,
        })
        .collect();
    assert_eq!(tasks.len(), 1);
    assert!(engine.signing_pending(tasks[0]));
    tasks[0]
}
fn packet(engine: &mut Engine<Probe>, command: u32, marker: u8) {
    let requests: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 1);
    assert_packet(&requests[0], command, marker);
}
fn assert_packet(request: &HttpRequest, command: u32, marker: u8) {
    assert_eq!(request.method, Method::Post);
    assert_eq!(request.url, "https://venue.test/order");
    assert_eq!(
        request.body.as_deref(),
        Some(format!("owner-order-{command}").as_str())
    );
    assert_eq!(request.headers, vec![("test-owner", marker.to_string())]);
}
fn revoked(engine: &mut Engine<Probe>) {
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(
        !actions.iter().any(|a| matches!(
            a,
            Action::Http(_) | Action::Sign { .. } | Action::SignedOutbound { .. }
        )),
        "stale owner must not sign or send: {actions:?}"
    );
    assert!(!actions.iter().any(|a| matches!(a, Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Prepare {..} | HttpAction::Bind {..}))));
    assert_eq!(engine.venue().errors.len(), 1);
    assert_eq!(engine.venue().errors[0].0, 7);
    assert_eq!(engine.venue().errors[0].1.outcome, Some(Outcome::NotSent));
}

#[test]
fn queued_owner_a_write_is_not_prepared_by_replacement_provider_b() {
    let (a, calls_a) = provider(b'A', true);
    let (b, calls_b) = provider(b'B', true);
    let mut engine = configured(a, false);
    queue(&mut engine, 7);
    assert!(!engine.has_started());
    engine.set_signer(Some(b)).unwrap();
    revoked(&mut engine);
    assert_eq!(calls_a.load(Ordering::Relaxed), 0);
    assert_eq!(calls_b.load(Ordering::Relaxed), 0);
    queue(&mut engine, 8);
    packet(&mut engine, 8, b'B');
    assert_eq!(calls_b.load(Ordering::Relaxed), 1);
}

#[test]
fn old_provider_completion_and_error_cannot_release_or_poison_b_work() {
    for late_error in [false, true] {
        let (a, _) = provider(b'A', false);
        let (b, calls_b) = provider(b'B', true);
        let mut engine = configured(a, false);
        queue(&mut engine, 7);
        let attempt = signing(&mut engine);
        engine.set_signer(Some(b)).unwrap();
        assert!(!engine.signing_pending(attempt));
        signed(
            &mut engine,
            attempt,
            if late_error {
                Err(SignError::MissingKey)
            } else {
                Ok(synthetic_result(b'A'))
            },
        );
        revoked(&mut engine);
        signed(&mut engine, attempt, Ok(synthetic_result(b'A')));
        revoked(&mut engine);
        queue(&mut engine, 8);
        packet(&mut engine, 8, b'B');
        assert_eq!(calls_b.load(Ordering::Relaxed), 1);
    }
}

#[test]
fn signed_owner_a_packet_is_revoked_before_final_release() {
    let (a, _) = provider(b'A', false);
    let (b, _) = provider(b'B', true);
    let mut engine = configured(a, false);
    queue(&mut engine, 7);
    let attempt = signing(&mut engine);
    signed(&mut engine, attempt, Ok(synthetic_result(b'A')));
    engine.set_signer(Some(b)).unwrap();
    revoked(&mut engine);
    queue(&mut engine, 8);
    packet(&mut engine, 8, b'B');
}

#[test]
fn same_provider_and_rejected_post_start_replacement_preserve_a_bytes() {
    for immediate in [false, true] {
        for started in [false, true] {
            let (a, _) = provider(b'A', immediate);
            let mut engine = configured(a.clone(), false);
            if started {
                engine.start(0.);
            }
            queue(&mut engine, 7);
            let attempt = (!immediate).then(|| signing(&mut engine));
            if started {
                let (b, _) = provider(b'B', true);
                assert!(engine.set_signer(Some(b)).is_err());
            } else {
                engine.set_signer(Some(a)).unwrap();
            }
            if let Some(attempt) = attempt {
                assert!(engine.signing_pending(attempt));
                signed(&mut engine, attempt, Ok(synthetic_result(b'A')));
            }
            packet(&mut engine, 7, b'A');
            assert!(engine.venue().errors.is_empty());
        }
    }
}

#[test]
fn removing_provider_or_restoring_a_after_b_never_revives_a_old_queue() {
    for restore_a in [false, true] {
        let (a, _) = provider(b'A', true);
        let (b, _) = provider(b'B', true);
        let mut engine = configured(a.clone(), false);
        queue(&mut engine, 7);
        engine
            .set_signer(if restore_a { Some(b) } else { None })
            .unwrap();
        if restore_a {
            engine.set_signer(Some(a)).unwrap();
        }
        revoked(&mut engine);
    }
}

#[test]
fn provider_switch_revokes_managed_signing_waiting_for_route_preparation() {
    let (a, _) = provider(b'A', true);
    let (b, calls_b) = provider(b'B', true);
    let mut engine = configured(a, true);
    queue(&mut engine, 7);
    let preparation = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    engine.set_signer(Some(b)).unwrap();
    engine.http_prepared(
        2.,
        preparation,
        Prepared {
            id: 77,
            expires_at: None,
        },
    );
    revoked(&mut engine);
    assert_eq!(calls_b.load(Ordering::Relaxed), 0);
}

#[test]
fn provider_switch_revokes_managed_signed_permit_before_claim() {
    for replace in [false, true] {
        let (a, _) = provider(b'A', true);
        let (b, _) = provider(b'B', true);
        let mut engine = configured(a, true);
        queue(&mut engine, 7);
        let preparation = engine
            .drain_actions()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match *a {
                    HttpAction::Prepare { preparation, .. } => Some(preparation),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        engine.http_prepared(
            2.,
            preparation,
            Prepared {
                id: 77,
                expires_at: None,
            },
        );
        let (permit, request) = engine
            .drain_actions()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match *a {
                    HttpAction::Bind {
                        permit, request, ..
                    } => Some((permit, request)),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        assert_packet(&request, 7, b'A');
        if replace {
            engine.set_signer(Some(b)).unwrap();
        }
        let claimed = engine.claim_http(3., permit);
        if replace {
            assert_eq!(
                claimed,
                Err(safe_cex_engine::request_context::Error::Revoked)
            );
        } else {
            assert!(
                matches!(claimed, Ok(Claim::Released(_))),
                "same-owner claim: {claimed:?}"
            );
        }
    }
}
