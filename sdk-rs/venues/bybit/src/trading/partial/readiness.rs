//! Live proof access before destroying a frame-fed page's Partial protection.
use super::{Axis, Page, PartialStopGuard, matches_original, proof};
use crate::trading::{OrderType, Planned};
use safe_cex_engine::{ErrorKind, Outcome, Response, Store, VenueError};

pub(in crate::trading) struct Readiness {
    guard: PartialStopGuard,
    next: usize,
}

impl Readiness {
    pub(in crate::trading) fn new(guard: PartialStopGuard) -> Self {
        Self { guard, next: 0 }
    }

    pub(in crate::trading) fn symbol(&self) -> &str {
        &self.guard.symbol
    }

    pub(in crate::trading) fn matches(&self, guard: &PartialStopGuard) -> bool {
        // validate() compares the stored original; retain the rebuilt edit's target too.
        self.guard.category == guard.category
            && self.guard.position_idx == guard.position_idx
            && self.guard.symbol == guard.symbol
            && self.guard.side == guard.side
            && self.guard.kind == guard.kind
            && self.guard.price == guard.price
            && self.guard.reference == guard.reference
    }

    pub(in crate::trading) fn planned(&self) -> Option<Planned> {
        self.axis()
            .map(|axis| proof(&self.guard.category, &self.guard.original, axis))
    }

    fn axis(&self) -> Option<Axis> {
        [Axis::Target, Axis::Live, Axis::History]
            .get(self.next)
            .copied()
    }

    pub(in crate::trading) fn validate(&self, store: &Store, now: f64) -> Result<(), VenueError> {
        self.guard
            .validate_order(store, false)
            .and_then(|()| self.guard.validate_original(store, now))
            .and_then(|()| self.guard.validate(store, now))
    }

    pub(in crate::trading) fn observe(
        &mut self,
        response: &Response<'_>,
    ) -> Result<(), VenueError> {
        let what = if self.guard.original.kind == OrderType::TakeProfit {
            "take profit"
        } else {
            "stop"
        };
        let failed = || {
            VenueError::new(format!(
                "Couldn't confirm your current {what} with Bybit, so the edit wasn't sent. Your {what} is unchanged; try again."
            ))
            .kind(ErrorKind::Network)
            .outcome(Outcome::NotSent)
        };
        if response.status != 200 {
            return Err(failed());
        }
        let page: Page = super::decode_page(response.body).map_err(|_| failed())?;
        if page.category != self.guard.category
            || page.list.len() > 50
            || page.list.iter().any(|row| row.symbol != self.guard.symbol)
        {
            return Err(failed());
        }
        let live_original = |row: &super::NativeStop| {
            matches!(row.order_status.as_str(), "Untriggered" | "New")
                && matches_original(row, &self.guard)
        };
        if self.axis() == Some(Axis::Target) {
            // AGT-108: as in the post-cancel proof (AGT-34), an orderId names at most one
            // order, so `nextPageCursor` is only a page token; live Bybit returns one
            // beside the single row, and refusing on it blocked every frame-fed edit.
            if page.list.len() != 1 || !live_original(&page.list[0]) {
                return Err(failed());
            }
        } else if page
            .list
            .iter()
            .any(|row| row.order_id == self.guard.original.id && !live_original(row))
        {
            return Err(failed());
        }
        self.next += 1;
        Ok(())
    }
}
