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
    /// Acknowledged ids reported filled: a venue may never store them.
    filled: HashSet<String>,
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

    /// A fill settles the wait: the venue may drop a filled order rather than
    /// store it, and the cancel's answer no longer depends on the stream.
    pub fn filled(&mut self, order_id: &str) {
        if self.acked.iter().any(|(acked, _)| acked == order_id) {
            self.filled.insert(order_id.to_owned());
        }
    }

    fn awaited(&self, store: &Store, command: &Command, now: f64) -> bool {
        named_ids(command).iter().any(|id| {
            !stored(store, id)
                && !self.filled.contains(id)
                && !self.filled.contains(native(id))
                && self
                    .acked
                    .iter()
                    .any(|(acked, expiry)| *expiry > now && (acked == id || acked == native(id)))
        })
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
        if !behind && !self.awaited(store, command, now) {
            return false;
        }
        self.held.push((command.clone(), now + ACK_WAIT_MS));
        true
    }

    /// Commands whose ids are now stored or whose wait expired, in arrival order.
    pub fn release(&mut self, store: &Store, now: f64) -> Vec<Command> {
        self.acked
            .retain(|(id, expiry)| *expiry > now && !stored(store, id));
        let acked = &self.acked;
        self.filled
            .retain(|id| acked.iter().any(|(acked, _)| acked == id));
        if self.held.is_empty() {
            return Vec::new();
        }
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
