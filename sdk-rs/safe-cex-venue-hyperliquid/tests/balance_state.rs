use safe_cex_venue_hyperliquid::{balance_state::*, spot_json::JsString, ws_ping_lifecycle::*};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    panic::{AssertUnwindSafe, catch_unwind},
};
#[derive(Clone, Debug)]
enum Native {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    Text(JsString),
    Handle(u64, String),
}
impl Native {
    fn decode(v: &Value) -> Self {
        match v[0].as_str().unwrap() {
            "undefined" => Self::Undefined,
            "null" => Self::Null,
            "bool" => Self::Bool(v[1].as_bool().unwrap()),
            "number" => Self::Number(if v[1] == "nan" {
                f64::NAN
            } else {
                f64::from_bits(u64::from_str_radix(v[1].as_str().unwrap(), 16).unwrap())
            }),
            "string" => Self::Text(JsString(
                v[1].as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap() as u16)
                    .collect(),
            )),
            "ref" => Self::Handle(v[1].as_u64().unwrap(), v[2].as_str().unwrap().into()),
            _ => panic!("unsupported native tag {v}"),
        }
    }
    fn native(&self) -> Value {
        match self {
            Self::Undefined => json!(["undefined"]),
            Self::Null => json!(["null"]),
            Self::Bool(v) => json!(["bool", v]),
            Self::Number(v) => json!([
                "number",
                if v.is_nan() {
                    "nan".into()
                } else {
                    format!("{:016x}", v.to_bits())
                }
            ]),
            Self::Text(v) => json!(["string", v.0]),
            Self::Handle(id, tag) => json!(["ref", id, tag]),
        }
    }
    fn tag(&self) -> &str {
        if let Self::Handle(_, tag) = self {
            tag
        } else {
            ""
        }
    }
    fn id(&self) -> u64 {
        if let Self::Handle(id, _) = self {
            *id
        } else {
            panic!("handle required")
        }
    }
}
#[derive(Clone)]
struct Object {
    fields: Vec<(JsString, Native)>,
    prototype: Native,
}
struct Host {
    script: VecDeque<Value>,
    case: String,
    objects: HashMap<u64, Object>,
    arrays: HashMap<u64, Vec<Native>>,
    iterators: HashMap<u64, (u64, usize)>,
    predicates: HashMap<u64, SpotMarketPredicate<Native>>,
    fields: HashMap<String, Native>,
    updates: Vec<Value>,
}
fn eq(a: &Native, b: &Native) -> bool {
    match (a, b) {
        (Native::Null, Native::Null) | (Native::Undefined, Native::Undefined) => true,
        (Native::Bool(a), Native::Bool(b)) => a == b,
        (Native::Number(a), Native::Number(b)) => a == b,
        (Native::Text(a), Native::Text(b)) => a == b,
        (Native::Handle(a, _), Native::Handle(b, _)) => a == b,
        _ => false,
    }
}
fn key(value: &Native) -> JsString {
    match value {
        Native::Text(t) => t.clone(),
        Native::Number(v) if *v == 0.0 => JsString(vec![b'0' as u16]),
        _ => panic!("fixture property key"),
    }
}
impl Host {
    fn frame(&mut self, op: &str, args: Value) -> Value {
        let f = self
            .script
            .pop_front()
            .unwrap_or_else(|| panic!("BALANCE native operation exhausted {} {op}", self.case));
        assert_eq!(f["op"], op, "BALANCE native operation {}", self.case);
        assert_eq!(
            f["args"], args,
            "BALANCE native argument {} {op}",
            self.case
        );
        f
    }
    fn decode(f: &Value) -> Result<Native, Native> {
        if let Some(e) = f.get("throw") {
            Err(Native::decode(e))
        } else {
            Ok(Native::decode(&f["result"]))
        }
    }
    fn step(&mut self, op: &str, args: Value) -> Result<Native, Native> {
        Self::decode(&self.frame(op, args))
    }
    fn put(&mut self, o: &Native, key: JsString, v: &Native) {
        let obj = self.objects.get_mut(&o.id()).unwrap();
        if let Some((_, value)) = obj.fields.iter_mut().find(|(k, _)| *k == key) {
            *value = v.clone();
        } else {
            obj.fields.push((key, v.clone()));
        }
    }
    fn payload(&self, v: &Native) -> Value {
        if let Native::Handle(id, _) = v {
            if let Some(o) = self.objects.get(id) {
                return json!([
                    "literal",
                    v.native(),
                    o.prototype.native(),
                    o.fields
                        .iter()
                        .map(|(k, v)| json!([k.0, self.payload(v)]))
                        .collect::<Vec<_>>()
                ]);
            }
        }
        v.native()
    }
    fn outcome(result: &Result<Native, Native>) -> Value {
        match result {
            Ok(v) => json!({"value":v.native()}),
            Err(e) => json!({"throw":e.native()}),
        }
    }
    fn snapshot(&self) -> Value {
        let names = [
            "latestCorePerpsAccountValue",
            "latestCorePerpsUsedMargin",
            "latestCorePerpsUpnl",
        ];
        json!(names.map(|n| json!([n, self.fields[n].native()])))
    }
}
impl WsPingHost for Host {
    type Value = Native;
    type Error = Native;
    fn undefined(&self) -> Native {
        Native::Undefined
    }
    fn literal(&self, t: &JsString) -> Native {
        Native::Text(t.clone())
    }
    fn truthy(&self, v: &Native) -> bool {
        match v {
            Native::Undefined | Native::Null => false,
            Native::Bool(v) => *v,
            Native::Number(v) => *v != 0.0 && !v.is_nan(),
            Native::Text(v) => !v.0.is_empty(),
            Native::Handle(..) => true,
        }
    }
    fn global(&mut self, n: &str) -> Result<Native, Native> {
        self.step("global", json!({"name":n}))
    }
    fn get(&mut self, o: &Native, n: &str) -> Result<Native, Native> {
        let value = self.step("get", json!({"object":o.native(),"name":n}))?;
        if o.tag() == "exchange" {
            if let Some(expected) = self.fields.get(n) {
                assert_eq!(
                    value.native(),
                    expected.native(),
                    "BALANCE native tracked getter {}",
                    self.case
                );
            }
        }
        Ok(value)
    }
    fn set(&mut self, o: &Native, n: &str, v: &Native) -> Result<(), Native> {
        self.step(
            "set",
            json!({"object":o.native(),"name":n,"value":v.native()}),
        )?;
        assert!(
            self.fields.contains_key(n),
            "independent three tracked fields"
        );
        self.fields.insert(n.into(), v.clone());
        Ok(())
    }
    fn call(&mut self, f: &Native, o: &Native, args: &[Native]) -> Result<Native, Native> {
        let mut input = json!({"function":f.native(),"receiver":o.native(),"args":args.iter().map(Native::native).collect::<Vec<_>>()});
        if f.tag() == "update" {
            input["payload"] = self.payload(&args[0]);
        }
        let frame = self.frame("call", input);
        let expected = Self::decode(&frame);
        if f.tag() == "find" && frame["findInvoked"] == true {
            let capture = self.predicates[&args[0].id()].clone();
            let values = self.arrays[&o.id()].clone();
            let mut actual = Ok(Native::Undefined);
            for value in values {
                match run_spot_market_predicate(self, &capture, &value) {
                    Ok(true) => {
                        actual = Ok(value);
                        break;
                    }
                    Ok(false) => {}
                    Err(e) => {
                        actual = Err(e);
                        break;
                    }
                }
            }
            assert_eq!(
                Self::outcome(&actual),
                Self::outcome(&expected),
                "BALANCE native owned find result {}",
                self.case
            );
            return actual;
        }
        if expected.is_ok() && f.tag() == "update" {
            self.updates.push(self.payload(&args[0]));
        }
        expected
    }
    fn object(&mut self) -> Result<Native, Native> {
        let frame = self.frame("object", json!({}));
        let value = Self::decode(&frame)?;
        self.objects.insert(
            value.id(),
            Object {
                fields: Vec::new(),
                prototype: Native::decode(&frame["prototype"]),
            },
        );
        Ok(value)
    }
    fn define_data(&mut self, o: &Native, n: &str, v: &Native) -> Result<(), Native> {
        self.step(
            "define",
            json!({"object":o.native(),"name":n,"value":v.native()}),
        )?;
        self.put(o, JsString(n.encode_utf16().collect()), v);
        Ok(())
    }
    fn callback(&mut self, _: PingCallback<Native>) -> Result<Native, Native> {
        unreachable!()
    }
}
impl BalanceHost for Host {
    fn null(&self) -> Native {
        Native::Null
    }
    fn number(&self, v: f64) -> Native {
        Native::Number(v)
    }
    fn boolean(&self, v: bool) -> Native {
        Native::Bool(v)
    }
    fn is_nullish(&self, v: &Native) -> bool {
        matches!(v, Native::Undefined | Native::Null)
    }
    fn is_undefined(&self, v: &Native) -> bool {
        matches!(v, Native::Undefined)
    }
    fn strict_equal(&self, a: &Native, b: &Native) -> bool {
        eq(a, b)
    }
    fn index(&mut self, o: &Native, k: &Native) -> Result<Native, Native> {
        self.step("index", json!({"object":o.native(),"key":k.native()}))
    }
    fn set_index(&mut self, o: &Native, k: &Native, v: &Native) -> Result<(), Native> {
        self.step(
            "setIndex",
            json!({"object":o.native(),"key":k.native(),"value":v.native()}),
        )?;
        let k = key(k);
        if k.0 == "__proto__".encode_utf16().collect::<Vec<_>>() {
            if matches!(v, Native::Handle(..) | Native::Null) {
                self.objects.get_mut(&o.id()).unwrap().prototype = v.clone();
            }
        } else {
            self.put(o, k, v);
        }
        Ok(())
    }
    fn define_index(&mut self, o: &Native, k: &Native, v: &Native) -> Result<(), Native> {
        self.step(
            "defineIndex",
            json!({"object":o.native(),"key":k.native(),"value":v.native()}),
        )?;
        self.put(o, key(k), v);
        Ok(())
    }
    fn array(&mut self) -> Result<Native, Native> {
        let v = self.step("array", json!({}))?;
        self.arrays.insert(v.id(), Vec::new());
        Ok(v)
    }
    fn array_define(&mut self, o: &Native, i: usize, v: &Native) -> Result<(), Native> {
        self.step(
            "arrayDefine",
            json!({"object":o.native(),"index":i,"value":v.native()}),
        )?;
        let a = self.arrays.get_mut(&o.id()).unwrap();
        assert_eq!(a.len(), i, "literal contiguous array elements");
        a.push(v.clone());
        Ok(())
    }
    fn iterator(&mut self, v: &Native) -> Result<Native, Native> {
        let i = self.step("iterator", json!({"value":v.native()}))?;
        if let Native::Handle(id, _) = v {
            if self.arrays.contains_key(id) {
                self.iterators.insert(i.id(), (*id, 0));
            }
        }
        Ok(i)
    }
    fn next(&mut self, i: &Native) -> Result<Option<Native>, Native> {
        let f = self.frame("next", json!({"iterator":i.native()}));
        if f.get("throw").is_some() {
            return Self::decode(&f).map(Some);
        }
        let value = if f["result"] == json!(["done"]) {
            None
        } else {
            Some(Native::decode(&f["result"]))
        };
        if let Some((array, index)) = self.iterators.get_mut(&i.id()) {
            let expected = self.arrays[array].get(*index);
            assert_eq!(
                value.as_ref().map(Native::native),
                expected.map(Native::native),
                "BALANCE native owned array iteration {}",
                self.case
            );
            *index += 1;
        }
        Ok(value)
    }
    fn close_after_throw(&mut self, i: &Native) {
        let _ = self.step("close", json!({"iterator":i.native()}));
    }
    fn close_after_return(&mut self, i: &Native) -> Result<(), Native> {
        self.step("close", json!({"iterator":i.native()}))
            .map(|_| ())
    }
    fn pair(&mut self, v: &Native) -> Result<[Native; 2], Native> {
        let f = self.frame("pair", json!({"value":v.native()}));
        if let Some(e) = f.get("throw") {
            return Err(Native::decode(e));
        }
        assert_eq!(
            f["result"].as_array().unwrap().len(),
            2,
            "independent two binding elements"
        );
        Ok([
            Native::decode(&f["result"][0]),
            Native::decode(&f["result"][1]),
        ])
    }
    fn template_string(&mut self, v: &Native) -> Result<JsString, Native> {
        let value = self.step("template", json!({"value":v.native()}))?;
        let Native::Text(t) = value else {
            panic!("native template string")
        };
        Ok(t)
    }
    fn operator(&mut self, op: BalanceOperator, a: &Native, b: &Native) -> Result<Native, Native> {
        let name = match op {
            BalanceOperator::Add => "add",
            BalanceOperator::Subtract => "subtract",
            BalanceOperator::Multiply => "multiply",
            BalanceOperator::Greater => "greater",
        };
        let value = self.step(
            "operator",
            json!({"operator":name,"left":a.native(),"right":b.native()}),
        )?;
        if let (Native::Number(a), Native::Number(b)) = (a, b) {
            if op == BalanceOperator::Greater {
                assert_eq!(
                    value.native(),
                    Native::Bool(*a > *b).native(),
                    "BALANCE native numeric comparison {}",
                    self.case
                );
            } else {
                let expected = match op {
                    BalanceOperator::Add => a + b,
                    BalanceOperator::Subtract => a - b,
                    BalanceOperator::Multiply => a * b,
                    _ => unreachable!(),
                };
                let Native::Number(actual) = &value else {
                    panic!("numeric operator result")
                };
                assert!(
                    expected.is_nan() && actual.is_nan() || expected.to_bits() == actual.to_bits(),
                    "BALANCE native arithmetic bits {}",
                    self.case
                );
            }
        }
        Ok(value)
    }
    fn spread_into(&mut self, o: &Native, source: &Native) -> Result<(), Native> {
        let f = self.script.pop_front().unwrap();
        assert_eq!(
            f["op"], "spread",
            "BALANCE native spread operation {}",
            self.case
        );
        assert_eq!(
            f["args"]["object"],
            o.native(),
            "BALANCE native spread target {}",
            self.case
        );
        assert_eq!(
            f["args"]["source"],
            source.native(),
            "BALANCE native spread source {}",
            self.case
        );
        let members = f["args"]["members"].as_array().unwrap();
        if let Native::Handle(id, _) = source {
            if let Some(object) = self.objects.get(id) {
                let own = object
                    .fields
                    .iter()
                    .map(|(k, v)| json!([String::from_utf16(&k.0).unwrap(), v.native()]))
                    .collect::<Vec<_>>();
                assert_eq!(
                    json!(own),
                    json!(members),
                    "BALANCE native owned spread fields {}",
                    self.case
                );
            }
        }
        Self::decode(&f)?;
        for pair in members {
            self.put(
                o,
                JsString(pair[0].as_str().unwrap().encode_utf16().collect()),
                &Native::decode(&pair[1]),
            );
        }
        Ok(())
    }
    fn spot_market_predicate(
        &mut self,
        capture: SpotMarketPredicate<Native>,
    ) -> Result<Native, Native> {
        let v = self.step(
            "predicate",
            json!({"upper":capture.upper.native(),"tokenIndex":capture.token_index.native()}),
        )?;
        self.predicates.insert(v.id(), capture);
        Ok(v)
    }
}
#[test]
fn native_ten_balance_roots_match_actual_source() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = std::process::Command::new("node")
        .arg(root.join("scripts/balance-state-oracle.cjs"))
        .output()
        .expect("source oracle");
    assert!(
        output.status.success(),
        "source oracle failed {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    let names = [
        "core.getSpotCollateralMarkPrice",
        "core.buildUnifiedSpotCollateralSnapshot",
        "core.getAuthoritativeUnifiedUsdcAvailability",
        "core.applyBalanceFromCurrentMode",
        "core.applyCorePerpsBalanceSnapshot",
        "core.requestUnifiedSpotCollateralRefresh",
        "spot.getSpotCoinMarkPrice",
        "spot.buildSpotBalance",
        "spot.applySpotBalances",
        "spot.requestSpotBalanceRefresh",
    ];
    assert_eq!(oracle["names"], json!(names), "independent ten roots");
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 152, "independent152cases");
    assert_eq!(oracle["iterationStop"], 151);
    let mut counts = [0; 10];
    let mut failures = Vec::new();
    for case in cases {
        let name = format!(
            "{}.{}",
            case["kind"].as_str().unwrap(),
            case["name"].as_str().unwrap()
        );
        counts[names.iter().position(|n| *n == name).unwrap()] += 1;
        let verdict = catch_unwind(AssertUnwindSafe(|| {
            let input = Native::decode(&case["input"]);
            let args = case["args"]
                .as_array()
                .unwrap()
                .iter()
                .map(Native::decode)
                .collect::<Vec<_>>();
            let initial = case["initial"].as_array().unwrap();
            assert_eq!(initial.len(), 3, "independent three state fields");
            for (i, (key, n)) in [
                ("latestCorePerpsAccountValue", 501.25f64),
                ("latestCorePerpsUsedMargin", 91.25),
                ("latestCorePerpsUpnl", 17.5),
            ]
            .iter()
            .enumerate()
            {
                assert_eq!(initial[i][0], *key);
                let Native::Number(actual) = Native::decode(&initial[i][1]) else {
                    panic!("native initial number")
                };
                assert_eq!(
                    actual.to_bits(),
                    n.to_bits(),
                    "independent native initial field bits"
                );
            }
            let mut host = Host {
                script: case["calls"].as_array().unwrap().iter().cloned().collect(),
                case: format!("{name} {}", case["label"]),
                objects: HashMap::new(),
                arrays: HashMap::new(),
                iterators: HashMap::new(),
                predicates: HashMap::new(),
                fields: initial
                    .iter()
                    .map(|v| (v[0].as_str().unwrap().into(), Native::decode(&v[1])))
                    .collect(),
                updates: Vec::new(),
            };
            for seed in case["seedArrays"].as_array().unwrap() {
                host.arrays.insert(
                    Native::decode(&seed["value"]).id(),
                    seed["elements"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(Native::decode)
                        .collect(),
                );
            }
            let result = match name.as_str() {
                "core.getSpotCollateralMarkPrice" => {
                    get_spot_collateral_mark_price(&mut host, &input, &args[0])
                }
                "core.buildUnifiedSpotCollateralSnapshot" => {
                    build_unified_spot_collateral_snapshot(&mut host, &input, &args[0])
                }
                "core.getAuthoritativeUnifiedUsdcAvailability" => {
                    get_authoritative_unified_usdc_availability(&mut host, &args[0])
                }
                "core.applyBalanceFromCurrentMode" => {
                    apply_balance_from_current_mode(&mut host, &input).map(|()| Native::Undefined)
                }
                "core.applyCorePerpsBalanceSnapshot" => {
                    apply_core_perps_balance_snapshot(&mut host, &input, &args[0])
                        .map(|()| Native::Undefined)
                }
                "core.requestUnifiedSpotCollateralRefresh" => {
                    request_unified_spot_collateral_refresh(&mut host, &input, &args[0])
                        .map(|()| Native::Undefined)
                }
                "spot.getSpotCoinMarkPrice" => {
                    get_spot_coin_mark_price(&mut host, &input, &args[0], &args[1])
                }
                "spot.buildSpotBalance" => build_spot_balance(&mut host, &input, &args[0]),
                "spot.applySpotBalances" => {
                    apply_spot_balances(&mut host, &input, &args[0]).map(|()| Native::Undefined)
                }
                "spot.requestSpotBalanceRefresh" => {
                    request_spot_balance_refresh(&mut host, &input, &args[0])
                        .map(|()| Native::Undefined)
                }
                _ => unreachable!(),
            };
            let actual = match result {
                Ok(v) => json!({"value":host.payload(&v)}),
                Err(e) => json!({"throw":e.native()}),
            };
            assert_eq!(
                actual, case["result"],
                "BALANCE native result/field tree/aliases {name}"
            );
            assert_eq!(
                json!(host.updates),
                case["updates"],
                "BALANCE native store update payloads {name}"
            );
            assert_eq!(
                host.snapshot(),
                case["final"],
                "BALANCE native final state fields {name}"
            );
            assert!(
                host.script.is_empty(),
                "BALANCE native unconsumed effects {name}"
            );
        }));
        if verdict.is_err() && !failures.contains(&name) {
            failures.push(name);
        }
    }
    assert_eq!(
        counts,
        [22, 16, 17, 15, 12, 9, 28, 16, 8, 9],
        "independent root fixture counts"
    );
    assert!(
        failures.is_empty(),
        "BALANCE named native failures:{failures:?}"
    );
}
