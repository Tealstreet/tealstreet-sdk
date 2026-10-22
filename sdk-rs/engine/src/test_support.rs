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

/// Immediate fake routes for native venue fixtures. Physical HTTP is returned
/// only after prepare, bind and the engine's final synchronous release claim.
/// Delayed pacing/routes and delegated signing deliberately remain unsupported.
pub fn prepared_actions<V: Venue>(engine: &mut Engine<V>, now: f64) -> Vec<Action> {
    use crate::request_context::{Claim, HttpAction, Prepared};
    use std::collections::{HashMap, VecDeque};

    engine.prepare_dispatch(now);
    let mut queue = VecDeque::from(engine.drain_actions().collect::<Vec<_>>());
    let mut resources = HashMap::new();
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
                let id = u64::from(request.id.0);
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
            HttpAction::Cancel { preparation, .. } => {
                let request = preparation.attempt().request;
                if resources
                    .get(&request)
                    .is_some_and(|(token, _)| *token == preparation)
                {
                    resources.remove(&request);
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
