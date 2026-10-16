//! Exact synchronous field bodies over fallible receiver/host operations.
//! This ports the bodies, not the still-pending exchange or REST reconciler.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamTimeout {
    WriteCheck,
    DegradedReconcile,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamSocket {
    Public,
    Private,
    SecondsTickers,
    ThirdTickers,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UndefinedProperty {
    Absent,
    PresentUndefined,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrivateStreamHealthUpdate {
    pub private_degraded_reason: UndefinedProperty,
}

/// Receiver properties and called operations; error, clock and timer values
/// remain opaque. Each property read is performed at its source call site.
/// Truthiness operations implement JS ToBoolean without additional effects.
pub trait StreamLifecycleHost {
    type Error;
    type Timestamp;
    type Timeout;
    fn super_dispose(&self) -> Result<(), Self::Error>;
    fn timeout(&self, slot: StreamTimeout) -> Result<Self::Timeout, Self::Error>;
    fn timeout_truthy(&self, value: &Self::Timeout) -> bool;
    fn clear_timeout(&self, value: Self::Timeout) -> Result<(), Self::Error>;
    fn unset_timeout(&self, slot: StreamTimeout) -> Result<(), Self::Error>;
    fn dispose_socket(&self, socket: StreamSocket) -> Result<(), Self::Error>;
    fn set_degraded(&self, value: bool) -> Result<(), Self::Error>;
    fn date_now(&self) -> Result<Self::Timestamp, Self::Error>;
    fn set_last_message_at(&self, value: Self::Timestamp) -> Result<(), Self::Error>;
    fn set_warned(&self, value: bool) -> Result<(), Self::Error>;
    fn update_socket_health(&self, update: PrivateStreamHealthUpdate) -> Result<(), Self::Error>;
    fn stop_reconcile(&self) -> Result<(), Self::Error>;
    fn reconcile_running_truthy(&self) -> Result<bool, Self::Error>;
    /// Invokes the method without awaiting its result. The host owns detached
    /// async work; only synchronous property/call failures reach this Result.
    fn launch_rest_reconcile(&self) -> Result<(), Self::Error>;
}

// ts: BitgetExchange.dispose
pub fn dispose<H: StreamLifecycleHost>(host: &H) -> Result<(), H::Error> {
    host.super_dispose()?;
    if host.timeout_truthy(&host.timeout(StreamTimeout::WriteCheck)?) {
        host.clear_timeout(host.timeout(StreamTimeout::WriteCheck)?)?;
        host.unset_timeout(StreamTimeout::WriteCheck)?;
    }
    if host.timeout_truthy(&host.timeout(StreamTimeout::DegradedReconcile)?) {
        host.clear_timeout(host.timeout(StreamTimeout::DegradedReconcile)?)?;
        host.unset_timeout(StreamTimeout::DegradedReconcile)?;
    }
    host.dispose_socket(StreamSocket::Public)?;
    host.dispose_socket(StreamSocket::Private)?;
    host.dispose_socket(StreamSocket::SecondsTickers)?;
    host.dispose_socket(StreamSocket::ThirdTickers)?;
    Ok(())
}

// ts: BitgetExchange.markPrivateStreamMessageReceived
pub fn mark_private_stream_message_received<H: StreamLifecycleHost>(
    host: &H,
) -> Result<(), H::Error> {
    host.set_degraded(false)?;
    host.set_last_message_at(host.date_now()?)?;
    host.set_warned(false)?;
    host.update_socket_health(PrivateStreamHealthUpdate {
        private_degraded_reason: UndefinedProperty::PresentUndefined,
    })?;
    host.stop_reconcile()?;
    Ok(())
}

// ts: BitgetExchange.startPrivateStreamDegradedReconcile
pub fn start_private_stream_degraded_reconcile<H: StreamLifecycleHost>(
    host: &H,
) -> Result<(), H::Error> {
    if host.reconcile_running_truthy()?
        || host.timeout_truthy(&host.timeout(StreamTimeout::DegradedReconcile)?)
    {
        return Ok(());
    }
    host.launch_rest_reconcile()?;
    Ok(())
}

// ts: BitgetExchange.stopPrivateStreamDegradedReconcile
pub fn stop_private_stream_degraded_reconcile<H: StreamLifecycleHost>(
    host: &H,
) -> Result<(), H::Error> {
    if host.timeout_truthy(&host.timeout(StreamTimeout::DegradedReconcile)?) {
        host.clear_timeout(host.timeout(StreamTimeout::DegradedReconcile)?)?;
        host.unset_timeout(StreamTimeout::DegradedReconcile)?;
    }
    Ok(())
}
