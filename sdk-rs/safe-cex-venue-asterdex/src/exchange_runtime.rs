//! Aster construction and startup/reconciliation/disposal bodies.
use crate::{
    api_runtime::create_api_instances,
    runtime_host::{ResultValue, RuntimeHost, Task, awaited, copy, emit_log, invoke},
};
use std::rc::Rc;
/// Field members bind their existing mapped Rust bodies; construction binds shared owners.
pub trait AssemblyHost: RuntimeHost {
    fn base_exchange(
        &self,
        name: Self::Value,
        options: Self::Value,
        store: Self::Value,
    ) -> ResultValue<Self::Value>;
    fn member(&self, name: &'static str, owner: &Self::Value) -> ResultValue<Self::Value>;
    fn construct(&self, name: &str, args: Vec<Self::Value>) -> ResultValue<Self::Value>;
    fn base_dispose(&self, owner: Self::Value) -> ResultValue<Self::Value>;
}
pub const METHOD_FIELDS: &[&str] = &[
    "markPositionStateUpdatedByWebsocket",
    "getPositionStateVersion",
    "markBalanceStateUpdatedByWebsocket",
    "getBalanceStateVersion",
    "reconcileBalanceAfterPrivateUpdate",
    "getFapiPath",
    "getAccountEndpoint",
    "start",
    "tick",
    "fetchBalance",
    "getAccount",
    "validateAccount",
    "fetchPositionMode",
    "syncPositionMode",
    "getAccountPositionMode",
    "fetchPositions",
    "fetchOrders",
    "fetchMarkets",
    "fetchTickers",
    "fetchPublicTrades",
    "setLeverage",
    "changePositionMode",
    "afterMethodCalledInWorker",
    "changeMarginType",
    "fetchSymbolMaxLeverage",
    "assertV3Feature",
    "fetchStpMode",
    "changeStpMode",
    "placeOrder",
    "placeOrders",
    "updateOrder",
    "cancelOrders",
    "fetchOHLCV",
    "fetchHistoricalFills",
    "listenOHLCV",
    "fetchOrderBook",
    "listenOrderBook",
    "getOrderPositionSide",
    "fetchOpenInterestMap",
    "collectOpenInterestFromPayload",
    "updateStoreOpenInterest",
    "pollOpenInterest",
    "startOpenInterestPolling",
    "dispose",
];
/// ts: AsterDEXExchange.
pub struct AsterDEXExchange<H: AssemblyHost> {
    pub owner: H::Value,
    host: Rc<H>,
}
impl<H: AssemblyHost> AsterDEXExchange<H> {
    pub const OPEN_INTEREST_POLL_INTERVAL_MS: f64 =
        crate::order_execution::OPEN_INTEREST_POLL_INTERVAL_MS;
    pub const OPEN_INTEREST_BATCH_SIZE: usize = crate::order_execution::OPEN_INTEREST_BATCH_SIZE;
    /// ts: AsterDEXExchange.constructor.
    pub fn new(
        host: Rc<H>,
        name: H::Value,
        options: H::Value,
        store: H::Value,
    ) -> ResultValue<H::Value, Self> {
        let owner = host.base_exchange(name, options.clone(), store.clone())?;
        for key in [
            "primaryAPI",
            "secondaryAPI",
            "unlimitedAPI",
            "publicAPI",
            "apiVersion",
            "publicWebsocket",
            "privateWebsocket",
        ] {
            host.define(&owner, key, host.undefined())?;
        }
        for key in [
            "positionStateVersion",
            "balanceStateVersion",
            "lastBalanceReconciliationAt",
            "lastPositionSafetyReconciliationAt",
            "lastOrderSafetyReconciliationAt",
            "openInterestCursor",
        ] {
            host.define(&owner, key, host.number(0.))?;
        }
        for key in METHOD_FIELDS {
            host.define(&owner, key, host.member(key, &owner)?)?;
        }
        host.set(
            &owner,
            "apiVersion",
            host.import("resolveAsterDEXApiVersion", vec![options])?,
        )?;
        initialize_apis(host.clone(), owner.clone())?;
        host.set(
            &owner,
            "publicWebsocket",
            host.construct(
                "AsterDEXPublicWebsocket",
                vec![owner.clone(), store.clone()],
            )?,
        )?;
        host.set(
            &owner,
            "privateWebsocket",
            host.construct("AsterDEXPrivateWebsocket", vec![owner.clone(), store])?,
        )?;
        Ok(Self { owner, host })
    }
    pub fn start(&self) -> Task<H::Value> {
        start_exchange(self.host.clone(), self.owner.clone())
    }
    pub fn tick(&self) -> Task<H::Value> {
        tick(self.host.clone(), self.owner.clone())
    }
    pub fn dispose(&self) -> ResultValue<H::Value> {
        dispose(self.host.as_ref(), self.owner.clone())
    }
}
/// ts: AsterDEXExchange.initializeAPIs.
pub fn initialize_apis<H: RuntimeHost>(h: Rc<H>, owner: H::Value) -> ResultValue<H::Value, ()> {
    let apis = create_api_instances(h.clone(), owner.clone())?;
    for (field, key) in [
        ("primaryAPI", "primary"),
        ("secondaryAPI", "secondary"),
        ("unlimitedAPI", "unlimited"),
        ("publicAPI", "public"),
    ] {
        h.set(&owner, field, h.get(&apis, key)?)?;
    }
    Ok(())
}
fn update<H: RuntimeHost>(
    h: &H,
    owner: &H::Value,
    kind: &str,
    rows: H::Value,
) -> ResultValue<H::Value, ()> {
    let store = h.get(owner, "store")?;
    let function = h.get(&store, "update")?;
    let loaded = copy(h, h.get(&h.get(owner, "store")?, "loaded")?)?;
    h.define(&loaded, kind, h.boolean(true))?;
    h.call(
        function,
        store,
        vec![h.object(vec![(kind, rows), ("loaded", loaded)])],
    )?;
    Ok(())
}
fn now<H: RuntimeHost>(h: &H) -> ResultValue<H::Value> {
    h.import("Date.now", vec![])
}
fn public_only<H: RuntimeHost>(h: &H, owner: &H::Value) -> ResultValue<H::Value, bool> {
    Ok(h.truthy(&h.get(owner, "isPublicDataOnly")?))
}
async fn start_body<H: AssemblyHost>(h: Rc<H>, owner: H::Value) -> ResultValue<H::Value> {
    emit_log!(
        h.as_ref(),
        owner.clone(),
        h.template(vec![
            h.text("Starting AsterDEX exchange (API "),
            h.get(&owner, "apiVersion")?,
            h.text(")..."),
        ])?,
        "Info",
    )?;
    let external = awaited(
        h.as_ref(),
        invoke!(h.as_ref(), owner.clone(), "initExternalPublicData", vec![]),
    )
    .await?;
    awaited(
        h.as_ref(),
        invoke!(
            h.as_ref(),
            h.constant("asterDEXVirtualClock"),
            "start",
            vec![
                h.get(&h.get(&owner, "options")?, "testnet")?,
                h.get(&owner, "apiVersion")?
            ],
        ),
    )
    .await?;
    emit_log!(
        h.as_ref(),
        owner.clone(),
        h.text("Time synced with AsterDEX server"),
        "Info",
    )?;
    if !public_only(h.as_ref(), &owner)? {
        awaited(
            h.as_ref(),
            invoke!(h.as_ref(), owner.clone(), "validateAccount", vec![]),
        )
        .await?;
        awaited(
            h.as_ref(),
            invoke!(h.as_ref(), owner.clone(), "syncPositionMode", vec![]),
        )
        .await?;
    }
    if !h.truthy(&external) {
        let markets = awaited(
            h.as_ref(),
            invoke!(h.as_ref(), owner.clone(), "fetchMarkets", vec![]),
        )
        .await?;
        emit_log!(
            h.as_ref(),
            owner.clone(),
            h.template(vec![
                h.text("Fetched "),
                h.get(&markets, "length")?,
                h.text(" markets"),
            ])?,
            "Info",
        )?;
        if h.less(h.number(0.), h.get(&markets, "length")?)? {
            let host = h.clone();
            let predicate = h.function(Rc::new(move |a| {
                Ok(host.boolean(host.strict_equal(
                    &host.get(
                        &a.first().cloned().unwrap_or_else(|| host.undefined()),
                        "symbol",
                    )?,
                    &host.text("BTCUSDT"),
                )))
            }));
            let selected = invoke!(h.as_ref(), markets.clone(), "find", vec![predicate])?;
            let sample = if h.truthy(&selected) {
                selected
            } else {
                h.get(&markets, "0")?
            };
            emit_log!(
                h.as_ref(),
                owner.clone(),
                h.template(vec![
                    h.text("Sample market ("),
                    h.get(&sample, "symbol")?,
                    h.text("): pricePrecision="),
                    h.get(&h.get(&sample, "precision")?, "price")?,
                    h.text(" (decimal), amountPrecision="),
                    h.get(&h.get(&sample, "precision")?, "amount")?,
                    h.text(" (decimal)"),
                ])?,
                "Info",
            )?;
        }
        update(h.as_ref(), &owner, "markets", markets)?;
        let tickers = awaited(
            h.as_ref(),
            invoke!(h.as_ref(), owner.clone(), "fetchTickers", vec![]),
        )
        .await?;
        emit_log!(
            h.as_ref(),
            owner.clone(),
            h.template(vec![
                h.text("Fetched "),
                h.get(&tickers, "length")?,
                h.text(" tickers"),
            ])?,
            "Info",
        )?;
        update(h.as_ref(), &owner, "tickers", tickers)?;
        invoke!(
            h.as_ref(),
            owner.clone(),
            "startOpenInterestPolling",
            vec![],
        )?;
    }
    if !public_only(h.as_ref(), &owner)? {
        for (kind, method, time, success, failure) in [
            (
                "positions",
                "fetchPositions",
                "lastPositionSafetyReconciliationAt",
                " positions",
                "Failed to fetch initial positions: ",
            ),
            (
                "orders",
                "fetchOrders",
                "lastOrderSafetyReconciliationAt",
                " open orders",
                "Failed to fetch initial orders: ",
            ),
        ] {
            let attempt = async {
                let rows = awaited(
                    h.as_ref(),
                    invoke!(h.as_ref(), owner.clone(), method, vec![]),
                )
                .await?;
                update(h.as_ref(), &owner, kind, rows.clone())?;
                h.set(&owner, time, now(h.as_ref())?)?;
                emit_log!(
                    h.as_ref(),
                    owner.clone(),
                    h.template(vec![
                        h.text("Loaded "),
                        h.get(&rows, "length")?,
                        h.text(success),
                    ])?,
                    "Info",
                )?;
                Ok::<_, H::Value>(())
            }
            .await;
            if let Err(error) = attempt {
                emit_log!(
                    h.as_ref(),
                    owner.clone(),
                    h.template(vec![h.text(failure), h.get(&error, "message")?])?,
                    "Warning",
                )?;
            }
        }
    }
    if !h.truthy(&external) {
        invoke!(
            h.as_ref(),
            h.get(&owner, "publicWebsocket")?,
            "connectAndSubscribe",
            vec![],
        )?;
    }
    if !public_only(h.as_ref(), &owner)? {
        invoke!(
            h.as_ref(),
            h.get(&owner, "privateWebsocket")?,
            "connectAndSubscribe",
            vec![],
        )?;
    }
    if !public_only(h.as_ref(), &owner)? {
        invoke!(h.as_ref(), owner.clone(), "tick", vec![])?;
    }
    emit_log!(
        h.as_ref(),
        owner,
        h.text("AsterDEX exchange started successfully"),
        "Info",
    )?;
    Ok(h.undefined())
}
/// ts: AsterDEXExchange.start.
pub fn start_exchange<H: AssemblyHost>(h: Rc<H>, owner: H::Value) -> Task<H::Value> {
    let host = h.clone();
    h.start(Box::pin(async move {
        match start_body(host.clone(), owner.clone()).await {
            Ok(value) => Ok(value),
            Err(error) => {
                emit_log!(
                    host.as_ref(),
                    owner,
                    host.template(vec![
                        host.text("Failed to start AsterDEX exchange: "),
                        host.get(&error, "message")?,
                    ])?,
                    "Error",
                )?;
                Err(error)
            }
        }
    }))
}
fn due<H: RuntimeHost>(
    h: &H,
    owner: &H::Value,
    now: H::Value,
    key: &str,
    interval: f64,
) -> ResultValue<H::Value, bool> {
    if h.strict_equal(&h.get(owner, key)?, &h.number(0.)) {
        return Ok(true);
    }
    let elapsed = h.subtract(now, h.get(owner, key)?)?;
    h.greater_equal(elapsed, h.number(interval))
}
/// ts: AsterDEXExchange.tick.
pub fn tick<H: AssemblyHost>(h: Rc<H>, owner: H::Value) -> Task<H::Value> {
    let host = h.clone();
    h.start(Box::pin(async move {
        let h = host;
        if h.truthy(&h.get(&owner, "isDisposed")?) {
            return Ok(h.undefined());
        }
        let time = now(h.as_ref())?;
        let balance = due(
            h.as_ref(),
            &owner,
            time.clone(),
            "lastBalanceReconciliationAt",
            25000.,
        )?;
        let positions = due(
            h.as_ref(),
            &owner,
            time.clone(),
            "lastPositionSafetyReconciliationAt",
            25000.,
        )?;
        let orders = due(
            h.as_ref(),
            &owner,
            time,
            "lastOrderSafetyReconciliationAt",
            75000.,
        )?;
        if balance {
            awaited(
                h.as_ref(),
                invoke!(
                    h.as_ref(),
                    owner.clone(),
                    "reconcileBalanceAfterPrivateUpdate",
                    vec![h.get(&owner, "balanceStateVersion")?],
                ),
            )
            .await?;
        }
        if positions {
            let version = invoke!(h.as_ref(), owner.clone(), "getPositionStateVersion", vec![])?;
            let attempt = async {
                let positions = awaited(
                    h.as_ref(),
                    invoke!(h.as_ref(), owner.clone(), "fetchPositions", vec![]),
                )
                .await?;
                if h.truthy(&h.get(&owner, "isDisposed")?) {
                    return Ok(true);
                }
                if h.strict_equal(
                    &version,
                    &invoke!(h.as_ref(), owner.clone(), "getPositionStateVersion", vec![])?,
                ) {
                    let store = h.get(&owner, "store")?;
                    invoke!(
                        h.as_ref(),
                        store.clone(),
                        "addOrUpdatePositionsFull",
                        vec![positions],
                    )?;
                    let store = h.get(&owner, "store")?;
                    let function = h.get(&store, "update")?;
                    let loaded = copy(h.as_ref(), h.get(&h.get(&owner, "store")?, "loaded")?)?;
                    h.define(&loaded, "positions", h.boolean(true))?;
                    h.call(function, store, vec![h.object(vec![("loaded", loaded)])])?;
                }
                h.set(
                    &owner,
                    "lastPositionSafetyReconciliationAt",
                    now(h.as_ref())?,
                )?;
                Ok::<_, H::Value>(false)
            }
            .await;
            match attempt {
                Ok(true) => return Ok(h.undefined()),
                Err(error) => emit_log!(
                    h.as_ref(),
                    owner.clone(),
                    h.template(vec![
                        h.text("Position safety reconciliation failed: "),
                        h.get(&error, "message")?,
                    ])?,
                    "Warning",
                )?,
                _ => {}
            }
        }
        if orders {
            let attempt = async {
                let orders = awaited(
                    h.as_ref(),
                    invoke!(h.as_ref(), owner.clone(), "fetchOrders", vec![]),
                )
                .await?;
                if h.truthy(&h.get(&owner, "isDisposed")?) {
                    return Ok(true);
                }
                update(h.as_ref(), &owner, "orders", orders)?;
                h.set(&owner, "lastOrderSafetyReconciliationAt", now(h.as_ref())?)?;
                Ok::<_, H::Value>(false)
            }
            .await;
            match attempt {
                Ok(true) => return Ok(h.undefined()),
                Err(error) => emit_log!(
                    h.as_ref(),
                    owner.clone(),
                    h.template(vec![
                        h.text("Order safety reconciliation failed: "),
                        h.get(&error, "message")?,
                    ])?,
                    "Warning",
                )?,
                _ => {}
            }
        }
        if !h.truthy(&h.get(&owner, "isDisposed")?) {
            let host = h.clone();
            let ex = owner.clone();
            let callback = h.function(Rc::new(move |_| {
                invoke!(host.as_ref(), ex.clone(), "tick", vec![])
            }));
            h.import("loop", vec![callback, h.number(3000.)])?;
        }
        Ok(h.undefined())
    }))
}
/// ts: AsterDEXExchange.dispose.
pub fn dispose<H: AssemblyHost>(h: &H, owner: H::Value) -> ResultValue<H::Value> {
    invoke!(h, h.constant("asterDEXVirtualClock"), "stop", vec![])?;
    for key in ["publicWebsocket", "privateWebsocket"] {
        if h.truthy(&h.get(&owner, key)?) {
            invoke!(h, h.get(&owner, key)?, "dispose", vec![])?;
        }
    }
    h.base_dispose(owner)?;
    Ok(h.undefined())
}
