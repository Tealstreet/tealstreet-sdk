use crate::bootstrap_reads::{BootstrapAwait, BootstrapOps};
use std::rc::Rc;

pub struct SpotBodyOps<V, E> {
    pub base: Rc<BootstrapOps<V, E>>,
    /// Standard named AsyncFunction arrow with eager, retained promise completion.
    pub async_arrow: Rc<dyn Fn(&str, usize, Rc<dyn Fn(Vec<V>) -> BootstrapAwait<V, E>>) -> V>,
    pub triple: Rc<dyn Fn(V) -> Result<(V, V, V), E>>,
    pub has: Rc<dyn Fn(&str, &V) -> Result<bool, E>>,
    pub new_error: Rc<dyn Fn(V, V) -> Result<E, E>>,
    pub comparator: Rc<dyn Fn(Rc<dyn Fn(V, V) -> Result<V, E>>) -> V>,
    pub spread_arguments: Rc<dyn Fn(V) -> Result<Vec<V>, E>>,
    pub assignment_reference: Rc<dyn Fn(V, V) -> Result<V, E>>,
    pub assign: Rc<dyn Fn(V, V) -> Result<(), E>>,
}
fn call<V, E>(
    h: &SpotBodyOps<V, E>,
    path: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let v = &h.base.value;
    let callee = (v.capture)(path)?;
    (v.invoke)(callee, args()?)
}
fn member<V, E>(
    h: &SpotBodyOps<V, E>,
    value: V,
    key: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let callee = (h.base.member)(value, key)?;
    (h.base.value.invoke)(callee, args()?)
}
fn optional<V, E>(h: &SpotBodyOps<V, E>, value: &V, key: &str) -> Result<V, E> {
    if (h.base.value.nullish)(value) {
        Ok((h.base.value.undefined)())
    } else {
        (h.base.value.get)(value, key)
    }
}
fn fallback<V, E>(
    h: &SpotBodyOps<V, E>,
    value: V,
    next: impl FnOnce() -> Result<V, E>,
) -> Result<V, E> {
    if (h.base.value.truthy)(&value) {
        Ok(value)
    } else {
        next()
    }
}
fn intrinsic<V, E>(h: &SpotBodyOps<V, E>, name: &str, args: Vec<V>) -> Result<V, E> {
    (h.base.value.intrinsic)(name, args)
}
fn message<V, E>(h: &SpotBodyOps<V, E>, e: &E) -> Result<V, E> {
    let value = (h.base.value.error_value)(e);
    let response = optional(h, &value, "response")?;
    let data = optional(h, &response, "data")?;
    fallback(h, optional(h, &data, "msg")?, || {
        optional(h, &value, "message")
    })
}
fn emit<V, E>(h: &SpotBodyOps<V, E>, msg: impl FnOnce() -> Result<V, E>) -> Result<(), E> {
    call(h, "this.emitAndLogError", || Ok(vec![msg()?]))?;
    Ok(())
}
fn error<V, E>(h: &SpotBodyOps<V, E>, msg: impl FnOnce() -> Result<V, E>) -> Result<E, E> {
    let ctor = (h.base.value.capture)("Error")?;
    (h.new_error)(ctor, msg()?)
}
fn field_float<V, E>(h: &SpotBodyOps<V, E>, row: &V, key: &str) -> Result<V, E> {
    call(h, "parseFloat", || {
        Ok(vec![fallback(h, (h.base.value.get)(row, key)?, || {
            Ok((h.base.value.literal)("0"))
        })?])
    })
}
fn loaded<V: Clone, E>(h: &SpotBodyOps<V, E>, fields: &[&str]) -> Result<V, E> {
    let v = &h.base.value;
    let object = (v.object)(vec![]);
    intrinsic(
        h,
        "spread-into",
        vec![object.clone(), (v.capture)("this.store.loaded")?],
    )?;
    for key in fields {
        intrinsic(
            h,
            "define-field",
            vec![object.clone(), (v.literal)(key), (h.base.boolean)(true)],
        )?;
    }
    Ok(object)
}
fn bad_code<V, E>(h: &SpotBodyOps<V, E>, data: &V) -> Result<bool, E> {
    let v = &h.base.value;
    Ok(!(v.equal)(&(v.get)(data, "code")?, &(v.literal)("200000")))
}
fn fail_message<V, E>(h: &SpotBodyOps<V, E>, data: &V, text: &str) -> Result<V, E> {
    fallback(h, (h.base.value.get)(data, "msg")?, || {
        Ok((h.base.value.literal)(text))
    })
}
async fn await_call<V, E>(h: &SpotBodyOps<V, E>, path: &str) -> Result<V, E> {
    (h.base.value.await_value)(call(h, path, || Ok(vec![]))?).await
}
pub async fn start<V: Clone + 'static, E: 'static>(h: Rc<SpotBodyOps<V, E>>) -> Result<V, E> {
    let v = &h.base.value;
    let external = await_call(&h, "this.initExternalPublicData").await?;
    if !(v.truthy)(&external) {
        let markets = await_call(&h, "this.fetchMarkets").await?;
        if (v.truthy)(&(v.capture)("this.isDisposed")?) {
            return Ok((v.undefined)());
        }
        call(&h, "this.store.update", || {
            Ok(vec![(v.object)(vec![
                ("markets", markets.clone()),
                ("loaded", loaded(&h, &["markets"])?),
            ])])
        })?;
        let tickers = await_call(&h, "this.fetchTickers").await?;
        if (v.truthy)(&(v.capture)("this.isDisposed")?) {
            return Ok((v.undefined)());
        }
        call(&h, "this.store.update", || {
            Ok(vec![(v.object)(vec![
                ("tickers", tickers.clone()),
                ("loaded", loaded(&h, &["tickers"])?),
            ])])
        })?;
        call(&h, "this.publicWebsocket.connectAndSubscribe", || {
            Ok(vec![])
        })?;
        call(&h, "this.log", || {
            let count = call(&h, "Math.min", || {
                Ok(vec![
                    (v.get)(&markets, "length")?,
                    (v.get)(&tickers, "length")?,
                ])
            })?;
            Ok(vec![intrinsic(
                &h,
                "template-loaded-spot-markets",
                vec![count],
            )?])
        })?;
    }
    let positions = await_call(&h, "this.fetchPositions").await?;
    if (v.truthy)(&(v.capture)("this.isDisposed")?) {
        return Ok((v.undefined)());
    }
    call(&h, "this.store.addOrUpdatePositionsFull", || {
        Ok(vec![positions])
    })?;
    if (v.truthy)(&(v.capture)("this.isPublicDataOnly")?) {
        call(&h, "this.store.update", || {
            Ok(vec![(v.object)(vec![
                ("orders", (v.array)(vec![])),
                (
                    "balance",
                    (v.object)(vec![
                        ("used", (v.number)(0.)),
                        ("free", (v.number)(0.)),
                        ("total", (v.number)(0.)),
                        ("upnl", (v.number)(0.)),
                    ]),
                ),
                ("loaded", loaded(&h, &["balance", "positions", "orders"])?),
            ])])
        })?;
        call(&h, "this.log", || {
            Ok(vec![(v.literal)(
                "Ready to watch public data on KuCoin Spot",
            )])
        })?;
        return Ok((v.undefined)());
    }
    call(&h, "this.privateWebsocket.connectAndSubscribe", || {
        Ok(vec![])
    })?;
    await_call(&h, "this.tick").await?;
    if (v.truthy)(&(v.capture)("this.isDisposed")?) {
        return Ok((v.undefined)());
    }
    call(&h, "this.log", || {
        Ok(vec![(v.literal)("Ready to trade on KuCoin Spot")])
    })?;
    Ok((v.undefined)())
}
pub async fn tick<V: Clone + 'static, E: 'static>(h: Rc<SpotBodyOps<V, E>>) -> Result<V, E> {
    let v = &h.base.value;
    if (v.truthy)(&(v.capture)("this.isDisposed")?)
        || (v.truthy)(&(v.capture)("this.isPublicDataOnly")?)
    {
        return Ok((v.undefined)());
    }
    let result = async {
        let promise = call(&h, "Promise.all", || {
            Ok(vec![(v.array)(vec![
                call(&h, "this.fetchOrders", || Ok(vec![]))?,
                call(&h, "this.fetchPositions", || Ok(vec![]))?,
                call(&h, "this.fetchBalance", || Ok(vec![]))?,
            ])])
        })?;
        let (orders, positions, balance) = (h.triple)((v.await_value)(promise).await?)?;
        if (v.truthy)(&(v.capture)("this.isDisposed")?) {
            return Ok(true);
        }
        call(&h, "this.store.addOrUpdatePositionsFull", || {
            Ok(vec![positions])
        })?;
        call(&h, "this.store.update", || {
            Ok(vec![(v.object)(vec![
                ("orders", orders),
                ("balance", balance),
                ("loaded", loaded(&h, &["balance", "positions", "orders"])?),
            ])])
        })?;
        Ok(false)
    }
    .await;
    match result {
        Ok(true) => return Ok((v.undefined)()),
        Ok(false) => (),
        Err(e) => emit(&h, || message(&h, &e))?,
    }
    if !(v.truthy)(&(v.capture)("this.isDisposed")?) {
        let ops = h.clone();
        call(&h, "loop", || {
            Ok(vec![
                (h.base.callback)(Rc::new(move || call(&ops, "this.tick", || Ok(vec![])))),
                call(&h, "this.getTickInterval", || Ok(vec![]))?,
            ])
        })?;
    }
    Ok((v.undefined)())
}
pub async fn fetch_balance<V: Clone + 'static, E: 'static>(
    h: Rc<SpotBodyOps<V, E>>,
) -> Result<V, E> {
    let v = &h.base.value;
    let result = async {
        let promise = call(&h, "this.xhr.get", || {
            Ok(vec![
                (v.capture)("KUCOIN_SPOT_ENDPOINTS.ACCOUNTS")?,
                (v.object)(vec![(
                    "params",
                    (v.object)(vec![("type", (v.literal)("trade"))]),
                )]),
            ])
        })?;
        let response = (v.await_value)(promise).await?;
        let data = (v.get)(&response, "data")?;
        if bad_code(&h, &data)? {
            emit(&h, || {
                fail_message(&h, &data, "Failed to fetch KuCoin Spot balance")
            })?;
            return (v.capture)("this.store.balance");
        }
        let currencies = (v.object)(vec![]);
        let coins = (v.object)(vec![]);
        let mut free = (v.number)(0.);
        let mut used = (v.number)(0.);
        let mut total = (v.number)(0.);
        let accounts = fallback(&h, (v.get)(&data, "data")?, || Ok((v.array)(vec![])))?;
        let iterator = (h.base.iterator)(accounts)?;
        while let Some(account) = (h.base.next)(&iterator)? {
            let row = (|| {
                let currency = (v.get)(&account, "currency")?;
                let available = field_float(&h, &account, "available")?;
                let holds = field_float(&h, &account, "holds")?;
                let balance = field_float(&h, &account, "balance")?;
                let price = call(&h, "this.getUsdPrice", || Ok(vec![currency.clone()]))?;
                let reference = (h.assignment_reference)(currencies.clone(), currency.clone())?;
                (h.assign)(
                    reference,
                    (v.object)(vec![
                        ("free", available.clone()),
                        ("used", holds.clone()),
                        ("total", balance.clone()),
                        ("upnl", (v.number)(0.)),
                        ("info", account.clone()),
                    ]),
                )?;
                let reference = (h.assignment_reference)(coins.clone(), currency.clone())?;
                (h.assign)(
                    reference,
                    (v.object)(vec![
                        ("base", currency),
                        ("amount", balance.clone()),
                        (
                            "notional",
                            intrinsic(&h, "multiply", vec![balance.clone(), price.clone()])?,
                        ),
                    ]),
                )?;
                free = intrinsic(
                    &h,
                    "add",
                    vec![
                        free.clone(),
                        intrinsic(&h, "multiply", vec![available, price.clone()])?,
                    ],
                )?;
                used = intrinsic(
                    &h,
                    "add",
                    vec![
                        used.clone(),
                        intrinsic(&h, "multiply", vec![holds, price.clone()])?,
                    ],
                )?;
                total = intrinsic(
                    &h,
                    "add",
                    vec![
                        total.clone(),
                        intrinsic(&h, "multiply", vec![balance, price])?,
                    ],
                )?;
                Ok(())
            })();
            if let Err(e) = row {
                return Err((h.base.close_throw)(&iterator, e));
            }
        }
        Ok((v.object)(vec![
            ("free", free),
            ("used", used),
            ("total", total),
            ("upnl", (v.number)(0.)),
            ("currencies", currencies),
            ("coins", coins),
            ("info", (v.get)(&data, "data")?),
        ]))
    }
    .await;
    match result {
        Ok(value) => Ok(value),
        Err(e) => {
            emit(&h, || message(&h, &e))?;
            (v.capture)("this.store.balance")
        }
    }
}
pub async fn fetch_orders<V: Clone + 'static, E: 'static>(
    h: Rc<SpotBodyOps<V, E>>,
    symbols: V,
) -> Result<V, E> {
    let v = &h.base.value;
    let result = async {
        let inner = h.clone();
        let fetch_symbol = (h.async_arrow)(
            "fetchSymbolOrders",
            1,
            Rc::new(move |args| {
                let symbol = args
                    .first()
                    .cloned()
                    .unwrap_or_else(|| (inner.base.value.undefined)());
                fetch_symbol_orders(inner.clone(), symbol)
            }),
        );
        let target = if (v.truthy)(&optional(&h, &symbols, "length")?) {
            symbols
        } else {
            await_call(&h, "this.fetchActiveOrderSymbols").await?
        };
        if !(v.truthy)(&(v.get)(&target, "length")?) {
            return Ok((v.array)(vec![]));
        }
        let ops = h.clone();
        let all = call(&h, "Promise.all", || {
            Ok(vec![member(&h, target, "map", || {
                Ok(vec![(h.base.mapper)(Rc::new(move |symbol| {
                    (ops.base.value.invoke)(fetch_symbol.clone(), vec![symbol])
                }))])
            })?])
        })?;
        let orders = (v.await_value)(all).await?;
        member(&h, orders, "flat", || Ok(vec![]))
    }
    .await;
    match result {
        Ok(value) => Ok(value),
        Err(e) => {
            emit(&h, || message(&h, &e))?;
            (v.capture)("this.store.orders")
        }
    }
}
fn fetch_symbol_orders<V: Clone + 'static, E: 'static>(
    h: Rc<SpotBodyOps<V, E>>,
    symbol: V,
) -> BootstrapAwait<V, E> {
    Box::pin(async move {
        let v = &h.base.value;
        let orders = (v.array)(vec![]);
        let mut page_num = (v.number)(1.);
        let mut total_page = (v.number)(1.);
        while (v.truthy)(&intrinsic(
            &h,
            "lte",
            vec![page_num.clone(), total_page.clone()],
        )?) && (v.truthy)(&intrinsic(
            &h,
            "lte",
            vec![page_num.clone(), (v.number)(100.)],
        )?) {
            let promise = call(&h, "this.xhr.get", || {
                Ok(vec![
                    (v.capture)("KUCOIN_SPOT_ENDPOINTS.OPEN_ORDERS")?,
                    (v.object)(vec![(
                        "params",
                        call(&h, "omitUndefined", || {
                            Ok(vec![(v.object)(vec![
                                ("symbol", symbol.clone()),
                                ("pageNum", page_num.clone()),
                                ("pageSize", (v.number)(50.)),
                            ])])
                        })?,
                    )]),
                ])
            })?;
            let response = (v.await_value)(promise).await?;
            let data = (v.get)(&response, "data")?;
            if bad_code(&h, &data)? {
                emit(&h, || {
                    fail_message(&h, &data, "Failed to fetch KuCoin Spot orders")
                })?;
                return Ok(orders);
            }
            let page = fallback(&h, (v.get)(&data, "data")?, || Ok((v.object)(vec![])))?;
            let items = if (v.truthy)(&call(&h, "Array.isArray", || {
                Ok(vec![(v.get)(&page, "items")?])
            })?) {
                (v.get)(&page, "items")?
            } else {
                (v.array)(vec![])
            };
            let push = (h.base.member)(orders.clone(), "push")?;
            let ops = h.clone();
            let mapped = member(&h, items.clone(), "map", || {
                Ok(vec![(h.base.mapper)(Rc::new(move |order| {
                    call(&ops, "this.mapOrder", || Ok(vec![order]))
                }))])
            })?;
            (v.invoke)(push, (h.spread_arguments)(mapped)?)?;
            let next_total = call(&h, "parseInt", || {
                let value = (v.get)(&page, "totalPage")?;
                Ok(vec![
                    intrinsic(
                        &h,
                        "template-string",
                        vec![if (v.nullish)(&value) {
                            (v.literal)("")
                        } else {
                            value
                        }],
                    )?,
                    (v.number)(10.),
                ])
            })?;
            if (v.truthy)(&call(&h, "Number.isFinite", || {
                Ok(vec![next_total.clone()])
            })?) {
                total_page = next_total.clone();
            }
            let more = if (v.truthy)(&call(&h, "Number.isFinite", || Ok(vec![next_total]))?) {
                intrinsic(&h, "lt", vec![page_num.clone(), total_page.clone()])?
            } else {
                intrinsic(&h, "gte", vec![(v.get)(&items, "length")?, (v.number)(50.)])?
            };
            if !(v.truthy)(&more) {
                break;
            }
            page_num = intrinsic(&h, "add", vec![page_num, (v.number)(1.)])?;
        }
        Ok(orders)
    })
}
fn market<V, E>(h: &SpotBodyOps<V, E>, row: &V) -> Result<V, E> {
    let v = &h.base.value;
    intrinsic(
        h,
        "get-index",
        vec![
            (v.capture)("this.store.marketsBySymbol")?,
            (v.get)(row, "symbol")?,
        ],
    )
}
fn missing_market<V, E>(h: &SpotBodyOps<V, E>, row: &V) -> Result<E, E> {
    error(h, || {
        intrinsic(
            h,
            "template-missing-market",
            vec![(h.base.value.get)(row, "symbol")?],
        )
    })
}
fn adjusted<V, E>(
    h: &SpotBodyOps<V, E>,
    row: &V,
    market: &V,
    key: &str,
    precision: &str,
) -> Result<V, E> {
    call(h, "adjust", || {
        Ok(vec![
            (h.base.value.get)(row, key)?,
            (h.base.value.get)(&(h.base.value.get)(market, "precision")?, precision)?,
        ])
    })
}
pub async fn place_order<V: Clone + 'static, E: 'static>(
    h: Rc<SpotBodyOps<V, E>>,
    opts: V,
) -> Result<V, E> {
    let v = &h.base.value;
    let client_id = call(&h, "this.resolveCallerClientOrderId", || {
        Ok(vec![opts.clone()])
    })?;
    if !(v.equal)(&(v.get)(&opts, "type")?, &(v.capture)("OrderType.Limit")?)
        && !(v.equal)(&(v.get)(&opts, "type")?, &(v.capture)("OrderType.Market")?)
    {
        let msg = intrinsic(
            &h,
            "template-unsupported-spot-order",
            vec![(v.get)(&opts, "type")?],
        )?;
        emit(&h, || Ok(msg.clone()))?;
        return Err(error(&h, || Ok(msg))?);
    }
    if (v.truthy)(&(v.get)(&opts, "stopLoss")?) || (v.truthy)(&(v.get)(&opts, "takeProfit")?) {
        let msg = (v.literal)("KuCoin Spot attached TP/SL orders are not supported");
        emit(&h, || Ok(msg.clone()))?;
        return Err(error(&h, || Ok(msg))?);
    }
    let market = market(&h, &opts)?;
    if !(v.truthy)(&market) {
        return Err(missing_market(&h, &opts)?);
    }
    let amount = adjusted(&h, &opts, &market, "amount", "amount")?;
    let price = if (v.truthy)(&(v.get)(&opts, "price")?)
        && (v.equal)(&(v.get)(&opts, "type")?, &(v.capture)("OrderType.Limit")?)
    {
        adjusted(&h, &opts, &market, "price", "price")?
    } else {
        (v.undefined)()
    };
    let tif = if (v.equal)(
        &(v.get)(&opts, "timeInForce")?,
        &(v.capture)("OrderTimeInForce.ImmediateOrCancel")?,
    ) {
        "IOC"
    } else if (v.equal)(
        &(v.get)(&opts, "timeInForce")?,
        &(v.capture)("OrderTimeInForce.FillOrKill")?,
    ) {
        "FOK"
    } else {
        "GTC"
    };
    let payload = call(&h, "omitUndefined", || {
        Ok(vec![(v.object)(vec![
            (
                "clientOid",
                if (v.nullish)(&client_id) {
                    call(&h, "uuid", || Ok(vec![]))?
                } else {
                    client_id
                },
            ),
            ("symbol", (v.get)(&market, "id")?),
            (
                "type",
                (v.literal)(
                    if (v.equal)(&(v.get)(&opts, "type")?, &(v.capture)("OrderType.Market")?) {
                        "market"
                    } else {
                        "limit"
                    },
                ),
            ),
            (
                "side",
                (v.literal)(
                    if (v.equal)(&(v.get)(&opts, "side")?, &(v.capture)("OrderSide.Sell")?) {
                        "sell"
                    } else {
                        "buy"
                    },
                ),
            ),
            (
                "price",
                if !(v.equal)(&price, &(v.undefined)()) {
                    intrinsic(&h, "template-string", vec![price])?
                } else {
                    (v.undefined)()
                },
            ),
            ("size", intrinsic(&h, "template-string", vec![amount])?),
            (
                "timeInForce",
                if (v.equal)(&(v.get)(&opts, "type")?, &(v.capture)("OrderType.Limit")?) {
                    (v.literal)(tif)
                } else {
                    (v.undefined)()
                },
            ),
            (
                "postOnly",
                if (v.equal)(
                    &(v.get)(&opts, "timeInForce")?,
                    &(v.capture)("OrderTimeInForce.PostOnly")?,
                ) {
                    (h.base.boolean)(true)
                } else {
                    (v.undefined)()
                },
            ),
        ])])
    })?;
    let result = async {
        let promise = call(&h, "this.unlimitedXHR.post", || {
            Ok(vec![
                (v.capture)("KUCOIN_SPOT_ENDPOINTS.PLACE_ORDER")?,
                payload,
            ])
        })?;
        let data = (v.get)(&(v.await_value)(promise).await?, "data")?;
        if bad_code(&h, &data)? {
            return Err(error(&h, || {
                fail_message(&h, &data, "Failed to place KuCoin Spot order")
            })?);
        }
        let nested = (v.get)(&data, "data")?;
        Ok((v.array)(
            if (v.truthy)(&optional(&h, &nested, "orderId")?) {
                vec![(v.get)(&(v.get)(&data, "data")?, "orderId")?]
            } else {
                vec![]
            },
        ))
    }
    .await;
    match result {
        Ok(value) => Ok(value),
        Err(e) => {
            emit(&h, || message(&h, &e))?;
            Err(e)
        }
    }
}
pub async fn update_order<V: Clone + 'static, E: 'static>(
    h: Rc<SpotBodyOps<V, E>>,
    opts: V,
) -> Result<V, E> {
    let v = &h.base.value;
    let order = (v.get)(&opts, "order")?;
    let update = (v.get)(&opts, "update")?;
    let market = market(&h, &order)?;
    if !(v.truthy)(&market) {
        return Err(missing_market(&h, &order)?);
    }
    let payload = call(&h, "omitUndefined", || {
        Ok(vec![(v.object)(vec![
            ("symbol", (v.get)(&market, "id")?),
            ("orderId", (v.get)(&order, "id")?),
            (
                "newPrice",
                if (h.has)("price", &update)?
                    && !(v.equal)(&(v.get)(&update, "price")?, &(v.undefined)())
                {
                    intrinsic(
                        &h,
                        "template-string",
                        vec![adjusted(&h, &update, &market, "price", "price")?],
                    )?
                } else {
                    (v.undefined)()
                },
            ),
            (
                "newSize",
                if (h.has)("amount", &update)?
                    && !(v.equal)(&(v.get)(&update, "amount")?, &(v.undefined)())
                {
                    intrinsic(
                        &h,
                        "template-string",
                        vec![adjusted(&h, &update, &market, "amount", "amount")?],
                    )?
                } else {
                    (v.undefined)()
                },
            ),
        ])])
    })?;
    let result = async {
        let promise = call(&h, "this.unlimitedXHR.post", || {
            Ok(vec![
                (v.capture)("KUCOIN_SPOT_ENDPOINTS.MODIFY_ORDER")?,
                payload,
            ])
        })?;
        let data = (v.get)(&(v.await_value)(promise).await?, "data")?;
        if bad_code(&h, &data)? {
            return Err(error(&h, || {
                fallback(&h, (v.get)(&data, "msg")?, || {
                    intrinsic(
                        &h,
                        "template-update-spot-order",
                        vec![(v.get)(&order, "id")?],
                    )
                })
            })?);
        }
        let nested = (v.get)(&data, "data")?;
        Ok((v.array)(vec![
            if (v.truthy)(&optional(&h, &nested, "newOrderId")?) {
                (v.get)(&(v.get)(&data, "data")?, "newOrderId")?
            } else {
                (v.get)(&order, "id")?
            },
        ]))
    }
    .await;
    match result {
        Ok(value) => Ok(value),
        Err(e) => {
            emit(&h, || message(&h, &e))?;
            Err(e)
        }
    }
}
pub async fn fetch_historical_fills<V: Clone + 'static, E: 'static>(
    h: Rc<SpotBodyOps<V, E>>,
    opts: V,
) -> Result<V, E> {
    let v = &h.base.value;
    let symbol = (v.get)(&opts, "symbol")?;
    let order_id = (v.get)(&opts, "orderId")?;
    let from = (v.get)(&opts, "from")?;
    let to = (v.get)(&opts, "to")?;
    let limit = (v.get)(&opts, "limit")?;
    if !(v.truthy)(&symbol) {
        return Ok((v.array)(vec![]));
    }
    let result = async {
        let promise = call(&h, "this.xhr.get", || {
            Ok(vec![
                (v.capture)("KUCOIN_SPOT_ENDPOINTS.FILLS")?,
                (v.object)(vec![(
                    "params",
                    call(&h, "omitUndefined", || {
                        Ok(vec![(v.object)(vec![
                            ("symbol", symbol),
                            ("orderId", order_id),
                            (
                                "limit",
                                if (v.truthy)(&limit) {
                                    call(&h, "Math.min", || Ok(vec![limit, (v.number)(100.)]))?
                                } else {
                                    (v.number)(100.)
                                },
                            ),
                            ("startAt", from),
                            ("endAt", to),
                        ])])
                    })?,
                )]),
            ])
        })?;
        let data = (v.get)(&(v.await_value)(promise).await?, "data")?;
        if bad_code(&h, &data)? {
            emit(&h, || {
                fail_message(&h, &data, "Failed to fetch KuCoin Spot executions")
            })?;
            return Ok((v.array)(vec![]));
        }
        let rows = fallback(&h, optional(&h, &(v.get)(&data, "data")?, "items")?, || {
            Ok((v.array)(vec![]))
        })?;
        let ops = h.clone();
        member(&h, rows, "map", || {
            Ok(vec![(h.base.mapper)(Rc::new(move |fill| {
                map_fill(&ops, fill)
            }))])
        })
    }
    .await;
    match result {
        Ok(value) => Ok(value),
        Err(e) => {
            emit(&h, || message(&h, &e))?;
            Ok((v.array)(vec![]))
        }
    }
}
fn map_fill<V, E>(h: &SpotBodyOps<V, E>, fill: V) -> Result<V, E> {
    let v = &h.base.value;
    Ok((v.object)(vec![
        (
            "timestamp",
            call(h, "parseInt", || {
                Ok(vec![
                    fallback(h, (v.get)(&fill, "createdAt")?, || {
                        intrinsic(
                            h,
                            "template-string",
                            vec![call(h, "Date.now", || Ok(vec![]))?],
                        )
                    })?,
                    (v.number)(10.),
                ])
            })?,
        ),
        ("symbol", (v.get)(&fill, "symbol")?),
        ("orderId", (v.get)(&fill, "orderId")?),
        ("price", field_float(h, &fill, "price")?),
        ("qty", field_float(h, &fill, "size")?),
        (
            "side",
            (v.capture)(
                if (v.equal)(&(v.get)(&fill, "side")?, &(v.literal)("sell")) {
                    "OrderSide.Sell"
                } else {
                    "OrderSide.Buy"
                },
            )?,
        ),
        ("commission", field_float(h, &fill, "fee")?),
        ("commissionAsset", (v.get)(&fill, "feeCurrency")?),
    ]))
}
pub async fn fetch_ohlcv<V: Clone + 'static, E: 'static>(
    h: Rc<SpotBodyOps<V, E>>,
    opts: V,
) -> Result<V, E> {
    let v = &h.base.value;
    let limit = call(&h, "Math.min", || {
        Ok(vec![
            fallback(&h, (v.get)(&opts, "limit")?, || Ok((v.number)(500.)))?,
            (v.number)(1500.),
        ])
    })?;
    let interval = intrinsic(
        &h,
        "get-index",
        vec![
            (v.capture)("KUCOIN_SPOT_INTERVAL")?,
            (v.get)(&opts, "interval")?,
        ],
    )?;
    let (amount, unit) = (h.base.interval_parts)((v.get)(&opts, "interval")?)?;
    let has_to = (v.truthy)(&(v.get)(&opts, "to")?);
    let end = call(&h, "dayjs", || {
        Ok(if has_to {
            vec![(v.get)(&opts, "to")?]
        } else {
            vec![]
        })
    })?;
    let start = if !(v.truthy)(&(v.get)(&opts, "limit")?) && (v.truthy)(&(v.get)(&opts, "from")?) {
        call(&h, "dayjs", || Ok(vec![(v.get)(&opts, "from")?]))?
    } else {
        member(&h, end.clone(), "subtract", || {
            Ok(vec![
                intrinsic(
                    &h,
                    "multiply",
                    vec![call(&h, "parseFloat", || Ok(vec![amount]))?, limit],
                )?,
                unit,
            ])
        })?
    };
    let promise = call(&h, "this.xhr.get", || {
        Ok(vec![
            (v.capture)("KUCOIN_SPOT_ENDPOINTS.KLINE")?,
            (v.object)(vec![(
                "params",
                (v.object)(vec![
                    ("symbol", (v.get)(&opts, "symbol")?),
                    ("type", interval),
                    (
                        "startAt",
                        call(&h, "Math.floor", || {
                            Ok(vec![intrinsic(
                                &h,
                                "divide",
                                vec![
                                    member(&h, start, "valueOf", || Ok(vec![]))?,
                                    (v.number)(1000.),
                                ],
                            )?])
                        })?,
                    ),
                    (
                        "endAt",
                        call(&h, "Math.floor", || {
                            Ok(vec![intrinsic(
                                &h,
                                "divide",
                                vec![
                                    member(&h, end, "valueOf", || Ok(vec![]))?,
                                    (v.number)(1000.),
                                ],
                            )?])
                        })?,
                    ),
                ]),
            )]),
        ])
    })?;
    let data = (v.get)(&(v.await_value)(promise).await?, "data")?;
    if bad_code(&h, &data)? {
        emit(&h, || {
            fail_message(&h, &data, "Failed to fetch KuCoin Spot candles")
        })?;
        return Ok((v.array)(vec![]));
    }
    let ops = h.clone();
    let mapped = member(&h, (v.get)(&data, "data")?, "map", || {
        Ok(vec![(h.base.mapper)(Rc::new(move |c| map_candle(&ops, c)))])
    })?;
    let ops = h.clone();
    member(&h, mapped, "sort", || {
        Ok(vec![(h.comparator)(Rc::new(move |a, b| {
            intrinsic(
                &ops,
                "subtract",
                vec![
                    (ops.base.value.get)(&a, "timestamp")?,
                    (ops.base.value.get)(&b, "timestamp")?,
                ],
            )
        }))])
    })
}
fn map_candle<V, E>(h: &SpotBodyOps<V, E>, candle: V) -> Result<V, E> {
    let v = &h.base.value;
    let timestamp = call(h, "parseInt", || {
        Ok(vec![(v.get)(&candle, "0")?, (v.number)(10.)])
    })?;
    let mut fields = vec![("timestamp", timestamp)];
    for (key, index) in [
        ("open", "1"),
        ("close", "2"),
        ("high", "3"),
        ("low", "4"),
        ("volume", "5"),
    ] {
        fields.push((
            key,
            call(h, "parseFloat", || Ok(vec![(v.get)(&candle, index)?]))?,
        ));
    }
    Ok((v.object)(fields))
}
