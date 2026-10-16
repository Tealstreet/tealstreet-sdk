use safe_cex_types::OrderStatus;
use std::{cell::Cell, rc::Rc};

// Predicates and decomposition retain whole live source expressions.
pub fn futures_public_message<J, S, E>(
    disposed: &Cell<bool>,
    mut effect: impl FnMut(&str, Option<&Rc<J>>, Option<&[u16]>) -> Result<(), E>,
    parse: impl FnOnce() -> Result<Option<Rc<J>>, E>,
    mut predicate: impl FnMut(&Rc<J>, &str) -> Result<bool, E>,
    mut topic_component: impl FnMut(&Rc<J>, &str) -> Result<S, E>,
    candle_components: impl FnOnce(S) -> Result<(S, S), E>,
    mut internal_symbol: impl FnMut(&S) -> Result<Option<S>, E>,
    mut format_component: impl FnMut(&S) -> Result<Vec<u16>, E>,
) -> Result<(), E> {
    if disposed.get() {
        return Ok(());
    }
    effect("track", None, None)?;
    let Some(json) = parse()? else {
        return Ok(());
    };
    if predicate(&json, "type:pong")? {
        return effect("pong", None, None);
    }
    for (prefix, stats, target) in [
        ("/contractMarket/tickerV2", "ticker", "tickerV2"),
        ("/contractMarket/snapshot", "snapshot", "snapshot"),
        ("/contract/instrument", "instrument", "instrument"),
    ] {
        if predicate(&json, prefix)? {
            let stats: Vec<u16> = stats.encode_utf16().collect();
            effect("stats", None, Some(&stats))?;
            return effect(target, Some(&json), None);
        }
    }
    if predicate(&json, "/contractMarket/limitCandle")? {
        let argument = topic_component(&json, "candle")?;
        let (symbol, interval) = candle_components(argument)?;
        let internal = internal_symbol(&symbol)?;
        let symbol = format_component(internal.as_ref().unwrap_or(&symbol))?;
        let interval = format_component(&interval)?;
        let key = [
            "candle_".encode_utf16().collect::<Vec<_>>(),
            symbol,
            vec![95],
            interval,
        ]
        .concat();
        effect("stats", None, Some(&key))?;
        return effect("for-of", Some(&json), Some(&key));
    }
    if predicate(&json, "/contractMarket/level2")? {
        let symbol = topic_component(&json, "book")?;
        let internal = internal_symbol(&symbol)?;
        let symbol = format_component(internal.as_ref().unwrap_or(&symbol))?;
        let key = ["orderBook_".encode_utf16().collect::<Vec<_>>(), symbol].concat();
        effect("stats", None, Some(&key))?;
        effect("for-of", Some(&json), Some(&key))?;
    }
    Ok(())
}

pub fn futures_private_message<J, E>(
    disposed: &Cell<bool>,
    mut effect: impl FnMut(&str, Option<&Rc<J>>, Option<&[u16]>) -> Result<(), E>,
    parse: impl FnOnce() -> Result<Option<Rc<J>>, E>,
    mut predicate: impl FnMut(&Rc<J>, &str) -> Result<bool, E>,
) -> Result<(), E> {
    if disposed.get() {
        return Ok(());
    }
    effect("track", None, None)?;
    effect("diagnostic", None, None)?;
    let Some(json) = parse()? else {
        return Ok(());
    };
    if predicate(&json, "type:pong")? {
        return effect("pong", None, None);
    }
    if predicate(&json, "type:welcome")? {
        return effect("subscribe", None, None);
    }
    for (prefix, stats, target) in [
        ("/contractMarket/tradeOrders", "order", "order"),
        ("/contractMarket/advancedOrders", "orderAlgo", "algo"),
    ] {
        if predicate(&json, prefix)? {
            let stats: Vec<u16> = stats.encode_utf16().collect();
            effect("stats", None, Some(&stats))?;
            return effect(target, Some(&json), None);
        }
    }
    if predicate(&json, "optional:/contract/position")?
        || predicate(&json, "equal:/contractAccount/wallet")?
        || predicate(&json, "equal:/contract/marginMode")?
        || predicate(&json, "equal:/contract/crossLeverage")?
    {
        effect(
            "stats",
            None,
            Some(&"account".encode_utf16().collect::<Vec<_>>()),
        )?;
        effect("refresh-snapshot", None, None)?;
    }
    Ok(())
}

pub fn spot_private_message<J, E>(
    disposed: &Cell<bool>,
    mut effect: impl FnMut(&str, Option<&Rc<J>>, Option<&[u16]>) -> Result<(), E>,
    parse: impl FnOnce() -> Result<Option<Rc<J>>, E>,
    mut predicate: impl FnMut(&Rc<J>, &str) -> Result<bool, E>,
) -> Result<(), E> {
    if disposed.get() {
        return Ok(());
    }
    effect("track", None, None)?;
    effect("diagnostic", None, None)?;
    let Some(json) = parse()? else {
        return Ok(());
    };
    if predicate(&json, "type:pong")? {
        return effect("pong-interval", None, None);
    }
    if predicate(&json, "optional:/spotMarket/tradeOrdersV2")? {
        effect(
            "stats",
            None,
            Some(&"order".encode_utf16().collect::<Vec<_>>()),
        )?;
        return effect("order", Some(&json), None);
    }
    if predicate(&json, "optional:/account/balance")? {
        effect(
            "stats",
            None,
            Some(&"balance".encode_utf16().collect::<Vec<_>>()),
        )?;
        effect("refresh-balance", None, None)?;
    }
    Ok(())
}

pub fn spot_public_message<J, E>(
    disposed: &Cell<bool>,
    mut effect: impl FnMut(&str, Option<&Rc<J>>, Option<&[u16]>) -> Result<(), E>,
    parse: impl FnOnce() -> Result<Option<Rc<J>>, E>,
    mut predicate: impl FnMut(&Rc<J>, &str) -> Result<bool, E>,
    mut read_text: impl FnMut(&Rc<J>, &str) -> Result<Vec<u16>, E>,
) -> Result<(), E> {
    if disposed.get() {
        return Ok(());
    }
    effect("track", None, None)?;
    let Some(json) = parse()? else {
        return Ok(());
    };
    if predicate(&json, "type:pong")? {
        return effect("pong-interval", None, None);
    }
    if predicate(&json, "optional:/market/ticker")? {
        effect(
            "stats",
            None,
            Some(&"ticker".encode_utf16().collect::<Vec<_>>()),
        )?;
        return effect("ticker", Some(&json), None);
    }
    if predicate(&json, "optional:/market/candles")? {
        let key = read_text(&json, "candle-helper")?;
        effect("stats", None, Some(&key))?;
        return effect("for-each", Some(&json), Some(&key));
    }
    if predicate(&json, "optional:/spotMarket/level2Depth50")? {
        let text = read_text(&json, "topic-template")?;
        let symbol = text.split(|u| *u == 58).nth(1);
        let key = [
            "orderBook_".encode_utf16().collect::<Vec<_>>(),
            symbol.map_or_else(|| "undefined".encode_utf16().collect(), |s| s.to_vec()),
        ]
        .concat();
        effect("stats", None, Some(&key))?;
        effect("for-each", Some(&json), Some(&key))?;
    }
    Ok(())
}

pub fn algo_order_message<J, R, O, P, E>(
    message: &J,
    has_data: impl FnOnce(&J) -> Result<bool, E>,
    read_data: impl FnOnce(&J) -> Result<Rc<R>, E>,
    map_order: impl FnOnce(&Rc<R>) -> Result<O, E>,
    first_order: impl FnOnce(&O) -> Result<Option<Rc<P>>, E>,
    read_event: impl FnOnce(&Rc<R>) -> Result<Option<Vec<u16>>, E>,
    mut update: impl FnMut(&Rc<P>, Option<OrderStatus>) -> Result<(), E>,
) -> Result<(), E> {
    if !has_data(message)? {
        return Ok(());
    }
    let raw = read_data(message)?;
    let orders = map_order(&raw)?;
    let parent = first_order(&orders)?;
    let event = read_event(&raw)?;
    let Some(parent) = parent else {
        return Ok(());
    };
    if event.as_deref() == Some(&"cancel".encode_utf16().collect::<Vec<_>>()) {
        update(&parent, Some(OrderStatus::Canceled))?;
    }
    if ["triggered", "active", "open"]
        .iter()
        .any(|s| event.as_deref() == Some(&s.encode_utf16().collect::<Vec<_>>()))
    {
        update(&parent, None)?;
    }
    Ok(())
}
