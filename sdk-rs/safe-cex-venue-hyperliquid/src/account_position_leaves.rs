//! Account guards and position configuration/cache leaves; full exchange and
//! virtual generator/collection/primitive implementations remain host contracts.
use crate::{spot_json::JsString, ws_ping_lifecycle::WsPingHost};
#[derive(Clone, Debug)]
pub struct OppositePredicate<V> {
    pub symbol: V,
    pub target_side: V,
    pub amount: V,
}
pub trait AccountPositionHost: WsPingHost {
    fn null(&self) -> Self::Value;
    fn boolean(&self, value: bool) -> Self::Value;
    fn number(&self, value: f64) -> Self::Value;
    fn is_nullish(&self, value: &Self::Value) -> bool;
    fn strict_equal(&self, left: &Self::Value, right: &Self::Value) -> bool;
    fn greater_equal(
        &mut self,
        left: &Self::Value,
        right: &Self::Value,
    ) -> Result<bool, Self::Error>;
    /// Allocate a lexical Array.some predicate. The native some adapter invokes
    /// `run_opposite_predicate` with this capture for each candidate position.
    fn opposite_predicate(
        &mut self,
        capture: OppositePredicate<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
}
fn text(s: &str) -> JsString {
    JsString(s.encode_utf16().collect())
}
fn literal<H: WsPingHost>(h: &H, s: &str) -> H::Value {
    h.literal(&text(s))
}
fn method<H: WsPingHost>(
    h: &mut H,
    receiver: &H::Value,
    name: &str,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    let function = h.get(receiver, name)?;
    h.call(&function, receiver, args)
}
fn enum_value<H: WsPingHost>(h: &mut H, name: &str, member: &str) -> Result<H::Value, H::Error> {
    let enumeration = h.global(name)?;
    h.get(&enumeration, member)
}
pub fn should_use_spot_collateral_balance<H: AccountPositionHost>(
    h: &mut H,
    exchange: &H::Value,
) -> Result<bool, H::Error> {
    let first = h.get(exchange, "userAbstractionMode")?;
    let unified = literal(h, "unifiedAccount");
    if h.strict_equal(&first, &unified) {
        return Ok(true);
    }
    let second = h.get(exchange, "userAbstractionMode")?;
    let portfolio = literal(h, "portfolioMargin");
    Ok(h.strict_equal(&second, &portfolio))
}
pub fn should_request_portfolio_margin_spot_state<H: AccountPositionHost>(
    h: &mut H,
    exchange: &H::Value,
) -> Result<bool, H::Error> {
    let mode = h.get(exchange, "userAbstractionMode")?;
    let portfolio = literal(h, "portfolioMargin");
    Ok(h.strict_equal(&mode, &portfolio))
}
pub fn should_track_spot_holdings<H: AccountPositionHost>(
    h: &mut H,
    exchange: &H::Value,
) -> Result<H::Value, H::Error> {
    let collateral = method(h, exchange, "shouldUseSpotCollateralBalance", &[])?;
    if h.truthy(&collateral) {
        return Ok(collateral);
    }
    let boolean = h.global("Boolean")?;
    let options = h.get(exchange, "options")?;
    let enable = h.get(&options, "enableSpotMarkets")?;
    let receiver = h.undefined();
    h.call(&boolean, &receiver, &[enable])
}
pub fn symbol_configuration_from_asset_data<H: AccountPositionHost>(
    h: &mut H,
    exchange: &H::Value,
    coin: &H::Value,
) -> Result<H::Value, H::Error> {
    let map = h.get(exchange, "activeAssetData")?;
    let has = method(h, &map, "has", &[coin.clone()])?;
    if h.truthy(&has) {
        let map = h.get(exchange, "activeAssetData")?;
        let data = method(h, &map, "get", &[coin.clone()])?;
        let result = h.object()?;
        // Optional chaining guards only activeAssetData; missing leverage on a
        // non-nullish record still throws at .type / .value as in the source.
        let leverage_type = if h.is_nullish(&data) {
            h.undefined()
        } else {
            let leverage = h.get(&data, "leverage")?;
            h.get(&leverage, "type")?
        };
        let isolated = literal(h, "isolated");
        let member = if h.strict_equal(&leverage_type, &isolated) {
            "Isolated"
        } else {
            "Cross"
        };
        let margin = enum_value(h, "MarginType", member)?;
        h.define_data(&result, "marginType", &margin)?;
        let mode = enum_value(h, "PositionMode", "OneWay")?;
        h.define_data(&result, "positionMode", &mode)?;
        let leverage = if h.is_nullish(&data) {
            h.undefined()
        } else {
            let leverage = h.get(&data, "leverage")?;
            h.get(&leverage, "value")?
        };
        let leverage = if h.truthy(&leverage) {
            leverage
        } else {
            h.number(20.0)
        };
        h.define_data(&result, "leverage", &leverage)?;
        return Ok(result);
    }
    let class = h.global("HyperliquidExchange")?;
    h.get(&class, "DEFAULT_SYMBOL_CONFIG")
}
pub fn spot_symbol_configuration_from_asset_data<H: WsPingHost>(
    h: &mut H,
) -> Result<H::Value, H::Error> {
    let class = h.global("HyperliquidSpotExchange")?;
    h.get(&class, "SPOT_SYMBOL_CONFIG")
}
pub fn get_fake_positions<H: AccountPositionHost>(
    h: &mut H,
    exchange: &H::Value,
) -> Result<H::Value, H::Error> {
    let cache = h.get(exchange, "_fakePositionsCache")?;
    if !h.truthy(&cache) {
        let generate = h.get(exchange, "generateFakePositionsFromMarkets")?;
        let store = h.get(exchange, "store")?;
        let markets = h.get(&store, "markets")?;
        let value = h.call(&generate, exchange, &[markets])?;
        h.set(exchange, "_fakePositionsCache", &value)?;
    }
    h.get(exchange, "_fakePositionsCache")
}
pub fn invalidate_fake_positions_cache<H: AccountPositionHost>(
    h: &mut H,
    exchange: &H::Value,
) -> Result<(), H::Error> {
    let null = h.null();
    h.set(exchange, "_fakePositionsCache", &null)
}
pub fn spot_get_fake_positions<H: WsPingHost>(
    h: &mut H,
    exchange: &H::Value,
) -> Result<H::Value, H::Error> {
    let generate = h.get(exchange, "generateFakePositionsFromMarkets")?;
    let store = h.get(exchange, "store")?;
    let markets = h.get(&store, "markets")?;
    h.call(&generate, exchange, &[markets])
}
pub fn resolve_reduce_only<H: AccountPositionHost>(
    h: &mut H,
    market: &H::Value,
    requested: &H::Value,
) -> Result<H::Value, H::Error> {
    let spot = h.get(market, "isSpot")?;
    Ok(if h.truthy(&spot) {
        h.boolean(false)
    } else {
        requested.clone()
    })
}
pub fn has_full_sized_position_opposite_side<H: AccountPositionHost>(
    h: &mut H,
    exchange: &H::Value,
    symbol: &H::Value,
    side: &H::Value,
    amount: &H::Value,
) -> Result<H::Value, H::Error> {
    let buy = enum_value(h, "OrderSide", "Buy")?;
    let side = if h.strict_equal(side, &buy) {
        "Short"
    } else {
        "Long"
    };
    let target_side = enum_value(h, "PositionSide", side)?;
    let store = h.get(exchange, "store")?;
    let positions = h.get(&store, "positions")?;
    let some = h.get(&positions, "some")?;
    let callback = h.opposite_predicate(OppositePredicate {
        symbol: symbol.clone(),
        target_side,
        amount: amount.clone(),
    })?;
    h.call(&some, &positions, &[callback])
}
pub fn run_opposite_predicate<H: AccountPositionHost>(
    h: &mut H,
    capture: &OppositePredicate<H::Value>,
    position: &H::Value,
) -> Result<bool, H::Error> {
    let symbol = h.get(position, "symbol")?;
    if !h.strict_equal(&symbol, &capture.symbol) {
        return Ok(false);
    }
    let side = h.get(position, "side")?;
    if !h.strict_equal(&side, &capture.target_side) {
        return Ok(false);
    }
    let contracts = h.get(position, "contracts")?;
    let zero = h.number(0.0);
    if h.strict_equal(&contracts, &zero) {
        return Ok(false);
    }
    let contracts = h.get(position, "contracts")?;
    h.greater_equal(&contracts, &capture.amount)
}
