use crate::exchange_class::ExchangeClassOps;

fn method<V, E>(
    h: &ExchangeClassOps<V, E>,
    receiver: V,
    key: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let value = &h.construction.value.value;
    let callee = (value.get)(&receiver, key)?;
    (value.invoke)(callee, receiver, args()?)
}

pub fn normalize_transaction_symbol<V: Clone, E>(
    h: &ExchangeClassOps<V, E>,
    raw: V,
) -> Result<V, E> {
    let value = &h.construction.value.value;
    if !(h.is_string)(&raw) {
        return Ok((value.undefined)());
    }
    let length = (value.get)(&raw, "length")?;
    if (value.strict_equal)(&length, &(value.number)(0.)) {
        return Ok((value.undefined)());
    }
    let normalize = (h.construction.value.import)("normalizeSymbol")?;
    let replaced = method(h, raw, "replace", || {
        Ok(vec![(h.regexp)("/", "g")?, (value.string)("_")])
    })?;
    (value.invoke)(normalize, (value.undefined)(), vec![replaced])
}

pub fn normalize_requested_symbol<V: Clone, E>(
    h: &ExchangeClassOps<V, E>,
    receiver: V,
    raw: V,
) -> Result<V, E> {
    let value = &h.construction.value.value;
    if !(value.truthy)(&raw) {
        return Ok((value.undefined)());
    }
    let upper = method(h, raw.clone(), "toUpperCase", || Ok(vec![]))?;
    let pieces = method(h, upper, "split", || Ok(vec![(value.string)(":")]))?;
    let mut stripped = method(h, pieces, "pop", || Ok(vec![]))?;
    if !(value.truthy)(&stripped) {
        stripped = method(h, raw, "toUpperCase", || Ok(vec![]))?;
    }
    for pattern in ["\\.P$", "_PERP$", "-PERP$", "PERP$"] {
        stripped = method(h, stripped, "replace", || {
            Ok(vec![(h.regexp)(pattern, "")?, (value.string)("")])
        })?;
    }
    method(h, receiver, "normalizeTransactionSymbol", || {
        Ok(vec![stripped])
    })
}
