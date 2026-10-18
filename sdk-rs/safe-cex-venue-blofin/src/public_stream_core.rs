//! Subscription startup and connection watchdog bodies from blofin.ws-public.ts.
//! The complete public socket facade and inbound/listener bodies remain separate.
use safe_cex_num::{JsJsonValue, js_json_stringify};
use safe_cex_types::object_map::JsObjectMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

pub const BLOFIN_MAX_SUBSCRIBE_FRAME_BYTES: f64 = 4096.0;
const CONNECTION_FAILURE_NOTICE_DELAY_MS: f64 = 20_000.0;
const CONNECTION_FAILURE_NOTICE: &str = "BloFin live WebSocket connection failed to establish. Historical candles may still load, but live charts and order books will not update. Check your network, VPN, firewall, or browser extensions, then reload Tealstreet.";

/// Retained own JSON properties, including input order and extra fields.
/// No getters/toJSON callbacks, cycles, BigInt or lone surrogates cross this boundary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SubscriptionTopic {
    pub fields: JsObjectMap<Value>,
}
fn topic(channel: &str, inst_id: String) -> SubscriptionTopic {
    SubscriptionTopic {
        fields: [
            ("channel".into(), Value::String(channel.into())),
            ("instId".into(), Value::String(inst_id)),
        ]
        .into_iter()
        .collect(),
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublicStreamError<E> {
    Host(E),
    FrameTooLarge,
}
impl<E: std::fmt::Display> std::fmt::Display for PublicStreamError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Host(e) => e.fmt(f),
            Self::FrameTooLarge => {
                f.write_str("BloFin subscription topic exceeds the maximum WebSocket frame size")
            }
        }
    }
}
impl<E: std::error::Error> std::error::Error for PublicStreamError<E> {}

fn frame(topics: &[SubscriptionTopic]) -> String {
    let args = topics
        .iter()
        .map(|topic| {
            JsJsonValue::Object(
                topic
                    .fields
                    .iter()
                    .map(|(key, value)| (key.clone(), JsJsonValue::from(value.clone())))
                    .collect(),
            )
        })
        .collect();
    js_json_stringify(&JsJsonValue::Object(vec![
        ("op".into(), JsJsonValue::String("subscribe".into())),
        ("args".into(), JsJsonValue::Array(args)),
    ]))
    .expect("JSON-only topic serialization")
    .expect("object frame is defined")
}
pub fn build_blofin_subscribe_frames(
    topics: &[SubscriptionTopic],
    max_bytes: Option<f64>,
) -> Result<Vec<String>, PublicStreamError<std::convert::Infallible>> {
    let max_bytes = max_bytes.unwrap_or(BLOFIN_MAX_SUBSCRIBE_FRAME_BYTES);
    let mut frames = Vec::new();
    let mut batch = Vec::new();
    for topic in topics {
        let single_topic_frame = frame(std::slice::from_ref(topic));
        if single_topic_frame.len() as f64 > max_bytes {
            return Err(PublicStreamError::FrameTooLarge);
        }
        let mut candidate = batch.clone();
        candidate.push(topic.clone());
        let serialized = frame(&candidate);
        if !batch.is_empty() && serialized.len() as f64 > max_bytes {
            frames.push(frame(&batch));
            batch = vec![topic.clone()];
            continue;
        }
        batch = candidate;
    }
    if !batch.is_empty() {
        frames.push(frame(&batch));
    }
    Ok(frames)
}

/// Body state only. Topic arrays are owned snapshots rather than externally
/// mutable JS aliases; base lifecycle, inbound dispatch and listeners are not here.
pub struct PublicStreamState<Timer> {
    pub topics: RefCell<JsObjectMap<Vec<SubscriptionTopic>>>,
    pub ping_at: Cell<f64>,
    pub connection_failure_notice_timeout: RefCell<Option<Timer>>,
    pub connection_failure_notified: Cell<bool>,
}
impl<Timer> Default for PublicStreamState<Timer> {
    fn default() -> Self {
        Self {
            topics: RefCell::new(JsObjectMap::new()),
            ping_at: Cell::new(0.0),
            connection_failure_notice_timeout: RefCell::new(None),
            connection_failure_notified: Cell::new(false),
        }
    }
}

/// Synchronous base/socket/emitter capabilities. Timers MUST defer callbacks
/// until after set_timeout returns, as browser/Node setTimeout does. Timer truthiness
/// is explicit to preserve numeric zero handles and object-valued Node handles.
pub trait PublicStreamHost: 'static {
    type Error: 'static;
    type Timer: Clone + 'static;
    fn is_disposed(&self) -> bool;
    fn is_testnet(&self) -> bool;
    fn market_ids(&self) -> Result<Vec<String>, Self::Error>;
    fn has_socket(&self) -> bool;
    fn create_and_assign_socket(&self, url: &str) -> Result<(), Self::Error>;
    fn add_socket_listener(&self, event: &str) -> Result<(), Self::Error>;
    fn send(&self, frame: &str) -> Result<(), Self::Error>;
    fn performance_now(&self) -> f64;
    fn markets_changed_off(&self) -> Result<(), Self::Error>;
    fn markets_changed_on(&self) -> Result<(), Self::Error>;
    fn reconnecting_enabled(&self) -> bool;
    fn emit_ws_reconnected(&self) -> Result<(), Self::Error>;
    fn set_timeout(
        &self,
        callback: Rc<dyn Fn() -> Result<(), Self::Error>>,
        delay_ms: f64,
    ) -> Result<Self::Timer, Self::Error>;
    fn timer_is_truthy(&self, timer: &Self::Timer) -> bool;
    fn clear_timeout(&self, timer: &Self::Timer) -> Result<(), Self::Error>;
    fn log_warning(&self, message: &str) -> Result<(), Self::Error>;
    fn emit_warning(&self, message: &str, force_toast: bool) -> Result<(), Self::Error>;
}

pub fn connect_and_subscribe<H: PublicStreamHost>(
    state: &Rc<PublicStreamState<H::Timer>>,
    host: &Rc<H>,
) -> Result<(), PublicStreamError<H::Error>> {
    if !host.is_disposed() {
        arm_connection_failure_notice(state, host)?;
        let network = if host.is_testnet() {
            "testnet"
        } else {
            "livenet"
        };
        let url = crate::types::BASE_WS_URL["public"][network]
            .as_str()
            .expect("declared Blofin public URL");
        host.create_and_assign_socket(url)
            .map_err(PublicStreamError::Host)?;
        let tickers = host
            .market_ids()
            .map_err(PublicStreamError::Host)?
            .into_iter()
            .map(|id| topic("tickers", id))
            .collect();
        state.topics.borrow_mut().insert("tickers".into(), tickers);
        let funding = host
            .market_ids()
            .map_err(PublicStreamError::Host)?
            .into_iter()
            .map(|id| topic("funding-rate", id))
            .collect();
        state
            .topics
            .borrow_mut()
            .insert("funding-rate".into(), funding);
        for event in ["open", "message", "close"] {
            host.add_socket_listener(event)
                .map_err(PublicStreamError::Host)?;
        }
    }
    Ok(())
}
pub fn on_open<H: PublicStreamHost>(
    state: &PublicStreamState<H::Timer>,
    host: &H,
) -> Result<(), PublicStreamError<H::Error>> {
    if !host.is_disposed() {
        subscribe(state, host)?;
        ping(state, host)?;
        host.markets_changed_off()
            .map_err(PublicStreamError::Host)?;
        host.markets_changed_on().map_err(PublicStreamError::Host)?;
        if host.reconnecting_enabled() {
            host.emit_ws_reconnected()
                .map_err(PublicStreamError::Host)?;
        }
    }
    Ok(())
}
pub fn on_markets_changed<H: PublicStreamHost>(
    state: &PublicStreamState<H::Timer>,
    host: &H,
    added: &[String],
) -> Result<(), PublicStreamError<H::Error>> {
    if added.is_empty() || !host.has_socket() || host.is_disposed() {
        return Ok(());
    }
    let ticker_topics: Vec<_> = added
        .iter()
        .map(|id| topic("tickers", id.clone()))
        .collect();
    let funding_topics: Vec<_> = added
        .iter()
        .map(|id| topic("funding-rate", id.clone()))
        .collect();
    for (key, new_topics) in [
        ("tickers", &ticker_topics),
        ("funding-rate", &funding_topics),
    ] {
        let mut topics = state.topics.borrow_mut();
        let mut combined = topics.get(key).cloned().unwrap_or_default();
        combined.extend_from_slice(new_topics);
        topics.insert(key.into(), combined);
    }
    let combined: Vec<_> = ticker_topics.into_iter().chain(funding_topics).collect();
    send_subscription_topics(host, &combined)
}
pub fn ping<H: PublicStreamHost>(
    state: &PublicStreamState<H::Timer>,
    host: &H,
) -> Result<(), PublicStreamError<H::Error>> {
    if !host.is_disposed() {
        state.ping_at.set(host.performance_now());
        host.send("ping").map_err(PublicStreamError::Host)?;
    }
    Ok(())
}
pub fn subscribe<H: PublicStreamHost>(
    state: &PublicStreamState<H::Timer>,
    host: &H,
) -> Result<(), PublicStreamError<H::Error>> {
    let topics: Vec<_> = state.topics.borrow().values().flatten().cloned().collect();
    send_subscription_topics(host, &topics)
}
pub fn send_subscription_topics<H: PublicStreamHost>(
    host: &H,
    topics: &[SubscriptionTopic],
) -> Result<(), PublicStreamError<H::Error>> {
    // Build all frames BEFORE sending any: an oversized later topic causes zero sends.
    let frames = build_blofin_subscribe_frames(topics, None).map_err(|e| match e {
        PublicStreamError::FrameTooLarge => PublicStreamError::FrameTooLarge,
        PublicStreamError::Host(never) => match never {},
    })?;
    for frame in frames {
        host.send(&frame).map_err(PublicStreamError::Host)?;
    }
    Ok(())
}
pub fn arm_connection_failure_notice<H: PublicStreamHost>(
    state: &Rc<PublicStreamState<H::Timer>>,
    host: &Rc<H>,
) -> Result<(), PublicStreamError<H::Error>> {
    let timeout = state.connection_failure_notice_timeout.borrow().clone();
    if host.is_disposed()
        || state.connection_failure_notified.get()
        || timeout
            .as_ref()
            .is_some_and(|timer| host.timer_is_truthy(timer))
    {
        return Ok(());
    }
    let callback_state = state.clone();
    let callback_host = host.clone();
    let callback = Rc::new(move || {
        *callback_state
            .connection_failure_notice_timeout
            .borrow_mut() = None;
        if callback_host.is_disposed() || callback_state.connection_failure_notified.get() {
            return Ok(());
        }
        callback_state.connection_failure_notified.set(true);
        callback_host.log_warning(CONNECTION_FAILURE_NOTICE)?;
        callback_host.emit_warning(CONNECTION_FAILURE_NOTICE, true)
    });
    let timer = host
        .set_timeout(callback, CONNECTION_FAILURE_NOTICE_DELAY_MS)
        .map_err(PublicStreamError::Host)?;
    *state.connection_failure_notice_timeout.borrow_mut() = Some(timer);
    Ok(())
}
pub fn mark_connection_established<H: PublicStreamHost>(
    state: &PublicStreamState<H::Timer>,
    host: &H,
) -> Result<(), PublicStreamError<H::Error>> {
    let timeout = state.connection_failure_notice_timeout.borrow().clone();
    if let Some(timer) = timeout.filter(|timer| host.timer_is_truthy(timer)) {
        host.clear_timeout(&timer)
            .map_err(PublicStreamError::Host)?;
        *state.connection_failure_notice_timeout.borrow_mut() = None;
    }
    state.connection_failure_notified.set(false);
    Ok(())
}
