use safe_cex_exchange::{
    js_runtime::value::{Js, from_retained_input},
    runtime::ExchangeError,
};
use safe_cex_store::{Store, StoreMutationError};
use safe_cex_types::{Order, OrderStatus};
use std::{cell::RefCell, future::Future, pin::pin, rc::Rc, task::Poll};

/// Configured DELETE arguments. The wire adapter nests symbol under params.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelSymbolRequest {
    pub symbol: String,
}

/// Preserve the original request/emitter identity and complete Store failure.
/// Store notifications retain subscriber ID and the original opaque cause.
#[derive(Debug, Clone, PartialEq)]
pub enum CancelSymbolError {
    Request(Rc<ExchangeError>),
    Store(Box<StoreMutationError>),
    Emission(Rc<ExchangeError>),
}
impl std::fmt::Display for CancelSymbolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Request(error) | Self::Emission(error) => error.fmt(f),
            Self::Store(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for CancelSymbolError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Request(error) | Self::Emission(error) => error.as_ref(),
            Self::Store(error) => error.as_ref(),
        })
    }
}

/// Source order-object references; mutations through a handle remain visible to
/// snapshots across await and across the first status notification.
pub type CancelOrderHandle = Rc<RefCell<Order>>;

// ts: BinanceExchange.cancelSymbolOrders
/// Configured order view and update_status preserve source object identity.
/// get_orders returns stable handles; update_status binds the actual Store using
/// current handle IDs at EACH call. Continuous Store/view and class/client wiring
/// remain pending composition. DELETE starts on invocation; outer Result models
/// synchronous throw, future models async settlement. An observation future's
/// drop must not cancel an already-started request at the configured boundary.
pub async fn cancel_binance_symbol_orders<F>(
    get_orders: impl FnOnce() -> Vec<CancelOrderHandle>,
    symbol: &str,
    mut delete: impl FnMut(&'static str, CancelSymbolRequest) -> Result<F, Rc<ExchangeError>>,
    mut update_status: impl FnMut(&[CancelOrderHandle], OrderStatus) -> Result<(), StoreMutationError>,
    emit_error: impl FnOnce(Js) -> Result<(), Rc<ExchangeError>>,
) -> Result<(), CancelSymbolError>
where
    F: Future<Output = Result<(), Rc<ExchangeError>>>,
{
    let result = async {
        // Retain object handles, not early ID copies. Classification stays fixed
        // while later changes to the captured objects remain visible.
        let (regular_orders, algo_orders) = {
            let orders = get_orders();
            let mut regular_orders = vec![];
            let mut algo_orders = vec![];
            for handle in orders {
                let order = handle.borrow();
                if order.symbol != symbol {
                    continue;
                }
                let algo = order
                    .r#type
                    .is_some_and(crate::order_logic::is_algo_order_type);
                drop(order);
                if algo {
                    algo_orders.push(handle);
                } else {
                    regular_orders.push(handle);
                }
            }
            (regular_orders, algo_orders)
        };
        let regular_future = delete(
            "/fapi/v1/allOpenOrders",
            CancelSymbolRequest {
                symbol: symbol.into(),
            },
        )
        .map_err(CancelSymbolError::Request)?;
        let algo_future = delete(
            "/fapi/v1/algoOpenOrders",
            CancelSymbolRequest {
                symbol: symbol.into(),
            },
        )
        .map_err(CancelSymbolError::Request)?;
        let (regular_result, algo_result) = all_settled(regular_future, algo_future).await;
        if regular_result.is_ok() {
            update_status(&regular_orders, OrderStatus::Canceled)
                .map_err(|error| CancelSymbolError::Store(Box::new(error)))?;
        }
        if algo_result.is_ok() {
            update_status(&algo_orders, OrderStatus::Canceled)
                .map_err(|error| CancelSymbolError::Store(Box::new(error)))?;
        }
        regular_result.map_err(CancelSymbolError::Request)?;
        algo_result.map_err(CancelSymbolError::Request)?;
        Ok(())
    }
    .await;
    finish(result, emit_error)
}

// ts: BinancePMExchange.cancelSymbolOrders
/// DELETE is configured papiXhr.delete. Read live orders only after settlement;
/// subscriber failure escapes with committed state and its original cause.
pub async fn cancel_binance_pm_symbol_orders<F>(
    store: &dyn Store,
    symbol: &str,
    delete: impl FnOnce(&'static str, CancelSymbolRequest) -> Result<F, Rc<ExchangeError>>,
    emit_error: impl FnOnce(Js) -> Result<(), Rc<ExchangeError>>,
) -> Result<(), CancelSymbolError>
where
    F: Future<Output = Result<(), Rc<ExchangeError>>>,
{
    let result = async {
        delete(
            "/papi/v1/um/allOpenOrders",
            CancelSymbolRequest {
                symbol: symbol.into(),
            },
        )
        .map_err(CancelSymbolError::Request)?
        .await
        .map_err(CancelSymbolError::Request)?;
        let matching: Vec<String> = store
            .orders()
            .iter()
            .filter(|order| order.symbol == symbol)
            .map(|order| order.id.clone())
            .collect();
        let ids: Vec<&str> = matching.iter().map(String::as_str).collect();
        store
            .update_orders_status(&ids, OrderStatus::Canceled)
            .map_err(|error| CancelSymbolError::Store(Box::new(error)))
    }
    .await;
    finish(result, emit_error)
}

// ts: BinanceSpotExchange.cancelSymbolOrders
/// DELETE is configured xhr.delete. Matching IDs are selected from the live
/// Store after awaiting the request, unlike the futures pre-request snapshots.
pub async fn cancel_binance_spot_symbol_orders<F>(
    store: &dyn Store,
    symbol: &str,
    delete: impl FnOnce(&'static str, CancelSymbolRequest) -> Result<F, Rc<ExchangeError>>,
    emit_error: impl FnOnce(Js) -> Result<(), Rc<ExchangeError>>,
) -> Result<(), CancelSymbolError>
where
    F: Future<Output = Result<(), Rc<ExchangeError>>>,
{
    let result = async {
        delete(
            "/api/v3/openOrders",
            CancelSymbolRequest {
                symbol: symbol.into(),
            },
        )
        .map_err(CancelSymbolError::Request)?
        .await
        .map_err(CancelSymbolError::Request)?;
        let matching: Vec<String> = store
            .orders()
            .iter()
            .filter(|order| order.symbol == symbol)
            .map(|order| order.id.clone())
            .collect();
        let ids: Vec<&str> = matching.iter().map(String::as_str).collect();
        store
            .update_orders_status(&ids, OrderStatus::Canceled)
            .map_err(|error| CancelSymbolError::Store(Box::new(error)))
    }
    .await;
    finish(result, emit_error)
}

async fn all_settled<F>(
    left: F,
    right: F,
) -> (Result<(), Rc<ExchangeError>>, Result<(), Rc<ExchangeError>>)
where
    F: Future<Output = Result<(), Rc<ExchangeError>>>,
{
    let mut left = pin!(left);
    let mut right = pin!(right);
    let mut left_result = None;
    let mut right_result = None;
    std::future::poll_fn(|cx| {
        if left_result.is_none() {
            if let Poll::Ready(result) = left.as_mut().poll(cx) {
                left_result = Some(result);
            }
        }
        if right_result.is_none() {
            if let Poll::Ready(result) = right.as_mut().poll(cx) {
                right_result = Some(result);
            }
        }
        if left_result.is_some() && right_result.is_some() {
            Poll::Ready((left_result.take().unwrap(), right_result.take().unwrap()))
        } else {
            Poll::Pending
        }
    })
    .await
}

fn selected_message(error: &ExchangeError) -> Js {
    error
        .retained_response_input()
        .as_ref()
        .and_then(|response| response.get("data"))
        .and_then(|data| data.get("msg"))
        .cloned()
        .map(from_retained_input)
        .filter(Js::truthy)
        .unwrap_or_else(|| Js::str(&error.message))
}

fn finish(
    result: Result<(), CancelSymbolError>,
    emit_error: impl FnOnce(Js) -> Result<(), Rc<ExchangeError>>,
) -> Result<(), CancelSymbolError> {
    match result {
        Ok(()) => Ok(()),
        Err(error) => {
            let message = match &error {
                CancelSymbolError::Request(error) | CancelSymbolError::Emission(error) => {
                    selected_message(error)
                }
                // Project only for selecting emitted message; return the complete
                // original StoreMutationError, including its opaque listener cause.
                CancelSymbolError::Store(error) => {
                    selected_message(&ExchangeError::from(error.as_ref().clone()))
                }
            };
            emit_error(message).map_err(CancelSymbolError::Emission)?;
            Err(error)
        }
    }
}
