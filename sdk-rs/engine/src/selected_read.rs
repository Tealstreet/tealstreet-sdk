//! One selected opaque identity through Read admission, queueing and retry.
use crate::credential_selection::{
    CredentialRole, CredentialUnavailable, PrimaryReason, Selection, Slots,
};
use crate::json::ArrayAt;
use crate::nonce::{ClaimAuthority, NonceClaim, NonceDomain};
use crate::request_context::{
    CommandEntryError, Context, Credential, Error, Operation, Pacing, Policy, RetryEvidence,
    RetryPolicy, Scope,
};
use crate::signing::Identity;
use crate::{AccountReadScope, CredentialSlot, Cx, Method, RequestId, SignStep, VenueError};
use std::sync::Arc;

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

/// An original signed command Read bounded by the genuine caller receipt.
/// Only this wrapper derives physical pages; borrowed Context authority is unchanged.
///
/// ```compile_fail,E0599
/// use safe_cex_engine::selected_read::ReadRoot;
/// fn copy(root: ReadRoot) { let _ = root.clone(); }
/// ```
#[must_use]
pub struct ReadRoot {
    selected: Arc<SelectedRead>,
    context: Context,
}

/// One physical page, including a native re-send, under an original Read root.
///
/// ```compile_fail,E0599
/// use safe_cex_engine::{Cx, selected_read::{ReadLeg, ReadLegPolicy}};
/// fn grandchild(leg: ReadLeg, cx: &mut Cx<'_, ()>) {
///     let _ = leg.leg(cx, ReadLegPolicy { timeout_ms:20_000, pacing:None });
/// }
/// ```
/// ```compile_fail,E0599
/// use safe_cex_engine::{Cx, request_context::RetryEvidence, selected_read::ReadLeg};
/// fn resend(leg: ReadLeg, cx: &Cx<'_, ()>) {
///     let _ = leg.retry(cx, RetryEvidence::ReadFailure, ());
/// }
/// ```
/// ```compile_fail,E0599
/// use safe_cex_engine::{request_context::Context, selected_read::ReadLeg};
/// fn external(context: Context) { let _ = ReadLeg::from(context); }
/// ```
#[must_use]
pub struct ReadLeg {
    selected: Arc<SelectedRead>,
    context: Context,
}

pub struct ReadLegPolicy {
    pub timeout_ms: u32,
    pub pacing: Option<Pacing>,
}

#[must_use]
pub struct RetryRefusal<T> {
    pub read: AdmittedRead,
    pub pending: T,
    pub error: Error,
}

impl SelectedRead {
    fn nonce_recipe(
        &self,
        domain: &'static NonceDomain,
        recipe: impl FnOnce(&Identity, NonceClaim) -> Box<dyn SignStep>,
    ) -> Result<(Box<dyn SignStep>, Arc<ClaimAuthority>), Error> {
        let claim =
            NonceClaim::mint(domain, self.selection, &self.identity).map_err(|_| Error::Invalid)?;
        let authority = claim.authority();
        Ok((recipe(&self.identity, claim), authority))
    }

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
            Err(_) => Err(Self::refuse_missing(cx, &context, missing)),
        }
    }

    /// Capture the exact original receipt once; no timeout override or renewal.
    /// Expired/unstamped caller handling stays with the shared Fresh-entry APIs.
    pub fn command_root<P>(
        cx: &mut Cx<'_, P>,
        selected: Result<Self, CredentialUnavailable>,
        missing: LocalRefusal,
    ) -> Result<ReadRoot, CommandReadError> {
        let credential = selected.as_ref().ok().map(Self::credential);
        let context = cx
            .admit_caller_read_root(credential, None)
            .map_err(CommandReadError::Entry)?;
        match selected {
            Ok(selected) => Ok(ReadRoot {
                selected: Arc::new(selected),
                context,
            }),
            Err(_) => Err(Self::refuse_missing(cx, &context, missing)),
        }
    }

    fn refuse_missing<P>(
        cx: &mut Cx<'_, P>,
        context: &Context,
        missing: LocalRefusal,
    ) -> CommandReadError {
        let terminal = missing
            .diagnostic
            .map_or(Ok(()), |diagnostic| {
                cx.command_error_context(context, diagnostic)
            })
            .and_then(|()| cx.fail_context(context, missing.error.clone()));
        CommandReadError::LocalRefusal {
            error: missing.error,
            terminal,
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
        let step = recipe(&self.selected.identity);
        self.queue_recipe(cx, request, step, None, pending)
    }

    /// Mint from the retained selection once for this physical queue.
    /// Missing subject or a recipe declaring a different claim refuses before IDs.
    pub fn queue_with_nonce<P>(
        self,
        cx: &mut Cx<'_, P>,
        request: ReadRequest,
        domain: &'static NonceDomain,
        recipe: impl FnOnce(&Identity, NonceClaim) -> Box<dyn SignStep>,
        pending: impl FnOnce(Self) -> P,
    ) -> Result<RequestId, (P, Error)> {
        let (step, authority) = match self.selected.nonce_recipe(domain, recipe) {
            Ok(recipe) => recipe,
            Err(error) => return Err((pending(self), error)),
        };
        self.queue_recipe(cx, request, step, Some(authority), pending)
    }

    fn queue_recipe<P>(
        self,
        cx: &mut Cx<'_, P>,
        request: ReadRequest,
        step: Box<dyn SignStep>,
        authority: Option<Arc<ClaimAuthority>>,
        pending: impl FnOnce(Self) -> P,
    ) -> Result<RequestId, (P, Error)> {
        let context = self.context.clone();
        let scope = self.selected.scope;
        let slot = self.selected.selection.slot();
        queue_read(
            cx,
            context,
            request,
            step,
            authority,
            scope.map(|scope| (slot, scope)),
            pending(self),
        )
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

impl ReadRoot {
    pub fn context(&self) -> &Context {
        &self.context
    }

    /// Mint at an eligible native dequeue, always from this original root.
    pub fn leg<P>(&self, cx: &mut Cx<'_, P>, policy: ReadLegPolicy) -> Result<ReadLeg, Error> {
        cx.validate_read_root_context(&self.context)?;
        let context = cx.http_leg_context(
            &self.context,
            ReadPolicy {
                timeout_ms: policy.timeout_ms,
                pacing: policy.pacing,
                retry: RetryPolicy::Never,
            }
            .policy(Some(self.selected.credential())),
        )?;
        Ok(ReadLeg {
            selected: self.selected.clone(),
            context,
        })
    }

    /// Custody only: native body validity and shared history admission remain separate.
    pub fn validate_delivery<P>(&self, cx: &Cx<'_, P>, leg: &ReadLeg) -> Result<(), Error> {
        if !Arc::ptr_eq(&self.selected, &leg.selected) {
            return Err(Error::Revoked);
        }
        cx.validate_read_http_delivery(&self.context, &leg.context)
    }
}

impl ReadLeg {
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
        let step = recipe(&self.selected.identity);
        self.queue_recipe(cx, request, step, None, pending)
    }

    /// Mint from the retained selection once for this physical queue.
    /// Missing subject or a recipe declaring a different claim refuses before IDs.
    pub fn queue_with_nonce<P>(
        self,
        cx: &mut Cx<'_, P>,
        request: ReadRequest,
        domain: &'static NonceDomain,
        recipe: impl FnOnce(&Identity, NonceClaim) -> Box<dyn SignStep>,
        pending: impl FnOnce(Self) -> P,
    ) -> Result<RequestId, (P, Error)> {
        let (step, authority) = match self.selected.nonce_recipe(domain, recipe) {
            Ok(recipe) => recipe,
            Err(error) => return Err((pending(self), error)),
        };
        self.queue_recipe(cx, request, step, Some(authority), pending)
    }

    fn queue_recipe<P>(
        self,
        cx: &mut Cx<'_, P>,
        request: ReadRequest,
        step: Box<dyn SignStep>,
        authority: Option<Arc<ClaimAuthority>>,
        pending: impl FnOnce(Self) -> P,
    ) -> Result<RequestId, (P, Error)> {
        let context = self.context.clone();
        let evidence = self
            .selected
            .scope
            .map(|scope| (self.selected.selection.slot(), scope));
        queue_read(
            cx,
            context,
            request,
            step,
            authority,
            evidence,
            pending(self),
        )
    }
}

fn queue_read<P>(
    cx: &mut Cx<'_, P>,
    context: Context,
    request: ReadRequest,
    step: Box<dyn SignStep>,
    authority: Option<Arc<ClaimAuthority>>,
    evidence: Option<(CredentialSlot, AccountReadScope)>,
    pending: P,
) -> Result<RequestId, (P, Error)> {
    let id = cx.request_selected_read_managed(
        context,
        request.method,
        request.url,
        request.body,
        step,
        authority,
        pending,
    )?;
    if let Some(at) = request.stream {
        cx.stream_response(id, at);
    }
    if let Some((slot, scope)) = evidence {
        cx.tag_account_read(id, slot, scope);
    }
    Ok(id)
}
