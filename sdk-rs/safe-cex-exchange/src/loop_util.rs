//! Source loop scheduler; the host supplies browser scheduling and function source.
use crate::{
    eager::start_eager,
    js_runtime::{
        promise::Promise,
        value::{Js, Thrown},
    },
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime},
};
use std::{cell::RefCell, collections::HashSet, future::Future, pin::Pin, rc::Rc};
type Callback = Rc<dyn Fn()>;
type Sleep = Rc<dyn Fn(f64) -> Pin<Box<dyn Future<Output = Result<(), String>>>>>;
type Schedule = Rc<dyn Fn(Callback) -> Result<(), String>>;
#[derive(Clone)]
pub struct LoopHost {
    pub runtime: Rc<dyn ExchangeRuntime>,
    pub sleep: Sleep,
    /// Evaluated after sleep: window && document && visibilityState === visible.
    pub document_visible: Rc<dyn Fn() -> bool>,
    pub request_animation_frame: Schedule,
    /// Must enqueue setTimeout(fn,0), never invoke inline.
    pub set_timeout_zero: Schedule,
    pub warn: Rc<dyn Fn(&str)>,
}
pub fn warn_missing_interval(source: &[u16], host: &LoopHost) {
    let key: Vec<u16> = source.iter().copied().take(200).collect();
    let fresh = with_logged(|logged| logged.borrow_mut().insert(key.clone()));
    if fresh {
        (host.warn)(&format!(
            "[safe-cex] loop() called without a minInterval — defaulting to 5000ms. This is a safety fallback; the upstream caller should pass a real interval. Caller: {}",
            String::from_utf16_lossy(&key)
        ));
    }
}
fn with_logged<T>(operation: impl FnOnce(&RefCell<HashSet<Vec<u16>>>) -> T) -> T {
    thread_local! { static LOGGED: RefCell<HashSet<Vec<u16>>> = RefCell::new(HashSet::new()); }
    LOGGED.with(operation)
}
pub fn loop_once(
    callback: Callback,
    source: &[u16],
    min_interval: Option<f64>,
    host: &LoopHost,
) -> ExchangeFuture<'static, ()> {
    let host = host.clone();
    let source = source.to_vec();
    let runtime = host.runtime.clone();
    start_eager(&runtime, async move {
        if min_interval == Some(f64::INFINITY) {
            return Ok(());
        }
        let effective = match min_interval {
            Some(interval) if interval > 0.0 => interval,
            _ => {
                warn_missing_interval(&source, &host);
                5000.0
            }
        };
        let sleep = (host.sleep)(effective);
        let slept = Promise::spawn(
            &host.runtime,
            Box::pin(async move {
                sleep
                    .await
                    .map_err(|error| Thrown::from(ExchangeError::new(error)))?;
                Ok(Js::Undefined)
            }),
        );
        slept
            .wait()
            .await
            .map_err(|error| error.to_exchange_error())?;
        if (host.document_visible)() {
            (host.request_animation_frame)(callback).map_err(ExchangeError::new)?;
        } else {
            (host.set_timeout_zero)(callback).map_err(ExchangeError::new)?;
        }
        Ok(())
    })
}
