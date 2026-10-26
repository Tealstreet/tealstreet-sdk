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

/// `D`: 0 no domain, 1 an all-REST domain on the registered group, 2 a domain
/// naming an unregistered group, 3 a Mutations domain, 4 a Mutations domain
/// on the registered zero-gap group.
#[derive(Default)]
struct Fifo<const D: u8>;

impl<const D: u8> Venue for Fifo<D> {
    type Pending = u32;
    fn http_pacing(&self) -> &[Pacing] {
        &[GROUP, ZERO]
    }
    fn http_release_domain() -> Option<ReleaseDomain> {
        match D {
            1 => Some(ReleaseDomain {
                pacing: GROUP,
                members: DomainMembers::AllRest,
            }),
            2 => Some(ReleaseDomain {
                pacing: Pacing {
                    group: 9,
                    minimum_gap_ms: GAP,
                },
                members: DomainMembers::AllRest,
            }),
            3 => Some(ReleaseDomain {
                pacing: GROUP,
                members: DomainMembers::Mutations,
            }),
            4 => Some(ReleaseDomain {
                pacing: ZERO,
                members: DomainMembers::Mutations,
            }),
            _ => None,
        }
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
    serial_call_leg(engine, Operation::Mutation, None).expect("a serial Plan joins the domain")
}

/// The first leg of a serial Plan whose first call names `pacing`.
fn serial_call_leg<const D: u8>(
    engine: &mut Engine<Fifo<D>>,
    operation: Operation,
    pacing: Option<Pacing>,
) -> Result<Context, crate::request_context::Error> {
    let preflight = write_preflight(engine, 7);
    let mut cx = Cx {
        now: 0.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    let (plan, slot) = cx.admit_serial_plan(
        &preflight,
        crate::request_context::plan::SerialCall {
            operation,
            timeout_ms: 20_000,
            attempts: 1,
            pacing,
        },
        crate::request_context::plan::SerialPolicy {
            credential: Some(credential()),
            pacing: None,
            refusals_complete: false,
        },
    )?;
    cx.plan_leg_context(&plan, &slot)
}

#[test]
fn a_serial_call_names_only_its_domain_pacing() {
    let mut e = engine::<3>();
    assert_eq!(
        serial_call_leg(&mut e, Operation::Mutation, Some(GROUP)).map(|leg| leg.pacing()),
        Ok(Some(GROUP)),
        "a member call may name the domain pacing"
    );
    for (operation, pacing) in [
        (Operation::Mutation, ZERO),
        // A Read is no Mutations member, so it cannot borrow the domain's gap.
        (Operation::Read, GROUP),
    ] {
        let mut e = engine::<3>();
        assert_eq!(
            serial_call_leg(&mut e, operation, Some(pacing)).err(),
            Some(crate::request_context::Error::Invalid),
            "{operation:?} naming {pacing:?}"
        );
    }
    let mut e = engine::<3>();
    assert_eq!(
        serial_call_leg(&mut e, Operation::Read, None).map(|leg| leg.pacing()),
        Ok(None),
        "a Read call does not inherit the domain-paced root"
    );
}

#[test]
fn outside_a_domain_a_serial_call_names_only_a_zero_gap_group() {
    let mut e = engine::<0>();
    assert_eq!(
        serial_call_leg(&mut e, Operation::Mutation, Some(ZERO)).map(|leg| leg.pacing()),
        Ok(Some(ZERO)),
        "the leg carries the call's own pacing"
    );
    let mut e = engine::<0>();
    assert_eq!(
        serial_call_leg(&mut e, Operation::Mutation, Some(GROUP)).err(),
        Some(crate::request_context::Error::Invalid),
        "a nonzero gap needs a domain"
    );
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
