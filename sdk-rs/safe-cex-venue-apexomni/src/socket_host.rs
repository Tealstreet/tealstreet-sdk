//! Imported BaseWebSocket, clock, timer, transport and remote-signer capabilities.
use crate::{
    data,
    exchange::{failure, numeric},
};
use safe_cex_exchange::runtime::{ExchangeError, ExchangeFuture};
use safe_cex_num::{JsJsonValue as J, js_number_string};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use std::rc::Rc;

pub type SocketTask = Rc<dyn Fn() -> ExchangeFuture<'static, ()>>;
pub enum SocketListener {
    Open(SocketTask),
    Message(Rc<dyn Fn(&str) -> Result<(), ExchangeError>>),
    Close,
}
pub trait ApexSocketHost {
    fn initialize(
        &self,
        private: bool,
        treat_any_message_as_pong: bool,
    ) -> Result<(), ExchangeError>;
    fn is_disposed(&self) -> bool;
    fn is_connected(&self) -> bool;
    fn now_ms(&self) -> f64;
    fn date_now_ms(&self) -> f64;
    fn monotonic_now_ms(&self) -> f64;
    fn nanoid(&self) -> String;
    fn enable_reconnecting(&self) -> bool;
    fn private_route(
        &self,
        name: &ExchangeName,
        options: &ExchangeOptions,
    ) -> Result<Option<String>, ExchangeError>;
    fn create_socket(&self, url: &str) -> Result<(), ExchangeError>;
    fn add_listener(&self, listener: SocketListener) -> Result<(), ExchangeError>;
    fn send(&self, frame: &str, with_log: bool) -> Result<(), ExchangeError>;
    fn set_ping_at(&self, at: f64) -> Result<(), ExchangeError>;
    fn schedule_ping(&self, delay_ms: u64) -> Result<(), ExchangeError>;
    fn track_last_received(&self) -> Result<(), ExchangeError>;
    fn diagnostic_receive(&self, frame: &str) -> Result<(), ExchangeError>;
    fn record_stats(&self, topic: &str) -> Result<(), ExchangeError>;
    fn cleanup_stats(&self, topic: &str) -> Result<(), ExchangeError>;
    fn handle_pong(&self, next_ping_ms: Option<u64>) -> Result<(), ExchangeError>;
    fn emit_reconnected(&self) -> Result<(), ExchangeError>;
    fn reconnect(&self) -> Result<(), ExchangeError>;
    fn report_error(&self, error: ExchangeError) -> Result<(), ExchangeError>;
    fn warn(&self, message: &str, argument: &J) -> Result<(), ExchangeError>;
    fn set_timeout(&self, delay_ms: u64, callback: SocketTask) -> Result<u64, ExchangeError>;
    fn clear_timeout(&self, timer: u64) -> Result<(), ExchangeError>;
    fn spawn(&self, future: ExchangeFuture<'static, ()>) -> Result<(), ExchangeError>;
    fn dispose(&self) -> Result<(), ExchangeError>;
    fn remote_signer_account(&self) -> Option<J>;
    fn remote_sign(&self, request: J) -> ExchangeFuture<'_, J>;
}
pub(crate) fn object(values: impl IntoIterator<Item = (&'static str, J)>) -> J {
    J::Object(values.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
pub(crate) fn send(
    host: &dyn ApexSocketHost,
    value: J,
    with_log: bool,
) -> Result<(), ExchangeError> {
    let frame = safe_cex_num::js_json_stringify(&value)
        .map_err(|e| failure(e.message.into()))?
        .ok_or_else(|| failure("socket frame did not stringify".into()))?;
    host.send(&frame, with_log)
}
pub(crate) fn subscription(
    host: &dyn ApexSocketHost,
    op: &str,
    topics: &[String],
    with_log: bool,
) -> Result<(), ExchangeError> {
    send(
        host,
        object([
            ("op", J::String(op.into())),
            (
                "args",
                J::Array(topics.iter().cloned().map(J::String).collect()),
            ),
        ]),
        with_log,
    )
}
pub(crate) fn endpoint(default: &str, now: f64, request_id: &str) -> String {
    let mut url = default.to_owned();
    if let Some(start) = url.find("timestamp=") {
        let start = start + 10;
        let count = url[start..].bytes().take_while(u8::is_ascii_digit).count();
        if count > 0 {
            url.replace_range(start..start + count, &js_number_string(now));
        }
    }
    let joiner = if url.contains('?') { '&' } else { '?' };
    format!("{url}{joiner}requestId={request_id}")
}
pub(crate) fn property<'a>(value: &'a J, key: &str) -> Result<&'a J, ExchangeError> {
    if matches!(value, J::Null | J::Undefined) {
        let mut error = failure(format!(
            "Cannot read properties of {} (reading '{key}')",
            if matches!(value, J::Null) {
                "null"
            } else {
                "undefined"
            }
        ));
        error.name = "TypeError".into();
        return Err(error);
    }
    Ok(data::get(value, key))
}
pub(crate) fn number(value: &J) -> Result<f64, ExchangeError> {
    numeric(value)
}
