use crate::{
    public_streams::StreamOps,
    spot_bodies::SpotBodyOps,
    spot_private_class::{PrivateClassOps, call, fallback, optional},
};
use std::rc::Rc;

fn index<V, E>(h: &StreamOps<V, E>, object: V, key: V) -> Result<V, E> {
    (h.intrinsic)("get-index", vec![object, key])
}
fn parsed<V, E>(
    h: &StreamOps<V, E>,
    name: &str,
    value: impl FnOnce() -> Result<V, E>,
) -> Result<V, E> {
    call(h, name, || Ok(vec![value()?]))
}
fn field_float<V, E>(h: &StreamOps<V, E>, row: &V, key: &str, default: &str) -> Result<V, E> {
    parsed(h, "parseFloat", || {
        fallback(h, (h.get)(row, key)?, || Ok((h.literal)(default)))
    })
}
fn bad_code<V, E>(h: &StreamOps<V, E>, data: &V) -> Result<bool, E> {
    Ok(!(h.equal)(&(h.get)(data, "code")?, &(h.literal)("200000")))
}
fn caught_message<V, E>(h: &StreamOps<V, E>, error: &E) -> Result<V, E> {
    let value = (h.error_value)(error);
    let response = optional(h, &value, "response")?;
    let data = optional(h, &response, "data")?;
    fallback(h, optional(h, &data, "msg")?, || {
        optional(h, &value, "message")
    })
}
fn emit<V, E>(h: &StreamOps<V, E>, message: impl FnOnce() -> Result<V, E>) -> Result<(), E> {
    call(h, "this.emitAndLogError", || Ok(vec![message()?]))?;
    Ok(())
}
async fn request<V, E>(
    h: &StreamOps<V, E>,
    method: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let response = call(h, method, args)?;
    let response = (h.await_value)(response).await?;
    (h.intrinsic)("destructure-data", vec![response])
}
fn endpoint<V, E>(h: &StreamOps<V, E>, key: &str) -> Result<V, E> {
    let table = (h.capture)("KUCOIN_SPOT_ENDPOINTS")?;
    (h.get)(&table, key)
}
fn market<V, E>(h: &StreamOps<V, E>, symbol: impl FnOnce() -> Result<V, E>) -> Result<V, E> {
    let table = (h.capture)("this.store.marketsBySymbol")?;
    index(h, table, symbol()?)
}
fn template<V, E>(ops: &PrivateClassOps<V, E>, prefix: &str, value: &V) -> Result<V, E> {
    let mut units = prefix.encode_utf16().collect::<Vec<_>>();
    units.extend((ops.template)(value)?);
    Ok((ops.string_value)(units))
}
pub async fn fetch_active_order_symbols<V: Clone + 'static, E: 'static>(
    h: Rc<StreamOps<V, E>>,
) -> Result<V, E> {
    let result: Result<V, E> = async {
        let data = request(&h, "this.xhr.get", || {
            Ok(vec![endpoint(&h, "ACTIVE_ORDER_SYMBOLS")?])
        })
        .await?;
        if bad_code(&h, &data)? {
            return Ok((h.array)(vec![]));
        }
        let array = call(&h, "Array.isArray", || {
            let inner = (h.get)(&data, "data")?;
            Ok(vec![optional(&h, &inner, "symbols")?])
        })?;
        if (h.truthy)(&array) {
            let inner = (h.get)(&data, "data")?;
            (h.get)(&inner, "symbols")
        } else {
            Ok((h.array)(vec![]))
        }
    }
    .await;
    match result {
        Ok(value) => Ok(value),
        Err(_) => Ok((h.array)(vec![])),
    }
}
pub fn map_order<V: Clone, E>(
    ops: &PrivateClassOps<V, E>,
    h: &StreamOps<V, E>,
    order: V,
) -> Result<V, E> {
    let size = parsed(h, "parseFloat", || {
        fallback(h, (h.get)(&order, "size")?, || {
            fallback(h, (h.get)(&order, "originSize")?, || Ok((h.literal)("0")))
        })
    })?;
    let filled = parsed(h, "parseFloat", || {
        fallback(h, (h.get)(&order, "dealSize")?, || {
            fallback(h, (h.get)(&order, "filledSize")?, || Ok((h.literal)("0")))
        })
    })?;
    let remaining = parsed(h, "parseFloat", || {
        let value = (h.get)(&order, "remainSize")?;
        if !(h.nullish)(&value) {
            return Ok(value);
        }
        let remaining = call(h, "Math.max", || {
            Ok(vec![
                (h.number)(0.),
                (h.intrinsic)("subtract", vec![size.clone(), filled.clone()])?,
            ])
        })?;
        Ok((ops.string_value)((ops.template)(&remaining)?))
    })?;
    let time_in_force = if (h.equal)(&(h.get)(&order, "postOnly")?, &(ops.boolean)(true)) {
        (h.capture)("OrderTimeInForce.PostOnly")?
    } else {
        let table = (h.capture)("KUCOIN_SPOT_TIME_IN_FORCE")?;
        fallback(h, index(h, table, (h.get)(&order, "timeInForce")?)?, || {
            (h.capture)("OrderTimeInForce.GoodTillCancel")
        })?
    };
    let id = fallback(h, (h.get)(&order, "id")?, || (h.get)(&order, "orderId"))?;
    let status = {
        let table = (h.capture)("KUCOIN_SPOT_ORDER_STATUS")?;
        fallback(h, index(h, table, (h.get)(&order, "status")?)?, || {
            let table = (h.capture)("KUCOIN_SPOT_ORDER_STATUS")?;
            let active = (h.truthy)(&(h.get)(&order, "active")?);
            index(
                h,
                table,
                (h.literal)(if active { "active" } else { "done" }),
            )
        })?
    };
    let symbol = (h.get)(&order, "symbol")?;
    let order_type = {
        let table = (h.capture)("KUCOIN_SPOT_ORDER_TYPE")?;
        let key = fallback(h, (h.get)(&order, "type")?, || (h.get)(&order, "orderType"))?;
        fallback(h, index(h, table, key)?, || {
            let table = (h.capture)("KUCOIN_SPOT_ORDER_TYPE")?;
            (h.get)(&table, "limit")
        })?
    };
    let side = if (h.equal)(&(h.get)(&order, "side")?, &(h.literal)("sell")) {
        (h.capture)("OrderSide.Sell")?
    } else {
        (h.capture)("OrderSide.Buy")?
    };
    let price = parsed(h, "parseFloat", || {
        fallback(h, (h.get)(&order, "price")?, || {
            fallback(h, (h.get)(&order, "matchPrice")?, || Ok((h.literal)("0")))
        })
    })?;
    let last_update = call(h, "parseInt", || {
        let input = fallback(h, (h.get)(&order, "lastUpdatedAt")?, || {
            fallback(h, (h.get)(&order, "orderTime")?, || {
                fallback(h, (h.get)(&order, "createdAt")?, || {
                    let now = call(h, "Date.now", || Ok(vec![]))?;
                    Ok((ops.string_value)((ops.template)(&now)?))
                })
            })
        })?;
        Ok(vec![input, (h.number)(10.)])
    })?;
    Ok((h.object)(vec![
        ("id", id),
        ("status", status),
        ("symbol", symbol),
        ("type", order_type),
        ("side", side),
        ("price", price),
        ("amount", size),
        ("filled", filled),
        ("remaining", remaining),
        ("reduceOnly", (ops.boolean)(false)),
        ("timeInForce", time_in_force),
        ("lastUpdate", last_update),
        ("info", order),
    ]))
}
pub async fn fetch_markets<V: Clone + 'static, E: 'static>(
    ops: Rc<PrivateClassOps<V, E>>,
    h: Rc<StreamOps<V, E>>,
) -> Result<V, E> {
    let result = async {
        let data = request(&h, "this.xhr.get", || Ok(vec![endpoint(&h, "MARKETS")?])).await?;
        if bad_code(&h, &data)? {
            emit(&h, || {
                fallback(&h, (h.get)(&data, "msg")?, || {
                    Ok((h.literal)("Failed to fetch KuCoin Spot markets"))
                })
            })?;
            return (h.capture)("this.store.markets");
        }
        let rows = (h.get)(&data, "data")?;
        let map = (h.get)(&rows, "map")?;
        let values = h.clone();
        let metadata = ops.clone();
        let mapper = (ops.arrow)(
            "",
            1,
            Rc::new(move |args| {
                let row = args
                    .first()
                    .cloned()
                    .unwrap_or_else(|| (values.undefined)());
                let id = (values.get)(&row, "symbol")?;
                let symbol = (values.get)(&row, "symbol")?;
                let mut pretty = (metadata.template)(&(values.get)(&row, "baseCurrency")?)?;
                pretty.extend((metadata.template)(&(values.get)(&row, "quoteCurrency")?)?);
                let base = (values.get)(&row, "baseCurrency")?;
                let quote = (values.get)(&row, "quoteCurrency")?;
                let active = (values.equal)(
                    &(values.get)(&row, "enableTrading")?,
                    &(metadata.boolean)(true),
                );
                let amount = field_float(&values, &row, "baseIncrement", "1")?;
                let price = field_float(&values, &row, "priceIncrement", "1")?;
                let min = field_float(&values, &row, "baseMinSize", "0")?;
                let max = parsed(&values, "parseFloat", || {
                    fallback(&values, (values.get)(&row, "baseMaxSize")?, || {
                        let max = (values.capture)("Number.MAX_SAFE_INTEGER")?;
                        Ok((metadata.string_value)((metadata.template)(&max)?))
                    })
                })?;
                Ok((values.object)(vec![
                    ("id", id),
                    ("symbol", symbol),
                    ("prettySymbol", (metadata.string_value)(pretty)),
                    ("base", base),
                    ("quote", quote),
                    ("active", (metadata.boolean)(active)),
                    (
                        "precision",
                        (values.object)(vec![("amount", amount), ("price", price)]),
                    ),
                    (
                        "limits",
                        (values.object)(vec![
                            ("amount", (values.object)(vec![("min", min), ("max", max)])),
                            (
                                "leverage",
                                (values.object)(vec![
                                    ("min", (values.number)(1.)),
                                    ("max", (values.number)(1.)),
                                ]),
                            ),
                        ]),
                    ),
                    ("contractSize", (values.number)(1.)),
                    ("isSpot", (metadata.boolean)(true)),
                    ("isInverse", (metadata.boolean)(false)),
                    ("info", row),
                ]))
            }),
        );
        (h.intrinsic)("call-with-this", vec![map, rows, (h.array)(vec![mapper])])
    }
    .await;
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            emit(&h, || caught_message(&h, &error))?;
            (h.capture)("this.store.markets")
        }
    }
}
pub async fn fetch_tickers<V: Clone + 'static, E: 'static>(
    ops: Rc<PrivateClassOps<V, E>>,
    h: Rc<StreamOps<V, E>>,
) -> Result<V, E> {
    let result = async {
        let data = request(&h, "this.xhr.get", || Ok(vec![endpoint(&h, "TICKERS")?])).await?;
        if bad_code(&h, &data)? {
            emit(&h, || {
                fallback(&h, (h.get)(&data, "msg")?, || {
                    Ok((h.literal)("Failed to fetch KuCoin Spot tickers"))
                })
            })?;
            return (h.capture)("this.store.tickers");
        }
        let inner = (h.get)(&data, "data")?;
        let rows = fallback(&h, optional(&h, &inner, "ticker")?, || {
            Ok((h.array)(vec![]))
        })?;
        let reduce = (h.get)(&rows, "reduce")?;
        let values = h.clone();
        let reducer = (ops.arrow)(
            "",
            2,
            Rc::new(move |args| {
                let tickers = args
                    .first()
                    .cloned()
                    .unwrap_or_else(|| (values.undefined)());
                let row = args.get(1).cloned().unwrap_or_else(|| (values.undefined)());
                let market = market(&values, || (values.get)(&row, "symbol"))?;
                if !(values.truthy)(&market) {
                    return Ok(tickers);
                }
                let last = field_float(&values, &row, "last", "0")?;
                let push = (values.get)(&tickers, "push")?;
                let id = (values.get)(&market, "id")?;
                let symbol = (values.get)(&market, "symbol")?;
                let bid = field_float(&values, &row, "buy", "0")?;
                let ask = field_float(&values, &row, "sell", "0")?;
                let percentage = (values.intrinsic)(
                    "multiply",
                    vec![
                        field_float(&values, &row, "changeRate", "0")?,
                        (values.number)(100.),
                    ],
                )?;
                let volume = field_float(&values, &row, "vol", "0")?;
                let quote_volume = field_float(&values, &row, "volValue", "0")?;
                let ticker = (values.object)(vec![
                    ("id", id),
                    ("symbol", symbol),
                    ("bid", bid),
                    ("ask", ask),
                    ("last", last.clone()),
                    ("mark", last.clone()),
                    ("index", last),
                    ("percentage", percentage),
                    ("fundingRate", (values.number)(0.)),
                    ("fundingTime", (values.number)(0.)),
                    ("openInterest", (values.number)(0.)),
                    ("volume", volume),
                    ("quoteVolume", quote_volume),
                    ("info", row),
                ]);
                (values.intrinsic)(
                    "call-with-this",
                    vec![push, tickers.clone(), (values.array)(vec![ticker])],
                )?;
                Ok(tickers)
            }),
        );
        (h.intrinsic)(
            "call-with-this",
            vec![reduce, rows, (h.array)(vec![reducer, (h.array)(vec![])])],
        )
    }
    .await;
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            emit(&h, || caught_message(&h, &error))?;
            (h.capture)("this.store.tickers")
        }
    }
}
fn new_error<V, E>(h: &SpotBodyOps<V, E>, message: impl FnOnce() -> Result<V, E>) -> Result<E, E> {
    let constructor = (h.base.value.capture)("Error")?;
    (h.new_error)(constructor, message()?)
}
pub async fn cancel_orders<V: Clone + 'static, E: 'static>(
    ops: Rc<PrivateClassOps<V, E>>,
    h: Rc<SpotBodyOps<V, E>>,
    orders: V,
) -> Result<V, E> {
    let v = &h.base.value;
    let iterator = (h.base.iterator)(orders)?;
    while let Some(order) = (h.base.next)(&iterator)? {
        let result = async {
            let market = market(v, || (v.get)(&order, "symbol"))?;
            let symbol = fallback(v, optional(v, &market, "id")?, || (v.get)(&order, "symbol"))?;
            let response = async {
                let data = request(v, "this.unlimitedXHR.delete", || {
                    let mut route = (ops.template)(&endpoint(v, "CANCEL_ORDER")?)?;
                    route.push('/' as u16);
                    route.extend((ops.template)(&(v.get)(&order, "id")?)?);
                    Ok(vec![
                        (ops.string_value)(route),
                        (v.object)(vec![("params", (v.object)(vec![("symbol", symbol)]))]),
                    ])
                })
                .await?;
                if bad_code(v, &data)? {
                    return Err(new_error(&h, || {
                        fallback(v, (v.get)(&data, "msg")?, || {
                            template(
                                &ops,
                                "Failed to cancel KuCoin Spot order ",
                                &(v.get)(&order, "id")?,
                            )
                        })
                    })?);
                }
                Ok((v.undefined)())
            }
            .await;
            match response {
                Ok(value) => Ok(value),
                Err(error) => {
                    emit(v, || caught_message(v, &error))?;
                    Err(error)
                }
            }
        }
        .await;
        if let Err(error) = result {
            return Err((h.base.close_throw)(&iterator, error));
        }
    }
    Ok((v.undefined)())
}
pub async fn cancel_symbol_orders<V: Clone + 'static, E: 'static>(
    ops: Rc<PrivateClassOps<V, E>>,
    h: Rc<SpotBodyOps<V, E>>,
    symbol: V,
) -> Result<V, E> {
    let v = &h.base.value;
    let market = market(v, || Ok(symbol.clone()))?;
    let api_symbol = fallback(v, optional(v, &market, "id")?, || Ok(symbol.clone()))?;
    let result = async {
        let data = request(v, "this.unlimitedXHR.delete", || {
            Ok(vec![
                endpoint(v, "CANCEL_SYMBOL_ORDERS")?,
                (v.object)(vec![("params", (v.object)(vec![("symbol", api_symbol)]))]),
            ])
        })
        .await?;
        if bad_code(v, &data)? {
            return Err(new_error(&h, || {
                fallback(v, (v.get)(&data, "msg")?, || {
                    template(&ops, "Failed to cancel KuCoin Spot orders for ", &symbol)
                })
            })?);
        }
        Ok((v.undefined)())
    }
    .await;
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            emit(v, || caught_message(v, &error))?;
            Err(error)
        }
    }
}
pub async fn cancel_all_orders<V: Clone + 'static, E: 'static>(
    h: Rc<SpotBodyOps<V, E>>,
) -> Result<V, E> {
    let v = &h.base.value;
    let result = async {
        let data = request(v, "this.unlimitedXHR.delete", || {
            Ok(vec![endpoint(v, "CANCEL_ALL_ORDERS")?])
        })
        .await?;
        if bad_code(v, &data)? {
            return Err(new_error(&h, || {
                fallback(v, (v.get)(&data, "msg")?, || {
                    Ok((v.literal)("Failed to cancel all KuCoin Spot orders"))
                })
            })?);
        }
        Ok((v.undefined)())
    }
    .await;
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            emit(v, || caught_message(v, &error))?;
            Err(error)
        }
    }
}
pub fn listen_ohlcv<V, E>(h: &StreamOps<V, E>, opts: V, callback: V) -> Result<V, E> {
    let external = (h.capture)("this.externalPublicDataProvider")?;
    call(
        h,
        if (h.truthy)(&external) {
            "this.externalPublicDataProvider.subscribeCandle"
        } else {
            "this.publicWebsocket.listenOHLCV"
        },
        || Ok(vec![opts, callback]),
    )
}
pub fn listen_order_book<V, E>(h: &StreamOps<V, E>, symbol: V, callback: V) -> Result<V, E> {
    let external = (h.capture)("this.externalPublicDataProvider")?;
    call(
        h,
        if (h.truthy)(&external) {
            "this.externalPublicDataProvider.subscribeOrderbook"
        } else {
            "this.publicWebsocket.listenOrderBook"
        },
        || Ok(vec![symbol, callback]),
    )
}
