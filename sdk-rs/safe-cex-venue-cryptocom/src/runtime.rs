//! Full factory, construction and orchestration bodies at explicit native operations.
//! Every own application member is assembled from its Rust body.
use crate::socket_handlers::{SocketFuture, SocketOps};
use std::rc::Rc;

pub type Constructor<V, E> = Rc<dyn Fn(V, V, Vec<V>) -> Result<V, E>>;
pub type NativeMethod<V, E> = Rc<dyn Fn(V, Vec<V>) -> Result<V, E>>;
pub struct RuntimeOps<V, E> {
    pub value: Rc<SocketOps<V, E>>,
    pub is_string: Rc<dyn Fn(&V) -> bool>,
    pub regexp: Rc<dyn Fn(&str, &str) -> Result<V, E>>,
    pub binary: Rc<dyn Fn(&str, V, V) -> Result<V, E>>,
    pub construct: Rc<dyn Fn(V, Vec<V>) -> Result<V, E>>,
    pub throw_value: Rc<dyn Fn(V) -> E>,
    /// Drive eagerly to suspension and retain execution through observer drops.
    /// Observer Drop must not run cleanup or cancel shared requests.
    pub promise: Rc<dyn Fn(SocketFuture<V, E>) -> V>,
    pub define: Rc<dyn Fn(&V, &str, V) -> Result<(), E>>,
    pub superclass: Rc<dyn Fn(&V) -> Result<V, E>>,
    pub super_construct: Constructor<V, E>,
    pub derive: Rc<dyn Fn(&str, V, usize, Constructor<V, E>) -> Result<V, E>>,
    pub prototype_method: Rc<dyn Fn(&V, &str, usize, NativeMethod<V, E>) -> Result<(), E>>,
    pub prototype_getter: Rc<dyn Fn(&V, &str, Rc<dyn Fn(V) -> Result<V, E>>) -> Result<(), E>>,
    pub binding: Rc<crate::native_bindings::BindingOps<V, E>>,
}
fn argument<V: Clone, E>(h: &RuntimeOps<V, E>, args: &[V], i: usize) -> V {
    args.get(i)
        .cloned()
        .unwrap_or_else(|| (h.value.undefined)())
}
fn get<V, E>(h: &RuntimeOps<V, E>, value: &V, key: &str) -> Result<V, E> {
    (h.value.get)(value, key)
}
fn optional<V, E>(h: &RuntimeOps<V, E>, value: V, key: &str) -> Result<V, E> {
    if (h.value.nullish)(&value) {
        Ok((h.value.undefined)())
    } else {
        get(h, &value, key)
    }
}
fn method<V: Clone, E>(
    h: &RuntimeOps<V, E>,
    receiver: V,
    key: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let callee = get(h, &receiver, key)?;
    (h.value.invoke)(callee, receiver, args()?)
}
fn own<V: Clone, E>(
    h: &RuntimeOps<V, E>,
    receiver: &V,
    key: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    method(h, receiver.clone(), key, args)
}
fn global<V, E>(
    h: &RuntimeOps<V, E>,
    name: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let callee = (h.value.global)(name)?;
    (h.value.invoke)(callee, (h.value.undefined)(), args()?)
}
fn text<V, E>(h: &RuntimeOps<V, E>, prefix: &str, value: V, suffix: &str) -> Result<V, E> {
    Ok((h.value.utf16)(
        prefix
            .encode_utf16()
            .chain((h.value.text)(&value)?)
            .chain(suffix.encode_utf16())
            .collect(),
    ))
}
fn loaded<V, E>(h: &RuntimeOps<V, E>, receiver: &V, flags: &[&str]) -> Result<V, E> {
    let store = get(h, receiver, "store")?;
    let mut fields = (h.value.spread_object)(&get(h, &store, "loaded")?)?;
    for flag in flags {
        fields.push(((h.value.literal)(flag), (h.value.boolean)(true)));
    }
    Ok((h.value.object_keys)(fields))
}
fn update<V: Clone, E>(
    h: &RuntimeOps<V, E>,
    receiver: &V,
    fields: impl FnOnce() -> Result<Vec<(&'static str, V)>, E>,
) -> Result<(), E> {
    method(h, get(h, receiver, "store")?, "update", || {
        Ok(vec![(h.value.object)(fields()?)])
    })?;
    Ok(())
}
fn disposed<V, E>(h: &RuntimeOps<V, E>, receiver: &V) -> Result<bool, E> {
    Ok((h.value.truthy)(&get(h, receiver, "isDisposed")?))
}

pub fn start<V: Clone + 'static, E: 'static>(
    h: Rc<RuntimeOps<V, E>>,
    receiver: V,
) -> SocketFuture<V, E> {
    let begun = own(&h, &receiver, "initExternalPublicData", || Ok(vec![]));
    Box::pin(async move {
        let external = (h.value.await_value)(begun?).await?;
        if !(h.value.truthy)(&external) {
            let markets =
                (h.value.await_value)(own(&h, &receiver, "fetchMarkets", || Ok(vec![]))?).await?;
            if disposed(&h, &receiver)? {
                return Ok((h.value.undefined)());
            }
            update(&h, &receiver, || {
                Ok(vec![
                    ("markets", markets.clone()),
                    ("loaded", loaded(&h, &receiver, &["markets"])?),
                ])
            })?;
            let tickers =
                (h.value.await_value)(own(&h, &receiver, "fetchTickers", || Ok(vec![]))?).await?;
            if disposed(&h, &receiver)? {
                return Ok((h.value.undefined)());
            }
            own(&h, &receiver, "log", || {
                let min = method(&h, (h.value.global)("Math")?, "min", || {
                    Ok(vec![
                        get(&h, &tickers, "length")?,
                        get(&h, &markets, "length")?,
                    ])
                })?;
                Ok(vec![text(&h, "Loaded ", min, " Crypto.com markets")?])
            })?;
            update(&h, &receiver, || {
                Ok(vec![
                    ("tickers", tickers),
                    ("loaded", loaded(&h, &receiver, &["tickers"])?),
                ])
            })?;
            method(
                &h,
                get(&h, &receiver, "publicWebsocket")?,
                "connectAndSubscribe",
                || Ok(vec![]),
            )?;
        }
        let public_only = get(&h, &receiver, "isPublicDataOnly")?;
        if (h.value.truthy)(&public_only) {
            update(&h, &receiver, || {
                Ok(vec![(
                    "loaded",
                    loaded(&h, &receiver, &["balance", "positions", "orders"])?,
                )])
            })?;
            return Ok((h.value.undefined)());
        }
        for key in [
            "connectAndSubscribe",
            "subscribeToOrderUpdates",
            "subscribeToPositionUpdates",
            "subscribeToBalanceUpdates",
        ] {
            method(&h, get(&h, &receiver, "privateWebsocket")?, key, || {
                Ok(vec![])
            })?;
        }
        let promise = (h.value.global)("Promise")?;
        let timer_h = h.clone();
        let executor = (h.value.function)(
            "",
            1,
            Rc::new(move |args| {
                global(&timer_h, "setTimeout", || {
                    Ok(vec![
                        argument(&timer_h, &args, 0),
                        (timer_h.value.number)(1000.),
                    ])
                })
            }),
        );
        (h.value.await_value)((h.construct)(promise, vec![executor])?).await?;
        if !(h.value.truthy)(&external) {
            method(
                &h,
                get(&h, &receiver, "publicWebsocket")?,
                "subscribeToMarketData",
                || {
                    let markets = get(&h, &get(&h, &receiver, "store")?, "markets")?;
                    let map_h = h.clone();
                    Ok(vec![method(&h, markets, "map", || {
                        Ok(vec![(h.value.function)(
                            "",
                            1,
                            Rc::new(move |args| {
                                let market = argument(&map_h, &args, 0);
                                Ok((map_h.value.object)(vec![
                                    ("instrumentName", get(&map_h, &market, "id")?),
                                    (
                                        "indexInstrumentName",
                                        optional(
                                            &map_h,
                                            get(&map_h, &market, "info")?,
                                            "underlyingSymbol",
                                        )?,
                                    ),
                                ]))
                            }),
                        )])
                    })?])
                },
            )?;
        }
        (h.value.await_value)(own(&h, &receiver, "tick", || Ok(vec![]))?).await?;
        if disposed(&h, &receiver)? {
            return Ok((h.value.undefined)());
        }
        own(&h, &receiver, "log", || {
            Ok(vec![(h.value.literal)("Ready to trade on Crypto.com")])
        })?;
        let orders =
            (h.value.await_value)(own(&h, &receiver, "fetchOrders", || Ok(vec![]))?).await?;
        if disposed(&h, &receiver)? {
            return Ok((h.value.undefined)());
        }
        own(&h, &receiver, "log", || {
            Ok(vec![(h.value.literal)("Loaded Crypto.com orders")])
        })?;
        update(&h, &receiver, || {
            Ok(vec![
                ("orders", orders),
                ("loaded", loaded(&h, &receiver, &["orders"])?),
            ])
        })?;
        Ok((h.value.undefined)())
    })
}
pub fn tick<V: Clone + 'static, E: 'static>(
    h: Rc<RuntimeOps<V, E>>,
    receiver: V,
    count: V,
) -> SocketFuture<V, E> {
    let count = if (h.value.equal)(&count, &(h.value.undefined)()) {
        (h.value.number)(0.)
    } else {
        count
    };
    let begun = (|| {
        if disposed(&h, &receiver)? || (h.value.truthy)(&get(&h, &receiver, "isPublicDataOnly")?) {
            return Ok(None);
        }
        Ok(Some(own(&h, &receiver, "fetchPositions", || Ok(vec![]))))
    })();
    Box::pin(async move {
        let Some(begun) = begun? else {
            return Ok((h.value.undefined)());
        };
        let work = async {
            let positions = (h.value.await_value)(begun?).await?;
            if disposed(&h, &receiver)? {
                return Ok(true);
            }
            update(&h, &receiver, || {
                Ok(vec![
                    ("positions", positions.clone()),
                    ("loaded", loaded(&h, &receiver, &["positions"])?),
                ])
            })?;
            let balance =
                (h.value.await_value)(own(&h, &receiver, "fetchBalance", || Ok(vec![]))?).await?;
            if disposed(&h, &receiver)? {
                return Ok(true);
            }
            let positive = (h.binary)(">", get(&h, &positions, "length")?, (h.value.number)(0.))?;
            let upnl = if (h.value.truthy)(&positive) {
                global(&h, "sumBy", || {
                    Ok(vec![positions, (h.value.literal)("unrealizedPnl")])
                })?
            } else {
                (h.value.number)(0.)
            };
            (h.value.set)(&balance, "upnl", upnl)?;
            update(&h, &receiver, || {
                Ok(vec![
                    ("balance", balance),
                    ("loaded", loaded(&h, &receiver, &["balance"])?),
                ])
            })?;
            if (h.value.equal)(
                &(h.binary)("%", count.clone(), (h.value.number)(4.))?,
                &(h.value.number)(0.),
            ) {
                let orders =
                    (h.value.await_value)(own(&h, &receiver, "fetchOrders", || Ok(vec![]))?)
                        .await?;
                if disposed(&h, &receiver)? {
                    return Ok(true);
                }
                update(&h, &receiver, || {
                    Ok(vec![
                        ("orders", orders),
                        ("loaded", loaded(&h, &receiver, &["orders"])?),
                    ])
                })?;
            }
            Ok(false)
        }
        .await;
        match work {
            Ok(true) => return Ok((h.value.undefined)()),
            Ok(false) => {}
            Err(error) => {
                own(&h, &receiver, "log", || {
                    Ok(vec![
                        text(
                            &h,
                            "Error in tick: ",
                            optional(&h, (h.value.error_value)(error), "message")?,
                            "",
                        )?,
                        get(&h, &(h.value.global)("LogSeverity")?, "Error")?,
                    ])
                })?;
            }
        }
        if !disposed(&h, &receiver)? {
            let callback_h = h.clone();
            global(&h, "setTimeout", || {
                Ok(vec![
                    (h.value.function)(
                        "",
                        0,
                        Rc::new(move |_| {
                            own(&callback_h, &receiver, "tick", || {
                                Ok(vec![(callback_h.binary)(
                                    "+",
                                    count.clone(),
                                    (callback_h.value.number)(1.),
                                )?])
                            })
                        }),
                    ),
                    (h.value.number)(10_000.),
                ])
            })?;
        }
        Ok((h.value.undefined)())
    })
}

fn request_body<V: Clone, E: Clone>(
    h: &RuntimeOps<V, E>,
    options: &V,
    exchange: &V,
    config: V,
) -> Result<V, E> {
    let next = (h.value.object_keys)((h.value.spread_object)(&config)?);
    let url = get(h, &config, "url")?;
    let endpoint = if (h.value.truthy)(&url) {
        url
    } else {
        (h.value.literal)("")
    };
    let auth = !(h.value.truthy)(&global(h, "isPublicEndpoint", || {
        Ok(vec![endpoint.clone()])
    })?);
    let method = method(h, endpoint, "replace", || {
        Ok(vec![(h.value.literal)("/"), (h.value.literal)("")])
    })?;
    let raw = if (h.is_string)(&get(h, &config, "data")?) {
        let parsed = global(h, "parseJsonObject", || Ok(vec![get(h, &config, "data")?]))?;
        if (h.value.nullish)(&parsed) {
            get(h, &config, "data")?
        } else {
            parsed
        }
    } else {
        let data = get(h, &config, "data")?;
        if (h.value.truthy)(&data) {
            data
        } else {
            (h.value.object)(vec![])
        }
    };
    if auth
        && (h.value.truthy)(&get(h, options, "key")?)
        && (h.value.truthy)(&get(h, options, "secret")?)
    {
        (h.value.set)(&next, "method", (h.value.literal)("POST"))?;
        let result: Result<(), E> = (|| {
            // A replay re-enters with the envelope it sent: re-sign its params.
            let unsigned = if (h.value.truthy)(&global(h, "isAuthenticatedEnvelope", || {
                Ok(vec![raw.clone(), method.clone(), get(h, options, "key")?])
            })?) {
                get(h, &raw, "params")?
            } else {
                raw
            };
            let params = if (h.value.truthy)(&global(h, "isRecord", || Ok(vec![unsigned.clone()]))?)
            {
                unsigned
            } else {
                (h.value.object)(vec![])
            };
            let signed = global(h, "generateAuthenticatedRequest", || {
                Ok(vec![
                    method,
                    params,
                    get(h, options, "key")?,
                    get(h, options, "secret")?,
                ])
            })?;
            // Serialized here: axios-retry resends through an identity transformRequest.
            let body = own(h, &(h.value.global)("JSON")?, "stringify", || {
                Ok(vec![signed])
            })?;
            (h.value.set)(&next, "data", body)?;
            Ok(())
        })();
        if let Err(error) = result {
            own(h, exchange, "emitAndLogError", || {
                Ok(vec![text(
                    h,
                    "Failed to generate auth request: ",
                    (h.value.error_value)(error.clone()),
                    "",
                )?])
            })?;
            return Err(error);
        }
    } else {
        (h.value.set)(&next, "method", (h.value.literal)("GET"))?;
        (h.value.set)(&next, "data", (h.value.undefined)())?;
        let params = if (h.value.truthy)(&global(h, "isRecord", || Ok(vec![raw.clone()]))?) {
            raw
        } else {
            (h.value.object)(vec![])
        };
        (h.value.set)(&next, "params", params)?;
    }
    Ok(next)
}
fn response_body<V: Clone, E>(
    h: &RuntimeOps<V, E>,
    exchange: &V,
    xhr: &V,
    unwrapped: &V,
    response: V,
) -> Result<V, E> {
    // A resend passes through here twice; the second pass must not re-read the result.
    if (h.value.truthy)(&method(h, unwrapped.clone(), "has", || {
        Ok(vec![response.clone()])
    })?) {
        return Ok(response);
    }
    let resend = global(h, "resendOnSignatureRejection", || {
        Ok(vec![
            xhr.clone(),
            response.clone(),
            (h.value.global)("isCryptocomSignatureRejection")?,
        ])
    })?;
    if (h.value.truthy)(&resend) {
        return Ok(resend);
    }
    let data = get(h, &response, "data")?;
    if !(h.value.equal)(&get(h, &data, "code")?, &(h.value.number)(0.)) {
        let mut units: Vec<u16> = "Crypto.com API Error ".encode_utf16().collect();
        units.extend((h.value.text)(&get(h, &data, "code")?)?);
        units.extend(": ".encode_utf16());
        let message = get(h, &data, "message")?;
        units.extend((h.value.text)(&if (h.value.truthy)(&message) {
            message
        } else {
            (h.value.literal)("Unknown error")
        })?);
        let msg = (h.value.utf16)(units);
        if !(h.value.truthy)(&global(
            h,
            "shouldSuppressValidationAuthErrorEmission",
            || Ok(vec![exchange.clone(), get(h, &data, "code")?]),
        )?) {
            own(h, exchange, "emitAndLogError", || Ok(vec![msg.clone()]))?;
        }
        let error = (h.construct)((h.value.global)("Error")?, vec![msg])?;
        return Err((h.throw_value)(error));
    }
    if !(h.value.equal)(&get(h, &data, "result")?, &(h.value.undefined)()) {
        (h.value.set)(&response, "data", get(h, &data, "result")?)?;
    }
    method(h, unwrapped.clone(), "add", || Ok(vec![response.clone()]))?;
    Ok(response)
}
fn rejection_body<V: Clone, E>(h: &RuntimeOps<V, E>, exchange: &V, error: V) -> Result<V, E> {
    let msg = global(h, "formatCryptocomApiError", || Ok(vec![error.clone()]))?;
    if (h.value.truthy)(&msg) {
        if !(h.value.truthy)(&global(
            h,
            "shouldSuppressValidationAuthErrorEmission",
            || {
                let response = optional(h, error.clone(), "response")?;
                let data = optional(h, response, "data")?;
                Ok(vec![exchange.clone(), optional(h, data, "code")?])
            },
        )?) {
            own(h, exchange, "emitAndLogError", || Ok(vec![msg.clone()]))?;
        }
        let error = (h.construct)((h.value.global)("Error")?, vec![msg])?;
        return Err((h.throw_value)(error));
    }
    Err((h.throw_value)(error))
}
pub fn create_api<V: Clone + 'static, E: Clone + 'static>(
    h: Rc<RuntimeOps<V, E>>,
    options: V,
    exchange: V,
) -> Result<V, E> {
    let bases = (h.value.global)("BASE_URLS")?;
    let key = if (h.value.truthy)(&get(&h, &options, "testnet")?) {
        "TESTNET"
    } else {
        "PRODUCTION"
    };
    let base = (h.value.index)(&bases, &(h.value.literal)(key))?;
    let xhr = global(&h, "safeCexAxiosCreate", || {
        let extra = optional(&h, options.clone(), "extra")?;
        let window = optional(&h, extra, "recvWindow")?;
        let timeout = if (h.value.truthy)(&window) {
            window
        } else {
            (h.value.number)(30000.)
        };
        Ok(vec![
            (h.value.object)(vec![
                ("baseURL", base),
                ("timeout", timeout),
                (
                    "headers",
                    (h.value.object)(vec![(
                        "Content-Type",
                        (h.value.literal)("application/json"),
                    )]),
                ),
            ]),
            options.clone(),
            exchange.clone(),
        ])
    })?;
    let retry_h = h.clone();
    global(&h, "retry", || {
        Ok(vec![
            xhr.clone(),
            (h.value.object)(vec![
                ("retries", (h.value.number)(3.)),
                (
                    "retryCondition",
                    (h.value.function)(
                        "retryCondition",
                        1,
                        Rc::new(move |args| {
                            let error = argument(&retry_h, &args, 0);
                            let network =
                                global(&retry_h, "isNetworkError", || Ok(vec![error.clone()]))?;
                            let read = if (retry_h.value.truthy)(&network) {
                                let write = global(&retry_h, "isNonRetryablePrivateWrite", || {
                                    Ok(vec![optional(
                                        &retry_h,
                                        get(&retry_h, &error, "config")?,
                                        "url",
                                    )?])
                                })?;
                                (retry_h.value.boolean)(!(retry_h.value.truthy)(&write))
                            } else {
                                network
                            };
                            if (retry_h.value.truthy)(&read) {
                                return Ok(read);
                            }
                            // A signature rejection is a refusal, so resending a write is safe.
                            let signature = global(&retry_h, "signatureRejectedResponse", || {
                                Ok(vec![(retry_h.value.global)(
                                    "isCryptocomSignatureRejection",
                                )?])
                            })?;
                            (retry_h.value.invoke)(
                                signature,
                                (retry_h.value.undefined)(),
                                vec![error],
                            )
                        }),
                    ),
                ),
            ]),
        ])
    })?;
    if (h.value.truthy)(&get(&h, &options, "tealstreetTunnel")?) {
        global(&h, "installTransportAdapter", || {
            Ok(vec![
                xhr.clone(),
                options.clone(),
                get(&h, &exchange, "name")?,
            ])
        })?;
    }
    let request_h = h.clone();
    let request_options = options;
    let request_exchange = exchange.clone();
    method(
        &h,
        get(&h, &get(&h, &xhr, "interceptors")?, "request")?,
        "use",
        || {
            Ok(vec![(h.value.function)(
                "",
                1,
                Rc::new(move |args| {
                    let result = request_body(
                        &request_h,
                        &request_options,
                        &request_exchange,
                        argument(&request_h, &args, 0),
                    );
                    Ok((request_h.promise)(Box::pin(async move { result })))
                }),
            )])
        },
    )?;
    let unwrapped = (h.construct)((h.value.global)("WeakSet")?, vec![])?;
    let response_h = h.clone();
    let response_exchange = exchange.clone();
    let response_xhr = xhr.clone();
    let rejection_h = h.clone();
    method(
        &h,
        get(&h, &get(&h, &xhr, "interceptors")?, "response")?,
        "use",
        || {
            Ok(vec![
                (h.value.function)(
                    "",
                    1,
                    Rc::new(move |args| {
                        let result = response_body(
                            &response_h,
                            &response_exchange,
                            &response_xhr,
                            &unwrapped,
                            argument(&response_h, &args, 0),
                        );
                        Ok((response_h.promise)(Box::pin(async move { result })))
                    }),
                ),
                (h.value.function)(
                    "",
                    1,
                    Rc::new(move |args| {
                        let result = rejection_body(
                            &rejection_h,
                            &exchange,
                            argument(&rejection_h, &args, 0),
                        );
                        Ok((rejection_h.promise)(Box::pin(async move { result })))
                    }),
                ),
            ])
        },
    )?;
    Ok(xhr)
}

fn initialize_exchange<V: Clone + 'static, E: Clone + 'static>(
    h: Rc<RuntimeOps<V, E>>,
    receiver: V,
) -> Result<(), E> {
    // Class field order is the original declaration order; declare fields are absent.
    let rule = global(&h, "patternClientOrderIdRule", || {
        Ok(vec![
            (h.value.literal)("Crypto.com"),
            (h.regexp)("^.{1,36}$", "")?,
        ])
    })?;
    (h.define)(&receiver, "clientOrderIdRule", rule)?;
    (h.define)(&receiver, "xhr", (h.value.undefined)())?;
    (h.define)(
        &receiver,
        "_suppressCryptocomValidationErrorEmission",
        (h.value.boolean)(false),
    )?;
    (h.define)(
        &receiver,
        "grafanaEndpointsEvery",
        (h.value.global)("GRAFANA_ENDPOINTS_EVERY")?,
    )?;
    (h.define)(
        &receiver,
        "grafanaEndpointsPeriodic",
        (h.value.global)("GRAFANA_ENDPOINTS_PERIODIC")?,
    )?;
    for name in FIELD_MEMBERS {
        let value = match *name {
            "start" => {
                let ops = h.clone();
                let this = receiver.clone();
                (h.value.function)(
                    "start",
                    0,
                    Rc::new(move |_| Ok((ops.promise)(start(ops.clone(), this.clone())))),
                )
            }
            "tick" => {
                let ops = h.clone();
                let this = receiver.clone();
                (h.value.function)(
                    "tick",
                    0,
                    Rc::new(move |args| {
                        Ok((ops.promise)(tick(
                            ops.clone(),
                            this.clone(),
                            argument(&ops, &args, 0),
                        )))
                    }),
                )
            }
            _ => crate::class_exchange::member(h.binding.clone(), receiver.clone(), name),
        };
        (h.define)(&receiver, name, value)?;
    }
    Ok(())
}
pub const FIELD_MEMBERS: &[&str] = &[
    "dispose",
    "getAccount",
    "validateAccount",
    "start",
    "tick",
    "fetchMarkets",
    "fetchTickers",
    "fetchOHLCV",
    "fetchOrderBook",
    "fetchOrders",
    "fetchPositions",
    "placeOrder",
    "cancelOrder",
    "cancelOrders",
    "cancelSymbolOrders",
    "updateOrder",
    "setLeverage",
    "cancelAllOrders",
    "changePositionMode",
    "changeMarginType",
    "placeOrders",
    "fetchBalance",
    "listenOHLCV",
    "listenOrderBook",
    "mapOrder",
    "mapOrderStatus",
    "mapOrderType",
    "mapCryptocomOrderType",
    "mapTimeInForce",
    "mapStopTriggerType",
    "mapCryptocomTimeInForce",
];
pub fn exchange_constructor<V: Clone + 'static, E: Clone + 'static>(
    h: Rc<RuntimeOps<V, E>>,
    active: V,
    target: V,
    args: Vec<V>,
) -> Result<V, E> {
    let base = (h.superclass)(&active)?;
    let name = argument(&h, &args, 0);
    let options = argument(&h, &args, 1);
    let store = argument(&h, &args, 2);
    let receiver = (h.super_construct)(base, target, vec![name, options.clone(), store])?;
    initialize_exchange(h.clone(), receiver.clone())?;
    let limit_h = h.clone();
    let xhr = own(&h, &receiver, "createXhrWithSecondaryKey", || {
        Ok(vec![
            options,
            (h.value.global)("createAPI")?,
            (h.value.object)(vec![(
                "rateLimit",
                (h.value.function)(
                    "rateLimit",
                    1,
                    Rc::new(move |args| {
                        global(&limit_h, "rateLimit", || {
                            Ok(vec![
                                argument(&limit_h, &args, 0),
                                (limit_h.value.object)(vec![(
                                    "maxRPS",
                                    (limit_h.value.number)(50.),
                                )]),
                            ])
                        })
                    }),
                ),
            )]),
        ])
    })?;
    (h.value.set)(&receiver, "xhr", xhr)?;
    let public = (h.construct)(
        (h.value.global)("CryptocomPublicWebSocket")?,
        vec![receiver.clone()],
    )?;
    (h.value.set)(&receiver, "publicWebsocket", public)?;
    let private = (h.construct)(
        (h.value.global)("CryptocomPrivateWebSocket")?,
        vec![receiver.clone()],
    )?;
    (h.value.set)(&receiver, "privateWebsocket", private)?;
    Ok(receiver)
}
pub fn define_exchange<V: Clone + 'static, E: Clone + 'static>(
    h: Rc<RuntimeOps<V, E>>,
) -> Result<V, E> {
    let base = (h.value.global)("BaseExchange")?;
    let ops = h.clone();
    let class = (h.derive)(
        "CryptocomExchange",
        base,
        3,
        Rc::new(move |active, target, args| {
            exchange_constructor(ops.clone(), active, target, args)
        }),
    )?;
    let getter_h = h.clone();
    (h.prototype_getter)(
        &class,
        "suppressCryptocomValidationErrorEmission",
        Rc::new(move |receiver| {
            get(
                &getter_h,
                &receiver,
                "_suppressCryptocomValidationErrorEmission",
            )
        }),
    )?;
    let method_h = h.clone();
    (h.prototype_method)(
        &class,
        "getPositionMode",
        0,
        Rc::new(move |_, _| {
            let result = get(
                &method_h,
                &(method_h.value.global)("PositionMode")?,
                "Hedged",
            );
            Ok((method_h.promise)(Box::pin(async move { result })))
        }),
    )?;
    Ok(class)
}

#[cfg(test)]
mod accepted_runtime_helpers {
    use super::*;
    use crate::native_bindings::BindingOps;
    use safe_cex_exchange::js_runtime::value::{Js, Thrown};
    use std::cell::RefCell;
    type Trace = Rc<RefCell<Vec<&'static str>>>;
    fn fixture(globals: Js, trace: Trace) -> RuntimeOps<Js, Thrown> {
        let object_trace = trace.clone();
        let value = Rc::new(SocketOps {
            undefined: Rc::new(|| Js::Undefined),
            boolean: Rc::new(Js::Bool),
            number: Rc::new(Js::Number),
            literal: Rc::new(Js::str),
            object: Rc::new(move |fields| {
                object_trace.borrow_mut().push("object");
                Js::object(fields)
            }),
            array: Rc::new(Js::array),
            get: Rc::new(|v, k| v.get(k)),
            index: Rc::new(|_, _| panic!("unused index")),
            key: Rc::new(|_| panic!("unused key")),
            set: Rc::new(|_, _, _| panic!("unused set")),
            set_index: Rc::new(|_, _, _| panic!("unused set_index")),
            delete: Rc::new(|_, _| panic!("unused delete")),
            global: Rc::new(move |name| globals.get(name)),
            invoke: Rc::new(|f, this, args| f.call(this, args)),
            function: Rc::new(|_, _, _| panic!("unused function")),
            truthy: Rc::new(Js::truthy),
            nullish: Rc::new(Js::is_nullish),
            equal: Rc::new(|a, b| a.strict_equals(b)),
            is_number: Rc::new(|_| panic!("unused is_number")),
            has: Rc::new(|_, _| panic!("unused has")),
            numeric: Rc::new(|_| panic!("unused numeric")),
            text: Rc::new(|v| Ok(v.to_js_string()?.to_vec())),
            utf16: Rc::new(|v| Js::utf16(&v)),
            error_value: Rc::new(|_| panic!("unused error_value")),
            spread: Rc::new(|_| panic!("unused spread")),
            spread_object: Rc::new(|v| {
                Ok(v.own_entries()?
                    .into_iter()
                    .map(|(k, v)| (Js::utf16(&k), v))
                    .collect())
            }),
            object_keys: Rc::new(|fields| {
                let v = Js::object(vec![]);
                for (key, item) in fields {
                    v.define_key(
                        &key.to_js_string().expect("runtime helper expected success"),
                        item,
                    )
                    .expect("runtime helper expected success");
                }
                v
            }),
            tuple_two: Rc::new(|_| panic!("unused tuple_two")),
            await_value: Rc::new(|_| panic!("unused await_value")),
        });
        let binding = Rc::new(BindingOps {
            null: Rc::new(|| Js::Null),
            is_string: Rc::new(Js::is_string),
            value: value.clone(),
            binary: Rc::new(|_, _, _| panic!("unused binary")),
            construct: Rc::new(|_, _| panic!("unused construct")),
            throw_value: Rc::new(Thrown::new),
            promise: Rc::new(|_| panic!("unused promise")),
            iterator: Rc::new(|_| panic!("unused iterator")),
            next: Rc::new(|_| panic!("unused next")),
            close: Rc::new(|_| panic!("unused close")),
            close_throw: Rc::new(|_, _| panic!("unused close_throw")),
        });
        RuntimeOps {
            value,
            is_string: Rc::new(Js::is_string),
            regexp: Rc::new(|_, _| panic!("unused regexp")),
            binary: Rc::new(|_, _, _| panic!("unused binary")),
            construct: Rc::new(|_, _| panic!("unused construct")),
            throw_value: Rc::new(Thrown::new),
            promise: Rc::new(|_| panic!("unused promise")),
            define: Rc::new(|_, _, _| panic!("unused define")),
            superclass: Rc::new(|_| panic!("unused superclass")),
            super_construct: Rc::new(|_, _, _| panic!("unused super_construct")),
            derive: Rc::new(|_, _, _, _| panic!("unused derive")),
            prototype_method: Rc::new(|_, _, _, _| panic!("unused prototype_method")),
            prototype_getter: Rc::new(|_, _, _| panic!("unused prototype_getter")),
            binding,
        }
    }
    fn empty() -> (RuntimeOps<Js, Thrown>, Trace) {
        let trace = Rc::new(RefCell::new(vec![]));
        (fixture(Js::object(vec![]), trace.clone()), trace)
    }
    fn token() -> Js {
        Js::object(vec![])
    }
    fn same(v: Js, expected: &Js) {
        assert!(v.same_reference(expected), "runtime helper native identity");
    }
    fn thrown(result: Result<Js, Thrown>, expected: &Js) {
        match result {
            Err(e) => same(e.value, expected),
            Ok(_) => panic!("runtime helper swallowed error"),
        }
    }
    fn property(receiver: &Js, key: &str, value: Js, trace: Trace) {
        receiver
            .define_accessor(
                key,
                Some(Rc::new(move |_| {
                    trace.borrow_mut().push("get");
                    Ok(value.clone())
                })),
                None,
            )
            .expect("fresh extensible accessor target");
    }
    #[test]
    fn get_native_contract() {
        let (h, trace) = empty();
        let object = token();
        let result = token();
        property(&object, "item", result.clone(), trace.clone());
        same(
            get(&h, &object, "item").expect("runtime helper expected success"),
            &result,
        );
        assert_eq!(*trace.borrow(), ["get"], "runtime helper get trace");
        let error = token();
        let kept = error.clone();
        object
            .define_accessor(
                "bad",
                Some(Rc::new(move |_| Err(Thrown::new(kept.clone())))),
                None,
            )
            .expect("fresh extensible accessor target");
        thrown(get(&h, &object, "bad"), &error);
    }
    #[test]
    fn optional_native_contract() {
        let (h, trace) = empty();
        for value in [Js::Null, Js::Undefined] {
            assert!(matches!(
                optional(&h, value, "item").expect("runtime helper expected success"),
                Js::Undefined
            ));
        }
        assert!(trace.borrow().is_empty());
        let object = token();
        let result = token();
        property(&object, "item", result.clone(), trace.clone());
        same(
            optional(&h, object, "item").expect("runtime helper expected success"),
            &result,
        );
        assert_eq!(*trace.borrow(), ["get"], "runtime helper optional trace");
        let error = token();
        let kept = error.clone();
        let object = token();
        object
            .define_accessor(
                "item",
                Some(Rc::new(move |_| Err(Thrown::new(kept.clone())))),
                None,
            )
            .expect("fresh extensible accessor target");
        thrown(optional(&h, object, "item"), &error);
    }
    fn method_probe(own_call: bool) {
        let (h, trace) = empty();
        let receiver = token();
        let result = token();
        let arg = token();
        let expected_this = receiver.clone();
        let expected_arg = arg.clone();
        let expected_result = result.clone();
        let invoked = trace.clone();
        let f = Js::function("actual", move |this, args| {
            invoked.borrow_mut().push("invoke");
            assert!(this.same_reference(&expected_this));
            assert_eq!(args.len(), 1, "runtime helper argument count");
            assert!(args[0].same_reference(&expected_arg));
            Ok(expected_result.clone())
        });
        property(&receiver, "run", f, trace.clone());
        let args_trace = trace.clone();
        let args = || {
            args_trace.borrow_mut().push("args");
            Ok(vec![arg])
        };
        let got = if own_call {
            own(&h, &receiver, "run", args)
        } else {
            method(&h, receiver.clone(), "run", args)
        };
        same(got.expect("runtime helper expected success"), &result);
        assert_eq!(
            *trace.borrow(),
            ["get", "args", "invoke"],
            "runtime helper method/own order"
        );
        let error = token();
        let kept = error.clone();
        receiver
            .define_accessor(
                "bad",
                Some(Rc::new(move |_| Err(Thrown::new(kept.clone())))),
                None,
            )
            .expect("fresh extensible accessor target");
        let got = if own_call {
            own(&h, &receiver, "bad", || panic!("args after getter error"))
        } else {
            method(&h, receiver.clone(), "bad", || {
                panic!("args after getter error")
            })
        };
        thrown(got, &error);
        let error = token();
        let kept = error.clone();
        let args = || Err(Thrown::new(kept));
        let got = if own_call {
            own(&h, &receiver, "run", args)
        } else {
            method(&h, receiver, "run", args)
        };
        thrown(got, &error);
    }
    #[test]
    fn method_native_contract() {
        method_probe(false);
    }
    #[test]
    fn own_native_contract() {
        method_probe(true);
    }
    #[test]
    fn global_native_contract() {
        let trace = Rc::new(RefCell::new(vec![]));
        let globals = token();
        let result = token();
        let kept = result.clone();
        let invoked = trace.clone();
        let arg = token();
        let expected_arg = arg.clone();
        let f = Js::function("global", move |this, args| {
            invoked.borrow_mut().push("invoke");
            assert!(
                matches!(this, Js::Undefined),
                "runtime helper global receiver"
            );
            assert_eq!(args.len(), 1, "runtime helper argument count");
            assert!(args[0].same_reference(&expected_arg));
            Ok(kept.clone())
        });
        property(&globals, "task", f, trace.clone());
        let h = fixture(globals.clone(), trace.clone());
        let args_trace = trace.clone();
        same(
            global(&h, "task", || {
                args_trace.borrow_mut().push("args");
                Ok(vec![arg])
            })
            .expect("runtime helper expected success"),
            &result,
        );
        assert_eq!(
            *trace.borrow(),
            ["get", "args", "invoke"],
            "runtime helper global order"
        );
        let error = token();
        let kept = error.clone();
        globals
            .define_accessor(
                "bad",
                Some(Rc::new(move |_| Err(Thrown::new(kept.clone())))),
                None,
            )
            .expect("fresh extensible accessor target");
        thrown(
            global(&h, "bad", || panic!("args after global getter error")),
            &error,
        );
    }
    #[test]
    fn text_native_contract() {
        let (h, _) = empty();
        let raw = Js::utf16(&[0xd800, 0x61, 0xdc00]);
        let output = text(&h, "Ω", raw, "🚀").expect("runtime helper expected success");
        assert_eq!(
            output
                .to_js_string()
                .expect("runtime helper expected success")
                .to_vec(),
            [937, 0xd800, 0x61, 0xdc00, 55357, 56960],
            "runtime helper text UTF16"
        );
        let error = token();
        let kept = error.clone();
        let object = token();
        object
            .define(
                "toString",
                Js::function("throws", move |_, _| Err(Thrown::new(kept.clone()))),
            )
            .expect("runtime helper expected success");
        thrown(text(&h, "prefix", object, "suffix"), &error);
    }
    #[test]
    fn loaded_native_contract() {
        let (h, _) = empty();
        let retained = token();
        let original = Js::object(vec![
            ("old", retained.clone()),
            ("markets", Js::Bool(false)),
        ]);
        let receiver = Js::object(vec![(
            "store",
            Js::object(vec![("loaded", original.clone())]),
        )]);
        let output =
            loaded(&h, &receiver, &["markets", "orders"]).expect("runtime helper expected success");
        same(
            output.get("old").expect("runtime helper expected success"),
            &retained,
        );
        assert!(
            matches!(
                output
                    .get("markets")
                    .expect("runtime helper expected success"),
                Js::Bool(true)
            ),
            "runtime helper loaded markets flag"
        );
        assert!(
            matches!(
                output
                    .get("orders")
                    .expect("runtime helper expected success"),
                Js::Bool(true)
            ),
            "runtime helper loaded orders flag"
        );
        assert!(matches!(
            original
                .get("markets")
                .expect("runtime helper expected success"),
            Js::Bool(false)
        ));
        assert!(matches!(
            original
                .get("orders")
                .expect("runtime helper expected success"),
            Js::Undefined
        ));
        assert!(
            !output.same_reference(&original),
            "runtime helper loaded snapshot"
        );
        let copy = loaded(&h, &receiver, &[]).expect("runtime helper expected success");
        same(
            copy.get("old").expect("runtime helper expected success"),
            &retained,
        );
        assert!(matches!(
            copy.get("markets")
                .expect("runtime helper expected success"),
            Js::Bool(false)
        ));
    }
    #[test]
    fn update_native_contract() {
        let (h, trace) = empty();
        let store = token();
        let expected_store = store.clone();
        let value = token();
        let expected_value = value.clone();
        let invoked = trace.clone();
        let f = Js::function("update", move |this, args| {
            invoked.borrow_mut().push("invoke");
            assert!(this.same_reference(&expected_store));
            assert_eq!(args.len(), 1, "runtime helper argument count");
            assert!(
                args[0]
                    .get("balance")
                    .expect("runtime helper expected success")
                    .same_reference(&expected_value)
            );
            Ok(token())
        });
        property(&store, "update", f, trace.clone());
        let receiver = Js::object(vec![("store", store.clone())]);
        let fields_trace = trace.clone();
        update(&h, &receiver, || {
            fields_trace.borrow_mut().push("fields");
            Ok(vec![("balance", value)])
        })
        .expect("runtime helper expected success");
        assert_eq!(
            *trace.borrow(),
            ["get", "fields", "object", "invoke"],
            "runtime helper update order"
        );
        let error = token();
        let kept = error.clone();
        store
            .define_accessor(
                "update",
                Some(Rc::new(move |_| Err(Thrown::new(kept.clone())))),
                None,
            )
            .expect("fresh extensible accessor target");
        match update(&h, &receiver, || panic!("fields after getter error")) {
            Err(e) => same(e.value, &error),
            Ok(_) => panic!("runtime helper update swallowed error"),
        };
    }
}
