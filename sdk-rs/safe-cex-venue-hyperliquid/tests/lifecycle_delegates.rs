use safe_cex_venue_hyperliquid::{
    lifecycle_delegates::*, spot_json::JsString, ws_ping_lifecycle::*,
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
    fields: HashMap<String, Native>,
    collections: Vec<Vec<String>>,
    effects: [u64; 5],
    callbacks: HashMap<u64, LifecycleCallback<Native>>,
    registered: Vec<Native>,
    catch_error: Option<Native>,
    catch_call_throws: bool,
    end_call_throws: bool,
}
impl Host {
    fn frame(&mut self, op: &str, args: Value) -> Value {
        let next = self
            .script
            .pop_front()
            .unwrap_or_else(|| panic!("LIFECYCLE native operation exhausted {} {op}", self.case));
        assert_eq!(next["op"], op, "LIFECYCLE native operation {}", self.case);
        assert_eq!(
            next["args"], args,
            "LIFECYCLE native argument {} {op}",
            self.case
        );
        next
    }
    fn decode_result(frame: &Value) -> Result<Native, Native> {
        match frame.get("throw") {
            Some(error) => Err(Native::decode(error)),
            None => Ok(Native::decode(&frame["result"])),
        }
    }
    fn step(&mut self, op: &str, args: Value) -> Result<Native, Native> {
        Self::decode_result(&self.frame(op, args))
    }
    fn result(result: &Result<Native, Native>) -> Value {
        match result {
            Ok(v) => json!({"value":v.native()}),
            Err(e) => json!({"throw":e.native()}),
        }
    }
    fn snapshot(&self) -> Value {
        let names = [
            "_fetchOrdersInFlight",
            "_orderRefreshDebounceTimer",
            "_hip3PositionRefreshDebounceTimer",
            "_retryChainTimer",
            "_retryChainActive",
            "_orderRefreshPending",
            "_lastOrderRefreshTime",
        ];
        json!({"fields":names.map(|n|json!([n,self.fields[n].native()])),"collections":self.collections,"effects":self.effects})
    }
}
impl WsPingHost for Host {
    type Value = Native;
    type Error = Native;
    fn undefined(&self) -> Native {
        Native::Undefined
    }
    fn literal(&self, text: &JsString) -> Native {
        Native::Text(text.clone())
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
    fn global(&mut self, name: &str) -> Result<Native, Native> {
        self.step("global", json!({"name":name}))
    }
    fn get(&mut self, o: &Native, name: &str) -> Result<Native, Native> {
        self.step("get", json!({"object":o.native(),"name":name}))
    }
    fn set(&mut self, o: &Native, name: &str, value: &Native) -> Result<(), Native> {
        self.step(
            "set",
            json!({"object":o.native(),"name":name,"value":value.native()}),
        )?;
        assert!(
            self.fields.contains_key(name),
            "independent tracked lifecycle field {name}"
        );
        self.fields.insert(name.into(), value.clone());
        Ok(())
    }
    fn call(
        &mut self,
        function: &Native,
        receiver: &Native,
        args: &[Native],
    ) -> Result<Native, Native> {
        let frame=self.frame("call",json!({"function":function.native(),"receiver":receiver.native(),"args":args.iter().map(Native::native).collect::<Vec<_>>()}));
        let expected = Self::decode_result(&frame);
        let owned = if let Native::Handle(id, _) = function {
            self.callbacks.get(id).cloned()
        } else {
            None
        };
        if let Some(capture) = owned {
            let actual = match capture.kind {
                LifecycleCallbackKind::OrderRefreshTimeout => {
                    run_order_refresh_timeout(self, &capture.exchange)
                }
                LifecycleCallbackKind::OrderRefreshError => {
                    run_order_refresh_error(self, &capture.exchange, &args[0])
                }
            }
            .map(|()| Native::Undefined);
            assert_eq!(
                Self::result(&actual),
                Self::result(&expected),
                "LIFECYCLE native owned callback result {}",
                self.case
            );
            return actual;
        }
        if function.tag() == "endRetryChain" && !self.end_call_throws {
            let actual = end_retry_chain(self, receiver).map(|()| Native::Undefined);
            assert_eq!(
                Self::result(&actual),
                Self::result(&expected),
                "LIFECYCLE native retry delegate result {}",
                self.case
            );
            return actual;
        }
        if function.tag() == "catch" && !self.catch_call_throws {
            if let Some(error) = self.catch_error.clone() {
                let capture = self.callbacks[&args[0].id()].clone();
                assert_eq!(capture.kind, LifecycleCallbackKind::OrderRefreshError);
                let actual = run_order_refresh_error(self, &capture.exchange, &error);
                if let Err(e) = actual {
                    assert_eq!(
                        Self::result(&Err(e.clone())),
                        Self::result(&expected),
                        "LIFECYCLE native rejection callback throw {}",
                        self.case
                    );
                    return Err(e);
                }
            }
        }
        if expected.is_ok() {
            match function.tag() {
                "base.dispose" => self.effects[0] += 1,
                "public.dispose" => self.effects[1] += 1,
                "private.dispose" => self.effects[2] += 1,
                "stopPingLoop" => self.effects[3] += 1,
                "close" => self.effects[4] += 1,
                "tif.clear" => self.collections[0].clear(),
                "dexes.clear" => self.collections[1].clear(),
                "ids.clear" => self.collections[2].clear(),
                "setTimeout" => {
                    assert_eq!(args.len(), 2, "independent timer arity");
                    assert_eq!(
                        self.callbacks[&args[0].id()].kind,
                        LifecycleCallbackKind::OrderRefreshTimeout
                    );
                    self.registered.push(args[0].clone());
                }
                _ => {}
            }
        }
        expected
    }
    fn object(&mut self) -> Result<Native, Native> {
        unreachable!("no object literal in lifecycle leaves")
    }
    fn define_data(&mut self, _: &Native, _: &str, _: &Native) -> Result<(), Native> {
        unreachable!()
    }
    fn callback(&mut self, _: PingCallback<Native>) -> Result<Native, Native> {
        unreachable!("lifecycle capture required")
    }
}
impl LifecycleHost for Host {
    fn null(&self) -> Native {
        Native::Null
    }
    fn boolean(&self, v: bool) -> Native {
        Native::Bool(v)
    }
    fn number(&self, v: f64) -> Native {
        Native::Number(v)
    }
    fn is_nullish(&self, v: &Native) -> bool {
        matches!(v, Native::Undefined | Native::Null)
    }
    fn super_get(&mut self, receiver: &Native, name: &str) -> Result<Native, Native> {
        self.step(
            "superGet",
            json!({"receiver":receiver.native(),"name":name}),
        )
    }
    fn lifecycle_callback(&mut self, capture: LifecycleCallback<Native>) -> Result<Native, Native> {
        let kind = match capture.kind {
            LifecycleCallbackKind::OrderRefreshTimeout => "timeout",
            LifecycleCallbackKind::OrderRefreshError => "error",
        };
        let value = self.step(
            "callback",
            json!({"exchange":capture.exchange.native(),"kind":kind}),
        )?;
        self.callbacks.insert(value.id(), capture);
        Ok(value)
    }
    fn subtract(&mut self, left: &Native, right: &Native) -> Result<Native, Native> {
        let value = self.step(
            "subtract",
            json!({"left":left.native(),"right":right.native()}),
        )?;
        if let (Native::Number(l), Native::Number(r), Native::Number(v)) = (left, right, &value) {
            assert!(
                (l - r).is_nan() && v.is_nan() || (l - r).to_bits() == v.to_bits(),
                "LIFECYCLE native subtract bits {}",
                self.case
            );
        }
        Ok(value)
    }
    fn greater_than(&mut self, left: &Native, right: &Native) -> Result<bool, Native> {
        let result = self.step("gt", json!({"left":left.native(),"right":right.native()}))?;
        let Native::Bool(value) = result else {
            panic!("native gt boolean")
        };
        if let (Native::Number(l), Native::Number(r)) = (left, right) {
            assert_eq!(*l > *r, value, "LIFECYCLE native comparison {}", self.case);
        }
        Ok(value)
    }
    fn template_string(&mut self, value: &Native) -> Result<JsString, Native> {
        let result = self.step("template", json!({"value":value.native()}))?;
        let Native::Text(text) = result else {
            panic!("template native string")
        };
        Ok(text)
    }
}
#[test]
fn native_ten_lifecycle_leaves_match_actual_source() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = std::process::Command::new("node")
        .arg(root.join("scripts/lifecycle-delegates-oracle.cjs"))
        .output()
        .expect("Node oracle");
    assert!(
        output.status.success(),
        "source oracle failed {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    let names = [
        "core.listenOHLCV",
        "core.listenOrderBook",
        "spot.listenOHLCV",
        "spot.listenOrderBook",
        "core.dispose",
        "spot.dispose",
        "public.dispose",
        "private.dispose",
        "core.endRetryChain",
        "core.scheduleOrderRefresh",
    ];
    assert_eq!(
        oracle["names"],
        json!(names),
        "independent ten source roots"
    );
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 111, "independent111cases");
    assert_eq!(oracle["iterationStop"], 110);
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
            assert_eq!(
                case["initial"]["collections"],
                json!([["91", "92"], ["dex:1", "dex:2"], ["91", "92"]]),
                "independent three collections/two members"
            );
            assert_eq!(
                case["initial"]["effects"],
                json!([0, 0, 0, 0, 0]),
                "independent five initial effects"
            );
            assert_eq!(
                case["initial"]["fields"].as_array().unwrap().len(),
                7,
                "independent seven tracked fields"
            );
            let fields = case["initial"]["fields"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| (v[0].as_str().unwrap().into(), Native::decode(&v[1])))
                .collect();
            let mut host = Host {
                script: case["calls"].as_array().unwrap().iter().cloned().collect(),
                case: format!("{name} {}", case["label"]),
                fields,
                collections: vec![
                    vec!["91".into(), "92".into()],
                    vec!["dex:1".into(), "dex:2".into()],
                    vec!["91".into(), "92".into()],
                ],
                effects: [0; 5],
                callbacks: HashMap::new(),
                registered: Vec::new(),
                catch_error: case["catchError"].get("value").map(Native::decode),
                catch_call_throws: case["catchCallThrows"] == true,
                end_call_throws: case["endRetryCallThrows"] == true,
            };
            let result = match name.as_str() {
                "core.listenOHLCV" => core_listen_ohlcv(&mut host, &input, &args[0], &args[1]),
                "core.listenOrderBook" => {
                    core_listen_order_book(&mut host, &input, &args[0], &args[1])
                }
                "spot.listenOHLCV" => spot_listen_ohlcv(&mut host, &input, &args[0], &args[1]),
                "spot.listenOrderBook" => {
                    spot_listen_order_book(&mut host, &input, &args[0], &args[1])
                }
                "core.dispose" => core_dispose(&mut host, &input).map(|()| Native::Undefined),
                "spot.dispose" => spot_dispose(&mut host, &input).map(|()| Native::Undefined),
                "public.dispose" => public_dispose(&mut host, &input).map(|()| Native::Undefined),
                "private.dispose" => private_dispose(&mut host, &input).map(|()| Native::Undefined),
                "core.endRetryChain" => {
                    end_retry_chain(&mut host, &input).map(|()| Native::Undefined)
                }
                "core.scheduleOrderRefresh" => {
                    schedule_order_refresh(&mut host, &input).map(|()| Native::Undefined)
                }
                _ => unreachable!(),
            };
            assert_eq!(
                Host::result(&result),
                case["result"],
                "LIFECYCLE native root result/alias {name}"
            );
            let registered = host.registered.clone();
            let mut callback_results = Vec::new();
            for callback in registered {
                let result = host.call(&callback, &Native::Undefined, &[]);
                callback_results.push(Host::result(&result));
            }
            assert_eq!(
                json!(callback_results),
                case["callbacks"],
                "LIFECYCLE native callback results {name}"
            );
            assert_eq!(
                host.snapshot(),
                case["final"],
                "LIFECYCLE native final fields/collections/effects {name}"
            );
            assert!(
                host.script.is_empty(),
                "LIFECYCLE native unconsumed effects {name}"
            );
        }));
        if verdict.is_err() && !failures.contains(&name) {
            failures.push(name);
        }
    }
    assert_eq!(
        counts,
        [6, 6, 6, 6, 21, 7, 4, 15, 5, 35],
        "independent rootcase counts"
    );
    assert!(
        failures.is_empty(),
        "LIFECYCLE named native failures:{failures:?}"
    );
}
