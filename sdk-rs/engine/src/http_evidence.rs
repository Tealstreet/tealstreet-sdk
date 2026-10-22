//! One retained-evidence budget, independent of native execution classification.
use super::WriteResult;
use crate::VenueError;
use std::collections::HashSet;

pub(super) const MAX_ATTEMPTS: usize = 256;
const ID_COUNT: usize = 4_096;
const ID_BYTES: usize = 256 * 1024;
const ID_FIELD: usize = 1_024;
const TEXT_BYTES: usize = 64 * 1024;
const TEXT_FIELD: usize = 4_096;
const TRUNCATED: &str = "[truncated]";
const ID_OVERFLOW: &str = " [truncated IDs: reconcile remaining identities through reads]";

/// Entry budgets include duplicates and historical attempts. At overload,
/// retained prefixes follow admission order, not logical error selection order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct EvidenceBudget {
    ids: usize,
    id_bytes: usize,
    text_bytes: usize,
}

impl EvidenceBudget {
    pub(super) fn result(mut self, result: WriteResult) -> (Self, WriteResult, bool) {
        let (result, overflow) = match result {
            WriteResult::Accepted { order_ids } => {
                let (order_ids, overflow) = self.identities(order_ids);
                (WriteResult::Accepted { order_ids }, overflow)
            }
            WriteResult::Rejected(error) => {
                let (error, overflow) = self.normalize_error(error);
                (WriteResult::Rejected(error), overflow)
            }
            WriteResult::Failed(error) => {
                let (error, overflow) = self.normalize_error(error);
                (WriteResult::Failed(error), overflow)
            }
        };
        (self, result, overflow)
    }

    pub(super) fn error(
        mut self,
        mut error: VenueError,
        known: &HashSet<&str>,
    ) -> (Self, VenueError, bool) {
        let mut overflow = false;
        if let Some(partial) = error.partial.as_mut() {
            let mut retained = Vec::new();
            let mut copied = HashSet::new();
            for id in std::mem::take(&mut partial.order_ids) {
                // Borrow only canonical native identities. Neither this index
                // nor the terminal projection retains an unbounded fallback.
                if let Some(&native) = known.get(id.as_str()) {
                    if copied.insert(native) {
                        retained.push(native.to_owned());
                    }
                } else if self.admit_identity(&id) {
                    retained.push(id.into_boxed_str().into_string());
                } else {
                    overflow = true;
                    break;
                }
            }
            partial.order_ids = retained.into_boxed_slice().into_vec();
        }
        self.error_text(&mut error);
        (self, error, overflow)
    }

    fn admit_identity(&mut self, id: &str) -> bool {
        let bytes = std::mem::size_of::<String>() + id.len();
        if self.ids == ID_COUNT || id.len() > ID_FIELD || bytes > ID_BYTES - self.id_bytes {
            return false;
        }
        self.ids += 1;
        self.id_bytes += bytes;
        true
    }

    fn identities(&mut self, ids: Vec<String>) -> (Vec<String>, bool) {
        let mut retained = Vec::new();
        let mut overflow = false;
        for id in ids {
            if !self.admit_identity(&id) {
                overflow = true;
                break;
            }
            // Box round-trip discards spare capacity instead of retaining an
            // arbitrarily large allocation for a small native identifier.
            retained.push(id.into_boxed_str().into_string());
        }
        (retained.into_boxed_slice().into_vec(), overflow)
    }

    fn normalize_error(&mut self, mut error: VenueError) -> (VenueError, bool) {
        self.error_text(&mut error);
        let mut overflow = false;
        if let Some(partial) = error.partial.as_mut() {
            (partial.order_ids, overflow) = self.identities(std::mem::take(&mut partial.order_ids));
        }
        (error, overflow)
    }

    fn error_text(&mut self, error: &mut VenueError) {
        error.message = self.text(&error.message);
        error.venue_code = error.venue_code.as_ref().map(|value| self.text(value));
        error.venue_message = error.venue_message.as_ref().map(|value| self.text(value));
    }

    fn text(&mut self, source: &str) -> String {
        let remaining = TEXT_BYTES - self.text_bytes;
        if source.len() <= TEXT_FIELD && source.len() <= remaining {
            self.text_bytes += source.len();
            return source.to_owned().into_boxed_str().into_string();
        }
        let prefix = utf8_prefix(source, remaining.min(TEXT_FIELD - TRUNCATED.len()));
        self.text_bytes += prefix.len();
        // Marker bytes have a separate fixed ceiling: three fields per bounded
        // attempt plus one terminal fallback. No text is charged twice at seal.
        format!("{prefix}{TRUNCATED}")
            .into_boxed_str()
            .into_string()
    }
}

fn utf8_prefix(source: &str, limit: usize) -> &str {
    let mut end = source.len().min(limit);
    while !source.is_char_boundary(end) {
        end -= 1;
    }
    &source[..end]
}

pub(super) fn default_report(message: &str) -> String {
    EvidenceBudget::default().text(message)
}

/// Retain the representative native message and add the explicit reason that a
/// sent command cannot seal success. This is final presentation, not new facts.
pub(super) fn mark_id_overflow(error: &mut VenueError) {
    let prefix = utf8_prefix(&error.message, TEXT_FIELD - ID_OVERFLOW.len());
    error.message = format!("{prefix}{ID_OVERFLOW}")
        .into_boxed_str()
        .into_string();
}
