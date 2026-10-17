use super::streams::Topics;
use crate::public_data_recovery::{self as source, DataValue};
use safe_cex_exchange::{
    OwnedOperation, exchange::BaseExchange, runtime::ExchangeError, websocket::BaseWebSocket,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::HashMap,
    rc::{Rc, Weak},
};

pub(crate) struct Recovery {
    socket: Weak<BaseWebSocket>,
    parent: Weak<BaseExchange>,
    topics: Topics,
    timers: RefCell<HashMap<String, OwnedOperation>>,
    last: RefCell<HashMap<String, f64>>,
}
impl Recovery {
    pub(crate) fn new(
        socket: &Rc<BaseWebSocket>,
        parent: &Rc<BaseExchange>,
        topics: Topics,
    ) -> Rc<Self> {
        Rc::new(Self {
            socket: Rc::downgrade(socket),
            parent: Rc::downgrade(parent),
            topics,
            timers: RefCell::new(HashMap::new()),
            last: RefCell::new(HashMap::new()),
        })
    }
    fn handlers(&self, topic: &str) -> usize {
        self.socket
            .upgrade()
            .and_then(|socket| {
                socket
                    .message_handlers
                    .borrow()
                    .get(topic)
                    .map(|handlers| handlers.borrow().len())
            })
            .unwrap_or(0)
    }
    pub(crate) fn clear(&self, topic: &str) -> Result<(), ExchangeError> {
        source::clear_ohlcv_no_data_timeout(
            topic,
            |topic| Ok(DataValue::Bool(self.timers.borrow().contains_key(topic))),
            |_| {
                if let Some(timer) = self.timers.borrow().get(topic).cloned() {
                    timer.cancel();
                }
                Ok(())
            },
            |topic| {
                self.timers.borrow_mut().remove(topic);
                Ok(())
            },
        )
    }
    pub(crate) fn forget(&self, topic: &str) {
        self.last.borrow_mut().remove(topic);
    }
    pub(crate) fn arm(
        self: &Rc<Self>,
        topic: &str,
        secondary: Option<&str>,
    ) -> Result<(), ExchangeError> {
        let Some(socket) = self.socket.upgrade() else {
            return Ok(());
        };
        let pending = RefCell::new(None);
        let handlers = Rc::downgrade(self);
        let delete = Rc::downgrade(self);
        let disposed = Rc::downgrade(&socket);
        let recover = Rc::downgrade(self);
        let arm = Rc::downgrade(self);
        source::arm_ohlcv_no_data_timeout(
            topic.into(),
            secondary.map(str::to_owned),
            |topic| self.clear(topic),
            Rc::new(move |topic| Ok(handlers.upgrade().map_or(0, |value| value.handlers(topic)))),
            |callback, delay| {
                *pending.borrow_mut() =
                    Some(socket.start_timeout(delay as u64, move || callback())?);
                Ok(DataValue::Bool(true))
            },
            || {
                Ok(|topic: &str, _| {
                    self.timers.borrow_mut().insert(
                        topic.into(),
                        pending
                            .borrow_mut()
                            .take()
                            .expect("source timer factory runs before assignment"),
                    );
                    Ok(())
                })
            },
            Rc::new(move |topic| {
                if let Some(value) = delete.upgrade() {
                    value.timers.borrow_mut().remove(topic);
                }
                Ok(())
            }),
            Rc::new(move || Ok(disposed.upgrade().is_none_or(|socket| socket.is_disposed()))),
            Rc::new(move |topic, secondary, reason| {
                if let Some(value) = recover.upgrade() {
                    value.recover(topic, secondary, reason)?;
                }
                Ok(())
            }),
            Rc::new(move |topic, secondary| {
                if let Some(value) = arm.upgrade() {
                    value.arm(topic, secondary)?;
                }
                Ok(())
            }),
        )
    }
    pub(crate) fn arm_existing(self: &Rc<Self>) -> Result<(), ExchangeError> {
        let Some(socket) = self.socket.upgrade() else {
            return Ok(());
        };
        source::arm_existing_ohlcv_no_data_timeouts(
            || Ok(socket.message_handlers.borrow().keys().cloned().collect()),
            |topic| Ok(self.handlers(topic)),
            |topic, secondary| self.arm(topic, Some(secondary)),
        )
    }
    pub(crate) fn recover(
        &self,
        topic: &str,
        secondary: Option<&str>,
        reason: &str,
    ) -> Result<(), ExchangeError> {
        let (Some(socket), Some(parent)) = (self.socket.upgrade(), self.parent.upgrade()) else {
            return Ok(());
        };
        source::maybe_recover_ohlcv_stream_from_topics(
            topic,
            secondary,
            reason,
            |topic| Ok(self.handlers(topic)),
            || Ok(parent.runtime.now_ms().floor()),
            |topic| Ok(self.last.borrow().get(topic).copied()),
            |topic, now| {
                self.last.borrow_mut().insert(topic.into(), now);
                Ok(())
            },
            |topic, value| {
                self.topics
                    .borrow_mut()
                    .insert(topic.into(), Value::String(value.into()));
                Ok(())
            },
            || Ok(socket.is_connected()),
            |message, severity| parent.log(message, severity),
            || socket.reconnect(),
            |payload| {
                let payload = payload.borrow();
                socket.send_with_log(
                    &json!({"op":payload.op,"args":*payload.args.borrow()}).to_string(),
                )
            },
        )
    }
}
