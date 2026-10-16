//! Stateless bodies from BitruePrivateWebsocket; transport and callers remain separate.

/// Lossless classes for the source's strict comparisons. All non-number/string
/// JavaScript values compare false without invoking any coercion hook.
#[derive(Clone, Copy, Debug)]
pub enum SuccessCode<'a> {
    Number(f64),
    String(&'a [u16]),
    Other,
}

pub fn is_successful_code(code: SuccessCode<'_>) -> bool {
    match code {
        SuccessCode::Number(code) => code == 0.0 || code == 200.0,
        SuccessCode::String(code) => code == [0x30] || code == [0x32, 0x30, 0x30],
        SuccessCode::Other => false,
    }
}

// Source: noop because server/client ping directions are reversed.
pub fn ping() {}

/// Query the disposal guard once, then call subscribe and ping in source order.
/// Dependency boundaries include property resolution and native truthiness.
pub fn on_open<R, P, E>(
    is_disposed: impl FnOnce() -> Result<bool, E>,
    subscribe: impl FnOnce() -> Result<R, E>,
    ping: impl FnOnce() -> Result<P, E>,
) -> Result<(), E> {
    if !is_disposed()? {
        subscribe()?;
        ping()?;
    }
    Ok(())
}
