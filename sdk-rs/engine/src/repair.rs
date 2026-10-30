//! Engine-issued repair authority for managed cancel and cancel-then-place
//! workflows that outlive their discovery callback. See
//! docs/plans/RUST_REPAIR_MUTATION_AUTHORITY.md.
//!
//! The engine-side custody lives in `engine/repair_core.rs`.
use crate::{CommandId, RequestId};

/// Spendable repair authority. Opaque; only the issuing engine accepts it.
#[derive(Debug)]
pub struct Entitlement {
    pub(crate) issuer: std::sync::Arc<()>,
    pub(crate) id: u64,
}

/// One admitted physical workflow spending one or more subjects.
#[derive(Clone, Debug)]
pub struct RepairRoot {
    pub(crate) issuer: std::sync::Arc<()>,
    pub(crate) id: u64,
}

/// Handles compare by issuing engine and identity, never by number alone.
macro_rules! issued_eq {
    ($($handle:ty),*) => {$(
        impl PartialEq for $handle {
            fn eq(&self, other: &Self) -> bool {
                self.id == other.id && std::sync::Arc::ptr_eq(&self.issuer, &other.issuer)
            }
        }
        impl Eq for $handle {}
    )*};
}
issued_eq!(Entitlement, RepairRoot, SnapshotReceipt);

/// What an entitlement may act on: a native client or order ID and the
/// adapter's discovery generation for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Subject {
    pub key: String,
    pub generation: u64,
}

/// The native order a subject resolves to at root admission. Core checks
/// that the Store publishes `order_id` on `symbol` as the subject's order (by
/// native or client ID). `side` is the adapter's native grouping key (position
/// side); the Store carries no position side, so core only requires that
/// members of one root agree on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding<'a> {
    pub order_id: &'a str,
    pub symbol: &'a str,
    pub side: &'a str,
}

/// Audit origin of an entitlement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepairOrigin {
    Command(CommandId),
    Snapshot(RequestId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepairRefusal {
    /// Command-origin mint outside this command's admitted entry.
    NotInEntry,
    /// Snapshot-origin mint outside the exact live Background DATA delivery.
    NotData,
    /// The snapshot subject is not published in the Store with that symbol.
    StoreMissing,
    /// A live, spent-unresolved or Unknown entitlement already covers it.
    Duplicate,
    /// The snapshot receipt or its tokens no longer match the Store.
    Stale,
    Spent,
    Retired,
    /// Members disagree on issuer, primary, intent, origin kind or side.
    Incompatible,
    /// An order lock held by a command or another root covers it.
    Conflict,
    TooMany,
    Capacity,
    /// The declared primary credential is not the installed primary.
    NoPrimary,
    /// The Read's credential is not owned by this engine.
    NotOwned,
    Invalid,
}

/// An accepted whole Orders snapshot committed inside one exact live
/// Background Read DATA delivery. Only `commit_repair_snapshot` issues it.
#[derive(Debug)]
pub struct SnapshotReceipt {
    pub(crate) issuer: std::sync::Arc<()>,
    pub(crate) id: u64,
}

/// Why an unspent subject no longer needs repair. Core checks each proof.
#[derive(Clone, Copy, Debug)]
pub enum Retire<'a> {
    /// The Store shows the subject's order closed or cancelled.
    Terminal,
    /// A complete accepted snapshot, begun after the subject was minted, that
    /// no longer contains it.
    CompleteRead(&'a SnapshotReceipt),
    /// Frame-fed: the data source's ORDERS frame being ingested speaks for
    /// every write sent before `since` (`Cx::source_orders_cover`) and does not
    /// contain it.
    SourceOrders { since: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Accepted,
    Rejected,
    Unknown,
    NotSent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubjectFact {
    pub key: String,
    pub order_id: Option<String>,
    pub origin: RepairOrigin,
    pub cancel: Effect,
    /// Cancel-then-place only.
    pub place: Option<Effect>,
}

/// Delivered through `Venue::on_repair_notice`, outside any command entry.
#[derive(Debug)]
pub enum RepairNotice<'a> {
    Settled {
        root: &'a RepairRoot,
        subjects: &'a [SubjectFact],
    },
    MintRefused {
        origin: RepairOrigin,
        subjects: &'a [Subject],
        reason: RepairRefusal,
    },
}
