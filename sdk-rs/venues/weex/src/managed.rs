//! Single-owner command reads under engine-issued Read authority. Key choice,
//! route, release, deadline and outcome policy stay with SelectedRead.
use crate::dispatch_signing::{Http, url};
use crate::{Pending, Task};
use safe_cex_engine::credential_selection::CredentialUnavailable;
use safe_cex_engine::request_context::{Context, RetryPolicy, Scope};
use safe_cex_engine::selected_read::{ReadPolicy, ReadRequest, SelectedRead};
use safe_cex_engine::{CommandId, Cx, ErrorKind, Method, Outcome, VenueError};

fn refused() -> VenueError {
    VenueError::new("WEEX managed read lifecycle is unavailable")
        .kind(ErrorKind::Internal)
        .outcome(Outcome::NotSent)
}

/// Issue one signed GET under `command`'s scope whose task owns its context
/// until it completes. Native WEEX reads never re-sign, so the read keeps its
/// single physical attempt. Without authority the caller fails the command.
pub(crate) fn read(
    cx: &mut Cx<'_, Pending>,
    selected: Result<SelectedRead, CredentialUnavailable>,
    command: CommandId,
    path_query: String,
    seed_generation: u32,
    task: impl FnOnce(Context) -> Task,
) -> Result<(), VenueError> {
    let policy = ReadPolicy {
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    };
    let Ok(read) = SelectedRead::scoped(cx, selected, Scope::Command(command), policy) else {
        return Err(refused());
    };
    let context = read.context().clone();
    let request = ReadRequest {
        method: Method::Get,
        url: url(&path_query),
        body: None,
        stream: None,
    };
    let queued = read.queue(
        cx,
        request,
        |identity| Box::new(Http::new(identity.clone(), Method::Get, path_query, None)),
        |read| Pending {
            task: task(read.context().clone()),
            invalid: false,
            skipped: Vec::new(),
            seed_generation,
        },
    );
    if queued.is_err() {
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
