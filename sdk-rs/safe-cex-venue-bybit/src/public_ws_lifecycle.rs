//! Inverse/spot public socket lifecycle bodies; concrete socket wiring is separate.
use serde_json::{Map, Value};
use std::future::Future;

#[allow(clippy::too_many_arguments)]
pub fn connect_and_subscribe<E, O, S, H>(
    category: &str,
    is_disposed: impl FnOnce() -> Result<bool, E>,
    route: impl FnOnce() -> Result<(bool, Option<String>, Option<String>), E>,
    direct_url: impl FnOnce() -> Result<String, E>,
    resolve_url: impl FnOnce(&str, Option<&str>) -> Result<String, E>,
    proxy_info: impl FnOnce(&str, &str) -> Result<(), E>,
    is_demo: impl FnOnce() -> Result<bool, E>,
    options: impl FnOnce() -> Result<Option<O>, E>,
    unique_id: impl FnOnce() -> Result<String, E>,
    create: impl FnOnce(String, Option<Vec<String>>, Option<O>) -> Result<S, E>,
    set_ws: impl FnOnce(S) -> Result<(), E>,
    mut listener_receiver: impl FnMut() -> Result<S, E>,
    mut handler: impl FnMut(&str) -> Result<H, E>,
    mut listen: impl FnMut(S, &str, H) -> Result<(), E>,
) -> Result<(), E> {
    if !is_disposed()? {
        let (tunnelled, host, fallback) = route()?;
        let direct = direct_url()?;
        let primary = host
            .as_deref()
            .filter(|host| tunnelled && !host.is_empty())
            .unwrap_or(&direct);
        let base = resolve_url(primary, fallback.as_deref())?;
        if Some(base.as_str()) == fallback.as_deref() {
            // The maintained inverse/spot bodies deliberately swallow proxy notice errors.
            let _ = proxy_info(
                "grafana-info",
                &format!("Using tealstreet bybit public {category} proxy"),
            );
        }
        let options = if is_demo()? { None } else { options()? };
        let url = format!("{base}?requestId={}", unique_id()?);
        let ws = create(url, None, options)?;
        set_ws(ws)?;
        for (event, field) in [
            ("open", "onOpen"),
            ("message", "onMessage"),
            ("close", "onClose"),
        ] {
            let receiver = listener_receiver()?;
            let callback = handler(field)?;
            listen(receiver, event, callback)?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn on_open<E, H>(
    is_disposed: impl FnOnce() -> Result<bool, E>,
    sync_topics: impl FnOnce() -> Result<(), E>,
    start_subscribe: impl FnOnce() -> Result<(), E>,
    ping: impl FnOnce() -> Result<(), E>,
    mut markets_handler: impl FnMut() -> Result<H, E>,
    off: impl FnOnce(&str, H) -> Result<(), E>,
    on: impl FnOnce(&str, H) -> Result<(), E>,
    reconnecting_enabled: impl FnOnce() -> bool,
    emit_reconnected: impl FnOnce() -> Result<(), E>,
) -> Result<(), E> {
    if !is_disposed()? {
        sync_topics()?;
        // The starter schedules the subscription; completion/rejection is not awaited here.
        start_subscribe()?;
        ping()?;
        off("markets:changed", markets_handler()?)?;
        on("markets:changed", markets_handler()?)?;
        if reconnecting_enabled() {
            emit_reconnected()?;
        }
    }
    Ok(())
}

pub fn dispose<E, H>(
    active_handler: impl FnOnce() -> Result<H, E>,
    off: impl FnOnce(&str, H) -> Result<(), E>,
    base_dispose: impl FnOnce() -> Result<(), E>,
) -> Result<(), E> {
    off("activeTickerSymbols:changed", active_handler()?)?;
    base_dispose()?;
    Ok(())
}

pub fn ping<E>(
    is_disposed: impl FnOnce() -> Result<bool, E>,
    now: impl FnOnce() -> Result<f64, E>,
    set_ping_at: impl FnOnce(f64) -> Result<(), E>,
    send: impl FnOnce(&str) -> Result<(), E>,
) -> Result<(), E> {
    if !is_disposed()? {
        set_ping_at(now()?)?;
        send(r#"{"op":"ping"}"#)?;
    }
    Ok(())
}

pub fn subscribe<E, F: Future<Output = Result<(), E>>>(
    topics: impl FnOnce() -> Result<Map<String, Value>, E>,
    mut send: impl FnMut(&str) -> Result<(), E>,
    mut sleep: impl FnMut(u64) -> F,
) -> impl Future<Output = Result<(), E>> {
    // JS async functions execute through the first await at invocation time.
    let prepared = (|| {
        let topics = topics()?;
        let topics: Vec<Value> = crate::api_request::ordered_entries(&topics)
            .into_iter()
            .flat_map(|(_, value)| match value {
                Value::Array(values) => values.clone(),
                value => vec![value.clone()],
            })
            .collect();
        let frames: Vec<String> = topics
            .chunks(10)
            .map(|topics| {
                format!(
                    r#"{{"op":"subscribe","args":{}}}"#,
                    serde_json::to_string(topics).unwrap()
                )
            })
            .collect();
        let Some(first) = frames.first() else {
            return Ok(None);
        };
        send(first)?;
        Ok(Some((frames, sleep(10))))
    })();
    async move {
        if let Some((frames, first)) = prepared? {
            first.await?;
            for frame in frames.into_iter().skip(1) {
                send(&frame)?;
                sleep(10).await?;
            }
        }
        Ok(())
    }
}
