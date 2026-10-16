//! Subscription and diagnostic emission algorithms. Full websocket classes and
//! global/iterator/property/JSON/callee primitives remain explicit host bindings.
use crate::{spot_json::JsString, ws_ping_lifecycle::WsPingHost, ws_transport_leaves::WsVenue};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticScope {
    Public,
    Private,
}
pub trait WsEmissionHost: WsPingHost {
    fn boolean(&self, value: bool) -> Self::Value;
    fn system_types(&mut self, venue: WsVenue) -> Result<Self::Value, Self::Error>;
    fn iterator(&mut self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    /// IteratorStep failure does not close the iterator.
    fn next(&mut self, iterator: &Self::Value) -> Result<Option<Self::Value>, Self::Error>;
    /// For-of IteratorClose on a throwing body; original throw takes precedence.
    fn close_after_throw(&mut self, iterator: &Self::Value);
    fn triple(&mut self, value: &Self::Value) -> Result<[Self::Value; 3], Self::Error>;
    fn string_units<'a>(&self, value: &'a Self::Value) -> Option<&'a [u16]>;
    fn spread_into(
        &mut self,
        target: &Self::Value,
        source: &Self::Value,
    ) -> Result<(), Self::Error>;
    /// Allocate a format closure. Invoke `run_diagnostic_formatter` with its
    /// captured scope when the native diagnostic consumer calls the closure.
    fn formatter(&mut self, scope: DiagnosticScope) -> Result<Self::Value, Self::Error>;
}
fn text(s: &str) -> JsString {
    JsString(s.encode_utf16().collect())
}
fn literal<H: WsPingHost>(h: &H, s: &str) -> H::Value {
    h.literal(&text(s))
}
fn method<H: WsPingHost>(
    h: &mut H,
    receiver: &H::Value,
    name: &str,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    let function = h.get(receiver, name)?;
    h.call(&function, receiver, args)
}
fn define<H: WsPingHost>(
    h: &mut H,
    object: &H::Value,
    name: &str,
    value: &H::Value,
) -> Result<(), H::Error> {
    h.define_data(object, name, value)
}
fn scope_name(scope: DiagnosticScope) -> &'static str {
    match scope {
        DiagnosticScope::Public => "public",
        DiagnosticScope::Private => "private",
    }
}
pub fn run_diagnostic_formatter<H: WsEmissionHost>(
    h: &mut H,
    scope: DiagnosticScope,
    message: &H::Value,
) -> Result<H::Value, H::Error> {
    let function = h.global("formatHyperliquidWsDiagnostic")?;
    let scope = literal(h, scope_name(scope));
    let receiver = h.undefined();
    h.call(&function, &receiver, &[scope, message.clone()])
}
fn send_diagnostics<H: WsEmissionHost>(
    h: &mut H,
    socket: &H::Value,
    data: &H::Value,
    scope: DiagnosticScope,
) -> Result<(), H::Error> {
    method(h, socket, "send", &[data.clone()])?;
    let predicate = h.global("shouldEmitHyperliquidWsSendDiagnostic")?;
    let receiver = h.undefined();
    let emit = h.call(&predicate, &receiver, &[data.clone()])?;
    if h.truthy(&emit) {
        emit_diagnostic(h, socket, "emitWsDiagnosticSend", data, scope)?;
    }
    Ok(())
}
fn receive_diagnostics<H: WsEmissionHost>(
    h: &mut H,
    socket: &H::Value,
    data: &H::Value,
    json: &H::Value,
    scope: DiagnosticScope,
) -> Result<(), H::Error> {
    let predicate = h.global("shouldEmitHyperliquidWsReceiveDiagnostic")?;
    let scope_value = literal(h, scope_name(scope));
    let receiver = h.undefined();
    let emit = h.call(&predicate, &receiver, &[scope_value, json.clone()])?;
    if h.truthy(&emit) {
        emit_diagnostic(h, socket, "emitWsDiagnosticReceive", data, scope)?;
    }
    Ok(())
}
fn emit_diagnostic<H: WsEmissionHost>(
    h: &mut H,
    socket: &H::Value,
    name: &str,
    data: &H::Value,
    scope: DiagnosticScope,
) -> Result<(), H::Error> {
    let function = h.get(socket, name)?;
    let options = h.object()?;
    let format = h.formatter(scope)?;
    define(h, &options, "format", &format)?;
    h.call(&function, socket, &[data.clone(), options])?;
    Ok(())
}
pub fn public_send_with_diagnostics<H: WsEmissionHost>(
    h: &mut H,
    socket: &H::Value,
    data: &H::Value,
) -> Result<(), H::Error> {
    send_diagnostics(h, socket, data, DiagnosticScope::Public)
}
pub fn private_send_with_diagnostics<H: WsEmissionHost>(
    h: &mut H,
    socket: &H::Value,
    data: &H::Value,
) -> Result<(), H::Error> {
    send_diagnostics(h, socket, data, DiagnosticScope::Private)
}
pub fn public_emit_receive_diagnostics<H: WsEmissionHost>(
    h: &mut H,
    socket: &H::Value,
    data: &H::Value,
    json: &H::Value,
) -> Result<(), H::Error> {
    receive_diagnostics(h, socket, data, json, DiagnosticScope::Public)
}
pub fn private_emit_receive_diagnostics<H: WsEmissionHost>(
    h: &mut H,
    socket: &H::Value,
    data: &H::Value,
    json: &H::Value,
) -> Result<(), H::Error> {
    receive_diagnostics(h, socket, data, json, DiagnosticScope::Private)
}
fn user<H: WsPingHost>(h: &mut H, socket: &H::Value) -> Result<H::Value, H::Error> {
    let parent = h.get(socket, "parent")?;
    let options = h.get(&parent, "options")?;
    let vault = h.get(&options, "vault")?;
    if h.truthy(&vault) {
        Ok(vault)
    } else {
        let parent = h.get(socket, "parent")?;
        let options = h.get(&parent, "options")?;
        h.get(&options, "applicationId")
    }
}
fn for_each<H, F>(h: &mut H, value: &H::Value, mut body: F) -> Result<(), H::Error>
where
    H: WsEmissionHost,
    F: FnMut(&mut H, H::Value) -> Result<(), H::Error>,
{
    let iterator = h.iterator(value)?;
    while let Some(value) = h.next(&iterator)? {
        if let Err(error) = body(h, value) {
            h.close_after_throw(&iterator);
            return Err(error);
        }
    }
    Ok(())
}
// Resolve sender/stringify before allocating the outer literal and evaluating
// the nested subscription's values. Every literal uses CreateDataProperty.
fn send_subscription<H, F>(
    h: &mut H,
    socket: &H::Value,
    sender: &str,
    build: F,
) -> Result<(), H::Error>
where
    H: WsEmissionHost,
    F: FnOnce(&mut H) -> Result<H::Value, H::Error>,
{
    let send = h.get(socket, sender)?;
    let json = h.global("JSON")?;
    let stringify = h.get(&json, "stringify")?;
    let payload = h.object()?;
    let subscribe = literal(h, "subscribe");
    define(h, &payload, "method", &subscribe)?;
    let subscription = build(h)?;
    define(h, &payload, "subscription", &subscription)?;
    let data = h.call(&stringify, &json, &[payload])?;
    h.call(&send, socket, &[data])?;
    Ok(())
}
fn subscription<H: WsPingHost>(
    h: &mut H,
    type_value: &H::Value,
    user_value: Option<&H::Value>,
    dex: Option<&H::Value>,
) -> Result<H::Value, H::Error> {
    let result = h.object()?;
    define(h, &result, "type", type_value)?;
    if let Some(user) = user_value {
        define(h, &result, "user", user)?;
    }
    if let Some(dex) = dex {
        define(h, &result, "dex", dex)?;
    }
    Ok(result)
}
pub fn private_subscribe_system_topics<H: WsEmissionHost>(
    h: &mut H,
    socket: &H::Value,
) -> Result<(), H::Error> {
    let user = user(h, socket)?;
    let types = h.system_types(WsVenue::Perp)?;
    for_each(h, &types, |h, type_value| {
        send_subscription(h, socket, "sendWithDiagnostics", |h| {
            subscription(h, &type_value, Some(&user), None)
        })
    })?;
    let parent = h.get(socket, "parent")?;
    let track = method(h, &parent, "shouldTrackSpotHoldings", &[])?;
    if h.truthy(&track) {
        send_subscription(h, socket, "sendWithDiagnostics", |h| {
            let type_value = literal(h, "spotState");
            let result = subscription(h, &type_value, Some(&user), None)?;
            let parent = h.get(socket, "parent")?;
            let flag = method(h, &parent, "shouldRequestPortfolioMarginSpotState", &[])?;
            let spread = h.object()?;
            if h.truthy(&flag) {
                let yes = h.boolean(true);
                define(h, &spread, "isPortfolioMargin", &yes)?;
            }
            h.spread_into(&result, &spread)?;
            Ok(result)
        })?;
    }
    send_subscription(h, socket, "sendWithDiagnostics", |h| {
        let type_value = literal(h, "clearinghouseState");
        subscription(h, &type_value, Some(&user), None)
    })?;
    let parent = h.get(socket, "parent")?;
    let dexes = h.get(&parent, "hip3DexNames")?;
    for_each(h, &dexes, |h, dex| {
        for type_name in [
            "allMids",
            "clearinghouseState",
            "userFills",
            "orderUpdates",
            "userEvents",
        ] {
            send_subscription(h, socket, "sendWithDiagnostics", |h| {
                let type_value = literal(h, type_name);
                subscription(
                    h,
                    &type_value,
                    (type_name != "allMids").then_some(&user),
                    Some(&dex),
                )
            })?;
        }
        Ok(())
    })
}
pub fn private_subscribe_all_active_asset_data<H: WsEmissionHost>(
    h: &mut H,
    socket: &H::Value,
) -> Result<(), H::Error> {
    let user = user(h, socket)?;
    let parent = h.get(socket, "parent")?;
    let store = h.get(&parent, "store")?;
    let markets = h.get(&store, "markets")?;
    for_each(h, &markets, |h, market| {
        let coin = h.get(&market, "id")?;
        let spot = h.get(&market, "isSpot")?;
        if h.truthy(&spot) {
            return Ok(());
        }
        let colon = literal(h, ":");
        let hip3 = method(h, &coin, "includes", &[colon])?;
        if h.truthy(&hip3) {
            return Ok(());
        }
        send_subscription(h, socket, "sendWithDiagnostics", |h| {
            let type_value = literal(h, "activeAssetData");
            let result = subscription(h, &type_value, Some(&user), None)?;
            define(h, &result, "coin", &coin)?;
            Ok(result)
        })
    })
}
pub fn spot_private_subscribe_system_topics<H: WsEmissionHost>(
    h: &mut H,
    socket: &H::Value,
) -> Result<(), H::Error> {
    let types = h.system_types(WsVenue::Spot)?;
    for_each(h, &types, |h, type_value| {
        send_subscription(h, socket, "send", |h| {
            let result = subscription(h, &type_value, None, None)?;
            let user = user(h, socket)?;
            define(h, &result, "user", &user)?;
            Ok(result)
        })
    })
}
pub fn spot_private_subscribe_coins<H: WsEmissionHost>(
    h: &mut H,
    socket: &H::Value,
) -> Result<(), H::Error> {
    let parent = h.get(socket, "parent")?;
    let store = h.get(&parent, "store")?;
    let markets = h.get(&store, "markets")?;
    for_each(h, &markets, |h, market| {
        send_subscription(h, socket, "send", |h| {
            let type_value = literal(h, "activeAssetData");
            let result = subscription(h, &type_value, None, None)?;
            let user = user(h, socket)?;
            define(h, &result, "user", &user)?;
            let coin = h.get(&market, "id")?;
            define(h, &result, "coin", &coin)?;
            Ok(result)
        })
    })
}
pub fn spot_public_subscribe_public_topics<H: WsEmissionHost>(
    h: &mut H,
    socket: &H::Value,
) -> Result<(), H::Error> {
    send_subscription(h, socket, "send", |h| {
        let type_value = literal(h, "allMids");
        subscription(h, &type_value, None, None)
    })?;
    method(h, socket, "subscribeUserTopics", &[])?;
    Ok(())
}
fn equal<H: WsEmissionHost>(h: &H, value: &H::Value, s: &str) -> bool {
    h.string_units(value)
        .is_some_and(|units| units.iter().copied().eq(s.encode_utf16()))
}
pub fn spot_public_subscribe_user_topics<H: WsEmissionHost>(
    h: &mut H,
    socket: &H::Value,
) -> Result<(), H::Error> {
    let flatten = h.global("flatten")?;
    let object = h.global("Object")?;
    let keys = h.get(&object, "keys")?;
    let topics = h.get(socket, "topics")?;
    let keys = h.call(&keys, &object, &[topics])?;
    let receiver = h.undefined();
    let topics = h.call(&flatten, &receiver, &[keys])?;
    for_each(h, &topics, |h, topic| {
        let dot = literal(h, ".");
        let split = method(h, &topic, "split", &[dot])?;
        let [id, symbol, interval] = h.triple(&split)?;
        let type_name = if equal(h, &id, "candle") {
            "candle"
        } else if equal(h, &id, "l2Book") {
            "l2Book"
        } else {
            return Ok(());
        };
        // This source method creates its payload BEFORE resolving this.send.
        let payload = h.object()?;
        let subscribe = literal(h, "subscribe");
        define(h, &payload, "method", &subscribe)?;
        let type_value = literal(h, type_name);
        let subscription = subscription(h, &type_value, None, None)?;
        define(h, &subscription, "coin", &symbol)?;
        if type_name == "candle" {
            define(h, &subscription, "interval", &interval)?;
        }
        define(h, &payload, "subscription", &subscription)?;
        let send = h.get(socket, "send")?;
        let json = h.global("JSON")?;
        let stringify = h.get(&json, "stringify")?;
        let data = h.call(&stringify, &json, &[payload])?;
        h.call(&send, socket, &[data])?;
        Ok(())
    })
}

/// Core fixed feeds deliberately bypass the active-subscription Set.
pub fn core_public_subscribe_public_topics<H: WsEmissionHost>(
    h: &mut H,
    socket: &H::Value,
) -> Result<(), H::Error> {
    for name in ["allMids", "fastAssetCtxs"] {
        send_subscription(h, socket, "sendWithDiagnostics", |h| {
            let kind = literal(h, name);
            subscription(h, &kind, None, None)
        })?;
    }
    method(h, socket, "subscribeUserTopics", &[])?;
    Ok(())
}
pub fn core_public_send_subscription<H: WsEmissionHost>(
    h: &mut H,
    socket: &H::Value,
    subscription: &H::Value,
) -> Result<H::Value, H::Error> {
    let json = h.global("JSON")?;
    let key = method(h, &json, "stringify", &[subscription.clone()])?;
    let active = h.get(socket, "activeSubscriptions")?;
    let has = method(h, &active, "has", &[key.clone()])?;
    if h.truthy(&has) {
        return Ok(h.boolean(false));
    }
    let active = h.get(socket, "activeSubscriptions")?;
    method(h, &active, "add", &[key])?;
    core_public_send_envelope(h, socket, "subscribe", subscription)?;
    Ok(h.boolean(true))
}
fn core_public_send_envelope<H: WsEmissionHost>(
    h: &mut H,
    socket: &H::Value,
    name: &str,
    subscription: &H::Value,
) -> Result<(), H::Error> {
    let sender = h.get(socket, "sendWithDiagnostics")?;
    let json = h.global("JSON")?;
    let stringify = h.get(&json, "stringify")?;
    let payload = h.object()?;
    let name = literal(h, name);
    h.define_data(&payload, "method", &name)?;
    h.define_data(&payload, "subscription", subscription)?;
    let data = h.call(&stringify, &json, &[payload])?;
    h.call(&sender, socket, &[data])?;
    Ok(())
}
pub fn core_public_send_unsubscription<H: WsEmissionHost>(
    h: &mut H,
    socket: &H::Value,
    subscription: &H::Value,
) -> Result<(), H::Error> {
    // The member-reference/callee is captured before JSON.stringify is evaluated.
    let active = h.get(socket, "activeSubscriptions")?;
    let delete = h.get(&active, "delete")?;
    let json = h.global("JSON")?;
    let key = method(h, &json, "stringify", &[subscription.clone()])?;
    h.call(&delete, &active, &[key])?;
    core_public_send_envelope(h, socket, "unsubscribe", subscription)
}
pub fn core_public_subscribe_user_topics<H: WsEmissionHost>(
    h: &mut H,
    socket: &H::Value,
) -> Result<(), H::Error> {
    let flatten = h.global("flatten")?;
    let object = h.global("Object")?;
    let keys = h.get(&object, "keys")?;
    let topics = h.get(socket, "topics")?;
    let keys = h.call(&keys, &object, &[topics])?;
    let topics = h.call(&flatten, &h.undefined(), &[keys])?;
    for_each(h, &topics, |h, topic| {
        let dot = literal(h, ".");
        let split = method(h, &topic, "split", &[dot])?;
        let [id, coin, interval] = h.triple(&split)?;
        let types: &[&str] = if equal(h, &id, "candle") {
            &["candle"]
        } else if equal(h, &id, "l2Book") {
            &["l2Book", "bbo"]
        } else {
            &[]
        };
        for kind in types {
            let sender = h.get(socket, "sendSubscription")?;
            let type_value = literal(h, kind);
            let sub = subscription(h, &type_value, None, None)?;
            h.define_data(&sub, "coin", &coin)?;
            if *kind == "candle" {
                h.define_data(&sub, "interval", &interval)?;
            }
            h.call(&sender, socket, &[sub])?;
        }
        Ok(())
    })
}
