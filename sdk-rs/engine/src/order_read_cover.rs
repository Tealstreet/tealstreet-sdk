//! Which venue read the source's order rows are as fresh as, and which reads
//! may still replace them (docs/plans/RUST_FRAME_FED_READ_COVER.md §4.1).
//!
//! Ordering is by a causal sequence, never wall time: every row change and
//! every accepted read commit advances it, a read records it at its release,
//! and a read whose rows changed after that is refused for them (H8, H10).
//! Send times are kept only for the covers a frame-fed page compares against
//! its fences.
use std::collections::HashMap;

/// A read's release: the row sequence then, and its send time on this
/// engine's clock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrderReadMark {
    pub seq: u64,
    pub sent: f64,
}

/// What one read commit speaks for.
#[derive(Clone, Copy, Debug)]
pub enum OrderReadScope<'a> {
    /// One scope of a complete account read (`key`), whose symbols `in_scope`
    /// selects. The account is complete when every key in `account` has
    /// committed; its cover is the oldest of their sends (H9).
    Account {
        key: &'a str,
        account: &'a [&'a str],
    },
    /// These symbols, and nothing else.
    Symbols(&'a [&'a str]),
}

#[derive(Default)]
pub(crate) struct Covers {
    /// Advanced by every row change and accepted read commit.
    clock: u64,
    /// The sequence of the last full row replacement (every symbol changed).
    full: u64,
    /// The sequence of each symbol's last row change. Never removed: a symbol
    /// whose rows are gone keeps the proof that refuses an older read.
    symbols: HashMap<Box<str>, u64>,
    /// The sequence and send of each account scope's last accepted commit.
    scopes: HashMap<Box<str>, (u64, f64)>,
    /// The oldest accepted send across the declared account scopes.
    account: Option<f64>,
    /// Per-symbol reads newer than `account`.
    symbol_sent: HashMap<Box<str>, f64>,
}

impl Covers {
    /// The sequence a read released now records.
    pub(crate) fn sequence(&self) -> u64 {
        self.clock
    }

    pub(crate) fn mutated_all(&mut self) {
        self.clock += 1;
        self.full = self.clock;
    }

    pub(crate) fn mutated(&mut self, symbol: &str) {
        self.clock += 1;
        self.stamp(symbol, self.clock);
    }

    fn stamp(&mut self, symbol: &str, seq: u64) {
        match self.symbols.get_mut(symbol) {
            Some(held) => *held = seq,
            None => {
                self.symbols.insert(symbol.into(), seq);
            }
        }
    }

    /// Whether `symbol`'s rows changed after `read` was released.
    fn changed_since(&self, symbol: &str, read: OrderReadMark) -> bool {
        self.full > read.seq || self.symbols.get(symbol).is_some_and(|seq| *seq > read.seq)
    }

    /// The rows a read may replace, and its refusals, decided before anything
    /// changes. An account scope committed after this read refuses all of it.
    pub(crate) fn admit<'o>(
        &self,
        read: OrderReadMark,
        scope: OrderReadScope<'_>,
        symbols: impl Iterator<Item = &'o str>,
    ) -> Admission {
        let whole_scope_newer = match scope {
            OrderReadScope::Account { key, .. } => {
                self.full > read.seq || self.scopes.get(key).is_some_and(|(seq, _)| *seq > read.seq)
            }
            // Membership is unknown here: any later account commit may cover it.
            OrderReadScope::Symbols(_) => {
                self.full > read.seq || self.scopes.values().any(|(seq, _)| *seq > read.seq)
            }
        };
        let mut refused: Vec<String> = Vec::new();
        for symbol in symbols {
            if (whole_scope_newer || self.changed_since(symbol, read))
                && !refused.iter().any(|known| known == symbol)
            {
                refused.push(symbol.to_owned());
            }
        }
        Admission {
            whole_scope_refused: whole_scope_newer,
            refused,
        }
    }

    /// Record an accepted commit: the symbols it wrote (changed or not, so an
    /// older read is refused against them), and the cover it proves. A scope
    /// with any refused symbol proves no account cover. Returns whether the
    /// account cover is now known.
    pub(crate) fn committed(
        &mut self,
        read: OrderReadMark,
        scope: OrderReadScope<'_>,
        written: &[String],
        clean: bool,
    ) -> bool {
        self.clock += 1;
        let revision = self.clock;
        for symbol in written {
            self.stamp(symbol, revision);
        }
        match scope {
            OrderReadScope::Account { key, account } => {
                // Even refused in part, the scope's commit is newer than any
                // read released before it.
                let sent = if clean {
                    read.sent
                } else {
                    self.scopes
                        .get(key)
                        .map_or(f64::NEG_INFINITY, |(_, sent)| *sent)
                };
                self.scopes.insert(key.into(), (revision, sent));
                let mut oldest = f64::INFINITY;
                for key in account {
                    match self.scopes.get(*key) {
                        Some((_, sent)) if sent.is_finite() => oldest = oldest.min(*sent),
                        _ => return self.account.is_some(),
                    }
                }
                if self.account.is_none_or(|current| oldest > current) {
                    self.account = Some(oldest);
                    self.symbol_sent.retain(|_, sent| *sent > oldest);
                }
                true
            }
            OrderReadScope::Symbols(symbols) => {
                for symbol in symbols {
                    if written.iter().any(|w| w == symbol) || !self.changed_since(symbol, read) {
                        let sent = self
                            .symbol_sent
                            .entry((*symbol).into())
                            .or_insert(read.sent);
                        *sent = sent.max(read.sent);
                    }
                }
                if let Some(account) = self.account {
                    self.symbol_sent.retain(|_, sent| *sent > account);
                }
                self.account.is_some()
            }
        }
    }

    pub(crate) fn account(&self) -> Option<f64> {
        self.account
    }

    pub(crate) fn symbol_covers(&self) -> impl Iterator<Item = (&str, f64)> {
        self.symbol_sent
            .iter()
            .map(|(symbol, sent)| (&**symbol, *sent))
    }

    /// Whether any cover exists to send beside ORDERS.
    pub(crate) fn any(&self) -> bool {
        self.account.is_some() || !self.symbol_sent.is_empty()
    }

    /// The cover for one symbol: its own read, or the account's.
    #[cfg(test)]
    pub(crate) fn cover(&self, symbol: &str) -> Option<f64> {
        let own = self.symbol_sent.get(symbol).copied();
        match (self.account, own) {
            (Some(account), Some(own)) => Some(account.max(own)),
            (account, own) => account.or(own),
        }
    }
}

pub(crate) struct Admission {
    pub whole_scope_refused: bool,
    pub refused: Vec<String>,
}
