//! Account method bodies over the current receiver and ECMAScript intrinsics.
//! The factory, classes, HTTP transport and clob-client order serializer are
//! separately bound dependencies; this module does not claim their implementation.
use crate::{order_adapters::Completion, ws_dispatch::Runtime};
use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

/// Captured child receiver/callable, acquired before evaluating call arguments.
pub trait AccountHost: Runtime {
    type Callable;
    type Operation: Completion<Self::Value, Self::Error>;
    /// Own and eagerly start the whole async body, retaining await continuations
    /// after its caller drops the returned observer.
    fn start_task(&self, task: Pin<Box<dyn Future<Output = ()>>>);
    fn capture(
        &self,
        receiver: &'static str,
        method: &'static str,
    ) -> Result<Self::Callable, Self::Error>;
    fn invoke(
        &self,
        callable: Self::Callable,
        args: Vec<Self::Value>,
    ) -> Result<Rc<Self::Operation>, Self::Error>;
    /// Synchronous invocation of a captured logger, after its arguments evaluate.
    fn invoke_log(
        &self,
        callable: Self::Callable,
        message: &[u16],
        severity: &'static str,
    ) -> Result<(), Self::Error>;
    /// Async return resolution including thenable assimilation.
    fn resolve(&self, value: Self::Value) -> Result<Rc<Self::Operation>, Self::Error>;
    fn credentials(&self) -> Result<Self::Value, Self::Error>;
    fn wallet(&self) -> Result<Self::Value, Self::Error>;
    fn exchange_credentials(&self) -> Result<Self::Value, Self::Error>;
    fn signer_type(&self) -> Result<Self::Value, Self::Error>;
    fn text(&self, text: &'static str) -> Self::Value;
    fn number(&self, value: f64) -> Self::Value;
    fn null(&self) -> Self::Value;
    fn object(&self, fields: Vec<(&'static str, Self::Value)>) -> Result<Self::Value, Self::Error>;
    /// Ordinary property Set, including inherited setters, distinct from literal creation.
    fn assign(
        &self,
        object: &Self::Value,
        key: &'static str,
        value: Self::Value,
    ) -> Result<(), Self::Error>;
    /// CopyDataProperties followed by the two overrides, with ordinary property order.
    fn spread_params(
        &self,
        source: &Self::Value,
        limit: Self::Value,
        offset: Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn array(&self, slots: Vec<Option<Self::Value>>) -> Self::Value;
    /// Array.map's species allocation occurs before callbacks; length is already captured.
    fn map_output(&self, input: &Self::Value, length: usize) -> Result<Self::Value, Self::Error>;
    /// Array.map's CreateDataPropertyOrThrow, distinct from ordinary assignment.
    fn map_set(
        &self,
        output: &Self::Value,
        index: usize,
        value: Self::Value,
    ) -> Result<(), Self::Error>;
    fn is_array(&self, value: &Self::Value) -> bool;
    fn length(&self, value: &Self::Value) -> Result<usize, Self::Error>;
    fn at(&self, value: &Self::Value, index: usize) -> Result<Option<Self::Value>, Self::Error>;
    /// Array.push with spread's iterator semantics, including holes as undefined.
    fn push_spread(&self, array: &Self::Value, iterable: &Self::Value) -> Result<(), Self::Error>;
    fn less(&self, left: &Self::Value, right: &Self::Value) -> Result<bool, Self::Error>;
    fn greater_equal(&self, left: &Self::Value, right: &Self::Value) -> Result<bool, Self::Error>;
    fn add(&self, left: &Self::Value, right: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn slice(&self, array: &Self::Value, end: &Self::Value) -> Result<Self::Value, Self::Error>;
    /// Synchronous bound clob-client-v2 orderToJsonV2 dependency, not reimplemented.
    fn order_to_json(
        &self,
        order: Self::Value,
        owner: Self::Value,
        kind: Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn new_error(&self, message: &'static str) -> Self::Error;
    fn error_message(&self, error: &Self::Error) -> Result<Self::Value, Self::Error>;
}
struct State<V, E> {
    result: Option<Result<V, E>>,
    waker: Option<Waker>,
}
pub(crate) struct Observer<V, E>(Rc<RefCell<State<V, E>>>);
impl<V, E> Future for Observer<V, E> {
    type Output = Result<V, E>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.0.borrow_mut();
        if let Some(result) = state.result.take() {
            Poll::Ready(result)
        } else {
            state.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}
pub(crate) fn start<H: AccountHost>(
    host: &H,
    body: impl Future<Output = Result<H::Value, H::Error>> + 'static,
) -> Observer<H::Value, H::Error> {
    let state = Rc::new(RefCell::new(State {
        result: None,
        waker: None,
    }));
    let out = state.clone();
    host.start_task(Box::pin(async move {
        let result = body.await;
        let waker = {
            let mut s = out.borrow_mut();
            s.result = Some(result);
            s.waker.take()
        };
        if let Some(w) = waker {
            w.wake();
        }
    }));
    Observer(state)
}
struct Await<O, V, E> {
    op: Rc<O>,
    yielded: bool,
    marker: std::marker::PhantomData<fn() -> (V, E)>,
}
impl<O: Completion<V, E>, V, E> Future for Await<O, V, E> {
    type Output = Result<V, E>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let s = self.get_mut();
        if !s.yielded {
            s.yielded = true;
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        s.op.poll(cx)
    }
}
pub(crate) async fn wait<O: Completion<V, E>, V, E>(op: Result<Rc<O>, E>) -> Result<V, E> {
    Await {
        op: op?,
        yielded: false,
        marker: std::marker::PhantomData,
    }
    .await
}
pub(crate) async fn returned<H: AccountHost>(
    host: &H,
    value: H::Value,
) -> Result<H::Value, H::Error> {
    Await {
        op: host.resolve(value)?,
        yielded: true,
        marker: std::marker::PhantomData,
    }
    .await
}
async fn get<H: AccountHost>(
    host: &H,
    call: H::Callable,
    url: &'static str,
    params: H::Value,
) -> Result<H::Value, H::Error> {
    let config = host.object(vec![("params", params)])?;
    let response = wait(host.invoke(call, vec![host.text(url), config])).await?;
    host.field(&response, "data")
}
/// PolymarketAPI.postOrder: credentials precede serializer and xhr lookup.
pub fn post_order<H: AccountHost>(
    host: Rc<H>,
    order: H::Value,
    kind: H::Value,
) -> impl Future<Output = Result<H::Value, H::Error>> {
    start(host.as_ref(), {
        let host = host.clone();
        async move {
            let credentials = host.credentials()?;
            if !host.truthy(&credentials) {
                return Err(host.new_error("Credentials not initialized"));
            }
            let payload = host.order_to_json(order, host.field(&credentials, "apiKey")?, kind)?;
            let call = host.capture("xhr", "post")?;
            let response = wait(host.invoke(call, vec![host.text("/order"), payload])).await?;
            returned(host.as_ref(), host.field(&response, "data")?).await
        }
    })
}
/// PolymarketAPI.postOrders: Array.map length snapshot/live slots and fresh rows.
pub fn post_orders<H: AccountHost>(
    host: Rc<H>,
    orders: H::Value,
) -> impl Future<Output = Result<H::Value, H::Error>> {
    start(host.as_ref(), {
        let host = host.clone();
        async move {
            let credentials = host.credentials()?;
            if !host.truthy(&credentials) {
                return Err(host.new_error("Credentials not initialized"));
            }
            let call = host.capture("xhr", "post")?;
            let length = host.length(&orders)?;
            let mapped = host.map_output(&orders, length)?;
            for index in 0..length {
                if let Some(o) = host.at(&orders, index)? {
                    let row = host.object(vec![
                        ("order", host.field(&o, "order")?),
                        ("owner", host.field(&credentials, "apiKey")?),
                        ("orderType", host.field(&o, "orderType")?),
                    ])?;
                    host.map_set(&mapped, index, row)?;
                }
            }
            let payload = host.object(vec![("orders", mapped)])?;
            let response = wait(host.invoke(call, vec![host.text("/orders"), payload])).await?;
            returned(host.as_ref(), host.field(&response, "data")?).await
        }
    })
}
/// PolymarketAPI.getOpenOrders, including direct-array identity and cursor fallback.
pub fn get_open_orders<H: AccountHost>(
    host: Rc<H>,
    market: H::Value,
) -> impl Future<Output = Result<H::Value, H::Error>> {
    start(host.as_ref(), {
        let host = host.clone();
        async move {
            let orders = host.array(vec![]);
            let mut cursor = host.text("MA==");
            while !host.is_text(&cursor, "LTE=") {
                let params = host.object(vec![("next_cursor", cursor)])?;
                if host.truthy(&market) {
                    host.assign(&params, "market", market.clone())?;
                }
                let call = host.capture("xhr", "get")?;
                let data = get(host.as_ref(), call, "/data/orders", params).await?;
                if host.is_array(&data) {
                    return returned(host.as_ref(), data).await;
                }
                let rows = host.field(&data, "data")?;
                let rows = if host.truthy(&rows) {
                    rows
                } else {
                    host.array(vec![])
                };
                host.push_spread(&orders, &rows)?;
                let next = host.field(&data, "next_cursor")?;
                cursor = if host.truthy(&next) {
                    next
                } else {
                    host.text("LTE=")
                };
            }
            returned(host.as_ref(), orders).await
        }
    })
}
/// PolymarketAPI.getTrades: direct arrays bypass limit; envelopes truncate after append.
pub fn get_trades<H: AccountHost>(
    host: Rc<H>,
    market: H::Value,
    limit: H::Value,
) -> impl Future<Output = Result<H::Value, H::Error>> {
    start(host.as_ref(), {
        let host = host.clone();
        async move {
            let trades = host.array(vec![]);
            let mut cursor = host.text("MA==");
            while !host.is_text(&cursor, "LTE=") {
                let params = host.object(vec![("next_cursor", cursor)])?;
                if host.truthy(&market) {
                    host.assign(&params, "market", market.clone())?;
                }
                if host.truthy(&limit) {
                    host.assign(&params, "limit", limit.clone())?;
                }
                let call = host.capture("xhr", "get")?;
                let data = get(host.as_ref(), call, "/data/trades", params).await?;
                if host.is_array(&data) {
                    return returned(host.as_ref(), data).await;
                }
                let rows = host.field(&data, "data")?;
                let rows = if host.truthy(&rows) {
                    rows
                } else {
                    host.array(vec![])
                };
                host.push_spread(&trades, &rows)?;
                let next = host.field(&data, "next_cursor")?;
                cursor = if host.truthy(&next) {
                    next
                } else {
                    host.text("LTE=")
                };
                if host.truthy(&limit)
                    && host.greater_equal(&host.number(host.length(&trades)? as f64), &limit)?
                {
                    return returned(host.as_ref(), host.slice(&trades, &limit)?).await;
                }
            }
            returned(host.as_ref(), trades).await
        }
    })
}
/// PolymarketAPI.getGammaMarkets: nullish defaults, copied flags and cumulative offsets.
pub fn get_gamma_markets<H: AccountHost>(
    host: Rc<H>,
    params: H::Value,
) -> impl Future<Output = Result<H::Value, H::Error>> {
    start(host.as_ref(), {
        let host = host.clone();
        async move {
            let get_param = |key| {
                if host.nullish(&params) {
                    Ok(host.undefined())
                } else {
                    host.field(&params, key)
                }
            };
            let limit = get_param("limit")?;
            let limit = if host.nullish(&limit) {
                host.number(500.0)
            } else {
                limit
            };
            let offset = get_param("offset")?;
            let mut offset = if host.nullish(&offset) {
                host.number(0.0)
            } else {
                offset
            };
            let markets = host.array(vec![]);
            loop {
                let call = host.capture("gammaXhr", "get")?;
                let request = host.spread_params(&params, limit.clone(), offset.clone())?;
                let data = get(host.as_ref(), call, "/markets", request).await?;
                host.push_spread(&markets, &data)?;
                if host.less(&host.number(host.length(&data)? as f64), &limit)? {
                    return returned(host.as_ref(), markets).await;
                }
                offset = host.add(&offset, &limit)?;
            }
        }
    })
}
/// PolymarketAPI.getPositions: fixed 500 pages, sizeThreshold zero and user identity.
pub fn get_positions<H: AccountHost>(
    host: Rc<H>,
    user: H::Value,
) -> impl Future<Output = Result<H::Value, H::Error>> {
    start(host.as_ref(), {
        let host = host.clone();
        async move {
            let limit = host.number(500.0);
            let mut offset = host.number(0.0);
            let positions = host.array(vec![]);
            loop {
                let call = host.capture("dataXhr", "get")?;
                let params = host.object(vec![
                    ("user", user.clone()),
                    ("limit", limit.clone()),
                    ("offset", offset.clone()),
                    ("sizeThreshold", host.number(0.0)),
                ])?;
                let data = get(host.as_ref(), call, "/positions", params).await?;
                host.push_spread(&positions, &data)?;
                if host.less(&host.number(host.length(&data)? as f64), &limit)? {
                    return returned(host.as_ref(), positions).await;
                }
                offset = host.add(&offset, &limit)?;
            }
        }
    })
}
/// PolymarketExchange.getAccount: wallet result is awaited without string narrowing.
pub fn get_account<H: AccountHost>(
    host: Rc<H>,
) -> impl Future<Output = Result<H::Value, H::Error>> {
    start(host.as_ref(), {
        let host = host.clone();
        async move {
            let address = if host.truthy(&host.wallet()?) {
                let call = host.capture("wallet", "getAddress")?;
                wait(host.invoke(call, vec![])).await?
            } else {
                host.text("unknown")
            };
            let result = host.object(vec![("userId", address), ("affiliateId", host.null())])?;
            returned(host.as_ref(), result).await
        }
    })
}
/// PolymarketExchange.validateAccount: re-read credentials after optional derivation;
/// failed child calls become a logged error result, unless logging itself throws.
pub fn validate_account<H: AccountHost>(
    host: Rc<H>,
) -> impl Future<Output = Result<H::Value, H::Error>> {
    start(host.as_ref(), {
        let host = host.clone();
        async move {
            let body = async {
                if host.truthy(&host.wallet()?) && !host.truthy(&host.exchange_credentials()?) {
                    let call = host.capture("exchange", "deriveApiCredentials")?;
                    wait(host.invoke(call, vec![])).await?;
                }
                if host.truthy(&host.exchange_credentials()?) {
                    let call = host.capture("api", "getBalanceAllowance")?;
                    wait(host.invoke(call, vec![host.signer_type()?])).await?;
                    host.object(vec![("error", host.text(""))])
                } else {
                    host.object(vec![("error", host.text("No wallet configured"))])
                }
            }
            .await;
            let result = match body {
                Ok(result) => result,
                Err(error) => {
                    let message = host.error_message(&error)?;
                    let message = if host.truthy(&message) {
                        message
                    } else {
                        host.text("Failed to validate account")
                    };
                    let logger = host.capture("exchange", "log")?;
                    let mut log = "Validation error: ".encode_utf16().collect::<Vec<_>>();
                    log.extend(host.string(&message)?);
                    host.invoke_log(logger, &log, "error")?;
                    host.object(vec![("error", message)])?
                }
            };
            returned(host.as_ref(), result).await
        }
    })
}
