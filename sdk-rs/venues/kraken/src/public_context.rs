//! Public codec policy and synchronous native results use shared custody.
use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy};
use safe_cex_engine::{CommandWait, Cx, VenueError};

pub(crate) fn policy(operation: Operation) -> Policy {
    Policy {
        operation,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}

pub(crate) fn local<P>(
    cx: &mut Cx<'_, P>,
    operation: Operation,
    result: Result<String, VenueError>,
) {
    match cx.command_wait() {
        Some(CommandWait::Remaining(_)) => {}
        Some(CommandWait::Expired) => {
            let _ = cx.reject_expired_command(operation);
            return;
        }
        None => {
            let _ = cx.reject_unstamped_command(operation);
            return;
        }
    }
    let Ok(context) = cx.admit_http_command(policy(operation)) else {
        return;
    };
    match result {
        Ok(data) => {
            let _ = cx.done_context(&context, data);
        }
        Err(error) => {
            let _ = cx.fail_context(&context, error);
        }
    }
}
