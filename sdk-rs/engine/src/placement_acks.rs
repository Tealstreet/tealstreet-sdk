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
/// After a fill of an unstored id the order may be gone (venues drop filled
/// orders) or partly live with its row still in flight on another topic.
pub(crate) const FILL_GRACE_MS: f64 = 1_500.0;

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
    LEG_SUFFIXES
        .iter()
        .find_map(|suffix| id.strip_suffix(suffix))
        .unwrap_or(id)
}

const LEG_SUFFIXES: [&str; 6] = [
    "__stop_loss",
    "__take_profit",
    "__trailing_stop_loss",
    "_tsl",
    "_sl",
    "_tp",
];

/// Whether the store holds the order `id` names: the row itself, its parent
/// for a virtual leg, its legs (`{id}_sl`), or children of a group `id`.
fn stored(store: &Store, id: &str) -> bool {
    let native = native(id);
    store.orders().iter().any(|order| {
        order.id == id
            || order.id == native
            || order.parent_id.as_deref() == Some(native)
            || LEG_SUFFIXES
                .iter()
                .any(|suffix| order.id.strip_suffix(suffix) == Some(native))
    })
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

    /// A fill shortens the wait to `FILL_GRACE_MS`: a venue may drop a filled
    /// order rather than store it, while a partial fill's row may still follow.
    pub fn filled(&mut self, order_id: &str, now: f64) {
        let grace = now + FILL_GRACE_MS;
        let mut matched = false;
        for (acked, expiry) in &mut self.acked {
            if acked == order_id {
                *expiry = expiry.min(grace);
                matched = true;
            }
        }
        if !matched {
            return;
        }
        for (command, deadline) in &mut self.held {
            if named_ids(command)
                .iter()
                .any(|id| native(id) == order_id || id == order_id)
            {
                *deadline = deadline.min(grace);
            }
        }
    }

    /// The latest expiry among the acknowledged, unstored ids `command` names.
    fn awaited_until(&self, store: &Store, command: &Command, now: f64) -> Option<f64> {
        named_ids(command)
            .iter()
            .filter(|id| !stored(store, id))
            .flat_map(|id| {
                self.acked
                    .iter()
                    .filter(move |(acked, expiry)| {
                        *expiry > now && (acked == id || acked == native(id))
                    })
                    .map(|(_, expiry)| *expiry)
            })
            .reduce(f64::max)
    }

    fn awaited(&self, store: &Store, command: &Command, now: f64) -> bool {
        self.awaited_until(store, command, now).is_some()
    }

    /// Whether to hold `command` until its acknowledged ids are stored. A
    /// command naming an id that an earlier held command names waits behind it,
    /// so writes to one order keep their arrival order.
    pub fn hold(&mut self, store: &Store, command: &Command, now: f64) -> bool {
        let behind = named_ids(command).iter().any(|id| {
            self.held.iter().any(|(held, _)| {
                named_ids(held)
                    .iter()
                    .any(|other| native(other) == native(id))
            })
        });
        let until = self.awaited_until(store, command, now);
        if !behind && until.is_none() {
            return false;
        }
        self.held
            .push((command.clone(), until.unwrap_or(now + ACK_WAIT_MS)));
        true
    }

    /// Commands whose ids are now stored or whose wait expired, in arrival order.
    pub fn release(&mut self, store: &Store, now: f64) -> Vec<Command> {
        if self.held.is_empty() {
            // Runs on every engine event: without held commands only expiry
            // matters, and `hold` checks the store when a command arrives.
            self.acked.retain(|(_, expiry)| *expiry > now);
            return Vec::new();
        }
        self.acked
            .retain(|(id, expiry)| *expiry > now && !stored(store, id));
        let mut ready = Vec::new();
        let mut index = 0;
        while index < self.held.len() {
            let (command, deadline) = &self.held[index];
            // An earlier held write to the same order goes first.
            let behind = self.held[..index].iter().any(|(earlier, _)| {
                named_ids(earlier).iter().any(|other| {
                    named_ids(command)
                        .iter()
                        .any(|id| native(id) == native(other))
                })
            });
            if !behind && (now >= *deadline || !self.awaited(store, command, now)) {
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
