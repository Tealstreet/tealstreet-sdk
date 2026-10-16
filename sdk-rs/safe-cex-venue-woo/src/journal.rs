//! WOO order-axis projection; shared interval folding and grading are not owned here.
use safe_cex_types::{Execution, HistoricalOrder, OrderSide};

// ts: optionalNum
pub fn optional_num(value: Option<f64>) -> Option<f64> {
    value.filter(|number| number.is_finite())
}

// ts: wooOrdersToExecutions
pub fn woo_orders_to_executions(orders: &[HistoricalOrder]) -> Vec<Execution> {
    let mut executions = Vec::new();
    for order in orders {
        if order.filled.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
            continue;
        }
        let Some(price) = order.average_price.filter(|price| *price > 0.0) else {
            continue;
        };
        let at = order.updated_time.unwrap_or(order.created_time);
        if at.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
            continue;
        }
        executions.push(Execution {
            id: Some(order.id.clone()),
            timestamp: (at / 1000.0).floor(),
            symbol: order.symbol.clone(),
            order_id: Some(order.id.clone()),
            price,
            qty: order.filled,
            side: if order.side == OrderSide::Buy {
                OrderSide::Buy
            } else {
                OrderSide::Sell
            },
            position_side: order.position_side.clone(),
            commission: optional_num(order.fees),
            commission_asset: order.fee_asset.clone(),
            realized_pnl: optional_num(order.realized_pnl),
            maker: None,
            venue_reported_revenue: None,
            venue_reported_revenue_asset: None,
        });
    }
    executions
}
