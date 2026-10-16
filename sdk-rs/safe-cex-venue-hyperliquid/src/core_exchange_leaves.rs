//! Raw receiver versions of the exchange's state and placement selectors.
use crate::{
    spot_account_reads::SpotAccountHost, spot_json::JsString, ws_ping_lifecycle::WsPingHost,
};
#[derive(Clone, Debug)]
pub enum ExchangePredicate<V> {
    FinitePositive,
    Position { market: V, side: V },
}
pub trait CoreExchangeLeafHost: SpotAccountHost {
    fn typeof_value(&self, value: &Self::Value) -> &'static str;
    /// Module singleton invoking run_exchange_predicate(FinitePositive).
    /// Repeated reads retain the same native function identity.
    fn finite_positive_predicate(&mut self) -> Result<Self::Value, Self::Error>;
    /// Allocate a native lexical arrow invoking run_exchange_predicate. The
    /// position arrow captures original market/side handles; both have arity1.
    fn exchange_predicate(
        &mut self,
        capture: ExchangePredicate<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
}
fn method<H: WsPingHost>(
    h: &mut H,
    r: &H::Value,
    name: &str,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    let f = h.get(r, name)?;
    h.call(&f, r, args)
}
fn finite_positive<H: CoreExchangeLeafHost>(h: &mut H, v: &H::Value) -> Result<H::Value, H::Error> {
    if h.typeof_value(v) != "number" {
        return Ok(SpotAccountHost::boolean(h, false));
    }
    let number = h.global("Number")?;
    let finite = method(h, &number, "isFinite", &[v.clone()])?;
    if !h.truthy(&finite) {
        return Ok(finite);
    }
    SpotAccountHost::binary(h, ">", v, &SpotAccountHost::number(h, 0.))
}
pub fn run_exchange_predicate<H: CoreExchangeLeafHost>(
    h: &mut H,
    c: &ExchangePredicate<H::Value>,
    value: &H::Value,
) -> Result<H::Value, H::Error> {
    match c {
        ExchangePredicate::FinitePositive => finite_positive(h, value),
        ExchangePredicate::Position { market, side } => {
            let symbol = h.get(value, "symbol")?;
            let expected = h.get(market, "symbol")?;
            if !h.strict_equal(&symbol, &expected) {
                return Ok(SpotAccountHost::boolean(h, false));
            }
            let actual = h.get(value, "side")?;
            if !h.strict_equal(&actual, side) {
                return Ok(SpotAccountHost::boolean(h, false));
            }
            let contracts = h.get(value, "contracts")?;
            SpotAccountHost::binary(h, ">", &contracts, &SpotAccountHost::number(h, 0.))
        }
    }
}
pub fn generate_nonce<H: CoreExchangeLeafHost>(
    h: &mut H,
    e: &H::Value,
) -> Result<H::Value, H::Error> {
    let date = h.global("Date")?;
    let now = method(h, &date, "now", &[])?;
    let math = h.global("Math")?;
    let max = h.get(&math, "max")?;
    let last = h.get(e, "lastNonce")?;
    let one = SpotAccountHost::number(h, 1.);
    let next = SpotAccountHost::binary(h, "+", &last, &one)?;
    let value = h.call(&max, &math, &[now, next])?;
    h.set(e, "lastNonce", &value)?;
    h.get(e, "lastNonce")
}
pub fn has_private_session_identity<H: CoreExchangeLeafHost>(
    h: &mut H,
    e: &H::Value,
) -> Result<H::Value, H::Error> {
    let boolean = h.global("Boolean")?;
    let opts = h.get(e, "options")?;
    let vault = h.get(&opts, "vault")?;
    let identity = if h.truthy(&vault) {
        vault
    } else {
        let opts = h.get(e, "options")?;
        h.get(&opts, "applicationId")?
    };
    h.call(&boolean, &h.undefined(), &[identity])
}
pub fn is_public_data_only<H: CoreExchangeLeafHost>(
    h: &mut H,
    e: &H::Value,
) -> Result<H::Value, H::Error> {
    let opts = h.get(e, "options")?;
    let explicit = h.get(&opts, "publicDataOnly")?;
    if h.strict_equal(&explicit, &SpotAccountHost::boolean(h, true)) {
        return Ok(SpotAccountHost::boolean(h, true));
    }
    let identity = h.get(e, "hasPrivateSessionIdentity")?;
    Ok(SpotAccountHost::boolean(h, !h.truthy(&identity)))
}
pub fn get_order_base_price<H: CoreExchangeLeafHost>(
    h: &mut H,
    e: &H::Value,
    market: &H::Value,
    purpose: &H::Value,
) -> Result<H::Value, H::Error> {
    let store = h.get(e, "store")?;
    let table = h.get(&store, "tickersBySymbol")?;
    let symbol = h.get(market, "symbol")?;
    let ticker = SpotAccountHost::index(h, &table, &symbol)?;
    let spot = h.get(market, "isSpot")?;
    let keys = if h.truthy(&spot) {
        ["last", "midPrice", "mark", "index"]
    } else {
        ["mark", "last", "midPrice", "index"]
    };
    let mut values = Vec::with_capacity(4);
    for key in keys {
        values.push(if SpotAccountHost::is_nullish(h, &ticker) {
            h.undefined()
        } else {
            h.get(&ticker, key)?
        })
    }
    let candidates = SpotAccountHost::array(h, &values)?;
    let find = h.get(&candidates, "find")?;
    let predicate = h.finite_positive_predicate()?;
    let price = h.call(&find, &candidates, &[predicate])?;
    if !h.truthy(&price) {
        let error = h.global("Error")?;
        let mut text = "Cannot place Hyperliquid "
            .encode_utf16()
            .collect::<Vec<_>>();
        text.extend(SpotAccountHost::template_string(h, purpose)?.0);
        text.extend(" for ".encode_utf16());
        let symbol = h.get(market, "symbol")?;
        text.extend(SpotAccountHost::template_string(h, &symbol)?.0);
        text.extend(": missing valid ticker price data".encode_utf16());
        let message = h.literal(&JsString(text));
        let error = SpotAccountHost::construct(h, &error, &[message])?;
        return Err(SpotAccountHost::throw_value(h, error));
    }
    Ok(price)
}
pub fn should_use_position_tpsl<H: CoreExchangeLeafHost>(
    h: &mut H,
    e: &H::Value,
    opts: &H::Value,
    market: &H::Value,
    size: &H::Value,
) -> Result<H::Value, H::Error> {
    let amount = h.get(opts, "amount")?;
    if h.strict_equal(&amount, &SpotAccountHost::number(h, 0.)) {
        return Ok(SpotAccountHost::boolean(h, true));
    }
    let reduce = h.get(opts, "reduceOnly")?;
    if !h.truthy(&reduce) {
        return Ok(SpotAccountHost::boolean(h, false));
    }
    let finite = finite_positive(h, size)?;
    if !h.truthy(&finite) {
        return Ok(SpotAccountHost::boolean(h, false));
    }
    let side = h.get(opts, "side")?;
    let sides = SpotAccountHost::imported(h, "OrderSide")?;
    let buy = h.get(&sides, "Buy")?;
    let buy = h.strict_equal(&side, &buy);
    let sides = SpotAccountHost::imported(h, "PositionSide")?;
    let side = h.get(&sides, if buy { "Short" } else { "Long" })?;
    let store = h.get(e, "store")?;
    let positions = h.get(&store, "positions")?;
    let find = h.get(&positions, "find")?;
    let pred = h.exchange_predicate(ExchangePredicate::Position {
        market: market.clone(),
        side,
    })?;
    let position = h.call(&find, &positions, &[pred])?;
    if !h.truthy(&position) {
        return Ok(SpotAccountHost::boolean(h, false));
    }
    let contracts = h.get(&position, "contracts")?;
    SpotAccountHost::binary(h, ">=", size, &contracts)
}
pub fn remember_limit_order_time_in_force<H: CoreExchangeLeafHost>(
    h: &mut H,
    e: &H::Value,
    id: &H::Value,
    tif: &H::Value,
) -> Result<(), H::Error> {
    let map = h.get(e, "_placedLimitOrderTif")?;
    method(h, &map, "delete", &[id.clone()])?;
    let map = h.get(e, "_placedLimitOrderTif")?;
    method(h, &map, "set", &[id.clone(), tif.clone()])?;
    loop {
        let map = h.get(e, "_placedLimitOrderTif")?;
        let size = h.get(&map, "size")?;
        let cap = h.get(e, "PLACED_TIF_CACHE_LIMIT")?;
        let over = SpotAccountHost::binary(h, ">", &size, &cap)?;
        if !h.truthy(&over) {
            break;
        }
        let map = h.get(e, "_placedLimitOrderTif")?;
        let keys = method(h, &map, "keys", &[])?;
        let oldest = method(h, &keys, "next", &[])?;
        let done = h.get(&oldest, "done")?;
        if h.truthy(&done) {
            break;
        }
        let map = h.get(e, "_placedLimitOrderTif")?;
        let delete = h.get(&map, "delete")?;
        let value = h.get(&oldest, "value")?;
        h.call(&delete, &map, &[value])?;
    }
    Ok(())
}
pub fn get_placed_limit_order_time_in_force<H: CoreExchangeLeafHost>(
    h: &mut H,
    e: &H::Value,
    id: &H::Value,
) -> Result<H::Value, H::Error> {
    let map = h.get(e, "_placedLimitOrderTif")?;
    method(h, &map, "get", &[id.clone()])
}
pub fn forget_limit_order_time_in_force<H: CoreExchangeLeafHost>(
    h: &mut H,
    e: &H::Value,
    id: &H::Value,
) -> Result<(), H::Error> {
    let map = h.get(e, "_placedLimitOrderTif")?;
    method(h, &map, "delete", &[id.clone()])?;
    Ok(())
}
pub fn is_order_cancelled<H: CoreExchangeLeafHost>(
    h: &mut H,
    e: &H::Value,
    id: &H::Value,
) -> Result<H::Value, H::Error> {
    let map = h.get(e, "_cancelledOrders")?;
    let time = method(h, &map, "get", &[id.clone()])?;
    if SpotAccountHost::is_undefined(h, &time) {
        return Ok(SpotAccountHost::boolean(h, false));
    }
    let date = h.global("Date")?;
    let now = method(h, &date, "now", &[])?;
    let elapsed = SpotAccountHost::binary(h, "-", &now, &time)?;
    let limit = h.get(e, "CANCEL_SUPPRESSION_MAX_AGE_MS")?;
    let expired = SpotAccountHost::binary(h, ">", &elapsed, &limit)?;
    if h.truthy(&expired) {
        let map = h.get(e, "_cancelledOrders")?;
        method(h, &map, "delete", &[id.clone()])?;
        return Ok(SpotAccountHost::boolean(h, false));
    }
    Ok(SpotAccountHost::boolean(h, true))
}
pub fn suppress_cancelled_order<H: CoreExchangeLeafHost>(
    h: &mut H,
    e: &H::Value,
    id: &H::Value,
) -> Result<(), H::Error> {
    let map = h.get(e, "_cancelledOrders")?;
    let set = h.get(&map, "set")?;
    let date = h.global("Date")?;
    let now = method(h, &date, "now", &[])?;
    h.call(&set, &map, &[id.clone(), now])?;
    Ok(())
}
pub fn unsuppress_cancelled_order<H: CoreExchangeLeafHost>(
    h: &mut H,
    e: &H::Value,
    id: &H::Value,
) -> Result<(), H::Error> {
    let map = h.get(e, "_cancelledOrders")?;
    method(h, &map, "delete", &[id.clone()])?;
    Ok(())
}
