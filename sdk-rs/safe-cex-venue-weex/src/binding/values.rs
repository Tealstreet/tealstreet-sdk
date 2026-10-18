use super::*;
use crate::utils::{self, WeexUtilityError, WeexUtilityInput};
use safe_cex_types::*;
pub(super) fn get(_: &Host, value: &Js, key: &str) -> Result<Js, Thrown> {
    value.get(key)
}
pub(super) fn optional(_: &Host, value: &Js, key: &str) -> Result<Js, Thrown> {
    if value.is_nullish() {
        Ok(Js::Undefined)
    } else {
        value.get(key)
    }
}
pub(super) fn define(_: &Host, value: &Js, key: &'static str, field: Js) -> Result<(), Thrown> {
    value.define(key, field)
}
pub(super) fn method(_: &Host, value: &Js, key: &'static str) -> Result<Js, Thrown> {
    safe_cex_exchange::js_runtime::scope::bound_member(value, key)
}
pub(super) fn invoke(_: &Host, function: Js, args: Vec<Js>) -> Result<Js, Thrown> {
    function.call(Js::Undefined, args)
}
pub(super) fn invoke_async<'a>(
    h: &'a Host,
    function: Js,
    args: Vec<Js>,
) -> Result<JsFuture<'a>, Thrown> {
    let pending = function.call(Js::Undefined, args)?;
    Ok(Box::pin(await_js(&h.0.exchange_runtime(), pending)))
}
pub(super) fn units(value: &Js) -> Result<Vec<u16>, Thrown> {
    Ok(value.to_js_string()?.to_vec())
}
pub(super) fn integer(value: &Js) -> Result<Js, Thrown> {
    if value.is_nullish() {
        Ok(Js::str(""))
    } else {
        Ok(Js::utf16(&value.to_js_string()?))
    }
}
pub(super) fn number(value: &Js, fallback: f64) -> Result<f64, Thrown> {
    let value = safe_cex_num::parse_float(&String::from_utf16_lossy(&value.to_js_string()?));
    Ok(if value.is_finite() { value } else { fallback })
}
pub(super) fn length(value: &Js) -> Result<usize, Thrown> {
    Ok(value.get("length")?.to_number()?.max(0.0) as usize)
}
pub(super) fn at(value: &Js, i: usize) -> Result<Js, Thrown> {
    value.get(&i.to_string())
}
pub(super) fn equals(value: &Js, text: &str) -> bool {
    value.strict_equals(&Js::str(text))
}
pub(super) fn capture(value: &Js) -> Result<WeexUtilityInput, Thrown> {
    to_typed(value)
}
pub(super) fn maybe_capture(value: &Js) -> Result<Option<WeexUtilityInput>, Thrown> {
    if value.is_undefined() {
        Ok(None)
    } else {
        capture(value).map(Some)
    }
}
pub(super) fn utility_error(error: WeexUtilityError) -> Thrown {
    Thrown::error(&error.name, &error.message)
}
pub(super) fn coercion(error: safe_cex_num::JsCoercionError) -> Thrown {
    Thrown::error(error.name, &error.message)
}
pub(super) fn timestamp(h: &Host, value: Js) -> Result<f64, Thrown> {
    let captured = maybe_capture(&value)?;
    utils::normalize_timestamp_to_ms(captured.as_ref(), |ms| (h.0.shared.extra.local_to_utc)(ms))
        .map_err(coercion)
}
pub(super) fn normalize(kind: &str, value: Js) -> Result<Js, Thrown> {
    let input = maybe_capture(&value)?;
    let input = input.as_ref();
    match kind {
        "position_mode" => from_typed(&utils::normalize_position_mode(input).map_err(coercion)?),
        "margin_type" => from_typed(&utils::normalize_margin_type(input).map_err(coercion)?),
        "position_side" => from_typed(&utils::normalize_position_side(input).map_err(coercion)?),
        "side" | "order_side" => from_typed(&utils::normalize_order_side(input).map_err(coercion)?),
        "status" => from_typed(&utils::normalize_order_status(input).map_err(coercion)?),
        "type" => from_typed(&utils::normalize_order_type(input).map_err(coercion)?),
        "trigger" => from_typed(&utils::normalize_trigger_type(input).map_err(coercion)?),
        _ => Err(Thrown::type_error("Unknown WEEX enum normalizer")),
    }
}
pub(super) fn extract(h: &Host, value: Js, keys: &[&str]) -> Result<Js, Thrown> {
    crate::selection::extract_array(
        &mut h.clone(),
        value,
        &keys
            .iter()
            .map(|s| s.encode_utf16().collect())
            .collect::<Vec<_>>(),
        &crate::selection::WeexArrayRuntime {
            is_array: |v| Ok(v.is_array()),
            is_nullish: Js::is_nullish,
            undefined: || Js::Undefined,
            get_property: |_, v, k| v.get_key(k),
            empty_array: |_| Js::array(std::vec![]),
        },
    )
}
pub(super) fn omit(h: &Host, value: Js) -> Result<Js, Thrown> {
    h.global("omitUndefined")?
        .call(Js::Undefined, std::vec![value])
}
pub(super) fn iterator(_: &Host, value: &Js) -> Result<Js, Thrown> {
    value.iterator()
}
pub(super) fn next(_: &Host, iterator: &Js) -> Result<Option<Js>, Thrown> {
    let step = iterator.call_method("next", std::vec![])?;
    if step.get("done")?.truthy() {
        Ok(None)
    } else {
        step.get("value").map(Some)
    }
}
pub(super) fn map(
    value: &Js,
    mut callback: impl FnMut(Js, usize) -> Result<Js, Thrown>,
) -> Result<Js, Thrown> {
    if !value.is_array() {
        return Err(Thrown::type_error("value.map is not a function"));
    }
    let n = length(value)?;
    let mut out = std::vec![None; n];
    for (i, slot) in out.iter_mut().enumerate() {
        if value.has_own(&i.to_string()) {
            *slot = Some(callback(at(value, i)?, i)?);
        }
    }
    Ok(Js::holey_array(out))
}
pub(super) fn filter(
    value: &Js,
    mut callback: impl FnMut(Js, usize) -> Result<bool, Thrown>,
) -> Result<Js, Thrown> {
    if !value.is_array() {
        return Err(Thrown::type_error("value.filter is not a function"));
    }
    let n = length(value)?;
    let mut out = std::vec![];
    for i in 0..n {
        if value.has_own(&i.to_string()) {
            let v = at(value, i)?;
            if callback(v.clone(), i)? {
                out.push(v);
            }
        }
    }
    Ok(Js::array(out))
}
pub(super) fn new_map(h: &Host, value: Js) -> Result<Js, Thrown> {
    h.global("Map")?.construct(std::vec![value], None)
}
pub(super) fn set(h: &Host, value: Js) -> Result<Js, Thrown> {
    h.global("Set")?.construct(std::vec![value], None)
}
pub(super) fn default_config(
    value: crate::symbol_settings::WeexDefaultSymbolConfig,
) -> Result<Js, Thrown> {
    Ok(Js::object(std::vec![
        ("marginType", from_typed(&value.margin_type)?),
        ("positionMode", from_typed(&value.position_mode)?),
        ("crossLeverage", Js::Number(value.cross_leverage)),
        (
            "isolatedLongLeverage",
            Js::Number(value.isolated_long_leverage),
        ),
        (
            "isolatedShortLeverage",
            Js::Number(value.isolated_short_leverage),
        ),
    ]))
}
pub(super) fn query_value(value: &Js) -> Result<QueryValue, Thrown> {
    Ok(match value {
        Js::Undefined => QueryValue::Undefined,
        Js::Null => QueryValue::Null,
        Js::Bool(v) => QueryValue::Bool(*v),
        Js::Number(v) => QueryValue::Number(*v),
        Js::String(_) | Js::BigInt(_) => {
            QueryValue::String(String::from_utf16_lossy(&value.to_js_string()?))
        }
        Js::Array(_) => QueryValue::Array(
            value
                .array_values()
                .unwrap()
                .iter()
                .map(query_value)
                .collect::<Result<_, _>>()?,
        ),
        Js::Object(_) => QueryValue::Object(
            value
                .own_entries()?
                .into_iter()
                .map(|(k, v)| Ok((String::from_utf16_lossy(&k), query_value(&v)?)))
                .collect::<Result<_, Thrown>>()?,
        ),
        _ => {
            return Err(Thrown::type_error(
                "WEEX query value has no transport query representation",
            ));
        }
    })
}
pub(super) fn optional_dummy(value: &Js, key: &str) -> Result<Js, Thrown> {
    if value.is_nullish() {
        Ok(Js::Undefined)
    } else {
        value.get(key)
    }
}
pub(super) fn market(h: &Host, raw: Js) -> Result<Js, Thrown> {
    match crate::market_data::normalize_market_row(&capture(&raw)?).map_err(utility_error)? {
        Ok(market) => from_typed(&market),
        Err(refusal) => {
            let received = refusal.value.map_or_else(
                || "undefined".into(),
                |value| {
                    value
                        .json_stringify()
                        .ok()
                        .flatten()
                        .unwrap_or_else(|| "undefined".into())
                },
            );
            h.warn(Js::str(&format!(
                "WEEX market row refused: {}; received: {}",
                refusal.error.message, received
            )))?;
            Ok(Js::Undefined)
        }
    }
}
pub(super) fn ticker(_: &Host, raw: Js, market: Js, book: Js) -> Result<Js, Thrown> {
    from_typed(
        &crate::market_data::normalize_ticker(
            maybe_capture(&raw)?.as_ref(),
            &to_typed(&market)?,
            maybe_capture(&book)?.as_ref(),
        )
        .map_err(utility_error)?,
    )
}
pub(super) fn candle(h: &Host, raw: Js) -> Result<Js, Thrown> {
    // TS materializes tuple slots first, but reads object aliases lazily. Never
    // capture the whole row: source does not inspect unrelated properties.
    let source = if raw.is_array() {
        Js::object(std::vec![
            ("timestamp", raw.get("0")?),
            ("open", raw.get("1")?),
            ("high", raw.get("2")?),
            ("low", raw.get("3")?),
            ("close", raw.get("4")?),
            ("volume", raw.get("5")?),
        ])
    } else {
        raw
    };
    let pick = |keys: &[&str]| -> Result<Js, Thrown> {
        let mut value = Js::Undefined;
        for key in keys {
            value = source.get(key)?;
            if !value.is_nullish() {
                break;
            }
        }
        Ok(value)
    };
    let ms = timestamp(h, pick(&["timestamp", "t", "time"])?)?;
    let timestamp = if ms == 0.0 {
        0.0
    } else {
        (ms / 1000.0).floor()
    };
    let open = number(&pick(&["open", "o"])?, 0.0)?;
    let high = number(&pick(&["high", "h"])?, 0.0)?;
    let low = number(&pick(&["low", "l"])?, 0.0)?;
    let close = number(&pick(&["close", "c"])?, 0.0)?;
    let volume = number(&pick(&["volume", "v"])?, 0.0)?;
    if [timestamp, open, high, low, close, volume]
        .iter()
        .any(|v| !v.is_finite())
    {
        return Ok(Js::Null);
    }
    Ok(Js::object(std::vec![
        ("timestamp", Js::Number(timestamp)),
        ("open", Js::Number(open)),
        ("high", Js::Number(high)),
        ("low", Js::Number(low)),
        ("close", Js::Number(close)),
        ("volume", Js::Number(volume)),
    ]))
}
pub(super) fn book_side(rows: Js) -> Result<Js, Thrown> {
    use safe_cex_exchange::js_runtime::iterator::IteratorRecord;
    let rows = if rows.truthy() {
        rows
    } else {
        Js::array(std::vec![])
    };
    let reduce = Js::function("normalizeOrderBookSide", |_, args| {
        let acc = arg(&args, 0);
        // Destructuring reads exactly two iterator values, then closes it. In
        // particular, an unused third tuple slot is never captured or traversed.
        let iterator = IteratorRecord::get(&arg(&args, 1))?;
        let price = iterator.step_value()?.unwrap_or(Js::Undefined);
        let amount = iterator.step_value()?.unwrap_or(Js::Undefined);
        iterator.close()?;
        let price = number(&price, 0.0)?;
        let amount = number(&amount, 0.0)?;
        if price > 0.0 && amount >= 0.0 {
            acc.call_method(
                "push",
                std::vec![Js::object(std::vec![
                    ("price", Js::Number(price)),
                    ("amount", Js::Number(amount)),
                    ("total", Js::Number(0.0)),
                ])],
            )?;
        }
        Ok(acc)
    });
    rows.call_method("reduce", std::vec![reduce, Js::array(std::vec![])])
}
pub(super) fn book(h: &Host, raw: Js) -> Result<Js, Thrown> {
    let bids = raw.get("bids")?;
    let bids = book_side(if bids.is_nullish() {
        raw.get("b")?
    } else {
        bids
    })?;
    let asks = raw.get("asks")?;
    let asks = book_side(if asks.is_nullish() {
        raw.get("a")?
    } else {
        asks
    })?;
    let book = Js::object(std::vec![("bids", bids), ("asks", asks)]);
    h.global("calcOrderBookTotal")?
        .call(Js::Undefined, std::vec![book.clone()])?;
    Ok(book)
}
pub(super) fn public_trade(h: &Host, raw: Js, symbol: &Js) -> Result<Js, Thrown> {
    from_typed(
        &crate::public_trade::normalize_public_trade(&capture(&raw)?, &symbol.to_display(), |ms| {
            (h.0.shared.extra.local_to_utc)(ms)
        })
        .map_err(utility_error)?,
    )
}
pub(super) fn execution(h: &Host, raw: Js, market: Js) -> Result<Js, Thrown> {
    let market = if market.is_nullish() {
        None
    } else {
        Some(to_typed::<Market>(&market)?)
    };
    match crate::fills::normalize_execution_row(&capture(&raw)?, market.as_ref(), |ms| {
        (h.0.shared.extra.local_to_utc)(ms)
    }) {
        Ok(execution) => from_typed(&execution),
        Err(crate::fills::ExecutionRowError::Source(error)) => Err(utility_error(error)),
        Err(crate::fills::ExecutionRowError::Projection(refusal)) => {
            h.warn(Js::str(&format!(
                "WEEX historical fill row refused: {}; received: {}",
                refusal.error.message,
                refusal
                    .value
                    .json_stringify()
                    .map_err(|cause| Thrown::type_error(&cause.message))?
                    .unwrap_or_else(|| "undefined".into())
            )))?;
            Ok(Js::Undefined)
        }
    }
}
pub(super) fn income(h: &Host, raw: Js) -> Result<Js, Thrown> {
    from_typed(
        &crate::income::normalize_income(&capture(&raw)?, |ms| (h.0.shared.extra.local_to_utc)(ms))
            .map_err(utility_error)?,
    )
}
