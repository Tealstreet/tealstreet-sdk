//! Admission for adapters that prepare replacements before dispatching writes.
use crate::{Command, CommandId, Store};
use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct OrderLocks {
    active: Vec<(CommandId, Scope)>,
    waiting: Vec<(Command, bool)>,
    uncertain: HashMap<String, u64>,
    epoch: u64,
}

struct Scope {
    placing: bool,
    sent: bool,
    unknown_symbol: bool,
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
            || self.waiting.iter().any(|(c, _)| matches!(c, Command::CancelSymbolOrders { command: id, .. } if *id == command))
    }

    pub fn acquire(&mut self, command: &Command, store: &Store) -> Result<(), CommandId> {
        let mut scope = Scope {
            ids: Vec::new(),
            symbols: Vec::new(),
            all_symbol: None,
            placing: false,
            sent: false,
            unknown_symbol: false,
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
        if scope
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

    pub fn defer(&mut self, command: &Command) -> bool {
        if let Command::CancelSymbolOrders { symbol, .. } = command
            && self.active.iter().any(|(_, held)| {
                held.placing && (symbol.is_empty() || held.symbols.contains(symbol))
            })
        {
            if self.waiting.len() >= 64 {
                return false;
            }
            self.waiting.push((command.clone(), false));
            return true;
        }
        false
    }

    pub fn ready(&mut self) -> Vec<(Command, bool)> {
        let mut ready = Vec::new();
        let uncertain = &self.uncertain;
        self.waiting.retain(|(command, failed)| {
            let Command::CancelSymbolOrders { symbol, .. } = command else {
                return false;
            };
            let blocked = self.active.iter().any(|(_, held)| {
                held.placing && (symbol.is_empty() || held.symbols.contains(symbol))
            });
            if !blocked {
                ready.push((
                    command.clone(),
                    *failed
                        || uncertain.contains_key("")
                        || uncertain.contains_key(symbol)
                        || symbol.is_empty() && !uncertain.is_empty(),
                ));
            }
            blocked
        });
        ready
    }

    pub fn finish(&mut self, command: CommandId, failed: bool) {
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
            if scope.symbols.is_empty() {
                self.uncertain.insert(String::new(), self.epoch);
            }
            for (waiting, failed) in &mut self.waiting {
                if matches!(waiting, Command::CancelSymbolOrders { symbol, .. }
                    if symbol.is_empty() || scope.symbols.contains(symbol))
                {
                    *failed = true;
                }
            }
        }
        self.active.retain(|(id, _)| *id != command);
    }
    pub fn resolved(&mut self, command: CommandId) {
        if let Some((_, scope)) = self
            .active
            .iter_mut()
            .find(|(id, scope)| *id == command && scope.placing)
        {
            scope.sent = false;
        }
    }

    pub fn note_write(&mut self) {
        for (_, scope) in &mut self.active {
            if scope.placing {
                scope.sent = true;
            }
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
