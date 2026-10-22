//! Synthetic managed recipes exercise the shared ingress, not family admission.
use super::*;
use crate::request_context::{Credential, Policy, RetryPolicy, Scope};
use crate::signing::{KeyRef, Operation as SignOperation, SignError, SignRequest, SignResult};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

const PUBLIC: &[(&str, &str)] = &[("X-Source", "public"), ("Content-Type", "application/json")];

struct Recipe {
    headers: &'static [(&'static str, &'static str)],
    calls: Arc<AtomicUsize>,
}
impl crate::SignStep for Recipe {
    fn unsigned_http_headers(&self) -> &'static [(&'static str, &'static str)] {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.headers
    }
    fn prepare(
        &mut self,
        outbound: &mut Action,
        _: &crate::DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        if http.headers != owned(self.headers) || !http.signer_headers.is_empty() {
            return Err(SignError::InvalidRequest);
        }
        Ok(SignRequest {
            context: Default::default(),
            receipt: Default::default(),
            operations: vec![SignOperation::Present {
                key: KeyRef("ref:owned-primary".into()),
            }],
        })
    }
    fn apply(&mut self, outbound: &mut Action, _: &SignResult) -> Result<(), SignError> {
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        http.signer_headers
            .push(("x-credential".into(), "provider-value".into()));
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        1_000.
    }
}

struct Probe {
    headers: &'static [(&'static str, &'static str)],
    calls: Arc<AtomicUsize>,
    result: Option<Result<RequestId, (u8, Error)>>,
}
impl Venue for Probe {
    type Pending = u8;
    fn start(&mut self, cx: &mut Cx<'_, u8>) {
        let context = cx
            .http_context(
                Scope::Background,
                Policy {
                    operation: Operation::Read,
                    credential: Some(Credential {
                        slot: CredentialSlot::Primary,
                        reference: KeyRef("ref:owned-primary".into()),
                    }),
                    timeout_ms: 1_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            )
            .unwrap();
        self.result = Some(cx.request_signed_managed(
            context,
            Method::Get,
            "https://venue.test/read".into(),
            None,
            Box::new(Recipe {
                headers: self.headers,
                calls: self.calls.clone(),
            }),
            41,
        ));
    }
    fn on_response(&mut self, _: &mut Cx<'_, u8>, _: u8, _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, u8>, _: u8, error: &str) {
        panic!("unexpected fixture failure: {error}");
    }
    fn on_command(&mut self, _: &mut Cx<'_, u8>, _: Command) {}
    fn on_open(&mut self, _: &mut Cx<'_, u8>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, u8>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, u8>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, u8>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn owned(headers: &'static [(&'static str, &'static str)]) -> Vec<(&'static str, String)> {
    headers
        .iter()
        .map(|&(name, value)| (name, value.into()))
        .collect()
}
fn start(
    headers: &'static [(&'static str, &'static str)],
) -> (Engine<Probe>, Vec<Action>, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut engine = crate::test_support::seeded_engine(Probe {
        headers,
        calls: calls.clone(),
        result: None,
    });
    engine.start(0.);
    let actions = engine.drain_actions().collect();
    (engine, actions, calls)
}
fn prepared(actions: &[Action]) -> (Preparation, &HttpRequest) {
    actions
        .iter()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => Some((*preparation, request)),
                _ => None,
            },
            _ => None,
        })
        .expect("managed preparation")
}

#[test]
fn declared_headers_reach_managed_preparation_in_exact_order() {
    let (_, actions, _) = start(PUBLIC);
    let (_, request) = prepared(&actions);
    assert_eq!(request.headers, owned(PUBLIC));
    assert!(request.signer_headers.is_empty());
    assert!(!actions.iter().any(|a| matches!(a, Action::Sign { .. })));
}

#[test]
fn projection_is_read_once_and_survives_expired_route_reprepare() {
    let (mut engine, actions, calls) = start(PUBLIC);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let (token, _) = prepared(&actions);
    engine.http_prepared(
        1.,
        token,
        Prepared {
            id: 7,
            expires_at: Some(5.),
        },
    );
    let work = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::Sign { attempt, work, .. } => Some((attempt, work)),
            _ => None,
        })
        .expect("header-attested signing work");
    engine.handle(
        2.,
        Event::Signed {
            attempt: work.0,
            result: Ok(SignResult {
                values: vec![b"provider-value".to_vec()],
                receipt: work.1.receipt,
                headers: vec![],
            }),
        },
    );
    let permit = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Bind { permit, .. } => Some(permit),
                _ => None,
            },
            _ => None,
        })
        .expect("managed bind");
    assert_eq!(engine.claim_http(6., permit), Err(Error::Stale));
    let actions = engine.drain_actions().collect::<Vec<_>>();
    let (_, request) = prepared(&actions);
    assert_eq!(request.headers, owned(PUBLIC));
    assert!(request.signer_headers.is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn empty_declaration_keeps_managed_preparation_empty() {
    let (_, actions, _) = start(&[]);
    let (_, request) = prepared(&actions);
    assert!(request.headers.is_empty() && request.signer_headers.is_empty());
}
