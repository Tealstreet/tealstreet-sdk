//! Admission for adapters that prepare replacements before dispatching writes.
use crate::{Command, CommandId, Store};
use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct OrderLocks {
    active: Vec<(CommandId, Scope)>,
    /// Deferred cancel-all commands (ID and symbol): failed flag and the
    /// caller-wait receipt each carries. Only the fields are kept, so no
    /// whole Command is cloned.
    waiting: Vec<(CommandId, String, bool, Option<u64>)>,
    uncertain: HashMap<String, u64>,
    epoch: u64,
}

struct Scope {
    placing: bool,
    sent: bool,
    managed: bool,
    unknown_symbol: bool,
    // An absent-order answer arrived while this placement still held its
    // lock: its reconcile read waits for finish, when a read can clear it.
    absent: bool,
    ids: Vec<String>,
    symbols: Vec<String>,
    all_symbol: Option<String>,
}

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

impl OrderLocks {
    pub fn contains(&self, command: CommandId) -> bool {
        self.active.iter().any(|(id, _)| *id == command)
            || self.waiting.iter().any(|(id, ..)| *id == command)
    }

    pub fn acquire(
        &mut self,
        command: &Command,
        store: &Store,
        fence_uncertain: bool,
    ) -> Result<(), CommandId> {
        let mut scope = Scope {
            ids: Vec::new(),
            symbols: Vec::new(),
            all_symbol: None,
            placing: false,
            sent: false,
            managed: false,
            unknown_symbol: false,
            absent: false,
        };
        let input = command;
        let command = match command {
            Command::PlaceOrders { command, orders } => {
                scope.placing = true;
                scope
                    .symbols
                    .extend(orders.iter().map(|o| o.symbol.clone()));
                *command
            }
            Command::AmendOrderWithSymbol {
                command,
                id,
                symbol,
                ..
            } => {
                scope.ids.push(id.clone());
                scope.symbols.push(symbol.clone());
                *command
            }
            Command::AmendOrder { command, id, .. } => {
                scope.ids.push(id.clone());
                *command
            }
            Command::CancelOrders { command, ids } => {
                scope.ids.clone_from(ids);
                *command
            }
            Command::CancelOrdersWithSymbols { command, orders } => {
                scope.ids.extend(orders.iter().map(|o| o.id.clone()));
                scope
                    .symbols
                    .extend(orders.iter().map(|o| o.symbol.clone()));
                *command
            }
            Command::CancelSymbolOrders { command, symbol } => {
                scope.all_symbol = Some(symbol.clone());
                *command
            }
            _ => return Ok(()),
        };
        for id in &mut scope.ids {
            let normalized = native(id);
            let order = store.order(id).or_else(|| store.order(normalized));
            if let Some(order) = order {
                scope.symbols.push(order.symbol.clone());
                *id = native(order.parent_id.as_deref().unwrap_or(&order.id)).to_owned();
            } else {
                let routed = matches!(input, Command::CancelOrdersWithSymbols { orders, .. }
                    if orders.iter().any(|order| &order.id == id && !order.symbol.is_empty()))
                    || matches!(input, Command::AmendOrderWithSymbol { symbol, .. } if !symbol.is_empty());
                scope.unknown_symbol |= !routed;
                *id = normalized.to_owned();
            }
        }
        let overlaps_symbol = |all: &str, other: &Scope| {
            all.is_empty()
                || other.unknown_symbol
                || other.symbols.iter().any(|s| s == all)
                || other
                    .all_symbol
                    .as_ref()
                    .is_some_and(|s| s.is_empty() || s == all)
        };
        if fence_uncertain
            && scope
                .all_symbol
                .as_deref()
                .is_some_and(|symbol| self.uncertain(symbol))
            || self.active.len() >= 64
            || self.active.iter().any(|(_, held)| {
                scope.ids.iter().any(|id| held.ids.contains(id))
                    || scope
                        .all_symbol
                        .as_ref()
                        .is_some_and(|s| overlaps_symbol(s, held))
                    || held
                        .all_symbol
                        .as_ref()
                        .is_some_and(|s| overlaps_symbol(s, &scope))
            })
        {
            return Err(command);
        }
        self.active.push((command, scope));
        Ok(())
    }

    pub fn defer(&mut self, command: &Command, receipt: Option<u64>) -> bool {
        if let Command::CancelSymbolOrders { command, symbol } = command
            && self.active.iter().any(|(_, held)| {
                held.placing && (symbol.is_empty() || held.symbols.contains(symbol))
            })
        {
            if self.waiting.len() >= 64 {
                return false;
            }
            self.waiting
                .push((*command, symbol.clone(), false, receipt));
            return true;
        }
        false
    }

    /// Caller-wait receipts the deferred commands carry.
    pub fn carries(&self, receipt: u64) -> bool {
        self.waiting.iter().any(|(.., held)| *held == Some(receipt))
    }

    pub fn ready(&mut self) -> Vec<(Command, bool, Option<u64>)> {
        let (active, uncertain) = (&self.active, &self.uncertain);
        self.waiting
            .extract_if(.., |(_, symbol, ..)| {
                !active.iter().any(|(_, held)| {
                    held.placing && (symbol.is_empty() || held.symbols.contains(symbol))
                })
            })
            .map(|(command, symbol, failed, receipt)| {
                let failed = failed
                    || uncertain.contains_key("")
                    || uncertain.contains_key(&symbol)
                    || symbol.is_empty() && !uncertain.is_empty();
                (
                    Command::CancelSymbolOrders { command, symbol },
                    failed,
                    receipt,
                )
            })
            .collect()
    }

    /// Returns the symbols this finish newly marks uncertain (an empty symbol
    /// means every symbol); the engine schedules their reconcile read.
    pub fn finish(&mut self, command: CommandId, failed: bool) -> Vec<String> {
        let mut marked = Vec::new();
        if failed
            && let Some((_, scope)) = self
                .active
                .iter()
                .find(|(id, scope)| *id == command && scope.placing && scope.sent)
        {
            self.epoch = self.epoch.saturating_add(1);
            self.uncertain.extend(
                scope
                    .symbols
                    .iter()
                    .cloned()
                    .map(|symbol| (symbol, self.epoch)),
            );
            marked.clone_from(&scope.symbols);
            if scope.symbols.is_empty() {
                self.uncertain.insert(String::new(), self.epoch);
                marked.push(String::new());
            }
            for (_, symbol, failed, _) in &mut self.waiting {
                if symbol.is_empty() || scope.symbols.contains(symbol) {
                    *failed = true;
                }
            }
        }
        self.active.retain(|(id, _)| *id != command);
        marked
    }
    /// Fence symbols (`None`: every symbol) until a fresh order read clears
    /// them through `reconciled`: an absent-order answer proved the order is not
    /// open, not that it was cancelled.
    pub fn fence(&mut self, symbols: Option<&[String]>) {
        self.epoch = self.epoch.saturating_add(1);
        match symbols {
            Some(symbols) => {
                for symbol in symbols {
                    self.uncertain.insert(symbol.clone(), self.epoch);
                }
            }
            None => {
                self.uncertain.insert(String::new(), self.epoch);
            }
        }
    }

    /// Defer the reconcile read of an absent-order answer to this command's
    /// finish while its placing scope is held (a read now could not clear the
    /// fence). False when the command holds no placing scope.
    pub fn defer_absent(&mut self, command: CommandId) -> bool {
        match self
            .active
            .iter_mut()
            .find(|(id, scope)| *id == command && scope.placing)
        {
            Some((_, scope)) => {
                scope.absent = true;
                true
            }
            None => false,
        }
    }

    pub fn absent_pending(&self, command: CommandId) -> bool {
        self.active
            .iter()
            .any(|(id, scope)| *id == command && scope.absent)
    }

    /// What a reconcile read must cover for this held command: `None` is a
    /// full read (unrouted IDs or an all-symbol cancel), else its symbols.
    pub fn reconcile_scope(&self, command: CommandId) -> Option<Option<Vec<String>>> {
        let (_, scope) = self.active.iter().find(|(id, _)| *id == command)?;
        let mut symbols = scope.symbols.clone();
        match &scope.all_symbol {
            Some(symbol) if symbol.is_empty() => return Some(None),
            Some(symbol) => symbols.push(symbol.clone()),
            None => {}
        }
        if scope.unknown_symbol || symbols.is_empty() || symbols.iter().any(String::is_empty) {
            return Some(None);
        }
        // The engine de-duplicates when it drains; no sort here (wasm size).
        Some(Some(symbols))
    }

    pub fn resolved(&mut self, command: CommandId) {
        if let Some((_, scope)) = self
            .active
            .iter_mut()
            .find(|(id, scope)| *id == command && scope.placing && !scope.managed)
        {
            scope.sent = false;
        }
    }

    pub fn managed_resolved(&mut self, command: CommandId) {
        if let Some((_, scope)) = self
            .active
            .iter_mut()
            .find(|(id, scope)| *id == command && scope.placing && scope.managed)
        {
            scope.sent = false;
        }
    }

    pub fn note_write(&mut self) {
        for (_, scope) in &mut self.active {
            if scope.placing && !scope.managed {
                scope.sent = true;
            }
        }
    }

    /// New owned preflight cannot erase evidence of an earlier legacy send.
    #[cfg(feature = "http-plans")]
    pub fn can_manage_unsent(&self, command: CommandId) -> bool {
        self.active
            .iter()
            .find(|(id, _)| *id == command)
            .is_none_or(|(_, scope)| !scope.sent)
    }

    pub fn manage(&mut self, command: CommandId) {
        if let Some((_, scope)) = self.active.iter_mut().find(|(id, _)| *id == command) {
            scope.managed = true;
        }
    }

    pub fn note_command_write(&mut self, command: CommandId) {
        if let Some((_, scope)) = self.active.iter_mut().find(|(id, _)| *id == command)
            && scope.placing
        {
            scope.sent = true;
        }
    }

    pub fn uncertain(&self, symbol: &str) -> bool {
        self.uncertain.contains_key("")
            || self.uncertain.contains_key(symbol)
            || symbol.is_empty() && !self.uncertain.is_empty()
    }

    pub fn token(&self) -> u64 {
        self.epoch
    }

    pub fn reconciled(&mut self, symbol: Option<&str>, token: u64) {
        if self.active.iter().any(|(_, scope)| {
            scope.placing
                && symbol.is_none_or(|s| {
                    scope
                        .symbols
                        .iter()
                        .any(|held| held.is_empty() || held == s)
                })
        }) {
            return;
        }
        if let Some(symbol) = symbol {
            if self
                .uncertain
                .get(symbol)
                .is_some_and(|epoch| *epoch <= token)
            {
                self.uncertain.remove(symbol);
            }
        } else {
            self.uncertain.retain(|_, epoch| *epoch > token);
        }
    }
}

#[cfg(test)]
mod managed_tests {
    use super::*;

    #[test]
    fn one_managed_release_and_an_opaque_legacy_send_cannot_mark_an_unsent_sibling_sent() {
        let mut locks = OrderLocks::default();
        let store = Store::default();
        for command in [7, 8] {
            locks
                .acquire(
                    &Command::PlaceOrders {
                        command,
                        orders: vec![],
                    },
                    &store,
                    true,
                )
                .unwrap();
            locks.manage(command);
        }
        locks.note_command_write(7);
        locks.note_write(); // An unrelated legacy request cannot supply evidence.
        locks.finish(8, true);
        assert!(
            !locks.uncertain(""),
            "unsent sibling acquired phantom uncertainty"
        );
        locks.finish(7, true);
        assert!(
            locks.uncertain(""),
            "released unresolved write lost its uncertainty"
        );
    }

    #[test]
    fn legacy_placement_uncertainty_is_preserved_until_its_migration() {
        let mut locks = OrderLocks::default();
        locks
            .acquire(
                &Command::PlaceOrders {
                    command: 7,
                    orders: vec![],
                },
                &Store::default(),
                true,
            )
            .unwrap();
        locks.note_write();
        locks.finish(7, true);
        assert!(locks.uncertain(""));
    }
}
