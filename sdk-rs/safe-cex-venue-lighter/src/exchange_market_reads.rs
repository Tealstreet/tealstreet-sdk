//! Market/position/balance read bodies and the order-fetch cache wrapper.
//! Required full live JS, Get/key/coercion, Call/import, array/iterator/spread,
//! Promise/finally/return adoption hosts remain separate bindings. Numeric
//! operands take JS ToNumber (objects throw TypeError); this does
//! not construct an exchange, HTTP client, store or JavaScript engine.
use crate::{
    exchange_trading_preflight::TradingHost,
    sdk_account_api::{AccountValue, primitive_to_number},
    transaction_promise::{NativePromise, PromiseResolver},
    transaction_signing::{SigningResult, TransactionSigningError},
};
use std::rc::Rc;
pub enum ForOfStep {
    Continue,
    Break(AccountValue),
}
pub type ForOfBody = Rc<dyn Fn(AccountValue) -> SigningResult<ForOfStep>>;
pub trait MarketReadHost: TradingHost {
    /// Full JS addition, including ordered ToPrimitive and string concatenation.
    fn add(&self, left: AccountValue, right: AccountValue) -> SigningResult<AccountValue>;
    /// Pure ECMAScript Number::exponentiate; not an observable Math.pow lookup.
    fn number_power(&self, base: f64, exponent: f64) -> f64;
    /// Actual imported/local helper callable value, preserving binding and identity.
    fn function_value(&self, name: &str) -> SigningResult<AccountValue>;
    /// Actual full GetIterator/IteratorStepValue/IteratorClose on early return/error;
    /// execute the native body per live iterator value, with original completions.
    fn for_of(&self, array: AccountValue, body: ForOfBody) -> SigningResult<Option<AccountValue>>;
    /// Full ordinary CopyDataProperties plus these ordered own overrides. Preserve
    /// enumerable UTF16/Symbol keys, getter/descriptors, own order and original throws.
    fn spread_override(
        &self,
        source: AccountValue,
        fields: &[(&str, AccountValue)],
    ) -> SigningResult<AccountValue>;
}
fn text(s: &str) -> AccountValue {
    AccountValue::String(s.encode_utf16().collect())
}
fn optional(h: &dyn MarketReadHost, v: &AccountValue, key: &str) -> SigningResult<AccountValue> {
    if v.is_nullish() {
        Ok(AccountValue::Undefined)
    } else {
        h.get(v, key)
    }
}
fn equal(a: &AccountValue, b: &AccountValue) -> bool {
    match (a, b) {
        (AccountValue::Undefined, AccountValue::Undefined)
        | (AccountValue::Null, AccountValue::Null) => true,
        (AccountValue::Boolean(a), AccountValue::Boolean(b)) => a == b,
        (AccountValue::Number(a), AccountValue::Number(b)) => a == b,
        (AccountValue::String(a), AccountValue::String(b)) => a == b,
        (AccountValue::BigInt(a), AccountValue::BigInt(b)) => a == b,
        (AccountValue::Object(a), AccountValue::Object(b)) => Rc::ptr_eq(a, b),
        (AccountValue::Function(a), AccountValue::Function(b)) => a.ptr_eq(b),
        (AccountValue::Symbol(a), AccountValue::Symbol(b)) => a.ptr_eq(b),
        _ => false,
    }
}
fn num(v: AccountValue) -> SigningResult<f64> {
    primitive_to_number(&v)
}
fn call(h: &dyn MarketReadHost, key: &str, args: &[AccountValue]) -> SigningResult<AccountValue> {
    h.member(&h.owner(), key)?.call(args)
}
fn template(
    h: &dyn MarketReadHost,
    prefix: &str,
    value: AccountValue,
    suffix: &str,
) -> SigningResult<AccountValue> {
    let mut units: Vec<u16> = prefix.encode_utf16().collect();
    units.extend(h.template_string(value)?);
    units.extend(suffix.encode_utf16());
    Ok(AccountValue::String(units))
}
fn error_message(
    h: &dyn MarketReadHost,
    error: TransactionSigningError,
) -> SigningResult<AccountValue> {
    let error = h.error_value(error);
    let message = optional(h, &error, "message")?;
    if message.truthy() {
        Ok(message)
    } else {
        h.global("String")?.call(&[error])
    }
}
type Continuation = Box<dyn FnOnce(SigningResult<AccountValue>) -> SigningResult<Step>>;
enum Step {
    Done(AccountValue),
    Await(AccountValue, Continuation),
}
fn wait(
    value: SigningResult<AccountValue>,
    next: impl FnOnce(SigningResult<AccountValue>) -> SigningResult<Step> + 'static,
) -> SigningResult<Step> {
    match value {
        Ok(v) => Ok(Step::Await(v, Box::new(next))),
        Err(e) => next(Err(e)),
    }
}
fn drive(h: Rc<dyn MarketReadHost>, r: PromiseResolver<AccountValue>, step: SigningResult<Step>) {
    match step {
        Err(e) => r.settle(Err(e)),
        Ok(Step::Done(v)) => h.resolve_return(v, r),
        Ok(Step::Await(v, next)) => match h.await_value(v) {
            Err(e) => drive(h, r, next(Err(e))),
            Ok(p) => p.subscribe(move |v| drive(h, r, next(v.map(|v| (*v).clone())))),
        },
    }
}
fn start(
    h: Rc<dyn MarketReadHost>,
    work: impl FnOnce() -> SigningResult<Step>,
) -> NativePromise<AccountValue> {
    let (p, r) = NativePromise::pending(h.runtime());
    let step = work();
    drive(h, r, step);
    p
}
fn finite(h: &dyn MarketReadHost, v: AccountValue) -> SigningResult<bool> {
    Ok(h.global("Number.isFinite")?.call(&[v])?.truthy())
}
fn v(h: &dyn MarketReadHost, p: &AccountValue, key: &str) -> SigningResult<AccountValue> {
    h.global("v")?.call(&[p.clone(), text(key)])
}
fn optional_or(
    h: &dyn MarketReadHost,
    p: &AccountValue,
    keys: &[&str],
) -> SigningResult<AccountValue> {
    let mut last = AccountValue::Undefined;
    for key in keys {
        last = h.get(p, key)?;
        if !last.is_nullish() {
            break;
        }
    }
    Ok(last)
}
fn v_chain(h: &dyn MarketReadHost, p: &AccountValue, keys: &[&str]) -> SigningResult<AccountValue> {
    let mut last = AccountValue::Undefined;
    for key in keys {
        last = v(h, p, key)?;
        if !last.is_nullish() {
            break;
        }
    }
    Ok(last)
}
fn loose_or(
    h: &dyn MarketReadHost,
    p: &AccountValue,
    keys: &[&str],
) -> SigningResult<AccountValue> {
    for key in keys {
        let value = v(h, p, key)?;
        if value.truthy() {
            return Ok(value);
        }
    }
    Ok(AccountValue::Number(0.0))
}
fn v_nullish(
    h: &dyn MarketReadHost,
    p: &AccountValue,
    keys: &[&str],
    default: AccountValue,
) -> SigningResult<AccountValue> {
    for key in keys {
        let value = v(h, p, key)?;
        if !value.is_nullish() {
            return Ok(value);
        }
    }
    Ok(default)
}
fn parse_string(
    h: &dyn MarketReadHost,
    p: &AccountValue,
    keys: &[&str],
    default: AccountValue,
) -> SigningResult<AccountValue> {
    let parse = h.global("Number.parseFloat")?;
    let stringify = h.global("String")?;
    let value = v_nullish(h, p, keys, default)?;
    parse.call(&[stringify.call(&[value])?])
}
fn clamp(
    h: &dyn MarketReadHost,
    value: AccountValue,
    min: AccountValue,
    max: AccountValue,
) -> SigningResult<AccountValue> {
    let minimum = h.global("Math.min")?;
    let maximum = h.global("Math.max")?;
    minimum.call(&[maximum.call(&[value, min])?, max])
}
// ts: LighterExchange#resolvePositionLeverage
pub fn resolve_position_leverage(
    h: Rc<dyn MarketReadHost>,
    p: AccountValue,
    market: AccountValue,
) -> SigningResult<AccountValue> {
    let limits = h.get(&market, "limits")?;
    let lev = optional(h.as_ref(), &limits, "leverage")?;
    let mut min = optional(h.as_ref(), &lev, "min")?;
    if !min.truthy() {
        min = AccountValue::Number(1.0)
    }
    let limits = h.get(&market, "limits")?;
    let lev = optional(h.as_ref(), &limits, "leverage")?;
    let mut max = optional(h.as_ref(), &lev, "max")?;
    if !max.truthy() {
        max = AccountValue::Number(10.0)
    }
    let array = h.array(&[
        h.get(&p, "leverage")?,
        h.get(&p, "initialLeverage")?,
        h.get(&p, "initial_leverage")?,
    ])?;
    let live = h.clone();
    let low = min.clone();
    let high = max.clone();
    let direct = h.for_of(
        array,
        Rc::new(move |candidate| {
            let parse = live.global("Number.parseFloat")?;
            let stringify = live.global("String")?;
            let value = if candidate.is_nullish() {
                text("")
            } else {
                candidate
            };
            let parsed = parse.call(&[stringify.call(&[value])?])?;
            if finite(live.as_ref(), parsed.clone())? && num(parsed.clone())? > 0.0 {
                Ok(ForOfStep::Break(clamp(
                    live.as_ref(),
                    parsed,
                    low.clone(),
                    high.clone(),
                )?))
            } else {
                Ok(ForOfStep::Continue)
            }
        }),
    )?;
    if let Some(value) = direct {
        return Ok(value);
    }
    let fraction = h.global("parseLighterMarginFractionRatio")?;
    let value = v_chain(
        h.as_ref(),
        &p,
        &["initial_margin_fraction", "initialMarginFraction"],
    )?;
    let fraction = fraction.call(&[value])?;
    if fraction.truthy() {
        let derived = AccountValue::Number(1.0 / num(fraction)?);
        if finite(h.as_ref(), derived.clone())? && num(derived.clone())? > 0.0 {
            return clamp(h.as_ref(), derived, min, max);
        }
    }
    Ok(max)
}
// ts: LighterExchange#resolvePositionLiquidationPrice
pub fn resolve_position_liquidation_price(
    h: &dyn MarketReadHost,
    p: AccountValue,
    market: AccountValue,
    side: AccountValue,
    entry: f64,
    contracts: f64,
    leverage: f64,
    mode: AccountValue,
) -> SigningResult<AccountValue> {
    let raw = parse_string(
        h,
        &p,
        &["liquidationPrice", "liquidation_price"],
        AccountValue::Number(0.0),
    )?;
    let raw_n = num(raw.clone())?;
    let raw_finite = finite(h, raw)? && raw_n > 0.0;
    let profit = raw_finite
        && finite(h, AccountValue::Number(entry))?
        && entry > 0.0
        && ((equal(&side, &h.constant("PositionSide.Long")?) && raw_n >= entry)
            || (equal(&side, &h.constant("PositionSide.Short")?) && raw_n <= entry));
    if raw_finite && (!profit || !equal(&mode, &AccountValue::Number(1.0))) {
        return Ok(AccountValue::Number(raw_n));
    }
    if !equal(&mode, &AccountValue::Number(1.0))
        || !finite(h, AccountValue::Number(entry))?
        || entry <= 0.0
        || !finite(h, AccountValue::Number(contracts))?
        || contracts <= 0.0
    {
        return Ok(AccountValue::Number(if raw_finite { raw_n } else { 0.0 }));
    }
    let allocated = parse_string(
        h,
        &p,
        &[
            "allocatedMargin",
            "allocated_margin",
            "marginSize",
            "margin_size",
        ],
        text(""),
    )?;
    let allocated_n = num(allocated.clone())?;
    let notional = entry * contracts;
    let fallback = if finite(h, AccountValue::Number(leverage))? && leverage > 0.0 {
        notional / leverage
    } else {
        num(h.constant("Number.NaN")?)?
    };
    let allocated = if finite(h, allocated)? && allocated_n > 0.0 {
        allocated_n
    } else {
        fallback
    };
    let parse = h.global("parseLighterMarginFractionRatio")?;
    let input = v_chain(
        h,
        &p,
        &["maintenanceMarginFraction", "maintenance_margin_fraction"],
    )?;
    let mut maintenance = parse.call(&[input])?;
    if maintenance.is_nullish() {
        let parse = h.global("parseLighterMarginFractionRatio")?;
        let info = h.get(&market, "info")?;
        let first = optional(h, &info, "maintenance_margin_fraction")?;
        let second = if first.is_nullish() {
            let info = h.get(&market, "info")?;
            optional(h, &info, "maintenanceMarginFraction")?
        } else {
            first
        };
        maintenance = parse.call(&[second])?;
    }
    if maintenance.is_nullish() {
        maintenance = AccountValue::Number(0.0)
    }
    let maintenance = num(maintenance)?;
    if !finite(h, AccountValue::Number(allocated))?
        || allocated <= 0.0
        || !finite(h, AccountValue::Number(maintenance))?
        || maintenance < 0.0
        || maintenance >= 1.0
    {
        return Ok(AccountValue::Number(if raw_finite { raw_n } else { 0.0 }));
    }
    let derived = if equal(&side, &h.constant("PositionSide.Long")?) {
        (notional - allocated) / (contracts * (1.0 - maintenance))
    } else {
        (notional + allocated) / (contracts * (1.0 + maintenance))
    };
    if finite(h, AccountValue::Number(derived))? && derived > 0.0 {
        return Ok(AccountValue::Number(derived));
    }
    Ok(AccountValue::Number(if raw_finite { raw_n } else { 0.0 }))
}
// ts: LighterExchange.mapPosition
pub fn map_position(h: &dyn MarketReadHost, p: AccountValue) -> SigningResult<AccountValue> {
    let parse = h.global("parseFloat")?;
    let entry = parse.call(&[loose_or(
        h,
        &p,
        &[
            "avgEntryPrice",
            "avg_entry_price",
            "entryPrice",
            "entry_price",
        ],
    )?])?;
    let parse = h.global("parseFloat")?;
    let raw = optional_or(h, &p, &["size", "position", "qty"])?;
    let raw = parse.call(&[if raw.is_nullish() {
        AccountValue::Number(0.0)
    } else {
        raw
    }])?;
    let numeric = h.global("Number")?;
    let sign = h.get(&p, "sign")?;
    let sign = numeric.call(&[if sign.is_nullish() {
        AccountValue::Number(0.0)
    } else {
        sign
    }])?;
    let contracts = h.global("Math.abs")?.call(&[raw.clone()])?;
    if !contracts.truthy() {
        return Ok(AccountValue::Null);
    }
    let short = equal(&h.get(&p, "side")?, &text("SHORT"))
        || equal(&h.get(&p, "side")?, &text("SELL"))
        || num(sign)? < 0.0
        || num(raw)? < 0.0;
    let side = h.constant(if short {
        "PositionSide.Short"
    } else {
        "PositionSide.Long"
    })?;
    let parse = h.global("parseFloat")?;
    let upnl = parse.call(&[loose_or(h, &p, &["unrealizedPnl", "unrealized_pnl"])?])?;
    let id = optional_or(h, &p, &["marketId", "market_id"])?;
    let store = h.get(&h.owner(), "store")?;
    let markets = h.get(&store, "marketsById")?;
    let mut market = h.index(&markets, id)?;
    if !market.truthy() {
        let store = h.get(&h.owner(), "store")?;
        let markets = h.get(&store, "marketsBySymbol")?;
        market = h.index(&markets, h.get(&p, "symbol")?)?;
    }
    if !market.truthy() {
        let log = h.member(&h.owner(), "log")?;
        let id = h.get(&p, "marketId")?;
        let id = if id.truthy() {
            id
        } else {
            h.get(&p, "symbol")?
        };
        let message = template(h, "Market not found for position: ", id, "")?;
        let severity = h.constant("LogSeverity.Warning")?;
        log.call(&[message, severity])?;
        return Ok(AccountValue::Null);
    }
    let parse = h.member(&h.owner(), "parseMarginMode")?;
    let mode = parse.call(&[v_chain(h, &p, &["marginMode", "margin_mode"])?])?;
    if !matches!(mode, AccountValue::Undefined) {
        let cache = h.get(&h.owner(), "marginModeBySymbol")?;
        let set = h.member(&cache, "set")?;
        set.call(&[h.get(&market, "symbol")?, mode.clone()])?;
    }
    let leverage = call(h, "resolvePositionLeverage", &[p.clone(), market.clone()])?;
    let liquidation = call(
        h,
        "resolvePositionLiquidationPrice",
        &[
            p.clone(),
            market.clone(),
            side.clone(),
            entry.clone(),
            contracts.clone(),
            leverage.clone(),
            mode.clone(),
        ],
    )?;
    let create = h.global("createPositionId")?;
    let symbol = h.get(&market, "symbol")?;
    let one_way = h.constant("PositionMode.OneWay")?;
    let id = create.call(&[symbol, side.clone(), one_way])?;
    let symbol = h.get(&market, "symbol")?;
    let notional = AccountValue::Number(num(contracts.clone())? * num(entry.clone())?);
    let parse = h.global("parseFloat")?;
    let realized = parse.call(&[loose_or(h, &p, &["realizedPnl", "realized_pnl"])?])?;
    let position_mode = h.constant("PositionMode.OneWay")?;
    let margin_type = call(h, "marginModeToMarginType", &[mode])?;
    let update = h.global("virtualClock.getCurrentTimeMs")?.call(&[])?;
    h.literal(&[
        ("id", id),
        ("symbol", symbol),
        ("side", side),
        ("entryPrice", entry),
        ("notional", notional),
        ("leverage", leverage),
        ("unrealizedPnl", upnl),
        ("realizedPnl", realized),
        ("contracts", contracts),
        ("liquidationPrice", liquidation),
        ("positionMode", position_mode),
        ("marginType", margin_type),
        ("info", p),
        ("lastUpdate", update),
    ])
}
fn balance_value(
    h: Rc<dyn MarketReadHost>,
    balance: AccountValue,
    limits: AccountValue,
) -> SigningResult<Step> {
    let store = h.get(&h.owner(), "store")?;
    let positions = h.get(&store, "positions")?;
    let reduce = h.member(&positions, "reduce")?;
    let live = h.clone();
    let callback = h.function(Rc::new(move |args| {
        let acc = args.first().cloned().unwrap_or(AccountValue::Undefined);
        let p = args.get(1).cloned().unwrap_or(AccountValue::Undefined);
        let store = live.get(&live.owner(), "store")?;
        let tickers = live.get(&store, "tickersBySymbol")?;
        let ticker = live.index(&tickers, live.get(&p, "symbol")?)?;
        if !ticker.truthy() {
            return Ok(acc);
        }
        let is_long = equal(&live.get(&p, "side")?, &live.constant("PositionSide.Long")?);
        // Binary operands are both read before either is converted.
        let (left, right) = if is_long {
            (live.get(&ticker, "last")?, live.get(&p, "entryPrice")?)
        } else {
            (live.get(&p, "entryPrice")?, live.get(&ticker, "last")?)
        };
        let delta = num(left)? - num(right)?;
        let contracts = live.get(&p, "contracts")?;
        live.add(acc, AccountValue::Number(delta * num(contracts)?))
    }))?;
    let upnl = reduce.call(&[callback, AccountValue::Number(0.0)])?;
    let parse = h.global("parseFloat")?;
    let mut equity = h.get(&balance, "total_asset_value")?;
    if !equity.truthy() {
        equity = h.get(&balance, "collateral")?;
    }
    if !equity.truthy() {
        equity = h.get(&balance, "cross_asset_value")?;
    }
    if !equity.truthy() {
        equity = text("0");
    }
    let equity = parse.call(&[equity])?;
    let parse = h.global("parseFloat")?;
    let mut free = h.get(&balance, "available_balance")?;
    if !free.truthy() {
        free = h.get(&balance, "availableBalance")?;
    }
    if !free.truthy() {
        free = text("0");
    }
    let free = parse.call(&[free])?;
    let max = h.global("Math.max")?;
    let used = h
        .global("subtract")?
        .call(&[equity.clone(), free.clone()])?;
    let used = max.call(&[AccountValue::Number(0.0), used])?;
    let total = h.global("subtract")?.call(&[equity, upnl.clone()])?;
    let info = if limits.truthy() {
        h.spread_override(balance, &[("lighterAccountLimits", limits)])?
    } else {
        balance
    };
    Ok(Step::Done(h.literal(&[
        ("total", total),
        ("upnl", upnl),
        ("used", used),
        ("free", free),
        ("info", info),
    ])?))
}
// ts: LighterExchange.fetchRawBalance
pub fn fetch_raw_balance(
    h: Rc<dyn MarketReadHost>,
    opts: AccountValue,
) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        let opts = if matches!(opts, AccountValue::Undefined) {
            h.literal(&[])?
        } else {
            opts
        };
        let fetch = h.member(&h.owner(), "fetchAccountSnapshot")?;
        let recent = h.get(&opts, "allowRecentAccountSnapshot")?;
        let snapshot = fetch.call(&[h.literal(&[("allowRecent", recent)])?])?;
        let live = h.clone();
        wait(Ok(snapshot), move |balance| {
            let balance = balance?;
            if equal(
                &live.get(&opts, "includeAccountLimits")?,
                &AccountValue::Boolean(false),
            ) {
                let limits = live.get(&live.owner(), "accountLimits")?;
                balance_value(live, balance, limits)
            } else {
                let limits = call(live.as_ref(), "fetchAccountLimits", &[])?;
                let after = live.clone();
                wait(Ok(limits), move |limits| {
                    balance_value(after, balance, limits?)
                })
            }
        })
    })
}
// ts: LighterExchange.fetchMarkets
pub fn fetch_markets(h: Rc<dyn MarketReadHost>) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        let value = call(h.as_ref(), "fetchMarketsHelper", &[])?;
        let live = h.clone();
        wait(Ok(value), move |value| {
            let markets = value?;
            let some = live.member(&markets, "some")?;
            let callback = live.function(Rc::new(|args| {
                Ok(AccountValue::Boolean(matches!(
                    args.first(),
                    Some(AccountValue::Null)
                )))
            }))?;
            if some.call(&[callback])?.truthy() {
                call(
                    live.as_ref(),
                    "emitAndLogError",
                    &[text("Failed to fetch markets")],
                )?;
                let store = live.get(&live.owner(), "store")?;
                return Ok(Step::Done(live.get(&store, "markets")?));
            }
            Ok(Step::Done(markets))
        })
    })
}
fn request_rows(h: &dyn MarketReadHost, key: &str, details: bool) -> SigningResult<AccountValue> {
    let client = h.get(&h.owner(), "lighterApiClient")?;
    let get = h.member(&client, "get")?;
    let endpoint = h.constant(key)?;
    if details {
        get.call(&[
            endpoint,
            h.literal(&[("__suppressExchangeError", AccountValue::Boolean(true))])?,
        ])
    } else {
        get.call(&[endpoint])
    }
}
fn fail_message(h: &dyn MarketReadHost, p: &AccountValue, fallback: &str) -> SigningResult<Step> {
    let ctor = h.global("Error")?;
    let msg = optional(h, p, "message")?;
    let error = ctor.construct(&[if msg.truthy() { msg } else { text(fallback) }])?;
    Err(h.throw_value(error))
}
fn fallback_rows(h: Rc<dyn MarketReadHost>, error: TransactionSigningError) -> SigningResult<Step> {
    let log = h.member(&h.owner(), "log")?;
    let message = error_message(h.as_ref(), error)?;
    let message = template(
        h.as_ref(),
        "Lighter orderBookDetails unavailable; falling back to orderBooks (",
        message,
        ").",
    )?;
    let severity = h.constant("LogSeverity.Warning")?;
    log.call(&[message, severity])?;
    let fallback = request_rows(h.as_ref(), "ENDPOINTS.MARKETS", false)?;
    let live = h.clone();
    wait(Ok(fallback), move |result| {
        let data = result?;
        let code = optional(live.as_ref(), &data, "code")?;
        if !equal(&code, &AccountValue::Number(200.0)) {
            return fail_message(live.as_ref(), &data, "Failed to fetch markets");
        }
        let is_array = live.global("Array.isArray")?;
        let orders = optional(live.as_ref(), &data, "order_books")?;
        if !is_array.call(&[orders])?.truthy() {
            return fail_message(live.as_ref(), &data, "Failed to fetch markets");
        }
        Ok(Step::Done(live.get(&data, "order_books")?))
    })
}
// ts: LighterExchange#fetchLighterMarketRows
pub fn fetch_lighter_market_rows(h: Rc<dyn MarketReadHost>) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        let details = request_rows(h.as_ref(), "ENDPOINTS.ORDER_BOOK_DETAILS", true);
        let live = h.clone();
        wait(details, move |details| {
            let selected = (|| {
                let details = details?;
                let check = live.global("Array.isArray")?;
                let rows = optional(live.as_ref(), &details, "order_book_details")?;
                let rows = if check.call(&[rows])?.truthy() {
                    live.get(&details, "order_book_details")?
                } else {
                    let check = live.global("Array.isArray")?;
                    let rows = optional(live.as_ref(), &details, "order_books")?;
                    if check.call(&[rows])?.truthy() {
                        live.get(&details, "order_books")?
                    } else {
                        AccountValue::Undefined
                    }
                };
                if equal(
                    &optional(live.as_ref(), &details, "code")?,
                    &AccountValue::Number(200.0),
                ) && rows.truthy()
                {
                    return Ok(Step::Done(rows));
                }
                fail_message(
                    live.as_ref(),
                    &details,
                    "Failed to fetch Lighter market details",
                )
            })();
            match selected {
                Ok(s) => Ok(s),
                Err(e) => fallback_rows(live, e),
            }
        })
    })
}
// ts: LighterExchange.fetchMarketsHelper
pub fn fetch_markets_helper(h: Rc<dyn MarketReadHost>) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        let rows = call(h.as_ref(), "fetchLighterMarketRows", &[])?;
        let live = h.clone();
        wait(Ok(rows), move |rows| {
            let rows = rows?;
            let filter = live.member(&rows, "filter")?;
            let predicate = live.function_value("isLighterPerpMarket")?;
            let filtered = filter.call(&[predicate])?;
            let map = live.member(&filtered, "map")?;
            let owner = live.clone();
            let callback = live.function(Rc::new(move |args| {
                let market = args.first().cloned().unwrap_or(AccountValue::Undefined);
                let stringify = owner.global("String")?;
                let sym = owner.get(&market, "symbol")?;
                let symbol = stringify.call(&[if sym.truthy() { sym } else { text("") }])?;
                let parse = owner.global("parseLighterMarketSymbol")?;
                let profile = owner.get(&owner.owner(), "lighterDeploymentProfile")?;
                let quote = owner.get(&profile, "quoteCurrency")?;
                let parsed = parse.call(&[symbol.clone(), quote])?;
                let base = owner.get(&parsed, "base")?;
                let quote = owner.get(&parsed, "quote")?;
                let numeric = owner.global("Number")?;
                let amount = owner.get(&market, "supported_size_decimals")?;
                let amount = numeric.call(&[if amount.is_nullish() {
                    AccountValue::Number(3.0)
                } else {
                    amount
                }])?;
                let numeric = owner.global("Number")?;
                let price = owner.get(&market, "supported_price_decimals")?;
                let price = numeric.call(&[if price.is_nullish() {
                    AccountValue::Number(2.0)
                } else {
                    price
                }])?;
                let amount_increment = if num(amount.clone())? >= 0.0 {
                    1.0 / owner.number_power(10.0, num(amount.clone())?)
                } else {
                    0.001
                };
                let price_increment = if num(price.clone())? >= 0.0 {
                    1.0 / owner.number_power(10.0, num(price.clone())?)
                } else {
                    0.01
                };
                let leverage = owner
                    .global("getLighterMarketLeverageMax")?
                    .call(&[market.clone()])?;
                let stringify = owner.global("String")?;
                let id = stringify.call(&[owner.get(&market, "market_id")?])?;
                let precision = owner.literal(&[
                    ("amount", AccountValue::Number(amount_increment)),
                    ("price", AccountValue::Number(price_increment)),
                ])?;
                let parse = owner.global("parseFloat")?;
                let mut min = owner.get(&market, "min_base_amount")?;
                if !min.truthy() {
                    min = text("0")
                }
                let min = parse.call(&[min])?;
                let parse = owner.global("parseFloat")?;
                let mut max = owner.get(&market, "order_quote_limit")?;
                if !max.truthy() {
                    max = text("1000000")
                }
                let max = parse.call(&[max])?;
                let amount_limits = owner.literal(&[("min", min), ("max", max)])?;
                let leverage_limits =
                    owner.literal(&[("min", AccountValue::Number(1.0)), ("max", leverage)])?;
                let limits =
                    owner.literal(&[("amount", amount_limits), ("leverage", leverage_limits)])?;
                let info = owner.spread_override(
                    market,
                    &[("amountDecimals", amount), ("priceDecimals", price)],
                )?;
                owner.literal(&[
                    ("id", id),
                    ("symbol", symbol),
                    ("base", base),
                    ("quote", quote),
                    ("active", AccountValue::Boolean(true)),
                    ("precision", precision),
                    ("limits", limits),
                    ("isExpiring", AccountValue::Boolean(false)),
                    ("isSpot", AccountValue::Boolean(false)),
                    ("info", info),
                ])
            }))?;
            Ok(Step::Done(map.call(&[callback])?))
        })
    })
}
// ts: LighterExchange.fetchOrders
pub fn fetch_orders(h: Rc<dyn MarketReadHost>) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        if h.get(&h.owner(), "ordersFetchPromise")?.truthy() {
            return Ok(Step::Done(h.get(&h.owner(), "ordersFetchPromise")?));
        }
        let fetched = call(h.as_ref(), "fetchOrdersOnce", &[])?;
        let finally = h.member(&fetched, "finally")?;
        let live = h.clone();
        let callback = h.function(Rc::new(move |_| {
            let now = live.global("virtualClock.getCurrentTimeMs")?.call(&[])?;
            live.set(&live.owner(), "lastOrdersFetchAt", now)?;
            live.set(&live.owner(), "ordersFetchPromise", AccountValue::Undefined)?;
            Ok(AccountValue::Undefined)
        }))?;
        let fetched = finally.call(&[callback])?;
        h.set(&h.owner(), "ordersFetchPromise", fetched)?;
        Ok(Step::Done(h.get(&h.owner(), "ordersFetchPromise")?))
    })
}
