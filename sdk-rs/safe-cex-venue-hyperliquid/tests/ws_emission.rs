use safe_cex_venue_hyperliquid::ws_transport_leaves::WsVenue;
use safe_cex_venue_hyperliquid::{spot_json::JsString, ws_emission::*, ws_ping_lifecycle::*};
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
    Done,
}
impl Native {
    fn decode(v: &Value) -> Self {
        match v[0].as_str().unwrap() {
            "done" => Self::Done,
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
            Self::Done => json!(["done"]),
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
    objects: HashMap<u64, Vec<(String, Native)>>,
    formats: HashMap<u64, DiagnosticScope>,
    registered: Vec<Native>,
    sent: Vec<Native>,
}
impl Host {
    fn raw(&mut self, op: &str, args: Value) -> Result<Value, Native> {
        let next = self.script.pop_front().expect("source operation exhausted");
        assert_eq!(next["op"], op, "EMISSION native operation {}", self.case);
        assert_eq!(
            next["args"], args,
            "EMISSION native argument {} {op}",
            self.case
        );
        if let Some(error) = next.get("throw") {
            Err(Native::decode(error))
        } else {
            Ok(next["result"].clone())
        }
    }
    fn step(&mut self, op: &str, args: Value) -> Result<Native, Native> {
        self.raw(op, args).map(|v| Native::decode(&v))
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
            Native::Done => panic!("iterator done used as value"),
        }
    }
    fn global(&mut self, name: &str) -> Result<Native, Native> {
        self.step("global", json!({"name":name}))
    }
    fn get(&mut self, object: &Native, name: &str) -> Result<Native, Native> {
        self.step("get", json!({"object":object.native(),"name":name}))
    }
    fn set(&mut self, _: &Native, _: &str, _: &Native) -> Result<(), Native> {
        panic!("emission uses literal definitions, not assignments")
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
        panic!("emission allocates formatter callbacks, not timers")
    }
    fn call(
        &mut self,
        function: &Native,
        receiver: &Native,
        args: &[Native],
    ) -> Result<Native, Native> {
        let mut actual = json!({"function":function.native(),"receiver":receiver.native(),"args":args.iter().map(Native::native).collect::<Vec<_>>()});
        if function.tag() == "stringify" || function.tag().starts_with("emitWsDiagnostic") {
            actual["payload"] = self.payload(args.last().unwrap());
        }
        let value = self.step("call", actual)?;
        if matches!(function.tag(), "send" | "sendWithDiagnostics") {
            self.sent.push(args[0].clone());
        }
        if function.tag().starts_with("emitWsDiagnostic") {
            let options = &self.objects[&args[1].id()];
            assert_eq!(
                options.len(),
                1,
                "EMISSION native one format option {}",
                self.case
            );
            assert_eq!(
                options[0].0, "format",
                "EMISSION native format option name {}",
                self.case
            );
            self.registered.push(options[0].1.clone());
        }
        Ok(value)
    }
}
impl WsEmissionHost for Host {
    fn boolean(&self, value: bool) -> Native {
        Native::Bool(value)
    }
    fn system_types(&mut self, venue: WsVenue) -> Result<Native, Native> {
        self.step("global",json!({"name":"systemSubscriptionTypes","venue":match venue{WsVenue::Perp=>"perp",WsVenue::Spot=>"spot"}}))
    }
    fn iterator(&mut self, value: &Native) -> Result<Native, Native> {
        self.step("iterator", json!({"value":value.native()}))
    }
    fn next(&mut self, iterator: &Native) -> Result<Option<Native>, Native> {
        self.step("next", json!({"iterator":iterator.native()}))
            .map(|v| {
                if matches!(v, Native::Done) {
                    None
                } else {
                    Some(v)
                }
            })
    }
    fn close_after_throw(&mut self, iterator: &Native) {
        let _ = self.step("close", json!({"iterator":iterator.native()}));
    }
    fn triple(&mut self, value: &Native) -> Result<[Native; 3], Native> {
        let result = self.raw("triple", json!({"value":value.native()}))?;
        assert_eq!(
            result.as_array().unwrap().len(),
            3,
            "independent three destructured values"
        );
        Ok([
            Native::decode(&result[0]),
            Native::decode(&result[1]),
            Native::decode(&result[2]),
        ])
    }
    fn string_units<'a>(&self, value: &'a Native) -> Option<&'a [u16]> {
        if let Native::Text(s) = value {
            Some(&s.0)
        } else {
            None
        }
    }
    fn spread_into(&mut self, target: &Native, source: &Native) -> Result<(), Native> {
        self.step("spread",json!({"target":target.native(),"source":source.native(),"payload":self.payload(source)}))?;
        let fields = self.objects[&source.id()].clone();
        self.objects.get_mut(&target.id()).unwrap().extend(fields);
        Ok(())
    }
    fn formatter(&mut self, scope: DiagnosticScope) -> Result<Native, Native> {
        let value=self.step("formatter",json!({"scope":match scope{DiagnosticScope::Public=>"public",DiagnosticScope::Private=>"private"}}))?;
        self.formats.insert(value.id(), scope);
        Ok(value)
    }
}
fn outcome(result: Result<(), Native>) -> Value {
    match result {
        Ok(()) => json!({"value":["undefined"]}),
        Err(e) => json!({"throw":e.native()}),
    }
}
#[test]
fn native_ten_websocket_emission_methods_match_actual_source() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = std::process::Command::new("node")
        .arg(root.join("scripts/ws-emission-oracle.cjs"))
        .output()
        .expect("Node source oracle");
    assert!(
        output.status.success(),
        "source oracle failed {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    let names = [
        "public.sendWithDiagnostics",
        "public.emitReceiveDiagnostics",
        "private.sendWithDiagnostics",
        "private.emitReceiveDiagnostics",
        "private.subscribeSystemTopics",
        "private.subscribeAllActiveAssetData",
        "spot_public.subscribePublicTopics",
        "spot_public.subscribeUserTopics",
        "spot_private.subscribeSystemTopics",
        "spot_private.subscribeCoins",
    ];
    assert_eq!(oracle["names"], json!(names), "independent ten roots");
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 118, "independent 118cases");
    assert_eq!(oracle["iterationStop"], 117);
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
            let socket = Native::decode(&case["input"]);
            let args = case["args"]
                .as_array()
                .unwrap()
                .iter()
                .map(Native::decode)
                .collect::<Vec<_>>();
            let mut host = Host {
                script: case["calls"].as_array().unwrap().iter().cloned().collect(),
                case: format!("{name} {}", case["label"]),
                objects: HashMap::new(),
                formats: HashMap::new(),
                registered: Vec::new(),
                sent: Vec::new(),
            };
            let result = match name.as_str() {
                "public.sendWithDiagnostics" => {
                    public_send_with_diagnostics(&mut host, &socket, &args[0])
                }
                "private.sendWithDiagnostics" => {
                    private_send_with_diagnostics(&mut host, &socket, &args[0])
                }
                "public.emitReceiveDiagnostics" => {
                    public_emit_receive_diagnostics(&mut host, &socket, &args[0], &args[1])
                }
                "private.emitReceiveDiagnostics" => {
                    private_emit_receive_diagnostics(&mut host, &socket, &args[0], &args[1])
                }
                "private.subscribeSystemTopics" => {
                    private_subscribe_system_topics(&mut host, &socket)
                }
                "private.subscribeAllActiveAssetData" => {
                    private_subscribe_all_active_asset_data(&mut host, &socket)
                }
                "spot_public.subscribePublicTopics" => {
                    spot_public_subscribe_public_topics(&mut host, &socket)
                }
                "spot_public.subscribeUserTopics" => {
                    spot_public_subscribe_user_topics(&mut host, &socket)
                }
                "spot_private.subscribeSystemTopics" => {
                    spot_private_subscribe_system_topics(&mut host, &socket)
                }
                "spot_private.subscribeCoins" => spot_private_subscribe_coins(&mut host, &socket),
                _ => unreachable!(),
            };
            assert_eq!(
                outcome(result),
                case["result"],
                "EMISSION native result {name}"
            );
            assert_eq!(
                host.script.len(),
                case["calls"].as_array().unwrap().len()
                    - case["mainStop"].as_u64().unwrap() as usize,
                "EMISSION native source phase {name}"
            );
            assert_eq!(
                host.registered.len(),
                case["formatCount"].as_u64().unwrap() as usize,
                "EMISSION native registered format closures {name}"
            );
            if case["fireFormat"] == true {
                let scope = host.formats[&host.registered[0].id()];
                let message = Native::decode(&case["formatMessage"]);
                let result = run_diagnostic_formatter(&mut host, scope, &message);
                let actual = match result {
                    Ok(v) => json!({"value":v.native()}),
                    Err(e) => json!({"throw":e.native()}),
                };
                assert_eq!(
                    actual, case["formatted"][0],
                    "EMISSION native formatter result {name}"
                );
            }
            assert!(
                host.script.is_empty(),
                "EMISSION native unconsumed effects {name}"
            );
            assert_eq!(
                host.sent.iter().map(Native::native).collect::<Vec<_>>(),
                *case["sent"].as_array().unwrap(),
                "EMISSION native sender arguments {name}"
            );
        }));
        if verdict.is_err() && !failures.contains(&name) {
            failures.push(name);
        }
    }
    assert_eq!(
        counts,
        [14, 14, 14, 14, 11, 12, 5, 13, 10, 11],
        "independent family counts"
    );
    assert!(
        failures.is_empty(),
        "EMISSION named native failures:{failures:?}"
    );
}
