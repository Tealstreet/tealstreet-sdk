//! Ping field bodies; socket construction and host operations are supplied.
use std::cell::Cell;

// ts: BitgetPublicWebsocket.ping
pub fn ping_public<E>(
    is_disposed: bool,
    ping_at: &Cell<f64>,
    performance_now: impl FnOnce() -> f64,
    send: impl FnOnce(&str) -> Result<(), E>,
) -> Result<(), E> {
    if !is_disposed {
        ping_at.set(performance_now());
        send("ping")?;
    }
    Ok(())
}

// ts: BitgetSecondaryPublicWebsocket.ping
pub fn ping_secondary_public<E>(
    is_disposed: bool,
    ping_at: &Cell<f64>,
    performance_now: impl FnOnce() -> f64,
    send: impl FnOnce(&str) -> Result<(), E>,
) -> Result<(), E> {
    if !is_disposed {
        ping_at.set(performance_now());
        send("ping")?;
    }
    Ok(())
}

// ts: BitgetPrivateWebsocket.ping
pub fn ping_private<E>(
    is_disposed: bool,
    ping_at: &Cell<f64>,
    performance_now: impl FnOnce() -> f64,
    send_with_log: impl FnOnce(&str) -> Result<(), E>,
) -> Result<(), E> {
    if !is_disposed {
        ping_at.set(performance_now());
        send_with_log("ping")?;
    }
    Ok(())
}
