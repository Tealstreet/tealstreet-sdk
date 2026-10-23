//! Cancellation of an acknowledged placement before the store sees it.
//!
//! A placement resolves with venue ids before the private stream (or the next
//! reconciliation) adds those orders to the store. A caller that cancels or
//! amends at once (a chaser stopped mid-placement) names an id the store does
//! not hold, and venues that need a stored order to route the write refuse it,
//! leaving the order live. Such a command waits here until every acknowledged
//! id it names is stored, or until `ACK_WAIT_MS` passes; it is then dispatched
//! unchanged, so an id that never appears still fails explicitly at the venue.

use crate::engine::{Action, Fill, Notice};
use crate::{Command, CommandId, Store};

/// How long an acknowledged id may wait for the store before a command naming
/// it is dispatched anyway.
pub(crate) const ACK_WAIT_MS: f64 = 10_000.0;
/// After a fill of an unstored id the order may be gone (venues drop filled
/// orders) or partly live with its row still in flight on another topic.
pub(crate) const FILL_GRACE_MS: f64 = 1_500.0;

#[derive(Default)]
pub(crate) struct PlacementAcks {
    placing: Vec<CommandId>,
    /// Acknowledged ids not yet seen in the store, with their expiry.
    acked: Acked,
    /// Held commands, each with a hard cap; the live deadline is computed from
    /// the acknowledgements it still awaits, so it never goes stale. Each
    /// keeps the caller-wait receipt it arrived with.
    held: Vec<(Command, f64, Option<u64>)>,
    /// Store order count at the last prune, to notice removals cheaply.
    pruned_orders: usize,
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

/// Every identity the store answers for, built once per pass: each row's id,
/// the order its virtual leg belongs to (`{id}_sl` → `id`) and its group parent.
/// Sorted vectors rather than hashed containers keep this off the venue wasm
/// size budget (each hashed instantiation adds code to every opted-in venue).
struct Stored<'a>(Vec<&'a str>);

impl<'a> Stored<'a> {
    fn of(store: &'a Store) -> Self {
        let mut ids = Vec::with_capacity(store.orders().len() * 2);
        for order in store.orders() {
            ids.push(order.id.as_str());
            ids.push(native(&order.id));
            if let Some(parent) = order.parent_id.as_deref() {
                ids.push(parent);
            }
        }
        ids.sort_unstable();
        Self(ids)
    }

    /// Whether the store holds the order `id` names, its legs or its children.
    fn has(&self, id: &str) -> bool {
        self.0.binary_search(&id).is_ok() || self.0.binary_search(&native(id)).is_ok()
    }
}

/// Acknowledged ids and their expiry, sorted by id.
#[derive(Default)]
struct Acked(Vec<(String, f64)>);

impl Acked {
    fn find(&self, id: &str) -> Result<usize, usize> {
        self.0.binary_search_by(|(acked, _)| acked.as_str().cmp(id))
    }

    fn get(&self, id: &str) -> Option<f64> {
        self.find(id).ok().map(|index| self.0[index].1)
    }

    fn insert(&mut self, id: String, expiry: f64) {
        match self.find(&id) {
            Ok(index) => self.0[index].1 = expiry,
            Err(index) => self.0.insert(index, (id, expiry)),
        }
    }

    fn remove(&mut self, id: &str) {
        if let Ok(index) = self.find(id) {
            self.0.remove(index);
        }
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn retain(&mut self, mut keep: impl FnMut(&str, f64) -> bool) {
        self.0.retain(|(id, expiry)| keep(id, *expiry));
    }
}

/// The ids in a placement result: a JSON array of plain strings. Anything else
/// (an object, escapes) is not tracked, so its cancels are never held.
fn result_ids(json: &str) -> Option<Vec<String>> {
    let inner = json.trim().strip_prefix('[')?.strip_suffix(']')?.trim();
    if inner.is_empty() {
        return Some(Vec::new());
    }
    inner
        .split(',')
        .map(|item| {
            let id = item.trim().strip_prefix('"')?.strip_suffix('"')?;
            (!id.contains(['"', '\\'])).then(|| id.to_owned())
        })
        .collect()
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
        self.placing.push(command);
    }

    /// A placement finished; on success its result lists the venue ids.
    pub fn finished(&mut self, command: CommandId, result: Option<&str>, now: f64) {
        let Some(index) = self.placing.iter().position(|placing| *placing == command) else {
            return;
        };
        self.placing.swap_remove(index);
        let Some(ids) = result.and_then(result_ids) else {
            return;
        };
        let expiry = now + ACK_WAIT_MS;
        for id in ids.into_iter().filter(|id| !id.is_empty()) {
            self.acked.insert(id, expiry);
        }
    }

    /// Track this event's placement outcomes and fills. Non-generic and out of
    /// line, so the opted-in venue engines share one copy in a combined wasm.
    #[inline(never)]
    pub fn observe(
        &mut self,
        actions: &[Action],
        fills: &[(Fill, Option<crate::engine::FillFinancials>)],
        now: f64,
    ) {
        for action in actions {
            match action {
                Action::Notice(Notice::CommandDone { command, result }) => {
                    self.finished(*command, Some(result.as_str()), now);
                }
                Action::Notice(Notice::CommandFailed { command, .. }) => {
                    self.finished(*command, None, now);
                }
                _ => {}
            }
        }
        for (fill, _) in fills {
            self.filled(&fill.order_id, now);
        }
    }

    /// A fill shortens the wait to `FILL_GRACE_MS`: a venue may drop a filled
    /// order rather than store it, while a partial fill's row may still follow.
    pub fn filled(&mut self, order_id: &str, now: f64) {
        if let Some(expiry) = self.acked.get(order_id) {
            self.acked
                .insert(order_id.to_owned(), expiry.min(now + FILL_GRACE_MS));
        }
    }

    /// The latest expiry among the acknowledged, unstored ids `command` names.
    fn awaited_until(&self, stored: &Stored<'_>, command: &Command, now: f64) -> Option<f64> {
        named_ids(command)
            .iter()
            .filter(|id| !stored.has(id))
            .filter_map(|id| {
                [id.as_str(), native(id)]
                    .into_iter()
                    .filter_map(|key| self.acked.get(key))
                    .filter(|expiry| *expiry > now)
                    .reduce(f64::max)
            })
            .reduce(f64::max)
    }

    /// Whether to hold `command` until its acknowledged ids are stored. A
    /// command naming an id that an earlier held command names waits behind it,
    /// so writes to one order keep their arrival order.
    #[inline(never)]
    pub fn hold(
        &mut self,
        store: &Store,
        command: &Command,
        receipt: Option<u64>,
        now: f64,
    ) -> bool {
        let behind = named_ids(command).iter().any(|id| {
            self.held.iter().any(|(held, ..)| {
                named_ids(held)
                    .iter()
                    .any(|other| native(other) == native(id))
            })
        });
        if !behind
            && (self.acked.is_empty()
                || self
                    .awaited_until(&Stored::of(store), command, now)
                    .is_none())
        {
            return false;
        }
        self.held
            .push((command.clone(), now + ACK_WAIT_MS, receipt));
        true
    }

    fn prune(&mut self, stored: &Stored<'_>, now: f64) {
        self.acked
            .retain(|id, expiry| expiry > now && !stored.has(id));
    }

    /// Commands whose ids are now stored or whose wait expired, in arrival order.
    /// Runs on every engine event, so with nothing held it scans the store only
    /// when its order count moved (an acknowledged order stored, then removed).
    #[inline(never)]
    pub fn release(&mut self, store: &Store, now: f64) -> Vec<(Command, Option<u64>)> {
        if self.held.is_empty() {
            self.acked.retain(|_, expiry| expiry > now);
            if !self.acked.is_empty() && store.orders().len() != self.pruned_orders {
                self.pruned_orders = store.orders().len();
                self.prune(&Stored::of(store), now);
            }
            return Vec::new();
        }
        let stored = Stored::of(store);
        self.pruned_orders = store.orders().len();
        self.prune(&stored, now);
        let mut ready = Vec::new();
        let mut index = 0;
        while index < self.held.len() {
            let (command, cap, _) = &self.held[index];
            if !self.behind(index)
                && (now >= *cap || self.awaited_until(&stored, command, now).is_none())
            {
                let (command, _, receipt) = self.held.remove(index);
                // An expired wait must not hold the same id again on dispatch.
                for id in named_ids(&command) {
                    self.acked.remove(id);
                    self.acked.remove(native(id));
                }
                ready.push((command, receipt));
            } else {
                index += 1;
            }
        }
        ready
    }

    /// Caller-wait receipts the held commands carry.
    pub fn carries(&self, receipt: u64) -> bool {
        self.held.iter().any(|(.., held)| *held == Some(receipt))
    }

    /// Whether an earlier held write names the same order; it goes first.
    fn behind(&self, index: usize) -> bool {
        let command = &self.held[index].0;
        self.held[..index].iter().any(|(earlier, ..)| {
            named_ids(earlier).iter().any(|other| {
                named_ids(command)
                    .iter()
                    .any(|id| native(id) == native(other))
            })
        })
    }

    /// When the earliest releasable held command is due, for the engine's
    /// wake-up timer. Commands waiting behind another are released with it.
    /// Always later than `now` after `release`, so the timer cannot spin.
    #[inline(never)]
    pub fn next_deadline(&self, store: &Store, now: f64) -> Option<f64> {
        if self.held.is_empty() {
            return None;
        }
        let stored = Stored::of(store);
        (0..self.held.len())
            .filter(|&index| !self.behind(index))
            .map(|index| {
                let (command, cap, _) = &self.held[index];
                self.awaited_until(&stored, command, now)
                    .map_or(*cap, |until| until.min(*cap))
            })
            .reduce(f64::min)
    }
}

#[cfg(test)]
mod tests {
    use super::result_ids;

    #[test]
    fn placement_results_track_plain_id_arrays_only() {
        assert_eq!(
            result_ids(r#"["7","8"]"#),
            Some(vec!["7".into(), "8".into()])
        );
        assert_eq!(result_ids(" [ \"a-1\" ] "), Some(vec!["a-1".into()]));
        assert_eq!(result_ids("[]"), Some(Vec::new()));
        assert_eq!(result_ids(r#"{"orderIds":["7"]}"#), None);
        assert_eq!(result_ids(r#"["a\"b"]"#), None);
        assert_eq!(result_ids("[7]"), None);
    }
}
