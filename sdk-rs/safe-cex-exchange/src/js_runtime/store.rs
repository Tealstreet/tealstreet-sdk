//! JS facade over the typed Store. Reads project a fresh value each access
//! (owned snapshots, not the TS live arrays); writes cross the typed boundary.
use super::{
    value::arg,
    value::{Getter, Js, Thrown, from_typed, to_json, to_typed},
};
use crate::runtime::ExchangeError;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{Order, OrderStatus, Position};
use std::rc::Rc;

fn mutation(e: safe_cex_store::StoreMutationError) -> Thrown {
    Thrown::from(ExchangeError::from(e))
}

fn ids(value: &Js) -> Result<Vec<String>, Thrown> {
    let mut out = vec![];
    for v in value.array_values().unwrap_or_default() {
        let id = if v.is_object() { v.get("id")? } else { v };
        out.push(id.to_display());
    }
    Ok(out)
}

fn patch(value: &Js) -> Result<serde_json::Map<String, serde_json::Value>, Thrown> {
    match to_json(value)? {
        serde_json::Value::Object(map) => Ok(map),
        _ => Ok(serde_json::Map::new()),
    }
}

pub fn facade(store: Rc<dyn Store>) -> Js {
    let s = Js::object_with_class("Store", vec![]);
    let getter = |name: &str, read: Rc<dyn Fn(&dyn Store) -> Result<Js, Thrown>>| {
        let store = store.clone();
        let g: Getter = Rc::new(move |_| read(store.as_ref()));
        s.define_accessor(name, Some(g), None);
    };
    getter("markets", Rc::new(|st| from_typed(&*st.markets())));
    getter("tickers", Rc::new(|st| from_typed(&*st.tickers())));
    getter("orders", Rc::new(|st| from_typed(&*st.orders())));
    getter("positions", Rc::new(|st| from_typed(&*st.positions())));
    getter("balance", Rc::new(|st| from_typed(&*st.balance())));
    getter("loaded", Rc::new(|st| from_typed(&*st.loaded())));
    getter("options", Rc::new(|st| from_typed(&*st.options())));
    getter("latency", Rc::new(|st| Ok(Js::Number(st.latency()))));
    getter(
        "marketsBySymbol",
        Rc::new(|st| from_typed(&*st.markets_by_symbol())),
    );
    getter(
        "marketsById",
        Rc::new(|st| from_typed(&*st.markets_by_id())),
    );
    getter(
        "tickersBySymbol",
        Rc::new(|st| from_typed(&*st.tickers_by_symbol())),
    );
    getter(
        "tickersById",
        Rc::new(|st| from_typed(&*st.tickers_by_id())),
    );
    getter(
        "fetchedLeverage",
        Rc::new(|st| from_typed(&*st.fetched_leverage())),
    );

    let method = |name: &str, f: Rc<dyn Fn(&dyn Store, Vec<Js>) -> Result<Js, Thrown>>| {
        let store = store.clone();
        let _ = s.define(name, Js::function(name, move |_, a| f(store.as_ref(), a)));
    };
    method(
        "update",
        Rc::new(|st, a| {
            let update: StoreUpdate = to_typed(&arg(&a, 0))?;
            st.update(update, false).map_err(mutation)?;
            Ok(Js::Undefined)
        }),
    );
    method(
        "reset",
        Rc::new(|st, _| {
            st.reset().map_err(mutation)?;
            Ok(Js::Undefined)
        }),
    );
    method(
        "addOrUpdatePosition",
        Rc::new(|st, a| {
            let p: Position = to_typed(&arg(&a, 0))?;
            st.add_or_update_position(p).map_err(mutation)?;
            Ok(Js::Undefined)
        }),
    );
    method(
        "addOrUpdatePositions",
        Rc::new(|st, a| {
            let p: Vec<Position> = to_typed(&arg(&a, 0))?;
            st.add_or_update_positions(p).map_err(mutation)?;
            Ok(Js::Undefined)
        }),
    );
    method(
        "addOrUpdatePositionsFull",
        Rc::new(|st, a| {
            let p: Vec<Position> = to_typed(&arg(&a, 0))?;
            st.add_or_update_positions_full(p).map_err(mutation)?;
            Ok(Js::Undefined)
        }),
    );
    method(
        "updatePosition",
        Rc::new(|st, a| {
            let id = arg(&a, 0).to_display();
            st.update_position(&id, &patch(&arg(&a, 1))?)
                .map_err(mutation)?;
            Ok(Js::Undefined)
        }),
    );
    method(
        "addOrder",
        Rc::new(|st, a| {
            let o: Order = to_typed(&arg(&a, 0))?;
            st.add_order(o).map_err(mutation)?;
            Ok(Js::Undefined)
        }),
    );
    method(
        "addOrUpdateOrder",
        Rc::new(|st, a| {
            let o: Order = to_typed(&arg(&a, 0))?;
            st.add_or_update_order(o).map_err(mutation)?;
            Ok(Js::Undefined)
        }),
    );
    method(
        "addOrUpdateOrders",
        Rc::new(|st, a| {
            let o: Vec<Order> = to_typed(&arg(&a, 0))?;
            st.add_or_update_orders(o).map_err(mutation)?;
            Ok(Js::Undefined)
        }),
    );
    method(
        "addOrUpdateOrdersFull",
        Rc::new(|st, a| {
            let o: Vec<Order> = to_typed(&arg(&a, 0))?;
            st.add_or_update_orders_full(o).map_err(mutation)?;
            Ok(Js::Undefined)
        }),
    );
    method(
        "setOrdersForSymbol",
        Rc::new(|st, a| {
            let symbol = arg(&a, 0).to_display();
            let o: Vec<Order> = to_typed(&arg(&a, 1))?;
            st.set_orders_for_symbol(&symbol, o).map_err(mutation)?;
            Ok(Js::Undefined)
        }),
    );
    method(
        "updateOrder",
        Rc::new(|st, a| {
            let id = if arg(&a, 0).is_object() {
                arg(&a, 0).get("id")?
            } else {
                arg(&a, 0)
            };
            st.update_order(&id.to_display(), &patch(&arg(&a, 1))?)
                .map_err(mutation)?;
            Ok(Js::Undefined)
        }),
    );
    method(
        "updateOrderStatus",
        Rc::new(|st, a| {
            let status: OrderStatus = to_typed(&arg(&a, 1))?;
            st.update_order_status(&arg(&a, 0).to_display(), status)
                .map_err(mutation)?;
            Ok(Js::Undefined)
        }),
    );
    method(
        "updateOrdersStatus",
        Rc::new(|st, a| {
            let orders = arg(&a, 0).array_values().unwrap_or_default();
            if orders.is_empty() || st.orders().is_empty() {
                return Ok(Js::Undefined);
            }
            // ts: buildIdLookup keys on `items[i].id`, so every element is read as an object.
            let mut keys = vec![];
            for order in orders {
                keys.push(String::from_utf16_lossy(&order.get("id")?.to_js_string()?));
            }
            let status: OrderStatus = to_typed(&arg(&a, 1))?;
            let refs: Vec<&str> = keys.iter().map(String::as_str).collect();
            st.update_orders_status(&refs, status).map_err(mutation)?;
            Ok(Js::Undefined)
        }),
    );
    method(
        "setSetting",
        Rc::new(|st, a| {
            let key = arg(&a, 0).to_display();
            if key != "isHedged" {
                return Err(Thrown::type_error(&format!(
                    "store options carry only isHedged (setting '{key}')"
                )));
            }
            let value: bool = to_typed(&arg(&a, 1))?;
            st.set_setting(&key, value).map_err(mutation)?;
            Ok(Js::Undefined)
        }),
    );
    method(
        "removeOrder",
        Rc::new(|st, a| {
            let id = if arg(&a, 0).is_object() {
                arg(&a, 0).get("id")?
            } else {
                arg(&a, 0)
            };
            st.remove_order(&id.to_display()).map_err(mutation)?;
            Ok(Js::Undefined)
        }),
    );
    method(
        "removeOrders",
        Rc::new(|st, a| {
            let ids = ids(&arg(&a, 0))?;
            let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
            st.remove_orders(&refs).map_err(mutation)?;
            Ok(Js::Undefined)
        }),
    );
    method(
        "updateTicker",
        Rc::new(|st, a| {
            st.update_ticker(&arg(&a, 0).to_display(), &patch(&arg(&a, 1))?)
                .map_err(mutation)?;
            Ok(Js::Undefined)
        }),
    );
    method(
        "updateMarket",
        Rc::new(|st, a| {
            st.update_market(&arg(&a, 0).to_display(), &patch(&arg(&a, 1))?)
                .map_err(mutation)?;
            Ok(Js::Undefined)
        }),
    );
    method(
        "updateSocketHealth",
        Rc::new(|st, a| {
            st.update_socket_health(&patch(&arg(&a, 0))?)
                .map_err(mutation)?;
            Ok(Js::Undefined)
        }),
    );
    s
}
