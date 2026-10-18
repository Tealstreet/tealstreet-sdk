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
        || Ok(order_list(store.orders()?.to_vec())),
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

fn local_fallback(rt: &Rt) -> Result<bool, Thrown> {
    Ok(rt
        .this
        .call_method("shouldUseLocalRestOrderLifecycleFallback", vec![])?
        .truthy())
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
            lifecycle::should_reconcile_local_reduce_only_limit_order(order, || local_fallback(rt))
        },
        |id| clear_timers(rt, id),
        move |callback, delay| Ok(timer_rt.set_timeout(Rc::new(move || callback()), delay)),
        get_reconcile,
    )
}

// ts: BybitSpotExchange#recordRecentlyPlacedOrder and BybitInverseExchange#recordRecentlyPlacedOrder
pub(crate) fn record_variant_recently_placed_order(rt: &Rt, order: Js) -> Result<Js, Thrown> {
    if order.get("type")?.strict_equals(&Js::str("market")) {
        return Ok(Js::Undefined);
    }
    let now = rt
        .global("virtualClock.getCurrentTimeMs")?
        .call(Js::Undefined, vec![])?;
    rt.this.get("recentlyPlacedOrders")?.call_method(
        "set",
        vec![
            order.get("id")?,
            Js::object(vec![("placedAt", now), ("order", order.clone())]),
        ],
    )?;
    if rt
        .this
        .call_method("shouldUseLocalRestOrderLifecycleFallback", vec![])?
        .truthy()
    {
        rt.this
            .get("store")?
            .call_method("addOrUpdateOrders", vec![Js::array(vec![order])])?;
    }
    Ok(Js::Undefined)
}

// ts: BybitExchange#recordRecentlyPlacedOrder
pub(crate) fn record_recently_placed_order(rt: &Rt, order: &Js) -> Result<(), Thrown> {
    let venue = venue(rt)?;
    let order: Order = to_typed(order)?;
    lifecycle::record_recently_placed_order(
        &venue.order_state,
        Rc::new(RefCell::new(order)),
        || now(rt),
        || local_fallback(rt),
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

// ts: BybitExchange#shouldReconcileLocalReduceOnlyLimitOrder
pub(crate) fn should_reconcile(rt: &Rt, order: &Js) -> Result<bool, Thrown> {
    let order: Order = to_typed(order)?;
    lifecycle::should_reconcile_local_reduce_only_limit_order(&Rc::new(RefCell::new(order)), || {
        local_fallback(rt)
    })
}

// ts: BybitExchange#getFilledOrderReconciliationTarget
pub(crate) fn filled_order_reconciliation_target(order: &Js) -> Result<f64, Thrown> {
    let order: Order = to_typed(order)?;
    Ok(lifecycle::get_filled_order_reconciliation_target(&Rc::new(
        RefCell::new(order),
    )))
}

/// `this.recentlyPlacedOrders`: the typed map, read through `get(id)`.
pub(crate) fn recently_placed_orders(rt: &Rt) -> Js {
    let (get_rt, set_rt) = (rt.clone(), rt.clone());
    let view = Js::object_with_class("Map", vec![]);
    let _ = view.define(
        "get",
        Js::function("get", move |_, a| {
            let id = crate::binding::host::text(&a.first().cloned().unwrap_or(Js::Undefined))?;
            let venue = venue(&get_rt)?;
            let Some((_, entry)) = venue
                .order_state
                .recent()
                .into_iter()
                .find(|(k, _)| *k == id)
            else {
                return Ok(Js::Undefined);
            };
            let entry = entry.borrow();
            let order = from_typed(&*entry.order.borrow())?;
            Ok(Js::object(vec![
                ("placedAt", Js::Number(entry.placed_at)),
                ("order", order),
            ]))
        }),
    );
    let _ = view.define(
        "set",
        Js::function("set", move |this, a| {
            let arg = |i: usize| a.get(i).cloned().unwrap_or(Js::Undefined);
            let id = crate::binding::host::text(&arg(0))?;
            let entry = arg(1);
            let order: Order = to_typed(&entry.get("order")?)?;
            venue(&set_rt)?.order_state.put_recent(
                id,
                lifecycle::RecentEntry {
                    placed_at: entry.get("placedAt")?.to_number()?,
                    order: Rc::new(RefCell::new(order)),
                },
            );
            Ok(this)
        }),
    );
    view
}

// ts: BybitExchange#retireCancelledOrders
pub(crate) fn retire_cancelled_orders(rt: &Rt, orders: &Js) -> Result<(), Thrown> {
    let venue = venue(rt)?;
    // The source accepts Pick<Order, 'id' | 'symbol'>, including the snapshots
    // produced by cancelSymbolOrders. No other Order fields are consumed here.
    let orders = orders.array_values().unwrap_or_default();
    if orders.is_empty() {
        return Ok(());
    }
    let primary = matches!(
        venue.base.name,
        safe_cex_types::ExchangeName::Bybit
            | safe_cex_types::ExchangeName::Bybitdemo
            | safe_cex_types::ExchangeName::Hyrotrader
    );
    let retired_at = if primary { now(rt)? } else { 0. };
    for order in &orders {
        let id = order.get("id")?;
        rt.this.call_method("markOrderSeen", vec![id.clone()])?;
        if primary {
            let symbol = order.get("symbol")?;
            if symbol.truthy() {
                venue.order_state.put_retired(
                    crate::binding::host::text(&id)?,
                    lifecycle::RetiredSymbol {
                        symbol: crate::binding::host::text(&symbol)?,
                        retired_at,
                    },
                );
            }
        }
    }
    if local_fallback(rt)? {
        let ids = orders
            .iter()
            .map(|order| Ok(Js::object(vec![("id", order.get("id")?)])))
            .collect::<Result<Vec<_>, Thrown>>()?;
        rt.this
            .get("store")?
            .call_method("removeOrders", vec![Js::array(ids)])?;
    }
    Ok(())
}
