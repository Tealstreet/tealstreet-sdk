//! One unsigned command candle read keeps its genuine receipt and physical bound.
use crate::Pending;
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::request_context::{Context, Egress, Operation, Policy, RetryPolicy, Scope};
use safe_cex_engine::{CommandId, CommandWait, Cx, Method, VenueError};

const HEADERS: &[(&str, &str)] = &[
    ("Content-Type", "application/json"),
    ("x-api-version", crate::API_VERSION),
];

pub struct Read {
    root: Context,
    context: Context,
}

impl Read {
    pub(crate) fn new(cx: &mut Cx<'_, Pending>, command: CommandId) -> Option<Self> {
        let remaining = match cx.command_wait() {
            Some(CommandWait::Remaining(remaining)) => remaining,
            Some(CommandWait::Expired) => {
                let _ = cx.reject_expired_command(Operation::Read);
                return None;
            }
            None => {
                let _ = cx.reject_unstamped_command(Operation::Read);
                return None;
            }
        };
        let root = cx.admit_caller_read_root(None, None).ok()?;
        let context = cx.http_context_with_egress(
            Scope::Command(command),
            Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: remaining.min(20_000),
                pacing: None,
                retry: RetryPolicy::Never,
            },
            Egress::PublicCors,
        );
        match context {
            Ok(context) => Some(Self { root, context }),
            Err(reason) => {
                let _ = cx.fail_context(&root, reason.read_venue_error());
                None
            }
        }
    }

    pub(crate) fn queue(self, cx: &mut Cx<'_, Pending>, body: String) {
        let context = self.context.clone();
        let pending = Pending::Candles {
            read: self,
            rows: Vec::new(),
            valid: true,
        };
        match cx.request_managed_with_headers(
            context,
            None,
            Method::Post,
            format!("{}/info", crate::REST_URL),
            HEADERS,
            Some(body),
            pending,
        ) {
            Ok(request) => cx.stream_response(request, ArrayAt::Field("data")),
            Err((Pending::Candles { read, .. }, reason)) => {
                read.failed(cx, reason.read_venue_error())
            }
            Err(_) => unreachable!("candle queue retains its continuation"),
        }
    }

    pub(crate) fn validate(&self, cx: &Cx<'_, Pending>) -> bool {
        cx.validate_read_http_delivery(&self.root, &self.context)
            .is_ok()
    }

    pub(crate) fn finish(self, cx: &mut Cx<'_, Pending>, result: Result<String, String>) {
        match result {
            Ok(data) => {
                let _ = cx.done_context(&self.root, data);
            }
            Err(error) => self.failed(cx, VenueError::new(error)),
        }
    }

    pub(crate) fn failed(self, cx: &mut Cx<'_, Pending>, error: VenueError) {
        let _ = cx.fail_context(&self.root, error);
    }
}
