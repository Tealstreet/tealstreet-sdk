use safe_cex_venue_hyperliquid::{
    spot_json::JsString, ws_ping_lifecycle::*, ws_transport_leaves::*,
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
    socket: Native,
    state: HashMap<String, Native>,
    websockets: Vec<(Native, Vec<(JsString, Native)>)>,
    objects: HashMap<u64, HashMap<String, Native>>,
}
impl Host {
    fn step(&mut self, op: &str, args: Value) -> Result<Native, Native> {
        let next = self.script.pop_front().expect("source operation exhausted");
        assert_eq!(next["op"], op, "TRANSPORT native operation {}", self.case);
        assert_eq!(
            next["args"], args,
            "TRANSPORT native argument {} {op}",
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
            self.socket.id(),
            "TRANSPORT native assignment receiver {}",
            self.case
        );
        self.state.insert(name.into(), value.clone());
        Ok(())
    }
    fn object(&mut self) -> Result<Native, Native> {
        let value = self.step("object", json!({}))?;
        self.objects.insert(value.id(), HashMap::new());
        Ok(value)
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
    fn callback(&mut self, _: PingCallback<Native>) -> Result<Native, Native> {
        panic!("transport leaves allocate no timer callbacks")
    }
    fn call(
        &mut self,
        function: &Native,
        receiver: &Native,
        args: &[Native],
    ) -> Result<Native, Native> {
        // Read native literal members before the stringify result can reach a
        // sender. This independently detects a symmetric payload-field error.
        if function.tag() == "stringify" {
            let object = &self.objects[&args[0].id()];
            let Native::Text(method) = &object["method"] else {
                panic!("TRANSPORT native payload type {}", self.case)
            };
            assert_eq!(
                method.0,
                "ping".encode_utf16().collect::<Vec<_>>(),
                "TRANSPORT native payload method {}",
                self.case
            );
            assert_eq!(
                object.len(),
                1,
                "TRANSPORT native independent one payload member {}",
                self.case
            );
        }
        let result=self.step("call",json!({"function":function.native(),"receiver":receiver.native(),"args":args.iter().map(Native::native).collect::<Vec<_>>()}))?;
        if function.tag() == "addEventListener" {
            let Native::Text(event) = &args[0] else {
                panic!("TRANSPORT native listener event type {}", self.case)
            };
            let (_, listeners) = self
                .websockets
                .iter_mut()
                .find(|(ws, _)| ws.id() == receiver.id())
                .unwrap();
            listeners.push((event.clone(), args[1].clone()));
        }
        Ok(result)
    }
}
impl WsTransportHost for Host {
    fn urls_binding(&mut self, venue: WsVenue) -> Result<Native, Native> {
        let venue = match venue {
            WsVenue::Perp => "perp",
            WsVenue::Spot => "spot",
        };
        self.step("global", json!({"name":"WSS_URLS","venue":venue}))
    }
    fn boolean(&self, value: bool) -> Native {
        Native::Bool(value)
    }
}
fn outcome(result: Result<(), Native>) -> Value {
    match result {
        Ok(()) => json!({"value":["undefined"]}),
        Err(error) => json!({"throw":error.native()}),
    }
}
#[test]
fn native_ten_websocket_transport_leaves_match_actual_source() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = std::process::Command::new("node")
        .arg(root.join("scripts/ws-transport-oracle.cjs"))
        .output()
        .expect("Node source oracle");
    assert!(
        output.status.success(),
        "source oracle failed {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    let names = [
        "public.connectAndSubscribe",
        "private.connectAndSubscribe",
        "private.subscribe",
        "spot_public.connectAndSubscribe",
        "spot_public.onOpen",
        "spot_public.ping",
        "spot_private.connectAndSubscribe",
        "spot_private.onOpen",
        "spot_private.ping",
        "spot_private.subscribe",
    ];
    assert_eq!(oracle["names"], json!(names), "independent ten roots");
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 202, "independent 202cases");
    assert_eq!(oracle["iterationStop"], 201);
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
                websockets: case["initialWebsockets"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|ws| (Native::decode(ws), Vec::new()))
                    .collect(),
                objects: HashMap::new(),
            };
            assert_eq!(
                host.websockets.len(),
                3,
                "independent three source fixture sockets"
            );
            assert_eq!(
                host.websockets
                    .iter()
                    .map(|(ws, _)| ws.tag())
                    .collect::<Vec<_>>(),
                ["oldWs", "createdWs", "alternateWs"],
                "independent source fixture socket identities"
            );
            let result = match name.as_str() {
                "public.connectAndSubscribe" => public_connect_and_subscribe(&mut host, &socket),
                "private.connectAndSubscribe" => private_connect_and_subscribe(&mut host, &socket),
                "spot_public.connectAndSubscribe" => {
                    spot_public_connect_and_subscribe(&mut host, &socket)
                }
                "spot_private.connectAndSubscribe" => {
                    spot_private_connect_and_subscribe(&mut host, &socket)
                }
                "spot_public.onOpen" => spot_public_on_open(&mut host, &socket),
                "spot_private.onOpen" => spot_private_on_open(&mut host, &socket),
                "spot_public.ping" => spot_public_ping(&mut host, &socket),
                "spot_private.ping" => spot_private_ping(&mut host, &socket),
                "private.subscribe" => private_subscribe(&mut host, &socket),
                "spot_private.subscribe" => spot_private_subscribe(&mut host, &socket),
                _ => unreachable!(),
            };
            assert_eq!(
                outcome(result),
                case["result"],
                "TRANSPORT native result {name}"
            );
            assert!(
                host.script.is_empty(),
                "TRANSPORT native unconsumed effects {name}"
            );
            for (key, value) in &host.state {
                assert_eq!(
                    value.native(),
                    case["finalState"][key],
                    "TRANSPORT native final field {name} {key}"
                );
            }
            for (index, (ws, listeners)) in host.websockets.iter().enumerate() {
                let expected = &case["finalListeners"][index];
                assert_eq!(
                    ws.native(),
                    expected["ws"],
                    "TRANSPORT native final socket identity {name}"
                );
                let actual = listeners
                    .iter()
                    .map(|(event, handler)| json!([event.0, handler.native()]))
                    .collect::<Vec<_>>();
                assert_eq!(
                    json!(actual),
                    expected["listeners"],
                    "TRANSPORT native final event/handler/receiver {name}"
                );
            }
        }));
        if verdict.is_err() && !failures.contains(&name) {
            failures.push(name);
        }
    }
    assert_eq!(
        counts,
        [36, 36, 5, 36, 9, 15, 36, 9, 15, 5],
        "independent family counts"
    );
    assert!(
        failures.is_empty(),
        "TRANSPORT named native failures:{failures:?}"
    );
}
