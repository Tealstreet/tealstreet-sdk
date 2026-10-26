//! Native read intent; key choice, route, attempt, deadline, evidence and
//! release policy stay with the engine's SelectedRead.
use crate::sign::{Credentials, Params, dispatch::Http};
use crate::{Pending, Signed};
use safe_cex_engine::credential_selection::Slots;
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::request_context::{
    Context, Credential, Operation, Policy, RetryEvidence, RetryPolicy, Scope, Session,
};
use safe_cex_engine::selected_read::{
    AdmittedRead, ReadPolicy, ReadRequest, ReadRole, SelectedRead,
};
use safe_cex_engine::signing::Identity;
use safe_cex_engine::{CredentialSlot, Cx, ErrorKind, Method, Outcome, RequestId, VenueError};

type Queued = Result<RequestId, (Box<Pending>, VenueError)>;
fn refused() -> VenueError {
    VenueError::new("BingX managed read lifecycle is unavailable")
        .kind(ErrorKind::Internal)
        .outcome(Outcome::NotSent)
}

/// The engine authority a signed request holds until it completes.
pub(crate) enum Custody {
    Read(AdmittedRead),
    Session(Context),
    /// A keyless public read: there is no signature to resend.
    Public(Context),
}
impl Custody {
    pub(crate) fn context(&self) -> &Context {
        match self {
            Self::Read(read) => read.context(),
            Self::Session(context) | Self::Public(context) => context,
        }
    }
}

pub(crate) struct Read {
    pub method: Method,
    pub path: &'static str,
    pub params: Params,
    pub at: Option<ArrayAt>,
}
/// One account-data read on the key `role` selects: the configured read key
/// (the primary without one) unless the role is primary-only.
pub(crate) fn read(
    cx: &mut Cx<'_, Pending>,
    slots: &Slots<Identity>,
    read: Read,
    pending: Pending,
    scope: Scope,
    role: ReadRole,
) -> Queued {
    let Read {
        method,
        path,
        params,
        at,
    } = read;
    let selected = SelectedRead::select(slots, role);
    let policy = ReadPolicy {
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::ReadFailure,
    };
    let Ok(admitted) = SelectedRead::scoped(cx, selected, scope, policy) else {
        return Err((Box::new(pending), refused()));
    };
    queue(
        cx,
        admitted,
        Signed {
            inner: pending,
            method,
            path,
            params,
            at,
            resent: false,
            custody: None,
        },
    )
}
/// A keyless public read: unsigned managed HTTP under its own Read Context,
/// with the native public headers.
pub(crate) fn public(
    cx: &mut Cx<'_, Pending>,
    read: Read,
    pending: Pending,
    scope: Scope,
) -> Queued {
    let Read {
        method,
        path,
        params,
        at,
    } = read;
    let policy = Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    };
    let Ok(context) = cx.http_context(scope, policy) else {
        return Err((Box::new(pending), refused()));
    };
    let (url, _, body) = crate::sign::dispatch::unsigned(method, path, &params);
    let signed = Signed {
        inner: pending,
        method,
        path,
        params,
        at,
        resent: true,
        custody: Some(Custody::Public(context.clone())),
    };
    let id = cx
        .request_managed_with_headers(
            context,
            None,
            method,
            url,
            crate::sign::unsigned_headers(method),
            body,
            Pending::Signed(Box::new(signed)),
        )
        .map_err(|(pending, _)| (Box::new(pending), refused()))?;
    if let Some(at) = at {
        cx.stream_response(id, at);
    }
    Ok(id)
}
/// Listen-key upkeep as one session write. Shared dispatch signs at send time;
/// listen keys present only the API key, so there is no signature resend.
pub(crate) fn session_write(
    cx: &mut Cx<'_, Pending>,
    credentials: &Credentials,
    session: &Session,
    method: Method,
    path: &'static str,
    params: Params,
    pending: Pending,
) -> Queued {
    let context = match cx.session_context(
        session,
        Policy {
            operation: Operation::Mutation,
            credential: Some(session_credential(credentials, method)),
            timeout_ms: 20_000,
            pacing: None,
            retry: RetryPolicy::Never,
        },
    ) {
        Ok(context) => context,
        Err(_) => return Err((Box::new(pending), refused())),
    };
    let recipe = Http::new(
        credentials.identity(method).clone(),
        method,
        path,
        params.clone(),
    );
    let (url, _, body) = recipe.unsigned();
    let signed = Signed {
        inner: pending,
        method,
        path,
        params,
        at: None,
        resent: true,
        custody: Some(Custody::Session(context.clone())),
    };
    cx.request_signed_managed(
        context,
        None,
        method,
        url,
        body,
        Box::new(recipe),
        Pending::Signed(Box::new(signed)),
    )
    .map_err(|(pending, _)| (Box::new(pending), refused()))
}
pub(crate) fn session_credential(credentials: &Credentials, method: Method) -> Credential {
    Credential {
        slot: CredentialSlot::Primary,
        reference: credentials.identity(method).credential.clone(),
    }
}
/// Re-sign a refused read once under a retry of its own Read authority.
pub(crate) fn retry(cx: &mut Cx<'_, Pending>, mut signed: Signed) -> Queued {
    let read = match signed.custody.take() {
        Some(Custody::Read(read)) => read,
        // Other authority is never re-signed here; keep it so `finish` retires it.
        other => {
            signed.custody = other;
            return Err((Box::new(Pending::Signed(Box::new(signed))), refused()));
        }
    };
    match read.retry(cx, RetryEvidence::ReadFailure, ()) {
        Ok((read, ())) => {
            signed.resent = true;
            queue(cx, read, signed)
        }
        Err(refusal) => {
            signed.custody = Some(Custody::Read(refusal.read));
            Err((Box::new(Pending::Signed(Box::new(signed))), refused()))
        }
    }
}
fn queue(cx: &mut Cx<'_, Pending>, read: AdmittedRead, signed: Signed) -> Queued {
    let (url, _, body) =
        crate::sign::dispatch::unsigned(signed.method, signed.path, &signed.params);
    let request = ReadRequest {
        method: signed.method,
        url,
        body,
        stream: signed.at,
    };
    let (method, path, params) = (signed.method, signed.path, signed.params.clone());
    read.queue(
        cx,
        request,
        |identity| Box::new(Http::new(identity.clone(), method, path, params)),
        |read| {
            Pending::Signed(Box::new(Signed {
                custody: Some(Custody::Read(read)),
                ..signed
            }))
        },
    )
    .map_err(|(pending, _)| (Box::new(pending), refused()))
}
pub(crate) fn finish(cx: &mut Cx<'_, Pending>, context: Option<Context>) {
    if let Some(context) = context {
        // A native command completion may already have retired this authority.
        let _ = cx.finish_http_context(&context);
    }
}
