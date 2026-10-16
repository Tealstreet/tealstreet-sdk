use crate::spot_order_writes::SpotOrderHost;

fn call<H: SpotOrderHost>(
    h: &H,
    owner: H::Value,
    key: &str,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    let selected = h.select(owner, key)?;
    h.apply(selected, args)
}
pub fn apply_local_order_update_after_rest_write<H: SpotOrderHost>(
    h: &H,
    owner: &H::Value,
    order: H::Value,
    update: H::Value,
) -> Result<(), H::Error> {
    if !h.truthy(&call(
        h,
        owner.clone(),
        "shouldUseLocalRestOrderLifecycleFallback",
        vec![],
    )?) {
        return Ok(());
    }
    let next = h.spread(order.clone())?;
    let updated = h.spread(update.clone())?;
    h.intrinsic("copyOwnDataProperties", vec![next.clone(), updated])?;
    if h.equal(
        &h.intrinsic("typeof", vec![h.get(&update, "amount")?])?,
        &h.literal("number"),
    ) {
        let maximum = h.select(h.get(&h.global(), "Math")?, "max")?;
        let amount = h.get(&update, "amount")?;
        let filled = h.get(&order, "filled")?;
        let filled = if h.truthy(&filled) {
            filled
        } else {
            h.number(0.)
        };
        let remaining = h.intrinsic("subtract", vec![amount, filled])?;
        let remaining = h.apply(maximum, vec![h.number(0.), remaining])?;
        h.define(&next, "remaining", remaining)?;
    }
    let now = call(
        h,
        h.get(&h.global(), "virtualClock")?,
        "getCurrentTimeMs",
        vec![],
    )?;
    h.define(&next, "lastUpdate", now)?;
    let get = h.select(h.get(owner, "recentlyPlacedOrders")?, "get")?;
    let id = h.get(&order, "id")?;
    let recent = h.apply(get, vec![id])?;
    if h.truthy(&recent) {
        let set = h.select(h.get(owner, "recentlyPlacedOrders")?, "set")?;
        let id = h.get(&order, "id")?;
        let entry = h.spread(recent)?;
        h.define(&entry, "order", next.clone())?;
        h.apply(set, vec![id, entry])?;
    }
    call(
        h,
        h.get(owner, "store")?,
        "addOrUpdateOrders",
        vec![h.array(vec![next])],
    )?;
    Ok(())
}
