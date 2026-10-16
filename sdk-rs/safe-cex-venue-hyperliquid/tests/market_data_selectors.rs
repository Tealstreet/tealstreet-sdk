use safe_cex_venue_hyperliquid::{
    market_data_selectors::*, spot_json::JsString, ws_ping_lifecycle::*,
};
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
struct Host {
    script: VecDeque<Value>,
    case: String,
    lower: Vec<(Native, Native)>,
    objects: HashMap<u64, Vec<(String, Native)>>,
    sets: HashMap<u64, Vec<Native>>,
}
fn equal(a: &Native, b: &Native) -> bool {
    match (a, b) {
        (Native::Undefined, Native::Undefined) | (Native::Null, Native::Null) => true,
        (Native::Bool(a), Native::Bool(b)) => a == b,
        (Native::Number(a), Native::Number(b)) => a == b,
        (Native::Text(a), Native::Text(b)) => a == b,
        (Native::Handle(a, _), Native::Handle(b, _)) => a == b,
        _ => false,
    }
}
impl Host {
    fn frame(&mut self, op: &str, args: Value) -> Value {
        let frame = self
            .script
            .pop_front()
            .unwrap_or_else(|| panic!("SELECTOR native operation exhausted {} {op}", self.case));
        assert_eq!(frame["op"], op, "SELECTOR native operation {}", self.case);
        assert_eq!(
            frame["args"], args,
            "SELECTOR native argument {} {op}",
            self.case
        );
        frame
    }
    fn decode_result(frame: &Value) -> Result<Native, Native> {
        if let Some(e) = frame.get("throw") {
            Err(Native::decode(e))
        } else {
            Ok(Native::decode(&frame["result"]))
        }
    }
    fn step(&mut self, op: &str, args: Value) -> Result<Native, Native> {
        Self::decode_result(&self.frame(op, args))
    }
    fn payload(&self, value: &Native) -> Value {
        if let Native::Handle(id, _) = value {
            if let Some(fields) = self.objects.get(id) {
                return json!([
                    "literal",
                    value.native(),
                    fields
                        .iter()
                        .map(|(k, v)| json!([
                            k.encode_utf16().collect::<Vec<_>>(),
                            self.payload(v)
                        ]))
                        .collect::<Vec<_>>()
                ]);
            }
            if let Some(members) = self.sets.get(id) {
                return json!([
                    "set",
                    value.native(),
                    members.iter().map(Native::native).collect::<Vec<_>>()
                ]);
            }
        }
        value.native()
    }
}
impl WsPingHost for Host {
    type Value = Native;
    type Error = Native;
    fn undefined(&self) -> Native {
        Native::Undefined
    }
    fn literal(&self, value: &JsString) -> Native {
        Native::Text(value.clone())
    }
    fn truthy(&self, v: &Native) -> bool {
        match v {
            Native::Null | Native::Undefined => false,
            Native::Bool(v) => *v,
            Native::Number(v) => *v != 0.0 && !v.is_nan(),
            Native::Text(v) => !v.0.is_empty(),
            Native::Handle(..) => true,
        }
    }
    fn global(&mut self, name: &str) -> Result<Native, Native> {
        self.step("global", json!({"name":name}))
    }
    fn get(&mut self, o: &Native, name: &str) -> Result<Native, Native> {
        let result = self.step("get", json!({"object":o.native(),"name":name}))?;
        if o.tag() == "lowerMap" && name == "size" {
            let Native::Number(size) = result else {
                panic!("native map size")
            };
            assert_eq!(
                size.to_bits(),
                (self.lower.len() as f64).to_bits(),
                "SELECTOR native map size {}",
                self.case
            );
            return Ok(Native::Number(size));
        }
        Ok(result)
    }
    fn set(&mut self, _: &Native, _: &str, _: &Native) -> Result<(), Native> {
        unreachable!("no assignment in selector roots")
    }
    fn call(
        &mut self,
        function: &Native,
        receiver: &Native,
        args: &[Native],
    ) -> Result<Native, Native> {
        let result=self.step("call",json!({"function":function.native(),"receiver":receiver.native(),"args":args.iter().map(Native::native).collect::<Vec<_>>()}))?;
        match (receiver.tag(), function.tag()) {
            ("lowerMap", "clear") => {
                assert!(matches!(result, Native::Undefined));
                self.lower.clear();
            }
            ("lowerMap", "set") => {
                assert!(
                    equal(&result, receiver),
                    "SELECTOR native map set return alias {}",
                    self.case
                );
                if let Some(slot) = self.lower.iter_mut().find(|(k, _)| equal(k, &args[0])) {
                    slot.1 = args[1].clone();
                } else {
                    self.lower.push((args[0].clone(), args[1].clone()));
                }
            }
            ("lowerMap", "get") => {
                let expected = self
                    .lower
                    .iter()
                    .find(|(k, _)| equal(k, &args[0]))
                    .map(|(_, v)| v.clone())
                    .unwrap_or(Native::Undefined);
                assert_eq!(
                    result.native(),
                    expected.native(),
                    "SELECTOR native map lookup alias {}",
                    self.case
                );
            }
            ("newSet", "add") => {
                assert!(
                    equal(&result, receiver),
                    "SELECTOR native Set.add return alias {}",
                    self.case
                );
                let members = self.sets.get_mut(&receiver.id()).unwrap();
                if !members.iter().any(|v| equal(v, &args[0])) {
                    members.push(args[0].clone());
                }
            }
            _ => {}
        }
        Ok(result)
    }
    fn object(&mut self) -> Result<Native, Native> {
        let result = self.step("object", json!({}))?;
        self.objects.insert(result.id(), Vec::new());
        Ok(result)
    }
    fn define_data(&mut self, o: &Native, name: &str, value: &Native) -> Result<(), Native> {
        self.step(
            "define",
            json!({"object":o.native(),"name":name,"value":value.native()}),
        )?;
        self.objects
            .get_mut(&o.id())
            .unwrap()
            .push((name.into(), value.clone()));
        Ok(())
    }
    fn callback(&mut self, _: PingCallback<Native>) -> Result<Native, Native> {
        unreachable!()
    }
}
impl MarketDataHost for Host {
    fn null(&self) -> Native {
        Native::Null
    }
    fn number(&self, v: f64) -> Native {
        Native::Number(v)
    }
    fn is_nullish(&self, v: &Native) -> bool {
        matches!(v, Native::Null | Native::Undefined)
    }
    fn is_string(&self, v: &Native) -> bool {
        matches!(v, Native::Text(_))
    }
    fn strict_equal(&self, a: &Native, b: &Native) -> bool {
        equal(a, b)
    }
    fn index(&mut self, o: &Native, key: &Native) -> Result<Native, Native> {
        self.step("index", json!({"object":o.native(),"key":key.native()}))
    }
    fn iterator(&mut self, v: &Native) -> Result<Native, Native> {
        self.step("iterator", json!({"value":v.native()}))
    }
    fn next(&mut self, iterator: &Native) -> Result<Option<Native>, Native> {
        let frame = self.frame("next", json!({"iterator":iterator.native()}));
        if frame["result"] == json!(["done"]) {
            Ok(None)
        } else {
            Self::decode_result(&frame).map(Some)
        }
    }
    fn close_after_throw(&mut self, iterator: &Native) {
        let _ = self.step("close", json!({"iterator":iterator.native()}));
    }
    fn construct(&mut self, f: &Native, args: &[Native]) -> Result<Native, Native> {
        let result=self.step("construct",json!({"function":f.native(),"args":args.iter().map(Native::native).collect::<Vec<_>>()}))?;
        assert_eq!(f.tag(), "Set", "independent Set constructor");
        assert!(args.is_empty(), "independent constructor arity0");
        self.sets.insert(result.id(), Vec::new());
        Ok(result)
    }
    fn first_binding(&mut self, v: &Native) -> Result<Native, Native> {
        self.step("first", json!({"value":v.native()}))
    }
    fn unit_regex(&mut self) -> Result<Native, Native> {
        self.step("regex", json!({"pattern":"^U[A-Z0-9]"}))
    }
    fn template_string(&mut self, v: &Native) -> Result<JsString, Native> {
        let result = self.step("template", json!({"value":v.native()}))?;
        let Native::Text(text) = result else {
            panic!("native template string")
        };
        Ok(text)
    }
    fn subtract(&mut self, left: &Native, right: &Native) -> Result<Native, Native> {
        let result = self.step(
            "subtract",
            json!({"left":left.native(),"right":right.native()}),
        )?;
        if let (Native::Number(l), Native::Number(r), Native::Number(v)) = (left, right, &result) {
            assert!(
                (l - r).is_nan() && v.is_nan() || (l - r).to_bits() == v.to_bits(),
                "SELECTOR native subtract bits {}",
                self.case
            );
        }
        Ok(result)
    }
}
#[test]
fn native_ten_market_data_selectors_match_actual_source() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = std::process::Command::new("node")
        .arg(root.join("scripts/market-data-selectors-oracle.cjs"))
        .output()
        .expect("source oracle");
    assert!(
        output.status.success(),
        "source oracle failed {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    let names = [
        "core.buildMarketsLowerCaseMap",
        "core.resolveCoinToSymbol",
        "spot.symbolOrIdToCoin",
        "core.resolveSpotDisplayName",
        "core.extractClearinghouseBalanceSummary",
        "core.responseHasDeferredOrderStatus",
        "private.getStaleCheckChannelKey",
        "private.collectHip3DexesFromFills",
        "core.priceToPrecision",
        "spot.priceToPrecision",
    ];
    assert_eq!(
        oracle["names"],
        json!(names),
        "independent ten source roots"
    );
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 161, "independent161cases");
    assert_eq!(oracle["iterationStop"], 160);
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
            let initial = case["initialLower"].as_array().unwrap();
            assert_eq!(initial.len(), 2, "independent two initial map entries");
            assert_eq!(
                initial[0][0],
                json!(["string", "old".encode_utf16().collect::<Vec<_>>()]),
                "independent old key"
            );
            assert_eq!(
                initial[1][0],
                json!(["string", "dex:btc".encode_utf16().collect::<Vec<_>>()]),
                "independent fallback key"
            );
            let mut host = Host {
                script: case["calls"].as_array().unwrap().iter().cloned().collect(),
                case: format!("{name} {}", case["label"]),
                lower: initial
                    .iter()
                    .map(|v| (Native::decode(&v[0]), Native::decode(&v[1])))
                    .collect(),
                objects: HashMap::new(),
                sets: HashMap::new(),
            };
            let result = match name.as_str() {
                "core.buildMarketsLowerCaseMap" => {
                    build_markets_lower_case_map(&mut host, &input, &args[0])
                        .map(|()| Native::Undefined)
                }
                "core.resolveCoinToSymbol" => resolve_coin_to_symbol(&mut host, &input, &args[0]),
                "spot.symbolOrIdToCoin" => spot_symbol_or_id_to_coin(&mut host, &args[0]),
                "core.resolveSpotDisplayName" => {
                    resolve_spot_display_name(&mut host, &args[0], &args[1])
                }
                "core.extractClearinghouseBalanceSummary" => {
                    extract_clearinghouse_balance_summary(&mut host, &args[0])
                }
                "core.responseHasDeferredOrderStatus" => {
                    response_has_deferred_order_status(&mut host, &args[0])
                }
                "private.getStaleCheckChannelKey" => {
                    get_stale_check_channel_key(&mut host, &args[0], &args[1])
                }
                "private.collectHip3DexesFromFills" => {
                    collect_hip3_dexes_from_fills(&mut host, &input, &args[0])
                }
                "core.priceToPrecision" => {
                    core_price_to_precision(&mut host, &input, &args[0], &args[1])
                }
                "spot.priceToPrecision" => {
                    spot_price_to_precision(&mut host, &input, &args[0], &args[1])
                }
                _ => unreachable!(),
            };
            let actual = match result {
                Ok(v) => json!({"value":host.payload(&v)}),
                Err(e) => json!({"throw":e.native()}),
            };
            assert_eq!(
                actual, case["result"],
                "SELECTOR native fields/result/alias {name}"
            );
            let actual = host
                .lower
                .iter()
                .map(|(k, v)| json!([k.native(), v.native()]))
                .collect::<Vec<_>>();
            assert_eq!(
                json!(actual),
                case["finalLower"],
                "SELECTOR native final map members/aliases {name}"
            );
            assert!(
                host.script.is_empty(),
                "SELECTOR native unconsumed effects {name}"
            );
        }));
        if verdict.is_err() && !failures.contains(&name) {
            failures.push(name);
        }
    }
    assert_eq!(
        counts,
        [15, 17, 12, 17, 16, 8, 12, 20, 22, 22],
        "independent familycounts"
    );
    assert!(
        failures.is_empty(),
        "SELECTOR named native failures:{failures:?}"
    );
}
