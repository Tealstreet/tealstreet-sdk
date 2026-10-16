//! Exchange lifecycle and position bodies over current native source bindings.
use crate::collection_reads::{Callback, Op, ReadHost};
use std::rc::Rc;
pub trait LifecycleHost: ReadHost {
    fn builtin(&self, name: &'static str) -> Result<Self::Value, Self::Error>;
    fn dynamic(&self, value: Self::Value, key: Self::Value) -> Result<Self::Value, Self::Error>;
    fn set_add(&self, set: Self::Value, value: Self::Value) -> Result<(), Self::Error>;
    fn set_clear(&self, set: Self::Value) -> Result<(), Self::Error>;
    fn set_values(&self, set: Self::Value) -> Result<Self::Value, Self::Error>;
    fn set_size(&self, set: Self::Value) -> Result<Self::Value, Self::Error>;
    fn find(
        &self,
        array: Self::Value,
        predicate: impl FnMut(Self::Value) -> Result<bool, Self::Error>,
    ) -> Result<Self::Value, Self::Error>;
    fn super_dispose(&self, receiver: Self::Value) -> Result<Self::Value, Self::Error>;
}
fn get<H: LifecycleHost>(h: &H, v: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    h.get(v, k)
}
fn opt<H: LifecycleHost>(h: &H, v: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    if h.nullish(v) {
        Ok(h.undefined())
    } else {
        get(h, v, k)
    }
}
fn method<H: LifecycleHost>(h: &H, r: H::Value, k: &str) -> Result<(H::Value, H::Value), H::Error> {
    Ok((get(h, &r, k)?, r))
}
fn invoke<H: LifecycleHost>(
    h: &H,
    p: (H::Value, H::Value),
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    h.call(p.0, p.1, args)
}
fn call<H: LifecycleHost>(
    h: &H,
    r: H::Value,
    k: &str,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    invoke(h, method(h, r, k)?, args)
}
fn ignored<H: LifecycleHost>(h: &H, v: H::Value) {
    h.observe_detached(v)
}
fn loaded<H: LifecycleHost>(
    h: &H,
    r: &H::Value,
    fields: &[&'static str],
) -> Result<H::Value, H::Error> {
    let out = h.object(vec![]);
    h.assign_spread(out.clone(), get(h, &get(h, r, "store")?, "loaded")?)?;
    for f in fields {
        h.set(&out, f, h.boolean(true))?;
    }
    Ok(out)
}
pub fn dispose<H: LifecycleHost>(h: &H, r: H::Value) -> Result<H::Value, H::Error> {
    if h.truthy(&get(h, &r, "fastPrivateRefreshTimeoutId")?) {
        let f = h.builtin("clearTimeout")?;
        h.call(
            f,
            h.undefined(),
            vec![get(h, &r, "fastPrivateRefreshTimeoutId")?],
        )?;
        h.set(&r, "fastPrivateRefreshTimeoutId", h.undefined())?;
    }
    let p = method(h, get(h, &r, "emitter")?, "off")?;
    invoke(
        h,
        p,
        vec![h.text("fill"), get(h, &r, "handleFillForFastRefresh")?],
    )?;
    ignored(h, h.super_dispose(r.clone())?);
    ignored(
        h,
        call(h, get(h, &r, "publicWebsocket")?, "dispose", vec![])?,
    );
    ignored(
        h,
        call(h, get(h, &r, "privateWebsocket")?, "dispose", vec![])?,
    );
    Ok(h.undefined())
}
pub fn queue_refresh<H: LifecycleHost>(
    h: &H,
    r: H::Value,
    symbol: H::Value,
) -> Result<H::Value, H::Error> {
    if h.truthy(&symbol) {
        h.set_add(get(h, &r, "pendingOrderRefreshSymbols")?, symbol)?;
    }
    if h.truthy(&get(h, &r, "fastPrivateRefreshTimeoutId")?) {
        let f = h.builtin("clearTimeout")?;
        h.call(
            f,
            h.undefined(),
            vec![get(h, &r, "fastPrivateRefreshTimeoutId")?],
        )?;
    }
    let timeout = h.builtin("setTimeout")?;
    let owner = h.clone();
    let receiver = r.clone();
    let callback = h.function(Rc::new(move |_| {
        owner.set(&receiver, "fastPrivateRefreshTimeoutId", owner.undefined())?;
        let value = call(&owner, receiver.clone(), "runFastPrivateRefresh", vec![])?;
        ignored(&owner, value);
        Ok(owner.undefined())
    }) as Callback<H>);
    let id = h.call(timeout, h.undefined(), vec![callback, h.number(350.0)])?;
    h.set(&r, "fastPrivateRefreshTimeoutId", id)?;
    Ok(h.undefined())
}
fn refresh_failure<H: LifecycleHost>(
    h: &H,
    r: H::Value,
    e: H::Error,
) -> Result<H::Value, H::Error> {
    let p = method(h, r, "emitAndLogError")?;
    let e = h.error_value(&e);
    let msg = opt(h, &opt(h, &opt(h, &e, "response")?, "data")?, "msg")?;
    let reason = if h.truthy(&msg) {
        msg
    } else {
        opt(h, &e, "message")?
    };
    let value = invoke(h, p, vec![reason])?;
    ignored(h, value);
    Ok(h.undefined())
}
pub async fn run_refresh<H: LifecycleHost>(h: &H, r: H::Value) -> Result<H::Value, H::Error> {
    if h.truthy(&get(h, &r, "isDisposed")?)
        || h.truthy(&get(h, &r, "isPublicDataOnly")?)
        || h.truthy(&get(h, &r, "isFastPrivateRefreshRunning")?)
    {
        return Ok(h.undefined());
    }
    h.set(&r, "isFastPrivateRefreshRunning", h.boolean(true))?;
    let result = async {
        let p = call(h, r.clone(), "fetchBalanceAndPositions", vec![])?;
        let value = h.await_value(p).await?;
        let balance = get(h, &value, "balance")?;
        let positions = get(h, &value, "positions")?;
        if h.truthy(&get(h, &r, "isDisposed")?) {
            return Ok(h.undefined());
        }
        ignored(
            h,
            call(
                h,
                get(h, &r, "store")?,
                "addOrUpdatePositionsFull",
                vec![positions],
            )?,
        );
        let p = method(h, get(h, &r, "store")?, "update")?;
        let patch = h.object(vec![
            ("balance", balance),
            ("loaded", loaded(h, &r, &["balance", "positions"])?),
        ]);
        ignored(h, invoke(h, p, vec![patch])?);
        let symbols = h.set_values(get(h, &r, "pendingOrderRefreshSymbols")?)?;
        h.set_clear(get(h, &r, "pendingOrderRefreshSymbols")?)?;
        if h.truthy(&get(h, &symbols, "length")?) {
            let p = call(h, r.clone(), "fetchAndSetOrders", vec![symbols])?;
            h.await_value(p).await?;
        }
        Ok(h.undefined())
    }
    .await;
    let result = match result {
        Ok(v) => Ok(v),
        Err(e) => refresh_failure(h, r.clone(), e),
    };
    h.set(&r, "isFastPrivateRefreshRunning", h.boolean(false))?;
    if h.truthy(&h.set_size(get(h, &r, "pendingOrderRefreshSymbols")?)?) {
        ignored(h, call(h, r, "queueFastPrivateRefresh", vec![])?);
    }
    result
}
pub async fn validate<H: LifecycleHost>(h: &H, r: H::Value) -> Result<H::Value, H::Error> {
    let endpoint = crate::types::ENDPOINTS
        .iter()
        .find(|(k, _)| *k == "ACCOUNT")
        .expect("account endpoint")
        .1;
    let p = call(h, get(h, &r, "xhr")?, "get", vec![h.text(endpoint)])?;
    let response = h.await_value(p).await?;
    let data = get(h, &response, "data")?;
    let p = method(h, r.clone(), "isSuccessCode")?;
    if !h.truthy(&invoke(h, p, vec![opt(h, &data, "code")?])?) {
        let msg = opt(h, &data, "msg")?;
        return Ok(h.object(vec![(
            "error",
            if h.truthy(&msg) {
                msg
            } else {
                h.text("Invalid API Key or Secret")
            },
        )]));
    }
    let p = call(h, r, "fetchBalanceAndPositions", vec![])?;
    let owner = h.clone();
    let then = call(
        h,
        p,
        "then",
        vec![h.function(Rc::new(move |args| {
            get(
                &owner,
                &args.first().cloned().unwrap_or_else(|| owner.undefined()),
                "balance",
            )
        }))],
    )?;
    let owner = h.clone();
    let p = call(
        h,
        then,
        "catch",
        vec![h.function(Rc::new(move |_| Ok(owner.undefined())))],
    )?;
    let balance = h.await_value(p).await?;
    Ok(h.object(vec![("error", h.text("")), ("balance", balance)]))
}
pub async fn start<H: LifecycleHost>(h: &H, r: H::Value) -> Result<H::Value, H::Error> {
    let p = call(h, r.clone(), "initExternalPublicData", vec![])?;
    let external = h.await_value(p).await?;
    let public = get(h, &r, "isPublicDataOnly")?;
    if !h.truthy(&external) {
        let p = call(h, r.clone(), "fetchMarkets", vec![])?;
        let markets = h.await_value(p).await?;
        if h.truthy(&get(h, &r, "isDisposed")?) {
            return Ok(h.undefined());
        }
        let tickers = h.map(markets.clone(), |m| {
            let id = get(h, &m, "symbol")?;
            let symbol = get(h, &m, "symbol")?;
            Ok(h.object(vec![
                ("id", id),
                ("symbol", symbol),
                ("bid", h.number(0.0)),
                ("ask", h.number(0.0)),
                ("last", h.number(0.0)),
                ("mark", h.number(0.0)),
                ("index", h.number(0.0)),
                ("percentage", h.number(0.0)),
                ("openInterest", h.number(0.0)),
                ("fundingRate", h.number(0.0)),
                ("fundingTime", h.number(0.0)),
                ("volume", h.number(0.0)),
                ("quoteVolume", h.number(0.0)),
            ]))
        })?;
        let p = method(h, get(h, &r, "store")?, "update")?;
        let patch = h.object(vec![
            ("markets", markets),
            ("tickers", tickers),
            ("loaded", loaded(h, &r, &["markets", "tickers"])?),
        ]);
        ignored(h, invoke(h, p, vec![patch])?);
        ignored(
            h,
            call(
                h,
                get(h, &r, "publicWebsocket")?,
                "connectAndSubscribe",
                vec![],
            )?,
        );
    }
    if !h.truthy(&public) {
        ignored(
            h,
            call(
                h,
                get(h, &r, "privateWebsocket")?,
                "connectAndSubscribe",
                vec![],
            )?,
        );
        let p = call(h, r.clone(), "fetchBalanceAndPositions", vec![])?;
        let value = h.await_value(p).await?;
        let balance = get(h, &value, "balance")?;
        let positions = get(h, &value, "positions")?;
        if h.truthy(&get(h, &r, "isDisposed")?) {
            return Ok(h.undefined());
        }
        let p = method(h, get(h, &r, "store")?, "update")?;
        let patch = h.object(vec![
            ("balance", balance),
            ("positions", positions),
            ("loaded", loaded(h, &r, &["balance", "positions"])?),
        ]);
        ignored(h, invoke(h, p, vec![patch])?);
        let p = call(h, r.clone(), "fetchOrders", vec![])?;
        let orders = h.await_value(p).await?;
        if h.truthy(&get(h, &r, "isDisposed")?) {
            return Ok(h.undefined());
        }
        let p = method(h, get(h, &r, "store")?, "update")?;
        let patch = h.object(vec![
            ("orders", orders),
            ("loaded", loaded(h, &r, &["orders"])?),
        ]);
        ignored(h, invoke(h, p, vec![patch])?);
        ignored(h, call(h, r, "tick", vec![])?);
    }
    Ok(h.undefined())
}
pub async fn tick<H: LifecycleHost>(h: &H, r: H::Value) -> Result<H::Value, H::Error> {
    if !h.truthy(&get(h, &r, "isDisposed")?) {
        let result = async {
            let p = call(h, r.clone(), "fetchBalanceAndPositions", vec![])?;
            let value = h.await_value(p).await?;
            let balance = get(h, &value, "balance")?;
            let positions = get(h, &value, "positions")?;
            if h.truthy(&get(h, &r, "isDisposed")?) {
                return Ok(true);
            }
            let old = get(h, &get(h, &r, "store")?, "positions")?;
            let closed = h.filter(old, |p| {
                if h.equal(&get(h, &p, "contracts")?, &h.number(0.0)) {
                    return Ok(false);
                }
                let found = h.find(positions.clone(), |sp| {
                    Ok(h.equal(&get(h, &sp, "id")?, &get(h, &p, "id")?))
                })?;
                Ok(!h.truthy(&found) || h.equal(&get(h, &found, "contracts")?, &h.number(0.0)))
            })?;
            ignored(
                h,
                call(
                    h,
                    get(h, &r, "store")?,
                    "addOrUpdatePositionsFull",
                    vec![positions],
                )?,
            );
            let p = method(h, get(h, &r, "store")?, "update")?;
            let patch = h.object(vec![
                ("balance", balance),
                ("loaded", loaded(h, &r, &["balance", "positions"])?),
            ]);
            ignored(h, invoke(h, p, vec![patch])?);
            if h.truthy(&get(h, &closed, "length")?) {
                let p = method(h, r.clone(), "fetchAndSetOrders")?;
                let symbols = h.map(closed, |p| get(h, &p, "symbol"))?;
                let value = invoke(h, p, vec![symbols])?;
                h.await_value(value).await?;
            }
            Ok::<_, H::Error>(false)
        }
        .await;
        match result {
            Ok(true) => return Ok(h.undefined()),
            Ok(false) => {}
            Err(e) => {
                let p = method(h, r.clone(), "emitAndLogError")?;
                let error = h.error_value(&e);
                ignored(h, invoke(h, p, vec![opt(h, &error, "message")?])?);
            }
        }
        if !h.truthy(&get(h, &r, "isDisposed")?) {
            let f = h.builtin("loop")?;
            let owner = h.clone();
            let receiver = r.clone();
            let callback = h.function(Rc::new(move |_| {
                call(&owner, receiver.clone(), "tick", vec![])
            }));
            let interval = call(h, r, "getTickInterval", vec![])?;
            ignored(h, h.call(f, h.undefined(), vec![callback, interval])?);
        }
    }
    Ok(h.undefined())
}
pub async fn parse_positions<H: LifecycleHost>(
    h: &H,
    r: H::Value,
    data: H::Value,
) -> Result<H::Value, H::Error> {
    let accounts = call(h, r.clone(), "getPositionAccounts", vec![data.clone()])?;
    let current = call(h, r.clone(), "getAccountPositionSnapshots", vec![data])?;
    if !h.truthy(&accounts) || !h.truthy(&current) {
        return get(h, &get(h, &r, "store")?, "positions");
    }
    let symbols = h.map(current.clone(), |p| get(h, &p, "symbol"))?;
    let value = call(
        h,
        r.clone(),
        "fetchPositionMetadata",
        vec![symbols, current],
    )?;
    let metadata = h.await_value(value).await?;
    let rows = h.map(accounts, |a| {
        h.flatten(h.map(get(h, &a, "positionVos")?, |vo| {
            h.map(get(h, &vo, "positions")?, |position| {
                let symbol = get(h, &vo, "contractName")?;
                let meta = h.find(metadata.clone(), |m| {
                    Ok(h.equal(&get(h, &m, "symbol")?, &symbol))
                })?;
                let field = |key: &str, to: &mut dyn FnMut() -> Result<H::Value, H::Error>| {
                    let v = opt(h, &meta, key)?;
                    if h.truthy(&v) { Ok(v) } else { to() }
                };
                let margin = field("marginType", &mut || {
                    Ok(
                        if h.equal(&get(h, &position, "positionType")?, &h.number(1.0)) {
                            h.text("cross")
                        } else {
                            h.text("isolated")
                        },
                    )
                })?;
                let leverage = field("leverage", &mut || get(h, &position, "leverageLevel"))?;
                let mode = field("positionMode", &mut || Ok(h.text("oneway")))?;
                let side = if h.equal(&get(h, &position, "side")?, &h.text("BUY")) {
                    h.text("long")
                } else {
                    h.text("short")
                };
                let id = h.position_id(get(h, &vo, "contractName")?, side.clone(), mode.clone())?;
                let existing = h.find(get(h, &get(h, &r, "store")?, "positions")?, |p| {
                    Ok(h.equal(&get(h, &p, "id")?, &id))
                })?;
                let ticker = h.dynamic(
                    get(h, &get(h, &r, "store")?, "tickersBySymbol")?,
                    symbol.clone(),
                )?;
                let contracts = get(h, &position, "volume")?;
                let last = opt(h, &ticker, "last")?;
                let last = if h.truthy(&last) { last } else { h.number(0.0) };
                let entry = get(h, &position, "openPrice")?;
                let pnl = h.arithmetic(
                    Op::Mul,
                    h.arithmetic(
                        Op::Mul,
                        if h.equal(&side, &h.text("long")) {
                            h.number(1.0)
                        } else {
                            h.number(-1.0)
                        },
                        h.arithmetic(Op::Sub, last, entry.clone())?,
                    )?,
                    get(h, &position, "volume")?,
                )?;
                let liqs = h.map(
                    h.array(vec![
                        get(h, &position, "forceLiquidationPrice")?,
                        get(h, &position, "reducePrice")?,
                    ]),
                    |v| h.numeric(v),
                )?;
                let parsed = h.find(liqs, |v| Ok(h.is_finite(&v) && h.less(h.number(0.0), v)?))?;
                let liq = if h.truthy(&parsed) {
                    parsed
                } else {
                    let prior = opt(h, &existing, "liquidationPrice")?;
                    if h.truthy(&prior) {
                        prior
                    } else {
                        h.number(0.0)
                    }
                };
                Ok(h.object(vec![
                    ("id", id),
                    ("symbol", symbol),
                    ("side", side),
                    ("positionMode", mode),
                    ("marginType", margin),
                    ("entryPrice", entry),
                    ("notional", get(h, &position, "positionBalance")?),
                    ("leverage", leverage),
                    ("unrealizedPnl", pnl),
                    ("contracts", contracts),
                    ("liquidationPrice", liq),
                    ("info", position),
                ]))
            })
        })?)
    })?;
    h.flatten(rows)
}
pub fn listen_candles<H: LifecycleHost>(
    h: &H,
    r: H::Value,
    opts: H::Value,
    callback: H::Value,
) -> Result<H::Value, H::Error> {
    let set = get(h, &r, "prioritySymbols")?;
    h.set_add(set, get(h, &opts, "symbol")?)?;
    let p = method(h, get(h, &r, "publicWebsocket")?, "listenOHLCV")?;
    invoke(h, p, vec![opts, callback])
}
pub fn listen_book<H: LifecycleHost>(
    h: &H,
    r: H::Value,
    symbol: H::Value,
    callback: H::Value,
) -> Result<H::Value, H::Error> {
    call(
        h,
        get(h, &r, "publicWebsocket")?,
        "listenOrderBook",
        vec![symbol, callback],
    )
}
