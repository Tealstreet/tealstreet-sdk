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
    policies: Vec<Pacing>,
    timeout: u32,
    preparations: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}
struct Recipe(std::sync::Arc<std::sync::atomic::AtomicUsize>);
impl SignStep for Recipe {
    fn prepare(
        &mut self,
        action: &mut Action,
        cx: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let Action::Http(request) = action else {
            return Err(SignError::InvalidRequest);
        };
        request.url = format!("https://pacing.test/read?stamp={}", cx.stamp_ms());
        Ok(SignRequest {
            receipt: SignReceipt {
                nonce: None,
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
        &[GLOBAL, ALGO]
    }
    fn http_pacing_sets(&self) -> &[safe_cex_engine::request_context::PacingSet] {
        &SETS
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        let policies = if self.policies.is_empty() {
            vec![GLOBAL; 2]
        } else {
            self.policies.clone()
        };
        for pacing in policies {
            let context = cx
                .http_context(
                    Scope::Background,
                    Policy {
                        operation: Operation::Read,
                        credential: Some(Credential {
                            slot: CredentialSlot::Primary,
                            reference: KeyRef("fixture".into()),
                        }),
                        timeout_ms: if self.timeout == 0 {
                            1000
                        } else {
                            self.timeout
                        },
                        pacing: Some(pacing),
                        retry: RetryPolicy::Never,
                    },
                )
                .unwrap();
            cx.request_signed_managed(
                context.clone(),
                None,
                Method::Post,
                "https://pacing.test/read".into(),
                None,
                Box::new(Recipe(self.preparations.clone())),
                Pending(context),
            )
            .unwrap();
        }
    }
    fn http_release() -> safe_cex_engine::HttpRelease<Self> {
        safe_cex_engine::HttpRelease::Mutable(|this, _: &mut Pending, _: &HttpRelease| {
            this.releases += 1;
            Ok(Admission::Unscheduled)
        })
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
    start_with(Probe::default())
}
fn start_with(probe: Probe) -> (Engine<Probe>, Vec<Preparation>) {
    let mut e = safe_cex_engine::test_support::seeded_engine(probe);
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

use safe_cex_engine::request_context::{
    Attempt, Lifecycle, Owner, PacingSet, ReleaseTime, SendPacing,
};
const ALGO: Pacing = Pacing {
    group: 2,
    minimum_gap_ms: 500.,
};
const BOTH: Pacing = Pacing {
    group: 10,
    minimum_gap_ms: 0.,
};
const ALIAS: Pacing = Pacing {
    group: 11,
    minimum_gap_ms: 0.,
};
const SETS: [PacingSet; 2] = [
    PacingSet {
        group: 10,
        constraints: &[GLOBAL, ALGO],
    },
    PacingSet {
        group: 11,
        constraints: &[GLOBAL, ALGO],
    },
];
fn registry() -> SendPacing {
    SendPacing::with_sets(&[GLOBAL, ALGO], &SETS).unwrap()
}
fn life(id: u64, pacing: Pacing, deadline: f64) -> (Lifecycle, Permit) {
    let cx = Context::new(
        Owner::Command {
            id: 7,
            generation: 1,
        },
        id,
        Operation::Mutation,
        None,
        0.,
        deadline,
        Some(pacing),
    )
    .unwrap();
    let mut life = Lifecycle::new(
        cx,
        Attempt {
            request: RequestId(id),
            generation: 1,
        },
    );
    life.prepared(
        life.preparation(),
        Prepared {
            id,
            expires_at: None,
        },
    )
    .unwrap();
    life.unsigned().unwrap();
    let permit = life.permit().unwrap();
    (life, permit)
}
fn claim(
    life: &mut Lifecycle,
    permit: Permit,
    now: f64,
    registry: &mut SendPacing,
) -> Result<Claim, Error> {
    life.claim(
        permit,
        ReleaseTime {
            raw_ms: now,
            protocol_ms: now,
            revision: 0,
        },
        registry,
        &mut |_| Ok(Admission::Unscheduled),
    )
}
#[test]
fn multi_sets_preserve_woo_algo_ordinary_algo_and_put_clocks() {
    let mut r = registry();
    let (mut a, pa) = life(1, BOTH, 2000.);
    assert!(matches!(
        claim(&mut a, pa, 0., &mut r),
        Ok(Claim::Released(_))
    ));
    let (mut ordinary, po) = life(2, GLOBAL, 2000.);
    assert_eq!(
        claim(&mut ordinary, po, 99., &mut r),
        Ok(Claim::WaitUntil(100.))
    );
    assert!(matches!(
        claim(&mut ordinary, po, 100., &mut r),
        Ok(Claim::Released(_))
    ));
    let (mut again, pb) = life(3, ALIAS, 2000.);
    assert_eq!(
        claim(&mut again, pb, 200., &mut r),
        Ok(Claim::WaitUntil(500.))
    );
    assert!(matches!(
        claim(&mut again, pb, 500., &mut r),
        Ok(Claim::Released(_))
    ));
    let (mut put, pp) = life(
        4,
        Pacing {
            minimum_gap_ms: 200.,
            ..GLOBAL
        },
        2000.,
    );
    assert!(matches!(
        claim(&mut put, pp, 600., &mut r),
        Ok(Claim::Released(_))
    ));
    let (mut next, pn) = life(5, GLOBAL, 2000.);
    assert_eq!(
        claim(&mut next, pn, 700., &mut r),
        Ok(Claim::WaitUntil(800.))
    );
    assert!(matches!(
        claim(&mut next, pn, 800., &mut r),
        Ok(Claim::Released(_))
    ));
}
#[test]
fn a_second_constraint_wait_never_partially_charges_the_first() {
    let mut r = registry();
    let (mut algo, pa) = life(1, ALGO, 2000.);
    assert!(matches!(
        claim(&mut algo, pa, 0., &mut r),
        Ok(Claim::Released(_))
    ));
    let (mut both, pb) = life(2, BOTH, 2000.);
    let mut admissions = 0;
    assert_eq!(
        both.claim(
            pb,
            ReleaseTime {
                raw_ms: 0.,
                protocol_ms: 0.,
                revision: 0
            },
            &mut r,
            &mut |_| {
                admissions += 1;
                Ok(Admission::Unscheduled)
            }
        ),
        Ok(Claim::WaitUntil(500.))
    );
    assert_eq!(admissions, 0);
    let (mut global, pg) = life(3, GLOBAL, 2000.);
    assert!(matches!(
        claim(&mut global, pg, 0., &mut r),
        Ok(Claim::Released(_))
    ));
}
#[test]
fn denied_or_fallible_admission_charges_neither_primitive() {
    for err in [false, true] {
        let mut r = registry();
        let (mut both, pb) = life(1, BOTH, 2000.);
        let mut calls = 0;
        let result = both.claim(
            pb,
            ReleaseTime {
                raw_ms: 0.,
                protocol_ms: 0.,
                revision: 0,
            },
            &mut r,
            &mut |_| {
                calls += 1;
                if err {
                    Err(Error::Capacity)
                } else {
                    Ok(Admission::Denied)
                }
            },
        );
        assert_eq!(
            result,
            Err(if err { Error::Capacity } else { Error::Revoked })
        );
        assert_eq!(calls, 1);
        for (id, pacing) in [(2, GLOBAL), (3, ALGO)] {
            let (mut next, permit) = life(id, pacing, 2000.);
            assert!(matches!(
                claim(&mut next, permit, 0., &mut r),
                Ok(Claim::Released(_))
            ));
        }
    }
}
#[test]
fn member_floor_outer_gap_and_shared_aliases_use_the_same_primitive_state() {
    let sets = [
        PacingSet {
            group: 10,
            constraints: &[
                Pacing {
                    group: 1,
                    minimum_gap_ms: 150.,
                },
                ALGO,
            ],
        },
        SETS[1],
    ];
    let mut r = SendPacing::with_sets(&[GLOBAL, ALGO], &sets).unwrap();
    let (mut both, pb) = life(
        1,
        Pacing {
            minimum_gap_ms: 200.,
            ..BOTH
        },
        2000.,
    );
    assert!(matches!(
        claim(&mut both, pb, 0., &mut r),
        Ok(Claim::Released(_))
    ));
    let (mut plain, pp) = life(2, GLOBAL, 2000.);
    assert_eq!(
        claim(&mut plain, pp, 150., &mut r),
        Ok(Claim::WaitUntil(200.))
    );
    let (mut alias, pa) = life(3, ALIAS, 2000.);
    assert_eq!(
        claim(&mut alias, pa, 200., &mut r),
        Ok(Claim::WaitUntil(500.))
    );
}
#[test]
fn expanded_zero_validation_refuses_outer_zero_over_nonzero_members() {
    let r = registry();
    assert_eq!(r.validate_zero(Some(BOTH)), Err(Error::Invalid));
    assert_eq!(
        r.validate_zero(Some(Pacing {
            minimum_gap_ms: 0.,
            ..GLOBAL
        })),
        Err(Error::Invalid)
    );
    assert_eq!(r.validate_zero(None), Ok(()));
    assert_eq!(
        r.validate_zero(Some(Pacing {
            group: 99,
            minimum_gap_ms: 0.
        })),
        Err(Error::Invalid)
    );
    let r = SendPacing::with_sets(
        &[Pacing {
            group: 1,
            minimum_gap_ms: 0.,
        }],
        &[PacingSet {
            group: 10,
            constraints: &[Pacing {
                group: 1,
                minimum_gap_ms: 0.,
            }],
        }],
    )
    .unwrap();
    assert_eq!(r.validate_zero(Some(BOTH)), Ok(()));
    assert_eq!(
        r.validate_zero(Some(Pacing {
            minimum_gap_ms: 1.,
            ..BOTH
        })),
        Err(Error::Invalid)
    );
}
#[test]
fn invalid_or_nested_duplicate_colliding_and_oversized_sets_never_install() {
    for (sets, error) in [
        (
            vec![PacingSet {
                group: 10,
                constraints: &[],
            }],
            Error::Invalid,
        ),
        (
            vec![PacingSet {
                group: 1,
                constraints: &[GLOBAL],
            }],
            Error::Duplicate,
        ),
        (vec![SETS[0], SETS[0]], Error::Duplicate),
        (
            vec![PacingSet {
                group: 10,
                constraints: &[GLOBAL, GLOBAL],
            }],
            Error::Duplicate,
        ),
        (
            vec![PacingSet {
                group: 10,
                constraints: &[Pacing {
                    group: 99,
                    minimum_gap_ms: 0.,
                }],
            }],
            Error::Invalid,
        ),
        (
            vec![
                PacingSet {
                    group: 12,
                    constraints: &[BOTH],
                },
                SETS[0],
            ],
            Error::Invalid,
        ),
        (
            vec![PacingSet {
                group: 10,
                constraints: &[Pacing {
                    group: 1,
                    minimum_gap_ms: f64::NAN,
                }],
            }],
            Error::Invalid,
        ),
        (
            vec![PacingSet {
                group: 10,
                constraints: &[Pacing {
                    group: 1,
                    minimum_gap_ms: -1.,
                }],
            }],
            Error::Invalid,
        ),
        (
            vec![PacingSet {
                group: 10,
                constraints: &[GLOBAL; 17],
            }],
            Error::Capacity,
        ),
        (vec![SETS[0]; 17], Error::Capacity),
    ] {
        assert!(matches!(SendPacing::with_sets(&[GLOBAL,ALGO],&sets),Err(found) if found==error));
    }
}
#[test]
fn a_multi_constraint_wait_cannot_extend_or_spend_the_raw_deadline() {
    let mut r = registry();
    let (mut a, pa) = life(1, BOTH, 2000.);
    assert!(matches!(
        claim(&mut a, pa, 0., &mut r),
        Ok(Claim::Released(_))
    ));
    let (mut too_short, p) = life(2, ALIAS, 500.);
    assert_eq!(claim(&mut too_short, p, 100., &mut r), Err(Error::TimedOut));
    assert_eq!(too_short.deadline(), 500.);
    let (mut fresh, p) = life(3, ALIAS, 2000.);
    assert!(matches!(
        claim(&mut fresh, p, 500., &mut r),
        Ok(Claim::Released(_))
    ));
}
fn wake(out: &[Action], at: f64) -> safe_cex_engine::TimerId {
    out.iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == at => Some(*timer),
            _ => None,
        })
        .expect("bounded pacing wake")
}
#[test]
fn early_and_duplicate_wakes_preserve_resource_then_sign_once_at_eligibility() {
    let (mut e, tokens) = start();
    let (attempt, input) = work(&route(&mut e, tokens[0], 0.));
    let permit = signed(&mut e, 0., attempt, input);
    assert!(matches!(e.claim_http(0., permit), Ok(Claim::Released(_))));
    let out = route(&mut e, tokens[1], 10.);
    let timer = wake(&out, 100.);
    e.handle(50., Event::TimerFired { timer });
    let out: Vec<_> = e.drain_actions().collect();
    assert_eq!(wake(&out, 100.), timer);
    assert!(!out.iter().any(|a| matches!(a, Action::Sign { .. })));
    e.handle(100., Event::TimerFired { timer });
    let out: Vec<_> = e.drain_actions().collect();
    let (attempt, input) = work(&out);
    assert_eq!(input.receipt.stamp_ms, Some(100.));
    assert_eq!(wake(&out, 1000.), timer);
    e.handle(100., Event::TimerFired { timer });
    assert!(!e.drain_actions().any(|a| matches!(a, Action::Sign { .. })));
    let permit = signed(&mut e, 101., attempt, input);
    assert!(matches!(e.claim_http(101., permit), Ok(Claim::Released(_))));
}
#[test]
fn waiting_route_expiry_reprepares_without_extending_the_physical_bound() {
    let (mut e, tokens) = start();
    let (attempt, input) = work(&route(&mut e, tokens[0], 0.));
    let permit = signed(&mut e, 0., attempt, input);
    assert!(matches!(e.claim_http(0., permit), Ok(Claim::Released(_))));
    e.http_prepared(
        10.,
        tokens[1],
        Prepared {
            id: 77,
            expires_at: Some(50.),
        },
    );
    let out: Vec<_> = e.drain_actions().collect();
    let timer = wake(&out, 50.);
    e.http_prepared(
        20.,
        tokens[1],
        Prepared {
            id: 88,
            expires_at: None,
        },
    );
    assert!(!e.drain_actions().any(|a| matches!(a, Action::Sign { .. })));
    e.handle(50., Event::TimerFired { timer });
    let out: Vec<_> = e.drain_actions().collect();
    let fresh = out
        .iter()
        .find_map(|a| match a {
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => {
                    assert_eq!(request.timeout_ms, Some(950));
                    Some(*preparation)
                }
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    assert_ne!(fresh, tokens[1]);
    assert_eq!(fresh.attempt(), tokens[1].attempt());
    assert!(route(&mut e, tokens[1], 60.).is_empty());
    let out = route(&mut e, fresh, 60.);
    assert!(!out.iter().any(|a| matches!(a, Action::Sign { .. })));
    e.handle(100., Event::TimerFired { timer });
    let out: Vec<_> = e.drain_actions().collect();
    let (_, input) = work(&out);
    assert_eq!(input.receipt.stamp_ms, Some(100.));
}
#[test]
fn pacing_wait_deadline_equality_fails_before_provider_and_releases_custody() {
    let (mut e, tokens) = start_with(Probe {
        timeout: 100,
        ..Default::default()
    });
    let (attempt, input) = work(&route(&mut e, tokens[0], 0.));
    let permit = signed(&mut e, 0., attempt, input);
    assert!(matches!(e.claim_http(0., permit), Ok(Claim::Released(_))));
    let out = route(&mut e, tokens[1], 10.);
    let timer = wake(&out, 100.);
    e.handle(100., Event::TimerFired { timer });
    let out: Vec<_> = e.drain_actions().collect();
    assert!(!out.iter().any(|a| matches!(a, Action::Sign { .. })));
    assert_eq!(e.venue().failures, [Error::TimedOut]);
    e.handle(101., Event::TimerFired { timer });
    assert!(e.drain_actions().next().is_none());
}
#[test]
fn delayed_claim_rechecks_the_full_set_after_another_primitive_release() {
    let (mut e, tokens) = start_with(Probe {
        policies: vec![GLOBAL, BOTH, GLOBAL],
        ..Default::default()
    });
    let (a, w) = work(&route(&mut e, tokens[0], 0.));
    let p = signed(&mut e, 0., a, w);
    assert!(matches!(e.claim_http(0., p), Ok(Claim::Released(_))));
    let (a, w) = work(&route(&mut e, tokens[1], 100.));
    let held = signed(&mut e, 100., a, w);
    let (a, w) = work(&route(&mut e, tokens[2], 150.));
    let p = signed(&mut e, 150., a, w);
    assert!(matches!(e.claim_http(150., p), Ok(Claim::Released(_))));
    assert_eq!(e.claim_http(160., held), Ok(Claim::WaitUntil(250.)));
    assert!(matches!(e.claim_http(250., held), Ok(Claim::Released(_))));
    assert_eq!(e.venue().releases, 3);
}

#[test]
fn expanded_algo_wait_blocks_recipe_preparation_until_both_clocks_are_ready() {
    let (mut e, tokens) = start_with(Probe {
        policies: vec![BOTH, GLOBAL, ALIAS],
        ..Default::default()
    });
    let (a, w) = work(&route(&mut e, tokens[0], 0.));
    let p = signed(&mut e, 0., a, w);
    assert!(matches!(e.claim_http(0., p), Ok(Claim::Released(_))));
    let (a, w) = work(&route(&mut e, tokens[1], 100.));
    let p = signed(&mut e, 100., a, w);
    assert!(matches!(e.claim_http(100., p), Ok(Claim::Released(_))));
    let out = route(&mut e, tokens[2], 200.);
    let timer = wake(&out, 500.);
    assert_eq!(
        e.venue()
            .preparations
            .load(std::sync::atomic::Ordering::SeqCst),
        2,
        "blocked recipe cannot draw entropy or stamp before the algo floor"
    );
    assert!(!out.iter().any(|a| matches!(a, Action::Sign { .. })));
    e.handle(500., Event::TimerFired { timer });
    let out: Vec<_> = e.drain_actions().collect();
    let (a, w) = work(&out);
    assert_eq!(w.receipt.stamp_ms, Some(500.));
    assert_eq!(
        e.venue()
            .preparations
            .load(std::sync::atomic::Ordering::SeqCst),
        3
    );
    let p = signed(&mut e, 500., a, w);
    assert!(matches!(e.claim_http(500., p), Ok(Claim::Released(_))));
}
