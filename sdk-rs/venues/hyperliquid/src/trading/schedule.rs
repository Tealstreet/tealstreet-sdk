//! Signed dead-man control: no account reconciliation can prove arming.
use super::{Hyperliquid, Job, Pending, Step, not_sent, unproven};
use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy};
use safe_cex_engine::{CommandId, Cx, ErrorKind, Outcome, Response, VenueError};
use serde::Deserialize;
use serde_json::value::RawValue;
use std::collections::VecDeque;

pub(super) fn deadline(time: Option<f64>, now: f64) -> Result<Option<u64>, String> {
    time.map(|time| {
        if !time.is_finite() || time.fract() != 0.0 || !(0.0..=9_007_199_254_740_991.0).contains(&time) {
            return Err("Hyperliquid scheduleCancel time must be a safe integer UTC timestamp in milliseconds".into());
        }
        if !now.is_finite() || time < now + 5000.0 {
            return Err("Hyperliquid scheduleCancel time must be at least 5 seconds in the future".into());
        }
        Ok(time as u64)
    }).transpose()
}
impl Hyperliquid {
    pub(crate) fn schedule_cancel(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        time: Option<f64>,
    ) {
        if self.spot {
            let Ok(context) = cx.admit_http_command(Policy {
                operation: Operation::Mutation,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            }) else {
                return;
            };
            let _ = cx.fail_context(
                &context,
                not_sent("scheduleCancel is not supported by Hyperliquid spot")
                    .kind(ErrorKind::Unsupported),
            );
            return;
        }
        let time_ms = match deadline(time, cx.now) {
            Ok(time) => time,
            Err(error) => {
                let Ok(context) = cx.admit_http_command(Policy {
                    operation: Operation::Mutation,
                    credential: None,
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                }) else {
                    return;
                };
                let _ = cx.fail_context(&context, not_sent(error).kind(ErrorKind::InvalidInput));
                return;
            }
        };
        if self.options.public_data_only || self.options.secret.as_deref().is_none_or(str::is_empty)
        {
            cx.fail(
                command,
                not_sent("Hyperliquid signing key is missing").kind(ErrorKind::AuthInvalid),
            );
            return;
        }
        self.trading.queue.push_back(Job {
            command,
            steps: VecDeque::from([Step::ScheduleCancel { time_ms }]),
            ids: vec![],
            replacement: None,
            amend_source: None,
            amend_filled: false,
            builder_checked: false,
            effective: false,
        });
        self.pump_trades(cx);
    }
}
#[derive(Deserialize)]
struct Ack<'a> {
    #[serde(borrow)]
    status: &'a str,
    #[serde(borrow)]
    response: &'a RawValue,
}
#[derive(Deserialize)]
struct Control<'a> {
    #[serde(rename = "type", borrow)]
    kind: &'a str,
    #[serde(borrow)]
    data: Option<&'a RawValue>,
}
#[derive(Deserialize)]
struct Statuses<'a> {
    #[serde(borrow)]
    statuses: Vec<&'a RawValue>,
}
fn object<'a, T: Deserialize<'a>>(raw: &'a str) -> Option<T> {
    raw.trim_start()
        .starts_with('{')
        .then(|| serde_json::from_str(raw).ok())
        .flatten()
}
pub(super) fn reply(result: Result<Response<'_>, &str>) -> Result<(), VenueError> {
    let response = result.map_err(|error| unproven(error).kind(ErrorKind::Network))?;
    let ack = object::<Ack<'_>>(response.body);
    let rejection = ack
        .as_ref()
        .filter(|ack| ack.status == "err")
        .and_then(|ack| serde_json::from_str::<String>(ack.response.get()).ok());
    let kind = match response.status {
        401 => ErrorKind::AuthInvalid,
        403 => ErrorKind::AuthPermission,
        429 => ErrorKind::RateLimited,
        408 | 504 => ErrorKind::Timeout,
        200..300 => ErrorKind::Internal,
        _ => ErrorKind::VenueUnavailable,
    };
    if response.status < 500
        && response.status != 408
        && let Some(message) = rejection.as_ref()
    {
        let message: String = message.chars().take(512).collect();
        return Err(
            VenueError::new(format!("Hyperliquid scheduleCancel rejected: {message}"))
                .venue_message(message)
                .kind(if (200..300).contains(&response.status) {
                    ErrorKind::Rejected
                } else {
                    kind
                })
                .outcome(Outcome::Rejected),
        );
    }
    if (200..300).contains(&response.status)
        && let Some(ack) = ack
        && ack.status == "ok"
        && let Some(control) = object::<Control<'_>>(ack.response.get())
        && control.kind == "default"
        && control.data.is_none_or(|data| {
            object::<Statuses<'_>>(data.get()).is_some_and(|data| data.statuses.is_empty())
        })
    {
        return Ok(());
    }
    let mut error = unproven(format!(
        "Unproven Hyperliquid scheduleCancel response (HTTP {})",
        response.status
    ))
    .kind(kind);
    if let Some(message) = rejection {
        error = error.venue_message(message.chars().take(512).collect::<String>());
    }
    Err(error)
}
