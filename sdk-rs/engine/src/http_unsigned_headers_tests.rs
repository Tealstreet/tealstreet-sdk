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
    before_id: u32,
    count: usize,
}
impl Venue for Probe {
    type Pending = u8;
    fn start(&mut self, cx: &mut Cx<'_, u8>) {
        for _ in 0..self.count {
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
            self.before_id = cx.io.next_id;
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
    start_count(headers, 1)
}
fn start_count(
    headers: &'static [(&'static str, &'static str)],
    count: usize,
) -> (Engine<Probe>, Vec<Action>, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut engine = crate::test_support::seeded_engine(Probe {
        headers,
        calls: calls.clone(),
        result: None,
        before_id: 0,
        count,
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

// Leaked values are bounded synthetic test declarations, never credentials.
fn declaration(values: Vec<(String, String)>) -> &'static [(&'static str, &'static str)] {
    Box::leak(
        values
            .into_iter()
            .map(|(name, value)| {
                let name: &'static str = Box::leak(name.into_boxed_str());
                let value: &'static str = Box::leak(value.into_boxed_str());
                (name, value)
            })
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    )
}

#[test]
fn invalid_last_header_returns_pending_without_allocating_or_publishing() {
    let mut invalid = vec![
        declaration(vec![
            ("valid".into(), "first".into()),
            ("bad name".into(), "last".into()),
        ]),
        declaration(vec![("x".into(), "line\r\nbreak".into())]),
        declaration(vec![("x".into(), "delete\u{7f}".into())]),
        declaration(vec![("".into(), "value".into())]),
        declaration(vec![("x:y".into(), "value".into())]),
        declaration(vec![("x".repeat(257), "value".into())]),
        declaration(vec![("x".into(), "é".repeat(4096) + "x")]),
        declaration(vec![("x".into(), "v".into()); 33]),
    ];
    let mut aggregate = vec![("x".into(), "v".repeat(8191)); 4];
    aggregate.push(("x".into(), "".into()));
    invalid.push(declaration(aggregate));
    for headers in invalid {
        let (mut engine, actions, calls) = start(headers);
        assert_eq!(engine.venue().result, Some(Err((41, Error::Invalid))));
        assert_eq!(engine.io.next_id, engine.venue().before_id);
        assert!(engine.io.pending.is_empty() && engine.io.managed_http.is_empty());
        assert!(engine.io.timers.is_empty() && actions.is_empty());
        assert!(engine.io.signing.next_work().is_none());
        assert_eq!(engine.io.signing.next_id(), RequestId(1));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn exact_header_boundaries_and_duplicate_case_order_are_preserved() {
    for headers in [
        declaration(vec![("x".repeat(256), "value".into())]),
        declaration(vec![("x".into(), "é".repeat(4096))]),
        declaration(vec![("x".into(), "v".into()); 32]),
        declaration(vec![("x".into(), "v".repeat(8191)); 4]),
        &[("x-Public", "one\ttwo"), ("X-public", "three")],
    ] {
        let (engine, actions, calls) = start(headers);
        assert!(engine.venue().result.as_ref().unwrap().is_ok());
        assert_eq!(prepared(&actions).1.headers, owned(headers));
        assert!(prepared(&actions).1.signer_headers.is_empty());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn existing_context_and_credential_rejections_do_not_read_declaration() {
    let (mut engine, _, _) = start(&[]);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut cx = Cx {
        now: 0.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    for credential in [
        None,
        Some(Credential {
            slot: CredentialSlot::Primary,
            reference: KeyRef("ref:owned-primary".into()),
        }),
    ] {
        let context = cx
            .http_context(
                Scope::Background,
                Policy {
                    operation: Operation::Read,
                    credential: credential.clone(),
                    timeout_ms: 1,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            )
            .unwrap();
        cx.now = if credential.is_some() { 2. } else { 0. };
        let before = cx.io.next_id;
        assert!(
            cx.request_signed_managed(
                context,
                Method::Get,
                "https://venue.test/read".into(),
                None,
                Box::new(Recipe {
                    headers: PUBLIC,
                    calls: calls.clone()
                }),
                99
            )
            .is_err()
        );
        assert_eq!(cx.io.next_id, before);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        cx.now = 0.;
    }
}

#[test]
fn full_managed_capacity_does_not_read_or_allocate_for_the_refused_recipe() {
    let (mut engine, actions, calls) = start_count(PUBLIC, 256);
    // Both registries have capacity 256. Reuse valid existing read authority so
    // the extra request reaches managed admission, not context creation.
    let context = engine
        .io
        .managed_http
        .values()
        .next()
        .unwrap()
        .lifecycle
        .context()
        .clone();
    let before = engine.io.next_id;
    {
        let mut cx = Cx {
            now: 0.,
            store: &mut engine.store,
            io: &mut engine.io,
        };
        assert_eq!(
            cx.request_signed_managed(
                context,
                Method::Get,
                "https://venue.test/read".into(),
                None,
                Box::new(Recipe {
                    headers: PUBLIC,
                    calls: calls.clone()
                }),
                99
            ),
            Err((99, Error::Capacity))
        );
    }
    assert_eq!(engine.io.next_id, before);
    assert!(engine.io.actions.is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 256);
    assert_eq!(engine.io.pending.len(), 256);
    assert_eq!(engine.io.managed_http.len(), 256);
    assert_eq!(engine.io.signing.next_id(), RequestId(257));
    assert_eq!(
        actions
            .iter()
            .filter(|action| matches!(action,
                Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Prepare { .. })
            ))
            .count(),
        256
    );
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Sign { .. }))
    );
}
