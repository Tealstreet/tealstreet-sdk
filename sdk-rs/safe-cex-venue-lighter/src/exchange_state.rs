//! Exchange cache/current-state bodies. The owner and ECMAScript operations are
//! explicit required hosts; this does not instantiate the full exchange class.
use crate::{sdk_account_api::AccountValue, transaction_signing::SigningResult};
use num_bigint::BigInt;
use std::rc::Rc;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateCallKind {
    HintSet,
    HintGet,
    HintDelete,
    MarginGet,
    TierExtract,
    MarginTypeToMode,
    MarketMode,
    SnapTier,
    MathMin,
    MathMax,
    NumberFinite,
    Boolean,
}
/// Capture the source callee AND receiver at lookup, including a noncallable
/// value. Callability failure is deferred until arguments have been evaluated.
pub trait StateCall {
    fn call(&self, arguments: &[AccountValue]) -> SigningResult<AccountValue>;
}
/// Actual receiver-bound Array.find: snapshot length, visit holes/live values,
/// preserve callback argument identity and interpret its returned truthiness.
pub trait PositionFinder {
    fn find(
        &self,
        predicate: &mut dyn FnMut(AccountValue) -> SigningResult<AccountValue>,
    ) -> SigningResult<AccountValue>;
}
pub struct RecentAccountSnapshot {
    pub fetched_at: AccountValue,
    pub cache_version: f64,
    pub value: AccountValue,
}
pub trait ExchangeStateHost {
    fn callable(&self, kind: StateCallKind) -> SigningResult<Rc<dyn StateCall>>;
    /// Actual orderId.toString() lookup/call with BigInt receiver and no args.
    fn order_id_string(&self, order_id: &BigInt) -> SigningResult<AccountValue>;
    /// Fresh ordinary object + CopyDataProperties, including symbols and getters.
    fn copy_hint(&self, hint: &AccountValue) -> SigningResult<AccountValue>;
    /// Define own createdAt data property (not assignment through a setter).
    fn define_created_at(&self, hint: &AccountValue, value: AccountValue) -> SigningResult<()>;
    fn wall_now(&self) -> SigningResult<AccountValue>;
    fn virtual_now(&self) -> SigningResult<AccountValue>;
    fn hint_size(&self) -> SigningResult<f64>;
    /// Current placedOrderHints.keys().next().value, with all source lookups.
    fn oldest_hint_key(&self) -> SigningResult<AccountValue>;
    fn property(&self, value: &AccountValue, name: &str) -> SigningResult<AccountValue>;
    /// ECMAScript subtraction followed by the indicated numeric comparison;
    /// retain coercion order, BigInt/Symbol errors and original thrown identity.
    fn age_greater_than(
        &self,
        now: AccountValue,
        at: AccountValue,
        threshold: f64,
    ) -> SigningResult<bool>;
    fn age_less_than(
        &self,
        now: AccountValue,
        at: AccountValue,
        threshold: AccountValue,
    ) -> SigningResult<bool>;
    fn greater_than_zero(&self, value: AccountValue) -> SigningResult<bool>;
    fn cache_version(&self) -> SigningResult<f64>;
    fn set_cache_version(&self, value: f64) -> SigningResult<()>;
    /// Assign a fresh source { fetchedAt, cacheVersion, value }, or undefined.
    fn set_recent_snapshot(&self, value: Option<RecentAccountSnapshot>) -> SigningResult<()>;
    fn account_limits(&self) -> SigningResult<AccountValue>;
    fn orders_fetch_promise(&self) -> SigningResult<AccountValue>;
    fn last_orders_fetch_at(&self) -> SigningResult<f64>;
    fn market_by_symbol(&self, symbol: &[u16]) -> SigningResult<AccountValue>;
    fn positions_find(&self) -> SigningResult<Rc<dyn PositionFinder>>;
}
fn same_symbol(value: AccountValue, symbol: &[u16]) -> bool {
    matches!(value, AccountValue::String(v) if v==symbol)
}
fn false_value() -> AccountValue {
    AccountValue::Boolean(false)
}
// ts: LighterExchange#rememberPlacedOrderHint
pub fn remember_placed_order_hint(
    host: &dyn ExchangeStateHost,
    order_id: &BigInt,
    hint: &AccountValue,
) -> SigningResult<()> {
    let set = host.callable(StateCallKind::HintSet)?;
    let key = host.order_id_string(order_id)?;
    let copied = host.copy_hint(hint)?;
    let now = host.wall_now()?;
    host.define_created_at(&copied, now)?;
    set.call(&[key, copied])?;
    while host.hint_size()? > 2000.0 {
        let oldest = host.oldest_hint_key()?;
        if !oldest.truthy() {
            break;
        }
        host.callable(StateCallKind::HintDelete)?.call(&[oldest])?;
    }
    Ok(())
}
// ts: LighterExchange#readPlacedOrderHint
pub fn read_placed_order_hint(
    host: &dyn ExchangeStateHost,
    order_id: Option<&[u16]>,
) -> SigningResult<AccountValue> {
    let Some(order_id) = order_id.filter(|v| !v.is_empty()) else {
        return Ok(AccountValue::Undefined);
    };
    let key = AccountValue::String(order_id.to_vec());
    let hint = host
        .callable(StateCallKind::HintGet)?
        .call(&[key.clone()])?;
    if !hint.truthy() {
        return Ok(AccountValue::Undefined);
    }
    let now = host.wall_now()?;
    let created_at = host.property(&hint, "createdAt")?;
    if host.age_greater_than(now, created_at, 600_000.0)? {
        host.callable(StateCallKind::HintDelete)?.call(&[key])?;
        return Ok(AccountValue::Undefined);
    }
    Ok(hint)
}
// ts: LighterExchange#rememberRecentAccountSnapshot
pub fn remember_recent_account_snapshot(
    host: &dyn ExchangeStateHost,
    value: AccountValue,
    fetch_version: f64,
) -> SigningResult<()> {
    if fetch_version != host.cache_version()? {
        return Ok(());
    }
    let fetched_at = host.virtual_now()?;
    host.set_recent_snapshot(Some(RecentAccountSnapshot {
        fetched_at,
        cache_version: fetch_version,
        value,
    }))
}
// ts: LighterExchange.invalidateAccountSnapshotCache
pub fn invalidate_account_snapshot_cache(host: &dyn ExchangeStateHost) -> SigningResult<()> {
    let version = host.cache_version()?;
    host.set_cache_version(version + 1.0)?;
    host.set_recent_snapshot(None)
}
// ts: LighterExchange#getCachedAccountTier
pub fn get_cached_account_tier(host: &dyn ExchangeStateHost) -> SigningResult<AccountValue> {
    let extract = host.callable(StateCallKind::TierExtract)?;
    let limits = host.account_limits()?;
    extract.call(&[limits])
}
// ts: LighterExchange.shouldSkipOrderReconciliationFetch
pub fn should_skip_order_reconciliation_fetch(
    host: &dyn ExchangeStateHost,
    cooldown_ms: Option<AccountValue>,
) -> SigningResult<AccountValue> {
    let cooldown = match cooldown_ms {
        None | Some(AccountValue::Undefined) => AccountValue::Number(3000.0),
        Some(value) => value,
    };
    let boolean = host.callable(StateCallKind::Boolean)?;
    let promise = host.orders_fetch_promise()?;
    let current = boolean.call(&[promise])?;
    if current.truthy() {
        return Ok(current);
    }
    if host.last_orders_fetch_at()? > 0.0 {
        let now = host.virtual_now()?;
        let last = host.last_orders_fetch_at()?;
        return host
            .age_less_than(now, AccountValue::Number(last), cooldown)
            .map(AccountValue::Boolean);
    }
    Ok(AccountValue::Boolean(false))
}
// ts: LighterExchange#getCurrentMarginModeForSymbol
pub fn get_current_margin_mode_for_symbol(
    host: &dyn ExchangeStateHost,
    symbol: &[u16],
) -> SigningResult<AccountValue> {
    let cached = host
        .callable(StateCallKind::MarginGet)?
        .call(&[AccountValue::String(symbol.to_vec())])?;
    if !matches!(cached, AccountValue::Undefined) {
        return Ok(cached);
    }
    let market = host.market_by_symbol(symbol)?;
    let active = host.positions_find()?.find(&mut |position| {
        if !same_symbol(host.property(&position, "symbol")?, symbol) {
            return Ok(false_value());
        }
        Ok(AccountValue::Boolean(host.greater_than_zero(
            host.property(&position, "contracts")?,
        )?))
    })?;
    if active.truthy() {
        let convert = host.callable(StateCallKind::MarginTypeToMode)?;
        let margin_type = host.property(&active, "marginType")?;
        return convert.call(&[margin_type]);
    }
    let configured = host.callable(StateCallKind::MarketMode)?.call(&[market])?;
    if !matches!(configured, AccountValue::Undefined) {
        return Ok(configured);
    }
    let first = host.positions_find()?.find(&mut |position| {
        Ok(AccountValue::Boolean(same_symbol(
            host.property(&position, "symbol")?,
            symbol,
        )))
    })?;
    if first.truthy() {
        let convert = host.callable(StateCallKind::MarginTypeToMode)?;
        let margin_type = host.property(&first, "marginType")?;
        return convert.call(&[margin_type]);
    }
    Ok(AccountValue::Number(0.0))
}
fn finite_position(
    host: &dyn ExchangeStateHost,
    position: AccountValue,
    symbol: &[u16],
    require_exposure: bool,
) -> SigningResult<AccountValue> {
    if !same_symbol(host.property(&position, "symbol")?, symbol) {
        return Ok(false_value());
    }
    if require_exposure && !host.greater_than_zero(host.property(&position, "contracts")?)? {
        return Ok(false_value());
    }
    let finite = host.callable(StateCallKind::NumberFinite)?;
    let leverage = host.property(&position, "leverage")?;
    finite.call(&[leverage])
}
fn market_limit(
    host: &dyn ExchangeStateHost,
    market: &AccountValue,
    name: &str,
) -> SigningResult<AccountValue> {
    let limits = host.property(market, "limits")?;
    let leverage = host.property(&limits, "leverage")?;
    host.property(&leverage, name)
}
// ts: LighterExchange#getCurrentLeverageForSymbol
pub fn get_current_leverage_for_symbol(
    host: &dyn ExchangeStateHost,
    symbol: &[u16],
    market: AccountValue,
) -> SigningResult<AccountValue> {
    let exposed = host
        .positions_find()?
        .find(&mut |position| finite_position(host, position, symbol, true))?;
    // The fallback find is unconditional, even when exposed is already present.
    let fallback = host
        .positions_find()?
        .find(&mut |position| finite_position(host, position, symbol, false))?;
    let mut candidate = if exposed.is_nullish() {
        AccountValue::Undefined
    } else {
        host.property(&exposed, "leverage")?
    };
    if candidate.is_nullish() {
        candidate = if fallback.is_nullish() {
            AccountValue::Undefined
        } else {
            host.property(&fallback, "leverage")?
        };
    }
    if candidate.is_nullish() {
        candidate = market_limit(host, &market, "max")?;
    }
    if candidate.is_nullish() {
        candidate = AccountValue::Number(10.0);
    }
    let min = host.callable(StateCallKind::MathMin)?;
    let max = host.callable(StateCallKind::MathMax)?;
    let lower = market_limit(host, &market, "min")?;
    let clamped_lower = max.call(&[candidate, lower])?;
    let upper = market_limit(host, &market, "max")?;
    let clamped = min.call(&[clamped_lower, upper])?;
    host.callable(StateCallKind::SnapTier)?.call(&[clamped])
}
