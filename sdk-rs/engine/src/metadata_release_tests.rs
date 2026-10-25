//! Metadata's release contract against the actual shared scheduler/lifecycle.
//! Private adapters remain legacy until signer and observer-group adoption.
use super::*;
use crate::read_scheduler::{self as reads, Scheduler};
use crate::request_context as http;
use crate::{
    Action, Cx, Engine, Event, Method, Notice, RequestId, Response, SocketId, TimerId, Venue,
};

fn policy() -> reads::Policy {
    reads::Policy {
        debounce_ms: 0.,
        refresh_debounce_ms: 0.,
        admission_timeout_ms: 50.,
        response_timeout_ms: 100.,
        response_expiry: reads::ExpiryPolicy::Retry,
        retry: reads::RetryPolicy::default(),
    }
}
fn token(store: &mut crate::Store, _command: u32) -> Token {
    store
        .begin_metadata("BTC", true, Fallback::CompleteSetting)
        .unwrap()
}

fn reserve() -> (crate::Store, Token, Scheduler<Token, u8>, reads::Attempt) {
    let mut store = crate::Store::default();
    let key = token(&mut store, 7);
    let mut scheduler = Scheduler::new(2, vec![0]).unwrap();
    scheduler.register(key.clone(), &0, policy()).unwrap();
    scheduler.request(&key, 0., reads::Demand::Refresh).unwrap();
    let (reserved, attempt) = scheduler
        .take_ready(0., |_| panic!("no retry draw"))
        .unwrap()
        .unwrap();
    assert_eq!(reserved, &key);
    (store, key, scheduler, attempt)
}
fn release(raw: f64) -> http::HttpRelease {
    http::HttpRelease {
        attempt: http::Attempt {
            request: RequestId(1),
            generation: 1,
        },
        time: http::ReleaseTime {
            raw_ms: raw,
            protocol_ms: raw + 100_000.,
            revision: 3,
        },
        signature: Default::default(),
        deadline: 1_000.,
        response_lease: None,
        signature_valid_until: None,
    }
}

#[test]
fn queued_metadata_has_no_response_lease_and_continuations_do_not_renew() {
    let (_, _, mut scheduler, attempt) = reserve();
    assert_eq!(scheduler.next_deadline(), Some(50.));
    let first = release(20.);
    let expected = http::Admission::Scheduled(first.response_lease(120.).unwrap());
    assert_eq!(scheduler.admit_http(attempt, &first).unwrap(), expected);
    assert_eq!(scheduler.next_deadline(), Some(120.));
    assert_eq!(
        scheduler.admit_http(attempt, &release(70.)).unwrap(),
        expected
    );
    assert_eq!(scheduler.next_deadline(), Some(120.));
    assert_eq!(
        scheduler.admit_http(attempt, &release(120.)).unwrap(),
        http::Admission::Denied
    );
}
#[test]
fn admission_expiry_denies_without_turning_scheduled_metadata_into_unscheduled() {
    let (_, _, mut scheduler, attempt) = reserve();
    assert_eq!(
        scheduler.admit_http(attempt, &release(50.)).unwrap(),
        http::Admission::Denied
    );
    let (_, _, mut before, active) = reserve();
    assert!(matches!(
        before.admit_http(active, &release(49.)).unwrap(),
        http::Admission::Scheduled(_)
    ));
    assert_eq!(before.next_deadline(), Some(149.));
}
#[test]
fn pause_before_release_revokes_only_the_old_reservation() {
    let (_, key, mut scheduler, old) = reserve();
    scheduler.pause(&0, 200.).unwrap();
    assert_eq!(
        scheduler.admit_http(old, &release(10.)).unwrap(),
        http::Admission::Denied
    );
    assert!(
        scheduler
            .take_ready(199., |_| panic!("denial is not network failure"))
            .unwrap()
            .is_none()
    );
    let (ready, current) = scheduler
        .take_ready(200., |_| panic!("no retry draw"))
        .unwrap()
        .unwrap();
    assert_eq!(ready, &key);
    assert_ne!(current, old);
    assert_eq!(
        scheduler.admit_http(old, &release(200.)).unwrap(),
        http::Admission::Denied
    );
    assert!(matches!(
        scheduler.admit_http(current, &release(200.)).unwrap(),
        http::Admission::Scheduled(_)
    ));
}
#[test]
fn pause_after_release_blocks_a_new_leg_but_not_the_existing_response() {
    let (_, _, mut scheduler, attempt) = reserve();
    assert!(matches!(
        scheduler.admit_http(attempt, &release(10.)).unwrap(),
        http::Admission::Scheduled(_)
    ));
    scheduler.pause(&0, 200.).unwrap();
    assert_eq!(
        scheduler.admit_http(attempt, &release(20.)).unwrap(),
        http::Admission::Denied
    );
    assert!(
        scheduler
            .finish(attempt, 30., true, |_| panic!("successful response"))
            .unwrap()
    );
}
#[test]
fn retired_and_recreated_metadata_key_never_revives_an_old_attempt() {
    let (_, key, mut scheduler, old) = reserve();
    scheduler.cancel(&key).unwrap();
    scheduler.retire(&key).unwrap();
    scheduler.register(key.clone(), &0, policy()).unwrap();
    scheduler.request(&key, 1., reads::Demand::Refresh).unwrap();
    let (_, current) = scheduler
        .take_ready(1., |_| panic!("neutral cleanup"))
        .unwrap()
        .unwrap();
    assert_ne!(old, current);
    assert_eq!(
        scheduler.admit_http(old, &release(2.)).unwrap(),
        http::Admission::Denied
    );
    assert!(matches!(
        scheduler.admit_http(current, &release(2.)).unwrap(),
        http::Admission::Scheduled(_)
    ));
}
#[test]
fn invalid_release_times_do_not_consume_first_release_or_draw_retry_entropy() {
    let (_, _, mut scheduler, attempt) = reserve();
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            scheduler.admit_http(attempt, &release(bad)),
            Err(reads::Error::InvalidTime)
        );
        assert_eq!(scheduler.next_deadline(), Some(50.));
    }
    assert!(matches!(
        scheduler.admit_http(attempt, &release(10.)).unwrap(),
        http::Admission::Scheduled(_)
    ));
    assert_eq!(scheduler.next_deadline(), Some(110.));
}
#[test]
fn unrepresentable_admission_budget_fails_before_reserving_or_retrying() {
    let mut store = crate::Store::default();
    let key = token(&mut store, 8);
    let mut scheduler = Scheduler::new(1, vec![0]).unwrap();
    scheduler.register(key.clone(), &0, policy()).unwrap();
    scheduler
        .request(&key, f64::MAX, reads::Demand::Refresh)
        .unwrap();
    assert!(matches!(
        scheduler.take_ready(f64::MAX, |_| panic!("no retry on arithmetic refusal")),
        Err(reads::Error::InvalidTime)
    ));
    scheduler.cancel(&key).unwrap();
    scheduler.request(&key, 0., reads::Demand::Refresh).unwrap();
    assert!(
        scheduler
            .take_ready(0., |_| panic!("no retry draw"))
            .unwrap()
            .is_some()
    );
}
#[test]
fn publication_supersession_does_not_revoke_read_liveness_or_fallback() {
    let (mut store, key, mut scheduler, attempt) = reserve();
    store.invalidate_metadata(Some("BTC"));
    assert!(!store.metadata_eligible(&key));
    assert!(matches!(
        scheduler.admit_http(attempt, &release(10.)).unwrap(),
        http::Admission::Scheduled(_)
    ));
    let done = store.finish_metadata(&key, Ok(proposal())).unwrap();
    assert!(!done.published);
    assert_eq!(done.result().as_ref().unwrap().buy, 7.);
}
fn proposal() -> Proposal {
    Proposal::complete(
        Snapshot::new(crate::store::MarketSetting {
            leverage: 7.,
            buy_leverage: Some(7.),
            sell_leverage: Some(9.),
            margin_type: Some(crate::model::MarginType::Cross),
        })
        .unwrap(),
    )
}

// A single public command exercises the production Engine preparation/claim
// path. This grants no cross-owner completion or private-signing capability.
#[derive(Debug)]
struct Pending {
    root: http::Context,
    context: http::Context,
    token: Token,
    attempt: reads::Attempt,
    second_leg: bool,
}
struct PublicMetadata {
    scheduler: Scheduler<Token, u8>,
    pause_on_release: bool,
    second_leg: bool,
    failures: Vec<(http::Error, Option<Outcome>)>,
    responses: usize,
    root_mode: u8,
    rejected_deliveries: usize,
}
impl PublicMetadata {
    fn new(pause_on_release: bool) -> Self {
        Self {
            scheduler: Scheduler::new(1, vec![0]).unwrap(),
            pause_on_release,
            second_leg: false,
            failures: vec![],
            responses: 0,
            root_mode: 0,
            rejected_deliveries: 0,
        }
    }
}
impl Venue for PublicMetadata {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        let token = token(cx.store, 7);
        self.scheduler
            .register(token.clone(), &0, policy())
            .unwrap();
        self.scheduler
            .request(&token, cx.now, reads::Demand::Refresh)
            .unwrap();
        let (_, attempt) = self
            .scheduler
            .take_ready(cx.now, |_| panic!("new read"))
            .unwrap()
            .unwrap();
        let context = cx
            .http_context(
                http::Scope::Command(7),
                http::Policy {
                    operation: http::Operation::Read,
                    credential: None,
                    timeout_ms: 1_000,
                    pacing: None,
                    retry: http::RetryPolicy::Never,
                },
            )
            .unwrap();
        let pending = Pending {
            root: context.clone(),
            context: context.clone(),
            token,
            attempt,
            second_leg: self.second_leg,
        };
        cx.request_managed(
            context,
            None,
            Method::Get,
            "https://metadata.invalid/public".into(),
            None,
            pending,
        )
        .unwrap();
    }
    fn on_http_release(
        &mut self,
        pending: &mut Pending,
        release: &http::HttpRelease,
    ) -> Result<http::Admission, http::Error> {
        if self.pause_on_release {
            self.scheduler
                .pause(&0, release.time.raw_ms + 1_000.)
                .unwrap();
        }
        self.scheduler
            .admit_http(pending.attempt, release)
            .map_err(|_| http::Error::Invalid)
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        reason: http::Error,
        error: VenueError,
    ) {
        self.failures.push((reason, error.outcome));
        self.scheduler.retire(&pending.token).unwrap();
        let done = cx
            .store
            .finish_metadata(&pending.token, Err(error.clone()))
            .unwrap();
        assert!(done.result().is_err());
        cx.fail_context(&pending.root, error).unwrap();
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, _: Response<'_>) {
        // Read authority is distinct from a still-current metadata token. Gate
        // before either another leg or the synchronous publication/completion.
        if cx
            .validate_read_http_delivery(&pending.root, &pending.context)
            .is_err()
        {
            self.rejected_deliveries += 1;
            return;
        }
        self.responses += 1;
        if pending.second_leg {
            let context = cx
                .http_leg_context(
                    &pending.context,
                    http::Policy {
                        operation: http::Operation::Read,
                        credential: None,
                        timeout_ms: 1_000,
                        pacing: None,
                        retry: http::RetryPolicy::Never,
                    },
                )
                .unwrap();
            assert_eq!(context.deadline(), pending.context.deadline());
            // Test-only root substitutions model an adapter accidentally losing
            // its original logical root or confusing another live command.
            let root = match self.root_mode {
                1 => context.clone(),
                2 => cx
                    .http_context(
                        http::Scope::Command(8),
                        http::Policy {
                            operation: http::Operation::Read,
                            credential: None,
                            timeout_ms: 1_000,
                            pacing: None,
                            retry: http::RetryPolicy::Never,
                        },
                    )
                    .unwrap(),
                _ => pending.root.clone(),
            };
            let next = Pending {
                root,
                context: context.clone(),
                second_leg: false,
                ..pending
            };
            cx.request_managed(
                context,
                None,
                Method::Get,
                "https://metadata.invalid/public-margin".into(),
                None,
                next,
            )
            .unwrap();
            return;
        }
        assert!(
            self.scheduler
                .finish(pending.attempt, cx.now, true, |_| panic!("success"))
                .unwrap()
        );
        self.scheduler.retire(&pending.token).unwrap();
        let done = cx
            .store
            .finish_metadata(&pending.token, Ok(proposal()))
            .unwrap();
        assert!(done.result().is_ok());
        assert!(done.published);
        cx.done_context(&pending.root, "{}".into()).unwrap();
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, Pending>, _: Pending, _: &str) {
        panic!("denial must stay typed preparation failure")
    }
    fn on_command(&mut self, _: &mut Cx<'_, Pending>, _: crate::Command) {}
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Pending>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&crate::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &crate::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn public_request(deny: bool) -> (Engine<PublicMetadata>, http::Permit) {
    public_request_with(deny, false)
}
fn public_request_with(deny: bool, second_leg: bool) -> (Engine<PublicMetadata>, http::Permit) {
    public_request_with_root(deny, second_leg, 0)
}
fn public_request_with_root(
    deny: bool,
    second_leg: bool,
    root_mode: u8,
) -> (Engine<PublicMetadata>, http::Permit) {
    let mut venue = PublicMetadata::new(deny);
    venue.second_leg = second_leg;
    venue.root_mode = root_mode;
    let mut engine = crate::test_support::seeded_engine(venue);
    engine.start(0.);
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::Sign { .. }))
    );
    let preparation = actions
        .into_iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                http::HttpAction::Prepare {
                    preparation,
                    intent,
                    request,
                } => {
                    assert_eq!(intent, http::RoutingIntent::Public);
                    assert!(request.headers.is_empty() && request.signer_headers.is_empty());
                    Some(preparation)
                }
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    engine.http_prepared(
        10.,
        preparation,
        http::Prepared {
            id: 1,
            expires_at: None,
        },
    );
    let permit = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                http::HttpAction::Bind { permit, .. } => Some(permit),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    (engine, permit)
}
#[test]
fn unsigned_metadata_uses_managed_prepare_bind_release_and_owned_completion() {
    let (mut engine, permit) = public_request(false);
    let http::Claim::Released(release) = engine.claim_http(20., permit).unwrap() else {
        panic!("ready release")
    };
    assert_eq!(release.deadline, 1_000.);
    assert_eq!(release.response_lease, Some(120.));
    engine.handle(
        30.,
        Event::HttpResponse {
            request: release.attempt.request,
            response: Response {
                status: 200,
                headers: "",
                body: "{}",
            },
        },
    );
    assert_eq!(engine.venue().responses, 1);
    assert_eq!(
        engine.store().setting("BTC").unwrap().buy_leverage,
        Some(7.)
    );
    assert_eq!(
        engine
            .drain_actions()
            .filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. })))
            .count(),
        1
    );
}
fn assert_delivered_unsent_denial(actions: &[Action]) {
    let errors = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].kind, Some(ErrorKind::InvalidInput));
    assert_eq!(errors[0].outcome, Some(Outcome::NotSent));
    assert!(!actions.iter().any(|a| match a {
        Action::Http(_) | Action::Sign { .. } | Action::Notice(Notice::CommandDone { .. }) => true,
        Action::ManagedHttp(a) => matches!(
            a.as_ref(),
            http::HttpAction::Prepare { .. } | http::HttpAction::Bind { .. }
        ),
        _ => false,
    }));
}
#[test]
fn unsigned_metadata_scheduler_denial_never_releases_or_publishes_or_succeeds() {
    let (mut engine, permit) = public_request(true);
    assert_eq!(engine.claim_http(20., permit), Err(http::Error::Revoked));
    assert_eq!(
        engine.venue().failures,
        [(http::Error::Revoked, Some(Outcome::NotSent))]
    );
    assert_eq!(engine.venue().responses, 0);
    assert!(engine.store().setting("BTC").is_none());
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert_delivered_unsent_denial(&actions);
    assert_eq!(engine.claim_http(21., permit), Err(http::Error::Revoked));
    assert!(!engine.drain_actions().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandFailed { .. } | Notice::CommandDone { .. })
    )));
}

#[test]
fn unsigned_metadata_second_leg_keeps_the_original_context_and_scheduler_bounds() {
    for second_release in [80., 120.] {
        let (mut engine, first) = public_request_with(false, true);
        let http::Claim::Released(release) = engine.claim_http(20., first).unwrap() else {
            panic!("first release")
        };
        engine.handle(
            30.,
            Event::HttpResponse {
                request: release.attempt.request,
                response: Response {
                    status: 200,
                    headers: "",
                    body: "{}",
                },
            },
        );
        assert!(engine.store().setting("BTC").is_none());
        let preparation = engine
            .drain_actions()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match *a {
                    http::HttpAction::Prepare { preparation, .. } => Some(preparation),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        engine.http_prepared(
            70.,
            preparation,
            http::Prepared {
                id: 2,
                expires_at: None,
            },
        );
        let permit = engine
            .drain_actions()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match *a {
                    http::HttpAction::Bind { permit, .. } => Some(permit),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        if second_release == 80. {
            let http::Claim::Released(release) = engine.claim_http(second_release, permit).unwrap()
            else {
                panic!("second release")
            };
            assert_eq!(release.deadline, 1_000.);
            assert_eq!(release.response_lease, Some(120.));
            engine.handle(
                90.,
                Event::HttpResponse {
                    request: release.attempt.request,
                    response: Response {
                        status: 200,
                        headers: "",
                        body: "{}",
                    },
                },
            );
            assert!(engine.store().setting("BTC").is_some());
            assert_eq!(engine.venue().responses, 2);
            assert_eq!(
                engine
                    .drain_actions()
                    .filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. })))
                    .count(),
                1
            );
        } else {
            assert_eq!(
                engine.claim_http(second_release, permit),
                Err(http::Error::Revoked)
            );
            assert!(engine.store().setting("BTC").is_none());
            assert_eq!(
                engine.venue().failures,
                [(http::Error::Revoked, Some(Outcome::NotSent))]
            );
            assert_eq!(engine.venue().responses, 1);
            let actions = engine.drain_actions().collect::<Vec<_>>();
            assert_delivered_unsent_denial(&actions);
        }
    }
}

#[test]
fn paired_metadata_rejects_child_or_unrelated_root_before_fresh_store_publication() {
    for root_mode in [1, 2] {
        let (mut engine, first) = public_request_with_root(false, true, root_mode);
        let http::Claim::Released(first) = engine.claim_http(20., first).unwrap() else {
            panic!("first release")
        };
        engine.handle(
            30.,
            Event::HttpResponse {
                request: first.attempt.request,
                response: Response {
                    status: 200,
                    headers: "",
                    body: "{}",
                },
            },
        );
        assert!(engine.store().setting("BTC").is_none());
        let preparation = engine
            .drain_actions()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match *a {
                    http::HttpAction::Prepare { preparation, .. } => Some(preparation),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        engine.http_prepared(
            70.,
            preparation,
            http::Prepared {
                id: 2,
                expires_at: None,
            },
        );
        let permit = engine
            .drain_actions()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match *a {
                    http::HttpAction::Bind { permit, .. } => Some(permit),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        let http::Claim::Released(second) = engine.claim_http(80., permit).unwrap() else {
            panic!("second release")
        };
        engine.handle(
            90.,
            Event::HttpResponse {
                request: second.attempt.request,
                response: Response {
                    status: 200,
                    headers: "",
                    body: "{}",
                },
            },
        );
        assert!(
            engine.store().setting("BTC").is_none(),
            "invalid root cannot publish even a current metadata proposal"
        );
        assert_eq!(engine.venue().rejected_deliveries, 1);
        assert!(!engine.drain_actions().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
        )));
    }
}
