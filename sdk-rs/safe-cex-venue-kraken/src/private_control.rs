//! Private authentication and refresh bodies with bound signing, timers and Store.
use crate::message_dispatch::MessageHost;
use safe_cex_exchange::runtime::{ExchangeError, LogSeverity};
use std::{future::Future, rc::Rc};

pub struct SignRequest<V: ?Sized> {
    pub id: Rc<V>,
    pub kind: &'static str,
    pub exchange: &'static str,
    pub account_name: Rc<V>,
    pub to_sign: Rc<V>,
    pub output_format: &'static str,
    pub challenge: Rc<V>,
}

pub struct Subscription<V: ?Sized> {
    pub event: &'static str,
    pub key: Rc<V>,
    pub original: Rc<V>,
    pub signed: Rc<V>,
    pub feed: &'static str,
}

pub trait AuthHost: MessageHost {
    type Signer: ?Sized;
    fn read(&self, slot: &'static str) -> Result<Rc<Self::Value>, ExchangeError>;
    fn write(
        &self,
        slot: &'static str,
        value: Option<Rc<Self::Value>>,
    ) -> Result<(), ExchangeError>;
    fn signer(&self, value: Rc<Self::Value>) -> Result<Rc<Self::Signer>, ExchangeError>;
    fn nanoid(&self) -> Result<Rc<Self::Value>, ExchangeError>;
    fn sign(
        &self,
        signer: Rc<Self::Signer>,
        request: SignRequest<Self::Value>,
    ) -> Result<impl Future<Output = Result<Rc<Self::Value>, ExchangeError>>, ExchangeError>;
    fn hash_challenge(
        &self,
        challenge: impl FnOnce() -> Result<Rc<Self::Value>, ExchangeError>,
    ) -> Result<Rc<Self::Value>, ExchangeError>;
    fn decode_secret(&self, secret: Rc<Self::Value>) -> Result<Rc<Self::Value>, ExchangeError>;
    fn hmac_signature(
        &self,
        secret: Rc<Self::Value>,
        hash: Rc<Self::Value>,
    ) -> Result<Rc<Self::Value>, ExchangeError>;
    fn emit_error(&self, message: Vec<u16>) -> Result<(), ExchangeError>;
    fn error_text(&self, error: &ExchangeError) -> Result<Vec<u16>, ExchangeError>;
    fn subscribe(&self) -> Result<(), ExchangeError>;
    fn send_subscription(
        &self,
        subscription: Subscription<Self::Value>,
    ) -> Result<(), ExchangeError>;
}

pub async fn perform_auth<H: AuthHost>(host: &H) -> Result<(), ExchangeError> {
    if !host.truthy(&*host.read("pending")?) {
        return Ok(());
    }
    let body = async {
        let signature;
        if host.truthy(&*host.read("remote")?) {
            let signer = host.signer(host.read("remote")?)?;
            let id = host.nanoid()?;
            let account_name = host.field(&*host.read("remote")?, "accountName", false)?;
            let to_sign = host.read("pending")?;
            let challenge = host.read("pending")?;
            let result = host
                .sign(
                    signer,
                    SignRequest {
                        id,
                        kind: "ws-auth",
                        exchange: "kraken",
                        account_name,
                        to_sign,
                        output_format: "kraken-challenge",
                        challenge,
                    },
                )?
                .await?;
            if host.truthy(&*host.field(&result, "error", false)?) {
                let error = host.field(&result, "error", false)?;
                let message = "Remote WS signing unavailable: "
                    .encode_utf16()
                    .chain(host.string(&error)?)
                    .collect();
                host.emit_error(message)?;
                host.authenticated(false)?;
                return Ok(());
            }
            host.write("original", Some(host.read("pending")?))?;
            signature = host.field(&result, "wsAuthSignature", false)?;
        } else {
            let hash = host.hash_challenge(|| host.read("pending"))?;
            let secret = host.decode_secret(host.read("secret")?)?;
            signature = host.hmac_signature(secret, hash)?;
            host.write("original", Some(host.read("pending")?))?;
        }
        host.write("signed", Some(signature))?;
        host.write("pending", None)?;
        host.authenticated(true)?;
        host.subscribe()?;
        Ok::<(), ExchangeError>(())
    }
    .await;
    if let Err(error) = body {
        let message = "Failed to perform auth: "
            .encode_utf16()
            .chain(host.error_text(&error)?)
            .collect();
        host.log(message, LogSeverity::Error)?;
        host.authenticated(false)?;
    }
    Ok(())
}

pub fn subscribe_private_feeds<H: AuthHost>(host: &H) -> Result<(), ExchangeError> {
    let body = || -> Result<(), ExchangeError> {
        let key = host.read("key")?;
        let original = host.read("original")?;
        let signed = host.read("signed")?;
        for feed in ["fills", "open_orders", "open_positions", "account_balances"] {
            host.send_subscription(Subscription {
                event: "subscribe",
                key: key.clone(),
                original: original.clone(),
                signed: signed.clone(),
                feed,
            })?;
        }
        Ok(())
    };
    // The source catches send/getter failures and stops this subscription batch.
    match body() {
        Ok(()) | Err(_) => Ok(()),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefreshCallback {
    Futures,
    Spot,
}

pub trait RefreshHost {
    type Value: ?Sized;
    type Timer: ?Sized;
    fn timeout(&self) -> Result<Rc<Self::Timer>, ExchangeError>;
    fn timer_truthy(&self, timer: &Self::Timer) -> bool;
    fn clear_timeout(&self, timer: Rc<Self::Timer>) -> Result<(), ExchangeError>;
    /// Capture the current Futures method or Spot closure before registering the timer.
    fn schedule(
        &self,
        callback: RefreshCallback,
        delay: u32,
    ) -> Result<Rc<Self::Timer>, ExchangeError>;
    fn set_timeout(&self, timer: Option<Rc<Self::Timer>>) -> Result<(), ExchangeError>;
    fn disposed(&self) -> Result<bool, ExchangeError>;
    fn fetch_orders(
        &self,
    ) -> Result<impl Future<Output = Result<Rc<Self::Value>, ExchangeError>>, ExchangeError>;
    fn current_loaded(&self) -> Result<Rc<Self::Value>, ExchangeError>;
    fn update_orders(
        &self,
        orders: Rc<Self::Value>,
        loaded: Rc<Self::Value>,
        orders_loaded: bool,
    ) -> Result<(), ExchangeError>;
    fn fetch_and_set_orders(
        &self,
    ) -> Result<impl Future<Output = Result<(), ExchangeError>>, ExchangeError>;
}

fn throttle<H: RefreshHost>(host: &H, callback: RefreshCallback) -> Result<(), ExchangeError> {
    if host.timer_truthy(&*host.timeout()?) {
        host.clear_timeout(host.timeout()?)?;
    }
    let timer = host.schedule(callback, 1000)?;
    host.set_timeout(Some(timer))
}

pub fn futures_throttled_fetch_orders<H: RefreshHost>(host: &H) -> Result<(), ExchangeError> {
    throttle(host, RefreshCallback::Futures)
}

pub fn spot_throttled_fetch_orders<H: RefreshHost>(host: &H) -> Result<(), ExchangeError> {
    throttle(host, RefreshCallback::Spot)
}

pub async fn futures_fetch_throttled_orders<H: RefreshHost>(host: &H) -> Result<(), ExchangeError> {
    host.set_timeout(None)?;
    if !host.disposed()? {
        let body = async {
            let orders = host.fetch_orders()?.await?;
            let loaded = host.current_loaded()?;
            host.update_orders(orders, loaded, true)
        }
        .await;
        // The source swallows refresh errors; they retry on the next event.
        match body {
            Ok(()) | Err(_) => {}
        }
    }
    Ok(())
}

pub async fn spot_refresh_callback<H: RefreshHost>(host: &H) -> Result<(), ExchangeError> {
    let body = async { host.fetch_and_set_orders()?.await }.await;
    match body {
        Ok(()) | Err(_) => Ok(()),
    }
}
