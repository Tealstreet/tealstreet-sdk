//! Private message routing over landed account/order/listen-key operations.
use crate::{
    listen_key::BingxListenKey, private_events::BingxPrivateEvents, public_stream::PublicFrame,
};
use safe_cex_exchange::runtime::{ExchangeError, ExchangeFuture};
use safe_cex_types::retained_json::RetainedJsonInput as Input;
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
    fn account(&self, events: Vec<Input>) -> ExchangeFuture<'_, ()>;
    fn order(&self, events: &[Input]) -> Result<(), ExchangeError>;
}
pub struct NativePrivateDispatchOperations {
    pub events: Rc<BingxPrivateEvents>,
    pub keys: Rc<BingxListenKey>,
}
impl PrivateDispatchOperations for NativePrivateDispatchOperations {
    fn rebuild(&self) -> ExchangeFuture<'_, ()> {
        Box::pin(self.keys.rebuild_with_fresh_listen_key())
    }
    fn account(&self, events: Vec<Input>) -> ExchangeFuture<'_, ()> {
        Box::pin(async move { self.events.handle_account_events(&events).await })
    }
    fn order(&self, events: &[Input]) -> Result<(), ExchangeError> {
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
        let json = safe_cex_transport::how::big_int_as_string_json_parse(&raw);
        if raw == "Ping" || json.get("ping").is_some_and(Input::is_truthy) {
            return self.host.send_with_log("Pong");
        }
        if raw == "Pong" || json.get("pong").is_some_and(Input::is_truthy) {
            return self.host.handle_pong();
        }
        let event = json.get("e");
        if event.is_some_and(|v| v.text_equals("listenKeyExpired")) {
            self.host.record_stats("listenKey")?;
            self.operations.rebuild().await?;
            return Ok(());
        }
        if event.is_some_and(|v| v.text_equals("ACCOUNT_UPDATE")) {
            self.operations.account(vec![json.clone()]).await?;
            self.host.record_stats("position")?;
        }
        if event
            .is_some_and(|v| v.text_equals("ORDER_TRADE_UPDATE") || v.text_equals("TRADE_UPDATE"))
        {
            self.operations.order(&[json])?;
            self.host.record_stats("order")?;
        }
        Ok(())
    }
}
