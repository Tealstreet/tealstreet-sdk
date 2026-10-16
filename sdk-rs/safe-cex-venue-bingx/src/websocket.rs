//! Complete public/private carriers bind every venue body to host socket callbacks.
use crate::{
    data_reads::{BingxDataReads, ReadTransport},
    listen_key::{BingxListenKey, ListenKeyHost},
    private_dispatch::{
        BingxPrivateDispatcher, NativePrivateDispatchOperations, PrivateMessageHost,
    },
    private_events::{BingxPrivateEvents, PrivateEventHost},
    public_stream::{BingxPublicStream, PublicFrame, PublicSocketHost},
    stream_subscriptions::{BingxSubscriptions, SubscriptionHost},
};
use safe_cex_exchange::runtime::{ExchangeError, ExchangeFuture};
use serde_json::Value;
use std::rc::Rc;

pub type CloseCallback = Rc<dyn Fn(Option<Value>) -> Result<(), ExchangeError>>;
pub struct PrivateCallbacks {
    pub open: Rc<dyn Fn() -> Result<(), ExchangeError>>,
    pub message: Rc<dyn Fn(PublicFrame) -> ExchangeFuture<'static, ()>>,
    pub close: CloseCallback,
}
pub trait WebsocketBindings {
    fn bind_private(&self, callbacks: PrivateCallbacks) -> Result<(), ExchangeError>;
    fn bind_public_close(&self, callback: CloseCallback) -> Result<(), ExchangeError>;
    fn public_base_close(&self, event: Option<Value>) -> Result<(), ExchangeError>;
    fn private_base_close(&self, event: Option<Value>) -> Result<(), ExchangeError>;
    fn public_base_dispose(&self) -> Result<(), ExchangeError>;
}
pub struct BingxPublicWebsocket {
    pub stream: Rc<BingxPublicStream>,
    pub subscriptions: Rc<BingxSubscriptions>,
    pub bindings: Rc<dyn WebsocketBindings>,
}
impl BingxPublicWebsocket {
    pub fn new(
        reads: Rc<BingxDataReads>,
        socket: Rc<dyn PublicSocketHost>,
        subscriptions: Rc<dyn SubscriptionHost>,
        bindings: Rc<dyn WebsocketBindings>,
    ) -> Result<Rc<Self>, ExchangeError> {
        let stream = BingxPublicStream::new(reads, socket);
        let subscriptions = BingxSubscriptions::new(stream.clone(), subscriptions);
        let base = Rc::downgrade(&bindings);
        bindings.bind_public_close(Rc::new(move |event| {
            base.upgrade()
                .ok_or_else(|| ExchangeError::new("BingX public close bindings released"))?
                .public_base_close(event)
        }))?;
        Ok(Rc::new(Self {
            stream,
            subscriptions,
            bindings,
        }))
    }
    pub fn dispose(&self) -> Result<(), ExchangeError> {
        self.bindings.public_base_dispose()
    }
}
pub struct BingxPrivateWebsocket {
    pub keys: Rc<BingxListenKey>,
    pub events: Rc<BingxPrivateEvents>,
    pub dispatcher: Rc<BingxPrivateDispatcher>,
    pub bindings: Rc<dyn WebsocketBindings>,
}
impl BingxPrivateWebsocket {
    pub fn new(
        reads: Rc<BingxDataReads>,
        transport: Rc<dyn ReadTransport>,
        key_host: Rc<dyn ListenKeyHost>,
        event_host: Rc<dyn PrivateEventHost>,
        message_host: Rc<dyn PrivateMessageHost>,
        bindings: Rc<dyn WebsocketBindings>,
    ) -> Result<Rc<Self>, ExchangeError> {
        let keys = BingxListenKey::new(transport, key_host);
        let events = BingxPrivateEvents::new(reads, event_host);
        let dispatcher = Rc::new(BingxPrivateDispatcher {
            host: message_host,
            operations: Rc::new(NativePrivateDispatchOperations {
                events: events.clone(),
                keys: keys.clone(),
            }),
        });
        let opened = Rc::downgrade(&keys);
        let messaged = Rc::downgrade(&dispatcher);
        let closed = Rc::downgrade(&keys);
        let base = Rc::downgrade(&bindings);
        bindings.bind_private(PrivateCallbacks {
            open: Rc::new(move || {
                opened
                    .upgrade()
                    .ok_or_else(|| ExchangeError::new("BingX private open owner released"))?
                    .on_open()
            }),
            message: Rc::new(move |frame| {
                let owner = messaged.upgrade();
                Box::pin(async move {
                    owner
                        .ok_or_else(|| ExchangeError::new("BingX private message owner released"))?
                        .on_message(frame)
                        .await
                })
            }),
            close: Rc::new(move |event| {
                closed
                    .upgrade()
                    .ok_or_else(|| ExchangeError::new("BingX private close owner released"))?
                    .clear_timer()?;
                base.upgrade()
                    .ok_or_else(|| ExchangeError::new("BingX private close bindings released"))?
                    .private_base_close(event)
            }),
        })?;
        Ok(Rc::new(Self {
            keys,
            events,
            dispatcher,
            bindings,
        }))
    }
}
