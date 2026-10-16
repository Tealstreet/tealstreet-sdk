//! The recently-placed-order shield, over the typed `PrimaryOrderState` the venue owns.
use crate::{
    binding::{
        rt::{Rt, venue},
        value::{Js, Thrown, from_typed, to_typed},
    },
    primary_order_lifecycle::{self as lifecycle, OrderList},
};
use safe_cex_types::Order;
use std::{cell::RefCell, rc::Rc};

fn order_list(orders: Vec<Order>) -> OrderList {
    Rc::new(RefCell::new(
        orders
            .into_iter()
            .map(|o| Rc::new(RefCell::new(o)))
            .collect(),
    ))
}

fn now(rt: &Rt) -> Result<f64, Thrown> {
    rt.global("virtualClock.getCurrentTimeMs")?
        .call(Js::Undefined, vec![])?
        .to_number()
}

// ts: BybitExchange#clearRecentlyPlacedOrderReconciliationTimers
pub(crate) fn clear_timers(rt: &Rt, order_id: &str) -> Result<(), Thrown> {
    let venue = venue(rt)?;
    lifecycle::clear_recently_placed_order_reconciliation_timers(
        &venue.order_state,
        order_id,
        |timer: Js| {
            rt.clear_timeout(&timer);
            Ok(())
        },
    )
}

// ts: BybitExchange.markOrderSeen
pub(crate) fn mark_order_seen(rt: &Rt, order_id: &str) -> Result<(), Thrown> {
    let venue = venue(rt)?;
    lifecycle::mark_order_seen(&venue.order_state, order_id, |id| clear_timers(rt, id))
}

// ts: BybitExchange#preserveRecentlyPlacedOrders
pub(crate) fn preserve_recently_placed_orders(rt: &Rt, fetched: &Js) -> Result<Js, Thrown> {
    let venue = venue(rt)?;
    let fetched = order_list(to_typed(fetched)?);
    let store = venue.base.store.clone();
    let kept = lifecycle::preserve_recently_placed_orders(
        &venue.order_state,
        fetched,
        || now(rt),
        |id| mark_order_seen(rt, id),
        || Ok(order_list(store.orders().to_vec())),
    )?;
    let kept: Vec<Order> = kept.borrow().iter().map(|o| o.borrow().clone()).collect();
    from_typed(&kept)
}
