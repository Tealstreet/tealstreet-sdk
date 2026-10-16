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

// ts: BybitExchange#shouldUseLocalRestOrderLifecycleFallback
pub(crate) fn should_use_local_fallback(rt: &Rt) -> Result<bool, Thrown> {
    let venue = venue(rt)?;
    lifecycle::should_use_local_rest_order_lifecycle_fallback(
        || Ok::<_, Thrown>(venue.uses_bybit_demo_environment()),
        || {
            Ok(venue
                .base
                .options()
                .bybit_site_id
                .as_ref()
                .and_then(|id| serde_json::to_value(id).ok()))
        },
    )
}

// ts: BybitExchange#scheduleLocalReduceOnlyLimitOrderReconciliation
pub(crate) fn schedule_reconciliation(rt: &Rt, order: Order) -> Result<(), Thrown> {
    let venue = venue(rt)?;
    let handle = Rc::new(RefCell::new(order));
    let timer_rt = rt.clone();
    let getter_rt = rt.clone();
    let get_reconcile: lifecycle::ReconcileGetter<'static, Js, Thrown> = Rc::new(move || {
        let rt = getter_rt.clone();
        let method: lifecycle::ReconcileMethod<'static, Js, Thrown> = Rc::new(move |id: &str| {
            rt.this
                .call_method("reconcileLocalReduceOnlyLimitOrder", vec![Js::str(id)])
        });
        Ok(method)
    });
    lifecycle::schedule_local_reduce_only_limit_order_reconciliation(
        &venue.order_state,
        handle,
        |order| {
            lifecycle::should_reconcile_local_reduce_only_limit_order(order, || {
                should_use_local_fallback(rt)
            })
        },
        |id| clear_timers(rt, id),
        move |callback, delay| Ok(timer_rt.set_timeout(Rc::new(move || callback()), delay)),
        get_reconcile,
    )
}

// ts: BybitExchange#recordRecentlyPlacedOrder
pub(crate) fn record_recently_placed_order(rt: &Rt, order: &Js) -> Result<(), Thrown> {
    let venue = venue(rt)?;
    let order: Order = to_typed(order)?;
    lifecycle::record_recently_placed_order(
        &venue.order_state,
        Rc::new(RefCell::new(order)),
        || now(rt),
        || should_use_local_fallback(rt),
        |orders| {
            let rows: Vec<Order> = orders.borrow().iter().map(|o| o.borrow().clone()).collect();
            rt.this
                .get("store")?
                .call_method("addOrUpdateOrders", vec![from_typed(&rows)?])?;
            Ok(())
        },
        |order| schedule_reconciliation(rt, order.borrow().clone()),
    )
}
