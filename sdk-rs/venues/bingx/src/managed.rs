//! Native read intent; route, attempt, deadline and release policy stay shared.
use crate::sign::{Credentials, Params, dispatch::Http};
use crate::{Pending, Signed};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::request_context::{
    Context, Credential, Operation, Policy, RetryEvidence, RetryPolicy, Scope, Session,
};
use safe_cex_engine::{CredentialSlot, Cx, ErrorKind, Method, Outcome, RequestId, VenueError};

type Queued = Result<RequestId, (Box<Pending>, VenueError)>;
fn refused() -> VenueError {
    VenueError::new("BingX managed read lifecycle is unavailable")
        .kind(ErrorKind::Internal)
        .outcome(Outcome::NotSent)
}
pub(crate) struct Read {
    pub method: Method,
    pub path: &'static str,
    pub params: Params,
    pub at: Option<ArrayAt>,
}
pub(crate) fn read(
    cx: &mut Cx<'_, Pending>,
    credentials: &Credentials,
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
    let identity = credentials.identity(method);
    let slot = if method == Method::Get && credentials.read.is_some() {
        CredentialSlot::Secondary
    } else {
        CredentialSlot::Primary
    };
    let context = match cx.http_context(
        scope,
        Policy {
            operation: Operation::Read,
            credential: Some(Credential {
                slot,
                reference: identity.credential.clone(),
            }),
            timeout_ms: 20_000,
            pacing: None,
            retry: if matches!(&pending, Pending::Identity(_)) {
                RetryPolicy::Never
            } else {
                RetryPolicy::ReadFailure
            },
        },
    ) {
        Ok(context) => context,
        Err(_) => return Err((Box::new(pending), refused())),
    };
    queue(
        cx,
        credentials,
        Signed {
            inner: pending,
            method,
            path,
            params,
            at,
            resent: false,
            context: Some(context),
        },
    )
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
    queue(
        cx,
        credentials,
        Signed {
            inner: pending,
            method,
            path,
            params,
            at: None,
            resent: true,
            context: Some(context),
        },
    )
}
pub(crate) fn session_credential(credentials: &Credentials, method: Method) -> Credential {
    Credential {
        slot: CredentialSlot::Primary,
        reference: credentials.identity(method).credential.clone(),
    }
}
pub(crate) fn retry(
    cx: &mut Cx<'_, Pending>,
    credentials: &Credentials,
    mut signed: Signed,
) -> Queued {
    let context = match signed.context.as_ref().and_then(|context| {
        cx.http_retry_context(context, RetryEvidence::ReadFailure)
            .ok()
    }) {
        Some(context) => context,
        None => return Err((Box::new(Pending::Signed(Box::new(signed))), refused())),
    };
    signed.context = Some(context);
    signed.resent = true;
    queue(cx, credentials, signed)
}
fn queue(cx: &mut Cx<'_, Pending>, credentials: &Credentials, signed: Signed) -> Queued {
    let context = signed
        .context
        .clone()
        .expect("managed read has engine authority");
    let recipe = Http::new(
        credentials.identity(signed.method).clone(),
        signed.method,
        signed.path,
        signed.params.clone(),
    );
    let (url, _, body) = recipe.unsigned();
    let at = signed.at;
    match cx.request_signed_managed(
        context,
        None,
        signed.method,
        url,
        body,
        Box::new(recipe),
        Pending::Signed(Box::new(signed)),
    ) {
        Ok(id) => {
            if let Some(at) = at {
                cx.stream_response(id, at);
            }
            Ok(id)
        }
        Err((pending, _)) => Err((Box::new(pending), refused())),
    }
}
pub(crate) fn finish(cx: &mut Cx<'_, Pending>, context: Option<Context>) {
    if let Some(context) = context {
        // A native command completion may already have retired this authority.
        let _ = cx.finish_http_context(&context);
    }
}
