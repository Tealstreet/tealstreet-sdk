//! Socket method bodies over compatible property and invocation operations.
use crate::public_control::JsString;
use std::future::Future;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Network {
    Production,
    Testnet,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Open,
    Message,
    Close,
}
impl Event {
    pub fn name(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Message => "message",
            Self::Close => "close",
        }
    }
}

pub trait Connection {
    type Socket;
    type Error;
    fn is_disposed(&mut self) -> Result<bool, Self::Error>;
    fn is_testnet(&mut self) -> Result<bool, Self::Error>;
    fn websocket_url(&mut self, network: Network) -> Result<JsString, Self::Error>;
    fn create_websocket(&mut self, url: JsString) -> Result<Self::Socket, Self::Error>;
    fn assign_websocket(&mut self, socket: Self::Socket) -> Result<(), Self::Error>;
    /// Evaluates the live socket's addEventListener lookup, handler lookup and invocation in order.
    fn add_listener(&mut self, event: Event) -> Result<(), Self::Error>;
}
fn connect<H: Connection>(host: &mut H) -> Result<(), H::Error> {
    if !host.is_disposed()? {
        let network = if host.is_testnet()? {
            Network::Testnet
        } else {
            Network::Production
        };
        let url = host.websocket_url(network)?;
        let socket = host.create_websocket(url)?;
        host.assign_websocket(socket)?;
        host.add_listener(Event::Open)?;
        host.add_listener(Event::Message)?;
        host.add_listener(Event::Close)?;
    }
    Ok(())
}
// ts: BackpackPublicWebSocket.connectAndSubscribe
pub fn connect_public<H: Connection>(host: &mut H) -> Result<(), H::Error> {
    connect(host)
}
// ts: BackpackPrivateWebSocket.connectAndSubscribe
pub fn connect_private<H: Connection>(
    host: &mut H,
) -> impl Future<Output = Result<(), H::Error>> + '_ {
    let result = connect(host);
    async move { result }
}

pub trait PrivateOpen {
    type Error;
    type Authentication: Future<Output = Result<(), Self::Error>>;
    fn is_disposed(&mut self) -> Result<bool, Self::Error>;
    fn track_last_received(&mut self) -> Result<(), Self::Error>;
    fn authenticate(&mut self) -> Result<Self::Authentication, Self::Error>;
    /// Starts the resubscribe call without waiting for its returned promise/future.
    fn resubscribe(&mut self) -> Result<(), Self::Error>;
    fn ping(&mut self) -> Result<(), Self::Error>;
}
// ts: BackpackPrivateWebSocket.onOpen
pub fn on_open_private<H: PrivateOpen>(mut host: H) -> impl Future<Output = Result<(), H::Error>> {
    let authentication: Result<Option<H::Authentication>, H::Error> = (|| {
        if host.is_disposed()? {
            Ok(None)
        } else {
            host.track_last_received()?;
            Ok(Some(host.authenticate()?))
        }
    })();
    async move {
        if let Some(authentication) = authentication? {
            authentication.await?;
            host.resubscribe()?;
            host.ping()?;
        }
        Ok(())
    }
}

pub trait PrivateDisposal {
    type Error;
    type Timer;
    type Socket;
    fn clear_subscriptions(&mut self) -> Result<(), Self::Error>;
    fn timeout(&mut self) -> Result<Self::Timer, Self::Error>;
    fn timeout_truthy(&self, timer: &Self::Timer) -> bool;
    fn clear_timeout(&mut self, timer: Self::Timer) -> Result<(), Self::Error>;
    fn unset_timeout(&mut self) -> Result<(), Self::Error>;
    fn socket(&mut self) -> Result<Option<Self::Socket>, Self::Error>;
    /// Performs close access/call on the current value, including absent-value property errors.
    fn close_socket(&mut self, socket: Option<Self::Socket>) -> Result<(), Self::Error>;
}
// ts: BackpackPrivateWebSocket.dispose
pub fn dispose_private<H: PrivateDisposal>(host: &mut H) -> Result<(), H::Error> {
    host.clear_subscriptions()?;
    let timer = host.timeout()?;
    if host.timeout_truthy(&timer) {
        let timer = host.timeout()?;
        host.clear_timeout(timer)?;
        host.unset_timeout()?;
    }
    if host.socket()?.is_some() {
        let socket = host.socket()?;
        host.close_socket(socket)?;
    }
    Ok(())
}

pub trait DepthThrottle {
    type Error;
    type Value;
    type Updater;
    type Target;
    fn throttle_target(&mut self) -> Result<Self::Target, Self::Error>;
    fn cached_throttle(&mut self, symbol: &[u16]) -> Result<Self::Value, Self::Error>;
    fn is_truthy(&self, value: &Self::Value) -> bool;
    fn depth_updater(&mut self) -> Result<Self::Updater, Self::Error>;
    fn create_throttle(
        &mut self,
        updater: Self::Updater,
        wait: f64,
        leading: bool,
        trailing: bool,
    ) -> Result<Self::Value, Self::Error>;
    fn set_throttle(
        &mut self,
        target: Self::Target,
        symbol: &[u16],
        value: Self::Value,
    ) -> Result<(), Self::Error>;
}
// ts: BackpackPublicWebSocket#getDepthTickerUpdateThrottle
pub fn get_depth_ticker_update_throttle<H: DepthThrottle>(
    symbol: &[u16],
    host: &mut H,
) -> Result<H::Value, H::Error> {
    let current = host.cached_throttle(symbol)?;
    if !host.is_truthy(&current) {
        let target = host.throttle_target()?;
        let updater = host.depth_updater()?;
        let next = host.create_throttle(updater, 100.0, true, true)?;
        host.set_throttle(target, symbol, next)?;
    }
    host.cached_throttle(symbol)
}
