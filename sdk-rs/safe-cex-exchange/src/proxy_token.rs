//! Runtime-scoped port of packages/safe-cex/src/proxyTokenClient.ts.
use crate::runtime::{ExchangeRuntime, ExchangeTask};
use base64::Engine;
use safe_cex_store::virtual_clock::VirtualClock;
use std::{cell::RefCell, future::poll_fn, rc::Rc, task::Poll};

pub type TokenUpdateCallback = Rc<dyn Fn(&str) -> Result<(), String>>;
#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "type")]
pub enum BroadcastMessage {
    #[serde(rename = "proxyTokenUpdate")]
    ProxyTokenUpdate {
        token: String,
        #[serde(rename = "expiresAt")]
        expires_at: f64,
    },
    #[serde(rename = "proxyTokenRequest")]
    ProxyTokenRequest,
}
// Capture the whole tagged message before serde's internal enum buffer.
#[derive(serde::Deserialize)]
#[serde(remote = "BroadcastMessage", tag = "type")]
enum BroadcastMessageWire {
    #[serde(rename = "proxyTokenUpdate")]
    ProxyTokenUpdate {
        token: String,
        #[serde(rename = "expiresAt")]
        expires_at: f64,
    },
    #[serde(rename = "proxyTokenRequest")]
    ProxyTokenRequest,
}
impl<'de> serde::Deserialize<'de> for BroadcastMessage {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        <safe_cex_num::numeric_ingress::JsNumberInput as serde::Deserialize>::deserialize(
            deserializer,
        )?
        .decode_with(|decoder| {
            BroadcastMessageWire::deserialize(safe_cex_types::strict_decode::object(decoder))
        })
        .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for BroadcastMessage {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { ProxyTokenUpdate { token, expires_at }, ProxyTokenRequest });
    }
}

pub trait ProxyTokenHost {
    fn is_main_window(&self) -> bool;
    fn has_self(&self) -> bool;
    fn origin(&self) -> Option<String>;
    fn virtual_clock(&self) -> VirtualClock;
    fn initialize_broadcast(
        &self,
        channel: &str,
        receive: Rc<dyn Fn(BroadcastMessage)>,
    ) -> Result<bool, String>;
    fn broadcast(&self, message: BroadcastMessage) -> Result<(), String>;
    fn fetch<'a>(
        &'a self,
        url: String,
        credentials_include: bool,
    ) -> crate::runtime::ExchangeFuture<'a, Option<String>>;
}
struct Flight {
    result: Option<String>,
    waiters: crate::waiters::Waiters,
}
struct State {
    token: String,
    expires_at: f64,
    channel: bool,
    callbacks: Vec<TokenUpdateCallback>,
    flight: Option<Rc<RefCell<Flight>>>,
    last_fetch_attempt: f64,
    timer: Option<Box<dyn ExchangeTask>>,
    generation: u64,
}
// Construct before spawn: dropping an unpolled host task must release the flight too.
struct FlightCompletion {
    state: Rc<RefCell<State>>,
    flight: Rc<RefCell<Flight>>,
}
// Pre-spawn ownership covers discarded unpolled timers and inline completion.
struct RefreshTimerCompletion {
    state: Rc<RefCell<State>>,
    generation: u64,
}
impl Drop for RefreshTimerCompletion {
    fn drop(&mut self) {
        let timer = {
            let mut state = self.state.borrow_mut();
            if state.generation != self.generation {
                return;
            }
            state.generation += 1;
            state.timer.take()
        };
        // Host handle destruction can reenter; never hold the state borrow.
        drop(timer);
    }
}
impl Drop for FlightCompletion {
    fn drop(&mut self) {
        let token = {
            let mut state = self.state.borrow_mut();
            if state
                .flight
                .as_ref()
                .is_some_and(|flight| Rc::ptr_eq(flight, &self.flight))
            {
                state.flight = None;
            }
            state.token.clone()
        };
        let waiters = {
            let mut flight = self.flight.borrow_mut();
            if flight.result.is_some() {
                return;
            }
            flight.result = Some(token);
            flight.waiters.clone()
        };
        waiters.wake_all();
    }
}

#[derive(Clone)]
pub struct ProxyTokenClient {
    state: Rc<RefCell<State>>,
    host: Rc<dyn ProxyTokenHost>,
    runtime: Rc<dyn ExchangeRuntime>,
}
impl ProxyTokenClient {
    pub fn new(host: Rc<dyn ProxyTokenHost>, runtime: Rc<dyn ExchangeRuntime>) -> Self {
        Self {
            host,
            runtime,
            state: Rc::new(RefCell::new(State {
                token: String::new(),
                expires_at: 0.,
                channel: false,
                callbacks: vec![],
                flight: None,
                last_fetch_attempt: 0.,
                timer: None,
                generation: 0,
            })),
        }
    }
    pub fn is_main_window(&self) -> bool {
        self.host.is_main_window()
    }
    pub fn is_worker_context(&self) -> bool {
        !self.is_main_window() && self.host.has_self()
    }
    pub fn get_proxy_key_api_url(&self) -> String {
        format!("{}/api/proxykey", self.host.origin().unwrap_or_default())
    }
    pub fn get_token_expiration(token: &str) -> f64 {
        let Some(value) = decode_token(token) else {
            return 0.;
        };
        expiration(&value).unwrap_or(0.0)
    }
    pub fn init_broadcast_channel(&self) {
        if self.state.borrow().channel {
            return;
        }
        let weak = Rc::downgrade(&self.state);
        let host = Rc::downgrade(&self.host);
        let runtime = Rc::downgrade(&self.runtime);
        let receive: Rc<dyn Fn(BroadcastMessage)> = Rc::new(move |message| {
            let (Some(state), Some(host), Some(runtime)) =
                (weak.upgrade(), host.upgrade(), runtime.upgrade())
            else {
                return;
            };
            let client = Self {
                state,
                host: host.clone(),
                runtime: runtime.clone(),
            };
            match message {
                BroadcastMessage::ProxyTokenUpdate { token, expires_at } => {
                    let mut state = client.state.borrow_mut();
                    state.token = token.clone();
                    state.expires_at = expires_at;
                    drop(state);
                    client.notify_callbacks(&token, false);
                }
                BroadcastMessage::ProxyTokenRequest => {
                    if host.is_main_window() {
                        let token = client.get_current_token();
                        if !token.is_empty() && !client.token_expired(&token) {
                            let expiry = client.state.borrow().expires_at;
                            client.broadcast_token_update(token, expiry);
                        } else {
                            let future = client.fetch_proxy_token();
                            let _task = runtime.spawn_local(Box::pin(async move {
                                let _ = future.await;
                            }));
                        }
                    }
                }
            }
        });
        match self
            .host
            .initialize_broadcast("proxy-token-channel", receive)
        {
            Ok(channel) => self.state.borrow_mut().channel = channel,
            Err(error) => self.runtime.console(
                "error",
                &format!("[ProxyTokenClient] Failed to create BroadcastChannel: {error}"),
            ),
        }
    }
    fn notify_callbacks(&self, token: &str, read_cache: bool) {
        let mut index = 0;
        loop {
            let callback = self.state.borrow().callbacks.get(index).cloned();
            let Some(callback) = callback else { break };
            let token = if read_cache {
                self.get_current_token()
            } else {
                token.to_owned()
            };
            let _ = callback(&token);
            index += 1;
        }
    }
    pub fn broadcast_token_update(&self, token: String, expires_at: f64) {
        self.init_broadcast_channel();
        if self.state.borrow().channel {
            let _ = self
                .host
                .broadcast(BroadcastMessage::ProxyTokenUpdate { token, expires_at });
        }
    }
    pub fn request_token_from_main_thread(&self) {
        self.init_broadcast_channel();
        if self.state.borrow().channel {
            let _ = self.host.broadcast(BroadcastMessage::ProxyTokenRequest);
        }
    }
    pub fn schedule_refresh(&self) {
        let mut state = self.state.borrow_mut();
        state.generation += 1;
        let generation = state.generation;
        let timer = state.timer.take();
        let expiry = state.expires_at;
        let valid = !state.token.is_empty() && (expiry > 0. || expiry.is_nan());
        drop(state);
        if let Some(timer) = timer {
            timer.cancel();
        }
        if !valid {
            return;
        }
        let delay = if expiry.is_nan() {
            0
        } else {
            (expiry - 60000. - self.runtime.now_ms()).max(30000.) as u64
        };
        let sleep = self.runtime.sleep(delay);
        let client = self.clone();
        let completion = RefreshTimerCompletion {
            state: self.state.clone(),
            generation,
        };
        let task = self.runtime.spawn_local(Box::pin(async move {
            let _completion = completion;
            if sleep.await.is_ok() && client.state.borrow().generation == generation {
                let _ = client.fetch_proxy_token().await;
            }
        }));
        let mut state = self.state.borrow_mut();
        if state.generation == generation {
            state.timer = Some(task);
        } else {
            drop(state);
            task.cancel();
        }
    }
    pub fn fetch_proxy_token(&self) -> crate::runtime::ExchangeFuture<'static, String> {
        let existing = self.state.borrow().flight.clone();
        if let Some(flight) = existing {
            return wait_for_flight(flight);
        }
        let now = self.runtime.now_ms();
        {
            let mut state = self.state.borrow_mut();
            if now - state.last_fetch_attempt < 30000. && !state.token.is_empty() {
                let token = state.token.clone();
                return Box::pin(async move { Ok(token) });
            }
            state.last_fetch_attempt = now;
        }
        let flight = Rc::new(RefCell::new(Flight {
            result: None,
            waiters: crate::waiters::Waiters::default(),
        }));
        self.state.borrow_mut().flight = Some(flight.clone());
        let client = self.clone();
        let completion = FlightCompletion {
            state: self.state.clone(),
            flight: flight.clone(),
        };
        let _task = self.runtime.spawn_local(Box::pin(async move {
            let result = client
                .host
                .fetch(client.get_proxy_key_api_url(), true)
                .await;
            if let Ok(Some(token)) = result
                && !token.is_empty()
            {
                let expires_at = Self::get_token_expiration(&token);
                {
                    let mut state = client.state.borrow_mut();
                    state.token = token.clone();
                    state.expires_at = expires_at;
                }
                client.broadcast_token_update(token.clone(), expires_at);
                client.notify_callbacks(&token, true);
                client.schedule_refresh();
            }
            drop(completion);
        }));
        wait_for_flight(flight)
    }
    pub async fn get_proxy_token(&self) -> String {
        self.init_broadcast_channel();
        let token = self.get_current_token();
        if !token.is_empty() && !self.token_expired(&token) {
            return token;
        }
        if self.is_main_window() {
            return self.fetch_proxy_token().await.unwrap_or_default();
        }
        if self.is_worker_context() {
            self.request_token_from_main_thread();
            let token = self.get_current_token();
            if !token.is_empty() && !self.token_expired(&token) {
                return token;
            }
            return self.fetch_proxy_token().await.unwrap_or_default();
        }
        token
    }
    pub async fn refresh_proxy_token(&self) -> String {
        self.state.borrow_mut().last_fetch_attempt = 0.;
        self.fetch_proxy_token().await.unwrap_or_default()
    }
    pub fn get_current_token(&self) -> String {
        self.state.borrow().token.clone()
    }
    pub fn set_token(&self, token: String) {
        let expires_at = Self::get_token_expiration(&token);
        let mut state = self.state.borrow_mut();
        state.token = token;
        state.expires_at = expires_at;
    }
    pub fn on_token_update(&self, callback: TokenUpdateCallback) -> Box<dyn Fn()> {
        self.state.borrow_mut().callbacks.push(callback.clone());
        let weak = Rc::downgrade(&self.state);
        Box::new(move || {
            if let Some(state) = weak.upgrade() {
                let mut state = state.borrow_mut();
                if let Some(index) = state
                    .callbacks
                    .iter()
                    .position(|item| Rc::ptr_eq(item, &callback))
                {
                    state.callbacks.remove(index);
                }
            }
        })
    }
    pub async fn initialize_proxy_token_client(&self) {
        self.init_broadcast_channel();
        if self.is_main_window() {
            let _ = self.fetch_proxy_token().await;
        }
    }
    fn token_expired(&self, token: &str) -> bool {
        is_token_expired(token, &self.host.virtual_clock())
    }
}
/// Shared proxy-JWT expiry contract; callers supply their service clock clone.
/// Malformed JWTs and failed JavaScript coercions are expired, as in TypeScript.
pub fn is_token_expired(token: &str, clock: &VirtualClock) -> bool {
    if token.is_empty() {
        return true;
    }
    let Some(value) = decode_token(token) else {
        return true;
    };
    if !crate::error_reporting::truthy(&value) {
        return true;
    }
    expiration(&value).map_or(true, |expiry| expiry < clock.get_current_time_ms())
}

fn expiration(value: &serde_json::Value) -> Result<f64, ()> {
    let Some(value) = value.get("exp") else {
        return Ok(0.0);
    };
    let falsy = match value {
        serde_json::Value::Null => true,
        serde_json::Value::Bool(value) => !value,
        serde_json::Value::Number(_) => js_number(value)? == 0.0,
        serde_json::Value::String(value) => value.is_empty(),
        _ => false,
    };
    if falsy {
        Ok(0.0)
    } else {
        js_number(value).map(|number| number * 1000.0)
    }
}
fn js_number(value: &serde_json::Value) -> Result<f64, ()> {
    match value {
        serde_json::Value::Null => Ok(0.0),
        serde_json::Value::Bool(value) => Ok(if *value { 1.0 } else { 0.0 }),
        serde_json::Value::Number(value) => Ok(value
            .as_f64()
            .unwrap_or_else(|| value.to_string().parse().unwrap())),
        serde_json::Value::String(value) => Ok(js_number_text(value)),
        serde_json::Value::Array(_) => Ok(js_number_text(&js_array_text(value)?)),
        serde_json::Value::Object(value) => {
            if value.contains_key("toString") {
                Err(())
            } else {
                Ok(f64::NAN)
            }
        }
    }
}
fn js_array_text(value: &serde_json::Value) -> Result<String, ()> {
    match value {
        serde_json::Value::Null => Ok(String::new()),
        serde_json::Value::Array(values) => values
            .iter()
            .map(js_array_text)
            .collect::<Result<Vec<_>, _>>()
            .map(|parts| parts.join(",")),
        serde_json::Value::String(value) => Ok(value.clone()),
        serde_json::Value::Number(value) => Ok(safe_cex_num::js_number_string(
            value
                .as_f64()
                .unwrap_or_else(|| value.to_string().parse().unwrap()),
        )),
        serde_json::Value::Bool(value) => Ok(value.to_string()),
        serde_json::Value::Object(value) => {
            if value.contains_key("toString") {
                Err(())
            } else {
                Ok("[object Object]".into())
            }
        }
    }
}
fn js_number_text(text: &str) -> f64 {
    let text = safe_cex_num::js_trim(text);
    if text.is_empty() {
        return 0.0;
    }
    match text {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    for (prefix, radix) in [
        ("0x", 16),
        ("0X", 16),
        ("0o", 8),
        ("0O", 8),
        ("0b", 2),
        ("0B", 2),
    ] {
        if let Some(digits) = text.strip_prefix(prefix) {
            if digits.is_empty() {
                return f64::NAN;
            }
            let digit_bits = match radix {
                2 => 1,
                8 => 3,
                _ => 4,
            };
            let mut bits = 0;
            let mut mantissa = 0_u64;
            let mut guard = false;
            let mut sticky = false;
            for digit in digits.chars() {
                let Some(digit) = digit.to_digit(radix) else {
                    return f64::NAN;
                };
                for shift in (0..digit_bits).rev() {
                    let bit = (digit >> shift) & 1;
                    if bits == 0 && bit == 0 {
                        continue;
                    }
                    bits = (bits + 1).min(1025);
                    if bits <= 53 {
                        mantissa = (mantissa << 1) | u64::from(bit);
                    } else if bits == 54 {
                        guard = bit == 1;
                    } else {
                        sticky |= bit == 1;
                    }
                }
            }
            if bits <= 53 {
                return mantissa as f64;
            }
            if bits > 1024 {
                return f64::INFINITY;
            }
            if guard && (sticky || mantissa & 1 == 1) {
                mantissa += 1;
            }
            return mantissa as f64 * 2.0_f64.powi(bits - 53);
        }
    }
    let bytes = text.as_bytes();
    let mut index = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let start = index;
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    let integer_digits = index - start;
    let mut fractional_digits = 0;
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        let start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        fractional_digits = index - start;
    }
    if integer_digits + fractional_digits == 0 {
        return f64::NAN;
    }
    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        index += 1;
        if matches!(bytes.get(index), Some(b'+' | b'-')) {
            index += 1;
        }
        let start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if index == start {
            return f64::NAN;
        }
    }
    if index != bytes.len() {
        return f64::NAN;
    }
    text.parse().unwrap_or(f64::NAN)
}

fn decode_token(token: &str) -> Option<serde_json::Value> {
    // jws decode uses its URL-safe alphabet regex, not signature verification.
    let pieces: Vec<_> = token.split('.').collect();
    if pieces.len() != 3
        || pieces[0].is_empty()
        || pieces[1].is_empty()
        || pieces.iter().any(|piece| {
            !piece
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        })
    {
        return None;
    }
    let decoder = base64::engine::GeneralPurpose::new(
        &base64::alphabet::URL_SAFE,
        base64::engine::GeneralPurposeConfig::new()
            .with_decode_padding_mode(base64::engine::DecodePaddingMode::RequireNone)
            .with_decode_allow_trailing_bits(true),
    );
    let header = decoder.decode(pieces[0]).ok()?;
    let header: serde_json::Value = serde_json::from_str(
        &header
            .iter()
            .map(|byte| char::from(*byte))
            .collect::<String>(),
    )
    .ok()?;
    if !crate::error_reporting::truthy(&header) {
        return None;
    }
    let bytes = decoder.decode(pieces[1]).ok()?;
    let mut value: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&bytes)).ok()?;
    // jsonwebtoken additionally parses a string payload containing an object/array.
    if let Some(text) = value.as_str()
        && let Ok(parsed) = serde_json::from_str::<serde_json::Value>(text)
        && (parsed.is_object() || parsed.is_array())
    {
        value = parsed;
    }
    Some(value)
}

fn wait_for_flight(flight: Rc<RefCell<Flight>>) -> crate::runtime::ExchangeFuture<'static, String> {
    let registration = flight.borrow().waiters.register();
    Box::pin(poll_fn(move |cx| {
        let flight = flight.borrow();
        if let Some(value) = &flight.result {
            Poll::Ready(Ok(value.clone()))
        } else {
            drop(flight);
            registration.update(cx.waker());
            Poll::Pending
        }
    }))
}

#[cfg(test)]
#[path = "proxy_token_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "proxy_refresh_drop_tests.rs"]
mod refresh_drop_tests;
