//! Test-only helpers shared by venue crates (`features = ["test-support"]`).
use crate::book::Book;
use crate::{Action, Command, Cx, Engine, HttpRequest, Response, SocketId, TimerId, Venue};

/// Wraps a venue so a test can drive one of its private request helpers from
/// `start` and observe the emitted requests; every event handler is inert.
pub struct ProbeVenue<V, F> {
    pub venue: V,
    start: F,
}

impl<V: Venue, F: FnMut(&mut V, &mut Cx<'_, V::Pending>)> ProbeVenue<V, F> {
    pub fn new(venue: V, start: F) -> Self {
        Self { venue, start }
    }
}

impl<V: Venue, F: FnMut(&mut V, &mut Cx<'_, V::Pending>)> Venue for ProbeVenue<V, F> {
    type Pending = V::Pending;
    fn http_mutation_barriers() -> bool {
        V::http_mutation_barriers()
    }
    fn start(&mut self, cx: &mut Cx<'_, V::Pending>) {
        (self.start)(&mut self.venue, cx);
    }
    fn on_open(&mut self, _: &mut Cx<'_, V::Pending>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, V::Pending>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, V::Pending>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, V::Pending>, _: V::Pending, _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, V::Pending>, _: V::Pending, _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, V::Pending>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, V::Pending>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

/// Drain the engine's queued actions, keeping only HTTP requests.
pub fn http_requests<V: Venue>(engine: &mut Engine<V>) -> Vec<HttpRequest> {
    engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect()
}

/// `prepared_actions` at the latest handled event's time: managed requests
/// leave only through preparation, so fixtures that drained after `handle`
/// prepare at that same instant.
pub fn prepared_now<V: Venue>(engine: &mut Engine<V>) -> Vec<Action> {
    let now = engine.dispatch_now();
    prepared_actions(engine, now)
}

/// Immediate fake routes for native venue fixtures. Physical HTTP is returned
/// only after prepare, bind and the engine's final synchronous release claim.
/// Delayed pacing/routes and delegated signing deliberately remain unsupported.
pub fn prepared_actions<V: Venue>(engine: &mut Engine<V>, now: f64) -> Vec<Action> {
    use crate::request_context::{Claim, GroupClaim, GroupPacket, HttpAction, Prepared};
    use std::collections::{HashMap, VecDeque};

    engine.prepare_dispatch(now);
    let mut queue = VecDeque::from(engine.drain_actions().collect::<Vec<_>>());
    let mut resources = HashMap::new();
    let mut groups: HashMap<crate::RequestId, GroupPacket> = HashMap::new();
    let mut actions = Vec::new();
    let mut count = 0;
    while let Some(action) = queue.pop_front() {
        count += 1;
        assert!(
            count <= 65_536,
            "prepared fixture action loop exceeded its bound"
        );
        let Action::ManagedHttp(action) = action else {
            actions.push(action);
            continue;
        };
        match *action {
            HttpAction::Prepare {
                preparation,
                request,
                ..
            } => {
                if !engine.http_preparation_pending(preparation) {
                    continue;
                }
                assert_eq!(request.id, preparation.attempt().request);
                let id = request.id.0;
                assert!(
                    resources.insert(request.id, (preparation, id)).is_none(),
                    "fixture route replaced without cancellation"
                );
                engine.http_prepared(
                    now,
                    preparation,
                    Prepared {
                        id,
                        expires_at: None,
                    },
                );
            }
            HttpAction::Bind {
                preparation,
                permit,
                request,
            } => {
                assert_eq!(
                    resources.remove(&request.id),
                    Some((preparation, permit.prepared_id())),
                    "fixture bind needs its exact prepared route"
                );
                match engine.claim_http(now, permit) {
                    Ok(Claim::Released(_)) => actions.push(Action::Http(request)),
                    Ok(Claim::WaitUntil(_)) => {
                        panic!("delayed prepared fixture pacing is unsupported")
                    }
                    Err(_) => {} // Engine publishes the typed preparation failure/reprepare.
                }
            }
            HttpAction::BindGroup { permit, members } => {
                if !engine.http_group_pending(&permit) {
                    continue;
                }
                for mut member in members {
                    let request = member.preparation.attempt().request;
                    let current = permit
                        .members()
                        .iter()
                        .find(|p| p.attempt().request == request)
                        .unwrap();
                    assert_eq!(
                        resources.get(&request),
                        Some(&(member.preparation, current.prepared_id()))
                    );
                    if let Some(packet) = member.request.take() {
                        assert_eq!(packet.id, request);
                        assert!(
                            groups
                                .insert(
                                    request,
                                    GroupPacket {
                                        preparation: member.preparation,
                                        request: Some(packet)
                                    }
                                )
                                .is_none()
                        );
                    } else {
                        assert!(
                            groups
                                .get(&request)
                                .is_some_and(|packet| packet.preparation == member.preparation
                                    && packet.request.is_some())
                        );
                    }
                }
                match engine.claim_http_group(now, permit.clone()) {
                    Ok(GroupClaim::Released(_)) => {
                        for token in permit.members() {
                            let request = token.attempt().request;
                            let mut member = groups.remove(&request).unwrap();
                            resources.remove(&request);
                            actions.push(Action::Http(member.request.take().unwrap()));
                        }
                    }
                    Ok(GroupClaim::WaitUntil(_)) => {
                        panic!("delayed group fixture pacing is unsupported")
                    }
                    Err(_) => {}
                }
            }
            HttpAction::Cancel { preparation, .. } => {
                let request = preparation.attempt().request;
                if resources
                    .get(&request)
                    .is_some_and(|(token, _)| *token == preparation)
                {
                    resources.remove(&request);
                    groups.remove(&request);
                }
            }
        }
        // Preserve progress/receipt order at each handoff, rather than moving
        // an earlier packet behind all later sibling actions.
        for action in engine.drain_actions().collect::<Vec<_>>().into_iter().rev() {
            queue.push_front(action);
        }
    }
    assert!(
        resources.is_empty(),
        "parked external signing is unsupported by the immediate fixture"
    );
    actions
}

/// Explicit fixture injection for BOTH identity and signing. Never a production default.
pub fn seeded_entropy(seed: u64) -> crate::entropy::Entropy {
    fn source(mut state: u64) -> crate::entropy::Fill {
        Box::new(move |bytes| {
            for byte in bytes {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                *byte = state as u8;
            }
            Ok(())
        })
    }
    crate::entropy::Entropy::new(
        source(seed.max(1)),
        source((seed ^ 0xa076_1d64_78bd_642f).max(1)),
    )
}

/// Tests explicitly choose reproducible signing entropy as well as identities.
pub fn seeded_engine<V: Venue>(venue: V) -> Engine<V> {
    Engine::new(venue, Some(seeded_entropy(0x7365_6564_6564_4944))).unwrap()
}

/// Permanently exhaust the order-read admission domain for lifecycle controls.
pub fn exhaust_order_snapshots(store: &mut crate::Store) {
    store.exhaust_order_snapshots_for_test();
}
