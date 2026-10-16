use safe_cex_venue_hyperliquid::{
    private_message_dispatch::*, spot_json::JsString, ws_ping_lifecycle::*,
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
    Handle(u64, String, String),
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
                    .map(|x| x.as_u64().unwrap() as u16)
                    .collect(),
            )),
            "ref" => Self::Handle(
                v[1].as_u64().unwrap(),
                v[2].as_str().unwrap().into(),
                v[3].as_str().unwrap().into(),
            ),
            _ => panic!("badnative {v}"),
        }
    }
    fn wire(&self) -> Value {
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
            Self::Handle(id, tag, kind) => json!(["ref", id, tag, kind]),
        }
    }
    fn id(&self) -> u64 {
        if let Self::Handle(id, ..) = self {
            *id
        } else {
            panic!("nativehandle")
        }
    }
    fn tag(&self) -> &str {
        if let Self::Handle(_, tag, _) = self {
            tag
        } else {
            ""
        }
    }
    fn equal(&self, b: &Self) -> bool {
        match (self, b) {
            (Self::Undefined, Self::Undefined) | (Self::Null, Self::Null) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Number(a), Self::Number(b)) => a == b,
            (Self::Text(a), Self::Text(b)) => a.0 == b.0,
            (Self::Handle(a, ..), Self::Handle(b, ..)) => a == b,
            _ => false,
        }
    }
}
fn assert_native(actual: &Native, expected: &Native, label: &str) {
    match (actual, expected) {
        (Native::Undefined, Native::Undefined) | (Native::Null, Native::Null) => {}
        (Native::Bool(a), Native::Bool(b)) => assert_eq!(a, b, "DISPATCH native bool {label}"),
        (Native::Number(a), Native::Number(b)) => assert!(
            a.is_nan() && b.is_nan() || a.to_bits() == b.to_bits(),
            "DISPATCH native number {label}: {a:?} vs {b:?}"
        ),
        (Native::Text(a), Native::Text(b)) => assert_eq!(a.0, b.0, "DISPATCH native UTF16 {label}"),
        (Native::Handle(a, at, ak), Native::Handle(b, bt, bk)) => {
            assert_eq!(a, b, "DISPATCH native identity {label}");
            assert_eq!(at, bt, "DISPATCH native tag {label}");
            assert_eq!(ak, bk, "DISPATCH native typeof {label}");
        }
        _ => panic!("DISPATCH native type {label}: {actual:?} vs {expected:?}"),
    }
}
#[derive(Clone, Debug)]
enum Tree {
    Atom(Native),
    Literal(Native, Native, Vec<(JsString, Tree)>),
    Buffer(Native, Vec<(JsString, Tree)>),
}
impl Tree {
    fn decode(v: &Value) -> Self {
        match v[0].as_str().unwrap() {
            "literal" => Self::Literal(
                Native::decode(&v[1]),
                Native::decode(&v[2]),
                v[3].as_array()
                    .unwrap()
                    .iter()
                    .map(|p| {
                        (
                            JsString(
                                p[0].as_array()
                                    .unwrap()
                                    .iter()
                                    .map(|x| x.as_u64().unwrap() as u16)
                                    .collect(),
                            ),
                            Self::decode(&p[1]),
                        )
                    })
                    .collect(),
            ),
            "buffer" => Self::Buffer(
                Native::decode(&v[1]),
                v[2].as_array()
                    .unwrap()
                    .iter()
                    .map(|p| {
                        (
                            JsString(
                                p[0].as_array()
                                    .unwrap()
                                    .iter()
                                    .map(|x| x.as_u64().unwrap() as u16)
                                    .collect(),
                            ),
                            Self::decode(&p[1]),
                        )
                    })
                    .collect(),
            ),
            _ => Self::Atom(Native::decode(v)),
        }
    }
    fn assert(&self, expected: &Self, label: &str) {
        match (self, expected) {
            (Self::Atom(a), Self::Atom(b)) => assert_native(a, b, label),
            (Self::Literal(a, ap, af), Self::Literal(b, bp, bf)) => {
                assert_native(a, b, label);
                assert_native(ap, bp, label);
                fields(af, bf, label)
            }
            (Self::Buffer(a, af), Self::Buffer(b, bf)) => {
                assert_native(a, b, label);
                fields(af, bf, label)
            }
            _ => panic!("DISPATCH native tree {label}: {self:?} vs {expected:?}"),
        }
    }
}
fn fields(a: &[(JsString, Tree)], b: &[(JsString, Tree)], label: &str) {
    assert_eq!(a.len(), b.len(), "DISPATCH native field count {label}");
    for ((ak, av), (bk, bv)) in a.iter().zip(b) {
        assert_eq!(ak.0, bk.0, "DISPATCH native field key {label}");
        av.assert(bv, label);
    }
}
struct Host {
    script: VecDeque<Value>,
    case: String,
    objects: HashMap<u64, (Native, Vec<(JsString, Tree)>)>,
    buffers: HashMap<u64, Vec<(JsString, Tree)>>,
    maps: HashMap<u64, Vec<(Native, Native)>>,
    sets: HashMap<u64, Vec<Native>>,
    effects: Vec<(String, Native, Vec<Tree>)>,
    effect_names: Vec<String>,
    wire: Vec<(Value, Value)>,
}
impl Host {
    fn frame(&mut self, op: &str) -> Value {
        let v = self
            .script
            .pop_front()
            .unwrap_or_else(|| panic!("DISPATCH native operation exhausted {} {op}", self.case));
        assert_eq!(v["op"], op, "DISPATCH native operation {}", self.case);
        v
    }
    fn arg(&self, f: &Value, n: &str, v: &Native) {
        assert_native(v, &Native::decode(&f["args"][n]), &self.case);
    }
    fn finish(&mut self, f: Value, args: Value) -> Result<Native, Native> {
        self.wire.push((f["args"].clone(), args));
        if let Some(v) = f.get("throw") {
            Err(Native::decode(v))
        } else {
            Ok(Native::decode(&f["result"]))
        }
    }
    fn tree(&self, v: &Native) -> Tree {
        if let Native::Handle(id, ..) = v {
            if let Some((proto, fields)) = self.objects.get(id) {
                return Tree::Literal(v.clone(), proto.clone(), fields.clone());
            }
            if let Some(fields) = self.buffers.get(id) {
                return Tree::Buffer(v.clone(), fields.clone());
            }
        }
        Tree::Atom(v.clone())
    }
    fn put(fields: &mut Vec<(JsString, Tree)>, n: &str, v: Tree) {
        let key = JsString(n.encode_utf16().collect());
        if let Some(p) = fields.iter_mut().find(|p| p.0.0 == key.0) {
            p.1 = v;
        } else {
            fields.push((key, v));
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
    fn global(&mut self, n: &str) -> Result<Native, Native> {
        let f = self.frame("global");
        assert_eq!(f["args"]["name"], n, "DISPATCH native global {}", self.case);
        self.finish(f, json!({"name":n}))
    }
    fn get(&mut self, o: &Native, n: &str) -> Result<Native, Native> {
        let f = self.frame("get");
        self.arg(&f, "object", o);
        assert_eq!(f["args"]["name"], n, "DISPATCH native getter {}", self.case);
        self.finish(f, json!({"object":o.wire(),"name":n}))
    }
    fn set(&mut self, o: &Native, n: &str, v: &Native) -> Result<(), Native> {
        let f = self.frame("set");
        self.arg(&f, "object", o);
        self.arg(&f, "value", v);
        assert_eq!(f["args"]["name"], n, "DISPATCH native setter {}", self.case);
        self.finish(f, json!({"object":o.wire(),"name":n,"value":v.wire()}))?;
        let t = self.tree(v);
        Self::put(
            self.buffers
                .get_mut(&o.id())
                .expect("tracked native buffer"),
            n,
            t,
        );
        Ok(())
    }
    fn object(&mut self) -> Result<Native, Native> {
        let f = self.frame("object");
        let proto = Native::decode(&f["prototype"]);
        let v = self.finish(f, json!({}))?;
        self.objects.insert(v.id(), (proto, vec![]));
        Ok(v)
    }
    fn define_data(&mut self, o: &Native, n: &str, v: &Native) -> Result<(), Native> {
        let f = self.frame("define");
        self.arg(&f, "object", o);
        self.arg(&f, "value", v);
        assert_eq!(
            f["args"]["name"], n,
            "DISPATCH native literal key {}",
            self.case
        );
        self.finish(f, json!({"object":o.wire(),"name":n,"value":v.wire()}))?;
        let t = self.tree(v);
        Self::put(&mut self.objects.get_mut(&o.id()).unwrap().1, n, t);
        Ok(())
    }
    fn callback(&mut self, _: PingCallback<Native>) -> Result<Native, Native> {
        panic!("no pingcallback")
    }
    fn call(
        &mut self,
        function: &Native,
        receiver: &Native,
        args: &[Native],
    ) -> Result<Native, Native> {
        let f = self.frame("call");
        self.arg(&f, "function", function);
        self.arg(&f, "receiver", receiver);
        let expected = f["args"]["args"].as_array().unwrap();
        assert_eq!(
            args.len(),
            expected.len(),
            "DISPATCH native call arity {}",
            self.case
        );
        for (a, b) in args.iter().zip(expected) {
            assert_native(a, &Native::decode(b), &self.case);
        }
        if let Some(callbacks) = f.get("callbacks") {
            for cb in callbacks.as_array().unwrap() {
                let input = Native::decode(&cb["input"]);
                let actual = run_valid_fill_predicate(self, &input);
                match (&actual, cb.get("throw")) {
                    (Err(a), Some(b)) => assert_native(a, &Native::decode(b), &self.case),
                    (Ok(a), None) => assert_native(a, &Native::decode(&cb["result"]), &self.case),
                    _ => panic!("DISPATCH native predicate outcome {}", self.case),
                }
            }
        }
        let result=self.finish(f,json!({"function":function.wire(),"receiver":receiver.wire(),"args":args.iter().map(Native::wire).collect::<Vec<_>>()}))?;
        if function.tag() == "set" {
            let map = self.maps.get_mut(&receiver.id()).unwrap();
            if let Some(p) = map.iter_mut().find(|(k, _)| k.equal(&args[0])) {
                p.1 = args[1].clone();
            } else {
                map.push((args[0].clone(), args[1].clone()));
            }
        }
        if function.tag() == "get" {
            if let Some(map) = self.maps.get(&receiver.id()) {
                let actual = map
                    .iter()
                    .find(|(k, _)| k.equal(&args[0]))
                    .map(|(_, v)| v.clone())
                    .unwrap_or(Native::Undefined);
                assert_native(&actual, &result, &self.case);
            }
        }
        if function.tag() == "add" {
            let set = self.sets.get_mut(&receiver.id()).unwrap();
            if !set.iter().any(|v| v.equal(&args[0])) {
                set.push(args[0].clone());
            }
        }
        if self.effect_names.iter().any(|n| n == function.tag()) {
            self.effects.push((
                function.tag().into(),
                receiver.clone(),
                args.iter().map(|a| self.tree(a)).collect(),
            ));
        }
        Ok(result)
    }
}
impl PrivateMessageHost for Host {
    fn typeof_value<'a>(&self, v: &'a Native) -> &'a str {
        match v {
            Native::Undefined => "undefined",
            Native::Null => "object",
            Native::Bool(_) => "boolean",
            Native::Number(_) => "number",
            Native::Text(_) => "string",
            Native::Handle(_, _, kind) => kind,
        }
    }
    fn number(&self, v: f64) -> Native {
        Native::Number(v)
    }
    fn boolean(&self, v: bool) -> Native {
        Native::Bool(v)
    }
    fn is_nullish(&self, v: &Native) -> bool {
        matches!(v, Native::Null | Native::Undefined)
    }
    fn is_null(&self, v: &Native) -> bool {
        matches!(v, Native::Null)
    }
    fn strict_equal(&self, a: &Native, b: &Native) -> bool {
        a.equal(b)
    }
    fn index(&mut self, o: &Native, key: &Native) -> Result<Native, Native> {
        let f = self.frame("index");
        self.arg(&f, "object", o);
        self.arg(&f, "key", key);
        self.finish(f, json!({"object":o.wire(),"key":key.wire()}))
    }
    fn array(&mut self) -> Result<Native, Native> {
        let f = self.frame("array");
        self.finish(f, json!({}))
    }
    fn template_string(&mut self, v: &Native) -> Result<JsString, Native> {
        let f = self.frame("template");
        self.arg(&f, "value", v);
        let value = self.finish(f, json!({"value":v.wire()}))?;
        let Native::Text(t) = value else {
            panic!("native template text")
        };
        Ok(t)
    }
    fn operator(&mut self, op: DispatchOperator, a: &Native, b: &Native) -> Result<Native, Native> {
        let n = match op {
            DispatchOperator::Less => "less",
            DispatchOperator::LessEqual => "lessEqual",
            DispatchOperator::Greater => "greater",
            DispatchOperator::Subtract => "subtract",
            DispatchOperator::Divide => "divide",
        };
        let f = self.frame("operator");
        assert_eq!(
            f["args"]["operator"], n,
            "DISPATCH native operator {}",
            self.case
        );
        self.arg(&f, "left", a);
        self.arg(&f, "right", b);
        self.finish(f, json!({"operator":n,"left":a.wire(),"right":b.wire()}))
    }
    fn has_property(&mut self, n: &str, o: &Native) -> Result<bool, Native> {
        let f = self.frame("has");
        assert_eq!(
            f["args"]["name"], n,
            "DISPATCH native has key {}",
            self.case
        );
        self.arg(&f, "object", o);
        let value = self.finish(f, json!({"name":n,"object":o.wire()}))?;
        let Native::Bool(v) = value else {
            panic!("native has bool")
        };
        Ok(v)
    }
    fn valid_fill_predicate(&mut self) -> Result<Native, Native> {
        let f = self.frame("predicate");
        self.finish(f, json!({}))
    }
    fn for_in(&mut self, o: &Native) -> Result<Native, Native> {
        let f = self.frame("forIn");
        self.arg(&f, "object", o);
        self.finish(f, json!({"object":o.wire()}))
    }
    fn for_in_next(&mut self, i: &Native) -> Result<Option<Native>, Native> {
        let f = self.frame("forInNext");
        self.arg(&f, "iterator", i);
        if f["result"][0] == "done" {
            self.wire
                .push((f["args"].clone(), json!({"iterator":i.wire()})));
            Ok(None)
        } else {
            self.finish(f, json!({"iterator":i.wire()})).map(Some)
        }
    }
    fn construct(&mut self, function: &Native, args: &[Native]) -> Result<Native, Native> {
        let f = self.frame("construct");
        self.arg(&f, "function", function);
        let expected = f["args"]["args"].as_array().unwrap();
        assert_eq!(
            args.len(),
            expected.len(),
            "DISPATCH native construct arity {}",
            self.case
        );
        for (a, b) in args.iter().zip(expected) {
            assert_native(a, &Native::decode(b), &self.case);
        }
        let result=self.finish(f,json!({"function":function.wire(),"args":args.iter().map(Native::wire).collect::<Vec<_>>()}))?;
        self.sets.insert(result.id(), vec![]);
        Ok(result)
    }
    fn regex(&mut self, pattern: &str) -> Result<Native, Native> {
        let f = self.frame("regex");
        assert_eq!(
            f["args"]["pattern"], pattern,
            "DISPATCH native regex {}",
            self.case
        );
        self.finish(f, json!({"pattern":pattern}))
    }
}
#[test]
fn actual_source_private_message_native_values() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let output = std::process::Command::new("node")
        .arg(root.join("sdk-rs/safe-cex-venue-hyperliquid/scripts/private-message-oracle.cjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 140);
    assert_eq!(oracle["iterationStop"], 139);
    let expected = [
        ("core.isStaleMessage", 20),
        ("core.onMessage", 24),
        ("core.processMessage", 24),
        ("core.handleFundings", 3),
        ("core.handleUserNonFundingLedgerUpdates", 8),
        ("core.handleUserEvents", 16),
        ("core.handleServerError", 8),
        ("core.handleHip3AllMids", 14),
        ("public.handleAllMids", 11),
        ("public.handleFastAssetCtxs", 12),
    ];
    for (n, count) in expected {
        assert_eq!(
            cases
                .iter()
                .filter(|c| format!(
                    "{}.{}",
                    c["kind"].as_str().unwrap(),
                    c["name"].as_str().unwrap()
                ) == n)
                .count(),
            count,
            "independent floor {n}"
        );
    }
    let mut all_wire = vec![];
    let mut failed = vec![];
    for c in cases {
        let label = format!(
            "{}.{} {}",
            c["kind"].as_str().unwrap(),
            c["name"].as_str().unwrap(),
            c["label"].as_str().unwrap()
        );
        let run = catch_unwind(AssertUnwindSafe(|| {
            let mut h = Host {
                script: c["calls"].as_array().unwrap().iter().cloned().collect(),
                case: label.clone(),
                objects: HashMap::new(),
                buffers: HashMap::new(),
                maps: HashMap::new(),
                sets: HashMap::new(),
                effects: vec![],
                effect_names: oracle["effectNames"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_str().unwrap().into())
                    .collect(),
                wire: vec![],
            };
            for map in c["mapSeed"].as_array().unwrap() {
                h.maps.insert(
                    Native::decode(&map["value"]).id(),
                    map["entries"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|p| (Native::decode(&p[0]), Native::decode(&p[1])))
                        .collect(),
                );
            }
            for b in c["buffers"].as_array().unwrap() {
                h.buffers.insert(
                    Native::decode(&b["value"]).id(),
                    b["fields"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|p| {
                            (
                                JsString(
                                    p[0].as_array()
                                        .unwrap()
                                        .iter()
                                        .map(|x| x.as_u64().unwrap() as u16)
                                        .collect(),
                                ),
                                Tree::decode(&p[1]),
                            )
                        })
                        .collect(),
                );
            }
            let socket = Native::decode(&c["input"]);
            let arg = Native::decode(&c["args"][0]);
            let result = match (c["kind"].as_str().unwrap(), c["name"].as_str().unwrap()) {
                ("core", "isStaleMessage") => core_is_stale_message(&mut h, &socket, &arg),
                ("core", "onMessage") => core_on_message(&mut h, &socket, &arg),
                ("core", "processMessage") => core_process_message(&mut h, &socket, &arg),
                ("core", "handleFundings") => core_handle_fundings(&mut h, &socket, &arg),
                ("core", "handleUserNonFundingLedgerUpdates") => {
                    core_handle_ledger_updates(&mut h, &socket, &arg)
                }
                ("core", "handleUserEvents") => core_handle_user_events(&mut h, &socket, &arg),
                ("core", "handleServerError") => core_handle_server_error(&mut h, &socket, &arg),
                ("core", "handleHip3AllMids") => core_handle_hip3_all_mids(&mut h, &socket, &arg),
                ("public", "handleAllMids") => public_handle_all_mids(&mut h, &socket, &arg),
                ("public", "handleFastAssetCtxs") => {
                    public_handle_fast_asset_ctxs(&mut h, &socket, &arg)
                }
                _ => panic!("unknown root"),
            };
            match (&result, c["result"].get("throw")) {
                (Err(a), Some(b)) => assert_native(a, &Native::decode(b), &label),
                (Ok(a), None) => h
                    .tree(a)
                    .assert(&Tree::decode(&c["result"]["value"]), &label),
                _ => panic!("DISPATCH native outcome {label}"),
            }
            assert!(
                h.script.is_empty(),
                "DISPATCH native remaining operations {label}"
            );
            for map in c["mapFinal"].as_array().unwrap() {
                let actual = &h.maps[&Native::decode(&map["value"]).id()];
                let expected = map["entries"].as_array().unwrap();
                assert_eq!(
                    actual.len(),
                    expected.len(),
                    "DISPATCH native map cardinality {label}"
                );
                for ((ak, av), e) in actual.iter().zip(expected) {
                    assert_native(ak, &Native::decode(&e[0]), &label);
                    assert_native(av, &Native::decode(&e[1]), &label);
                }
            }
            for b in c["bufferFinal"].as_array().unwrap() {
                let actual = &h.buffers[&Native::decode(&b["value"]).id()];
                let expected = b["fields"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| {
                        (
                            JsString(
                                p[0].as_array()
                                    .unwrap()
                                    .iter()
                                    .map(|x| x.as_u64().unwrap() as u16)
                                    .collect(),
                            ),
                            Tree::decode(&p[1]),
                        )
                    })
                    .collect::<Vec<_>>();
                fields(actual, &expected, &label);
            }
            for set in c["sets"].as_array().unwrap() {
                let actual = &h.sets[&Native::decode(&set["value"]).id()];
                let expected = set["values"].as_array().unwrap();
                assert_eq!(
                    actual.len(),
                    expected.len(),
                    "DISPATCH native set cardinality {label}"
                );
                for (a, b) in actual.iter().zip(expected) {
                    assert_native(a, &Native::decode(b), &label);
                }
            }
            let effects = c["effects"].as_array().unwrap();
            assert_eq!(
                h.effects.len(),
                effects.len(),
                "DISPATCH native effect count {label}"
            );
            for ((name, receiver, args), e) in h.effects.iter().zip(effects) {
                assert_eq!(
                    name,
                    e["name"].as_str().unwrap(),
                    "DISPATCH native effect name {label}"
                );
                assert_native(receiver, &Native::decode(&e["receiver"]), &label);
                let expected = e["args"].as_array().unwrap();
                assert_eq!(
                    args.len(),
                    expected.len(),
                    "DISPATCH native effect arity {label}"
                );
                for (a, b) in args.iter().zip(expected) {
                    a.assert(&Tree::decode(b), &label);
                }
            }
            h.wire
        }));
        match run {
            Ok(w) => all_wire.extend(w),
            Err(_) => failed.push(label),
        }
    }
    assert!(failed.is_empty(), "DISPATCH native failed cases {failed:?}");
    // Every native assertion in every case completes before any wire assertion.
    for (expected, actual) in all_wire {
        assert_eq!(actual, expected, "supplemental wire arguments");
    }
}
