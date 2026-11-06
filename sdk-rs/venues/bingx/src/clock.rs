//! After a 109400 timestamp refusal (`sign::timestamp`), one unsigned
//! server-time read recalibrates this engine's protocol clock for every later
//! stamp. The refused request itself was already re-signed on a fresh stamp and
//! resent once at its own site: an engine retry cannot outlive its callback.
use crate::{Bingx, Pending, paths};
use safe_cex_engine::log::code;
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy, Scope};
use safe_cex_engine::{Cx, Level, Method, Response};
use serde::Deserialize;

const SERVER_TIME_TIMEOUT_MS: u32 = 5_000;

/// The one in-flight server-time read.
pub struct Probe {
    context: Context,
    sent_at: f64,
}

#[derive(Deserialize)]
struct ServerTime {
    data: ServerTimeData,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServerTimeData {
    server_time: f64,
}

impl Bingx {
    /// Every timestamp refusal asks; one read runs at a time.
    pub(crate) fn resync_clock(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.clock_reading {
            return;
        }
        match read_server_time(cx) {
            Ok(()) => self.clock_reading = true,
            Err(reason) => clock_settled(cx, Err(reason)),
        }
    }

    pub(crate) fn clock_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        probe: Probe,
        response: Response<'_>,
    ) {
        let delivered = cx.validate_http_context(&probe.context).is_ok();
        let _ = cx.finish_http_context(&probe.context);
        self.clock_reading = false;
        let server = serde_json::from_str::<ServerTime>(response.body)
            .ok()
            .map(|time| time.data.server_time)
            .filter(|_| delivered && (200..300).contains(&response.status))
            .filter(|time| time.is_finite() && *time > 0.);
        let Some(server) = server else {
            let reason = format!("HTTP {}: {}", response.status, response.body);
            return clock_settled(cx, Err(reason));
        };
        // The server read its clock about halfway through the round trip.
        let half_trip = (cx.now - probe.sent_at).max(0.) / 2.;
        let result = if cx.observe_protocol_time(server + half_trip) {
            Ok(())
        } else {
            Err(format!("unusable server time {server}"))
        };
        clock_settled(cx, result);
    }

    pub(crate) fn clock_failed(&mut self, cx: &mut Cx<'_, Pending>, probe: Probe, error: &str) {
        let _ = cx.finish_http_context(&probe.context);
        self.clock_reading = false;
        clock_settled(cx, Err(error.to_owned()));
    }
}

fn clock_settled(cx: &mut Cx<'_, Pending>, measured: Result<(), String>) {
    let offset = cx.protocol_now() - cx.now;
    let message = match measured {
        Ok(()) => format!(
            "BingX server clock read after a timestamp refusal (109400): offset now {offset:+.0} ms"
        ),
        Err(reason) => format!(
            "BingX server clock read after a timestamp refusal (109400) failed ({reason}); keeping offset {offset:+.0} ms"
        ),
    };
    cx.notice(Level::Warn, code::REQUEST_FAILED, message);
}

fn read_server_time(cx: &mut Cx<'_, Pending>) -> Result<(), String> {
    let policy = Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: SERVER_TIME_TIMEOUT_MS,
        pacing: None,
        retry: RetryPolicy::Never,
    };
    let context = cx
        .http_context(Scope::Background, policy)
        .map_err(|_| "read admission refused".to_owned())?;
    let probe = Probe {
        context: context.clone(),
        sent_at: cx.now,
    };
    cx.request_managed_with_headers(
        context.clone(),
        None,
        Method::Get,
        format!("{}{}", paths::REST, paths::SERVER_TIME),
        crate::sign::unsigned_headers(Method::Get),
        None,
        Pending::Clock(probe),
    )
    .map(|_| ())
    .map_err(|_| {
        let _ = cx.finish_http_context(&context);
        "read could not be queued".to_owned()
    })
}
