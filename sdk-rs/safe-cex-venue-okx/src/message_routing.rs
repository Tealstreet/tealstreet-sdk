use std::{cell::Cell, rc::Rc};

// Capabilities evaluate complete live callee/field expressions at each call site.
pub fn private_message<J, E>(
    disposed: &Cell<bool>,
    data_is_pong: bool,
    mut effect: impl FnMut(&str, Option<&Rc<J>>) -> Result<(), E>,
    mut includes: impl FnMut(&str) -> Result<bool, E>,
    mut parse: impl FnMut() -> Result<Rc<J>, E>,
    mut predicate: impl FnMut(&Rc<J>, &str) -> Result<bool, E>,
) -> Result<(), E> {
    if disposed.get() {
        return Ok(());
    }
    effect("track", None)?;
    effect("fallback", None)?;
    effect("diagnostic", None)?;
    if includes("event\":\"subscribe\"")? {
        return Ok(());
    }
    if data_is_pong {
        return effect("pong", None);
    }
    if includes("login")? {
        // The maintained login catch includes parse, coercion and subscribe.
        match (|| {
            let json = parse()?;
            if predicate(&json, "optional-code-zero")? {
                effect("subscribe", None)?;
            }
            Ok::<(), E>(())
        })() {
            Ok(()) | Err(_) => return Ok(()),
        }
    }
    if includes("\"event\":\"error\"")? {
        // The maintained error catch also includes the emitter call.
        match (|| {
            let json = parse()?;
            if predicate(&json, "message-truthy")? {
                effect("emit-error", Some(&json))?;
            }
            Ok::<(), E>(())
        })() {
            Ok(()) | Err(_) => return Ok(()),
        }
    }
    for (needle, target) in [
        ("\"channel\":\"orders\"", "order"),
        ("\"channel\":\"positions\"", "position"),
    ] {
        if includes(needle)? {
            let json = parse()?;
            if predicate(&json, "data-truthy")? {
                effect(target, Some(&json))?;
            }
            return Ok(());
        }
    }
    if includes("\"channel\":\"account\"")? {
        let json = parse()?;
        if predicate(&json, "root-truthy")? {
            effect("account", Some(&json))?;
        }
    }
    Ok(())
}

pub fn business_message<J, V, T, E>(
    disposed: &Cell<bool>,
    data_is_pong: bool,
    mut effect: impl FnMut(&str, Option<&Rc<J>>, Option<&[u16]>, Option<&V>) -> Result<(), E>,
    parse: impl FnOnce() -> Result<Option<Rc<J>>, E>,
    mut predicate: impl FnMut(&Rc<J>, &str) -> Result<bool, E>,
    mut read_arg: impl FnMut(&Rc<J>, &str) -> Result<V, E>,
    is_truthy: impl Fn(&V) -> bool,
    has_order_channel: impl FnOnce(&V) -> Result<bool, E>,
    timeframe: impl FnOnce(&V) -> Result<Option<T>, E>,
    mut format_arg: impl FnMut(&V) -> Result<Vec<u16>, E>,
    format_timeframe: impl FnOnce(T) -> Result<Vec<u16>, E>,
) -> Result<(), E> {
    if disposed.get() {
        return Ok(());
    }
    effect("track", None, None, None)?;
    effect("fallback", None, None, None)?;
    effect("diagnostic", None, None, None)?;
    if data_is_pong {
        return effect("pong", None, None, None);
    }
    let Some(json) = parse()? else {
        return Ok(());
    };
    if predicate(&json, "event:subscribe")? {
        return Ok(());
    }
    if predicate(&json, "event:login")? {
        if predicate(&json, "code-zero")? {
            effect("subscribe", None, None, None)?;
        }
        return Ok(());
    }
    let channel = read_arg(&json, "channel")?;
    let id = read_arg(&json, "instId")?;
    let kind = read_arg(&json, "instType")?;
    if !is_truthy(&channel) {
        return Ok(());
    }
    if has_order_channel(&channel)? {
        effect(
            "stats",
            None,
            Some(&"orders".encode_utf16().collect::<Vec<_>>()),
            None,
        )?;
        return effect("order", Some(&json), None, None);
    }
    let interval = timeframe(&channel)?;
    if let Some(interval) = interval
        && is_truthy(&id)
    {
        let interval = format_timeframe(interval)?;
        let id = format_arg(&id)?;
        let key = [
            "candle.".encode_utf16().collect::<Vec<_>>(),
            interval,
            vec![46],
            id,
        ]
        .concat();
        effect("dispatch", Some(&json), Some(&key), None)?;
    }
    if is_truthy(&kind) && is_truthy(&id) {
        let channel = format_arg(&channel)?;
        let kind = format_arg(&kind)?;
        let id = format_arg(&id)?;
        let key = [channel, vec![46], kind, vec![46], id].concat();
        effect("dispatch", Some(&json), Some(&key), None)?;
    }
    if is_truthy(&id) {
        let channel = format_arg(&channel)?;
        let id = format_arg(&id)?;
        let key = [channel, vec![46], id].concat();
        effect("dispatch", Some(&json), Some(&key), None)?;
    }
    effect("dispatch-value", Some(&json), None, Some(&channel))?;
    Ok(())
}

pub fn dispatch_message_handler<H, J, C, E>(
    handler: &H,
    json: &Rc<J>,
    lookup: impl FnOnce(&H) -> Result<Option<Rc<C>>, E>,
    has_length: impl FnOnce(&Rc<C>) -> Result<bool, E>,
    stats: impl FnOnce(&H) -> Result<(), E>,
    for_of: impl FnOnce(Rc<C>, &Rc<J>) -> Result<(), E>,
) -> Result<(), E> {
    let Some(callbacks) = lookup(handler)? else {
        return Ok(());
    };
    if !has_length(&callbacks)? {
        return Ok(());
    }
    stats(handler)?;
    for_of(callbacks, json)
}
