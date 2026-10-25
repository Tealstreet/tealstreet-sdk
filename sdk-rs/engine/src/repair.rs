//! Engine-issued repair authority for managed cancel and cancel-then-place
//! workflows that outlive their discovery callback. See
//! docs/plans/RUST_REPAIR_MUTATION_AUTHORITY.md.
//!
//! Inert declarations for the original controls; source follows reviewed REDs.
use crate::request_context::{Context, Credential};
use crate::{CommandId, Cx, RequestId};

/// Spendable repair authority. Opaque; only the issuing engine validates it.
#[derive(Debug, PartialEq, Eq)]
pub struct Entitlement {
    pub(crate) id: u64,
}

/// One admitted physical workflow spending one or more subjects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepairRoot {
    pub(crate) id: u64,
}

/// What an entitlement may act on: a native client or order ID and the
/// adapter's discovery generation for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Subject {
    pub key: String,
    pub generation: u64,
}

/// The validated native order a subject resolves to at root admission.
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
    Spent,
    Retired,
    /// Members disagree on issuer, primary, intent, origin kind or side.
    Incompatible,
    TooMany,
    Capacity,
    /// The declared primary credential is not the installed primary.
    NoPrimary,
    /// The Read's credential is not owned by this engine.
    NotOwned,
    Invalid,
}

/// Adapter-declared reason an unspent subject no longer needs repair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Retire {
    /// The Store shows the bound order filled, cancelled or rejected.
    Terminal,
    /// A complete definitive order read postdating the subject generation.
    CompleteRead,
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

impl<P> Cx<'_, P> {
    /// Mint cancel authority for subjects owed by the current admitted command.
    pub fn mint_command_repair(
        &mut self,
        _primary: Credential,
        _subjects: Vec<Subject>,
    ) -> Result<Entitlement, RepairRefusal> {
        Err(RepairRefusal::Invalid)
    }

    /// Mint cancel-then-place authority for one published snapshot order.
    pub fn mint_snapshot_repair(
        &mut self,
        _read: &Context,
        _primary: Credential,
        _subject: Subject,
        _symbol: &str,
    ) -> Result<Entitlement, RepairRefusal> {
        Err(RepairRefusal::Invalid)
    }

    /// Spend compatible subjects as one cancel request.
    pub fn admit_repair_cancel(
        &mut self,
        _members: &[(&Entitlement, &Subject, Binding<'_>)],
    ) -> Result<(RepairRoot, Context), RepairRefusal> {
        Err(RepairRefusal::Invalid)
    }

    /// The cancel leg of one snapshot subject's cancel-then-place.
    pub fn admit_repair_replace(
        &mut self,
        _entitlement: &Entitlement,
        _subject: &Subject,
        _binding: Binding<'_>,
    ) -> Result<(RepairRoot, Context), RepairRefusal> {
        Err(RepairRefusal::Invalid)
    }

    /// The single placement leg after an engine-recorded accepted cancel.
    pub fn repair_successor(&mut self, _root: &RepairRoot) -> Result<Context, RepairRefusal> {
        Err(RepairRefusal::Invalid)
    }

    pub fn retire_repair_subject(
        &mut self,
        _entitlement: &Entitlement,
        _key: &str,
        _proof: Retire,
    ) -> Result<(), RepairRefusal> {
        Err(RepairRefusal::Invalid)
    }

    /// Unspent subjects still covered by this entitlement.
    pub fn repair_subjects(&self, _entitlement: &Entitlement) -> Vec<Subject> {
        Vec::new()
    }
}
