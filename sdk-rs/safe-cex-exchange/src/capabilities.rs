use crate::{
    exchange::{Exchange, ValidatedBatch, ValidatedOrder},
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_types::{
    AccountGreeks, ClosedPnlEntry, ExchangeName, Execution, ExecutionsOptions, HistoricalOrder,
    HistoricalTradeOptions, IncomeHistoryOptions, IncomeRecord, OptionChainQuery, OptionChainRow,
    OrderHistoryOptions, PlaceOrderOpts,
};
use std::{borrow::Borrow, rc::Rc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaseCapabilityMethod {
    FetchClosedPnl,
    FetchHistoricalFills,
    FetchIncomeHistory,
    FetchIncomeHistoryRange,
    FetchHistoricalOrders,
    PlaceOrder,
    PlaceOrders,
    Start,
    FetchOptionChain,
    FetchOptionExpiries,
    FetchAccountGreeks,
}
#[derive(Clone, Copy)]
enum Origin {
    Base(BaseCapabilityMethod),
    Empty,
    Implemented,
}
type Handler<A, R> = dyn for<'a> Fn(&'a dyn Exchange, A) -> ExchangeFuture<'a, R>;
pub struct CapabilityBinding<A, R> {
    origin: Origin,
    handler: Rc<Handler<A, R>>,
}
impl<A, R> CapabilityBinding<A, R> {
    pub fn implemented(
        handler: impl for<'a> Fn(&'a dyn Exchange, A) -> ExchangeFuture<'a, R> + 'static,
    ) -> Self {
        Self {
            origin: Origin::Implemented,
            handler: Rc::new(handler),
        }
    }
    pub fn empty_override(
        handler: impl for<'a> Fn(&'a dyn Exchange, A) -> ExchangeFuture<'a, R> + 'static,
    ) -> Self {
        Self {
            origin: Origin::Empty,
            handler: Rc::new(handler),
        }
    }
    pub fn call<'a>(&self, exchange: &'a dyn Exchange, args: A) -> ExchangeFuture<'a, R> {
        (self.handler)(exchange, args)
    }
}
/// ts: base.ts option fallbacks, `${what} are not supported on ${this._exchangeName}`.
pub(crate) fn options_unsupported(name: &ExchangeName, what: &str) -> ExchangeError {
    let name = serde_json::to_value(name).expect("exchange name");
    ExchangeError::new(format!(
        "{what} are not supported on {}",
        name.as_str().expect("exchange name string")
    ))
}
pub(crate) fn mark_base_capability_fallback<A, R>(
    method: BaseCapabilityMethod,
    handler: impl for<'a> Fn(&'a dyn Exchange, A) -> ExchangeFuture<'a, R> + 'static,
) -> CapabilityBinding<A, R> {
    CapabilityBinding {
        origin: Origin::Base(method),
        handler: Rc::new(handler),
    }
}
mod sealed {
    pub trait Sealed {}
}
pub trait CapabilityBrand: sealed::Sealed {
    fn is_base(&self, method: BaseCapabilityMethod) -> bool;
    fn is_empty(&self) -> bool;
}
impl<A, R> sealed::Sealed for CapabilityBinding<A, R> {}
impl<A, R> CapabilityBrand for CapabilityBinding<A, R> {
    fn is_base(&self, method: BaseCapabilityMethod) -> bool {
        matches!(self.origin, Origin::Base(actual) if actual == method)
    }
    fn is_empty(&self) -> bool {
        matches!(self.origin, Origin::Empty)
    }
}
pub fn is_base_capability_fallback<B: CapabilityBrand>(
    binding: &B,
    method: BaseCapabilityMethod,
) -> bool {
    binding.is_base(method)
}
pub fn is_empty_stub_capability<B: CapabilityBrand>(binding: &B) -> bool {
    binding.is_empty()
}
pub fn is_inert_capability<B: CapabilityBrand>(binding: &B, method: BaseCapabilityMethod) -> bool {
    is_base_capability_fallback(binding, method) || is_empty_stub_capability(binding)
}
pub(crate) enum RawOrder {
    Typed(Box<dyn std::any::Any>),
    Json(serde_json::Value),
}
impl RawOrder {
    fn decode<V: Exchange>(self) -> Result<V::PlaceOpts, ExchangeError> {
        match self {
            Self::Typed(value) if value.is::<V::PlaceOpts>() => Ok(*value
                .downcast::<V::PlaceOpts>()
                .expect("checked options type")),
            Self::Typed(value) if value.is::<PlaceOrderOpts>() => Ok(V::PlaceOpts::from(
                *value
                    .downcast::<PlaceOrderOpts>()
                    .expect("checked shared options"),
            )),
            Self::Typed(_) => Err(ExchangeError::new(
                "Placement options belong to another venue type",
            )),
            Self::Json(value) => serde_json::from_value(value).map_err(|error| {
                ExchangeError::new(format!("Invalid venue placement options: {error}"))
            }),
        }
    }
}
fn placement_venue<V: Exchange>(venue: &dyn Exchange) -> Result<&V, ExchangeError> {
    (venue as &dyn std::any::Any)
        .downcast_ref::<V>()
        .ok_or_else(|| ExchangeError::new("Placement binding belongs to another venue type"))
}
fn prepare_batch<V: Exchange>(
    venue: &V,
    orders: Vec<RawOrder>,
) -> Result<Vec<V::PlaceOpts>, ExchangeError> {
    let orders = orders
        .into_iter()
        .map(RawOrder::decode::<V>)
        .collect::<Result<Vec<_>, _>>()?;
    let shared: Vec<PlaceOrderOpts> = orders.iter().map(|order| order.borrow().clone()).collect();
    venue.base().assert_caller_client_order_ids(&shared)?;
    for order in &orders {
        venue.validate_order_capabilities(order)?;
    }
    Ok(orders)
}
pub struct PlaceOrderBinding {
    inner: CapabilityBinding<RawOrder, Vec<String>>,
    batch: Rc<Handler<Vec<RawOrder>, Vec<String>>>,
}
impl PlaceOrderBinding {
    pub fn implemented<V: Exchange>(
        handler: impl for<'a> Fn(&'a V, ValidatedOrder<V>) -> ExchangeFuture<'a, Vec<String>> + 'static,
    ) -> Self {
        let handler = Rc::new(handler);
        let single = handler.clone();
        Self {
            inner: CapabilityBinding::implemented(move |venue, order: RawOrder| {
                let prepared: Result<_, crate::runtime::ExchangeError> = (|| {
                    let venue = placement_venue::<V>(venue)?;
                    let order = order.decode::<V>()?;
                    venue
                        .base()
                        .resolve_caller_client_order_id(order.borrow())?;
                    venue.validate_order_capabilities(&order)?;
                    Ok((venue, order))
                })();
                match prepared {
                    Ok((venue, order)) => single(venue, ValidatedOrder::new(order)),
                    Err(error) => Box::pin(async { Err(error) }),
                }
            }),
            batch: Rc::new(move |venue, orders| {
                let prepared: Result<_, crate::runtime::ExchangeError> = (|| {
                    let venue = placement_venue::<V>(venue)?;
                    let orders = prepare_batch(venue, orders)?;
                    Ok((venue, orders))
                })();
                let handler = handler.clone();
                Box::pin(async move {
                    let (venue, orders) = prepared?;
                    let mut ids = vec![];
                    for order in orders {
                        ids.extend(handler(venue, ValidatedOrder::new(order)).await?);
                    }
                    Ok(ids)
                })
            }),
        }
    }
    pub fn empty_override<V: Exchange>(
        handler: impl for<'a> Fn(&'a V, ValidatedOrder<V>) -> ExchangeFuture<'a, Vec<String>> + 'static,
    ) -> Self {
        let mut binding = Self::implemented(handler);
        binding.inner.origin = Origin::Empty;
        binding
    }
    pub(crate) fn call_raw<'a>(
        &self,
        venue: &'a dyn Exchange,
        order: RawOrder,
    ) -> ExchangeFuture<'a, Vec<String>> {
        self.inner.call(venue, order)
    }
}
impl Default for PlaceOrderBinding {
    fn default() -> Self {
        Self {
            inner: mark_base_capability_fallback(BaseCapabilityMethod::PlaceOrder, |_, _| {
                Box::pin(async { Err(ExchangeError::new("Not implemented")) })
            }),
            batch: Rc::new(|venue, orders| {
                Box::pin(async move {
                    if orders.is_empty() {
                        Ok(vec![])
                    } else {
                        venue
                            .capabilities()
                            .place_order
                            .call_raw(venue, orders.into_iter().next().expect("nonempty"))
                            .await
                    }
                })
            }),
        }
    }
}
impl sealed::Sealed for PlaceOrderBinding {}
impl CapabilityBrand for PlaceOrderBinding {
    fn is_base(&self, method: BaseCapabilityMethod) -> bool {
        self.inner.is_base(method)
    }
    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}
pub struct PlaceOrdersBinding {
    inner: CapabilityBinding<Vec<RawOrder>, Vec<String>>,
}
impl PlaceOrdersBinding {
    pub fn implemented<V: Exchange>(
        handler: impl for<'a> Fn(&'a V, ValidatedBatch<V>) -> ExchangeFuture<'a, Vec<String>> + 'static,
    ) -> Self {
        Self {
            inner: CapabilityBinding::implemented(move |venue, orders| {
                let prepared: Result<_, crate::runtime::ExchangeError> = (|| {
                    let venue = placement_venue::<V>(venue)?;
                    let orders = prepare_batch(venue, orders)?;
                    Ok((venue, orders))
                })();
                match prepared {
                    Ok((venue, orders)) => handler(venue, ValidatedBatch::new(orders)),
                    Err(error) => Box::pin(async { Err(error) }),
                }
            }),
        }
    }
    pub(crate) fn call_raw<'a>(
        &self,
        venue: &'a dyn Exchange,
        orders: Vec<RawOrder>,
    ) -> ExchangeFuture<'a, Vec<String>> {
        self.inner.call(venue, orders)
    }
}
impl Default for PlaceOrdersBinding {
    fn default() -> Self {
        Self {
            inner: mark_base_capability_fallback(
                BaseCapabilityMethod::PlaceOrders,
                |venue, orders| (venue.capabilities().place_order.batch)(venue, orders),
            ),
        }
    }
}
impl sealed::Sealed for PlaceOrdersBinding {}
impl CapabilityBrand for PlaceOrdersBinding {
    fn is_base(&self, method: BaseCapabilityMethod) -> bool {
        self.inner.is_base(method)
    }
    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}
pub struct ExchangeCapabilities {
    pub start: CapabilityBinding<(), ()>,
    pub place_order: PlaceOrderBinding,
    pub place_orders: PlaceOrdersBinding,
    pub fetch_historical_fills: CapabilityBinding<ExecutionsOptions, Vec<Execution>>,
    pub fetch_income_history: CapabilityBinding<Option<IncomeHistoryOptions>, Vec<IncomeRecord>>,
    pub fetch_income_history_range:
        CapabilityBinding<(f64, f64, Option<String>), Vec<IncomeRecord>>,
    pub fetch_closed_pnl: CapabilityBinding<HistoricalTradeOptions, Vec<ClosedPnlEntry>>,
    pub fetch_historical_orders:
        CapabilityBinding<Option<OrderHistoryOptions>, Vec<HistoricalOrder>>,
    pub fetch_option_chain: CapabilityBinding<OptionChainQuery, Vec<OptionChainRow>>,
    pub fetch_option_expiries: CapabilityBinding<String, Vec<f64>>,
    pub fetch_account_greeks: CapabilityBinding<(), Vec<AccountGreeks>>,
}
impl Default for ExchangeCapabilities {
    fn default() -> Self {
        use BaseCapabilityMethod::{
            FetchClosedPnl, FetchHistoricalFills, FetchHistoricalOrders, FetchIncomeHistory,
            FetchIncomeHistoryRange, Start,
        };
        Self {
            start: mark_base_capability_fallback(Start, |_, _| {
                Box::pin(async { Err(ExchangeError::new("Not implemented")) })
            }),
            place_order: PlaceOrderBinding::default(),
            place_orders: PlaceOrdersBinding::default(),
            fetch_historical_fills: mark_base_capability_fallback(FetchHistoricalFills, |_, _| {
                Box::pin(async { Ok(vec![]) })
            }),
            fetch_income_history: mark_base_capability_fallback(FetchIncomeHistory, |_, _| {
                Box::pin(async { Ok(vec![]) })
            }),
            fetch_income_history_range: mark_base_capability_fallback(
                FetchIncomeHistoryRange,
                |_, _| Box::pin(async { Ok(vec![]) }),
            ),
            fetch_closed_pnl: mark_base_capability_fallback(FetchClosedPnl, |_, _| {
                Box::pin(async { Ok(vec![]) })
            }),
            fetch_historical_orders: mark_base_capability_fallback(
                FetchHistoricalOrders,
                |_, _| Box::pin(async { Ok(vec![]) }),
            ),
            fetch_option_chain: mark_base_capability_fallback(FetchOptionChain, |venue, _| {
                let error = options_unsupported(venue.name(), "Option chains");
                Box::pin(async move { Err(error) })
            }),
            fetch_option_expiries: mark_base_capability_fallback(
                FetchOptionExpiries,
                |venue, _| {
                    let error = options_unsupported(venue.name(), "Option expiries");
                    Box::pin(async move { Err(error) })
                },
            ),
            fetch_account_greeks: mark_base_capability_fallback(FetchAccountGreeks, |venue, ()| {
                let error = options_unsupported(venue.name(), "Account greeks");
                Box::pin(async move { Err(error) })
            }),
        }
    }
}

impl ExchangeCapabilities {
    pub fn fetch_historical_fills<'a>(
        &self,
        venue: &'a dyn Exchange,
        options: ExecutionsOptions,
    ) -> ExchangeFuture<'a, Vec<Execution>> {
        self.fetch_historical_fills.call(venue, options)
    }
    pub fn fetch_income_history<'a>(
        &self,
        venue: &'a dyn Exchange,
        options: Option<IncomeHistoryOptions>,
    ) -> ExchangeFuture<'a, Vec<IncomeRecord>> {
        self.fetch_income_history.call(venue, options)
    }
    pub fn fetch_income_history_range<'a>(
        &self,
        venue: &'a dyn Exchange,
        start: f64,
        end: f64,
        kind: Option<String>,
    ) -> ExchangeFuture<'a, Vec<IncomeRecord>> {
        self.fetch_income_history_range
            .call(venue, (start, end, kind))
    }
    pub fn fetch_closed_pnl<'a>(
        &self,
        venue: &'a dyn Exchange,
        options: HistoricalTradeOptions,
    ) -> ExchangeFuture<'a, Vec<ClosedPnlEntry>> {
        self.fetch_closed_pnl.call(venue, options)
    }
    pub fn fetch_historical_orders<'a>(
        &self,
        venue: &'a dyn Exchange,
        options: Option<OrderHistoryOptions>,
    ) -> ExchangeFuture<'a, Vec<HistoricalOrder>> {
        self.fetch_historical_orders.call(venue, options)
    }
    pub fn fetch_option_chain<'a>(
        &self,
        venue: &'a dyn Exchange,
        query: OptionChainQuery,
    ) -> ExchangeFuture<'a, Vec<OptionChainRow>> {
        self.fetch_option_chain.call(venue, query)
    }
    pub fn fetch_option_expiries<'a>(
        &self,
        venue: &'a dyn Exchange,
        underlying: String,
    ) -> ExchangeFuture<'a, Vec<f64>> {
        self.fetch_option_expiries.call(venue, underlying)
    }
    pub fn fetch_account_greeks<'a>(
        &self,
        venue: &'a dyn Exchange,
    ) -> ExchangeFuture<'a, Vec<AccountGreeks>> {
        self.fetch_account_greeks.call(venue, ())
    }
}
