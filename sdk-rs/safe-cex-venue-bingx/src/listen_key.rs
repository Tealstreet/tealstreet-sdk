//! Listen-key lifecycle bodies over configured HTTP and imported socket/timer operations.
use crate::{constants, data_reads::ReadTransport, private_events::private_ping};
use safe_cex_exchange::runtime::{ExchangeError, ExchangeFuture};
use safe_cex_num::numeric_ingress::{JsNumberOutput, js_deserialize_passthrough, js_value_as_f64};
use safe_cex_num::{JsJsonValue, js_json_stringify, js_string, js_truthy};
use safe_cex_transport::RestRequest;
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

/// The opaque token and arbitrary host errors preserve source timer identity.
pub fn clear_refresh_timer<T, E>(
    timer: Option<T>,
    truthy: impl FnOnce(&T) -> bool,
    clear: impl FnOnce(T) -> Result<(), E>,
    unset: impl FnOnce() -> Result<(), E>,
) -> Result<(), E> {
    if let Some(timer) = timer
        && truthy(&timer)
    {
        clear(timer)?;
        unset()?;
    }
    Ok(())
}
#[derive(Clone, Copy)]
pub enum PrivateListener {
    Message,
    Close,
    Open,
}
/// Imported socket/listener identities and BaseWebSocket/timer operations, not venue bodies.
pub trait ListenKeyHost {
    fn is_disposed(&self) -> bool;
    fn testnet(&self) -> bool;
    fn create_socket(&self, url: &str) -> Result<u64, ExchangeError>;
    fn add_listener(&self, socket: u64, listener: PrivateListener) -> Result<(), ExchangeError>;
    fn remove_listener(&self, socket: u64, listener: PrivateListener) -> Result<(), ExchangeError>;
    fn close_socket(&self, socket: u64) -> Result<(), ExchangeError>;
    fn retry_failed_connect(&self, message: &str) -> Result<(), ExchangeError>;
    fn warning(&self, message: &str) -> Result<(), ExchangeError>;
    fn set_timeout(
        &self,
        delay: f64,
        callback: Rc<dyn Fn() -> Result<(), ExchangeError>>,
    ) -> Result<u64, ExchangeError>;
    fn clear_timeout(&self, timer: u64) -> Result<(), ExchangeError>;
    fn spawn(&self, future: ExchangeFuture<'static, ()>) -> Result<(), ExchangeError>;
    fn monotonic_now_ms(&self) -> Result<f64, ExchangeError>;
    fn set_ping_at(&self, at: f64) -> Result<(), ExchangeError>;
    fn send_with_log(&self, frame: &str) -> Result<(), ExchangeError>;
    fn base_dispose(&self) -> Result<(), ExchangeError>;
    fn report_exception(&self, error: &ExchangeError);
}
pub struct BingxListenKey {
    pub transport: Rc<dyn ReadTransport>,
    pub host: Rc<dyn ListenKeyHost>,
    pub listen_key: RefCell<Option<Value>>,
    pub refresh_timer: Cell<Option<u64>>,
    pub socket: Cell<Option<u64>>,
    pub connecting: Cell<bool>,
    pub rebuilding: Cell<bool>,
}
#[derive(Serialize)]
#[serde(transparent)]
struct Response {
    value: Value,
}
impl JsNumberOutput for Response {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{} passthrough{value});
    }
}
fn text(value: Option<&Value>) -> Result<String, ExchangeError> {
    js_string(value).map_err(|e| {
        let mut error = ExchangeError::new(e.message);
        error.name = e.name.into();
        error
    })
}
fn error_message(error: &ExchangeError) -> Result<String, ExchangeError> {
    let response_view = error.response_json_view();
    let msg = response_view
        .as_ref()
        .and_then(|r| r.get("data"))
        .and_then(|d| d.get("msg"));
    if js_truthy(msg) {
        text(msg)
    } else if !error.message.is_empty() {
        Ok(error.message.clone())
    } else {
        Ok(error.js_error_string())
    }
}
fn expired_message(message: &str) -> bool {
    let units = message
        .encode_utf16()
        .map(|u| if (65..=90).contains(&u) { u + 32 } else { u })
        .collect::<Vec<_>>();
    let newline = |u: u16| matches!(u, 10 | 13 | 0x2028 | 0x2029);
    let literal = |at: usize, word: &str| {
        units
            .get(at..at + word.len())
            .is_some_and(|v| v.iter().copied().eq(word.bytes().map(u16::from)))
    };
    for i in 0..units.len() {
        if !literal(i, "listen") {
            continue;
        }
        for gap in 0..=1 {
            let start = i + 6;
            if gap == 1 && (start >= units.len() || newline(units[start])) {
                continue;
            }
            let key = start + gap;
            if !literal(key, "key") {
                continue;
            }
            let end = (key + 3..units.len())
                .find(|&j| newline(units[j]))
                .unwrap_or(units.len());
            for j in key + 3..end {
                for word in ["expired", "does not exist", "invalid"] {
                    if j + word.len() <= end && literal(j, word) {
                        return true;
                    }
                }
            }
        }
    }
    false
}
impl BingxListenKey {
    pub fn new(transport: Rc<dyn ReadTransport>, host: Rc<dyn ListenKeyHost>) -> Rc<Self> {
        Rc::new(Self {
            transport,
            host,
            listen_key: RefCell::default(),
            refresh_timer: Cell::new(None),
            socket: Cell::new(None),
            connecting: Cell::new(false),
            rebuilding: Cell::new(false),
        })
    }
    async fn request(&self, method: &str, params: Option<Value>) -> Result<Value, ExchangeError> {
        let response = self
            .transport
            .send(RestRequest {
                method: method.into(),
                url: constants::ENDPOINTS[16].1.into(),
                params,
                ..Default::default()
            })
            .await;
        let mut yielded = false;
        std::future::poll_fn(|cx| {
            if yielded {
                std::task::Poll::Ready(())
            } else {
                yielded = true;
                cx.waker().wake_by_ref();
                std::task::Poll::Pending
            }
        })
        .await;
        response?
            .decode_with(|decoder| {
                js_deserialize_passthrough(decoder).map(|value| Response { value })
            })
            .map(|r| r.value)
            .map_err(|e| ExchangeError::new(e.to_string()))
    }
    pub async fn fetch_listen_key(&self) -> Result<Value, ExchangeError> {
        let data = self.request("POST", None).await?;
        let key = data
            .get("listenKey")
            .filter(|v| js_truthy(Some(v)))
            .or_else(|| data.get("data").and_then(|d| d.get("listenKey")));
        if !js_truthy(key) {
            let serialized = js_json_stringify(&JsJsonValue::from(data.clone()))
                .map_err(|e| ExchangeError::new(e.message))?
                .unwrap_or_else(|| "undefined".into());
            return Err(ExchangeError::new(format!(
                "BingX listen key missing in response: {serialized}"
            )));
        }
        let key = key
            .cloned()
            .ok_or_else(|| ExchangeError::new("listen key vanished"))?;
        *self.listen_key.borrow_mut() = Some(key.clone());
        Ok(key)
    }
    pub async fn connect_and_subscribe(self: &Rc<Self>) -> Result<(), ExchangeError> {
        if self.host.is_disposed() || self.connecting.get() {
            return Ok(());
        }
        self.clear_timer()?;
        self.connecting.set(true);
        let result = self.fetch_listen_key().await;
        let key = match result {
            Ok(key) => key,
            Err(error) => {
                let result = error_message(&error).and_then(|message| {
                    self.host.retry_failed_connect(&format!(
                        "Failed to fetch BingX listen key: {message}"
                    ))
                });
                self.connecting.set(false);
                return result;
            }
        };
        self.connecting.set(false);
        if self.host.is_disposed() {
            return Ok(());
        }
        let environment = usize::from(self.host.testnet());
        let url = format!(
            "{}?listenKey={}",
            constants::BASE_WS_URL[1].1[environment].1,
            text(Some(&key))?
        );
        let socket = self.host.create_socket(&url)?;
        self.socket.set(Some(socket));
        for listener in [
            PrivateListener::Message,
            PrivateListener::Close,
            PrivateListener::Open,
        ] {
            self.host.add_listener(socket, listener)?;
        }
        Ok(())
    }
    pub fn on_open(self: &Rc<Self>) -> Result<(), ExchangeError> {
        if !self.host.is_disposed() {
            self.schedule_listen_key_refresh(None)?;
            private_ping(
                self.host.is_disposed(),
                || self.host.monotonic_now_ms(),
                |at| self.host.set_ping_at(at),
                |frame| self.host.send_with_log(frame),
            )?;
        }
        Ok(())
    }
    pub fn clear_timer(&self) -> Result<(), ExchangeError> {
        clear_refresh_timer(
            self.refresh_timer.get(),
            |timer| *timer != 0,
            |timer| self.host.clear_timeout(timer),
            || {
                self.refresh_timer.set(None);
                Ok(())
            },
        )
    }
    fn timer(self: &Rc<Self>, delay: f64, recovery: bool) -> Result<(), ExchangeError> {
        self.clear_timer()?;
        let weak = Rc::downgrade(self);
        let timer = self.host.set_timeout(
            delay,
            Rc::new(move || {
                let owner = weak
                    .upgrade()
                    .ok_or_else(|| ExchangeError::new("BingX listen-key owner released"))?;
                let host = owner.host.clone();
                let reporting = host.clone();
                host.spawn(Box::pin(async move {
                    let result = if recovery {
                        owner.rebuild_with_fresh_listen_key().await
                    } else {
                        owner.update_listen_key().await
                    };
                    if let Err(error) = &result {
                        reporting.report_exception(error);
                    }
                    result
                }))
            }),
        )?;
        self.refresh_timer.set(Some(timer));
        Ok(())
    }
    pub fn schedule_listen_key_refresh(
        self: &Rc<Self>,
        delay: Option<f64>,
    ) -> Result<(), ExchangeError> {
        self.timer(delay.unwrap_or(30.0 * 60.0 * 1000.0), false)
    }
    pub fn schedule_listen_key_recovery(self: &Rc<Self>) -> Result<(), ExchangeError> {
        self.timer(30.0 * 1000.0, true)
    }
    pub async fn update_listen_key(self: &Rc<Self>) -> Result<(), ExchangeError> {
        if self.host.is_disposed() {
            return Ok(());
        }
        let key = self.listen_key.borrow().clone();
        if !js_truthy(key.as_ref()) {
            return self.rebuild_with_fresh_listen_key().await;
        }
        let result = async {
            self.request("PUT", Some(json!({"listenKey":key}))).await?;
            self.schedule_listen_key_refresh(None)
        }
        .await;
        if let Err(error) = result {
            let message = error_message(&error)?;
            self.host.warning(&format!(
                "Failed to refresh BingX listen key: {message}. Retrying in 30s"
            ))?;
            let response_view = error.response_json_view();
            let status = response_view
                .as_ref()
                .and_then(|r| r.get("status"))
                .and_then(js_value_as_f64);
            if status == Some(404.0) || expired_message(&message) {
                self.rebuild_with_fresh_listen_key().await?;
            } else {
                self.schedule_listen_key_refresh(Some(30.0 * 1000.0))?;
            }
        }
        Ok(())
    }
    pub async fn rebuild_with_fresh_listen_key(self: &Rc<Self>) -> Result<(), ExchangeError> {
        if self.host.is_disposed() || self.rebuilding.get() {
            return Ok(());
        }
        self.rebuilding.set(true);
        self.clear_timer()?;
        *self.listen_key.borrow_mut() = None;
        let stale = self.socket.get();
        if let Some(socket) = stale {
            for listener in [
                PrivateListener::Message,
                PrivateListener::Close,
                PrivateListener::Open,
            ] {
                self.host.remove_listener(socket, listener)?;
            }
        }
        self.socket.set(None);
        if let Some(socket) = stale {
            self.host.close_socket(socket)?;
        }
        let result = self.connect_and_subscribe().await;
        let caught=match result{Ok(())=>Ok(()),Err(error)=>{match error_message(&error).and_then(|message|self.host.warning(&format!("Failed to rebuild BingX private WebSocket with a fresh listen key: {message}. Retrying in 30s"))){Ok(())=>self.schedule_listen_key_recovery(),Err(e)=>Err(e)}}};
        self.rebuilding.set(false);
        caught
    }
    pub fn dispose(&self) -> Result<(), ExchangeError> {
        self.clear_timer()?;
        *self.listen_key.borrow_mut() = None;
        self.host.base_dispose()
    }
}
