//! The maintained mapOrder body. Business decisions are Rust; required live
//! language/import operations preserve property capture, original throws, fresh
//! objects, regular expressions and IteratorClose. Full class binding is separate.
use crate::{
    exchange_trading_preflight::TradingHost,
    sdk_account_api::AccountValue,
    transaction_signing::{SigningResult, TransactionSigningError},
};
use std::rc::Rc;
#[derive(Clone, Copy)]
pub enum OrderMapBinary {
    Add,
    Subtract,
    Multiply,
    Divide,
    LessOrEqual,
    GreaterThan,
    GreaterOrEqual,
}
pub trait OrderMapIterator {
    /// IteratorStepValue; abrupt next/done/value does not close this iterator.
    fn next(&mut self) -> SigningResult<Option<AccountValue>>;
    /// IteratorClose with a nonthrow completion; close errors supersede it.
    fn close_return(&mut self) -> SigningResult<()>;
    /// IteratorClose with an original Throw completion and its precedence.
    fn close_throw(&mut self, error: TransactionSigningError) -> TransactionSigningError;
}
pub trait OrderMappingHost: TradingHost {
    fn iterator(&self, value: &AccountValue) -> SigningResult<Box<dyn OrderMapIterator>>;
    /// Fresh ordinary object and full CopyDataProperties in source order.
    fn copy_data_properties(&self, sources: &[AccountValue]) -> SigningResult<AccountValue>;
    fn binary(
        &self,
        op: OrderMapBinary,
        left: AccountValue,
        right: AccountValue,
    ) -> SigningResult<AccountValue>;
    /// Actual regex literal allocation; not a replace/test business substitute.
    fn regex_literal(&self, pattern: &str, flags: &str) -> SigningResult<AccountValue>;
}
fn text(v: &str) -> AccountValue {
    AccountValue::String(v.encode_utf16().collect())
}
fn num(v: f64) -> AccountValue {
    AccountValue::Number(v)
}
fn undef(v: &AccountValue) -> bool {
    matches!(v, AccountValue::Undefined)
}
fn optional(h: &dyn OrderMappingHost, v: &AccountValue, key: &str) -> SigningResult<AccountValue> {
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
        (AccountValue::BigInt(a), AccountValue::BigInt(b)) => a == b,
        (AccountValue::String(a), AccountValue::String(b)) => a == b,
        (AccountValue::Object(a), AccountValue::Object(b)) => Rc::ptr_eq(a, b),
        (AccountValue::Function(a), AccountValue::Function(b)) => a.ptr_eq(b),
        (AccountValue::Symbol(a), AccountValue::Symbol(b)) => a.ptr_eq(b),
        _ => false,
    }
}
fn method(
    h: &dyn OrderMappingHost,
    v: &AccountValue,
    key: &str,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    h.member(v, key)?.call(args)
}
fn for_of(
    h: &dyn OrderMappingHost,
    value: &AccountValue,
    mut body: impl FnMut(AccountValue) -> SigningResult<Option<AccountValue>>,
) -> SigningResult<Option<AccountValue>> {
    let mut it = h.iterator(value)?;
    while let Some(value) = it.next()? {
        match body(value) {
            Ok(Some(v)) => {
                it.close_return()?;
                return Ok(Some(v));
            }
            Ok(None) => {}
            Err(e) => return Err(it.close_throw(e)),
        }
    }
    Ok(None)
}
fn pair(
    h: &dyn OrderMappingHost,
    value: &AccountValue,
) -> SigningResult<(AccountValue, AccountValue)> {
    let mut it = h.iterator(value)?;
    let Some(a) = it.next()? else {
        return Ok((AccountValue::Undefined, AccountValue::Undefined));
    };
    let Some(b) = it.next()? else {
        return Ok((a, AccountValue::Undefined));
    };
    it.close_return()?;
    Ok((a, b))
}
fn normalize_key(h: &dyn OrderMappingHost, key: &AccountValue) -> SigningResult<AccountValue> {
    let replace = h.member(key, "replace")?;
    let re = h.regex_literal("[_\\s-]", "g")?;
    let v = replace.call(&[re, text("")])?;
    method(h, &v, "toLowerCase", &[])
}
fn read_loose(
    h: &dyn OrderMappingHost,
    source: &AccountValue,
    entries: &AccountValue,
    keys: &[&str],
) -> SigningResult<AccountValue> {
    let keys = h.array(&keys.iter().map(|v| text(v)).collect::<Vec<_>>())?;
    if let Some(v) = for_of(h, &keys, |key| {
        let direct = h.global("v")?.call(&[source.clone(), key])?;
        Ok((!undef(&direct)).then_some(direct))
    })? {
        return Ok(v);
    }
    Ok(for_of(h, entries, |entry| {
        let (raw_key, raw_value) = pair(h, &entry)?;
        let normalized = normalize_key(h, &raw_key)?;
        for_of(h, &keys, |key| {
            let candidate = normalize_key(h, &key)?;
            Ok(eq(&normalized, &candidate).then(|| raw_value.clone()))
        })
    })?
    .unwrap_or(AccountValue::Undefined))
}
fn loose_number(h: &dyn OrderMappingHost, value: AccountValue) -> SigningResult<AccountValue> {
    if value.is_nullish() {
        return Ok(AccountValue::Undefined);
    }
    let parse = h.global("Number.parseFloat")?;
    let value = h.global("String")?.call(&[value])?;
    let value = method(h, &value, "trim", &[])?;
    let parsed = parse.call(&[value])?;
    if !h
        .global("Number.isFinite")?
        .call(&[parsed.clone()])?
        .truthy()
    {
        Ok(AccountValue::Undefined)
    } else {
        Ok(parsed)
    }
}
fn loose_bool(h: &dyn OrderMappingHost, value: AccountValue) -> SigningResult<AccountValue> {
    match value {
        AccountValue::Boolean(_) => Ok(value),
        AccountValue::Number(n) => Ok(AccountValue::Boolean(n != 0.0)),
        AccountValue::String(_) => {
            let v = method(h, &value, "trim", &[])?;
            let v = method(h, &v, "toLowerCase", &[])?;
            if eq(&v, &text("true")) || eq(&v, &text("1")) {
                Ok(AccountValue::Boolean(true))
            } else if eq(&v, &text("false")) || eq(&v, &text("0")) {
                Ok(AccountValue::Boolean(false))
            } else {
                Ok(AccountValue::Undefined)
            }
        }
        _ => Ok(AccountValue::Undefined),
    }
}
fn parse_id(h: &dyn OrderMappingHost, candidates: &[AccountValue]) -> SigningResult<AccountValue> {
    let candidates = h.array(candidates)?;
    let max = AccountValue::BigInt(
        "9223372036854775807"
            .parse()
            .expect("internal decimal BigInt literal"),
    );
    let zero = AccountValue::BigInt(0.into());
    Ok(for_of(h, &candidates, |candidate| {
        if candidate.is_nullish() {
            return Ok(None);
        }
        let raw = h.global("String")?.call(&[candidate])?;
        let raw = method(h, &raw, "trim", &[])?;
        let regex = h.regex_literal("^\\d+$", "")?;
        if !method(h, &regex, "test", &[raw.clone()])?.truthy() {
            return Ok(None);
        }
        // Return leaves the source try before the surrounding for-of closes.
        let parsed: SigningResult<Option<AccountValue>> = (|| {
            let parsed = h.global("BigInt")?.call(&[raw])?;
            if h.binary(OrderMapBinary::GreaterOrEqual, parsed.clone(), zero.clone())?
                .truthy()
                && h.binary(OrderMapBinary::LessOrEqual, parsed.clone(), max.clone())?
                    .truthy()
            {
                Ok(Some(method(h, &parsed, "toString", &[])?))
            } else {
                Ok(None)
            }
        })();
        match parsed {
            Ok(value) => Ok(value),
            Err(_) => Ok(None),
        }
    })?
    .unwrap_or(AccountValue::Undefined))
}
fn normalized_code(
    h: &dyn OrderMappingHost,
    value: &AccountValue,
    pattern: &str,
) -> SigningResult<AccountValue> {
    let s = h.global("String")?.call(&[if value.is_nullish() {
        text("")
    } else {
        value.clone()
    }])?;
    let s = method(h, &s, "toUpperCase", &[])?;
    let replace = h.member(&s, "replace")?;
    let regex = h.regex_literal(pattern, "g")?;
    replace.call(&[regex, text("_")])
}
fn raw_code(h: &dyn OrderMappingHost, value: &AccountValue) -> SigningResult<AccountValue> {
    h.global("String")?.call(&[if value.is_nullish() {
        text("")
    } else {
        value.clone()
    }])
}
fn explicit(h: &dyn OrderMappingHost, value: &AccountValue) -> SigningResult<bool> {
    if value.is_nullish() {
        return Ok(false);
    }
    let s = h.global("String")?.call(&[value.clone()])?;
    Ok(!eq(&method(h, &s, "trim", &[])?, &text("")))
}
fn math_max(
    h: &dyn OrderMappingHost,
    left: AccountValue,
    right: AccountValue,
) -> SigningResult<AccountValue> {
    let max = h.global("Math.max")?;
    let sub = h.binary(OrderMapBinary::Subtract, left, right)?;
    max.call(&[sub, num(0.0)])
}
// ts: LighterExchange.mapOrder
pub fn map_order(h: Rc<dyn OrderMappingHost>, o: AccountValue) -> SigningResult<AccountValue> {
    let nested = optional(h.as_ref(), &o, "order")?;
    let nested = if nested.truthy() {
        let v = h.get(&o, "order")?;
        if matches!(v, AccountValue::Object(_) | AccountValue::Null) {
            h.get(&o, "order")?
        } else {
            AccountValue::Undefined
        }
    } else {
        AccountValue::Undefined
    };
    let source = if nested.truthy() {
        h.copy_data_properties(&[o, nested])?
    } else {
        o
    };
    let entries = h.global("Object.entries")?.call(&[source.clone()])?;
    let read = |keys: &[&str]| read_loose(h.as_ref(), &source, &entries, keys);
    let raw_type = read(&[
        "orderTypeValue",
        "order_type_value",
        "Type",
        "orderType",
        "order_type",
        "type",
    ])?;
    let normalized_type = normalized_code(h.as_ref(), &raw_type, "[-\\s]")?;
    let raw_tif = read(&[
        "timeInForceValue",
        "time_in_force_value",
        "TimeInForce",
        "timeInForce",
        "time_in_force",
    ])?;
    let normalized_tif = normalized_code(h.as_ref(), &raw_tif, "-")?;
    let boolean = h.global("Boolean")?;
    let post_only = boolean.call(&[read(&["postOnly", "post_only"])?])?;
    let type_numeric = h.literal(&[
        ("0", h.constant("OrderType.Limit")?),
        ("1", h.constant("OrderType.Market")?),
        ("2", h.constant("OrderType.StopLoss")?),
        ("3", h.constant("OrderType.StopLoss")?),
        ("4", h.constant("OrderType.TakeProfit")?),
        ("5", h.constant("OrderType.TakeProfit")?),
    ])?;
    let tif_numeric = h.literal(&[
        ("0", h.constant("OrderTimeInForce.ImmediateOrCancel")?),
        ("1", h.constant("OrderTimeInForce.GoodTillCancel")?),
        ("2", h.constant("OrderTimeInForce.PostOnly")?),
    ])?;
    let mut kind = h.index(&h.constant("ORDER_TYPE")?, normalized_type)?;
    if !kind.truthy() {
        let key = raw_code(h.as_ref(), &raw_type)?;
        kind = h.index(&type_numeric, key)?;
        if !kind.truthy() {
            kind = h.constant("OrderType.Limit")?
        }
    }
    let mut tif = if post_only.truthy() {
        h.constant("OrderTimeInForce.PostOnly")?
    } else {
        let mut v = h.index(&h.constant("ORDER_TIME_IN_FORCE")?, normalized_tif)?;
        if !v.truthy() {
            let key = raw_code(h.as_ref(), &raw_tif)?;
            v = h.index(&tif_numeric, key)?;
            if !v.truthy() {
                v = h.constant("OrderTimeInForce.GoodTillCancel")?
            }
        }
        v
    };
    let market_id = read(&["marketId", "market_id", "marketIndex", "market_index"])?;
    let string = h.global("String")?;
    let symbol = read(&["symbol", "marketSymbol", "market_symbol"])?;
    let symbol = string.call(&[if symbol.is_nullish() {
        text("")
    } else {
        symbol
    }])?;
    let store = h.get(&h.owner(), "store")?;
    let by_id = h.get(&store, "marketsById")?;
    let mut market = h.index(&by_id, market_id.clone())?;
    if !market.truthy() {
        let store = h.get(&h.owner(), "store")?;
        let by_symbol = h.get(&store, "marketsBySymbol")?;
        market = h.index(&by_symbol, symbol.clone())?;
    }
    if !market.truthy() {
        h.global("console.warn")?.call(&[
            text("[Lighter] Market not found for order:"),
            if market_id.truthy() {
                market_id
            } else {
                symbol
            },
        ])?;
        return h.array(&[]);
    }
    let a = read(&["orderId", "order_id"])?;
    let b = read(&["orderIndex", "order_index"])?;
    let c = read(&["OrderId", "OrderIndex"])?;
    let order_id = parse_id(h.as_ref(), &[a, b, c])?;
    let hint = h
        .member(&h.owner(), "readPlacedOrderHint")?
        .call(&[order_id.clone()])?;
    let existing = if order_id.truthy() {
        let store = h.get(&h.owner(), "store")?;
        let orders = h.get(&store, "orders")?;
        let find = h.member(&orders, "find")?;
        let id = order_id.clone();
        let live = h.clone();
        let callback = h.function(Rc::new(move |args| {
            let v = args.first().cloned().unwrap_or(AccountValue::Undefined);
            Ok(AccountValue::Boolean(eq(&live.get(&v, "id")?, &id)))
        }))?;
        find.call(&[callback])?
    } else {
        AccountValue::Undefined
    };
    let has_type = explicit(h.as_ref(), &raw_type)?;
    if eq(&kind, &h.constant("OrderType.Limit")?)
        && optional(h.as_ref(), &hint, "type")?.truthy()
        && !eq(&h.get(&hint, "type")?, &h.constant("OrderType.Limit")?)
    {
        kind = h.get(&hint, "type")?
    }
    if eq(&kind, &h.constant("OrderType.Limit")?)
        && optional(h.as_ref(), &existing, "type")?.truthy()
        && !eq(&h.get(&existing, "type")?, &h.constant("OrderType.Limit")?)
    {
        kind = h.get(&existing, "type")?
    }
    if !has_type && eq(&kind, &h.constant("OrderType.Limit")?) {
        if optional(h.as_ref(), &hint, "type")?.truthy()
            && !eq(&h.get(&hint, "type")?, &h.constant("OrderType.Limit")?)
        {
            kind = h.get(&hint, "type")?
        } else if existing.truthy()
            && !eq(&h.get(&existing, "type")?, &h.constant("OrderType.Limit")?)
        {
            kind = h.get(&existing, "type")?
        }
    }
    let has_tif = explicit(h.as_ref(), &raw_tif)?;
    if !has_tif {
        if optional(h.as_ref(), &hint, "timeInForce")?.truthy() {
            tif = h.get(&hint, "timeInForce")?
        } else if optional(h.as_ref(), &existing, "timeInForce")?.truthy() {
            tif = h.get(&existing, "timeInForce")?
        }
    }
    let raw_amount = read(&[
        "initialBaseAmount",
        "initial_base_amount",
        "quantity",
        "size",
        "baseAmount",
        "base_amount",
        "qty",
        "base_size",
        "origQty",
        "orderQuantity",
        "amount",
    ])?;
    let raw_filled = read(&[
        "filledBaseAmount",
        "filled_base_amount",
        "filledQuantity",
        "filledAmount",
        "filledSize",
        "filled_quantity",
        "filledQty",
        "filled_base_size",
        "executedQty",
        "cumQty",
    ])?;
    let raw_remaining = read(&[
        "remainingBaseAmount",
        "remaining_base_amount",
        "remainingQuantity",
        "remainingAmount",
        "remainingSize",
        "remaining_quantity",
        "remainingQty",
        "remaining_base_size",
        "leavesQty",
        "openQty",
    ])?;
    let raw_quote = read(&[
        "quoteAmount",
        "quote_amount",
        "quoteQuantity",
        "quote_quantity",
        "notional",
    ])?;
    let raw_remaining_quote = read(&[
        "remainingQuoteAmount",
        "remaining_quote_amount",
        "remainingNotional",
        "remaining_notional",
        "remainingQuoteQty",
        "leavesQuoteQty",
        "openQuoteQty",
    ])?;
    let amount_parsed = loose_number(h.as_ref(), raw_amount)?;
    let filled_parsed = loose_number(h.as_ref(), raw_filled)?;
    let remaining_parsed = loose_number(h.as_ref(), raw_remaining)?;
    let quote_parsed = loose_number(h.as_ref(), raw_quote)?;
    let remaining_quote = loose_number(h.as_ref(), raw_remaining_quote)?;
    let amount = if !undef(&amount_parsed) {
        amount_parsed.clone()
    } else {
        h.binary(
            OrderMapBinary::Add,
            if filled_parsed.is_nullish() {
                num(0.0)
            } else {
                filled_parsed.clone()
            },
            if remaining_parsed.is_nullish() {
                num(0.0)
            } else {
                remaining_parsed.clone()
            },
        )?
    };
    let filled = if !undef(&filled_parsed) {
        filled_parsed.clone()
    } else if !undef(&amount_parsed) && !undef(&remaining_parsed) {
        math_max(h.as_ref(), amount_parsed.clone(), remaining_parsed.clone())?
    } else {
        num(0.0)
    };
    let mut remaining = if !undef(&remaining_parsed) {
        remaining_parsed
    } else {
        math_max(h.as_ref(), amount.clone(), filled.clone())?
    };
    let string = h.global("String")?;
    let raw_side = read(&["side"])?;
    let raw_side = string.call(&[if raw_side.is_nullish() {
        text("")
    } else {
        raw_side
    }])?;
    let raw_side = method(h.as_ref(), &raw_side, "toUpperCase", &[])?;
    let buy = loose_bool(h.as_ref(), read(&["isBuy"])?)?;
    let ask = loose_bool(h.as_ref(), read(&["isAsk", "is_ask"])?)?;
    let mut side = h.index(&h.constant("ORDER_SIDE")?, raw_side)?;
    if !side.truthy() {
        side = if !undef(&ask) {
            h.constant(if ask.truthy() {
                "OrderSide.Sell"
            } else {
                "OrderSide.Buy"
            })?
        } else if !undef(&buy) {
            h.constant(if buy.truthy() {
                "OrderSide.Buy"
            } else {
                "OrderSide.Sell"
            })?
        } else {
            let v = optional(h.as_ref(), &existing, "side")?;
            if v.truthy() {
                v
            } else {
                h.constant("OrderSide.Sell")?
            }
        }
    }
    let limit = loose_number(
        h.as_ref(),
        read(&["price", "Price", "limitPrice", "limit_price", "orderPrice"])?,
    )?;
    let trigger = loose_number(
        h.as_ref(),
        read(&[
            "triggerPrice",
            "trigger_price",
            "TriggerPrice",
            "stopPrice",
            "stop_price",
            "triggerPx",
            "trigger_px",
        ])?,
    )?;
    if eq(&kind, &h.constant("OrderType.Limit")?)
        && optional(h.as_ref(), &hint, "type")?.truthy()
        && !eq(&h.get(&hint, "type")?, &h.constant("OrderType.Limit")?)
    {
        kind = h.get(&hint, "type")?
    }
    if !has_tif && optional(h.as_ref(), &hint, "timeInForce")?.truthy() {
        tif = h.get(&hint, "timeInForce")?
    }
    if eq(&kind, &h.constant("OrderType.Limit")?)
        && !undef(&trigger)
        && h.binary(OrderMapBinary::GreaterThan, trigger.clone(), num(0.0))?
            .truthy()
    {
        if eq(
            &optional(h.as_ref(), &hint, "type")?,
            &h.constant("OrderType.StopLoss")?,
        ) || eq(
            &optional(h.as_ref(), &hint, "type")?,
            &h.constant("OrderType.TakeProfit")?,
        ) {
            kind = h.get(&hint, "type")?
        } else if eq(
            &optional(h.as_ref(), &existing, "type")?,
            &h.constant("OrderType.StopLoss")?,
        ) || eq(
            &optional(h.as_ref(), &existing, "type")?,
            &h.constant("OrderType.TakeProfit")?,
        ) {
            kind = h.get(&existing, "type")?
        }
    }
    if eq(&kind, &h.constant("OrderType.StopLoss")?)
        || eq(&kind, &h.constant("OrderType.TakeProfit")?)
    {
        if optional(h.as_ref(), &hint, "timeInForce")?.truthy() {
            tif = h.get(&hint, "timeInForce")?
        } else if optional(h.as_ref(), &existing, "timeInForce")?.truthy() {
            tif = h.get(&existing, "timeInForce")?
        }
    }
    let protective = eq(&kind, &h.constant("OrderType.StopLoss")?)
        || eq(&kind, &h.constant("OrderType.TakeProfit")?);
    let price = if protective
        && !undef(&trigger)
        && h.binary(OrderMapBinary::GreaterThan, trigger.clone(), num(0.0))?
            .truthy()
    {
        trigger
    } else {
        limit
    };
    let mut price = if price.is_nullish() { num(0.0) } else { price };
    if (!h
        .global("Number.isFinite")?
        .call(&[price.clone()])?
        .truthy()
        || h.binary(OrderMapBinary::LessOrEqual, price.clone(), num(0.0))?
            .truthy())
        && optional(h.as_ref(), &hint, "price")?.truthy()
        && h.binary(
            OrderMapBinary::GreaterThan,
            h.get(&hint, "price")?,
            num(0.0),
        )?
        .truthy()
    {
        price = h.get(&hint, "price")?
    }
    let string = h.global("String")?;
    let status = read(&["status"])?;
    let status = string.call(&[if status.is_nullish() {
        text("")
    } else {
        status
    }])?;
    let raw_status = method(h.as_ref(), &status, "toUpperCase", &[])?;
    let status = h.index(&h.constant("ORDER_STATUS")?, raw_status.clone())?;
    let status = if status.truthy() {
        status
    } else {
        h.constant("OrderStatus.Open")?
    };
    let open = eq(&status, &h.constant("OrderStatus.Open")?)
        || eq(&raw_status, &text("PENDING"))
        || eq(&raw_status, &text("PARTIALLY_FILLED"));
    if open
        && h.binary(OrderMapBinary::LessOrEqual, remaining.clone(), num(0.0))?
            .truthy()
        && h.binary(OrderMapBinary::GreaterThan, amount.clone(), num(0.0))?
            .truthy()
    {
        remaining = amount.clone()
    }
    if open
        && h.binary(OrderMapBinary::LessOrEqual, amount.clone(), num(0.0))?
            .truthy()
        && !undef(&remaining_quote)
        && h.binary(OrderMapBinary::GreaterThan, price.clone(), num(0.0))?
            .truthy()
    {
        remaining = h.binary(
            OrderMapBinary::Divide,
            remaining_quote.clone(),
            price.clone(),
        )?
    }
    let notional = if !undef(&remaining_quote) {
        remaining_quote
    } else if !undef(&quote_parsed)
        && h.binary(OrderMapBinary::GreaterThan, amount.clone(), num(0.0))?
            .truthy()
    {
        let ratio = h.binary(OrderMapBinary::Divide, remaining.clone(), amount.clone())?;
        h.binary(OrderMapBinary::Multiply, ratio, quote_parsed)?
    } else {
        h.binary(OrderMapBinary::Multiply, remaining.clone(), price.clone())?
    };
    let reduce = loose_bool(h.as_ref(), read(&["reduceOnly", "reduce_only"])?)?;
    let side = if side.truthy() {
        side
    } else {
        let v = optional(h.as_ref(), &hint, "side")?;
        if v.truthy() {
            v
        } else {
            let v = optional(h.as_ref(), &existing, "side")?;
            if v.truthy() {
                v
            } else {
                h.constant("OrderSide.Sell")?
            }
        }
    };
    let reduce = if !reduce.is_nullish() {
        reduce
    } else {
        let v = optional(h.as_ref(), &hint, "reduceOnly")?;
        if !v.is_nullish() {
            v
        } else {
            let v = optional(h.as_ref(), &existing, "reduceOnly")?;
            if !v.is_nullish() {
                v
            } else {
                AccountValue::Boolean(false)
            }
        }
    };
    let infer = h.member(&h.owner(), "inferStopLossTypeFromReduceOnlyLimit")?;
    let params = h.literal(&[
        ("symbol", h.get(&market, "symbol")?),
        ("type", kind.clone()),
        ("side", side.clone()),
        ("price", price.clone()),
        ("reduceOnly", reduce.clone()),
    ])?;
    let inferred = infer.call(&[params])?;
    if inferred.truthy() {
        kind = inferred
    }
    let id = h.global("String")?.call(&[if order_id.is_nullish() {
        text("")
    } else {
        order_id.clone()
    }])?;
    let market_symbol = h.get(&market, "symbol")?;
    let notional = if h
        .global("Number.isFinite")?
        .call(&[notional.clone()])?
        .truthy()
    {
        notional
    } else {
        num(0.0)
    };
    let extra = h.literal(&[
        ("remainingNative", remaining.clone()),
        ("remainingNotional", notional),
    ])?;
    let updated = h.global("virtualClock.getCurrentTimeMs")?.call(&[])?;
    let order = h.literal(&[
        ("id", id),
        ("status", status),
        ("symbol", market_symbol),
        ("type", kind),
        ("side", side),
        ("price", price),
        ("amount", amount),
        ("filled", filled),
        ("reduceOnly", reduce),
        ("remaining", remaining),
        ("extra", extra),
        ("info", source),
        ("timeInForce", tif),
        ("lastUpdate", updated),
    ])?;
    let orders = h.array(&[order])?;
    if !order_id.truthy() {
        h.array(&[])
    } else {
        Ok(orders)
    }
}
