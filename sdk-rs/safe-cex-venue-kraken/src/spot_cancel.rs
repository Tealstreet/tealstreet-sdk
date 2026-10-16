//! Spot cancellation body with original IDs and live order-array iteration.
use safe_cex_exchange::runtime::{ExchangeError, ExchangeFuture};
use std::rc::Rc;

pub trait SpotCancelHost {
    type Orders: ?Sized;
    type Order: ?Sized;
    type Id: ?Sized;
    type Items: Iterator<Item = Result<Rc<Self::Order>, ExchangeError>>;
    fn length(&self, orders: &Self::Orders) -> Result<usize, ExchangeError>;
    fn first(&self, orders: &Self::Orders) -> Result<Rc<Self::Order>, ExchangeError>;
    fn items(&self, orders: Rc<Self::Orders>) -> Result<Self::Items, ExchangeError>;
    fn id(&self, order: &Self::Order) -> Result<Rc<Self::Id>, ExchangeError>;
    fn post_cancel(
        &self,
        endpoint: &'static str,
        field: &'static str,
        id: Rc<Self::Id>,
    ) -> Result<ExchangeFuture<'static, ()>, ExchangeError>;
    fn error_message(
        &self,
        error: &ExchangeError,
        fallback: &'static str,
    ) -> Result<String, ExchangeError>;
    fn emit_error(&self, message: &str, context: &'static str) -> Result<(), ExchangeError>;
}

pub async fn cancel_orders<H: SpotCancelHost + ?Sized>(
    host: &H,
    orders: Rc<H::Orders>,
) -> Result<(), ExchangeError> {
    let result = async {
        if host.length(&orders)? == 0 {
            return Ok(());
        }
        let endpoint = crate::spot_types::ENDPOINTS
            .iter()
            .find(|(key, _)| *key == "CANCEL_ORDER")
            .unwrap()
            .1;
        if host.length(&orders)? == 1 {
            let order = host.first(&orders)?;
            let id = host.id(&order)?;
            host.post_cancel(endpoint, "txid", id)?.await?;
            return Ok(());
        }
        for order in host.items(orders)? {
            let order = order?;
            let id = host.id(&order)?;
            host.post_cancel(endpoint, "txid", id)?.await?;
        }
        Ok::<_, ExchangeError>(())
    }
    .await;
    if let Err(error) = result {
        let message = host.error_message(&error, "Failed to cancel orders")?;
        host.emit_error(&message, "CancelOrders Error")?;
        return Err(ExchangeError::new(message));
    }
    Ok(())
}
