//! Symbol and string keys occupy different slots in the actual Store dictionaries.
#[path = "support/store_correctness_source.rs"]
mod store_correctness_source;
use safe_cex_exchange::{
    js_runtime::{
        store,
        value::{Js, from_json},
    },
    native_store::DefaultStore,
};
use serde_json::json;
use std::{cell::Cell, rc::Rc};
#[test]
fn symbol_ids_remain_distinct_after_computed_property_coercion() {
    let source = store_correctness_source::capture();
    for array_key in [false, true] {
        let facade = store::facade(Rc::new(DefaultStore::new())).unwrap();
        let order = |id| {
            let value = from_json(&json!({"id":"x","status":"open"}));
            value.set("id", id).unwrap();
            value
        };
        let position = |id| {
            let value = from_json(&json!({"id":"x","contracts":0}));
            value.set("id", id).unwrap();
            value
        };
        facade
            .call_method(
                "update",
                vec![Js::object(vec![
                    (
                        "orders",
                        Js::array(vec![
                            order(Js::SymbolIterator),
                            order(Js::str("Symbol(Symbol.iterator)")),
                        ]),
                    ),
                    (
                        "positions",
                        Js::array(vec![
                            position(Js::SymbolIterator),
                            position(Js::str("Symbol(Symbol.iterator)")),
                        ]),
                    ),
                ])],
            )
            .unwrap();
        let calls = Rc::new(Cell::new(0));
        let seen = calls.clone();
        let key = if array_key {
            Js::array(vec![])
        } else {
            Js::object(vec![])
        };
        key.set(
            "toString",
            Js::function("symbolKey", move |_, _| {
                seen.set(seen.get() + 1);
                Ok(Js::SymbolIterator)
            }),
        )
        .unwrap();
        let selector = Js::object(vec![("id", key)]);
        facade
            .call_method("removeOrders", vec![Js::array(vec![selector.clone()])])
            .unwrap();
        facade
            .call_method(
                "updatePositions",
                vec![Js::array(vec![
                    Js::array(vec![selector, from_json(&json!({"contracts":7}))]),
                    Js::array(vec![
                        from_json(&json!({"id":"Symbol(Symbol.iterator)"})),
                        from_json(&json!({"contracts":9})),
                    ]),
                ])],
            )
            .unwrap();
        let orders = facade
            .get("orders")
            .unwrap()
            .array_values()
            .unwrap()
            .unwrap();
        let positions = facade
            .get("positions")
            .unwrap()
            .array_values()
            .unwrap()
            .unwrap();
        assert_eq!(
            json!({"ids":orders.iter().map(|row|row.get("id").unwrap().to_display()).collect::<Vec<_>>(),
      "contracts":positions.iter().map(|row|row.get("contracts").unwrap().as_f64().unwrap() as i32).collect::<Vec<_>>(),
      "conversions":calls.get()}),
            source["symbols"][usize::from(array_key)]
        );
    }
}

#[test]
fn indexed_upsert_preserves_source_assignment_coercion_order() {
    let source = store_correctness_source::capture();
    let facade = store::facade(Rc::new(DefaultStore::new())).unwrap();
    let seed = |id| {
        let row = from_json(&json!({"id":"x","status":"open","amount":1}));
        row.set("id", id).unwrap();
        row
    };
    facade
        .call_method(
            "update",
            vec![Js::object(vec![(
                "orders",
                Js::array(vec![seed(Js::str("a")), seed(Js::str("b"))]),
            )])],
        )
        .unwrap();
    let Js::Object(owner) = &facade else {
        unreachable!()
    };
    let weak = Rc::downgrade(owner);
    let calls = Rc::new(Cell::new(0));
    let seen = calls.clone();
    let key = Js::object(vec![(
        "toString",
        Js::function("grow", move |_, _| {
            seen.set(seen.get() + 1);
            if seen.get() == 2 {
                let facade = Js::Object(weak.upgrade().unwrap());
                facade.get("orders")?.call_method(
                    "push",
                    vec![from_json(
                        &json!({"id":"filler","status":"open","amount":1}),
                    )],
                )?;
            }
            Ok(Js::str("new"))
        }),
    )]);
    let first = seed(key);
    first.set("amount", Js::Number(7.)).unwrap();
    let second = seed(Js::str("new"));
    second.set("amount", Js::Number(9.)).unwrap();
    facade
        .call_method("addOrUpdateOrders", vec![Js::array(vec![first, second])])
        .unwrap();
    let rows = facade
        .get("orders")
        .unwrap()
        .array_values()
        .unwrap()
        .unwrap();
    assert_eq!(
        json!({"rows":rows.iter().map(|row|{let id=row.get("id").unwrap();json!({"id":if id.is_string(){id.to_display()}else{"object".into()},"amount":row.get("amount").unwrap().as_f64().unwrap() as i32})}).collect::<Vec<_>>(),"conversions":calls.get()}),
        source["upsert"]
    );
}
