//! Stored listeners retain weak ownership; an outstanding callback can outlive its socket.
use crate::{
    public_adapter::{BackpackPublicWebSocket, PublicFrame, Throttle},
    public_control::JsString,
    public_listeners as l,
};
use safe_cex_exchange::runtime::ExchangeError;
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};
pub struct ListenerFrame {
    pub owner: Weak<BackpackPublicWebSocket>,
    pub state: Weak<l::ListenerState<ExchangeError, Rc<Throttle>>>,
}
impl ListenerFrame {
    fn live(&self) -> Result<PublicFrame, ExchangeError> {
        Ok(PublicFrame {
            owner: self
                .owner
                .upgrade()
                .ok_or_else(|| ExchangeError::new("Backpack listener socket disposed"))?,
        })
    }
}
impl l::Host for ListenerFrame {
    type Error = ExchangeError;
    type Throttle = Rc<Throttle>;
    fn state(&self) -> Rc<l::ListenerState<ExchangeError, Rc<Throttle>>> {
        self.state.upgrade().unwrap_or_default()
    }
    fn market(&mut self, s: &[u16]) -> Result<Option<JsString>, ExchangeError> {
        self.live()?.market(s)
    }
    fn connected(&mut self) -> Result<bool, ExchangeError> {
        self.live()?.connected()
    }
    fn disposed(&mut self) -> Result<bool, ExchangeError> {
        Ok(self.owner.upgrade().is_none_or(|o| o.base.is_disposed()))
    }
    fn emit(&mut self, s: JsString) -> Result<(), ExchangeError> {
        self.live()?.emit(s)
    }
    fn error_text(&mut self, e: &ExchangeError) -> Result<JsString, ExchangeError> {
        Ok(e.js_error_string().encode_utf16().collect())
    }
    fn timeout(&mut self, c: l::Action<ExchangeError>, d: f64) -> Result<(), ExchangeError> {
        self.live()?.timeout(c, d)
    }
    fn subscribe_kline(&mut self, s: JsString, i: JsString) -> Result<(), ExchangeError> {
        self.live()?.subscribe_kline(s, i)
    }
    fn unsubscribe_kline(&mut self, s: JsString, i: JsString) -> Result<(), ExchangeError> {
        self.live()?.unsubscribe_kline(s, i)
    }
    fn subscribe_depth(&mut self, s: JsString) -> Result<(), ExchangeError> {
        self.live()?.subscribe_depth(s)
    }
    fn unsubscribe(&mut self, s: JsString) -> Result<(), ExchangeError> {
        self.live()?.unsubscribe(s)
    }
    fn cleanup_stats(&mut self, s: JsString) -> Result<(), ExchangeError> {
        self.live()?.cleanup_stats(s)
    }
    fn date(&mut self, s: JsString) -> Result<f64, ExchangeError> {
        self.live()?.date(s)
    }
    fn snapshot(
        &mut self,
        s: JsString,
    ) -> Result<l::Task<l::Snapshot, ExchangeError>, ExchangeError> {
        self.live()?.snapshot(s)
    }
    fn spawn(&mut self, f: l::Task<(), ExchangeError>) -> Result<(), ExchangeError> {
        self.live()?.spawn(f)
    }
    fn apply(&mut self, b: l::Book, d: Rc<RefCell<l::Delta>>) -> Result<(), ExchangeError> {
        self.live()?.apply(b, d)
    }
    fn totals(&mut self, b: l::Book) -> Result<(), ExchangeError> {
        self.live()?.totals(b)
    }
    fn recover(&mut self, s: JsString, r: JsString) -> Result<(), ExchangeError> {
        self.live()?.recover(s, r)
    }
    fn terminate(
        &mut self,
        s: JsString,
        state: l::State<ExchangeError>,
        r: JsString,
    ) -> Result<(), ExchangeError> {
        self.live()?.terminate(s, state, r)
    }
    fn cancel_throttle(&mut self, t: Rc<Throttle>) -> Result<(), ExchangeError> {
        t.cancel();
        Ok(())
    }
}
