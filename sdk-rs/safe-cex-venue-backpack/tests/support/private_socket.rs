//! Transport event fixture; all protocol and parent behavior stays production.
use safe_cex_exchange::{
    runtime::{ExchangeError, ExchangeFuture},
    websocket::{WsEvent, WsSocket},
};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
    task::Poll,
};
#[derive(Default)]
pub struct Socket {
    pub events: Rc<RefCell<VecDeque<WsEvent>>>,
    pub sent: RefCell<Vec<String>>,
    closed: Cell<bool>,
}
impl WsSocket for Socket {
    fn ready_state(&self) -> u8 {
        if self.closed.get() { 3 } else { 1 }
    }
    fn next_event(&self) -> ExchangeFuture<'static, WsEvent> {
        let events = self.events.clone();
        Box::pin(std::future::poll_fn(move |_| {
            events
                .borrow_mut()
                .pop_front()
                .map_or(Poll::Pending, |e| Poll::Ready(Ok(e)))
        }))
    }
    fn send(&self, message: &str) -> Result<(), ExchangeError> {
        self.sent.borrow_mut().push(message.into());
        Ok(())
    }
    fn close(&self) {
        self.closed.set(true);
    }
    fn terminate(&self) {
        self.close();
    }
    fn reconnect(&self) {}
    fn is_reconnecting(&self) -> bool {
        false
    }
}
