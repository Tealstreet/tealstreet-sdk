//! One selected opaque identity through Read admission, queueing and retry.
use crate::credential_selection::{
    CredentialRole, CredentialUnavailable, PrimaryReason, Selection, Slots,
};
use crate::json::ArrayAt;
use crate::request_context::{
    CommandEntryError, Context, Credential, Error, Operation, Pacing, Policy, RetryEvidence,
    RetryPolicy, Scope,
};
use crate::signing::Identity;
use crate::{AccountReadScope, Cx, Method, RequestId, SignStep, VenueError};

pub enum ReadRole {
    Read {
        scope: Option<AccountReadScope>,
    },
    Primary {
        reason: PrimaryReason,
        scope: Option<AccountReadScope>,
    },
}

pub struct ReadPolicy {
    pub timeout_ms: u32,
    pub pacing: Option<Pacing>,
    pub retry: RetryPolicy,
}

impl ReadPolicy {
    fn policy(self, credential: Option<Credential>) -> Policy {
        Policy {
            operation: Operation::Read,
            credential,
            timeout_ms: self.timeout_ms,
            pacing: self.pacing,
            retry: self.retry,
        }
    }
}

pub struct ReadRequest {
    pub method: Method,
    pub url: String,
    pub body: Option<String>,
    pub stream: Option<ArrayAt>,
}

pub struct LocalRefusal {
    pub diagnostic: Option<String>,
    pub error: VenueError,
}

pub enum CommandReadError {
    Entry(CommandEntryError),
    LocalRefusal {
        error: VenueError,
        terminal: Result<(), Error>,
    },
}

pub enum BackgroundReadError {
    Selection(CredentialUnavailable),
    Admission(Error),
}

#[must_use]
pub struct SelectedRead {
    selection: Selection,
    identity: Identity,
    scope: Option<AccountReadScope>,
}

#[must_use]
pub struct AdmittedRead {
    selected: SelectedRead,
    context: Context,
}

#[must_use]
pub struct RetryRefusal<T> {
    pub read: AdmittedRead,
    pub pending: T,
    pub error: Error,
}

impl SelectedRead {
    pub fn select(slots: &Slots<Identity>, role: ReadRole) -> Result<Self, CredentialUnavailable> {
        let (role, scope) = match role {
            ReadRole::Read { scope } => (CredentialRole::Read { scope }, scope),
            ReadRole::Primary { reason, scope } => {
                (CredentialRole::PrimaryRead { reason, scope }, scope)
            }
        };
        let (selection, identity) = slots.select(role)?;
        Ok(Self {
            selection,
            identity: identity.clone(),
            scope,
        })
    }

    pub fn command<P>(
        cx: &mut Cx<'_, P>,
        selected: Result<Self, CredentialUnavailable>,
        policy: ReadPolicy,
        missing: LocalRefusal,
    ) -> Result<AdmittedRead, CommandReadError> {
        let credential = selected.as_ref().ok().map(Self::credential);
        let context = cx
            .admit_http_command(policy.policy(credential))
            .map_err(CommandReadError::Entry)?;
        match selected {
            Ok(selected) => Ok(AdmittedRead { selected, context }),
            Err(_) => {
                let terminal = missing
                    .diagnostic
                    .map_or(Ok(()), |diagnostic| {
                        cx.command_error_context(&context, diagnostic)
                    })
                    .and_then(|()| cx.fail_context(&context, missing.error.clone()));
                Err(CommandReadError::LocalRefusal {
                    error: missing.error,
                    terminal,
                })
            }
        }
    }

    pub fn background<P>(
        cx: &mut Cx<'_, P>,
        selected: Result<Self, CredentialUnavailable>,
        policy: ReadPolicy,
    ) -> Result<AdmittedRead, BackgroundReadError> {
        Self::scoped(cx, selected, Scope::Background, policy)
    }

    /// Issue under an existing scope, such as a delivered command's own
    /// `Scope::Command(id)`, rather than a new entry root. A refusal issues no
    /// context and settles nothing; the caller keeps its pending work.
    pub fn scoped<P>(
        cx: &mut Cx<'_, P>,
        selected: Result<Self, CredentialUnavailable>,
        scope: Scope,
        policy: ReadPolicy,
    ) -> Result<AdmittedRead, BackgroundReadError> {
        let selected = selected.map_err(BackgroundReadError::Selection)?;
        let context = cx
            .http_context(scope, policy.policy(Some(selected.credential())))
            .map_err(BackgroundReadError::Admission)?;
        Ok(AdmittedRead { selected, context })
    }

    fn credential(&self) -> Credential {
        self.selection.credential(self.identity.credential.clone())
    }
}

impl AdmittedRead {
    pub fn context(&self) -> &Context {
        &self.context
    }

    pub fn queue<P>(
        self,
        cx: &mut Cx<'_, P>,
        request: ReadRequest,
        recipe: impl FnOnce(&Identity) -> Box<dyn SignStep>,
        pending: impl FnOnce(Self) -> P,
    ) -> Result<RequestId, (P, Error)> {
        let context = self.context.clone();
        let step = recipe(&self.selected.identity);
        let scope = self.selected.scope;
        let slot = self.selected.selection.slot();
        let pending = pending(self);
        let id = cx.request_signed_managed(
            context,
            request.method,
            request.url,
            request.body,
            step,
            pending,
        )?;
        if let Some(at) = request.stream {
            cx.stream_response(id, at);
        }
        if let Some(scope) = scope {
            cx.tag_account_read(id, slot, scope);
        }
        Ok(id)
    }

    pub fn retry<P, T>(
        mut self,
        cx: &Cx<'_, P>,
        evidence: RetryEvidence,
        pending: T,
    ) -> Result<(Self, T), Box<RetryRefusal<T>>> {
        match cx.http_retry_context(&self.context, evidence) {
            Ok(context) => {
                self.context = context;
                Ok((self, pending))
            }
            Err(error) => Err(Box::new(RetryRefusal {
                read: self,
                pending,
                error,
            })),
        }
    }
}
