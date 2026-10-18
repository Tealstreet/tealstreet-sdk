//! JS facade over the typed Store. Reads project a fresh value each access
//! (owned snapshots, not the TS live arrays); writes cross the typed boundary.
use super::{
    value::arg,
    value::{Getter, Js, Thrown, from_typed, to_json, to_retained_input, to_typed},
};
use crate::runtime::ExchangeError;
use safe_cex_store::{Store, StoreUpdate, model_patch::ModelPatch};
use safe_cex_types::{OptionTerms, Order, OrderStatus, Position, PositionOption};
use std::rc::Rc;

// Source findIndex/Map lookups compare to the native model's string id without coercion.
fn strict_id_of(target: &Js) -> Result<Option<String>, Thrown> {
    Ok(match target.get("id")? {
        Js::String(id) => String::from_utf16(&id).ok(),
        _ => None,
    })
}

// Object-key Store lookups use property keys, rather than strict id equality.
fn property_id_of(target: &Js) -> Result<Option<String>, Thrown> {
    let id = target.get("id")?;
    // ToPropertyKey first uses the string hint, then preserves Symbol keys.
    let id = if matches!(id, Js::Object(_) | Js::Function(_)) {
        id.ordinary_to_primitive(["toString", "valueOf"])?
    } else {
        id
    };
    if matches!(id, Js::SymbolIterator) {
        return Ok(None);
    }
    Ok(String::from_utf16(&id.to_js_string()?).ok())
}

fn mutation(e: safe_cex_store::StoreMutationError) -> Thrown {
    Thrown::from(ExchangeError::from(e))
}

fn ids(value: &Js) -> Result<Vec<String>, Thrown> {
    let mut out = vec![];
    for v in value.array_values().unwrap_or_default() {
        if let Some(id) = property_id_of(&v)? {
            out.push(id);
        }
    }
    Ok(out)
}

fn json_patch(value: &Js) -> Result<serde_json::Map<String, serde_json::Value>, Thrown> {
    match to_json(value)? {
        serde_json::Value::Object(map) => Ok(map),
        _ => Ok(serde_json::Map::new()),
    }
}

// Scalar model numbers use the same native typed bridge as complete models.
// Other fields retain their existing JSON admission and omission policy.
fn patch(value: &Js) -> Result<ModelPatch, Thrown> {
    if !matches!(value, Js::Object(_) | Js::Builtin(_)) || value.is_callable() {
        return json_patch(value).map(ModelPatch::from);
    }
    let fields = Js::object(vec![]);
    let mut numbers = vec![];
    let mut undefined = vec![];
    let mut info = None;
    for (key, value) in value.own_entries()? {
        let field = String::from_utf16_lossy(&key);
        if field == "info" && !value.is_undefined() {
            info = Some(to_retained_input(&value)?);
        } else if value.is_undefined() {
            undefined.push(field);
        } else if matches!(value, Js::Number(_)) {
            numbers.push((field, to_typed::<f64>(&value)?));
        } else {
            fields.define_key(&key, value)?;
        }
    }
    let mut changes = ModelPatch::from(json_patch(&fields)?);
    if let Some(info) = info {
        changes.set_native("info", info);
    }
    for key in undefined {
        changes.set_undefined(key);
    }
    for (key, value) in numbers {
        changes.set_number(key, value);
    }
    Ok(changes)
}

// Option terms are a nested native typed field, including nonfinite numbers.
fn option_patch<T: serde::de::DeserializeOwned + Send + Sync + 'static>(
    value: &Js,
) -> Result<ModelPatch, Thrown> {
    let fields = value.spread_object()?;
    let option = fields.get("option")?;
    fields.delete(&super::value::utf16("option"));
    let mut changes = patch(&fields)?;
    if !option.is_undefined() {
        changes.set_native("option", to_typed::<T>(&option)?);
    }
    Ok(changes)
}

fn position_patch(value: &Js) -> Result<ModelPatch, Thrown> {
    let mut changes = option_patch::<PositionOption>(value)?;
    if value.has_own("option") && value.get("option")?.is_undefined() {
        changes.set_undefined("option");
    }
    Ok(changes)
}

pub fn facade(store: Rc<dyn Store>) -> Js {
    let s = Js::object_with_class("Store", vec![]);
    let getter = |name: &str, read: Rc<dyn Fn(&dyn Store) -> Result<Js, Thrown>>| {
        let store = store.clone();
        let g: Getter = Rc::new(move |_| read(store.as_ref()));
        s.define_accessor(name, Some(g), None)
            .expect("accessor target is a fresh extensible host object");
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
            st.update(update, arg(&a, 1).truthy()).map_err(mutation)?;
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
            if st.positions().is_empty() {
                return Ok(Js::Undefined);
            }
            let Some(id) = strict_id_of(&arg(&a, 0))? else {
                return Ok(Js::Undefined);
            };
            st.update_position(&id, &position_patch(&arg(&a, 1))?)
                .map_err(mutation)?;
            Ok(Js::Undefined)
        }),
    );
    method(
        "updatePositions",
        Rc::new(|st, a| {
            let pairs = arg(&a, 0).array_values().unwrap_or_default();
            let positions = st.positions().to_vec();
            if pairs.is_empty() || positions.is_empty() {
                return Ok(Js::Undefined);
            }
            let indexed = pairs.len() > 1 && positions.len() > 1;
            // Keep every selector row: removing unmatched keys could change
            // the core Store's index mode and duplicate-ID selection.
            let mut selected = vec![];
            for pair in pairs {
                let (position, changes) = (pair.get("0")?, pair.get("1")?);
                let id = if indexed {
                    property_id_of(&position)?
                } else {
                    strict_id_of(&position)?
                };
                selected.push((id, changes));
            }
            let mut updates = vec![];
            for (id, changes) in selected {
                let patch = if id
                    .as_ref()
                    .is_some_and(|id| positions.iter().any(|position| &position.id == id))
                {
                    position_patch(&changes)?
                } else {
                    // Source updateInArray never reads changes for a missing row.
                    ModelPatch::default()
                };
                updates.push((id, patch));
            }
            drop(positions);
            st.update_positions(&updates).map_err(mutation)?;
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
            if st.orders().is_empty() {
                return Ok(Js::Undefined);
            }
            let Some(id) = strict_id_of(&arg(&a, 0))? else {
                return Ok(Js::Undefined);
            };
            st.update_order(&id, &patch(&arg(&a, 1))?)
                .map_err(mutation)?;
            Ok(Js::Undefined)
        }),
    );
    method(
        "updateOrderStatus",
        Rc::new(|st, a| {
            if st.orders().is_empty() {
                return Ok(Js::Undefined);
            }
            let Some(id) = strict_id_of(&arg(&a, 0))? else {
                return Ok(Js::Undefined);
            };
            let status: OrderStatus = to_typed(&arg(&a, 1))?;
            st.update_order_status(&id, status).map_err(mutation)?;
            Ok(Js::Undefined)
        }),
    );
    method(
        "markOrdersTerminal",
        Rc::new(|st, a| {
            let orders = arg(&a, 0)
                .array_values()
                .ok_or_else(|| Thrown::type_error("orders.forEach is not a function"))?;
            let mut keys = vec![];
            for order in orders {
                keys.push(to_typed::<String>(&order.get("id")?)?);
            }
            let status: OrderStatus = to_typed(&arg(&a, 1))?;
            let refs: Vec<&str> = keys.iter().map(String::as_str).collect();
            // Remember IDs even before any matching order reaches the Store.
            st.mark_orders_terminal(&refs, status).map_err(mutation)?;
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
            if st.orders().is_empty() {
                return Ok(Js::Undefined);
            }
            let Some(id) = strict_id_of(&arg(&a, 0))? else {
                return Ok(Js::Undefined);
            };
            st.remove_order(&id).map_err(mutation)?;
            Ok(Js::Undefined)
        }),
    );
    method(
        "removeOrders",
        Rc::new(|st, a| {
            let orders = arg(&a, 0);
            if orders.array_values().is_some_and(|rows| rows.is_empty()) || st.orders().is_empty() {
                return Ok(Js::Undefined);
            }
            let ids = ids(&orders)?;
            let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
            st.remove_orders(&refs).map_err(mutation)?;
            Ok(Js::Undefined)
        }),
    );
    method(
        "updateTicker",
        Rc::new(|st, a| {
            let Some(id) = property_id_of(&arg(&a, 0))? else {
                return Ok(Js::Undefined);
            };
            st.update_ticker(&id, &patch(&arg(&a, 1))?)
                .map_err(mutation)?;
            Ok(Js::Undefined)
        }),
    );
    method(
        "updateMarket",
        Rc::new(|st, a| {
            let Some(id) = strict_id_of(&arg(&a, 0))? else {
                return Ok(Js::Undefined);
            };
            st.update_market(&id, &option_patch::<OptionTerms>(&arg(&a, 1))?)
                .map_err(mutation)?;
            Ok(Js::Undefined)
        }),
    );
    method(
        "updateSocketHealth",
        Rc::new(|st, a| {
            st.update_socket_health(&json_patch(&arg(&a, 0))?)
                .map_err(mutation)?;
            Ok(Js::Undefined)
        }),
    );
    s
}
