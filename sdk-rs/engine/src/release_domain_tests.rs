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

/// `D`: 0 no domain, 1 an all-REST domain on the registered group, 2 a domain
/// naming an unregistered group.
#[derive(Default)]
struct Fifo<const D: u8>;

impl<const D: u8> Venue for Fifo<D> {
    type Pending = u32;
    fn http_pacing(&self) -> &[Pacing] {
        &[GROUP]
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
