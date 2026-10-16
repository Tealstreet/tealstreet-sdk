//! Order/book adapter bodies at bound receiver/runtime intrinsics.
//! Complete classes, transport and general JavaScript intrinsics stay separate.
use crate::ws_dispatch::Runtime;
use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

/// The source's plain mapped Order object. `info` retains the original input.
#[derive(Debug, Clone)]
pub struct NativeOrder<V> {
    pub id: V,
    pub status: V,
    pub symbol: V,
    pub order_type: V,
    pub side: V,
    pub price: f64,
    pub amount: f64,
    pub filled: f64,
    pub remaining: f64,
    pub reduce_only: bool,
    pub info: V,
    pub last_update: f64,
}
#[derive(Debug, Clone)]
pub struct NativeFill<V> {
    pub id: V,
    pub side: V,
    pub symbol: V,
    pub price: f64,
    pub amount: f64,
}
#[derive(Debug, Clone)]
pub struct BookLevel {
    pub price: f64,
    pub amount: f64,
    pub total: f64,
}
/// Optional slots preserve Array.map holes; each side is a newly mapped array.
#[derive(Debug, Clone)]
pub struct NativeBook {
    pub bids: Vec<Option<BookLevel>>,
    pub asks: Vec<Option<BookLevel>>,
}
/// Field reads, enum-object lookup (including inherited entries), token maps and clock.
pub trait OrderHost: Runtime {
    /// Captured token-map receiver and its get callable, before argument reads.
    type TokenMap: Clone;
    /// Resolve/capture the get receiver+callable before arguments. If optional
    /// is false, an absent/null map raises the original property access error;
    /// None is reserved for a nullish optional map.
    fn token_map(&self, optional: bool) -> Result<Option<Self::TokenMap>, Self::Error>;
    fn token_symbol(
        &self,
        map: &Self::TokenMap,
        token: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn enum_lookup(
        &self,
        table: &'static str,
        key: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn literal(&self, text: &'static str) -> Self::Value;
    fn clock(&self) -> Result<f64, Self::Error>;
    fn error_string(&self, error: &Self::Error) -> Result<Vec<u16>, Self::Error>;
    fn lowercase(&self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn map_order(
        &self,
        value: &Self::Value,
    ) -> Result<Rc<RefCell<NativeOrder<Self::Value>>>, Self::Error>;
    fn add_orders(
        &self,
        orders: Vec<Rc<RefCell<NativeOrder<Self::Value>>>>,
    ) -> Result<(), Self::Error>;
    fn emit_fill(&self, fill: NativeFill<Self::Value>) -> Result<(), Self::Error>;
}
/// Host-owned started operation: dropping an observer must not cancel it.
pub trait Completion<V, E> {
    fn poll(&self, cx: &mut Context<'_>) -> Poll<Result<V, E>>;
}
#[derive(Debug, Clone)]
pub enum ApiCall<V> {
    OpenOrders,
    CancelOrders(Vec<Option<V>>),
    CancelAllOrders,
    OrderBook(V),
}
/// Ordinary Array.map bindings capture length then read live slots; null/invalid
/// arrays fail, rather than silently becoming empty. Child methods remain current.
pub trait AdapterHost: OrderHost {
    type Array: Clone;
    type Operation: Completion<Self::Value, Self::Error>;
    type Callback;
    type Cleanup;
    fn array(&self, value: &Self::Value) -> Result<Self::Array, Self::Error>;
    fn length(&self, array: &Self::Array) -> Result<usize, Self::Error>;
    fn at(&self, array: &Self::Array, index: usize) -> Result<Option<Self::Value>, Self::Error>;
    /// Eagerly start and retain the whole method task until completion even if
    /// its observer is dropped; schedule await continuations like source microtasks.
    fn start_task(&self, task: Pin<Box<dyn Future<Output = ()>>>);
    fn call(&self, call: ApiCall<Self::Value>) -> Result<Rc<Self::Operation>, Self::Error>;
    fn token(&self, symbol: &[u16]) -> Result<Self::Value, Self::Error>;
    fn new_error(&self, message: Vec<u16>) -> Self::Error;
    fn subscribe_book(
        &self,
        token: Self::Value,
        callback: Self::Callback,
    ) -> Result<Self::Cleanup, Self::Error>;
    fn connect(&self) -> Result<(), Self::Error>;
}
struct TaskState<T, E> {
    value: Option<Result<T, E>>,
    waker: Option<Waker>,
}
struct MethodObserver<T, E> {
    state: Rc<RefCell<TaskState<T, E>>>,
}
impl<T, E> Future for MethodObserver<T, E> {
    type Output = Result<T, E>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.state.borrow_mut();
        if let Some(value) = state.value.take() {
            Poll::Ready(value)
        } else {
            state.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}
fn self_start<H: AdapterHost, T: 'static>(
    host: &H,
    body: impl Future<Output = Result<T, H::Error>> + 'static,
) -> MethodObserver<T, H::Error> {
    let state = Rc::new(RefCell::new(TaskState {
        value: None,
        waker: None,
    }));
    let output = state.clone();
    host.start_task(Box::pin(async move {
        let value = body.await;
        let waker = {
            let mut state = output.borrow_mut();
            state.value = Some(value);
            state.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }));
    MethodObserver { state }
}
struct Observe<O, V, E> {
    op: Rc<O>,
    yielded: bool,
    marker: std::marker::PhantomData<fn() -> (V, E)>,
}
impl<O: Completion<V, E>, V, E> Future for Observe<O, V, E> {
    type Output = Result<V, E>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        if !this.yielded {
            this.yielded = true;
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        this.op.poll(cx)
    }
}
async fn observe<O: Completion<V, E>, V, E>(op: Result<Rc<O>, E>) -> Result<V, E> {
    Observe {
        op: op?,
        yielded: false,
        marker: std::marker::PhantomData,
    }
    .await
}
fn number<H: Runtime>(host: &H, value: &H::Value) -> Result<f64, H::Error> {
    Ok(safe_cex_num::parse_float(&String::from_utf16_lossy(
        &host.string(value)?,
    )))
}
fn mapped<H: OrderHost>(
    host: &H,
    data: &H::Value,
    optional: bool,
) -> Result<NativeOrder<H::Value>, H::Error> {
    let map = host.token_map(optional)?;
    let symbol = if let Some(map) = map {
        host.token_symbol(&map, &host.field(data, "asset_id")?)?
    } else {
        host.undefined()
    };
    let symbol = if host.truthy(&symbol) {
        symbol
    } else {
        host.field(data, "asset_id")?
    };
    let id = host.field(data, "id")?;
    let status = host.enum_lookup("ORDER_STATUS_MAP", &host.field(data, "status")?)?;
    let status = if host.truthy(&status) {
        status
    } else {
        host.field(data, "status")?
    };
    let order_type = host.enum_lookup("ORDER_TYPE_MAP", &host.field(data, "order_type")?)?;
    let order_type = if host.truthy(&order_type) {
        order_type
    } else {
        host.literal("limit")
    };
    let side = host.enum_lookup("ORDER_SIDE_MAP", &host.field(data, "side")?)?;
    let side = if host.truthy(&side) {
        side
    } else {
        host.literal("buy")
    };
    let price = number(host, &host.field(data, "price")?)?;
    let amount = number(host, &host.field(data, "original_size")?)?;
    let filled = number(host, &host.field(data, "size_matched")?)?;
    let remaining = number(host, &host.field(data, "original_size")?)?
        - number(host, &host.field(data, "size_matched")?)?;
    let info = data.clone();
    let last_update = host.clock()?;
    Ok(NativeOrder {
        id,
        status,
        symbol,
        order_type,
        side,
        price,
        amount,
        filled,
        remaining,
        reduce_only: false,
        info,
        last_update,
    })
}
/// Source PolymarketExchange#mapOrder; its token map is required.
pub fn map_exchange_order<H: OrderHost>(
    host: &H,
    data: &H::Value,
) -> Result<NativeOrder<H::Value>, H::Error> {
    mapped(host, data, false)
}
/// Source PolymarketPrivateWebSocket#mapOrder; its token map is optional.
pub fn map_private_order<H: OrderHost>(
    host: &H,
    data: &H::Value,
) -> Result<NativeOrder<H::Value>, H::Error> {
    mapped(host, data, true)
}
fn fetch_error<H: OrderHost>(host: &H, error: H::Error) -> H::Error {
    let reported = (|| {
        let mut text: Vec<u16> = "Error fetching orders: ".encode_utf16().collect();
        text.extend(host.error_string(&error)?);
        host.log(&text, "error")
    })();
    match reported {
        Ok(()) => error,
        Err(replacement) => replacement,
    }
}
/// Source PolymarketExchange.fetchOrders, including synchronous catch logging.
pub fn fetch_orders<H: AdapterHost>(
    host: Rc<H>,
) -> impl Future<Output = Result<Vec<Option<Rc<RefCell<NativeOrder<H::Value>>>>>, H::Error>> {
    let operation = match host.call(ApiCall::OpenOrders) {
        Ok(op) => op,
        Err(error) => {
            let error = fetch_error(&*host, error);
            return self_start(&*host, async move { Err(error) });
        }
    };
    let runner = host.clone();
    self_start(&*host, async move {
        let host = runner;
        let result = async {
            let data = observe(Ok(operation)).await?;
            let array = host.array(&data)?;
            let length = host.length(&array)?;
            let mut orders = Vec::with_capacity(length);
            for index in 0..length {
                orders.push(match host.at(&array, index)? {
                    Some(value) => Some(host.map_order(&value)?),
                    None => None,
                });
            }
            Ok(orders)
        }
        .await;
        result.map_err(|error| fetch_error(&*host, error))
    })
}
/// Source PolymarketExchange.cancelOrders; log the live input length after await.
pub fn cancel_orders<H: AdapterHost>(
    host: Rc<H>,
    orders: H::Array,
) -> impl Future<Output = Result<(), H::Error>> {
    let start = (|| {
        let length = host.length(&orders)?;
        if length == 0 {
            return Ok(None);
        }
        let length = host.length(&orders)?;
        let mut ids = Vec::with_capacity(length);
        for index in 0..length {
            ids.push(match host.at(&orders, index)? {
                Some(order) => Some(host.field(&order, "id")?),
                None => None,
            });
        }
        host.call(ApiCall::CancelOrders(ids)).map(Some)
    })();
    let runner = host.clone();
    self_start(&*host, async move {
        let host = runner;
        if let Some(op) = start? {
            observe(Ok(op)).await?;
            let length = host.length(&orders)?;
            host.log(
                &format!("Cancelled {length} orders")
                    .encode_utf16()
                    .collect::<Vec<_>>(),
                "info",
            )?;
        }
        Ok(())
    })
}
/// Source PolymarketExchange.cancelAllOrders.
pub fn cancel_all_orders<H: AdapterHost>(
    host: Rc<H>,
) -> impl Future<Output = Result<(), H::Error>> {
    let operation = host.call(ApiCall::CancelAllOrders);
    let runner = host.clone();
    self_start(&*host, async move {
        let host = runner;
        observe(operation).await?;
        host.log(
            &"Cancelled all orders".encode_utf16().collect::<Vec<_>>(),
            "info",
        )
    })
}
fn levels<H: AdapterHost>(
    host: &H,
    book: &H::Value,
    key: &str,
) -> Result<Vec<Option<BookLevel>>, H::Error> {
    let array = host.array(&host.field(book, key)?)?;
    let length = host.length(&array)?;
    let mut total = 0.0;
    let mut result = Vec::with_capacity(length);
    for index in 0..length {
        result.push(match host.at(&array, index)? {
            None => None,
            Some(level) => {
                let price = number(host, &host.field(&level, "price")?)?;
                let amount = number(host, &host.field(&level, "size")?)?;
                total += amount;
                Some(BookLevel {
                    price,
                    amount,
                    total,
                })
            }
        });
    }
    Ok(result)
}
/// Source PolymarketExchange.fetchOrderBook; independent running totals per side.
pub fn fetch_order_book<H: AdapterHost>(
    host: Rc<H>,
    symbol: &[u16],
) -> impl Future<Output = Result<NativeBook, H::Error>> + use<H> {
    let start = (|| {
        let token = host.token(symbol)?;
        if !host.truthy(&token) {
            let mut text: Vec<_> = "Unknown symbol: ".encode_utf16().collect();
            text.extend(symbol);
            return Err(host.new_error(text));
        }
        host.call(ApiCall::OrderBook(token))
    })();
    let runner = host.clone();
    self_start(&*host, async move {
        let host = runner;
        let book = observe(start).await?;
        let bids = levels(&*host, &book, "bids")?;
        let asks = levels(&*host, &book, "asks")?;
        Ok(NativeBook { bids, asks })
    })
}
/// Source PolymarketExchange.listenOrderBook; return the exact child cleanup.
pub fn listen_order_book<H: AdapterHost>(
    host: &H,
    symbol: &[u16],
    callback: H::Callback,
) -> Result<H::Cleanup, H::Error> {
    let token = host.token(symbol)?;
    if !host.truthy(&token) {
        let mut text: Vec<_> = "Unknown symbol: ".encode_utf16().collect();
        text.extend(symbol);
        return Err(host.new_error(text));
    }
    let unsubscribe = host.subscribe_book(token, callback)?;
    host.connect()?;
    Ok(unsubscribe)
}
/// Source PolymarketPrivateWebSocket#handleOrderUpdate, including catch logging.
pub fn handle_order_update<H: OrderHost>(host: &H, data: &H::Value) -> Result<(), H::Error> {
    let result = (|| {
        let order = host.map_order(data)?;
        host.add_orders(vec![order.clone()])?;
        if host.is_text(&host.lowercase(&host.field(data, "status")?)?, "matched") {
            let fill = {
                let o = order.borrow();
                NativeFill {
                    id: o.id.clone(),
                    side: o.side.clone(),
                    symbol: o.symbol.clone(),
                    price: o.price,
                    amount: o.filled,
                }
            };
            host.emit_fill(fill)?;
        }
        let id = order.borrow().id.clone();
        let mut text: Vec<_> = "Order update: ".encode_utf16().collect();
        text.extend(host.string(&id)?);
        text.extend(" - ".encode_utf16());
        let status = order.borrow().status.clone();
        text.extend(host.string(&status)?);
        host.log(&text, "debug")
    })();
    if let Err(error) = result {
        let mut text: Vec<_> = "Error handling order update: ".encode_utf16().collect();
        text.extend(host.error_string(&error)?);
        host.log(&text, "error")?;
    }
    Ok(())
}
