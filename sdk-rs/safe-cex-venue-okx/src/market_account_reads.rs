use std::{cell::Cell, future::Future, pin::Pin, rc::Rc};
pub type ReadAwait<V, E> = Pin<Box<dyn Future<Output = Result<V, E>>>>;
pub type ReadMapper<V, E> = Rc<dyn Fn(V) -> Result<V, E>>;
pub type ReadAsyncMapper<V, E> = Rc<dyn Fn(V) -> ReadAwait<V, E>>;
pub struct MarketAccountOps<V, E> {
    pub get: Rc<dyn Fn(&V, &str) -> Result<V, E>>,
    pub set: Rc<dyn Fn(&V, &str, V) -> Result<(), E>>,
    pub capture: Rc<dyn Fn(&str) -> Result<V, E>>,
    pub member: Rc<dyn Fn(V, &str) -> Result<V, E>>,
    pub invoke: Rc<dyn Fn(V, Vec<V>) -> Result<V, E>>,
    pub intrinsic: Rc<dyn Fn(&str, Vec<V>) -> Result<V, E>>,
    pub truthy: Rc<dyn Fn(&V) -> bool>,
    pub nullish: Rc<dyn Fn(&V) -> bool>,
    pub equal: Rc<dyn Fn(&V, &V) -> bool>,
    pub literal: Rc<dyn Fn(&str) -> V>,
    pub number: Rc<dyn Fn(f64) -> V>,
    pub boolean: Rc<dyn Fn(bool) -> V>,
    pub undefined: Rc<dyn Fn() -> V>,
    pub array: Rc<dyn Fn(Vec<V>) -> V>,
    pub object: Rc<dyn Fn(Vec<(&str, V)>) -> V>,
    pub map: Rc<dyn Fn(V, ReadMapper<V, E>) -> Result<V, E>>,
    pub bind_first: Rc<dyn Fn(V, ReadMapper<V, E>) -> Result<V, E>>,
    pub flat_map: Rc<dyn Fn(V, ReadMapper<V, E>) -> Result<V, E>>,
    pub filter: Rc<dyn Fn(V, Rc<dyn Fn(V) -> Result<bool, E>>) -> Result<V, E>>,
    pub reduce: Rc<dyn Fn(V, V, Rc<dyn Fn(V, V) -> Result<V, E>>) -> Result<V, E>>,
    pub for_of: Rc<dyn Fn(V, Rc<dyn Fn(V) -> Result<(), E>>) -> Result<(), E>>,
    pub sum_by: Rc<dyn Fn(V, V, Option<ReadMapper<V, E>>) -> Result<V, E>>,
    pub parallel_map: Rc<dyn Fn(V, V, ReadAsyncMapper<V, E>) -> ReadAwait<V, E>>,
    pub some: Rc<dyn Fn(V, Rc<dyn Fn(V) -> Result<bool, E>>) -> Result<bool, E>>,
    pub await_value: Rc<dyn Fn(V) -> ReadAwait<V, E>>,
    pub error_value: Rc<dyn Fn(&E) -> V>,
    pub new_error: Rc<dyn Fn(V, V) -> Result<E, E>>,
}
fn optional<V, E>(h: &MarketAccountOps<V, E>, v: &V, key: &str) -> Result<V, E> {
    if (h.nullish)(v) {
        Ok((h.undefined)())
    } else {
        (h.get)(v, key)
    }
}
fn or<V, E>(
    h: &MarketAccountOps<V, E>,
    value: V,
    next: impl FnOnce() -> Result<V, E>,
) -> Result<V, E> {
    if (h.truthy)(&value) {
        Ok(value)
    } else {
        next()
    }
}
fn index<V, E>(h: &MarketAccountOps<V, E>, object: V, key: V) -> Result<V, E> {
    (h.intrinsic)("get-index", vec![object, key])
}
fn put_index<V, E>(h: &MarketAccountOps<V, E>, object: V, key: V, value: V) -> Result<(), E> {
    (h.intrinsic)("set-index", vec![object, key, value])?;
    Ok(())
}
fn call<V, E>(
    h: &MarketAccountOps<V, E>,
    name: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let callee = (h.capture)(name)?;
    (h.invoke)(callee, args()?)
}
fn float<V, E>(h: &MarketAccountOps<V, E>, value: impl FnOnce() -> Result<V, E>) -> Result<V, E> {
    call(h, "parseFloat", || Ok(vec![value()?]))
}
fn finite<V, E>(h: &MarketAccountOps<V, E>, value: impl FnOnce() -> Result<V, E>) -> Result<V, E> {
    call(h, "this.toFiniteNumber", || Ok(vec![value()?]))
}
fn emitted<V: Clone, E>(h: &MarketAccountOps<V, E>, error: &E) -> Result<(), E> {
    let emitter = (h.capture)("this.emitAndLogError")?;
    let err = (h.error_value)(error);
    let response = optional(h, &err, "response")?;
    let data = optional(h, &response, "data")?;
    let message = or(h, optional(h, &data, "msg")?, || {
        optional(h, &err, "message")
    })?;
    (h.invoke)(emitter, vec![message])?;
    Ok(())
}
fn response_rows<V: Clone + 'static, E: 'static>(
    h: Rc<MarketAccountOps<V, E>>,
    responses: V,
) -> Result<V, E> {
    let ops = h.clone();
    (h.flat_map)(
        responses,
        Rc::new(move |response| {
            let data = (ops.get)(&response, "data")?;
            (ops.get)(&data, "data")
        }),
    )
}
pub async fn fetch_markets<V: Clone + 'static, E: 'static>(
    h: Rc<MarketAccountOps<V, E>>,
) -> Result<V, E> {
    let loaded = async {
        let all = (h.capture)("Promise.all")?;
        let types = call(&h, "this.getInstrumentTypes", || Ok(vec![]))?;
        let ops = h.clone();
        let requests = (h.map)(
            types,
            Rc::new(move |kind| {
                let get = (ops.capture)("this.xhr.get")?;
                let endpoint = call(&ops, "this.getMarketInstrumentsEndpoint", || Ok(vec![]))?;
                (ops.invoke)(
                    get,
                    vec![
                        endpoint,
                        (ops.object)(vec![("params", (ops.object)(vec![("instType", kind)]))]),
                    ],
                )
            }),
        )?;
        let responses = (h.await_value)((h.invoke)(all, vec![requests])?).await?;
        let data = response_rows(h.clone(), responses)?;
        let ops = h.clone();
        let data = (h.filter)(
            data,
            Rc::new(move |row| {
                Ok((ops.truthy)(&call(
                    &ops,
                    "this.isSupportedMarketInstrument",
                    || Ok(vec![row]),
                )?))
            }),
        )?;
        let ops = h.clone();
        (h.map)(data, Rc::new(move |row| map_market(ops.clone(), row)))
    }
    .await;
    match loaded {
        Ok(value) => Ok(value),
        Err(error) => {
            emitted(&h, &error)?;
            (h.capture)("this.store.markets")
        }
    }
}
pub fn map_market<V: Clone + 'static, E: 'static>(
    h: Rc<MarketAccountOps<V, E>>,
    m: V,
) -> Result<V, E> {
    let inverse = (h.equal)(&(h.get)(&m, "ctType")?, &(h.literal)("inverse"));
    let underlying = call(&h, "String", || {
        Ok(vec![or(&h, (h.get)(&m, "uly")?, || {
            or(&h, (h.get)(&m, "instFamily")?, || Ok((h.literal)("")))
        })?])
    })?;
    let split = (h.member)(underlying, "split")?;
    let parts = (h.invoke)(split, vec![(h.literal)("-")])?;
    let ops = h.clone();
    let parts = (h.map)(
        parts,
        Rc::new(move |value| {
            if (ops.nullish)(&value) {
                return Ok((ops.literal)(""));
            }
            let upper = (ops.member)(value, "toUpperCase")?;
            let value = if (ops.nullish)(&upper) {
                (ops.undefined)()
            } else {
                (ops.invoke)(upper, vec![])?
            };
            or(&ops, value, || Ok((ops.literal)("")))
        }),
    )?;
    let parts = (h.intrinsic)("destructure-two", vec![parts])?;
    let uly_base = (h.get)(&parts, "0")?;
    let uly_quote = (h.get)(&parts, "1")?;
    let minimum = (h.capture)("Math.min")?;
    let mut maxima = Vec::new();
    for key in [
        "maxIcebergSz",
        "maxLmtSz",
        "maxMktSz",
        "maxStopSz",
        "maxTriggerSz",
        "maxTwapSz",
    ] {
        maxima.push(float(&h, || (h.get)(&m, key))?);
    }
    let max = (h.invoke)(minimum, maxima)?;
    let base = or(&h, (h.get)(&m, "baseCcy")?, || {
        or(
            &h,
            (h.get)(&m, if inverse { "settleCcy" } else { "ctValCcy" })?,
            || or(&h, uly_base, || Ok((h.literal)(""))),
        )
    })?;
    let base = (h.intrinsic)("toUpperCase-expression", vec![base])?;
    let quote = or(&h, (h.get)(&m, "quoteCcy")?, || {
        or(
            &h,
            (h.get)(&m, if inverse { "ctValCcy" } else { "settleCcy" })?,
            || or(&h, uly_quote, || Ok((h.literal)(""))),
        )
    })?;
    let quote = (h.intrinsic)("toUpperCase-expression", vec![quote])?;
    let pretty = call(&h, "this.formatMarketPrettySymbol", || {
        Ok(vec![m.clone(), base.clone(), quote.clone()])
    })?;
    let mut fields = vec![
        ("id", (h.get)(&m, "instId")?),
        (
            "symbol",
            call(&h, "this.formatMarketSymbol", || Ok(vec![m.clone()]))?,
        ),
    ];
    if (h.truthy)(&pretty) {
        fields.push(("prettySymbol", pretty));
    }
    fields.extend(vec![
        ("base", base),
        ("quote", quote),
        ("isInverse", (h.boolean)(inverse)),
        (
            "active",
            (h.boolean)((h.equal)(&(h.get)(&m, "state")?, &(h.literal)("live"))),
        ),
    ]);
    let precision = (h.object)(vec![
        ("amount", float(&h, || (h.get)(&m, "lotSz"))?),
        ("price", float(&h, || (h.get)(&m, "tickSz"))?),
    ]);
    fields.push(("precision", precision));
    let number = call(&h, "BigNumber", || Ok(vec![(h.get)(&m, "minSz")?]))?;
    let multiply = (h.member)(number, "multipliedBy")?;
    let product = (h.invoke)(multiply, vec![(h.get)(&m, "ctVal")?])?;
    let to_number = (h.member)(product, "toNumber")?;
    let min = (h.invoke)(to_number, vec![])?;
    let amount = (h.object)(vec![("min", min), ("max", max)]);
    let leverage = (h.object)(vec![
        ("min", (h.number)(1.)),
        ("max", float(&h, || (h.get)(&m, "lever"))?),
    ]);
    fields.push((
        "limits",
        (h.object)(vec![("amount", amount), ("leverage", leverage)]),
    ));
    fields.push(("contractSize", float(&h, || (h.get)(&m, "ctVal"))?));
    fields.push(("info", m));
    Ok((h.object)(fields))
}
pub async fn fetch_tickers_raw<V: Clone + 'static, E: 'static>(
    h: Rc<MarketAccountOps<V, E>>,
) -> Result<V, E> {
    let loaded = async {
        let all = (h.capture)("Promise.all")?;
        let types = call(&h, "this.getInstrumentTypes", || Ok(vec![]))?;
        let ops = h.clone();
        let requests = (h.map)(
            types,
            Rc::new(move |kind| {
                let get = (ops.capture)("this.xhr.get:ENDPOINTS.TICKERS")?;
                (ops.invoke)(
                    get,
                    vec![(ops.object)(vec![(
                        "params",
                        (ops.object)(vec![("instType", kind)]),
                    )])],
                )
            }),
        )?;
        let responses = (h.await_value)((h.invoke)(all, vec![requests])?).await?;
        response_rows(h.clone(), responses)
    }
    .await;
    match loaded {
        Ok(value) => Ok(value),
        Err(error) => {
            emitted(&h, &error)?;
            Ok((h.array)(vec![]))
        }
    }
}
pub fn parse_tickers<V: Clone + 'static, E: 'static>(
    h: Rc<MarketAccountOps<V, E>>,
    data: V,
) -> Result<V, E> {
    let ops = h.clone();
    let parsed = (h.reduce)(
        data,
        (h.array)(vec![]),
        Rc::new(move |acc, row| {
            let markets = (ops.capture)("this.store.marketsById")?;
            let market = index(&ops, markets, (ops.get)(&row, "instId")?)?;
            if !(ops.truthy)(&market) {
                return Ok(acc);
            }
            let open = float(&ops, || (ops.get)(&row, "open24h"))?;
            let last = float(&ops, || (ops.get)(&row, "last"))?;
            let round = (ops.capture)("roundUSD")?;
            let percentage = (ops.intrinsic)("ticker-percentage", vec![last.clone(), open])?;
            let percentage = (ops.invoke)(round, vec![percentage])?;
            let ticker = (ops.object)(vec![
                ("id", (ops.get)(&market, "id")?),
                ("symbol", (ops.get)(&market, "symbol")?),
                ("bid", float(&ops, || (ops.get)(&row, "bidPx"))?),
                ("ask", float(&ops, || (ops.get)(&row, "askPx"))?),
                ("last", last.clone()),
                ("mark", last.clone()),
                ("index", last),
                ("percentage", percentage),
                ("fundingRate", (ops.number)(0.)),
                ("fundingTime", (ops.number)(0.)),
                ("volume", float(&ops, || (ops.get)(&row, "volCcy24h"))?),
                ("quoteVolume", float(&ops, || (ops.get)(&row, "vol24h"))?),
                ("openInterest", (ops.number)(0.)),
            ]);
            (ops.intrinsic)("spread-append", vec![acc, ticker])
        }),
    );
    match parsed {
        Ok(value) => Ok(value),
        Err(error) => {
            emitted(&h, &error)?;
            (h.capture)("this.store.tickers")
        }
    }
}
pub fn metadata_pair<V: Clone, E>(
    h: &MarketAccountOps<V, E>,
    symbol: V,
    mode: V,
    leverages: V,
) -> Result<V, E> {
    let mut rows = Vec::new();
    for side in ["PositionSide.Long", "PositionSide.Short"] {
        let fields = vec![
            ("symbol", symbol.clone()),
            ("side", (h.capture)(side)?),
            (
                "leverage",
                or(h, index(h, leverages.clone(), symbol.clone())?, || {
                    Ok((h.number)(0.))
                })?,
            ),
            ("positionMode", mode.clone()),
            ("marginType", (h.capture)("MarginType.Cross")?),
        ];
        rows.push((h.object)(fields));
    }
    Ok((h.array)(rows))
}
pub async fn fetch_position_metadata<V: Clone + 'static, E: 'static>(
    h: Rc<MarketAccountOps<V, E>>,
    symbols: V,
) -> Result<V, E> {
    let all = (h.capture)("Promise.all")?;
    let mode = call(&h, "this.getOrFetchPositionMode", || Ok(vec![]))?;
    let leverages = call(&h, "this.fetchLeverages", || Ok(vec![symbols.clone()]))?;
    let results = (h.await_value)((h.invoke)(all, vec![(h.array)(vec![mode, leverages])])?).await?;
    let pair = (h.intrinsic)("destructure-two", vec![results])?;
    let mode = (h.get)(&pair, "0")?;
    let leverages = (h.get)(&pair, "1")?;
    let ops = h.clone();
    (h.flat_map)(
        symbols,
        Rc::new(move |symbol| {
            let one_way = (ops.capture)("PositionMode.OneWay")?;
            // ts-bug: okx.exchange.ts fetchPositionMetadata branches on
            // `positionMode === PositionMode.OneWay` and both arms return the
            // identical Long+Short pair. The branch is kept because capture and
            // equal are RECORDED host effects the source oracle compares; a
            // collapsed `if` drops the comparison from the trace.
            #[allow(clippy::if_same_then_else)]
            if (ops.equal)(&mode, &one_way) {
                metadata_pair(&ops, symbol, mode.clone(), leverages.clone())
            } else {
                metadata_pair(&ops, symbol, mode.clone(), leverages.clone())
            }
        }),
    )
}
pub async fn fetch_balance_and_positions<V: Clone + 'static, E: 'static>(
    h: Rc<MarketAccountOps<V, E>>,
    symbols: V,
) -> Result<V, E> {
    let mode = call(&h, "this.getOrFetchPositionMode", || {
        Ok(vec![(h.boolean)(true)])
    })?;
    (h.await_value)(mode).await?;
    let response = call(&h, "this.fetchBalanceAndOpenPositions", || Ok(vec![]))?;
    let response = (h.await_value)(response).await?;
    let balance = (h.get)(&response, "balance")?;
    let open = (h.get)(&response, "positions")?;
    if !(h.truthy)(&symbols) {
        return Ok((h.object)(vec![("balance", balance), ("positions", open)]));
    }
    let ops = h.clone();
    let list = symbols.clone();
    let selected = (h.filter)(
        open,
        Rc::new(move |position| {
            let includes = (ops.member)(list.clone(), "includes")?;
            let value = (ops.invoke)(includes, vec![(ops.get)(&position, "symbol")?])?;
            Ok((ops.truthy)(&value))
        }),
    )?;
    let ops = h.clone();
    let current = selected.clone();
    let missing = (h.filter)(
        symbols,
        Rc::new(move |symbol| {
            let ops2 = ops.clone();
            let exists = (ops.some)(
                current.clone(),
                Rc::new(move |position| {
                    Ok((ops2.equal)(&(ops2.get)(&position, "symbol")?, &symbol))
                }),
            )?;
            Ok(!exists)
        }),
    )?;
    let metadata = call(&h, "this.fetchPositionMetadata", || Ok(vec![missing]))?;
    let metadata = (h.await_value)(metadata).await?;
    let ops = h.clone();
    let fake = (h.map)(
        metadata,
        Rc::new(move |metadata| {
            let id = call(&ops, "createPositionId", || {
                Ok(vec![
                    (ops.get)(&metadata, "symbol")?,
                    (ops.get)(&metadata, "side")?,
                    (ops.get)(&metadata, "positionMode")?,
                ])
            })?;
            let position = (ops.intrinsic)("object-spread-after-id", vec![id, metadata])?;
            for key in [
                "entryPrice",
                "notional",
                "unrealizedPnl",
                "contracts",
                "liquidationPrice",
            ] {
                (ops.set)(&position, key, (ops.number)(0.))?;
            }
            Ok(position)
        }),
    )?;
    let positions = (h.intrinsic)("array-spread-two", vec![selected, fake])?;
    Ok((h.object)(vec![
        ("balance", balance),
        ("positions", positions),
    ]))
}
pub async fn fetch_balance_and_open_positions<V: Clone + 'static, E: 'static>(
    h: Rc<MarketAccountOps<V, E>>,
) -> Result<V, E> {
    if !(h.truthy)(&(h.capture)("this._accountPositionMode")?) {
        let ctor = (h.capture)("Error")?;
        return Err((h.new_error)(
            ctor,
            (h.literal)("Position mode not loaded"),
        )?);
    }
    let loaded = async {
        let get = (h.capture)("this.xhr.get:ENDPOINTS.BALANCE")?;
        let types = call(&h, "this.getInstrumentTypes", || Ok(vec![]))?;
        let kind = (h.get)(&types, "0")?;
        let response = (h.invoke)(
            get,
            vec![(h.object)(vec![(
                "params",
                (h.object)(vec![("instType", kind)]),
            )])],
        )?;
        let response = (h.await_value)(response).await?;
        let data = (h.get)(&response, "data")?;
        let data = (h.get)(&data, "data")?;
        let ops = h.clone();
        let bal = (h.bind_first)(data, Rc::new(move |first| (ops.get)(&first, "balData")))?;
        let all = (h.capture)("Promise.all")?;
        let types = call(&h, "this.getInstrumentTypes", || Ok(vec![]))?;
        let ops = h.clone();
        let requests = (h.map)(
            types,
            Rc::new(move |kind| {
                let get = (ops.capture)("this.xhr.get:ENDPOINTS.POSITIONS")?;
                (ops.invoke)(
                    get,
                    vec![(ops.object)(vec![(
                        "params",
                        (ops.object)(vec![("instType", kind)]),
                    )])],
                )
            }),
        )?;
        let responses = (h.await_value)((h.invoke)(all, vec![requests])?).await?;
        let raw = response_rows(h.clone(), responses)?;
        let positions = call(&h, "this.mapPositions", || Ok(vec![raw]))?;
        let formatter = (h.capture)("this.formatBalanceUsedAvail")?;
        let orders = or(&h, (h.capture)("this.store.orders")?, || {
            Ok((h.array)(vec![]))
        })?;
        let balance =
            (h.await_value)((h.invoke)(formatter, vec![bal, positions.clone(), orders])?).await?;
        Ok((h.object)(vec![
            ("balance", balance),
            ("positions", positions),
        ]))
    }
    .await;
    match loaded {
        Ok(value) => Ok(value),
        Err(error) => {
            emitted(&h, &error)?;
            Ok((h.object)(vec![
                ("balance", (h.capture)("this.store.balance")?),
                ("positions", (h.capture)("this.store.positions")?),
            ]))
        }
    }
}
fn currency_totals<V: Clone, E>(h: &MarketAccountOps<V, E>, totals: V, key: V) -> Result<(), E> {
    if !(h.truthy)(&index(h, totals.clone(), key.clone())?) {
        let property = (h.intrinsic)("to-property-key", vec![key.clone()])?;
        put_index(
            h,
            totals.clone(),
            property,
            (h.object)(vec![("used", (h.number)(0.)), ("upnl", (h.number)(0.))]),
        )?;
    }
    Ok(())
}
fn order_leverage<V: Clone, E>(h: &MarketAccountOps<V, E>, order: &V) -> Result<V, E> {
    let maximum = (h.capture)("Math.max")?;
    let value = finite(h, || {
        let info = (h.get)(order, "info")?;
        or(h, (h.get)(&info, "lever")?, || Ok((h.literal)("1")))
    })?;
    (h.invoke)(maximum, vec![value, (h.number)(1.)])
}
fn order_used<V: Clone, E>(h: &MarketAccountOps<V, E>, order: &V, leverage: V) -> Result<V, E> {
    let remaining = (h.get)(order, "remaining")?;
    let price = (h.get)(order, "price")?;
    (h.intrinsic)(
        "order-notional-over-leverage",
        vec![remaining, price, leverage],
    )
}
pub async fn format_balance_used_avail<V: Clone + 'static, E: 'static>(
    h: Rc<MarketAccountOps<V, E>>,
    bal: V,
    positions: V,
    orders: V,
) -> Result<V, E> {
    let loaded = async {
        let level = call(&h, "this.getOrFetchAccountLevel", || Ok(vec![]))?;
        let level = (h.await_value)(level).await?;
        let level = or(&h, level, || Ok((h.number)(2.)))?;
        let ops = h.clone();
        let p_data = (h.map)(positions, Rc::new(move |p| (ops.get)(&p, "info")))?;
        let ops = h.clone();
        let p_data = (h.filter)(p_data, Rc::new(move |p| Ok((ops.truthy)(&p))))?;
        let ops = h.clone();
        let o_data = (h.filter)(
            orders,
            Rc::new(move |o| {
                if !(ops.truthy)(&(ops.get)(&o, "info")?) {
                    return Ok(false);
                }
                let kind = (ops.get)(&o, "type")?;
                if !(ops.equal)(&kind, &(ops.capture)("OrderType.Limit")?) {
                    return Ok(false);
                }
                let status = (ops.get)(&o, "status")?;
                if !(ops.equal)(&status, &(ops.capture)("OrderStatus.Open")?) {
                    return Ok(false);
                }
                Ok(!(ops.truthy)(&(ops.get)(&o, "reduceOnly")?))
            }),
        )?;
        let eq = (h.object)(vec![]);
        let ops = h.clone();
        let values = eq.clone();
        (h.for_of)(
            bal.clone(),
            Rc::new(move |b| {
                let mark = call(&ops, "findTickerMarkForCurrency", || {
                    Ok(vec![
                        (ops.capture)("this.store.tickers")?,
                        (ops.get)(&b, "ccy")?,
                    ])
                })?;
                let key = (ops.intrinsic)("to-property-key", vec![(ops.get)(&b, "ccy")?])?;
                let equity = finite(&ops, || (ops.get)(&b, "eq"))?;
                let value = (ops.intrinsic)("multiply", vec![equity, mark])?;
                put_index(&ops, values.clone(), key, value)
            }),
        )?;
        let round = (h.capture)("roundUSD")?;
        let sum = (h.capture)("sumBy")?;
        let values = (h.intrinsic)("Object.values-expression", vec![eq])?;
        let collateral = (h.sum_by)(sum, values, None)?;
        let collateral = (h.invoke)(round, vec![collateral])?;
        let used_p = margin_sum(h.clone(), p_data.clone(), "imr")?;
        let round = (h.capture)("roundUSD")?;
        let sum = (h.capture)("sumBy")?;
        let ops = h.clone();
        let used_o = (h.sum_by)(
            sum,
            o_data.clone(),
            Some(Rc::new(move |o| {
                let leverage = order_leverage(&ops, &o)?;
                order_used(&ops, &o, leverage)
            })),
        )?;
        let used_o = (h.intrinsic)("multiply", vec![used_o, (h.number)(1.1)])?;
        let used_o = (h.invoke)(round, vec![used_o])?;
        let upnl = margin_sum(h.clone(), p_data.clone(), "upl")?;
        let totals = (h.object)(vec![]);
        let ops = h.clone();
        let values = totals.clone();
        (h.for_of)(
            p_data,
            Rc::new(move |p| {
                let ccy = (ops.get)(&p, "ccy")?;
                currency_totals(&ops, values.clone(), ccy.clone())?;
                for (target, source) in [("used", "imr"), ("upnl", "upl")] {
                    let lhs = index(&ops, values.clone(), ccy.clone())?;
                    let add = (ops.capture)("add")?;
                    let rhs = index(&ops, values.clone(), ccy.clone())?;
                    let old = (ops.get)(&rhs, target)?;
                    let amount = finite(&ops, || (ops.get)(&p, source))?;
                    let new = (ops.invoke)(add, vec![old, amount])?;
                    (ops.set)(&lhs, target, new)?;
                }
                Ok(())
            }),
        )?;
        let ops = h.clone();
        let values = totals.clone();
        (h.for_of)(
            o_data,
            Rc::new(move |o| {
                let info = (ops.get)(&o, "info")?;
                let ccy = (ops.get)(&info, "ccy")?;
                currency_totals(&ops, values.clone(), ccy.clone())?;
                let markets = (ops.capture)("this.store.marketsBySymbol")?;
                let market = index(&ops, markets, (ops.get)(&o, "symbol")?)?;
                let leverage = order_leverage(&ops, &o)?;
                let used = order_used(&ops, &o, leverage)?;
                let used = (ops.intrinsic)("multiply", vec![used, (ops.number)(1.1)])?;
                let converted = call(&ops, "this.convertUsdValueToMarginCurrency", || {
                    Ok(vec![
                        used,
                        ccy.clone(),
                        market,
                        finite(&ops, || (ops.get)(&o, "price"))?,
                    ])
                })?;
                let lhs = index(&ops, values.clone(), ccy.clone())?;
                let add = (ops.capture)("add")?;
                let rhs = index(&ops, values.clone(), ccy.clone())?;
                let old = (ops.get)(&rhs, "used")?;
                let new = (ops.invoke)(add, vec![old, converted])?;
                (ops.set)(&lhs, "used", new)?;
                Ok(())
            }),
        )?;
        let free = call(&h, "subtract", || {
            Ok(vec![
                call(&h, "subtract", || {
                    Ok(vec![collateral.clone(), used_p.clone()])
                })?,
                used_o,
            ])
        })?;
        let total = call(&h, "subtract", || Ok(vec![collateral, upnl.clone()]))?;
        let currencies = if (h.equal)(&level, &(h.number)(2.)) {
            let ops = h.clone();
            (h.reduce)(
                bal.clone(),
                (h.object)(vec![]),
                Rc::new(move |acc, b| {
                    let ccy = (ops.get)(&b, "ccy")?;
                    let equity = finite(&ops, || (ops.get)(&b, "eq"))?;
                    let used = index(&ops, totals.clone(), ccy.clone())?;
                    let used = or(&ops, optional(&ops, &used, "used")?, || {
                        Ok((ops.number)(0.))
                    })?;
                    let pnl = index(&ops, totals.clone(), ccy.clone())?;
                    let pnl = or(&ops, optional(&ops, &pnl, "upnl")?, || Ok((ops.number)(0.)))?;
                    let key = (ops.intrinsic)("to-property-key", vec![ccy])?;
                    let free = call(&ops, "subtract", || Ok(vec![equity.clone(), used.clone()]))?;
                    let total = call(&ops, "subtract", || Ok(vec![equity, pnl.clone()]))?;
                    let balance = (ops.object)(vec![
                        ("used", used),
                        ("free", free),
                        ("total", total),
                        ("upnl", pnl),
                    ]);
                    put_index(&ops, acc.clone(), key, balance)?;
                    Ok(acc)
                }),
            )?
        } else {
            (h.object)(vec![])
        };
        Ok((h.object)(vec![
            ("used", used_p),
            ("free", free),
            ("total", total),
            ("upnl", upnl),
            ("currencies", currencies),
            ("info", bal),
        ]))
    }
    .await;
    match loaded {
        Ok(value) => Ok(value),
        Err(error) => {
            emitted(&h, &error)?;
            (h.capture)("this.store.balance")
        }
    }
}
pub fn margin_sum<V: Clone + 'static, E: 'static>(
    h: Rc<MarketAccountOps<V, E>>,
    rows: V,
    field: &'static str,
) -> Result<V, E> {
    let round = (h.capture)("roundUSD")?;
    let sum = (h.capture)("sumBy")?;
    let ops = h.clone();
    let value = (h.sum_by)(
        sum,
        rows,
        Some(Rc::new(move |p| {
            let markets = (ops.capture)("this.store.marketsById")?;
            let market = index(&ops, markets, (ops.get)(&p, "instId")?)?;
            call(&ops, "this.convertMarginValueToUsd", || {
                Ok(vec![
                    finite(&ops, || (ops.get)(&p, field))?,
                    (ops.get)(&p, "ccy")?,
                    market,
                    finite(&ops, || (ops.get)(&p, "markPx"))?,
                ])
            })
        })),
    )?;
    (h.invoke)(round, vec![value])
}
pub async fn fetch_leverage_batch<V: Clone + 'static, E: 'static>(
    h: Rc<MarketAccountOps<V, E>>,
    batch: V,
    counter: Rc<Cell<usize>>,
) -> Result<V, E> {
    if (h.truthy)(&(h.capture)("this.isDisposed")?) {
        return Ok((h.array)(vec![]));
    }
    let inner = counter.get();
    counter.set(inner + 1);
    let sleep = (h.capture)("sleep")?;
    let delay = (h.intrinsic)("multiply", vec![(h.number)(inner as f64), (h.number)(250.)])?;
    (h.await_value)((h.invoke)(sleep, vec![delay])?).await?;
    let get = (h.capture)("this.xhr.get:ENDPOINTS.LEVERAGE")?;
    let ops = h.clone();
    let ids = (h.map)(
        batch,
        Rc::new(move |symbol| {
            let markets = (ops.capture)("this.store.marketsBySymbol")?;
            let market = index(&ops, markets, symbol)?;
            (ops.get)(&market, "id")
        }),
    )?;
    let ids = (h.intrinsic)("join-comma", vec![ids])?;
    let params = (h.object)(vec![("instId", ids), ("mgnMode", (h.literal)("cross"))]);
    let response =
        (h.await_value)((h.invoke)(get, vec![(h.object)(vec![("params", params)])])?).await?;
    let data = (h.get)(&response, "data")?;
    (h.get)(&data, "data")
}
pub async fn fetch_leverages<V: Clone + 'static, E: 'static>(
    h: Rc<MarketAccountOps<V, E>>,
    symbols: V,
) -> Result<V, E> {
    let flatten = (h.capture)("flatten")?;
    let map = (h.capture)("p-iteration.map")?;
    let batches = call(&h, "chunk", || Ok(vec![symbols, (h.number)(20.)]))?;
    let counter = Rc::new(Cell::new(0));
    let ops = h.clone();
    let native = Rc::new(move |batch| {
        let ops = ops.clone();
        let counter = counter.clone();
        Box::pin(async move { fetch_leverage_batch(ops, batch, counter).await }) as ReadAwait<V, E>
    });
    let responses = (h.parallel_map)(map, batches, native).await?;
    let responses = (h.invoke)(flatten, vec![responses])?;
    let ops = h.clone();
    (h.reduce)(
        responses,
        (h.object)(vec![]),
        Rc::new(move |acc, row| {
            let markets = (ops.capture)("this.store.marketsById")?;
            let market = index(&ops, markets, (ops.get)(&row, "instId")?)?;
            let symbol = (ops.get)(&market, "symbol")?;
            let key = (ops.intrinsic)("to-property-key", vec![symbol])?;
            let value = float(&ops, || (ops.get)(&row, "lever"))?;
            put_index(&ops, acc.clone(), key, value)?;
            Ok(acc)
        }),
    )
}
