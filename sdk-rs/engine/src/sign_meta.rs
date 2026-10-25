//! Read-only metadata for the current managed signing task, for hosted
//! signing. See docs/plans/RUST_ENGINE_SIGN_META.md.
use crate::credential_selection::CredentialRole;
use crate::{CredentialSlot, RequestId};

/// Who owns a signing task: a managed request's attempt, or a Session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignOwner {
    Managed { request: RequestId, generation: u64 },
    Session { generation: u64 },
}

/// Engine-retained facts about one signing task. Grants no authority.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SignMeta {
    pub task: RequestId,
    pub owner: SignOwner,
    /// The task's absolute bound in raw UTC milliseconds, frozen at first
    /// activation and never renewed by a re-sign.
    pub deadline_ms: f64,
    pub slot: CredentialSlot,
    pub role: CredentialRole,
}
