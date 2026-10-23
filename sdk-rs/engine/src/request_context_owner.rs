//! Engine-issued command ownership before final native plan admission.
use super::{Credential, Error, Operation, Owner, Pacing, RetryPolicy};
use crate::CommandId;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreflightPurpose {
    ReadOnly,
    WriteAfterReads,
}

/// Immutable trusted credential intents. A read may be public; a final write
/// must name its private credential before any preflight work starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreflightPolicy {
    pub purpose: PreflightPurpose,
    pub read_credential: Option<Credential>,
    pub write_credential: Option<Credential>,
}

impl PreflightPolicy {
    pub(crate) fn validate(&self) -> Result<(), Error> {
        for credential in [&self.read_credential, &self.write_credential]
            .into_iter()
            .flatten()
        {
            credential.validate()?;
        }
        match self.purpose {
            PreflightPurpose::ReadOnly if self.write_credential.is_some() => Err(Error::Invalid),
            PreflightPurpose::WriteAfterReads if self.write_credential.is_none() => {
                Err(Error::Invalid)
            }
            _ => Ok(()),
        }
    }
}

/// No deadline-bearing dummy Context is created for command preflight. This
/// handle identifies its retained generation; operations require live registry
/// state and cannot be authorized by a cloned handle alone.
#[derive(Clone, Debug)]
pub struct Preflight {
    issuer: Arc<()>,
    owner: Owner,
}

impl Preflight {
    pub(crate) fn new(issuer: &Arc<()>, command: CommandId, generation: u64) -> Self {
        Self {
            issuer: issuer.clone(),
            owner: Owner::Command {
                id: command,
                generation,
            },
        }
    }

    /// A physical owner's retirement token, never a usable preflight.
    pub(crate) fn for_owner(issuer: &Arc<()>, owner: Owner) -> Self {
        Self {
            issuer: issuer.clone(),
            owner,
        }
    }

    pub fn owner(&self) -> Owner {
        self.owner
    }

    pub(crate) fn issued_by(&self, issuer: &Arc<()>) -> bool {
        Arc::ptr_eq(&self.issuer, issuer)
    }
}

/// Common semantics; per-call windows and retry credits come from the plan.
#[derive(Clone, Debug, PartialEq)]
pub struct PlanPolicy {
    /// Independent Mutation calls may settle an unproven reply and continue siblings.
    pub independent_calls: bool,
    pub operation: Operation,
    pub credential: Option<Credential>,
    pub pacing: Option<Pacing>,
    pub retry: RetryPolicy,
}

/// Registry authority is still required for every operation. Reading this
/// immutable raw deadline grants no send, publication or completion permission.
#[derive(Clone, Debug)]
pub struct Plan {
    issuer: Arc<()>,
    owner: Owner,
    deadline: f64,
}

impl Plan {
    pub(crate) fn new(preflight: &Preflight, deadline: f64) -> Self {
        Self {
            issuer: preflight.issuer.clone(),
            owner: preflight.owner,
            deadline,
        }
    }
    /// The same admitted Plan, owned by its execution's physical owner.
    pub(crate) fn with_owner(preflight: &Preflight, owner: Owner, deadline: f64) -> Self {
        Self {
            issuer: preflight.issuer.clone(),
            owner,
            deadline,
        }
    }
    pub fn owner(&self) -> Owner {
        self.owner
    }
    pub fn deadline(&self) -> f64 {
        self.deadline
    }
    pub(crate) fn same_plan(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.issuer, &other.issuer)
            && self.owner == other.owner
            && self.deadline == other.deadline
    }
    pub(crate) fn issued_by(&self, issuer: &Arc<()>) -> bool {
        Arc::ptr_eq(&self.issuer, issuer)
    }
}

/// Exact native continuation cleanup only. It carries no Cx, response, Store,
/// completion or send authority, and is constructed only by the engine.
#[derive(Clone, Debug)]
pub struct HttpOwnerRetirement {
    preflight: Preflight,
}

impl HttpOwnerRetirement {
    pub(crate) fn new(preflight: Preflight) -> Self {
        Self { preflight }
    }

    pub fn matches_preflight(&self, preflight: &Preflight) -> bool {
        Arc::ptr_eq(&self.preflight.issuer, &preflight.issuer)
            && self.preflight.owner == preflight.owner
    }

    pub fn matches_plan(&self, plan: &Plan) -> bool {
        Arc::ptr_eq(&self.preflight.issuer, &plan.issuer) && self.preflight.owner == plan.owner
    }

    pub(crate) fn preflight(&self) -> &Preflight {
        &self.preflight
    }
}
