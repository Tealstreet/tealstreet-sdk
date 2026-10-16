use crate::{legacy_lifecycle::LifecycleOps, legacy_metadata::Task};
use std::{cell::RefCell, rc::Rc};
pub struct StreamOps<V, E> {
    pub lifecycle: Rc<LifecycleOps<V, E>>,
    pub set: Rc<dyn Fn(V, V, V) -> Result<(), E>>,
    pub delete: Rc<dyn Fn(V, V) -> Result<(), E>>,
    pub divide: Rc<dyn Fn(V, V) -> Result<V, E>>,
    pub greater: Rc<dyn Fn(V, V) -> Result<bool, E>>,
    pub template: Rc<dyn Fn(&[&str], Vec<V>) -> Result<V, E>>,
    pub null: Rc<dyn Fn() -> V>,
    pub define: Rc<dyn Fn(V, &'static str, V) -> Result<(), E>>,
}
impl<V, E> std::ops::Deref for StreamOps<V, E> {
    type Target = LifecycleOps<V, E>;
    fn deref(&self) -> &Self::Target {
        &self.lifecycle
    }
}
fn get<V, E>(h: &StreamOps<V, E>, v: &V, k: &str) -> Result<V, E> {
    (h.value.get)(v, k)
}
fn key<V, E>(h: &StreamOps<V, E>, v: &V, k: &V) -> Result<V, E> {
    (h.value.get_key)(v, k)
}
fn path<V: Clone, E>(h: &StreamOps<V, E>, v: &V, keys: &[&str]) -> Result<V, E> {
    let mut v = v.clone();
    for k in keys {
        v = get(h, &v, k)?;
    }
    Ok(v)
}
fn method<V, E>(
    h: &StreamOps<V, E>,
    r: V,
    k: &str,
    a: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let f = get(h, &r, k)?;
    (h.value.invoke)(f, r, a()?)
}
fn global<V, E>(
    h: &StreamOps<V, E>,
    name: &str,
    a: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let f = (h.value.global)(name)?;
    (h.value.invoke)(f, (h.value.undefined)(), a()?)
}
fn import<V, E>(
    h: &StreamOps<V, E>,
    name: &str,
    a: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let f = (h.import)(name)?;
    (h.value.invoke)(f, (h.value.undefined)(), a()?)
}
fn call<V, E>(h: &StreamOps<V, E>, f: V, args: Vec<V>) -> Result<V, E> {
    (h.value.invoke)(f, (h.value.undefined)(), args)
}
fn argument<V: Clone, E>(h: &StreamOps<V, E>, args: &[V], i: usize) -> V {
    args.get(i)
        .cloned()
        .unwrap_or_else(|| (h.value.undefined)())
}
fn property<V, E>(h: &StreamOps<V, E>, target: V, name: &str, value: V) -> Result<(), E> {
    (h.set)(target, (h.value.string)(name), value)
}
fn assign<V, E>(
    h: &StreamOps<V, E>,
    target: V,
    name: &str,
    value: impl FnOnce() -> Result<V, E>,
) -> Result<(), E> {
    let k = (h.value.string)(name);
    (h.set)(target, k, value()?)
}
fn attached<V: Clone, E>(
    h: &StreamOps<V, E>,
    receiver: &V,
    topic: &V,
    field: &str,
) -> Result<V, E> {
    key(h, &get(h, receiver, field)?, topic)
}
fn add_handler<V: Clone, E>(h: &StreamOps<V, E>, r: &V, topic: &V, parser: V) -> Result<(), E> {
    let target = get(h, r, "messageHandlers")?;
    let current = attached(h, r, topic, "messageHandlers")?;
    let current = if (h.value.truthy)(&current) {
        current
    } else {
        (h.value.array)(vec![])
    };
    let mut values = (h.value.spread)(&current)?;
    values.push(parser);
    (h.set)(target, topic.clone(), (h.value.array)(values))
}
fn remove_handler<V: Clone + 'static, E: 'static>(
    h: Rc<StreamOps<V, E>>,
    r: &V,
    topic: &V,
    parser: V,
) -> Result<(), E> {
    let target = get(&h, r, "messageHandlers")?;
    let current = attached(&h, r, topic, "messageHandlers")?;
    let current = if (h.value.truthy)(&current) {
        current
    } else {
        (h.value.array)(vec![])
    };
    let c = h.clone();
    let values = method(&h, current, "filter", || {
        Ok(vec![(h.value.function)(
            "",
            1,
            Rc::new(move |args| {
                Ok((c.boolean)(!(c.value.strict_equal)(
                    &argument(&c, &args, 0),
                    &parser,
                )))
            }),
        )])
    })?;
    (h.set)(
        target,
        topic.clone(),
        (h.value.array)((h.value.spread)(&values)?),
    )
}
fn no_handlers<V: Clone, E>(h: &StreamOps<V, E>, r: &V, topic: &V) -> Result<bool, E> {
    Ok(!(h.value.truthy)(&get(
        h,
        &attached(h, r, topic, "messageHandlers")?,
        "length",
    )?))
}
fn send<V: Clone, E>(h: &StreamOps<V, E>, r: V, event: &str, topic: V) -> Result<V, E> {
    method(h, r, "send", || {
        let json = (h.value.global)("JSON")?;
        let text = method(h, json, "stringify", || {
            Ok(vec![(h.value.object)(vec![
                ("event", (h.value.string)(event)),
                ("topic", topic),
            ])])
        })?;
        Ok(vec![text])
    })
}
fn last<V: Clone, E>(h: &StreamOps<V, E>, state: &RefCell<Option<V>>) -> V {
    state
        .borrow()
        .clone()
        .unwrap_or_else(|| (h.value.undefined)())
}
fn last_field<V: Clone, E>(
    h: &StreamOps<V, E>,
    state: &RefCell<Option<V>>,
    name: &str,
) -> Result<V, E> {
    get(h, &last(h, state), name)
}
fn candle_parser<V: Clone + 'static, E: 'static>(
    h: Rc<StreamOps<V, E>>,
    state: Rc<RefCell<Option<V>>>,
    callback: V,
) -> V {
    let c = h.clone();
    (h.value.function)(
        "parser",
        1,
        Rc::new(move |args| {
            let json = argument(&c, &args, 0);
            let timestamp = (c.divide)(
                path(&c, &json, &["data", "startTime"])?,
                (c.value.number)(1000.),
            )?;
            if (c.value.truthy)(&last(&c, &state))
                && (c.greater)(last_field(&c, &state, "timestamp")?, timestamp.clone())?
            {
                return Ok((c.value.undefined)());
            }
            let value = if !(c.value.truthy)(&last(&c, &state))
                || (c.less)(last_field(&c, &state, "timestamp")?, timestamp.clone())?
            {
                let mut fields = vec![("timestamp", timestamp)];
                for name in ["open", "high", "low", "close", "volume"] {
                    fields.push((name, path(&c, &json, &["data", name])?));
                }
                (c.value.object)(fields)
            } else {
                let prior = (c.spread_object)(last(&c, &state), vec![])?;
                let high = method(&c, (c.value.global)("Math")?, "max", || {
                    Ok(vec![
                        last_field(&c, &state, "high")?,
                        global(&c, "parseFloat", || {
                            Ok(vec![path(&c, &json, &["data", "high"])?])
                        })?,
                    ])
                })?;
                let low = method(&c, (c.value.global)("Math")?, "min", || {
                    Ok(vec![
                        last_field(&c, &state, "low")?,
                        global(&c, "parseFloat", || {
                            Ok(vec![path(&c, &json, &["data", "low"])?])
                        })?,
                    ])
                })?;
                (c.define)(prior.clone(), "high", high)?;
                (c.define)(prior.clone(), "low", low)?;
                (c.define)(prior.clone(), "close", path(&c, &json, &["data", "close"])?)?;
                (c.define)(
                    prior.clone(),
                    "volume",
                    path(&c, &json, &["data", "volume"])?,
                )?;
                prior
            };
            *state.borrow_mut() = Some(value);
            call(&c, callback.clone(), vec![last(&c, &state)])?;
            Ok((c.value.undefined)())
        }),
    )
}
fn trade_parser<V: Clone + 'static, E: 'static>(
    h: Rc<StreamOps<V, E>>,
    state: Rc<RefCell<Option<V>>>,
    opts: V,
    callback: V,
) -> V {
    let c = h.clone();
    (h.value.function)(
        "secondaryParser",
        1,
        Rc::new(move |args| {
            let json = argument(&c, &args, 0);
            let timestamp = (c.divide)(get(&c, &json, "ts")?, (c.value.number)(1000.))?;
            let data = key(&c, &get(&c, &json, "data")?, &(c.value.number)(0.))?;
            if !(c.value.truthy)(&data) {
                return Ok((c.value.undefined)());
            }
            let price = global(&c, "parseFloat", || Ok(vec![get(&c, &data, "p")?]))?;
            let amount = global(&c, "parseFloat", || Ok(vec![get(&c, &data, "a")?]))?;
            let start = import(&c, "getCandleStartTime", || {
                Ok(vec![get(&c, &opts, "interval")?, timestamp])
            })?;
            let seconds = method(&c, get(&c, &opts, "interval")?, "endsWith", || {
                Ok(vec![(c.value.string)("s")])
            })?;
            if (c.value.truthy)(&seconds)
                && (!(c.value.truthy)(&last(&c, &state))
                    || !(c.value.strict_equal)(&last_field(&c, &state, "timestamp")?, &start))
            {
                let previous = last(&c, &state);
                let open = if (c.is_nullish)(&previous) {
                    (c.value.undefined)()
                } else {
                    get(&c, &previous, "close")?
                };
                let open = if (c.value.truthy)(&open) {
                    open
                } else {
                    price.clone()
                };
                *state.borrow_mut() = Some((c.value.object)(vec![
                    ("timestamp", start.clone()),
                    ("open", open),
                    ("high", price.clone()),
                    ("low", price.clone()),
                    ("close", price.clone()),
                    ("volume", (c.value.number)(0.)),
                ]));
            }
            if !(c.value.truthy)(&last(&c, &state)) {
                return Ok((c.value.undefined)());
            }
            if (c.greater)(last_field(&c, &state, "timestamp")?, start.clone())? {
                method(&c, (c.value.global)("console")?, "warn", || {
                    Ok(vec![
                        (c.value.string)("Received trade with timestamp in the past, ignoring:"),
                        json,
                    ])
                })?;
                return Ok((c.value.undefined)());
            }
            if (c.less)(last_field(&c, &state, "timestamp")?, start.clone())? {
                *state.borrow_mut() = Some((c.value.object)(vec![
                    ("timestamp", start),
                    ("open", last_field(&c, &state, "close")?),
                    ("high", last_field(&c, &state, "close")?),
                    ("low", last_field(&c, &state, "close")?),
                    ("close", last_field(&c, &state, "close")?),
                    ("volume", (c.value.number)(0.)),
                ]));
            }
            let previous = (c.spread_object)(last(&c, &state), vec![])?;
            let high = method(&c, (c.value.global)("Math")?, "max", || {
                Ok(vec![last_field(&c, &state, "high")?, price.clone()])
            })?;
            let low = method(&c, (c.value.global)("Math")?, "min", || {
                Ok(vec![last_field(&c, &state, "low")?, price.clone()])
            })?;
            let volume = import(&c, "add", || {
                Ok(vec![last_field(&c, &state, "volume")?, amount])
            })?;
            (c.define)(previous.clone(), "high", high)?;
            (c.define)(previous.clone(), "low", low)?;
            (c.define)(previous.clone(), "close", price)?;
            (c.define)(previous.clone(), "volume", volume)?;
            *state.borrow_mut() = Some(previous);
            call(&c, callback.clone(), vec![last(&c, &state)])?;
            Ok((c.value.undefined)())
        }),
    )
}
struct CandleSubscription<V> {
    receiver: V,
    topic: V,
    secondary: V,
    parser: V,
    trades: V,
}
fn wait_candle<V: Clone + 'static, E: 'static>(
    h: Rc<StreamOps<V, E>>,
    state: Rc<CandleSubscription<V>>,
) -> Result<V, E> {
    if (h.value.truthy)(&get(&h, &state.receiver, "isConnected")?) {
        if !(h.value.truthy)(&get(&h, &state.receiver, "isDisposed")?) {
            add_handler(&h, &state.receiver, &state.topic, state.parser.clone())?;
            send(&h, state.receiver.clone(), "subscribe", state.topic.clone())?;
            (h.set)(
                get(&h, &state.receiver, "topics")?,
                state.topic.clone(),
                state.topic.clone(),
            )?;
            if !(h.value.truthy)(&path(
                &h,
                &state.receiver,
                &["parent", "options", "publicDataOnly"],
            )?) {
                add_handler(&h, &state.receiver, &state.secondary, state.trades.clone())?;
                send(
                    &h,
                    state.receiver.clone(),
                    "subscribe",
                    state.secondary.clone(),
                )?;
                (h.set)(
                    get(&h, &state.receiver, "topics")?,
                    state.secondary.clone(),
                    state.secondary.clone(),
                )?;
            }
        }
    } else {
        global(&h, "setTimeout", || {
            let c = h.clone();
            let s = state.clone();
            Ok(vec![
                (h.value.function)("", 0, Rc::new(move |_| wait_candle(c.clone(), s.clone()))),
                (h.value.number)(100.),
            ])
        })?;
    }
    Ok((h.value.undefined)())
}
pub fn listen_ohlcv<V: Clone + 'static, E: 'static>(
    h: Rc<StreamOps<V, E>>,
    receiver: V,
    opts: V,
    callback: V,
) -> Result<V, E> {
    let symbol = import(&h, "reverseSymbol", || Ok(vec![get(&h, &opts, "symbol")?]))?;
    let symbol = (h.template)(&["", ""], vec![symbol])?;
    let interval = get(&h, &opts, "interval")?;
    let topic = (h.template)(&["", "@kline_", ""], vec![symbol, interval])?;
    let symbol = import(&h, "reverseSymbol", || Ok(vec![get(&h, &opts, "symbol")?]))?;
    let secondary = (h.template)(&["", "@trades"], vec![symbol])?;
    let last = Rc::new(RefCell::new(None));
    let parser = candle_parser(h.clone(), last.clone(), callback.clone());
    let trades = trade_parser(h.clone(), last, opts, callback);
    let state = Rc::new(CandleSubscription {
        receiver,
        topic,
        secondary,
        parser,
        trades,
    });
    wait_candle(h.clone(), state.clone())?;
    let c = h.clone();
    Ok((h.value.function)(
        "",
        0,
        Rc::new(move |_| {
            remove_handler(
                c.clone(),
                &state.receiver,
                &state.topic,
                state.parser.clone(),
            )?;
            remove_handler(
                c.clone(),
                &state.receiver,
                &state.secondary,
                state.trades.clone(),
            )?;
            if no_handlers(&c, &state.receiver, &state.topic)? {
                (c.delete)(get(&c, &state.receiver, "topics")?, state.topic.clone())?;
                (c.delete)(get(&c, &state.receiver, "topics")?, state.secondary.clone())?;
            }
            for topic in [&state.topic, &state.secondary] {
                if (c.value.truthy)(&get(&c, &state.receiver, "isConnected")?)
                    && no_handlers(&c, &state.receiver, topic)?
                {
                    method(
                        &c,
                        state.receiver.clone(),
                        "cleanupMessageHandlerStats",
                        || Ok(vec![topic.clone()]),
                    )?;
                    send(&c, state.receiver.clone(), "unsubscribe", topic.clone())?;
                }
            }
            Ok((c.value.undefined)())
        }),
    ))
}
struct BookSubscription<V> {
    receiver: V,
    topic: V,
    book: V,
    inner: V,
    callback: V,
    timeout: RefCell<V>,
    retry: RefCell<V>,
    loading: RefCell<bool>,
    symbol: V,
}
fn optional<V: Clone, E>(h: &StreamOps<V, E>, value: &V, field: &str) -> Result<V, E> {
    if (h.is_nullish)(value) {
        Ok((h.value.undefined)())
    } else {
        get(h, value, field)
    }
}
fn is_delta<V: Clone, E>(h: &StreamOps<V, E>, update: &V) -> Result<bool, E> {
    for side in ["bids", "asks"] {
        let test = method(h, (h.value.global)("Array")?, "isArray", || {
            let data = optional(h, update, "data")?;
            Ok(vec![optional(h, &data, side)?])
        })?;
        if !(h.value.truthy)(&test) {
            return Ok(false);
        }
    }
    Ok(true)
}
fn map_snapshot_side<V: Clone + 'static, E: 'static>(
    h: Rc<StreamOps<V, E>>,
    data: &V,
    side: &str,
) -> Result<V, E> {
    method(&h, get(&h, data, side)?, "map", || {
        let c = h.clone();
        Ok(vec![(h.value.function)(
            "",
            1,
            Rc::new(move |args| {
                let row = argument(&c, &args, 0);
                Ok((c.value.object)(vec![
                    ("price", get(&c, &row, "price")?),
                    ("amount", get(&c, &row, "quantity")?),
                    ("total", (c.value.number)(0.)),
                ]))
            }),
        )])
    })
}
fn fetch_snapshot_task<V: Clone + 'static, E: 'static>(
    h: Rc<StreamOps<V, E>>,
    state: Rc<BookSubscription<V>>,
) -> Task<V, E> {
    Box::pin(async move {
        if (h.value.truthy)(&get(&h, &state.receiver, "isDisposed")?)
            || (h.value.truthy)(&get(&h, &state.inner, "isSnapshotLoaded")?)
            || *state.loading.borrow()
        {
            return Ok((h.value.undefined)());
        }
        *state.loading.borrow_mut() = true;
        let work: Result<(), E> = async {
            let response = method(
                &h,
                path(&h, &state.receiver, &["parent", "xhr"])?,
                "get",
                || {
                    let endpoints = (h.import)("ENDPOINTS")?;
                    let endpoint = get(&h, &endpoints, "ORDERBOOK")?;
                    let endpoint = (h.template)(&["", ""], vec![endpoint])?;
                    let symbol = import(&h, "reverseSymbol", || Ok(vec![state.symbol.clone()]))?;
                    Ok(vec![(h.template)(&["", "/", ""], vec![endpoint, symbol])?])
                },
            )?;
            let response = (h.await_value)(response).await?;
            let data = get(&h, &response, "data")?;
            if !(h.value.truthy)(&get(&h, &state.receiver, "isDisposed")?) {
                assign(&h, state.book.clone(), "bids", || {
                    map_snapshot_side(h.clone(), &data, "bids")
                })?;
                assign(&h, state.book.clone(), "asks", || {
                    map_snapshot_side(h.clone(), &data, "asks")
                })?;
                assign(&h, state.inner.clone(), "updates", || {
                    let c = h.clone();
                    let d = data.clone();
                    method(&h, get(&h, &state.inner, "updates")?, "filter", || {
                        Ok(vec![(h.value.function)(
                            "",
                            1,
                            Rc::new(move |args| {
                                let update = argument(&c, &args, 0);
                                if !is_delta(&c, &update)? {
                                    return Ok((c.boolean)(false));
                                }
                                let timestamp =
                                    global(&c, "Number", || Ok(vec![get(&c, &update, "ts")?]))?;
                                let finite =
                                    method(&c, (c.value.global)("Number")?, "isFinite", || {
                                        Ok(vec![timestamp.clone()])
                                    })?;
                                Ok((c.boolean)(if (c.value.truthy)(&finite) {
                                    (c.greater)(timestamp, get(&c, &d, "timestamp")?)?
                                } else {
                                    true
                                }))
                            }),
                        )])
                    })
                })?;
                method(&h, get(&h, &state.inner, "updates")?, "forEach", || {
                    let c = h.clone();
                    let s = state.clone();
                    Ok(vec![(h.value.function)(
                        "",
                        1,
                        Rc::new(move |args| {
                            let update = argument(&c, &args, 0);
                            if is_delta(&c, &update)? {
                                method(&c, s.receiver.clone(), "processOrderBookUpdate", || {
                                    Ok(vec![s.book.clone(), update])
                                })?;
                            }
                            Ok((c.value.undefined)())
                        }),
                    )])
                })?;
                import(&h, "calcOrderBookTotal", || Ok(vec![state.book.clone()]))?;
                property(
                    &h,
                    state.inner.clone(),
                    "isSnapshotLoaded",
                    (h.boolean)(true),
                )?;
                property(&h, state.inner.clone(), "updates", (h.value.array)(vec![]))?;
                call(&h, state.callback.clone(), vec![state.book.clone()])?;
            }
            Ok(())
        }
        .await;
        let caught = match work {
            Ok(()) => Ok(()),
            Err(_error) => (|| {
                if !(h.value.truthy)(&get(&h, &state.receiver, "isDisposed")?)
                    && !(h.value.truthy)(&get(&h, &state.inner, "isSnapshotLoaded")?)
                    && !(h.value.truthy)(&state.retry.borrow().clone())
                {
                    let timer = global(&h, "setTimeout", || {
                        let c = h.clone();
                        let s = state.clone();
                        Ok(vec![
                            (h.value.function)(
                                "",
                                0,
                                Rc::new(move |_| {
                                    *s.retry.borrow_mut() = (c.null)();
                                    fetch_snapshot(c.clone(), s.clone());
                                    Ok((c.value.undefined)())
                                }),
                            ),
                            (h.value.number)(1000.),
                        ])
                    })?;
                    *state.retry.borrow_mut() = timer;
                }
                Ok(())
            })(),
        };
        *state.loading.borrow_mut() = false;
        caught?;
        Ok((h.value.undefined)())
    })
}
fn fetch_snapshot<V: Clone + 'static, E: 'static>(
    h: Rc<StreamOps<V, E>>,
    state: Rc<BookSubscription<V>>,
) -> V {
    (h.promise)(fetch_snapshot_task(h.clone(), state))
}
fn book_parser<V: Clone + 'static, E: 'static>(
    h: Rc<StreamOps<V, E>>,
    state: Rc<BookSubscription<V>>,
) -> V {
    let c = h.clone();
    (h.value.function)(
        "parser",
        1,
        Rc::new(move |args| {
            let update = argument(&c, &args, 0);
            if !is_delta(&c, &update)? {
                if !(c.value.truthy)(&get(&c, &state.inner, "isSnapshotLoaded")?) {
                    fetch_snapshot(c.clone(), state.clone());
                }
                return Ok((c.value.undefined)());
            }
            if !(c.value.truthy)(&get(&c, &state.inner, "isSnapshotLoaded")?) {
                method(&c, get(&c, &state.inner, "updates")?, "push", || {
                    Ok(vec![update])
                })?;
                fetch_snapshot(c.clone(), state.clone());
                return Ok((c.value.undefined)());
            }
            method(&c, state.receiver.clone(), "processOrderBookUpdate", || {
                Ok(vec![state.book.clone(), update])
            })?;
            import(&c, "calcOrderBookTotal", || Ok(vec![state.book.clone()]))?;
            call(&c, state.callback.clone(), vec![state.book.clone()])?;
            Ok((c.value.undefined)())
        }),
    )
}
fn wait_book<V: Clone + 'static, E: 'static>(
    h: Rc<StreamOps<V, E>>,
    state: Rc<BookSubscription<V>>,
    parser: V,
) -> Result<V, E> {
    if (h.value.truthy)(&get(&h, &state.receiver, "isConnected")?) {
        if !(h.value.truthy)(&get(&h, &state.receiver, "isDisposed")?) {
            add_handler(&h, &state.receiver, &state.topic, parser)?;
            (h.set)(
                get(&h, &state.receiver, "topics")?,
                state.topic.clone(),
                state.topic.clone(),
            )?;
            send(&h, state.receiver.clone(), "subscribe", state.topic.clone())?;
            fetch_snapshot(h.clone(), state.clone());
        }
    } else {
        let timer = global(&h, "setTimeout", || {
            let c = h.clone();
            let s = state.clone();
            Ok(vec![
                (h.value.function)(
                    "",
                    0,
                    Rc::new(move |_| wait_book(c.clone(), s.clone(), parser.clone())),
                ),
                (h.value.number)(100.),
            ])
        })?;
        *state.timeout.borrow_mut() = timer;
    }
    Ok((h.value.undefined)())
}
pub fn listen_order_book<V: Clone + 'static, E: 'static>(
    h: Rc<StreamOps<V, E>>,
    receiver: V,
    symbol: V,
    callback: V,
) -> Result<V, E> {
    let reversed = import(&h, "reverseSymbol", || Ok(vec![symbol.clone()]))?;
    let topic = (h.template)(&["", "@orderbookupdate"], vec![reversed])?;
    let book = (h.value.object)(vec![
        ("bids", (h.value.array)(vec![])),
        ("asks", (h.value.array)(vec![])),
    ]);
    let id = import(&h, "nanoid", || Ok(vec![]))?;
    let inner = key(&h, &get(&h, &receiver, "orderBookInnerState")?, &id)?;
    if !(h.value.truthy)(&inner) {
        let target = get(&h, &receiver, "orderBookInnerState")?;
        (h.set)(
            target,
            id.clone(),
            (h.value.object)(vec![
                ("updates", (h.value.array)(vec![])),
                ("isSnapshotLoaded", (h.boolean)(false)),
            ]),
        )?;
    }
    let inner = key(&h, &get(&h, &receiver, "orderBookInnerState")?, &id)?;
    let state = Rc::new(BookSubscription {
        receiver,
        topic,
        book,
        inner,
        callback,
        symbol,
        timeout: RefCell::new((h.null)()),
        retry: RefCell::new((h.null)()),
        loading: RefCell::new(false),
    });
    let parser = book_parser(h.clone(), state.clone());
    wait_book(h.clone(), state.clone(), parser.clone())?;
    let c = h.clone();
    Ok((h.value.function)(
        "",
        0,
        Rc::new(move |_| {
            remove_handler(c.clone(), &state.receiver, &state.topic, parser.clone())?;
            if no_handlers(&c, &state.receiver, &state.topic)? {
                (c.delete)(get(&c, &state.receiver, "topics")?, state.topic.clone())?;
            }
            property(&c, state.book.clone(), "asks", (c.value.array)(vec![]))?;
            property(&c, state.book.clone(), "bids", (c.value.array)(vec![]))?;
            property(&c, state.inner.clone(), "updates", (c.value.array)(vec![]))?;
            property(
                &c,
                state.inner.clone(),
                "isSnapshotLoaded",
                (c.boolean)(false),
            )?;
            for timer in [&state.timeout, &state.retry] {
                if (c.value.truthy)(&timer.borrow().clone()) {
                    global(&c, "clearTimeout", || Ok(vec![timer.borrow().clone()]))?;
                    *timer.borrow_mut() = (c.null)();
                }
            }
            if (c.value.truthy)(&get(&c, &state.receiver, "isConnected")?)
                && no_handlers(&c, &state.receiver, &state.topic)?
            {
                method(
                    &c,
                    state.receiver.clone(),
                    "cleanupMessageHandlerStats",
                    || Ok(vec![state.topic.clone()]),
                )?;
                send(
                    &c,
                    state.receiver.clone(),
                    "unsubscribe",
                    state.topic.clone(),
                )?;
            }
            Ok((c.value.undefined)())
        }),
    ))
}
