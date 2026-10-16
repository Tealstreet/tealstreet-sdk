//! Core private frame routing, timestamp deduplication and public ticker updates.
//! Property access, JS primitives and virtual callees are venue-local bindings;
//! routing, loops, catch boundaries and payload construction live in Rust.
use crate::{spot_json::JsString, ws_ping_lifecycle::WsPingHost};
#[derive(Clone, Copy, Debug)]
pub enum DispatchOperator {
    Less,
    LessEqual,
    Greater,
    Subtract,
    Divide,
}
pub trait PrivateMessageHost: WsPingHost {
    fn typeof_value<'a>(&self, value: &'a Self::Value) -> &'a str;
    fn has_property(&mut self, name: &str, object: &Self::Value) -> Result<bool, Self::Error>;
    fn valid_fill_predicate(&mut self) -> Result<Self::Value, Self::Error>;
    fn for_in(&mut self, object: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn for_in_next(&mut self, iterator: &Self::Value) -> Result<Option<Self::Value>, Self::Error>;
    fn construct(
        &mut self,
        function: &Self::Value,
        args: &[Self::Value],
    ) -> Result<Self::Value, Self::Error>;
    fn regex(&mut self, pattern: &str) -> Result<Self::Value, Self::Error>;
    fn number(&self, value: f64) -> Self::Value;
    fn boolean(&self, value: bool) -> Self::Value;
    fn is_nullish(&self, value: &Self::Value) -> bool;
    fn is_null(&self, value: &Self::Value) -> bool;
    fn strict_equal(&self, a: &Self::Value, b: &Self::Value) -> bool;
    fn index(
        &mut self,
        object: &Self::Value,
        key: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn array(&mut self) -> Result<Self::Value, Self::Error>;
    fn template_string(&mut self, value: &Self::Value) -> Result<JsString, Self::Error>;
    fn operator(
        &mut self,
        op: DispatchOperator,
        a: &Self::Value,
        b: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
}
fn lit<H: WsPingHost>(h: &H, s: &str) -> H::Value {
    h.literal(&JsString(s.encode_utf16().collect()))
}
fn optional<H: PrivateMessageHost>(h: &mut H, v: &H::Value, n: &str) -> Result<H::Value, H::Error> {
    if h.is_nullish(v) {
        Ok(h.undefined())
    } else {
        h.get(v, n)
    }
}
fn path<H: PrivateMessageHost>(
    h: &mut H,
    v: &H::Value,
    names: &[&str],
) -> Result<H::Value, H::Error> {
    let mut v = v.clone();
    for n in names {
        v = optional(h, &v, n)?;
    }
    Ok(v)
}
fn method<H: WsPingHost>(
    h: &mut H,
    v: &H::Value,
    n: &str,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    let f = h.get(v, n)?;
    h.call(&f, v, args)
}
fn default_object<H: PrivateMessageHost>(h: &mut H, v: H::Value) -> Result<H::Value, H::Error> {
    if h.truthy(&v) { Ok(v) } else { h.object() }
}
fn default_array<H: PrivateMessageHost>(h: &mut H, v: H::Value) -> Result<H::Value, H::Error> {
    if h.truthy(&v) { Ok(v) } else { h.array() }
}
pub fn core_is_stale_message<H: PrivateMessageHost>(
    h: &mut H,
    socket: &H::Value,
    json: &H::Value,
) -> Result<H::Value, H::Error> {
    let channel = h.get(json, "channel")?;
    let data = h.get(json, "data")?;
    if !h.truthy(&channel) || !h.truthy(&data) {
        return Ok(h.boolean(false));
    }
    let time = method(h, socket, "getMessageTimestamp", &[data.clone()])?;
    if !h.is_null(&time) {
        let key = method(
            h,
            socket,
            "getStaleCheckChannelKey",
            &[channel.clone(), data.clone()],
        )?;
        let state = h.get(socket, "timestampState")?;
        let map = h.get(&state, "lastTime")?;
        let last = method(h, &map, "get", &[key.clone()])?;
        let last = if h.truthy(&last) { last } else { h.number(0.0) };
        let stale = h.operator(DispatchOperator::LessEqual, &time, &last)?;
        if h.truthy(&stale) {
            return Ok(h.boolean(true));
        }
        let state = h.get(socket, "timestampState")?;
        let map = h.get(&state, "lastTime")?;
        method(h, &map, "set", &[key, time])?;
        return Ok(h.boolean(false));
    }
    if h.strict_equal(&channel, &lit(h, "activeAssetData")) {
        let coin = h.get(&data, "coin")?;
        if h.truthy(&coin) {
            let mut key = h.template_string(&channel)?.0;
            key.push(b':' as u16);
            let coin = h.get(&data, "coin")?;
            key.extend(h.template_string(&coin)?.0);
            let key = h.literal(&JsString(key));
            let mark = h.get(&data, "markPx")?;
            let mark = if h.truthy(&mark) { mark } else { lit(h, "") };
            // Template coercion happens before the leverage getter.
            let mut hash = h.template_string(&mark)?.0;
            hash.push(b':' as u16);
            let leverage = h.get(&data, "leverage")?;
            let value = optional(h, &leverage, "value")?;
            let value = if h.truthy(&value) { value } else { lit(h, "") };
            hash.extend(h.template_string(&value)?.0);
            let hash = h.literal(&JsString(hash));
            let state = h.get(socket, "timestampState")?;
            let map = h.get(&state, "lastDataHash")?;
            let last = method(h, &map, "get", &[key.clone()])?;
            if h.strict_equal(&hash, &last) {
                return Ok(h.boolean(true));
            }
            let state = h.get(socket, "timestampState")?;
            let map = h.get(&state, "lastDataHash")?;
            method(h, &map, "set", &[key, hash])?;
            return Ok(h.boolean(false));
        }
    }
    Ok(h.boolean(false))
}
fn update_active_asset<H: PrivateMessageHost>(
    h: &mut H,
    socket: &H::Value,
    json: &H::Value,
) -> Result<(), H::Error> {
    let guard = path(h, json, &["data", "coin"])?;
    if !h.truthy(&guard) {
        return Ok(());
    }
    let coin = path(h, json, &["data", "coin"])?;
    let parent = h.get(socket, "parent")?;
    let map = h.get(&parent, "activeAssetData")?;
    let setter = h.get(&map, "set")?;
    let set_coin = coin.clone();
    let data = optional(h, json, "data")?;
    let data = default_object(h, data)?;
    h.call(&setter, &map, &[set_coin, data])?;
    let store = h.get(socket, "store")?;
    let markets = h.get(&store, "marketsById")?;
    let key = coin.clone();
    let market = h.index(&markets, &key)?;
    if h.truthy(&market) {
        let store = h.get(socket, "store")?;
        let positions = h.get(&store, "positions")?;
        let parent = h.get(socket, "parent")?;
        let config_fn = h.get(&parent, "_getSymbolConfigurationFromAssetData")?;
        let key = coin;
        let config = h.call(&config_fn, &parent, &[key])?;
        let mut i = 0usize;
        loop {
            let length = h.get(&positions, "length")?;
            let index = h.number(i as f64);
            let less = h.operator(DispatchOperator::Less, &index, &length)?;
            if !h.truthy(&less) {
                break;
            }
            let position = h.index(&positions, &index)?;
            let symbol = h.get(&position, "symbol")?;
            let market_symbol = h.get(&market, "symbol")?;
            if !h.strict_equal(&symbol, &market_symbol) {
                i += 1;
                continue;
            }
            let contracts = h.get(&position, "contracts")?;
            let zero = h.number(0.0);
            let open = h.operator(DispatchOperator::Greater, &contracts, &zero)?;
            let open = h.truthy(&open);
            let parent = h.get(socket, "parent")?;
            let store = h.get(&parent, "store")?;
            let updater = h.get(&store, "updatePosition")?;
            let patch = h.object()?;
            let mode = h.get(&config, "positionMode")?;
            h.define_data(&patch, "positionMode", &mode)?;
            let margin = h.get(if open { &position } else { &config }, "marginType")?;
            h.define_data(&patch, "marginType", &margin)?;
            let leverage = h.get(if open { &position } else { &config }, "leverage")?;
            h.define_data(&patch, "leverage", &leverage)?;
            h.call(&updater, &store, &[position, patch])?;
            i += 1;
        }
    }
    Ok(())
}
pub fn core_process_message<H: PrivateMessageHost>(
    h: &mut H,
    socket: &H::Value,
    json: &H::Value,
) -> Result<H::Value, H::Error> {
    let channel = optional(h, json, "channel")?;
    if h.strict_equal(&channel, &lit(h, "activeAssetData")) {
        update_active_asset(h, socket, json)?;
    } else if h.strict_equal(&channel, &lit(h, "spotState"))
        || h.strict_equal(&channel, &lit(h, "spotClearinghouseState"))
    {
        let parent = h.get(socket, "parent")?;
        let f = h.get(&parent, "handleSpotClearinghouseStateUpdate")?;
        let mut data = path(h, json, &["data", "spotState"])?;
        if !h.truthy(&data) {
            data = optional(h, json, "data")?;
        }
        if !h.truthy(&data) {
            data = h.object()?;
            let balances = h.array()?;
            h.define_data(&data, "balances", &balances)?;
        }
        let source = lit(h, "websocket");
        h.call(&f, &parent, &[data, source])?;
    } else if h.strict_equal(&channel, &lit(h, "clearinghouseState")) {
        let data = optional(h, json, "data")?;
        if h.truthy(&data) {
            let data = h.get(json, "data")?;
            let mut state = h.get(&data, "clearinghouseState")?;
            if !h.truthy(&state) {
                state = h.get(json, "data")?;
            }
            let data = h.get(json, "data")?;
            let dex = h.get(&data, "dex")?;
            if h.truthy(&dex) {
                let f = h.get(socket, "handleHip3ClearinghouseState")?;
                let data = h.get(json, "data")?;
                let dex = h.get(&data, "dex")?;
                h.call(&f, socket, &[dex, state])?;
            } else {
                method(h, socket, "applyCoreClearinghouseState", &[state])?;
            }
        }
    } else {
        let target = [
            ("allMids", "handleHip3AllMids", 0),
            ("error", "handleServerError", 2),
            ("userFills", "handleFills", 0),
            ("userFundings", "handleFundings", 0),
            (
                "userNonFundingLedgerUpdates",
                "handleUserNonFundingLedgerUpdates",
                0,
            ),
            ("orderUpdates", "handleOrderUpdates", 1),
            ("userEvents", "handleUserEvents", 2),
        ]
        .into_iter()
        .find(|(c, _, _)| h.strict_equal(&channel, &lit(h, c)));
        if let Some((_, name, default)) = target {
            let f = h.get(socket, name)?;
            let data = optional(h, json, "data")?;
            let data = match default {
                0 => default_object(h, data)?,
                1 => default_array(h, data)?,
                _ => data,
            };
            h.call(&f, socket, &[data])?;
        }
    }
    Ok(h.undefined())
}
pub fn core_on_message<H: PrivateMessageHost>(
    h: &mut H,
    socket: &H::Value,
    event: &H::Value,
) -> Result<H::Value, H::Error> {
    let data = h.get(event, "data")?;
    let disposed = h.get(socket, "isDisposed")?;
    if !h.truthy(&disposed) {
        method(h, socket, "trackLastReceived", &[])?;
        // Only this body is swallowed; parameter binding and tracking propagate.
        let _caught = (|| -> Result<(), H::Error> {
            let f = h.global("jsonParse")?;
            let receiver = h.undefined();
            let json = h.call(&f, &receiver, &[data.clone()])?;
            method(
                h,
                socket,
                "emitReceiveDiagnostics",
                &[data.clone(), json.clone()],
            )?;
            let channel = optional(h, &json, "channel")?;
            if h.strict_equal(&channel, &lit(h, "pong")) {
                method(h, socket, "recordMessageHandlerStats", &[lit(h, "pong")])?;
                let ping = h.get(socket, "pingAt")?;
                if h.truthy(&ping) {
                    let performance = h.global("performance")?;
                    let now = method(h, &performance, "now", &[])?;
                    let ping = h.get(socket, "pingAt")?;
                    let diff = h.operator(DispatchOperator::Subtract, &now, &ping)?;
                    let store = h.get(socket, "store")?;
                    let updater = h.get(&store, "update")?;
                    let payload = h.object()?;
                    let math = h.global("Math")?;
                    let round = h.get(&math, "round")?;
                    let two = h.number(2.0);
                    let half = h.operator(DispatchOperator::Divide, &diff, &two)?;
                    let latency = h.call(&round, &math, &[half])?;
                    h.define_data(&payload, "latency", &latency)?;
                    h.call(&updater, &store, &[payload])?;
                }
                return Ok(());
            }
            let channel = optional(h, &json, "channel")?;
            if h.truthy(&channel) {
                let class = h.global("HyperliquidPrivateWebSocket")?;
                let skip = h.get(&class, "SKIP_TIMESTAMP_CHECK_CHANNELS")?;
                let f = h.get(&skip, "has")?;
                let channel = h.get(&json, "channel")?;
                let skipped = h.call(&f, &skip, &[channel])?;
                if !h.truthy(&skipped) {
                    let stale = method(h, socket, "isStaleMessage", &[json.clone()])?;
                    if h.truthy(&stale) {
                        return Ok(());
                    }
                }
            }
            let mut stats = true;
            for ignored in ["subscriptionResponse", "SubscriptionResponse", "subscribed"] {
                let channel = optional(h, &json, "channel")?;
                if h.strict_equal(&channel, &lit(h, ignored)) {
                    stats = false;
                    break;
                }
            }
            if stats {
                let f = h.get(socket, "recordMessageHandlerStats")?;
                let channel = optional(h, &json, "channel")?;
                h.call(&f, socket, &[channel])?;
            }
            method(h, socket, "processMessage", &[json])?;
            Ok(())
        })();
    }
    Ok(h.undefined())
}
pub fn core_handle_fundings<H: PrivateMessageHost>(
    h: &mut H,
    _socket: &H::Value,
    _data: &H::Value,
) -> Result<H::Value, H::Error> {
    Ok(h.undefined())
}
pub fn core_handle_ledger_updates<H: PrivateMessageHost>(
    h: &mut H,
    socket: &H::Value,
    _data: &H::Value,
) -> Result<H::Value, H::Error> {
    let parent = h.get(socket, "parent")?;
    let track = method(h, &parent, "shouldTrackSpotHoldings", &[])?;
    if !h.truthy(&track) {
        return Ok(h.undefined());
    }
    let parent = h.get(socket, "parent")?;
    method(h, &parent, "requestUnifiedSpotCollateralRefresh", &[])?;
    Ok(h.undefined())
}

fn free<H: PrivateMessageHost>(
    h: &mut H,
    n: &str,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    let f = h.global(n)?;
    let u = h.undefined();
    h.call(&f, &u, args)
}
fn is_array<H: PrivateMessageHost>(h: &mut H, value: &H::Value) -> Result<bool, H::Error> {
    let array = h.global("Array")?;
    let yes = method(h, &array, "isArray", &[value.clone()])?;
    Ok(h.truthy(&yes))
}
pub fn run_valid_fill_predicate<H: PrivateMessageHost>(
    h: &mut H,
    entry: &H::Value,
) -> Result<H::Value, H::Error> {
    if !h.truthy(entry) {
        return Ok(entry.clone());
    }
    if h.typeof_value(entry) != "object" {
        return Ok(h.boolean(false));
    }
    for name in ["tid", "coin", "px", "sz"] {
        if !h.has_property(name, entry)? {
            return Ok(h.boolean(false));
        }
    }
    // JS && returns the first falsy operand, including null/zero/empty string.
    Ok(h.boolean(true))
}
fn filter_fills<H: PrivateMessageHost>(h: &mut H, fills: &H::Value) -> Result<H::Value, H::Error> {
    let f = h.get(fills, "filter")?;
    let predicate = h.valid_fill_predicate()?;
    h.call(&f, fills, &[predicate])
}
fn route_fills<H: PrivateMessageHost>(
    h: &mut H,
    socket: &H::Value,
    user: &H::Value,
    fills: &H::Value,
    snapshot: &H::Value,
) -> Result<(), H::Error> {
    if !is_array(h, fills)? {
        return Ok(());
    }
    let length = h.get(fills, "length")?;
    if h.strict_equal(&length, &h.number(0.0)) {
        return Ok(());
    }
    let normalized = filter_fills(h, fills)?;
    let length = h.get(&normalized, "length")?;
    if h.strict_equal(&length, &h.number(0.0)) {
        return Ok(());
    }
    let f = h.get(socket, "handleFills")?;
    let payload = h.object()?;
    h.define_data(&payload, "fills", &normalized)?;
    h.define_data(&payload, "isSnapshot", snapshot)?;
    h.define_data(&payload, "user", user)?;
    h.call(&f, socket, &[payload])?;
    Ok(())
}
fn route_orders<H: PrivateMessageHost>(
    h: &mut H,
    socket: &H::Value,
    orders: &H::Value,
) -> Result<(), H::Error> {
    if !is_array(h, orders)? {
        return Ok(());
    }
    let length = h.get(orders, "length")?;
    if h.strict_equal(&length, &h.number(0.0)) {
        return Ok(());
    }
    method(h, socket, "handleOrderUpdates", &[orders.clone()])?;
    Ok(())
}
pub fn core_handle_user_events<H: PrivateMessageHost>(
    h: &mut H,
    socket: &H::Value,
    data: &H::Value,
) -> Result<H::Value, H::Error> {
    if !h.truthy(data) {
        return Ok(h.undefined());
    }
    let mut user = optional(h, data, "user")?;
    for n in ["vault", "applicationId"] {
        if !h.truthy(&user) {
            let parent = h.get(socket, "parent")?;
            let options = h.get(&parent, "options")?;
            user = h.get(&options, n)?;
        }
    }
    if !h.truthy(&user) {
        user = lit(h, "");
    }
    if is_array(h, data)? {
        let fills = filter_fills(h, data)?;
        let length = h.get(&fills, "length")?;
        let zero = h.number(0.0);
        let has = h.operator(DispatchOperator::Greater, &length, &zero)?;
        if h.truthy(&has) {
            let snapshot = h.boolean(false);
            route_fills(h, socket, &user, &fills, &snapshot)?;
            return Ok(h.undefined());
        }
        route_orders(h, socket, data)?;
        return Ok(h.undefined());
    }
    let fills = optional(h, data, "fills")?;
    let boolean = h.global("Boolean")?;
    let snapshot = optional(h, data, "isSnapshot")?;
    let u = h.undefined();
    let snapshot = h.call(&boolean, &u, &[snapshot])?;
    route_fills(h, socket, &user, &fills, &snapshot)?;
    let orders = optional(h, data, "orders")?;
    route_orders(h, socket, &orders)?;
    Ok(h.undefined())
}
pub fn core_handle_server_error<H: PrivateMessageHost>(
    h: &mut H,
    socket: &H::Value,
    data: &H::Value,
) -> Result<H::Value, H::Error> {
    let subscription = if h.typeof_value(data) == "string" {
        let f = h.get(data, "match")?;
        let regex = h.regex(r#"/"type"\s*:\s*"([^\"]+)"/"#)?;
        let matches = h.call(&f, data, &[regex])?;
        if h.is_nullish(&matches) {
            h.undefined()
        } else {
            let one = h.number(1.0);
            h.index(&matches, &one)?
        }
    } else {
        h.undefined()
    };
    let parent = h.get(socket, "parent")?;
    let logger = h.get(&parent, "log")?;
    let mut message = "Hyperliquid WebSocket server error"
        .encode_utf16()
        .collect::<Vec<_>>();
    if h.truthy(&subscription) {
        message.extend(": rejected ".encode_utf16());
        message.extend(h.template_string(&subscription)?.0);
        message.extend(" subscription".encode_utf16());
    }
    // Outer template interpolates the inner template or empty string.
    let suffix = if h.truthy(&subscription) {
        JsString(message["Hyperliquid WebSocket server error".len()..].to_vec())
    } else {
        JsString(vec![])
    };
    let suffix = h.literal(&suffix);
    let suffix = h.template_string(&suffix)?;
    let mut full = "Hyperliquid WebSocket server error"
        .encode_utf16()
        .collect::<Vec<_>>();
    full.extend(suffix.0);
    let full = h.literal(&JsString(full));
    let severity = h.global("LogSeverity")?;
    let warning = h.get(&severity, "Warning")?;
    h.call(&logger, &parent, &[full, warning])?;
    Ok(h.undefined())
}
fn ticker_tables<H: PrivateMessageHost>(
    h: &mut H,
    socket: &H::Value,
) -> Result<(H::Value, H::Value), H::Error> {
    let parent = h.get(socket, "parent")?;
    let store = h.get(&parent, "store")?;
    let tickers = h.get(&store, "tickersById")?;
    let parent = h.get(socket, "parent")?;
    let store = h.get(&parent, "store")?;
    let positions = h.get(&store, "positions")?;
    let length = h.get(&positions, "length")?;
    let zero = h.number(0.0);
    let has = h.operator(DispatchOperator::Greater, &length, &zero)?;
    let set = if h.truthy(&has) {
        let class = h.global("Set")?;
        h.construct(&class, &[])?
    } else {
        h.undefined()
    };
    Ok((tickers, set))
}
fn sync_updated<H: PrivateMessageHost>(
    h: &mut H,
    socket: &H::Value,
    set: &H::Value,
) -> Result<(), H::Error> {
    if h.truthy(set) {
        let size = h.get(set, "size")?;
        let zero = h.number(0.0);
        let has = h.operator(DispatchOperator::Greater, &size, &zero)?;
        if h.truthy(&has) {
            let parent = h.get(socket, "parent")?;
            method(h, &parent, "syncBalanceFromLivePositions", &[set.clone()])?;
        }
    }
    Ok(())
}
fn mark_updated<H: PrivateMessageHost>(
    h: &mut H,
    set: &H::Value,
    ticker: &H::Value,
) -> Result<(), H::Error> {
    if h.truthy(set) {
        let f = h.get(set, "add")?;
        let symbol = h.get(ticker, "symbol")?;
        h.call(&f, set, &[symbol])?;
    }
    Ok(())
}
fn update_ticker<H: PrivateMessageHost>(
    h: &mut H,
    socket: &H::Value,
    ticker: &H::Value,
    buffer: &H::Value,
) -> Result<(), H::Error> {
    let parent = h.get(socket, "parent")?;
    let store = h.get(&parent, "store")?;
    method(h, &store, "updateTicker", &[ticker.clone(), buffer.clone()])?;
    Ok(())
}
fn all_mids<H: PrivateMessageHost>(
    h: &mut H,
    socket: &H::Value,
    data: &H::Value,
    hip3: bool,
) -> Result<H::Value, H::Error> {
    let mids = if hip3 {
        optional(h, data, "mids")?
    } else {
        data.clone()
    };
    if hip3 && (!h.truthy(&mids) || h.typeof_value(&mids) != "object") {
        return Ok(h.undefined());
    }
    let (tickers, set) = ticker_tables(h, socket)?;
    let buffer = h.get(
        socket,
        if hip3 {
            "_hip3MidsUpdateBuf"
        } else {
            "_midsUpdateBuf"
        },
    )?;
    let enumeration = h.for_in(&mids)?;
    while let Some(coin) = h.for_in_next(&enumeration)? {
        if hip3 {
            let colon = lit(h, ":");
            let includes = method(h, &coin, "includes", &[colon])?;
            if !h.truthy(&includes) {
                continue;
            }
        }
        let ticker = h.index(&tickers, &coin)?;
        if !h.truthy(&ticker) {
            continue;
        }
        let parser = h.global(if hip3 { "Number" } else { "parseFloat" })?;
        let value = h.index(&mids, &coin)?;
        let u = h.undefined();
        let last = h.call(&parser, &u, &[value])?;
        if hip3 {
            let number = h.global("Number")?;
            let finite = method(h, &number, "isFinite", &[last.clone()])?;
            if !h.truthy(&finite) {
                continue;
            }
        }
        h.set(&buffer, "last", &last)?;
        h.set(&buffer, "midPrice", &last)?;
        update_ticker(h, socket, &ticker, &buffer)?;
        mark_updated(h, &set, &ticker)?;
    }
    sync_updated(h, socket, &set)?;
    Ok(h.undefined())
}
pub fn core_handle_hip3_all_mids<H: PrivateMessageHost>(
    h: &mut H,
    socket: &H::Value,
    data: &H::Value,
) -> Result<H::Value, H::Error> {
    all_mids(h, socket, data, true)
}
pub fn public_handle_all_mids<H: PrivateMessageHost>(
    h: &mut H,
    socket: &H::Value,
    data: &H::Value,
) -> Result<H::Value, H::Error> {
    all_mids(h, socket, data, false)
}
pub fn public_handle_fast_asset_ctxs<H: PrivateMessageHost>(
    h: &mut H,
    socket: &H::Value,
    payload: &H::Value,
) -> Result<H::Value, H::Error> {
    let data = free(h, "decodeHyperliquidFastAssetCtxs", &[payload.clone()])?;
    let (tickers, set) = ticker_tables(h, socket)?;
    let enumeration = h.for_in(&data)?;
    while let Some(coin) = h.for_in_next(&enumeration)? {
        let ticker = h.index(&tickers, &coin)?;
        if !h.truthy(&ticker) {
            continue;
        }
        let context = h.index(&data, &coin)?;
        let number = h.global("Number")?;
        let mark = optional(h, &context, "markPx")?;
        let u = h.undefined();
        let mark = h.call(&number, &u, &[mark])?;
        let number = h.global("Number")?;
        let mid = optional(h, &context, "midPx")?;
        let mid = h.call(&number, &u, &[mid])?;
        let number = h.global("Number")?;
        let finite = method(h, &number, "isFinite", &[mark.clone()])?;
        let has_mark = if h.truthy(&finite) {
            let zero = h.number(0.0);
            let yes = h.operator(DispatchOperator::Greater, &mark, &zero)?;
            h.truthy(&yes)
        } else {
            false
        };
        let number = h.global("Number")?;
        let finite = method(h, &number, "isFinite", &[mid.clone()])?;
        let has_mid = if h.truthy(&finite) {
            let zero = h.number(0.0);
            let yes = h.operator(DispatchOperator::Greater, &mid, &zero)?;
            h.truthy(&yes)
        } else {
            false
        };
        if has_mark || has_mid {
            let buffer = h.get(
                socket,
                if has_mark && has_mid {
                    "_fastMarkAndMidUpdateBuf"
                } else if has_mark {
                    "_fastMarkUpdateBuf"
                } else {
                    "_fastMidUpdateBuf"
                },
            )?;
            if has_mark {
                h.set(&buffer, "mark", &mark)?;
            }
            if has_mid {
                h.set(&buffer, "last", &mid)?;
                h.set(&buffer, "midPrice", &mid)?;
            }
            update_ticker(h, socket, &ticker, &buffer)?;
            if has_mid {
                mark_updated(h, &set, &ticker)?;
            }
        }
    }
    sync_updated(h, socket, &set)?;
    Ok(h.undefined())
}
