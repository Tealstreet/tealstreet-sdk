//! Listen-key bodies at configured HTTP, scheduler and logger boundaries.
use safe_cex_exchange::runtime::ExchangeError;
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
};

/// Decoded response containers preserve undefined/null and aliased key handles.
/// Primitive responses normalize to Data(Undefined); primitive data to Present
/// with no key. Cloned keys must preserve native handle identity.
/// Executable getters/proxies and concrete HTTP/class wiring remain pending.
pub enum ListenKeyResponse<K> {
    Undefined,
    Null,
    Data(ListenKeyData<K>),
}
pub enum ListenKeyData<K> {
    Undefined,
    Null,
    Present(Rc<RefCell<Option<K>>>),
}
/// Error instanceof Error versus JSON/undefined thrown values. Rc retains
/// native error/value identity; custom prototypes and executable coercion are
/// outside the decoded boundary. JSON String(any) uses the shared JS conversion.
#[derive(Debug, Clone)]
pub enum KeepaliveFailure {
    Error(Rc<ExchangeError>),
    Other(Option<Rc<Value>>),
}
#[derive(Default)]
pub struct KeepaliveState {
    pub is_disposed: crate::class_flags::LiveFlag,
    pub keepalive_failing: Cell<bool>,
}
fn type_error(message: String) -> KeepaliveFailure {
    let mut e = ExchangeError::new(message);
    e.name = "TypeError".into();
    KeepaliveFailure::Error(Rc::new(e))
}
fn response_data<K>(response: ListenKeyResponse<K>) -> Result<ListenKeyData<K>, KeepaliveFailure> {
    match response {
        ListenKeyResponse::Undefined => Err(type_error(
            "Cannot destructure property 'data' of '(intermediate value)' as it is undefined."
                .into(),
        )),
        ListenKeyResponse::Null => Err(type_error(
            "Cannot destructure property 'data' of '(intermediate value)' as it is null.".into(),
        )),
        ListenKeyResponse::Data(data) => Ok(data),
    }
}
fn listen_key<K: Clone>(data: ListenKeyData<K>) -> Result<Option<K>, KeepaliveFailure> {
    match data {
        ListenKeyData::Undefined => Err(type_error(
            "Cannot read properties of undefined (reading 'listenKey')".into(),
        )),
        ListenKeyData::Null => Err(type_error(
            "Cannot read properties of null (reading 'listenKey')".into(),
        )),
        ListenKeyData::Present(key) => Ok(key.borrow().clone()),
    }
}
fn failure_message(error: &KeepaliveFailure) -> Result<String, KeepaliveFailure> {
    let detail = match error {
        KeepaliveFailure::Error(error) => error.message.clone(),
        KeepaliveFailure::Other(value) => {
            safe_cex_num::js_string(value.as_deref()).map_err(|e| type_error(e.message.into()))?
        }
    };
    Ok(format!(
        "Binance user data stream keepalive failed: {detail}"
    ))
}

// ts: BinancePrivateWebsocket#fetchListenKey
/// POST is the configured xhr.post, including synchronous invocation failure.
/// K::clone must preserve handle identity (e.g. Rc); key is read AFTER scheduling.
pub async fn fetch_binance_listen_key<K: Clone, F>(
    post: impl FnOnce(&'static str) -> Result<F, KeepaliveFailure>,
    schedule: impl FnOnce(f64) -> Result<(), KeepaliveFailure>,
) -> Result<Option<K>, KeepaliveFailure>
where
    F: Future<Output = Result<ListenKeyResponse<K>, KeepaliveFailure>>,
{
    let data = response_data(post("/fapi/v1/listenKey")?.await?)?;
    schedule(1_800_000.0)?;
    listen_key(data)
}

// ts: BinancePrivateWebsocket#updateListenKey
/// Warning callback is configured parent.log(message, LogSeverity.Warning).
/// Fallible synchronous PUT invocation belongs to the same catch as its await;
/// success scheduler failure also enters catch. Disposal is checked only once.
pub async fn update_binance_listen_key<F>(
    state: &KeepaliveState,
    put: impl FnOnce(&'static str) -> Result<F, KeepaliveFailure>,
    mut schedule: impl FnMut(f64) -> Result<(), KeepaliveFailure>,
    warning: impl FnOnce(&str) -> Result<(), KeepaliveFailure>,
    emit_error: impl FnOnce(&str) -> Result<(), KeepaliveFailure>,
) -> Result<(), KeepaliveFailure>
where
    F: Future<Output = Result<(), KeepaliveFailure>>,
{
    if state.is_disposed.get() {
        return Ok(());
    }
    let attempt = async {
        put("/fapi/v1/listenKey")?.await?;
        schedule(1_800_000.0)?;
        state.keepalive_failing.set(false);
        Ok::<(), KeepaliveFailure>(())
    }
    .await;
    if let Err(error) = attempt {
        let message = failure_message(&error)?;
        if state.keepalive_failing.get() {
            warning(&message)?;
        } else {
            emit_error(&message)?;
        }
        state.keepalive_failing.set(true);
        schedule(60_000.0)?;
    }
    Ok(())
}

// ts: BinancePMPrivateWebsocket#fetchListenKey
/// POST is the configured papiXhr.post, including synchronous invocation failure.
/// K::clone must preserve handle identity (e.g. Rc); key is read AFTER scheduling.
pub async fn fetch_binance_pm_listen_key<K: Clone, F>(
    post: impl FnOnce(&'static str) -> Result<F, KeepaliveFailure>,
    schedule: impl FnOnce(f64) -> Result<(), KeepaliveFailure>,
) -> Result<Option<K>, KeepaliveFailure>
where
    F: Future<Output = Result<ListenKeyResponse<K>, KeepaliveFailure>>,
{
    let data = response_data(post("/papi/v1/listenKey")?.await?)?;
    schedule(1_800_000.0)?;
    listen_key(data)
}

// ts: BinancePMPrivateWebsocket#updateListenKey
/// Warning callback is configured parent.log(message, LogSeverity.Warning).
/// Fallible synchronous PUT invocation belongs to the same catch as its await;
/// success scheduler failure also enters catch. Disposal is checked only once.
pub async fn update_binance_pm_listen_key<F>(
    state: &KeepaliveState,
    put: impl FnOnce(&'static str) -> Result<F, KeepaliveFailure>,
    mut schedule: impl FnMut(f64) -> Result<(), KeepaliveFailure>,
    warning: impl FnOnce(&str) -> Result<(), KeepaliveFailure>,
    emit_error: impl FnOnce(&str) -> Result<(), KeepaliveFailure>,
) -> Result<(), KeepaliveFailure>
where
    F: Future<Output = Result<(), KeepaliveFailure>>,
{
    if state.is_disposed.get() {
        return Ok(());
    }
    let attempt = async {
        put("/papi/v1/listenKey")?.await?;
        schedule(1_800_000.0)?;
        state.keepalive_failing.set(false);
        Ok::<(), KeepaliveFailure>(())
    }
    .await;
    if let Err(error) = attempt {
        let message = failure_message(&error)?;
        if state.keepalive_failing.get() {
            warning(&message)?;
        } else {
            emit_error(&message)?;
        }
        state.keepalive_failing.set(true);
        schedule(60_000.0)?;
    }
    Ok(())
}
