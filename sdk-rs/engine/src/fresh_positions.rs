//! Commands waiting on a positions read issued after they arrived. A venue
//! numbers its reads; a waiter holds, per read key (a product), the first read
//! number that may answer it, so an in-flight read can never settle it.
use crate::engine::Cx;
use crate::model::PositionSide;
use crate::request_context::{CommandEntryError, Context, Operation, Owner};
use crate::{ErrorKind, Outcome, VenueError};
use serde::Serialize;

struct Waiter {
    root: Context,
    legs: Vec<(u8, u32)>,
    /// Symbols of owned rows the reads that satisfied its legs could not represent.
    skipped: Vec<String>,
}

/// A success answer: what the read could not keep, and what it left open.
#[derive(Serialize)]
struct FreshAnswer<'a> {
    skipped: &'a [String],
    open: Vec<OpenRow<'a>>,
}

#[derive(Serialize)]
struct OpenRow<'a> {
    symbol: &'a str,
    side: PositionSide,
    contracts: f64,
}

#[derive(Default)]
pub struct FreshPositions {
    waiters: Vec<Waiter>,
}

/// A local refusal of `readPositions`, nothing having been sent.
pub fn refuse<P>(cx: &mut Cx<'_, P>, error: VenueError) {
    cx.refuse_command(Operation::Read, error.outcome(Outcome::NotSent));
}

/// A read the venue will not issue yet because it paused itself.
pub fn rate_limited(message: &str, retry_after_ms: f64) -> VenueError {
    VenueError::new(message)
        .kind(ErrorKind::RateLimited)
        .retry_after_ms(retry_after_ms)
}

impl FreshPositions {
    /// Register a command, bound by the caller's wait, before its read is issued:
    /// a read refused synchronously must find it. False once it has already ended.
    pub fn admit<P>(&mut self, cx: &mut Cx<'_, P>, legs: Vec<(u8, u32)>) -> bool {
        match cx.admit_caller_read_root(None, None) {
            Ok(root) => {
                self.waiters.push(Waiter {
                    root,
                    legs,
                    skipped: Vec::new(),
                });
                true
            }
            Err(CommandEntryError::Revoked) => {
                let _ = cx.reject_expired_command(Operation::Read);
                false
            }
            Err(CommandEntryError::Rejected(_)) => false,
        }
    }

    /// Some waiter still needs a committed read on `key`.
    pub fn owed(&self, key: u8) -> bool {
        self.waiters
            .iter()
            .any(|waiter| waiter.legs.iter().any(|(leg, _)| *leg == key))
    }

    /// Read `generation` was issued too early for some waiter on `key`.
    pub fn predates(&self, key: u8, generation: u32) -> bool {
        self.waiters.iter().any(|waiter| {
            waiter
                .legs
                .iter()
                .any(|(leg, need)| *leg == key && *need > generation)
        })
    }

    /// Read `generation` committed to the store. `skipped` names rows it dropped
    /// because their market is outside the catalogue. The answer lists them and the
    /// store's nonzero rows at the commit of the waiter's final leg.
    pub fn committed<P>(
        &mut self,
        cx: &mut Cx<'_, P>,
        key: u8,
        generation: u32,
        skipped: &[String],
    ) {
        self.waiters.retain_mut(|waiter| {
            let before = waiter.legs.len();
            waiter
                .legs
                .retain(|(leg, need)| *leg != key || *need > generation);
            if waiter.legs.len() != before {
                for symbol in skipped {
                    if !waiter.skipped.contains(symbol) {
                        waiter.skipped.push(symbol.clone());
                    }
                }
            }
            if !waiter.legs.is_empty() {
                return true;
            }
            let open = cx
                .store
                .positions()
                .iter()
                .filter(|position| position.contracts != 0.0)
                .map(|position| OpenRow {
                    symbol: &position.symbol,
                    side: position.side,
                    contracts: position.contracts,
                })
                .collect();
            let body = serde_json::to_string(&FreshAnswer {
                skipped: &waiter.skipped,
                open,
            });
            let _ = cx.settle_caller_read_root(
                &waiter.root,
                body.map_err(|_| VenueError::new("positions read: unserializable answer")),
            );
            false
        });
    }

    /// Read `generation` failed, or could not speak for every row.
    pub fn failed<P>(&mut self, cx: &mut Cx<'_, P>, key: u8, generation: u32, error: &VenueError) {
        self.waiters.retain(|waiter| {
            if !waiter
                .legs
                .iter()
                .any(|(leg, need)| *leg == key && *need <= generation)
            {
                return true;
            }
            let error = error.clone().outcome(Outcome::NotSent);
            let _ = cx.settle_caller_read_root(&waiter.root, Err(error));
            false
        });
    }

    /// The engine expired this waiter's root and sealed it.
    pub fn retired(&mut self, owner: Owner) {
        self.waiters.retain(|waiter| waiter.root.owner() != owner);
    }
}

#[cfg(test)]
#[path = "fresh_positions_tests.rs"]
mod tests;
