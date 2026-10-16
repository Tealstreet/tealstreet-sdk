use crate::core_snapshot_readers::SnapshotHost;
use std::rc::Rc;

const RECENTLY_PLACED_ORDER_GRACE_MS: f64 = 8_000.;

fn call<H: SnapshotHost>(
    h: &H,
    owner: H::Value,
    key: &str,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    let selected = h.select(owner, key)?;
    h.apply(selected, args)
}
fn nullish<H: SnapshotHost>(h: &H, value: &H::Value) -> Result<bool, H::Error> {
    Ok(h.equal(value, &h.undefined()) || h.equal(value, &h.intrinsic("null", vec![])?))
}
pub fn reconciliation_order_matches<H: SnapshotHost>(
    h: &H,
    id: &H::Value,
    row: &H::Value,
) -> Result<H::Value, H::Error> {
    Ok(h.boolean(h.equal(&h.get(row, "id")?, id)))
}
pub fn reconcile_local_reduce_only_limit_order<H: SnapshotHost>(
    h: Rc<H>,
    owner: H::Value,
    id: H::Value,
) -> H::Value {
    let launcher = h.clone();
    launcher.launch(Box::pin(async move {
        let entry = call(
            h.as_ref(),
            h.get(&owner, "recentlyPlacedOrders")?,
            "get",
            vec![id.clone()],
        )?;
        if !h.truthy(&entry) || h.truthy(&h.get(&owner, "isDisposed")?) {
            return Ok(h.undefined());
        }
        let find = h.select(h.get(&h.get(&owner, "store")?, "orders")?, "find")?;
        let predicate = h.callback(
            owner.clone(),
            "reconciliation_order_matches",
            vec![id.clone()],
        )?;
        let found = h.apply(find, vec![predicate])?;
        let order = if nullish(h.as_ref(), &found)? {
            h.get(&entry, "order")?
        } else {
            found
        };
        if !h.truthy(&call(
            h.as_ref(),
            owner.clone(),
            "shouldReconcileLocalReduceOnlyLimitOrder",
            vec![order.clone()],
        )?) {
            return Ok(h.undefined());
        }
        let result = async {
            let fills = call(
                h.as_ref(),
                owner.clone(),
                "fetchFillsForOrderIds",
                vec![h.array(vec![id.clone()])],
            )?;
            let fills = h.wait(fills).await?;
            if h.truthy(&h.get(&owner, "isDisposed")?) {
                return Ok(h.undefined());
            }
            let found = call(h.as_ref(), fills, "get", vec![id.clone()])?;
            let filled = if nullish(h.as_ref(), &found)? {
                h.number(0.)
            } else {
                found
            };
            let target = call(
                h.as_ref(),
                owner.clone(),
                "getFilledOrderReconciliationTarget",
                vec![order],
            )?;
            let global = h.intrinsic("globalThis", vec![])?;
            let maximum = h.select(h.get(&global, "Math")?, "max")?;
            let scaled = h.number(h.numeric(&target)? * 1e-8);
            let tolerance = h.apply(maximum, vec![scaled, h.number(1e-12)])?;
            let full = if h.numeric(&target)? > 0. {
                let threshold = h.numeric(&target)? - h.numeric(&tolerance)?;
                h.numeric(&filled)? >= threshold
            } else {
                false
            };
            if full {
                call(h.as_ref(), owner.clone(), "markOrderSeen", vec![id.clone()])?;
                call(
                    h.as_ref(),
                    h.get(&owner, "store")?,
                    "removeOrders",
                    vec![h.array(vec![h.object(vec![("id", id.clone())])])],
                )?;
                return Ok(h.undefined());
            }
            let clock = h.select(h.get(&global, "virtualClock")?, "getCurrentTimeMs")?;
            let now = h.apply(clock, vec![])?;
            if h.numeric(&now)? - h.numeric(&h.get(&entry, "placedAt")?)?
                < RECENTLY_PLACED_ORDER_GRACE_MS
            {
                return Ok(h.undefined());
            }
            let orders = call(h.as_ref(), owner.clone(), "fetchOrders", vec![])?;
            let orders = h.wait(orders).await?;
            if h.truthy(&h.get(&owner, "isDisposed")?) {
                return Ok(h.undefined());
            }
            let update = h.select(h.get(&owner, "store")?, "update")?;
            let loaded = h.spread(
                h.get(&h.get(&owner, "store")?, "loaded")?,
                vec![("orders", h.boolean(true))],
            )?;
            h.apply(
                update,
                vec![h.object(vec![("orders", orders), ("loaded", loaded)])],
            )?;
            Ok(h.undefined())
        }
        .await;
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                let log = h.select(owner, "log")?;
                let id = h.intrinsic("template", vec![id])?;
                let error = h.error_value(error);
                let message = h.optional(&error, "message")?;
                let message = if h.truthy(&message) { message } else { error };
                let message = h.intrinsic("template", vec![message])?;
                let text = h.intrinsic(
                    "string.concat",
                    vec![
                        h.literal("Failed to reconcile local Bybit reduce-only limit order "),
                        id,
                        h.literal(": "),
                        message,
                    ],
                )?;
                let global = h.intrinsic("globalThis", vec![])?;
                let warning = h.get(&h.get(&global, "LogSeverity")?, "Warning")?;
                h.apply(log, vec![text, warning])?;
                Ok(h.undefined())
            }
        }
    }))
}
