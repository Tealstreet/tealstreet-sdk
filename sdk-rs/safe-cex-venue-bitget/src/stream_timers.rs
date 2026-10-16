//! Degradation and timer field bodies, including their source callbacks.
use crate::stream_lifecycle::{StreamLifecycleHost, StreamTimeout};
use std::rc::Rc;

const DEGRADED_MESSAGE: &str = "Bitget private WS is not receiving live order/position updates. Tealstreet will reconcile through REST; if this account uses IP whitelisting, verify the terminal-listed browser/server WebSocket IPs are allowed.";

pub type StreamTimerCallback<E> = Rc<dyn Fn() -> Result<(), E>>;

pub struct ReasonHealthUpdate<'a, V> {
    pub private_degraded_reason: &'a V,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForcedWarningOptions {
    pub force_toast: bool,
}

/// Opaque values retain undefined, arbitrary strings/objects and their aliases.
/// Literal construction and value tests are pure language operations. Logging
/// binds the imported LogSeverity.Warning argument; called SDK operations remain
/// independently mapped. The scheduler owns callbacks until firing/cancellation.
pub trait StreamTimerHost: StreamLifecycleHost {
    type Value;
    fn is_undefined(value: &Self::Value) -> bool;
    fn string_literal(value: &'static str) -> Self::Value;
    fn boolean_literal(value: bool) -> Self::Value;
    fn value_truthy(value: &Self::Value) -> bool;
    fn update_reason_health(
        &self,
        update: ReasonHealthUpdate<'_, Self::Value>,
    ) -> Result<(), Self::Error>;
    fn log_warning(&self, reason: &Self::Value) -> Result<(), Self::Error>;
    fn recover_missing_private_stream(&self, reason: &Self::Value) -> Result<(), Self::Error>;
    fn start_reconcile(&self) -> Result<(), Self::Error>;
    fn warned_truthy(&self) -> Result<bool, Self::Error>;
    fn emit_warning(
        &self,
        reason: &Self::Value,
        options: ForcedWarningOptions,
    ) -> Result<(), Self::Error>;
    fn disposed_truthy(&self) -> Result<bool, Self::Error>;
    fn degraded_truthy(&self) -> Result<bool, Self::Error>;
    fn set_timeout(
        &self,
        callback: StreamTimerCallback<Self::Error>,
        delay_ms: u64,
    ) -> Result<Self::Timeout, Self::Error>;
    fn assign_timeout(&self, slot: StreamTimeout, value: Self::Timeout) -> Result<(), Self::Error>;
    fn last_message_at(&self) -> Result<Self::Timestamp, Self::Error>;
    /// JS >, including ordered operand coercion and its original failures.
    fn is_later(
        &self,
        current: &Self::Timestamp,
        captured: &Self::Timestamp,
    ) -> Result<bool, Self::Error>;
    fn mark_degraded(
        &self,
        reason: Self::Value,
        force_toast: Self::Value,
    ) -> Result<(), Self::Error>;
}

// ts: BitgetExchange.markPrivateStreamDegraded
pub fn mark_private_stream_degraded<H: StreamTimerHost>(
    host: &H,
    reason: H::Value,
    force_toast: H::Value,
) -> Result<(), H::Error> {
    let reason = if H::is_undefined(&reason) {
        H::string_literal(DEGRADED_MESSAGE)
    } else {
        reason
    };
    let force_toast = if H::is_undefined(&force_toast) {
        H::boolean_literal(false)
    } else {
        force_toast
    };
    host.set_degraded(true)?;
    host.update_reason_health(ReasonHealthUpdate {
        private_degraded_reason: &reason,
    })?;
    host.log_warning(&reason)?;
    host.recover_missing_private_stream(&reason)?;
    host.start_reconcile()?;
    if H::value_truthy(&force_toast) && !host.warned_truthy()? {
        host.set_warned(true)?;
        host.emit_warning(&reason, ForcedWarningOptions { force_toast: true })?;
    }
    Ok(())
}

// ts: BitgetExchange.schedulePrivateStreamDegradedReconcile
pub fn schedule_private_stream_degraded_reconcile<H>(host: Rc<H>) -> Result<(), H::Error>
where
    H: StreamTimerHost + 'static,
    H::Error: 'static,
{
    if host.disposed_truthy()?
        || !host.degraded_truthy()?
        || host.timeout_truthy(&host.timeout(StreamTimeout::DegradedReconcile)?)
    {
        return Ok(());
    }
    let receiver = host.clone();
    let callback: StreamTimerCallback<H::Error> = Rc::new(move || {
        receiver.unset_timeout(StreamTimeout::DegradedReconcile)?;
        receiver.launch_rest_reconcile()?;
        Ok(())
    });
    let timer = host.set_timeout(callback, 10_000)?;
    host.assign_timeout(StreamTimeout::DegradedReconcile, timer)?;
    Ok(())
}

// ts: BitgetExchange.schedulePrivateStreamWriteCheck
pub fn schedule_private_stream_write_check<H>(host: Rc<H>) -> Result<(), H::Error>
where
    H: StreamTimerHost + 'static,
    H::Error: 'static,
    H::Timestamp: 'static,
{
    let last_private_frame_at_schedule = host.last_message_at()?;
    if host.timeout_truthy(&host.timeout(StreamTimeout::WriteCheck)?) {
        host.clear_timeout(host.timeout(StreamTimeout::WriteCheck)?)?;
    }
    let receiver = host.clone();
    let callback: StreamTimerCallback<H::Error> = Rc::new(move || {
        receiver.unset_timeout(StreamTimeout::WriteCheck)?;
        if receiver.disposed_truthy()?
            || receiver.is_later(
                &receiver.last_message_at()?,
                &last_private_frame_at_schedule,
            )?
        {
            return Ok(());
        }
        receiver.mark_degraded(
            H::string_literal(DEGRADED_MESSAGE),
            H::boolean_literal(true),
        )?;
        Ok(())
    });
    let timer = host.set_timeout(callback, 8_000)?;
    host.assign_timeout(StreamTimeout::WriteCheck, timer)?;
    Ok(())
}
