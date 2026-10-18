//! Spot cancellation body with original IDs and live order-array iteration.
use safe_cex_exchange::js_runtime::{
    promise::JsFuture,
    value::{Js, Thrown, error_object},
};
use std::rc::Rc;

pub trait SpotCancelHost {
    type Orders: ?Sized;
    type Order: ?Sized;
    type Id: ?Sized;
    type Items: Iterator<Item = Result<Rc<Self::Order>, Thrown>>;
    fn length(&self, orders: &Self::Orders) -> Result<usize, Thrown>;
    fn first(&self, orders: &Self::Orders) -> Result<Rc<Self::Order>, Thrown>;
    fn items(&self, orders: Rc<Self::Orders>) -> Result<Self::Items, Thrown>;
    fn id(&self, order: &Self::Order) -> Result<Rc<Self::Id>, Thrown>;
    fn post_cancel(
        &self,
        endpoint: &'static str,
        field: &'static str,
        id: Rc<Self::Id>,
    ) -> Result<JsFuture<'static>, Thrown>;
    fn error_message(&self, error: &Thrown, fallback: &'static str) -> Result<Js, Thrown>;
    fn emit_error(&self, message: &Js, context: &'static str) -> Result<(), Thrown>;
}

pub async fn cancel_orders<H: SpotCancelHost + ?Sized>(
    host: &H,
    orders: Rc<H::Orders>,
) -> Result<(), Thrown> {
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
        Ok::<_, Thrown>(())
    }
    .await;
    if let Err(error) = result {
        let message = host.error_message(&error, "Failed to cancel orders")?;
        host.emit_error(&message, "CancelOrders Error")?;
        let error = error_object("Error", "");
        error.set("message", Js::utf16(&message.to_js_string()?))?;
        return Err(Thrown::new(error));
    }
    Ok(())
}
