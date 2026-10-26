//! Single-owner command reads under engine-issued Read authority. Key choice,
//! route, release, deadline and outcome policy stay with SelectedRead.
use crate::dispatch_signing::{Http, url};
use crate::{Pending, Task};
use safe_cex_engine::credential_selection::CredentialUnavailable;
use safe_cex_engine::request_context::{Context, Operation, RetryPolicy};
use safe_cex_engine::selected_read::{LocalRefusal, ReadPolicy, ReadRequest, SelectedRead};
use safe_cex_engine::{CommandWait, Cx, ErrorKind, Method, Outcome, VenueError};

fn refused() -> VenueError {
    VenueError::new("WEEX managed read lifecycle is unavailable")
        .kind(ErrorKind::Internal)
        .outcome(Outcome::NotSent)
}

/// Admit one signed GET from the genuine command entry. Its task retains the
/// context through completion; admission owns its refusal terminal.
pub(crate) fn read(
    cx: &mut Cx<'_, Pending>,
    selected: Result<SelectedRead, CredentialUnavailable>,
    path_query: String,
    seed_generation: u32,
    task: impl FnOnce(Context) -> Task,
) {
    match cx.command_wait() {
        Some(CommandWait::Remaining(_)) => {}
        Some(CommandWait::Expired) => {
            let _ = cx.reject_expired_command(Operation::Read);
            return;
        }
        None => {
            let _ = cx.reject_unstamped_command(Operation::Read);
            return;
        }
    }
    let policy = ReadPolicy {
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    };
    let Ok(read) = SelectedRead::command(
        cx,
        selected,
        policy,
        LocalRefusal {
            diagnostic: None,
            error: refused(),
        },
    ) else {
        return;
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
