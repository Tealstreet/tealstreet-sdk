#![cfg(all(feature = "test-support", feature = "dispatch-nonce"))]
use safe_cex_engine::book::Book;
use safe_cex_engine::credential_selection::{CredentialRole, SlotPolicy, Slots};
use safe_cex_engine::nonce::{KRAKEN, NonceClaim};
use safe_cex_engine::request_context::{
    Claim, Context, Error, HttpAction, Prepared, RetryEvidence, RetryPolicy, RoutingIntent,
};
use safe_cex_engine::selected_read::{
    AdmittedRead, LocalRefusal, ReadLeg, ReadLegPolicy, ReadPolicy, ReadRequest, ReadRole,
    ReadRoot, SelectedRead,
};
use safe_cex_engine::signing::{
    Identities, Identity, KeyRef, Operation, SignError, SignReceipt, SignRequest, SignResult,
};
use safe_cex_engine::{
    Action, Command, Cx, DispatchContext, Engine, Event, HttpRelease, Method, Notice, Response,
    SignStep, SocketId, TimerId, Venue, VenueError,
};
use std::sync::{Arc, Mutex};
fn identity(name: &str, subject: Option<&str>) -> Identity {
    Identity::new(
        KeyRef(format!("credential/{name}")),
        Some(KeyRef(format!("signing/{name}"))),
    )
    .with_stable_id(subject.map(Into::into))
}
struct Recipe {
    claim: Option<NonceClaim>,
    identity: Identity,
}
impl SignStep for Recipe {
    fn nonce_claim(&self) -> Option<&NonceClaim> {
        self.claim.as_ref()
    }
    fn prepare(
        &mut self,
        _: &mut Action,
        cx: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        assert_eq!(
            self.claim.as_ref().unwrap().slot(),
            safe_cex_engine::CredentialSlot::Secondary
        );
        let nonce = cx
            .reserve_nonce(self.claim.as_ref().unwrap())
            .map_err(|_| SignError::InvalidRequest)?;
        Ok(SignRequest {
            receipt: SignReceipt {
                nonce: Some(nonce),
                stamp_ms: Some(cx.stamp_ms()),
                expiry_ms: None,
            },
            operations: vec![Operation::Present {
                key: self.identity.credential.clone(),
            }],
            context: Default::default(),
        })
    }
    fn apply(&mut self, _: &mut Action, _: &SignResult) -> Result<(), SignError> {
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        10000.
    }
}
enum Pending {
    Read(AdmittedRead),
    Leg(ReadLeg),
}
impl Pending {
    fn context(&self) -> &Context {
        match self {
            Self::Read(r) => r.context(),
            Self::Leg(r) => r.context(),
        }
    }
}
struct Probe {
    slots: Slots<Identity>,
    mode: u8,
    leg: bool,
    root: Option<ReadRoot>,
    identities: Arc<Mutex<Vec<Identity>>>,
    failures: Vec<Error>,
    replies: usize,
}
impl Probe {
    fn queue(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending) {
        let external = self
            .slots
            .select_with_nonce(CredentialRole::Read { scope: None }, &KRAKEN)
            .ok()
            .map(|(_, _, claim)| claim);
        // Replacing venue slots cannot alter the identity already retained by SelectedRead.
        self.slots = Slots::new(
            Some(identity("replacement", Some("replacement"))),
            None,
            SlotPolicy::default(),
        );
        let request = ReadRequest {
            method: Method::Get,
            url: "https://selected.test/read".into(),
            body: None,
            stream: None,
        };
        let captured = self.identities.clone();
        let mode = self.mode;
        let result = match pending {
            Pending::Read(read) => {
                if mode == 1 {
                    read.queue(
                        cx,
                        request,
                        move |i| {
                            Box::new(Recipe {
                                identity: i.clone(),
                                claim: external,
                            })
                        },
                        Pending::Read,
                    )
                } else {
                    read.queue_with_nonce(
                        cx,
                        request,
                        &KRAKEN,
                        move |i, claim| {
                            captured.lock().unwrap().push(i.clone());
                            Box::new(Recipe {
                                identity: i.clone(),
                                claim: if mode == 2 {
                                    external
                                } else if mode == 3 {
                                    None
                                } else {
                                    Some(claim)
                                },
                            })
                        },
                        Pending::Read,
                    )
                }
            }
            Pending::Leg(read) => {
                if mode == 1 {
                    read.queue(
                        cx,
                        request,
                        move |i| {
                            Box::new(Recipe {
                                identity: i.clone(),
                                claim: external,
                            })
                        },
                        Pending::Leg,
                    )
                } else {
                    read.queue_with_nonce(
                        cx,
                        request,
                        &KRAKEN,
                        move |i, claim| {
                            captured.lock().unwrap().push(i.clone());
                            Box::new(Recipe {
                                identity: i.clone(),
                                claim: if mode == 2 {
                                    external
                                } else if mode == 3 {
                                    None
                                } else {
                                    Some(claim)
                                },
                            })
                        },
                        Pending::Leg,
                    )
                }
            }
        };
        if let Err((pending, error)) = result {
            self.failures.push(error);
            cx.fail_context(
                self.root
                    .as_ref()
                    .map_or_else(|| pending.context(), ReadRoot::context),
                error.venue_error(),
            )
            .unwrap();
        }
    }
}
impl Venue for Probe {
    #[inline]
    fn dispatch_nonce() -> bool {
        true
    }

    type Pending = Pending;
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            account: safe_cex_engine::AccountLookup::Query,
            ..safe_cex_engine::Capabilities::BASE
        }
    }
    fn http_release() -> HttpRelease<Self> {
        HttpRelease::NonceUnscheduled(|_, _, _| Ok(()))
    }
    fn start(&mut self, _: &mut Cx<'_, Pending>) {}
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, _: Command) {
        let selected = SelectedRead::select(&self.slots, ReadRole::Read { scope: None });
        let missing = LocalRefusal {
            diagnostic: None,
            error: VenueError::new("missing"),
        };
        let pending = if self.leg {
            let root = SelectedRead::command_root(cx, selected, missing)
                .unwrap_or_else(|_| panic!("valid entry"));
            let leg = root
                .leg(
                    cx,
                    ReadLegPolicy {
                        timeout_ms: 20000,
                        pacing: None,
                    },
                )
                .unwrap();
            self.root = Some(root);
            Pending::Leg(leg)
        } else {
            Pending::Read(
                SelectedRead::command(
                    cx,
                    selected,
                    ReadPolicy {
                        timeout_ms: 20000,
                        pacing: None,
                        retry: RetryPolicy::ReadFailure,
                    },
                    missing,
                )
                .unwrap_or_else(|_| panic!("valid entry")),
            )
        };
        self.queue(cx, pending);
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, _: VenueError) {
        if let Pending::Read(read) = pending {
            let (read, ()) = read
                .retry(cx, RetryEvidence::ReadFailure, ())
                .unwrap_or_else(|_| panic!("native Read retry"));
            self.queue(cx, Pending::Read(read));
        } else {
            panic!("only admitted retry control");
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, _: Response<'_>) {
        self.replies += 1;
        if let Some(root) = &self.root {
            root.validate_delivery(
                cx,
                match &pending {
                    Pending::Leg(leg) => leg,
                    _ => panic!("leg"),
                },
            )
            .unwrap();
            cx.done_context(root.context(), "ok".into()).unwrap();
        } else {
            cx.done_context(pending.context(), "ok".into()).unwrap();
        }
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, Pending>, _: Pending, _: &str) {
        panic!("typed only");
    }
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Pending>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn fixture(leg: bool, mode: u8, subject: Option<&str>) -> (Engine<Probe>, Vec<Action>) {
    let primary = identity("primary", Some("selected-primary"));
    let secondary = identity("secondary", subject);
    let mut e = safe_cex_engine::test_support::seeded_engine(Probe {
        slots: Slots::new(
            Some(primary.clone()),
            Some(secondary.clone()),
            SlotPolicy::default(),
        ),
        mode,
        leg,
        root: None,
        identities: Arc::default(),
        failures: vec![],
        replies: 0,
    });
    e.set_identities(Identities {
        primary: Some(primary),
        secondary: Some(secondary),
    })
    .unwrap();
    e.start(1000.);
    e.handle(1000., Event::Command(Command::FetchAccount { command: 7 }));
    let a = e.drain_actions().collect();
    (e, a)
}
fn work(e: &mut Engine<Probe>, actions: Vec<Action>) -> (safe_cex_engine::RequestId, SignRequest) {
    let prepare = actions
        .into_iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Prepare {
                    preparation,
                    intent,
                    ..
                } => {
                    assert_eq!(intent, RoutingIntent::Secondary);
                    Some(preparation)
                }
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    e.http_prepared(
        1000.,
        prepare,
        Prepared {
            id: prepare.attempt().request.0,
            expires_at: None,
        },
    );
    e.drain_actions()
        .find_map(|a| match a {
            Action::Sign { attempt, work, .. } => Some((attempt, work)),
            _ => None,
        })
        .unwrap()
}
fn release(
    e: &mut Engine<Probe>,
    attempt: safe_cex_engine::RequestId,
    work: SignRequest,
) -> safe_cex_engine::RequestId {
    e.handle(
        1000.,
        Event::Signed {
            attempt,
            result: Ok(SignResult::new(vec![b"credential".to_vec()], work.receipt)),
        },
    );
    let (permit, id) = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind {
                    permit, request, ..
                } => Some((permit, request.id)),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    assert!(matches!(
        e.claim_http(1000., permit),
        Ok(Claim::Released(_))
    ));
    id
}
#[test]
fn retained_secondary_snapshot_mints_for_both_wrappers_after_venue_slots_change() {
    for leg in [false, true] {
        let (mut e, actions) = fixture(
            leg,
            0,
            Some(if leg {
                "selected-nonce/leg"
            } else {
                "selected-nonce/admitted"
            }),
        );
        let (id, work) = work(&mut e, actions);
        assert_eq!(work.receipt.nonce, Some(1000000));
        assert_eq!(
            e.venue().identities.lock().unwrap()[0].credential,
            KeyRef("credential/secondary".into())
        );
        let id = release(&mut e, id, work);
        e.handle(
            1000.,
            Event::HttpResponse {
                request: id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: "{}",
                },
            },
        );
        assert_eq!(e.venue().replies, 1);
        assert!(e.venue().failures.is_empty());
        assert_eq!(
            e.drain_actions()
                .filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. })))
                .count(),
            1
        );
    }
}
#[test]
fn plain_queues_and_substituted_or_missing_claims_refuse_both_wrappers() {
    for leg in [false, true] {
        for mode in [1, 2, 3] {
            let (e, actions) = fixture(leg, mode, Some("selected-nonce/refusal"));
            assert_eq!(e.venue().failures, [Error::Invalid]);
            assert!(!actions.iter().any(|a| matches!(
                a,
                Action::ManagedHttp(_) | Action::Sign { .. } | Action::Http(_)
            )));
            assert_eq!(actions.iter().filter(|a|matches!(a,Action::Notice(Notice::CommandFailed {command:7,error}) if error.kind==Some(safe_cex_engine::ErrorKind::InvalidInput)&&error.outcome==Some(safe_cex_engine::Outcome::NotSent))).count(),1);
        }
    }
}
#[test]
fn missing_subject_refuses_before_recipe_creation_for_both_wrappers() {
    for leg in [false, true] {
        let (e, actions) = fixture(leg, 0, None);
        assert!(e.venue().identities.lock().unwrap().is_empty());
        assert_eq!(e.venue().failures, [Error::Invalid]);
        assert!(!actions.iter().any(|a| matches!(
            a,
            Action::ManagedHttp(_) | Action::Sign { .. } | Action::Http(_)
        )));
    }
}
#[test]
fn a_native_read_retry_mints_a_fresh_claim_from_the_same_retained_snapshot() {
    let (mut e, actions) = fixture(false, 0, Some("selected-nonce/retry"));
    let (attempt, first) = work(&mut e, actions);
    let nonce = first.receipt.nonce.unwrap();
    let first_signing_attempt = attempt;
    let id = release(&mut e, attempt, first);
    e.handle(
        1000.,
        Event::HttpFailed {
            request: id,
            error: "native network",
        },
    );
    let actions = e.drain_actions().collect();
    let (attempt, second) = work(&mut e, actions);
    assert!(second.receipt.nonce.unwrap() > nonce);
    assert_ne!(attempt, first_signing_attempt);
    let identities = e.venue().identities.lock().unwrap();
    assert_eq!(identities.len(), 2);
    assert_eq!(identities[0], identities[1]);
    drop(identities);
    let second_request = release(&mut e, attempt, second);
    assert_ne!(second_request, id);
}
