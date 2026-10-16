//! Trading guard and signed margin/leverage orchestration bodies. Required live
//! JS operations preserve callable/receiver capture, keys, coercion, callbacks,
//! import values, original throws, await and return adoption. No exchange,
//! array/JS engine, signer, timer or network construction is provided here.
//! Numeric operands take JS ToNumber; objects throw TypeError.
use crate::{
    exchange_account_access::ExchangeAccountAccessHost,
    sdk_account_api::{AccountValue, primitive_to_number},
    transaction_promise::{NativePromise, PromiseResolver},
    transaction_signing::{SigningResult, TransactionSigningError},
};
use std::rc::Rc;
pub type TradingCallback = Rc<dyn Fn(&[AccountValue]) -> SigningResult<AccountValue>>;
pub trait TradingHost: ExchangeAccountAccessHost {
    /// Fresh actual callable, retaining live captures and all JS callback args.
    fn function(&self, callback: TradingCallback) -> SigningResult<AccountValue>;
    /// Fresh Array in the bound source realm, with native elements/own order.
    fn array(&self, values: &[AccountValue]) -> SigningResult<AccountValue>;
    /// Full Get with actual ToPropertyKey, preserving UTF16/Symbol/throws.
    fn index(&self, object: &AccountValue, key: AccountValue) -> SigningResult<AccountValue>;
    /// Live bound source enum/import constant, not a fabricated default.
    fn constant(&self, name: &str) -> SigningResult<AccountValue>;
    /// Infallible original thrown value projection, retaining identity.
    fn error_value(&self, error: TransactionSigningError) -> AccountValue;
    /// Full template ToString/UTF16 (including original Symbol/coercion throws).
    fn template_string(&self, value: AccountValue) -> SigningResult<Vec<u16>>;
}
fn text(s: &str) -> AccountValue {
    AccountValue::String(s.encode_utf16().collect())
}
fn optional(h: &dyn TradingHost, v: &AccountValue, key: &str) -> SigningResult<AccountValue> {
    if v.is_nullish() {
        Ok(AccountValue::Undefined)
    } else {
        h.get(v, key)
    }
}
fn eq(a: &AccountValue, b: &AccountValue) -> bool {
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
fn call(h: &dyn TradingHost, key: &str, args: &[AccountValue]) -> SigningResult<AccountValue> {
    h.member(&h.owner(), key)?.call(args)
}
fn member_call(
    h: &dyn TradingHost,
    obj: &AccountValue,
    key: &str,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    h.member(obj, key)?.call(args)
}
fn template(
    h: &dyn TradingHost,
    prefix: &str,
    value: AccountValue,
    suffix: &str,
) -> SigningResult<AccountValue> {
    let mut units: Vec<u16> = prefix.encode_utf16().collect();
    units.extend(h.template_string(value)?);
    units.extend(suffix.encode_utf16());
    Ok(AccountValue::String(units))
}
fn fail(h: &dyn TradingHost, message: AccountValue) -> SigningResult<Step> {
    let error = h.global("Error")?.construct(&[message])?;
    Err(h.throw_value(error))
}
fn error_message(
    h: &dyn TradingHost,
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
fn warning(
    h: &dyn TradingHost,
    error: TransactionSigningError,
    prefix: &str,
) -> SigningResult<Step> {
    let log = h.member(&h.owner(), "log")?;
    let error = error_message(h, error)?;
    let message = template(h, prefix, error, "")?;
    let severity = h.constant("LogSeverity.Warning")?;
    log.call(&[message, severity])?;
    done()
}
type Continuation = Box<dyn FnOnce(SigningResult<AccountValue>) -> SigningResult<Step>>;
enum Step {
    Done(AccountValue),
    Await(AccountValue, Continuation),
}
fn done() -> SigningResult<Step> {
    Ok(Step::Done(AccountValue::Undefined))
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
fn drive(h: Rc<dyn TradingHost>, r: PromiseResolver<AccountValue>, step: SigningResult<Step>) {
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
    h: Rc<dyn TradingHost>,
    work: impl FnOnce() -> SigningResult<Step>,
) -> NativePromise<AccountValue> {
    let (p, r) = NativePromise::pending(h.runtime());
    let step = work();
    drive(h, r, step);
    p
}
// ts: LighterExchange#ensurePartnerAttributionTierEligible
pub fn ensure_partner_attribution_tier_eligible(
    h: Rc<dyn TradingHost>,
    config: AccountValue,
) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        if !config.truthy()
            || !h
                .global("hasNonZeroLighterIntegratorFee")?
                .call(&[config])?
                .truthy()
        {
            return done();
        }
        let fetched = call(h.as_ref(), "fetchAccountLimits", &[]);
        let live = h.clone();
        wait(fetched, move |result| {
            let limits = match result {
                Ok(v) => v,
                Err(e) => {
                    let log = live.member(&live.owner(), "log")?;
                    let error = error_message(live.as_ref(), e)?;
                    let message = template(
                        live.as_ref(),
                        "Lighter partner-attribution account tier lookup skipped: ",
                        error,
                        "",
                    )?;
                    let severity = live.constant("LogSeverity.Debug")?;
                    log.call(&[message, severity])?;
                    return done();
                }
            };
            let tier = live
                .global("extractLighterAccountTierFromPayload")?
                .call(&[limits])?;
            if tier.truthy()
                && !live
                    .global("isLighterAccountTierEligibleForPartnerAttribution")?
                    .call(&[tier])?
                    .truthy()
            {
                let emit = live.member(&live.owner(), "emitAndLogError")?;
                emit.call(&[
                    live.constant("LIGHTER_PARTNER_ATTRIBUTION_TIER_REQUIREMENT_MESSAGE")?
                ])?;
                // Error is captured before the second constant read.
                let constructor = live.global("Error")?;
                let message =
                    live.constant("LIGHTER_PARTNER_ATTRIBUTION_TIER_REQUIREMENT_MESSAGE")?;
                return Err(live.throw_value(constructor.construct(&[message])?));
            }
            done()
        })
    })
}
// ts: LighterExchange#ensurePartnerAttributionApproved
pub fn ensure_partner_attribution_approved(
    h: Rc<dyn TradingHost>,
    config: AccountValue,
) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        if !config.truthy()
            || !h
                .global("hasNonZeroLighterIntegratorFee")?
                .call(&[config.clone()])?
                .truthy()
        {
            return done();
        }
        let fetched = (|| {
            let fetch = h.member(&h.owner(), "fetchAccountSnapshot")?;
            fetch.call(&[h.literal(&[("allowRecent", AccountValue::Boolean(true))])?])
        })();
        let live = h.clone();
        wait(fetched, move |result| {
            let account = match result {
                Ok(v) => v,
                Err(e) => {
                    let log = live.member(&live.owner(), "log")?;
                    let error = error_message(live.as_ref(), e)?;
                    let message = template(
                        live.as_ref(),
                        "Lighter partner-attribution approval lookup skipped: ",
                        error,
                        "",
                    )?;
                    let severity = live.constant("LogSeverity.Debug")?;
                    log.call(&[message, severity])?;
                    return done();
                }
            };
            let is_array = live.global("Array.isArray")?;
            let list = optional(live.as_ref(), &account, "approved_integrators")?;
            let approved = if is_array.call(&[list])?.truthy() {
                live.get(&account, "approved_integrators")?
            } else {
                live.array(&[])?
            };
            let index = live.get(&config, "integratorAccountIndex")?;
            let index = member_call(live.as_ref(), &index, "toString", &[])?;
            let now = live.global("virtualClock.getCurrentTimeMs")?.call(&[])?;
            let some = live.member(&approved, "some")?;
            let owner = live.clone();
            let callback = live.function(Rc::new(move |args| {
                let entry = args.first().cloned().unwrap_or(AccountValue::Undefined);
                let stringify = owner.global("String")?;
                let mut candidate = optional(owner.as_ref(), &entry, "account_index")?;
                if candidate.is_nullish() {
                    candidate = optional(owner.as_ref(), &entry, "accountIndex")?;
                }
                if candidate.is_nullish() {
                    candidate = text("");
                }
                let candidate = stringify.call(&[candidate])?;
                let candidate = member_call(owner.as_ref(), &candidate, "trim", &[])?;
                if !eq(&candidate, &index) {
                    return Ok(AccountValue::Boolean(false));
                }
                let mut expiry = optional(owner.as_ref(), &entry, "approval_expiry")?;
                if expiry.is_nullish() {
                    expiry = optional(owner.as_ref(), &entry, "approvalExpiry")?;
                }
                if expiry.is_nullish() {
                    expiry = AccountValue::Number(0.0);
                }
                let expiry = owner.global("Number")?.call(&[expiry])?;
                let finite = owner.global("Number.isFinite")?.call(&[expiry.clone()])?;
                Ok(AccountValue::Boolean(
                    finite.truthy() && num(expiry)? > num(now.clone())?,
                ))
            }))?;
            if !some.call(&[callback])?.truthy() {
                let emit = live.member(&live.owner(), "emitAndLogError")?;
                emit.call(&[
                    live.constant("LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_MISSING_MESSAGE")?
                ])?;
                let constructor = live.global("Error")?;
                let message =
                    live.constant("LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_MISSING_MESSAGE")?;
                return Err(live.throw_value(constructor.construct(&[message])?));
            }
            done()
        })
    })
}
// ts: LighterExchange#refreshMarginModeStateAfterInvalidOrder
pub fn refresh_margin_mode_state_after_invalid_order(
    h: Rc<dyn TradingHost>,
    symbol: AccountValue,
) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        let config = h.member(&h.owner(), "getMarketConfiguredMarginMode")?;
        let store = h.get(&h.owner(), "store")?;
        let markets = h.get(&store, "marketsBySymbol")?;
        let configured = config.call(&[h.index(&markets, symbol.clone())?])?;
        let store = h.get(&h.owner(), "store")?;
        let positions = h.get(&store, "positions")?;
        let some = h.member(&positions, "some")?;
        let live = h.clone();
        let same_symbol = symbol.clone();
        let callback = h.function(Rc::new(move |args| {
            let position = args.first().cloned().unwrap_or(AccountValue::Undefined);
            let matches = eq(&live.get(&position, "symbol")?, &same_symbol);
            Ok(AccountValue::Boolean(
                matches && num(live.get(&position, "contracts")?)? > 0.0,
            ))
        }))?;
        let open = some.call(&[callback])?;
        if !matches!(configured, AccountValue::Undefined) && !open.truthy() {
            let cache = h.get(&h.owner(), "marginModeBySymbol")?;
            let set = h.member(&cache, "set")?;
            set.call(&[
                symbol.clone(),
                AccountValue::Number(if eq(&configured, &AccountValue::Number(1.0)) {
                    0.0
                } else {
                    1.0
                }),
            ])?;
        }
        call(h.as_ref(), "invalidateAccountSnapshotCache", &[])?;
        let fetched = call(h.as_ref(), "fetchPositions", &[]);
        let live = h.clone();
        wait(fetched, move |positions| {
            let update = (|| {
                let positions = positions?;
                if !live.get(&live.owner(), "isDisposed")?.truthy() {
                    let store = live.get(&live.owner(), "store")?;
                    member_call(
                        live.as_ref(),
                        &store,
                        "addOrUpdatePositionsFull",
                        &[positions],
                    )?;
                }
                done()
            })();
            match update {
                Ok(s) => Ok(s),
                Err(e) => {
                    let log = live.member(&live.owner(), "log")?;
                    let mut message:Vec<u16>="[Lighter] Failed to refresh margin mode state after invalid margin mode order rejection for ".encode_utf16().collect();
                    message.extend(live.template_string(symbol)?);
                    message.extend(": ".encode_utf16());
                    let error = error_message(live.as_ref(), e)?;
                    message.extend(live.template_string(error)?);
                    let severity = live.constant("LogSeverity.Warning")?;
                    log.call(&[AccountValue::String(message), severity])?;
                    done()
                }
            }
        })
    })
}
fn reduce_final(
    h: &dyn TradingHost,
    opts: AccountValue,
    positions: AccountValue,
) -> SigningResult<Step> {
    let check = h.member(&h.owner(), "hasOpenReduceOnlyTargetPosition")?;
    let symbol = h.get(&opts, "symbol")?;
    let side = h.get(&opts, "side")?;
    let positions = if positions.truthy() {
        positions
    } else {
        let store = h.get(&h.owner(), "store")?;
        h.get(&store, "positions")?
    };
    if check.call(&[symbol, side, positions])?.truthy() {
        return done();
    }
    let build = h.member(&h.owner(), "buildNoPositionReduceOnlyMessage")?;
    let message = build.call(&[h.get(&opts, "symbol")?, h.get(&opts, "side")?])?;
    call(h, "emitAndLogError", &[message.clone()])?;
    fail(h, message)
}
// ts: LighterExchange#assertReduceOnlyLimitHasPosition
pub fn assert_reduce_only_limit_has_position(
    h: Rc<dyn TradingHost>,
    opts: AccountValue,
) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        let is_limit = eq(&h.get(&opts, "type")?, &h.constant("OrderType.Limit")?);
        if !is_limit
            || !h
                .global("Boolean")?
                .call(&[h.get(&opts, "reduceOnly")?])?
                .truthy()
        {
            return done();
        }
        let check = h.member(&h.owner(), "hasOpenReduceOnlyTargetPosition")?;
        if check
            .call(&[h.get(&opts, "symbol")?, h.get(&opts, "side")?])?
            .truthy()
        {
            return done();
        }
        let fetched = call(h.as_ref(), "fetchPositions", &[]);
        let live = h.clone();
        wait(fetched, move |positions| {
            let mut refreshed = AccountValue::Undefined;
            let attempt = (|| {
                refreshed = positions?;
                if !live.get(&live.owner(), "isDisposed")?.truthy() {
                    let store = live.get(&live.owner(), "store")?;
                    member_call(
                        live.as_ref(),
                        &store,
                        "addOrUpdatePositionsFull",
                        &[refreshed.clone()],
                    )?;
                }
                done()
            })();
            if let Err(e) = attempt {
                warning(
                    live.as_ref(),
                    e,
                    "[Lighter] Failed to refresh positions before reduce-only validation: ",
                )?;
            }
            reduce_final(live.as_ref(), opts, refreshed)
        })
    })
}
// ts: LighterExchange#resolveConditionalExecutionPrice
pub fn resolve_conditional_execution_price(
    h: &dyn TradingHost,
    trigger: f64,
    side: AccountValue,
    opts: AccountValue,
) -> SigningResult<AccountValue> {
    let slippage = num(call(h, "resolveStopSlippagePct", &[opts.clone()])?)?;
    let multiplier = if eq(&side, &h.constant("OrderSide.Buy")?) {
        1.0 + slippage / 100.0
    } else {
        1.0 - slippage / 100.0
    };
    let adjust = h.global("adjust")?;
    let value = AccountValue::Number(trigger * multiplier);
    let store = h.get(&h.owner(), "store")?;
    let markets = h.get(&store, "marketsBySymbol")?;
    let market = h.index(&markets, h.get(&opts, "symbol")?)?;
    let precision = h.get(&market, "precision")?;
    adjust.call(&[value, h.get(&precision, "price")?])
}
fn market_for(h: &dyn TradingHost, symbol: AccountValue) -> SigningResult<AccountValue> {
    let store = h.get(&h.owner(), "store")?;
    let markets = h.get(&store, "marketsBySymbol")?;
    h.index(&markets, symbol)
}
fn market_id(h: &dyn TradingHost, market: &AccountValue) -> SigningResult<AccountValue> {
    let parse = h.global("Number.parseInt")?;
    let stringify = h.global("String")?;
    let id = h.get(market, "id")?;
    parse.call(&[stringify.call(&[id])?, AccountValue::Number(10.0)])
}
fn invalid_market(h: &dyn TradingHost, symbol: AccountValue) -> SigningResult<Step> {
    // Capture Error constructor before template coercion.
    let ctor = h.global("Error")?;
    let msg = template(h, "Market ", symbol, " not found")?;
    Err(h.throw_value(ctor.construct(&[msg])?))
}
fn invalid_id(
    h: &dyn TradingHost,
    symbol: AccountValue,
    market: AccountValue,
) -> SigningResult<Step> {
    let ctor = h.global("Error")?;
    let mut msg: Vec<u16> = "Invalid market id for ".encode_utf16().collect();
    msg.extend(h.template_string(symbol)?);
    msg.extend(": ".encode_utf16());
    let stringify = h.global("String")?;
    let value = stringify.call(&[h.get(&market, "id")?])?;
    msg.extend(h.template_string(value)?);
    Err(h.throw_value(ctor.construct(&[AccountValue::String(msg)])?))
}
fn string_error(h: &dyn TradingHost, e: TransactionSigningError) -> SigningResult<AccountValue> {
    let stringify = h.global("String")?;
    let error = h.error_value(e);
    let message = optional(h, &error, "message")?;
    stringify.call(&[if message.truthy() {
        message
    } else if error.truthy() {
        error
    } else {
        text("")
    }])
}
fn lower(h: &dyn TradingHost, v: &AccountValue) -> SigningResult<AccountValue> {
    member_call(h, v, "toLowerCase", &[])
}
fn contains(h: &dyn TradingHost, v: &AccountValue, needle: &str) -> SigningResult<bool> {
    Ok(member_call(h, v, "includes", &[text(needle)])?.truthy())
}
fn leverage_error(
    h: Rc<dyn TradingHost>,
    error: TransactionSigningError,
    symbol: AccountValue,
    leverage: AccountValue,
) -> SigningResult<Step> {
    let message = string_error(h.as_ref(), error.clone())?;
    let normalized = lower(h.as_ref(), &message)?;
    let restricted = contains(h.as_ref(), &normalized, "restricted jurisdiction")?;
    let fraction = contains(h.as_ref(), &normalized, "invalid initial margin fraction")?;
    let unsupported = contains(h.as_ref(), &normalized, "updateleverage is not available")?
        || contains(h.as_ref(), &normalized, "signerclient export not found")?
        || contains(
            h.as_ref(),
            &normalized,
            "leverage signer unavailable in runtime",
        )?
        || contains(
            h.as_ref(),
            &normalized,
            "failed to fetch dynamically imported module",
        )?
        || contains(h.as_ref(), &normalized, "node:fs")?
        || restricted
        || fraction;
    if unsupported {
        let log = h.member(&h.owner(), "log")?;
        let msg = template(
            h.as_ref(),
            "Lighter leverage update failed (",
            message.clone(),
            "); refreshing exchange leverage state.",
        )?;
        let severity = h.constant("LogSeverity.Warning")?;
        log.call(&[msg, severity])?;
    }
    let sync = call(h.as_ref(), "syncLeverageFromExchange", &[symbol.clone()]);
    let live = h.clone();
    wait(sync, move |synced| {
        synced?;
        if !unsupported {
            return Err(error);
        }
        if restricted {
            return fail(
                live.as_ref(),
                text(
                    "Lighter leverage updates are restricted in this environment. Leverage remains unchanged on exchange.",
                ),
            );
        }
        if fraction {
            let ctor = live.global("Error")?;
            let mut msg: Vec<u16> = "Lighter rejected leverage ".encode_utf16().collect();
            msg.extend(live.template_string(leverage)?);
            msg.extend("x for ".encode_utf16());
            msg.extend(live.template_string(symbol)?);
            msg.extend(
                ". This market/account does not currently allow that leverage tier.".encode_utf16(),
            );
            return Err(live.throw_value(ctor.construct(&[AccountValue::String(msg)])?));
        }
        let ctor = live.global("Error")?;
        let msg = if message.truthy() {
            message
        } else {
            template(
                live.as_ref(),
                "Failed to update leverage on Lighter for ",
                symbol,
                "",
            )?
        };
        Err(live.throw_value(ctor.construct(&[msg])?))
    })
}
// ts: LighterExchange.setLeverage
pub fn set_leverage(
    h: Rc<dyn TradingHost>,
    symbol: AccountValue,
    input: f64,
) -> NativePromise<AccountValue> {
    set_leverage_value(h, symbol, AccountValue::Number(input))
}

/// Live argument entry point; Math.max performs coercion after signer setup.
pub fn set_leverage_value(
    h: Rc<dyn TradingHost>,
    symbol: AccountValue,
    input: AccountValue,
) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        let market = market_for(h.as_ref(), symbol.clone())?;
        if !market.truthy() {
            return invalid_market(h.as_ref(), symbol);
        }
        let signer = call(h.as_ref(), "ensureSignerAdapter", &[])?;
        let live = h.clone();
        wait(Ok(signer), move |signer| {
            let signer = signer?;
            let id = market_id(live.as_ref(), &market)?;
            if !live
                .global("Number.isFinite")?
                .call(&[id.clone()])?
                .truthy()
            {
                return invalid_id(live.as_ref(), symbol, market);
            }
            let min = live.global("Math.min")?;
            let max = live.global("Math.max")?;
            let limits = live.get(&market, "limits")?;
            let limits = live.get(&limits, "leverage")?;
            let lower = live.get(&limits, "min")?;
            let clamped = max.call(&[input, lower])?;
            let limits = live.get(&market, "limits")?;
            let limits = live.get(&limits, "leverage")?;
            let upper = live.get(&limits, "max")?;
            let clamped = min.call(&[clamped, upper])?;
            let leverage = call(live.as_ref(), "snapToLeverageTier", &[clamped])?;
            let mode = call(
                live.as_ref(),
                "getCurrentMarginModeForSymbol",
                &[symbol.clone()],
            )?;
            let updated = (|| {
                let update = live.member(&signer, "updateLeverage")?;
                update.call(&[live.literal(&[
                    ("marketId", id),
                    ("leverage", leverage.clone()),
                    ("marginMode", mode.clone()),
                ])?])
            })();
            let after = live.clone();
            wait(updated, move |updated| {
                if let Err(e) = updated {
                    return leverage_error(after, e, symbol, leverage);
                }
                let synced = call(after.as_ref(), "syncLeverageFromExchange", &[symbol]);
                let log_host = after.clone();
                wait(synced, move |synced| {
                    synced?;
                    let log = log_host.member(&log_host.owner(), "log")?;
                    let mut message: Vec<u16> =
                        "Updated Lighter leverage via signed transaction to "
                            .encode_utf16()
                            .collect();
                    message.extend(log_host.template_string(leverage)?);
                    message.extend("x (".encode_utf16());
                    message.extend(
                        if eq(&mode, &AccountValue::Number(1.0)) {
                            "isolated"
                        } else {
                            "cross"
                        }
                        .encode_utf16(),
                    );
                    message.extend(" mode).".encode_utf16());
                    let severity = log_host.constant("LogSeverity.Info")?;
                    log.call(&[AccountValue::String(message), severity])?;
                    done()
                })
            })
        })
    })
}
// ts: LighterExchange.changeMarginType
pub fn change_margin_type(
    h: Rc<dyn TradingHost>,
    isolated: AccountValue,
    symbol: AccountValue,
) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        if !symbol.truthy() {
            return fail(
                h.as_ref(),
                text("Lighter margin mode update requires a symbol."),
            );
        }
        let market = market_for(h.as_ref(), symbol.clone())?;
        if !market.truthy() {
            return invalid_market(h.as_ref(), symbol);
        }
        let signer = call(h.as_ref(), "ensureSignerAdapter", &[])?;
        let live = h.clone();
        wait(Ok(signer), move |signer| {
            let signer = signer?;
            let id = market_id(live.as_ref(), &market)?;
            if !live
                .global("Number.isFinite")?
                .call(&[id.clone()])?
                .truthy()
            {
                return invalid_id(live.as_ref(), symbol, market);
            }
            let mode = AccountValue::Number(if isolated.truthy() { 1.0 } else { 0.0 });
            let leverage = call(
                live.as_ref(),
                "getCurrentLeverageForSymbol",
                &[symbol.clone(), market],
            )?;
            let updated = (|| {
                let update = live.member(&signer, "updateLeverage")?;
                update.call(&[live.literal(&[
                    ("marketId", id),
                    ("leverage", leverage),
                    ("marginMode", mode.clone()),
                ])?])
            })();
            let after = live.clone();
            wait(updated, move |updated| {
                if let Err(e) = updated {
                    let message = string_error(after.as_ref(), e.clone())?;
                    let normalized = lower(after.as_ref(), &message)?;
                    if contains(after.as_ref(), &normalized, "21132")?
                        || contains(
                            after.as_ref(),
                            &normalized,
                            "margin mode change on a market with position or open order",
                        )?
                    {
                        let ctor = after.global("Error")?;
                        let message = template(
                            after.as_ref(),
                            "Lighter rejected margin mode switch for ",
                            symbol,
                            ": close open positions and orders on this market first.",
                        )?;
                        return Err(after.throw_value(ctor.construct(&[message])?));
                    }
                    return Err(e);
                }
                let cache = after.get(&after.owner(), "marginModeBySymbol")?;
                member_call(after.as_ref(), &cache, "set", &[symbol.clone(), mode])?;
                let sync = call(
                    after.as_ref(),
                    "syncLeverageFromExchange",
                    &[symbol.clone()],
                );
                let log_host = after.clone();
                wait(sync, move |synced| {
                    synced?;
                    let log = log_host.member(&log_host.owner(), "log")?;
                    let mut message: Vec<u16> =
                        "Updated Lighter margin mode for ".encode_utf16().collect();
                    message.extend(log_host.template_string(symbol)?);
                    message.extend(" to ".encode_utf16());
                    message.extend(
                        if isolated.truthy() {
                            "isolated"
                        } else {
                            "cross"
                        }
                        .encode_utf16(),
                    );
                    message.extend(" via signed transaction.".encode_utf16());
                    let severity = log_host.constant("LogSeverity.Info")?;
                    log.call(&[AccountValue::String(message), severity])?;
                    done()
                })
            })
        })
    })
}
// ts: LighterExchange.cancelSymbolOrders
pub fn cancel_symbol_orders(
    h: Rc<dyn TradingHost>,
    symbol: AccountValue,
) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        let store = h.get(&h.owner(), "store")?;
        let orders = h.get(&store, "orders")?;
        let filter = h.member(&orders, "filter")?;
        let live = h.clone();
        let callback = h.function(Rc::new(move |args| {
            let order = args.first().unwrap_or(&AccountValue::Undefined);
            if !eq(&live.get(order, "symbol")?, &symbol) {
                return Ok(AccountValue::Boolean(false));
            }
            let terminal = live.global("isTerminalOrder")?.call(&[order.clone()])?;
            Ok(AccountValue::Boolean(!terminal.truthy()))
        }))?;
        let orders = filter.call(&[callback])?;
        if num(h.get(&orders, "length")?)? > 0.0 {
            let cancel = call(h.as_ref(), "cancelOrders", &[orders])?;
            return wait(Ok(cancel), |result| {
                result?;
                done()
            });
        }
        done()
    })
}
