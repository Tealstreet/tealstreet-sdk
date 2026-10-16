//! Constructor/private-session/confirmed-mode bodies at configured current operations.
//! Own data graphs retain f64, enums and Rc identity. Whole classes/implicit fields,
//! imported-helper/base/socket wiring and discarded-job/drop/unwind ownership remain
//! caller integrations. Dense arrays, immutable Error rejections, no executable getters.
use crate::{
    private_event_value::*,
    request_weight::{Failure, type_error},
};
use safe_cex_exchange::runtime::LogSeverity;
use safe_cex_types::{PositionMode, object_map::JsObjectMap};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Venue {
    Futures,
    PM,
    Spot,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Factory {
    API,
    PAPI,
    FAPI,
    PublicAPI,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Child {
    Public,
    Private,
}
pub type RateHook = Rc<dyn Fn(EventValue) -> Result<EventValue, Failure>>;
pub trait ConstructorBindings {
    fn super_fields(
        &self,
        name: EventValue,
        opts: EventValue,
        store: EventValue,
    ) -> Result<EventValue, Failure>;
    fn split(
        &self,
        v: Venue,
        opts: EventValue,
        owner: EventValue,
        factory: Factory,
        rate: Option<RateHook>,
    ) -> Result<EventValue, Failure>;
    fn api(
        &self,
        v: Venue,
        kind: Factory,
        opts: EventValue,
        owner: EventValue,
    ) -> Result<EventValue, Failure>;
    fn rate_limit(&self, xhr: EventValue, max_rps: f64) -> Result<EventValue, Failure>;
    fn guards(&self, owner: EventValue, clients: EventValue) -> Result<(), Failure>;
    fn child(&self, v: Venue, kind: Child, owner: EventValue) -> Result<EventValue, Failure>;
}
fn array(v: Vec<EventValue>) -> EventValue {
    EventValue::array(v)
}
fn construct(
    v: Venue,
    name: EventValue,
    opts: EventValue,
    store: EventValue,
    b: Rc<dyn ConstructorBindings>,
) -> Result<EventValue, Failure> {
    let owner = b.super_fields(name, opts.clone(), store)?;
    let kind = if v == Venue::PM {
        Factory::PAPI
    } else {
        Factory::API
    };
    let rate = if v == Venue::PM {
        None
    } else {
        let r = b.clone();
        Some(Rc::new(move |xhr| r.rate_limit(xhr, 3.0)) as RateHook)
    };
    let split = b.split(v, opts.clone(), owner.clone(), kind, rate)?;
    let (primary, secondary) = if v == Venue::Spot {
        (EventValue::Undefined, EventValue::Undefined)
    } else {
        (split.read("1")?, split.read("2")?)
    };
    if v == Venue::Spot {
        owner.assign("xhr", split.clone());
    } else {
        let field = if v == Venue::PM { "papiXhr" } else { "xhr" };
        owner.assign(field, split.read("0")?);
        owner.assign("_primaryXhr", primary.clone());
        owner.assign("_secondaryXhr", secondary.clone());
    }
    match v {
        Venue::Futures => {
            owner.assign(
                "publicXhr",
                b.api(v, Factory::PublicAPI, opts.clone(), owner.clone())?,
            );
            owner.assign(
                "unlimitedXHR",
                b.api(v, Factory::API, opts.clone(), owner.clone())?,
            );
            let clients = array(vec![
                primary,
                secondary,
                owner.read("publicXhr")?,
                owner.read("unlimitedXHR")?,
            ]);
            b.guards(owner.clone(), clients)?;
        }
        Venue::PM => {
            owner.assign(
                "fapiXhr",
                b.api(v, Factory::FAPI, opts.clone(), owner.clone())?,
            );
            owner.assign(
                "unlimitedXHR",
                b.api(v, Factory::PAPI, opts.clone(), owner.clone())?,
            );
            owner.assign(
                "publicXhr",
                b.api(v, Factory::PublicAPI, opts.clone(), owner.clone())?,
            );
        }
        Venue::Spot => {
            owner.assign("unlimitedXHR", b.api(v, Factory::API, opts, owner.clone())?);
        }
    }
    owner.assign("publicWebsocket", b.child(v, Child::Public, owner.clone())?);
    owner.assign(
        "privateWebsocket",
        b.child(v, Child::Private, owner.clone())?,
    );
    Ok(owner)
}
// ts: BinanceExchange.constructor
pub fn construct_futures(
    name: EventValue,
    opts: EventValue,
    store: EventValue,
    b: Rc<dyn ConstructorBindings>,
) -> Result<EventValue, Failure> {
    construct(Venue::Futures, name, opts, store, b)
}
// ts: BinancePMExchange.constructor
pub fn construct_pm(
    name: EventValue,
    opts: EventValue,
    store: EventValue,
    b: Rc<dyn ConstructorBindings>,
) -> Result<EventValue, Failure> {
    construct(Venue::PM, name, opts, store, b)
}
// ts: BinanceSpotExchange.constructor
pub fn construct_spot(
    name: EventValue,
    opts: EventValue,
    store: EventValue,
    b: Rc<dyn ConstructorBindings>,
) -> Result<EventValue, Failure> {
    construct(Venue::Spot, name, opts, store, b)
}
pub struct ConnectState {
    pub disposed: crate::class_flags::LiveFlag,
    pub force_closed: crate::class_flags::LiveFlag,
    pub connecting: Cell<bool>,
    pub resync_orders: Cell<bool>,
    pub retry_delay: Cell<f64>,
    pub retry_timer: RefCell<EventValue>,
    pub ws: RefCell<EventValue>,
}
impl Default for ConnectState {
    fn default() -> Self {
        Self {
            disposed: Cell::new(false).into(),
            force_closed: Cell::new(false).into(),
            connecting: Cell::new(false),
            resync_orders: Cell::new(false),
            retry_delay: Cell::new(5000.0),
            retry_timer: RefCell::new(EventValue::Undefined),
            ws: RefCell::new(EventValue::Undefined),
        }
    }
}
pub type KeyFuture = Pin<Box<dyn Future<Output = Result<String, Failure>>>>;
pub type NativeKeyFuture = Pin<Box<dyn Future<Output = Result<EventValue, Failure>>>>;
pub type RetryAction = Rc<dyn Fn() -> Result<(), Failure>>;
pub trait ConnectBindings {
    fn state(&self) -> &ConnectState;
    fn options(&self) -> Result<EventValue, Failure>;
    fn clear_timeout(&self, ticket: EventValue) -> Result<(), Failure>;
    fn fetch_key(&self) -> Result<KeyFuture, Failure>;
    fn fetch_native_key(&self) -> Result<NativeKeyFuture, Failure> {
        let future = self.fetch_key()?;
        Ok(Box::pin(async move { future.await.map(string) }))
    }
    fn emit(&self, message: String) -> Result<(), Failure>;
    fn log(&self, message: String, severity: LogSeverity) -> Result<(), Failure>;
    fn set_timeout(&self, action: RetryAction, delay: f64) -> Result<EventValue, Failure>;
    /// Resolve/invoke CURRENT connect method; its async outcome is caller-owned.
    fn start_connect(&self) -> Result<(), Failure>;
    fn detach(&self) -> Result<(), Failure>;
    fn create(&self, url: String) -> Result<EventValue, Failure>;
    fn add_listener(&self, ws: EventValue, event: &'static str) -> Result<(), Failure>;
}
fn base_url(v: Venue, test: bool) -> &'static str {
    let all = match v {
        Venue::Futures => crate::binance_types::BASE_WS_URL,
        Venue::PM => crate::binancepm_types::BASE_WS_URL,
        Venue::Spot => crate::binancespot_types::BASE_WS_URL,
    };
    all.iter()
        .find(|(k, _)| *k == "private")
        .unwrap()
        .1
        .iter()
        .find(|(k, _)| *k == if test { "testnet" } else { "livenet" })
        .unwrap()
        .1
}
fn component(s: &str) -> String {
    let mut result = String::new();
    for b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(b) {
            result.push(*b as char)
        } else {
            result.push_str(&format!("%{b:02X}"));
        }
    }
    result
}
fn install(b: &dyn ConnectBindings, url: String) -> Result<(), Failure> {
    let ws = b.create(url)?;
    *b.state().ws.borrow_mut() = ws;
    for kind in ["message", "close", "open"] {
        let ws = b.state().ws.borrow().clone();
        b.add_listener(ws, kind)?;
    }
    Ok(())
}
async fn connect_legacy(v: Venue, b: Rc<dyn ConnectBindings>) -> Result<(), Failure> {
    if b.options()?.read("payloadSigner")?.truthy() {
        return Ok(());
    }
    if b.state().disposed.get() || b.state().connecting.get() {
        return Ok(());
    }
    let timer = b.state().retry_timer.borrow().clone();
    b.clear_timeout(timer)?;
    b.state().connecting.set(true);
    let result: Result<Option<EventValue>, Failure> = async {
        let key = match b.fetch_native_key() {
            Ok(f) => f.await,
            Err(e) => Err(e),
        };
        match key {
            Ok(key) => Ok(Some(key)),
            Err(error) => {
                let message = format!("Binance user data stream unavailable: {}", error.message);
                if b.state().retry_delay.get() == 5000.0 {
                    b.emit(message)?;
                } else {
                    b.log(message, LogSeverity::Warning)?;
                }
                b.state().resync_orders.set(true);
                let delay = b.state().retry_delay.get();
                b.state().retry_delay.set(if delay.is_nan() {
                    f64::NAN
                } else {
                    (delay * 2.0).min(60000.0)
                });
                let again = b.clone();
                let ticket = b.set_timeout(
                    Rc::new(move || {
                        if !again.state().disposed.get() && !again.state().force_closed.get() {
                            again.start_connect()?;
                        }
                        Ok(())
                    }),
                    delay,
                )?;
                *b.state().retry_timer.borrow_mut() = ticket;
                Ok(None)
            }
        }
    }
    .await;
    b.state().connecting.set(false);
    let Some(key) = result? else {
        return Ok(());
    };
    b.state().retry_delay.set(5000.0);
    if b.state().disposed.get() || b.state().force_closed.get() {
        return Ok(());
    }
    let base = base_url(v, b.options()?.read("testnet")?.truthy());
    let key = key.text()?;
    let url = if v == Venue::Futures {
        format!("{base}?listenKey={}", component(&key))
    } else {
        format!("{base}/{key}")
    };
    b.detach()?;
    install(b.as_ref(), url)
}
// ts: BinancePrivateWebsocket.connectAndSubscribe
pub async fn connect_futures(b: Rc<dyn ConnectBindings>) -> Result<(), Failure> {
    connect_legacy(Venue::Futures, b).await
}
// ts: BinancePMPrivateWebsocket.connectAndSubscribe
pub async fn connect_pm(b: Rc<dyn ConnectBindings>) -> Result<(), Failure> {
    connect_legacy(Venue::PM, b).await
}
// ts: BinancePrivateWebsocket.connectAndSubscribe (Spot)
pub async fn connect_spot(b: Rc<dyn ConnectBindings>) -> Result<(), Failure> {
    if b.options()?.read("payloadSigner")?.truthy() {
        return Ok(());
    }
    if b.state().disposed.get() {
        return Ok(());
    }
    let opts = b.options()?;
    let key = opts.read("key")?;
    let secret = opts.read("secret")?;
    let remote = opts.read("remoteSigner")?;
    if !key.truthy() || (!secret.truthy() && !remote.truthy()) {
        b.emit("Binance Spot private WebSocket requires an API key and signing secret".into())?;
        return Ok(());
    }
    let url = base_url(Venue::Spot, b.options()?.read("testnet")?.truthy());
    install(b.as_ref(), url.into())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageOp {
    Track,
    Diagnostic,
    Stats,
    Account,
    Order,
    Ping,
    Pong,
    Emit,
    ForceClose,
    Reconnect,
}
pub trait SpotMessageBindings {
    fn disposed(&self) -> bool;
    fn call(&self, op: MessageOp, args: Vec<EventValue>) -> Result<(), Failure>;
}
pub(crate) fn native(value: serde_json::Value) -> EventValue {
    EventValue::from_json(value)
}
pub(crate) fn parse_data(data: EventValue) -> EventValue {
    data.text()
        .ok()
        .and_then(|s| {
            use serde::Deserialize;
            let mut d = serde_json::Deserializer::from_str(&s);
            d.disable_recursion_limit();
            let value = serde_json::Value::deserialize(&mut d).ok()?;
            d.end().ok()?;
            Some(value)
        })
        .map(native)
        .unwrap_or(EventValue::Null)
}
fn optional(v: EventValue, k: &str) -> Result<EventValue, Failure> {
    if v.nullish() {
        Ok(EventValue::Undefined)
    } else {
        v.read(k)
    }
}
// ts: BinancePrivateWebsocket.onMessage (Spot)
pub fn on_spot_message(
    read_data: impl FnOnce() -> Result<EventValue, Failure>,
    b: &dyn SpotMessageBindings,
) -> Result<(), Failure> {
    let data = read_data()?;
    if b.disposed() {
        return Ok(());
    }
    b.call(MessageOp::Track, vec![])?;
    b.call(MessageOp::Diagnostic, vec![data.clone()])?;
    let json = parse_data(data);
    if !json.truthy() {
        return Ok(());
    }
    if json.read("id")?.same(&string(
        crate::spot_protocol::BINANCE_SPOT_USER_DATA_REQUEST_ID,
    )) {
        if !json.read("status")?.same(&number(200.0))
            || !matches!(
                optional(json.read("result")?, "subscriptionId")?,
                EventValue::Number(_)
            )
        {
            let msg = optional(json.read("error")?, "msg")?;
            let reason = if msg.truthy() {
                msg.text()?
            } else {
                let status = json.read("status")?;
                format!(
                    "subscription rejected with status {}",
                    if status.nullish() {
                        "unknown".into()
                    } else {
                        status.text()?
                    }
                )
            };
            b.call(
                MessageOp::Emit,
                vec![string(format!(
                    "Binance Spot user-data subscription failed: {reason}"
                ))],
            )?;
            b.call(MessageOp::ForceClose, vec![])?;
            return Ok(());
        }
        b.call(MessageOp::Ping, vec![])?;
        return Ok(());
    }
    let wrapped = json.read("event")?;
    let event = if wrapped.truthy() {
        wrapped
    } else {
        json.clone()
    };
    if event.read("e")?.same(&string("outboundAccountPosition")) {
        b.call(MessageOp::Stats, vec![string("position")])?;
        b.call(MessageOp::Account, vec![array(vec![event.clone()])])?;
    }
    if event.read("e")?.same(&string("executionReport")) {
        b.call(MessageOp::Stats, vec![string("order")])?;
        b.call(MessageOp::Order, vec![array(vec![event])])?;
    }
    if json.read("id")?.same(&number(
        crate::spot_protocol::BINANCE_SPOT_HEARTBEAT_REQUEST_ID,
    )) {
        if json.read("status")?.same(&number(200.0))
            && optional(json.read("result")?, "userDataStream")?.same(&EventValue::Bool(true))
        {
            b.call(MessageOp::Pong, vec![])?;
        } else {
            b.call(
                MessageOp::Emit,
                vec![string(
                    "Binance Spot user-data subscription is no longer active; reconnecting",
                )],
            )?;
            b.call(MessageOp::Reconnect, vec![])?;
        }
    }
    Ok(())
}
pub trait ModeBindings {
    fn markets(&self) -> Result<EventValue, Failure>;
    fn leverage(&self) -> Result<EventValue, Failure>;
    fn positions(&self) -> Result<EventValue, Failure>;
    fn id(
        &self,
        symbol: EventValue,
        side: EventValue,
        mode: PositionMode,
    ) -> Result<EventValue, Failure>;
    fn setting(&self, hedged: bool) -> Result<(), Failure>;
    fn update(&self, value: EventValue) -> Result<(), Failure>;
}
fn dense(v: &EventValue, expr: &str) -> Result<usize, Failure> {
    if let EventValue::Array(a) = v {
        Ok(a.borrow().len())
    } else if v.nullish() {
        Err(type_error(format!(
            "Cannot read properties of {} (reading 'map')",
            v.text()?
        )))
    } else {
        Err(type_error(format!("{expr}.map is not a function")))
    }
}
// ts: BinanceExchange#applyConfirmedPositionMode
pub fn apply_confirmed_mode(
    hedged: bool,
    symbol: Option<String>,
    b: &dyn ModeBindings,
) -> Result<(), Failure> {
    let mode = if hedged {
        PositionMode::Hedged
    } else {
        PositionMode::OneWay
    };
    let symbols = if let Some(s) = symbol.filter(|s| !s.is_empty()) {
        vec![string(s)]
    } else {
        let markets = b.markets()?;
        let n = dense(&markets, "this.store.markets")?;
        let mut out = vec![];
        for i in 0..n {
            out.push(markets.get(&i.to_string())?.read("symbol")?);
        }
        out
    };
    let fetched = EventValue::record_object(b.leverage()?.spread());
    for symbol in &symbols {
        let key = symbol.text()?;
        let old = fetched.get(&key)?;
        let mut fields = old.spread();
        for field in ["buyLeverage", "sellLeverage"] {
            let v = optional(old.clone(), field)?;
            fields.insert(field.into(), if v.nullish() { number(0.0) } else { v });
        }
        fields.insert(
            "positionMode".into(),
            EventValue::enum_value(PrivateEnum::PositionMode(mode)),
        );
        fetched.assign(&key, EventValue::record_object(fields));
    }
    let rows = b.positions()?;
    let n = dense(&rows, "this.store.positions")?;
    let mut positions = vec![];
    for i in 0..n {
        let p = rows.get(&i.to_string())?;
        let s = p.read("symbol")?;
        if !symbols.iter().any(|v| v.same(&s)) || p.read("contracts")?.number()? != 0.0 {
            positions.push(p);
            continue;
        }
        let mut fields = p.spread();
        fields.insert("id".into(), b.id(p.read("symbol")?, p.read("side")?, mode)?);
        fields.insert(
            "positionMode".into(),
            EventValue::enum_value(PrivateEnum::PositionMode(mode)),
        );
        positions.push(EventValue::record_object(fields));
    }
    b.setting(hedged)?;
    b.update(EventValue::record_object(
        [
            ("fetchedLeverage".into(), fetched),
            ("positions".into(), array(positions)),
        ]
        .into_iter()
        .collect::<JsObjectMap<_>>(),
    ))
}

fn protect_big_integer_tokens(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut output = String::with_capacity(text.len());
    let mut index = 0;
    while index < bytes.len() {
        if matches!(bytes[index], b':' | b',' | b'[' | b']') {
            let tail = &text[index + 1..];
            let number = index + 1 + tail.len() - safe_cex_num::js_trim_start(tail).len();
            let digits = number + usize::from(bytes.get(number) == Some(&b'-'));
            let mut end = digits;
            while bytes.get(end).is_some_and(u8::is_ascii_digit) {
                end += 1;
            }
            if end - digits >= 16
                && !bytes
                    .get(end)
                    .is_some_and(|byte| matches!(byte, b'0'..=b'9' | b'.' | b'"' | b'e' | b'E'))
            {
                output.push_str(&text[index..number]);
                output.push('"');
                output.push_str(&text[number..end]);
                output.push('"');
                index = end;
                continue;
            }
        }
        let ch = text[index..].chars().next().expect("character boundary");
        output.push(ch);
        index += ch.len_utf8();
    }
    output
}

pub(crate) fn parse_big_data(data: EventValue) -> EventValue {
    match data {
        EventValue::String(text) => match String::from_utf16(&text) {
            Ok(value) => parse_data(string(protect_big_integer_tokens(&value))),
            Err(_) => EventValue::Null,
        },
        data => data,
    }
}
