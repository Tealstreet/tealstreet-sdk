//! Inverse/spot public ticker subscription method bodies with explicit receivers.
use serde_json::Value;
use std::{cell::RefCell, collections::HashSet, future::Future, rc::Rc};

/// Array input is returned by identity; scalar input gets a fresh mutable array.
pub fn get_current_ticker_topics<E>(
    current: impl FnOnce() -> Result<Option<Rc<RefCell<Value>>>, E>,
) -> Result<Rc<RefCell<Value>>, E> {
    let current = current()?;
    match current {
        Some(current) if current.borrow().is_array() => Ok(current),
        Some(current) if safe_cex_num::js_truthy(Some(&current.borrow())) => {
            let value = current.borrow().clone();
            Ok(Rc::new(RefCell::new(Value::Array(vec![value]))))
        }
        _ => Ok(Rc::new(RefCell::new(Value::Array(vec![])))),
    }
}

pub fn get_desired_ticker_topics<E>(
    symbols: impl FnOnce() -> Result<Vec<String>, E>,
) -> Result<Vec<String>, E> {
    Ok(symbols()?
        .into_iter()
        .map(|symbol| format!("tickers.{symbol}"))
        .collect())
}

/// The returned array and setter argument are the same mutable native value.
pub fn sync_ticker_topics<E>(
    current: impl FnOnce() -> Result<Vec<String>, E>,
    desired: impl FnOnce() -> Result<Vec<String>, E>,
    set: impl FnOnce(Rc<RefCell<Value>>) -> Result<(), E>,
) -> Result<Rc<RefCell<Value>>, E> {
    let current = current()?;
    let desired = desired()?;
    let mut seen = HashSet::new();
    let merged = current
        .into_iter()
        .chain(desired)
        .filter(|topic| seen.insert(topic.clone()))
        .map(Value::String)
        .collect();
    let merged = Rc::new(RefCell::new(Value::Array(merged)));
    set(Rc::clone(&merged))?;
    Ok(merged)
}

fn subscription(topics: &[String]) -> String {
    // Keep source property order while serde escapes the string-only topic array.
    format!(
        r#"{{"op":"subscribe","args":{}}}"#,
        serde_json::to_string(topics).unwrap()
    )
}

pub fn handle_active_ticker_symbols_changed<E>(
    is_disposed: impl FnOnce() -> bool,
    current: impl FnOnce() -> Result<Vec<String>, E>,
    sync: impl FnOnce() -> Result<Vec<String>, E>,
    is_connected: impl FnOnce() -> bool,
    mut send: impl FnMut(&str) -> Result<(), E>,
) -> Result<(), E> {
    if is_disposed() {
        return Ok(());
    }
    let previous: HashSet<String> = current()?.into_iter().collect();
    let next = sync()?;
    let subscribe: Vec<String> = next
        .into_iter()
        .filter(|topic| !previous.contains(topic))
        .collect();
    if is_connected() && !subscribe.is_empty() {
        for topics in subscribe.chunks(10) {
            send(&subscription(topics))?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn on_markets_changed<E, F: Future<Output = Result<(), E>>>(
    added_symbols: Vec<String>,
    has_ws: impl FnOnce() -> bool,
    is_disposed: impl FnOnce() -> bool,
    current: impl FnOnce() -> Result<Vec<String>, E>,
    set: impl FnOnce(Rc<RefCell<Value>>) -> Result<(), E>,
    mut send: impl FnMut(&str) -> Result<(), E>,
    mut sleep: impl FnMut(u64) -> F,
) -> impl Future<Output = Result<(), E>> {
    let prepared = (|| {
        if added_symbols.is_empty() || !has_ws() || is_disposed() {
            return Ok(None);
        }
        let topics: Vec<String> = added_symbols
            .into_iter()
            .map(|symbol| format!("tickers.{symbol}"))
            .collect();
        let merged = current()?
            .into_iter()
            .chain(topics.iter().cloned())
            .map(Value::String)
            .collect();
        set(Rc::new(RefCell::new(Value::Array(merged))))?;
        let chunks: Vec<Vec<String>> = topics.chunks(10).map(<[String]>::to_vec).collect();
        send(&subscription(&chunks[0]))?;
        Ok(Some((chunks, sleep(5))))
    })();
    async move {
        if let Some((chunks, first)) = prepared? {
            first.await?;
            for topics in chunks.into_iter().skip(1) {
                send(&subscription(&topics))?;
                sleep(5).await?;
            }
        }
        Ok(())
    }
}
