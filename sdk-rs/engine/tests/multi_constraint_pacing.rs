//! Real managed signed HTTP controls; original scalar pre-sign REDs precede set support.
#![cfg(feature = "test-support")]
use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{
    Admission, Claim, Context, Credential, Error, HttpAction, HttpRelease, Operation, Pacing,
    Permit, Policy, Preparation, Prepared, RetryPolicy, Scope,
};
use safe_cex_engine::signing::{
    KeyRef, Operation as SignOperation, SignError, SignReceipt, SignRequest, SignResult,
};
use safe_cex_engine::{
    Action, CredentialSlot, Cx, DispatchContext, Engine, Event, Method, RequestId, SignStep, Venue,
    VenueError,
};
const GLOBAL: Pacing = Pacing {
    group: 1,
    minimum_gap_ms: 100.,
};
#[derive(Debug)]
struct Pending(Context);
#[derive(Default)]
struct Probe {
    failures: Vec<Error>,
    releases: usize,
}
struct Recipe;
impl SignStep for Recipe {
    fn prepare(
        &mut self,
        action: &mut Action,
        cx: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let Action::Http(request) = action else {
            return Err(SignError::InvalidRequest);
        };
        request.url = format!("https://pacing.test/read?stamp={}", cx.stamp_ms());
        Ok(SignRequest {
            receipt: SignReceipt {
                stamp_ms: Some(cx.stamp_ms()),
                expiry_ms: Some(cx.stamp_ms() + 1000.),
            },
            context: Default::default(),
            operations: vec![SignOperation::Present {
                key: KeyRef("fixture".into()),
            }],
        })
    }
    fn apply(&mut self, _: &mut Action, _: &SignResult) -> Result<(), SignError> {
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        1000.
    }
}
impl Venue for Probe {
    type Pending = Pending;
    fn http_pacing(&self) -> &[Pacing] {
        &[GLOBAL]
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        for _ in 0..2 {
            let context = cx
                .http_context(
                    Scope::Background,
                    Policy {
                        operation: Operation::Read,
                        credential: Some(Credential {
                            slot: CredentialSlot::Primary,
                            reference: KeyRef("fixture".into()),
                        }),
                        timeout_ms: 1000,
                        pacing: Some(GLOBAL),
                        retry: RetryPolicy::Never,
                    },
                )
                .unwrap();
            cx.request_signed_managed(
                context.clone(),
                Method::Post,
                "https://pacing.test/read".into(),
                None,
                Box::new(Recipe),
                Pending(context),
            )
            .unwrap();
        }
    }
    fn on_http_release(&mut self, _: &mut Pending, _: &HttpRelease) -> Result<Admission, Error> {
        self.releases += 1;
        Ok(Admission::Unscheduled)
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        reason: Error,
        _: VenueError,
    ) {
        self.failures.push(reason);
        cx.finish_http_context(&pending.0).unwrap();
    }
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: safe_cex_engine::SocketId) {
        panic!("no fixture socket")
    }
    fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: safe_cex_engine::SocketId, _: &str) {
        panic!("no fixture socket")
    }
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: safe_cex_engine::SocketId, _: u16) {
        panic!("no fixture socket")
    }
    fn on_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        _: safe_cex_engine::Response<'_>,
    ) {
        cx.finish_http_context(&pending.0).unwrap();
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, Pending>, _: Pending, _: &str) {
        panic!("typed fixture failure required")
    }
    fn on_timer(&mut self, _: &mut Cx<'_, Pending>, _: safe_cex_engine::TimerId) {
        panic!("managed timer escaped")
    }
    fn on_command(&mut self, _: &mut Cx<'_, Pending>, _: safe_cex_engine::Command) {
        panic!("no fixture command")
    }
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn start() -> (Engine<Probe>, Vec<Preparation>) {
    let mut e = safe_cex_engine::test_support::seeded_engine(Probe::default());
    e.start(0.);
    let tokens = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .collect();
    (e, tokens)
}
fn route(e: &mut Engine<Probe>, token: Preparation, at: f64) -> Vec<Action> {
    e.http_prepared(
        at,
        token,
        Prepared {
            id: token.attempt().request.0,
            expires_at: None,
        },
    );
    e.drain_actions().collect()
}
fn work(out: &[Action]) -> (RequestId, SignRequest) {
    out.iter()
        .find_map(|a| match a {
            Action::Sign { attempt, work, .. } => Some((*attempt, work.clone())),
            _ => None,
        })
        .expect("eligible provider work")
}
fn signed(e: &mut Engine<Probe>, at: f64, attempt: RequestId, work: SignRequest) -> Permit {
    e.handle(
        at,
        Event::Signed {
            attempt,
            result: Ok(SignResult {
                values: vec![b"fixture".to_vec()],
                receipt: work.receipt,
                headers: vec![],
            }),
        },
    );
    e.drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Bind { permit, .. } => Some(permit),
                _ => None,
            },
            _ => None,
        })
        .expect("eligible signed Bind")
}
#[test]
fn a_known_pacing_wait_does_not_start_the_provider_early() {
    let (mut e, tokens) = start();
    let (attempt, work) = work(&route(&mut e, tokens[0], 0.));
    let permit = signed(&mut e, 0., attempt, work);
    assert!(matches!(e.claim_http(0., permit), Ok(Claim::Released(_))));
    let out = route(&mut e, tokens[1], 10.);
    assert!(
        !out.iter().any(|a| matches!(a, Action::Sign { .. })),
        "known100ms release bound must be checked before provider/entropy"
    );
    assert!(
        out.iter()
            .any(|a| matches!(a,Action::SetTimer {at_ms,..} if *at_ms==100.))
    );
}
#[test]
fn delayed_route_observes_the_latest_successful_claim_before_signing() {
    let (mut e, tokens) = start();
    let (attempt, work) = work(&route(&mut e, tokens[0], 0.));
    let permit = signed(&mut e, 300., attempt, work);
    assert!(matches!(e.claim_http(300., permit), Ok(Claim::Released(_))));
    let out = route(&mut e, tokens[1], 310.);
    assert!(
        !out.iter().any(|a| matches!(a, Action::Sign { .. })),
        "delayed route must see400ms eligibility before preparing a stamp"
    );
    assert!(
        out.iter()
            .any(|a| matches!(a,Action::SetTimer {at_ms,..} if *at_ms==400.))
    );
}
#[test]
fn an_eligible_signed_request_keeps_the_existing_route_provider_bind_claim_path() {
    let (mut e, tokens) = start();
    let (attempt, work) = work(&route(&mut e, tokens[0], 0.));
    assert_eq!(work.receipt.stamp_ms, Some(0.));
    let permit = signed(&mut e, 0., attempt, work);
    assert!(matches!(e.claim_http(0., permit), Ok(Claim::Released(_))));
    assert_eq!(e.venue().releases, 1);
    assert!(e.venue().failures.is_empty());
}
