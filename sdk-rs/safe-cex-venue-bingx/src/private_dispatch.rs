//! Private message routing over landed account/order/listen-key operations.
use crate::{
    listen_key::BingxListenKey, private_events::BingxPrivateEvents, public_stream::PublicFrame,
};
use safe_cex_exchange::runtime::{ExchangeError, ExchangeFuture};
use safe_cex_num::js_truthy;
use serde_json::Value;
use std::rc::Rc;
pub trait PrivateMessageHost {
    fn is_disposed(&self) -> bool;
    fn track_last_received(&self) -> Result<(), ExchangeError>;
    fn read_blob(&self, blob: u64) -> ExchangeFuture<'_, String>;
    fn diagnostic_receive(&self, raw: &str) -> Result<(), ExchangeError>;
    fn send_with_log(&self, raw: &str) -> Result<(), ExchangeError>;
    fn handle_pong(&self) -> Result<(), ExchangeError>;
    fn record_stats(&self, topic: &str) -> Result<(), ExchangeError>;
}
pub trait PrivateDispatchOperations {
    fn rebuild(&self) -> ExchangeFuture<'_, ()>;
    fn account(&self, events: Vec<Value>) -> ExchangeFuture<'_, ()>;
    fn order(&self, events: &[Value]) -> Result<(), ExchangeError>;
}
pub struct NativePrivateDispatchOperations {
    pub events: Rc<BingxPrivateEvents>,
    pub keys: Rc<BingxListenKey>,
}
impl PrivateDispatchOperations for NativePrivateDispatchOperations {
    fn rebuild(&self) -> ExchangeFuture<'_, ()> {
        Box::pin(self.keys.rebuild_with_fresh_listen_key())
    }
    fn account(&self, events: Vec<Value>) -> ExchangeFuture<'_, ()> {
        Box::pin(async move { self.events.handle_account_events(&events).await })
    }
    fn order(&self, events: &[Value]) -> Result<(), ExchangeError> {
        self.events.handle_order_events(events)
    }
}
pub struct BingxPrivateDispatcher {
    pub host: Rc<dyn PrivateMessageHost>,
    pub operations: Rc<dyn PrivateDispatchOperations>,
}
impl BingxPrivateDispatcher {
    pub async fn on_message(&self, frame: PublicFrame) -> Result<(), ExchangeError> {
        if self.host.is_disposed() {
            return Ok(());
        }
        self.host.track_last_received()?;
        let raw = match frame {
            PublicFrame::Text(s) => s,
            PublicFrame::Blob(b) => {
                let s = self.host.read_blob(b).await?;
                if self.host.is_disposed() {
                    return Ok(());
                }
                s
            }
        };
        self.host.diagnostic_receive(&raw)?;
        let json = safe_cex_transport::how::big_int_as_string_json_parse(&raw)
            .into_value()
            .map_err(|_| {
                ExchangeError::new("BingX private frame UTF-8 boundary refuses a lone surrogate")
            })?;
        if json.is_null()
            && serde_json::from_str::<Box<serde_json::value::RawValue>>(&raw).is_ok()
            && serde_json::from_str::<Value>(&raw).is_err()
        {
            return Err(ExchangeError::new(
                "BingX private frame UTF-8 boundary refuses a lone surrogate",
            ));
        }
        if raw == "Ping" || js_truthy(json.get("ping")) {
            return self.host.send_with_log("Pong");
        }
        if raw == "Pong" || js_truthy(json.get("pong")) {
            return self.host.handle_pong();
        }
        let event = json.get("e").and_then(Value::as_str);
        if event == Some("listenKeyExpired") {
            self.host.record_stats("listenKey")?;
            self.operations.rebuild().await?;
            return Ok(());
        }
        if event == Some("ACCOUNT_UPDATE") {
            self.operations.account(vec![json.clone()]).await?;
            self.host.record_stats("position")?;
        }
        if matches!(event, Some("ORDER_TRADE_UPDATE" | "TRADE_UPDATE")) {
            self.operations.order(&[json])?;
            self.host.record_stats("order")?;
        }
        Ok(())
    }
}
