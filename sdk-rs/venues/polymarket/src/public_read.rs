//! Native public-read continuations delegate custody to the shared Context APIs.
use crate::Pending;
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy, Scope};
use safe_cex_engine::{CommandWait, Cx, VenueError};

pub struct Read {
    root: Option<Context>,
    pub(crate) context: Context,
}

pub(crate) fn policy(operation: Operation) -> Policy {
    Policy {
        operation,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}

pub(crate) fn local(
    cx: &mut Cx<'_, Pending>,
    operation: Operation,
    result: Result<String, String>,
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

impl Read {
    pub(crate) fn new(cx: &mut Cx<'_, Pending>, command: bool) -> Option<Self> {
        if command {
            match cx.command_wait() {
                Some(CommandWait::Remaining(_)) => {}
                Some(CommandWait::Expired) => {
                    let _ = cx.reject_expired_command(Operation::Read);
                    return None;
                }
                None => {
                    let _ = cx.reject_unstamped_command(Operation::Read);
                    return None;
                }
            }
            let root = cx.admit_caller_read_root(None, None).ok()?;
            match cx.http_leg_context(&root, policy(Operation::Read)) {
                Ok(context) => Some(Self {
                    root: Some(root),
                    context,
                }),
                Err(reason) => {
                    let _ = cx.fail_context(&root, reason.read_venue_error());
                    None
                }
            }
        } else {
            match cx.http_context(Scope::Background, policy(Operation::Read)) {
                Ok(context) => Some(Self {
                    root: None,
                    context,
                }),
                Err(reason) => {
                    cx.notice(
                        safe_cex_engine::Level::Error,
                        safe_cex_engine::log::code::REQUEST_FAILED,
                        reason.to_string(),
                    );
                    None
                }
            }
        }
    }

    pub(crate) fn validate(&self, cx: &Cx<'_, Pending>) -> bool {
        match &self.root {
            Some(root) => cx.validate_read_http_delivery(root, &self.context),
            None => cx.validate_background_read_http_delivery(&self.context),
        }
        .is_ok()
    }

    pub(crate) fn finish(self, cx: &mut Cx<'_, Pending>, result: Result<String, String>) {
        if let Some(root) = self.root {
            match result {
                Ok(data) => {
                    let _ = cx.done_context(&root, data);
                }
                Err(error) => {
                    let _ = cx.fail_context(&root, error);
                }
            }
        } else {
            let _ = cx.finish_http_context(&self.context);
        }
    }

    pub(crate) fn command(&self) -> bool {
        self.root.is_some()
    }

    pub(crate) fn failed(self, cx: &mut Cx<'_, Pending>, error: VenueError) {
        if let Some(root) = self.root {
            let _ = cx.fail_context(&root, error);
        } else {
            let _ = cx.finish_http_context(&self.context);
            cx.notice(
                safe_cex_engine::Level::Error,
                safe_cex_engine::log::code::REQUEST_FAILED,
                error.message,
            );
        }
    }
}
