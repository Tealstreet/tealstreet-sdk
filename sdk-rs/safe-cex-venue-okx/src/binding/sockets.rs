//! OKXPublicWebsocket, OKXPrivateWebsocket and OKXBusinessWebsocket: JS socket
//! objects over the shared BaseWebSocket bridge, each member running its port.
use super::{
    members::{define, sync_member},
    ops::{OkxScope, OkxShared, runtime_ops, socket_ops},
};
use crate::{
    family_policy, finite, history_stream_api, logged_socket, message_routing, public_lifecycle,
    public_lifecycle::SubscriptionTopic,
    public_socket, startup,
    types::{self, OkxInstrumentType, OkxRegion},
    websocket_account_events::{
        self as events, BusinessSocketState, PrivateSocketState, PublicSocketState,
    },
};
use safe_cex_exchange::{
    js_runtime::{
        promise::await_js,
        scope::{bound_member, scope_of},
        sockets::{base_websocket, core_of},
        value::{Getter, Js, Thrown, arg, error_object, from_json, from_typed, to_json, to_typed},
    },
    runtime::ExchangeError,
    websocket::{BaseWebSocket, MessageHandler},
};
use safe_cex_store::config::ENABLE_RECONNECTING_WEBSOCKET;
use safe_cex_transport::{RoutingOptions, socket::SocketFrame};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::{Rc, Weak},
};

fn text(v: &Js) -> Result<String, Thrown> {
    Ok(String::from_utf16_lossy(&v.to_js_string()?))
}

fn units(v: &Js) -> Result<Vec<u16>, Thrown> {
    Ok(v.to_js_string()?.to_vec())
}

fn coercion(e: safe_cex_types::js_coercion::JsCoercionError) -> Thrown {
    Thrown::error(e.name, e.message)
}

fn items(v: &Js) -> Result<Vec<Js>, Thrown> {
    v.array_values()
        .ok_or_else(|| Thrown::type_error(&format!("{} is not iterable", v.to_display())))
}

fn unit(result: Result<Js, Thrown>) -> Result<(), Thrown> {
    result.map(|_| ())
}

/// The typed BaseWebSocket core behind this socket object.
fn core(s: &OkxScope) -> Result<Rc<BaseWebSocket>, Thrown> {
    core_of(&s.shared.sockets, &s.this)
        .ok_or_else(|| Thrown::error("Error", "OKX socket has no BaseWebSocket core"))?
        .base()
}

/// `resolveTransportRouting(this.parent.name, this.parent.options)[name]`.
fn route(s: &OkxScope, name: &str) -> Result<Js, Thrown> {
    let extra = &s.shared.extra;
    let routing = s
        .shared
        .runtime
        .transport()
        .ws_routing(&extra.name, &RoutingOptions::from(&extra.options));
    from_typed(&routing)?.get(name)
}

/// `this.parent.options.testnet ? 'testnet' : 'livenet'` inputs of the endpoint getters.
fn region_and_testnet(this: &Js) -> Result<(OkxRegion, bool), Thrown> {
    let parent = this.get("parent")?;
    let region = to_typed(&parent.get("region")?)?;
    Ok((region, parent.get("options")?.get("testnet")?.truthy()))
}

/// `jsonParse(data)`.
fn json_parse(data: &Js) -> Result<Js, Thrown> {
    Ok(safe_cex_exchange::js_runtime::value::from_retained_input(
        safe_cex_transport::how::json_parse(&text(data)?),
    ))
}

/// `error instanceof Error ? error : new Error(String(error))`.
fn as_error(value: &Js) -> Result<Js, Thrown> {
    Ok(match value {
        Js::Object(o) if o.class == "Error" || o.class == "AxiosError" => value.clone(),
        other => error_object("Error", &text(other)?),
    })
}

fn optional(v: &Js, key: &str) -> Result<Js, Thrown> {
    if v.is_nullish() {
        Ok(Js::Undefined)
    } else {
        v.get(key)
    }
}

fn topic(t: &SubscriptionTopic<Js>) -> Result<Js, Thrown> {
    Ok(match t {
        SubscriptionTopic::Account => Js::object(vec![("channel", Js::str("account"))]),
        SubscriptionTopic::Product {
            channel,
            instrument_type,
        } => Js::object(vec![
            ("channel", Js::str(channel)),
            ("instType", from_typed(instrument_type)?),
        ]),
        SubscriptionTopic::Existing(value) => (**value).clone(),
    })
}

/// `this.sendWithLog(JSON.stringify({ op, args }))`.
fn send_frame(s: &OkxScope, op: &str, topics: &[SubscriptionTopic<Js>]) -> Result<(), Thrown> {
    let args = topics.iter().map(topic).collect::<Result<Vec<_>, _>>()?;
    let frame = Js::object(vec![("op", Js::str(op)), ("args", Js::array(args))]);
    let wire = s
        .global("JSON.stringify")?
        .call(Js::Undefined, vec![frame])?;
    unit(s.this.call_method("sendWithLog", vec![wire]))
}

fn instrument_types(this: &Js) -> Result<Vec<OkxInstrumentType>, Thrown> {
    to_typed(
        &this
            .get("parent")?
            .call_method("getInstrumentTypes", vec![])?,
    )
}

/// The connectAndSubscribe `call` capability shared by all three classes.
fn connect_call(
    s: &OkxScope,
    resolver: &dyn Fn() -> Result<Js, Thrown>,
    op: &str,
    args: &[&Js],
    literal: Option<&str>,
) -> Result<Js, Thrown> {
    let this = &s.this;
    match op {
        "capture-resolve" => resolver(),
        "relay-enabled" => Ok(Js::Bool(safe_cex_transport::ws_relay::is_ws_relay_enabled())),
        "resolve" => args[0].call(
            Js::Undefined,
            args[1..].iter().map(|v| (*v).clone()).collect(),
        ),
        "capture-info" => this.get("parent")?.get("emitter"),
        "info" => args[0].call_method(
            "emit",
            vec![
                Js::str("grafana-info"),
                Js::str(literal.unwrap_or_default()),
            ],
        ),
        "create" => this.call_method("createWebsocket", vec![args[0].clone()]),
        "set-ws" => this.set("ws", args[0].clone()).map(|_| Js::Undefined),
        "listen" => {
            let event = literal.unwrap_or_default();
            let member = match event {
                "open" => "onOpen",
                "message" => "onMessage",
                _ => "onClose",
            };
            this.get("ws")?
                .call_method("addEventListener", vec![Js::str(event), this.get(member)?])
        }
        other => Err(Thrown::error(
            "Error",
            &format!("unknown OKX connect capability {other}"),
        )),
    }
}

fn private_connect(s: &OkxScope, business: bool) -> Result<(), Thrown> {
    let this = s.this.clone();
    let resolver = || bound_member(&this.get("directFallback")?, "resolveUrl");
    let disposed = || Ok(this.get("isDisposed")?.truthy());
    let route = |name: &str| route(s, name);
    let read = |key: &str| this.get(key);
    let get = |v: &Js, key: &str| v.get(key);
    let truthy = |v: &Js| v.truthy();
    let equal = |a: &Js, b: &Js| a.strict_equals(b);
    let call = |op: &str, args: &[&Js], literal: Option<&str>| {
        connect_call(s, &resolver, op, args, literal)
    };
    if business {
        startup::connect_business(
            disposed,
            route,
            read,
            get,
            truthy,
            equal,
            || Js::Undefined,
            call,
        )
    } else {
        startup::connect_private(
            disposed,
            route,
            read,
            get,
            truthy,
            equal,
            || Js::Undefined,
            call,
        )
    }
}

async fn private_open(s: OkxScope, business: bool) -> Result<Js, Thrown> {
    let this = s.this.clone();
    let disposed = || Ok(this.get("isDisposed")?.truthy());
    let auth = || {
        Ok(await_js(
            &s.exchange_runtime(),
            this.call_method("auth", vec![])?,
        ))
    };
    let emit_error =
        |e: &Thrown| unit(this.call_method("emitAndLogError", vec![as_error(&e.value)?]));
    let invoke = |name: &str| {
        unit(match name {
            "disregardSilence" => this
                .get("directFallback")?
                .call_method("disregardSilence", vec![]),
            "private-snapshot" => this
                .get("parent")?
                .call_method("fetchAndSetPrivateSnapshot", vec![]),
            member => this.call_method(member, vec![]),
        })
    };
    let reconnect_enabled = || Ok(ENABLE_RECONNECTING_WEBSOCKET);
    if business {
        startup::on_open_business(
            disposed,
            auth,
            Js::truthy,
            emit_error,
            invoke,
            reconnect_enabled,
        )
        .await?;
    } else {
        startup::on_open_private(
            disposed,
            auth,
            Js::truthy,
            emit_error,
            invoke,
            reconnect_enabled,
        )
        .await?;
    }
    Ok(Js::Undefined)
}

async fn private_auth(s: OkxScope, business: bool) -> Result<Js, Thrown> {
    let this = s.this.clone();
    let parent = this.get("parent")?;
    let options = parent.get("options")?;
    let read = |key: &str| match key {
        "timestamp" => s
            .global("virtualClock.getCurrentTime")?
            .call(Js::Undefined, vec![])?
            .call_method("unix", vec![]),
        "remoteSigner" => options.get("remoteSigner"),
        "nanoid" => s.global("nanoid")?.call(Js::Undefined, vec![]),
        "name" => parent.get("name"),
        "remote-account" => options.get("remoteSigner")?.get("accountName"),
        other => options.get(other),
    };
    let call = |op: &str, args: &[&Js], _: Option<&str>| match op {
        "capture-sign" => bound_member(&options.get("remoteSigner")?, "sign"),
        "capture-error" => bound_member(&this, "emitAndLogError"),
        "capture-send" => bound_member(&this, "sendWithLog"),
        "capture-json" => s.global("JSON.stringify"),
        "emit-error" | "stringify" | "send" => args[0].call(Js::Undefined, vec![args[1].clone()]),
        "hmac" => {
            let mac = s.global("createHmac")?.call(
                Js::Undefined,
                vec![Js::str("sha256"), options.get("secret")?],
            )?;
            let parts = Js::array(vec![
                args[0].clone(),
                Js::str("GET"),
                Js::str("/users/self/verify"),
            ]);
            mac.call_method(
                "update",
                vec![parts.call_method("join", vec![Js::str("")])?],
            )?
            .call_method("digest", vec![Js::str("base64")])
        }
        other => Err(Thrown::error(
            "Error",
            &format!("unknown OKX auth capability {other}"),
        )),
    };
    let sign = |signer: Js, request: Js| {
        Ok(await_js(
            &s.exchange_runtime(),
            signer.call(Js::Undefined, vec![request])?,
        ))
    };
    let get = |v: &Js, key: &str| v.get(key);
    let template = |v: &Js| units(v);
    let string = |u: Vec<u16>| Js::utf16(&u);
    let authed = if business {
        startup::auth_business(
            read,
            get,
            Js::truthy,
            call,
            sign,
            template,
            Js::str,
            string,
            Js::object,
            Js::array,
        )
        .await?
    } else {
        startup::auth_private(
            read,
            get,
            Js::truthy,
            call,
            sign,
            template,
            Js::str,
            string,
            Js::object,
            Js::array,
        )
        .await?
    };
    Ok(Js::Bool(authed))
}

fn private_subscribe(s: &OkxScope, business: bool) -> Result<(), Thrown> {
    let this = &s.this;
    let send = |op: &str, topics: &[SubscriptionTopic<Js>]| send_frame(s, op, topics);
    if !business {
        return public_lifecycle::subscribe_private(|| instrument_types(this), send);
    }
    // flatten(Object.values(this.topics))
    let flattened = || {
        let values = s
            .global("Object.values")?
            .call(Js::Undefined, vec![this.get("topics")?])?;
        let flat = s.global("flatten")?.call(Js::Undefined, vec![values])?;
        Ok(items(&flat)?.into_iter().map(Rc::new).collect())
    };
    public_lifecycle::subscribe_business(|| instrument_types(this), flattened, send)
}

/// Shared onMessage effects of the private and business classes.
fn private_effect(this: &Js, data: &Js, kind: &str, json: Option<&Rc<Js>>) -> Result<(), Thrown> {
    let json = || json.map(|j| (**j).clone()).unwrap_or(Js::Undefined);
    unit(match kind {
        "track" => this.call_method("trackLastReceived", vec![]),
        "fallback" => this.get("directFallback")?.call_method("onMessage", vec![]),
        "diagnostic" => this.call_method("emitWsDiagnosticReceive", vec![data.clone()]),
        "pong" => this.call_method("handlePongEvent", vec![]),
        "subscribe" => this.call_method("subscribe", vec![]),
        "emit-error" => this
            .get("parent")?
            .call_method("emitAndLogError", vec![json().get("msg")?]),
        "order" => this.call_method("handleOrderTopic", vec![json()]),
        "position" => this.call_method("handlePositionTopic", vec![json()]),
        "account" => this.call_method("handleAccountTopic", vec![json()]),
        other => Err(Thrown::error(
            "Error",
            &format!("unknown OKX message effect {other}"),
        )),
    })
}

/// `+json?.code === 0`
fn code_zero(json: &Js) -> Result<bool, Thrown> {
    Ok(optional(json, "code")?.to_number()? == 0.0)
}

fn private_message(s: &OkxScope, event: &Js) -> Result<(), Thrown> {
    let this = &s.this;
    let data = event.get("data")?;
    let disposed = Cell::new(this.get("isDisposed")?.truthy());
    let is_pong = data.strict_equals(&Js::str("pong"));
    message_routing::private_message(
        &disposed,
        is_pong,
        |kind, json| private_effect(this, &data, kind, json),
        |needle| {
            Ok(data
                .call_method("includes", vec![Js::str(needle)])?
                .truthy())
        },
        || Ok(Rc::new(json_parse(&data)?)),
        |json, kind| {
            let json = &**json;
            Ok(match kind {
                "optional-code-zero" => code_zero(json)?,
                "message-truthy" => json.truthy() && json.get("msg")?.truthy(),
                "data-truthy" => json.truthy() && json.get("data")?.truthy(),
                _ => json.truthy(),
            })
        },
    )
}

/// TIMEFRAME_BY_CANDLE_CHANNEL[channel]
fn candle_timeframe(channel: &Js) -> Result<Option<String>, Thrown> {
    let channel = text(channel)?;
    Ok(types::INTERVAL
        .iter()
        .find(|(_, interval)| format!("candle{interval}") == channel)
        .map(|(timeframe, _)| (*timeframe).to_owned()))
}

fn business_message(s: &OkxScope, event: &Js) -> Result<(), Thrown> {
    let this = &s.this;
    let data = event.get("data")?;
    let disposed = Cell::new(this.get("isDisposed")?.truthy());
    let is_pong = data.strict_equals(&Js::str("pong"));
    let effect = |kind: &str, json: Option<&Rc<Js>>, key: Option<&[u16]>, value: Option<&Js>| {
        let json_value = || json.map(|j| (**j).clone()).unwrap_or(Js::Undefined);
        let key = || key.map(Js::utf16).unwrap_or(Js::Undefined);
        match kind {
            "stats" => unit(this.call_method("recordMessageHandlerStats", vec![key()])),
            "dispatch" => {
                unit(this.call_method("dispatchMessageHandler", vec![key(), json_value()]))
            }
            "dispatch-value" => unit(this.call_method(
                "dispatchMessageHandler",
                vec![value.cloned().unwrap_or(Js::Undefined), json_value()],
            )),
            other => private_effect(this, &data, other, json),
        }
    };
    let parse = || {
        let json = json_parse(&data)?;
        Ok(json.truthy().then(|| Rc::new(json)))
    };
    let predicate = |json: &Rc<Js>, kind: &str| {
        Ok(match kind {
            "event:subscribe" => json.get("event")?.strict_equals(&Js::str("subscribe")),
            "event:login" => json.get("event")?.strict_equals(&Js::str("login")),
            _ => json.get("code")?.to_number()? == 0.0,
        })
    };
    // typeof json.arg?.[name] === 'string' ? json.arg[name] : ''
    let read_arg = |json: &Rc<Js>, name: &str| {
        let value = optional(&json.get("arg")?, name)?;
        Ok(if value.is_string() {
            value
        } else {
            Js::str("")
        })
    };
    let has_order_channel = |channel: &Js| {
        startup::create_order_channels(|names| {
            Ok::<_, Thrown>(names.iter().any(|n| channel.strict_equals(&Js::str(n))))
        })
    };
    message_routing::business_message(
        &disposed,
        is_pong,
        effect,
        parse,
        predicate,
        read_arg,
        Js::truthy,
        has_order_channel,
        candle_timeframe,
        units,
        |t: String| Ok(t.encode_utf16().collect()),
    )
}

fn business_dispatch(s: &OkxScope, handler: Js, json: Js) -> Result<(), Thrown> {
    let this = &s.this;
    message_routing::dispatch_message_handler(
        &handler,
        &Rc::new(json),
        |h| {
            let callbacks = this.get("messageHandlers")?.get_key(&h.to_js_string()?)?;
            Ok((!callbacks.is_nullish()).then(|| Rc::new(callbacks)))
        },
        |callbacks| Ok(callbacks.get("length")?.truthy()),
        |h| unit(this.call_method("recordMessageHandlerStats", vec![h.clone()])),
        |callbacks, json| {
            for callback in items(&callbacks)? {
                callback.call(Js::Undefined, vec![(**json).clone()])?;
            }
            Ok(())
        },
    )
}

/// Members the private class declares and the business class inherits.
fn account_members(s: &OkxScope) {
    sync_member(s, "parseFinite", |_, a| {
        let value = arg(&a, 0);
        let value = if value.is_undefined() {
            None
        } else {
            Some(to_json(&value)?)
        };
        Ok(Js::Number(
            finite::parse_finite(value.as_ref()).map_err(coercion)?,
        ))
    });
    sync_member(s, "sumFinite", |_, a| {
        let values = items(&arg(&a, 0))?
            .iter()
            .map(to_json)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Js::Number(finite::sum_finite(&values).map_err(coercion)?))
    });
    sync_member(s, "handleOrderTopic", |s, a| {
        events::handle_order_topic(socket_ops(s), arg(&a, 0))
    });
    sync_member(s, "isDuplicateFillEvent", |s, a| {
        events::is_duplicate_fill_event(socket_ops(s), arg(&a, 0))
    });
    sync_member(s, "handlePositionTopic", |s, a| {
        events::handle_position_topic(socket_ops(s), arg(&a, 0))
    });
    sync_member(s, "handleAccountTopic", |s, a| {
        events::handle_account_topic(socket_ops(s), arg(&a, 0))
    });
}

/// The members OKXPrivateWebsocket declares and OKXBusinessWebsocket overrides.
fn session_members(s: &OkxScope, business: bool) -> Result<(), Thrown> {
    let endpoint: Getter = Rc::new(move |this| {
        let (region, testnet) = region_and_testnet(this)?;
        Ok(Js::str(if business {
            family_policy::get_business_websocket_endpoint(region, Some(testnet))
        } else {
            family_policy::get_private_websocket_endpoint(region, Some(testnet))
        }))
    });
    s.this.define_accessor("endpoint", Some(endpoint), None)?;
    sync_member(s, "connectAndSubscribe", move |s, _| {
        private_connect(s, business).map(|_| Js::Undefined)
    });
    let opener = s.clone();
    define(
        s,
        "onOpen",
        Js::function("onOpen", move |_, _| {
            Ok(opener.spawn(Box::pin(private_open(opener.clone(), business))))
        }),
    );
    sync_member(s, "ping", move |s, _| {
        let socket = core(s)?;
        if business {
            logged_socket::business_ping(&socket)?;
        } else {
            logged_socket::private_ping(&socket)?;
        }
        Ok(Js::Undefined)
    });
    let authenticator = s.clone();
    define(
        s,
        "auth",
        Js::function("auth", move |_, _| {
            Ok(authenticator.spawn(Box::pin(private_auth(authenticator.clone(), business))))
        }),
    );
    sync_member(s, "subscribe", move |s, _| {
        private_subscribe(s, business).map(|_| Js::Undefined)
    });
    sync_member(s, "onMessage", move |s, a| {
        if business {
            business_message(s, &arg(&a, 0))?;
        } else {
            private_message(s, &arg(&a, 0))?;
        }
        Ok(Js::Undefined)
    });
    Ok(())
}

fn define_fields(this: &Js, fields: Vec<(&str, Js)>) -> Result<(), Thrown> {
    for (k, v) in fields {
        this.define(k, v)?;
    }
    Ok(())
}

fn private_fields(this: &Js, state: PrivateSocketState<Js>) -> Result<(), Thrown> {
    define_fields(
        this,
        vec![
            ("recentFillKeys", state.recent_fill_keys),
            ("directFallback", state.direct_fallback),
            ("fillDedupeWindowMs", state.fill_dedupe_window_ms),
        ],
    )
}

fn new_private(s: &OkxScope) -> Result<(), Thrown> {
    private_fields(&s.this, PrivateSocketState::initialize(&socket_ops(s))?)?;
    session_members(s, false)?;
    account_members(s);
    Ok(())
}

fn new_business(s: &OkxScope) -> Result<(), Thrown> {
    let state = BusinessSocketState::initialize(&socket_ops(s))?;
    private_fields(&s.this, state.base)?;
    s.this.define("topics", state.topics)?;
    let resyncs: Getter = Rc::new(|_| {
        Ok(Js::Bool(
            family_policy::business_resyncs_private_state_on_reopen(),
        ))
    });
    s.this
        .define_accessor("resyncsPrivateStateOnReopen", Some(resyncs), None)?;
    session_members(s, true)?;
    sync_member(s, "dispatchMessageHandler", |s, a| {
        business_dispatch(s, arg(&a, 0), arg(&a, 1)).map(|_| Js::Undefined)
    });
    sync_member(s, "listenOHLCV", |s, a| {
        history_stream_api::listen_ohlcv(runtime_ops(s), arg(&a, 0), arg(&a, 1))
    });
    account_members(s);
    Ok(())
}

/// Public order-book state lives in the JS fields; the typed bodies borrow a copy.
struct BookState {
    sequences: RefCell<HashMap<String, String>>,
    recovering: RefCell<HashSet<String>>,
    topics: RefCell<HashMap<String, Vec<(String, String)>>>,
}

fn load_state(this: &Js) -> Result<BookState, Thrown> {
    let mut sequences = HashMap::new();
    if let Js::Map(map) = this.get("orderBookSequenceByHandler")? {
        for (k, v) in map.borrow().iter() {
            sequences.insert(text(k)?, text(v)?);
        }
    }
    let mut recovering = HashSet::new();
    if let Js::Set(set) = this.get("recoveringOrderBooks")? {
        for v in set.borrow().iter() {
            recovering.insert(text(v)?);
        }
    }
    let mut topics = HashMap::new();
    let js_topics = this.get("topics")?;
    if !js_topics.is_nullish() {
        for (k, list) in js_topics.own_entries()? {
            let mut pairs = vec![];
            for t in list.array_values().unwrap_or_default() {
                pairs.push((text(&t.get("channel")?)?, text(&t.get("instId")?)?));
            }
            topics.insert(String::from_utf16_lossy(&k), pairs);
        }
    }
    Ok(BookState {
        sequences: RefCell::new(sequences),
        recovering: RefCell::new(recovering),
        topics: RefCell::new(topics),
    })
}

/// Write the typed copy back, keeping surviving entries in their JS order.
fn store_state(this: &Js, state: &BookState) -> Result<(), Thrown> {
    if let Js::Map(map) = this.get("orderBookSequenceByHandler")? {
        let typed = state.sequences.borrow();
        let mut next: Vec<(Js, Js)> = vec![];
        let mut seen = HashSet::new();
        for (k, _) in map.borrow().iter() {
            let key = text(k)?;
            if let Some(v) = typed.get(&key) {
                next.push((k.clone(), Js::str(v)));
                seen.insert(key);
            }
        }
        for (k, v) in typed.iter().filter(|(k, _)| !seen.contains(*k)) {
            next.push((Js::str(k), Js::str(v)));
        }
        let mut slots = map.borrow_mut();
        slots.retain(|(key, _)| next.iter().any(|(next, _)| next.same_value_zero(key)));
        for (key, value) in next {
            if let Some(slot) = slots
                .iter_mut()
                .find(|(existing, _)| existing.same_value_zero(&key))
            {
                slot.1 = value;
            } else {
                slots.push((key, value));
            }
        }
    }
    if let Js::Set(set) = this.get("recoveringOrderBooks")? {
        let typed = state.recovering.borrow();
        let mut next: Vec<Js> = vec![];
        let mut seen = HashSet::new();
        for v in set.borrow().iter() {
            let key = text(v)?;
            if typed.contains(&key) {
                next.push(v.clone());
                seen.insert(key);
            }
        }
        next.extend(
            typed
                .iter()
                .filter(|k| !seen.contains(*k))
                .map(|k| Js::str(k)),
        );
        let mut slots = set.borrow_mut();
        slots.retain(|key| next.iter().any(|next| next.same_value_zero(key)));
        for value in next {
            if !slots
                .iter()
                .any(|existing| existing.same_value_zero(&value))
            {
                slots.push(value);
            }
        }
    }
    Ok(())
}

/// The core dispatch reads its own handler registry; mirror `this.messageHandlers` into it.
fn sync_handlers(this: &Js, socket: &BaseWebSocket) -> Result<(), Thrown> {
    let handlers = this.get("messageHandlers")?;
    let mut registry = socket.message_handlers.borrow_mut();
    registry.clear();
    for (key, list) in handlers.own_entries()? {
        let Some(callbacks) = list.array_values() else {
            continue;
        };
        let callbacks: Vec<MessageHandler> = callbacks
            .into_iter()
            .map(|callback| {
                Rc::new(move |value: &Js| {
                    callback
                        .call(Js::Undefined, vec![value.clone()])
                        .map(|_| ())
                        .map_err(|e| e.to_exchange_error())
                }) as MessageHandler
            })
            .collect();
        registry.insert(
            String::from_utf16_lossy(&key),
            Rc::new(RefCell::new(callbacks)),
        );
    }
    Ok(())
}

/// Run a typed order-book body over the loaded state, then write the state back.
fn with_book<T>(
    s: &OkxScope,
    body: impl FnOnce(&Rc<BaseWebSocket>, &BookState) -> Result<T, ExchangeError>,
) -> Result<T, Thrown> {
    let socket = core(s)?;
    sync_handlers(&s.this, &socket)?;
    let state = load_state(&s.this)?;
    let result = body(&socket, &state);
    store_state(&s.this, &state)?;
    result.map_err(Thrown::from)
}

fn recover(
    socket: &Rc<BaseWebSocket>,
    state: &BookState,
    handler: &str,
) -> Result<(), ExchangeError> {
    public_socket::recover_order_book(
        socket,
        handler,
        &state.sequences,
        &state.recovering,
        &state.topics,
    )
}

fn public_connect(s: &OkxScope) -> Result<(), Thrown> {
    let this = s.this.clone();
    let core_scope = s.clone();
    // The typed onMessage counts traffic on the core's fallback chain, so URLs resolve there too.
    let resolver = move || {
        let scope = core_scope.clone();
        Ok(Js::function("resolveUrl", move |_, a| {
            let socket = core(&scope)?;
            let primary = text(&arg(&a, 0))?;
            let fallbacks = a
                .iter()
                .skip(1)
                .map(|v| {
                    if v.is_string() {
                        text(v).map(Some)
                    } else {
                        Ok(None)
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            let refs: Vec<Option<&str>> = fallbacks.iter().map(Option::as_deref).collect();
            let url = socket
                .direct_fallback
                .borrow_mut()
                .resolve_url(&primary, &refs)
                .to_owned();
            Ok(Js::str(&url))
        }))
    };
    let direct_url = || {
        let parent = this.get("parent")?;
        let testnet = parent.get("options")?.get("testnet")?.truthy();
        s.global("BASE_WS_URLS")?
            .get_key(&parent.get("region")?.to_js_string()?)?
            .get("public")?
            .get(if testnet { "testnet" } else { "livenet" })
    };
    let map_and_assign = |channel: &str, f: Rc<dyn Fn(Js) -> Result<Js, Thrown>>| {
        let markets = this.get("store")?.get("markets")?;
        let mapped = items(&markets)?
            .into_iter()
            .map(|m| f(m))
            .collect::<Result<_, _>>()?;
        this.get("topics")?.set(channel, Js::array(mapped))
    };
    let parent = this.get("parent")?;
    let index_ticker_id: Rc<dyn Fn(&Js) -> Result<Js, Thrown>> =
        Rc::new(move |m| parent.call_method("getIndexTickerInstId", vec![m.clone()]));
    startup::connect_public(
        || Ok(this.get("isDisposed")?.truthy()),
        |name| route(s, name),
        direct_url,
        |v, key| v.get(key),
        |a, b| a.strict_equals(b),
        |op, args, literal| connect_call(s, &resolver, op, args, literal),
        map_and_assign,
        Rc::new(|m| m.get("id")),
        index_ticker_id,
        Rc::new(Js::str),
        Rc::new(Js::object),
    )
}

fn public_open(s: &OkxScope) -> Result<(), Thrown> {
    let this = &s.this;
    let disposed = Cell::new(this.get("isDisposed")?.truthy());
    let invoke = |action: &str| {
        let emitter = || this.get("parent")?.get("emitter");
        unit(match action {
            "orderBookSequenceByHandler.clear" | "recoveringOrderBooks.clear" => this
                .get(action.trim_end_matches(".clear"))?
                .call_method("clear", vec![]),
            "emitter.off:markets:changed" => emitter()?.call_method(
                "off",
                vec![Js::str("markets:changed"), this.get("onMarketsChanged")?],
            ),
            "emitter.on:markets:changed" => emitter()?.call_method(
                "on",
                vec![Js::str("markets:changed"), this.get("onMarketsChanged")?],
            ),
            member => this.call_method(member, vec![]),
        })
    };
    public_lifecycle::on_open_public(&disposed, invoke, || ENABLE_RECONNECTING_WEBSOCKET)
}

/// `this.parent.getMarketsForIndexTickerInstId(update.instId)` market IDs, read up front
/// because the typed body borrows them; a failure surfaces where the source would throw.
fn index_market_ids(this: &Js, data: &Value) -> Result<Vec<String>, ExchangeError> {
    let instrument = data
        .get("data")
        .and_then(|d| d.get(0))
        .and_then(|u| u.get("instId"))
        .map_or(Js::Undefined, from_json);
    let markets = this
        .get("parent")?
        .call_method("getMarketsForIndexTickerInstId", vec![instrument])?;
    Ok(items(&markets)?
        .iter()
        .map(|m| text(&m.get("id")?))
        .collect::<Result<_, _>>()?)
}

fn ticker_members(s: &OkxScope) {
    type Typed = fn(&dyn safe_cex_store::Store, &Value) -> Result<(), ExchangeError>;
    let plain: [(&'static str, Typed); 3] = [
        ("handleTickerEvents", public_socket::handle_ticker_events),
        (
            "handleOpenInterestEvents",
            public_socket::handle_open_interest_events,
        ),
        (
            "handleFundingRateEvents",
            public_socket::handle_funding_rate_events,
        ),
    ];
    for (name, body) in plain {
        sync_member(s, name, move |s, a| {
            body(&*s.shared.store, &to_json(&arg(&a, 0))?)?;
            Ok(Js::Undefined)
        });
    }
    sync_member(s, "handleMarkPriceEvents", |s, a| {
        let parent = s.this.get("parent")?;
        public_socket::handle_mark_price_events(&*s.shared.store, &to_json(&arg(&a, 0))?, |id| {
            let id = id.map_or(Js::Undefined, from_json);
            unit(parent.call_method("markPriceLoaded", vec![id])).map_err(|e| e.to_exchange_error())
        })?;
        Ok(Js::Undefined)
    });
    sync_member(s, "handleIndexTickerEvents", |s, a| {
        let data = to_json(&arg(&a, 0))?;
        let ids = index_market_ids(&s.this, &data);
        let parent = s.this.get("parent")?;
        public_socket::handle_index_ticker_events(
            &*s.shared.store,
            &data,
            |_| match &ids {
                Ok(ids) => Ok(ids.iter().map(String::as_str).collect()),
                Err(e) => Err(e.clone()),
            },
            |id| {
                unit(parent.call_method("indexPriceLoaded", vec![Js::str(id)]))
                    .map_err(|e| e.to_exchange_error())
            },
        )?;
        Ok(Js::Undefined)
    });
}

fn new_public(s: &OkxScope) -> Result<(), Thrown> {
    let state = PublicSocketState::initialize(socket_ops(s))?;
    define_fields(
        &s.this,
        vec![
            ("orderBookSequenceByHandler", state.order_book_sequences),
            ("recoveringOrderBooks", state.recovering_order_books),
            ("directFallback", state.direct_fallback),
            ("topics", state.topics),
            ("messageHandlers", state.message_handlers),
        ],
    )?;
    sync_member(s, "connectAndSubscribe", |s, _| {
        public_connect(s).map(|_| Js::Undefined)
    });
    sync_member(s, "onOpen", |s, _| public_open(s).map(|_| Js::Undefined));
    sync_member(s, "onMarketsChanged", |s, a| {
        events::on_markets_changed(socket_ops(s), arg(&a, 0))
    });
    sync_member(s, "ping", |s, _| {
        public_socket::ping(&core(s)?)?;
        Ok(Js::Undefined)
    });
    sync_member(s, "subscribe", |s, _| {
        let topics = match to_json(&s.this.get("topics")?)? {
            Value::Object(map) => map,
            _ => serde_json::Map::new(),
        };
        public_socket::subscribe(&core(s)?, &topics)?;
        Ok(Js::Undefined)
    });
    sync_member(s, "onMessage", |s, a| {
        let data = arg(&a, 0).get("data")?;
        let frame = SocketFrame::Text(text(&data)?);
        with_book(s, |socket, st| {
            public_socket::on_message(socket, &frame, &st.sequences, &st.recovering, &st.topics)
        })?;
        Ok(Js::Undefined)
    });
    sync_member(s, "dispatchMessageHandler", |s, a| {
        let handler = text(&arg(&a, 0))?;
        let json = arg(&a, 1);
        with_book(s, |socket, st| {
            let recover = |h: &str| recover(socket, st, h);
            public_socket::dispatch_message_handler(
                socket,
                &handler,
                &json,
                &st.sequences,
                &st.recovering,
                &recover,
            )
        })?;
        Ok(Js::Undefined)
    });
    sync_member(s, "acceptOrderBookSequence", |s, a| {
        let handler = text(&arg(&a, 0))?;
        let json = arg(&a, 1);
        let accepted = with_book(s, |socket, st| {
            let recover = |h: &str| recover(socket, st, h);
            public_socket::accept_order_book_sequence(
                &handler,
                &json,
                &st.sequences,
                &st.recovering,
                &recover,
            )
        })?;
        Ok(Js::Bool(accepted))
    });
    sync_member(s, "recoverOrderBook", |s, a| {
        let handler = text(&arg(&a, 0))?;
        with_book(s, |socket, st| recover(socket, st, &handler))?;
        Ok(Js::Undefined)
    });
    ticker_members(s);
    sync_member(s, "listenOrderBook", |s, a| {
        history_stream_api::listen_order_book(runtime_ops(s), arg(&a, 0), arg(&a, 1))
    });
    Ok(())
}

/// The three socket classes as module-scope constructors taking the exchange.
pub(crate) fn register(g: &Js, weak: Weak<OkxShared>) {
    let classes: [(&str, fn(&OkxScope) -> Result<(), Thrown>); 3] = [
        ("OKXPublicWebsocket", new_public),
        ("OKXPrivateWebsocket", new_private),
        ("OKXBusinessWebsocket", new_business),
    ];
    for (name, init) in classes {
        let w = weak.clone();
        let _ = g.define(
            name,
            Js::constructor(name, None, move |_, a| {
                let scope = scope_of(&w)?;
                let socket = base_websocket(&scope.shared.sockets, arg(&a, 0));
                let s = scope.with_this(socket.clone());
                init(&s)?;
                Ok(socket)
            }),
        );
    }
}
