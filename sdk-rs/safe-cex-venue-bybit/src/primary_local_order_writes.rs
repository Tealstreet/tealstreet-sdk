use crate::inverse_symbol::is_bybit_inverse_symbol;
use crate::spot_order_writes::{SpotOrderHost, record_with_remaining};

fn call<H: SpotOrderHost>(
    h: &H,
    owner: H::Value,
    key: &str,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    let selected = h.select(owner, key)?;
    h.apply(selected, args)
}
fn nullish<H: SpotOrderHost>(h: &H, value: &H::Value) -> Result<bool, H::Error> {
    Ok(h.equal(value, &h.undefined()) || h.equal(value, &h.intrinsic("null", vec![])?))
}
fn inverse<H: SpotOrderHost>(h: &H, symbol: &H::Value) -> Result<bool, H::Error> {
    let units = h.units(symbol)?;
    Ok(is_bybit_inverse_symbol(&String::from_utf16_lossy(&units)))
}
// ts: BybitExchange#remainingInStoreUnits — inverse rows hold `remaining` in the coin at
// `price` and the USD contracts in `extra.remainingNative`.
fn remaining_in_store_units<H: SpotOrderHost>(
    h: &H,
    symbol: &H::Value,
    remaining_native: H::Value,
    price: &H::Value,
) -> Result<Vec<(&'static str, H::Value)>, H::Error> {
    let price = h.numeric(price)?;
    if !inverse(h, symbol)? || price.is_nan() || price <= 0. {
        return Ok(vec![("remaining", remaining_native)]);
    }
    let coin = safe_cex_num::math_round((h.numeric(&remaining_native)? / price) * 1e8) / 1e8;
    Ok(vec![
        ("remaining", h.number(coin)),
        (
            "extra",
            h.object(vec![("remainingNative", remaining_native)]),
        ),
    ])
}
// ts: BybitExchange#recordRecentlyPlacedOrderFromOpts
pub fn record_recently_placed_order_from_opts<H: SpotOrderHost>(
    h: &H,
    owner: &H::Value,
    id: H::Value,
    opts: H::Value,
    payload: H::Value,
) -> Result<(), H::Error> {
    record_with_remaining(h, owner, id, opts, payload, |opts, qty, price| {
        remaining_in_store_units(h, &h.get(opts, "symbol")?, qty, &price)
    })
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
    let amended = h.equal(
        &h.intrinsic("typeof", vec![h.get(&update, "amount")?])?,
        &h.literal("number"),
    );
    let remaining_native = if amended {
        let maximum = h.select(h.get(&h.global(), "Math")?, "max")?;
        let amount = h.get(&update, "amount")?;
        let filled = h.get(&order, "filled")?;
        let filled = if h.truthy(&filled) {
            filled
        } else {
            h.number(0.)
        };
        let remaining = h.intrinsic("subtract", vec![amount, filled])?;
        h.apply(maximum, vec![h.number(0.), remaining])?
    } else {
        let extra = h.get(&order, "extra")?;
        let native = if nullish(h, &extra)? {
            h.undefined()
        } else {
            h.get(&extra, "remainingNative")?
        };
        if nullish(h, &native)? {
            h.get(&order, "remaining")?
        } else {
            native
        }
    };
    let symbol = h.get(&order, "symbol")?;
    let units = if inverse(h, &symbol)? {
        let price = h.get(&update, "price")?;
        let price = if nullish(h, &price)? {
            h.get(&order, "price")?
        } else {
            price
        };
        remaining_in_store_units(h, &symbol, remaining_native, &price)?
    } else if amended {
        vec![("remaining", remaining_native)]
    } else {
        vec![]
    };
    let next = h.spread(order.clone())?;
    let updated = h.spread(update.clone())?;
    h.intrinsic("copyOwnDataProperties", vec![next.clone(), updated])?;
    for (key, value) in units {
        if key == "extra" {
            let extra = h.spread(h.get(&order, "extra")?)?;
            h.intrinsic("copyOwnDataProperties", vec![extra.clone(), value])?;
            h.define(&next, "extra", extra)?;
        } else {
            h.define(&next, key, value)?;
        }
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
