use super::{Rt, host::Host, members, scope};
use safe_cex_exchange::js_runtime::{
    scope::bound_member,
    value::{Js, Thrown, arg},
};
use std::rc::Rc;
pub(super) fn construct(rt: &Rt, private: bool, parent: Js) -> Result<Js, Thrown> {
    let h = Host::new(rt);
    let args = vec![parent.clone()];
    let socket = if private {
        crate::runtime_assembly::allocate_private_socket(&h, args)?
    } else {
        crate::runtime_assembly::allocate_public_socket(&h, args)?
    };
    let urls = || Ok::<_, Thrown>(());
    let testnet = |p: &Js| Ok(p.get("options")?.get("testnet")?.truthy());
    let lookup = |_, testnet| {
        Ok(Js::str(if testnet {
            "wss://ws.api.testnet.paradex.trade/v1?cancel-on-disconnect=false"
        } else {
            "wss://ws.api.prod.paradex.trade/v1?cancel-on-disconnect=false"
        }))
    };
    if private {
        crate::private_ingress::initialize_private_socket(
            &parent,
            |_| Ok(()),
            urls,
            testnet,
            lookup,
            |url| socket.set("connectUrl", url),
            |bindings| {
                let handlers = Js::object(vec![]);
                for b in bindings {
                    use crate::private_ingress::PrivateHandler as P;
                    let target = match b.handler {
                        P::Ping | P::Pong => "handlePongEvent",
                        P::Orders => "handleOrderUpdate",
                        P::Positions => "handlePositionUpdate",
                        P::Balance => "handleBalanceUpdate",
                    };
                    let callback = members::callback(
                        &rt.with_this(socket.clone()),
                        "dispatch",
                        vec![
                            Js::str(target),
                            Js::Bool(!matches!(b.handler, P::Ping | P::Pong)),
                        ],
                    );
                    handlers.define(b.channel, Js::array(vec![callback]))?;
                }
                socket.set("messageHandlers", handlers)
            },
        )?;
    } else {
        crate::public_ingress::initialize_public_socket(
            &parent,
            |_| Ok(()),
            urls,
            testnet,
            lookup,
            |url| socket.set("connectUrl", url),
        )?;
    }
    Ok(socket)
}
fn private(rt: &Rt) -> Result<bool, Thrown> {
    let parent = rt.this.get("parent")?;
    Ok(parent.get("privateWebsocket")?.same_reference(&rt.this))
}
fn timer(rt: &Rt, k: &str) -> Result<Option<Js>, Thrown> {
    let v = rt.this.get(k)?;
    Ok(v.truthy().then_some(v))
}
fn clear(rt: &Rt, v: Option<Js>) -> Result<(), Thrown> {
    if let Some(v) = v {
        rt.clear_timeout(&v);
    }
    Ok(())
}
fn send(rt: &Rt, v: Js) -> Result<(), Thrown> {
    rt.this.call_method("send", vec![v])?;
    Ok(())
}
fn base_dispose(rt: &Rt) -> Result<(), Thrown> {
    safe_cex_exchange::js_runtime::sockets::base_websocket_class(&rt.shared.sockets)
        .get("prototype")?
        .get("dispose")?
        .call(rt.this.clone(), vec![])?;
    Ok(())
}
fn each_values(v: Js, c: &mut dyn FnMut(Js) -> Result<(), Thrown>) -> Result<(), Thrown> {
    for v in super::primitives::iterate(&v.call_method("values", vec![])?)? {
        c(v)?;
    }
    Ok(())
}
pub(super) fn sync(rt: &Rt, n: &str, a: &[Js], captures: &[Js]) -> Result<Option<Js>, Thrown> {
    if rt.this.get("parent")?.is_undefined() {
        return Ok(None);
    }
    let x = arg(a, 0);
    match n {
        "listenOrderBook" => {
            let callback = arg(a, 1);
            let callbacks = crate::order_execution::register_order_book_listener(
                &Host::new(rt),
                x.clone(),
                callback.clone(),
            )?;
            let weak = Rc::downgrade(&rt.shared);
            let owner = rt.this.clone();
            return Ok(Some(Js::function("cleanup", move |_, _| {
                let rt = scope(&weak, owner.clone())?;
                crate::order_execution::cleanup_order_book_listener(
                    &Host::new(&rt),
                    x.clone(),
                    callbacks.clone(),
                    callback.clone(),
                )
            })));
        }
        "subscribeToOrderBook" | "unsubscribeFromOrderBook" => {
            let connected = || Ok(rt.this.get("isConnected")?.truthy());
            let channel = |symbol: Js| {
                Ok(Js::str(&crate::public_ws_helpers::get_order_book_channel(
                    &super::text(&symbol)?,
                )))
            };
            let send = |method, channel| rpc(rt, method, channel, None);
            if n == "subscribeToOrderBook" {
                crate::public_subscriptions::subscribe_to_order_book(x, connected, channel, send)?;
            } else {
                crate::public_subscriptions::unsubscribe_from_order_book(
                    x, connected, channel, send,
                )?;
            }
        }
        "subscribeToTrades" => {
            crate::public_subscriptions::subscribe_to_trades(
                &x.to_js_string()?,
                |symbol| optional_count(rt, "tradeSubscriptionCounts", symbol),
                |symbol, count| set_count(rt, "tradeSubscriptionCounts", symbol, count),
                |symbol| {
                    rt.this
                        .call_method("sendTradeSubscribe", vec![Js::utf16(symbol)])
                        .map(|_| ())
                },
            )?;
        }
        "unsubscribeFromTrades" => {
            crate::public_subscriptions::unsubscribe_from_trades(
                &x.to_js_string()?,
                |symbol| optional_count(rt, "tradeSubscriptionCounts", symbol),
                |symbol| {
                    rt.this
                        .get("tradeSubscriptionCounts")?
                        .call_method("delete", vec![Js::utf16(symbol)])
                        .map(|_| ())
                },
                |symbol, count| set_count(rt, "tradeSubscriptionCounts", symbol, count),
            )?;
        }
        "listenOHLCV" => {
            return Ok(Some(crate::runtime_assembly::listen_ohlcv(
                &Host::new(rt),
                rt.this.clone(),
                x,
                arg(a, 1),
            )?));
        }
        "ohlcv-cleanup" => {
            return Ok(Some(crate::runtime_assembly::cleanup_ohlcv_listener(
                &Host::new(rt),
                rt.this.clone(),
                arg(captures, 0),
                arg(captures, 1),
                arg(captures, 2),
            )?));
        }
        "ohlcv-slippage-error" => {
            return Ok(Some(crate::runtime_assembly::ohlcv_slippage_error(
                &Host::new(rt),
                rt.this.clone(),
                arg(captures, 0),
                Thrown::new(x),
            )?));
        }
        "cleanupOhlcvSubscription" => {
            crate::public_subscriptions::cleanup_ohlcv_subscription(
                &x.to_js_string()?,
                |key, active| {
                    rt.this
                        .get("ohlcvPollingActive")?
                        .call_method("set", vec![Js::utf16(key), Js::Bool(active)])
                        .map(|_| ())
                },
                |key| optional_count(rt, "ohlcvSubscriptionVersions", key),
                |key, count| set_count(rt, "ohlcvSubscriptionVersions", key, count),
                |key| {
                    Ok(rt
                        .this
                        .get("ohlcvPollingTimeouts")?
                        .call_method("has", vec![Js::utf16(key)])?
                        .truthy())
                },
                |key| {
                    Ok(Some(
                        rt.this
                            .get("ohlcvPollingTimeouts")?
                            .call_method("get", vec![Js::utf16(key)])?,
                    ))
                },
                |timer| clear(rt, timer),
                |key| {
                    rt.this
                        .get("ohlcvPollingTimeouts")?
                        .call_method("delete", vec![Js::utf16(key)])
                        .map(|_| ())
                },
                |field, key| {
                    rt.this
                        .get(field)?
                        .call_method("delete", vec![Js::utf16(key)])
                        .map(|_| ())
                },
            )?;
        }
        "throttledFetchPositions" => {
            let scheduled = deferred(rt, |rt, value| {
                rt.this.set("positionFetchScheduled", Js::Bool(value))
            });
            crate::private_ingress::throttled_fetch_positions(
                || {
                    rt.global("Date.now")?
                        .call(Js::Undefined, vec![])?
                        .to_number()
                },
                owned(rt, |rt| Ok(rt.this.get("isDisposed")?.truthy())),
                || Ok(rt.this.get("positionFetchScheduled")?.truthy()),
                || rt.this.get("lastPositionFetchTime")?.to_number(),
                || rt.this.get("positionFetchThrottleMs")?.to_number(),
                scheduled,
                owned(rt, |rt| {
                    Ok(rt.this.get("hasPendingPositionEvents")?.truthy())
                }),
                owned(rt, |rt| {
                    rt.this
                        .call_method("executePositionFetch", vec![])
                        .map(|_| ())
                }),
                |callback, delay| {
                    schedule_once(rt, callback, delay);
                    Ok(())
                },
            )?;
        }

        "onOpen" if !private(rt)? => crate::ws_public_lifecycle::on_open(
            || Ok(rt.this.get("isDisposed")?.truthy()),
            |message, severity| {
                Host::new(rt).log(
                    Host::new(rt).logger()?,
                    message.encode_utf16().collect(),
                    severity,
                )
            },
            || rt.this.call_method("ping", vec![]).map(|_| ()),
            || {
                rt.this
                    .get("sentTradeSymbols")?
                    .call_method("clear", vec![])
                    .map(|_| ())
            },
            || {
                rt.this
                    .get("pendingSubscribeRequests")?
                    .call_method("clear", vec![])
                    .map(|_| ())
            },
            || Ok(rt.this.get("isConnected")?.truthy()),
            || {
                let value = rt.this.get("ws")?;
                Ok((!value.is_nullish()).then_some(value))
            },
            |ws| ws.call_method("close", vec![]).map(|_| ()),
            || {
                rt.this
                    .call_method("subscribeToChannels", vec![])
                    .map(|_| ())
            },
        )?,
        "sendJsonRpcSubscription" => {
            let method = if x.strict_equals(&Js::str("unsubscribe")) {
                crate::public_subscriptions::SubscriptionMethod::Unsubscribe
            } else {
                crate::public_subscriptions::SubscriptionMethod::Subscribe
            };
            let attempt = arg(a, 2);
            let attempt = if attempt.is_undefined() {
                None
            } else {
                Some(attempt.to_number()?)
            };
            crate::public_subscriptions::send_json_rpc_subscription(
                method,
                arg(a, 1),
                attempt,
                || Ok(rt.this.get("isConnected")?.truthy()),
                || rt.this.get("messageId")?.to_number(),
                |id| rt.this.set("messageId", Js::Number(id)),
                |id, channel, attempt| {
                    rt.this
                        .get("pendingSubscribeRequests")?
                        .call_method(
                            "set",
                            vec![
                                Js::Number(id),
                                Js::object(vec![
                                    ("channel", channel.clone()),
                                    ("attempt", Js::Number(attempt)),
                                ]),
                            ],
                        )
                        .map(|_| ())
                },
                |request| {
                    let method = match request.method {
                        crate::public_subscriptions::SubscriptionMethod::Subscribe => "subscribe",
                        crate::public_subscriptions::SubscriptionMethod::Unsubscribe => {
                            "unsubscribe"
                        }
                    };
                    send_json(
                        rt,
                        Js::object(vec![
                            ("jsonrpc", Js::str(request.jsonrpc)),
                            ("method", Js::str(method)),
                            ("params", Js::object(vec![("channel", request.channel)])),
                            ("id", Js::Number(request.id)),
                        ]),
                    )
                },
            )?;
        }
        "sendTradeSubscribe" => {
            let force = arg(a, 1);
            let attempt = arg(a, 2);
            crate::public_subscriptions::send_trade_subscribe(
                &x.to_js_string()?,
                if force.is_undefined() {
                    None
                } else {
                    Some(force.truthy())
                },
                if attempt.is_undefined() {
                    None
                } else {
                    Some(attempt.to_number()?)
                },
                |symbol| {
                    rt.this
                        .get("desiredTradeSymbols")?
                        .call_method("add", vec![Js::utf16(symbol)])
                        .map(|_| ())
                },
                |symbol| {
                    Ok(rt
                        .this
                        .get("sentTradeSymbols")?
                        .call_method("has", vec![Js::utf16(symbol)])?
                        .truthy())
                },
                |symbol| {
                    rt.this
                        .get("sentTradeSymbols")?
                        .call_method("add", vec![Js::utf16(symbol)])
                        .map(|_| ())
                },
                |method, channel, attempt| rpc(rt, method, Js::utf16(&channel), Some(attempt)),
            )?;
        }
        "subscribeToChannels" => crate::public_subscriptions::subscribe_to_channels(
            || rt.this.get("parent")?.get("store")?.get("markets"),
            |markets, callback| {
                for market in super::primitives::iterate(markets)? {
                    callback(&market.get("symbol")?.to_js_string()?)?;
                }
                Ok(())
            },
            |markets| markets.get("length")?.to_number(),
            |method, channel| rpc(rt, method, Js::utf16(&channel), None),
            |symbol, force| {
                rt.this
                    .call_method(
                        "sendTradeSubscribe",
                        vec![Js::utf16(symbol), Js::Bool(force)],
                    )
                    .map(|_| ())
            },
            |message, severity| {
                Host::new(rt).log(
                    Host::new(rt).logger()?,
                    message.encode_utf16().collect(),
                    severity,
                )
            },
            || rt.this.get("orderbookCallbacks")?.get("size")?.to_number(),
            |callback| {
                for entry in super::primitives::iterate(&rt.this.get("orderbookCallbacks")?)? {
                    callback(
                        &entry.get("0")?.to_js_string()?,
                        entry.get("1")?.get("size")?.to_number()?,
                    )?;
                }
                Ok(())
            },
            |symbol| {
                rt.this
                    .call_method("subscribeToOrderBook", vec![Js::utf16(symbol)])
                    .map(|_| ())
            },
        )?,

        "onMessage" if private(rt)? => {
            crate::private_ingress::on_private_message(&super::ingress::SocketHost {
                rt,
                event: x.clone(),
            })?
        }
        "handleOrderUpdate" => crate::private_ingress::handle_order_update(
            &super::ingress::SocketHost {
                rt,
                event: Js::Undefined,
            },
            &x,
        )?,
        "handlePositionUpdate" => crate::private_ingress::handle_position_update(
            &super::ingress::SocketHost {
                rt,
                event: Js::Undefined,
            },
            &x,
        )?,
        "handleBalanceUpdate" => crate::private_ingress::handle_balance_update(
            &super::ingress::SocketHost {
                rt,
                event: Js::Undefined,
            },
            &x,
        )?,
        "formatCancelReason" => {
            let reason = crate::private_ingress::format_cancel_reason(
                &x.to_js_string()?,
                |key| {
                    let value = Js::object(vec![]).get_key(key)?;
                    let truthy = value.truthy();
                    Ok::<_, Thrown>(Some((value, truthy)))
                },
                |units| {
                    Ok(Js::utf16(&units)
                        .call_method("toLowerCase", vec![])?
                        .to_js_string()?
                        .to_vec())
                },
            )?;
            return Ok(Some(match reason {
                crate::private_ingress::CancelReason::Text(t) => Js::utf16(&t),
                crate::private_ingress::CancelReason::Inherited(v) => v,
            }));
        }

        "onMessage" if !private(rt)? => crate::public_ingress::on_public_message(
            &super::ingress::SocketHost {
                rt,
                event: x.clone(),
            },
            &x,
        )?,
        "handleBBOUpdate" => crate::public_ingress::handle_bbo_update(
            &super::ingress::SocketHost {
                rt,
                event: Js::Undefined,
            },
            &x,
        )?,
        "handleMarketSummaryUpdate" => crate::public_ingress::handle_summary_update(
            &super::ingress::SocketHost {
                rt,
                event: Js::Undefined,
            },
            &x,
        )?,
        "handleTradesUpdate" => crate::public_ingress::handle_trades_update(
            &super::ingress::SocketHost {
                rt,
                event: Js::Undefined,
            },
            &x,
        )?,
        "handleOrderbookUpdate" => {
            crate::market_data::handle_orderbook_update(&Host::new(rt), x)?;
        }

        "dispatch" => {
            crate::runtime_assembly::invoke_bound_handler(
                &Host::new(rt),
                rt.this.clone(),
                &super::text(&arg(captures, 0))?,
                arg(captures, 1).truthy().then_some(x),
            )?;
        }
        "connectAndSubscribe" => {
            crate::ws_public_lifecycle::connect_and_subscribe(
                || Ok(rt.this.get("isDisposed")?.truthy()),
                || rt.this.get("connectUrl"),
                |url| rt.this.call_method("createWebsocket", vec![url]),
                |ws| rt.this.set("ws", ws),
                |event| {
                    rt.this.get("ws")?.call_method(
                        "addEventListener",
                        vec![
                            Js::str(event),
                            bound_member(
                                &rt.this,
                                match event {
                                    "open" => "onOpen",
                                    "message" => "onMessage",
                                    _ => "onClose",
                                },
                            )?,
                        ],
                    )?;
                    Ok(())
                },
            )?;
        }
        "ping" => {
            crate::ws_public_lifecycle::ping(
                owned(rt, |rt| Ok(rt.this.get("isConnected")?.truthy())),
                owned(rt, |rt| Ok(rt.this.get("isDisposed")?.truthy())),
                |message| send(rt, Js::str(message)),
                owned(rt, |rt| rt.this.call_method("ping", vec![]).map(|_| ())),
                |callback, delay| Ok(schedule_once(rt, callback, delay as f64)),
                |timer| rt.this.set("pingInterval", timer),
            )?;
        }

        "dispose" => {
            if private(rt)? {
                crate::ws_private_lifecycle::dispose(
                    || timer(rt, "pingInterval"),
                    || timer(rt, "ordersThrottleTimeout"),
                    |v| clear(rt, v),
                    |v| rt.this.set("pingInterval", v.unwrap_or(Js::Undefined)),
                    |v| {
                        rt.this
                            .set("ordersThrottleTimeout", v.unwrap_or(Js::Undefined))
                    },
                    || base_dispose(rt),
                )?;
            } else {
                crate::ws_public_lifecycle::dispose(
                    || timer(rt, "pingInterval"),
                    |v| clear(rt, v),
                    |v| rt.this.set("pingInterval", v.unwrap_or(Js::Undefined)),
                    |f| each_values(rt.this.get("subscriptionRetryTimeouts")?, f),
                    || {
                        rt.this
                            .get("subscriptionRetryTimeouts")?
                            .call_method("clear", vec![])?;
                        Ok(())
                    },
                    |f| each_values(rt.this.get("ohlcvPollingTimeouts")?, f),
                    || {
                        rt.this
                            .get("ohlcvPollingTimeouts")?
                            .call_method("clear", vec![])?;
                        Ok(())
                    },
                    || base_dispose(rt),
                )?;
            }
        }
        "subscribeToPrivateChannels" => crate::ws_private_lifecycle::subscribe_to_private_channels(
            || rt.this.get("messageId")?.to_number(),
            |n| rt.this.set("messageId", Js::Number(n)),
            |s| {
                let message = Js::object(vec![
                    ("jsonrpc", Js::str(s.jsonrpc)),
                    ("method", Js::str(s.method)),
                    ("params", Js::object(vec![("channel", Js::str(s.channel))])),
                    ("id", Js::Number(s.id)),
                ]);
                send(
                    rt,
                    rt.global("JSON.stringify")?
                        .call(Js::Undefined, vec![message])?,
                )
            },
        )?,
        "throttledFetchOrders" => crate::ws_private_lifecycle::throttled_fetch_orders(
            || timer(rt, "ordersThrottleTimeout"),
            |v| clear(rt, v),
            || bound_member(&rt.this, "fetchThrottledOrders"),
            |callback, delay| {
                Ok(rt.set_timeout(
                    Rc::new(move || callback.call(Js::Undefined, vec![]).map(|_| ())),
                    delay as f64,
                ))
            },
            |v| rt.this.set("ordersThrottleTimeout", v),
        )?,
        _ => return Ok(None),
    }
    Ok(Some(Js::Undefined))
}

fn send_json(rt: &Rt, value: Js) -> Result<(), Thrown> {
    send(
        rt,
        rt.global("JSON.stringify")?
            .call(Js::Undefined, vec![value])?,
    )
}
fn rpc(
    rt: &Rt,
    method: crate::public_subscriptions::SubscriptionMethod,
    channel: Js,
    attempt: Option<f64>,
) -> Result<(), Thrown> {
    let method = match method {
        crate::public_subscriptions::SubscriptionMethod::Subscribe => "subscribe",
        crate::public_subscriptions::SubscriptionMethod::Unsubscribe => "unsubscribe",
    };
    let mut args = vec![Js::str(method), channel];
    if let Some(attempt) = attempt {
        args.push(Js::Number(attempt));
    }
    rt.this.call_method("sendJsonRpcSubscription", args)?;
    Ok(())
}

pub(super) async fn asynchronous(rt: &Rt, name: &str) -> Result<Option<Js>, Thrown> {
    if rt.this.get("parent")?.is_undefined() {
        return Ok(None);
    }
    match name {
        "authenticate" => {
            authenticate(rt, None).await?;
        }
        "onOpen" if private(rt)? => {
            let core =
                safe_cex_exchange::js_runtime::sockets::core_of(&rt.shared.sockets, &rt.this)
                    .and_then(|socket| socket.base.get().cloned())
                    .ok_or_else(|| Thrown::error("Error", "Paradex socket has no shared base"))?;
            let task = rt.clone();
            core.authenticate_and_subscribe(
                Box::pin(async move {
                    let failure = Rc::new(std::cell::RefCell::new(None));
                    crate::auth_operations::private_on_open(
                        || Ok(task.this.get("isDisposed")?.truthy()),
                        |b| task.this.set("authenticated", Js::Bool(b)),
                        |id| {
                            task.this
                                .set("authRequestId", id.map_or(Js::Undefined, Js::Number))
                        },
                        |message, severity| {
                            Host::new(&task).log(
                                Host::new(&task).logger()?,
                                message.encode_utf16().collect(),
                                severity,
                            )
                        },
                        || task.this.call_method("ping", vec![]).map(|_| ()),
                        || Ok::<_, Thrown>(authenticate(&task, Some(failure.clone()))),
                    )
                    .await
                    .map_err(|error| error.to_exchange_error())?;
                    let error = failure.borrow_mut().take();
                    match error {
                        Some(error) => Err(error.to_exchange_error()),
                        None => Ok(()),
                    }
                }),
                |_| Ok(()),
            )
            .await
            .map_err(Thrown::from)?;
        }
        "executePositionFetch" => {
            let _ =
                crate::runtime_assembly::execute_position_fetch(&Host::new(rt), rt.this.clone())
                    .await;
        }
        "fetchThrottledOrders" => {
            let _ =
                crate::runtime_assembly::fetch_throttled_orders(&Host::new(rt), rt.this.clone())
                    .await;
        }
        _ => return Ok(None),
    }
    Ok(Some(Js::Undefined))
}

async fn authenticate(
    rt: &Rt,
    observer: Option<Rc<std::cell::RefCell<Option<Thrown>>>>,
) -> Result<(), Thrown> {
    let parent = rt.this.get("parent")?;
    crate::auth_operations::authenticate(
        || Ok(parent.get("sdkInitPromise")?.truthy()),
        || Ok::<_, Thrown>(Host::new(rt).future(parent.get("sdkInitPromise")?)),
        || Ok(parent.get("jwtManager")?.truthy()),
        || Ok(Host::new(rt).future(parent.get("jwtManager")?.call_method("getToken", vec![])?)),
        || rt.this.get("messageId")?.to_number(),
        |id| rt.this.set("messageId", Js::Number(id)),
        |id| rt.this.set("authRequestId", Js::Number(id)),
        |request| {
            send_json(
                rt,
                Js::object(vec![
                    ("jsonrpc", Js::str(request.jsonrpc)),
                    ("method", Js::str(request.method)),
                    ("params", Js::object(vec![("bearer", request.bearer)])),
                    ("id", Js::Number(request.id)),
                ]),
            )
        },
        |error: &Thrown| {
            let logger = Host::new(rt).logger()?;
            let mut message = "Failed to authenticate WebSocket: "
                .encode_utf16()
                .collect::<Vec<_>>();
            message.extend(error.value.to_js_string()?.iter().copied());
            Ok((logger, message, error.clone()))
        },
        |(logger, message, error), severity| {
            Host::new(rt).log(logger, message, severity)?;
            if let Some(observer) = observer {
                *observer.borrow_mut() = Some(error);
            }
            Ok(())
        },
    )
    .await
}

fn owned<T: 'static>(
    rt: &Rt,
    operation: impl Fn(&Rt) -> Result<T, Thrown> + 'static,
) -> Box<dyn Fn() -> Result<T, Thrown>> {
    let weak = Rc::downgrade(&rt.shared);
    let owner = rt.this.clone();
    Box::new(move || operation(&scope(&weak, owner.clone())?))
}
fn deferred<T: 'static>(
    rt: &Rt,
    operation: impl Fn(&Rt, T) -> Result<(), Thrown> + 'static,
) -> Box<dyn Fn(T) -> Result<(), Thrown>> {
    let weak = Rc::downgrade(&rt.shared);
    let owner = rt.this.clone();
    Box::new(move |value| operation(&scope(&weak, owner.clone())?, value))
}
fn schedule_once(rt: &Rt, callback: Box<dyn FnOnce() -> Result<(), Thrown>>, delay: f64) -> Js {
    let callback = std::cell::RefCell::new(Some(callback));
    rt.set_timeout(
        Rc::new(move || {
            let callback = callback.borrow_mut().take();
            match callback {
                Some(callback) => callback(),
                None => Ok(()),
            }
        }),
        delay,
    )
}

fn optional_count(rt: &Rt, field: &str, key: &[u16]) -> Result<Option<f64>, Thrown> {
    let value = rt
        .this
        .get(field)?
        .call_method("get", vec![Js::utf16(key)])?;
    if value.is_undefined() {
        Ok(None)
    } else {
        Ok(Some(value.to_number()?))
    }
}
fn set_count(rt: &Rt, field: &str, key: &[u16], count: f64) -> Result<(), Thrown> {
    rt.this
        .get(field)?
        .call_method("set", vec![Js::utf16(key), Js::Number(count)])
        .map(|_| ())
}
