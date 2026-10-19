//! Socket adapters; shared BaseWebSocket owns dial, reconnect and disposal.
use super::values as v;
use super::*;
use crate::{
    classes::ClassKind,
    subscriptions::{
        self as sub, WeexSubscriptionMethod, WeexSubscriptionRuntime, WeexSubscriptionState,
    },
    ws_control::{self as control, WeexWebsocketControlRuntime, WeexWebsocketControlState},
};
use safe_cex_exchange::js_runtime::sockets;
#[path = "socket_private.rs"]
mod private;
#[path = "socket_public.rs"]
mod public;

pub(super) fn base(h: &Host, kind: ClassKind, parent: Js) -> Result<Js, Thrown> {
    let socket = sockets::base_websocket(&h.0.shared.sockets, parent);
    if let Some((_, prototype)) =
        h.0.shared
            .extra
            .prototypes
            .borrow()
            .iter()
            .find(|(k, _)| *k == kind)
        && let Js::Object(object) = &socket
    {
        *object.proto.borrow_mut() = Some(prototype.clone());
    }
    Ok(socket)
}
fn base_dispose(h: &Host) -> Result<(), Thrown> {
    sockets::base_websocket_class(&h.0.shared.sockets)
        .get("prototype")?
        .get("dispose")?
        .call(h.this(), std::vec![])?;
    Ok(())
}
fn disposed(h: &Host) -> Result<bool, Thrown> {
    Ok(h.root("isDisposed")?.truthy())
}
fn parent(h: &Host) -> Result<Js, Thrown> {
    h.root("parent")
}
fn parent_call(h: &Host, name: &str, args: Vec<Js>) -> Result<Js, Thrown> {
    {
        let mut names = name.split('.').peekable();
        let mut receiver = parent(h)?;
        while let Some(name) = names.next() {
            if names.peek().is_none() {
                return receiver.call_method(name, args);
            }
            receiver = receiver.get(name)?;
        }
        std::unreachable!()
    }
}
fn warn(h: &Host, message: Js) -> Result<(), Thrown> {
    parent_call(h, "log", std::vec![message, Js::str("warning")])?;
    Ok(())
}
fn call_unit(h: &Host, name: &str, args: Vec<Js>) -> Result<(), Thrown> {
    h.call(name, args)?;
    Ok(())
}
fn parse(h: &Host, units: &[u16]) -> Result<Js, Thrown> {
    let _ = h;
    Ok(safe_cex_exchange::js_runtime::value::from_retained_input(
        safe_cex_transport::how::big_int_as_string_json_parse_utf16(units),
    ))
}
fn serialize(h: &Host, value: Js) -> Result<Js, Thrown> {
    h.global("JSON.stringify")?
        .call(Js::Undefined, std::vec![value])
}
fn frame(h: &Host, method: &str, topics: Option<Js>, id: f64, logged: bool) -> Result<(), Thrown> {
    let payload = Js::object(std::vec![
        ("method", Js::str(method)),
        ("id", Js::Number(id))
    ]);
    if let Some(topics) = topics {
        payload.define("params", topics)?;
    }
    call_unit(
        h,
        if logged { "sendWithLog" } else { "send" },
        std::vec![serialize(h, payload)?],
    )
}
fn strings(value: Js) -> Result<Vec<Vec<u16>>, Thrown> {
    let values = if value.is_array() {
        value.array_values()?.unwrap()
    } else {
        value
            .call_method("values", std::vec![])?
            .iterable_values()?
    };
    values.iter().map(v::units).collect()
}
fn desired(h: &Host) -> Result<Js, Thrown> {
    let markets = h
        .root("store.markets")?
        .array_values()?
        .ok_or_else(|| Thrown::type_error("markets is not iterable"))?;
    let available = markets
        .iter()
        .map(|market| v::units(&market.get("symbol")?))
        .collect::<Result<Vec<_>, _>>()?;
    let active =
        h.0.base()
            .get_active_ticker_symbols()
            .iter()
            .map(|symbol| symbol.encode_utf16().collect())
            .collect::<Vec<_>>();
    let topics = h.root("topics")?.get("size")?.to_number()?;
    Ok(Js::array(
        crate::selection::get_desired_ticker_topics(&available, &active, topics as usize)
            .iter()
            .map(|s| Js::utf16(s))
            .collect(),
    ))
}
fn subscriptions(h: &Host) -> Result<WeexSubscriptionState, Thrown> {
    Ok(WeexSubscriptionState {
        topics: strings(h.root("topics")?)?,
        ticker_topics: strings(h.root("tickerTopics")?)?,
        is_connected: h.root("isConnected")?.truthy(),
        is_disposed: disposed(h)?,
        operation_timestamps: h
            .root("subscriptionOperationTimestamps")?
            .array_values()?
            .ok_or_else(|| Thrown::type_error("operation times is not iterable"))?
            .iter()
            .map(Js::to_number)
            .collect::<Result<_, _>>()?,
        has_logged_limit: h.root("hasLoggedSubscriptionLimit")?.truthy(),
        subscription_id: h.root("subscriptionId")?.to_number()?,
    })
}
fn save_subscriptions(h: &Host, state: &WeexSubscriptionState) -> Result<(), Thrown> {
    for (name, values) in [
        ("topics", &state.topics),
        ("tickerTopics", &state.ticker_topics),
    ] {
        let set = h.root(name)?;
        set.call_method("clear", std::vec![])?;
        for value in values {
            set.call_method("add", std::vec![Js::utf16(value)])?;
        }
    }
    h.this().set(
        "subscriptionOperationTimestamps",
        Js::array(
            state
                .operation_timestamps
                .iter()
                .copied()
                .map(Js::Number)
                .collect(),
        ),
    )?;
    h.this().set(
        "hasLoggedSubscriptionLimit",
        Js::Bool(state.has_logged_limit),
    )?;
    h.this()
        .set("subscriptionId", Js::Number(state.subscription_id))
}
fn subscription_runtime() -> WeexSubscriptionRuntime<Host, Thrown> {
    WeexSubscriptionRuntime {
        now: |h, s| {
            save_subscriptions(h, s)?;
            let now = h.0.shared.runtime.now_ms().floor();
            *s = subscriptions(h)?;
            Ok(now)
        },
        desired_topics: |h, s| {
            save_subscriptions(h, s)?;
            let desired = strings(h.call("getDesiredTickerTopics", std::vec![])?)?;
            *s = subscriptions(h)?;
            Ok(desired)
        },
        log_warning: |h, s, message| {
            save_subscriptions(h, s)?;
            warn(h, Js::str(message))?;
            *s = subscriptions(h)?;
            Ok(())
        },
        send: |h, s, method, topics, id| {
            save_subscriptions(h, s)?;
            frame(
                h,
                if method == WeexSubscriptionMethod::Subscribe {
                    "SUBSCRIBE"
                } else {
                    "UNSUBSCRIBE"
                },
                Some(Js::array(topics.iter().map(|s| Js::utf16(s)).collect())),
                id,
                false,
            )?;
            *s = subscriptions(h)?;
            Ok(())
        },
    }
}
fn control_state(h: &Host) -> Result<WeexWebsocketControlState, Thrown> {
    Ok(WeexWebsocketControlState {
        is_disposed: disposed(h)?,
        subscription_id: h.root("subscriptionId")?.to_number()?,
        ping_at: h.root("pingAt")?.to_number()?,
        has_logged_unsupported: h.root("hasLoggedUnsupportedTransport")?.truthy(),
    })
}
fn save_control(h: &Host, s: &WeexWebsocketControlState) -> Result<(), Thrown> {
    h.this()
        .set("subscriptionId", Js::Number(s.subscription_id))?;
    h.this().set("pingAt", Js::Number(s.ping_at))?;
    h.this().set(
        "hasLoggedUnsupportedTransport",
        Js::Bool(s.has_logged_unsupported),
    )
}
fn control_runtime() -> WeexWebsocketControlRuntime<Host, Thrown> {
    WeexWebsocketControlRuntime {
        performance_now: |h, s| {
            save_control(h, s)?;
            Ok(h.0.shared.runtime.monotonic_now_ms())
        },
        send_with_log: |h, s, method, topics, id| {
            save_control(h, s)?;
            frame(
                h,
                method,
                topics.map(|topics| Js::array(topics.iter().map(|s| Js::str(s)).collect())),
                id,
                true,
            )?;
            *s = control_state(h)?;
            Ok(())
        },
        snapshot: |h, s, tries| {
            save_control(h, s)?;
            parent_call(
                h,
                "fetchAndSetPrivateSnapshot",
                std::vec![Js::Number(tries)],
            )?;
            *s = control_state(h)?;
            Ok(())
        },
        emit_reconnected: |h, s| {
            save_control(h, s)?;
            call_unit(h, "emitWsReconnected", std::vec![])?;
            *s = control_state(h)?;
            Ok(())
        },
        log_warning: |h, s, message| {
            save_control(h, s)?;
            warn(h, Js::utf16(message))?;
            *s = control_state(h)?;
            Ok(())
        },
    }
}
fn handlers() -> crate::message_handlers::WeexMessageHandlerRuntime<Host, Js, Js, Js, Thrown> {
    crate::message_handlers::WeexMessageHandlerRuntime {
        registry: |h| h.root("messageHandlers"),
        get_property: |_, o, k| o.get_key(k),
        set_property: |_, o, k, v| o.set_key(k, v),
        truthy: Js::truthy,
        nullish: Js::is_nullish,
        array: |_, v| Ok(Js::array(v)),
        spread: |_, v| {
            v.array_values()?
                .ok_or_else(|| Thrown::type_error("handlers is not iterable"))
        },
        filter: |_, array, callback| v::filter(array, |value, _| Ok(callback(&value))),
        strict_equal: Js::strict_equals,
        iterator: |h, value| v::iterator(h, value),
        next: |h, value| v::next(h, value),
        close_on_throw: |_, _, error| error,
        invoke: |_, handler, message| {
            handler.call(Js::Undefined, std::vec![message.clone()])?;
            Ok(())
        },
        cleanup_stats: |h, key| {
            call_unit(h, "cleanupMessageHandlerStats", std::vec![Js::utf16(key)])
        },
    }
}
pub(super) fn dispatch(
    mut h: Host,
    kind: ClassKind,
    name: &'static str,
    args: Vec<Js>,
) -> Result<Js, Thrown> {
    let first = arg(&args, 0);
    let second = arg(&args, 1);
    if kind == ClassKind::Private {
        return private::dispatch(h, name, args);
    }
    let feed = public::runtime();
    match name {
        "connectAndSubscribe" => crate::public_feed::connect_and_subscribe(&h, &feed)?,
        "onMessage" => crate::public_feed::on_message(&h, &feed, first)?,
        "listenOHLCV" => return crate::public_feed::listen_ohlcv(&h, &feed, first, second),
        "listenOrderBook" => {
            return crate::public_feed::listen_order_book(&h, &feed, first, second);
        }
        "handleTicker" => crate::public_feed::handle_ticker(&h, &feed, first)?,
        "handleDepth" => crate::public_feed::handle_depth(&h, &feed, first)?,
        "dispose" => crate::public_feed::dispose(&h, &feed)?,
        "getDesiredTickerTopics" => return desired(&h),
        "addHandler" => {
            crate::message_handlers::add_handler(&mut h, &v::units(&first)?, second, &handlers())?
        }
        "removeHandler" => crate::message_handlers::remove_handler(
            &mut h,
            &v::units(&first)?,
            &second,
            &handlers(),
        )?,
        "handleRoutedMessage" => crate::message_handlers::handle_routed_message(
            &mut h,
            &v::units(&first)?,
            &second,
            &handlers(),
        )?,
        "ping" => {
            let mut state = control_state(&h)?;
            let result = control::public_ping(&mut h, &mut state, &control_runtime());
            save_control(&h, &state)?;
            result?;
        }
        _ => {
            let mut state = subscriptions(&h)?;
            let runtime = subscription_runtime();
            let result = match name {
                "onOpen" => control::public_on_open(&mut h, &mut state, &runtime, |h, s| {
                    save_subscriptions(h, s)?;
                    call_unit(h, "emitWsReconnected", std::vec![])?;
                    *s = subscriptions(h)?;
                    Ok(())
                }),
                "subscribeTickerTopics" => {
                    sub::subscribe_ticker_topics(&mut h, &mut state, &runtime)
                }
                "handleActiveTickerSymbolsChanged" => {
                    sub::handle_active_ticker_symbols_changed(&mut h, &mut state, &runtime)
                }
                "reconcileTickerTopics" => {
                    sub::reconcile_ticker_topics(&mut h, &mut state, &runtime)
                }
                "resubscribeTopics" => sub::resubscribe_topics(&mut h, &mut state, &runtime),
                "subscribeTopic" => {
                    sub::subscribe_topic(&mut h, &mut state, v::units(&first)?, &runtime)
                }
                "unsubscribeTopic" => {
                    sub::unsubscribe_topic(&mut h, &mut state, &v::units(&first)?, &runtime)
                }
                "subscribeTopics" => sub::subscribe_topics(
                    &mut h,
                    &mut state,
                    &strings(first)?,
                    second.is_undefined() || second.truthy(),
                    &runtime,
                ),
                "sendSubscriptionOperation" => {
                    let method = if first.strict_equals(&Js::str("SUBSCRIBE")) {
                        WeexSubscriptionMethod::Subscribe
                    } else {
                        WeexSubscriptionMethod::Unsubscribe
                    };
                    let result = sub::send_subscription_operation(
                        &mut h,
                        &mut state,
                        method,
                        &strings(second)?,
                        &runtime,
                    );
                    save_subscriptions(&h, &state)?;
                    return result.map(Js::Bool);
                }
                _ => {
                    return Err(Thrown::type_error(&std::format!(
                        "Unknown WEEX public socket member {name}"
                    )));
                }
            };
            save_subscriptions(&h, &state)?;
            result?;
        }
    }
    Ok(Js::Undefined)
}
