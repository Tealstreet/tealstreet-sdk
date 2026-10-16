//! Live-value versions of the small LighterExchange helpers needed by class
//! callbacks. Generic language operations stay required realm operations; Rust
//! owns the source business branches, short circuits and repeated property reads.
use crate::{
    exchange_order_mapping::{OrderMapBinary, OrderMappingHost},
    sdk_account_api::AccountValue,
    transaction_signing::SigningResult,
};
use std::rc::Rc;
pub trait LiveHelperHost: OrderMappingHost {
    /// The lexical LighterExchange constructor binding, not this.constructor.
    fn class_constructor(&self) -> AccountValue;
    /// Full GetBindingValue without invoking the value, e.g. filter(Boolean).
    fn global_value(&self, name: &str) -> SigningResult<AccountValue>;
    /// Full ECMAScript <, including repeated object coercion and throws.
    fn less_than(&self, left: AccountValue, right: AccountValue) -> SigningResult<AccountValue>;
    /// Full unary negation, preserving negative zero and original throws.
    fn negate(&self, value: AccountValue) -> SigningResult<AccountValue>;
    /// DefineProperty data override after CopyDataProperties, bypassing setters.
    fn define_data(
        &self,
        object: &AccountValue,
        key: &str,
        value: AccountValue,
    ) -> SigningResult<()>;
}
fn text(s: &str) -> AccountValue {
    AccountValue::String(s.encode_utf16().collect())
}
fn number(n: f64) -> AccountValue {
    AccountValue::Number(n)
}
fn arg(args: &[AccountValue], i: usize) -> AccountValue {
    args.get(i).cloned().unwrap_or(AccountValue::Undefined)
}
fn undefined(v: &AccountValue) -> bool {
    matches!(v, AccountValue::Undefined)
}
fn eq(a: &AccountValue, b: &AccountValue) -> bool {
    match (a, b) {
        (AccountValue::Undefined, AccountValue::Undefined)
        | (AccountValue::Null, AccountValue::Null) => true,
        (AccountValue::Boolean(a), AccountValue::Boolean(b)) => a == b,
        (AccountValue::Number(a), AccountValue::Number(b)) => a == b,
        (AccountValue::BigInt(a), AccountValue::BigInt(b)) => a == b,
        (AccountValue::String(a), AccountValue::String(b)) => a == b,
        (AccountValue::Object(a), AccountValue::Object(b)) => Rc::ptr_eq(a, b),
        (AccountValue::Function(a), AccountValue::Function(b)) => a.ptr_eq(b),
        (AccountValue::Symbol(a), AccountValue::Symbol(b)) => a.ptr_eq(b),
        _ => false,
    }
}
fn optional(h: &dyn LiveHelperHost, v: &AccountValue, key: &str) -> SigningResult<AccountValue> {
    if v.is_nullish() {
        Ok(AccountValue::Undefined)
    } else {
        h.get(v, key)
    }
}
fn path(h: &dyn LiveHelperHost, v: &AccountValue, keys: &[&str]) -> SigningResult<AccountValue> {
    let mut v = v.clone();
    for key in keys {
        v = optional(h, &v, key)?
    }
    Ok(v)
}
fn method(
    h: &dyn LiveHelperHost,
    v: &AccountValue,
    key: &str,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    h.member(v, key)?.call(args)
}
fn template(
    h: &dyn LiveHelperHost,
    parts: &[&str],
    values: &[AccountValue],
) -> SigningResult<AccountValue> {
    let mut s = parts[0].encode_utf16().collect::<Vec<_>>();
    for (i, v) in values.iter().enumerate() {
        s.extend(h.template_string(v.clone())?);
        s.extend(parts[i + 1].encode_utf16())
    }
    Ok(AccountValue::String(s))
}
fn includes_any(
    h: &dyn LiveHelperHost,
    message: &AccountValue,
    needles: &[&str],
) -> SigningResult<AccountValue> {
    let mut result = AccountValue::Boolean(false);
    for needle in needles {
        result = method(h, message, "includes", &[text(needle)])?;
        if result.truthy() {
            return Ok(result);
        }
    }
    Ok(result)
}
fn error_code(
    h: &dyn LiveHelperHost,
    error: &AccountValue,
    nested: bool,
) -> SigningResult<AccountValue> {
    let mut code = optional(h, error, "code")?;
    if code.is_nullish() {
        code = path(h, error, &["response", "data", "code"])?
    }
    if nested && code.is_nullish() {
        code = path(h, error, &["response", "data", "error", "code"])?
    }
    Ok(code)
}
// ts: LighterExchange#parseAccountIndex
pub fn parse_account_index(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let value = arg(args, 0);
    if value.is_nullish() {
        return Ok(AccountValue::Undefined);
    }
    let result = (|| {
        let bigint = h.global("BigInt")?;
        let string = h.global("String")?.call(&[value])?;
        bigint.call(&[string])
    })();
    Ok(result.unwrap_or(AccountValue::Undefined))
}
// ts: LighterExchange#inferStopLossTypeFromReduceOnlyLimit
pub fn infer_stop_loss_type_from_reduce_only_limit(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let order = arg(args, 0);
    if !eq(&h.get(&order, "type")?, &h.constant("OrderType.Limit")?)
        || !h.get(&order, "reduceOnly")?.truthy()
    {
        return Ok(AccountValue::Undefined);
    }
    let finite = h.global("Number.isFinite")?;
    if !finite.call(&[h.get(&order, "price")?])?.truthy()
        || h.binary(
            OrderMapBinary::LessOrEqual,
            h.get(&order, "price")?,
            number(0.0),
        )?
        .truthy()
    {
        return Ok(AccountValue::Undefined);
    }
    let store = h.get(&h.owner(), "store")?;
    let tickers = h.get(&store, "tickersBySymbol")?;
    let ticker = h.index(&tickers, h.get(&order, "symbol")?)?;
    let positive = h.member(&h.owner(), "getPositivePriceCandidate")?;
    let reference = positive.call(&[
        optional(h.as_ref(), &ticker, "mark")?,
        optional(h.as_ref(), &ticker, "last")?,
        optional(h.as_ref(), &ticker, "index")?,
    ])?;
    if !reference.truthy() {
        return Ok(AccountValue::Undefined);
    }
    let sell = eq(&h.get(&order, "side")?, &h.constant("OrderSide.Sell")?);
    let loss = h
        .binary(
            if sell {
                OrderMapBinary::LessOrEqual
            } else {
                OrderMapBinary::GreaterOrEqual
            },
            h.get(&order, "price")?,
            reference,
        )?
        .truthy();
    if loss {
        h.constant("OrderType.StopLoss")
    } else {
        Ok(AccountValue::Undefined)
    }
}
// ts: LighterExchange#extractL1AddressCandidate
pub fn extract_l1_address_candidate(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let value = arg(args, 0);
    if !value.truthy() {
        return Ok(AccountValue::Undefined);
    }
    let trimmed = method(h.as_ref(), &value, "trim", &[])?;
    if h.global("isLighterL1Address")?
        .call(&[trimmed.clone()])?
        .truthy()
    {
        return Ok(trimmed);
    }
    let matcher = h.member(&trimmed, "match")?;
    let regex = h.regex_literal("0x[a-fA-F0-9]{40}", "")?;
    let matched = matcher.call(&[regex])?;
    if matched.is_nullish() {
        Ok(AccountValue::Undefined)
    } else {
        h.index(&matched, number(0.0))
    }
}
// ts: LighterExchange#isWasmRuntimeIssueMessage
pub fn is_wasm_runtime_issue_message(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let normalized = method(h.as_ref(), &arg(args, 0), "toLowerCase", &[])?;
    includes_any(
        h.as_ref(),
        &normalized,
        &[
            "wasm not initialized",
            "failed to fetch dynamically imported module",
            "signerclient export not found",
            "leverage signer unavailable in runtime",
            "node:fs",
        ],
    )
}
// ts: LighterExchange#getInitialTickDelay
pub fn get_initial_tick_delay(
    h: Rc<dyn LiveHelperHost>,
    _args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let tick = h.member(&h.owner(), "getTickInterval")?.call(&[])?;
    if tick.truthy()
        && h.binary(OrderMapBinary::GreaterThan, tick.clone(), number(0.0))?
            .truthy()
    {
        let min = h.global("Math.min")?;
        min.call(&[tick, h.constant("LIGHTER_INITIAL_TICK_MAX_DELAY_MS")?])
    } else {
        h.constant("LIGHTER_INITIAL_TICK_MAX_DELAY_MS")
    }
}
// ts: LighterExchange#parseMarginMode
pub fn parse_margin_mode(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let value = arg(args, 0);
    if value.is_nullish() {
        return Ok(AccountValue::Undefined);
    }
    if eq(&value, &number(0.0)) || eq(&value, &text("0")) {
        return Ok(number(0.0));
    }
    if eq(&value, &number(1.0)) || eq(&value, &text("1")) {
        return Ok(number(1.0));
    }
    let normalized = h.global("String")?.call(&[value])?;
    let normalized = method(h.as_ref(), &normalized, "toLowerCase", &[])?;
    if eq(&normalized, &text("cross")) {
        Ok(number(0.0))
    } else if eq(&normalized, &text("isolated")) {
        Ok(number(1.0))
    } else {
        Ok(AccountValue::Undefined)
    }
}
// ts: LighterExchange#marginModeToMarginType
pub fn margin_mode_to_margin_type(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    h.constant(if eq(&arg(args, 0), &number(1.0)) {
        "MarginType.Isolated"
    } else {
        "MarginType.Cross"
    })
}
// ts: LighterExchange#marginTypeToMarginMode
pub fn margin_type_to_margin_mode(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    Ok(number(
        if eq(&arg(args, 0), &h.constant("MarginType.Isolated")?) {
            1.0
        } else {
            0.0
        },
    ))
}
// ts: LighterExchange#getMarketConfiguredMarginMode
pub fn get_market_configured_margin_mode(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let info = optional(h.as_ref(), &arg(args, 0), "info")?;
    let parse = h.member(&h.owner(), "parseMarginMode")?;
    let mut mode = path(h.as_ref(), &info, &["market_config", "market_margin_mode"])?;
    if mode.is_nullish() {
        mode = path(h.as_ref(), &info, &["marketConfig", "marketMarginMode"])?
    }
    if mode.is_nullish() {
        mode = optional(h.as_ref(), &info, "market_margin_mode")?
    }
    if mode.is_nullish() {
        mode = optional(h.as_ref(), &info, "marketMarginMode")?
    }
    parse.call(&[mode])
}
// ts: LighterExchange#getOhlcvFallbackKey
pub fn get_ohlcv_fallback_key(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let opts = arg(args, 0);
    let mut s = h.template_string(h.get(&opts, "symbol")?)?;
    s.extend(":".encode_utf16());
    s.extend(h.template_string(h.get(&opts, "interval")?)?);
    Ok(AccountValue::String(s))
}
// ts: LighterExchange#getReduceOnlyClosingPositionSide
pub fn get_reduce_only_closing_position_side(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let buy = eq(&arg(args, 0), &h.constant("OrderSide.Buy")?);
    h.constant(if buy {
        "PositionSide.Short"
    } else {
        "PositionSide.Long"
    })
}
// ts: LighterExchange#hasOpenReduceOnlyTargetPosition
pub fn has_open_reduce_only_target_position(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let symbol = arg(args, 0);
    let side = arg(args, 1);
    let positions = arg(args, 2);
    let positions = if undefined(&positions) {
        let store = h.get(&h.owner(), "store")?;
        h.get(&store, "positions")?
    } else {
        positions
    };
    let closing = h
        .member(&h.owner(), "getReduceOnlyClosingPositionSide")?
        .call(&[side])?;
    let some = h.member(&positions, "some")?;
    let live = h.clone();
    let callback = h.function(Rc::new(move |args| {
        let position = arg(args, 0);
        if !eq(&live.get(&position, "symbol")?, &symbol)
            || !eq(&live.get(&position, "side")?, &closing)
        {
            return Ok(AccountValue::Boolean(false));
        }
        live.binary(
            OrderMapBinary::GreaterThan,
            live.get(&position, "contracts")?,
            number(0.0),
        )
    }))?;
    some.call(&[callback])
}
// ts: LighterExchange#buildNoPositionReduceOnlyMessage
pub fn build_no_position_reduce_only_message(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let label = if eq(&arg(args, 1), &h.constant("OrderSide.Buy")?) {
        "short"
    } else {
        "long"
    };
    template(
        h.as_ref(),
        &[
            "Lighter rejected reduce-only limit order for ",
            ": no open ",
            " position to reduce.",
        ],
        &[arg(args, 0), text(label)],
    )
}
// ts: LighterExchange#isRateLimitOrderError
pub fn is_rate_limit_order_error(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let error = arg(args, 0);
    let number_ = h.global("Number")?;
    let mut status = optional(h.as_ref(), &error, "statusCode")?;
    if status.is_nullish() {
        status = path(h.as_ref(), &error, &["response", "status"])?
    }
    let status = number_.call(&[status])?;
    let string = h.global("String")?;
    let code = error_code(h.as_ref(), &error, false)?;
    let code = string.call(&[if code.is_nullish() { text("") } else { code }])?;
    let code = method(h.as_ref(), &code, "toLowerCase", &[])?;
    let message = method(h.as_ref(), &arg(args, 1), "toLowerCase", &[])?;
    if eq(&status, &number(429.0))
        || eq(&status, &number(405.0))
        || eq(&code, &text("429"))
        || eq(&code, &text("405"))
    {
        return Ok(AccountValue::Boolean(true));
    }
    includes_any(
        h.as_ref(),
        &message,
        &[
            "rate limit",
            "too many requests",
            "too many websocket messages",
            "volume quota",
        ],
    )
}
// ts: LighterExchange#isInvalidMarginModeOrderError
pub fn is_invalid_margin_mode_order_error(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let string = h.global("String")?;
    let code = error_code(h.as_ref(), &arg(args, 0), false)?;
    let code = string.call(&[if code.is_nullish() { text("") } else { code }])?;
    let code = method(h.as_ref(), &code, "toLowerCase", &[])?;
    let message = method(h.as_ref(), &arg(args, 1), "toLowerCase", &[])?;
    if eq(&code, &text("21613")) {
        Ok(AccountValue::Boolean(true))
    } else {
        method(
            h.as_ref(),
            &message,
            "includes",
            &[text("invalid margin mode")],
        )
    }
}
fn partner_error(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
    name: &str,
) -> SigningResult<AccountValue> {
    let imported = h.global(name)?;
    let code = error_code(h.as_ref(), &arg(args, 0), true)?;
    let input = h.literal(&[("code", code), ("message", arg(args, 1))])?;
    imported.call(&[input])
}
// ts: LighterExchange#isPartnerAttributionTierOrderError
pub fn is_partner_attribution_tier_order_error(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    partner_error(h, args, "isLighterPartnerAttributionTierRejection")
}
// ts: LighterExchange#isPartnerAttributionApprovalMissingOrderError
pub fn is_partner_attribution_approval_missing_order_error(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    partner_error(
        h,
        args,
        "isLighterPartnerAttributionApprovalMissingRejection",
    )
}
// ts: LighterExchange#buildInvalidMarginModeOrderMessage
pub fn build_invalid_margin_mode_order_message(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let symbol = arg(args, 0);
    let configured = h.member(&h.owner(), "getMarketConfiguredMarginMode")?;
    let store = h.get(&h.owner(), "store")?;
    let markets = h.get(&store, "marketsBySymbol")?;
    let market = h.index(&markets, symbol.clone())?;
    let mode = configured.call(&[market])?;
    let required = if eq(&mode, &number(1.0)) {
        "isolated"
    } else if eq(&mode, &number(0.0)) {
        "cross"
    } else {
        "the intended margin mode"
    };
    template(
        h.as_ref(),
        &[
            "Lighter rejected this order because the ",
            " margin mode appears out of sync. This market requires ",
            " to trade.",
        ],
        &[symbol, text(required)],
    )
}
// ts: LighterExchange#getOrderMinimumViolationReasons
pub fn get_order_minimum_violation_reasons(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let amount = arg(args, 0);
    let execution = arg(args, 1);
    let base = arg(args, 2);
    let quote = arg(args, 3);
    let notional = h.binary(OrderMapBinary::Multiply, amount.clone(), execution)?;
    let below_base = h
        .binary(OrderMapBinary::GreaterThan, base.clone(), number(0.0))?
        .truthy()
        && h.less_than(amount.clone(), base.clone())?.truthy();
    let below_quote = h
        .binary(OrderMapBinary::GreaterThan, quote.clone(), number(0.0))?
        .truthy()
        && h.less_than(notional.clone(), quote.clone())?.truthy();
    if !below_base && !below_quote {
        return Ok(AccountValue::Undefined);
    }
    let a = if below_base {
        template(
            h.as_ref(),
            &["base ", " < min_base_amount ", ""],
            &[amount, base],
        )?
    } else {
        AccountValue::Undefined
    };
    let b = if below_quote {
        template(
            h.as_ref(),
            &["quote ", " < min_quote_amount ", ""],
            &[notional, quote],
        )?
    } else {
        AccountValue::Undefined
    };
    let array = h.array(&[a, b])?;
    let filter = h.member(&array, "filter")?;
    let boolean = h.global_value("Boolean")?;
    let filtered = filter.call(&[boolean])?;
    method(h.as_ref(), &filtered, "join", &[text(", ")])
}
// ts: LighterExchange#getPositivePriceCandidate
pub fn get_positive_price_candidate(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let values = h.array(args)?;
    let find = h.member(&values, "find")?;
    let live = h.clone();
    let callback = h.function(Rc::new(move |args| {
        let value = arg(args, 0);
        if !matches!(value, AccountValue::Number(_)) {
            return Ok(AccountValue::Boolean(false));
        }
        if !live
            .global("Number.isFinite")?
            .call(&[value.clone()])?
            .truthy()
        {
            return Ok(AccountValue::Boolean(false));
        }
        live.binary(OrderMapBinary::GreaterThan, value, number(0.0))
    }))?;
    find.call(&[callback])
}
// ts: LighterExchange#resolveMarketOrderReferencePrice
pub fn resolve_market_order_reference_price(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let symbol = arg(args, 0);
    let side = arg(args, 1);
    let store = h.get(&h.owner(), "store")?;
    let tickers = h.get(&store, "tickersBySymbol")?;
    let ticker = h.index(&tickers, symbol.clone())?;
    if !ticker.truthy() {
        let error = h.global("Error")?;
        let message = template(
            h.as_ref(),
            &["Ticker ", " not found for market-style order"],
            &[symbol],
        )?;
        return Err(h.throw_value(error.construct(&[message])?));
    }
    let midpoint = if h
        .binary(
            OrderMapBinary::GreaterThan,
            h.get(&ticker, "bid")?,
            number(0.0),
        )?
        .truthy()
        && h.binary(
            OrderMapBinary::GreaterThan,
            h.get(&ticker, "ask")?,
            number(0.0),
        )?
        .truthy()
    {
        let finite = h.global("Number.isFinite")?;
        if finite.call(&[h.get(&ticker, "bid")?])?.truthy() {
            let finite = h.global("Number.isFinite")?;
            if finite.call(&[h.get(&ticker, "ask")?])?.truthy() {
                let sum = h.binary(
                    OrderMapBinary::Add,
                    h.get(&ticker, "bid")?,
                    h.get(&ticker, "ask")?,
                )?;
                h.binary(OrderMapBinary::Divide, sum, number(2.0))?
            } else {
                AccountValue::Undefined
            }
        } else {
            AccountValue::Undefined
        }
    } else {
        AccountValue::Undefined
    };
    let side_price = h.get(
        &ticker,
        if eq(&side, &h.constant("OrderSide.Buy")?) {
            "ask"
        } else {
            "bid"
        },
    )?;
    let opposite = h.get(
        &ticker,
        if eq(&side, &h.constant("OrderSide.Buy")?) {
            "bid"
        } else {
            "ask"
        },
    )?;
    let positive = h.member(&h.owner(), "getPositivePriceCandidate")?;
    let price = positive.call(&[
        side_price,
        h.get(&ticker, "mark")?,
        h.get(&ticker, "last")?,
        h.get(&ticker, "index")?,
        midpoint,
        opposite,
    ])?;
    if !price.truthy() {
        let error = h.global("Error")?;
        let message = template(
            h.as_ref(),
            &[
                "No live Lighter price available for market-style order (",
                ")",
            ],
            &[symbol],
        )?;
        return Err(h.throw_value(error.construct(&[message])?));
    }
    Ok(price)
}
// ts: LighterExchange#resolveStopSlippagePct
pub fn resolve_stop_slippage_pct(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let normalize = h.global("normalizeLighterStopSlippagePct")?;
    let first = normalize.call(&[h.get(&arg(args, 0), "maxSlippagePct")?])?;
    if !first.is_nullish() {
        return Ok(first);
    }
    let normalize = h.global("normalizeLighterStopSlippagePct")?;
    let options = h.get(&h.owner(), "options")?;
    let extra = h.get(&options, "extra")?;
    let fallback = normalize.call(&[optional(h.as_ref(), &extra, "lighterStopSlippagePct")?])?;
    if !fallback.is_nullish() {
        Ok(fallback)
    } else {
        h.constant("LIGHTER_DEFAULT_STOP_SLIPPAGE_PCT")
    }
}
// ts: LighterExchange#shouldUseBatchedCreateOrders
pub fn should_use_batched_create_orders(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let orders = arg(args, 0);
    if h.binary(
        OrderMapBinary::LessOrEqual,
        h.get(&orders, "length")?,
        number(1.0),
    )?
    .truthy()
    {
        return Ok(AccountValue::Boolean(false));
    }
    let first = h.index(&orders, number(0.0))?;
    let symbol = optional(h.as_ref(), &first, "symbol")?;
    let every = h.member(&orders, "every")?;
    let live = h.clone();
    let callback = h.function(Rc::new(move |args| {
        let order = arg(args, 0);
        if !eq(&live.get(&order, "symbol")?, &symbol)
            || !eq(
                &live.get(&order, "type")?,
                &live.constant("OrderType.Limit")?,
            )
            || live.get(&order, "reduceOnly")?.truthy()
            || !undefined(&live.get(&order, "stopLoss")?)
            || !undefined(&live.get(&order, "takeProfit")?)
            || undefined(&live.get(&order, "price")?)
        {
            return Ok(AccountValue::Boolean(false));
        }
        let finite = live.global("Number.isFinite")?;
        if !finite.call(&[live.get(&order, "price")?])?.truthy() {
            return Ok(AccountValue::Boolean(false));
        }
        live.binary(
            OrderMapBinary::GreaterThan,
            live.get(&order, "price")?,
            number(0.0),
        )
    }))?;
    every.call(&[callback])
}
// ts: LighterExchange#snapToLeverageTier
pub fn snap_to_leverage_tier(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let requested = arg(args, 0);
    let tiers = h.get(&h.class_constructor(), "VALID_LEVERAGE_TIERS")?;
    let mut closest = h.index(&tiers, number(0.0))?;
    let abs = h.global("Math.abs")?;
    let mut distance =
        abs.call(&[h.binary(OrderMapBinary::Subtract, requested.clone(), closest.clone())?])?;
    let mut iterator = h.iterator(&tiers)?;
    while let Some(tier) = iterator.next()? {
        let result = (|| {
            let abs = h.global("Math.abs")?;
            let next = abs.call(&[h.binary(
                OrderMapBinary::Subtract,
                requested.clone(),
                tier.clone(),
            )?])?;
            let closer = h.less_than(next.clone(), distance.clone())?.truthy();
            if closer {
                closest = tier;
                distance = next
            }
            Ok(())
        })();
        if let Err(error) = result {
            return Err(iterator.close_throw(error));
        }
    }
    Ok(closest)
}
// ts: LighterExchange#rememberPlacedOrderHint
pub fn remember_placed_order_hint(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let hints = h.get(&h.owner(), "placedOrderHints")?;
    let set = h.member(&hints, "set")?;
    let key = method(h.as_ref(), &arg(args, 0), "toString", &[])?;
    let hint = h.copy_data_properties(&[arg(args, 1)])?;
    let now = h.global("Date.now")?.call(&[])?;
    h.define_data(&hint, "createdAt", now)?;
    set.call(&[key, hint])?;
    loop {
        let hints = h.get(&h.owner(), "placedOrderHints")?;
        if !h
            .binary(
                OrderMapBinary::GreaterThan,
                h.get(&hints, "size")?,
                number(2000.0),
            )?
            .truthy()
        {
            break;
        }
        let hints = h.get(&h.owner(), "placedOrderHints")?;
        let keys = method(h.as_ref(), &hints, "keys", &[])?;
        let next = method(h.as_ref(), &keys, "next", &[])?;
        let oldest = h.get(&next, "value")?;
        if !oldest.truthy() {
            break;
        }
        let hints = h.get(&h.owner(), "placedOrderHints")?;
        method(h.as_ref(), &hints, "delete", &[oldest])?;
    }
    Ok(AccountValue::Undefined)
}
// ts: LighterExchange#readPlacedOrderHint
pub fn read_placed_order_hint(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let id = arg(args, 0);
    if !id.truthy() {
        return Ok(AccountValue::Undefined);
    }
    let hints = h.get(&h.owner(), "placedOrderHints")?;
    let hint = method(h.as_ref(), &hints, "get", &[id.clone()])?;
    if !hint.truthy() {
        return Ok(AccountValue::Undefined);
    }
    let now = h.global("Date.now")?.call(&[])?;
    let age = h.binary(OrderMapBinary::Subtract, now, h.get(&hint, "createdAt")?)?;
    if h.binary(OrderMapBinary::GreaterThan, age, number(600000.0))?
        .truthy()
    {
        let hints = h.get(&h.owner(), "placedOrderHints")?;
        method(h.as_ref(), &hints, "delete", &[id])?;
        return Ok(AccountValue::Undefined);
    }
    Ok(hint)
}
// ts: LighterExchange#rememberRecentAccountSnapshot
pub fn remember_recent_account_snapshot(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let version = arg(args, 1);
    if !eq(&version, &h.get(&h.owner(), "accountSnapshotCacheVersion")?) {
        return Ok(AccountValue::Undefined);
    }
    let now = h.global("virtualClock.getCurrentTimeMs")?.call(&[])?;
    let snapshot = h.literal(&[
        ("fetchedAt", now),
        ("cacheVersion", version),
        ("value", arg(args, 0)),
    ])?;
    h.set(&h.owner(), "recentAccountSnapshot", snapshot)?;
    Ok(AccountValue::Undefined)
}
// ts: LighterExchange.invalidateAccountSnapshotCache
pub fn invalidate_account_snapshot_cache(
    h: Rc<dyn LiveHelperHost>,
    _args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let owner = h.owner();
    let version = h.get(&owner, "accountSnapshotCacheVersion")?;
    let version = h.binary(OrderMapBinary::Add, version, number(1.0))?;
    h.set(&owner, "accountSnapshotCacheVersion", version)?;
    h.set(&h.owner(), "recentAccountSnapshot", AccountValue::Undefined)?;
    Ok(AccountValue::Undefined)
}
// ts: LighterExchange#getCachedAccountTier
pub fn get_cached_account_tier(
    h: Rc<dyn LiveHelperHost>,
    _args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let extract = h.global("extractLighterAccountTierFromPayload")?;
    extract.call(&[h.get(&h.owner(), "accountLimits")?])
}
fn find_position(
    h: Rc<dyn LiveHelperHost>,
    symbol: AccountValue,
    exposed: bool,
    finite_leverage: bool,
) -> SigningResult<AccountValue> {
    let store = h.get(&h.owner(), "store")?;
    let positions = h.get(&store, "positions")?;
    let find = h.member(&positions, "find")?;
    let live = h.clone();
    let callback = h.function(Rc::new(move |args| {
        let position = arg(args, 0);
        if !eq(&live.get(&position, "symbol")?, &symbol) {
            return Ok(AccountValue::Boolean(false));
        }
        if exposed
            && !live
                .binary(
                    OrderMapBinary::GreaterThan,
                    live.get(&position, "contracts")?,
                    number(0.0),
                )?
                .truthy()
        {
            return Ok(AccountValue::Boolean(false));
        }
        if finite_leverage {
            let finite = live.global("Number.isFinite")?;
            return finite.call(&[live.get(&position, "leverage")?]);
        }
        Ok(AccountValue::Boolean(true))
    }))?;
    find.call(&[callback])
}
// ts: LighterExchange#getCurrentMarginModeForSymbol
pub fn get_current_margin_mode_for_symbol(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let symbol = arg(args, 0);
    let modes = h.get(&h.owner(), "marginModeBySymbol")?;
    let mode = method(h.as_ref(), &modes, "get", &[symbol.clone()])?;
    if !undefined(&mode) {
        return Ok(mode);
    }
    let store = h.get(&h.owner(), "store")?;
    let markets = h.get(&store, "marketsBySymbol")?;
    let market = h.index(&markets, symbol.clone())?;
    let active = find_position(h.clone(), symbol.clone(), true, false)?;
    if active.truthy() {
        let convert = h.member(&h.owner(), "marginTypeToMarginMode")?;
        return convert.call(&[h.get(&active, "marginType")?]);
    }
    let configured = h
        .member(&h.owner(), "getMarketConfiguredMarginMode")?
        .call(&[market])?;
    if !undefined(&configured) {
        return Ok(configured);
    }
    let first = find_position(h.clone(), symbol, false, false)?;
    if first.truthy() {
        let convert = h.member(&h.owner(), "marginTypeToMarginMode")?;
        return convert.call(&[h.get(&first, "marginType")?]);
    }
    Ok(number(0.0))
}
// ts: LighterExchange#getCurrentLeverageForSymbol
pub fn get_current_leverage_for_symbol(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let symbol = arg(args, 0);
    let market = arg(args, 1);
    let exposed = find_position(h.clone(), symbol.clone(), true, true)?;
    let fallback = find_position(h.clone(), symbol, false, true)?;
    let mut candidate = optional(h.as_ref(), &exposed, "leverage")?;
    if candidate.is_nullish() {
        candidate = optional(h.as_ref(), &fallback, "leverage")?
    }
    if candidate.is_nullish() {
        let limits = h.get(&market, "limits")?;
        let leverage = h.get(&limits, "leverage")?;
        candidate = h.get(&leverage, "max")?
    }
    if candidate.is_nullish() {
        candidate = number(10.0)
    }
    let min = h.global("Math.min")?;
    let max = h.global("Math.max")?;
    let limits = h.get(&market, "limits")?;
    let leverage = h.get(&limits, "leverage")?;
    let low = h.get(&leverage, "min")?;
    let clamped = max.call(&[candidate, low])?;
    let limits = h.get(&market, "limits")?;
    let leverage = h.get(&limits, "leverage")?;
    let high = h.get(&leverage, "max")?;
    let clamped = min.call(&[clamped, high])?;
    h.member(&h.owner(), "snapToLeverageTier")?.call(&[clamped])
}
// ts: LighterExchange.shouldSkipOrderReconciliationFetch
pub fn should_skip_order_reconciliation_fetch(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let cooldown = arg(args, 0);
    let cooldown = if undefined(&cooldown) {
        h.constant("LIGHTER_ORDER_RECONCILIATION_COOLDOWN_MS")?
    } else {
        cooldown
    };
    let boolean = h.global("Boolean")?;
    let pending = boolean.call(&[h.get(&h.owner(), "ordersFetchPromise")?])?;
    if pending.truthy() {
        return Ok(pending);
    }
    let last = h.get(&h.owner(), "lastOrdersFetchAt")?;
    let positive = h.binary(OrderMapBinary::GreaterThan, last, number(0.0))?;
    if !positive.truthy() {
        return Ok(positive);
    }
    let now = h.global("virtualClock.getCurrentTimeMs")?.call(&[])?;
    let age = h.binary(
        OrderMapBinary::Subtract,
        now,
        h.get(&h.owner(), "lastOrdersFetchAt")?,
    )?;
    h.less_than(age, cooldown)
}
// ts: LighterExchange#resolveConditionalExecutionPrice
pub fn resolve_conditional_execution_price(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let slip = h
        .member(&h.owner(), "resolveStopSlippagePct")?
        .call(&[arg(args, 2)])?;
    let buy = eq(&arg(args, 1), &h.constant("OrderSide.Buy")?);
    let fraction = h.binary(OrderMapBinary::Divide, slip, number(100.0))?;
    let multiplier = h.binary(
        if buy {
            OrderMapBinary::Add
        } else {
            OrderMapBinary::Subtract
        },
        number(1.0),
        fraction,
    )?;
    let adjust = h.global("adjust")?;
    let price = h.binary(OrderMapBinary::Multiply, arg(args, 0), multiplier)?;
    let store = h.get(&h.owner(), "store")?;
    let markets = h.get(&store, "marketsBySymbol")?;
    let market = h.index(&markets, h.get(&arg(args, 2), "symbol")?)?;
    let precision = h.get(&market, "precision")?;
    adjust.call(&[price, h.get(&precision, "price")?])
}
// ts: parseLighterMarginFractionRatio
pub fn parse_lighter_margin_fraction_ratio(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let parse = h.global("Number.parseFloat")?;
    let value = arg(args, 0);
    let string = h
        .global("String")?
        .call(&[if value.is_nullish() { text("") } else { value }])?;
    let parsed = parse.call(&[string])?;
    if !h
        .global("Number.isFinite")?
        .call(&[parsed.clone()])?
        .truthy()
        || h.binary(OrderMapBinary::LessOrEqual, parsed.clone(), number(0.0))?
            .truthy()
    {
        return Ok(AccountValue::Undefined);
    }
    if h.binary(OrderMapBinary::GreaterThan, parsed.clone(), number(100.0))?
        .truthy()
    {
        return h.binary(
            OrderMapBinary::Divide,
            parsed,
            h.constant("LIGHTER_MARGIN_FRACTION_TICK")?,
        );
    }
    if h.binary(OrderMapBinary::GreaterThan, parsed.clone(), number(1.0))?
        .truthy()
    {
        return h.binary(OrderMapBinary::Divide, parsed, number(100.0));
    }
    Ok(parsed)
}
// ts: normalizeLighterStopSlippagePct
pub fn normalize_lighter_stop_slippage_pct(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let parse = h.global("Number.parseFloat")?;
    let value = arg(args, 0);
    let string = h
        .global("String")?
        .call(&[if value.is_nullish() { text("") } else { value }])?;
    let string = method(h.as_ref(), &string, "trim", &[])?;
    let parsed = parse.call(&[string])?;
    if !h
        .global("Number.isFinite")?
        .call(&[parsed.clone()])?
        .truthy()
        || h.less_than(parsed.clone(), number(0.0))?.truthy()
    {
        return Ok(AccountValue::Undefined);
    }
    let min = h.global("Math.min")?;
    min.call(&[parsed, h.constant("LIGHTER_MAX_STOP_SLIPPAGE_PCT")?])
}

// ts: parseLighterMarketSymbol
pub fn parse_lighter_market_symbol(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let symbol = arg(args, 0);
    let mut default_quote = arg(args, 1);
    if undefined(&default_quote) {
        default_quote = h.constant("LIGHTER_DEFAULT_QUOTE")?;
    }
    if method(h.as_ref(), &symbol, "includes", &[text("/")])?.truthy() {
        let parts = method(h.as_ref(), &symbol, "split", &[text("/")])?;
        let mut it = h.iterator(&parts)?;
        let (base, quote) = match it.next()? {
            None => (AccountValue::Undefined, AccountValue::Undefined),
            Some(base) => match it.next()? {
                None => (base, AccountValue::Undefined),
                Some(quote) => {
                    it.close_return()?;
                    (base, quote)
                }
            },
        };
        return h.literal(&[
            ("base", base),
            ("quote", if quote.truthy() { quote } else { default_quote }),
        ]);
    }
    let quotes = h.constant("LIGHTER_KNOWN_QUOTES")?;
    let find = h.member(&quotes, "find")?;
    let live = h.clone();
    let captured_symbol = symbol.clone();
    let callback = h.function(Rc::new(move |args| {
        method(live.as_ref(), &captured_symbol, "endsWith", &[arg(args, 0)])
    }))?;
    let quote = find.call(&[callback])?;
    if quote.truthy() {
        let slice = h.member(&symbol, "slice")?;
        let length = h.get(&quote, "length")?;
        let negative_length = h.negate(length)?;
        let base = slice.call(&[number(0.0), negative_length])?;
        return h.literal(&[("base", base), ("quote", quote)]);
    }
    h.literal(&[("base", symbol), ("quote", default_quote)])
}
// ts: isLighterPerpMarket
pub fn is_lighter_perp_market(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let market = arg(args, 0);
    let string = h.global("String")?;
    let mut kind = h.get(&market, "market_type")?;
    if kind.is_nullish() {
        kind = h.get(&market, "marketType")?;
    }
    if kind.is_nullish() {
        kind = text("");
    }
    let kind = string.call(&[kind])?;
    let kind = method(h.as_ref(), &kind, "toLowerCase", &[])?;
    Ok(AccountValue::Boolean(
        eq(&h.get(&market, "status")?, &text("active")) && !eq(&kind, &text("spot")),
    ))
}
// ts: getLighterMarketLeverageMax
pub fn get_lighter_market_leverage_max(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let market = arg(args, 0);
    let parse = h.global("Number.parseFloat")?;
    let string = h.global("String")?;
    let mut value = AccountValue::Undefined;
    for key in [
        "min_initial_margin_fraction",
        "minInitialMarginFraction",
        "default_initial_margin_fraction",
        "defaultInitialMarginFraction",
    ] {
        value = h.get(&market, key)?;
        if !value.is_nullish() {
            break;
        }
    }
    if value.is_nullish() {
        value = text("");
    }
    let margin = parse.call(&[string.call(&[value])?])?;
    if !h
        .global("Number.isFinite")?
        .call(&[margin.clone()])?
        .truthy()
        || h.binary(OrderMapBinary::LessOrEqual, margin.clone(), number(0.0))?
            .truthy()
    {
        return Ok(number(50.0));
    }
    h.binary(
        OrderMapBinary::Divide,
        h.constant("LIGHTER_MARGIN_FRACTION_TICK")?,
        margin,
    )
}
fn parse_v_chain(
    h: &dyn LiveHelperHost,
    p: &AccountValue,
    keys: &[&str],
    fallback: AccountValue,
) -> SigningResult<AccountValue> {
    let parse = h.global("Number.parseFloat")?;
    let string = h.global("String")?;
    let mut value = AccountValue::Undefined;
    for key in keys {
        value = h.global("v")?.call(&[p.clone(), text(key)])?;
        if !value.is_nullish() {
            break;
        }
    }
    if value.is_nullish() {
        value = fallback;
    }
    parse.call(&[string.call(&[value])?])
}
fn finite(h: &dyn LiveHelperHost, v: &AccountValue) -> SigningResult<bool> {
    Ok(h.global("Number.isFinite")?.call(&[v.clone()])?.truthy())
}
fn gt(h: &dyn LiveHelperHost, a: &AccountValue, b: f64) -> SigningResult<bool> {
    Ok(h.binary(OrderMapBinary::GreaterThan, a.clone(), number(b))?
        .truthy())
}
fn le(h: &dyn LiveHelperHost, a: &AccountValue, b: f64) -> SigningResult<bool> {
    Ok(h.binary(OrderMapBinary::LessOrEqual, a.clone(), number(b))?
        .truthy())
}
// ts: LighterExchange#resolvePositionLiquidationPrice
pub fn resolve_position_liquidation_price(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let p = arg(args, 0);
    let market = arg(args, 1);
    let side = arg(args, 2);
    let entry = arg(args, 3);
    let contracts = arg(args, 4);
    let leverage = arg(args, 5);
    let mode = arg(args, 6);
    let raw = parse_v_chain(
        h.as_ref(),
        &p,
        &["liquidationPrice", "liquidation_price"],
        number(0.0),
    )?;
    let raw_finite = finite(h.as_ref(), &raw)? && gt(h.as_ref(), &raw, 0.0)?;
    let profit = raw_finite
        && finite(h.as_ref(), &entry)?
        && gt(h.as_ref(), &entry, 0.0)?
        && ((eq(&side, &h.constant("PositionSide.Long")?)
            && h.binary(OrderMapBinary::GreaterOrEqual, raw.clone(), entry.clone())?
                .truthy())
            || (eq(&side, &h.constant("PositionSide.Short")?)
                && h.binary(OrderMapBinary::LessOrEqual, raw.clone(), entry.clone())?
                    .truthy()));
    if raw_finite && (!profit || !eq(&mode, &number(1.0))) {
        return Ok(raw);
    }
    let raw_or_zero = if raw_finite { raw } else { number(0.0) };
    if !eq(&mode, &number(1.0))
        || !finite(h.as_ref(), &entry)?
        || le(h.as_ref(), &entry, 0.0)?
        || !finite(h.as_ref(), &contracts)?
        || le(h.as_ref(), &contracts, 0.0)?
    {
        return Ok(raw_or_zero);
    }
    let allocated_raw = parse_v_chain(
        h.as_ref(),
        &p,
        &[
            "allocatedMargin",
            "allocated_margin",
            "marginSize",
            "margin_size",
        ],
        text(""),
    )?;
    let notional = h.binary(OrderMapBinary::Multiply, entry, contracts.clone())?;
    let fallback = if finite(h.as_ref(), &leverage)? && gt(h.as_ref(), &leverage, 0.0)? {
        h.binary(OrderMapBinary::Divide, notional.clone(), leverage)?
    } else {
        h.constant("Number.NaN")?
    };
    let allocated = if finite(h.as_ref(), &allocated_raw)? && gt(h.as_ref(), &allocated_raw, 0.0)? {
        allocated_raw
    } else {
        fallback
    };
    let parse = h.global("parseLighterMarginFractionRatio")?;
    let mut input = h
        .global("v")?
        .call(&[p.clone(), text("maintenanceMarginFraction")])?;
    if input.is_nullish() {
        input = h
            .global("v")?
            .call(&[p, text("maintenance_margin_fraction")])?;
    }
    let mut maintenance = parse.call(&[input])?;
    if maintenance.is_nullish() {
        let parse = h.global("parseLighterMarginFractionRatio")?;
        let info = h.get(&market, "info")?;
        let mut input = optional(h.as_ref(), &info, "maintenance_margin_fraction")?;
        if input.is_nullish() {
            input = optional(
                h.as_ref(),
                &h.get(&market, "info")?,
                "maintenanceMarginFraction",
            )?;
        }
        maintenance = parse.call(&[input])?;
    }
    if maintenance.is_nullish() {
        maintenance = number(0.0);
    }
    if !finite(h.as_ref(), &allocated)?
        || le(h.as_ref(), &allocated, 0.0)?
        || !finite(h.as_ref(), &maintenance)?
        || h.less_than(maintenance.clone(), number(0.0))?.truthy()
        || h.binary(
            OrderMapBinary::GreaterOrEqual,
            maintenance.clone(),
            number(1.0),
        )?
        .truthy()
    {
        return Ok(raw_or_zero);
    }
    let long = eq(&side, &h.constant("PositionSide.Long")?);
    let op = if long {
        OrderMapBinary::Subtract
    } else {
        OrderMapBinary::Add
    };
    let numerator = h.binary(op, notional, allocated)?;
    let factor = h.binary(op, number(1.0), maintenance)?;
    let denominator = h.binary(OrderMapBinary::Multiply, contracts, factor)?;
    let derived = h.binary(OrderMapBinary::Divide, numerator, denominator)?;
    if finite(h.as_ref(), &derived)? && gt(h.as_ref(), &derived, 0.0)? {
        return Ok(derived);
    }
    Ok(raw_or_zero)
}
// ts: LighterExchange#shouldShortCircuitSignerRetry
pub fn should_short_circuit_signer_retry(
    h: Rc<dyn LiveHelperHost>,
    _args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let now = h.global("virtualClock.getCurrentTimeMs")?.call(&[])?;
    let issue = h.member(&h.owner(), "isWasmRuntimeIssueMessage")?;
    let message = h.get(&h.owner(), "signerInitErrorMessage")?;
    let issue = issue.call(&[if message.truthy() { message } else { text("") }])?;
    if issue.truthy() && !h.global("isWasmReady")?.call(&[])?.truthy() {
        return Ok(AccountValue::Boolean(true));
    }
    h.less_than(now, h.get(&h.owner(), "signerRetryBlockedUntil")?)
}
// ts: LighterExchange#getSignerRetryBlockMessage
pub fn get_signer_retry_block_message(
    h: Rc<dyn LiveHelperHost>,
    _args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let now = h.global("virtualClock.getCurrentTimeMs")?.call(&[])?;
    let max = h.global("Math.max")?;
    let until = h.get(&h.owner(), "signerRetryBlockedUntil")?;
    let remaining = max.call(&[number(0.0), h.binary(OrderMapBinary::Subtract, until, now)?])?;
    let issue = h.member(&h.owner(), "isWasmRuntimeIssueMessage")?;
    let message = h.get(&h.owner(), "signerInitErrorMessage")?;
    let issue = issue.call(&[if message.truthy() { message } else { text("") }])?;
    let hint = if issue.truthy() && !h.global("isWasmReady")?.call(&[])?.truthy() {
        text("WASM runtime is still unavailable.")
    } else if gt(h.as_ref(), &remaining, 0.0)? {
        let ceil = h.global("Math.ceil")?;
        let seconds =
            ceil.call(&[h.binary(OrderMapBinary::Divide, remaining, number(1000.0))?])?;
        template(h.as_ref(), &["Retrying is paused for ", "s."], &[seconds])?
    } else {
        text("Retrying is temporarily paused.")
    };
    let reason = if h.get(&h.owner(), "signerInitErrorMessage")?.truthy() {
        template(
            h.as_ref(),
            &[" Last init failure: ", ""],
            &[h.get(&h.owner(), "signerInitErrorMessage")?],
        )?
    } else {
        text("")
    };
    template(
        h.as_ref(),
        &[
            "Lighter signer unavailable; execution is disabled until signer runtime recovers. ",
            "",
            "",
        ],
        &[hint, reason],
    )
}
// ts: LighterExchange#warnSignerUnavailable
pub fn warn_signer_unavailable(
    h: Rc<dyn LiveHelperHost>,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let now = h.global("virtualClock.getCurrentTimeMs")?.call(&[])?;
    let elapsed = h.binary(
        OrderMapBinary::Subtract,
        now.clone(),
        h.get(&h.owner(), "lastSignerWarningAt")?,
    )?;
    let cooldown = h.get(&h.class_constructor(), "SIGNER_WARNING_LOG_COOLDOWN_MS")?;
    if h.less_than(elapsed, cooldown)?.truthy() {
        return Ok(AccountValue::Undefined);
    }
    h.set(&h.owner(), "lastSignerWarningAt", now)?;
    let log = h.member(&h.owner(), "log")?;
    log.call(&[arg(args, 0), h.constant("LogSeverity.Warning")?])?;
    Ok(AccountValue::Undefined)
}
