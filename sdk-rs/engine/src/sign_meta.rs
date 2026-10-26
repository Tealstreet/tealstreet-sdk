//! Read-only facts for a current signing task; never authority.
use crate::credential_selection::CredentialRole;
use crate::{CredentialSlot, RequestId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupOperation {
    PublicAddress,
}
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SignMeta {
    pub task: RequestId,
    pub deadline_ms: f64,
    pub kind: SignKind,
}
