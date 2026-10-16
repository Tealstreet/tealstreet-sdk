//! Private socket recovery and open bodies over supplied receiver operations.
use safe_cex_exchange::runtime::LogSeverity;

/// Fallible source property/call sites. Routing resolution includes the original
/// parent name/options lookup. Launching orders invokes its method without
/// awaiting; the host owns detached work. The full private socket stays separate.
pub trait PrivateRecoveryHost {
    type Error;
    fn disposed(&self) -> Result<bool, Self::Error>;
    fn date_now(&self) -> Result<f64, Self::Error>;
    fn last_recovery_at(&self) -> Result<f64, Self::Error>;
    fn recovery_cooldown_ms(&self) -> Result<f64, Self::Error>;
    fn set_last_recovery_at(&self, value: f64) -> Result<(), Self::Error>;
    fn resolved_private_fallback_truthy(&self) -> Result<bool, Self::Error>;
    fn uses_uta_routing(&self) -> Result<bool, Self::Error>;
    fn using_fallback(&self) -> Result<bool, Self::Error>;
    fn set_using_fallback(&self, value: bool) -> Result<(), Self::Error>;
    fn log(&self, message: &[u16], severity: LogSeverity) -> Result<(), Self::Error>;
    fn reconnect(&self) -> Result<(), Self::Error>;
    fn auth(&self) -> Result<(), Self::Error>;
    fn ping(&self) -> Result<(), Self::Error>;
    fn reconnecting_enabled(&self) -> bool;
    fn launch_orders_fetch(&self) -> Result<(), Self::Error>;
}

// ts: BitgetPrivateWebsocket.recoverMissingPrivateStream
pub fn recover_missing_private_stream<H: PrivateRecoveryHost>(
    host: &H,
    reason: &[u16],
) -> Result<(), H::Error> {
    if host.disposed()? {
        return Ok(());
    }
    let now = host.date_now()?;
    if now - host.last_recovery_at()? < host.recovery_cooldown_ms()? {
        return Ok(());
    }
    host.set_last_recovery_at(now)?;
    let has_fallback = host.resolved_private_fallback_truthy()?;
    let prefix = if !host.uses_uta_routing()? && !host.using_fallback()? && has_fallback {
        host.set_using_fallback(true)?;
        "Bitget private WS received no authenticated stream payloads after a write; switching to Tealstreet fallback route. Reason: "
    } else {
        "Bitget private WS received no authenticated stream payloads after a write; reconnecting current route. Reason: "
    };
    // JS strings are UTF-16 code units, including unpaired surrogates.
    let mut message: Vec<u16> = prefix.encode_utf16().collect();
    message.extend_from_slice(reason);
    host.log(&message, LogSeverity::Warning)?;
    host.reconnect()?;
    Ok(())
}

// ts: BitgetPrivateWebsocket.onOpen
pub fn on_open<H: PrivateRecoveryHost>(host: &H) -> Result<(), H::Error> {
    if !host.disposed()? {
        host.auth()?;
        host.ping()?;
        if host.reconnecting_enabled() {
            host.launch_orders_fetch()?;
        }
    }
    Ok(())
}
