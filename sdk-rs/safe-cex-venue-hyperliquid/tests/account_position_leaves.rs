use safe_cex_venue_hyperliquid::{
    account_position_leaves::*, spot_json::JsString, ws_ping_lifecycle::*,
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
    exchange: Native,
    cache: Native,
    objects: HashMap<u64, Vec<(String, Native)>>,
    positions: Vec<Option<Native>>,
    predicates: HashMap<u64, OppositePredicate<Native>>,
    some_call_throws: bool,
}
impl Host {
    fn frame(&mut self, op: &str, args: Value) -> Value {
        let next = self.script.pop_front().expect("source operation exhausted");
        assert_eq!(next["op"], op, "POSITION native operation {}", self.case);
        assert_eq!(
            next["args"], args,
            "POSITION native argument {} {op}",
            self.case
        );
        next
    }
    fn decode_result(frame: &Value) -> Result<Native, Native> {
        if let Some(error) = frame.get("throw") {
            Err(Native::decode(error))
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
                        .map(|(key, v)| json!([
                            key.encode_utf16().collect::<Vec<_>>(),
                            self.payload(v)
                        ]))
                        .collect::<Vec<_>>()
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
    fn truthy(&self, value: &Native) -> bool {
        match value {
            Native::Undefined | Native::Null => false,
            Native::Bool(v) => *v,
            Native::Number(v) => *v != 0.0 && !v.is_nan(),
            Native::Text(v) => !v.0.is_empty(),
            Native::Handle(..) => true,
        }
    }
    fn global(&mut self, name: &str) -> Result<Native, Native> {
        self.step("global", json!({"name":name}))
    }
    fn get(&mut self, object: &Native, name: &str) -> Result<Native, Native> {
        self.step("get", json!({"object":object.native(),"name":name}))
    }
    fn set(&mut self, object: &Native, name: &str, value: &Native) -> Result<(), Native> {
        self.step(
            "set",
            json!({"object":object.native(),"name":name,"value":value.native()}),
        )?;
        assert_eq!(
            object.id(),
            self.exchange.id(),
            "POSITION native cache receiver {}",
            self.case
        );
        assert_eq!(
            name, "_fakePositionsCache",
            "POSITION native cache field {}",
            self.case
        );
        self.cache = value.clone();
        Ok(())
    }
    fn object(&mut self) -> Result<Native, Native> {
        let value = self.step("object", json!({}))?;
        self.objects.insert(value.id(), Vec::new());
        Ok(value)
    }
    fn define_data(&mut self, object: &Native, name: &str, value: &Native) -> Result<(), Native> {
        self.step(
            "define",
            json!({"object":object.native(),"name":name,"value":value.native()}),
        )?;
        let fields = self.objects.get_mut(&object.id()).unwrap();
        if let Some((_, v)) = fields.iter_mut().find(|(k, _)| k == name) {
            *v = value.clone();
        } else {
            fields.push((name.into(), value.clone()));
        }
        Ok(())
    }
    fn callback(&mut self, _: PingCallback<Native>) -> Result<Native, Native> {
        panic!("position leaves allocate some predicates, not timers")
    }
    fn call(
        &mut self,
        function: &Native,
        receiver: &Native,
        args: &[Native],
    ) -> Result<Native, Native> {
        let frame=self.frame("call",json!({"function":function.native(),"receiver":receiver.native(),"args":args.iter().map(Native::native).collect::<Vec<_>>()}));
        if function.tag() == "some" && !self.some_call_throws {
            let capture = self.predicates[&args[0].id()].clone();
            let mut result = Ok(Native::Bool(false));
            for position in self.positions.clone().into_iter().flatten() {
                match run_opposite_predicate(self, &capture, &position) {
                    Ok(true) => {
                        result = Ok(Native::Bool(true));
                        break;
                    }
                    Ok(false) => {}
                    Err(error) => {
                        result = Err(error);
                        break;
                    }
                }
            }
            let actual = match &result {
                Ok(v) => json!({"value":v.native()}),
                Err(e) => json!({"throw":e.native()}),
            };
            let expected = match Self::decode_result(&frame) {
                Ok(v) => json!({"value":v.native()}),
                Err(e) => json!({"throw":e.native()}),
            };
            assert_eq!(
                actual, expected,
                "POSITION native some predicate aggregate {}",
                self.case
            );
            return result;
        }
        Self::decode_result(&frame)
    }
}
impl AccountPositionHost for Host {
    fn null(&self) -> Native {
        Native::Null
    }
    fn boolean(&self, value: bool) -> Native {
        Native::Bool(value)
    }
    fn number(&self, value: f64) -> Native {
        Native::Number(value)
    }
    fn is_nullish(&self, value: &Native) -> bool {
        matches!(value, Native::Undefined | Native::Null)
    }
    fn strict_equal(&self, left: &Native, right: &Native) -> bool {
        match (left, right) {
            (Native::Undefined, Native::Undefined) | (Native::Null, Native::Null) => true,
            (Native::Bool(a), Native::Bool(b)) => a == b,
            (Native::Number(a), Native::Number(b)) => a == b,
            (Native::Text(a), Native::Text(b)) => a.0 == b.0,
            (Native::Handle(a, _), Native::Handle(b, _)) => a == b,
            _ => false,
        }
    }
    fn greater_equal(&mut self, left: &Native, right: &Native) -> Result<bool, Native> {
        let value = self.step("ge", json!({"left":left.native(),"right":right.native()}))?;
        let Native::Bool(value) = value else {
            panic!("native relational boolean required")
        };
        Ok(value)
    }
    fn opposite_predicate(&mut self, capture: OppositePredicate<Native>) -> Result<Native, Native> {
        let value=self.step("predicate",json!({"symbol":capture.symbol.native(),"targetSide":capture.target_side.native(),"amount":capture.amount.native()}))?;
        self.predicates.insert(value.id(), capture);
        Ok(value)
    }
}
#[test]
fn native_ten_account_position_leaves_match_actual_source() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = std::process::Command::new("node")
        .arg(root.join("scripts/account-position-oracle.cjs"))
        .output()
        .expect("Node source oracle");
    assert!(
        output.status.success(),
        "source oracle failed {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    let names = [
        "core.shouldUseSpotCollateralBalance",
        "core.shouldRequestPortfolioMarginSpotState",
        "core.shouldTrackSpotHoldings",
        "core._getSymbolConfigurationFromAssetData",
        "core.getFakePositions",
        "core.invalidateFakePositionsCache",
        "core._hasFullSizedPositionOppositeSide",
        "core.resolveReduceOnly",
        "spot._getSymbolConfigurationFromAssetData",
        "spot.getFakePositions",
    ];
    assert_eq!(oracle["names"], json!(names), "independent ten roots");
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 129, "independent 129cases");
    assert_eq!(oracle["iterationStop"], 128);
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
            let exchange = Native::decode(&case["input"]);
            let args = case["args"]
                .as_array()
                .unwrap()
                .iter()
                .map(Native::decode)
                .collect::<Vec<_>>();
            let mut host = Host {
                script: case["calls"].as_array().unwrap().iter().cloned().collect(),
                case: format!("{name} {}", case["label"]),
                exchange: exchange.clone(),
                cache: Native::decode(&case["initialCache"]),
                objects: HashMap::new(),
                positions: case["initialPositions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| {
                        if v[0] == "hole" {
                            None
                        } else {
                            Some(Native::decode(v))
                        }
                    })
                    .collect(),
                predicates: HashMap::new(),
                some_call_throws: case["someCallThrows"] == true,
            };
            for config in case["configs"].as_array().unwrap() {
                let value = Native::decode(&config["handle"]);
                let fields = config["fields"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|pair| {
                        (
                            pair[0].as_str().unwrap().to_string(),
                            Native::decode(&pair[1]),
                        )
                    })
                    .collect::<Vec<_>>();
                assert_eq!(fields.len(), 3, "independent three config members");
                let margin = &fields[0];
                let mode = &fields[1];
                let leverage = &fields[2];
                assert_eq!(margin.0, "marginType");
                assert_eq!(
                    margin.1.native(),
                    json!(["string", "cross".encode_utf16().collect::<Vec<_>>()]),
                    "native default margin"
                );
                assert_eq!(mode.0, "positionMode");
                assert_eq!(
                    mode.1.native(),
                    json!(["string", "oneway".encode_utf16().collect::<Vec<_>>()]),
                    "native default mode"
                );
                assert_eq!(leverage.0, "leverage");
                let Native::Number(number) = leverage.1 else {
                    panic!("native default leverage")
                };
                assert_eq!(
                    number.to_bits(),
                    if value.tag() == "coreDefault" {
                        20.0f64
                    } else {
                        1.0f64
                    }
                    .to_bits(),
                    "native default leverage bits"
                );
                host.objects.insert(value.id(), fields);
            }
            let result = match name.as_str() {
                "core.shouldUseSpotCollateralBalance" => {
                    should_use_spot_collateral_balance(&mut host, &exchange)
                        .map(|v| host.boolean(v))
                }
                "core.shouldRequestPortfolioMarginSpotState" => {
                    should_request_portfolio_margin_spot_state(&mut host, &exchange)
                        .map(|v| host.boolean(v))
                }
                "core.shouldTrackSpotHoldings" => should_track_spot_holdings(&mut host, &exchange),
                "core._getSymbolConfigurationFromAssetData" => {
                    symbol_configuration_from_asset_data(&mut host, &exchange, &args[0])
                }
                "spot._getSymbolConfigurationFromAssetData" => {
                    spot_symbol_configuration_from_asset_data(&mut host)
                }
                "core.getFakePositions" => get_fake_positions(&mut host, &exchange),
                "spot.getFakePositions" => spot_get_fake_positions(&mut host, &exchange),
                "core.invalidateFakePositionsCache" => {
                    invalidate_fake_positions_cache(&mut host, &exchange).map(|()| host.undefined())
                }
                "core._hasFullSizedPositionOppositeSide" => has_full_sized_position_opposite_side(
                    &mut host, &exchange, &args[0], &args[1], &args[2],
                ),
                "core.resolveReduceOnly" => resolve_reduce_only(&mut host, &args[0], &args[1]),
                _ => unreachable!(),
            };
            let actual = match result {
                Ok(v) => json!({"value":host.payload(&v)}),
                Err(e) => json!({"throw":e.native()}),
            };
            assert_eq!(
                actual, case["result"],
                "POSITION native result fields/alias {name}"
            );
            assert_eq!(
                host.cache.native(),
                case["finalCache"],
                "POSITION native final cache alias {name}"
            );
            assert!(
                host.script.is_empty(),
                "POSITION native unconsumed effects {name}"
            );
        }));
        if verdict.is_err() && !failures.contains(&name) {
            failures.push(name);
        }
    }
    assert_eq!(
        counts,
        [13, 12, 20, 20, 16, 3, 23, 15, 3, 4],
        "independent family counts"
    );
    assert!(
        failures.is_empty(),
        "POSITION named native failures:{failures:?}"
    );
}
