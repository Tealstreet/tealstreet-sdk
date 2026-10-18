//! HyroTrader snapshot publication and simulator read facade.
use crate::hyro_private_session::SessionFuture;
use crate::variant_account_refresh::AccountRefreshHost;
macro_rules! call {
    ($h:expr,$owner:expr,$key:expr,$args:expr) => {{
        let selected = $h.select($owner, $key)?;
        $h.apply(selected, $args)?
    }};
}
/// Closed set of lexical callbacks created by this module and selected by the native host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Callback {
    SnapshotCurrent,
    SnapshotPnl,
    MarketUsdt,
    MarketEntry,
}
impl Callback {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "snapshot-current" => Some(Self::SnapshotCurrent),
            "snapshot-pnl" => Some(Self::SnapshotPnl),
            "market-usdt" => Some(Self::MarketUsdt),
            "market-entry" => Some(Self::MarketEntry),
            _ => None,
        }
    }
}

pub fn dispatch<H: AccountRefreshHost>(
    h: &H,
    code: Callback,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    match code {
        Callback::SnapshotCurrent => Ok(h.boolean(true)),
        Callback::SnapshotPnl => h.intrinsic(
            "addition",
            vec![args[0].clone(), h.get(&args[1], "unrealizedPnl")?],
        ),
        Callback::MarketUsdt => {
            Ok(h.boolean(h.equal(&h.get(&args[0], "quote")?, &h.literal("USDT"))))
        }
        Callback::MarketEntry => Ok(h.array(vec![h.get(&args[0], "symbol")?, args[0].clone()])),
    }
}
pub fn reconcile_private_state<H: AccountRefreshHost>(
    h: &H,
    owner: H::Value,
    current: H::Value,
    orders: H::Value,
) -> SessionFuture<'_, H::Value, H::Error> {
    let initial = (|| {
        let current = if h.equal(&current, &h.undefined()) {
            h.callback(owner.clone(), "snapshot-current", vec![])?
        } else {
            current
        };
        let orders = if h.equal(&orders, &h.undefined()) {
            h.boolean(true)
        } else {
            orders
        };
        let version = h.number(h.numeric(&h.get(&owner, "snapshotVersion")?)? + 1.0);
        h.set(&owner, "snapshotVersion", version.clone())?;
        let revision = h.get(&h.get(&owner, "privateWebsocket")?, "stateRevision")?;
        Ok((current, orders, version, revision))
    })();
    let requests = initial.map(|(current, orders, version, revision)| {
        let promise = (|| {
            let orders = if h.truthy(&orders) {
                call!(h, owner.clone(), "fetchOrders", vec![])
            } else {
                h.intrinsic("Promise.resolve", vec![h.undefined()])?
            };
            let positions = call!(h, owner.clone(), "fetchPositions", vec![]);
            let raw = call!(h, owner.clone(), "fetchRawBalance", vec![]);
            h.all(vec![orders, positions, raw])
        })();
        (current, version, revision, promise)
    });
    Box::pin(async move {
        let (current, version, revision, promise) = requests?;
        let run: Result<H::Value, H::Error> = async {
            let results = h.wait(promise?).await?;
            let values = h.intrinsic("destructure three", vec![results])?;
            let orders = h.get(&values, "0")?;
            let positions = h.get(&values, "1")?;
            let raw = h.get(&values, "2")?;
            if h.truthy(&h.get(&owner, "isDisposed")?)
                || !h.truthy(&h.receiver_call(h.undefined(), current, vec![])?)
                || !h.equal(&version, &h.get(&owner, "snapshotVersion")?)
                || !h.equal(
                    &revision,
                    &h.get(&h.get(&owner, "privateWebsocket")?, "stateRevision")?,
                )
            {
                return Ok(h.boolean(false));
            }
            let formatted = call!(h, owner.clone(), "formatBalance", vec![raw]);
            let reduce = h.select(positions.clone(), "reduce")?;
            let upnl = h.apply(
                reduce,
                vec![
                    h.callback(owner.clone(), "snapshot-pnl", vec![])?,
                    h.number(0.0),
                ],
            )?;
            let update = h.select(h.get(&owner, "store")?, "update")?;
            let balance_base = h.spread(formatted.clone(), vec![])?;
            let sum = h.intrinsic(
                "addition",
                vec![h.get(&formatted, "total")?, h.get(&formatted, "upnl")?],
            )?;
            let total = h.number(h.numeric(&sum)? - h.numeric(&upnl)?);
            let balance = h.spread(balance_base, vec![("total", total), ("upnl", upnl)])?;
            let loaded = h.get(&h.get(&owner, "store")?, "loaded")?;
            let mut flags = vec![];
            if h.truthy(&orders) {
                flags.push(("orders", h.boolean(true)));
            }
            flags.extend([("positions", h.boolean(true)), ("balance", h.boolean(true))]);
            let loaded = h.spread(loaded, flags)?;
            let mut fields = vec![];
            if h.truthy(&orders) {
                fields.push(("orders", orders.clone()));
            }
            fields.extend([
                ("positions", positions),
                ("balance", balance),
                ("loaded", loaded),
            ]);
            h.apply(update, vec![h.object(fields)])?;
            if h.truthy(&orders) {
                let n = h.numeric(&h.get(&owner, "ordersSnapshotVersion")?)?;
                h.set(&owner, "ordersSnapshotVersion", h.number(n + 1.0))?;
            }
            Ok(h.boolean(true))
        }
        .await;
        match run {
            Ok(value) => Ok(value),
            Err(error) => {
                let emit = h.select(owner, "emitAndLogError")?;
                let message = h.normalize_error(error.clone())?;
                h.apply(emit, vec![message])?;
                Err(error)
            }
        }
    })
}
fn snapshot<H: AccountRefreshHost>(
    h: &H,
    owner: H::Value,
) -> SessionFuture<'_, H::Value, H::Error> {
    let request = (|| Ok(call!(h, owner, "reconcilePrivateState", vec![])))();
    Box::pin(async move {
        h.wait(request?).await?;
        Ok(h.undefined())
    })
}
pub fn fetch_and_set_private_snapshot<H: AccountRefreshHost>(
    h: &H,
    owner: H::Value,
) -> SessionFuture<'_, H::Value, H::Error> {
    snapshot(h, owner)
}
pub fn fetch_orders_positions_and_balance<H: AccountRefreshHost>(
    h: &H,
    owner: H::Value,
) -> SessionFuture<'_, H::Value, H::Error> {
    snapshot(h, owner)
}
pub fn fetch_and_set_private_snapshot_helper<H: AccountRefreshHost>(
    h: &H,
    owner: H::Value,
    orders: H::Value,
) -> SessionFuture<'_, H::Value, H::Error> {
    let request = (|| {
        let orders = if h.equal(&orders, &h.undefined()) {
            h.boolean(true)
        } else {
            orders
        };
        if h.truthy(&h.get(&h.get(&owner, "privateWebsocket")?, "started")?)
            && !h.truthy(&h.get(&h.get(&owner, "privateWebsocket")?, "ready")?)
        {
            return Ok(None);
        }
        let reconcile = h.select(owner.clone(), "reconcilePrivateState")?;
        let current = h.callback(owner, "snapshot-current", vec![])?;
        Ok(Some(h.apply(reconcile, vec![current, orders])?))
    })();
    Box::pin(async move {
        if let Some(value) = request? {
            h.wait(value).await?;
        }
        Ok(h.undefined())
    })
}
pub fn fetch_markets<H: AccountRefreshHost>(
    h: &H,
    owner: H::Value,
) -> SessionFuture<'_, H::Value, H::Error> {
    let request = (|| {
        Ok(call!(
            h,
            owner.clone(),
            "fetchMarketsHelper",
            vec![h.literal("linear")]
        ))
    })();
    Box::pin(async move {
        let markets = h.wait(request?).await?;
        let constructor = h.get(&h.global(), "Map")?;
        let filter = h.select(markets, "filter")?;
        let filtered = h.apply(
            filter,
            vec![h.callback(owner.clone(), "market-usdt", vec![])?],
        )?;
        let map = h.select(filtered, "map")?;
        let entries = h.apply(map, vec![h.callback(owner, "market-entry", vec![])?])?;
        let map = h.intrinsic("construct", vec![constructor, entries])?;
        let values = call!(h, map, "values", vec![]);
        h.intrinsic("iterable spread", vec![values])
    })
}
pub fn get_bybit_rest_base_url<H: AccountRefreshHost>(
    h: &H,
    owner: H::Value,
    network: H::Value,
) -> Result<H::Value, H::Error> {
    let config = call!(h, owner.clone(), "getConfiguredUrls", vec![]);
    let value = h.intrinsic("computed property", vec![h.get(&config, "rest")?, network])?;
    if !h.truthy(&value) {
        let mode = call!(h, owner, "hyrotraderMode", vec![]);
        let hint = if h.equal(&mode, &h.literal("production")) {
            " Set NEXT_PUBLIC_HYROTRADER_PROD_REST_URL, or NEXT_PUBLIC_HYROTRADER_SIMULATOR_REST_URL while v1 is simulator-backed."
        } else {
            ""
        };
        let mut message = h.units(&h.literal("HyroTrader "))?;
        message.extend(h.units(&h.get(&config, "label")?)?);
        message.extend(h.units(&h.literal(&format!(" REST URL is not configured.{hint}")))?);
        return Err(h.error(h.text(message))?);
    }
    Ok(value)
}
pub fn get_bybit_public_ws_url<H: AccountRefreshHost>(
    h: &H,
    owner: H::Value,
    category: H::Value,
    network: H::Value,
) -> Result<H::Value, H::Error> {
    if !h.equal(&category, &h.literal("linear")) {
        return Err(h.error(h.literal("HyroTrader only supports linear public WebSocket data"))?);
    }
    let config = call!(h, owner, "getConfiguredUrls", vec![]);
    let table = h.get(&h.get(&h.get(&config, "ws")?, "public")?, "linear")?;
    let value = h.intrinsic("computed property", vec![table, network])?;
    configured_ws(h, value, &config, " public WebSocket URL is not configured")
}
pub fn get_bybit_private_ws_url<H: AccountRefreshHost>(
    h: &H,
    owner: H::Value,
    network: H::Value,
) -> Result<H::Value, H::Error> {
    let config = call!(h, owner, "getConfiguredUrls", vec![]);
    let table = h.get(&h.get(&config, "ws")?, "private")?;
    let value = h.intrinsic("computed property", vec![table, network])?;
    configured_ws(
        h,
        value,
        &config,
        " private WebSocket URL is not configured",
    )
}
fn configured_ws<H: AccountRefreshHost>(
    h: &H,
    value: H::Value,
    config: &H::Value,
    suffix: &str,
) -> Result<H::Value, H::Error> {
    if !h.truthy(&value) {
        let mut message = h.units(&h.literal("HyroTrader "))?;
        message.extend(h.units(&h.get(config, "label")?)?);
        message.extend(h.units(&h.literal(suffix))?);
        return Err(h.error(h.text(message))?);
    }
    Ok(value)
}
