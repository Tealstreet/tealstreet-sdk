//! Live proof access before destroying a frame-fed page's Partial protection.
use super::{Axis, Page, PartialStopGuard, matches_original, proof};
use crate::trading::Planned;
use crate::trading::acknowledgement::{self as ack, Answer};
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
        let mut current = guard.clone();
        current.original.last_update = self.guard.original.last_update;
        current == self.guard
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
        let failed = || {
            VenueError::new("Native protective-edit proof is unavailable or changed; your protection is unchanged")
                .kind(ErrorKind::Network)
                .outcome(Outcome::NotSent)
        };
        if response.status != 200 {
            return Err(failed());
        }
        let Answer::Success {
            result: Some(raw), ..
        } = ack::answer(response.body).map_err(|_| failed())?
        else {
            return Err(failed());
        };
        let page: Page = serde_json::from_str(raw.get()).map_err(|_| failed())?;
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
            if page.list.len() != 1
                || !page.next_page_cursor.is_empty()
                || !live_original(&page.list[0])
            {
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
