//! Venue release domains (docs/plans/RUST_RELEASE_DOMAINS.md): one FIFO of
//! managed REST releases in acceptance order, each at least the registered gap
//! after the previous member's final release.
use super::*;
use crate::book::Book;
use crate::request_context::{
    Claim, Context, DomainMembers, HttpAction, Operation, Pacing, Policy, Preparation, Prepared,
    ReleaseDomain, RetryPolicy, Scope,
};
use crate::{Action, Command, Engine, Response, SocketId, TimerId, Venue};

const GAP: f64 = 334.;
const GROUP: Pacing = Pacing {
    group: 1,
    minimum_gap_ms: GAP,
};
const ZERO: Pacing = Pacing {
    group: 3,
    minimum_gap_ms: 0.,
};
// WOO: one FIFO over ordinary POST (the shared 100 ms clock), PUT (that clock
// at 200 ms) and algo POST (the shared clock plus a 500 ms algo clock).
const GLOBAL: Pacing = Pacing {
    group: 10,
    minimum_gap_ms: 100.,
};
const ALGO: Pacing = Pacing {
    group: 11,
    minimum_gap_ms: 500.,
};
const ALGO_SET: crate::request_context::PacingSet = crate::request_context::PacingSet {
    group: 12,
    constraints: &[GLOBAL, ALGO],
};
const POST: Pacing = Pacing {
    group: 10,
    minimum_gap_ms: 0.,
};
const PUT: Pacing = Pacing {
    group: 10,
    minimum_gap_ms: 200.,
};
const ALGO_ORDER: Pacing = Pacing {
    group: 12,
    minimum_gap_ms: 0.,
};
// MEXC: one submit clock at a 450 ms base, and a 1500 ms multi-order job gap.
const SUBMIT: Pacing = Pacing {
    group: 20,
    minimum_gap_ms: 450.,
};
const SUBMIT_BASE: Pacing = Pacing {
    group: 20,
    minimum_gap_ms: 0.,
};
const SUBMIT_JOB: Pacing = Pacing {
    group: 20,
    minimum_gap_ms: 1500.,
};

/// `D`: 0 no domain, 1 an all-REST domain on the registered group, 2 a domain
/// naming an unregistered group, 3 a Mutations domain, 4 a Mutations domain
/// on the registered zero-gap group, 5 WOO's admissible set, 6 MEXC's.
#[derive(Default)]
struct Fifo<const D: u8>;

impl<const D: u8> Venue for Fifo<D> {
    type Pending = u32;
    fn http_pacing(&self) -> &[Pacing] {
        match D {
            5 => &[GLOBAL, ALGO],
            6 => &[SUBMIT],
            _ => &[GROUP, ZERO],
        }
    }
    fn http_release_domain() -> Option<ReleaseDomain> {
        match D {
            1 => Some(ReleaseDomain {
                pacings: &[GROUP],
                members: DomainMembers::AllRest,
            }),
            2 => Some(ReleaseDomain {
                pacings: &[Pacing {
                    group: 9,
                    minimum_gap_ms: GAP,
                }],
                members: DomainMembers::AllRest,
            }),
            3 => Some(ReleaseDomain {
                pacings: &[GROUP],
                members: DomainMembers::Mutations,
            }),
            4 => Some(ReleaseDomain {
                pacings: &[ZERO],
                members: DomainMembers::Mutations,
            }),
            5 => Some(ReleaseDomain {
                pacings: &[POST, PUT, ALGO_ORDER],
                members: DomainMembers::AllRest,
            }),
            6 => Some(ReleaseDomain {
                pacings: &[SUBMIT_BASE, SUBMIT_JOB],
                members: DomainMembers::AllRest,
            }),
            _ => None,
        }
    }
    fn http_pacing_sets(&self) -> &[crate::request_context::PacingSet] {
        if D == 5 { &[ALGO_SET] } else { &[] }
    }
    fn start(&mut self, _: &mut Cx<'_, u32>) {}
    fn on_open(&mut self, _: &mut Cx<'_, u32>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, u32>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, u32>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, u32>, _: u32, _: Response<'_>) {}
    fn on_timer(&mut self, _: &mut Cx<'_, u32>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, u32>, _: Command) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, u32>, _: u32, _: &str) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn engine<const D: u8>() -> Engine<Fifo<D>> {
    let mut engine = crate::test_support::seeded_engine(Fifo::<D>);
    engine.start(0.);
    engine.drain_actions().for_each(drop);
    engine
}

fn read(pacing: Option<Pacing>) -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 20_000,
        pacing,
        retry: RetryPolicy::Never,
    }
}

fn context<const D: u8>(engine: &mut Engine<Fifo<D>>, now: f64) -> Context {
    Cx {
        now,
        store: &mut engine.store,
        io: &mut engine.io,
    }
    .http_context(Scope::Background, read(None))
    .unwrap()
}

/// Accepts one managed request and returns its preparation token.
fn queue<const D: u8>(
    engine: &mut Engine<Fifo<D>>,
    context: Context,
    id: u32,
    now: f64,
) -> Preparation {
    Cx {
        now,
        store: &mut engine.store,
        io: &mut engine.io,
    }
    .request_managed(
        context,
        None,
        Method::Get,
        format!("https://venue.test/{id}"),
        None,
        id,
    )
    .map_err(|(_, error)| error)
    .unwrap();
    engine
        .drain_actions()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .expect("a managed Prepare")
}

/// Completes preparation and returns the ids whose Bind was emitted, in order.
fn prepare<const D: u8>(
    engine: &mut Engine<Fifo<D>>,
    token: Preparation,
    now: f64,
) -> Vec<(u64, crate::request_context::Permit)> {
    engine.http_prepared(
        now,
        token,
        Prepared {
            id: token.attempt().request.0,
            expires_at: None,
        },
    );
    binds(engine)
}

fn binds<const D: u8>(engine: &mut Engine<Fifo<D>>) -> Vec<(u64, crate::request_context::Permit)> {
    engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Bind {
                    permit, request, ..
                } => Some((request.id.0, permit)),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

fn id(token: Preparation) -> u64 {
    token.attempt().request.0
}

#[test]
fn binds_follow_acceptance_order_whatever_the_preparation_order() {
    let mut e = engine::<1>();
    let tokens: Vec<_> = (0..3)
        .map(|n| {
            let context = context(&mut e, 0.);
            queue(&mut e, context, n, 0.)
        })
        .collect();
    // C, then B, prepare before the head A: both hold their Bind.
    assert!(
        prepare(&mut e, tokens[2], 0.).is_empty(),
        "C holds behind A"
    );
    assert!(
        prepare(&mut e, tokens[1], 0.).is_empty(),
        "B holds behind A"
    );
    let a = prepare(&mut e, tokens[0], 0.);
    assert_eq!(
        a.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        [id(tokens[0])]
    );
    assert!(matches!(e.claim_http(0., a[0].1), Ok(Claim::Released(_))));
    let b = binds(&mut e);
    assert_eq!(
        b.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        [id(tokens[1])],
        "B is the next head"
    );
}

#[test]
fn the_head_waits_out_the_gap_from_the_previous_release() {
    let mut e = engine::<1>();
    let first = context(&mut e, 0.);
    let a = queue(&mut e, first, 0, 0.);
    let second = context(&mut e, 0.);
    let b = queue(&mut e, second, 1, 0.);
    let a = prepare(&mut e, a, 0.);
    assert!(prepare(&mut e, b, 0.).is_empty());
    assert!(matches!(e.claim_http(100., a[0].1), Ok(Claim::Released(_))));
    let b = binds(&mut e);
    assert_eq!(b.len(), 1);
    assert_eq!(
        e.claim_http(100. + GAP - 1., b[0].1),
        Ok(Claim::WaitUntil(100. + GAP))
    );
    assert!(matches!(
        e.claim_http(100. + GAP, b[0].1),
        Ok(Claim::Released(_))
    ));
}

#[test]
fn only_an_accepted_request_enters_the_queue() {
    let mut e = engine::<1>();
    // A's Context exists first, but B is accepted first.
    let a_context = context(&mut e, 0.);
    let b_context = context(&mut e, 0.);
    let b = queue(&mut e, b_context, 1, 0.);
    let a = queue(&mut e, a_context, 0, 0.);
    assert!(
        prepare(&mut e, a, 0.).is_empty(),
        "A holds behind the earlier-accepted B"
    );
    let b = prepare(&mut e, b, 0.);
    assert_eq!(b.len(), 1);
    assert!(matches!(e.claim_http(0., b[0].1), Ok(Claim::Released(_))));
    assert_eq!(binds(&mut e).len(), 1, "A binds after B's release");
}

#[test]
fn a_head_that_fails_before_release_hands_off_without_spending_the_gap() {
    let mut e = engine::<1>();
    let first = context(&mut e, 0.);
    let a = queue(&mut e, first, 0, 0.);
    let second = context(&mut e, 0.);
    let b = queue(&mut e, second, 1, 0.);
    assert!(prepare(&mut e, b, 0.).is_empty());
    e.http_preparation_failed(
        10.,
        a,
        crate::request_context::Error::Invalid,
        VenueError::new("signing refused"),
    );
    let b = binds(&mut e);
    assert_eq!(b.len(), 1, "B becomes the head");
    assert!(
        matches!(e.claim_http(10., b[0].1), Ok(Claim::Released(_))),
        "no release happened, so B waits for no gap"
    );
}

#[test]
fn a_member_policy_naming_another_group_is_invalid() {
    let mut e = engine::<1>();
    let foreign = Pacing {
        group: 2,
        minimum_gap_ms: 0.,
    };
    assert!(
        Cx {
            now: 0.,
            store: &mut e.store,
            io: &mut e.io,
        }
        .http_context(Scope::Background, read(Some(foreign)))
        .is_err()
    );
}

#[test]
fn an_unregistered_domain_group_refuses_engine_construction() {
    assert!(
        Engine::new(
            Fifo::<2>,
            Some(crate::test_support::seeded_entropy(0x7365_6564_6564_4944))
        )
        .is_err()
    );
}

#[test]
fn a_venue_without_a_domain_binds_and_releases_at_once() {
    let mut e = engine::<0>();
    let first = context(&mut e, 0.);
    let a = queue(&mut e, first, 0, 0.);
    let second = context(&mut e, 0.);
    let b = queue(&mut e, second, 1, 0.);
    let b = prepare(&mut e, b, 0.);
    assert_eq!(b.len(), 1, "no domain: no hold");
    let a = prepare(&mut e, a, 0.);
    assert!(matches!(e.claim_http(0., a[0].1), Ok(Claim::Released(_))));
    assert!(matches!(e.claim_http(1., b[0].1), Ok(Claim::Released(_))));
}

fn write_preflight<const D: u8>(
    engine: &mut Engine<Fifo<D>>,
    command: u32,
) -> crate::request_context::Preflight {
    Cx {
        now: 0.,
        store: &mut engine.store,
        io: &mut engine.io,
    }
    .issue_preflight(
        command,
        crate::request_context::PreflightPolicy {
            purpose: crate::request_context::PreflightPurpose::WriteAfterReads,
            read_credential: None,
            write_credential: Some(credential()),
        },
    )
    .unwrap()
}

fn credential() -> crate::request_context::Credential {
    crate::request_context::Credential {
        slot: crate::CredentialSlot::Primary,
        reference: crate::signing::KeyRef("domain".into()),
    }
}

fn serial_leg<const D: u8>(engine: &mut Engine<Fifo<D>>) -> Context {
    let preflight = write_preflight(engine, 7);
    let mut cx = Cx {
        now: 0.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    let (plan, slot) = cx
        .admit_serial_plan(
            &preflight,
            crate::request_context::plan::SerialCall {
                operation: Operation::Mutation,
                timeout_ms: 20_000,
                attempts: 1,
            },
            crate::request_context::plan::SerialPolicy {
                credential: Some(credential()),
                pacing: None,
                refusals_complete: false,
            },
        )
        .expect("a serial Plan joins the domain");
    cx.plan_leg_context(&plan, &slot).unwrap()
}

#[test]
fn a_mutations_domain_paces_writes_and_leaves_reads_unpaced() {
    let mut e = engine::<3>();
    let read = Cx {
        now: 0.,
        store: &mut e.store,
        io: &mut e.io,
    }
    .http_context(Scope::Background, read(None))
    .unwrap();
    assert_eq!(read.pacing(), None, "a read is not a member");
    assert_eq!(serial_leg(&mut e).pacing(), Some(GROUP), "a write is");
}

#[test]
fn an_all_rest_domain_stamps_reads_and_writes() {
    let mut e = engine::<1>();
    let read = Cx {
        now: 0.,
        store: &mut e.store,
        io: &mut e.io,
    }
    .http_context(Scope::Background, read(None))
    .unwrap();
    assert_eq!(read.pacing(), Some(GROUP));
    assert_eq!(serial_leg(&mut e).pacing(), Some(GROUP));
}

#[cfg(feature = "http-mutation-barriers")]
#[test]
fn a_barrier_plan_with_omitted_pacing_is_still_refused_in_a_domain() {
    use crate::request_context::plan::{CallBound, MutationBarrierBound, RetryWindow, StepBound};
    let mut e = engine::<3>();
    let preflight = write_preflight(&mut e, 7);
    let call = || CallBound {
        timeout_ms: 20_000,
        attempts: 1,
        retry_window: RetryWindow::FreshAttempt,
        after_refusal: &[],
    };
    let calls = [call(), call()];
    let steps = [StepBound {
        wait_ms: 0,
        calls: &calls,
    }];
    let refused = Cx {
        now: 0.,
        store: &mut e.store,
        io: &mut e.io,
    }
    .admit_plan_with_barriers(
        &preflight,
        &steps,
        crate::request_context::PlanPolicy {
            independent_calls: false,
            operation: Operation::Mutation,
            credential: Some(credential()),
            pacing: None,
            retry: RetryPolicy::Never,
        },
        &[MutationBarrierBound { step: 0 }],
    );
    assert_eq!(refused.err(), Some(crate::request_context::Error::Invalid));
}

#[test]
fn a_non_head_member_terminal_removes_only_itself() {
    let mut e = engine::<1>();
    let tokens: Vec<_> = (0..3)
        .map(|n| {
            let context = context(&mut e, 0.);
            queue(&mut e, context, n, 0.)
        })
        .collect();
    assert!(prepare(&mut e, tokens[2], 0.).is_empty(), "C holds");
    // B fails while A is still the head: only B leaves.
    e.http_preparation_failed(
        1.,
        tokens[1],
        crate::request_context::Error::Invalid,
        VenueError::new("refused"),
    );
    assert!(
        binds(&mut e).is_empty(),
        "A, the head, has not prepared yet"
    );
    let a = prepare(&mut e, tokens[0], 1.);
    assert_eq!(a.len(), 1);
    assert!(matches!(e.claim_http(1., a[0].1), Ok(Claim::Released(_))));
    let next = binds(&mut e);
    assert_eq!(
        next.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        [id(tokens[2])],
        "C is next after A; B is gone"
    );
}

fn preflight_read<const D: u8>(
    engine: &mut Engine<Fifo<D>>,
    preflight: &crate::request_context::Preflight,
    pacing: Option<Pacing>,
) -> Result<Context, crate::request_context::Error> {
    Cx {
        now: 0.,
        store: &mut engine.store,
        io: &mut engine.io,
    }
    .preflight_read_context(
        preflight,
        read(pacing),
        crate::request_context::plan::CallTerms {
            wait_ms: 0,
            timeout_ms: 20_000,
            attempts: 1,
            retry_window: crate::request_context::plan::RetryWindow::FreshAttempt,
        },
    )
}

#[test]
fn a_preflight_read_joins_an_all_rest_domain_behind_an_earlier_member() {
    let mut e = engine::<1>();
    let background = context(&mut e, 0.);
    let a = queue(&mut e, background, 0, 0.);
    let preflight = write_preflight(&mut e, 7);
    let read = preflight_read(&mut e, &preflight, None).unwrap();
    assert_eq!(read.pacing(), Some(GROUP), "stamped with the domain pacing");
    let token = queue(&mut e, read, 1, 0.);
    assert!(
        prepare(&mut e, token, 0.).is_empty(),
        "prepared first, the Preflight read still holds behind A"
    );
    let a = prepare(&mut e, a, 0.);
    assert!(matches!(e.claim_http(100., a[0].1), Ok(Claim::Released(_))));
    let p = binds(&mut e);
    assert_eq!(p.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [id(token)]);
    assert_eq!(
        e.claim_http(100. + GAP - 1., p[0].1),
        Ok(Claim::WaitUntil(100. + GAP))
    );
    assert!(matches!(
        e.claim_http(100. + GAP, p[0].1),
        Ok(Claim::Released(_))
    ));
}

/// Takes one probe ID; with nothing spent between two probes they are adjacent.
fn next_id<const D: u8>(engine: &mut Engine<Fifo<D>>) -> u64 {
    engine.io.core.next_id.reserve(1).expect("ids").start + 1
}

#[test]
fn a_preflight_read_naming_another_group_spends_nothing() {
    let mut e = engine::<1>();
    let preflight = write_preflight(&mut e, 7);
    let before = next_id(&mut e);
    let foreign = Pacing {
        group: 2,
        minimum_gap_ms: 0.,
    };
    assert_eq!(
        preflight_read(&mut e, &preflight, Some(foreign)).err(),
        Some(crate::request_context::Error::Invalid)
    );
    assert_eq!(next_id(&mut e), before + 1, "no ID was spent");
    assert!(
        preflight_read(&mut e, &preflight, None).is_ok(),
        "the call was not spent"
    );
}

#[test]
fn a_preflight_read_is_unpaced_outside_an_all_rest_domain() {
    let mut mutations = engine::<3>();
    let preflight = write_preflight(&mut mutations, 7);
    let read = preflight_read(&mut mutations, &preflight, None).unwrap();
    assert_eq!(read.pacing(), None, "a read is not a Mutations member");
    let p = queue(&mut mutations, read, 1, 0.);
    assert_eq!(prepare(&mut mutations, p, 0.).len(), 1, "binds at once");
    let mut none = engine::<0>();
    let preflight = write_preflight(&mut none, 7);
    assert_eq!(
        preflight_read(&mut none, &preflight, None)
            .unwrap()
            .pacing(),
        None
    );
}

#[cfg(feature = "http-mutation-barriers")]
#[test]
fn a_zero_gap_barrier_plan_is_refused_in_a_domain() {
    use crate::request_context::plan::{CallBound, MutationBarrierBound, RetryWindow, StepBound};
    let mut e = engine::<4>();
    let preflight = write_preflight(&mut e, 7);
    let call = || CallBound {
        timeout_ms: 20_000,
        attempts: 1,
        retry_window: RetryWindow::FreshAttempt,
        after_refusal: &[],
    };
    let calls = [call(), call()];
    let steps = [StepBound {
        wait_ms: 0,
        calls: &calls,
    }];
    let before = next_id(&mut e);
    let refused = Cx {
        now: 0.,
        store: &mut e.store,
        io: &mut e.io,
    }
    .admit_plan_with_barriers(
        &preflight,
        &steps,
        crate::request_context::PlanPolicy {
            independent_calls: false,
            operation: Operation::Mutation,
            credential: Some(credential()),
            pacing: None,
            retry: RetryPolicy::Never,
        },
        &[MutationBarrierBound { step: 0 }],
    );
    assert_eq!(refused.err(), Some(crate::request_context::Error::Invalid));
    assert_eq!(next_id(&mut e), before + 1, "no Plan ID was spent");
}

#[test]
fn the_managed_boundary_refuses_unstamped_grouped_and_overflowing_members() {
    use crate::request_context::Error;
    let mut e = engine::<1>();
    let core = &mut e.io.core;
    assert_eq!(
        core.domain_admits(Operation::Read, None, false),
        Err(Error::Invalid)
    );
    assert_eq!(
        core.domain_admits(Operation::Read, Some(GROUP), true),
        Err(Error::Invalid)
    );
    assert_eq!(
        core.domain_admits(Operation::Read, Some(GROUP), false),
        Ok(())
    );
    let reserved = core.domain_queue.capacity();
    for n in 0..super::release_domain::CAPACITY as u64 {
        core.domain_accept(RequestId(n), Operation::Read, Some(GROUP));
    }
    assert_eq!(core.domain_queue.capacity(), reserved, "no reallocation");
    assert_eq!(
        core.domain_admits(Operation::Read, Some(GROUP), false),
        Err(Error::Capacity)
    );
}

#[test]
fn a_held_bind_is_wipeable_with_release_domains_alone() {
    let mut e = engine::<1>();
    let background = context(&mut e, 0.);
    let token = queue(&mut e, background, 0, 0.);
    e.http_prepared(
        0.,
        token,
        Prepared {
            id: token.attempt().request.0,
            expires_at: None,
        },
    );
    let mut bind = e
        .drain_actions()
        .find(|action| {
            matches!(action, Action::ManagedHttp(managed) if matches!(**managed, HttpAction::Bind { .. }))
        })
        .expect("the head binds");
    crate::signing_dispatch::wipe(&mut bind);
    let Action::ManagedHttp(managed) = &bind else {
        unreachable!()
    };
    let HttpAction::Bind { request, .. } = &**managed else {
        unreachable!()
    };
    assert!(request.url.is_empty(), "the Bind's request is wiped");
}

#[test]
fn a_non_member_read_naming_the_group_does_not_join_a_mutations_fifo() {
    let mut e = engine::<3>();
    let read = Cx {
        now: 0.,
        store: &mut e.store,
        io: &mut e.io,
    }
    .http_context(Scope::Background, read(Some(GROUP)))
    .unwrap();
    assert_eq!(read.pacing(), Some(GROUP), "the read keeps its own pacing");
    let token = queue(&mut e, read, 1, 0.);
    assert!(
        e.io.core.domain_queue.is_empty(),
        "a non-member is never a FIFO member"
    );
    assert_eq!(prepare(&mut e, token, 0.).len(), 1, "it binds at once");
}

fn paced<const D: u8>(engine: &mut Engine<Fifo<D>>, pacing: Pacing) -> Context {
    Cx {
        now: 0.,
        store: &mut engine.store,
        io: &mut engine.io,
    }
    .http_context(Scope::Background, read(Some(pacing)))
    .unwrap()
}

/// Queues one member under `pacing`, prepares it as the head, and returns the
/// Claim result at `at` (releasing it if the floor allows).
fn head<const D: u8>(engine: &mut Engine<Fifo<D>>, pacing: Pacing, id: u32, at: f64) -> Claim {
    let context = paced(engine, pacing);
    let token = queue(engine, context, id, 0.);
    let bind = prepare(engine, token, 0.);
    engine.claim_http(at, bind[0].1).unwrap()
}

fn release<const D: u8>(engine: &mut Engine<Fifo<D>>, pacing: Pacing, id: u32, at: f64) {
    match head(engine, pacing, id, at) {
        Claim::Released(_) => {}
        Claim::WaitUntil(floor) => panic!("expected a release at {at}, floor {floor}"),
    }
}

fn wait<const D: u8>(engine: &mut Engine<Fifo<D>>, pacing: Pacing, id: u32, at: f64) -> f64 {
    match head(engine, pacing, id, at) {
        Claim::WaitUntil(floor) => floor,
        _ => panic!("expected a wait at {at}"),
    }
}

#[test]
fn woo_charges_each_release_at_its_own_admissible_pacing() {
    let mut e = engine::<5>();
    release(&mut e, PUT, 0, 0.);
    assert_eq!(wait(&mut e, POST, 1, 0.), 200., "PUT then POST waits 200");
    let mut e = engine::<5>();
    release(&mut e, POST, 0, 0.);
    assert_eq!(wait(&mut e, PUT, 1, 0.), 100., "POST then PUT waits 100");
}

#[test]
fn woo_algo_floor_persists_across_ordinary_writes() {
    let mut e = engine::<5>();
    release(&mut e, ALGO_ORDER, 0, 0.);
    assert_eq!(wait(&mut e, POST, 1, 0.), 100., "the shared clock at 100");
    let mut e = engine::<5>();
    release(&mut e, ALGO_ORDER, 0, 0.);
    let token = {
        let context = paced(&mut e, POST);
        queue(&mut e, context, 1, 0.)
    };
    let bind = prepare(&mut e, token, 0.);
    assert!(matches!(
        e.claim_http(100., bind[0].1),
        Ok(Claim::Released(_))
    ));
    assert_eq!(
        wait(&mut e, ALGO_ORDER, 2, 100.),
        500.,
        "the algo clock outlives the ordinary write"
    );
}

#[test]
fn an_inadmissible_pacing_is_invalid_and_spends_nothing() {
    let mut e = engine::<5>();
    let before = next_id(&mut e);
    let outside = Pacing {
        group: 10,
        minimum_gap_ms: 150.,
    };
    assert!(
        Cx {
            now: 0.,
            store: &mut e.store,
            io: &mut e.io,
        }
        .http_context(Scope::Background, read(Some(outside)))
        .is_err(),
        "membership is exact: group and gap"
    );
    assert_eq!(next_id(&mut e), before + 1, "no ID was spent");
    assert_eq!(
        paced(&mut e, PUT).pacing(),
        Some(PUT),
        "an admissible entry keeps itself"
    );
    assert_eq!(
        context(&mut e, 0.).pacing(),
        Some(POST),
        "an unnamed member takes the first entry"
    );
}

#[test]
fn mexc_job_gap_and_base_gap_share_one_submit_clock() {
    let mut e = engine::<6>();
    release(&mut e, SUBMIT_JOB, 0, 0.);
    assert_eq!(
        wait(&mut e, SUBMIT_BASE, 1, 0.),
        1500.,
        "after a job call, 1500"
    );
    let mut e = engine::<6>();
    release(&mut e, SUBMIT_BASE, 0, 0.);
    assert_eq!(
        wait(&mut e, SUBMIT_JOB, 1, 0.),
        450.,
        "after a base call, 450"
    );
}

#[test]
fn an_unresolvable_or_empty_admissible_set_refuses_construction() {
    assert!(
        super::release_domain::validate(
            ReleaseDomain {
                pacings: &[],
                members: DomainMembers::AllRest,
            },
            &crate::request_context::SendPacing::new(&[GROUP]).unwrap(),
        )
        .is_err()
    );
    assert!(
        super::release_domain::validate(
            ReleaseDomain {
                pacings: &[
                    GROUP,
                    Pacing {
                        group: 13,
                        minimum_gap_ms: 0.
                    }
                ],
                members: DomainMembers::AllRest,
            },
            &crate::request_context::SendPacing::new(&[GROUP]).unwrap(),
        )
        .is_err(),
        "every entry must resolve"
    );
}
