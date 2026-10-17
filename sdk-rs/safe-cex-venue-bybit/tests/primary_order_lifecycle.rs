// Field tables written in the loop shape their siblings use, so adding a second
// field is one row rather than a restructure. One row today is not a reason to
// unroll the table.
#![allow(clippy::single_element_loop)]
use safe_cex_types::{Order, OrderSide, OrderStatus, OrderTimeInForce, OrderType};
use safe_cex_venue_bybit::primary_order_lifecycle::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};
type Error = Rc<Value>;
fn number(actual: f64, expected: &Value, field: &str) {
    let bits = if actual.is_nan() {
        "nan".into()
    } else {
        format!("{:016x}", actual.to_bits())
    };
    assert_eq!(
        bits,
        expected["number"].as_str().unwrap(),
        "PRIMARY native number {field}"
    );
}
fn order(actual: &Order, expected: &Value) {
    assert_eq!(
        actual.id,
        expected["id"].as_str().unwrap(),
        "PRIMARY native order id"
    );
    assert_eq!(
        actual.symbol,
        expected["symbol"].as_str().unwrap(),
        "PRIMARY native order symbol"
    );
    assert_eq!(
        actual.r#type,
        serde_json::from_value::<OrderType>(expected["type"].clone()).unwrap(),
        "PRIMARY native order type"
    );
    assert_eq!(
        actual.status,
        serde_json::from_value::<OrderStatus>(expected["status"].clone()).unwrap(),
        "PRIMARY native order status"
    );
    assert_eq!(
        actual.side,
        serde_json::from_value::<OrderSide>(expected["side"].clone()).unwrap(),
        "PRIMARY native order side"
    );
    for (name, value) in [
        ("price", actual.price.to_number()),
        ("amount", actual.amount),
        ("filled", actual.filled),
        ("remaining", actual.remaining),
        ("lastUpdate", actual.last_update),
    ] {
        number(value, &expected[name], name);
    }
    assert_eq!(
        actual.reduce_only,
        expected["reduceOnly"].as_bool().unwrap(),
        "PRIMARY native reduce only"
    );
    assert_eq!(
        actual.time_in_force,
        Some(serde_json::from_value::<OrderTimeInForce>(expected["timeInForce"].clone()).unwrap()),
        "PRIMARY native time in force"
    );
}
fn mutate(order: &Rc<RefCell<Order>>, fields: &Value) {
    let mut order = order.borrow_mut();
    if let Some(value) = fields.get("id") {
        order.id = value.as_str().unwrap().into();
    }
    if let Some(value) = fields.get("symbol") {
        order.symbol = value.as_str().unwrap().into();
    }
    if let Some(value) = fields.get("reduceOnly") {
        order.reduce_only = value.as_bool().unwrap().into();
    }
    if let Some(value) = fields.get("type") {
        order.r#type = serde_json::from_value(value.clone()).unwrap();
    }
    for (name, target) in [("amount", &mut order.amount)] {
        if let Some(value) = fields.get(name) {
            *target = value.as_f64().unwrap();
        }
    }
    if let Some(value) = fields.get("price") {
        order.price = (value.as_f64().unwrap()).into();
    }
    if let Some(value) = fields.get("filled") {
        order.filled = value.as_f64().unwrap();
    }
    if let Some(value) = fields.get("remaining") {
        order.remaining = value.as_f64().unwrap();
    }
    if let Some(value) = fields.get("lastUpdate") {
        order.last_update = value.as_f64().unwrap();
    }
}

#[derive(Debug)]
struct Event {
    kind: String,
    recent: Vec<String>,
    timer_keys: Vec<String>,
    retired: Vec<String>,
    refs: Vec<String>,
    ids: Vec<String>,
    numbers: Vec<f64>,
}
struct Host {
    state: PrimaryOrderState<String>,
    registry: BTreeMap<String, OrderHandle>,
    current: RefCell<Value>,
    events: RefCell<Vec<Event>>,
    errors: BTreeMap<String, Error>,
    store: RefCell<OrderList>,
    fetched: RefCell<OrderList>,
    callbacks: RefCell<BTreeMap<String, TimerCallback<'static, Error>>>,
    timer_seq: Cell<usize>,
    inserted: Cell<bool>,
}
impl Host {
    fn new(case: &Value) -> Rc<Self> {
        Rc::new(Self{state:PrimaryOrderState::default(),registry:case["orders"].as_object().unwrap().iter().map(|(key,v)|(key.clone(),Rc::new(RefCell::new(serde_json::from_value(v.clone()).unwrap())))).collect(),current:RefCell::new(Value::Null),events:RefCell::new(vec![]),errors:["demo","site","fallback","should","clear","set","clock","add","schedule","mark","remove","store","reconcileGet","reconcile"].into_iter().map(|p|(p.into(),Rc::new(json!({"name":format!("{p}Error"),"message":format!("{p} failed"),"marker":42})))).collect(),store:RefCell::new(Rc::new(RefCell::new(vec![]))),fetched:RefCell::new(Rc::new(RefCell::new(vec![]))),callbacks:RefCell::new(BTreeMap::new()),timer_seq:Cell::new(0),inserted:Cell::new(false)})
    }
    fn key(&self, order: &OrderHandle) -> String {
        self.registry
            .iter()
            .find(|(_, v)| Rc::ptr_eq(v, order))
            .unwrap()
            .0
            .clone()
    }
    fn keys(&self) -> Vec<String> {
        self.state.recent().into_iter().map(|(id, _)| id).collect()
    }
    fn event(
        &self,
        kind: &str,
        refs: Vec<OrderHandle>,
        ids: Vec<String>,
        numbers: Vec<f64>,
    ) -> Result<(), Error> {
        self.events.borrow_mut().push(Event {
            kind: kind.into(),
            recent: self.keys(),
            timer_keys: self.state.timers().into_iter().map(|(id, _)| id).collect(),
            retired: self.state.retired().into_iter().map(|(id, _)| id).collect(),
            refs: refs.iter().map(|v| self.key(v)).collect(),
            ids,
            numbers,
        });
        if self.current.borrow()["fail"] == kind {
            Err(self.errors[kind].clone())
        } else {
            Ok(())
        }
    }
    fn simple(&self, kind: &str) -> Result<(), Error> {
        self.event(kind, vec![], vec![], vec![])
    }
    fn list(&self, keys: &Value) -> OrderList {
        Rc::new(RefCell::new(
            keys.as_array()
                .map(|keys| {
                    keys.iter()
                        .map(|k| self.registry[k.as_str().unwrap()].clone())
                        .collect()
                })
                .unwrap_or_default(),
        ))
    }
    fn now(&self) -> Result<f64, Error> {
        self.simple("clock")?;
        let c = self.current.borrow().clone();
        if let Some(change) = c.get("clockMutate") {
            mutate(
                &self.registry[change["key"].as_str().unwrap()],
                &change["fields"],
            );
        }
        if let Some(key) = c["clockAdd"].as_str() {
            self.insert(key);
        }
        Ok(safe_cex_num::number_from_string(
            c.get("time").and_then(Value::as_str).unwrap_or("0"),
        ))
    }
    fn insert(&self, key: &str) {
        let order = self.registry[key].clone();
        let id = order.borrow().id.clone();
        self.state.put_recent(
            id,
            RecentEntry {
                placed_at: 0.,
                order,
            },
        );
    }
    fn fallback(&self) -> Result<bool, Error> {
        self.simple("fallback")?;
        let c = self.current.borrow();
        if let Some(fields) = c.get("fallbackMutate") {
            mutate(
                &self.registry[c.get("key").and_then(Value::as_str).unwrap_or("A")],
                fields,
            );
        }
        Ok(c["fallback"] == true)
    }
    fn clear_timeout(&self, timer: String) -> Result<(), Error> {
        self.event("clear", vec![], vec![timer], vec![])?;
        let c = self.current.borrow();
        let id = c
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| self.registry["A"].borrow().id.clone());
        let list = self
            .state
            .timers()
            .into_iter()
            .find(|(key, _)| key == &id)
            .map(|(_, v)| v);
        if c["appendTimer"] == true && !self.inserted.replace(true) {
            if let Some(list) = &list {
                list.borrow_mut().push("extra".into());
            }
        }
        if c["spliceTimer"] == true && !self.inserted.replace(true) {
            if let Some(list) = &list {
                if !list.borrow().is_empty() {
                    list.borrow_mut().remove(0);
                }
            }
        }
        if c["replaceTimer"] == true && !self.inserted.replace(true) {
            self.state
                .put_timers(id, Rc::new(RefCell::new(vec!["replacement".into()])));
        }
        Ok(())
    }
    fn clear_method(&self, id: &str) -> Result<(), Error> {
        self.event("clearMethod", vec![], vec![id.into()], vec![])?;
        if let Some(id) = self.current.borrow()["clearMutate"].as_str() {
            self.registry["A"].borrow_mut().id = id.into();
        }
        clear_recently_placed_order_reconciliation_timers(&self.state, id, |timer| {
            self.clear_timeout(timer)
        })
    }
    fn mark(&self, id: &str) -> Result<(), Error> {
        self.event("mark", vec![], vec![id.into()], vec![])?;
        mark_order_seen(&self.state, id, |id| self.clear_method(id))?;
        let c = self.current.borrow();
        if let Some(change) = c.get("markMutate") {
            mutate(
                &self.registry[change["key"].as_str().unwrap()],
                &change["fields"],
            );
        }
        if let Some(key) = c["markAdd"].as_str() {
            if !self.inserted.replace(true) {
                self.insert(key);
            }
        }
        Ok(())
    }
    fn should(&self, order: &OrderHandle) -> Result<bool, Error> {
        self.event("should", vec![order.clone()], vec![], vec![])?;
        if let Some(value) = self.current.borrow()["should"].as_bool() {
            return Ok(value);
        }
        should_reconcile_local_reduce_only_limit_order(order, || self.fallback())
    }
    fn schedule(self: &Rc<Self>, order: OrderHandle) -> Result<(), Error> {
        let weak = Rc::downgrade(self);
        let getter: ReconcileGetter<'static, Rc<String>, Error> = Rc::new(move || {
            let h = weak.upgrade().unwrap();
            h.simple("reconcileGet")?;
            if let Some(id) = h.current.borrow()["getterMutate"].as_str() {
                h.registry["A"].borrow_mut().id = id.into();
            }
            let weak = Rc::downgrade(&h);
            Ok(Rc::new(move |id: &str| {
                let h = weak.upgrade().unwrap();
                h.event("reconcile", vec![], vec![id.into()], vec![])?;
                Ok(Rc::new("return-token".into()))
            }))
        });
        schedule_local_reduce_only_limit_order_reconciliation(
            &self.state,
            order,
            |o| self.should(o),
            |id| self.clear_method(id),
            |callback, delay| {
                let id = format!("t{}", self.timer_seq.get());
                self.timer_seq.set(self.timer_seq.get() + 1);
                self.event("set", vec![], vec![id.clone()], vec![delay])?;
                self.callbacks.borrow_mut().insert(id.clone(), callback);
                if let Some(id) = self.current.borrow()["setMutate"].as_str() {
                    self.registry["A"].borrow_mut().id = id.into();
                }
                Ok(id)
            },
            getter,
        )
    }
    fn schedule_method(self: &Rc<Self>, order: OrderHandle) -> Result<(), Error> {
        self.event("schedule", vec![order.clone()], vec![], vec![])?;
        self.schedule(order)
    }
    fn add(&self, rows: OrderList) -> Result<(), Error> {
        self.event("add", rows.borrow().clone(), vec![], vec![])?;
        if self.current.borrow()["addSeen"] == true {
            let id = rows.borrow()[0].borrow().id.clone();
            self.mark(&id)?;
        }
        Ok(())
    }
    fn remove(&self, rows: RemovalList) -> Result<(), Error> {
        let rows = rows.borrow();
        let ids = rows
            .iter()
            .map(|record| record.borrow().id.clone())
            .collect();
        for i in 0..rows.len() {
            for j in i + 1..rows.len() {
                assert!(
                    !Rc::ptr_eq(&rows[i], &rows[j]),
                    "PRIMARY native distinct removal id records"
                );
            }
        }
        self.event("remove", vec![], ids, vec![])
    }
    fn store(&self) -> Result<OrderList, Error> {
        self.simple("store")?;
        let c = self.current.borrow();
        if let Some(key) = c["storeAdd"].as_str() {
            if !self.inserted.replace(true) {
                self.insert(key);
            }
        }
        if let Some(key) = c["fetchedAdd"].as_str() {
            if !self.inserted.replace(true) {
                self.fetched
                    .borrow()
                    .borrow_mut()
                    .push(self.registry[key].clone());
            }
        }
        if let Some(key) = c["entryOrderMutate"].as_str() {
            if !self.inserted.replace(true) {
                let id = c["entryId"].as_str().unwrap();
                let entry = self
                    .state
                    .recent()
                    .into_iter()
                    .find(|(k, _)| k == id)
                    .unwrap()
                    .1;
                entry.borrow_mut().order = self.registry[key].clone();
            }
        }
        Ok(self.store.borrow().clone())
    }
}
enum NativeReturn {
    Unit,
    Bool(bool),
    Number(f64),
    List(OrderList),
    Symbol(Option<String>),
}
fn step(h: &Rc<Host>, c: &Value) -> Result<NativeReturn, Error> {
    let op = c["op"].as_str().unwrap();
    match op {
        "fallback" => should_use_local_rest_order_lifecycle_fallback(
            || {
                h.simple("demo")?;
                Ok(c["demo"] == true)
            },
            || {
                h.simple("site")?;
                Ok(c.get("site").cloned())
            },
        )
        .map(NativeReturn::Bool),
        "should" => should_reconcile_local_reduce_only_limit_order(
            &h.registry[c.get("key").and_then(Value::as_str).unwrap_or("A")],
            || h.fallback(),
        )
        .map(NativeReturn::Bool),
        "target" => {
            let mut order = h.registry["A"].borrow_mut();
            for (key, value) in c["numbers"].as_object().unwrap() {
                let n = safe_cex_num::number_from_string(value.as_str().unwrap());
                match key.as_str() {
                    "amount" => order.amount = n,
                    "remaining" => order.remaining = n,
                    "filled" => order.filled = n,
                    _ => panic!("number field"),
                }
            }
            drop(order);
            Ok(NativeReturn::Number(
                get_filled_order_reconciliation_target(&h.registry["A"]),
            ))
        }
        "record" => record_recently_placed_order(
            &h.state,
            h.registry[c["key"].as_str().unwrap()].clone(),
            || h.now(),
            || h.fallback(),
            |rows| h.add(rows),
            |order| h.schedule_method(order),
        )
        .map(|()| NativeReturn::Unit),
        "seen" => mark_order_seen(&h.state, c["id"].as_str().unwrap(), |id| h.clear_method(id))
            .map(|()| NativeReturn::Unit),
        "clear" => clear_recently_placed_order_reconciliation_timers(
            &h.state,
            c["id"].as_str().unwrap(),
            |timer| h.clear_timeout(timer),
        )
        .map(|()| NativeReturn::Unit),
        "schedule" => h
            .schedule(h.registry[c.get("key").and_then(Value::as_str).unwrap_or("A")].clone())
            .map(|()| NativeReturn::Unit),
        "fire" => {
            let callback = h.callbacks.borrow()[c["timer"].as_str().unwrap()].clone();
            callback().map(|()| NativeReturn::Unit)
        }
        "retire" => retire_cancelled_orders(
            &h.state,
            h.list(&c["keys"]),
            || h.now(),
            |id| h.mark(id),
            || h.fallback(),
            |rows| h.remove(rows),
        )
        .map(|()| NativeReturn::Unit),
        "resolve" => {
            *h.store.borrow_mut() = h.list(&c["store"]);
            resolve_order_symbol_for_fills(
                &h.state,
                c["id"].as_str().unwrap(),
                || h.store(),
                || h.now(),
            )
            .map(NativeReturn::Symbol)
        }
        "preserve" => {
            *h.fetched.borrow_mut() = h.list(&c["fetched"]);
            *h.store.borrow_mut() = h.list(&c["store"]);
            preserve_recently_placed_orders(
                &h.state,
                h.fetched.borrow().clone(),
                || h.now(),
                |id| h.mark(id),
                || h.store(),
            )
            .map(NativeReturn::List)
        }
        "mutate" => {
            mutate(&h.registry[c["key"].as_str().unwrap()], &c["fields"]);
            Ok(NativeReturn::Unit)
        }
        "seed" => {
            if let Some(rows) = c["recent"].as_array() {
                for row in rows {
                    h.state.put_recent(
                        row[0].as_str().unwrap().into(),
                        RecentEntry {
                            placed_at: safe_cex_num::number_from_string(row[2].as_str().unwrap()),
                            order: h.registry[row[1].as_str().unwrap()].clone(),
                        },
                    );
                }
            }
            if let Some(rows) = c["timers"].as_array() {
                for row in rows {
                    h.state.put_timers(
                        row[0].as_str().unwrap().into(),
                        Rc::new(RefCell::new(
                            row[1]
                                .as_array()
                                .unwrap()
                                .iter()
                                .map(|v| v.as_str().unwrap().into())
                                .collect(),
                        )),
                    );
                }
            }
            if let Some(rows) = c["retired"].as_array() {
                for row in rows {
                    h.state.put_retired(
                        row[0].as_str().unwrap().into(),
                        RetiredSymbol {
                            symbol: row[1].as_str().unwrap().into(),
                            retired_at: safe_cex_num::number_from_string(row[2].as_str().unwrap()),
                        },
                    );
                }
            }
            Ok(NativeReturn::Unit)
        }
        _ => panic!("op"),
    }
}
fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().into())
        .collect()
}
fn assert_state(h: &Host, e: &Value) {
    let recent = h.state.recent();
    assert_eq!(
        recent.len(),
        e["recent"].as_array().unwrap().len(),
        "PRIMARY native recent count"
    );
    for ((id, entry), e) in recent.iter().zip(e["recent"].as_array().unwrap()) {
        assert_eq!(id, e["id"].as_str().unwrap(), "PRIMARY native recent key");
        number(entry.borrow().placed_at, &e["placedAt"], "recent clock");
        assert_eq!(
            h.key(&entry.borrow().order),
            e["ref"],
            "PRIMARY native retained cache order identity"
        );
        order(&entry.borrow().order.borrow(), &e["order"]);
    }
    let timers = h.state.timers();
    assert_eq!(
        timers.len(),
        e["timers"].as_array().unwrap().len(),
        "PRIMARY native timer count"
    );
    for ((id, rows), e) in timers.iter().zip(e["timers"].as_array().unwrap()) {
        assert_eq!(id, e["id"].as_str().unwrap(), "PRIMARY native timer key");
        assert_eq!(
            *rows.borrow(),
            strings(&e["timers"]),
            "PRIMARY native timer list"
        );
    }
    let retired = h.state.retired();
    assert_eq!(
        retired.len(),
        e["retired"].as_array().unwrap().len(),
        "PRIMARY native retired count"
    );
    for ((id, entry), e) in retired.iter().zip(e["retired"].as_array().unwrap()) {
        assert_eq!(id, e["id"].as_str().unwrap(), "PRIMARY native retired key");
        assert_eq!(entry.symbol, e["symbol"], "PRIMARY native retired symbol");
        number(entry.retired_at, &e["retiredAt"], "retired clock");
    }
    for (key, handle) in &h.registry {
        order(&handle.borrow(), &e["orders"][key]);
    }
}
#[test]
#[ignore = "requires maintained TypeScript"]
fn primary_order_lifecycle_matches_maintained_bodies() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/primary-order-lifecycle-fixtures.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "PRIMARY maintained oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let fixture: Value = serde_json::from_slice(&output.stdout).unwrap();
    let mut iterations = 0;
    for (group, floor) in [
        ("fallback", 12),
        ("reconcile", 8),
        ("target", 8),
        ("cases", 12),
    ] {
        assert_eq!(
            fixture[group].as_array().unwrap().len(),
            floor,
            "PRIMARY independent {group} floor"
        );
        for (case_index, case) in fixture[group].as_array().unwrap().iter().enumerate() {
            let h = Host::new(case);
            for (step_index, e) in case["steps"].as_array().unwrap().iter().enumerate() {
                let c = &e["command"];
                *h.current.borrow_mut() = c.clone();
                h.events.borrow_mut().clear();
                h.inserted.set(false);
                let result = step(&h, c);
                let expected = &e["result"];
                let label = format!(
                    "PRIMARY native {group} case {case_index} step {step_index} {}",
                    c["op"].as_str().unwrap()
                );
                assert_eq!(
                    result.is_err(),
                    expected.get("error").is_some(),
                    "{label}: result discriminant"
                );
                match result {
                    Err(error) => {
                        assert_eq!(error["name"], expected["error"], "{label}: error name");
                        assert_eq!(
                            error["message"], expected["message"],
                            "{label}: error message"
                        );
                        let phase = expected["origin"].as_str().unwrap();
                        assert!(
                            Rc::ptr_eq(&error, &h.errors[phase]),
                            "{label}: original error identity"
                        );
                    }
                    Ok(value) => match value {
                        NativeReturn::Unit => assert_eq!(
                            expected["value"],
                            json!({"undefined":true}),
                            "{label}: native undefined"
                        ),
                        NativeReturn::Bool(value) => assert_eq!(
                            value,
                            expected["value"].as_bool().unwrap(),
                            "{label}: native predicate"
                        ),
                        NativeReturn::Number(value) => number(value, &expected["value"], "target"),
                        NativeReturn::Symbol(value) => match value {
                            Some(value) => {
                                assert_eq!(value, expected["value"], "{label}: native symbol")
                            }
                            None => assert_eq!(
                                expected["value"],
                                json!({"undefined":true}),
                                "{label}: absent symbol"
                            ),
                        },
                        NativeReturn::List(value) => {
                            let rows = value.borrow();
                            let refs = strings(&expected["refs"]);
                            assert_eq!(rows.len(), refs.len(), "{label}: native preserved count");
                            for ((handle, ref_key), e) in rows
                                .iter()
                                .zip(refs)
                                .zip(expected["value"].as_array().unwrap())
                            {
                                assert!(
                                    Rc::ptr_eq(handle, &h.registry[&ref_key]),
                                    "{label}: original preserved order identity"
                                );
                                order(&handle.borrow(), e);
                            }
                            assert_eq!(
                                Rc::ptr_eq(&value, &h.fetched.borrow()),
                                expected["sameFetched"].as_bool().unwrap(),
                                "{label}: fetched array identity"
                            );
                        }
                    },
                }
                let events = h.events.borrow();
                assert_eq!(
                    events.len(),
                    e["events"].as_array().unwrap().len(),
                    "{label}: event count"
                );
                for (event, e) in events.iter().zip(e["events"].as_array().unwrap()) {
                    assert_eq!(event.kind, e["kind"], "{label}: event kind");
                    for (name, actual) in [
                        ("recent", &event.recent),
                        ("timerKeys", &event.timer_keys),
                        ("retired", &event.retired),
                        ("refs", &event.refs),
                        ("ids", &event.ids),
                    ] {
                        assert_eq!(*actual, strings(&e[name]), "{label}: event {name}");
                    }
                    assert_eq!(
                        event.numbers.len(),
                        e["numbers"].as_array().unwrap().len(),
                        "{label}: event number count"
                    );
                    for (actual, expected) in
                        event.numbers.iter().zip(e["numbers"].as_array().unwrap())
                    {
                        number(*actual, expected, "timer delay");
                    }
                }
                drop(events);
                assert_state(&h, e);
                iterations += 1;
            }
        }
    }
    assert_eq!(iterations, 79, "PRIMARY independent native iteration floor");
}
