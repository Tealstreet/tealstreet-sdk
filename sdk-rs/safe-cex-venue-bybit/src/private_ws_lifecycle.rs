//! Inverse/spot private socket method bodies. Host callbacks retain receiver
//! dependencies; constructing sockets and signing authentication remain separate.
use safe_cex_exchange::runtime::ExchangeError;
use std::{future::Future, rc::Rc};

pub fn on_open<E>(start_reconnect_sequence: impl FnOnce() -> Result<(), E>) -> Result<(), E> {
    // Start the operation without awaiting its completion, as the source does.
    start_reconnect_sequence()
}

pub fn ping<E>(
    is_disposed: impl FnOnce() -> bool,
    performance_now: impl FnOnce() -> Result<f64, E>,
    set_ping_at: impl FnOnce(f64) -> Result<(), E>,
    send_with_log: impl FnOnce(&str) -> Result<(), E>,
) -> Result<(), E> {
    if !is_disposed() {
        set_ping_at(performance_now()?)?;
        send_with_log(r#"{"op":"ping"}"#)?;
    }
    Ok(())
}

pub fn inverse_subscribe<E>(send_with_log: impl FnOnce(&str) -> Result<(), E>) -> Result<(), E> {
    send_with_log(r#"{"op":"subscribe","args":["order","position"]}"#)
}

pub fn spot_subscribe<E>(send_with_log: impl FnOnce(&str) -> Result<(), E>) -> Result<(), E> {
    send_with_log(r#"{"op":"subscribe","args":["order"]}"#)
}

#[allow(clippy::too_many_arguments)]
pub async fn send_reconnect_sequence<
    Sync: Future<Output = Result<(), Rc<ExchangeError>>>,
    Sleep: Future<Output = Result<(), Rc<ExchangeError>>>,
    Auth: Future<Output = Result<bool, Rc<ExchangeError>>>,
>(
    is_disposed: impl FnOnce() -> bool,
    sync_clock: impl FnOnce() -> Sync,
    sleep: impl FnOnce(u64) -> Sleep,
    auth: impl FnOnce() -> Auth,
    emit_and_log_error: impl FnOnce(Rc<ExchangeError>) -> Result<(), Rc<ExchangeError>>,
    disregard_silence: impl FnOnce() -> Result<(), Rc<ExchangeError>>,
    reconnect: impl FnOnce() -> Result<(), Rc<ExchangeError>>,
    subscribe: impl FnOnce() -> Result<(), Rc<ExchangeError>>,
    ping: impl FnOnce() -> Result<(), Rc<ExchangeError>>,
    reconnecting_enabled: impl FnOnce() -> bool,
    start_private_snapshot: impl FnOnce() -> Result<(), Rc<ExchangeError>>,
    _sync_time: bool,
) -> Result<(), Rc<ExchangeError>> {
    if !is_disposed() {
        sync_clock().await?;
        sleep(25).await?;
        let authed = match auth().await {
            Ok(value) => value,
            Err(error) => {
                emit_and_log_error(error)?;
                false
            }
        };
        if !authed {
            disregard_silence()?;
            reconnect()?;
            return Ok(());
        }
        subscribe()?;
        ping()?;
        if reconnecting_enabled() {
            start_private_snapshot()?;
        }
    }
    Ok(())
}
