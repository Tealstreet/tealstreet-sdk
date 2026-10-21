//! Cancellation of an acknowledged placement before the store sees it.
//!
//! A placement resolves with venue ids before the private stream (or the next
//! reconciliation) adds those orders to the store. A caller that cancels or
//! amends at once (a chaser stopped mid-placement) names an id the store does
//! not hold, and venues that need a stored order to route the write refuse it,
//! leaving the order live. Such a command waits here until every acknowledged
//! id it names is stored, or until `ACK_WAIT_MS` passes; it is then dispatched
//! unchanged, so an id that never appears still fails explicitly at the venue.

use crate::{Command, CommandId, Store};
use std::collections::HashSet;

/// How long an acknowledged id may wait for the store before a command naming
/// it is dispatched anyway.
pub(crate) const ACK_WAIT_MS: f64 = 10_000.0;

#[derive(Default)]
pub(crate) struct PlacementAcks {
    placing: HashSet<CommandId>,
    /// Acknowledged ids not yet seen in the store, with their expiry.
    acked: Vec<(String, f64)>,
    /// Held commands, each with its dispatch deadline.
    held: Vec<(Command, f64)>,
}

/// Virtual protective legs share their parent's stored identity.
fn native(id: &str) -> &str {
    [
        "__stop_loss",
        "__take_profit",
        "__trailing_stop_loss",
        "_tsl",
        "_sl",
        "_tp",
    ]
    .iter()
    .find_map(|suffix| id.strip_suffix(suffix))
    .unwrap_or(id)
}

fn stored(store: &Store, id: &str) -> bool {
    store.order(id).is_some() || store.order(native(id)).is_some()
}

fn named_ids(command: &Command) -> &[String] {
    match command {
        Command::CancelOrders { ids, .. } => ids,
        Command::AmendOrder { id, .. } => std::slice::from_ref(id),
        _ => &[],
    }
}

impl PlacementAcks {
    pub fn placing(&mut self, command: CommandId) {
        self.placing.insert(command);
    }

    /// A placement finished; on success its result lists the venue ids.
    pub fn finished(&mut self, command: CommandId, result: Option<&str>, now: f64) {
        if !self.placing.remove(&command) {
            return;
        }
        let Some(ids) = result.and_then(|json| serde_json::from_str::<Vec<String>>(json).ok())
        else {
            return;
        };
        let expiry = now + ACK_WAIT_MS;
        for id in ids.into_iter().filter(|id| !id.is_empty()) {
            self.acked.push((id, expiry));
        }
    }

    fn awaited(&self, store: &Store, command: &Command, now: f64) -> bool {
        named_ids(command).iter().any(|id| {
            !stored(store, id)
                && self
                    .acked
                    .iter()
                    .any(|(acked, expiry)| *expiry > now && (acked == id || acked == native(id)))
        })
    }

    /// Whether to hold `command` until its acknowledged ids are stored.
    pub fn hold(&mut self, store: &Store, command: &Command, now: f64) -> bool {
        if !self.awaited(store, command, now) {
            return false;
        }
        self.held.push((command.clone(), now + ACK_WAIT_MS));
        true
    }

    /// Commands whose ids are now stored or whose wait expired, in arrival order.
    pub fn release(&mut self, store: &Store, now: f64) -> Vec<Command> {
        self.acked
            .retain(|(id, expiry)| *expiry > now && !stored(store, id));
        if self.held.is_empty() {
            return Vec::new();
        }
        let mut ready = Vec::new();
        let mut index = 0;
        while index < self.held.len() {
            let (command, deadline) = &self.held[index];
            if now >= *deadline || !self.awaited(store, command, now) {
                let (command, _) = self.held.remove(index);
                // An expired wait must not hold the same id again on dispatch.
                for id in named_ids(&command) {
                    self.acked
                        .retain(|(acked, _)| acked != id && acked != native(id));
                }
                ready.push(command);
            } else {
                index += 1;
            }
        }
        ready
    }

    /// The earliest held deadline, for the engine's wake-up timer.
    pub fn next_deadline(&self) -> Option<f64> {
        self.held
            .iter()
            .map(|(_, deadline)| *deadline)
            .reduce(f64::min)
    }
}
