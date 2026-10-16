//! Inert authentication and local Promise plumbing for the Starbase client.
use crate::{
    protocol::COINBASE_STARBASE_AUTH_REFRESH_AFTER_MS,
    starbase::{
        CoinbaseStarbaseProtocolError, CoinbaseStarbaseTransportKind, validate_auth_result,
    },
    starbase_rpc::{
        CoinbaseStarbaseJsonRpcClient, CoinbaseStarbaseJsonRpcParams, CoinbaseStarbaseRequestError,
    },
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, Ref, RefCell, RefMut},
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
    time::{SystemTime, UNIX_EPOCH},
};

pub type CoinbaseStarbaseLocalFuture<T> = Pin<Box<dyn Future<Output = T>>>;
/// Host local task boundary: poll new producer tasks synchronously to their first suspension,
/// then enqueue wakes as FIFO microtasks; never resume inline during wake. Drive tasks even
/// when their returned completion Promise is not awaited.
pub type CoinbaseStarbasePromiseSpawner = Rc<dyn Fn(CoinbaseStarbaseLocalFuture<()>)>;
#[derive(Debug)]
pub enum CoinbaseStarbaseSessionError<E> {
    Request(CoinbaseStarbaseRequestError<E>),
    Coercion(safe_cex_num::JsCoercionError),
}
pub type CoinbaseStarbaseSessionFailure<E> = Rc<CoinbaseStarbaseSessionError<E>>;
enum PromiseReaction<T> {
    Observe(Box<dyn FnOnce(T)>),
    Await(std::task::Waker),
}
struct PromiseState<T> {
    value: Option<T>,
    reactions: Vec<PromiseReaction<T>>,
}
// Completion receiver only: polling cannot drive the producer. Every new await of an
// already fulfilled Promise still suspends once, as JavaScript await does.
pub struct CoinbaseStarbasePromise<T: Clone> {
    state: Rc<RefCell<PromiseState<T>>>,
    await_started: bool,
}
pub struct CoinbaseStarbasePromiseResolver<T: Clone>(Rc<RefCell<PromiseState<T>>>);
impl<T: Clone> Clone for CoinbaseStarbasePromise<T> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            await_started: false,
        }
    }
}
impl<T: Clone> Clone for CoinbaseStarbasePromiseResolver<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T: Clone> Future for CoinbaseStarbasePromise<T> {
    type Output = T;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        let started = self.await_started;
        self.await_started = true;
        let mut state = self.state.borrow_mut();
        if let Some(value) = &state.value {
            if !started {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            return Poll::Ready(value.clone());
        }
        if !state.reactions.iter().any(|reaction| matches!(reaction, PromiseReaction::Await(waiter) if waiter.will_wake(cx.waker()))) {
            state.reactions.push(PromiseReaction::Await(cx.waker().clone()));
        }
        Poll::Pending
    }
}
impl<T: Clone + 'static> CoinbaseStarbasePromise<T> {
    pub fn pending() -> (Self, CoinbaseStarbasePromiseResolver<T>) {
        let state = Rc::new(RefCell::new(PromiseState {
            value: None,
            reactions: Vec::new(),
        }));
        (
            Self {
                state: state.clone(),
                await_started: false,
            },
            CoinbaseStarbasePromiseResolver(state),
        )
    }
    pub fn resolved(value: T) -> Self {
        let (promise, resolve) = Self::pending();
        resolve.resolve(value);
        promise
    }
    pub fn same_promise(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.state, &other.state)
    }
    pub(crate) fn from_future(
        future: CoinbaseStarbaseLocalFuture<T>,
        spawn: &CoinbaseStarbasePromiseSpawner,
    ) -> Self {
        let (promise, resolve) = Self::pending();
        spawn(Box::pin(async move {
            resolve.resolve(future.await);
        }));
        promise
    }
    pub(crate) fn observe(
        &self,
        spawn: &CoinbaseStarbasePromiseSpawner,
        callback: impl FnOnce(T) + 'static,
    ) {
        let spawn = spawn.clone();
        let queued: Box<dyn FnOnce(T)> =
            Box::new(move |value| queue_job(&spawn, move || callback(value)));
        let value = self.state.borrow().value.clone();
        if let Some(value) = value {
            queued(value);
        } else {
            self.state
                .borrow_mut()
                .reactions
                .push(PromiseReaction::Observe(queued));
        }
    }
}
impl<T: Clone> CoinbaseStarbasePromiseResolver<T> {
    pub fn resolve(&self, value: T) {
        let reactions = {
            let mut state = self.0.borrow_mut();
            if state.value.is_some() {
                return;
            }
            state.value = Some(value.clone());
            std::mem::take(&mut state.reactions)
        };
        for reaction in reactions {
            match reaction {
                PromiseReaction::Observe(observer) => observer(value.clone()),
                PromiseReaction::Await(waiter) => waiter.wake(),
            }
        }
    }
}
struct MicrotaskJob {
    queued: bool,
    callback: Option<Box<dyn FnOnce()>>,
}
impl Future for MicrotaskJob {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if !self.queued {
            self.queued = true;
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        self.callback.take().expect("one microtask invocation")();
        Poll::Ready(())
    }
}
pub(crate) fn queue_job(spawn: &CoinbaseStarbasePromiseSpawner, callback: impl FnOnce() + 'static) {
    spawn(Box::pin(MicrotaskJob {
        queued: false,
        callback: Some(Box::new(callback)),
    }));
}

#[derive(Debug, Clone)]
pub struct CoinbaseStarbaseAuthResultHandle(Rc<RefCell<Value>>);
impl CoinbaseStarbaseAuthResultHandle {
    fn new(value: Value) -> Self {
        Self(Rc::new(RefCell::new(value)))
    }
    pub fn borrow(&self) -> Ref<'_, Value> {
        self.0.borrow()
    }
    pub fn borrow_mut(&self) -> RefMut<'_, serde_json::Map<String, Value>> {
        RefMut::map(self.0.borrow_mut(), |value| {
            value.as_object_mut().expect("validated auth record")
        })
    }
    pub fn shares_result(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
pub type CoinbaseStarbaseAuthPromise<E> = CoinbaseStarbasePromise<
    Result<CoinbaseStarbaseAuthResultHandle, CoinbaseStarbaseSessionFailure<E>>,
>;
// Runtime overlay for Number.isFinite, including non-finite JS numbers that JSON cannot encode.
#[derive(Debug, Clone)]
pub enum CoinbaseStarbaseRefreshInterval {
    Number(f64),
    Json(Value),
}
// ts: CoinbaseStarbaseCdpJwtProvider
pub type CoinbaseStarbaseCdpJwtProvider<E> =
    Rc<dyn Fn() -> CoinbaseStarbasePromise<Result<String, CoinbaseStarbaseSessionFailure<E>>>>;
pub type CoinbaseStarbaseClock<E> = Rc<dyn Fn() -> Result<f64, CoinbaseStarbaseSessionFailure<E>>>;

pub(crate) fn protocol_failure<E>(message: &str) -> CoinbaseStarbaseSessionFailure<E> {
    Rc::new(CoinbaseStarbaseSessionError::Request(
        CoinbaseStarbaseRequestError::Protocol(CoinbaseStarbaseProtocolError::new(message)),
    ))
}
pub(crate) fn request_failure<E>(
    error: CoinbaseStarbaseRequestError<E>,
) -> CoinbaseStarbaseSessionFailure<E> {
    Rc::new(CoinbaseStarbaseSessionError::Request(error))
}
fn coercion_failure<E>(error: safe_cex_num::JsCoercionError) -> CoinbaseStarbaseSessionFailure<E> {
    Rc::new(CoinbaseStarbaseSessionError::Coercion(error))
}
fn js_min(left: f64, right: f64) -> f64 {
    if left.is_nan() || right.is_nan() {
        f64::NAN
    } else if left == 0.0 && right == 0.0 && (left.is_sign_negative() || right.is_sign_negative()) {
        -0.0
    } else {
        left.min(right)
    }
}
fn date_now() -> f64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(value) => value.as_millis() as f64,
        Err(error) => -(error.duration().as_millis() as f64),
    }
}
struct AuthState<E> {
    authenticated_at: Cell<f64>,
    auth_result: RefCell<Option<CoinbaseStarbaseAuthResultHandle>>,
    client: Rc<CoinbaseStarbaseJsonRpcClient<E>>,
    clock: CoinbaseStarbaseClock<E>,
    create_cdp_jwt: CoinbaseStarbaseCdpJwtProvider<E>,
    refresh_after_ms: f64,
    spawn: CoinbaseStarbasePromiseSpawner,
}
// ts: CoinbaseStarbaseAuthSession
pub struct CoinbaseStarbaseAuthSession<E>(Rc<AuthState<E>>);
impl<E> Clone for CoinbaseStarbaseAuthSession<E> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<E: 'static> CoinbaseStarbaseAuthSession<E> {
    // ts: CoinbaseStarbaseAuthSession.constructor
    pub fn new(
        client: Rc<CoinbaseStarbaseJsonRpcClient<E>>,
        create_cdp_jwt: CoinbaseStarbaseCdpJwtProvider<E>,
        clock: Option<CoinbaseStarbaseClock<E>>,
        refresh_after_ms: Option<&CoinbaseStarbaseRefreshInterval>,
        spawn: CoinbaseStarbasePromiseSpawner,
    ) -> Result<Self, CoinbaseStarbaseProtocolError> {
        let key = match client.kind {
            CoinbaseStarbaseTransportKind::Http => "http",
            CoinbaseStarbaseTransportKind::Websocket => "websocket",
        };
        let default = COINBASE_STARBASE_AUTH_REFRESH_AFTER_MS
            .iter()
            .find(|(kind, _)| *kind == key)
            .expect("transport policy")
            .1 as f64;
        let refresh_after_ms = match refresh_after_ms {
            None | Some(CoinbaseStarbaseRefreshInterval::Json(Value::Null)) => default,
            Some(CoinbaseStarbaseRefreshInterval::Number(value)) => *value,
            Some(CoinbaseStarbaseRefreshInterval::Json(value)) => {
                value.as_f64().unwrap_or(f64::NAN)
            }
        };
        if !refresh_after_ms.is_finite() || refresh_after_ms <= 0.0 {
            return Err(CoinbaseStarbaseProtocolError::new(
                "Auth refresh interval must be positive",
            ));
        }
        Ok(Self(Rc::new(AuthState {
            authenticated_at: Cell::new(0.0),
            auth_result: RefCell::new(None),
            client,
            clock: clock.unwrap_or_else(|| Rc::new(|| Ok(date_now()))),
            create_cdp_jwt,
            refresh_after_ms,
            spawn,
        })))
    }
    // ts: CoinbaseStarbaseAuthSession#clock
    fn clock(&self) -> Result<f64, CoinbaseStarbaseSessionFailure<E>> {
        (self.0.clock)()
    }
    // ts: CoinbaseStarbaseAuthSession#createCdpJwt
    fn create_cdp_jwt(
        &self,
    ) -> CoinbaseStarbasePromise<Result<String, CoinbaseStarbaseSessionFailure<E>>> {
        (self.0.create_cdp_jwt)()
    }
    // ts: CoinbaseStarbaseAuthSession.authenticate
    pub fn authenticate(&self, force: bool) -> CoinbaseStarbaseAuthPromise<E> {
        // The clock, cache decision and provider callback run before the first await.
        let prepared = (|| {
            let now = self.clock()?;
            let cached = self.0.auth_result.borrow().clone();
            let response_expires_at = match &cached {
                Some(result) => {
                    self.0.authenticated_at.get()
                        + safe_cex_num::js_number(result.borrow().get("expires_in"))
                            .map_err(coercion_failure)?
                            * 1000.0
                }
                None => 0.0,
            };
            let policy_expires_at = self.0.authenticated_at.get() + self.0.refresh_after_ms;
            let refresh_at = js_min(response_expires_at, policy_expires_at);
            if !force && cached.is_some() && now < refresh_at {
                return Ok((now, cached, None));
            }
            Ok((now, None, Some(self.create_cdp_jwt())))
        })();
        match prepared {
            Err(error) => CoinbaseStarbasePromise::resolved(Err(error)),
            Ok((_, Some(cached), _)) => CoinbaseStarbasePromise::resolved(Ok(cached)),
            Ok((now, _, Some(token))) => {
                let (promise, complete) = CoinbaseStarbasePromise::pending();
                let state = self.0.clone();
                token.observe(&self.0.spawn, move |token| {
                    let token = match token {
                        Err(error) => {
                            complete.resolve(Err(error));
                            return;
                        }
                        Ok(token) if token.is_empty() => {
                            complete.resolve(Err(protocol_failure(
                                "CDP JWT provider returned an empty token",
                            )));
                            return;
                        }
                        Ok(token) => token,
                    };
                    let response = state.client.request_promise(
                        Some(&json!("public/auth")),
                        CoinbaseStarbaseJsonRpcParams::new(
                            json!({"grant_type":"coinbase_cdp","token":token}),
                        ),
                        None,
                        &state.spawn,
                    );
                    let spawn = state.spawn.clone();
                    response.observe(&spawn, move |result| {
                        let result = result.and_then(|result| {
                            validate_auth_result(Some(&result))
                                .map_err(|error| request_failure(error.into()))?;
                            let result = CoinbaseStarbaseAuthResultHandle::new(result);
                            *state.auth_result.borrow_mut() = Some(result.clone());
                            state.authenticated_at.set(now);
                            Ok(result)
                        });
                        complete.resolve(result);
                    });
                });
                promise
            }
            _ => unreachable!("prepared cache or provider"),
        }
    }
    // ts: CoinbaseStarbaseAuthSession.getHttpAuthorization
    pub fn get_http_authorization(
        &self,
        force_refresh: bool,
    ) -> CoinbaseStarbasePromise<Result<String, CoinbaseStarbaseSessionFailure<E>>> {
        if self.0.client.kind != CoinbaseStarbaseTransportKind::Http {
            return CoinbaseStarbasePromise::resolved(Err(protocol_failure(
                "Bearer authorization headers apply only to Starbase HTTP",
            )));
        }
        let (promise, complete) = CoinbaseStarbasePromise::pending();
        self.authenticate(force_refresh)
            .observe(&self.0.spawn, move |result| {
                complete.resolve(result.and_then(|result| {
                    let token = safe_cex_num::js_string(result.borrow().get("access_token"))
                        .map_err(coercion_failure)?;
                    Ok(format!("Bearer {token}"))
                }));
            });
        promise
    }
}
