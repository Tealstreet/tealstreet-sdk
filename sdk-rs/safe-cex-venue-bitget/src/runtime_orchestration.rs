//! Runtime orchestration over native receiver/callee and owned callback contracts.
use crate::order_preparation::OrderPreparationHost;
use std::{cell::RefCell, rc::Rc};
pub trait RuntimeOrchestrationHost: OrderPreparationHost {
    /// Native one-element iterator destructuring, including IteratorClose.
    fn first(&self, value: Self::Value) -> Result<Self::Value, Self::Error>;
    /// Make an independently owned native callable; invoke preserves bare-call vs
    /// captured-member owners. Callback errors propagate at the original call site.
    fn function<F>(&self, body: F) -> Self::Value
    where
        F: Fn(Vec<Self::Value>) -> Result<Self::Value, Self::Error> + 'static;
}
fn s<H: RuntimeOrchestrationHost>(h: &H, v: &str) -> H::Value {
    h.string(&v.encode_utf16().collect::<Vec<_>>())
}
fn p<H: RuntimeOrchestrationHost>(h: &H, v: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    h.property(v, k)
}
fn opt<H: RuntimeOrchestrationHost>(h: &H, v: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    if h.nullish(v) {
        Ok(h.undefined())
    } else {
        p(h, v, k)
    }
}
fn call<H: RuntimeOrchestrationHost>(
    h: &H,
    k: &str,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    let f = h.capture(k)?;
    h.invoke(f, args)
}
async fn acall<H: RuntimeOrchestrationHost>(
    h: &H,
    k: &str,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    let f = h.capture(k)?;
    h.start(f, args)?.await
}
fn op<H: RuntimeOrchestrationHost>(
    h: &H,
    k: &str,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    h.operation(k, args)
}
fn message<H: RuntimeOrchestrationHost>(h: &H, e: &H::Error) -> Result<H::Value, H::Error> {
    let v = h.error_value(e);
    let msg = opt(h, &opt(h, &opt(h, &v, "response")?, "data")?, "msg")?;
    if h.truthy(&msg) {
        Ok(msg)
    } else {
        opt(h, &v, "message")
    }
}
fn spread<H: RuntimeOrchestrationHost>(h: &H, v: H::Value) -> Result<H::Value, H::Error> {
    op(h, "objectSpread", vec![v])
}
fn loaded<H: RuntimeOrchestrationHost>(
    h: &H,
    fields: Vec<(&str, H::Value)>,
) -> Result<H::Value, H::Error> {
    let v = spread(h, h.receiver("store.loaded")?)?;
    for (k, f) in fields {
        h.define(&v, k, f)?;
    }
    Ok(v)
}
pub async fn validate_account<H: RuntimeOrchestrationHost>(h: &H) -> Result<H::Value, H::Error> {
    let previous = h.receiver("_suppressBitgetValidationErrorEmission")?;
    h.set_receiver("_suppressBitgetValidationErrorEmission", h.boolean(true))?;
    let result = async {
        match async {
            acall(h, "ensureAccountTypeDetected", vec![]).await?;
            acall(h, "fetchBalance", vec![]).await
        }
        .await
        {
            Ok(balance) => Ok(h.object(vec![("error", s(h, "")), ("balance", balance)])),
            Err(e) => {
                let f = h.capture("getBitgetApiErrorMessage")?;
                let v = h.error_value(&e);
                let data = opt(h, &opt(h, &v, "response")?, "data")?;
                let msg = opt(h, &v, "message")?;
                let msg = if h.truthy(&msg) {
                    msg
                } else {
                    s(h, "Invalid API key or secret")
                };
                let msg = h.invoke(f, vec![data, msg])?;
                if h.truthy(&call(h, "isBitgetEnvironmentMismatch", vec![msg.clone()])?) {
                    let current = if h.truthy(&h.receiver("options.testnet")?) {
                        s(h, "Demo/Testnet")
                    } else {
                        s(h, "Live/Production")
                    };
                    let suggested = if h.truthy(&h.receiver("options.testnet")?) {
                        s(h, "Live/Production")
                    } else {
                        s(h, "Demo/Testnet")
                    };
                    let msg = op(h, "environmentMessage", vec![suggested, current])?;
                    Ok(h.object(vec![("error", msg)]))
                } else {
                    Ok(h.object(vec![(
                        "error",
                        if h.truthy(&msg) {
                            msg
                        } else {
                            s(h, "Invalid API key or secret")
                        },
                    )]))
                }
            }
        }
    }
    .await;
    h.set_receiver("_suppressBitgetValidationErrorEmission", previous)?;
    result
}
pub async fn start<H: RuntimeOrchestrationHost>(h: &H) -> Result<H::Value, H::Error> {
    let external = acall(h, "initExternalPublicData", vec![]).await?;
    if !h.truthy(&external) {
        let markets = acall(h, "fetchMarkets", vec![]).await?;
        if h.truthy(&h.receiver("isDisposed")?) {
            return Ok(h.undefined());
        }
        let update = h.capture("store.update")?;
        let data = h.object(vec![
            ("markets", markets.clone()),
            ("loaded", loaded(h, vec![("markets", h.boolean(true))])?),
        ]);
        h.invoke(update, vec![data])?;
        call(h, "publicWebsocket.connectAndSubscribe", vec![])?;
        let tickers = acall(h, "fetchTickers", vec![]).await?;
        if h.truthy(&h.receiver("isDisposed")?) {
            return Ok(h.undefined());
        }
        let log = h.capture("log")?;
        let min = h.capture("Math.min")?;
        let min = h.invoke(
            min,
            vec![p(h, &tickers, "length")?, p(h, &markets, "length")?],
        )?;
        h.invoke(log, vec![op(h, "marketsLoadedMessage", vec![min])?])?;
        let update = h.capture("store.update")?;
        let data = h.object(vec![
            ("tickers", tickers),
            ("loaded", loaded(h, vec![("tickers", h.boolean(true))])?),
        ]);
        h.invoke(update, vec![data])?;
        call(h, "secondsTickersWebsocket.connectAndSubscribe", vec![])?;
        call(h, "thirdTickersWebsocket.connectAndSubscribe", vec![])?;
    }
    if h.truthy(&h.receiver("isPublicDataOnly")?) {
        let update = h.capture("store.update")?;
        let data = h.object(vec![(
            "loaded",
            loaded(
                h,
                vec![
                    ("balance", h.boolean(true)),
                    ("positions", h.boolean(true)),
                    ("orders", h.boolean(true)),
                ],
            )?,
        )]);
        h.invoke(update, vec![data])?;
        return Ok(h.undefined());
    }
    acall(h, "ensureAccountTypeDetected", vec![]).await?;
    if h.truthy(&h.receiver("isDisposed")?) {
        return Ok(h.undefined());
    }
    call(h, "privateWebsocket.connectAndSubscribe", vec![])?;
    acall(h, "tick", vec![]).await?;
    if h.truthy(&h.receiver("isDisposed")?) {
        return Ok(h.undefined());
    }
    call(h, "log", vec![s(h, "Ready to trade on Bitget")])?;
    let orders = acall(h, "fetchOrders", vec![]).await?;
    if h.truthy(&h.receiver("isDisposed")?) {
        return Ok(h.undefined());
    }
    call(h, "log", vec![s(h, "Loaded Bitget orders")])?;
    let update = h.capture("store.update")?;
    let data = h.object(vec![
        ("orders", orders),
        ("loaded", loaded(h, vec![("orders", h.boolean(true))])?),
    ]);
    h.invoke(update, vec![data])?;
    Ok(h.undefined())
}
pub async fn tick<H: RuntimeOrchestrationHost>(
    h: &H,
    count: H::Value,
) -> Result<H::Value, H::Error> {
    let count = if h.equal(&count, &h.undefined()) {
        h.number(0.)
    } else {
        count
    };
    if !h.truthy(&h.receiver("isDisposed")?) {
        if h.truthy(&h.receiver("isPublicDataOnly")?) {
            return Ok(h.undefined());
        }
        let result = async {
            let positions = acall(h, "fetchPositions", vec![]).await?;
            if h.truthy(&h.receiver("isDisposed")?) {
                return Ok(true);
            }
            call(h, "store.addOrUpdatePositionsFull", vec![positions])?;
            let mut balance = h.undefined();
            if h.equal(
                &op(h, "remainder", vec![count.clone(), h.number(2.)])?,
                &h.number(0.),
            ) {
                balance = acall(h, "fetchBalance", vec![]).await?;
                if h.truthy(&h.receiver("isDisposed")?) {
                    return Ok(true);
                }
            }
            let mut orders = h.undefined();
            if h.equal(
                &op(h, "remainder", vec![count.clone(), h.number(4.)])?,
                &h.number(0.),
            ) {
                let normal = acall(h, "fetchNormalOrders", vec![]).await?;
                let f = h.capture_member(&normal, "filter")?;
                let virtuals = h.filter(f, normal, |v| {
                    Ok(
                        h.equal(&p(h, &v, "type")?, &h.receiver("OrderType.StopLoss")?)
                            || h.equal(&p(h, &v, "type")?, &h.receiver("OrderType.TakeProfit")?),
                    )
                })?;
                let existing = h.receiver("store.orders")?;
                let f = h.capture_member(&existing, "filter")?;
                let existing = h.filter(f, existing, |v| {
                    for end in ["__stop_loss", "__take_profit"] {
                        let id = p(h, &v, "id")?;
                        let f = h.capture_member(&id, "endsWith")?;
                        if h.truthy(&h.invoke(f, vec![s(h, end)])?) {
                            return Ok(false);
                        }
                    }
                    Ok(true)
                })?;
                orders = h.array(vec![]);
                h.spread_into(&orders, existing)?;
                h.spread_into(&orders, virtuals)?;
            }
            let update = h.capture("store.update")?;
            let mut fields = vec![];
            if h.truthy(&balance) {
                fields.push(("balance", balance.clone()));
            }
            if h.truthy(&orders) {
                fields.push(("orders", orders.clone()));
            }
            let flags = spread(h, h.receiver("store.loaded")?)?;
            if h.truthy(&balance) {
                h.define(&flags, "balance", h.boolean(true))?;
            }
            h.define(&flags, "positions", h.boolean(true))?;
            if h.truthy(&orders) {
                h.define(&flags, "orders", h.boolean(true))?;
            }
            fields.push(("loaded", flags));
            h.invoke(update, vec![h.object(fields)])?;
            Ok::<bool, H::Error>(false)
        }
        .await;
        if matches!(result, Ok(true)) {
            return Ok(h.undefined());
        }
        if let Err(e) = result {
            let f = h.capture("emitAndLogError")?;
            h.invoke(f, vec![message(h, &e)?])?;
        }
        if !h.truthy(&h.receiver("isDisposed")?) {
            let f = h.capture("loop")?;
            let host = h.clone();
            let count = count.clone();
            let callback = h.function(move |_| {
                let f = host.capture("tick")?;
                let n = op(&host, "add", vec![count.clone(), host.number(1.)])?;
                host.invoke(f, vec![n])
            });
            let interval = call(h, "getTickInterval", vec![])?;
            h.invoke(f, vec![callback, interval])?;
        }
    }
    Ok(h.undefined())
}
pub async fn detect_account_type<H: RuntimeOrchestrationHost>(h: &H) -> Result<H::Value, H::Error> {
    let mut propagate = false;
    let mut detected = false;
    let result = async {
        let probe = async {
            let f = h.capture("xhr.get")?;
            let response = h
                .start(f, vec![h.receiver("ENDPOINTS.ACCOUNT_SETTINGS_V3")?])?
                .await?;
            let data = p(h, &response, "data")?;
            let code = opt(h, &data, "code")?;
            let code = op(
                h,
                "template",
                vec![if h.nullish(&code) { s(h, "") } else { code }],
            )?;
            let success = h.capture("isBitgetSuccessCode")?;
            if h.truthy(&h.invoke(success, vec![opt(h, &data, "code")?])?) {
                h.set_receiver("isUta", h.boolean(true))?;
                detected = true;
            } else if h.equal(&code, &s(h, "40084")) {
                h.set_receiver("isUta", h.boolean(false))?;
                detected = true;
            } else {
                propagate = true;
                let ctor = h.capture("Error")?;
                let msg = call(
                    h,
                    "getBitgetApiErrorMessage",
                    vec![data, s(h, "Bitget account type probe failed")],
                )?;
                return Err(h.new_error(ctor, msg)?);
            }
            Ok::<(), H::Error>(())
        }
        .await;
        if let Err(e) = probe {
            if propagate {
                return Err(e);
            }
            let v = h.error_value(&e);
            let data = opt(h, &opt(h, &v, "response")?, "data")?;
            let code = opt(h, &data, "code")?;
            let code = op(
                h,
                "template",
                vec![if h.nullish(&code) { s(h, "") } else { code }],
            )?;
            if h.equal(&code, &s(h, "40084")) {
                h.set_receiver("isUta", h.boolean(false))?;
                detected = true;
            } else if h.truthy(&opt(h, &opt(h, &opt(h, &v, "response")?, "data")?, "code")?)
                || h.truthy(&call(
                    h,
                    "isBitgetEnvironmentMismatch",
                    vec![opt(h, &opt(h, &opt(h, &v, "response")?, "data")?, "msg")?],
                )?)
            {
                return Err(e);
            } else {
                let log = h.capture("log")?;
                let msg = opt(h, &v, "message")?;
                let msg = if h.nullish(&msg) { v } else { msg };
                h.invoke(log, vec![op(h, "probeFallbackMessage", vec![msg])?])?;
                detected = true;
            }
        }
        Ok(h.undefined())
    }
    .await;
    h.set_receiver("_accountTypeDetected", h.boolean(detected))?;
    result
}
pub async fn ensure_account_type_detected<H: RuntimeOrchestrationHost>(
    h: &H,
) -> Result<H::Value, H::Error> {
    if h.truthy(&h.receiver("isPublicDataOnly")?) || h.truthy(&h.receiver("_accountTypeDetected")?)
    {
        return Ok(h.undefined());
    }
    if !h.truthy(&h.receiver("_accountTypeDetectionInflight")?) {
        let f = h.capture("_detectAccountType")?;
        let promise = h.invoke(f, vec![])?;
        let finally = h.capture_member(&promise, "finally")?;
        let host = h.clone();
        let callback = h.function(move |_| {
            host.set_receiver("_accountTypeDetectionInflight", host.null())?;
            Ok(host.undefined())
        });
        let promise = h.invoke(finally, vec![callback])?;
        h.set_receiver("_accountTypeDetectionInflight", promise)?;
    }
    h.await_value(h.receiver("_accountTypeDetectionInflight")?)
        .await
}
pub async fn resolve_symbol_positioning<H: RuntimeOrchestrationHost>(
    h: &H,
    symbol: H::Value,
) -> Result<H::Value, H::Error> {
    let f = h.capture("getOrFetchPositionsForSymbol")?;
    let positions = h.start(f, vec![symbol.clone()])?.await?;
    let first = h.key_get(&positions, &h.number(0.))?;
    if h.truthy(&opt(h, &first, "positionMode")?) && h.truthy(&opt(h, &first, "marginType")?) {
        return Ok(h.object(vec![
            ("positionMode", p(h, &first, "positionMode")?),
            ("marginType", p(h, &first, "marginType")?),
        ]));
    }
    let cached = h.key_get(&h.receiver("_positioningCache")?, &symbol)?;
    if h.truthy(&cached) {
        let now = call(h, "Date.now", vec![])?;
        let elapsed = op(h, "subtract", vec![now, p(h, &cached, "at")?])?;
        if h.truthy(&op(
            h,
            "less",
            vec![
                elapsed,
                h.receiver("BitgetExchange.POSITIONING_CACHE_TTL_MS")?,
            ],
        )?) {
            return p(h, &cached, "value");
        }
    }
    let inflight = h.key_get(&h.receiver("_positioningInflight")?, &symbol)?;
    if h.truthy(&inflight) {
        return h.await_value(inflight).await;
    }
    let host = h.clone();
    let sym = symbol.clone();
    let promise = h.launch(Box::pin(async move {
        let h = &host;
        let f = h.capture("fetchPositionMetadata")?;
        let data = h.start(f, vec![h.array(vec![sym.clone()])])?.await?;
        let metadata = h.first(data)?;
        let value = h.object(vec![
            ("positionMode", opt(h, &metadata, "positionMode")?),
            ("marginType", opt(h, &metadata, "marginType")?),
        ]);
        if h.truthy(&p(h, &value, "positionMode")?) && h.truthy(&p(h, &value, "marginType")?) {
            let cache = h.receiver("_positioningCache")?;
            let at = call(h, "Date.now", vec![])?;
            h.assign_key(
                &cache,
                &sym,
                h.object(vec![("at", at), ("value", value.clone())]),
            )?;
        }
        Ok(value)
    }))?;
    h.assign_key(
        &h.receiver("_positioningInflight")?,
        &symbol,
        promise.clone(),
    )?;
    let result = h.await_value(promise).await;
    h.delete(&h.receiver("_positioningInflight")?, &symbol)?;
    result
}
pub async fn fetch_income_history_range<H: RuntimeOrchestrationHost>(
    h: &H,
    start: H::Value,
    end: H::Value,
    income_type: H::Value,
) -> Result<H::Value, H::Error> {
    let f = h.capture("paginateIncomeRange")?;
    let host = h.clone();
    let page = h.function(move |args| {
        let req = &args[0];
        let from = p(&host, req, "startTime")?;
        let to = p(&host, req, "endTime")?;
        let limit = p(&host, req, "limit")?;
        let f = host.capture("_fetchBillPage")?;
        host.invoke(
            f,
            vec![host.object(vec![("startTime", from), ("endTime", to), ("limit", limit)])],
        )
    });
    let options = h.object(vec![
        ("startTime", start.clone()),
        ("endTime", end.clone()),
        ("chunkMs", h.receiver("BILL_MAX_WINDOW_MS")?),
        ("pageLimit", h.receiver("BILL_PAGE_LIMIT")?),
    ]);
    let data = h.start(f, vec![page, options])?.await?;
    let records = p(h, &data, "records")?;
    let truncated = p(h, &data, "truncated")?;
    let exhausted = p(h, &data, "exhaustedChunks")?;
    if h.truthy(&truncated) {
        let f = h.capture("log")?;
        let from = op(h, "newDateISO", vec![start])?;
        let to = op(h, "newDateISO", vec![end])?;
        let msg = op(h, "incomeWarning", vec![from, to, exhausted])?;
        let warning = h.receiver("LogSeverity.Warning")?;
        h.invoke(f, vec![msg, warning])?;
    }
    if h.truthy(&income_type) {
        let f = h.capture_member(&records, "filter")?;
        h.filter(f, records, |v| {
            Ok(h.equal(&p(h, &v, "incomeType")?, &income_type))
        })
    } else {
        Ok(records)
    }
}
struct Listen<V> {
    active: bool,
    subscribed: bool,
    timeout: V,
    parser: V,
    book: V,
    topic: V,
    channel: V,
    symbol: V,
    opts: V,
    interval: V,
}
fn first_row<H: RuntimeOrchestrationHost>(h: &H, data: &H::Value) -> Result<H::Value, H::Error> {
    let f = h.capture("Array.isArray")?;
    if h.truthy(&h.invoke(f, vec![opt(h, data, "data")?])?) {
        h.key_get(&p(h, data, "data")?, &h.number(0.))
    } else {
        Ok(h.undefined())
    }
}
fn parse<H: RuntimeOrchestrationHost>(
    h: &H,
    data: H::Value,
    callback: H::Value,
    book: H::Value,
    is_book: bool,
) -> Result<H::Value, H::Error> {
    let row = first_row(h, &data)?;
    if !is_book {
        let f = h.capture("Array.isArray")?;
        if !h.truthy(&h.invoke(f, vec![row.clone()])?) {
            return Ok(h.undefined());
        }
        let f = h.capture("parseInt")?;
        let timestamp = h.invoke(f, vec![h.key_get(&row, &h.number(0.))?, h.number(10.)])?;
        let timestamp = op(h, "divide", vec![timestamp, h.number(1000.)])?;
        let mut fields = vec![("timestamp", timestamp)];
        for (k, i) in [
            ("open", 1.),
            ("high", 2.),
            ("low", 3.),
            ("close", 4.),
            ("volume", 6.),
        ] {
            let f = h.capture("parseFloat")?;
            fields.push((k, h.invoke(f, vec![h.key_get(&row, &h.number(i))?])?));
        }
        h.invoke(callback, vec![h.object(fields)])?;
        return Ok(h.undefined());
    }
    if !h.truthy(&row) || !h.truthy(&op(h, "typeofObject", vec![row.clone()])?) {
        return Ok(h.undefined());
    }
    if h.equal(&p(h, &data, "action")?, &s(h, "snapshot")) {
        h.assign(&book, "bids", h.array(vec![]))?;
        h.assign(&book, "asks", h.array(vec![]))?;
        for k in ["bids", "asks"] {
            let rows = p(h, &row, k)?;
            if h.truthy(&rows) {
                let mut i = h.number(0.);
                while h.truthy(&op(h, "less", vec![i.clone(), p(h, &rows, "length")?])?) {
                    let v = h.key_get(&rows, &i)?;
                    let array = p(h, &book, k)?;
                    let push = h.capture_member(&array, "push")?;
                    let f = h.capture("parseFloat")?;
                    let price = h.invoke(f, vec![h.key_get(&v, &h.number(0.))?])?;
                    let f = h.capture("parseFloat")?;
                    let amount = h.invoke(f, vec![h.key_get(&v, &h.number(1.))?])?;
                    h.invoke(
                        push,
                        vec![h.object(vec![
                            ("price", price),
                            ("amount", amount),
                            ("total", h.number(0.)),
                        ])],
                    )?;
                    i = op(h, "increment", vec![i])?;
                }
            }
        }
    }
    if h.equal(&p(h, &data, "action")?, &s(h, "update")) {
        for k in ["bids", "asks"] {
            let rows = p(h, &row, k)?;
            if h.truthy(&rows) {
                let mut i = h.number(0.);
                while h.truthy(&op(h, "less", vec![i.clone(), p(h, &rows, "length")?])?) {
                    let v = h.key_get(&rows, &i)?;
                    let update = h.capture("updateOrderBook")?;
                    let f = h.capture("parseFloat")?;
                    let price = h.invoke(f, vec![h.key_get(&v, &h.number(0.))?])?;
                    let f = h.capture("parseFloat")?;
                    let amount = h.invoke(f, vec![h.key_get(&v, &h.number(1.))?])?;
                    h.invoke(
                        update,
                        vec![
                            book.clone(),
                            s(h, k),
                            h.object(vec![
                                ("price", price),
                                ("amount", amount),
                                ("total", h.number(0.)),
                            ]),
                        ],
                    )?;
                    i = op(h, "increment", vec![i])?;
                }
            }
        }
    }
    call(h, "calcOrderBookTotal", vec![book.clone()])?;
    h.invoke(callback, vec![book])?;
    Ok(h.undefined())
}
fn uppercase<H: RuntimeOrchestrationHost>(h: &H) -> Result<H::Value, H::Error> {
    let v = h.receiver("parent.apiProductType")?;
    let f = h.capture_member(&v, "toUpperCase")?;
    h.invoke(f, vec![])
}
fn payload<H: RuntimeOrchestrationHost>(
    h: &H,
    kind: &str,
    channel: H::Value,
    symbol: impl FnOnce() -> Result<H::Value, H::Error>,
) -> Result<H::Value, H::Error> {
    Ok(h.object(vec![
        ("op", s(h, kind)),
        (
            "args",
            h.array(vec![h.object(vec![
                ("instType", uppercase(h)?),
                ("channel", channel),
                ("instId", symbol()?),
            ])]),
        ),
    ]))
}
fn send<H: RuntimeOrchestrationHost>(h: &H, payload: H::Value) -> Result<(), H::Error> {
    let f = h.capture("send")?;
    let json = h.capture("JSON.stringify")?;
    let json = h.invoke(json, vec![payload])?;
    h.invoke(f, vec![json])?;
    Ok(())
}
fn listen_symbol<H: RuntimeOrchestrationHost>(
    h: &H,
    state: &Rc<RefCell<Listen<H::Value>>>,
    book: bool,
) -> Result<H::Value, H::Error> {
    if book {
        Ok(state.borrow().symbol.clone())
    } else {
        let opts = state.borrow().opts.clone();
        p(h, &opts, "symbol")
    }
}
fn wait<H: RuntimeOrchestrationHost>(
    h: &H,
    state: Rc<RefCell<Listen<H::Value>>>,
    book: bool,
) -> Result<H::Value, H::Error> {
    let active = state.borrow().active;
    if !active || h.truthy(&h.receiver("isDisposed")?) {
        return Ok(h.undefined());
    }
    if h.truthy(&h.receiver("isConnected")?) {
        let topic = state.borrow().topic.clone();
        let map = h.receiver("messageHandlers")?;
        let old = h.key_get(&h.receiver("messageHandlers")?, &topic)?;
        let old = if h.truthy(&old) { old } else { h.array(vec![]) };
        let handlers = h.array(vec![]);
        h.spread_into(&handlers, old)?;
        let parser = state.borrow().parser.clone();
        op(h, "arrayAppendOwn", vec![handlers.clone(), parser])?;
        h.assign_key(&map, &topic, handlers)?;
        let channel = state.borrow().channel.clone();
        let packet = payload(h, "subscribe", channel.clone(), || {
            listen_symbol(h, &state, book)
        })?;
        if !h.truthy(&h.key_get(&h.receiver("topics")?, &channel)?) {
            h.assign_key(&h.receiver("topics")?, &channel, h.object(vec![]))?;
        }
        let channels = h.key_get(&h.receiver("topics")?, &channel)?;
        let symbol = listen_symbol(h, &state, book)?;
        let source = h.key_get(&h.receiver("topics")?, &channel)?;
        let current = h.key_get(&source, &listen_symbol(h, &state, book)?)?;
        let current = if h.truthy(&current) {
            current
        } else {
            h.number(0.)
        };
        h.assign_key(
            &channels,
            &symbol,
            op(h, "add", vec![current, h.number(1.)])?,
        )?;
        state.borrow_mut().subscribed = true;
        send(h, packet)?;
    } else {
        let f = h.capture("setTimeout")?;
        let host = h.clone();
        let next = state.clone();
        let cb = h.function(move |_| wait(&host, next.clone(), book));
        let id = h.invoke(f, vec![cb, h.number(100.)])?;
        state.borrow_mut().timeout = id;
    }
    Ok(h.undefined())
}
fn cleanup<H: RuntimeOrchestrationHost>(
    h: &H,
    state: Rc<RefCell<Listen<H::Value>>>,
    book: bool,
) -> Result<H::Value, H::Error> {
    if !state.borrow().active {
        return Ok(h.undefined());
    }
    state.borrow_mut().active = false;
    if !book && h.truthy(&state.borrow().timeout) {
        call(h, "clearTimeout", vec![state.borrow().timeout.clone()])?;
        state.borrow_mut().timeout = h.null();
    }
    let topic = state.borrow().topic.clone();
    let map = h.receiver("messageHandlers")?;
    let current = h.key_get(&h.receiver("messageHandlers")?, &topic)?;
    let current = if h.truthy(&current) {
        current
    } else {
        h.array(vec![])
    };
    let f = h.capture_member(&current, "filter")?;
    let parser = state.borrow().parser.clone();
    let filtered = h.filter(f, current, |v| Ok(!h.equal(&v, &parser)))?;
    let handlers = h.array(vec![]);
    h.spread_into(&handlers, filtered)?;
    h.assign_key(&map, &topic, handlers)?;
    if book {
        let value = state.borrow().book.clone();
        h.assign(&value, "asks", h.array(vec![]))?;
        h.assign(&value, "bids", h.array(vec![]))?;
        if h.truthy(&state.borrow().timeout) {
            call(h, "clearTimeout", vec![state.borrow().timeout.clone()])?;
            state.borrow_mut().timeout = h.null();
        }
    }
    if !state.borrow().subscribed {
        return Ok(h.undefined());
    }
    state.borrow_mut().subscribed = false;
    let channel = state.borrow().channel.clone();
    let channels = h.key_get(&h.receiver("topics")?, &channel)?;
    let count = if h.nullish(&channels) {
        h.undefined()
    } else {
        h.key_get(&channels, &listen_symbol(h, &state, book)?)?
    };
    let count = if h.truthy(&count) {
        count
    } else {
        h.number(0.)
    };
    let next = op(h, "subtract", vec![count, h.number(1.)])?;
    if h.truthy(&op(h, "greater", vec![next.clone(), h.number(0.)])?) {
        let channels = h.key_get(&h.receiver("topics")?, &channel)?;
        h.assign_key(&channels, &listen_symbol(h, &state, book)?, next.clone())?;
    } else {
        let channels = h.key_get(&h.receiver("topics")?, &channel)?;
        if !h.nullish(&channels) {
            h.delete(&channels, &listen_symbol(h, &state, book)?)?;
        }
        let keys = h.capture("Object.keys")?;
        let channels = h.key_get(&h.receiver("topics")?, &channel)?;
        let channels = if h.truthy(&channels) {
            channels
        } else {
            h.object(vec![])
        };
        let keys = h.invoke(keys, vec![channels])?;
        if h.equal(&p(h, &keys, "length")?, &h.number(0.)) {
            h.delete(&h.receiver("topics")?, &channel)?;
        }
    }
    if h.truthy(&h.receiver("isConnected")?)
        && h.truthy(&op(h, "lessEqual", vec![next, h.number(0.)])?)
    {
        let channel = state.borrow().channel.clone();
        if book {
            let packet = payload(h, "unsubscribe", channel, || listen_symbol(h, &state, book))?;
            call(h, "cleanupMessageHandlerStats", vec![topic])?;
            send(h, packet)?;
        } else {
            let f = h.capture("send")?;
            let json = h.capture("JSON.stringify")?;
            let interval = state.borrow().interval.clone();
            let channel = op(h, "candleChannel", vec![interval])?;
            let packet = payload(h, "unsubscribe", channel, || listen_symbol(h, &state, book))?;
            let json = h.invoke(json, vec![packet])?;
            h.invoke(f, vec![json])?;
            call(h, "cleanupMessageHandlerStats", vec![topic])?;
        }
    }
    Ok(h.undefined())
}
fn listen<H: RuntimeOrchestrationHost>(
    h: &H,
    opts: H::Value,
    callback: H::Value,
    book: bool,
) -> Result<H::Value, H::Error> {
    let (symbol, channel, topic, interval) = if book {
        let symbol = opts.clone();
        let topic = op(h, "bookTopic", vec![symbol.clone()])?;
        (symbol, s(h, "books"), topic, h.undefined())
    } else {
        let interval = h.key_get(&h.receiver("INTERVAL")?, &p(h, &opts, "interval")?)?;
        let topic = op(
            h,
            "candleTopic",
            vec![p(h, &opts, "symbol")?, interval.clone()],
        )?;
        let channel = op(h, "candleChannel", vec![interval.clone()])?;
        (h.undefined(), channel, topic, interval)
    };
    let value = if book {
        h.object(vec![("bids", h.array(vec![])), ("asks", h.array(vec![]))])
    } else {
        h.undefined()
    };
    let host = h.clone();
    let b = value.clone();
    let parser =
        h.function(move |args| parse(&host, args[0].clone(), callback.clone(), b.clone(), book));
    let state = Rc::new(RefCell::new(Listen {
        active: true,
        subscribed: false,
        timeout: h.null(),
        parser,
        book: value,
        topic,
        channel,
        symbol,
        opts,
        interval,
    }));
    wait(h, state.clone(), book)?;
    let host = h.clone();
    Ok(h.function(move |_| cleanup(&host, state.clone(), book)))
}
pub fn listen_ohlcv<H: RuntimeOrchestrationHost>(
    h: &H,
    opts: H::Value,
    callback: H::Value,
) -> Result<H::Value, H::Error> {
    listen(h, opts, callback, false)
}
pub fn listen_order_book<H: RuntimeOrchestrationHost>(
    h: &H,
    symbol: H::Value,
    callback: H::Value,
) -> Result<H::Value, H::Error> {
    listen(h, symbol, callback, true)
}
