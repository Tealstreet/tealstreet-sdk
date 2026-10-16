use crate::market_account_reads::{MarketAccountOps, ReadAwait};
use std::rc::Rc;

type Callback<V, E> = Rc<dyn Fn(Vec<V>) -> Result<V, E>>;
pub struct LifecycleOps<V, E> {
    pub value: Rc<MarketAccountOps<V, E>>,
    pub spawn: Rc<dyn Fn(ReadAwait<V, E>) -> Result<V, E>>,
    pub callback: Rc<dyn Fn(Callback<V, E>) -> V>,
}
pub struct ExchangeState<V> {
    pub receiver: V,
    pub region: V,
    pub xhr: V,
    pub public_websocket: V,
    pub private_websocket: V,
    pub business_websocket: V,
}
pub struct EuExchangeState<V> {
    pub base: V,
}
pub struct DemoExchangeState<V> {
    pub base: V,
    pub is_demo: V,
}
pub struct EuDemoExchangeState<V> {
    pub base: EuExchangeState<V>,
    pub is_demo: V,
}
fn call<V, E>(h: &LifecycleOps<V, E>, name: &str, args: Vec<V>) -> Result<V, E> {
    let callee = (h.value.capture)(name)?;
    (h.value.invoke)(callee, args)
}
fn assign<V, E>(h: &LifecycleOps<V, E>, name: &str, value: V) -> Result<(), E> {
    let receiver = (h.value.capture)("this")?;
    (h.value.set)(&receiver, name, value)
}
fn define<V: Clone, E>(h: &LifecycleOps<V, E>, object: &V, key: &str, value: V) -> Result<(), E> {
    (h.value.intrinsic)(
        "define-literal-field",
        vec![object.clone(), (h.value.literal)(key), value],
    )?;
    Ok(())
}
fn loaded<V: Clone, E>(h: &LifecycleOps<V, E>, fields: &[&str]) -> Result<V, E> {
    let old = (h.value.capture)("this.store.loaded")?;
    let new = (h.value.intrinsic)("object-spread", vec![old])?;
    for field in fields {
        define(h, &new, field, (h.value.boolean)(true))?;
    }
    Ok(new)
}
pub fn new_exchange<V: Clone, E>(
    h: &LifecycleOps<V, E>,
    name: V,
    opts: V,
    store: V,
) -> Result<ExchangeState<V>, E> {
    let receiver = call(h, "super", vec![name.clone(), opts.clone(), store])?;
    // Field initialization belongs to the host's inherited class assembly.
    (h.value.intrinsic)("initialize-okx-fields", vec![receiver.clone()])?;
    let region = (h.value.literal)(
        if (h.value.equal)(&name, &(h.value.literal)("okxeu"))
            || (h.value.equal)(&name, &(h.value.literal)("okxeudemo"))
        {
            "okxeu"
        } else {
            "okx"
        },
    );
    assign(h, "region", region.clone())?;
    let secondary = (h.value.capture)("this.createXhrWithSecondaryKey")?;
    let factory = (h.value.capture)("createAPI")?;
    let xhr = (h.value.invoke)(secondary, vec![opts, factory])?;
    assign(h, "xhr", xhr.clone())?;
    let mut sockets = Vec::new();
    for (class, field) in [
        ("OKXPublicWebsocket", "publicWebsocket"),
        ("OKXPrivateWebsocket", "privateWebsocket"),
        ("OKXBusinessWebsocket", "businessWebsocket"),
    ] {
        let constructor = (h.value.capture)(class)?;
        let this = (h.value.capture)("this")?;
        let socket = (h.value.intrinsic)("construct", vec![constructor, this])?;
        assign(h, field, socket.clone())?;
        sockets.push(socket);
    }
    Ok(ExchangeState {
        receiver,
        region,
        xhr,
        public_websocket: sockets[0].clone(),
        private_websocket: sockets[1].clone(),
        business_websocket: sockets[2].clone(),
    })
}
fn demo_base<V: Clone, E>(
    h: &LifecycleOps<V, E>,
    name: V,
    opts: V,
    store: V,
    eu: bool,
) -> Result<V, E> {
    let super_constructor = (h.value.capture)(if eu {
        "super:OKXEUExchange"
    } else {
        "super:OKXExchange"
    })?;
    let forced = (h.value.intrinsic)("object-spread", vec![opts])?;
    define(h, &forced, "testnet", (h.value.boolean)(true))?;
    let base = (h.value.invoke)(super_constructor, vec![name, forced, store])?;
    define(h, &base, "isDemo", (h.value.boolean)(true))?;
    Ok(base)
}
pub fn new_demo_exchange<V: Clone, E>(
    h: &LifecycleOps<V, E>,
    name: V,
    opts: V,
    store: V,
) -> Result<DemoExchangeState<V>, E> {
    Ok(DemoExchangeState {
        base: demo_base(h, name, opts, store, false)?,
        is_demo: (h.value.boolean)(true),
    })
}
pub fn new_eu_demo_exchange<V: Clone, E>(
    h: &LifecycleOps<V, E>,
    name: V,
    opts: V,
    store: V,
) -> Result<EuDemoExchangeState<V>, E> {
    Ok(EuDemoExchangeState {
        base: EuExchangeState {
            base: demo_base(h, name, opts, store, true)?,
        },
        is_demo: (h.value.boolean)(true),
    })
}
pub async fn start<V: Clone + 'static, E: 'static>(h: Rc<LifecycleOps<V, E>>) -> Result<V, E> {
    if (h.value.truthy)(&(h.value.capture)("this.needsAccountMarketOverlay")?) {
        let callee = (h.value.capture)("this.loadAccountMarketOverlay")?;
        let ops = h.clone();
        let callback = (h.callback)(Rc::new(move |_| {
            call(&ops, "this.fetchAccountMarketOverlay", vec![])
        }));
        let result = (h.value.invoke)(callee, vec![callback])?;
        (h.value.await_value)(result).await?;
        if (h.value.truthy)(&(h.value.capture)("this.isDisposed")?) {
            return Ok((h.value.undefined)());
        }
    }
    let external = call(&h, "this.initExternalPublicData", vec![])?;
    let external = (h.value.await_value)(external).await?;
    let public_only = (h.value.capture)("this.isPublicDataOnly")?;
    if !(h.value.truthy)(&external) {
        let all = (h.value.capture)("Promise.all")?;
        let markets = call(&h, "this.fetchMarkets", vec![])?;
        let ops = h.clone();
        let markets = (h.spawn)(Box::pin(async move {
            let markets = (ops.value.await_value)(markets).await?;
            let update = (ops.value.capture)("this.store.update")?;
            let payload = (ops.value.object)(vec![
                ("markets", markets.clone()),
                ("loaded", loaded(&ops, &["markets"])?),
            ]);
            (ops.value.invoke)(update, vec![payload])?;
            Ok(markets)
        }))?;
        let raw = call(&h, "this.fetchTickersRaw", vec![])?;
        let pair = (h.value.invoke)(all, vec![(h.value.array)(vec![markets, raw])])?;
        let pair = (h.value.await_value)(pair).await?;
        let markets = (h.value.get)(&pair, "0")?;
        let raw = (h.value.get)(&pair, "1")?;
        if (h.value.truthy)(&(h.value.capture)("this.isDisposed")?) {
            return Ok((h.value.undefined)());
        }
        let tickers = call(&h, "this.parseTickers", vec![raw])?;
        let update = (h.value.capture)("this.store.update")?;
        let payload = (h.value.object)(vec![
            ("tickers", tickers.clone()),
            ("loaded", loaded(&h, &["tickers"])?),
        ]);
        (h.value.invoke)(update, vec![payload])?;
        let log = (h.value.capture)("this.log")?;
        let min = (h.value.capture)("Math.min")?;
        let count = (h.value.invoke)(
            min,
            vec![
                (h.value.get)(&tickers, "length")?,
                (h.value.get)(&markets, "length")?,
            ],
        )?;
        let message = (h.value.intrinsic)("loaded-okx-message", vec![count])?;
        (h.value.invoke)(log, vec![message])?;
        call(&h, "this.publicWebsocket.connectAndSubscribe", vec![])?;
        call(&h, "this.businessWebsocket.connectAndSubscribe", vec![])?;
    }
    if (h.value.truthy)(&public_only) {
        call(
            &h,
            "this.log",
            vec![(h.value.literal)("Public data only mode enabled")],
        )?;
        return Ok((h.value.undefined)());
    }
    let mode = call(&h, "this.getOrFetchPositionMode", vec![])?;
    (h.value.await_value)(mode).await?;
    call(&h, "this.privateWebsocket.connectAndSubscribe", vec![])?;
    let all = (h.value.capture)("Promise.all")?;
    let balance = call(&h, "this.fetchBalanceAndPositions", vec![])?;
    let ops = h.clone();
    let balance = (h.spawn)(Box::pin(async move {
        let result = (ops.value.await_value)(balance).await?;
        let balance = (ops.value.get)(&result, "balance")?;
        let positions = (ops.value.get)(&result, "positions")?;
        let update = (ops.value.capture)("this.store.update")?;
        let payload = (ops.value.object)(vec![
            ("balance", balance),
            ("positions", positions),
            ("loaded", loaded(&ops, &["balance", "positions"])?),
        ]);
        (ops.value.invoke)(update, vec![payload])?;
        Ok((ops.value.undefined)())
    }))?;
    let orders = call(&h, "this.fetchOrders", vec![])?;
    let ops = h.clone();
    let orders = (h.spawn)(Box::pin(async move {
        let orders = (ops.value.await_value)(orders).await?;
        let update = (ops.value.capture)("this.store.update")?;
        let payload = (ops.value.object)(vec![
            ("orders", orders),
            ("loaded", loaded(&ops, &["orders"])?),
        ]);
        (ops.value.invoke)(update, vec![payload])?;
        Ok((ops.value.undefined)())
    }))?;
    let done = (h.value.invoke)(all, vec![(h.value.array)(vec![balance, orders])])?;
    (h.value.await_value)(done).await?;
    call(
        &h,
        "this.log",
        vec![(h.value.literal)("Ready to trade on OKX")],
    )?;
    call(&h, "this.tick", vec![])?;
    Ok((h.value.undefined)())
}
pub async fn tick<V: Clone + 'static, E: 'static>(h: Rc<LifecycleOps<V, E>>) -> Result<V, E> {
    if !(h.value.truthy)(&(h.value.capture)("this.isDisposed")?) {
        let format = (h.value.capture)("this.formatBalanceUsedAvail")?;
        let info = (h.value.capture)("this.store.balance.info")?;
        let positions = (h.value.capture)("this.store.positions")?;
        let positions = if (h.value.truthy)(&positions) {
            positions
        } else {
            (h.value.array)(vec![])
        };
        let orders = (h.value.capture)("this.store.orders")?;
        let orders = if (h.value.truthy)(&orders) {
            orders
        } else {
            (h.value.array)(vec![])
        };
        let result = (h.value.invoke)(format, vec![info, positions, orders])?;
        let balance = (h.value.await_value)(result).await?;
        call(
            &h,
            "this.store.update",
            vec![(h.value.object)(vec![("balance", balance)])],
        )?;
        let loop_fn = (h.value.capture)("loop")?;
        let ops = h.clone();
        let callback = (h.callback)(Rc::new(move |_| call(&ops, "this.tick", vec![])));
        let interval = call(&h, "this.getTickInterval", vec![])?;
        (h.value.invoke)(loop_fn, vec![callback, interval])?;
    }
    Ok((h.value.undefined)())
}
pub async fn fetch_eu_account_market_overlay<V: Clone + 'static, E: 'static>(
    h: Rc<LifecycleOps<V, E>>,
) -> Result<V, E> {
    let markets = call(&h, "this.fetchMarkets", vec![])?;
    let markets = (h.value.await_value)(markets).await?;
    if (h.value.equal)(&(h.value.get)(&markets, "length")?, &(h.value.number)(0.)) {
        return (h.value.intrinsic)("null", vec![]);
    }
    let map_constructor = (h.value.capture)("Map")?;
    let ops = h.clone();
    let pairs = (h.value.map)(
        markets,
        Rc::new(move |m| Ok((ops.value.array)(vec![(ops.value.get)(&m, "id")?, m]))),
    )?;
    let by_id = (h.value.intrinsic)("construct", vec![map_constructor, pairs])?;
    let ops = h.clone();
    Ok((h.callback)(Rc::new(move |args| {
        let h = ops.clone();
        let map = by_id.clone();
        (ops.value.flat_map)(
            args[0].clone(),
            Rc::new(move |m| {
                let getter = (h.value.member)(map.clone(), "get")?;
                let id = (h.value.get)(&m, "id")?;
                let own = (h.value.invoke)(getter, vec![id])?;
                if !(h.value.truthy)(&own) {
                    return Ok((h.value.array)(vec![]));
                }
                let copy = (h.value.intrinsic)("object-spread", vec![m])?;
                define(&h, &copy, "limits", (h.value.get)(&own, "limits")?)?;
                Ok((h.value.array)(vec![copy]))
            }),
        )
    })))
}
