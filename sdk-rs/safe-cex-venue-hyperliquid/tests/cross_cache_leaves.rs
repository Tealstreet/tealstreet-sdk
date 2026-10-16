use safe_cex_venue_hyperliquid::{
    cross_cache::*, json_boundaries::JsPayloadHost, spot_json::JsString,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet, VecDeque},
    rc::Rc,
};
#[derive(Clone, Debug)]
struct Native(Rc<RefCell<Kind>>);
#[derive(Debug)]
enum Kind {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(JsString),
    Symbol(u64),
    Accessor,
    Ref(u64, String),
    Array(u64, Vec<Native>),
    Object(u64, Vec<(JsString, Native)>, String),
}
fn units(v: &Value) -> JsString {
    JsString(
        v.as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_u64().unwrap() as u16)
            .collect(),
    )
}
fn key_index(s: &JsString) -> Option<u32> {
    let t = String::from_utf16(&s.0).ok()?;
    let n = t.parse::<u32>().ok()?;
    (n < u32::MAX && n.to_string() == t).then_some(n)
}
impl Native {
    fn literal(k: Kind) -> Self {
        Self(Rc::new(RefCell::new(k)))
    }
    fn native(&self) -> Value {
        self.native_with(&mut HashSet::new())
    }
    fn native_with(&self, active: &mut HashSet<u64>) -> Value {
        if let Some(id) = self.id() {
            if !active.insert(id) {
                return json!(["ref", id, "cycle"]);
            }
        }
        let result = match &*self.0.borrow() {
            Kind::Undefined => json!(["undefined"]),
            Kind::Null => json!(["null"]),
            Kind::Bool(v) => json!(["bool", v]),
            Kind::Number(v) => json!([
                "number",
                if v.is_nan() {
                    "nan".into()
                } else {
                    format!("{:016x}", v.to_bits())
                }
            ]),
            Kind::String(v) => json!(["string", v.0]),
            Kind::Symbol(id) => json!(["symbol", id]),
            Kind::Accessor => json!(["accessor"]),
            Kind::Ref(id, tag) => json!(["ref", id, tag]),
            Kind::Array(id, v) => json!([
                "array",
                id,
                v.iter().map(|v| v.native_with(active)).collect::<Vec<_>>()
            ]),
            Kind::Object(id, v, p) => {
                let mut members = v.iter().collect::<Vec<_>>();
                members.sort_by_key(|(k, _)| key_index(k).map(|i| (0, i)).unwrap_or((1, 0)));
                json!([
                    "object",
                    id,
                    members
                        .iter()
                        .map(|(k, v)| json!([k.0, v.native_with(active)]))
                        .collect::<Vec<_>>(),
                    p
                ])
            }
        };
        if let Some(id) = self.id() {
            active.remove(&id);
        }
        result
    }
    fn text(&self) -> JsString {
        let Kind::String(s) = &*self.0.borrow() else {
            panic!("native string required")
        };
        s.clone()
    }
    fn id(&self) -> Option<u64> {
        match &*self.0.borrow() {
            Kind::Ref(id, _) | Kind::Array(id, _) | Kind::Object(id, _, _) => Some(*id),
            _ => None,
        }
    }
}
struct Host {
    script: VecDeque<Value>,
    objects: HashMap<u64, Native>,
    case: String,
    functions: HashMap<u64, String>,
    seen: Vec<JsString>,
    body_pairs: Vec<[Native; 2]>,
}
impl Host {
    fn decode(&mut self, v: &Value) -> Native {
        let name = v[0].as_str().unwrap();
        if matches!(name, "ref" | "array" | "object") {
            let id = v[1].as_u64().unwrap();
            if let Some(existing) = self.objects.get(&id) {
                return existing.clone();
            }
            let value = Native::literal(Kind::Ref(id, v[2].as_str().unwrap_or("pending").into()));
            self.objects.insert(id, value.clone());
            let kind = match name {
                "ref" => Kind::Ref(id, v[2].as_str().unwrap().into()),
                "array" => Kind::Array(
                    id,
                    v[2].as_array()
                        .unwrap()
                        .iter()
                        .map(|v| self.decode(v))
                        .collect(),
                ),
                "object" => Kind::Object(
                    id,
                    v[2].as_array()
                        .unwrap()
                        .iter()
                        .map(|p| (units(&p[0]), self.decode(&p[1])))
                        .collect(),
                    v[3].as_str().unwrap().into(),
                ),
                _ => unreachable!(),
            };
            *value.0.borrow_mut() = kind;
            return value;
        }
        Native::literal(match name {
            "undefined" => Kind::Undefined,
            "null" => Kind::Null,
            "bool" => Kind::Bool(v[1].as_bool().unwrap()),
            "number" => Kind::Number(if v[1] == "nan" {
                f64::NAN
            } else {
                f64::from_bits(u64::from_str_radix(v[1].as_str().unwrap(), 16).unwrap())
            }),
            "string" => Kind::String(units(&v[1])),
            "symbol" => Kind::Symbol(v[1].as_u64().unwrap()),
            "accessor" => Kind::Accessor,
            _ => panic!("unknown native {v}"),
        })
    }
    fn raw(&mut self, name: &str, arg: Value) -> Result<Value, Native> {
        let row = self
            .script
            .pop_front()
            .unwrap_or_else(|| panic!("CACHE native unexpected operation {} {name}", self.case));
        assert_eq!(row["name"], name, "CACHE native operation {}", self.case);
        assert_eq!(
            row["arg"], arg,
            "CACHE native argument {} {name}",
            self.case
        );
        if row.get("error").is_some() {
            Err(self.decode(&row["error"]))
        } else {
            Ok(row["result"].clone())
        }
    }
    fn take(&mut self, name: &str, arg: Value) -> Result<Native, Native> {
        let r = self.raw(name, arg)?;
        Ok(self.decode(&r))
    }
    fn assign(&mut self, object: &Native, key: &JsString, value: &Native, define: bool) {
        let mut target = object.0.borrow_mut();
        let Kind::Object(_, fields, proto) = &mut *target else {
            panic!("native record required")
        };
        if let Some((_, old)) = fields.iter_mut().find(|(k, _)| k == key) {
            if define || !matches!(*old.0.borrow(), Kind::Accessor) {
                *old = value.clone();
            }
            return;
        }
        if !define && key.0 == "__proto__".encode_utf16().collect::<Vec<_>>() {
            match &*value.0.borrow() {
                Kind::Null => *proto = "null".into(),
                Kind::Object(..) | Kind::Array(..) | Kind::Ref(..) => *proto = "custom".into(),
                _ => {}
            }
            return;
        }
        fields.push((key.clone(), value.clone()));
    }
}
impl JsPayloadHost for Host {
    type Value = Native;
    type Error = Native;
    fn string_units<'a>(&self, _: &'a Native) -> Option<&'a [u16]> {
        panic!("unused primitive")
    }
    fn is_object(&self, v: &Native) -> bool {
        matches!(
            *v.0.borrow(),
            Kind::Object(..) | Kind::Array(..) | Kind::Ref(..)
        )
    }
    fn is_array(&mut self, _: &Native) -> Result<bool, Native> {
        panic!("unused primitive")
    }
    fn is_nullish(&self, v: &Native) -> bool {
        matches!(*v.0.borrow(), Kind::Null | Kind::Undefined)
    }
    fn undefined(&self) -> Native {
        Native::literal(Kind::Undefined)
    }
    fn parse_json(&mut self, _: &[u16]) -> Result<Native, Native> {
        panic!("unused primitive")
    }
    fn get(&mut self, v: &Native, key: &str) -> Result<Native, Native> {
        let r = self.take(&format!("get:{key}"), v.native())?;
        if let Kind::Ref(id, tag) = &*r.0.borrow() {
            if tag == "function" {
                self.functions.insert(*id, key.into());
            }
        }
        Ok(r)
    }
    fn json_stringify(&mut self, v: &Native) -> Result<Option<JsString>, Native> {
        let r = self.take("stringify", v.native())?;
        Ok(if matches!(*r.0.borrow(), Kind::Undefined) {
            None
        } else {
            Some(r.text())
        })
    }
    fn to_string(&mut self, _: &Native) -> Result<JsString, Native> {
        panic!("unused explicit String primitive")
    }
}
impl CrossCacheHost for Host {
    fn null(&self) -> Native {
        Native::literal(Kind::Null)
    }
    fn is_undefined(&self, v: &Native) -> bool {
        matches!(*v.0.borrow(), Kind::Undefined)
    }
    fn truthy(&self, v: &Native) -> bool {
        match &*v.0.borrow() {
            Kind::Undefined | Kind::Null => false,
            Kind::Bool(b) => *b,
            Kind::Number(n) => *n != 0.0 && !n.is_nan(),
            Kind::String(s) => !s.0.is_empty(),
            _ => true,
        }
    }
    fn session_storage(&mut self) -> Result<Native, Native> {
        self.take("sessionStorage", json!([]))
    }
    fn literal(&self, text: &JsString) -> Native {
        Native::literal(Kind::String(text.clone()))
    }
    fn object(&mut self) -> Result<Native, Native> {
        self.take("object", json!([]))
    }
    fn set(&mut self, object: &Native, key: &Native, value: &Native) -> Result<(), Native> {
        self.take(
            "set",
            json!({"object":object.native(),"key":key.native(),"value":value.native()}),
        )?;
        if let Kind::String(key) = &*key.0.borrow() {
            self.assign(object, key, value, false)
        }
        Ok(())
    }
    fn define_data(
        &mut self,
        object: &Native,
        key: &JsString,
        value: &Native,
    ) -> Result<(), Native> {
        self.take(
            "defineData",
            json!({"object":object.native(),"key":key.0,"value":value.native()}),
        )?;
        self.assign(object, key, value, true);
        Ok(())
    }
    fn call(
        &mut self,
        function: &Native,
        receiver: &Native,
        args: &[Native],
    ) -> Result<Native, Native> {
        let result=self.take("call",json!({"function":function.native(),"receiver":receiver.native(),"args":args.iter().map(Native::native).collect::<Vec<_>>()}))?;
        let is_set = matches!(&*receiver.0.borrow(),Kind::Ref(_,tag)if tag=="seenSet");
        if is_set {
            let name = self.functions[&function.id().unwrap()].clone();
            match name.as_str() {
                "add" => {
                    let s = args[0].text();
                    if !self.seen.contains(&s) {
                        self.seen.push(s)
                    }
                }
                "delete" => {
                    let s = args[0].text();
                    let was = self.seen.contains(&s);
                    self.seen.retain(|v| v != &s);
                    assert_eq!(
                        result.native(),
                        json!(["bool", was]),
                        "CACHE native Set.delete result"
                    );
                }
                "has" => assert_eq!(
                    result.native(),
                    json!(["bool", self.seen.contains(&args[0].text())]),
                    "CACHE native Set.has result"
                ),
                _ => {}
            }
        }
        Ok(result)
    }
    fn parse_json_value(&mut self, value: &Native) -> Result<Native, Native> {
        self.take("parse", value.native())
    }
    fn property_key(&mut self, value: &Native) -> Result<Native, Native> {
        self.take("propertyKey", value.native())
    }
    fn entries(&mut self, map: &Native) -> Result<Native, Native> {
        self.take("entries", map.native())
    }
    fn iterator_step(&mut self, iterator: &Native) -> Result<Option<Native>, Native> {
        let r = self.raw("iteratorStep", iterator.native())?;
        Ok(if r.is_null() {
            None
        } else {
            Some(self.decode(&r))
        })
    }
    fn entry_pair(&mut self, entry: &Native) -> Result<[Native; 2], Native> {
        let r = self.raw("entryPair", entry.native())?;
        let pair = [self.decode(&r[0]), self.decode(&r[1])];
        self.body_pairs.push(pair.clone());
        Ok(pair)
    }
    fn close_after_throw(&mut self, iterator: &Native) {
        let _ignored = self.take("closeAfterThrow", iterator.native());
    }
    fn spread(&mut self, value: &Native) -> Result<Native, Native> {
        let result = self.take("spread", value.native())?;
        let original = value.0.borrow();
        let Kind::Array(_, members) = &*original else {
            panic!("native spread input")
        };
        let mut output = result.0.borrow_mut();
        let Kind::Array(_, copied) = &mut *output else {
            panic!("native spread output")
        };
        *copied = members.clone();
        assert!(
            !Rc::ptr_eq(&result.0, &value.0),
            "CACHE shallow spread fresh array"
        );
        drop(output);
        Ok(result)
    }
    fn greater_than_500(&mut self, value: &Native) -> Result<bool, Native> {
        let r = self.raw("greater500", value.native())?;
        Ok(r.as_bool().unwrap())
    }
}
fn run(row: &Value) {
    let name = row["name"].as_str().unwrap();
    let mut h = Host {
        script: row["calls"].as_array().unwrap().iter().cloned().collect(),
        objects: HashMap::new(),
        case: format!("{name} {}", row["label"]),
        functions: HashMap::new(),
        seen: row["initialSeen"]
            .as_array()
            .unwrap()
            .iter()
            .map(units)
            .collect(),
        body_pairs: vec![],
    };
    let cache = h.decode(&row["input"]);
    let args = row["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| h.decode(a))
        .collect::<Vec<_>>();
    let result = match name {
        "getSessionStorage" => Ok(get_session_storage(&mut h)),
        "getPrePlacementKey" => {
            Ok(h.literal(&get_pre_placement_key(&args[0].text(), &args[1].text())))
        }
        "broadcastCacheUpdate" => {
            broadcast_cache_update(&mut h, &cache, &args[0].text(), &args[1].text(), &args[2])
                .map(|()| h.undefined())
        }
        "readPrePlacementCache" => read_pre_placement_cache(&mut h, &cache),
        "readPendingOrdersCache" => read_pending_orders_cache(&mut h, &cache),
        "writePrePlacementCache" => {
            write_pre_placement_cache(&mut h, &cache, &args[0]).map(|()| h.undefined())
        }
        "writePendingOrdersCache" => {
            write_pending_orders_cache(&mut h, &cache, &args[0]).map(|()| h.undefined())
        }
        "hasSeenFill" => has_seen_fill(&mut h, &cache, &args[0].text()),
        "markFillSeen" => mark_fill_seen(&mut h, &cache, &args[0].text()).map(|()| h.undefined()),
        "dispose" => dispose(&mut h, &cache).map(|()| h.undefined()),
        _ => unreachable!(),
    };
    match result {
        Ok(value) => {
            assert!(
                row.get("error").is_none(),
                "CACHE native expected error {}",
                h.case
            );
            assert_eq!(
                value.native(),
                row["native"],
                "CACHE native result {}",
                h.case
            );
            if name == "readPendingOrdersCache" && row["label"] == "worker entries" {
                let Kind::Object(_, members, _) = &*value.0.borrow() else {
                    panic!()
                };
                for (key, v) in members {
                    let original = h
                        .body_pairs
                        .iter()
                        .find(|pair| pair[0].text() == *key)
                        .unwrap();
                    assert!(
                        Rc::ptr_eq(&v.0, &original[1].0),
                        "CACHE pending member identity"
                    );
                }
            }
        }
        Err(error) => assert_eq!(
            error.native(),
            row["error"],
            "CACHE native error {}",
            h.case
        ),
    }
    assert_eq!(
        cache.native(),
        row["finalCache"],
        "CACHE native cache state {}",
        h.case
    );
    assert_eq!(
        h.seen.iter().map(|s| s.0.clone()).collect::<Vec<_>>(),
        serde_json::from_value::<Vec<Vec<u16>>>(row["finalSeen"].clone()).unwrap(),
        "CACHE native seen FIFO {}",
        h.case
    );
    assert!(
        h.script.is_empty(),
        "CACHE native missing operations {}: {:?}",
        h.case,
        h.script
    );
}
#[test]
fn native_ten_cache_methods_match_actual_source() {
    let output = std::process::Command::new("node")
        .arg(format!(
            "{}/scripts/cross-cache-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let names = [
        "getSessionStorage",
        "broadcastCacheUpdate",
        "getPrePlacementKey",
        "readPrePlacementCache",
        "writePrePlacementCache",
        "readPendingOrdersCache",
        "writePendingOrdersCache",
        "hasSeenFill",
        "markFillSeen",
        "dispose",
    ];
    assert_eq!(corpus["names"], json!(names));
    assert_eq!(corpus["iterationStop"], 79);
    assert_eq!(corpus["cases"].as_array().unwrap().len(), 80);
    let expected = [5, 5, 4, 17, 8, 17, 8, 3, 7, 6];
    let mut failures = vec![];
    for (name, count) in names.iter().zip(expected) {
        let rows = corpus["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["name"] == *name)
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), count, "independent family floor {name}");
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            for row in rows {
                run(row)
            }
        }))
        .is_err()
        {
            failures.push(*name)
        }
    }
    assert!(
        failures.is_empty(),
        "CACHE named native failures:{failures:?}"
    );
}

impl safe_cex_venue_hyperliquid::cache_orders::CacheOrdersHost for Host {
    fn now(&mut self) -> Result<Native, Native> {
        self.take("now", json!([]))
    }
    fn object_keys(&mut self, value: &Native) -> Result<Native, Native> {
        self.take("objectKeys", value.native())
    }
    fn get_index(&mut self, value: &Native, key: &Native) -> Result<Native, Native> {
        self.take("index", json!({"object":value.native(),"key":key.native()}))
    }
    fn delete(&mut self, value: &Native, key: &Native) -> Result<(), Native> {
        self.take(
            "delete",
            json!({"object":value.native(),"key":key.native()}),
        )?;
        if let Kind::Object(_, fields, _) = &mut *value.0.borrow_mut() {
            fields.retain(|(k, _)| k != &key.text());
        }
        Ok(())
    }
    fn older_than(
        &mut self,
        now: &Native,
        timestamp: &Native,
        milliseconds: f64,
    ) -> Result<bool, Native> {
        Ok(self.raw("olderThan",json!({"now":now.native(),"timestamp":timestamp.native(),"milliseconds":Native::literal(Kind::Number(milliseconds)).native()}))?.as_bool().unwrap())
    }
    fn singleton(&mut self) -> Result<Native, Native> {
        self.take("singleton", json!([]))
    }
    fn imported(&mut self, name: &str) -> Result<Native, Native> {
        self.take("imported", json!([name]))
    }
}
fn run_order_method(row: &Value) {
    use safe_cex_venue_hyperliquid::cache_orders::*;
    let name = row["name"].as_str().unwrap();
    let mut h = Host {
        script: row["calls"].as_array().unwrap().iter().cloned().collect(),
        objects: HashMap::new(),
        case: format!("{name} {}", row["label"]),
        functions: HashMap::new(),
        seen: vec![],
        body_pairs: vec![],
    };
    let cache = h.decode(&row["input"]);
    let record = h.decode(&row["initialRecord"]);
    let pre = h.decode(&row["initialPre"]);
    let args = row["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| h.decode(a))
        .collect::<Vec<_>>();
    let captured = if name == "getPendingOrder" {
        if let Kind::Object(_, fields, _) = &*record.0.borrow() {
            fields
                .iter()
                .find(|(key, _)| key == &args[0].text())
                .map(|(_, v)| v.clone())
        } else {
            None
        }
    } else {
        None
    };
    let result = match name {
        "clearStaleSessionStorage" => {
            clear_stale_session_storage(&mut h, &cache);
            Ok(h.undefined())
        }
        "clearPrePlacement" => {
            clear_pre_placement(&mut h, &cache, &args[0], &args[1]).map(|()| h.undefined())
        }
        "getPendingOrder" => get_pending_order(&mut h, &cache, &args[0]),
        "addPendingOrder" => {
            add_pending_order(&mut h, &cache, &args[0], &args[1], &args[2]).map(|()| h.undefined())
        }
        "removePendingOrder" => {
            remove_pending_order(&mut h, &cache, &args[0]).map(|()| h.undefined())
        }
        "exchange.addToPrePlacementCache" => exchange_add_to_pre_placement_cache(
            &mut h,
            &args[0],
            &args[1],
            &args[2],
            &args[3],
            &args[4],
            args.get(5),
        )
        .map(|()| h.undefined()),
        "exchange.matchPrePlacement" => {
            exchange_match_pre_placement(&mut h, &args[0], &args[1], args.get(2), args.get(3))
        }
        "exchange.getPendingHip3Order" => exchange_get_pending_hip3_order(&mut h, &args[0]),
        "exchange.addPendingHip3Order" => {
            exchange_add_pending_hip3_order(&mut h, &args[0], &args[1], &args[2])
                .map(|()| h.undefined())
        }
        "exchange.removePendingHip3Order" => {
            exchange_remove_pending_hip3_order(&mut h, &args[0]).map(|()| h.undefined())
        }
        "exchange.forgetPrePlacement" => {
            exchange_forget_pre_placement(&mut h, &args[0], &args[1], &args[2])
                .map(|()| h.undefined())
        }
        "exchange.recordPlacement" => {
            exchange_record_placement(&mut h, &args[0], &args[1]).map(|()| h.undefined())
        }
        "exchange.getPlacementRecord" => exchange_get_placement_record(&mut h, &args[0]),
        "exchange.forgetPlacementRecord" => {
            exchange_forget_placement_record(&mut h, &args[0]).map(|()| h.undefined())
        }
        "exchange.matchPrePlacementTrigger" => {
            exchange_match_pre_placement_trigger(&mut h, &args[0], &args[1], &args[2])
        }
        _ => unreachable!(),
    };
    match result {
        Ok(value) => {
            assert!(
                row.get("error").is_none(),
                "ORDERS native expected error {}",
                h.case
            );
            assert_eq!(
                value.native(),
                row["native"],
                "ORDERS native result {}",
                h.case
            );
            if name == "getPendingOrder" {
                if let Some(captured) = captured {
                    assert!(
                        Rc::ptr_eq(&value.0, &captured.0),
                        "ORDERS captured entry identity {}",
                        h.case
                    )
                }
            }
        }
        Err(error) => assert_eq!(
            error.native(),
            row["error"],
            "ORDERS native error {}",
            h.case
        ),
    }
    assert_eq!(
        record.native(),
        row["finalRecord"],
        "ORDERS native pending record {}",
        h.case
    );
    assert_eq!(
        pre.native(),
        row["finalPre"],
        "ORDERS native pre record {}",
        h.case
    );
    assert!(
        h.script.is_empty(),
        "ORDERS native missing effects {}: {:?}",
        h.case,
        h.script
    );
}
#[test]
fn native_fifteen_cache_order_methods_match_actual_source() {
    let output = std::process::Command::new("node")
        .arg(format!(
            "{}/scripts/cache-orders-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let names = [
        "clearStaleSessionStorage",
        "clearPrePlacement",
        "getPendingOrder",
        "addPendingOrder",
        "removePendingOrder",
        "exchange.addToPrePlacementCache",
        "exchange.matchPrePlacement",
        "exchange.getPendingHip3Order",
        "exchange.addPendingHip3Order",
        "exchange.removePendingHip3Order",
        "exchange.forgetPrePlacement",
        "exchange.recordPlacement",
        "exchange.getPlacementRecord",
        "exchange.forgetPlacementRecord",
        "exchange.matchPrePlacementTrigger",
    ];
    assert_eq!(corpus["names"], json!(names));
    assert_eq!(corpus["iterationStop"], 136);
    assert_eq!(corpus["cases"].as_array().unwrap().len(), 137);
    let expected = [7, 9, 17, 12, 8, 9, 10, 8, 8, 8, 8, 9, 8, 8, 8];
    let mut failures = vec![];
    for (name, count) in names.iter().zip(expected) {
        let rows = corpus["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["name"] == *name)
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), count, "independent order-method floor {name}");
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            for row in rows {
                run_order_method(row)
            }
        }))
        .is_err()
        {
            failures.push(*name)
        }
    }
    assert!(
        failures.is_empty(),
        "ORDERS named native failures:{failures:?}"
    );
}
