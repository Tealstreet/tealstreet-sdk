//! Native batch eligibility within the declared PlaceOrderOpts domain.
use safe_cex_types::{OrderStopTriggerType, OrderTimeInForce, OrderType};

// ts: ParadexExchange#isNativeBatchOrder
pub struct NativeBatchOrder {
    pub r#type: OrderType,
    pub trigger_price: Option<f64>,
    pub stop_loss: Option<f64>,
    pub take_profit: Option<f64>,
    pub tp_trigger_type: Option<OrderStopTriggerType>,
    pub sl_trigger_type: Option<OrderStopTriggerType>,
    pub close: Option<bool>,
    pub time_in_force: Option<OrderTimeInForce>,
}

// ts: ParadexExchange#isNativeBatchOrder
pub fn is_native_batch_order(opts: &NativeBatchOrder) -> bool {
    let supported_time_in_force = matches!(
        opts.time_in_force,
        None | Some(
            OrderTimeInForce::GoodTillCancel
                | OrderTimeInForce::ImmediateOrCancel
                | OrderTimeInForce::PostOnly
        )
    );
    opts.r#type == OrderType::Limit
        && opts.trigger_price.is_none()
        && opts.stop_loss.is_none()
        && opts.take_profit.is_none()
        && opts.tp_trigger_type.is_none()
        && opts.sl_trigger_type.is_none()
        && opts.close != Some(true)
        && supported_time_in_force
}
