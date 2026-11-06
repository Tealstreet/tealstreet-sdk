//! Read-only metadata for the current signing task, for hosted signing. See
//! docs/plans/RUST_ENGINE_SIGN_META.md.
use crate::credential_selection::CredentialRole;
use crate::{CredentialSlot, RequestId};

/// The setup operation a setup signing task performs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupOperation {
    PublicAddress,
    PublicKey(crate::signing::Algorithm),
}

/// Who owns a signing task, and what it may be: a business role belongs only
/// to a managed request or a Session, and a setup task carries no role.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignKind {
    Managed {
        request: RequestId,
        generation: u64,
        slot: CredentialSlot,
        role: CredentialRole,
    },
    Session {
        generation: u64,
        slot: CredentialSlot,
        role: CredentialRole,
    },
    Setup {
        generation: u64,
        operation: SetupOperation,
        slot: CredentialSlot,
    },
}

/// Engine-retained facts about one signing task. Grants no authority.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SignMeta {
    pub task: RequestId,
    /// The task's absolute bound in raw UTC milliseconds, frozen at first
    /// activation and never renewed by a re-sign.
    pub deadline_ms: f64,
    pub kind: SignKind,
}
