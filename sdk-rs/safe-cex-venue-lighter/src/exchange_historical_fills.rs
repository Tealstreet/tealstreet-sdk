//! Historical execution lookup and its source-local normalizers/parsers.
//! Required live JS/import/iterator bindings preserve source evaluation order.
use crate::{
    exchange_trading_preflight::TradingHost,
    sdk_account_api::AccountValue,
    transaction_promise::{NativePromise, PromiseResolver},
    transaction_signing::{SigningResult, TransactionSigningError},
};
use std::rc::Rc;
#[derive(Clone, Copy)]
pub enum FillBinary {
    Multiply,
    Divide,
    Subtract,
    LessThan,
    LessOrEqual,
    GreaterThan,
}
pub type FillVisitor = Rc<dyn Fn(AccountValue) -> SigningResult<()>>;
pub trait HistoricalFillsHost: TradingHost {
    /// Full initial object-binding coercibility check, preserving the realm's
    /// destructuring diagnostic for opts before any property Get.
    fn require_options_binding(&self, value: &AccountValue) -> SigningResult<()>;
    /// Infallible realm binding of an actual fresh native async Promise, retaining
    /// identity and queued adoption for the full await_value operation.
    fn promise_value(&self, value: NativePromise<AccountValue>) -> AccountValue;
    /// Actual JS operator, including ordered coercion and original throws.
    fn binary(
        &self,
        op: FillBinary,
        left: AccountValue,
        right: AccountValue,
    ) -> SigningResult<AccountValue>;
    fn regex(&self, pattern: &[u16], flags: &[u16]) -> SigningResult<AccountValue>;
    /// Full for-of iteration and IteratorClose with original completion precedence.
    /// The Rust visitor performs each loop body, not a substituted host result.
    fn for_each(&self, iterable: AccountValue, visitor: FillVisitor) -> SigningResult<()>;
    /// Actual spread evaluation into a call/Array expression, including iterator
    /// access/advance/close and original throws. No business filtering is delegated.
    fn spread(&self, iterable: AccountValue) -> SigningResult<Vec<AccountValue>>;
}
fn text(s: &str) -> AccountValue {
    AccountValue::String(s.encode_utf16().collect())
}
fn arg(args: &[AccountValue], i: usize) -> AccountValue {
    args.get(i).cloned().unwrap_or(AccountValue::Undefined)
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
fn get(h: &dyn HistoricalFillsHost, key: &str) -> SigningResult<AccountValue> {
    h.get(&h.owner(), key)
}
fn call(
    h: &dyn HistoricalFillsHost,
    key: &str,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    h.member(&h.owner(), key)?.call(args)
}
fn method(
    h: &dyn HistoricalFillsHost,
    v: &AccountValue,
    key: &str,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    h.member(v, key)?.call(args)
}
fn optional(
    h: &dyn HistoricalFillsHost,
    v: &AccountValue,
    key: &str,
) -> SigningResult<AccountValue> {
    if v.is_nullish() {
        Ok(AccountValue::Undefined)
    } else {
        h.get(v, key)
    }
}
fn path(
    h: &dyn HistoricalFillsHost,
    v: &AccountValue,
    keys: &[&str],
) -> SigningResult<AccountValue> {
    let mut value = v.clone();
    for key in keys {
        value = optional(h, &value, key)?;
    }
    Ok(value)
}
fn first_truthy(
    h: &dyn HistoricalFillsHost,
    v: &AccountValue,
    keys: &[&str],
    default: AccountValue,
) -> SigningResult<AccountValue> {
    for key in keys {
        let value = h.get(v, key)?;
        if value.truthy() {
            return Ok(value);
        }
    }
    Ok(default)
}
fn is_finite(h: &dyn HistoricalFillsHost, value: AccountValue) -> SigningResult<bool> {
    Ok(h.global("Number.isFinite")?.call(&[value])?.truthy())
}
fn compare(
    h: &dyn HistoricalFillsHost,
    op: FillBinary,
    a: AccountValue,
    b: AccountValue,
) -> SigningResult<bool> {
    Ok(h.binary(op, a, b)?.truthy())
}
// ts-local: normalizeToMilliseconds
fn milliseconds(h: &dyn HistoricalFillsHost, value: AccountValue) -> SigningResult<AccountValue> {
    if value.is_nullish() || !is_finite(h, value.clone())? {
        return Ok(AccountValue::Undefined);
    }
    if compare(
        h,
        FillBinary::GreaterThan,
        value.clone(),
        AccountValue::Number(1_000_000_000_000.0),
    )? {
        h.global("Math.floor")?.call(&[value])
    } else {
        let floor = h.global("Math.floor")?;
        let value = h.binary(FillBinary::Multiply, value, AccountValue::Number(1000.0))?;
        floor.call(&[value])
    }
}
// ts-local: normalizeToSeconds
fn seconds(h: &dyn HistoricalFillsHost, value: AccountValue) -> SigningResult<AccountValue> {
    if value.is_nullish() {
        return Ok(AccountValue::Undefined);
    }
    if matches!(value, AccountValue::String(_)) {
        let regex = h.regex(&"^\\d+$".encode_utf16().collect::<Vec<_>>(), &[])?;
        if !method(h, &regex, "test", &[value.clone()])?.truthy() {
            let parsed = h.global("Date.parse")?.call(&[value])?;
            return if is_finite(h, parsed.clone())? {
                let floor = h.global("Math.floor")?;
                let n = h.binary(FillBinary::Divide, parsed, AccountValue::Number(1000.0))?;
                floor.call(&[n])
            } else {
                Ok(AccountValue::Undefined)
            };
        }
    }
    let parse = h.global("Number.parseInt")?;
    let string = h.global("String")?.call(&[value])?;
    let parsed = parse.call(&[string, AccountValue::Number(10.0)])?;
    if !is_finite(h, parsed.clone())?
        || compare(
            h,
            FillBinary::LessOrEqual,
            parsed.clone(),
            AccountValue::Number(0.0),
        )?
    {
        return Ok(AccountValue::Undefined);
    }
    if compare(
        h,
        FillBinary::GreaterThan,
        parsed.clone(),
        AccountValue::Number(1_000_000_000_000_000.0),
    )? {
        let floor = h.global("Math.floor")?;
        let n = h.binary(
            FillBinary::Divide,
            parsed,
            AccountValue::Number(1_000_000_000.0),
        )?;
        floor.call(&[n])
    } else if compare(
        h,
        FillBinary::GreaterThan,
        parsed.clone(),
        AccountValue::Number(1_000_000_000_000.0),
    )? {
        let floor = h.global("Math.floor")?;
        let n = h.binary(FillBinary::Divide, parsed, AccountValue::Number(1000.0))?;
        floor.call(&[n])
    } else {
        Ok(parsed)
    }
}
// ts-local: normalizeSymbolForMatching
fn normalized_symbol(
    h: &dyn HistoricalFillsHost,
    value: AccountValue,
) -> SigningResult<AccountValue> {
    let replace = h.member(&value, "replace")?;
    let regex = h.regex(
        &"[-_/]".encode_utf16().collect::<Vec<_>>(),
        &"g".encode_utf16().collect::<Vec<_>>(),
    )?;
    let value = replace.call(&[regex, text("")])?;
    method(h, &value, "toUpperCase", &[])
}
// ts-local: parseSide
fn parse_side(
    h: &dyn HistoricalFillsHost,
    row: &AccountValue,
    index: AccountValue,
) -> SigningResult<AccountValue> {
    let string = h.global("String")?;
    let side = first_truthy(h, row, &["side", "order_side"], text(""))?;
    let side = string.call(&[side])?;
    let side = method(h, &side, "toUpperCase", &[])?;
    let table = h.constant("ORDER_SIDE")?;
    if h.index(&table, side.clone())?.truthy() {
        return h.index(&h.constant("ORDER_SIDE")?, side);
    }
    for (key, direct, word, side) in [
        ("isBuy", true, "true", "OrderSide.Buy"),
        ("isAsk", true, "true", "OrderSide.Sell"),
        ("is_ask", true, "true", "OrderSide.Sell"),
        ("is_ask", false, "false", "OrderSide.Buy"),
    ] {
        if eq(&h.get(row, key)?, &AccountValue::Boolean(direct)) {
            return h.constant(side);
        }
        let string = h.global("String")?;
        let value = string.call(&[h.get(row, key)?])?;
        let value = method(h, &value, "toLowerCase", &[])?;
        if eq(&value, &text(word)) {
            return h.constant(side);
        }
    }
    if index.truthy() {
        let string = h.global("String")?;
        let mut ask = h.get(row, "ask_account_id")?;
        if ask.is_nullish() {
            ask = h.get(row, "askAccountId")?;
        }
        if ask.is_nullish() {
            ask = text("");
        }
        let ask = string.call(&[ask])?;
        let string = h.global("String")?;
        let mut bid = h.get(row, "bid_account_id")?;
        if bid.is_nullish() {
            bid = h.get(row, "bidAccountId")?;
        }
        if bid.is_nullish() {
            bid = text("");
        }
        let bid = string.call(&[bid])?;
        if ask.truthy() && eq(&ask, &index) {
            return h.constant("OrderSide.Sell");
        }
        if bid.truthy() && eq(&bid, &index) {
            return h.constant("OrderSide.Buy");
        }
    }
    Ok(AccountValue::Undefined)
}
// ts-local: extractRows
fn extract_rows(
    h: Rc<dyn HistoricalFillsHost>,
    payload: AccountValue,
) -> SigningResult<AccountValue> {
    let roots = h.array(&[
        payload.clone(),
        optional(h.as_ref(), &payload, "data")?,
        optional(h.as_ref(), &payload, "result")?,
        path(h.as_ref(), &payload, &["data", "result"])?,
    ])?;
    let flattened = h.array(&[])?;
    let live = h.clone();
    let all = flattened.clone();
    h.for_each(
        roots,
        Rc::new(move |root| {
            if !root.truthy() {
                return Ok(());
            }
            let h = live.clone();
            let candidates = h.array(&[
                optional(h.as_ref(), &root, "fills")?,
                optional(h.as_ref(), &root, "trades")?,
                optional(h.as_ref(), &root, "orders")?,
                optional(h.as_ref(), &root, "results")?,
                optional(h.as_ref(), &root, "list")?,
                optional(h.as_ref(), &root, "data")?,
                root.clone(),
            ])?;
            let live = h.clone();
            let flat = all.clone();
            h.for_each(
                candidates,
                Rc::new(move |candidate| {
                    let h = live.clone();
                    if h.global("Array.isArray")?
                        .call(&[candidate.clone()])?
                        .truthy()
                    {
                        let push = h.member(&flat, "push")?;
                        push.call(&h.spread(candidate)?)?;
                        return Ok(());
                    }
                    if candidate.truthy() && matches!(candidate, AccountValue::Object(_)) {
                        let values = h.global("Object.values")?.call(&[candidate])?;
                        let filter = h.member(&values, "filter")?;
                        let inner = h.clone();
                        let callback = h.function(Rc::new(move |args| {
                            inner.global("Array.isArray")?.call(&[arg(args, 0)])
                        }))?;
                        let nested = filter.call(&[callback])?;
                        let inner = h.clone();
                        let flat = flat.clone();
                        h.for_each(
                            nested,
                            Rc::new(move |value| {
                                let push = inner.member(&flat, "push")?;
                                push.call(&inner.spread(value)?)?;
                                Ok(())
                            }),
                        )?;
                    }
                    Ok(())
                }),
            )?;
            Ok(())
        }),
    )?;
    let filter = h.member(&flattened, "filter")?;
    let callback = h.function(Rc::new(move |args| {
        let row = arg(args, 0);
        Ok(if !row.truthy() {
            row
        } else {
            AccountValue::Boolean(matches!(row, AccountValue::Object(_)))
        })
    }))?;
    filter.call(&[callback])
}
struct Fills {
    market: AccountValue,
    order_id: AccountValue,
    from_ms: AccountValue,
    to_ms: AccountValue,
    max_limit: AccountValue,
}
fn or_fields(
    h: &dyn HistoricalFillsHost,
    row: &AccountValue,
    keys: &[&str],
) -> SigningResult<AccountValue> {
    let mut last = AccountValue::Undefined;
    for key in keys {
        last = h.get(row, key)?;
        if last.truthy() {
            return Ok(last);
        }
    }
    Ok(last)
}
// ts-local: parseRows
fn parse_rows(
    h: Rc<dyn HistoricalFillsHost>,
    ctx: Rc<Fills>,
    rows: AccountValue,
    index: AccountValue,
) -> SigningResult<AccountValue> {
    let reduce = h.member(&rows, "reduce")?;
    let live = h.clone();
    let callback = h.function(Rc::new(move |args| {
        let h = live.clone();
        let acc = arg(args, 0);
        let row = arg(args, 1);
        let store = get(h.as_ref(), "store")?;
        let markets = h.get(&store, "marketsById")?;
        let key = or_fields(
            h.as_ref(),
            &row,
            &["marketId", "market_id", "marketIndex", "market_index"],
        )?;
        let row_market = h.index(&markets, key)?;
        let string = h.global("String")?;
        let mut symbol = or_fields(
            h.as_ref(),
            &row,
            &["symbol", "marketSymbol", "market_symbol"],
        )?;
        if !symbol.truthy() {
            symbol = optional(h.as_ref(), &row_market, "symbol")?;
        }
        if !symbol.truthy() {
            symbol = h.get(&ctx.market, "symbol")?;
        }
        let symbol = string.call(&[symbol])?;
        let symbol = method(h.as_ref(), &symbol, "trim", &[])?;
        let left = normalized_symbol(h.as_ref(), symbol)?;
        let right = normalized_symbol(h.as_ref(), h.get(&ctx.market, "symbol")?)?;
        if !eq(&left, &right) {
            return Ok(acc);
        }
        let side = parse_side(h.as_ref(), &row, index.clone())?;
        if !side.truthy() {
            return Ok(acc);
        }
        let float = h.global("parseFloat")?;
        let price = first_truthy(
            h.as_ref(),
            &row,
            &[
                "price",
                "avgPrice",
                "averagePrice",
                "avg_execution_price",
                "fillPrice",
                "execPrice",
                "executedPrice",
            ],
            text("0"),
        )?;
        let price = float.call(&[price])?;
        let float = h.global("parseFloat")?;
        let qty = first_truthy(
            h.as_ref(),
            &row,
            &[
                "fillSize",
                "filledBaseAmount",
                "filled_base_amount",
                "filledSize",
                "filledAmount",
                "base_amount",
                "executedQty",
                "execQty",
                "size",
                "qty",
                "tradeSize",
            ],
            text("0"),
        )?;
        let qty = float.call(&[qty])?;
        if !is_finite(h.as_ref(), price.clone())?
            || !is_finite(h.as_ref(), qty.clone())?
            || compare(
                h.as_ref(),
                FillBinary::LessOrEqual,
                price.clone(),
                AccountValue::Number(0.0),
            )?
            || compare(
                h.as_ref(),
                FillBinary::LessOrEqual,
                qty.clone(),
                AccountValue::Number(0.0),
            )?
        {
            return Ok(acc);
        }
        let time = or_fields(
            h.as_ref(),
            &row,
            &[
                "fillTime",
                "timestamp",
                "updatedAt",
                "updated_at",
                "createdAt",
                "created_at",
                "transaction_time",
                "transactTime",
                "time",
            ],
        )?;
        let timestamp = seconds(h.as_ref(), time)?;
        if !timestamp.truthy() {
            return Ok(acc);
        }
        let string = h.global("String")?;
        let mut id = or_fields(
            h.as_ref(),
            &row,
            &[
                "orderId",
                "order_id",
                "orderIndex",
                "order_index",
                "orderID",
                "clientOrderId",
            ],
        )?;
        if !id.truthy() {
            id = if eq(&side, &h.constant("OrderSide.Sell")?) {
                or_fields(h.as_ref(), &row, &["ask_id", "askId"])?
            } else {
                or_fields(h.as_ref(), &row, &["bid_id", "bidId"])?
            };
        }
        if !id.truthy() {
            id = text("");
        }
        let id = string.call(&[id])?;
        if ctx.order_id.truthy() && id.truthy() {
            let filter = h.global("String")?.call(&[ctx.order_id.clone()])?;
            if !eq(&id, &filter) {
                return Ok(acc);
            }
        }
        let float = h.global("parseFloat")?;
        let mut fee = or_fields(h.as_ref(), &row, &["fee", "execFee", "tradeFee"])?;
        if !fee.truthy() {
            fee = if eq(&side, &h.constant("OrderSide.Sell")?) {
                or_fields(h.as_ref(), &row, &["maker_fee", "taker_fee"])?
            } else {
                or_fields(h.as_ref(), &row, &["taker_fee", "maker_fee"])?
            };
        }
        if !fee.truthy() {
            fee = text("0");
        }
        let fee = float.call(&[fee])?;
        let push = h.member(&acc, "push")?;
        let symbol = h.get(&ctx.market, "symbol")?;
        let id = if id.truthy() {
            id
        } else {
            AccountValue::Undefined
        };
        let commission = if is_finite(h.as_ref(), fee.clone())? {
            h.global("Math.abs")?.call(&[fee])?
        } else {
            AccountValue::Undefined
        };
        let mut asset = or_fields(h.as_ref(), &row, &["feeAsset", "feeCurrency"])?;
        if !asset.truthy() {
            asset = h.get(&ctx.market, "quote")?;
        }
        let execution = h.literal(&[
            ("timestamp", timestamp),
            ("symbol", symbol),
            ("orderId", id),
            ("price", price),
            ("qty", qty),
            ("side", side),
            ("commission", commission),
            ("commissionAsset", asset),
        ])?;
        push.call(&[execution])?;
        Ok(acc)
    }))?;
    reduce.call(&[callback, h.array(&[])?])
}
// ts-local: fetchRowsByEndpoint
fn fetch_rows(
    h: Rc<dyn HistoricalFillsHost>,
    ctx: Rc<Fills>,
    endpoint: AccountValue,
    index: AccountValue,
    auth: AccountValue,
) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        let index = method(h.as_ref(), &index, "toString", &[])?;
        let parse = h.global("Number.parseInt")?;
        let string = h.global("String")?;
        let market_id = string.call(&[h.get(&ctx.market, "id")?])?;
        let market_id = parse.call(&[market_id, AccountValue::Number(10.0)])?;
        let inactive_fields = vec![
            ("auth", auth.clone()),
            ("account_index", index.clone()),
            (
                "market_id",
                if is_finite(h.as_ref(), market_id.clone())? {
                    market_id.clone()
                } else {
                    AccountValue::Undefined
                },
            ),
            ("order_index", ctx.order_id.clone()),
            (
                "limit",
                h.global("Math.min")?
                    .call(&[ctx.max_limit.clone(), AccountValue::Number(100.0)])?,
            ),
        ];
        let _inactive = h.literal(&inactive_fields)?;
        let trades_fields = vec![
            ("auth", auth),
            ("account_index", index),
            (
                "market_id",
                if is_finite(h.as_ref(), market_id.clone())? {
                    market_id
                } else {
                    AccountValue::Undefined
                },
            ),
            ("order_index", ctx.order_id.clone()),
            ("sort_by", text("timestamp")),
            ("sort_dir", text("desc")),
            ("from", ctx.from_ms.clone()),
            (
                "limit",
                h.global("Math.min")?
                    .call(&[ctx.max_limit.clone(), AccountValue::Number(100.0)])?,
            ),
        ];
        let _trades = h.literal(&trades_fields)?;
        let client = get(h.as_ref(), "lighterApiClient")?;
        let get = h.member(&client, "get")?;
        let trades = eq(&endpoint, &h.constant("ENDPOINTS.TRADES")?);
        let params = h.literal(if trades {
            &trades_fields
        } else {
            &inactive_fields
        })?;
        let options = h.literal(&[("params", params)])?;
        let payload = get.call(&[endpoint, options])?;
        Ok(Step::Await(
            payload,
            Box::new(move |payload| Ok(Step::Done(extract_rows(h, payload?)?))),
        ))
    })
}
type Continuation = Box<dyn FnOnce(SigningResult<AccountValue>) -> SigningResult<Step>>;
enum Step {
    Done(AccountValue),
    Await(AccountValue, Continuation),
}
fn drive(
    h: Rc<dyn HistoricalFillsHost>,
    resolver: PromiseResolver<AccountValue>,
    step: SigningResult<Step>,
) {
    match step {
        Err(error) => resolver.settle(Err(error)),
        Ok(Step::Done(value)) => h.resolve_return(value, resolver),
        Ok(Step::Await(value, next)) => match h.await_value(value) {
            Err(error) => drive(h, resolver, next(Err(error))),
            Ok(p) => {
                p.subscribe(move |value| drive(h, resolver, next(value.map(|v| (*v).clone()))))
            }
        },
    }
}
fn start(
    h: Rc<dyn HistoricalFillsHost>,
    work: impl FnOnce() -> SigningResult<Step>,
) -> NativePromise<AccountValue> {
    let (p, r) = NativePromise::pending(h.runtime());
    let step = work();
    drive(h, r, step);
    p
}
fn warn(h: Rc<dyn HistoricalFillsHost>, error: TransactionSigningError) -> SigningResult<Step> {
    let log = h.member(&h.owner(), "log")?;
    let error = h.error_value(error);
    let message = optional(h.as_ref(), &error, "message")?;
    let message = if message.truthy() { message } else { error };
    let mut units = "Failed to fetch executions: "
        .encode_utf16()
        .collect::<Vec<_>>();
    units.extend(h.template_string(message)?);
    log.call(&[
        AccountValue::String(units),
        h.constant("LogSeverity.Warning")?,
    ])?;
    Ok(Step::Done(h.array(&[])?))
}
/// The source try covers the body and every explicit await, but not final
/// async-return resolution. Throws inside its catch are not caught a second time.
fn caught(h: Rc<dyn HistoricalFillsHost>, step: SigningResult<Step>) -> SigningResult<Step> {
    match step {
        Err(error) => warn(h, error),
        Ok(Step::Done(value)) => Ok(Step::Done(value)),
        Ok(Step::Await(value, next)) => Ok(Step::Await(
            value,
            Box::new(move |value| caught(h, next(value))),
        )),
    }
}
fn finish(
    h: Rc<dyn HistoricalFillsHost>,
    ctx: Rc<Fills>,
    primary: AccountValue,
    fallback: AccountValue,
    index: AccountValue,
) -> SigningResult<Step> {
    let deduped = h.global("Map")?.construct(&[])?;
    let account = method(h.as_ref(), &index, "toString", &[])?;
    let first = parse_rows(h.clone(), ctx.clone(), primary, account)?;
    let mut values = h.spread(first)?;
    let account = method(h.as_ref(), &index, "toString", &[])?;
    let second = parse_rows(h.clone(), ctx.clone(), fallback, account)?;
    values.extend(h.spread(second)?);
    let all = h.array(&values)?;
    let live = h.clone();
    let map = deduped.clone();
    let range = ctx.clone();
    h.for_each(
        all,
        Rc::new(move |execution| {
            let h = live.clone();
            if range.from_ms.truthy() {
                let time = h.get(&execution, "timestamp")?;
                let time = h.binary(FillBinary::Multiply, time, AccountValue::Number(1000.0))?;
                if compare(
                    h.as_ref(),
                    FillBinary::LessThan,
                    time,
                    range.from_ms.clone(),
                )? {
                    return Ok(());
                }
            }
            if range.to_ms.truthy() {
                let time = h.get(&execution, "timestamp")?;
                let time = h.binary(FillBinary::Multiply, time, AccountValue::Number(1000.0))?;
                if compare(
                    h.as_ref(),
                    FillBinary::GreaterThan,
                    time,
                    range.to_ms.clone(),
                )? {
                    return Ok(());
                }
            }
            let id = h.get(&execution, "orderId")?;
            let id = if id.truthy() { id } else { text("no-order") };
            let mut units = h.template_string(id)?;
            for key in ["timestamp", "price", "qty", "side"] {
                units.extend(":".encode_utf16());
                units.extend(h.template_string(h.get(&execution, key)?)?);
            }
            method(
                h.as_ref(),
                &map,
                "set",
                &[AccountValue::String(units), execution],
            )?;
            Ok(())
        }),
    )?;
    let values = method(h.as_ref(), &deduped, "values", &[])?;
    let values = h.spread(values)?;
    let values = h.array(&values)?;
    let sort = h.member(&values, "sort")?;
    let live = h.clone();
    let comparator = h.function(Rc::new(move |args| {
        let b = live.get(&arg(args, 1), "timestamp")?;
        let a = live.get(&arg(args, 0), "timestamp")?;
        live.binary(FillBinary::Subtract, b, a)
    }))?;
    let sorted = sort.call(&[comparator])?;
    let slice = h.member(&sorted, "slice")?;
    Ok(Step::Done(slice.call(&[
        AccountValue::Number(0.0),
        ctx.max_limit.clone(),
    ])?))
}
fn after_primary(
    h: Rc<dyn HistoricalFillsHost>,
    ctx: Rc<Fills>,
    primary: AccountValue,
    index: AccountValue,
    auth: AccountValue,
) -> SigningResult<Step> {
    if compare(
        h.as_ref(),
        FillBinary::GreaterThan,
        h.get(&primary, "length")?,
        AccountValue::Number(0.0),
    )? {
        let fallback = h.array(&[])?;
        finish(h, ctx, primary, fallback, index)
    } else {
        let endpoint = h.constant("ENDPOINTS.TRADES")?;
        let fallback = fetch_rows(h.clone(), ctx.clone(), endpoint, index.clone(), auth);
        let value = h.promise_value(fallback);
        Ok(Step::Await(
            value,
            Box::new(move |fallback| finish(h, ctx, primary, fallback?, index)),
        ))
    }
}
fn inside_try(h: Rc<dyn HistoricalFillsHost>, ctx: Rc<Fills>) -> SigningResult<Step> {
    let index = call(h.as_ref(), "ensureAccountIndex", &[])?;
    Ok(Step::Await(
        index,
        Box::new(move |index| {
            let index = index?;
            let auth = call(
                h.as_ref(),
                "getPrivateWsAuthToken",
                &[AccountValue::Number(60.0)],
            )?;
            Ok(Step::Await(
                auth,
                Box::new(move |auth| {
                    let auth = auth?;
                    if !auth.truthy() {
                        return Ok(Step::Done(h.array(&[])?));
                    }
                    let endpoint = h.constant("ENDPOINTS.INACTIVE_ORDERS")?;
                    let primary = fetch_rows(
                        h.clone(),
                        ctx.clone(),
                        endpoint,
                        index.clone(),
                        auth.clone(),
                    );
                    let value = h.promise_value(primary);
                    Ok(Step::Await(
                        value,
                        Box::new(move |primary| after_primary(h, ctx, primary?, index, auth)),
                    ))
                }),
            ))
        }),
    ))
}
// ts: LighterExchange.fetchHistoricalFills
pub fn fetch_historical_fills(
    h: Rc<dyn HistoricalFillsHost>,
    opts: AccountValue,
) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        if get(h.as_ref(), "isPublicDataOnly")?.truthy() {
            return Ok(Step::Done(h.array(&[])?));
        }
        h.require_options_binding(&opts)?;
        let symbol = h.get(&opts, "symbol")?;
        let order_id = h.get(&opts, "orderId")?;
        let from = h.get(&opts, "from")?;
        let to = h.get(&opts, "to")?;
        let limit = h.get(&opts, "limit")?;
        let limit = if matches!(limit, AccountValue::Undefined) {
            AccountValue::Number(100.0)
        } else {
            limit
        };
        if !symbol.truthy() {
            return Ok(Step::Done(h.array(&[])?));
        }
        let store = get(h.as_ref(), "store")?;
        let markets = h.get(&store, "marketsBySymbol")?;
        let market = h.index(&markets, symbol)?;
        if !market.truthy() {
            return Ok(Step::Done(h.array(&[])?));
        }
        let min = h.global("Math.min")?;
        let max = h.global("Math.max")?;
        let limit = max.call(&[limit, AccountValue::Number(1.0)])?;
        let max_limit = min.call(&[limit, AccountValue::Number(200.0)])?;
        let from_ms = milliseconds(h.as_ref(), from)?;
        let to_ms = milliseconds(h.as_ref(), to)?;
        let ctx = Rc::new(Fills {
            market,
            order_id,
            from_ms,
            to_ms,
            max_limit,
        });
        caught(h.clone(), inside_try(h, ctx))
    })
}
