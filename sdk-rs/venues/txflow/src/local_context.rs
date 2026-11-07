//! Immediate native results consume only their genuine command entry.
use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy};
use safe_cex_engine::{CommandWait, Cx, VenueError};

pub(crate) fn complete<P>(
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
    let Ok(context) = cx.admit_http_command(Policy {
        operation,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }) else {
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
