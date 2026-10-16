use std::{cell::RefCell, rc::Rc};

pub type IngressCallback<V, E> = Rc<dyn Fn(Vec<V>) -> Result<V, E>>;
pub struct IngressOps<V, E> {
    pub undefined: Rc<dyn Fn() -> V>,
    pub boolean: Rc<dyn Fn(bool) -> V>,
    pub number: Rc<dyn Fn(f64) -> V>,
    pub literal: Rc<dyn Fn(&str) -> V>,
    pub object: Rc<dyn Fn(Vec<(&'static str, V)>) -> V>,
    pub array: Rc<dyn Fn(Vec<V>) -> V>,
    pub get: Rc<dyn Fn(&V, &str) -> Result<V, E>>,
    pub index: Rc<dyn Fn(&V, &V) -> Result<V, E>>,
    pub key: Rc<dyn Fn(&V) -> Result<V, E>>,
    pub set: Rc<dyn Fn(&V, &str, V) -> Result<(), E>>,
    pub set_index: Rc<dyn Fn(&V, &V, V) -> Result<(), E>>,
    pub delete: Rc<dyn Fn(&V, &V) -> Result<bool, E>>,
    pub global: Rc<dyn Fn(&str) -> Result<V, E>>,
    pub invoke: Rc<dyn Fn(V, V, Vec<V>) -> Result<V, E>>,
    pub function: Rc<dyn Fn(&str, usize, IngressCallback<V, E>) -> V>,
    pub truthy: Rc<dyn Fn(&V) -> bool>,
    pub nullish: Rc<dyn Fn(&V) -> bool>,
    pub equal: Rc<dyn Fn(&V, &V) -> bool>,
    pub text: Rc<dyn Fn(&V) -> Result<Vec<u16>, E>>,
    pub utf16: Rc<dyn Fn(Vec<u16>) -> V>,
    pub error_value: Rc<dyn Fn(E) -> V>,
    pub throw: Rc<dyn Fn(V) -> E>,
    pub spread: Rc<dyn Fn(&V) -> Result<Vec<V>, E>>,
    pub tuple_two: Rc<dyn Fn(&V) -> Result<(V, V), E>>,
    pub construct: Rc<dyn Fn(V, Vec<V>) -> Result<V, E>>,
    pub is_function: Rc<dyn Fn(&V) -> bool>,
    pub is_object: Rc<dyn Fn(&V) -> bool>,
    pub for_of: Rc<dyn Fn(V, Rc<dyn Fn(V) -> Result<(), E>>) -> Result<(), E>>,
    pub less: Rc<dyn Fn(&V, &V) -> Result<bool, E>>,
    pub greater: Rc<dyn Fn(&V, &V) -> Result<bool, E>>,
    pub less_equal: Rc<dyn Fn(&V, &V) -> Result<bool, E>>,
    pub for_of_until: Rc<dyn Fn(V, Rc<dyn Fn(V) -> Result<bool, E>>) -> Result<(), E>>,
    pub subtract: Rc<dyn Fn(V, V) -> Result<V, E>>,
    pub multiply: Rc<dyn Fn(V, V) -> Result<V, E>>,
    pub add_primitive: Rc<dyn Fn(V, V) -> Result<V, E>>,
    pub tdz_error: Rc<dyn Fn(&str) -> E>,
}
fn get<V, E>(h: &IngressOps<V, E>, object: &V, key: &str) -> Result<V, E> {
    (h.get)(object, key)
}
fn method<V: Clone, E>(
    h: &IngressOps<V, E>,
    receiver: V,
    key: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let callee = get(h, &receiver, key)?;
    (h.invoke)(callee, receiver, args()?)
}
fn global<V, E>(
    h: &IngressOps<V, E>,
    name: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let callee = (h.global)(name)?;
    (h.invoke)(callee, (h.undefined)(), args()?)
}
fn own<V: Clone, E>(
    h: &IngressOps<V, E>,
    receiver: &V,
    name: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    method(h, receiver.clone(), name, args)
}
fn arg<V: Clone, E>(h: &IngressOps<V, E>, args: &[V], index: usize) -> V {
    args.get(index).cloned().unwrap_or_else(|| (h.undefined)())
}
fn path<V, E>(h: &IngressOps<V, E>, object: &V, keys: &[&str]) -> Result<V, E> {
    let mut value = get(h, object, keys[0])?;
    for key in &keys[1..] {
        value = get(h, &value, key)?;
    }
    Ok(value)
}
fn or<V, E>(
    h: &IngressOps<V, E>,
    value: V,
    fallback: impl FnOnce() -> Result<V, E>,
) -> Result<V, E> {
    if (h.truthy)(&value) {
        Ok(value)
    } else {
        fallback()
    }
}
fn nullish<V, E>(
    h: &IngressOps<V, E>,
    value: V,
    fallback: impl FnOnce() -> Result<V, E>,
) -> Result<V, E> {
    if (h.nullish)(&value) {
        fallback()
    } else {
        Ok(value)
    }
}
fn optional<V, E>(h: &IngressOps<V, E>, value: V, key: &str) -> Result<V, E> {
    if (h.nullish)(&value) {
        Ok((h.undefined)())
    } else {
        get(h, &value, key)
    }
}
fn optional_index<V, E>(h: &IngressOps<V, E>, value: V, key: V) -> Result<V, E> {
    if (h.nullish)(&value) {
        Ok((h.undefined)())
    } else {
        (h.index)(&value, &key)
    }
}
fn is_undefined<V, E>(h: &IngressOps<V, E>, value: &V) -> bool {
    (h.equal)(value, &(h.undefined)())
}
fn stringify<V, E>(h: &IngressOps<V, E>, value: impl FnOnce() -> Result<V, E>) -> Result<V, E> {
    global(h, "String", || Ok(vec![value()?]))
}
fn template<V, E>(h: &IngressOps<V, E>, parts: Vec<V>) -> Result<V, E> {
    let mut units = Vec::new();
    for part in parts {
        units.extend((h.text)(&part)?);
    }
    Ok((h.utf16)(units))
}
pub fn strip_unsupported_headers<V: Clone + 'static, E: 'static>(
    h: Rc<IngressOps<V, E>>,
    headers: V,
) -> Result<V, E> {
    if !(h.truthy)(&headers) || !(h.is_object)(&headers) {
        return Ok((h.undefined)());
    }
    let unsupported = (h.global)("UNSUPPORTED_LIGHTER_REST_HEADERS")?;
    let ops = h.clone();
    (h.for_of)(
        unsupported,
        Rc::new(move |unsupported| {
            if (ops.is_function)(&get(&ops, &headers, "delete")?) {
                method(&ops, headers.clone(), "delete", || {
                    Ok(vec![unsupported.clone()])
                })?;
            }
            let names = object_method(&ops, "keys", || Ok(vec![headers.clone()]))?;
            let lang = ops.clone();
            let record = headers.clone();
            (ops.for_of)(
                names,
                Rc::new(move |name| {
                    let lower = method(&lang, name.clone(), "toLowerCase", || Ok(vec![]))?;
                    if (lang.equal)(&lower, &unsupported) {
                        let key = (lang.key)(&name)?;
                        (lang.delete)(&record, &key)?;
                    }
                    Ok(())
                }),
            )?;
            Ok(())
        }),
    )?;
    Ok((h.undefined)())
}
fn emit_error<V: Clone, E>(h: &IngressOps<V, E>, exchange: &V, message: &V) -> Result<V, E> {
    own(h, exchange, "emitAndLogError", || Ok(vec![message.clone()]))?;
    let ctor = (h.global)("Error")?;
    (h.construct)(ctor, vec![message.clone()])
}
pub fn create_sdk_client<V: Clone + 'static, E: 'static>(
    h: Rc<IngressOps<V, E>>,
    options: V,
    exchange: V,
) -> Result<V, E> {
    let base_url = global(&h, "getLighterApiBaseUrl", || {
        Ok(vec![(h.object)(vec![
            ("exchangeName", get(&h, &exchange, "name")?),
            ("testnet", get(&h, &options, "testnet")?),
        ])])
    })?;
    let ctor = (h.global)("LighterApiClient")?;
    let config = (h.object)(vec![
        ("baseUrl", base_url),
        (
            "timeout",
            nullish(
                &h,
                optional(&h, optional(&h, options, "extra")?, "recvWindow")?,
                || (h.global)("RECV_WINDOW"),
            )?,
        ),
        ("debug", (h.boolean)(false)),
        (
            "headers",
            (h.object)(vec![("Content-Type", (h.literal)("application/json"))]),
        ),
    ]);
    let client = (h.construct)(ctor, vec![config])?;
    let ops = h.clone();
    own(&h, &client, "addRequestInterceptor", || {
        Ok(vec![(h.function)(
            "",
            1,
            Rc::new(move |args| {
                let config = arg(&ops, &args, 0);
                strip_unsupported_headers(ops.clone(), get(&ops, &config, "headers")?)?;
                Ok(config)
            }),
        )])
    })?;
    let ops = h.clone();
    let owner = exchange.clone();
    own(&h, &client, "addResponseInterceptor", || {
        Ok(vec![(h.function)(
            "",
            1,
            Rc::new(move |args| {
                let response = arg(&ops, &args, 0);
                if (ops.truthy)(&optional(&ops, get(&ops, &response, "data")?, "error")?) {
                    let code = path(&ops, &response, &["data", "error", "code"])?;
                    let code = (ops.text)(&code)?;
                    let message = path(&ops, &response, &["data", "error", "message"])?;
                    let msg = (ops.utf16)(
                        "[".encode_utf16()
                            .chain(code)
                            .chain("] ".encode_utf16())
                            .chain((ops.text)(&message)?)
                            .collect(),
                    );
                    return emit_error(&ops, &owner, &msg)
                        .and_then(|error| Err((ops.throw)(error)));
                }
                if (ops.equal)(
                    &optional(&ops, get(&ops, &response, "data")?, "success")?,
                    &(ops.boolean)(false),
                ) {
                    let message =
                        optional(&ops, path(&ops, &response, &["data", "error"])?, "message")?;
                    let msg = or(&ops, message, || Ok((ops.literal)("Request failed")))?;
                    return emit_error(&ops, &owner, &msg)
                        .and_then(|error| Err((ops.throw)(error)));
                }
                Ok(response)
            }),
        )])
    })?;
    let ops = h.clone();
    own(&h, &client, "addErrorInterceptor", || {
        Ok(vec![(h.function)(
            "",
            1,
            Rc::new(move |args| {
                let error = arg(&ops, &args, 0);
                let suppress = global(&ops, "Boolean", || {
                    Ok(vec![optional(
                        &ops,
                        optional(&ops, error.clone(), "config")?,
                        "__suppressExchangeError",
                    )?])
                })?;
                if (ops.truthy)(&suppress) {
                    return Ok(error);
                }
                let url = stringify(&ops, || {
                    or(
                        &ops,
                        optional(&ops, optional(&ops, error.clone(), "config")?, "url")?,
                        || Ok((ops.literal)("")),
                    )
                })?;
                if (ops.truthy)(&method(&ops, url, "includes", || {
                    Ok(vec![(ops.literal)("/api/v1/sendTx")])
                })?) {
                    return Ok(error);
                }
                if (ops.truthy)(&get(&ops, &error, "message")?) {
                    own(&ops, &exchange, "emitAndLogError", || {
                        Ok(vec![get(&ops, &error, "message")?])
                    })?;
                }
                Ok(error)
            }),
        )])
    })?;
    Ok(client)
}
fn object_method<V: Clone, E>(
    h: &IngressOps<V, E>,
    name: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    method(h, (h.global)("Object")?, name, args)
}
fn number_method<V: Clone, E>(
    h: &IngressOps<V, E>,
    name: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    method(h, (h.global)("Number")?, name, args)
}
pub fn to_order_book_level<V: Clone, E>(h: &IngressOps<V, E>, level: &V) -> Result<V, E> {
    let price = global(h, "parseFloat", || {
        Ok(vec![nullish(h, get(h, level, "price")?, || {
            (h.index)(level, &(h.number)(0.))
        })?])
    })?;
    let amount = global(h, "parseFloat", || {
        Ok(vec![nullish(
            h,
            get(h, level, "remaining_base_amount")?,
            || {
                nullish(h, get(h, level, "remainingBaseAmount")?, || {
                    nullish(h, get(h, level, "size")?, || {
                        nullish(h, get(h, level, "quantity")?, || {
                            (h.index)(level, &(h.number)(1.))
                        })
                    })
                })
            },
        )?])
    })?;
    if !(h.truthy)(&number_method(h, "isFinite", || Ok(vec![price.clone()]))?)
        || !(h.truthy)(&number_method(h, "isFinite", || Ok(vec![amount.clone()]))?)
        || (h.less_equal)(&price, &(h.number)(0.))?
        || (h.less)(&amount, &(h.number)(0.))?
    {
        return Ok((h.undefined)());
    }
    Ok((h.object)(vec![
        ("price", price),
        ("amount", amount),
        ("total", (h.number)(0.)),
    ]))
}
pub fn aggregate_snapshot_levels<V: Clone + 'static, E: 'static>(
    h: Rc<IngressOps<V, E>>,
    receiver: V,
    levels: V,
) -> Result<V, E> {
    let ctor = (h.global)("Map")?;
    let map = (h.construct)(ctor, vec![])?;
    let ops = h.clone();
    let collected = map.clone();
    (h.for_of)(
        levels,
        Rc::new(move |raw| {
            let level = own(&ops, &receiver, "toOrderBookLevel", || Ok(vec![raw]))?;
            if !(ops.truthy)(&level)
                || (ops.less_equal)(&get(&ops, &level, "amount")?, &(ops.number)(0.))?
            {
                return Ok(());
            }
            let existing = method(&ops, collected.clone(), "get", || {
                Ok(vec![get(&ops, &level, "price")?])
            })?;
            if (ops.truthy)(&existing) {
                let amount = global(&ops, "add", || {
                    Ok(vec![
                        get(&ops, &existing, "amount")?,
                        get(&ops, &level, "amount")?,
                    ])
                })?;
                (ops.set)(&existing, "amount", amount)?;
            } else {
                method(&ops, collected.clone(), "set", || {
                    Ok(vec![get(&ops, &level, "price")?, level])
                })?;
            }
            Ok(())
        }),
    )?;
    let values = method(&h, map, "values", || Ok(vec![]))?;
    Ok((h.array)((h.spread)(&values)?))
}
pub fn apply_book_levels<V: Clone + 'static, E: 'static>(
    h: Rc<IngressOps<V, E>>,
    receiver: V,
    book: V,
    side: V,
    levels: V,
    mode: V,
) -> Result<V, E> {
    if (h.equal)(&mode, &(h.literal)("snapshot")) {
        let key = (h.key)(&side)?;
        let aggregated = own(&h, &receiver, "aggregateOrderBookSnapshotLevels", || {
            Ok(vec![levels])
        })?;
        let ops = h.clone();
        let direction = side.clone();
        let sorted = method(&h, aggregated, "sort", || {
            Ok(vec![(h.function)(
                "",
                2,
                Rc::new(move |args| {
                    let left = arg(&ops, &args, 0);
                    let right = arg(&ops, &args, 1);
                    if (ops.equal)(&direction, &(ops.literal)("bids")) {
                        (ops.subtract)(get(&ops, &right, "price")?, get(&ops, &left, "price")?)
                    } else {
                        (ops.subtract)(get(&ops, &left, "price")?, get(&ops, &right, "price")?)
                    }
                }),
            )])
        })?;
        (h.set_index)(&book, &key, sorted)?;
        return Ok((h.undefined)());
    }
    let ops = h.clone();
    method(&h, levels, "forEach", || {
        Ok(vec![(h.function)(
            "",
            1,
            Rc::new(move |args| {
                let level = own(&ops, &receiver, "toOrderBookLevel", || {
                    Ok(vec![arg(&ops, &args, 0)])
                })?;
                if (ops.truthy)(&level) {
                    global(&ops, "updateOrderBook", || {
                        Ok(vec![book.clone(), side.clone(), level])
                    })?;
                }
                Ok((ops.undefined)())
            }),
        )])
    })?;
    Ok((h.undefined)())
}
pub fn on_message<V: Clone + 'static, E: 'static>(
    h: Rc<IngressOps<V, E>>,
    receiver: V,
    event: &V,
) -> Result<V, E> {
    let data = get(&h, event, "data")?;
    if (h.truthy)(&get(&h, &receiver, "isDisposed")?) {
        return Ok((h.undefined)());
    }
    if !(h.truthy)(&get(&h, &receiver, "hasReceivedMessageSinceConnect")?) {
        (h.set)(
            &receiver,
            "hasReceivedMessageSinceConnect",
            (h.boolean)(true),
        )?;
        (h.set)(&receiver, "failedConnections", (h.number)(0.))?;
        (h.set)(&receiver, "onCloseCount", (h.number)(0.))?;
        if !(h.truthy)(&get(&h, &receiver, "connectedViaProxyRoute")?)
            && (h.greater)(
                &get(&h, &receiver, "directRouteFailureCount")?,
                &(h.number)(0.),
            )?
        {
            (h.set)(&receiver, "directRouteFailureCount", (h.number)(0.))?;
        }
    }
    own(&h, &receiver, "trackLastReceived", || Ok(vec![]))?;
    let handlers = object_method(&h, "entries", || {
        Ok(vec![get(&h, &receiver, "messageHandlers")?])
    })?;
    let json = global(&h, "jsonParse", || Ok(vec![data.clone()]))?;
    if (h.truthy)(&method(&h, data.clone(), "includes", || {
        Ok(vec![(h.literal)("type\":\"ping")])
    })?) {
        own(&h, &receiver, "sendPayload", || {
            Ok(vec![(h.object)(vec![
                ("type", (h.literal)("pong")),
                (
                    "timestamp",
                    method(&h, (h.global)("virtualClock")?, "getCurrentTimeMs", || {
                        Ok(vec![])
                    })?,
                ),
            ])])
        })?;
    }
    if (h.truthy)(&optional(&h, json.clone(), "error")?) {
        let code = nullish(&h, path(&h, &json, &["error", "code"])?, || {
            Ok((h.literal)("unknown"))
        })?;
        let message = nullish(&h, path(&h, &json, &["error", "message"])?, || {
            Ok((h.literal)("Unknown public stream error"))
        })?;
        own(&h, &receiver, "recordMessageHandlerStats", || {
            Ok(vec![(h.literal)("error")])
        })?;
        method(&h, get(&h, &receiver, "parent")?, "log", || {
            Ok(vec![
                template(
                    &h,
                    vec![
                        (h.literal)("[Lighter] Public WS error "),
                        code,
                        (h.literal)(": "),
                        message,
                    ],
                )?,
                get(&h, &(h.global)("LogSeverity")?, "Warning")?,
            ])
        })?;
    }
    let ops = h.clone();
    let owner = receiver.clone();
    (h.for_of)(
        handlers,
        Rc::new(move |entry| {
            let (topic, callbacks) = (ops.tuple_two)(&entry)?;
            if (ops.truthy)(&global(&ops, "shouldDispatchLighterPublicMessage", || {
                Ok(vec![topic.clone(), data.clone()])
            })?) {
                own(&ops, &owner, "recordMessageHandlerStats", || {
                    Ok(vec![topic])
                })?;
                let lang = ops.clone();
                let frame = json.clone();
                (ops.for_of)(
                    callbacks,
                    Rc::new(move |callback| {
                        if (lang.truthy)(&frame) {
                            (lang.invoke)(callback, (lang.undefined)(), vec![frame.clone()])?;
                        }
                        Ok(())
                    }),
                )?;
            }
            Ok(())
        }),
    )?;
    own(
        &h,
        &receiver,
        "syncTickerRestFallbackToPriceHydration",
        || {
            Ok(vec![(h.literal)(
                "public websocket is active but ticker prices are not hydrated yet; polling tickers via REST",
            )])
        },
    )?;
    Ok((h.undefined)())
}
fn fields_or<V, E>(
    h: &IngressOps<V, E>,
    object: &V,
    fields: &[&str],
    fallback: impl FnOnce() -> Result<V, E>,
) -> Result<V, E> {
    for field in fields {
        let value = get(h, object, field)?;
        if (h.truthy)(&value) {
            return Ok(value);
        }
    }
    fallback()
}
fn parse_fields<V, E>(h: &IngressOps<V, E>, object: &V, fields: &[&str]) -> Result<V, E> {
    global(h, "parseLighterWsNumber", || {
        fields.iter().map(|key| get(h, object, key)).collect()
    })
}
fn find_ticker<V: Clone + 'static, E: 'static>(
    h: Rc<IngressOps<V, E>>,
    receiver: &V,
    symbol: V,
    market_id: V,
    stats: bool,
) -> Result<V, E> {
    let tickers = path(&h, receiver, &["parent", "store", "tickers"])?;
    let ops = h.clone();
    let id = market_id.clone();
    let by_symbol = symbol.clone();
    let first = method(&h, tickers, "find", || {
        Ok(vec![(h.function)(
            "",
            1,
            Rc::new(move |args| {
                let ticker = arg(&ops, &args, 0);
                let matches = if stats {
                    ((ops.truthy)(&by_symbol)
                        && (ops.equal)(&get(&ops, &ticker, "symbol")?, &by_symbol))
                        || (!is_undefined(&ops, &id)
                            && (ops.equal)(
                                &get(&ops, &ticker, "id")?,
                                &stringify(&ops, || Ok(id.clone()))?,
                            ))
                } else {
                    (ops.truthy)(&by_symbol)
                        && ((ops.equal)(&get(&ops, &ticker, "symbol")?, &by_symbol)
                            || (ops.equal)(&get(&ops, &ticker, "id")?, &by_symbol))
                };
                Ok((ops.boolean)(matches))
            }),
        )])
    })?;
    if stats || (h.truthy)(&first) {
        return Ok(first);
    }
    let tickers = path(&h, receiver, &["parent", "store", "tickers"])?;
    let ops = h.clone();
    method(&h, tickers, "find", || {
        Ok(vec![(h.function)(
            "",
            1,
            Rc::new(move |args| {
                Ok((ops.boolean)(
                    (ops.truthy)(&market_id)
                        && (ops.equal)(&get(&ops, &arg(&ops, &args, 0), "id")?, &market_id),
                ))
            }),
        )])
    })
}
fn side_price<V: Clone, E>(h: &IngressOps<V, E>, data: &V, bid: bool) -> Result<V, E> {
    let fields: &[&str] = if bid {
        &[
            "bidPrice",
            "bid_price",
            "bestBidPrice",
            "best_bid_price",
            "bestBid",
            "best_bid",
            "b",
        ]
    } else {
        &[
            "askPrice",
            "ask_price",
            "bestAskPrice",
            "best_ask_price",
            "bestAsk",
            "best_ask",
            "a",
        ]
    };
    let (full, short) = if bid { ("bid", "b") } else { ("ask", "a") };
    global(h, "parseLighterWsNumber", || {
        let mut args = fields
            .iter()
            .map(|key| get(h, data, key))
            .collect::<Result<Vec<_>, E>>()?;
        args.push(optional_index(h, get(h, data, full)?, (h.number)(0.))?);
        args.push(optional_index(h, get(h, data, short)?, (h.number)(0.))?);
        args.push(optional(h, get(h, data, short)?, "price")?);
        args.push(optional(h, get(h, data, full)?, "price")?);
        args.push(global(h, "parseLighterWsBookSidePrice", || {
            Ok(vec![get(h, data, full)?])
        })?);
        args.push(global(h, "parseLighterWsBookSidePrice", || {
            Ok(vec![get(h, data, short)?])
        })?);
        Ok(args)
    })
}
fn normalized_funding<V, E>(h: &IngressOps<V, E>, rate: V) -> Result<V, E> {
    global(h, "normalizeLighterFundingRate", || Ok(vec![rate]))
}
pub fn ticker_event<V: Clone + 'static, E: 'static>(
    h: Rc<IngressOps<V, E>>,
    receiver: V,
    json: V,
) -> Result<V, E> {
    let data = or(&h, get(&h, &json, "data")?, || {
        or(&h, get(&h, &json, "ticker")?, || Ok(json.clone()))
    })?;
    if !(h.truthy)(&data) {
        return Ok((h.undefined)());
    }
    let channel = stringify(&h, || {
        or(&h, get(&h, &json, "channel")?, || Ok((h.literal)("")))
    })?;
    let id = global(&h, "extractLighterTickerMarketIdFromChannel", || {
        Ok(vec![channel])
    })?;
    let symbol = stringify(&h, || {
        fields_or(
            &h,
            &data,
            &["symbol", "marketSymbol", "market_symbol", "s"],
            || or(&h, get(&h, &json, "symbol")?, || Ok((h.literal)(""))),
        )
    })?;
    let ticker = find_ticker(h.clone(), &receiver, symbol.clone(), id, false)?;
    if !(h.truthy)(&ticker) {
        return Ok((h.undefined)());
    }
    let update = (h.object)(vec![]);
    let last = parse_fields(
        &h,
        &data,
        &[
            "lastPrice",
            "last_price",
            "lastTradePrice",
            "last_trade_price",
            "last",
            "price",
        ],
    )?;
    let mark = parse_fields(&h, &data, &["markPrice", "mark_price", "mark"])?;
    let index = parse_fields(&h, &data, &["indexPrice", "index_price", "index"])?;
    let bid = side_price(&h, &data, true)?;
    let ask = side_price(&h, &data, false)?;
    let volume = parse_fields(&h, &data, &["volume24h", "volume_24h", "volume"])?;
    let quote = parse_fields(
        &h,
        &data,
        &[
            "quoteVolume24h",
            "quote_volume_24h",
            "quoteVolume",
            "quote_volume",
        ],
    )?;
    let interest = parse_fields(
        &h,
        &data,
        &[
            "openInterest",
            "open_interest",
            "oi",
            "openInterestUsd",
            "open_interest_usd",
        ],
    )?;
    let funding = parse_fields(
        &h,
        &data,
        &["fundingRate", "funding_rate", "currentFundingRate"],
    )?;
    let percentage = parse_fields(&h, &data, &["priceChange24h", "price_change_24h"])?;
    for (key, value) in [
        ("last", last),
        ("mark", mark.clone()),
        ("index", index.clone()),
    ] {
        if !is_undefined(&h, &value) {
            (h.set)(&update, key, value)?;
        }
    }
    if is_undefined(&h, &mark) && !is_undefined(&h, &index) {
        (h.set)(&update, "mark", index)?;
    }
    for (key, value) in [("percentage", percentage), ("openInterest", interest)] {
        if !is_undefined(&h, &value) {
            (h.set)(&update, key, value)?;
        }
    }
    if !is_undefined(&h, &funding) {
        (h.set)(&update, "fundingRate", normalized_funding(&h, funding)?)?;
    }
    for (key, value) in [
        ("volume", volume),
        ("quoteVolume", quote),
        ("bid", bid),
        ("ask", ask),
    ] {
        if !is_undefined(&h, &value) {
            (h.set)(&update, key, value)?;
        }
    }
    method(
        &h,
        path(&h, &receiver, &["parent", "store"])?,
        "updateTicker",
        || Ok(vec![ticker.clone(), update.clone()]),
    )?;
    let resolved_symbol = or(&h, symbol, || get(&h, &ticker, "symbol"))?;
    let reference = or(&h, get(&h, &update, "mark")?, || get(&h, &update, "last"))?;
    if (h.truthy)(&reference) {
        let positions = path(&h, &receiver, &["store", "positions"])?;
        let ops = h.clone();
        let sym = resolved_symbol.clone();
        let matching = method(&h, positions, "filter", || {
            Ok(vec![(h.function)(
                "",
                1,
                Rc::new(move |args| {
                    let p = arg(&ops, &args, 0);
                    Ok((ops.boolean)(
                        (ops.equal)(&get(&ops, &p, "symbol")?, &sym)
                            && !(ops.equal)(&get(&ops, &p, "contracts")?, &(ops.number)(0.)),
                    ))
                }),
            )])
        })?;
        let ops = h.clone();
        (h.for_of_until)(
            matching,
            Rc::new(move |position| {
                let markets = path(&ops, &receiver, &["store", "markets"])?;
                let lang = ops.clone();
                let sym = resolved_symbol.clone();
                let market = method(&ops, markets, "find", || {
                    Ok(vec![(ops.function)(
                        "",
                        1,
                        Rc::new(move |args| {
                            Ok((lang.boolean)((lang.equal)(
                                &get(&lang, &arg(&lang, &args, 0), "symbol")?,
                                &sym,
                            )))
                        }),
                    )])
                })?;
                if !(ops.truthy)(&market) || (ops.truthy)(&get(&ops, &market, "isInverse")?) {
                    return Ok(false);
                }
                let entry = (ops.multiply)(
                    get(&ops, &position, "contracts")?,
                    get(&ops, &position, "entryPrice")?,
                )?;
                let current =
                    (ops.multiply)(get(&ops, &position, "contracts")?, reference.clone())?;
                let direction = (ops.number)(
                    if (ops.equal)(
                        &get(&ops, &position, "side")?,
                        &get(&ops, &(ops.global)("PositionSide")?, "Long")?,
                    ) {
                        1.
                    } else {
                        -1.
                    },
                );
                let pnl = (ops.multiply)(direction, (ops.subtract)(current, entry)?)?;
                method(
                    &ops,
                    get(&ops, &receiver, "store")?,
                    "updatePosition",
                    || Ok(vec![position, (ops.object)(vec![("unrealizedPnl", pnl)])]),
                )?;
                Ok(true)
            }),
        )?;
    }
    Ok((h.undefined)())
}
pub fn market_stats_event<V: Clone + 'static, E: 'static>(
    h: Rc<IngressOps<V, E>>,
    receiver: V,
    json: V,
) -> Result<V, E> {
    let stats = nullish(&h, get(&h, &json, "market_stats")?, || {
        optional(&h, get(&h, &json, "data")?, "market_stats")
    })?;
    if !(h.truthy)(&stats) {
        return Ok((h.undefined)());
    }
    let stats = object_method(&h, "values", || Ok(vec![stats]))?;
    let ops = h.clone();
    (h.for_of)(
        stats,
        Rc::new(move |stats| {
            let symbol = stringify(&ops, || {
                or(&ops, optional(&ops, stats.clone(), "symbol")?, || {
                    Ok((ops.literal)(""))
                })
            })?;
            let id = nullish(&ops, optional(&ops, stats.clone(), "market_id")?, || {
                optional(&ops, stats.clone(), "marketId")
            })?;
            let ticker = find_ticker(ops.clone(), &receiver, symbol, id, true)?;
            if !(ops.truthy)(&ticker) {
                return Ok(());
            }
            let mut parsed = Vec::new();
            for (key, fields) in [
                ("mark", &["mark_price", "markPrice", "mark"][..]),
                ("index", &["index_price", "indexPrice", "index"][..]),
                (
                    "last",
                    &["last_trade_price", "lastTradePrice", "last", "price"][..],
                ),
                ("volume", &["daily_base_token_volume", "volume"][..]),
                (
                    "quoteVolume",
                    &["daily_quote_token_volume", "quote_volume"][..],
                ),
                ("openInterest", &["open_interest", "openInterest"][..]),
                ("fundingRate", &["current_funding_rate", "funding_rate"][..]),
                ("percentage", &["daily_price_change"][..]),
                ("bid", &["best_bid_price", "bid_price", "bid"][..]),
                ("ask", &["best_ask_price", "ask_price", "ask"][..]),
            ] {
                parsed.push((key, parse_fields(&ops, &stats, fields)?));
            }
            method(
                &ops,
                path(&ops, &receiver, &["parent", "store"])?,
                "updateTicker",
                || {
                    let mut fields = Vec::new();
                    for (key, value) in parsed {
                        if !is_undefined(&ops, &value) {
                            fields.push((
                                key,
                                if key == "fundingRate" {
                                    normalized_funding(&ops, value)?
                                } else {
                                    value
                                },
                            ));
                        }
                    }
                    Ok(vec![ticker, (ops.object)(fields)])
                },
            )?;
            Ok(())
        }),
    )?;
    Ok((h.undefined)())
}
fn static_value<V, E>(h: &IngressOps<V, E>, name: &str) -> Result<V, E> {
    get(h, &(h.global)("LighterPublicWebsocket")?, name)
}
fn send_subscribe<V: Clone, E>(h: &IngressOps<V, E>, receiver: &V, topic: &V) -> Result<V, E> {
    if (h.truthy)(&get(h, receiver, "isDisposed")?)
        || !(h.truthy)(&get(h, receiver, "isConnected")?)
        || (h.truthy)(&get(h, receiver, "reconnecting")?)
    {
        return Ok((h.undefined)());
    }
    let sent = own(h, receiver, "sendPayload", || {
        Ok(vec![(h.object)(vec![
            ("type", (h.literal)("subscribe")),
            ("channel", topic.clone()),
        ])])
    });
    if let Err(error) = sent {
        method(h, get(h, receiver, "parent")?, "log", || {
            let topic_text = (h.text)(topic)?;
            let error = (h.error_value)(error);
            let description = or(h, optional(h, error.clone(), "message")?, || {
                stringify(h, || Ok(error))
            })?;
            Ok(vec![
                template(
                    h,
                    vec![
                        (h.literal)("[Lighter] Public WS delayed subscribe failed for "),
                        (h.utf16)(topic_text),
                        (h.literal)(": "),
                        description,
                    ],
                )?,
                get(h, &(h.global)("LogSeverity")?, "Warning")?,
            ])
        })?;
    }
    Ok((h.undefined)())
}
pub fn subscribe<V: Clone + 'static, E: 'static>(
    h: Rc<IngressOps<V, E>>,
    receiver: V,
) -> Result<V, E> {
    own(&h, &receiver, "clearPendingSubscribeTimeouts", || {
        Ok(vec![])
    })?;
    let flattened = global(&h, "flatten", || {
        Ok(vec![object_method(&h, "values", || {
            Ok(vec![get(&h, &receiver, "topics")?])
        })?])
    })?;
    let ops = h.clone();
    let mapped = method(&h, flattened, "map", || {
        Ok(vec![(h.function)(
            "",
            1,
            Rc::new(move |args| {
                let converted = stringify(&ops, || {
                    or(&ops, arg(&ops, &args, 0), || Ok((ops.literal)("")))
                })?;
                method(&ops, converted, "trim", || Ok(vec![]))
            }),
        )])
    })?;
    let ops = h.clone();
    let topics = method(&h, mapped, "filter", || {
        Ok(vec![(h.function)(
            "",
            1,
            Rc::new(move |args| {
                Ok((ops.boolean)((ops.greater)(
                    &get(&ops, &arg(&ops, &args, 0), "length")?,
                    &(ops.number)(0.),
                )?))
            }),
        )])
    })?;
    let unique = method(&h, (h.global)("Array")?, "from", || {
        let ctor = (h.global)("Set")?;
        Ok(vec![(h.construct)(ctor, vec![topics])?])
    })?;
    let ops = h.clone();
    method(&h, unique, "forEach", || {
        Ok(vec![(h.function)(
            "",
            2,
            Rc::new(move |args| {
                let topic = arg(&ops, &args, 0);
                let index = arg(&ops, &args, 1);
                if (ops.less)(
                    &index,
                    &static_value(&ops, "MAX_IMMEDIATE_SUBSCRIBE_MESSAGES")?,
                )? {
                    send_subscribe(&ops, &receiver, &topic)?;
                    return Ok((ops.undefined)());
                }
                let delay = (ops.multiply)(
                    (ops.add_primitive)(
                        (ops.subtract)(
                            index,
                            static_value(&ops, "MAX_IMMEDIATE_SUBSCRIBE_MESSAGES")?,
                        )?,
                        (ops.number)(1.),
                    )?,
                    static_value(&ops, "SUBSCRIBE_STAGGER_DELAY_MS")?,
                )?;
                let binding = Rc::new(RefCell::new(None::<V>));
                let lang = ops.clone();
                let owner = receiver.clone();
                let slot = binding.clone();
                let timeout = global(&ops, "setTimeout", || {
                    Ok(vec![
                        (ops.function)(
                            "",
                            0,
                            Rc::new(move |_| {
                                let timers = get(&lang, &owner, "pendingSubscribeTimeouts")?;
                                let value_ops = lang.clone();
                                let slot = slot.clone();
                                let filtered = method(&lang, timers, "filter", || {
                                    Ok(vec![(lang.function)(
                                        "",
                                        1,
                                        Rc::new(move |args| {
                                            let id = arg(&value_ops, &args, 0);
                                            let timeout =
                                                slot.borrow().clone().ok_or_else(|| {
                                                    (value_ops.tdz_error)("timeoutId")
                                                })?;
                                            Ok((value_ops.boolean)(!(value_ops.equal)(
                                                &id, &timeout,
                                            )))
                                        }),
                                    )])
                                })?;
                                (lang.set)(&owner, "pendingSubscribeTimeouts", filtered)?;
                                send_subscribe(&lang, &owner, &topic)?;
                                Ok((lang.undefined)())
                            }),
                        ),
                        delay,
                    ])
                })?;
                binding.replace(Some(timeout.clone()));
                method(
                    &ops,
                    get(&ops, &receiver, "pendingSubscribeTimeouts")?,
                    "push",
                    || Ok(vec![timeout]),
                )?;
                Ok((ops.undefined)())
            }),
        )])
    })?;
    Ok((h.undefined)())
}
