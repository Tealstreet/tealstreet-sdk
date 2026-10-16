use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::js_number;
use serde_json::Value;
use std::{future::Future, pin::Pin, rc::Rc};

pub type Failure = Rc<ExchangeError>;
pub type HeaderGetter = Rc<dyn Fn(&str) -> Result<Option<Value>, Failure>>;
pub enum HeaderGetMember {
    Callable(HeaderGetter),
    NotCallable,
}

// ts: BinanceExchange#getResponseHeader
/// Configured header truthiness, get-member reads and property reads. Names are
/// ASCII HTTP header names. get callbacks are bound to the original receiver.
pub fn get_response_header(
    has_headers: bool,
    name: &str,
    mut get_member: impl FnMut() -> Result<HeaderGetMember, Failure>,
    mut property: impl FnMut(&str) -> Result<Option<Value>, Failure>,
) -> Result<Option<Value>, Failure> {
    if !has_headers {
        return Ok(None);
    }
    if matches!(get_member()?, HeaderGetMember::Callable(_)) {
        return match get_member()? {
            HeaderGetMember::Callable(get) => get(name),
            HeaderGetMember::NotCallable => Err(type_error("headers.get is not a function")),
        };
    }
    if let Some(value) = property(name)?.filter(|value| !value.is_null()) {
        return Ok(Some(value));
    }
    if let Some(value) = property(&name.to_ascii_lowercase())?.filter(|value| !value.is_null()) {
        return Ok(Some(value));
    }
    property(&name.to_ascii_uppercase())
}

// ts: BinanceExchange#observeRequestWeight
/// Header expression and pause method are current configured class bindings.
/// Number coercion uses production shared JS semantics on decoded JSON values.
pub fn observe_request_weight(
    status: Option<&Value>,
    mut header: impl FnMut(&str) -> Result<Option<Value>, Failure>,
    mut pause: impl FnMut(f64) -> Result<(), Failure>,
) -> Result<(), Failure> {
    if status
        .and_then(Value::as_f64)
        .is_some_and(|status| status == 429.0 || status == 418.0)
    {
        let value = header("retry-after")?;
        let retry = js_number(value.as_ref()).map_err(|error| type_error(error.message))?;
        pause(if retry.is_finite() && retry > 0.0 {
            retry * 1_000.0
        } else {
            60_000.0
        })?;
        return Ok(());
    }
    let value = header("x-mbx-used-weight-1m")?;
    let used = js_number(value.as_ref()).map_err(|error| type_error(error.message))?;
    if used.is_finite() && used >= 1_920.0 {
        pause(60_000.0)?;
    }
    Ok(())
}

pub type Observer<R> = Rc<dyn Fn(Option<R>) -> Result<(), Failure>>;
pub type ObserveGetter<R> = Rc<dyn Fn() -> Result<Observer<R>, Failure>>;
pub type Fulfilled<R> = Rc<dyn Fn(R) -> Result<R, Failure>>;
pub type RejectedFuture<R, E> = Pin<Box<dyn Future<Output = Result<R, E>>>>;
/// Err is a synchronous callback failure. Ok is the source Promise.reject(error),
/// whose original error is delivered when polled; no scheduler ownership claim.
pub type Rejected<R, E> = Rc<dyn Fn(E) -> Result<RejectedFuture<R, E>, Failure>>;

// ts: BinanceExchange#installRequestWeightGuards
/// Dense array of client objects; identity deduplication precedes registration.
/// Observe is resolved at callback invocation, before reading error.response.
pub fn install_request_weight_guards<C, R: Clone + 'static, E: 'static>(
    clients: &[Rc<C>],
    get_observe: ObserveGetter<R>,
    error_response: Rc<dyn Fn(&E) -> Result<Option<R>, Failure>>,
    mut register: impl FnMut(&Rc<C>, Fulfilled<R>, Rejected<R, E>) -> Result<(), Failure>,
) -> Result<(), Failure> {
    let mut unique: Vec<Rc<C>> = Vec::new();
    for client in clients {
        if !unique.iter().any(|seen| Rc::ptr_eq(seen, client)) {
            unique.push(client.clone());
        }
    }
    for client in unique {
        let observe = get_observe.clone();
        let fulfilled: Fulfilled<R> = Rc::new(move |response| {
            observe()?(Some(response.clone()))?;
            Ok(response)
        });
        let observe = get_observe.clone();
        let read_response = error_response.clone();
        let rejected: Rejected<R, E> = Rc::new(move |error| {
            let observe = observe()?;
            observe(read_response(&error)?)?;
            Ok(Box::pin(async move { Err(error) }))
        });
        register(&client, fulfilled, rejected)?;
    }
    Ok(())
}

pub(crate) fn type_error(message: impl Into<String>) -> Failure {
    let mut error = ExchangeError::new(message);
    error.name = "TypeError".into();
    Rc::new(error)
}
