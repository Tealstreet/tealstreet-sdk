//! Single-owner command reads under engine-issued Read authority. Route,
//! release, deadline and outcome policy stay with the shared lifecycle.
use crate::dispatch_signing::Http;
use crate::{Pending, Task};
use safe_cex_engine::request_context::{
    Context, Credential, Operation, Policy, RetryPolicy, Scope,
};
use safe_cex_engine::signing::Identity;
use safe_cex_engine::{CommandId, CredentialSlot, Cx, ErrorKind, Method, Outcome, VenueError};

fn refused() -> VenueError {
    VenueError::new("WEEX managed read lifecycle is unavailable")
        .kind(ErrorKind::Internal)
        .outcome(Outcome::NotSent)
}

/// Issue one signed GET whose task owns `context` until it completes. Native
/// WEEX reads never re-sign, so the read keeps its single physical attempt.
/// Without engine authority nothing is issued and the caller fails the command.
pub(crate) fn read(
    cx: &mut Cx<'_, Pending>,
    (identity, slot): (&Identity, CredentialSlot),
    command: CommandId,
    path_query: String,
    seed_generation: u32,
    task: impl FnOnce(Context) -> Task,
) -> Result<(), VenueError> {
    let method = Method::Get;
    let Ok(context) = cx.http_context(
        Scope::Command(command),
        Policy {
            operation: Operation::Read,
            credential: Some(Credential {
                slot,
                reference: identity.credential.clone(),
            }),
            timeout_ms: 20_000,
            pacing: None,
            retry: RetryPolicy::Never,
        },
    ) else {
        return Err(refused());
    };
    let recipe = Http::new(identity.clone(), method, path_query, None);
    let (url, _, body) = recipe.unsigned();
    let pending = Pending {
        task: task(context.clone()),
        invalid: false,
        skipped: Vec::new(),
        seed_generation,
    };
    if cx
        .request_signed_managed(
            context.clone(),
            None,
            method,
            url,
            body,
            Box::new(recipe),
            pending,
        )
        .is_err()
    {
        reject(cx, &context, refused());
    }
    Ok(())
}

pub(crate) fn complete(cx: &mut Cx<'_, Pending>, context: &Context, data: String) {
    let _ = cx.done_context(context, data);
    let _ = cx.finish_http_context(context);
}

/// Native, provider and lifecycle failures all settle the owned command once.
pub(crate) fn reject(cx: &mut Cx<'_, Pending>, context: &Context, error: impl Into<VenueError>) {
    let _ = cx.fail_context(context, error);
    let _ = cx.finish_http_context(context);
}
