//! Public ticker fallback/startup bodies at required live owner/call/timer hosts.
//! Typed flags, Number timer delays/clocks and UTF16 strings retain source values;
//! class construction, actual emitter/transport/REST/timer bindings remain pending.
use crate::{sdk_account_api::AccountValue, transaction_signing::SigningResult};
use std::rc::Rc;
const WATCHDOG: f64 = 8_000.0;
const INTERVAL: f64 = 3_000.0;
const DEFAULT_REASON: &str = "public websocket connected but no messages; polling tickers via REST";
const UNHYDRATED_REASON: &str =
    "public websocket is active but ticker prices are not hydrated yet; polling tickers via REST";
/// Capture actual callee AND receiver (owner, current parent/emitter, or global
/// timer), including noncallable values whose Call fails only after arguments.
/// Return values/throws retain native identity. Discarded fetch returns are not
/// awaited/adopted; host still invokes the actual callee eagerly.
pub trait ReadinessCall {
    fn call(&self, args: &[AccountValue]) -> SigningResult<AccountValue>;
}
pub type ReadinessCallback = Rc<dyn Fn() -> SigningResult<()>>;
pub trait ReadinessTimerCall {
    fn call(&self, callback: ReadinessCallback, delay: f64) -> SigningResult<AccountValue>;
}
pub trait PublicReadinessHost {
    fn flag(&self, name: &str) -> SigningResult<bool>;
    fn set_flag(&self, name: &str, value: bool) -> SigningResult<()>;
    fn value(&self, name: &str) -> SigningResult<AccountValue>;
    fn set_value(&self, name: &str, value: AccountValue) -> SigningResult<()>;
    fn capture(&self, name: &str) -> SigningResult<Rc<dyn ReadinessCall>>;
    fn capture_timer(&self, name: &str) -> SigningResult<Rc<dyn ReadinessTimerCall>>;
    /// Actual performance.now and virtualClock.getCurrentTimeMs receiver-bound
    /// lookup/calls, preserving failures and distinct clock domains.
    fn performance_now(&self) -> SigningResult<f64>;
    fn virtual_now(&self) -> SigningResult<f64>;
    /// Fresh ordinary source object literal { type: 'ping', timestamp }, with
    /// own data properties in source order and original intrinsic prototype.
    /// No setter assignment, JSON surrogate or alias reuse. Resource-exhaustion
    /// observability of allocation before timestamp read is outside this adapter.
    fn ping_payload(&self, timestamp: f64) -> SigningResult<AccountValue>;
}
fn call(host: &dyn PublicReadinessHost, name: &str, args: &[AccountValue]) -> SigningResult<()> {
    host.capture(name)?.call(args)?;
    Ok(())
}
fn decision(host: &dyn PublicReadinessHost, name: &str) -> SigningResult<bool> {
    Ok(host.capture(name)?.call(&[])?.truthy())
}
// ts: LighterPublicWebsocket#syncTickerRestFallbackToPriceHydration
pub fn sync_ticker_rest_fallback_to_price_hydration(
    host: &dyn PublicReadinessHost,
    reason: Option<&[u16]>,
) -> SigningResult<()> {
    if decision(host, "hasHydratedTickerPrice")? {
        call(host, "clearTickerRestFallbackWatchdog", &[])?;
        call(host, "stopTickerRestFallback", &[])?;
        return Ok(());
    }
    if let Some(reason) = reason.filter(|r| !r.is_empty()) {
        call(
            host,
            "startTickerRestFallback",
            &[AccountValue::String(reason.to_vec())],
        )?;
    }
    Ok(())
}
// ts: LighterPublicWebsocket#primeTickerRestFallback
pub fn prime_ticker_rest_fallback(host: &dyn PublicReadinessHost) -> SigningResult<()> {
    if host.flag("connectedViaProxyRoute")?
        && host.flag("hasPrimedTickerRestFallbackForConnection")?
    {
        return Ok(());
    }
    host.set_flag("hasPrimedTickerRestFallbackForConnection", true)?;
    call(host, "fetchAndApplyTickerFallback", &[])
}
// ts: LighterPublicWebsocket#clearTickerRestFallbackWatchdog
pub fn clear_ticker_rest_fallback_watchdog(host: &dyn PublicReadinessHost) -> SigningResult<()> {
    if host.value("tickerRestFallbackWatchdog")?.truthy() {
        let clear = host.capture("clearTimeout")?;
        clear.call(&[host.value("tickerRestFallbackWatchdog")?])?;
        host.set_value("tickerRestFallbackWatchdog", AccountValue::Undefined)?;
    }
    Ok(())
}
// ts: LighterPublicWebsocket#armTickerRestFallbackWatchdog
pub fn arm_ticker_rest_fallback_watchdog(
    host: Rc<dyn PublicReadinessHost>,
    delay: Option<f64>,
    reason: Option<&[u16]>,
) -> SigningResult<()> {
    let delay = delay.unwrap_or(WATCHDOG);
    let reason = reason
        .map(<[u16]>::to_vec)
        .unwrap_or_else(|| DEFAULT_REASON.encode_utf16().collect());
    if decision(host.as_ref(), "isTickerRestFallbackCoolingDown")? {
        return Ok(());
    }
    call(host.as_ref(), "clearTickerRestFallbackWatchdog", &[])?;
    let timer = host.capture_timer("setTimeout")?;
    let owner = host.clone();
    let callback: ReadinessCallback = Rc::new(move || {
        owner.set_value("tickerRestFallbackWatchdog", AccountValue::Undefined)?;
        if owner.flag("isDisposed")? {
            return Ok(());
        }
        if !owner.flag("hasReceivedMessageSinceConnect")?
            || !decision(owner.as_ref(), "hasHydratedTickerPrice")?
        {
            // Capture the actual start callee before the second live flag read.
            let start = owner.capture("startTickerRestFallback")?;
            let text = if owner.flag("hasReceivedMessageSinceConnect")? {
                UNHYDRATED_REASON.encode_utf16().collect()
            } else {
                reason.clone()
            };
            start.call(&[AccountValue::String(text)])?;
        }
        Ok(())
    });
    let id = timer.call(callback, delay)?;
    host.set_value("tickerRestFallbackWatchdog", id)
}
// ts: LighterPublicWebsocket#stopTickerRestFallback
pub fn stop_ticker_rest_fallback(
    host: &dyn PublicReadinessHost,
    log_recovered: Option<bool>,
) -> SigningResult<()> {
    let log_recovered = log_recovered.unwrap_or(true);
    if host.value("tickerRestFallbackInterval")?.truthy() {
        let clear = host.capture("clearInterval")?;
        clear.call(&[host.value("tickerRestFallbackInterval")?])?;
        host.set_value("tickerRestFallbackInterval", AccountValue::Undefined)?;
        if log_recovered {
            call(
                host,
                "parent.log",
                &[
                    AccountValue::String(
                        "[Lighter] Ticker prices hydrated; disabling REST ticker fallback."
                            .encode_utf16()
                            .collect(),
                    ),
                    AccountValue::String("info".encode_utf16().collect()),
                ],
            )?;
        }
    }
    Ok(())
}
// ts: LighterPublicWebsocket#startTickerRestFallback
pub fn start_ticker_rest_fallback(
    host: Rc<dyn PublicReadinessHost>,
    reason: &[u16],
) -> SigningResult<()> {
    if decision(host.as_ref(), "isTickerRestFallbackCoolingDown")? {
        return Ok(());
    }
    if host.value("tickerRestFallbackInterval")?.truthy() {
        return Ok(());
    }
    if !host.flag("hasWarnedTickerRestFallback")? {
        host.set_flag("hasWarnedTickerRestFallback", true)?;
        let log = host.capture("parent.log")?;
        let mut message: Vec<u16> = "[Lighter] ".encode_utf16().collect();
        message.extend_from_slice(reason);
        log.call(&[
            AccountValue::String(message),
            AccountValue::String("warning".encode_utf16().collect()),
        ])?;
    }
    call(host.as_ref(), "fetchAndApplyTickerFallback", &[])?;
    let timer = host.capture_timer("setInterval")?;
    let owner = host.clone();
    let callback: ReadinessCallback = Rc::new(move || {
        if owner.flag("isDisposed")? {
            call(owner.as_ref(), "stopTickerRestFallback", &[])?;
            return Ok(());
        }
        if owner.flag("isConnected")?
            && owner.flag("hasReceivedMessageSinceConnect")?
            && decision(owner.as_ref(), "hasHydratedTickerPrice")?
        {
            call(owner.as_ref(), "stopTickerRestFallback", &[])?;
            return Ok(());
        }
        call(owner.as_ref(), "fetchAndApplyTickerFallback", &[])
    });
    let id = timer.call(callback, INTERVAL)?;
    host.set_value("tickerRestFallbackInterval", id)
}
// ts: LighterPublicWebsocket.onOpen
pub fn on_open(host: &dyn PublicReadinessHost) -> SigningResult<()> {
    if !host.flag("isDisposed")? {
        host.set_flag("openedSinceLastConnect", true)?;
        for name in [
            "syncBboTickerTopics",
            "subscribe",
            "primeTickerRestFallback",
            "armExistingOrderBookTimeouts",
            "armExistingOhlcvNoDataTimeouts",
            "armTickerRestFallbackWatchdog",
            "ping",
        ] {
            call(host, name, &[])?;
        }
        let off = host.capture("emitter.off")?;
        let handler = host.value("onMarketsChanged")?;
        off.call(&[
            AccountValue::String("markets:changed".encode_utf16().collect()),
            handler,
        ])?;
        let on = host.capture("emitter.on")?;
        let handler = host.value("onMarketsChanged")?;
        on.call(&[
            AccountValue::String("markets:changed".encode_utf16().collect()),
            handler,
        ])?;
        if host.flag("hasEverConnected")? {
            call(host, "emitWsReconnected", &[])?;
        }
        host.set_flag("hasEverConnected", true)?;
    }
    Ok(())
}
// ts: LighterPublicWebsocket.ping
pub fn ping(host: &dyn PublicReadinessHost) -> SigningResult<()> {
    if !host.flag("isDisposed")? {
        let now = host.performance_now()?;
        host.set_value("pingAt", AccountValue::Number(now))?;
        let send = host.capture("sendPayload")?;
        let timestamp = host.virtual_now()?;
        let payload = host.ping_payload(timestamp)?;
        send.call(&[payload])?;
    }
    Ok(())
}
