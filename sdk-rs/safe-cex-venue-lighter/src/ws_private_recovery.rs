//! Private stream recovery/timer bodies. Full websocket construction, transport
//! and authentication are separate required host bindings, not ported here.
//! Numeric/boolean owner fields use their source typed domains; timer values and
//! thrown values retain native identity. Math and readonly constants use ordinary
//! unmodified source intrinsics. Host getters/setters are live and fallible.
use crate::{sdk_account_api::AccountValue, transaction_signing::SigningResult};
use safe_cex_num::{js_number_string, math_max, math_round};
use std::rc::Rc;
const BUFFER: f64 = 2_000.0;
const STALE: f64 = 120_000.0;
const NO_STREAM: f64 = 180_000.0;
const COOLDOWN: f64 = 180_000.0;
const GRACE: f64 = 20_000.0;
/// Capture the actual callee and receiver now, including a noncallable value;
/// defer Call failures until after arguments. Parent log, owner methods, global
/// clear timers and super.dispose preserve their respective source receivers.
/// Authentication is called eagerly and its return is discarded, without await.
pub trait RecoveryCall {
    fn call(&self, args: &[AccountValue]) -> SigningResult<()>;
}
pub type RecoveryCallback = Rc<dyn Fn() -> SigningResult<()>>;
/// Captured global setTimeout. Retain the callback and actual argument/returned
/// timer identity; execute with live owner state, never snapshot callback fields.
/// Production host supplies actual timer queue/cancellation/exception reporting.
pub trait RecoveryTimerCall {
    fn call(&self, callback: RecoveryCallback, delay: f64) -> SigningResult<AccountValue>;
}
pub trait PrivateRecoveryHost {
    /// Ordinary names read the live typed boolean owner field. The special
    /// name isWsRateLimited() performs the actual owner method Get and Call
    /// with the owner receiver, including original lookup/call throws. Never
    /// treat this operation as a cached flag; its method can read the clock.
    fn boolean(&self, name: &str) -> SigningResult<bool>;
    fn number(&self, name: &str) -> SigningResult<f64>;
    fn set_number(&self, name: &str, value: f64) -> SigningResult<()>;
    fn timer(&self, name: &str) -> SigningResult<AccountValue>;
    fn set_timer(&self, name: &str, value: AccountValue) -> SigningResult<()>;
    fn capture(&self, name: &str) -> SigningResult<Rc<dyn RecoveryCall>>;
    fn capture_set_timeout(&self) -> SigningResult<Rc<dyn RecoveryTimerCall>>;
    /// Receiver-bound virtualClock.getCurrentTimeMs, preserving get/call throws.
    fn now(&self) -> SigningResult<f64>;
}
fn call(host: &dyn PrivateRecoveryHost, name: &str) -> SigningResult<()> {
    host.capture(name)?.call(&[])
}
fn log(host: &dyn PrivateRecoveryHost, message: &str, severity: &str) -> SigningResult<()> {
    host.capture("parent.log")?.call(&[
        AccountValue::String(message.encode_utf16().collect()),
        AccountValue::String(severity.encode_utf16().collect()),
    ])
}
fn clear(host: &dyn PrivateRecoveryHost, field: &str, operation: &str) -> SigningResult<()> {
    if host.timer(field)?.truthy() {
        let timer_call = host.capture(operation)?;
        // Source re-reads the field after callee lookup; do not clear test value.
        timer_call.call(&[host.timer(field)?])?;
        host.set_timer(field, AccountValue::Undefined)?;
    }
    Ok(())
}
// ts: LighterPrivateWebsocket#stopDiagnosticsLoop
pub fn stop_diagnostics_loop(host: &dyn PrivateRecoveryHost) -> SigningResult<()> {
    clear(host, "diagnosticsInterval", "clearInterval")
}
// ts: LighterPrivateWebsocket#clearStreamRecoveryReconnectTimeout
pub fn clear_stream_recovery_reconnect_timeout(
    host: &dyn PrivateRecoveryHost,
) -> SigningResult<()> {
    clear(host, "streamRecoveryReconnectTimeout", "clearTimeout")
}
// ts: LighterPrivateWebsocket#clearRateLimitResubscribeTimeout
pub fn clear_rate_limit_resubscribe_timeout(host: &dyn PrivateRecoveryHost) -> SigningResult<()> {
    clear(host, "rateLimitResubscribeTimeout", "clearTimeout")
}
// ts: LighterPrivateWebsocket#scheduleRateLimitResubscribe
pub fn schedule_rate_limit_resubscribe(host: Rc<dyn PrivateRecoveryHost>) -> SigningResult<()> {
    if host.boolean("isDisposed")? || !host.boolean("accountAllOrdersChannelEnabled")? {
        return Ok(());
    }
    let now = host.now()?;
    let delay = math_max(host.number("wsRateLimitedUntil")? - now + BUFFER, BUFFER);
    call(host.as_ref(), "clearRateLimitResubscribeTimeout")?;
    log(
        host.as_ref(),
        &format!(
            "[Lighter][PWS] Scheduling private WS subscribe retry in {}ms after rate-limit cooldown.",
            js_number_string(math_round(delay))
        ),
        "info",
    )?;
    let timer = host.capture_set_timeout()?;
    let owner = host.clone();
    let callback: RecoveryCallback = Rc::new(move || {
        owner.set_timer("rateLimitResubscribeTimeout", AccountValue::Undefined)?;
        if owner.boolean("isDisposed")?
            || !owner.boolean("isConnected")?
            || owner.boolean("reconnecting")?
        {
            return Ok(());
        }
        // Dynamic owner method result is boolean in the source typed contract.
        if owner.boolean("isWsRateLimited()")? {
            call(owner.as_ref(), "scheduleRateLimitResubscribe")?;
            return Ok(());
        }
        log(
            owner.as_ref(),
            "[Lighter][PWS] Retrying private WS subscriptions after rate-limit cooldown.",
            "info",
        )?;
        call(owner.as_ref(), "authenticateAndSubscribe")
    });
    let id = timer.call(callback, delay)?;
    host.set_timer("rateLimitResubscribeTimeout", id)
}
// ts: LighterPrivateWebsocket#scheduleStreamRecoveryReconnectFallback
pub fn schedule_stream_recovery_reconnect_fallback(
    host: Rc<dyn PrivateRecoveryHost>,
) -> SigningResult<()> {
    call(host.as_ref(), "clearStreamRecoveryReconnectTimeout")?;
    let timer = host.capture_set_timeout()?;
    let owner = host.clone();
    let callback: RecoveryCallback = Rc::new(move || {
        owner.set_timer("streamRecoveryReconnectTimeout", AccountValue::Undefined)?;
        if owner.boolean("isDisposed")?
            || !owner.boolean("isConnected")?
            || owner.boolean("reconnecting")?
        {
            return Ok(());
        }
        let now = owner.now()?;
        let no_stream = owner.number("accountStreamMessageCount")? == 0.0;
        let age = if owner.number("lastAccountStreamMessageAt")? > 0.0 {
            now - owner.number("lastAccountStreamMessageAt")?
        } else {
            f64::INFINITY
        };
        if !(no_stream || age > STALE) {
            return Ok(());
        }
        log(
            owner.as_ref(),
            "[Lighter][PWS] Private stream still stale after recovery subscribe attempt; reconnecting.",
            "warning",
        )?;
        call(owner.as_ref(), "reconnect")
    });
    let id = timer.call(callback, GRACE)?;
    host.set_timer("streamRecoveryReconnectTimeout", id)
}
// ts: LighterPrivateWebsocket#maybeRecoverPrivateStream
pub fn maybe_recover_private_stream(host: &dyn PrivateRecoveryHost) -> SigningResult<()> {
    if host.boolean("isDisposed")?
        || !host.boolean("isConnected")?
        || host.boolean("reconnecting")?
    {
        return Ok(());
    }
    let now = host.now()?;
    if now - host.number("lastStreamRecoveryAttemptAt")? < COOLDOWN {
        return Ok(());
    }
    let since_connect = now - host.number("lastConnectTime")?;
    let no_stream = host.number("accountStreamMessageCount")? == 0.0;
    let age = if host.number("lastAccountStreamMessageAt")? > 0.0 {
        now - host.number("lastAccountStreamMessageAt")?
    } else {
        f64::INFINITY
    };
    let recover_none = no_stream && since_connect > NO_STREAM;
    let recover_stale = host.number("lastAccountStreamMessageAt")? > 0.0 && age > STALE;
    if !recover_none && !recover_stale {
        return Ok(());
    }
    host.set_number("lastStreamRecoveryAttemptAt", now)?;
    if host.boolean("useReadonlyFallback")? {
        log(
            host,
            "[Lighter][PWS] Private stream recovery skipped while readonly fallback is active.",
            "info",
        )?;
        return Ok(());
    }
    if host.boolean("isWsRateLimited()")? {
        log(
            host,
            "[Lighter][PWS] Private stream recovery deferred while server rate-limit cooldown is active.",
            "warning",
        )?;
        call(host, "scheduleRateLimitResubscribe")?;
        return Ok(());
    }
    let reason = if recover_none {
        format!(
            "No account stream messages for {}s since connect",
            js_number_string(math_round(since_connect / 1000.0))
        )
    } else {
        format!(
            "Account stream stale for {}s",
            js_number_string(math_round(age / 1000.0))
        )
    };
    log(
        host,
        &format!(
            "[Lighter][PWS] {reason}. Attempting recovery via re-subscribe with guarded reconnect fallback."
        ),
        "warning",
    )?;
    call(host, "authenticateAndSubscribe")?;
    call(host, "scheduleStreamRecoveryReconnectFallback")
}
// ts: LighterPrivateWebsocket#noteAccountStreamActivity
pub fn note_account_stream_activity(
    host: &dyn PrivateRecoveryHost,
    stats_topic: &[u16],
    channel: &[u16],
) -> SigningResult<()> {
    for field in [
        "failedHandshakes",
        "readonlyFallbackFailures",
        "onCloseCount",
    ] {
        if host.number(field)? > 0.0 {
            host.set_number(field, 0.0)?;
        }
    }
    let count = host.number("accountStreamMessageCount")?;
    host.set_number("accountStreamMessageCount", count + 1.0)?;
    let now = host.now()?;
    host.set_number("lastAccountStreamMessageAt", now)?;
    call(host, "clearStreamRecoveryReconnectTimeout")?;
    call(host, "clearPrivateStreamRateLimitErrorState")?;
    host.capture("recordMessageHandlerStats")?
        .call(&[AccountValue::String(stats_topic.to_vec())])?;
    if host.number("accountStreamMessageCount")? == 1.0 {
        let log = host.capture("parent.log")?;
        let mut message: Vec<u16> =
            "[Lighter][PWS] First private account stream message received on channel \""
                .encode_utf16()
                .collect();
        message.extend(if channel.is_empty() {
            "unknown".encode_utf16().collect::<Vec<_>>()
        } else {
            channel.to_vec()
        });
        message.extend("\".".encode_utf16());
        log.call(&[
            AccountValue::String(message),
            AccountValue::String("info".encode_utf16().collect()),
        ])?;
    }
    Ok(())
}
// ts: LighterPrivateWebsocket.dispose
pub fn dispose(host: &dyn PrivateRecoveryHost) -> SigningResult<()> {
    call(host, "stopDiagnosticsLoop")?;
    call(host, "clearStreamRecoveryReconnectTimeout")?;
    call(host, "clearRateLimitResubscribeTimeout")?;
    clear(host, "positionsThrottleTimeout", "clearTimeout")?;
    clear(host, "balanceThrottleTimeout", "clearTimeout")?;
    call(host, "super.dispose")
}
