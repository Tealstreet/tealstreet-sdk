use crate::{legacy_metadata::Task, public_canonical::CanonicalOps};
use std::rc::Rc;

pub struct LifecycleOps<V, E> {
    pub value: Rc<CanonicalOps<V, E>>,
    /// Resume on the language promise job queue, including already fulfilled values.
    pub await_value: Rc<dyn Fn(V) -> Task<V, E>>,
    /// Poll immediately, retain pending work after consumers drop, and resume through promise jobs.
    pub promise: Rc<dyn Fn(Task<V, E>) -> V>,
    pub import: Rc<dyn Fn(&str) -> Result<V, E>>,
    pub spread_object: Rc<dyn Fn(V, Vec<(&'static str, V)>) -> Result<V, E>>,
    pub is_nullish: Rc<dyn Fn(&V) -> bool>,
    pub is_undefined: Rc<dyn Fn(&V) -> bool>,
    pub boolean: Rc<dyn Fn(bool) -> V>,
    pub add: Rc<dyn Fn(V, V) -> Result<V, E>>,
    pub remainder: Rc<dyn Fn(V, V) -> Result<V, E>>,
    pub less: Rc<dyn Fn(V, V) -> Result<bool, E>>,
    pub error_value: Rc<dyn Fn(E) -> V>,
    /// False entries advance without reading iterator-result value; close after the pattern.
    pub binding: Rc<dyn Fn(V, &[bool]) -> Result<Vec<V>, E>>,
    pub iterate: Rc<dyn Fn(V, &mut dyn FnMut(V) -> Result<(), E>) -> Result<(), E>>,
}
fn get<V, E>(h: &LifecycleOps<V, E>, value: &V, key: &str) -> Result<V, E> {
    (h.value.get)(value, key)
}
fn path<V: Clone, E>(h: &LifecycleOps<V, E>, value: &V, keys: &[&str]) -> Result<V, E> {
    let mut v = value.clone();
    for k in keys {
        v = get(h, &v, k)?;
    }
    Ok(v)
}
fn method<V, E>(
    h: &LifecycleOps<V, E>,
    receiver: V,
    key: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let c = get(h, &receiver, key)?;
    (h.value.invoke)(c, receiver, args()?)
}
fn imported<V, E>(
    h: &LifecycleOps<V, E>,
    name: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let c = (h.import)(name)?;
    (h.value.invoke)(c, (h.value.undefined)(), args()?)
}
fn disposed<V, E>(h: &LifecycleOps<V, E>, receiver: &V) -> Result<bool, E> {
    Ok((h.value.truthy)(&get(h, receiver, "isDisposed")?))
}
fn loaded<V: Clone, E>(
    h: &LifecycleOps<V, E>,
    receiver: &V,
    flags: &[&'static str],
) -> Result<V, E> {
    (h.spread_object)(
        path(h, receiver, &["store", "loaded"])?,
        flags.iter().map(|k| (*k, (h.boolean)(true))).collect(),
    )
}
fn update<V: Clone, E>(
    h: &LifecycleOps<V, E>,
    receiver: &V,
    args: impl FnOnce() -> Result<Vec<(&'static str, V)>, E>,
) -> Result<V, E> {
    method(h, get(h, receiver, "store")?, "update", || {
        Ok(vec![(h.value.object)(args()?)])
    })
}
fn callback<V: Clone + 'static, E: 'static>(
    h: Rc<LifecycleOps<V, E>>,
    receiver: V,
    key: &'static str,
    f: Rc<dyn Fn(Rc<LifecycleOps<V, E>>, V, V) -> Result<V, E>>,
) -> V {
    let o = h.value.clone();
    (o.function)(
        key,
        1,
        Rc::new(move |a| {
            let v = a.first().cloned().unwrap_or_else(|| (h.value.undefined)());
            f(h.clone(), receiver.clone(), v)
        }),
    )
}
fn then<V: Clone + 'static, E: 'static>(
    h: Rc<LifecycleOps<V, E>>,
    receiver: V,
    key: &str,
    f: Rc<dyn Fn(Rc<LifecycleOps<V, E>>, V, V) -> Result<V, E>>,
) -> Result<V, E> {
    let promise = method(&h, receiver.clone(), key, || Ok(vec![]))?;
    method(&h, promise, "then", || {
        Ok(vec![callback(h.clone(), receiver, "", f)])
    })
}
fn account_loaded<V: Clone + 'static, E: 'static>(
    h: Rc<LifecycleOps<V, E>>,
    receiver: V,
    value: V,
    key: &'static str,
) -> Result<V, E> {
    if disposed(&h, &receiver)? {
        return Ok((h.value.undefined)());
    }
    if key == "positions" {
        method(
            &h,
            get(&h, &receiver, "store")?,
            "addOrUpdatePositionsFull",
            || Ok(vec![value.clone()]),
        )?;
        update(&h, &receiver, || {
            Ok(vec![("loaded", loaded(&h, &receiver, &["positions"])?)])
        })?;
    } else {
        update(&h, &receiver, || {
            Ok(vec![
                (key, value.clone()),
                ("loaded", loaded(&h, &receiver, &[key])?),
            ])
        })?;
    }
    Ok(value)
}
fn start_task<V: Clone + 'static, E: 'static>(
    h: Rc<LifecycleOps<V, E>>,
    receiver: V,
) -> Task<V, E> {
    Box::pin(async move {
        let external = method(&h, receiver.clone(), "initExternalPublicData", || {
            Ok(vec![])
        })?;
        let external = (h.await_value)(external).await?;
        let public_only = get(&h, &receiver, "isPublicDataOnly")?;
        if !(h.value.truthy)(&external) {
            let promise = method(&h, (h.value.global)("Promise")?, "all", || {
                let markets = then(
                    h.clone(),
                    receiver.clone(),
                    "fetchMarkets",
                    Rc::new(|h, r, markets| {
                        if disposed(&h, &r)? {
                            return Ok((h.value.array)(vec![]));
                        }
                        update(&h, &r, || {
                            Ok(vec![
                                ("markets", markets.clone()),
                                ("loaded", loaded(&h, &r, &["markets"])?),
                            ])
                        })?;
                        method(
                            &h,
                            get(&h, &r, "publicWebsocket")?,
                            "connectAndSubscribe",
                            || Ok(vec![]),
                        )?;
                        Ok(markets)
                    }),
                )?;
                let raw = method(&h, receiver.clone(), "fetchTickersRaw", || Ok(vec![]))?;
                Ok(vec![(h.value.array)(vec![markets, raw])])
            })?;
            let pair = (h.await_value)(promise).await?;
            let raw = (h.binding)(pair, &[false, true])?[1].clone();
            if disposed(&h, &receiver)? {
                return Ok((h.value.undefined)());
            }
            let tickers = method(&h, receiver.clone(), "parseTickers", || Ok(vec![raw]))?;
            update(&h, &receiver, || {
                Ok(vec![
                    ("tickers", tickers),
                    ("loaded", loaded(&h, &receiver, &["tickers"])?),
                ])
            })?;
            method(&h, receiver.clone(), "startFundingRatesRefresh", || {
                Ok(vec![])
            })?;
        }
        if (h.value.truthy)(&public_only) {
            update(&h, &receiver, || {
                Ok(vec![(
                    "loaded",
                    loaded(&h, &receiver, &["orders", "positions", "balance"])?,
                )])
            })?;
            return Ok((h.value.undefined)());
        }
        method(
            &h,
            get(&h, &receiver, "privateWebsocket")?,
            "connectAndSubscribe",
            || Ok(vec![]),
        )?;
        let all = method(&h, (h.value.global)("Promise")?, "all", || {
            let mut values = vec![];
            for (member, key) in [
                ("fetchBalance", "balance"),
                ("fetchPositions", "positions"),
                ("fetchOrders", "orders"),
            ] {
                values.push(then(
                    h.clone(),
                    receiver.clone(),
                    member,
                    Rc::new(move |h, r, v| account_loaded(h, r, v, key)),
                )?);
            }
            Ok(vec![(h.value.array)(values)])
        })?;
        (h.await_value)(all).await?;
        if disposed(&h, &receiver)? {
            return Ok((h.value.undefined)());
        }
        let promise = method(&h, receiver.clone(), "tick", || Ok(vec![]))?;
        (h.await_value)(promise).await?;
        method(&h, receiver, "log", || {
            Ok(vec![(h.value.string)("Ready to trade on WOO X")])
        })?;
        Ok((h.value.undefined)())
    })
}
fn tick_task<V: Clone + 'static, E: 'static>(
    h: Rc<LifecycleOps<V, E>>,
    receiver: V,
    count: Option<V>,
) -> Task<V, E> {
    Box::pin(async move {
        let count = count
            .filter(|v| !(h.is_undefined)(v))
            .unwrap_or_else(|| (h.value.number)(0.));
        if disposed(&h, &receiver)? {
            return Ok((h.value.undefined)());
        }
        let work: Result<bool, E> = async {
            let full = (h.remainder)(count.clone(), (h.value.number)(5.))?;
            if (h.value.strict_equal)(&full, &(h.value.number)(0.)) {
                let promise = method(&h, receiver.clone(), "fetchPositionsAndBalance", || {
                    Ok(vec![])
                })?;
                let data = (h.await_value)(promise).await?;
                let positions = get(&h, &data, "positions")?;
                let balance = get(&h, &data, "balance")?;
                if disposed(&h, &receiver)? {
                    return Ok(false);
                }
                method(
                    &h,
                    get(&h, &receiver, "store")?,
                    "addOrUpdatePositionsFull",
                    || Ok(vec![positions]),
                )?;
                update(&h, &receiver, || {
                    Ok(vec![
                        ("balance", balance),
                        ("loaded", loaded(&h, &receiver, &["balance", "positions"])?),
                    ])
                })?;
            } else {
                let promise = method(&h, receiver.clone(), "fetchPositions", || Ok(vec![]))?;
                let positions = (h.await_value)(promise).await?;
                if disposed(&h, &receiver)? {
                    return Ok(false);
                }
                method(
                    &h,
                    get(&h, &receiver, "store")?,
                    "addOrUpdatePositionsFull",
                    || Ok(vec![positions]),
                )?;
            }
            let orders = (h.remainder)(
                (h.add)(count.clone(), (h.value.number)(5.))?,
                (h.value.number)(10.),
            )?;
            if (h.value.strict_equal)(&orders, &(h.value.number)(0.)) {
                let promise = method(&h, receiver.clone(), "fetchOrders", || Ok(vec![]))?;
                let orders = (h.await_value)(promise).await?;
                if disposed(&h, &receiver)? {
                    return Ok(false);
                }
                update(&h, &receiver, || {
                    Ok(vec![
                        ("orders", orders),
                        ("loaded", loaded(&h, &receiver, &["orders"])?),
                    ])
                })?;
            }
            Ok(true)
        }
        .await;
        match work {
            Ok(false) => return Ok((h.value.undefined)()),
            Err(error) => {
                let error = (h.error_value)(error);
                method(&h, receiver.clone(), "emitAndLogError", || {
                    Ok(vec![if (h.is_nullish)(&error) {
                        (h.value.undefined)()
                    } else {
                        get(&h, &error, "message")?
                    }])
                })?;
            }
            Ok(true) => {}
        }
        if !disposed(&h, &receiver)? {
            imported(&h, "loop", || {
                let c = h.clone();
                let r = receiver.clone();
                let n = count.clone();
                let callback = (h.value.function)(
                    "",
                    0,
                    Rc::new(move |_| {
                        method(&c, r.clone(), "tick", || {
                            Ok(vec![(c.add)(n.clone(), (c.value.number)(1.))?])
                        })
                    }),
                );
                let interval = method(&h, receiver.clone(), "getTickInterval", || Ok(vec![]))?;
                Ok(vec![callback, interval])
            })?;
        }
        Ok((h.value.undefined)())
    })
}
fn fetch_historical_positions_task<V: Clone + 'static, E: 'static>(
    h: Rc<LifecycleOps<V, E>>,
    receiver: V,
    opts: V,
) -> Task<V, E> {
    Box::pin(async move {
        let to = get(&h, &opts, "to")?;
        let to = if (h.is_nullish)(&to) {
            method(&h, (h.value.global)("Date")?, "now", || Ok(vec![]))?
        } else {
            to
        };
        let from = get(&h, &opts, "from")?;
        let from = if (h.is_nullish)(&from) {
            (h.value.subtract)(to.clone(), (h.value.number)(30. * 24. * 60. * 60. * 1000.))?
        } else {
            from
        };
        let promise = method(&h, receiver.clone(), "fetchHistoricalOrders", || {
            Ok(vec![(h.value.object)(vec![
                ("from", from.clone()),
                ("to", to),
            ])])
        })?;
        let orders = (h.await_value)(promise).await?;
        let executions = imported(&h, "wooOrdersToExecutions", || Ok(vec![orders]))?;
        let records = (h.value.array)(vec![]);
        let intervals = imported(&h, "reconstructPositionIntervals", || Ok(vec![executions]))?;
        (h.iterate)(intervals, &mut |interval| {
            if (h.is_undefined)(&get(&h, &interval, "closedAt")?)
                || (h.less)(get(&h, &interval, "closedAt")?, from.clone())?
            {
                return Ok(());
            }
            let record = imported(&h, "mapFoldedInterval", || {
                let account = get(&h, &opts, "account")?;
                let c = h.clone();
                let r = receiver.clone();
                let resolver = (h.value.function)(
                    "resolveSettleCurrency",
                    1,
                    Rc::new(move |args| {
                        let symbol = args
                            .first()
                            .cloned()
                            .unwrap_or_else(|| (c.value.undefined)());
                        let store = get(&c, &r, "store")?;
                        let markets = if (c.is_nullish)(&store) {
                            (c.value.undefined)()
                        } else {
                            get(&c, &store, "marketsBySymbol")?
                        };
                        let market = if (c.is_nullish)(&markets) {
                            (c.value.undefined)()
                        } else {
                            (c.value.get_key)(&markets, &symbol)?
                        };
                        let quote = if (c.is_nullish)(&market) {
                            (c.value.undefined)()
                        } else {
                            get(&c, &market, "quote")?
                        };
                        Ok(if (c.value.truthy)(&quote) {
                            quote
                        } else {
                            (c.value.string)("USDT")
                        })
                    }),
                );
                Ok(vec![
                    interval,
                    (h.value.object)(vec![
                        ("account", account),
                        ("resolveSettleCurrency", resolver),
                    ]),
                ])
            })?;
            if (h.value.truthy)(&record) {
                method(&h, records.clone(), "push", || Ok(vec![record]))?;
            }
            Ok(())
        })?;
        Ok(records)
    })
}
pub fn process_order_book_update<V: Clone + 'static, E: 'static>(
    h: &LifecycleOps<V, E>,
    book: V,
    update: V,
) -> Result<V, E> {
    for side in ["bids", "asks"] {
        let rows = path(h, &update, &["data", side])?;
        let is_array = method(h, (h.value.global)("Array")?, "isArray", || {
            Ok(vec![rows.clone()])
        })?;
        if (h.value.truthy)(&is_array) {
            let mut index = 0.;
            while (h.less)((h.value.number)(index), get(h, &rows, "length")?)? {
                imported(h, "updateOrderBookByPrice", || {
                    let price = (h.value.get_key)(
                        &(h.value.get_key)(&rows, &(h.value.number)(index))?,
                        &(h.value.number)(0.),
                    )?;
                    let amount = (h.value.get_key)(
                        &(h.value.get_key)(&rows, &(h.value.number)(index))?,
                        &(h.value.number)(1.),
                    )?;
                    Ok(vec![book.clone(), (h.value.string)(side), price, amount])
                })?;
                index += 1.;
            }
        }
    }
    Ok((h.value.undefined)())
}

pub fn start<V: Clone + 'static, E: 'static>(h: Rc<LifecycleOps<V, E>>, receiver: V) -> V {
    (h.promise)(start_task(h.clone(), receiver))
}

pub fn tick<V: Clone + 'static, E: 'static>(
    h: Rc<LifecycleOps<V, E>>,
    receiver: V,
    count: Option<V>,
) -> V {
    (h.promise)(tick_task(h.clone(), receiver, count))
}

pub fn fetch_historical_positions<V: Clone + 'static, E: 'static>(
    h: Rc<LifecycleOps<V, E>>,
    receiver: V,
    opts: V,
) -> V {
    (h.promise)(fetch_historical_positions_task(h.clone(), receiver, opts))
}
