//! Core public frame routing. Source parameter binding and the outside-catch
//! receive marker precede a swallowed processing region; l2Book/bbo isolate
//! callback failures while candles stop the whole processing region.
use crate::{spot_json::JsString, ws_ping_lifecycle::WsPingHost};
pub trait CorePublicMessageHost: WsPingHost {
    fn event_data(&mut self, event: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn is_nullish(&self, value: &Self::Value) -> bool;
    fn is_array(&mut self, value: &Self::Value) -> Result<bool, Self::Error>;
    fn equal(&self, left: &Self::Value, right: &Self::Value) -> bool;
    fn number(&self, value: f64) -> Self::Value;
    fn less(&mut self, left: &Self::Value, right: &Self::Value) -> Result<bool, Self::Error>;
    fn subtract(
        &mut self,
        left: &Self::Value,
        right: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn divide(
        &mut self,
        left: &Self::Value,
        right: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn index(&mut self, value: &Self::Value, key: &Self::Value)
    -> Result<Self::Value, Self::Error>;
    fn array(&mut self) -> Result<Self::Value, Self::Error>;
    fn template_string(&mut self, value: &Self::Value) -> Result<JsString, Self::Error>;
    /// Bind the imported jsonParse, including its own internal catch.
    fn json_parse(&mut self, data: &Self::Value) -> Result<Self::Value, Self::Error>;
}
fn text(s: &str) -> JsString {
    JsString(s.encode_utf16().collect())
}
fn lit<H: WsPingHost>(h: &H, s: &str) -> H::Value {
    h.literal(&text(s))
}
fn optional<H: CorePublicMessageHost>(
    h: &mut H,
    v: &H::Value,
    key: &str,
) -> Result<H::Value, H::Error> {
    if h.is_nullish(v) {
        Ok(h.undefined())
    } else {
        h.get(v, key)
    }
}
fn path<H: CorePublicMessageHost>(
    h: &mut H,
    v: &H::Value,
    keys: &[&str],
) -> Result<H::Value, H::Error> {
    let mut value = v.clone();
    for key in keys {
        value = optional(h, &value, key)?
    }
    Ok(value)
}
fn method<H: CorePublicMessageHost>(
    h: &mut H,
    receiver: &H::Value,
    key: &str,
    args: impl FnOnce(&mut H) -> Result<Vec<H::Value>, H::Error>,
) -> Result<H::Value, H::Error> {
    let function = h.get(receiver, key)?;
    let args = args(h)?;
    h.call(&function, receiver, &args)
}
fn is<H: CorePublicMessageHost>(h: &H, v: &H::Value, s: &str) -> bool {
    h.equal(v, &lit(h, s))
}
fn topic<H: CorePublicMessageHost>(
    h: &mut H,
    prefix: &str,
    value: &H::Value,
) -> Result<H::Value, H::Error> {
    let mut key = text(prefix).0;
    key.extend(h.template_string(value)?.0);
    Ok(h.literal(&JsString(key)))
}
fn handlers_loop<H: CorePublicMessageHost>(
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
            invoke?
        }
        i += 1.;
    }
    Ok(())
}
fn log_json<H: CorePublicMessageHost>(
    h: &mut H,
    socket: &H::Value,
    json: &H::Value,
    prefix: &str,
    severity: &str,
) -> Result<(), H::Error> {
    let parent = h.get(socket, "parent")?;
    let log = h.get(&parent, "log")?;
    let json_global = h.global("JSON")?;
    let stringify = h.get(&json_global, "stringify")?;
    let encoded = h.call(&stringify, &json_global, &[json.clone()])?;
    let message = topic(h, prefix, &encoded)?;
    let levels = h.global("LogSeverity")?;
    let level = h.get(&levels, severity)?;
    h.call(&log, &parent, &[message, level])?;
    Ok(())
}
pub fn core_public_on_message<H: CorePublicMessageHost>(
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
            method(h, socket, "emitReceiveDiagnostics", |_| {
                Ok(vec![data.clone(), json.clone()])
            })?;
            let first = optional(h, &json, "channel")?;
            let subscription = if is(h, &first, "subscriptionResponse") {
                true
            } else {
                let second = optional(h, &json, "channel")?;
                is(h, &second, "SubscriptionResponse")
            };
            if subscription {
                log_json(h, socket, &json, "Subscription response: ", "Debug")?;
            }
            let mut record = true;
            for excluded in [
                "l2Book",
                "bbo",
                "candle",
                "error",
                "subscriptionResponse",
                "SubscriptionResponse",
            ] {
                let channel = optional(h, &json, "channel")?;
                if is(h, &channel, excluded) {
                    record = false;
                    break;
                }
            }
            if record {
                method(h, socket, "recordMessageHandlerStats", |h| {
                    Ok(vec![optional(h, &json, "channel")?])
                })?;
            }
            method(h, socket, "recordMessageHandlerStats", |h| {
                Ok(vec![lit(h, "pong")])
            })?;
            let channel = optional(h, &json, "channel")?;
            if is(h, &channel, "error") {
                log_json(h, socket, &json, "WebSocket error: ", "Error")?;
            } else if is(h, &channel, "pong") {
                let ping_at = h.get(socket, "pingAt")?;
                if h.truthy(&ping_at) {
                    let performance = h.global("performance")?;
                    let now = method(h, &performance, "now", |_| Ok(vec![]))?;
                    let ping_at = h.get(socket, "pingAt")?;
                    let diff = h.subtract(&now, &ping_at)?;
                    let store = h.get(socket, "store")?;
                    let update = h.get(&store, "update")?;
                    let payload = h.object()?;
                    let math = h.global("Math")?;
                    let round = h.get(&math, "round")?;
                    let half = h.divide(&diff, &h.number(2.))?;
                    let latency = h.call(&round, &math, &[half])?;
                    h.define_data(&payload, "latency", &latency)?;
                    h.call(&update, &store, &[payload])?;
                }
            } else if is(h, &channel, "allMids") {
                method(h, socket, "handleAllMids", |h| {
                    let mids = path(h, &json, &["data", "mids"])?;
                    Ok(vec![if h.truthy(&mids) { mids } else { h.object()? }])
                })?;
            } else if is(h, &channel, "fastAssetCtxs") {
                method(h, socket, "handleFastAssetCtxs", |h| {
                    Ok(vec![optional(h, &json, "data")?])
                })?;
            } else if is(h, &channel, "candle") {
                let candle_data = optional(h, &json, "data")?;
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
                let mut key = text("candle.").0;
                key.extend(h.template_string(&symbol)?.0);
                key.push(46);
                let interval = h.get(&sample, "i")?;
                key.extend(h.template_string(&interval)?.0);
                let key = h.literal(&JsString(key));
                let handlers = h.index(&handlers, &key)?;
                if h.truthy(&handlers) {
                    let payload = if h.truthy(&candle_data) {
                        candle_data
                    } else {
                        h.array()?
                    };
                    handlers_loop(h, &handlers, &payload, false)?;
                }
            } else if is(h, &channel, "l2Book") || is(h, &channel, "bbo") {
                let coin = path(h, &json, &["data", "coin"])?;
                if !h.truthy(&coin) {
                    return Ok(());
                }
                let bbo = is(h, &channel, "bbo");
                // Source bbo computes its local topic before reading the handler dictionary;
                // l2Book resolves that dictionary before evaluating its template key.
                let (handlers, key) = if bbo {
                    let key = topic(h, "bbo.", &coin)?;
                    let handlers = h.get(socket, "messageHandlers")?;
                    (handlers, key)
                } else {
                    let handlers = h.get(socket, "messageHandlers")?;
                    let key = topic(h, "l2Book.", &coin)?;
                    (handlers, key)
                };
                let handlers = h.index(&handlers, &key)?;
                if h.truthy(&handlers) {
                    if bbo {
                        method(h, socket, "recordMessageHandlerStats", |_| {
                            Ok(vec![key.clone()])
                        })?;
                    }
                    let value = optional(h, &json, "data")?;
                    let payload = if bbo || h.truthy(&value) {
                        value
                    } else {
                        h.array()?
                    };
                    handlers_loop(h, &handlers, &payload, true)?;
                }
            }
            Ok(())
        })();
    }
    Ok(())
}
