//! The JS `store` object over the Rust Store.
//!
//! CHOSEN: reads are snapshots. `store.orders` returns a fresh array of fresh
//! order objects, so mutating a returned object does not write the Store; the
//! ported bodies write through the Store methods, which is what persists.
use super::{
    Realm,
    value::{Exotic, Kind, PropKey, Thrown, Value},
};
use safe_cex_store::{Store, StoreUpdate, model_patch::ModelPatch};
use safe_cex_types::{Order, Position};
use serde_json::Value as Json;
use std::rc::Rc;

#[derive(Clone, Copy)]
enum Table {
    MarketsBySymbol,
    MarketsById,
    TickersBySymbol,
    TickersById,
}

struct TableView {
    store: Rc<dyn Store>,
    table: Table,
}

impl TableView {
    fn native_map(
        &self,
        realm: &mut Realm,
    ) -> Result<safe_cex_exchange::js_runtime::value::Js, Thrown> {
        use safe_cex_types::{StoreReadError, StoreReadPhase};
        let field = match self.table {
            Table::MarketsBySymbol => "marketsBySymbol",
            Table::MarketsById => "marketsById",
            Table::TickersBySymbol => "tickersBySymbol",
            Table::TickersById => "tickersById",
        };
        safe_cex_exchange::native_store::facade(self.store.clone())
            .and_then(|store| store.get(field))
            .map_err(|cause| {
                realm.from_exchange_error(
                    StoreReadError::new(StoreReadPhase::Property, Rc::new(cause)).into(),
                )
            })
    }
}
impl Exotic for TableView {
    fn lookup(&self, realm: &mut Realm, key: &PropKey) -> Result<Option<Value>, Thrown> {
        use safe_cex_types::{StoreReadError, StoreReadPhase};
        let Some(units) = key.units() else {
            return Ok(None);
        };
        let map = self.native_map(realm)?;
        if !map
            .own_property_names()
            .map_err(|cause| {
                realm.from_exchange_error(
                    StoreReadError::new(StoreReadPhase::Property, Rc::new(cause)).into(),
                )
            })?
            .iter()
            .any(|key| key.as_ref() == units)
        {
            return Ok(None);
        }
        let row = map.get_key(units).map_err(|cause| {
            realm.from_exchange_error(
                StoreReadError::new(StoreReadPhase::Property, Rc::new(cause)).into(),
            )
        })?;
        // Preserve the legacy successful DTO snapshot boundary, but project only
        // the reached row. Unrelated rows and their getters remain untouched.
        fn selected<T: serde::de::DeserializeOwned + serde::Serialize>(
            realm: &mut Realm,
            row: &safe_cex_exchange::js_runtime::value::Js,
        ) -> Result<Option<Value>, Thrown> {
            let value: T = safe_cex_exchange::js_runtime::value::to_typed_preserving_undefined(row)
                .map_err(|cause| {
                    realm.from_exchange_error(
                        StoreReadError::new(StoreReadPhase::Projection, Rc::new(cause)).into(),
                    )
                })?;
            realm.from_typed(&value).map(Some)
        }
        match self.table {
            Table::MarketsBySymbol | Table::MarketsById => {
                selected::<safe_cex_types::Market>(realm, &row)
            }
            Table::TickersBySymbol | Table::TickersById => {
                selected::<safe_cex_types::Ticker>(realm, &row)
            }
        }
    }
    fn keys(&self, realm: &mut Realm) -> Result<Vec<PropKey>, Thrown> {
        Ok(self
            .native_map(realm)?
            .own_keys()
            .into_iter()
            .map(|units| PropKey::String(units.into()))
            .collect())
    }
}

fn arg(args: &[Value], i: usize) -> Value {
    args.get(i).cloned().unwrap_or(Value::Undefined)
}
fn mutation(
    r: &mut Realm,
    result: Result<(), safe_cex_store::StoreMutationError>,
) -> Result<Value, Thrown> {
    result
        .map(|()| Value::Undefined)
        .map_err(|e| r.from_exchange_error(e.into()))
}
fn id_of(r: &mut Realm, selector: &Value) -> Result<String, Thrown> {
    let id = r.get_str(selector, "id")?;
    r.to_rust_string(&id)
}
// Model fields cross the native patch boundary before JSON can omit undefined
// or turn a nonfinite scalar into null. Opaque metadata retains its JSON policy.
fn patch(r: &mut Realm, changes: &Value) -> Result<ModelPatch, Thrown> {
    if !matches!(changes, Value::Object(_)) || changes.is_callable() {
        return Err(r.type_error("store patch must be an object"));
    }
    let fields = r.new_object();
    let mut native = ModelPatch::default();
    for (key, value) in r.own_entries(changes)? {
        let Some(units) = key.units() else { continue };
        let field = r.to_rust_string(&Value::String(units.into()))?;
        match value {
            Value::Undefined => native.set_undefined(field),
            Value::Number(number) if field != "info" => native.set_number(field, number),
            value => r.define_str(&fields, &field, value)?,
        }
    }
    let Json::Object(fields) = r.to_serde(&fields)? else {
        return Err(r.type_error("store patch must be an object"));
    };
    let mut changes = ModelPatch::from(fields);
    changes.0.extend(native.0);
    Ok(changes)
}

fn snapshot<T: serde::Serialize + ?Sized>(r: &mut Realm, value: &T) -> Result<Value, Thrown> {
    r.from_typed(&value)
}

pub(crate) fn store_object(realm: &Realm, store: Rc<dyn Store>) -> Value {
    let o = realm.new_object();
    let getter =
        |name: &str, body: Rc<dyn Fn(&mut Realm, &Rc<dyn Store>) -> Result<Value, Thrown>>| {
            let store = store.clone();
            let f = realm.native(&format!("get {name}"), 0, move |r, _, _| body(r, &store));
            realm.define_getter(&o, PropKey::str(name), f);
        };
    getter(
        "orders",
        Rc::new(|r, s| {
            let value = s
                .orders()
                .map_err(|error| r.from_exchange_error(error.into()))?;
            snapshot(r, &value)
        }),
    );
    getter(
        "positions",
        Rc::new(|r, s| {
            let value = s
                .positions()
                .map_err(|error| r.from_exchange_error(error.into()))?;
            snapshot(r, &value)
        }),
    );
    getter(
        "markets",
        Rc::new(|r, s| {
            let value = s
                .markets()
                .map_err(|error| r.from_exchange_error(error.into()))?;
            snapshot(r, &value)
        }),
    );
    getter(
        "tickers",
        Rc::new(|r, s| {
            let value = s
                .tickers()
                .map_err(|error| r.from_exchange_error(error.into()))?;
            snapshot(r, &value)
        }),
    );
    getter(
        "balance",
        Rc::new(|r, s| {
            let value = s
                .balance()
                .map_err(|error| r.from_exchange_error(error.into()))?;
            snapshot(r, &value)
        }),
    );
    getter(
        "loaded",
        Rc::new(|r, s| {
            let value = s
                .loaded()
                .map_err(|error| r.from_exchange_error(error.into()))?;
            snapshot(r, &value)
        }),
    );
    getter(
        "latency",
        Rc::new(|_r, s| {
            Ok(Value::Number(
                s.latency()
                    .map_err(|error| _r.from_exchange_error(error.into()))?,
            ))
        }),
    );
    for (name, table) in [
        ("marketsBySymbol", Table::MarketsBySymbol),
        ("marketsById", Table::MarketsById),
        ("tickersBySymbol", Table::TickersBySymbol),
        ("tickersById", Table::TickersById),
    ] {
        getter(
            name,
            Rc::new(move |r, s| {
                Ok(r.alloc(
                    Kind::Exotic(Rc::new(TableView {
                        store: s.clone(),
                        table,
                    })),
                    Some(&r.intrinsics().object_proto),
                ))
            }),
        );
    }
    let method =
        |name: &str,
         length: u8,
         body: Rc<dyn Fn(&mut Realm, &Rc<dyn Store>, &[Value]) -> Result<Value, Thrown>>| {
            let store = store.clone();
            let f = realm.native(name, length, move |r, _, args| body(r, &store, args));
            realm.define_hidden(&o, PropKey::str(name), f);
        };
    method(
        "update",
        2,
        Rc::new(|r, s, a| {
            let changes: StoreUpdate = r.to_typed(&arg(a, 0))?;
            let from_worker = arg(a, 1).truthy();
            mutation(r, s.update(changes, from_worker))
        }),
    );
    method(
        "addOrder",
        1,
        Rc::new(|r, s, a| {
            let order: Order = r.to_typed(&arg(a, 0))?;
            mutation(r, s.add_order(order))
        }),
    );
    method(
        "addOrUpdateOrder",
        1,
        Rc::new(|r, s, a| {
            let order: Order = r.to_typed(&arg(a, 0))?;
            mutation(r, s.add_or_update_order(order))
        }),
    );
    method(
        "updateOrder",
        2,
        Rc::new(|r, s, a| {
            let id = id_of(r, &arg(a, 0))?;
            let changes = patch(r, &arg(a, 1))?;
            mutation(r, s.update_order(&id, &changes))
        }),
    );
    method(
        "removeOrder",
        1,
        Rc::new(|r, s, a| {
            let id = id_of(r, &arg(a, 0))?;
            mutation(r, s.remove_order(&id))
        }),
    );
    method(
        "removeOrders",
        1,
        Rc::new(|r, s, a| {
            let mut ids = vec![];
            for o in r.collect_iterable(&arg(a, 0))? {
                ids.push(id_of(r, &o)?);
            }
            let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
            mutation(r, s.remove_orders(&ids))
        }),
    );
    method(
        "updatePosition",
        2,
        Rc::new(|r, s, a| {
            let id = id_of(r, &arg(a, 0))?;
            let changes = patch(r, &arg(a, 1))?;
            mutation(r, s.update_position(&id, &changes))
        }),
    );
    method(
        "addOrUpdatePosition",
        1,
        Rc::new(|r, s, a| {
            let position: Position = r.to_typed(&arg(a, 0))?;
            mutation(r, s.add_or_update_position(position))
        }),
    );
    method(
        "addOrUpdatePositions",
        1,
        Rc::new(|r, s, a| {
            let positions: Vec<Position> = r.to_typed(&arg(a, 0))?;
            mutation(r, s.add_or_update_positions(positions))
        }),
    );
    method(
        "addOrUpdatePositionsFull",
        1,
        Rc::new(|r, s, a| {
            let positions: Vec<Position> = r.to_typed(&arg(a, 0))?;
            mutation(r, s.add_or_update_positions_full(positions))
        }),
    );
    method(
        "updateTicker",
        2,
        Rc::new(|r, s, a| {
            let id = id_of(r, &arg(a, 0))?;
            let changes = patch(r, &arg(a, 1))?;
            mutation(r, s.update_ticker(&id, &changes))
        }),
    );
    method(
        "updateMarket",
        2,
        Rc::new(|r, s, a| {
            let id = id_of(r, &arg(a, 0))?;
            let changes = patch(r, &arg(a, 1))?;
            mutation(r, s.update_market(&id, &changes))
        }),
    );
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::realm::test_runtime::TestRuntime;
    use safe_cex_exchange::native_store::DefaultStore;
    use serde_json::json;

    #[test]
    fn production_store_facade_retains_native_patch_numbers_and_undefined() {
        let mut realm = Realm::new(Rc::new(TestRuntime::default()));
        let store = Rc::new(DefaultStore::new());
        store
            .add_order(
                serde_json::from_value(json!({
                    "id":"o", "symbol":"BTC-PERP", "amount":1, "filled":0,
                    "remaining":1, "reduceOnly":false, "lastUpdate":1, "price":7
                }))
                .unwrap(),
            )
            .unwrap();
        let facade = store_object(&realm, store.clone());
        let update = realm.get_str(&facade, "updateOrder").unwrap();
        let selector = realm.object_from(vec![("id", Value::str("o"))]);
        for value in [-0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let changes = realm.object_from(vec![("price", Value::Number(value))]);
            realm
                .call(&update, &facade, &[selector.clone(), changes])
                .unwrap();
            assert_eq!(
                store.orders()?[0].price.as_ref().unwrap().to_bits(),
                value.to_bits()
            );
        }
        let changes = realm.object_from(vec![("price", Value::Undefined)]);
        realm
            .call(&update, &facade, &[selector.clone(), changes])
            .unwrap();
        assert!(store.orders()?[0].price.is_undefined());
        let omitted = realm.new_object();
        realm.call(&update, &facade, &[selector, omitted]).unwrap();
        assert!(
            store.orders()?[0].price.is_undefined(),
            "an absent patch preserves the field"
        );
    }
}
