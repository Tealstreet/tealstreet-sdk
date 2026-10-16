use crate::market_account_reads::MarketAccountOps;
use std::{cell::RefCell, rc::Rc};

pub struct RuntimeOps<V, E> {
    pub value: Rc<MarketAccountOps<V, E>>,
    pub callback: Rc<dyn Fn(Rc<dyn Fn(Vec<V>) -> Result<V, E>>) -> V>,
}
fn call<V, E>(
    h: &RuntimeOps<V, E>,
    name: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let f = (h.value.capture)(name)?;
    (h.value.invoke)(f, args()?)
}
fn method<V, E>(
    h: &RuntimeOps<V, E>,
    o: V,
    name: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let f = (h.value.member)(o, name)?;
    (h.value.invoke)(f, args()?)
}
fn optional<V, E>(h: &RuntimeOps<V, E>, o: &V, key: &str) -> Result<V, E> {
    if (h.value.nullish)(o) {
        Ok((h.value.undefined)())
    } else {
        (h.value.get)(o, key)
    }
}
fn or<V, E>(h: &RuntimeOps<V, E>, a: V, next: impl FnOnce() -> Result<V, E>) -> Result<V, E> {
    if (h.value.truthy)(&a) { Ok(a) } else { next() }
}
fn index<V, E>(h: &RuntimeOps<V, E>, o: V, key: V) -> Result<V, E> {
    (h.value.intrinsic)("get-index", vec![o, key])
}
fn expression<V, E>(h: &RuntimeOps<V, E>, op: &str, a: V, b: V) -> Result<V, E> {
    (h.value.intrinsic)(op, vec![a, b])
}
fn compare<V, E>(h: &RuntimeOps<V, E>, op: &str, a: V, b: V) -> Result<bool, E> {
    Ok((h.value.truthy)(&expression(h, op, a, b)?))
}
fn construct<V, E>(h: &RuntimeOps<V, E>, name: &str, args: Vec<V>) -> Result<V, E> {
    let mut values = vec![(h.value.capture)(name)?];
    values.extend(args);
    (h.value.intrinsic)("construct", values)
}
fn emitted<V, E>(h: &RuntimeOps<V, E>, error: &E) -> Result<(), E> {
    let f = (h.value.capture)("this.emitAndLogError")?;
    let error = (h.value.error_value)(error);
    let response = optional(h, &error, "response")?;
    let data = optional(h, &response, "data")?;
    let message = or(h, optional(h, &data, "msg")?, || {
        optional(h, &error, "message")
    })?;
    (h.value.invoke)(f, vec![message])?;
    Ok(())
}
fn sorted<V: Clone + 'static, E: 'static>(
    h: Rc<RuntimeOps<V, E>>,
    rows: V,
    newest: bool,
) -> Result<V, E> {
    let ops = h.clone();
    let comparator = (h.callback)(Rc::new(move |args| {
        let (first, second) = if newest {
            (&args[1], &args[0])
        } else {
            (&args[0], &args[1])
        };
        expression(
            &ops,
            "subtract",
            (ops.value.get)(first, "timestamp")?,
            (ops.value.get)(second, "timestamp")?,
        )
    }));
    method(&h, rows, "sort", || Ok(vec![comparator]))
}

pub async fn fetch_income_history<V: Clone + 'static, E: 'static>(
    h: Rc<RuntimeOps<V, E>>,
    opts: V,
) -> Result<V, E> {
    if (h.value.truthy)(&(h.value.capture)("this.isPublicDataOnly")?) {
        return Ok((h.value.array)(vec![]));
    }
    let symbol = optional(&h, &opts, "symbol")?;
    let market = if (h.value.truthy)(&symbol) {
        index(
            &h,
            (h.value.capture)("this.store.marketsBySymbol")?,
            (h.value.get)(&opts, "symbol")?,
        )?
    } else {
        (h.value.undefined)()
    };
    if (h.value.truthy)(&optional(&h, &opts, "symbol")?) && !(h.value.truthy)(&market) {
        return Ok((h.value.array)(vec![]));
    }
    let now = call(&h, "virtualClock.getCurrentTimeMs", || Ok(vec![]))?;
    let start = optional(&h, &opts, "startTime")?;
    let old = (h.value.truthy)(&start)
        && compare(
            &h,
            "greater-than",
            expression(&h, "subtract", now, start)?,
            (h.value.number)(518_400_000.),
        )?;
    let endpoint = (h.value.capture)(if old {
        "ENDPOINTS.BILLS_ARCHIVE"
    } else {
        "ENDPOINTS.BILLS"
    })?;
    let ops = h.clone();
    let resolve = (h.callback)(Rc::new(move |args| {
        let m = index(
            &ops,
            (ops.value.capture)("this.store.marketsById")?,
            args[0].clone(),
        )?;
        optional(&ops, &m, "symbol")
    }));
    let limit = optional(&h, &opts, "limit")?;
    let max = if (h.value.truthy)(&limit)
        && compare(
            &h,
            "greater-than",
            (h.value.get)(&opts, "limit")?,
            (h.value.number)(0.),
        )? {
        (h.value.get)(&opts, "limit")?
    } else {
        (h.value.number)(f64::INFINITY)
    };
    let result = async {
        let dedupe = construct(&h, "Map", vec![])?;
        let mut after = (h.value.undefined)();
        let mut exhausted = false;
        let mut seen = (h.value.number)(0.);
        for _ in 0..200 {
            let kind = optional(&h, &opts, "incomeType")?;
            let kind = if (h.value.truthy)(&kind) {
                index(
                    &h,
                    (h.value.capture)("OKX_BILL_TYPE_BY_INCOME_TYPE")?,
                    (h.value.get)(&opts, "incomeType")?,
                )?
            } else {
                (h.value.undefined)()
            };
            let params = call(&h, "omitUndefined", || {
                Ok(vec![(h.value.object)(vec![
                    ("instType", (h.value.literal)("SWAP")),
                    ("instId", optional(&h, &market, "id")?),
                    ("type", kind),
                    ("begin", optional(&h, &opts, "startTime")?),
                    ("end", optional(&h, &opts, "endTime")?),
                    ("limit", (h.value.number)(100.)),
                    ("after", after.clone()),
                ])])
            })?;
            let request = call(&h, "this.xhr.get", || {
                Ok(vec![
                    endpoint.clone(),
                    (h.value.object)(vec![("params", params)]),
                ])
            })?;
            let response = (h.value.await_value)(request).await?;
            let data = (h.value.get)(&(h.value.get)(&response, "data")?, "data")?;
            let rows = if (h.value.truthy)(&call(&h, "Array.isArray", || Ok(vec![data.clone()]))?) {
                data
            } else {
                (h.value.array)(vec![])
            };
            let length = (h.value.get)(&rows, "length")?;
            if (h.value.equal)(&length, &(h.value.number)(0.)) {
                exhausted = true;
                break;
            }
            // The source counts bills before mapping a bill to multiple records.
            seen = expression(&h, "add", seen, length.clone())?;
            let ops = h.clone();
            let map = dedupe.clone();
            let resolver = resolve.clone();
            (h.value.for_of)(
                rows.clone(),
                Rc::new(move |row| {
                    let records = call(&ops, "mapOkxBillToIncomeRecords", || {
                        Ok(vec![row, resolver.clone()])
                    })?;
                    let inner = ops.clone();
                    let map = map.clone();
                    (ops.value.for_of)(
                        records,
                        Rc::new(move |record| {
                            method(&inner, map.clone(), "set", || {
                                Ok(vec![(inner.value.get)(&record, "tranId")?, record])
                            })?;
                            Ok(())
                        }),
                    )
                }),
            )?;
            if compare(&h, "less-than", length, (h.value.number)(100.))?
                || compare(&h, "greater-or-equal", seen.clone(), max.clone())?
            {
                exhausted = true;
                break;
            }
            let cursor = call(&h, "oldestCursorId", || {
                Ok(vec![rows, (h.value.literal)("billId")])
            })?;
            let next = if (h.value.nullish)(&cursor) {
                (h.value.literal)("")
            } else {
                cursor
            };
            if !(h.value.truthy)(&next) || (h.value.equal)(&next, &after) {
                exhausted = true;
                break;
            }
            after = next;
            let sleep = call(&h, "sleep", || Ok(vec![(h.value.number)(100.)]))?;
            (h.value.await_value)(sleep).await?;
        }
        if !exhausted {
            let message = (h.value.intrinsic)("bills-cap-message", vec![seen])?;
            call(&h, "this.log", || {
                Ok(vec![message, (h.value.capture)("LogSeverity.Warning")?])
            })?;
        }
        let values = method(&h, dedupe, "values", || Ok(vec![]))?;
        let values = (h.value.intrinsic)("iterable-spread", vec![values])?;
        sorted(h.clone(), values, true)
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            emitted(&h, &e)?;
            Ok((h.value.array)(vec![]))
        }
    }
}

pub async fn fetch_ohlcv<V: Clone + 'static, E: 'static>(
    h: Rc<RuntimeOps<V, E>>,
    opts: V,
) -> Result<V, E> {
    let limit = call(&h, "Math.min", || {
        Ok(vec![
            or(&h, (h.value.get)(&opts, "limit")?, || {
                Ok((h.value.number)(300.))
            })?,
            (h.value.number)(300.),
        ])
    })?;
    let interval = (h.value.get)(&opts, "interval")?;
    let parts = (h.value.intrinsic)("split-interval-digits", vec![interval])?;
    let amount = (h.value.get)(&parts, "1")?;
    let unit = (h.value.get)(&parts, "2")?;
    let end = call(&h, "dayjs", || {
        Ok(vec![if (h.value.truthy)(&(h.value.get)(&opts, "to")?) {
            (h.value.get)(&opts, "to")?
        } else {
            call(&h, "virtualClock.getCurrentTimeMs", || Ok(vec![]))?
        }])
    })?;
    let start = if !(h.value.truthy)(&(h.value.get)(&opts, "limit")?)
        && (h.value.truthy)(&(h.value.get)(&opts, "from")?)
    {
        call(&h, "dayjs", || Ok(vec![(h.value.get)(&opts, "from")?]))?
    } else {
        method(&h, end.clone(), "subtract", || {
            Ok(vec![
                expression(
                    &h,
                    "multiply",
                    call(&h, "parseFloat", || Ok(vec![amount.clone()]))?,
                    limit.clone(),
                )?,
                unit.clone(),
            ])
        })?
    };
    let market = index(
        &h,
        (h.value.capture)("this.store.marketsBySymbol")?,
        (h.value.get)(&opts, "symbol")?,
    )?;
    if !(h.value.truthy)(&market) {
        let message = (h.value.intrinsic)(
            "market-error-message",
            vec![(h.value.get)(&opts, "symbol")?],
        )?;
        call(&h, "this.emitAndLogError", || Ok(vec![message]))?;
        return Ok((h.value.array)(vec![]));
    }
    let loaded = async {
        let fetch = |endpoint: V, request_limit: V| {
            let h = h.clone();
            let market = market.clone();
            let start = start.clone();
            let end = end.clone();
            let opts = opts.clone();
            async move {
                let request = call(&h, "this.xhr.get", || {
                    Ok(vec![
                        endpoint,
                        (h.value.object)(vec![(
                            "params",
                            (h.value.object)(vec![
                                ("instId", (h.value.get)(&market, "id")?),
                                (
                                    "bar",
                                    index(
                                        &h,
                                        (h.value.capture)("INTERVAL")?,
                                        (h.value.get)(&opts, "interval")?,
                                    )?,
                                ),
                                ("limit", request_limit),
                                ("before", method(&h, start, "valueOf", || Ok(vec![]))?),
                                ("after", method(&h, end, "valueOf", || Ok(vec![]))?),
                            ]),
                        )]),
                    ])
                })?;
                let response = (h.value.await_value)(request).await?;
                (h.value.get)(&(h.value.get)(&response, "data")?, "data")
            }
        };
        let recent = fetch((h.value.capture)("ENDPOINTS.KLINE")?, limit.clone()).await?;
        let now = call(&h, "dayjs", || {
            Ok(vec![call(&h, "virtualClock.getCurrentTimeMs", || {
                Ok(vec![])
            })?])
        })?;
        let recent_start = method(&h, now, "subtract", || {
            Ok(vec![
                expression(
                    &h,
                    "multiply",
                    call(&h, "parseFloat", || Ok(vec![amount]))?,
                    (h.value.number)(1440.),
                )?,
                unit,
            ])
        })?;
        let old = (h.value.truthy)(&(h.value.get)(&opts, "to")?)
            && (h.value.truthy)(&method(&h, end.clone(), "isBefore", || {
                Ok(vec![recent_start])
            })?);
        let data =
            if old || (h.value.equal)(&(h.value.get)(&recent, "length")?, &(h.value.number)(0.)) {
                fetch(
                    (h.value.capture)("ENDPOINTS.KLINE_HISTORY")?,
                    call(&h, "Math.min", || Ok(vec![limit, (h.value.number)(100.)]))?,
                )
                .await?
            } else {
                recent
            };
        let ops = h.clone();
        let candles = (h.value.map)(data, Rc::new(move |c| candle(&ops, c)))?;
        sorted(h.clone(), candles, false)
    }
    .await;
    match loaded {
        Ok(v) => Ok(v),
        Err(e) => {
            emitted(&h, &e)?;
            Ok((h.value.array)(vec![]))
        }
    }
}
fn candle<V, E>(h: &RuntimeOps<V, E>, c: V) -> Result<V, E> {
    let timestamp = call(h, "parseInt", || {
        Ok(vec![(h.value.get)(&c, "0")?, (h.value.number)(10.)])
    })?;
    let mut fields = vec![(
        "timestamp",
        expression(h, "divide", timestamp, (h.value.number)(1000.))?,
    )];
    for (key, i) in [
        ("open", "1"),
        ("high", "2"),
        ("low", "3"),
        ("close", "4"),
        ("volume", "7"),
    ] {
        fields.push((
            key,
            call(h, "parseFloat", || Ok(vec![(h.value.get)(&c, i)?]))?,
        ));
    }
    Ok((h.value.object)(fields))
}

pub fn listen_ohlcv<V: Clone + 'static, E: 'static>(
    h: Rc<RuntimeOps<V, E>>,
    opts: V,
    callback: V,
) -> Result<V, E> {
    listen(h, opts, callback, false)
}
pub fn listen_order_book<V: Clone + 'static, E: 'static>(
    h: Rc<RuntimeOps<V, E>>,
    symbol: V,
    callback: V,
) -> Result<V, E> {
    listen(h, symbol, callback, true)
}
fn no_op<V: Clone + 'static, E: 'static>(h: Rc<RuntimeOps<V, E>>) -> V {
    let ops = h.clone();
    (h.callback)(Rc::new(move |_| Ok((ops.value.undefined)())))
}
fn cancel_timeout<V: Clone, E>(h: &RuntimeOps<V, E>, timeout: &RefCell<V>) -> Result<(), E> {
    let id = timeout.borrow().clone();
    if (h.value.truthy)(&id) {
        call(h, "clearTimeout", || Ok(vec![id]))?;
        *timeout.borrow_mut() = (h.value.intrinsic)("null", vec![])?;
    }
    Ok(())
}
fn listen<V: Clone + 'static, E: 'static>(
    h: Rc<RuntimeOps<V, E>>,
    input: V,
    callback: V,
    book: bool,
) -> Result<V, E> {
    let timeout = Rc::new(RefCell::new((h.value.intrinsic)("null", vec![])?));
    if !(h.value.truthy)(&(h.value.capture)("this.store.loaded.markets")?) {
        let ops = h.clone();
        let input = input.clone();
        let retry = (h.callback)(Rc::new(move |_| {
            call(
                &ops,
                if book {
                    "this.listenOrderBook"
                } else {
                    "this.listenOHLCV"
                },
                || Ok(vec![input.clone(), callback.clone()]),
            )
        }));
        *timeout.borrow_mut() = call(&h, "setTimeout", || Ok(vec![retry, (h.value.number)(100.)]))?;
        let ops = h.clone();
        return Ok((h.callback)(Rc::new(move |_| {
            cancel_timeout(&ops, &timeout)?;
            Ok((ops.value.undefined)())
        })));
    }
    let markets = (h.value.capture)("this.store.markets")?;
    let ops = h.clone();
    let symbol = input.clone();
    let predicate = (h.callback)(Rc::new(move |args| {
        Ok((ops.value.boolean)((ops.value.equal)(
            &(ops.value.get)(&args[0], "symbol")?,
            &if book {
                symbol.clone()
            } else {
                (ops.value.get)(&symbol, "symbol")?
            },
        )))
    }));
    let market = method(&h, markets, "find", || Ok(vec![predicate]))?;
    if !(h.value.truthy)(&market) {
        return Ok(no_op(h));
    }
    let handler = if book {
        (h.value.intrinsic)(
            "template-concat",
            vec![(h.value.literal)("books."), (h.value.get)(&market, "id")?],
        )?
    } else {
        (h.value.intrinsic)(
            "candle-handler",
            vec![
                (h.value.get)(&input, "interval")?,
                (h.value.get)(&market, "id")?,
            ],
        )?
    };
    let channel = if book {
        (h.value.literal)("books")
    } else {
        (h.value.intrinsic)(
            "template-concat",
            vec![
                (h.value.literal)("candle"),
                index(
                    &h,
                    (h.value.capture)("INTERVAL")?,
                    (h.value.get)(&input, "interval")?,
                )?,
            ],
        )?
    };
    let topic = (h.value.object)(vec![
        ("channel", channel),
        ("instId", (h.value.get)(&market, "id")?),
    ]);
    let order_book = (h.value.object)(vec![
        ("bids", (h.value.array)(vec![])),
        ("asks", (h.value.array)(vec![])),
    ]);
    let ops = h.clone();
    let m = market.clone();
    let ob = order_book.clone();
    let parser = (h.callback)(Rc::new(move |args| {
        let data = args[0].clone();
        if !book {
            let rows = optional(&ops, &data, "data")?;
            let c = optional(&ops, &rows, "0")?;
            if (ops.value.truthy)(&c) {
                (ops.value.invoke)(callback.clone(), vec![candle(&ops, c)?])?;
            }
        } else {
            parse_book(ops.clone(), data, m.clone(), ob.clone())?;
            (ops.value.invoke)(callback.clone(), vec![ob.clone()])?;
        }
        Ok((ops.value.undefined)())
    }));
    wait_connected(
        h.clone(),
        handler.clone(),
        parser.clone(),
        topic.clone(),
        timeout.clone(),
    )?;
    let ops = h.clone();
    Ok((h.callback)(Rc::new(move |_| {
        let handlers = (ops.value.capture)("this.messageHandlers")?;
        let old = or(
            &ops,
            index(
                &ops,
                (ops.value.capture)("this.messageHandlers")?,
                handler.clone(),
            )?,
            || Ok((ops.value.array)(vec![])),
        )?;
        let value = ops.clone();
        let p = parser.clone();
        let next = (ops.value.filter)(old, Rc::new(move |f| Ok(!(value.value.equal)(&f, &p))))?;
        let next = if book {
            next
        } else {
            (ops.value.intrinsic)("iterable-spread", vec![next])?
        };
        (ops.value.intrinsic)("set-index", vec![handlers, handler.clone(), next])?;
        if handlers_empty(&ops, handler.clone())? {
            (ops.value.intrinsic)(
                "delete-index",
                vec![(ops.value.capture)("this.topics")?, handler.clone()],
            )?;
            if book {
                call(&ops, "this.orderBookSequenceByHandler.delete", || {
                    Ok(vec![handler.clone()])
                })?;
                call(&ops, "this.recoveringOrderBooks.delete", || {
                    Ok(vec![handler.clone()])
                })?;
            }
            call(&ops, "this.cleanupMessageHandlerStats", || {
                Ok(vec![handler.clone()])
            })?;
        }
        if book {
            (ops.value.set)(&order_book, "bids", (ops.value.array)(vec![]))?;
            (ops.value.set)(&order_book, "asks", (ops.value.array)(vec![]))?;
        }
        cancel_timeout(&ops, &timeout)?;
        if (ops.value.truthy)(&(ops.value.capture)("this.isConnected")?)
            && handlers_empty(&ops, handler.clone())?
        {
            send_topic(&ops, "unsubscribe", topic.clone())?;
        }
        Ok((ops.value.undefined)())
    })))
}
fn wait_connected<V: Clone + 'static, E: 'static>(
    h: Rc<RuntimeOps<V, E>>,
    key: V,
    parser: V,
    topic: V,
    timeout: Rc<RefCell<V>>,
) -> Result<V, E> {
    if (h.value.truthy)(&(h.value.capture)("this.isConnected")?) {
        if !(h.value.truthy)(&(h.value.capture)("this.isDisposed")?) {
            let handlers = (h.value.capture)("this.messageHandlers")?;
            let current = or(
                &h,
                index(&h, (h.value.capture)("this.messageHandlers")?, key.clone())?,
                || Ok((h.value.array)(vec![])),
            )?;
            let next =
                (h.value.intrinsic)("array-spread", vec![current, (h.value.array)(vec![parser])])?;
            (h.value.intrinsic)("set-index", vec![handlers, key.clone(), next])?;
            send_topic(&h, "subscribe", topic.clone())?;
            (h.value.intrinsic)(
                "set-index",
                vec![
                    (h.value.capture)("this.topics")?,
                    key,
                    (h.value.array)(vec![topic]),
                ],
            )?;
        }
    } else {
        let ops = h.clone();
        let timer = timeout.clone();
        let retry = (h.callback)(Rc::new(move |_| {
            wait_connected(
                ops.clone(),
                key.clone(),
                parser.clone(),
                topic.clone(),
                timer.clone(),
            )
        }));
        *timeout.borrow_mut() = call(&h, "setTimeout", || Ok(vec![retry, (h.value.number)(100.)]))?;
    }
    Ok((h.value.undefined)())
}
fn handlers_empty<V, E>(h: &RuntimeOps<V, E>, key: V) -> Result<bool, E> {
    let handlers = index(h, (h.value.capture)("this.messageHandlers")?, key)?;
    Ok(!(h.value.truthy)(&(h.value.get)(&handlers, "length")?))
}
fn send_topic<V, E>(h: &RuntimeOps<V, E>, op: &str, topic: V) -> Result<(), E> {
    let send = (h.value.capture)("this.send")?;
    let wire = call(h, "JSON.stringify", || {
        Ok(vec![(h.value.object)(vec![
            ("op", (h.value.literal)(op)),
            ("args", (h.value.array)(vec![topic])),
        ])])
    })?;
    (h.value.invoke)(send, vec![wire])?;
    Ok(())
}
fn parse_book<V: Clone + 'static, E: 'static>(
    h: Rc<RuntimeOps<V, E>>,
    data: V,
    market: V,
    book: V,
) -> Result<(), E> {
    if (h.value.equal)(
        &(h.value.get)(&data, "action")?,
        &(h.value.literal)("snapshot"),
    ) {
        let snapshot = (h.value.get)(&(h.value.get)(&data, "data")?, "0")?;
        for side in ["bids", "asks"] {
            let ops = h.clone();
            let m = market.clone();
            let rows = (h.value.reduce)(
                (h.value.get)(&snapshot, side)?,
                (h.value.array)(vec![]),
                Rc::new(move |acc, pair| {
                    let pair = (ops.value.intrinsic)("destructure-two", vec![pair])?;
                    let price = (ops.value.get)(&pair, "0")?;
                    let amount = (ops.value.get)(&pair, "1")?;
                    if (ops.value.equal)(
                        &call(&ops, "parseFloat", || Ok(vec![amount.clone()]))?,
                        &(ops.value.number)(0.),
                    ) {
                        return Ok(acc);
                    }
                    let level = book_level(&ops, price, amount, &m, false)?;
                    (ops.value.intrinsic)("array-spread", vec![acc, (ops.value.array)(vec![level])])
                }),
            )?;
            (h.value.set)(&book, side, rows)?;
        }
    }
    if (h.value.equal)(
        &(h.value.get)(&data, "action")?,
        &(h.value.literal)("update"),
    ) {
        let update = (h.value.get)(&(h.value.get)(&data, "data")?, "0")?;
        for side in ["bids", "asks"] {
            let ops = h.clone();
            let ob = book.clone();
            let m = market.clone();
            (h.value.for_of)(
                (h.value.get)(&update, side)?,
                Rc::new(move |pair| {
                    let pair = (ops.value.intrinsic)("destructure-two", vec![pair])?;
                    let level = book_level(
                        &ops,
                        (ops.value.get)(&pair, "0")?,
                        (ops.value.get)(&pair, "1")?,
                        &m,
                        true,
                    )?;
                    call(&ops, "updateOrderBook", || {
                        Ok(vec![ob.clone(), (ops.value.literal)(side), level])
                    })?;
                    Ok(())
                }),
            )?;
        }
    }
    let ticker = index(
        &h,
        (h.value.capture)("this.store.tickersBySymbol")?,
        (h.value.get)(&market, "symbol")?,
    )?;
    let last = or(&h, optional(&h, &ticker, "last")?, || {
        Ok((h.value.number)(0.))
    })?;
    for (side, op) in [("asks", "greater-or-equal"), ("bids", "less-or-equal")] {
        let ops = h.clone();
        let last = last.clone();
        let rows = (h.value.filter)(
            (h.value.get)(&book, side)?,
            Rc::new(move |level| {
                compare(&ops, op, (ops.value.get)(&level, "price")?, last.clone())
            }),
        )?;
        (h.value.set)(&book, side, rows)?;
    }
    call(&h, "calcOrderBookTotal", || Ok(vec![book]))?;
    Ok(())
}
fn book_level<V, E>(
    h: &RuntimeOps<V, E>,
    price: V,
    amount: V,
    market: &V,
    total: bool,
) -> Result<V, E> {
    let price = call(h, "parseFloat", || Ok(vec![price]))?;
    let amount = call(h, "parseFloat", || Ok(vec![amount]))?;
    let amount = call(h, "multiply", || {
        Ok(vec![
            amount,
            (h.value.get)(&(h.value.get)(market, "precision")?, "amount")?,
        ])
    })?;
    let mut fields = vec![("price", price), ("amount", amount)];
    if total {
        fields.push(("total", (h.value.number)(0.)));
    }
    Ok((h.value.object)(fields))
}

pub fn create_api<V: Clone + 'static, E: 'static>(
    h: Rc<RuntimeOps<V, E>>,
    options: V,
    exchange: V,
) -> Result<V, E> {
    let passphrase = (h.value.get)(&options, "passphrase")?;
    if !(h.value.truthy)(&passphrase)
        && !(h.value.truthy)(&(h.value.get)(&options, "publicDataOnly")?)
    {
        return Err((h.value.new_error)(
            (h.value.capture)("Error")?,
            (h.value.literal)("OKX requires a passphrase"),
        )?);
    }
    let region = (h.value.get)(&exchange, "region")?;
    let region = if (h.value.nullish)(&region) {
        (h.value.literal)("okx")
    } else {
        region
    };
    let urls = index(&h, (h.value.capture)("BASE_URLS")?, region)?;
    let base_url = (h.value.get)(
        &urls,
        if (h.value.truthy)(&(h.value.get)(&options, "testnet")?) {
            "testnet"
        } else {
            "livenet"
        },
    )?;
    let extra = optional(&h, &options, "extra")?;
    let timeout = optional(&h, &extra, "recvWindow")?;
    let timeout = if (h.value.nullish)(&timeout) {
        (h.value.capture)("RECV_WINDOW")?
    } else {
        timeout
    };
    let mut headers = vec![("Content-Type", (h.value.literal)("application/json"))];
    if (h.value.truthy)(&(h.value.get)(&options, "testnet")?) {
        headers.push(("x-simulated-trading", (h.value.number)(1.)));
    }
    let xhr = call(&h, "safeCexAxiosCreate", || {
        Ok(vec![
            (h.value.object)(vec![
                ("baseURL", base_url),
                ("timeout", timeout),
                ("headers", (h.value.object)(headers)),
            ]),
            options.clone(),
            exchange.clone(),
        ])
    })?;
    call(&h, "retry", || {
        Ok(vec![
            xhr.clone(),
            (h.value.object)(vec![
                ("retries", (h.value.number)(3.)),
                (
                    "retryCondition",
                    call(&h, "networkErrorOnRead", || Ok(vec![]))?,
                ),
            ]),
        ])
    })?;
    let ops = h.clone();
    let response_callback = (h.callback)(Rc::new(move |args| {
        let response = args[0].clone();
        let code = optional(&ops, &(ops.value.get)(&response, "data")?, "code")?;
        if (ops.value.truthy)(&code) && !(ops.value.equal)(&code, &(ops.value.literal)("0")) {
            let message = or(
                &ops,
                (ops.value.get)(&(ops.value.get)(&response, "data")?, "msg")?,
                || (ops.value.intrinsic)("api-error-message", vec![code]),
            )?;
            let error = (ops.value.new_error)((ops.value.capture)("Error")?, message)?;
            (ops.value.set)(&(ops.value.error_value)(&error), "response", response)?;
            return Err(error);
        }
        Ok(response)
    }));
    let interceptors = (h.value.get)(&xhr, "interceptors")?;
    method(&h, (h.value.get)(&interceptors, "response")?, "use", || {
        Ok(vec![response_callback])
    })?;
    let ops = h.clone();
    let opts = options.clone();
    let request_callback = (h.callback)(Rc::new(move |args| {
        sign_request(&ops, &opts, passphrase.clone(), args[0].clone())
    }));
    method(&h, (h.value.get)(&interceptors, "request")?, "use", || {
        Ok(vec![request_callback])
    })?;
    if (h.value.truthy)(&(h.value.get)(&options, "tealstreetTunnel")?) {
        call(&h, "installTransportAdapter", || {
            Ok(vec![
                xhr.clone(),
                options,
                (h.value.get)(&exchange, "name")?,
                (h.value.literal)("bigInt"),
            ])
        })?;
    }
    Ok(xhr)
}
fn sign_request<V: Clone, E>(
    h: &RuntimeOps<V, E>,
    options: &V,
    passphrase: V,
    config: V,
) -> Result<V, E> {
    if (h.value.truthy)(&call(h, "isPublicEndpoint", || {
        Ok(vec![(h.value.get)(&config, "url")?])
    })?) {
        call(h, "markSkipRemoteSigning", || Ok(vec![config.clone()]))?;
    }
    let url = optional(h, &config, "url")?;
    if !(h.value.nullish)(&url)
        && (h.value.truthy)(&method(h, url, "includes", || {
            Ok(vec![(h.value.literal)("public")])
        })?)
    {
        return Ok(config);
    }
    let next = (h.value.intrinsic)("object-spread", vec![config.clone()])?;
    if (h.value.truthy)(&(h.value.get)(options, "remoteSigner")?) {
        return Ok(next);
    }
    if !(h.value.truthy)(&(h.value.get)(options, "key")?)
        || !(h.value.truthy)(&(h.value.get)(options, "secret")?)
    {
        return Ok(next);
    }
    let method_value = (h.value.get)(&config, "method")?;
    let method_value = if (h.value.nullish)(&method_value) {
        (h.value.undefined)()
    } else {
        let upper = (h.value.member)(method_value, "toUpperCase")?;
        if (h.value.nullish)(&upper) {
            (h.value.undefined)()
        } else {
            (h.value.invoke)(upper, vec![])?
        }
    };
    let verb = or(h, method_value, || Ok((h.value.literal)("GET")))?;
    let params = if (h.value.truthy)(&(h.value.get)(&config, "params")?) {
        call(h, "decodeURIComponent", || {
            Ok(vec![call(h, "qs.stringify", || {
                Ok(vec![(h.value.get)(&config, "params")?])
            })?])
        })?
    } else {
        (h.value.intrinsic)("null", vec![])?
    };
    let url = (h.value.intrinsic)("signing-url", vec![(h.value.get)(&config, "url")?, params])?;
    let data = (h.value.get)(&config, "data")?;
    let has_body = !(h.value.equal)(&data, &(h.value.undefined)())
        && !(h.value.equal)(
            &(h.value.get)(&config, "data")?,
            &(h.value.intrinsic)("null", vec![])?,
        );
    let data = if has_body {
        if (h.value.truthy)(&(h.value.intrinsic)(
            "typeof-string",
            vec![(h.value.get)(&config, "data")?],
        )?) {
            (h.value.get)(&config, "data")?
        } else {
            or(
                h,
                call(h, "JSON.stringify", || {
                    Ok(vec![(h.value.get)(&config, "data")?])
                })?,
                || Ok((h.value.literal)("")),
            )?
        }
    } else {
        (h.value.literal)("")
    };
    let timestamp = method(
        h,
        call(h, "virtualClock.getCurrentTime", || Ok(vec![]))?,
        "toISOString",
        || Ok(vec![]),
    )?;
    let to_sign = (h.value.array)(vec![timestamp.clone(), verb, url, data.clone()]);
    let mac = call(h, "createHmac", || {
        Ok(vec![
            (h.value.literal)("sha256"),
            (h.value.get)(options, "secret")?,
        ])
    })?;
    let mac = method(h, mac, "update", || {
        Ok(vec![method(h, to_sign, "join", || {
            Ok(vec![(h.value.literal)("")])
        })?])
    })?;
    let signature = method(h, mac, "digest", || Ok(vec![(h.value.literal)("base64")]))?;
    let headers = (h.value.intrinsic)("object-spread", vec![(h.value.get)(&next, "headers")?])?;
    for (key, value) in [
        ("OK-ACCESS-KEY", (h.value.get)(options, "key")?),
        ("OK-ACCESS-SIGN", signature),
        ("OK-ACCESS-TIMESTAMP", timestamp),
        (
            "OK-ACCESS-PASSPHRASE",
            or(h, passphrase, || Ok((h.value.literal)("")))?,
        ),
    ] {
        (h.value.intrinsic)(
            "define-literal-field",
            vec![headers.clone(), (h.value.literal)(key), value],
        )?;
    }
    let headers = construct(h, "AxiosHeaders", vec![headers])?;
    let out = (h.value.intrinsic)("object-spread", vec![next])?;
    if has_body {
        (h.value.intrinsic)(
            "define-literal-field",
            vec![out.clone(), (h.value.literal)("data"), data],
        )?;
    }
    (h.value.intrinsic)(
        "define-literal-field",
        vec![out.clone(), (h.value.literal)("headers"), headers],
    )?;
    Ok(out)
}
