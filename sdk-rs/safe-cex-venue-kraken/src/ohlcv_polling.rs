//! Futures exchange OHLCV polling, retaining original options and candle allocations.
use crate::{
    rest_account::{Handle, ReadHost, ReadResult},
    trading::{field, obj, units},
};
use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::{Rc, Weak},
};
pub type PollFuture<H> = Pin<Box<dyn Future<Output = ReadResult<H, ()>>>>;
pub type PollCallback<H> = Rc<dyn Fn() -> PollFuture<H>>;
pub type CandleCallback<H> = Rc<dyn Fn(Handle<H>) -> ReadResult<H, ()>>;
pub type Cleanup<H> = Rc<dyn Fn() -> ReadResult<H, ()>>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Timeout,
    Active,
    Candle,
}
pub trait PollHost: ReadHost + 'static {
    fn map_has(&self, slot: Slot, key: &[u16]) -> ReadResult<Self, bool>;
    fn map_get(&self, slot: Slot, key: &[u16]) -> ReadResult<Self, Handle<Self>>;
    fn map_set(&self, slot: Slot, key: &[u16], value: Handle<Self>) -> ReadResult<Self, ()>;
    fn map_delete(&self, slot: Slot, key: &[u16]) -> ReadResult<Self, ()>;
    fn clear_timeout(&self, timer: Handle<Self>) -> ReadResult<Self, ()>;
    fn set_timeout(
        &self,
        callback: PollCallback<Self>,
        delay: f64,
    ) -> ReadResult<Self, Handle<Self>>;
    fn fetch_candles(
        &self,
        options: Handle<Self>,
    ) -> ReadResult<Self, impl Future<Output = ReadResult<Self, Handle<Self>>>>;
    fn console_error(&self, prefix: &'static str, error: Self::Error) -> ReadResult<Self, ()>;
    /// Run the synchronous prefix, retain pending work, and observe task rejection out of band.
    /// Even an immediately ready rejection must leave the listener cleanup available.
    fn spawn(&self, future: PollFuture<Self>);
}
struct Poller<H: PollHost> {
    host: Rc<H>,
    key: Vec<u16>,
    options: Handle<H>,
    callback: CandleCallback<H>,
    poll: RefCell<Option<Weak<dyn Fn() -> PollFuture<H>>>>,
}
impl<H: PollHost> Poller<H> {
    fn run(self: Rc<Self>, poll: PollCallback<H>) -> PollFuture<H> {
        Box::pin(async move {
            let h = &*self.host;
            if !h.truthy(&*h.map_get(Slot::Active, &self.key)?) {
                return Ok(());
            }
            let result = async {
                let now = h.now()?;
                let options = obj(
                    h,
                    vec![
                        ("symbol", field(h, &self.options, "symbol")?),
                        ("interval", field(h, &self.options, "interval")?),
                        ("limit", h.number(2.0)),
                        ("to", h.number(now)),
                    ],
                )?;
                let candles = h.fetch_candles(options)?.await?;
                let length = h.numeric(&*field(h, &candles, "length")?)?;
                if length > 0.0 {
                    let key = h.string(&h.number(length - 1.0))?;
                    let latest = h.field(&candles, &key, false)?;
                    let previous = h.map_get(Slot::Candle, &self.key)?;
                    let changed = !h.truthy(&previous)
                        || !h.strict_eq(
                            &*field(h, &previous, "timestamp")?,
                            &*field(h, &latest, "timestamp")?,
                        )
                        || !h.strict_eq(
                            &*field(h, &previous, "close")?,
                            &*field(h, &latest, "close")?,
                        )
                        || !h.strict_eq(
                            &*field(h, &previous, "volume")?,
                            &*field(h, &latest, "volume")?,
                        );
                    if changed {
                        (self.callback)(latest.clone())?;
                        h.map_set(Slot::Candle, &self.key, latest)?;
                    }
                }
                Ok::<(), H::Error>(())
            }
            .await;
            if let Err(error) = result {
                h.console_error("[Kraken] Error polling OHLCV:", error)?;
            }
            if h.truthy(&*h.map_get(Slot::Active, &self.key)?) {
                let timeout = h.set_timeout(poll, 5000.0)?;
                h.map_set(Slot::Timeout, &self.key, timeout)?;
            }
            Ok(())
        })
    }
}
pub fn listen_ohlcv<H: PollHost>(
    host: Rc<H>,
    options: Handle<H>,
    callback: CandleCallback<H>,
) -> ReadResult<H, Cleanup<H>> {
    let key = [
        host.string(&*field(&*host, &options, "symbol")?)?,
        units(":"),
        host.string(&*field(&*host, &options, "interval")?)?,
    ]
    .concat();
    if host.map_has(Slot::Timeout, &key)? {
        host.clear_timeout(host.map_get(Slot::Timeout, &key)?)?;
        host.map_delete(Slot::Timeout, &key)?;
    }
    host.map_set(Slot::Active, &key, host.boolean(true))?;
    let state = Rc::new(Poller {
        host: host.clone(),
        key: key.clone(),
        options,
        callback,
        poll: RefCell::new(None),
    });
    let held = state.clone();
    let poll: PollCallback<H> = Rc::new(move || {
        let callback = held
            .poll
            .borrow()
            .as_ref()
            .and_then(Weak::upgrade)
            .expect("live source poll callback");
        held.clone().run(callback)
    });
    *state.poll.borrow_mut() = Some(Rc::downgrade(&poll));
    host.spawn(poll());
    Ok(Rc::new(move || {
        host.map_set(Slot::Active, &key, host.boolean(false))?;
        if host.map_has(Slot::Timeout, &key)? {
            host.clear_timeout(host.map_get(Slot::Timeout, &key)?)?;
            host.map_delete(Slot::Timeout, &key)?;
        }
        host.map_delete(Slot::Candle, &key)
    }))
}
