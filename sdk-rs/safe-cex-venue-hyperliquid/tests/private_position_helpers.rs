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
    BigInt(JsString),
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
            Kind::BigInt(v) => json!(["bigint", v.0]),
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
    pnl: Vec<(Native, Native)>,
    queue: Vec<JsString>,
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
            "bigint" => Kind::BigInt(units(&v[1])),
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
        if key == "length" && matches!(&*v.0.borrow(),Kind::Ref(_,tag) if tag=="queue") {
            assert_eq!(
                r.native(),
                Native::literal(Kind::Number(self.queue.len() as f64)).native(),
                "PRIVATE native queue length {}",
                self.case
            );
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
    fn to_string(&mut self, value: &Native) -> Result<JsString, Native> {
        self.take("string", value.native()).map(|v| v.text())
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
        let tag = match &*receiver.0.borrow() {
            Kind::Ref(_, tag) => tag.clone(),
            _ => String::new(),
        };
        if tag == "queue" {
            match self.functions[&function.id().unwrap()].as_str() {
                "push" => {
                    self.queue.push(args[0].text());
                    assert_eq!(
                        result.native(),
                        Native::literal(Kind::Number(self.queue.len() as f64)).native(),
                        "PRIVATE native push result"
                    );
                }
                "shift" => {
                    let expected = if self.queue.is_empty() {
                        self.undefined()
                    } else {
                        {
                            let first = self.queue.remove(0);
                            self.literal(&first)
                        }
                    };
                    assert_eq!(
                        result.native(),
                        expected.native(),
                        "PRIVATE native shift result"
                    );
                }
                _ => {}
            }
        }
        if tag == "pnlMap" {
            match self.functions[&function.id().unwrap()].as_str() {
                "get" => {
                    let expected = self
                        .pnl
                        .iter()
                        .find(|(k, _)| k.native() == args[0].native())
                        .map(|(_, v)| v.clone())
                        .unwrap_or_else(|| self.undefined());
                    assert_eq!(
                        result.native(),
                        expected.native(),
                        "PRIVATE native map get result"
                    );
                }
                "set" => {
                    if let Some((_, old)) = self
                        .pnl
                        .iter_mut()
                        .find(|(k, _)| k.native() == args[0].native())
                    {
                        *old = args[1].clone()
                    } else {
                        self.pnl.push((args[0].clone(), args[1].clone()))
                    }
                }
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

impl safe_cex_venue_hyperliquid::private_position_helpers::PrivatePositionHost for Host {
    fn number(&mut self, value: &Native) -> Result<f64, Native> {
        let result = self.take("number", value.native())?;
        let Kind::Number(v) = &*result.0.borrow() else {
            panic!("native number required")
        };
        Ok(*v)
    }
    fn number_value(&self, value: f64) -> Native {
        Native::literal(Kind::Number(value))
    }
    fn lowercase(&mut self, value: &JsString) -> Result<JsString, Native> {
        self.take("lowercase", json!(value.0)).map(|v| v.text())
    }
    fn index(&mut self, object: &Native, key: &Native) -> Result<Native, Native> {
        self.take(
            "index",
            json!({"object":object.native(),"key":key.native()}),
        )
    }
    fn add(&mut self, left: &Native, right: &Native) -> Result<Native, Native> {
        self.take("add", json!({"left":left.native(),"right":right.native()}))
    }
    fn max_tracked_fill_ids(&mut self) -> Result<Native, Native> {
        self.take("classLimit", json!([]))
    }
    fn greater_than(&mut self, value: &Native, limit: &Native) -> Result<bool, Native> {
        Ok(self
            .raw(
                "greaterThan",
                json!({"value":value.native(),"limit":limit.native()}),
            )?
            .as_bool()
            .unwrap())
    }
}
fn run(row: &Value) {
    use safe_cex_venue_hyperliquid::private_position_helpers::*;
    let name = row["name"].as_str().unwrap();
    let length = row["queueLength"].as_u64().unwrap() as usize;
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
        pnl: vec![],
        queue: vec![JsString("queued".encode_utf16().collect()); length],
    };
    if length > 0 {
        h.queue[0] = units(&row["queueFirst"][1]);
        h.queue[length - 1] = units(&row["queueLast"][1]);
    }
    let socket = h.decode(&row["input"]);
    let args = row["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| h.decode(v))
        .collect::<Vec<_>>();
    h.pnl = row["initialPnl"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (h.decode(&p[0]), h.decode(&p[1])))
        .collect();
    let result: Result<Value, Native> = match name {
        "getFillPositionSide" => Ok(get_fill_position_side(&h, &args[0].text()).native()),
        "safeParseNumber" => {
            safe_parse_number(&mut h, &args[0]).map(|v| Native::literal(Kind::Number(v)).native())
        }
        "trackProcessedRealizedPnlFill" => {
            track_processed_realized_pnl_fill(&mut h, &socket, &args[0])
                .map(|()| h.undefined().native())
        }
        "getRealizedPnlKey" => Ok(h
            .literal(&get_realized_pnl_key(&args[0].text(), &args[1].text()))
            .native()),
        "resolveSymbolFromCoin" => {
            resolve_symbol_from_coin(&mut h, &socket, &args[0].text(), &args[1]).map(|v| v.native())
        }
        "setRealizedPnlForPosition" => {
            set_realized_pnl_for_position(&mut h, &socket, &args[0], &args[1], &args[2], &args[3])
                .map(|()| h.undefined().native())
        }
        "incrementRealizedPnlForPosition" => increment_realized_pnl_for_position(
            &mut h, &socket, &args[0], &args[1], &args[2], &args[3],
        )
        .map(|()| h.undefined().native()),
        "getRealizedPnlForPosition" => {
            get_realized_pnl_for_position(&mut h, &socket, &args[0], &args[1]).map(|v| v.native())
        }
        "resolvePositionMode" => resolve_position_mode(&mut h, &args[0]).map(|v| v.native()),
        "extractCoreBalanceSummary" => {
            extract_core_balance_summary(&mut h, &args[0]).map(|v| match v {
                None => json!(["null"]),
                Some(v) => json!([
                    "summary",
                    format!("{:016x}", v.account_value.to_bits()),
                    format!("{:016x}", v.total_margin_used.to_bits()),
                    v.source_field
                        .source_name()
                        .encode_utf16()
                        .collect::<Vec<_>>()
                ]),
            })
        }
        "getMessageTimestamp" => get_message_timestamp(&mut h, &args[0]).map(|v| {
            v.map(|v| Native::literal(Kind::Number(v)).native())
                .unwrap_or_else(|| json!(["null"]))
        }),
        _ => unreachable!(),
    };
    match result {
        Ok(value) => {
            assert!(
                row.get("error").is_none(),
                "PRIVATE native expected error {}",
                h.case
            );
            assert_eq!(value, row["native"], "PRIVATE native result {}", h.case);
        }
        Err(e) => assert_eq!(e.native(), row["error"], "PRIVATE native error {}", h.case),
    }
    assert_eq!(
        h.pnl
            .iter()
            .map(|(k, v)| json!([k.native(), v.native()]))
            .collect::<Vec<_>>(),
        *row["finalPnl"].as_array().unwrap(),
        "PRIVATE native PnL state {}",
        h.case
    );
    assert_eq!(
        h.seen.iter().map(|s| s.0.clone()).collect::<Vec<_>>(),
        serde_json::from_value::<Vec<Vec<u16>>>(row["finalSeen"].clone()).unwrap(),
        "PRIVATE native seen state {}",
        h.case
    );
    let first = h
        .queue
        .first()
        .map(|s| h.literal(s).native())
        .unwrap_or_else(|| h.undefined().native());
    let last = h
        .queue
        .last()
        .map(|s| h.literal(s).native())
        .unwrap_or_else(|| h.undefined().native());
    assert_eq!(
        json!({"length":h.queue.len(),"first":first,"last":last}),
        row["finalQueue"],
        "PRIVATE native queue state {}",
        h.case
    );
    assert!(
        h.script.is_empty(),
        "PRIVATE native missing effects {}: {:?}",
        h.case,
        h.script
    );
}
#[test]
fn native_eleven_private_position_helpers_match_actual_source() {
    let output = std::process::Command::new("node")
        .arg(format!(
            "{}/scripts/private-position-oracle.cjs",
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
        "getFillPositionSide",
        "safeParseNumber",
        "trackProcessedRealizedPnlFill",
        "getRealizedPnlKey",
        "resolveSymbolFromCoin",
        "setRealizedPnlForPosition",
        "incrementRealizedPnlForPosition",
        "getRealizedPnlForPosition",
        "resolvePositionMode",
        "extractCoreBalanceSummary",
        "getMessageTimestamp",
    ];
    assert_eq!(corpus["names"], json!(names));
    assert_eq!(corpus["iterationStop"], 135);
    assert_eq!(corpus["cases"].as_array().unwrap().len(), 136);
    let expected = [12, 21, 13, 4, 10, 11, 11, 11, 12, 13, 18];
    let mut failures = vec![];
    for (name, count) in names.iter().zip(expected) {
        let rows = corpus["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["name"] == *name)
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), count, "independent private helper floor {name}");
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
        "PRIVATE named native failures:{failures:?}"
    );
}
