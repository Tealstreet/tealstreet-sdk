use crate::public_streams::StreamOps;
use std::{future::Future, pin::Pin, rc::Rc};
pub type BootstrapAwait<V, E> = Pin<Box<dyn Future<Output = Result<V, E>>>>;
pub struct BootstrapOps<V, E> {
    pub value: Rc<StreamOps<V, E>>,
    pub member: Rc<dyn Fn(V, &str) -> Result<V, E>>,
    pub raw_filter: Rc<dyn Fn(V, Rc<dyn Fn(V) -> Result<V, E>>) -> Result<V, E>>,
    pub boolean: Rc<dyn Fn(bool) -> V>,
    pub mapper: Rc<dyn Fn(Rc<dyn Fn(V) -> Result<V, E>>) -> V>,
    pub async_mapper: Rc<dyn Fn(Rc<dyn Fn(V) -> BootstrapAwait<V, E>>) -> V>,
    pub callback: Rc<dyn Fn(Rc<dyn Fn() -> Result<V, E>>) -> V>,
    pub response_pair: Rc<dyn Fn(V) -> Result<(V, V), E>>,
    pub interval_parts: Rc<dyn Fn(V) -> Result<(V, V), E>>,
    pub iterator: Rc<dyn Fn(V) -> Result<V, E>>,
    pub next: Rc<dyn Fn(&V) -> Result<Option<V>, E>>,
    pub close_throw: Rc<dyn Fn(&V, E) -> E>,
    pub some: Rc<dyn Fn(V, Rc<dyn Fn(V) -> Result<bool, E>>) -> Result<bool, E>>,
}
fn call<V, E>(
    h: &BootstrapOps<V, E>,
    name: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let callee = (h.value.capture)(name)?;
    (h.value.invoke)(callee, args()?)
}
fn optional<V, E>(h: &BootstrapOps<V, E>, value: &V, key: &str) -> Result<V, E> {
    if (h.value.nullish)(value) {
        Ok((h.value.undefined)())
    } else {
        (h.value.get)(value, key)
    }
}
fn fallback<V, E>(
    h: &BootstrapOps<V, E>,
    value: V,
    next: impl FnOnce() -> Result<V, E>,
) -> Result<V, E> {
    if (h.value.truthy)(&value) {
        Ok(value)
    } else {
        next()
    }
}
fn message<V, E>(h: &BootstrapOps<V, E>, error: &E) -> Result<V, E> {
    let value = (h.value.error_value)(error);
    let response = optional(h, &value, "response")?;
    let data = optional(h, &response, "data")?;
    fallback(h, optional(h, &data, "msg")?, || {
        optional(h, &value, "message")
    })
}
fn emit<V, E>(
    h: &BootstrapOps<V, E>,
    error: Option<&E>,
    msg: impl FnOnce() -> Result<V, E>,
) -> Result<(), E> {
    call(h, "this.emitAndLogError", || {
        let mut args = Vec::new();
        if let Some(error) = error {
            args.push((h.value.error_value)(error));
        }
        args.push(msg()?);
        Ok(args)
    })?;
    Ok(())
}
fn index<V, E>(h: &BootstrapOps<V, E>, object: V, key: V) -> Result<V, E> {
    (h.value.intrinsic)("get-index", vec![object, key])
}
fn define<V: Clone, E>(h: &BootstrapOps<V, E>, object: &V, key: &str, value: V) -> Result<(), E> {
    (h.value.intrinsic)(
        "define-field",
        vec![object.clone(), (h.value.literal)(key), value],
    )?;
    Ok(())
}
fn spread<V: Clone, E>(h: &BootstrapOps<V, E>, target: &V, source: V) -> Result<(), E> {
    (h.value.intrinsic)("spread-into", vec![target.clone(), source])?;
    Ok(())
}
fn parsed<V, E>(
    h: &BootstrapOps<V, E>,
    name: &str,
    read: impl FnOnce() -> Result<V, E>,
) -> Result<V, E> {
    call(h, name, || Ok(vec![read()?]))
}
fn row_float<V, E>(h: &BootstrapOps<V, E>, row: &V, key: &str, default: &str) -> Result<V, E> {
    parsed(h, "parseFloat", || {
        fallback(h, (h.value.get)(row, key)?, || {
            Ok((h.value.literal)(default))
        })
    })
}
fn bad_code<V, E>(h: &BootstrapOps<V, E>, data: &V) -> Result<bool, E> {
    Ok(!(h.value.equal)(
        &(h.value.get)(data, "code")?,
        &(h.value.literal)("200000"),
    ))
}
fn loaded<V: Clone, E>(h: &BootstrapOps<V, E>, fields: &[&str]) -> Result<V, E> {
    let object = (h.value.object)(vec![]);
    spread(h, &object, (h.value.capture)("this.store.loaded")?)?;
    for key in fields {
        define(h, &object, key, (h.boolean)(true))?;
    }
    Ok(object)
}
async fn request<V: Clone, E>(h: &BootstrapOps<V, E>, endpoint: &str) -> Result<V, E> {
    let value = call(h, "this.xhr.get", || Ok(vec![(h.value.capture)(endpoint)?]))?;
    let response = (h.value.await_value)(value).await?;
    (h.value.get)(&response, "data")
}
pub async fn start<V: Clone + 'static, E: 'static>(h: Rc<BootstrapOps<V, E>>) -> Result<V, E> {
    let result = async {
        let value = call(&h, "this.initExternalPublicData", || Ok(vec![]))?;
        let external = (h.value.await_value)(value).await?;
        if !(h.value.truthy)(&external) {
            for (fetch, key) in [
                ("this.fetchMarkets", "markets"),
                ("this.fetchTickers", "tickers"),
            ] {
                let value = call(&h, fetch, || Ok(vec![]))?;
                let values = (h.value.await_value)(value).await?;
                if (h.value.truthy)(&(h.value.capture)("this.isDisposed")?) {
                    return Ok((h.value.undefined)());
                }
                call(&h, "this.store.update", || {
                    Ok(vec![(h.value.object)(vec![
                        (key, values.clone()),
                        ("loaded", loaded(&h, &[key])?),
                    ])])
                })?;
                call(&h, "this.log", || {
                    Ok(vec![(h.value.intrinsic)(
                        "loaded-log",
                        vec![(h.value.get)(&values, "length")?, (h.value.literal)(key)],
                    )?])
                })?;
            }
            call(&h, "this.publicWebsocket.connectAndSubscribe", || {
                Ok(vec![])
            })?;
            let sockets = call(&h, "createKucoinSnapshotWebsockets", || {
                let receiver = (h.value.capture)("this")?;
                let markets = (h.value.capture)("this.store.markets")?;
                Ok(vec![receiver, (h.value.get)(&markets, "length")?])
            })?;
            (h.value.set)(&(h.value.capture)("this")?, "snapshotWebsockets", sockets)?;
            let ops = h.clone();
            call(&h, "this.snapshotWebsockets.forEach", || {
                Ok(vec![(h.mapper)(Rc::new(move |ws| {
                    let callee = (ops.member)(ws, "connectAndSubscribe")?;
                    (ops.value.invoke)(callee, vec![])
                }))])
            })?;
        }
        let public_only = (h.value.capture)("this.isPublicDataOnly")?;
        if (h.value.truthy)(&public_only) {
            call(&h, "this.store.update", || {
                Ok(vec![(h.value.object)(vec![(
                    "loaded",
                    loaded(&h, &["balance", "positions", "orders"])?,
                )])])
            })?;
            return Ok((h.value.undefined)());
        }
        call(&h, "this.privateWebsocket.connectAndSubscribe", || {
            Ok(vec![])
        })?;
        let value = call(&h, "this.tick", || Ok(vec![]))?;
        (h.value.await_value)(value).await?;
        if (h.value.truthy)(&(h.value.capture)("this.isDisposed")?) {
            return Ok((h.value.undefined)());
        }
        let value = call(&h, "this.fetchOrders", || Ok(vec![]))?;
        let orders = (h.value.await_value)(value).await?;
        if (h.value.truthy)(&(h.value.capture)("this.isDisposed")?) {
            return Ok((h.value.undefined)());
        }
        call(&h, "this.store.update", || {
            Ok(vec![(h.value.object)(vec![
                ("orders", orders.clone()),
                ("loaded", loaded(&h, &["orders"])?),
            ])])
        })?;
        call(&h, "this.log", || {
            Ok(vec![(h.value.intrinsic)(
                "loaded-log",
                vec![
                    (h.value.get)(&orders, "length")?,
                    (h.value.literal)("orders"),
                ],
            )?])
        })?;
        call(&h, "this.log", || {
            Ok(vec![(h.value.literal)("Ready to trade on KuCoin Futures")])
        })?;
        Ok((h.value.undefined)())
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            emit(&h, None, || {
                fallback(&h, message(&h, &e)?, || {
                    Ok((h.value.literal)("Failed to start KuCoin exchange"))
                })
            })?;
            Ok((h.value.undefined)())
        }
    }
}
pub async fn tick<V: Clone + 'static, E: 'static>(
    h: Rc<BootstrapOps<V, E>>,
    count: V,
) -> Result<V, E> {
    let count = if (h.value.equal)(&count, &(h.value.undefined)()) {
        (h.value.number)(0.)
    } else {
        count
    };
    if !(h.value.truthy)(&(h.value.capture)("this.isDisposed")?) {
        if (h.value.truthy)(&(h.value.capture)("this.isPublicDataOnly")?) {
            return Ok((h.value.undefined)());
        }
        let result = async {
            let value = call(&h, "this.fetchPositions", || Ok(vec![]))?;
            let positions = (h.value.await_value)(value).await?;
            if (h.value.truthy)(&(h.value.capture)("this.isDisposed")?) {
                return Ok(true);
            }
            call(&h, "this.store.addOrUpdatePositionsFull", || {
                Ok(vec![positions])
            })?;
            let mut balance = (h.value.undefined)();
            if (h.value.truthy)(&(h.value.intrinsic)("mod-four-zero", vec![count.clone()])?) {
                let value = call(&h, "this.fetchBalance", || Ok(vec![]))?;
                balance = (h.value.await_value)(value).await?;
                if (h.value.truthy)(&(h.value.capture)("this.isDisposed")?) {
                    return Ok(true);
                }
            }
            let value = call(&h, "this.fetchOrders", || Ok(vec![]))?;
            let orders = (h.value.await_value)(value).await?;
            if (h.value.truthy)(&(h.value.capture)("this.isDisposed")?) {
                return Ok(true);
            }
            call(&h, "this.store.update", || {
                let update = (h.value.object)(vec![]);
                if (h.value.truthy)(&balance) {
                    define(&h, &update, "balance", balance.clone())?;
                }
                if (h.value.truthy)(&orders) {
                    define(&h, &update, "orders", orders.clone())?;
                }
                let flags = (h.value.object)(vec![]);
                spread(&h, &flags, (h.value.capture)("this.store.loaded")?)?;
                if (h.value.truthy)(&balance) {
                    define(&h, &flags, "balance", (h.boolean)(true))?;
                }
                define(&h, &flags, "positions", (h.boolean)(true))?;
                if (h.value.truthy)(&orders) {
                    define(&h, &flags, "orders", (h.boolean)(true))?;
                }
                define(&h, &update, "loaded", flags)?;
                Ok(vec![update])
            })?;
            Ok::<bool, E>(false)
        }
        .await;
        match result {
            Ok(true) => return Ok((h.value.undefined)()),
            Ok(false) => {}
            Err(e) => emit(&h, None, || message(&h, &e))?,
        }
        if !(h.value.truthy)(&(h.value.capture)("this.isDisposed")?) {
            let ops = h.clone();
            call(&h, "loop", || {
                let callback = (h.callback)(Rc::new(move || {
                    call(&ops, "this.tick", || {
                        Ok(vec![(ops.value.intrinsic)("add-one", vec![count.clone()])?])
                    })
                }));
                let interval = call(&h, "this.getTickInterval", || Ok(vec![]))?;
                Ok(vec![callback, interval])
            })?;
        }
    }
    Ok((h.value.undefined)())
}
pub async fn fetch_position_metadata<V: Clone + 'static, E: 'static>(
    h: Rc<BootstrapOps<V, E>>,
    symbols: V,
) -> Result<V, E> {
    let result = async {
        let flatten = (h.value.capture)("flatten")?;
        let ops = h.clone();
        let value = call(&h, "mapSeries", || {
            Ok(vec![
                symbols,
                (h.async_mapper)(Rc::new(move |symbol| {
                    let ops = ops.clone();
                    Box::pin(async move { metadata_for_symbol(ops, symbol).await })
                })),
            ])
        })?;
        let results = (h.value.await_value)(value).await?;
        (h.value.invoke)(flatten, vec![results])
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            emit(&h, Some(&e), || message(&h, &e))?;
            Ok((h.value.array)(vec![]))
        }
    }
}
async fn metadata_for_symbol<V: Clone, E>(h: Rc<BootstrapOps<V, E>>, symbol: V) -> Result<V, E> {
    if (h.value.truthy)(&(h.value.capture)("this.isDisposed")?) {
        return Ok((h.value.array)(vec![]));
    }
    let m = index(
        &h,
        (h.value.capture)("this.store.marketsBySymbol")?,
        symbol.clone(),
    )?;
    if !(h.value.truthy)(&m) {
        return Ok((h.value.array)(vec![]));
    }
    let all = (h.value.capture)("Promise.all")?;
    let first = call(&h, "this.xhr.get", || {
        Ok(vec![
            (h.value.capture)("ENDPOINTS.GET_MARGIN_MODE")?,
            (h.value.object)(vec![(
                "params",
                (h.value.object)(vec![("symbol", (h.value.get)(&m, "id")?)]),
            )]),
        ])
    })?;
    let second = call(&h, "this.xhr.get", || {
        Ok(vec![(h.value.capture)("ENDPOINTS.GET_POSITION_MODE")?])
    })?;
    let value = (h.value.invoke)(all, vec![(h.value.array)(vec![first, second])])?;
    let responses = (h.value.await_value)(value).await?;
    let (margin, mode) = (h.response_pair)(responses)?;
    if (h.value.truthy)(&(h.value.capture)("this.isDisposed")?) {
        return Ok((h.value.array)(vec![]));
    }
    if bad_code(&h, &margin)? {
        emit(&h, None, || {
            fallback(&h, (h.value.get)(&margin, "msg")?, || {
                (h.value.intrinsic)("margin-mode-error", vec![symbol.clone()])
            })
        })?;
        return Ok((h.value.array)(vec![]));
    }
    if bad_code(&h, &mode)? {
        emit(&h, None, || {
            fallback(&h, (h.value.get)(&mode, "msg")?, || {
                Ok((h.value.literal)("Failed to fetch position mode"))
            })
        })?;
        return Ok((h.value.array)(vec![]));
    }
    let position_mode = call(&h, "this.parsePositionMode", || {
        let data = (h.value.get)(&mode, "data")?;
        Ok(vec![optional(&h, &data, "positionMode")?])
    })?;
    let margin_type = call(&h, "this.parseMarginType", || {
        let data = (h.value.get)(&margin, "data")?;
        Ok(vec![optional(&h, &data, "marginMode")?])
    })?;
    let mut leverage = (h.value.number)(1.);
    if (h.value.equal)(&margin_type, &(h.value.capture)("MarginType.Cross")?) {
        let value = call(&h, "this.xhr.get", || {
            Ok(vec![
                (h.value.capture)("ENDPOINTS.POSITION_LEVERAGE")?,
                (h.value.object)(vec![(
                    "params",
                    (h.value.object)(vec![("symbol", (h.value.get)(&m, "id")?)]),
                )]),
            ])
        })?;
        let response = (h.value.await_value)(value).await?;
        let data = (h.value.get)(&response, "data")?;
        if !bad_code(&h, &data)? {
            leverage = parsed(&h, "parseFloat", || {
                let data = (h.value.get)(&data, "data")?;
                fallback(&h, optional(&h, &data, "leverage")?, || {
                    Ok((h.value.literal)("1"))
                })
            })?;
        }
    }
    let oneway = (h.value.equal)(&position_mode, &(h.value.capture)("PositionMode.OneWay")?);
    let mut rows = Vec::new();
    for side in if oneway {
        vec!["PositionSide.Long"]
    } else {
        vec!["PositionSide.Long", "PositionSide.Short"]
    } {
        rows.push((h.value.object)(vec![
            ("symbol", symbol.clone()),
            ("side", (h.value.capture)(side)?),
            ("leverage", leverage.clone()),
            ("positionMode", position_mode.clone()),
            ("marginType", margin_type.clone()),
        ]));
    }
    Ok((h.value.array)(rows))
}
pub async fn fetch_positions<V: Clone + 'static, E: 'static>(
    h: Rc<BootstrapOps<V, E>>,
    symbols: V,
) -> Result<V, E> {
    let value = call(&h, "this.fetchOpenPositions", || Ok(vec![]))?;
    let open = (h.value.await_value)(value).await?;
    if !(h.value.truthy)(&symbols) {
        return Ok(open);
    }
    let ops = h.clone();
    let selected = symbols.clone();
    let open = (h.raw_filter)(
        open,
        Rc::new(move |p| {
            let includes = (ops.member)(selected.clone(), "includes")?;
            (ops.value.invoke)(includes, vec![(ops.value.get)(&p, "symbol")?])
        }),
    )?;
    let ops = h.clone();
    let positions = open.clone();
    let missing = (h.value.filter)(
        symbols,
        Rc::new(move |symbol| {
            let h = ops.clone();
            let selected = symbol.clone();
            Ok(!(ops.some)(
                positions.clone(),
                Rc::new(move |p| Ok((h.value.equal)(&(h.value.get)(&p, "symbol")?, &selected))),
            )?)
        }),
    )?;
    let value = call(&h, "this.fetchPositionMetadata", || Ok(vec![missing]))?;
    let metadata = (h.value.await_value)(value).await?;
    let ops = h.clone();
    let fake = (h.value.map)(
        metadata,
        Rc::new(move |metadata| fake_position(&ops, metadata)),
    )?;
    (h.value.intrinsic)("spread-arrays", vec![open, fake])
}
fn fake_position<V: Clone, E>(h: &BootstrapOps<V, E>, metadata: V) -> Result<V, E> {
    let id = call(h, "createPositionId", || {
        Ok(vec![
            (h.value.get)(&metadata, "symbol")?,
            (h.value.get)(&metadata, "side")?,
            (h.value.get)(&metadata, "positionMode")?,
        ])
    })?;
    let object = (h.value.object)(vec![("id", id)]);
    spread(h, &object, metadata.clone())?;
    for key in [
        "entryPrice",
        "notional",
        "unrealizedPnl",
        "contracts",
        "liquidationPrice",
    ] {
        define(h, &object, key, (h.value.number)(0.))?;
    }
    let info = (h.value.object)(vec![]);
    spread(h, &info, metadata)?;
    define(h, &object, "info", info)?;
    Ok(object)
}
pub async fn fetch_open_positions<V: Clone, E>(h: &BootstrapOps<V, E>) -> Result<V, E> {
    let data = request(h, "ENDPOINTS.POSITIONS_V2").await?;
    if bad_code(h, &data)? {
        emit(h, None, || {
            fallback(h, (h.value.get)(&data, "msg")?, || {
                Ok((h.value.literal)("Failed to fetch positions"))
            })
        })?;
        return Ok((h.value.array)(vec![]));
    }
    let positions = (h.value.array)(vec![]);
    if (h.value.truthy)(&call(h, "Array.isArray", || {
        Ok(vec![(h.value.get)(&data, "data")?])
    })?) {
        let rows = (h.value.get)(&data, "data")?;
        let iterator = (h.iterator)(rows)?;
        while let Some(p) = (h.next)(&iterator)? {
            let result = map_open_position(h, p);
            match result {
                Ok(Some(position)) => {
                    (h.member)(positions.clone(), "push")
                        .and_then(|pushed| (h.value.invoke)(pushed, vec![position]))
                        .map_err(|e| (h.close_throw)(&iterator, e))?;
                }
                Ok(None) => {}
                Err(e) => return Err((h.close_throw)(&iterator, e)),
            }
        }
    }
    Ok(positions)
}
fn map_open_position<V: Clone, E>(h: &BootstrapOps<V, E>, p: V) -> Result<Option<V>, E> {
    let contracts = row_float(h, &p, "currentQty", "0")?;
    if (h.value.equal)(&contracts, &(h.value.number)(0.)) {
        return Ok(None);
    }
    let symbol = call(h, "this.toInternalSymbol", || {
        Ok(vec![(h.value.get)(&p, "symbol")?])
    })?;
    let price = row_float(h, &p, "markPrice", "0")?;
    let size = call(h, "this.contractSizeFor", || Ok(vec![symbol.clone()]))?;
    let native = call(h, "Math.abs", || Ok(vec![contracts.clone()]))?;
    let base = call(h, "multiply", || Ok(vec![native.clone(), size]))?;
    let side = if (h.value.equal)(
        &(h.value.get)(&p, "positionSide")?,
        &(h.value.literal)("LONG"),
    ) {
        (h.value.capture)("PositionSide.Long")?
    } else if (h.value.equal)(
        &(h.value.get)(&p, "positionSide")?,
        &(h.value.literal)("SHORT"),
    ) {
        (h.value.capture)("PositionSide.Short")?
    } else if (h.value.truthy)(&(h.value.intrinsic)("greater-zero", vec![contracts])?) {
        (h.value.capture)("PositionSide.Long")?
    } else {
        (h.value.capture)("PositionSide.Short")?
    };
    let mode = call(h, "this.parseRowPositionMode", || Ok(vec![p.clone()]))?;
    let margin = call(h, "this.parseMarginType", || {
        Ok(vec![
            (h.value.get)(&p, "marginMode")?,
            (h.value.get)(&p, "crossMode")?,
        ])
    })?;
    let id = call(h, "createPositionId", || {
        Ok(vec![symbol.clone(), side.clone(), mode.clone()])
    })?;
    let entry = row_float(h, &p, "avgEntryPrice", "0")?;
    let notional = call(h, "Math.abs", || {
        Ok(vec![call(h, "multiply", || Ok(vec![base.clone(), price]))?])
    })?;
    let leverage = parsed(h, "parseInt", || {
        fallback(h, (h.value.get)(&p, "leverage")?, || {
            Ok((h.value.literal)("1"))
        })
    })?;
    let pnl = row_float(h, &p, "unrealisedPnl", "0")?;
    let liquidation = row_float(h, &p, "liquidationPrice", "0")?;
    Ok(Some((h.value.object)(vec![
        ("id", id),
        ("symbol", symbol),
        ("side", side),
        ("entryPrice", entry),
        ("notional", notional),
        ("leverage", leverage),
        ("unrealizedPnl", pnl),
        ("contracts", base),
        ("liquidationPrice", liquidation),
        ("positionMode", mode),
        ("marginType", margin),
        ("extra", (h.value.object)(vec![("contractsNative", native)])),
        ("info", p),
    ])))
}
pub async fn fetch_markets<V: Clone + 'static, E: 'static>(
    h: Rc<BootstrapOps<V, E>>,
) -> Result<V, E> {
    let data = request(&h, "ENDPOINTS.MARKETS").await?;
    if bad_code(&h, &data)? {
        emit(&h, None, || {
            fallback(&h, (h.value.get)(&data, "msg")?, || {
                Ok((h.value.literal)("Failed to fetch markets"))
            })
        })?;
        return Ok((h.value.array)(vec![]));
    }
    let ops = h.clone();
    let markets = (h.value.map)(
        (h.value.get)(&data, "data")?,
        Rc::new(move |m| map_market(&ops, m)),
    )?;
    let ops = h.clone();
    (h.value.filter)(
        markets,
        Rc::new(move |m| {
            if !(ops.value.equal)(
                &(ops.value.get)(&m, "quote")?,
                &(ops.value.capture)("this.apiMarginCoin")?,
            ) {
                return Ok(false);
            }
            Ok(!(ops.value.truthy)(&(ops.value.get)(&m, "isExpiring")?))
        }),
    )
}
fn map_market<V: Clone, E>(h: &BootstrapOps<V, E>, m: V) -> Result<V, E> {
    let quote = fallback(h, (h.value.get)(&m, "settleCurrency")?, || {
        fallback(h, (h.value.get)(&m, "quoteCurrency")?, || {
            (h.value.get)(&m, "quoteCoin")
        })
    })?;
    let base = fallback(h, (h.value.get)(&m, "baseCurrency")?, || {
        (h.value.get)(&m, "rootSymbol")
    })?;
    let id = (h.value.get)(&m, "symbol")?;
    let symbol = call(h, "this.toInternalSymbol", || {
        Ok(vec![(h.value.get)(&m, "symbol")?])
    })?;
    let base = if (h.value.equal)(&base, &(h.value.literal)("XBT")) {
        (h.value.literal)("BTC")
    } else {
        base
    };
    let active = if (h.value.equal)(&(h.value.get)(&m, "status")?, &(h.value.literal)("Open")) {
        true
    } else {
        (h.value.equal)(&(h.value.get)(&m, "status")?, &(h.value.literal)("ONLINE"))
    };
    let precision = (h.value.object)(vec![
        ("amount", row_float(h, &m, "lotSize", "1")?),
        ("price", row_float(h, &m, "tickSize", "0.01")?),
    ]);
    let min = parsed(h, "parseFloat", || {
        fallback(h, (h.value.get)(&m, "minOrderQty")?, || {
            fallback(h, (h.value.get)(&m, "lotSize")?, || {
                Ok((h.value.literal)("1"))
            })
        })
    })?;
    let max = parsed(h, "parseFloat", || {
        fallback(h, (h.value.get)(&m, "maxOrderQty")?, || {
            Ok((h.value.number)(f64::INFINITY))
        })
    })?;
    let max_leverage = parsed(h, "parseInt", || {
        fallback(h, (h.value.get)(&m, "maxLeverage")?, || {
            Ok((h.value.number)(20.))
        })
    })?;
    let limits = (h.value.object)(vec![
        ("amount", (h.value.object)(vec![("min", min), ("max", max)])),
        (
            "leverage",
            (h.value.object)(vec![("min", (h.value.number)(1.)), ("max", max_leverage)]),
        ),
    ]);
    let expiring = call(h, "Boolean", || Ok(vec![(h.value.get)(&m, "expireDate")?]))?;
    let inverse = call(h, "Boolean", || Ok(vec![(h.value.get)(&m, "isInverse")?]))?;
    let size = row_float(h, &m, "multiplier", "1")?;
    let info = (h.value.object)(vec![]);
    spread(h, &info, m)?;
    define(h, &info, "isQuanto", (h.boolean)(false))?;
    Ok((h.value.object)(vec![
        ("id", id),
        ("symbol", symbol),
        ("base", base),
        ("quote", quote),
        ("active", (h.boolean)(active)),
        ("precision", precision),
        ("limits", limits),
        ("isExpiring", expiring),
        ("isInverse", inverse),
        ("contractSize", size),
        ("info", info),
    ]))
}
pub async fn fetch_tickers<V: Clone, E>(h: &BootstrapOps<V, E>) -> Result<V, E> {
    let data = request(h, "ENDPOINTS.TICKERS").await?;
    if bad_code(h, &data)? {
        emit(h, None, || {
            fallback(h, (h.value.get)(&data, "msg")?, || {
                Ok((h.value.literal)("Failed to fetch tickers"))
            })
        })?;
        return Ok((h.value.array)(vec![]));
    }
    let rows = (h.value.get)(&data, "data")?;
    if !(h.value.truthy)(&rows)
        || !(h.value.truthy)(&(h.value.intrinsic)("typeof-object", vec![rows.clone()])?)
    {
        return Ok((h.value.array)(vec![]));
    }
    let markets = (h.value.capture)("this.store.marketsById")?;
    let tickers = (h.value.array)(vec![]);
    let rows = if (h.value.truthy)(&call(h, "Array.isArray", || Ok(vec![rows.clone()]))?) {
        rows
    } else {
        call(h, "Object.values", || Ok(vec![rows]))?
    };
    let iterator = (h.iterator)(rows)?;
    while let Some(row) = (h.next)(&iterator)? {
        let result = map_ticker(h, &markets, row, &tickers);
        match result {
            Ok(Some((push, ticker))) => {
                (h.value.invoke)(push, vec![ticker]).map_err(|e| (h.close_throw)(&iterator, e))?;
            }
            Ok(None) => {}
            Err(e) => return Err((h.close_throw)(&iterator, e)),
        }
    }
    Ok(tickers)
}
fn map_ticker<V: Clone, E>(
    h: &BootstrapOps<V, E>,
    markets: &V,
    row: V,
    tickers: &V,
) -> Result<Option<(V, V)>, E> {
    let market = index(h, markets.clone(), (h.value.get)(&row, "symbol")?)?;
    if !(h.value.truthy)(&market) {
        return Ok(None);
    }
    let price = row_float(h, &row, "price", "0")?;
    let push = (h.member)(tickers.clone(), "push")?;
    let id = (h.value.get)(&market, "id")?;
    let symbol = (h.value.get)(&market, "symbol")?;
    let bid = row_float(h, &row, "bestBidPrice", "0")?;
    let ask = row_float(h, &row, "bestAskPrice", "0")?;
    let mark = parsed(h, "parseFloat", || {
        fallback(h, (h.value.get)(&row, "markPrice")?, || {
            fallback(h, (h.value.get)(&row, "price")?, || {
                Ok((h.value.literal)("0"))
            })
        })
    })?;
    let index = parsed(h, "parseFloat", || {
        fallback(h, (h.value.get)(&row, "indexPrice")?, || {
            fallback(h, (h.value.get)(&row, "price")?, || {
                Ok((h.value.literal)("0"))
            })
        })
    })?;
    let volume = parsed(h, "parseFloat", || {
        fallback(h, (h.value.get)(&row, "volumeOf24h")?, || {
            fallback(h, (h.value.get)(&row, "volume24h")?, || {
                fallback(h, (h.value.get)(&row, "size")?, || {
                    Ok((h.value.literal)("0"))
                })
            })
        })
    })?;
    let quote = parsed(h, "parseFloat", || {
        fallback(h, (h.value.get)(&row, "turnoverOf24h")?, || {
            fallback(h, (h.value.get)(&row, "turnover24h")?, || {
                Ok((h.value.literal)("0"))
            })
        })
    })?;
    Ok(Some((
        push,
        (h.value.object)(vec![
            ("id", id),
            ("symbol", symbol),
            ("bid", bid),
            ("ask", ask),
            ("last", price),
            ("mark", mark),
            ("index", index),
            ("percentage", (h.value.number)(0.)),
            ("fundingRate", (h.value.number)(0.)),
            ("fundingTime", (h.value.number)(0.)),
            ("openInterest", (h.value.number)(0.)),
            ("volume", volume),
            ("quoteVolume", quote),
        ]),
    )))
}
pub async fn fetch_ohlcv<V: Clone + 'static, E: 'static>(
    h: Rc<BootstrapOps<V, E>>,
    opts: V,
) -> Result<V, E> {
    let limit = call(&h, "Math.min", || {
        Ok(vec![
            fallback(&h, (h.value.get)(&opts, "limit")?, || {
                Ok((h.value.number)(500.))
            })?,
            (h.value.number)(1000.),
        ])
    })?;
    let interval = index(
        &h,
        (h.value.capture)("INTERVAL")?,
        (h.value.get)(&opts, "interval")?,
    )?;
    let (amount, unit) = (h.interval_parts)((h.value.get)(&opts, "interval")?)?;
    let end = if (h.value.truthy)(&(h.value.get)(&opts, "to")?) {
        call(&h, "dayjs", || Ok(vec![(h.value.get)(&opts, "to")?]))?
    } else {
        call(&h, "dayjs", || Ok(vec![]))?
    };
    let from = if !(h.value.truthy)(&(h.value.get)(&opts, "limit")?) {
        (h.value.truthy)(&(h.value.get)(&opts, "from")?)
    } else {
        false
    };
    let start = if from {
        call(&h, "dayjs", || Ok(vec![(h.value.get)(&opts, "from")?]))?
    } else {
        let subtract = (h.member)(end.clone(), "subtract")?;
        let parsed = parsed(&h, "parseFloat", || Ok(amount))?;
        let span = (h.value.intrinsic)("multiply-numbers", vec![parsed, limit])?;
        (h.value.invoke)(subtract, vec![span, unit])?
    };
    let value = call(&h, "this.xhr.get", || {
        let endpoint = (h.value.capture)("ENDPOINTS.KLINE")?;
        let market = index(
            &h,
            (h.value.capture)("this.store.marketsBySymbol")?,
            (h.value.get)(&opts, "symbol")?,
        )?;
        let symbol = fallback(&h, optional(&h, &market, "id")?, || {
            (h.value.get)(&opts, "symbol")
        })?;
        let from = (h.value.invoke)((h.member)(start, "valueOf")?, vec![])?;
        let to = (h.value.invoke)((h.member)(end, "valueOf")?, vec![])?;
        Ok(vec![
            endpoint,
            (h.value.object)(vec![(
                "params",
                (h.value.object)(vec![
                    ("symbol", symbol),
                    ("granularity", interval),
                    ("from", from),
                    ("to", to),
                ]),
            )]),
        ])
    })?;
    let response = (h.value.await_value)(value).await?;
    let data = (h.value.get)(&response, "data")?;
    let ops = h.clone();
    (h.value.map)(
        (h.value.get)(&data, "data")?,
        Rc::new(move |c| map_candle(&ops, c)),
    )
}
fn map_candle<V, E>(h: &BootstrapOps<V, E>, c: V) -> Result<V, E> {
    let timestamp = call(h, "parseInt", || {
        Ok(vec![(h.value.get)(&c, "0")?, (h.value.number)(10.)])
    })?;
    let timestamp = (h.value.intrinsic)("divide-thousand", vec![timestamp])?;
    let mut fields = vec![("timestamp", timestamp)];
    for (index, key) in [
        ("1", "open"),
        ("2", "high"),
        ("3", "low"),
        ("4", "close"),
        ("5", "volume"),
    ] {
        fields.push((key, parsed(h, "parseFloat", || (h.value.get)(&c, index))?));
    }
    Ok((h.value.object)(fields))
}
