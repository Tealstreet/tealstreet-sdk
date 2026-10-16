use crate::{TransportError, TransportFuture};
use std::future::Future;

pub trait TealstreetTimeHost {
    fn origin(&self) -> Option<String>;
    fn configured_time_origin(&self) -> Option<String>;
    fn now_ms(&self) -> f64;
    /// Start the request immediately when called. Decode unixMs only when the
    /// source value is a number (including NaN); nonnumbers select date parsing.
    fn get_time(&self, endpoint: String, timeout_ms: u64) -> TransportFuture<'_, TimeResponse>;
    /// Bind dayjs.utc(time). Missing and null remain distinct; this parses only
    /// after receivedAt is captured, so parse time is excluded from the RTT.
    fn parse_utc_time(&self, time: Option<&serde_json::Value>) -> Result<f64, TransportError>;
}
pub struct TimeResponse {
    pub unix_ms: Option<f64>,
    /// Server response data only; never a query-value projection.
    pub time: Option<serde_json::Value>,
}
pub fn get_base_url(host: &dyn TealstreetTimeHost) -> String {
    host.origin()
        .filter(|origin| !origin.is_empty())
        .unwrap_or_default()
}
pub fn get_tealstreet_time_endpoint(host: &dyn TealstreetTimeHost) -> String {
    let configured = host
        .configured_time_origin()
        .map(|value| {
            safe_cex_num::js_trim(&value)
                .trim_end_matches('/')
                .to_owned()
        })
        .filter(|value| !value.is_empty());
    match configured {
        None => format!("{}/api/time", get_base_url(host)),
        Some(value) if value.ends_with("/time") => value,
        Some(value) => format!("{value}/v1/time"),
    }
}
pub fn tealstreet_time_source(
    host: &dyn TealstreetTimeHost,
) -> impl Future<Output = Option<f64>> + '_ {
    let requested_at = host.now_ms();
    let pending = host.get_time(get_tealstreet_time_endpoint(host), 5000);
    async move {
        let result: Result<Option<f64>, TransportError> = async {
            let response = pending.await?;
            let received_at = host.now_ms();
            let server_ms = match response.unix_ms {
                Some(value) => value,
                None => host.parse_utc_time(response.time.as_ref())?,
            };
            Ok(server_ms
                .is_finite()
                .then(|| server_ms + ((received_at - requested_at) / 2.0).floor()))
        }
        .await;
        result.unwrap_or(None)
    }
}
