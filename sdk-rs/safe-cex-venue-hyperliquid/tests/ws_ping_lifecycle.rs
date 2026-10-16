use safe_cex_venue_hyperliquid::{spot_json::JsString, ws_ping_lifecycle::*};
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
struct Timer {
    id: Native,
    delay: Native,
    callback: Native,
}
struct Host {
    script: VecDeque<Value>,
    case: String,
    socket: Native,
    state: HashMap<String, Native>,
    active: Vec<String>,
    timers: Vec<Timer>,
    objects: HashMap<u64, HashMap<String, Native>>,
    capture: Option<PingCallback<Native>>,
}
impl Host {
    fn step(&mut self, op: &str, args: Value) -> Result<Native, Native> {
        let next = self.script.pop_front().expect("source operation exhausted");
        assert_eq!(next["op"], op, "PING native operation {}", self.case);
        assert_eq!(
            next["args"], args,
            "PING native argument {} {op}",
            self.case
        );
        if let Some(error) = next.get("throw") {
            Err(Native::decode(error))
        } else {
            Ok(Native::decode(&next["result"]))
        }
    }
}
impl WsPingHost for Host {
    type Value = Native;
    type Error = Native;
    fn undefined(&self) -> Native {
        Native::Undefined
    }
    fn literal(&self, v: &JsString) -> Native {
        Native::Text(v.clone())
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
            self.socket.id(),
            "PING native assignment receiver {}",
            self.case
        );
        self.state.insert(name.into(), value.clone());
        Ok(())
    }
    fn object(&mut self) -> Result<Native, Native> {
        let v = self.step("object", json!({}))?;
        self.objects.insert(v.id(), HashMap::new());
        Ok(v)
    }
    fn define_data(&mut self, object: &Native, name: &str, value: &Native) -> Result<(), Native> {
        self.step(
            "define",
            json!({"object":object.native(),"name":name,"value":value.native()}),
        )?;
        self.objects
            .get_mut(&object.id())
            .unwrap()
            .insert(name.into(), value.clone());
        Ok(())
    }
    fn callback(&mut self, capture: PingCallback<Native>) -> Result<Native, Native> {
        let kind = match capture.kind {
            SocketKind::Public => "public",
            SocketKind::Private => "private",
        };
        let callback = self.step(
            "callback",
            json!({"socket":capture.socket.native(),"kind":kind}),
        )?;
        self.capture = Some(capture);
        Ok(callback)
    }
    fn call(
        &mut self,
        function: &Native,
        receiver: &Native,
        args: &[Native],
    ) -> Result<Native, Native> {
        let result=self.step("call",json!({"function":function.native(),"receiver":receiver.native(),"args":args.iter().map(Native::native).collect::<Vec<_>>()}))?;
        match function.tag() {
            "setTimeout" => self.timers.push(Timer {
                id: result.clone(),
                delay: args[1].clone(),
                callback: args[0].clone(),
            }),
            "clearTimeout" => self.timers.retain(|t| t.id.native() != args[0].native()),
            "clear" => {
                assert_eq!(
                    receiver.tag(),
                    "activeSet",
                    "PING native Set receiver {}",
                    self.case
                );
                self.active.clear();
            }
            "stringify" => {
                let object = &self.objects[&args[0].id()];
                let Native::Text(method) = &object["method"] else {
                    panic!("PING native payload method {}", self.case)
                };
                assert_eq!(
                    method.0,
                    "ping".encode_utf16().collect::<Vec<_>>(),
                    "PING native payload method {}",
                    self.case
                );
                assert_eq!(
                    object.len(),
                    1,
                    "PING native payload independent one member {}",
                    self.case
                );
            }
            _ => {}
        }
        Ok(result)
    }
}
fn outcome(result: Result<(), Native>) -> Value {
    match result {
        Ok(()) => json!({"value":["undefined"]}),
        Err(e) => json!({"throw":e.native()}),
    }
}
#[test]
fn native_ten_websocket_ping_methods_match_actual_source() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = std::process::Command::new("node")
        .arg(root.join("scripts/ws-ping-oracle.cjs"))
        .output()
        .expect("Node source oracle");
    assert!(
        output.status.success(),
        "source oracle failed {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    let names = [
        "public.onOpen",
        "public.startPingLoop",
        "public.schedulePing",
        "public.stopPingLoop",
        "public.ping",
        "private.onOpen",
        "private.startPingLoop",
        "private.schedulePing",
        "private.stopPingLoop",
        "private.ping",
    ];
    assert_eq!(oracle["names"], json!(names), "independent ten roots");
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 123, "independent 123cases");
    assert_eq!(oracle["iterationStop"], 122);
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
            let mut host = Host {
                script: case["calls"].as_array().unwrap().iter().cloned().collect(),
                case: format!("{name} {}", case["label"]),
                socket: socket.clone(),
                state: case["initialState"]
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(k, v)| (k.clone(), Native::decode(v)))
                    .collect(),
                active: vec!["first".into(), "second".into()],
                timers: Vec::new(),
                objects: HashMap::new(),
                capture: None,
            };
            let result = match name.as_str() {
                "public.onOpen" => public_on_open(&mut host, &socket),
                "private.onOpen" => private_on_open(&mut host, &socket),
                "public.startPingLoop" => public_start_ping_loop(&mut host, &socket),
                "private.startPingLoop" => private_start_ping_loop(&mut host, &socket),
                "public.schedulePing" => public_schedule_ping(&mut host, &socket),
                "private.schedulePing" => private_schedule_ping(&mut host, &socket),
                "public.stopPingLoop" => public_stop_ping_loop(&mut host, &socket),
                "private.stopPingLoop" => private_stop_ping_loop(&mut host, &socket),
                "public.ping" => public_ping(&mut host, &socket),
                "private.ping" => private_ping(&mut host, &socket),
                _ => unreachable!(),
            };
            assert_eq!(outcome(result), case["result"], "PING native result {name}");
            assert_eq!(
                host.script.len(),
                case["calls"].as_array().unwrap().len()
                    - case["mainStop"].as_u64().unwrap() as usize,
                "PING native source phase boundary {name}"
            );
            if case["fire"] == true {
                let capture = host.capture.clone().expect("native timer capture");
                assert_eq!(
                    capture.socket.id(),
                    socket.id(),
                    "PING native lexical receiver {name}"
                );
                let result = run_ping_timeout(&mut host, &capture);
                assert_eq!(
                    outcome(result),
                    case["callbackResult"],
                    "PING native callback result {name}"
                );
            }
            assert!(
                host.script.is_empty(),
                "PING native unconsumed effects {name}"
            );
            for (key, value) in &host.state {
                assert_eq!(
                    value.native(),
                    case["finalState"][key],
                    "PING native final field {name} {key}"
                );
            }
            assert_eq!(
                host.active,
                case["finalActive"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().to_string())
                    .collect::<Vec<_>>(),
                "PING native active Set {name}"
            );
            let timers=host.timers.iter().map(|t|json!({"id":t.id.native(),"delay":t.delay.native(),"callback":t.callback.native()})).collect::<Vec<_>>();
            assert_eq!(
                json!(timers),
                case["finalTimers"],
                "PING native registered timer fields {name}"
            );
        }));
        if verdict.is_err() && !failures.contains(&name) {
            failures.push(name);
        }
    }
    assert_eq!(
        counts,
        [12, 4, 16, 16, 15, 9, 4, 16, 16, 15],
        "independent family counts"
    );
    assert!(
        failures.is_empty(),
        "PING named native failures:{failures:?}"
    );
}
