//! Exercise the host Prepare/Bind/Claim protocol at the event's clock.
#![allow(dead_code)]
use safe_cex_engine::request_context::{Claim, HttpAction, Prepared};
use safe_cex_engine::{Action, Engine, Venue};
use std::collections::VecDeque;

pub fn drain<V: Venue>(engine: &mut Engine<V>, now: f64) -> Vec<Action> {
    let mut queue: VecDeque<_> = engine.drain_actions().collect();
    let mut output = Vec::new();
    while let Some(action) = queue.pop_front() {
        match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => {
                    assert!(engine.http_preparation_pending(preparation));
                    engine.http_prepared(
                        now,
                        preparation,
                        Prepared {
                            id: request.id.0,
                            expires_at: None,
                        },
                    );
                }
                HttpAction::Bind {
                    permit, request, ..
                } => {
                    assert!(matches!(
                        engine.claim_http(now, permit),
                        Ok(Claim::Released(_))
                    ));
                    output.push(Action::Http(request));
                }
                HttpAction::Cancel { .. } => {}
            },
            action => output.push(action),
        }
        queue.extend(engine.drain_actions());
    }
    output
}
