//! Lighter position snapshot mapping and one-way placeholders.
use crate::{
    exchange_trading_preflight::TradingHost, sdk_account_api::AccountValue,
    transaction_promise::NativePromise, transaction_signing::SigningResult,
};
use std::rc::Rc;
pub trait PositionForIn {
    fn next_key(&mut self) -> SigningResult<Option<AccountValue>>;
}
pub trait PositionReadHost: TradingHost {
    /// PutValue for a computed reference whose base and raw key were captured
    /// before the RHS. Base validity, ToPropertyKey and Set occur here afterward.
    fn assign_index(
        &self,
        object: &AccountValue,
        key: AccountValue,
        value: AccountValue,
    ) -> SigningResult<()>;
    /// Live cursor; enumeration must interleave with body mutations as in JS.
    fn for_in(&self, object: &AccountValue) -> SigningResult<Box<dyn PositionForIn>>;
    /// Ordered array-literal iterator spreads, including abrupt IteratorClose.
    fn spread_array(&self, arrays: &[AccountValue]) -> SigningResult<AccountValue>;
}
fn text(s: &str) -> AccountValue {
    AccountValue::String(s.encode_utf16().collect())
}
fn arg(args: &[AccountValue], index: usize) -> AccountValue {
    args.get(index).cloned().unwrap_or(AccountValue::Undefined)
}
fn get(h: &dyn PositionReadHost, key: &str) -> SigningResult<AccountValue> {
    h.get(&h.owner(), key)
}
fn call(h: &dyn PositionReadHost, key: &str, args: &[AccountValue]) -> SigningResult<AccountValue> {
    h.member(&h.owner(), key)?.call(args)
}
fn market(h: &dyn PositionReadHost, position: &AccountValue) -> SigningResult<AccountValue> {
    let raw_id = h.get(position, "marketId")?;
    let raw_id = if raw_id.is_nullish() {
        h.get(position, "market_id")?
    } else {
        raw_id
    };
    let store = get(h, "store")?;
    let by_id = h.get(&store, "marketsById")?;
    let found = h.index(&by_id, raw_id)?;
    if found.truthy() {
        return Ok(found);
    }
    let store = get(h, "store")?;
    let by_symbol = h.get(&store, "marketsBySymbol")?;
    h.index(&by_symbol, h.get(position, "symbol")?)
}
fn mapped(h: Rc<dyn PositionReadHost>, account: AccountValue) -> SigningResult<AccountValue> {
    let array = h.global("Array.isArray")?;
    let candidate = if account.is_nullish() {
        AccountValue::Undefined
    } else {
        h.get(&account, "positions")?
    };
    let raw_positions = if array.call(&[candidate])?.truthy() {
        h.get(&account, "positions")?
    } else {
        h.array(&[])?
    };
    let map = h.member(&raw_positions, "map")?;
    let mapper = get(h.as_ref(), "mapPosition")?;
    let bound = h.member(&mapper, "bind")?.call(&[h.owner()])?;
    let positions = map.call(&[bound])?;
    let filter = h.member(&positions, "filter")?;
    let callback = h.function(Rc::new(|args| {
        Ok(AccountValue::Boolean(!matches!(
            arg(args, 0),
            AccountValue::Null
        )))
    }))?;
    let positions = filter.call(&[callback])?;
    let positions = if positions.truthy() {
        positions
    } else {
        h.array(&[])?
    };
    let reduce = h.member(&raw_positions, "reduce")?;
    let live = h.clone();
    let callback = h.function(Rc::new(move |args| {
        let acc = arg(args, 0);
        let position = arg(args, 1);
        let market = market(live.as_ref(), &position)?;
        if !market.truthy() {
            return Ok(acc);
        }
        let key = live.get(&market, "symbol")?;
        let value = call(
            live.as_ref(),
            "resolvePositionLeverage",
            &[position, market],
        )?;
        live.assign_index(&acc, key, value)?;
        Ok(acc)
    }))?;
    let leverage_by_symbol = reduce.call(&[callback, h.literal(&[])?])?;
    let reduce = h.member(&raw_positions, "reduce")?;
    let live = h.clone();
    let callback = h.function(Rc::new(move |args| {
        let acc = arg(args, 0);
        let position = arg(args, 1);
        let market = market(live.as_ref(), &position)?;
        if !market.truthy() {
            return Ok(acc);
        }
        let parse = live.member(&live.owner(), "parseMarginMode")?;
        let raw = live.get(&position, "marginMode")?;
        let raw = if raw.is_nullish() {
            live.get(&position, "margin_mode")?
        } else {
            raw
        };
        let parsed = parse.call(&[raw])?;
        if !matches!(parsed, AccountValue::Undefined) {
            live.assign_index(&acc, live.get(&market, "symbol")?, parsed)?;
        }
        Ok(acc)
    }))?;
    let margin_mode_by_symbol = reduce.call(&[callback, h.literal(&[])?])?;
    let mut keys = h.for_in(&margin_mode_by_symbol)?;
    while let Some(symbol) = keys.next_key()? {
        let margin_mode = h.index(&margin_mode_by_symbol, symbol.clone())?;
        if matches!(margin_mode, AccountValue::Undefined) {
            continue;
        }
        let modes = get(h.as_ref(), "marginModeBySymbol")?;
        h.member(&modes, "set")?.call(&[symbol, margin_mode])?;
    }
    let store = get(h.as_ref(), "store")?;
    let markets = h.get(&store, "markets")?;
    let reduce = h.member(&markets, "reduce")?;
    let live = h.clone();
    let callback = h.function(Rc::new(move |args| {
        let acc = arg(args, 0);
        let m = arg(args, 1);
        let mode_type = live.member(&live.owner(), "marginModeToMarginType")?;
        let mode = live.index(&margin_mode_by_symbol, live.get(&m, "symbol")?)?;
        let mode = if mode.is_nullish() {
            let modes = get(live.as_ref(), "marginModeBySymbol")?;
            let get_mode = live.member(&modes, "get")?;
            get_mode.call(&[live.get(&m, "symbol")?])?
        } else {
            mode
        };
        let mode = if mode.is_nullish() {
            call(live.as_ref(), "getMarketConfiguredMarginMode", &[m.clone()])?
        } else {
            mode
        };
        let position_margin_type = mode_type.call(&[mode])?;
        let push = live.member(&acc, "push")?;
        let create_id = live.global("createPositionId")?;
        let id = create_id.call(&[
            live.get(&m, "symbol")?,
            live.constant("PositionSide.Long")?,
            live.constant("PositionMode.OneWay")?,
        ])?;
        let symbol = live.get(&m, "symbol")?;
        let side = live.constant("PositionSide.Long")?;
        let leverage = live.index(&leverage_by_symbol, live.get(&m, "symbol")?)?;
        let leverage = if leverage.truthy() {
            leverage
        } else {
            let limits = live.get(&m, "limits")?;
            let leverage = live.get(&limits, "leverage")?;
            let max = live.get(&leverage, "max")?;
            if max.truthy() {
                max
            } else {
                AccountValue::Number(10.0)
            }
        };
        let position = live.literal(&[
            ("id", id),
            ("symbol", symbol),
            ("side", side),
            ("entryPrice", AccountValue::Number(0.0)),
            ("notional", AccountValue::Number(0.0)),
            ("unrealizedPnl", AccountValue::Number(0.0)),
            ("contracts", AccountValue::Number(0.0)),
            ("leverage", leverage),
            ("liquidationPrice", AccountValue::Number(0.0)),
            ("positionMode", live.constant("PositionMode.OneWay")?),
            ("marginType", position_margin_type),
        ])?;
        push.call(&[position])?;
        Ok(acc)
    }))?;
    let fake_positions = reduce.call(&[callback, h.array(&[])?])?;
    let uniq = h.global("uniqBy")?;
    let all = h.spread_array(&[positions, fake_positions])?;
    uniq.call(&[all, text("id")])
}
/// ts: LighterExchange.fetchPositions. Symbols remain intentionally unused.
pub fn fetch_positions(
    h: Rc<dyn PositionReadHost>,
    _symbols: AccountValue,
    opts: AccountValue,
) -> NativePromise<AccountValue> {
    let (promise, resolver) = NativePromise::pending(h.runtime());
    let work = (|| {
        let opts = if matches!(opts, AccountValue::Undefined) {
            h.literal(&[])?
        } else {
            opts
        };
        let fetch = h.member(&h.owner(), "fetchAccountSnapshot")?;
        let allow_recent = h.get(&opts, "allowRecentAccountSnapshot")?;
        let request = h.literal(&[("allowRecent", allow_recent)])?;
        let value = fetch.call(&[request])?;
        h.await_value(value)
    })();
    match work {
        Err(error) => resolver.settle(Err(error)),
        Ok(p) => p.subscribe(move |value| {
            let result = value.and_then(|account| mapped(h.clone(), (*account).clone()));
            match result {
                Ok(value) => h.resolve_return(value, resolver),
                Err(error) => resolver.settle(Err(error)),
            }
        }),
    }
    promise
}
