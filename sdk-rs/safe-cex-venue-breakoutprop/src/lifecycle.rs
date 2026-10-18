//! Concrete DXtrade exchange ownership and shared session-token acquisition.
//! API construction/rate limiting and inherited polling-socket operations are
//! required dependencies; the API's interceptors have their own parity row.
use crate::{
    exchange::parse_dx_session_timeout_ms,
    polling::{
        BookCallback, BreakoutpropPublicWebsocket, CandleCallback, HandlerTable, PollingAncestor,
        Unsubscribe,
    },
    types::{GRAFANA_ENDPOINTS_EVERY, GRAFANA_ENDPOINTS_PERIODIC, LoginResponse},
};
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::BaseExchange,
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_num::numeric_ingress::JsNumberInput;
use safe_cex_store::Store;
use safe_cex_transport::provider::RestRequest;
use safe_cex_types::{ExchangeName, ExchangeOptions, OHLCVOptions};
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

pub trait DxClient {
    fn rate_limit(&self, max_rps: f64) -> Result<(), ExchangeError>;
    fn request(&self, request: RestRequest) -> ExchangeFuture<'static, JsNumberInput>;
}
pub trait BreakoutHost {
    fn runtime_host(&self) -> Rc<dyn crate::native_reads::NativeRuntimeHost>;
    fn construct_base(
        &self,
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
    ) -> Result<BaseExchange, ExchangeError>;
    fn create_api(
        &self,
        exchange: Rc<BreakoutpropExchange>,
    ) -> Result<Rc<dyn DxClient>, ExchangeError>;
    fn rate_limit(
        &self,
        client: Rc<dyn DxClient>,
        max_rps: f64,
    ) -> Result<Rc<dyn DxClient>, ExchangeError>;
    fn construct_public_ancestor(
        &self,
        exchange: Rc<BreakoutpropExchange>,
        handlers: HandlerTable,
    ) -> Result<Rc<dyn PollingAncestor>, ExchangeError>;
    /// Retain and drive the login even if every caller drops its returned future.
    fn spawn_login(&self, future: ExchangeFuture<'static, ()>);
    fn report_abandoned_login(&self, error: &ExchangeError);
}
pub struct BreakoutpropExchange {
    pub base: Rc<BaseExchange>,
    pub host: Rc<dyn BreakoutHost>,
    pub xhr: RefCell<Option<Rc<dyn DxClient>>>,
    pub unlimited_xhr: RefCell<Option<Rc<dyn DxClient>>>,
    pub public_websocket: RefCell<Option<Rc<BreakoutpropPublicWebsocket>>>,
    pub grafana_endpoints_every: &'static [&'static str],
    pub grafana_endpoints_periodic: &'static [&'static str],
    pub session_token: RefCell<Option<String>>,
    pub session_token_expires_at: Cell<f64>,
    pub session_token_request: RefCell<Option<Rc<LoginTask>>>,
    pub account_code: RefCell<Option<String>>,
    pub recent_ws_requests: RefCell<Vec<(String, (f64, Vec<safe_cex_types::Candle>))>>,
    pub(crate) caps: ExchangeCapabilities,
    pub(crate) binding: RefCell<Option<safe_cex_exchange::js_runtime::value::Js>>,
}
impl BreakoutpropExchange {
    // ts: constructor. Assign rate-limited API, independent unlimited API, then socket.
    pub fn new(
        host: Rc<dyn BreakoutHost>,
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
    ) -> Result<Rc<Self>, ExchangeError> {
        let base = host.construct_base(name, options, store)?;
        let exchange = Rc::new(Self {
            base: Rc::new(base),
            host: host.clone(),
            xhr: RefCell::new(None),
            unlimited_xhr: RefCell::new(None),
            public_websocket: RefCell::new(None),
            grafana_endpoints_every: GRAFANA_ENDPOINTS_EVERY,
            grafana_endpoints_periodic: GRAFANA_ENDPOINTS_PERIODIC,
            session_token: RefCell::new(None),
            session_token_expires_at: Cell::new(0.),
            session_token_request: RefCell::new(None),
            account_code: RefCell::new(None),
            recent_ws_requests: RefCell::new(vec![]),
            caps: crate::binding::capabilities(),
            binding: RefCell::new(None),
        });
        let client = host.create_api(exchange.clone())?;
        let limited = host.rate_limit(client, 5.)?;
        *exchange.xhr.borrow_mut() = Some(limited);
        let unlimited = host.create_api(exchange.clone())?;
        *exchange.unlimited_xhr.borrow_mut() = Some(unlimited);
        let socket = BreakoutpropPublicWebsocket::new(|handlers| {
            host.construct_public_ancestor(exchange.clone(), handlers)
        })?;
        *exchange.public_websocket.borrow_mut() = Some(socket);
        *exchange.binding.borrow_mut() = Some(crate::binding::instance(&exchange)?);
        Ok(exchange)
    }
    pub fn dispose(&self) -> Result<(), ExchangeError> {
        let disposed = self.base.dispose();
        let login = self.session_token_request.borrow_mut().take();
        if let Some(login) = login.filter(|login| login.result.borrow().is_none()) {
            let mut error = ExchangeError::new("Breakout Prop login cancelled after disposal");
            error.name = "AbortError".into();
            *login.future.borrow_mut() = None;
            *login.result.borrow_mut() = Some(Err(error));
            for waiter in std::mem::take(&mut *login.waiters.borrow_mut()) {
                waiter.wake();
            }
        }
        disposed?;
        self.socket_for("dispose")?.dispose()
    }
    pub fn socket(&self) -> Result<Rc<BreakoutpropPublicWebsocket>, ExchangeError> {
        self.socket_for("socket")
    }
    fn socket_for(&self, member: &str) -> Result<Rc<BreakoutpropPublicWebsocket>, ExchangeError> {
        self.public_websocket.borrow().clone().ok_or_else(|| {
            let mut error = ExchangeError::new(format!(
                "Cannot read properties of undefined (reading '{member}')"
            ));
            error.name = "TypeError".into();
            error
        })
    }
    pub fn listen_ohlcv(
        self: &Rc<Self>,
        opts: OHLCVOptions,
        callback: CandleCallback,
    ) -> Result<Unsubscribe, ExchangeError> {
        Ok(self.socket_for("listenOHLCV")?.listen_ohlcv(opts, callback))
    }
    pub fn listen_order_book(
        self: &Rc<Self>,
        symbol: String,
        callback: BookCallback,
    ) -> Result<Unsubscribe, ExchangeError> {
        Ok(self
            .socket_for("listenOrderBook")?
            .listen_order_book(symbol, callback))
    }
    pub fn invalidate_session(&self) {
        let mut expires_at = self.session_token_expires_at.get();
        crate::account_auth::invalidate_session(
            &mut self.session_token.borrow_mut(),
            &mut expires_at,
        );
        self.session_token_expires_at.set(expires_at);
    }
    fn live_token(&self) -> Result<String, ExchangeError> {
        self.session_token
            .borrow()
            .clone()
            .ok_or_else(|| ExchangeError::new("DXtrade token became null during clock callback"))
    }
    // ts: getSessionToken. Fast cached return, one owned login, per-caller finally.
    pub fn get_session_token(self: &Rc<Self>) -> ExchangeFuture<'static, String> {
        let has_token = self
            .session_token
            .borrow()
            .as_ref()
            .is_some_and(|token| !token.is_empty());
        if has_token && self.base.runtime.now_ms() < self.session_token_expires_at.get() {
            let token = self.live_token();
            return Box::pin(async move { token });
        }
        let existing = self.session_token_request.borrow().clone();
        let task = if let Some(existing) = existing {
            existing
        } else {
            let pending: Result<ExchangeFuture<'static, JsNumberInput>, ExchangeError> = (|| {
                let client = self
                    .xhr
                    .borrow()
                    .clone()
                    .ok_or_else(|| ExchangeError::new("DXtrade API construction incomplete"))?;
                let options = self.base.options();
                Ok(client.request(RestRequest {
                    method: "POST".into(), url: "/login".into(),
                    body: Some(safe_cex_transport::provider::RestBody::Untransformed(safe_cex_types::retained_json::RetainedJsonInput::from_value(json!({"domain":"default","username":options.key,"password":options.secret})).map_err(|error|ExchangeError::new(error.to_string()))?)),
                    ..Default::default()
                }))
            })(
            );
            let owner = self.clone();
            let future = Box::pin(async move {
                let input = pending?.await?;
                let data: LoginResponse = input
                    .decode()
                    .map_err(|e| ExchangeError::new(e.to_string()))?;
                if data.session_token.is_empty() {
                    return Err(ExchangeError::new("No session token received"));
                }
                let timeout =
                    parse_dx_session_timeout_ms(&data.timeout).map_err(ExchangeError::new)?;
                *owner.session_token.borrow_mut() = Some(data.session_token.clone());
                owner
                    .session_token_expires_at
                    .set(owner.base.runtime.now_ms() + timeout);
                owner.live_token()
            });
            let task = Rc::new(LoginTask {
                future: RefCell::new(Some(future)),
                result: RefCell::new(None),
                waiters: RefCell::new(vec![]),
            });
            *self.session_token_request.borrow_mut() = Some(task.clone());
            task
        };
        let call = Rc::new(TokenCall {
            owner: self.clone(),
            task,
            finally_done: Cell::new(false),
            abandoned: Cell::new(false),
            reported: Cell::new(false),
            observed: Cell::new(false),
        });
        let driven = call.clone();
        self.host
            .spawn_login(Box::pin(std::future::poll_fn(move |cx| {
                match driven.poll_result(cx) {
                    Poll::Pending => Poll::Pending,
                    // This driver settles an owned caller, not the business result.
                    // The returned awaiter retains the original Ok/Err on LoginTask.
                    Poll::Ready(_) => Poll::Ready(Ok(())),
                }
            })));
        Box::pin(TokenAwaiter { call })
    }
}
pub struct LoginTask {
    future: RefCell<Option<ExchangeFuture<'static, String>>>,
    result: RefCell<Option<Result<String, ExchangeError>>>,
    waiters: RefCell<Vec<Waker>>,
}
impl LoginTask {
    fn poll_result(&self, cx: &mut Context<'_>) -> Poll<Result<String, ExchangeError>> {
        if let Some(result) = self.result.borrow().clone() {
            return Poll::Ready(result);
        }
        {
            let mut waiters = self.waiters.borrow_mut();
            if !waiters.iter().any(|w| w.will_wake(cx.waker())) {
                waiters.push(cx.waker().clone());
            }
        }
        let pending = self.future.borrow_mut().take();
        if let Some(mut future) = pending {
            match future.as_mut().poll(cx) {
                Poll::Pending => *self.future.borrow_mut() = Some(future),
                Poll::Ready(result) => {
                    *self.result.borrow_mut() = Some(result.clone());
                    let waiters = std::mem::take(&mut *self.waiters.borrow_mut());
                    for waiter in waiters {
                        waiter.wake();
                    }
                    return Poll::Ready(result);
                }
            }
        }
        Poll::Pending
    }
}
struct TokenCall {
    owner: Rc<BreakoutpropExchange>,
    task: Rc<LoginTask>,
    finally_done: Cell<bool>,
    abandoned: Cell<bool>,
    reported: Cell<bool>,
    observed: Cell<bool>,
}
impl TokenCall {
    fn poll_result(&self, cx: &mut Context<'_>) -> Poll<Result<String, ExchangeError>> {
        let result = self.task.poll_result(cx);
        if result.is_ready() && !self.finally_done.replace(true) {
            // Source finally clears unconditionally, including an older call.
            *self.owner.session_token_request.borrow_mut() = None;
        }
        if let Poll::Ready(Err(error)) = &result {
            if self.abandoned.get() && !self.reported.replace(true) {
                self.owner.host.report_abandoned_login(error);
            }
        }
        result
    }
    fn abandon(&self) {
        self.abandoned.set(true);
        let error = self
            .task
            .result
            .borrow()
            .as_ref()
            .and_then(|r| r.as_ref().err())
            .cloned();
        if let Some(error) = error {
            if !self.reported.replace(true) {
                self.owner.host.report_abandoned_login(&error);
            }
        }
    }
}
struct TokenAwaiter {
    call: Rc<TokenCall>,
}
impl Drop for TokenAwaiter {
    fn drop(&mut self) {
        if !self.call.observed.get() {
            self.call.abandon();
        }
    }
}
impl Future for TokenAwaiter {
    type Output = Result<String, ExchangeError>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let result = self.call.poll_result(cx);
        if result.is_ready() {
            self.call.observed.set(true);
        }
        result
    }
}
