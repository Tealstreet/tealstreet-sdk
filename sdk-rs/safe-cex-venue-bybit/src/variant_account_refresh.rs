//! Inverse/spot account refresh, snapshot publication and polling method bodies.
use crate::hyro_private_session::{SessionFuture, SessionHost};
pub trait AccountRefreshHost: SessionHost {
    // Source object spread: own enumerable keys/getters, followed by explicit fields.
    fn spread(
        &self,
        base: Self::Value,
        fields: Vec<(&'static str, Self::Value)>,
    ) -> Result<Self::Value, Self::Error>;
    // The original Promise runtime retains siblings and reports detached rejections.
    fn then(
        &self,
        promise: Self::Value,
        fulfilled: Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn all(&self, promises: Vec<Self::Value>) -> Result<Self::Value, Self::Error>;
}
macro_rules! call {
    ($h:expr,$owner:expr,$key:expr,$args:expr) => {{
        let selected = $h.select($owner, $key)?;
        $h.apply(selected, $args)?
    }};
}
fn undefined<H: AccountRefreshHost>(h: &H) -> Result<H::Value, H::Error> {
    Ok(h.undefined())
}
fn publish<H: AccountRefreshHost>(
    h: &H,
    owner: &H::Value,
    field: &'static str,
    value: Option<H::Value>,
) -> Result<(), H::Error> {
    let update = h.select(h.get(owner, "store")?, "update")?;
    let loaded = h.get(&h.get(owner, "store")?, "loaded")?;
    let loaded = h.spread(loaded, vec![(field, h.boolean(true))])?;
    let payload = if let Some(value) = value {
        h.object(vec![(field, value), ("loaded", loaded)])
    } else {
        h.object(vec![("loaded", loaded)])
    };
    h.apply(update, vec![payload])?;
    Ok(())
}
fn positions<H: AccountRefreshHost>(
    h: &H,
    owner: &H::Value,
    fetched: H::Value,
    inverse: bool,
) -> Result<H::Value, H::Error> {
    let selected = if inverse
        && !h.equal(
            &h.get(owner, "lastWebsocketPositionsUpdate")?,
            &h.undefined(),
        ) {
        let now = call!(
            h,
            h.intrinsic("virtualClock", vec![])?,
            "getCurrentTimeMs",
            vec![]
        );
        let last = h.get(owner, "lastWebsocketPositionsUpdate")?;
        if h.numeric(&now)? - h.numeric(&last)? < 1_000.0 {
            let merge = h.select(h.global(), "mergeUpdatesUsingLastUpdate")?;
            let existing = h.get(&h.get(owner, "store")?, "positions")?;
            h.apply(merge, vec![fetched, existing])?
        } else {
            fetched
        }
    } else {
        fetched
    };
    call!(
        h,
        h.get(owner, "store")?,
        "addOrUpdatePositionsFull",
        vec![selected.clone()]
    );
    publish(h, owner, "positions", None)?;
    Ok(selected)
}
/// Closed set of lexical callbacks created by this module and selected by the native host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Callback {
    AccountOrders,
    InverseAccountPositions,
    SpotAccountPositions,
    AccountTickAgain,
}
impl Callback {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "account-orders" => Some(Self::AccountOrders),
            "inverse-account-positions" => Some(Self::InverseAccountPositions),
            "spot-account-positions" => Some(Self::SpotAccountPositions),
            "account-tick-again" => Some(Self::AccountTickAgain),
            _ => None,
        }
    }
}

pub fn dispatch<H: AccountRefreshHost>(
    h: &H,
    owner: &H::Value,
    code: Callback,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    match code {
        Callback::AccountOrders => {
            if h.truthy(&h.get(owner, "isDisposed")?) {
                return Ok(h.array(vec![]));
            }
            let orders = args[0].clone();
            publish(h, owner, "orders", Some(orders.clone()))?;
            Ok(orders)
        }
        Callback::InverseAccountPositions | Callback::SpotAccountPositions => {
            if h.truthy(&h.get(owner, "isDisposed")?) {
                return Ok(h.array(vec![]));
            }
            positions(
                h,
                owner,
                args[0].clone(),
                code == Callback::InverseAccountPositions,
            )
        }
        Callback::AccountTickAgain => Ok(call!(h, owner.clone(), "tick", vec![])),
    }
}
fn refresh<H: AccountRefreshHost>(
    h: &H,
    owner: H::Value,
    inverse: bool,
) -> SessionFuture<'_, H::Value, H::Error> {
    let requests = (|| {
        let orders = call!(h, owner.clone(), "fetchOrders", vec![]);
        let orders = h.then(orders, h.callback(owner.clone(), "account-orders", vec![])?)?;
        let fetched = call!(h, owner.clone(), "fetchPositions", vec![]);
        let code = if inverse {
            "inverse-account-positions"
        } else {
            "spot-account-positions"
        };
        let fetched = h.then(fetched, h.callback(owner.clone(), code, vec![])?)?;
        let balance = call!(h, owner.clone(), "fetchRawBalance", vec![]);
        h.all(vec![orders, fetched, balance])
    })();
    Box::pin(async move {
        let run = async {
            let found = h.wait(requests?).await?;
            let raw = h.get(&found, "2")?;
            if h.truthy(&h.get(&owner, "isDisposed")?) {
                return undefined(h);
            }
            let balance = call!(h, owner.clone(), "formatBalance", vec![raw]);
            publish(h, &owner, "balance", Some(balance))?;
            undefined(h)
        }
        .await;
        match run {
            Ok(v) => Ok(v),
            Err(error) => {
                let selected = h.select(owner, "emitAndLogError")?;
                h.apply(selected, vec![h.error_value(error)])?;
                undefined(h)
            }
        }
    })
}
pub fn inverse_account_refresh<H: AccountRefreshHost>(
    h: &H,
    owner: H::Value,
) -> SessionFuture<'_, H::Value, H::Error> {
    refresh(h, owner, true)
}
pub fn spot_account_refresh<H: AccountRefreshHost>(
    h: &H,
    owner: H::Value,
) -> SessionFuture<'_, H::Value, H::Error> {
    refresh(h, owner, false)
}
fn snapshot<H: AccountRefreshHost>(
    h: &H,
    owner: H::Value,
) -> SessionFuture<'_, H::Value, H::Error> {
    let request = (|| Ok(call!(h, owner, "fetchAndSetPrivateSnapshotHelper", vec![])))();
    Box::pin(async move {
        h.wait(request?).await?;
        undefined(h)
    })
}
pub fn inverse_private_snapshot<H: AccountRefreshHost>(
    h: &H,
    owner: H::Value,
) -> SessionFuture<'_, H::Value, H::Error> {
    snapshot(h, owner)
}
pub fn spot_private_snapshot<H: AccountRefreshHost>(
    h: &H,
    owner: H::Value,
) -> SessionFuture<'_, H::Value, H::Error> {
    snapshot(h, owner)
}
async fn snapshot_helper<H: AccountRefreshHost>(
    h: &H,
    owner: &H::Value,
    fetch_orders: H::Value,
    inverse: bool,
) -> Result<H::Value, H::Error> {
    let fetch_orders = if h.equal(&fetch_orders, &h.undefined()) {
        h.boolean(true)
    } else {
        fetch_orders
    };
    let run = async {
        if h.truthy(&fetch_orders) {
            let request = call!(h, owner.clone(), "fetchOrders", vec![]);
            let orders = h.wait(request).await?;
            if h.truthy(&h.get(owner, "isDisposed")?) {
                return undefined(h);
            }
            publish(h, owner, "orders", Some(orders))?;
        }
        let request = call!(h, owner.clone(), "fetchPositions", vec![]);
        let fetched = h.wait(request).await?;
        if h.truthy(&h.get(owner, "isDisposed")?) {
            return undefined(h);
        }
        positions(h, owner, fetched, inverse)?;
        let request = call!(h, owner.clone(), "fetchBalance", vec![]);
        let balance = h.wait(request).await?;
        if h.truthy(&h.get(owner, "isDisposed")?) {
            return undefined(h);
        }
        publish(h, owner, "balance", Some(balance))?;
        undefined(h)
    }
    .await;
    match run {
        Ok(v) => Ok(v),
        Err(error) => {
            let selected = h.select(owner.clone(), "emitAndLogError")?;
            h.apply(selected, vec![h.error_value(error)])?;
            undefined(h)
        }
    }
}
pub async fn inverse_snapshot_helper<H: AccountRefreshHost>(
    h: &H,
    owner: &H::Value,
    fetch_orders: H::Value,
) -> Result<H::Value, H::Error> {
    snapshot_helper(h, owner, fetch_orders, true).await
}
pub async fn spot_snapshot_helper<H: AccountRefreshHost>(
    h: &H,
    owner: &H::Value,
    fetch_orders: H::Value,
) -> Result<H::Value, H::Error> {
    snapshot_helper(h, owner, fetch_orders, false).await
}
async fn tick<H: AccountRefreshHost>(
    h: &H,
    owner: &H::Value,
    retry_base: f64,
) -> Result<H::Value, H::Error> {
    if !h.truthy(&h.get(owner, "isDisposed")?) {
        let reinit = h.truthy(&h.get(owner, "needsReinit")?);
        let due = if reinit {
            let now = call!(h, h.intrinsic("Date", vec![])?, "now", vec![]);
            let last = h.get(owner, "lastReinitAt")?;
            let retry = h.get(owner, "reinitRetryMs")?;
            h.numeric(&now)? - h.numeric(&last)? >= h.numeric(&retry)?
        } else {
            false
        };
        if due {
            h.set(owner, "needsReinit", h.boolean(false))?;
            let now = call!(h, h.intrinsic("Date", vec![])?, "now", vec![]);
            h.set(owner, "lastReinitAt", now)?;
            let request = call!(h, owner.clone(), "fetchOrdersPositionsAndBalance", vec![]);
            h.wait(request).await?;
            let retry = if h.truthy(&h.get(owner, "needsReinit")?) {
                let min = h.select(h.intrinsic("Math", vec![])?, "min")?;
                let value = h.numeric(&h.get(owner, "reinitRetryMs")?)? * 2.0;
                h.apply(min, vec![h.number(value), h.number(300_000.0)])?
            } else {
                h.number(retry_base)
            };
            h.set(owner, "reinitRetryMs", retry)?;
        }
        let run = async {
            let request = call!(
                h,
                owner.clone(),
                "fetchAndSetPrivateSnapshotHelper",
                vec![h.boolean(false)]
            );
            h.wait(request).await?;
            Ok::<(), H::Error>(())
        }
        .await;
        if let Err(error) = run {
            let log = h.select(owner.clone(), "emitAndLogError")?;
            let message = h.optional(&h.error_value(error), "message")?;
            h.apply(log, vec![message])?;
        }
        if !h.truthy(&h.get(owner, "isDisposed")?) {
            let loop_fn = h.select(h.global(), "loop")?;
            let callback = h.callback(owner.clone(), "account-tick-again", vec![])?;
            let interval = call!(h, owner.clone(), "getTickInterval", vec![]);
            h.apply(loop_fn, vec![callback, interval])?;
        }
    }
    undefined(h)
}
pub async fn inverse_tick<H: AccountRefreshHost>(
    h: &H,
    owner: &H::Value,
) -> Result<H::Value, H::Error> {
    tick(h, owner, 15_000.0).await
}
pub async fn spot_tick<H: AccountRefreshHost>(
    h: &H,
    owner: &H::Value,
) -> Result<H::Value, H::Error> {
    tick(h, owner, 15_000.0).await
}

pub fn spot_positions<H: AccountRefreshHost>(h: &H, _symbols: H::Value) -> H::Value {
    h.array(vec![])
}
