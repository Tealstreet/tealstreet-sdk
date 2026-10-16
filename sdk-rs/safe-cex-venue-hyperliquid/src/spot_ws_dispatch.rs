//! Spot frame routing and private socket construction over mapped leaf bodies.
use crate::{
    spot_json::JsString, ws_emission::WsEmissionHost, ws_ping_lifecycle::WsPingHost,
    ws_transport_leaves::WsTransportHost,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivateMethod {
    Connect,
    Open,
    Subscribe,
    Ping,
    System,
    Coins,
    Message,
    Balances,
    Fills,
    Fundings,
    Ledger,
}
pub const PRIVATE_METHODS: &[(&str, PrivateMethod, u8)] = &[
    ("connectAndSubscribe", PrivateMethod::Connect, 0),
    ("onOpen", PrivateMethod::Open, 0),
    ("subscribe", PrivateMethod::Subscribe, 0),
    ("ping", PrivateMethod::Ping, 0),
    ("subscribeSystemTopics", PrivateMethod::System, 0),
    ("subscribeCoins", PrivateMethod::Coins, 0),
    ("onMessage", PrivateMethod::Message, 1),
    ("handleSpotClearinghouseState", PrivateMethod::Balances, 1),
    ("handleFills", PrivateMethod::Fills, 1),
    ("handleFundings", PrivateMethod::Fundings, 1),
    (
        "handleUserNonFundingLedgerUpdates",
        PrivateMethod::Ledger,
        1,
    ),
];
pub trait SpotWsHost: WsEmissionHost + WsTransportHost {
    /// Native `{data}` parameter binding, evaluated before guards and catches.
    fn event_data(&mut self, event: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn is_nullish(&self, value: &Self::Value) -> bool;
    fn is_array(&mut self, value: &Self::Value) -> Result<bool, Self::Error>;
    fn equal(&self, left: &Self::Value, right: &Self::Value) -> bool;
    fn number(&self, value: f64) -> Self::Value;
    fn less(&mut self, left: &Self::Value, right: &Self::Value) -> Result<bool, Self::Error>;
    fn greater(&mut self, left: &Self::Value, right: &Self::Value) -> Result<bool, Self::Error>;
    fn index(&mut self, value: &Self::Value, key: &Self::Value)
    -> Result<Self::Value, Self::Error>;
    fn array(&mut self) -> Result<Self::Value, Self::Error>;
    fn parse_float(&mut self, value: &Self::Value) -> Result<f64, Self::Error>;
    fn template_string(&mut self, value: &Self::Value) -> Result<JsString, Self::Error>;
    /// Bind the existing imported jsonParse function, including its internal catch.
    fn json_parse(&mut self, data: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn order_side(&mut self, buy: bool) -> Result<Self::Value, Self::Error>;
    fn for_in(&mut self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn for_in_next(&mut self, iterator: &Self::Value) -> Result<Option<Self::Value>, Self::Error>;
    /// Native named derived class (length 1), linked to BaseWebSocket's ctor/proto;
    /// its prototype owns only the nonenumerable constructor property.
    fn private_class(&mut self) -> Result<(Self::Value, Self::Value), Self::Error>;
    /// Construct the imported base with original parent/newTarget handles.
    /// Honor newTarget.prototype resolution, abrupt completion and native fallback.
    fn construct_base(
        &mut self,
        parent: &Self::Value,
        new_target: &Self::Value,
        prototype: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    /// Own lexical arrow; callbacks execute run_private_callback with captured this.
    fn arrow(
        &mut self,
        capture: &Self::Value,
        name: &str,
        length: u8,
        body: PrivateMethod,
    ) -> Result<Self::Value, Self::Error>;
}
pub struct PrivateSocketClass<V> {
    pub constructor: V,
    pub prototype: V,
}
pub struct HyperliquidSpotPrivateWebSocket<V> {
    pub native: V,
}
fn text(s: &str) -> JsString {
    JsString(s.encode_utf16().collect())
}
fn lit<H: WsPingHost>(h: &H, s: &str) -> H::Value {
    h.literal(&text(s))
}
fn optional<H: SpotWsHost>(h: &mut H, v: &H::Value, key: &str) -> Result<H::Value, H::Error> {
    if h.is_nullish(v) {
        Ok(h.undefined())
    } else {
        h.get(v, key)
    }
}
fn path<H: SpotWsHost>(h: &mut H, v: &H::Value, keys: &[&str]) -> Result<H::Value, H::Error> {
    let mut v = v.clone();
    for key in keys {
        v = optional(h, &v, key)?;
    }
    Ok(v)
}
fn method<H: WsPingHost>(
    h: &mut H,
    v: &H::Value,
    key: &str,
    args: impl FnOnce(&mut H) -> Result<Vec<H::Value>, H::Error>,
) -> Result<H::Value, H::Error> {
    let f = h.get(v, key)?;
    let args = args(h)?;
    h.call(&f, v, &args)
}
fn fallback_object<H: SpotWsHost>(h: &mut H, v: H::Value) -> Result<H::Value, H::Error> {
    if h.truthy(&v) { Ok(v) } else { h.object() }
}
fn fallback_array<H: SpotWsHost>(h: &mut H, v: H::Value) -> Result<H::Value, H::Error> {
    if h.truthy(&v) { Ok(v) } else { h.array() }
}
fn channel_is<H: SpotWsHost>(h: &H, v: &H::Value, name: &str) -> bool {
    h.equal(v, &lit(h, name))
}
fn keyed<H: SpotWsHost>(h: &mut H, prefix: &str, value: &H::Value) -> Result<H::Value, H::Error> {
    let mut t = text(prefix).0;
    t.extend(h.template_string(value)?.0);
    Ok(h.literal(&JsString(t)))
}
pub fn private_socket_class<H: SpotWsHost>(
    h: &mut H,
) -> Result<PrivateSocketClass<H::Value>, H::Error> {
    let (constructor, prototype) = h.private_class()?;
    Ok(PrivateSocketClass {
        constructor,
        prototype,
    })
}
pub fn new_private_socket<H: SpotWsHost>(
    h: &mut H,
    class: &PrivateSocketClass<H::Value>,
    parent: &H::Value,
    new_target: Option<&H::Value>,
) -> Result<HyperliquidSpotPrivateWebSocket<H::Value>, H::Error> {
    let target = new_target.unwrap_or(&class.constructor);
    let native = h.construct_base(parent, target, &class.prototype)?;
    for (name, body, length) in PRIVATE_METHODS {
        let arrow = h.arrow(&native, name, *length, *body)?;
        h.define_data(&native, name, &arrow)?;
    }
    Ok(HyperliquidSpotPrivateWebSocket { native })
}
pub fn handle_spot_clearinghouse_state<H: SpotWsHost>(
    h: &mut H,
    socket: &H::Value,
    data: &H::Value,
) -> Result<(), H::Error> {
    let parent = h.get(socket, "parent")?;
    method(h, &parent, "applySpotBalances", |h| {
        let balances = optional(h, data, "balances")?;
        Ok(vec![fallback_array(h, balances)?])
    })?;
    Ok(())
}
pub fn handle_fills<H: SpotWsHost>(
    h: &mut H,
    socket: &H::Value,
    data: &H::Value,
) -> Result<(), H::Error> {
    let snapshot = h.get(data, "isSnapshot")?;
    if h.truthy(&snapshot) {
        return Ok(());
    }
    let fills = h.get(data, "fills")?;
    let fills = fallback_array(h, fills)?;
    let parent = h.get(socket, "parent")?;
    let store = h.get(&parent, "store")?;
    let markets = h.get(&store, "marketsById")?;
    let mut i = 0.;
    loop {
        let length = h.get(&fills, "length")?;
        if !h.less(&h.number(i), &length)? {
            break;
        }
        let fill = h.index(&fills, &h.number(i))?;
        let parent = h.get(socket, "parent")?;
        let emitter = h.get(&parent, "emitter")?;
        method(h, &emitter, "emit", |h| {
            let event = lit(h, "fill");
            let record = h.object()?;
            let tid = h.get(&fill, "tid")?;
            let id = h.template_string(&tid)?;
            h.define_data(&record, "id", &h.literal(&id))?;
            let side = h.get(&fill, "side")?;
            let side = h.order_side(channel_is(h, &side, "B"))?;
            h.define_data(&record, "side", &side)?;
            let coin = h.get(&fill, "coin")?;
            let market = h.index(&markets, &coin)?;
            let symbol = optional(h, &market, "symbol")?;
            let symbol = if h.truthy(&symbol) {
                symbol
            } else {
                h.get(&fill, "coin")?
            };
            h.define_data(&record, "symbol", &symbol)?;
            let px = h.get(&fill, "px")?;
            let price = h.parse_float(&px)?;
            h.define_data(&record, "price", &h.number(price))?;
            let sz = h.get(&fill, "sz")?;
            let amount = h.parse_float(&sz)?;
            h.define_data(&record, "amount", &h.number(amount))?;
            Ok(vec![event, record])
        })?;
        i += 1.;
    }
    let length = h.get(&fills, "length")?;
    if h.greater(&length, &h.number(0.))? {
        let parent = h.get(socket, "parent")?;
        method(h, &parent, "requestSpotBalanceRefresh", |h| {
            Ok(vec![WsTransportHost::boolean(h, true)])
        })?;
    }
    Ok(())
}
pub fn handle_fundings<H: SpotWsHost>(_: &mut H, _: &H::Value, _: &H::Value) {}
pub fn handle_user_non_funding_ledger_updates<H: SpotWsHost>(
    h: &mut H,
    socket: &H::Value,
    _: &H::Value,
) -> Result<(), H::Error> {
    let parent = h.get(socket, "parent")?;
    method(h, &parent, "requestSpotBalanceRefresh", |_| Ok(vec![]))?;
    Ok(())
}
pub fn handle_all_mids<H: SpotWsHost>(
    h: &mut H,
    socket: &H::Value,
    data: &H::Value,
) -> Result<(), H::Error> {
    let parent = h.get(socket, "parent")?;
    let store = h.get(&parent, "store")?;
    let tickers = h.get(&store, "tickersById")?;
    let buffer = h.get(socket, "_midsUpdateBuf")?;
    let iterator = h.for_in(data)?;
    while let Some(prefix) = h.for_in_next(&iterator)? {
        let ticker = h.index(&tickers, &prefix)?;
        if !h.truthy(&ticker) {
            continue;
        }
        let raw = h.index(data, &prefix)?;
        let last = h.parse_float(&raw)?;
        let last = h.number(last);
        h.set(&buffer, "last", &last)?;
        h.set(&buffer, "midPrice", &last)?;
        let parent = h.get(socket, "parent")?;
        let store = h.get(&parent, "store")?;
        method(h, &store, "updateTicker", |_| {
            Ok(vec![ticker, buffer.clone()])
        })?;
    }
    Ok(())
}
fn active_asset_data<H: SpotWsHost>(
    h: &mut H,
    socket: &H::Value,
    json: &H::Value,
) -> Result<(), H::Error> {
    let coin = path(h, json, &["data", "coin"])?;
    if !h.truthy(&coin) {
        return Ok(());
    }
    let parent = h.get(socket, "parent")?;
    let map = h.get(&parent, "activeAssetData")?;
    method(h, &map, "set", |h| {
        let coin = path(h, json, &["data", "coin"])?;
        let data = path(h, json, &["data"])?;
        Ok(vec![coin, fallback_object(h, data)?])
    })?;
    let store = h.get(socket, "store")?;
    let markets = h.get(&store, "marketsById")?;
    let coin = path(h, json, &["data", "coin"])?;
    let market = h.index(&markets, &coin)?;
    if h.truthy(&market) {
        let store = h.get(socket, "store")?;
        let positions = h.get(&store, "positions")?;
        let parent = h.get(socket, "parent")?;
        let config = method(h, &parent, "_getSymbolConfigurationFromAssetData", |h| {
            Ok(vec![path(h, json, &["data", "coin"])?])
        })?;
        let mut i = 0.;
        loop {
            let length = h.get(&positions, "length")?;
            if !h.less(&h.number(i), &length)? {
                break;
            }
            let position = h.index(&positions, &h.number(i))?;
            let symbol = h.get(&position, "symbol")?;
            let market_symbol = h.get(&market, "symbol")?;
            if !h.equal(&symbol, &market_symbol) {
                i += 1.;
                continue;
            }
            let contracts = h.get(&position, "contracts")?;
            let open = h.greater(&contracts, &h.number(0.))?;
            let parent = h.get(socket, "parent")?;
            let store = h.get(&parent, "store")?;
            method(h, &store, "updatePosition", |h| {
                let changes = h.object()?;
                let mode = h.get(&config, "positionMode")?;
                h.define_data(&changes, "positionMode", &mode)?;
                let margin = h.get(if open { &position } else { &config }, "marginType")?;
                h.define_data(&changes, "marginType", &margin)?;
                let leverage = h.get(if open { &position } else { &config }, "leverage")?;
                h.define_data(&changes, "leverage", &leverage)?;
                Ok(vec![position, changes])
            })?;
            i += 1.;
        }
    }
    Ok(())
}
pub fn private_on_message<H: SpotWsHost>(
    h: &mut H,
    socket: &H::Value,
    event: &H::Value,
) -> Result<(), H::Error> {
    let data = h.event_data(event)?;
    let disposed = h.get(socket, "isDisposed")?;
    if !h.truthy(&disposed) {
        method(h, socket, "trackLastReceived", |_| Ok(vec![]))?;
        let _caught: Result<(), H::Error> = (|| {
            let json = h.json_parse(&data)?;
            method(h, socket, "recordMessageHandlerStats", |h| {
                Ok(vec![optional(h, &json, "channel")?])
            })?;
            method(h, socket, "recordMessageHandlerStats", |h| {
                Ok(vec![lit(h, "pong")])
            })?;
            let channel = optional(h, &json, "channel")?;
            if channel_is(h, &channel, "activeAssetData") {
                active_asset_data(h, socket, &json)?;
            } else if channel_is(h, &channel, "spotClearinghouseState") {
                method(h, socket, "handleSpotClearinghouseState", |h| {
                    let v = path(h, &json, &["data"])?;
                    Ok(vec![fallback_object(h, v)?])
                })?;
            } else if channel_is(h, &channel, "spotState") {
                method(h, socket, "handleSpotClearinghouseState", |h| {
                    let first = path(h, &json, &["data", "spotState"])?;
                    let v = if h.truthy(&first) {
                        first
                    } else {
                        path(h, &json, &["data"])?
                    };
                    Ok(vec![fallback_object(h, v)?])
                })?;
            } else if channel_is(h, &channel, "pong") {
                method(h, socket, "handlePongEvent", |_| Ok(vec![]))?;
            } else {
                for (name, handler) in [
                    ("userFills", "handleFills"),
                    ("userFundings", "handleFundings"),
                    (
                        "userNonFundingLedgerUpdates",
                        "handleUserNonFundingLedgerUpdates",
                    ),
                ] {
                    if channel_is(h, &channel, name) {
                        method(h, socket, handler, |h| {
                            let v = path(h, &json, &["data"])?;
                            Ok(vec![fallback_object(h, v)?])
                        })?;
                        break;
                    }
                }
            }
            Ok(())
        })();
    }
    Ok(())
}
pub fn public_on_message<H: SpotWsHost>(
    h: &mut H,
    socket: &H::Value,
    event: &H::Value,
) -> Result<(), H::Error> {
    let data = h.event_data(event)?;
    let disposed = h.get(socket, "isDisposed")?;
    if !h.truthy(&disposed) {
        method(h, socket, "trackLastReceived", |_| Ok(vec![]))?;
        let _caught: Result<(), H::Error> = (|| {
            let json = h.json_parse(&data)?;
            let channel = optional(h, &json, "channel")?;
            let allowed = if !channel_is(h, &channel, "l2Book") {
                let channel = optional(h, &json, "channel")?;
                if !channel_is(h, &channel, "candle") {
                    let channel = optional(h, &json, "channel")?;
                    !channel_is(h, &channel, "error")
                } else {
                    false
                }
            } else {
                false
            };
            if allowed {
                method(h, socket, "recordMessageHandlerStats", |h| {
                    Ok(vec![optional(h, &json, "channel")?])
                })?;
            }
            method(h, socket, "recordMessageHandlerStats", |h| {
                Ok(vec![lit(h, "pong")])
            })?;
            let channel = optional(h, &json, "channel")?;
            if channel_is(h, &channel, "pong") {
                method(h, socket, "handlePongEvent", |_| Ok(vec![]))?;
            } else if channel_is(h, &channel, "allMids") {
                method(h, socket, "handleAllMids", |h| {
                    let v = path(h, &json, &["data", "mids"])?;
                    Ok(vec![fallback_object(h, v)?])
                })?;
            } else if channel_is(h, &channel, "candle") {
                let candle_data = path(h, &json, &["data"])?;
                let sample = if h.is_array(&candle_data)? {
                    h.get(&candle_data, "0")?
                } else {
                    candle_data.clone()
                };
                let symbol = optional(h, &sample, "s")?;
                if !h.truthy(&symbol) {
                    return Ok(());
                }
                let interval = optional(h, &sample, "i")?;
                if !h.truthy(&interval) {
                    return Ok(());
                }
                let handlers = h.get(socket, "messageHandlers")?;
                let symbol = h.get(&sample, "s")?;
                let mut units = text("candle.").0;
                units.extend(h.template_string(&symbol)?.0);
                units.push(46);
                let interval = h.get(&sample, "i")?;
                units.extend(h.template_string(&interval)?.0);
                let topic = h.literal(&JsString(units));
                let handlers = h.index(&handlers, &topic)?;
                if h.truthy(&handlers) {
                    let payload = fallback_array(h, candle_data)?;
                    handlers_loop(h, &handlers, &payload, false)?;
                }
            } else if channel_is(h, &channel, "l2Book") {
                let coin = path(h, &json, &["data", "coin"])?;
                if !h.truthy(&coin) {
                    return Ok(());
                }
                let handlers = h.get(socket, "messageHandlers")?;
                let topic = keyed(h, "l2Book.", &coin)?;
                let handlers = h.index(&handlers, &topic)?;
                if h.truthy(&handlers) {
                    let payload = path(h, &json, &["data"])?;
                    let payload = fallback_array(h, payload)?;
                    handlers_loop(h, &handlers, &payload, true)?;
                }
            }
            Ok(())
        })();
    }
    Ok(())
}
fn handlers_loop<H: SpotWsHost>(
    h: &mut H,
    handlers: &H::Value,
    payload: &H::Value,
    catch_each: bool,
) -> Result<(), H::Error> {
    let mut i = 0.;
    loop {
        let length = h.get(handlers, "length")?;
        if !h.less(&h.number(i), &length)? {
            break;
        }
        let invoke = (|| {
            let f = h.index(handlers, &h.number(i))?;
            h.call(&f, handlers, &[payload.clone()])?;
            Ok(())
        })();
        if !catch_each {
            invoke?;
        }
        i += 1.;
    }
    Ok(())
}
pub fn run_private_callback<H: SpotWsHost>(
    h: &mut H,
    capture: &H::Value,
    body: PrivateMethod,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    let first = args.first().cloned().unwrap_or_else(|| h.undefined());
    match body {
        PrivateMethod::Connect => {
            crate::ws_transport_leaves::spot_private_connect_and_subscribe(h, capture)?
        }
        PrivateMethod::Open => crate::ws_transport_leaves::spot_private_on_open(h, capture)?,
        PrivateMethod::Subscribe => crate::ws_transport_leaves::spot_private_subscribe(h, capture)?,
        PrivateMethod::Ping => crate::ws_transport_leaves::spot_private_ping(h, capture)?,
        PrivateMethod::System => {
            crate::ws_emission::spot_private_subscribe_system_topics(h, capture)?
        }
        PrivateMethod::Coins => crate::ws_emission::spot_private_subscribe_coins(h, capture)?,
        PrivateMethod::Message => private_on_message(h, capture, &first)?,
        PrivateMethod::Balances => handle_spot_clearinghouse_state(h, capture, &first)?,
        PrivateMethod::Fills => handle_fills(h, capture, &first)?,
        PrivateMethod::Fundings => handle_fundings(h, capture, &first),
        PrivateMethod::Ledger => handle_user_non_funding_ledger_updates(h, capture, &first)?,
    }
    Ok(h.undefined())
}
