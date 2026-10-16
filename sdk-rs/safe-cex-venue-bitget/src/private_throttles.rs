//! Private fetch throttle bodies and inline timer callbacks.
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchKind {
    Positions,
    Balance,
    Orders,
}
pub type FetchThrottleCallback<E> = Rc<dyn Fn() -> Result<(), E>>;

/// Supplied fallible receiver reads/calls, retaining the typed source bool/number
/// fields. Timeout delay is the raw JS number (not pre-clamped to an integer).
/// Launch invokes the fetch method without awaiting its result; the host owns
/// detached work and timer callbacks until firing/cancellation.
pub trait FetchThrottleHost {
    type Error;
    fn date_now(&self) -> Result<f64, Self::Error>;
    fn disposed(&self) -> Result<bool, Self::Error>;
    fn scheduled(&self, kind: FetchKind) -> Result<bool, Self::Error>;
    fn last_fetch_at(&self, kind: FetchKind) -> Result<f64, Self::Error>;
    fn throttle_ms(&self, kind: FetchKind) -> Result<f64, Self::Error>;
    fn set_scheduled(&self, kind: FetchKind, value: bool) -> Result<(), Self::Error>;
    fn pending(&self, kind: FetchKind) -> Result<bool, Self::Error>;
    fn set_pending(&self, kind: FetchKind, value: bool) -> Result<(), Self::Error>;
    fn launch(&self, kind: FetchKind) -> Result<(), Self::Error>;
    fn set_timeout(
        &self,
        callback: FetchThrottleCallback<Self::Error>,
        delay_ms: f64,
    ) -> Result<(), Self::Error>;
}

// ts: BitgetPrivateWebsocket.throttledFetchPositions
pub fn throttled_fetch_positions<H>(host: Rc<H>) -> Result<(), H::Error>
where
    H: FetchThrottleHost + 'static,
    H::Error: 'static,
{
    let now = host.date_now()?;
    if host.disposed()? {
        return Ok(());
    }
    if host.scheduled(FetchKind::Positions)? {
        return Ok(());
    }
    let time_since_last_fetch = now - host.last_fetch_at(FetchKind::Positions)?;
    if time_since_last_fetch >= host.throttle_ms(FetchKind::Positions)? {
        host.launch(FetchKind::Positions)?;
    } else {
        host.set_scheduled(FetchKind::Positions, true)?;
        let receiver = host.clone();
        let callback: FetchThrottleCallback<H::Error> = Rc::new(move || {
            receiver.set_scheduled(FetchKind::Positions, false)?;
            if !receiver.disposed()? {
                if receiver.pending(FetchKind::Positions)? {
                    receiver.launch(FetchKind::Positions)?;
                }
            }
            Ok(())
        });
        let delay = host.throttle_ms(FetchKind::Positions)? - time_since_last_fetch;
        host.set_timeout(callback, delay)?;
    }
    Ok(())
}

// ts: BitgetPrivateWebsocket.throttledFetchBalance
pub fn throttled_fetch_balance<H>(host: Rc<H>) -> Result<(), H::Error>
where
    H: FetchThrottleHost + 'static,
    H::Error: 'static,
{
    let now = host.date_now()?;
    if host.disposed()? {
        return Ok(());
    }
    host.set_pending(FetchKind::Balance, true)?;
    if host.scheduled(FetchKind::Balance)? {
        return Ok(());
    }
    let time_since_last_fetch = now - host.last_fetch_at(FetchKind::Balance)?;
    if time_since_last_fetch >= host.throttle_ms(FetchKind::Balance)? {
        host.launch(FetchKind::Balance)?;
    } else {
        host.set_scheduled(FetchKind::Balance, true)?;
        let receiver = host.clone();
        let callback: FetchThrottleCallback<H::Error> = Rc::new(move || {
            receiver.set_scheduled(FetchKind::Balance, false)?;
            if !receiver.disposed()? && receiver.pending(FetchKind::Balance)? {
                receiver.launch(FetchKind::Balance)?;
            }
            Ok(())
        });
        let delay = host.throttle_ms(FetchKind::Balance)? - time_since_last_fetch;
        host.set_timeout(callback, delay)?;
    }
    Ok(())
}

// ts: BitgetPrivateWebsocket.throttledFetchOrders
pub fn throttled_fetch_orders<H>(host: Rc<H>) -> Result<(), H::Error>
where
    H: FetchThrottleHost + 'static,
    H::Error: 'static,
{
    let now = host.date_now()?;
    if host.disposed()? {
        return Ok(());
    }
    if host.scheduled(FetchKind::Orders)? {
        return Ok(());
    }
    let time_since_last_fetch = now - host.last_fetch_at(FetchKind::Orders)?;
    if time_since_last_fetch >= host.throttle_ms(FetchKind::Orders)? {
        host.launch(FetchKind::Orders)?;
    } else {
        host.set_scheduled(FetchKind::Orders, true)?;
        let receiver = host.clone();
        let callback: FetchThrottleCallback<H::Error> = Rc::new(move || {
            receiver.set_scheduled(FetchKind::Orders, false)?;
            if !receiver.disposed()? && receiver.pending(FetchKind::Orders)? {
                receiver.launch(FetchKind::Orders)?;
            }
            Ok(())
        });
        let delay = host.throttle_ms(FetchKind::Orders)? - time_since_last_fetch;
        host.set_timeout(callback, delay)?;
    }
    Ok(())
}
