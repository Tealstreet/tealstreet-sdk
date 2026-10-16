use std::collections::HashSet;

/// Caller-supplied JavaScript value operations; property reads may throw or change values.
pub struct WeexArrayRuntime<H, V, E> {
    pub is_array: fn(&V) -> Result<bool, E>,
    pub is_nullish: fn(&V) -> bool,
    pub undefined: fn() -> V,
    pub get_property: fn(&mut H, &V, &[u16]) -> Result<V, E>,
    pub empty_array: fn(&mut H) -> V,
}

fn optional_property<H, V, E>(
    host: &mut H,
    value: &V,
    key: &[u16],
    runtime: &WeexArrayRuntime<H, V, E>,
) -> Result<V, E> {
    if (runtime.is_nullish)(value) {
        Ok((runtime.undefined)())
    } else {
        (runtime.get_property)(host, value, key)
    }
}

pub fn extract_array<H, V, E>(
    host: &mut H,
    payload: V,
    keys: &[Vec<u16>],
    runtime: &WeexArrayRuntime<H, V, E>,
) -> Result<V, E> {
    let data = [
        u16::from(b'd'),
        u16::from(b'a'),
        u16::from(b't'),
        u16::from(b'a'),
    ];
    if (runtime.is_array)(&payload)? {
        return Ok(payload);
    }
    if (runtime.is_array)(&optional_property(host, &payload, &data, runtime)?)? {
        return (runtime.get_property)(host, &payload, &data);
    }
    for key in keys {
        if (runtime.is_array)(&optional_property(host, &payload, key, runtime)?)? {
            return (runtime.get_property)(host, &payload, key);
        }
        let nested = optional_property(host, &payload, &data, runtime)?;
        if (runtime.is_array)(&optional_property(host, &nested, key, runtime)?)? {
            let nested = (runtime.get_property)(host, &payload, &data)?;
            return (runtime.get_property)(host, &nested, key);
        }
    }
    Ok((runtime.empty_array)(host))
}

/// Callbacks evaluate source truthiness of process.versions?.node and process.isBun lazily.
pub fn is_node_or_bun_runtime<E>(
    process_is_defined: bool,
    node_truthy: impl FnOnce() -> Result<bool, E>,
    bun_truthy: impl FnOnce() -> Result<bool, E>,
) -> Result<bool, E> {
    if !process_is_defined {
        return Ok(false);
    }
    if node_truthy()? {
        return Ok(true);
    }
    bun_truthy()
}

/// Local topic selection over market symbols, active symbols and the existing topic count.
pub fn get_desired_ticker_topics(
    available_symbols: &[Vec<u16>],
    active_symbols: &[Vec<u16>],
    topic_count: usize,
) -> Vec<Vec<u16>> {
    let active_set: HashSet<&[u16]> = active_symbols.iter().map(Vec::as_slice).collect();
    let desired_symbols = available_symbols
        .iter()
        .filter(|symbol| active_symbols.is_empty() || active_set.contains(symbol.as_slice()));
    let capacity = 90.min(100_usize.saturating_sub(topic_count));
    desired_symbols
        .take(capacity)
        .map(|symbol| {
            let mut topic = symbol.clone();
            topic.extend("@ticker".encode_utf16());
            topic
        })
        .collect()
}
