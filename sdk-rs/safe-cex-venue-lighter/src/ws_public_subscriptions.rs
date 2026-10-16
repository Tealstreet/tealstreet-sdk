//! Public routing, topic hydration, sending and book-fallback startup bodies.
//! Live JS property/callee/receiver/iterator/literal/timer/transport bindings are
//! required hosts. Typed UTF16 strings, booleans, arrays and ordinary intrinsics;
//! complete websocket construction and reflective intrinsic overrides unproved.
use crate::{sdk_account_api::AccountValue, transaction_signing::SigningResult};
use std::rc::Rc;
pub trait SubscriptionCall {
    fn call(&self, args: &[AccountValue]) -> SigningResult<AccountValue>;
}
pub type SubscriptionCallback = Rc<dyn Fn() -> SigningResult<()>>;
pub trait SubscriptionTimer {
    fn call(&self, callback: SubscriptionCallback, delay: f64) -> SigningResult<AccountValue>;
}
pub trait PublicSubscriptionHost {
    fn owner(&self) -> AccountValue;
    /// Actual property Get/strict Set/Delete, including native identities,
    /// inherited/proxy/getter behavior and original throws, with UTF16 keys.
    fn get(&self, object: &AccountValue, key: &[u16]) -> SigningResult<AccountValue>;
    fn set(&self, object: &AccountValue, key: &[u16], value: AccountValue) -> SigningResult<()>;
    fn delete(&self, object: &AccountValue, key: &[u16]) -> SigningResult<()>;
    /// Capture actual callee AND receiver; noncallable failure at Call, after
    /// arguments. Global dotted names include actual object/member acquisition.
    fn global(&self, name: &str) -> SigningResult<Rc<dyn SubscriptionCall>>;
    fn member(&self, object: &AccountValue, key: &str) -> SigningResult<Rc<dyn SubscriptionCall>>;
    fn interval(&self) -> SigningResult<Rc<dyn SubscriptionTimer>>;
    /// Full source GetIterator/for-of, live yields and IteratorClose with original
    /// throw precedence, never snapshot an owner array to a Rust value vector.
    fn for_of(
        &self,
        value: &AccountValue,
        visit: &mut dyn FnMut(AccountValue) -> SigningResult<()>,
    ) -> SigningResult<()>;
    /// Fresh ordinary source literal, own ordered data fields, original intrinsic
    /// prototype. Descriptor/resource behavior required, not fixture proof.
    fn literal(&self, fields: &[(&str, AccountValue)]) -> SigningResult<AccountValue>;
    /// Full Abstract Relational Comparison with sampled Number clock on the left;
    /// live right-side ToPrimitive(number), Number/BigInt comparison semantics and
    /// original coercion throws, including inherited record values.
    fn less_than(&self, left: f64, right: AccountValue) -> SigningResult<bool>;
}
fn u(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
fn text(value: &[u16]) -> AccountValue {
    AccountValue::String(value.to_vec())
}
fn get(
    h: &dyn PublicSubscriptionHost,
    object: &AccountValue,
    key: &str,
) -> SigningResult<AccountValue> {
    h.get(object, &u(key))
}
fn path(h: &dyn PublicSubscriptionHost, keys: &[&str]) -> SigningResult<AccountValue> {
    let mut value = h.owner();
    for key in keys {
        value = get(h, &value, key)?;
    }
    Ok(value)
}
fn string(value: AccountValue) -> Vec<u16> {
    let AccountValue::String(value) = value else {
        unreachable!("typed string or actual String() result")
    };
    value
}
fn num(value: AccountValue) -> f64 {
    let AccountValue::Number(value) = value else {
        unreachable!("typed Number clock")
    };
    value
}
fn call(
    h: &dyn PublicSubscriptionHost,
    name: &str,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    h.member(&h.owner(), name)?.call(args)
}
fn log(
    h: &dyn PublicSubscriptionHost,
    prefix: &str,
    topic: &[u16],
    reason: &[u16],
) -> SigningResult<()> {
    let parent = path(h, &["parent"])?;
    let log = h.member(&parent, "log")?;
    let mut message = u(prefix);
    message.extend_from_slice(topic);
    message.extend(u("; enabling REST fallback ("));
    message.extend_from_slice(reason);
    message.extend(u(")."));
    log.call(&[text(&message), text(&u("warning"))])?;
    Ok(())
}
// ts: LighterPublicWebsocket#isWsProxyRouteEnabled
pub fn is_ws_proxy_route_enabled(h: &dyn PublicSubscriptionHost) -> SigningResult<bool> {
    if !path(
        h,
        &["parent", "lighterDeploymentProfile", "supportsWsProxy"],
    )?
    .truthy()
    {
        return Ok(false);
    }
    let route = path(h, &["parent", "options", "tealstreetWsRoute"])?;
    if route.is_nullish() || !route.truthy() {
        return Ok(false);
    }
    let testnet = path(h, &["parent", "options", "testnet"])?;
    if !testnet.is_nullish() && testnet.truthy() {
        return Ok(false);
    }
    let resolve = h.global("resolveTransportRouting")?;
    let name = path(h, &["parent", "name"])?;
    let options = path(h, &["parent", "options"])?;
    let routing = resolve.call(&[name, options])?;
    let public = get(h, &routing, "wsPublic")?;
    Ok(get(h, &public, "host")?.truthy())
}
// ts: LighterPublicWebsocket.connectAndSubscribe
pub fn connect_and_subscribe(
    h: &dyn PublicSubscriptionHost,
    _is_reconnect: Option<bool>,
) -> SigningResult<()> {
    if get(h, &h.owner(), "isDisposed")?.truthy() {
        return Ok(());
    }
    let proxy = call(h, "isWsProxyRouteEnabled", &[])?.truthy()
        && !get(h, &h.owner(), "useDirectFallbackRoute")?.truthy();
    let readonly = get(h, &h.owner(), "useReadonlyDirectRoute")?.truthy() && !proxy;
    let endpoint = if proxy {
        let resolve = h.global("resolveTransportRouting")?;
        let name = path(h, &["parent", "name"])?;
        let options = path(h, &["parent", "options"])?;
        let routing = resolve.call(&[name, options])?;
        let public = get(h, &routing, "wsPublic")?;
        get(h, &public, "host")?
    } else {
        path(h, &["parent", "lighterDeploymentProfile", "wsUrl"])?
    };
    let includes = h.member(&endpoint, "includes")?;
    let separator = if includes.call(&[text(&u("?"))])?.truthy() {
        '&'
    } else {
        '?'
    };
    let id = string(h.global("nanoid")?.call(&[])?);
    let mut query = u("requestId=");
    query.extend(id);
    if readonly {
        query.extend(u("&readonly=true"));
    }
    h.set(
        &h.owner(),
        &u("connectedViaProxyRoute"),
        AccountValue::Boolean(proxy),
    )?;
    h.set(
        &h.owner(),
        &u("openedSinceLastConnect"),
        AccountValue::Boolean(false),
    )?;
    let now = h.global("virtualClock.getCurrentTimeMs")?.call(&[])?;
    h.set(&h.owner(), &u("connectStartedAt"), now)?;
    h.set(
        &h.owner(),
        &u("hasReceivedMessageSinceConnect"),
        AccountValue::Boolean(false),
    )?;
    h.set(
        &h.owner(),
        &u("hasPrimedTickerRestFallbackForConnection"),
        AccountValue::Boolean(false),
    )?;
    let create = h.member(&h.owner(), "createWebsocket")?;
    let mut url = string(endpoint);
    url.push(separator as u16);
    url.extend(query);
    let ws = create.call(&[text(&url)])?;
    h.set(&h.owner(), &u("ws"), ws)?;
    for (event, handler) in [
        ("open", "onOpen"),
        ("message", "onMessage"),
        ("close", "onClose"),
    ] {
        let ws = get(h, &h.owner(), "ws")?;
        let add = h.member(&ws, "addEventListener")?;
        let handler = get(h, &h.owner(), handler)?;
        add.call(&[text(&u(event)), handler])?;
    }
    if proxy {
        call(h, "primeTickerRestFallback", &[])?;
        call(
            h,
            "armTickerRestFallbackWatchdog",
            &[
                AccountValue::Number(1500.0),
                text(&u(
                    "public proxy websocket has not produced messages yet; polling tickers via REST",
                )),
            ],
        )?;
    }
    Ok(())
}
fn market_id(h: &dyn PublicSubscriptionHost, market: &AccountValue) -> SigningResult<Vec<u16>> {
    let stringify = h.global("String")?;
    let id = get(h, market, "id")?;
    let id = if id.truthy() { id } else { text(&[]) };
    Ok(string(stringify.call(&[id])?))
}
// ts: LighterPublicWebsocket#onMarketsChanged
pub fn on_markets_changed(
    h: &dyn PublicSubscriptionHost,
    event: &AccountValue,
) -> SigningResult<()> {
    let added = get(h, event, "added")?;
    if matches!(get(h,&added,"length")?,AccountValue::Number(n) if n==0.0)
        || !get(h, &h.owner(), "ws")?.truthy()
        || get(h, &h.owner(), "isDisposed")?.truthy()
    {
        return Ok(());
    }
    if !get(h, &h.owner(), "isConnected")?.truthy() {
        return Ok(());
    }
    h.for_of(&added, &mut |market| {
        let id = market_id(h, &market)?;
        if id.is_empty() {
            return Ok(());
        }
        let mut key = string(get(h, &h.owner(), "bboTickerTopicPrefix")?);
        key.extend_from_slice(&id);
        let mut channel = u("ticker/");
        channel.extend(id);
        let topics = get(h, &h.owner(), "topics")?;
        h.set(&topics, &key, text(&channel))?;
        let send = h.member(&h.owner(), "send")?;
        let json = h.global("JSON.stringify")?;
        let payload = h.literal(&[("type", text(&u("subscribe"))), ("channel", text(&channel))])?;
        send.call(&[json.call(&[payload])?])?;
        Ok(())
    })
}
// ts: LighterPublicWebsocket#sendPayload
pub fn send_payload(h: &dyn PublicSubscriptionHost, payload: &AccountValue) -> SigningResult<()> {
    let send = h.member(&h.owner(), "send")?;
    let json = h.global("JSON.stringify")?;
    send.call(&[json.call(&[payload.clone()])?])?;
    Ok(())
}
// ts: LighterPublicWebsocket#syncBboTickerTopics
pub fn sync_bbo_ticker_topics(h: &dyn PublicSubscriptionHost) -> SigningResult<()> {
    let keys = h.global("Object.keys")?;
    let topics = get(h, &h.owner(), "topics")?;
    let keys = keys.call(&[topics])?;
    h.for_of(&keys, &mut |key| {
        let key = string(key);
        let prefix = string(get(h, &h.owner(), "bboTickerTopicPrefix")?);
        if key.starts_with(&prefix) {
            let topics = get(h, &h.owner(), "topics")?;
            h.delete(&topics, &key)?;
        }
        Ok(())
    })?;
    let markets = path(h, &["parent", "store", "markets"])?;
    h.for_of(&markets, &mut |market| {
        let id = market_id(h, &market)?;
        if id.is_empty() {
            return Ok(());
        }
        // Computed assignment evaluates owner record before prefix/key expression.
        let topics = get(h, &h.owner(), "topics")?;
        let mut key = string(get(h, &h.owner(), "bboTickerTopicPrefix")?);
        key.extend_from_slice(&id);
        let mut value = u("ticker/");
        value.extend(id);
        h.set(&topics, &key, text(&value))?;
        Ok(())
    })
}
// ts: LighterPublicWebsocket.handleTickersEvent
pub fn handle_tickers_event(
    h: &dyn PublicSubscriptionHost,
    json: &AccountValue,
) -> SigningResult<()> {
    let tickers = get(h, json, "data")?;
    if !h
        .global("Array.isArray")?
        .call(&[tickers.clone()])?
        .truthy()
    {
        return Ok(());
    }
    h.for_of(&tickers, &mut |ticker| {
        let handle = h.member(&h.owner(), "handleTickerEvent")?;
        let payload = h.literal(&[("data", ticker)])?;
        handle.call(&[payload])?;
        Ok(())
    })
}
// ts: LighterPublicWebsocket#isOrderBookRestFallbackCoolingDown
pub fn is_order_book_rest_fallback_cooling_down(
    h: &dyn PublicSubscriptionHost,
    topic: &[u16],
) -> SigningResult<bool> {
    let now = num(h.global("virtualClock.getCurrentTimeMs")?.call(&[])?);
    let record = get(h, &h.owner(), "orderBookRestFallbackCooldownUntilByTopic")?;
    let until = h.get(&record, topic)?;
    h.less_than(
        now,
        if until.truthy() {
            until
        } else {
            AccountValue::Number(0.0)
        },
    )
}
// ts: LighterPublicWebsocket#startOrderBookRestFallback
pub fn start_order_book_rest_fallback(
    h: Rc<dyn PublicSubscriptionHost>,
    topic: &[u16],
    market_id: &[u16],
    book: AccountValue,
    callback: AccountValue,
    reason: &[u16],
) -> SigningResult<()> {
    if call(
        h.as_ref(),
        "isOrderBookRestFallbackCoolingDown",
        &[text(topic)],
    )?
    .truthy()
    {
        return Ok(());
    }
    let intervals = get(h.as_ref(), &h.owner(), "orderBookRestFallbackIntervals")?;
    if h.get(&intervals, topic)?.truthy() {
        return Ok(());
    }
    log(
        h.as_ref(),
        "[Lighter] Public WS orderbook stream stalled for ",
        topic,
        reason,
    )?;
    let args = vec![text(topic), text(market_id), book, callback];
    call(h.as_ref(), "fetchAndEmitOrderBookFromRest", &args)?;
    // Assignment LHS is captured before setInterval lookup/call.
    let lhs = get(h.as_ref(), &h.owner(), "orderBookRestFallbackIntervals")?;
    let timer = h.interval()?;
    let owner = h.clone();
    let key = topic.to_vec();
    let args_copy = args.clone();
    let tick: SubscriptionCallback = Rc::new(move || {
        let disposed = get(owner.as_ref(), &owner.owner(), "isDisposed")?.truthy();
        let stop = if disposed {
            true
        } else {
            let handlers = get(owner.as_ref(), &owner.owner(), "messageHandlers")?;
            let value = owner.get(&handlers, &key)?;
            let length = if value.is_nullish() {
                AccountValue::Undefined
            } else {
                get(owner.as_ref(), &value, "length")?
            };
            !length.truthy()
        };
        if stop {
            call(owner.as_ref(), "stopOrderBookRestFallback", &[text(&key)])?;
        } else {
            call(owner.as_ref(), "fetchAndEmitOrderBookFromRest", &args_copy)?;
        }
        Ok(())
    });
    let id = timer.call(tick, 2000.0)?;
    h.set(&lhs, topic, id)
}
