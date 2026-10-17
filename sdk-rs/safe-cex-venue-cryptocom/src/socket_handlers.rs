use std::{future::Future, pin::Pin, rc::Rc};

pub type SocketFuture<V, E> = Pin<Box<dyn Future<Output = Result<V, E>>>>;
pub type SocketCallback<V, E> = Rc<dyn Fn(Vec<V>) -> Result<V, E>>;
pub struct SocketOps<V, E> {
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
    pub function: Rc<dyn Fn(&str, usize, SocketCallback<V, E>) -> V>,
    pub truthy: Rc<dyn Fn(&V) -> bool>,
    pub nullish: Rc<dyn Fn(&V) -> bool>,
    pub equal: Rc<dyn Fn(&V, &V) -> bool>,
    pub is_number: Rc<dyn Fn(&V) -> bool>,
    pub has: Rc<dyn Fn(&V, &str) -> Result<bool, E>>,
    pub numeric: Rc<dyn Fn(&V) -> Result<f64, E>>,
    pub text: Rc<dyn Fn(&V) -> Result<Vec<u16>, E>>,
    pub utf16: Rc<dyn Fn(Vec<u16>) -> V>,
    pub error_value: Rc<dyn Fn(E) -> V>,
    pub spread: Rc<dyn Fn(&V) -> Result<Vec<V>, E>>,
    pub spread_object: Rc<dyn Fn(&V) -> Result<Vec<(V, V)>, E>>,
    pub object_keys: Rc<dyn Fn(Vec<(V, V)>) -> V>,
    pub tuple_two: Rc<dyn Fn(&V) -> Result<(V, V), E>>,
    pub await_value: Rc<dyn Fn(V) -> SocketFuture<V, E>>,
}
fn get<V, E>(h: &SocketOps<V, E>, object: &V, key: &str) -> Result<V, E> {
    (h.get)(object, key)
}
fn method<V: Clone, E>(
    h: &SocketOps<V, E>,
    receiver: V,
    key: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let callee = get(h, &receiver, key)?;
    (h.invoke)(callee, receiver, args()?)
}
fn global<V, E>(
    h: &SocketOps<V, E>,
    name: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let callee = (h.global)(name)?;
    (h.invoke)(callee, (h.undefined)(), args()?)
}
fn own<V: Clone, E>(
    h: &SocketOps<V, E>,
    receiver: &V,
    name: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    method(h, receiver.clone(), name, args)
}
fn store<V, E>(h: &SocketOps<V, E>, receiver: &V) -> Result<V, E> {
    get(h, receiver, "store")
}
fn market<V, E>(h: &SocketOps<V, E>, receiver: &V, update: &V) -> Result<V, E> {
    let markets = get(h, &store(h, receiver)?, "marketsById")?;
    (h.index)(&markets, &get(h, update, "instrument_name")?)
}
fn ticker<V, E>(h: &SocketOps<V, E>, receiver: &V, market: &V) -> Result<V, E> {
    let tickers = get(h, &store(h, receiver)?, "tickersBySymbol")?;
    (h.index)(&tickers, &get(h, market, "symbol")?)
}
fn first<V, E>(h: &SocketOps<V, E>, update: &V) -> Result<V, E> {
    (h.index)(&get(h, update, "data")?, &(h.number)(0.))
}
fn parse<V, E>(h: &SocketOps<V, E>, value: impl FnOnce() -> Result<V, E>) -> Result<V, E> {
    global(h, "parseFloat", || Ok(vec![value()?]))
}
fn parsed_field<V, E>(h: &SocketOps<V, E>, data: &V, key: &str) -> Result<V, E> {
    parse(h, || get(h, data, key))
}
fn or<V, E>(
    h: &SocketOps<V, E>,
    value: V,
    fallback: impl FnOnce() -> Result<V, E>,
) -> Result<V, E> {
    if (h.truthy)(&value) {
        Ok(value)
    } else {
        fallback()
    }
}
fn coalesce<V, E>(
    h: &SocketOps<V, E>,
    value: V,
    fallback: impl FnOnce() -> Result<V, E>,
) -> Result<V, E> {
    if (h.nullish)(&value) {
        fallback()
    } else {
        Ok(value)
    }
}
fn parsed_or<V, E>(h: &SocketOps<V, E>, data: &V, key: &str, default: f64) -> Result<V, E> {
    or(h, parsed_field(h, data, key)?, || Ok((h.number)(default)))
}
fn math<V: Clone, E>(
    h: &SocketOps<V, E>,
    name: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    method(h, (h.global)("Math")?, name, args)
}
fn clock<V: Clone, E>(h: &SocketOps<V, E>) -> Result<V, E> {
    method(h, (h.global)("virtualClock")?, "getCurrentTimeMs", || {
        Ok(vec![])
    })
}
fn concat<V, E>(h: &SocketOps<V, E>, prefix: &str, value: &V) -> Result<V, E> {
    Ok((h.utf16)(
        prefix.encode_utf16().chain((h.text)(value)?).collect(),
    ))
}
fn emit<V: Clone, E>(
    h: &SocketOps<V, E>,
    receiver: &V,
    message: impl FnOnce() -> Result<V, E>,
) -> Result<(), E> {
    method(h, get(h, receiver, "parent")?, "emitAndLogError", || {
        Ok(vec![message()?])
    })?;
    Ok(())
}
fn caught<V: Clone, E>(
    h: &SocketOps<V, E>,
    receiver: &V,
    prefix: &str,
    result: Result<(), E>,
) -> Result<(), E> {
    if let Err(error) = result {
        emit(h, receiver, || concat(h, prefix, &(h.error_value)(error)))?;
    }
    Ok(())
}
fn route_message<V: Clone, E>(
    h: &SocketOps<V, E>,
    receiver: &V,
    data: V,
    private: bool,
) -> Result<(), E> {
    let json = global(h, "jsonParse", || Ok(vec![data]))?;
    if !(h.truthy)(&json) {
        return Ok(());
    }
    if (h.equal)(&get(h, &json, "method")?, &(h.literal)("public/heartbeat"))
        && (h.is_number)(&get(h, &json, "id")?)
    {
        own(h, receiver, "send", || {
            Ok(vec![method(h, (h.global)("JSON")?, "stringify", || {
                Ok(vec![(h.object)(vec![
                    ("id", get(h, &json, "id")?),
                    ("method", (h.literal)("public/respond-heartbeat")),
                ])])
            })?])
        })?;
        return Ok(());
    }
    if private
        && (h.has)(&json, "method")?
        && (h.equal)(&get(h, &json, "method")?, &(h.literal)("public/auth"))
        && (h.equal)(&get(h, &json, "code")?, &(h.number)(0.))
    {
        (h.set)(receiver, "isAuthenticated", (h.boolean)(true))?;
        own(h, receiver, "resubscribe", || Ok(vec![]))?;
        return Ok(());
    }
    if (h.truthy)(&get(h, &json, "result")?)
        && (h.truthy)(&get(h, &get(h, &json, "result")?, "channel")?)
    {
        let handler = if private {
            let channel = get(h, &get(h, &json, "result")?, "channel")?;
            own(h, receiver, "recordMessageHandlerStats", || {
                Ok(vec![channel.clone()])
            })?;
            if (h.truthy)(&method(h, channel.clone(), "startsWith", || {
                Ok(vec![(h.literal)("user.order")])
            })?) || (h.truthy)(&method(h, channel.clone(), "startsWith", || {
                Ok(vec![(h.literal)("user.advance.order")])
            })?) {
                Some("handleOrderUpdate")
            } else if (h.equal)(&channel, &(h.literal)("user.positions")) {
                Some("handlePositionUpdate")
            } else if (h.equal)(&channel, &(h.literal)("user.balance")) {
                Some("handleBalanceUpdate")
            } else {
                None
            }
        } else {
            own(h, receiver, "recordMessageHandlerStats", || {
                Ok(vec![get(h, &get(h, &json, "result")?, "channel")?])
            })?;
            // Public switch re-reads result/channel after recording statistics.
            let channel = get(h, &get(h, &json, "result")?, "channel")?;
            if (h.equal)(&channel, &(h.literal)("ticker")) {
                Some("handleTickerUpdate")
            } else if (h.equal)(&channel, &(h.literal)("trade")) {
                Some("handleTradeUpdate")
            } else if (h.equal)(&channel, &(h.literal)("book"))
                || (h.equal)(&channel, &(h.literal)("book.update"))
            {
                Some("handleBookUpdate")
            } else if (h.equal)(&channel, &(h.literal)("candlestick")) {
                Some("handleCandlestickUpdate")
            } else if ["mark", "index", "funding"]
                .iter()
                .any(|s| (h.equal)(&channel, &(h.literal)(s)))
            {
                Some("handleValuationUpdate")
            } else {
                None
            }
        };
        if let Some(handler) = handler {
            own(h, receiver, handler, || Ok(vec![get(h, &json, "result")?]))?;
        }
        return Ok(());
    }
    if (h.has)(&json, "code")? && !(h.equal)(&get(h, &json, "code")?, &(h.number)(0.)) {
        emit(h, receiver, || {
            let code = (h.text)(&get(h, &json, "code")?)?;
            let message = or(h, get(h, &json, "message")?, || {
                Ok((h.literal)("Unknown error"))
            })?;
            Ok((h.utf16)(
                (if private {
                    "Private WebSocket error "
                } else {
                    "WebSocket error "
                })
                .encode_utf16()
                .chain(code)
                .chain(": ".encode_utf16())
                .chain((h.text)(&message)?)
                .collect(),
            ))
        })?;
    }
    Ok(())
}
pub fn public_on_message<V: Clone, E>(
    h: &SocketOps<V, E>,
    receiver: &V,
    event: &V,
) -> Result<V, E> {
    on_message(h, receiver, event, false)
}
pub fn private_on_message<V: Clone, E>(
    h: &SocketOps<V, E>,
    receiver: &V,
    event: &V,
) -> Result<V, E> {
    on_message(h, receiver, event, true)
}
fn on_message<V: Clone, E>(
    h: &SocketOps<V, E>,
    receiver: &V,
    event: &V,
    private: bool,
) -> Result<V, E> {
    let data = get(h, event, "data")?;
    if !(h.truthy)(&get(h, receiver, "isDisposed")?) {
        own(h, receiver, "trackLastReceived", || Ok(vec![]))?;
        caught(
            h,
            receiver,
            if private {
                "Error parsing private WebSocket message: "
            } else {
                "Error parsing WebSocket message: "
            },
            route_message(h, receiver, data, private),
        )?;
    }
    Ok((h.undefined)())
}
pub fn ticker_update<V: Clone, E>(h: &SocketOps<V, E>, receiver: &V, update: &V) -> Result<V, E> {
    caught(
        h,
        receiver,
        "Error handling ticker update: ",
        (|| {
            let market = market(h, receiver, update)?;
            if !(h.truthy)(&market) {
                return Ok(());
            }
            let ticker = ticker(h, receiver, &market)?;
            if !(h.truthy)(&ticker) {
                return Ok(());
            }
            let data = first(h, update)?;
            if !(h.truthy)(&data) {
                return Ok(());
            }
            let patch = (h.object)(vec![
                ("bid", parsed_field(h, &data, "b")?),
                ("ask", parsed_field(h, &data, "k")?),
                ("last", parsed_field(h, &data, "a")?),
                (
                    "percentage",
                    (h.number)((h.numeric)(&parsed_field(h, &data, "c")?)? * 100.),
                ),
                ("volume", parsed_field(h, &data, "v")?),
                ("quoteVolume", parsed_field(h, &data, "vv")?),
                (
                    "openInterest",
                    if (h.truthy)(&get(h, &data, "oi")?) {
                        parsed_field(h, &data, "oi")?
                    } else {
                        (h.number)(0.)
                    },
                ),
            ]);
            method(h, store(h, receiver)?, "updateTicker", || {
                Ok(vec![ticker, patch])
            })?;
            Ok(())
        })(),
    )?;
    Ok((h.undefined)())
}
pub fn book_update<V: Clone + 'static, E: 'static>(
    h: Rc<SocketOps<V, E>>,
    receiver: V,
    update: V,
) -> Result<V, E> {
    caught(
        &h,
        &receiver,
        "Error handling book update: ",
        (|| {
            let market = market(&h, &receiver, &update)?;
            if !(h.truthy)(&market) {
                return Ok(());
            }
            let data = first(&h, &update)?;
            if !(h.truthy)(&data) {
                return Ok(());
            }
            let ticker = ticker(&h, &receiver, &market)?;
            if (h.truthy)(&ticker) {
                method(&h, store(&h, &receiver)?, "updateTicker", || {
                    let best = |side| -> Result<V, E> {
                        let row = (h.index)(&get(&h, &data, side)?, &(h.number)(0.))?;
                        if (h.truthy)(&row) {
                            parse(&h, || {
                                (h.index)(
                                    &(h.index)(&get(&h, &data, side)?, &(h.number)(0.))?,
                                    &(h.number)(0.),
                                )
                            })
                        } else {
                            Ok((h.number)(0.))
                        }
                    };
                    Ok(vec![
                        ticker,
                        (h.object)(vec![("bid", best("bids")?), ("ask", best("asks")?)]),
                    ])
                })?;
            }
            let handler = concat(&h, "book.", &get(&h, &update, "instrument_name")?)?;
            let callbacks = or(
                &h,
                (h.index)(&get(&h, &receiver, "messageHandlers")?, &handler)?,
                || Ok((h.array)(vec![])),
            )?;
            let ops = h.clone();
            let frame = update.clone();
            method(&h, callbacks, "forEach", || {
                Ok(vec![(h.function)(
                    "",
                    1,
                    Rc::new(move |args| {
                        let callback = args.first().cloned().unwrap_or_else(|| (ops.undefined)());
                        (ops.invoke)(callback, (ops.undefined)(), vec![frame.clone()])
                    }),
                )])
            })?;
            Ok(())
        })(),
    )?;
    Ok((h.undefined)())
}
pub fn trade_update<V, E>(h: &SocketOps<V, E>, _update: &V) -> Result<V, E> {
    Ok((h.undefined)())
}
fn throttled<V: Clone + 'static, E: 'static>(
    h: Rc<SocketOps<V, E>>,
    receiver: V,
    book: bool,
) -> Result<V, E> {
    global(&h, "throttle", || {
        let ops = h.clone();
        let callback = (h.function)(
            "",
            1,
            Rc::new(move |args| {
                let update = args.first().cloned().unwrap_or_else(|| (ops.undefined)());
                if book {
                    book_update(ops.clone(), receiver.clone(), update)
                } else {
                    ticker_update(&ops, &receiver, &update)
                }
            }),
        );
        Ok(vec![
            callback,
            (h.number)(if book { 100. } else { 1000. }),
            (h.object)(vec![
                ("leading", (h.boolean)(true)),
                ("trailing", (h.boolean)(true)),
            ]),
        ])
    })
}
pub fn initialize_ticker_handler<V: Clone + 'static, E: 'static>(
    h: Rc<SocketOps<V, E>>,
    receiver: V,
) -> Result<V, E> {
    throttled(h, receiver, false)
}
pub fn initialize_book_handler<V: Clone + 'static, E: 'static>(
    h: Rc<SocketOps<V, E>>,
    receiver: V,
) -> Result<V, E> {
    throttled(h, receiver, true)
}
pub fn order_update<V: Clone, E>(h: &SocketOps<V, E>, receiver: &V, update: &V) -> Result<V, E> {
    caught(
        h,
        receiver,
        "Error handling order update: ",
        (|| {
            let mut i = 0.;
            while i < (h.numeric)(&get(h, &get(h, update, "data")?, "length")?)? {
                let row = (h.index)(&get(h, update, "data")?, &(h.number)(i))?;
                let market = market(h, receiver, &row)?;
                if !(h.truthy)(&market) {
                    i += 1.;
                    continue;
                }
                let quantity = parsed_or(h, &row, "quantity", 0.)?;
                let filled = if (h.truthy)(&get(h, &row, "cumulative_quantity")?) {
                    parsed_or(h, &row, "cumulative_quantity", 0.)?
                } else {
                    (h.number)(0.)
                };
                let price = or(
                    h,
                    parse(h, || {
                        coalesce(h, get(h, &row, "ref_price")?, || {
                            coalesce(h, get(h, &row, "trigger_price")?, || {
                                coalesce(h, get(h, &row, "limit_price")?, || Ok((h.literal)("0")))
                            })
                        })
                    })?,
                    || Ok((h.number)(0.)),
                )?;
                let status = coalesce(h, get(h, &row, "status")?, || {
                    coalesce(h, get(h, &row, "order_status")?, || {
                        Ok((h.literal)("ACTIVE"))
                    })
                })?;
                let updated = coalesce(h, get(h, &row, "update_time")?, || {
                    get(h, &row, "time_updated")
                })?;
                let created = coalesce(h, get(h, &row, "create_time")?, || {
                    get(h, &row, "time_created")
                })?;
                let order = (h.object)(vec![
                    ("id", get(h, &row, "order_id")?),
                    ("symbol", get(h, &market, "symbol")?),
                    (
                        "type",
                        own(h, receiver, "mapOrderType", || {
                            Ok(vec![get(h, &row, "order_type")?])
                        })?,
                    ),
                    ("side", {
                        let member = if (h.equal)(&get(h, &row, "side")?, &(h.literal)("BUY")) {
                            "Buy"
                        } else {
                            "Sell"
                        };
                        get(h, &(h.global)("OrderSide")?, member)?
                    }),
                    ("price", price),
                    ("amount", quantity.clone()),
                    ("filled", filled.clone()),
                    (
                        "remaining",
                        math(h, "max", || {
                            Ok(vec![
                                (h.number)(0.),
                                (h.number)((h.numeric)(&quantity)? - (h.numeric)(&filled)?),
                            ])
                        })?,
                    ),
                    (
                        "status",
                        own(h, receiver, "mapOrderStatus", || Ok(vec![status]))?,
                    ),
                    (
                        "lastUpdate",
                        or(h, updated, || or(h, created, || clock(h)))?,
                    ),
                    ("reduceOnly", {
                        let inst = get(h, &row, "exec_inst")?;
                        let result = if (h.nullish)(&inst) {
                            (h.undefined)()
                        } else {
                            method(h, inst, "includes", || Ok(vec![(h.literal)("REDUCE_ONLY")]))?
                        };
                        coalesce(h, result, || Ok((h.boolean)(false)))?
                    }),
                    (
                        "timeInForce",
                        own(h, receiver, "mapTimeInForce", || {
                            Ok(vec![get(h, &row, "time_in_force")?])
                        })?,
                    ),
                ]);
                method(h, store(h, receiver)?, "addOrUpdateOrder", || {
                    Ok(vec![order])
                })?;
                i += 1.;
            }
            Ok(())
        })(),
    )?;
    Ok((h.undefined)())
}
pub fn position_update<V: Clone + 'static, E: 'static>(
    h: Rc<SocketOps<V, E>>,
    receiver: V,
    update: V,
) -> SocketFuture<V, E> {
    // An async source body initiates getPositionMode before returning its promise.
    let mode = (|| {
        method(&h, get(&h, &receiver, "parent")?, "getPositionMode", || {
            Ok(vec![])
        })
    })();
    let mode = match mode {
        Ok(mode) => mode,
        Err(error) => {
            let result = caught(
                &h,
                &receiver,
                "Error handling position update: ",
                Err(error),
            )
            .map(|()| (h.undefined)());
            return Box::pin(std::future::ready(result));
        }
    };
    let await_mode = (h.await_value)(mode);
    Box::pin(async move {
        let body = async {
            let mode = await_mode.await?;
            let mut i = 0.;
            while i < (h.numeric)(&get(&h, &get(&h, &update, "data")?, "length")?)? {
                let row = (h.index)(&get(&h, &update, "data")?, &(h.number)(i))?;
                let market = market(&h, &receiver, &row)?;
                if !(h.truthy)(&market) {
                    i += 1.;
                    continue;
                }
                let quantity = parsed_or(&h, &row, "quantity", 0.)?;
                let cost = parsed_or(&h, &row, "cost", 0.)?;
                if (h.equal)(&quantity, &(h.number)(0.)) {
                    let before = get(&h, &get(&h, &store(&h, &receiver)?, "positions")?, "length")?;
                    let positions = get(&h, &store(&h, &receiver)?, "positions")?;
                    let ops = h.clone();
                    let m = market.clone();
                    let filtered = method(&h, positions, "filter", || {
                        Ok(vec![(h.function)(
                            "",
                            1,
                            Rc::new(move |args| {
                                let p = args.first().cloned().unwrap_or_else(|| (ops.undefined)());
                                Ok((ops.boolean)(!(ops.equal)(
                                    &get(&ops, &p, "symbol")?,
                                    &get(&ops, &m, "symbol")?,
                                )))
                            }),
                        )])
                    })?;
                    if !(h.equal)(&get(&h, &filtered, "length")?, &before) {
                        method(&h, store(&h, &receiver)?, "update", || {
                            Ok(vec![(h.object)(vec![("positions", filtered)])])
                        })?;
                    }
                    i += 1.;
                    continue;
                }
                let side_member = if (h.numeric)(&quantity)? > 0. {
                    "Long"
                } else {
                    "Short"
                };
                let side = get(&h, &(h.global)("PositionSide")?, side_member)?;
                let contracts = math(&h, "abs", || Ok(vec![quantity]))?;
                let entry = or(
                    &h,
                    (h.number)((h.numeric)(&cost)? / (h.numeric)(&contracts)?),
                    || Ok((h.number)(0.)),
                )?;
                let leverage = or(
                    &h,
                    parse(&h, || {
                        coalesce(&h, get(&h, &row, "leverage")?, || {
                            coalesce(&h, get(&h, &row, "target_leverage")?, || {
                                Ok((h.literal)("1"))
                            })
                        })
                    })?,
                    || Ok((h.number)(1.)),
                )?;
                let liquidation = or(
                    &h,
                    parse(&h, || {
                        coalesce(&h, get(&h, &row, "liquidation_price")?, || {
                            Ok((h.literal)("0"))
                        })
                    })?,
                    || Ok((h.number)(0.)),
                )?;
                let mark = or(
                    &h,
                    parse(&h, || {
                        coalesce(&h, get(&h, &row, "mark_price")?, || Ok((h.literal)("0")))
                    })?,
                    || Ok((h.number)(0.)),
                )?;
                let position = (h.object)(vec![
                    (
                        "id",
                        global(&h, "createPositionId", || {
                            Ok(vec![
                                get(&h, &market, "symbol")?,
                                side.clone(),
                                mode.clone(),
                            ])
                        })?,
                    ),
                    ("symbol", get(&h, &market, "symbol")?),
                    ("side", side),
                    ("entryPrice", entry),
                    ("notional", math(&h, "abs", || Ok(vec![cost]))?),
                    ("leverage", leverage),
                    ("marginType", {
                        let member = if (h.equal)(
                            &get(&h, &row, "isolation_type")?,
                            &(h.literal)("ISOLATED_MARGIN"),
                        ) {
                            "Isolated"
                        } else {
                            "Cross"
                        };
                        get(&h, &(h.global)("MarginType")?, member)?
                    }),
                    ("positionMode", mode.clone()),
                    (
                        "unrealizedPnl",
                        parsed_or(&h, &row, "open_position_pnl", 0.)?,
                    ),
                    ("contracts", contracts),
                    ("liquidationPrice", liquidation),
                    (
                        "info",
                        (h.object)(vec![
                            ("markPrice", mark),
                            ("isolationId", get(&h, &row, "isolation_id")?),
                        ]),
                    ),
                    (
                        "lastUpdate",
                        or(&h, get(&h, &row, "update_timestamp_ms")?, || clock(&h))?,
                    ),
                ]);
                method(&h, store(&h, &receiver)?, "addOrUpdatePosition", || {
                    Ok(vec![position])
                })?;
                i += 1.;
            }
            Ok(())
        }
        .await;
        caught(&h, &receiver, "Error handling position update: ", body)?;
        Ok((h.undefined)())
    })
}
pub fn balance_update<V: Clone, E>(h: &SocketOps<V, E>, receiver: &V, update: &V) -> Result<V, E> {
    let summary = first(h, update)?;
    if !(h.truthy)(&summary) {
        return Ok((h.undefined)());
    }
    let currencies = (h.object)(vec![]);
    let mut i = 0.;
    while i
        < (h.numeric)(&get(
            h,
            &or(h, get(h, &summary, "position_balances")?, || {
                Ok((h.array)(vec![]))
            })?,
            "length",
        )?)?
    {
        let row = (h.index)(&get(h, &summary, "position_balances")?, &(h.number)(i))?;
        let quantity = parsed_or(h, &row, "quantity", 0.)?;
        let available = parsed_or(h, &row, "max_withdrawal_balance", 0.)?;
        let key = (h.key)(&get(h, &row, "instrument_name")?)?;
        let currency = (h.object)(vec![
            ("total", quantity.clone()),
            ("free", available.clone()),
            (
                "used",
                math(h, "max", || {
                    Ok(vec![
                        (h.number)(0.),
                        (h.number)((h.numeric)(&quantity)? - (h.numeric)(&available)?),
                    ])
                })?,
            ),
            ("upnl", (h.number)(0.)),
        ]);
        (h.set_index)(&currencies, &key, currency)?;
        i += 1.;
    }
    method(h, store(h, receiver)?, "update", || {
        let balance = (h.object)(vec![
            ("total", parsed_or(h, &summary, "total_margin_balance", 0.)?),
            (
                "free",
                parsed_or(h, &summary, "total_available_balance", 0.)?,
            ),
            ("used", parsed_or(h, &summary, "total_initial_margin", 0.)?),
            (
                "upnl",
                parsed_or(h, &summary, "total_session_unrealized_pnl", 0.)?,
            ),
            ("currencies", currencies),
        ]);
        let mut loaded = (h.spread_object)(&get(h, &store(h, receiver)?, "loaded")?)?;
        let key = (h.literal)("balance");
        if let Some((_, value)) = loaded.iter_mut().find(|(k, _)| (h.equal)(k, &key)) {
            *value = (h.boolean)(true);
        } else {
            loaded.push((key, (h.boolean)(true)));
        }
        Ok(vec![(h.object)(vec![
            ("balance", balance),
            ("loaded", (h.object_keys)(loaded)),
        ])])
    })?;
    Ok((h.undefined)())
}
struct Listener<V, E> {
    ops: Rc<SocketOps<V, E>>,
    receiver: V,
    channel: V,
    handler: V,
    options: V,
    interval: V,
    parser: V,
    book: bool,
}
fn callback_argument<V: Clone, E>(h: &SocketOps<V, E>, args: &[V], index: usize) -> V {
    args.get(index).cloned().unwrap_or_else(|| (h.undefined)())
}
fn candle_parser<V: Clone, E>(
    h: &SocketOps<V, E>,
    options: &V,
    callback: &V,
    update: &V,
) -> Result<V, E> {
    if (h.equal)(
        &get(h, update, "instrument_name")?,
        &get(h, options, "symbol")?,
    ) {
        let data = first(h, update)?;
        if (h.truthy)(&data) {
            (h.invoke)(
                callback.clone(),
                (h.undefined)(),
                vec![(h.object)(vec![
                    ("timestamp", get(h, &data, "t")?),
                    ("open", parsed_field(h, &data, "o")?),
                    ("high", parsed_field(h, &data, "h")?),
                    ("low", parsed_field(h, &data, "l")?),
                    ("close", parsed_field(h, &data, "c")?),
                    ("volume", parsed_field(h, &data, "v")?),
                ])],
            )?;
        }
    }
    Ok((h.undefined)())
}
fn book_parser<V: Clone + 'static, E: 'static>(
    h: Rc<SocketOps<V, E>>,
    symbol: &V,
    callback: &V,
    book: &V,
    update: &V,
) -> Result<V, E> {
    if !(h.equal)(&get(&h, update, "instrument_name")?, symbol) {
        return Ok((h.undefined)());
    }
    let data = first(&h, update)?;
    if !(h.truthy)(&data) {
        return Ok((h.undefined)());
    }
    for side in ["bids", "asks"] {
        let rows = get(&h, &data, side)?;
        let ops = h.clone();
        let levels = method(&h, rows, "map", || {
            Ok(vec![(h.function)(
                "",
                1,
                Rc::new(move |args| {
                    let row = callback_argument(&ops, &args, 0);
                    let (price, quantity) = (ops.tuple_two)(&row)?;
                    Ok((ops.object)(vec![
                        ("price", parse(&ops, || Ok(price))?),
                        ("amount", parse(&ops, || Ok(quantity))?),
                        ("total", (ops.number)(0.)),
                    ]))
                }),
            )])
        })?;
        (h.set)(book, side, levels)?;
    }
    global(&h, "sortOrderBook", || Ok(vec![book.clone()]))?;
    global(&h, "calcOrderBookTotal", || Ok(vec![book.clone()]))?;
    (h.invoke)(callback.clone(), (h.undefined)(), vec![book.clone()])?;
    Ok((h.undefined)())
}
fn listener_wait<V: Clone + 'static, E: 'static>(state: Rc<Listener<V, E>>) -> Result<V, E> {
    let h = &state.ops;
    if (h.truthy)(&get(h, &state.receiver, "isConnected")?) {
        let target = get(h, &state.receiver, "messageHandlers")?;
        let key = (h.key)(&state.handler)?;
        let current = or(
            h,
            (h.index)(&get(h, &state.receiver, "messageHandlers")?, &state.handler)?,
            || Ok((h.array)(vec![])),
        )?;
        let mut callbacks = (h.spread)(&current)?;
        callbacks.push(state.parser.clone());
        (h.set_index)(&target, &key, (h.array)(callbacks))?;
        if state.book {
            own(h, &state.receiver, "subscribeToOrderBook", || {
                Ok(vec![state.options.clone(), (h.number)(50.)])
            })?;
        } else {
            own(h, &state.receiver, "subscribeToCandlestick", || {
                Ok(vec![
                    get(h, &state.options, "symbol")?,
                    state.interval.clone(),
                ])
            })?;
        }
    } else {
        let next = state.clone();
        global(h, "setTimeout", || {
            Ok(vec![
                (h.function)("", 0, Rc::new(move |_| listener_wait(next.clone()))),
                (h.number)(100.),
            ])
        })?;
    }
    Ok((h.undefined)())
}
fn listener_remove<V: Clone + 'static, E: 'static>(state: Rc<Listener<V, E>>) -> Result<V, E> {
    let h = &state.ops;
    let target = get(h, &state.receiver, "messageHandlers")?;
    let key = (h.key)(&state.handler)?;
    let current = or(
        h,
        (h.index)(&get(h, &state.receiver, "messageHandlers")?, &state.handler)?,
        || Ok((h.array)(vec![])),
    )?;
    let ops = h.clone();
    let parser = state.parser.clone();
    let filtered = method(h, current, "filter", || {
        Ok(vec![(h.function)(
            "",
            1,
            Rc::new(move |args| {
                Ok((ops.boolean)(!(ops.equal)(
                    &callback_argument(&ops, &args, 0),
                    &parser,
                )))
            }),
        )])
    })?;
    let copied = (h.array)((h.spread)(&filtered)?);
    (h.set_index)(&target, &key, copied)?;
    if (h.truthy)(&get(h, &state.receiver, "isConnected")?)
        && !(h.truthy)(&get(
            h,
            &(h.index)(&get(h, &state.receiver, "messageHandlers")?, &state.handler)?,
            "length",
        )?)
    {
        own(h, &state.receiver, "unsubscribe", || {
            Ok(vec![(h.array)(vec![state.channel.clone()])])
        })?;
        own(h, &state.receiver, "cleanupMessageHandlerStats", || {
            Ok(vec![state.handler.clone()])
        })?;
        let table = get(h, &state.receiver, "messageHandlers")?;
        let key = (h.key)(&state.handler)?;
        (h.delete)(&table, &key)?;
    }
    Ok((h.undefined)())
}
pub fn listen_ohlcv<V: Clone + 'static, E: 'static>(
    h: Rc<SocketOps<V, E>>,
    receiver: V,
    options: V,
    callback: V,
) -> Result<V, E> {
    let interval = global(&h, "mapCandlestickInterval", || {
        Ok(vec![get(&h, &options, "interval")?])
    })?;
    let channel = (h.utf16)(
        "candlestick."
            .encode_utf16()
            .chain((h.text)(&interval)?)
            .chain(".".encode_utf16())
            .chain((h.text)(&get(&h, &options, "symbol")?)?)
            .collect(),
    );
    let ops = h.clone();
    let opts = options.clone();
    let parser = (h.function)(
        "parser",
        1,
        Rc::new(move |args| {
            candle_parser(&ops, &opts, &callback, &callback_argument(&ops, &args, 0))
        }),
    );
    let state = Rc::new(Listener {
        ops: h.clone(),
        receiver,
        handler: channel.clone(),
        channel,
        options,
        interval,
        parser,
        book: false,
    });
    listener_wait(state.clone())?;
    Ok((h.function)(
        "",
        0,
        Rc::new(move |_| listener_remove(state.clone())),
    ))
}
pub fn listen_order_book<V: Clone + 'static, E: 'static>(
    h: Rc<SocketOps<V, E>>,
    receiver: V,
    symbol: V,
    callback: V,
) -> Result<V, E> {
    let channel = (h.utf16)(
        "book."
            .encode_utf16()
            .chain((h.text)(&symbol)?)
            .chain(".50".encode_utf16())
            .collect(),
    );
    let handler = concat(&h, "book.", &symbol)?;
    let book = (h.object)(vec![
        ("bids", (h.array)(vec![])),
        ("asks", (h.array)(vec![])),
    ]);
    let ops = h.clone();
    let sym = symbol.clone();
    let parser = (h.function)(
        "parser",
        1,
        Rc::new(move |args| {
            book_parser(
                ops.clone(),
                &sym,
                &callback,
                &book,
                &callback_argument(&ops, &args, 0),
            )
        }),
    );
    let state = Rc::new(Listener {
        ops: h.clone(),
        receiver,
        handler,
        channel,
        options: symbol,
        interval: (h.undefined)(),
        parser,
        book: true,
    });
    listener_wait(state.clone())?;
    Ok((h.function)(
        "",
        0,
        Rc::new(move |_| listener_remove(state.clone())),
    ))
}

#[cfg(test)]
mod accepted_helpers {
    use super::*;
    use safe_cex_exchange::js_runtime::value::{Js, Thrown};
    use std::cell::RefCell;
    type Trace = Rc<RefCell<Vec<&'static str>>>;
    fn fixture(globals: Js) -> SocketOps<Js, Thrown> {
        crate::accepted_test_support::socket(globals)
    }
    fn token() -> Js {
        Js::object(vec![])
    }
    fn trace() -> Trace {
        Rc::new(RefCell::new(vec![]))
    }
    fn native(actual: Js, expected: &Js, label: &str) {
        match (&actual, expected) {
            (Js::Number(a), Js::Number(b)) => assert_eq!(a.to_bits(), b.to_bits(), "{label}"),
            _ => assert!(actual.strict_equals(expected), "{label}"),
        }
    }
    fn thrown<T>(result: Result<T, Thrown>, expected: &Js, label: &str) {
        match result {
            Err(e) => assert!(e.value.same_reference(expected), "{label}"),
            Ok(_) => panic!("{label}: swallowed error"),
        }
    }
    fn property(receiver: &Js, key: &str, value: Js, events: Trace, event: &'static str) {
        let expected = receiver.clone();
        receiver.define_accessor(
            key,
            Some(Rc::new(move |this| {
                assert!(
                    this.same_reference(&expected),
                    "socket property native receiver"
                );
                events.borrow_mut().push(event);
                Ok(value.clone())
            })),
            None,
        );
    }
    fn failing(receiver: &Js, key: &str, error: Js, events: Trace, event: &'static str) {
        receiver.define_accessor(
            key,
            Some(Rc::new(move |_| {
                events.borrow_mut().push(event);
                Err(Thrown::new(error.clone()))
            })),
            None,
        );
    }
    #[test]
    fn get_native_contract() {
        let h = fixture(token());
        let receiver = token();
        let value = token();
        let events = trace();
        property(&receiver, "Ω.item", value.clone(), events.clone(), "get");
        native(
            get(&h, &receiver, "Ω.item").unwrap(),
            &value,
            "socket get native identity",
        );
        assert_eq!(*events.borrow(), ["get"], "socket get native trace");
        let error = token();
        failing(&receiver, "bad", error.clone(), events.clone(), "bad");
        thrown(
            get(&h, &receiver, "bad"),
            &error,
            "socket get original error",
        );
        native(
            get(&h, &receiver, "missing").unwrap(),
            &Js::Undefined,
            "socket get missing value",
        );
    }
    #[test]
    fn method_native_contract() {
        let h = fixture(token());
        let receiver = token();
        let argument = token();
        let result = token();
        let events = trace();
        let (r, a, v, t) = (
            receiver.clone(),
            argument.clone(),
            result.clone(),
            events.clone(),
        );
        let f = Js::function("method", move |this, args| {
            t.borrow_mut().push("invoke");
            native(this, &r, "socket method native receiver");
            assert_eq!(args.len(), 1, "socket method native argument count");
            native(args[0].clone(), &a, "socket method native argument");
            Ok(v.clone())
        });
        property(&receiver, "method", f, events.clone(), "get");
        let got = method(&h, receiver.clone(), "method", || {
            events.borrow_mut().push("args");
            receiver
                .define_key(&"method".encode_utf16().collect::<Vec<_>>(), Js::Undefined)
                .unwrap();
            Ok(vec![argument.clone()])
        })
        .unwrap();
        native(got, &result, "socket method native identity");
        assert_eq!(
            *events.borrow(),
            ["get", "args", "invoke"],
            "socket method evaluation order"
        );
        for stage in 0..3 {
            let events = trace();
            let error = token();
            let receiver = token();
            let (e, t) = (error.clone(), events.clone());
            if stage == 0 {
                failing(&receiver, "method", e, events.clone(), "get")
            } else {
                property(
                    &receiver,
                    "method",
                    Js::function("method", move |_, _| {
                        t.borrow_mut().push("invoke");
                        Err(Thrown::new(e.clone()))
                    }),
                    events.clone(),
                    "get",
                );
            }
            thrown(
                method(&h, receiver, "method", || {
                    events.borrow_mut().push("args");
                    if stage == 1 {
                        Err(Thrown::new(error.clone()))
                    } else {
                        Ok(vec![])
                    }
                }),
                &error,
                "socket method original error",
            );
            assert_eq!(
                &*events.borrow(),
                &["get", "args", "invoke"][..stage + 1],
                "socket method failure order"
            );
        }
    }
    #[test]
    fn global_native_contract() {
        let globals = token();
        let events = trace();
        let argument = token();
        let result = token();
        let (a, r, t) = (argument.clone(), result.clone(), events.clone());
        property(
            &globals,
            "fn",
            Js::function("fn", move |this, args| {
                t.borrow_mut().push("invoke");
                native(this, &Js::Undefined, "socket global undefined receiver");
                assert_eq!(args.len(), 1);
                native(args[0].clone(), &a, "socket global original argument");
                Ok(r.clone())
            }),
            events.clone(),
            "global",
        );
        let h = fixture(globals.clone());
        let got = global(&h, "fn", || {
            events.borrow_mut().push("args");
            globals
                .define_key(&"fn".encode_utf16().collect::<Vec<_>>(), Js::Undefined)
                .unwrap();
            Ok(vec![argument.clone()])
        })
        .unwrap();
        native(got, &result, "socket global native identity");
        assert_eq!(
            *events.borrow(),
            ["global", "args", "invoke"],
            "socket global evaluation order"
        );
        for stage in 0..3 {
            let globals = token();
            let events = trace();
            let error = token();
            let (e, t) = (error.clone(), events.clone());
            if stage == 0 {
                failing(&globals, "fn", e, events.clone(), "global")
            } else {
                property(
                    &globals,
                    "fn",
                    Js::function("fn", move |_, _| {
                        t.borrow_mut().push("invoke");
                        Err(Thrown::new(e.clone()))
                    }),
                    events.clone(),
                    "global",
                )
            }
            let h = fixture(globals);
            thrown(
                global(&h, "fn", || {
                    events.borrow_mut().push("args");
                    if stage == 1 {
                        Err(Thrown::new(error.clone()))
                    } else {
                        Ok(vec![])
                    }
                }),
                &error,
                "socket global original error",
            );
            assert_eq!(
                &*events.borrow(),
                &["global", "args", "invoke"][..stage + 1],
                "socket global failure order"
            );
        }
    }
    #[test]
    fn own_native_contract() {
        let h = fixture(token());
        let receiver = token();
        let expected = receiver.clone();
        let result = token();
        let v = result.clone();
        let events = trace();
        let t = events.clone();
        property(
            &receiver,
            "own",
            Js::function("own", move |this, args| {
                t.borrow_mut().push("invoke");
                native(this, &expected, "socket own native receiver");
                assert_eq!(args.len(), 0);
                Ok(v.clone())
            }),
            events.clone(),
            "get",
        );
        native(
            own(&h, &receiver, "own", || {
                events.borrow_mut().push("args");
                Ok(vec![])
            })
            .unwrap(),
            &result,
            "socket own native identity",
        );
        assert_eq!(
            *events.borrow(),
            ["get", "args", "invoke"],
            "socket own evaluation order"
        );
        let error = token();
        failing(&receiver, "bad", error.clone(), events.clone(), "bad");
        thrown(
            own(&h, &receiver, "bad", || {
                panic!("socket own args after failed getter")
            }),
            &error,
            "socket own original error",
        );
        thrown(
            own(&h, &receiver, "own", || Err(Thrown::new(error.clone()))),
            &error,
            "socket own argument error",
        );
    }
    #[test]
    fn math_native_contract() {
        let globals = token();
        let math_object = token();
        let events = trace();
        let result = token();
        let argument = token();
        let (m, r, a, t) = (
            math_object.clone(),
            result.clone(),
            argument.clone(),
            events.clone(),
        );
        property(
            &math_object,
            "max",
            Js::function("max", move |this, args| {
                t.borrow_mut().push("invoke");
                native(this, &m, "socket math native receiver");
                assert_eq!(args.len(), 1);
                native(args[0].clone(), &a, "socket math native argument");
                Ok(r.clone())
            }),
            events.clone(),
            "get",
        );
        property(&globals, "Math", math_object, events.clone(), "Math");
        let h = fixture(globals);
        native(
            math(&h, "max", || {
                events.borrow_mut().push("args");
                Ok(vec![argument.clone()])
            })
            .unwrap(),
            &result,
            "socket math native identity",
        );
        assert_eq!(
            *events.borrow(),
            ["Math", "get", "args", "invoke"],
            "socket math evaluation order"
        );
        for stage in 0..4 {
            let globals = token();
            let obj = token();
            let events = trace();
            let error = token();
            let (e, t) = (error.clone(), events.clone());
            if stage == 0 {
                failing(&globals, "Math", e, events.clone(), "Math")
            } else {
                if stage == 1 {
                    failing(&obj, "max", e, events.clone(), "get")
                } else {
                    property(
                        &obj,
                        "max",
                        Js::function("max", move |_, _| {
                            t.borrow_mut().push("invoke");
                            Err(Thrown::new(e.clone()))
                        }),
                        events.clone(),
                        "get",
                    )
                }
                property(&globals, "Math", obj, events.clone(), "Math");
            }
            let h = fixture(globals);
            thrown(
                math(&h, "max", || {
                    events.borrow_mut().push("args");
                    if stage == 2 {
                        Err(Thrown::new(error.clone()))
                    } else {
                        Ok(vec![])
                    }
                }),
                &error,
                "socket math original error",
            );
            assert_eq!(
                &*events.borrow(),
                &["Math", "get", "args", "invoke"][..stage + 1],
                "socket math failure order"
            );
        }
    }
    #[test]
    fn parsed_field_native_contract() {
        let globals = token();
        let data = token();
        let input = token();
        let result = token();
        let events = trace();
        let (i, r, t) = (input.clone(), result.clone(), events.clone());
        property(
            &globals,
            "parseFloat",
            Js::function("parseFloat", move |this, args| {
                t.borrow_mut().push("invoke");
                native(
                    this,
                    &Js::Undefined,
                    "socket parsed_field undefined receiver",
                );
                assert_eq!(args.len(), 1);
                native(args[0].clone(), &i, "socket parsed_field native input");
                Ok(r.clone())
            }),
            events.clone(),
            "global",
        );
        property(&data, "price", input, events.clone(), "field");
        let h = fixture(globals);
        native(
            parsed_field(&h, &data, "price").unwrap(),
            &result,
            "socket parsed_field native identity",
        );
        assert_eq!(
            *events.borrow(),
            ["global", "field", "invoke"],
            "socket parsed_field evaluation order"
        );
        for stage in 0..3 {
            let globals = token();
            let data = token();
            let events = trace();
            let error = token();
            let (e, t) = (error.clone(), events.clone());
            if stage == 0 {
                failing(&globals, "parseFloat", e, events.clone(), "global")
            } else {
                property(
                    &globals,
                    "parseFloat",
                    Js::function("parseFloat", move |_, _| {
                        t.borrow_mut().push("invoke");
                        Err(Thrown::new(e.clone()))
                    }),
                    events.clone(),
                    "global",
                );
                if stage == 1 {
                    failing(&data, "price", error.clone(), events.clone(), "field")
                } else {
                    property(&data, "price", token(), events.clone(), "field")
                }
            }
            let h = fixture(globals);
            thrown(
                parsed_field(&h, &data, "price"),
                &error,
                "socket parsed_field original error",
            );
            assert_eq!(
                &*events.borrow(),
                &["global", "field", "invoke"][..stage + 1],
                "socket parsed_field failure order"
            );
        }
    }
    #[test]
    fn parsed_or_native_contract() {
        let cases = [
            (Js::Number(0.), false),
            (Js::Number(-0.), false),
            (Js::Number(f64::NAN), false),
            (Js::Undefined, false),
            (Js::Null, false),
            (Js::Bool(false), false),
            (Js::str(""), false),
            (Js::Number(f64::INFINITY), true),
            (Js::Number(-3.5), true),
            (Js::str("x"), true),
            (token(), true),
        ];
        assert_eq!(cases.len(), 11, "socket parsed_or independent cases");
        let mut executed = 0;
        for (result, truthy) in cases {
            let globals = token();
            let r = result.clone();
            let calls = trace();
            let t = calls.clone();
            globals
                .define_key(
                    &"parseFloat".encode_utf16().collect::<Vec<_>>(),
                    Js::function("parseFloat", move |_, _| {
                        t.borrow_mut().push("invoke");
                        Ok(r.clone())
                    }),
                )
                .unwrap();
            let mut h = fixture(globals);
            let t = calls.clone();
            h.number = Rc::new(move |n| {
                t.borrow_mut().push("fallback");
                Js::Number(n)
            });
            let got = parsed_or(&h, &Js::object(vec![("p", Js::str("input"))]), "p", -0.).unwrap();
            native(
                got,
                if truthy { &result } else { &Js::Number(-0.) },
                "socket parsed_or native value",
            );
            assert_eq!(
                &*calls.borrow(),
                if truthy {
                    &["invoke"][..]
                } else {
                    &["invoke", "fallback"][..]
                },
                "socket parsed_or lazy fallback"
            );
            executed += 1;
        }
        assert_eq!(executed, 11, "socket parsed_or executed cases");
        let error = token();
        let e = error.clone();
        let globals = Js::object(vec![(
            "parseFloat",
            Js::function("parseFloat", move |_, _| Err(Thrown::new(e.clone()))),
        )]);
        let mut h = fixture(globals);
        h.number = Rc::new(|_| panic!("socket parsed_or fallback after error"));
        thrown(
            parsed_or(&h, &Js::object(vec![("p", Js::str("input"))]), "p", 1.),
            &error,
            "socket parsed_or original error",
        );
    }
    #[test]
    fn concat_native_contract() {
        let h = fixture(token());
        let got = concat(&h, "Ω:", &Js::utf16(&[0xd800, 65, 0xdc00])).unwrap();
        native(
            got,
            &Js::utf16(&[937, 58, 0xd800, 65, 0xdc00]),
            "socket concat native UTF16",
        );
        native(
            concat(&h, "", &Js::str("")).unwrap(),
            &Js::str(""),
            "socket concat empty",
        );
        native(
            concat(&h, "n=", &Js::Number(-0.)).unwrap(),
            &Js::str("n=0"),
            "socket concat number",
        );
        let error = token();
        let e = error.clone();
        let mut h = fixture(token());
        h.text = Rc::new(move |_| Err(Thrown::new(e.clone())));
        h.utf16 = Rc::new(|_| panic!("socket concat constructs after coercion error"));
        thrown(
            concat(&h, "prefix", &token()),
            &error,
            "socket concat original error",
        );
    }
    #[test]
    fn caught_native_contract() {
        let h = fixture(token());
        caught(&h, &Js::Null, "prefix", Ok(())).unwrap();
        let receiver = token();
        let parent = token();
        let error = token();
        let output = token();
        let events = trace();
        let (p, t) = (parent.clone(), events.clone());
        property(
            &parent,
            "emitAndLogError",
            Js::function("emitAndLogError", move |this, args| {
                t.borrow_mut().push("invoke");
                native(this, &p, "socket caught native receiver");
                assert_eq!(args.len(), 1);
                native(
                    args[0].clone(),
                    &Js::utf16(&[87, 83, 58, 32, 0xd800, 65]),
                    "socket caught native message",
                );
                Ok(output.clone())
            }),
            events.clone(),
            "emit",
        );
        property(&receiver, "parent", parent, events.clone(), "parent");
        let (e, t) = (error.clone(), events.clone());
        let mut h = fixture(token());
        h.error_value = Rc::new(move |error| {
            t.borrow_mut().push("error");
            assert!(
                error.value.same_reference(&e),
                "socket caught native error identity"
            );
            error.value
        });
        let t = events.clone();
        h.text = Rc::new(move |_| {
            t.borrow_mut().push("text");
            Ok(vec![0xd800, 65])
        });
        caught(&h, &receiver, "WS: ", Err(Thrown::new(error))).unwrap();
        assert_eq!(
            *events.borrow(),
            ["parent", "emit", "error", "text", "invoke"],
            "socket caught native evaluation order"
        );
        for stage in 0..4 {
            let receiver = token();
            let parent = token();
            let events = trace();
            let replacement = token();
            let original = token();
            let (e, t) = (replacement.clone(), events.clone());
            if stage == 0 {
                failing(&receiver, "parent", e, events.clone(), "parent")
            } else {
                if stage == 1 {
                    failing(&parent, "emitAndLogError", e, events.clone(), "emit")
                } else {
                    property(
                        &parent,
                        "emitAndLogError",
                        Js::function("emitAndLogError", move |_, _| {
                            t.borrow_mut().push("invoke");
                            Err(Thrown::new(e.clone()))
                        }),
                        events.clone(),
                        "emit",
                    )
                }
                property(&receiver, "parent", parent, events.clone(), "parent");
            }
            let mut h = fixture(token());
            let t = events.clone();
            let e = replacement.clone();
            h.text = Rc::new(move |_| {
                t.borrow_mut().push("text");
                if stage == 2 {
                    Err(Thrown::new(e.clone()))
                } else {
                    Ok(vec![])
                }
            });
            thrown(
                caught(&h, &receiver, "prefix", Err(Thrown::new(original))),
                &replacement,
                "socket caught replacement error",
            );
            assert_eq!(
                &*events.borrow(),
                &["parent", "emit", "text", "invoke"][..stage + 1],
                "socket caught abrupt order"
            );
        }
    }
}
