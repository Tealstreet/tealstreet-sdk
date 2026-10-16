//! Futures heartbeat; clocks and send/log operations belong to the host.
use safe_cex_num::JsJsonValue;

pub fn ping<E>(
    disposed: bool,
    performance_now: impl FnOnce() -> Result<f64, E>,
    set_ping_at: impl FnOnce(f64) -> Result<(), E>,
    clock_now: impl FnOnce() -> Result<f64, E>,
    send_with_log: impl FnOnce(JsJsonValue) -> Result<(), E>,
) -> Result<(), E> {
    if !disposed {
        set_ping_at(performance_now()?)?;
        send_with_log(JsJsonValue::Object(vec![(
            "ping".into(),
            JsJsonValue::Number(clock_now()?),
        )]))?;
    }
    Ok(())
}

pub fn on_open<T, Topic, E>(
    mut is_disposed: impl FnMut() -> Result<bool, E>,
    reset_book: impl FnOnce() -> Result<(), E>,
    mut send_frame: impl FnMut(JsJsonValue) -> Result<(), E>,
    topics: impl FnOnce() -> Result<Vec<(Topic, f64)>, E>,
    mut send_topic: impl FnMut(Topic) -> Result<(), E>,
    (mut timer, active, clear): (
        impl FnMut() -> Result<Option<T>, E>,
        impl FnOnce(&Option<T>) -> bool,
        impl FnOnce(Option<T>) -> Result<(), E>,
    ),
    (ping, schedule, set): (
        impl FnOnce() -> Result<(), E>,
        impl FnOnce(&'static str, f64) -> Result<T, E>,
        impl FnOnce(T) -> Result<(), E>,
    ),
) -> Result<(), E> {
    if !is_disposed()? {
        reset_book()?;
        for channel in ["system_notice", "reconnected", "onconnected"] {
            send_frame(JsJsonValue::Object(vec![
                ("event".into(), JsJsonValue::String("sub".into())),
                (
                    "params".into(),
                    JsJsonValue::Object(vec![(
                        "channel".into(),
                        JsJsonValue::String(channel.into()),
                    )]),
                ),
            ]))?;
        }
        for (topic, count) in topics()? {
            if count > 0.0 {
                send_topic(topic)?;
            }
        }
        if active(&timer()?) {
            clear(timer()?)?;
        }
        ping_loop(is_disposed, ping, schedule, set)?;
    }
    Ok(())
}

pub async fn on_message<D, P, L, H, E, F: std::future::Future<Output = Result<D, E>>>(
    data: D,
    mut is_disposed: impl FnMut() -> Result<bool, E>,
    track: impl FnOnce() -> Result<(), E>,
    (count, set_count): (
        impl FnOnce() -> Result<f64, E>,
        impl FnOnce(f64) -> Result<(), E>,
    ),
    is_blob: impl FnOnce(&D) -> bool,
    read_blob: impl FnOnce(&D) -> F,
    report_blob: impl FnOnce(&'static str, E) -> Result<(), E>,
    parse: impl FnOnce(D) -> Result<P, E>,
    (has_handlers, handlers, mut next, mut call): (
        impl FnOnce(&P) -> Result<bool, E>,
        impl FnOnce(&P) -> Result<(L, usize), E>,
        impl FnMut(&L, usize) -> Result<Option<H>, E>,
        impl FnMut(H, &P) -> Result<(), E>,
    ),
) -> Result<(), E> {
    if !is_disposed()? {
        track()?;
        set_count(count()? + 1.0)?;
        let mut decoded = data;
        if is_blob(&decoded) {
            match read_blob(&decoded).await {
                Ok(text) => decoded = text,
                Err(error) => report_blob("bitunix: error reading blob", error)?,
            }
            if is_disposed()? {
                return Ok(());
            }
        }
        let parsed = parse(decoded)?;
        if has_handlers(&parsed)? {
            let (list, length) = handlers(&parsed)?;
            for index in 0..length {
                if let Some(handler) = next(&list, index)? {
                    call(handler, &parsed)?;
                }
            }
        }
    }
    Ok(())
}

pub fn ping_loop<T, E>(
    is_disposed: impl FnOnce() -> Result<bool, E>,
    ping: impl FnOnce() -> Result<(), E>,
    schedule: impl FnOnce(&'static str, f64) -> Result<T, E>,
    set: impl FnOnce(T) -> Result<(), E>,
) -> Result<(), E> {
    if !is_disposed()? {
        ping()?;
        set(schedule("doPingLoop", 15_000.0)?)?;
    }
    Ok(())
}
