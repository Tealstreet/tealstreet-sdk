use safe_cex_venue_hyperliquid::{
    core_account_snapshots::*, core_position_order_state::*, private_message_dispatch::*,
    spot_json::JsString, ws_ping_lifecycle::*,
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
    Array(Native, Native, Vec<Tree>),
    Atom(Native),
    Literal(Native, Native, Vec<(JsString, Tree)>),
    Buffer(Native, Vec<(JsString, Tree)>),
}
impl Tree {
    fn decode(v: &Value) -> Self {
        match v[0].as_str().unwrap() {
            "array" => Self::Array(
                Native::decode(&v[1]),
                Native::decode(&v[2]),
                v[3].as_array().unwrap().iter().map(Self::decode).collect(),
            ),
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
            (Self::Array(a, ap, af), Self::Array(b, bp, bf)) => {
                assert_native(a, b, label);
                assert_native(ap, bp, label);
                assert_eq!(af.len(), bf.len(), "DISPATCH native array count {label}");
                for (a, b) in af.iter().zip(bf) {
                    a.assert(b, label);
                }
            }
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
    exchange: Native,
    fields: HashMap<String, Native>,
    arrays: HashMap<u64, Vec<Native>>,
    array_protos: HashMap<u64, Native>,
    predicates: HashMap<u64, SnapshotPredicate<Native>>,
    position_callbacks: HashMap<u64, PositionCallback<Native>>,
    operations: std::rc::Rc<std::cell::Cell<usize>>,
    suspensions: std::rc::Rc<std::cell::RefCell<Vec<usize>>>,
    await_count: usize,
    dispose_at: Option<usize>,
    script: VecDeque<Value>,
    case: String,
    objects: HashMap<u64, (Native, Vec<(JsString, Tree)>)>,
    buffers: HashMap<u64, Vec<(JsString, Tree)>>,
    effects: Vec<(String, Native, Vec<Tree>)>,
    effect_names: Vec<String>,
    wire: Vec<(Value, Value)>,
}
impl Host {
    fn frame(&mut self, op: &str) -> Value {
        self.operations.set(self.operations.get() + 1);
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
        if v.tag() == "array" {
            let id = v.id();
            return Tree::Array(
                v.clone(),
                self.array_protos[&id].clone(),
                self.arrays[&id].iter().map(|v| self.tree(v)).collect(),
            );
        }
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
        if o.id() == self.exchange.id() {
            self.fields.insert(n.into(), v.clone());
        } else {
            panic!("untracked native assignment")
        };
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
        let mut callback_results = vec![];
        if let Some(callbacks) = f.get("callbacks") {
            let capture = self
                .position_callbacks
                .get(&args[0].id())
                .expect("owned position callback")
                .clone();
            for cb in callbacks.as_array().unwrap() {
                let inputs = cb["inputs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(Native::decode)
                    .collect::<Vec<_>>();
                let actual = run_position_callback(self, &capture, &inputs);
                match (&actual, cb.get("throw")) {
                    (Err(a), Some(b)) => assert_native(a, &Native::decode(b), &self.case),
                    (Ok(a), None) => {
                        assert_native(a, &Native::decode(&cb["result"]), &self.case);
                        callback_results.push((inputs[0].clone(), a.clone()));
                    }
                    _ => panic!("DISPATCH native callback outcome {}", self.case),
                }
            }
        }
        let elements = f.get("elements").cloned();
        let prototype = f.get("prototype").cloned();
        let state_after = f.get("stateAfter").cloned();
        let result=self.finish(f,json!({"function":function.wire(),"receiver":receiver.wire(),"args":args.iter().map(Native::wire).collect::<Vec<_>>()}))?;
        if function.tag() == "push" {
            self.arrays
                .get_mut(&receiver.id())
                .unwrap()
                .extend(args.iter().cloned());
        }
        if let Some(elements) = elements {
            let expected = elements
                .as_array()
                .unwrap()
                .iter()
                .map(Native::decode)
                .collect::<Vec<_>>();
            let actual = match function.tag() {
                "map" => callback_results
                    .into_iter()
                    .map(|(_, result)| result)
                    .collect::<Vec<_>>(),
                "filter" => callback_results
                    .into_iter()
                    .filter(|(_, v)| self.truthy(v))
                    .map(|(input, _)| input)
                    .collect::<Vec<_>>(),
                _ => expected.clone(), // Explicit primitive return, not a claimed implementation of ECMAScript libraries.
            };
            assert_eq!(
                actual.len(),
                expected.len(),
                "DISPATCH native callback array count {}",
                self.case
            );
            for (a, b) in actual.iter().zip(&expected) {
                assert_native(a, b, &self.case);
            }
            self.arrays.insert(result.id(), actual);
            self.array_protos
                .insert(result.id(), Native::decode(&prototype.unwrap()));
        }
        if self.effect_names.iter().any(|n| n == function.tag()) {
            self.effects.push((
                function.tag().into(),
                receiver.clone(),
                args.iter().map(|a| self.tree(a)).collect(),
            ));
        }
        if let Some(state) = state_after {
            for field in state.as_array().unwrap() {
                let name = field[0].as_str().unwrap();
                let expected = Tree::decode(&field[1]);
                if function.tag() == "addOrUpdatePositionsFull" && name == "positions" {
                    self.tree(&args[0]).assert(&expected, &self.case);
                    self.fields.insert(name.into(), args[0].clone());
                } else if function.tag() == "update" && name == "balance" {
                    let fields = &self.objects[&args[0].id()].1;
                    if let Some((_, Tree::Literal(value, _, _))) = fields
                        .iter()
                        .find(|(k, _)| k.0 == "balance".encode_utf16().collect::<Vec<_>>())
                    {
                        let value = value.clone();
                        self.tree(&value).assert(&expected, &self.case);
                        self.fields.insert(name.into(), value);
                    }
                } else if let Tree::Atom(value) = expected {
                    self.fields.insert(name.into(), value);
                }
            }
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
        let proto = Native::decode(&f["prototype"]);
        let array = self.finish(f, json!({}))?;
        self.array_protos.insert(array.id(), proto);
        self.arrays.insert(array.id(), vec![]);
        Ok(array)
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
    fn has_property(&mut self, _: &str, _: &Native) -> Result<bool, Native> {
        panic!("unused has")
    }
    fn valid_fill_predicate(&mut self) -> Result<Native, Native> {
        panic!("unused fillpredicate")
    }
    fn for_in(&mut self, _: &Native) -> Result<Native, Native> {
        panic!("unused forin")
    }
    fn for_in_next(&mut self, _: &Native) -> Result<Option<Native>, Native> {
        panic!("unused forinnext")
    }
    fn construct(&mut self, c: &Native, args: &[Native]) -> Result<Native, Native> {
        self.construct_error(c, args)
    }
    fn regex(&mut self, _: &str) -> Result<Native, Native> {
        panic!("unused regex")
    }
}

impl Host {
    fn await_frame(
        &mut self,
        value: Native,
        op: &str,
    ) -> impl std::future::Future<Output = Result<Native, Native>> {
        let frame = self.frame(op);
        self.arg(&frame, "value", &value);
        self.wire
            .push((frame["args"].clone(), json!({"value":value.wire()})));
        let ordinal = self.operations.get();
        let mut pending = true;
        std::future::poll_fn(move |cx| {
            if pending {
                pending = false;
                self.suspensions.borrow_mut().push(ordinal);
                cx.waker().wake_by_ref();
                return std::task::Poll::Pending;
            }
            assert_eq!(
                self.operations.get(),
                ordinal,
                "DISPATCH native no continuation before await resolution {}",
                self.case
            );
            self.await_count += 1;
            if self.dispose_at == Some(self.await_count) {
                self.fields.insert("isDisposed".into(), Native::Bool(true));
            }
            for p in frame["state"].as_array().unwrap() {
                self.tree(&self.fields[p[0].as_str().unwrap()])
                    .assert(&Tree::decode(&p[1]), &self.case);
            }
            std::task::Poll::Ready(if let Some(e) = frame.get("throw") {
                Err(Native::decode(e))
            } else {
                Ok(Native::decode(&frame["result"]))
            })
        })
    }
}
impl CoreSnapshotHost for Host {
    fn null_value(&self) -> Native {
        Native::Null
    }
    fn add_values(&mut self, a: &Native, b: &Native) -> Result<Native, Native> {
        let f = self.frame("operator");
        assert_eq!(
            f["args"]["operator"], "add",
            "DISPATCH native add {}",
            self.case
        );
        self.arg(&f, "left", a);
        self.arg(&f, "right", b);
        self.finish(
            f,
            json!({"operator":"add","left":a.wire(),"right":b.wire()}),
        )
    }
    fn await_value(
        &mut self,
        value: Native,
    ) -> impl std::future::Future<Output = Result<Native, Native>> {
        self.await_frame(value, "await")
    }
    fn return_await_value(
        &mut self,
        value: Native,
    ) -> impl std::future::Future<Output = Result<Native, Native>> {
        self.await_frame(value, "returnAwait")
    }
    fn error_value(&self, v: &Native) -> Native {
        v.clone()
    }
    fn construct_error(&mut self, c: &Native, args: &[Native]) -> Result<Native, Native> {
        let f = self.frame("construct");
        self.arg(&f, "constructor", c);
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
        self.finish(f,json!({"constructor":c.wire(),"args":args.iter().map(Native::wire).collect::<Vec<_>>()}))
    }
    fn instance_of(&mut self, v: &Native, c: &Native) -> Result<bool, Native> {
        let f = self.frame("instanceof");
        self.arg(&f, "value", v);
        self.arg(&f, "constructor", c);
        let r = self.finish(f, json!({"value":v.wire(),"constructor":c.wire()}))?;
        let Native::Bool(v) = r else {
            panic!("native instanceof bool")
        };
        Ok(v)
    }
    fn snapshot_predicate(&mut self, capture: SnapshotPredicate<Native>) -> Result<Native, Native> {
        let (kind, v) = match &capture {
            SnapshotPredicate::SpotBalance => ("spotBalance", Native::Undefined),
            SnapshotPredicate::PositionSymbol { symbols } => ("positionSymbol", symbols.clone()),
        };
        let f = self.frame("snapshotPredicate");
        assert_eq!(
            f["args"]["kind"], kind,
            "DISPATCH native predicate kind {}",
            self.case
        );
        self.arg(&f, "capture", &v);
        let value = self.finish(f, json!({"kind":kind,"capture":v.wire()}))?;
        self.predicates.insert(value.id(), capture);
        Ok(value)
    }
    fn array_define(&mut self, array: &Native, i: usize, value: &Native) -> Result<(), Native> {
        let f = self.frame("arrayDefine");
        self.arg(&f, "object", array);
        self.arg(&f, "value", value);
        assert_eq!(
            f["args"]["index"], i,
            "DISPATCH native array index {}",
            self.case
        );
        self.finish(
            f,
            json!({"object":array.wire(),"index":i,"value":value.wire()}),
        )?;
        let values = self.arrays.get_mut(&array.id()).unwrap();
        if values.len() <= i {
            values.resize(i + 1, Native::Undefined);
        }
        values[i] = value.clone();
        Ok(())
    }
    fn spread_into(&mut self, o: &Native, source: &Native) -> Result<(), Native> {
        let f = self.frame("spread");
        self.arg(&f, "object", o);
        self.arg(&f, "source", source);
        let members = f["args"]["members"].as_array().unwrap().clone();
        let wire_members = Value::Array(members.clone());
        let result = self.finish(
            f,
            json!({"object":o.wire(),"source":source.wire(),"members":wire_members}),
        );
        for p in members {
            let value = Native::decode(&p[1]);
            let tree = self.tree(&value);
            Self::put(
                &mut self.objects.get_mut(&o.id()).unwrap().1,
                p[0].as_str().unwrap(),
                tree,
            );
        }
        result.map(|_| ())
    }
    fn iterator(&mut self, v: &Native) -> Result<Native, Native> {
        let f = self.frame("iterator");
        self.arg(&f, "value", v);
        self.finish(f, json!({"value":v.wire()}))
    }
    fn next(&mut self, i: &Native) -> Result<Option<Native>, Native> {
        let f = self.frame("next");
        self.arg(&f, "iterator", i);
        if f["result"][0] == "done" {
            self.wire
                .push((f["args"].clone(), json!({"iterator":i.wire()})));
            Ok(None)
        } else {
            self.finish(f, json!({"iterator":i.wire()})).map(Some)
        }
    }
    fn close_after_throw(&mut self, i: &Native) {
        let _ = self.close_after_return(i);
    }
    fn close_after_return(&mut self, i: &Native) -> Result<(), Native> {
        let f = self.frame("close");
        self.arg(&f, "iterator", i);
        self.finish(f, json!({"iterator":i.wire()})).map(|_| ())
    }
    fn pair(&mut self, v: &Native) -> Result<[Native; 2], Native> {
        let f = self.frame("pair");
        self.arg(&f, "value", v);
        self.wire
            .push((f["args"].clone(), json!({"value":v.wire()})));
        if let Some(e) = f.get("throw") {
            return Err(Native::decode(e));
        }
        assert_eq!(
            f["result"].as_array().unwrap().len(),
            2,
            "independent tuplearity"
        );
        Ok([
            Native::decode(&f["result"][0]),
            Native::decode(&f["result"][1]),
        ])
    }
}
fn block_on<F: std::future::Future>(future: F) -> F::Output {
    let waker = std::task::Waker::noop().clone();
    let mut cx = std::task::Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            std::task::Poll::Ready(v) => return v,
            std::task::Poll::Pending => {}
        }
    }
}
#[test]
fn actual_source_position_order_state_native_values() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let output = std::process::Command::new("node")
        .arg(root.join("sdk-rs/safe-cex-venue-hyperliquid/scripts/position-state-oracle.cjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 106);
    assert_eq!(oracle["iterationStop"], 105);
    let names = [
        "applyCoreClearinghouseState",
        "handleHip3ClearinghouseState",
        "handleFills",
        "applyOptimisticCorePositionFromFill",
        "handleOrderUpdates",
        "applyRealizedPnlToCurrentPosition",
        "generateFakePositionsFromMarkets",
        "syncBalanceFromLivePositions",
        "fetchHip3Positions",
        "parseOpenOrders",
    ];
    let counts = [10, 10, 10, 12, 16, 8, 8, 12, 10, 10];
    for (n, count) in names.iter().zip(counts) {
        assert_eq!(
            cases.iter().filter(|c| c["name"] == *n).count(),
            count,
            "independent floor {n}"
        );
    }
    let expected_awaits = [0, 0, 0, 0, 0, 0, 0, 0, 9, 0];
    for (name, expected) in names.iter().zip(expected_awaits) {
        assert_eq!(
            cases
                .iter()
                .filter(|c| c["name"] == *name)
                .map(|c| c["expectedAwaits"].as_u64().unwrap())
                .sum::<u64>(),
            expected,
            "independent await floor {name}"
        );
    }
    let mut wires = vec![];
    let mut failed = vec![];
    for c in cases {
        let label = format!(
            "core.{} {}",
            c["name"].as_str().unwrap(),
            c["label"].as_str().unwrap()
        );
        let run = catch_unwind(AssertUnwindSafe(|| {
            let exchange = Native::decode(&c["input"]);
            let mut h = Host {
                exchange: exchange.clone(),
                fields: HashMap::new(),
                arrays: HashMap::new(),
                array_protos: HashMap::new(),
                predicates: HashMap::new(),
                position_callbacks: HashMap::new(),
                operations: Default::default(),
                suspensions: Default::default(),
                await_count: 0,
                dispose_at: c["disposeAt"].as_u64().map(|v| v as usize),
                script: c["calls"].as_array().unwrap().iter().cloned().collect(),
                case: label.clone(),
                objects: HashMap::new(),
                buffers: HashMap::new(),
                effects: vec![],
                effect_names: oracle["effectNames"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().into())
                    .collect(),
                wire: vec![],
            };
            for p in c["initial"].as_array().unwrap() {
                let Tree::Atom(v) = Tree::decode(&p[1]) else {
                    panic!("seed state opaque")
                };
                h.fields.insert(p[0].as_str().unwrap().into(), v);
            }
            for array in c["seedArrays"].as_array().unwrap() {
                h.arrays.insert(
                    Native::decode(&array["value"]).id(),
                    array["elements"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(Native::decode)
                        .collect(),
                );
            }
            let args = c["args"]
                .as_array()
                .unwrap()
                .iter()
                .map(Native::decode)
                .collect::<Vec<_>>();
            let arg = |i| args.get(i).cloned().unwrap_or(Native::Undefined);
            let result = match c["name"].as_str().unwrap() {
                "applyCoreClearinghouseState" => {
                    apply_core_clearinghouse_state(&mut h, &exchange, &arg(0), &arg(1))
                }
                "handleHip3ClearinghouseState" => {
                    handle_hip3_clearinghouse_state(&mut h, &exchange, &arg(0), &arg(1))
                }
                "handleFills" => handle_fills(&mut h, &exchange, &arg(0)),
                "applyOptimisticCorePositionFromFill" => {
                    apply_optimistic_core_position_from_fill(&mut h, &exchange, &arg(0), &arg(1))
                }
                "handleOrderUpdates" => handle_order_updates(&mut h, &exchange, &arg(0)),
                "applyRealizedPnlToCurrentPosition" => apply_realized_pnl_to_current_position(
                    &mut h,
                    &exchange,
                    &arg(0),
                    &arg(1),
                    &arg(2),
                ),
                "generateFakePositionsFromMarkets" => {
                    generate_fake_positions_from_markets(&mut h, &exchange, &arg(0))
                }
                "syncBalanceFromLivePositions" => {
                    sync_balance_from_live_positions(&mut h, &exchange, &arg(0))
                }
                "fetchHip3Positions" => block_on(fetch_hip3_positions(&mut h, &exchange, &arg(0))),
                "parseOpenOrders" => parse_open_orders(&mut h, &exchange, &arg(0)),
                _ => panic!("unknown root"),
            };
            match (&result, c["result"].get("throw")) {
                (Err(a), Some(b)) => assert_native(a, &Native::decode(b), &label),
                (Ok(a), None) => h
                    .tree(a)
                    .assert(&Tree::decode(&c["result"]["value"]), &label),
                _ => panic!("DISPATCH native outcome {label}"),
            };
            assert!(
                h.script.is_empty(),
                "DISPATCH native remaining operations {label}"
            );
            let expected_awaits = c["expectedAwaits"].as_u64().unwrap() as usize;
            assert_eq!(
                h.await_count, expected_awaits,
                "DISPATCH native await count {label}"
            );
            assert_eq!(
                h.suspensions.borrow().len(),
                expected_awaits,
                "DISPATCH native suspended await count {label}"
            );
            for p in c["final"].as_array().unwrap() {
                h.tree(&h.fields[p[0].as_str().unwrap()])
                    .assert(&Tree::decode(&p[1]), &label);
            }
            let expected = c["effects"].as_array().unwrap();
            assert_eq!(
                h.effects.len(),
                expected.len(),
                "DISPATCH native effect count {label}"
            );
            for ((name, receiver, args), e) in h.effects.iter().zip(expected) {
                assert_eq!(
                    name,
                    e["name"].as_str().unwrap(),
                    "DISPATCH native effect name {label}"
                );
                assert_native(receiver, &Native::decode(&e["receiver"]), &label);
                let values = e["args"].as_array().unwrap();
                assert_eq!(
                    args.len(),
                    values.len(),
                    "DISPATCH native effect arity {label}"
                );
                for (a, b) in args.iter().zip(values) {
                    a.assert(&Tree::decode(b), &label);
                }
            }
            h.wire
        }));
        match run {
            Ok(v) => wires.extend(v),
            Err(_) => failed.push(label),
        }
    }
    assert!(failed.is_empty(), "DISPATCH native failed cases {failed:?}");
    for (expected, actual) in wires {
        assert_eq!(actual, expected, "supplemental wire arguments");
    }
}
impl PositionOrderHost for Host {
    fn position_operator(
        &mut self,
        operator: &str,
        left: &Native,
        right: &Native,
    ) -> Result<Native, Native> {
        let f = self.frame("operator");
        assert_eq!(
            f["args"]["operator"], operator,
            "DISPATCH native operator {}",
            self.case
        );
        self.arg(&f, "left", left);
        self.arg(&f, "right", right);
        self.finish(
            f,
            json!({"operator":operator,"left":left.wire(),"right":right.wire()}),
        )
    }
    fn position_unary(&mut self, operator: &str, value: &Native) -> Result<Native, Native> {
        let f = self.frame("positionUnary");
        assert_eq!(
            f["args"]["operator"], operator,
            "DISPATCH native unary {}",
            self.case
        );
        self.arg(&f, "value", value);
        self.finish(f, json!({"operator":operator,"value":value.wire()}))
    }
    fn position_callback(&mut self, callback: PositionCallback<Native>) -> Result<Native, Native> {
        let (kind, captures): (&str, &[Native]) = match &callback {
            PositionCallback::HandleFills0(c) => ("handleFills_0", c),
            PositionCallback::ApplyOptimisticCorePositionFromFill0(c) => {
                ("applyOptimisticCorePositionFromFill_0", c)
            }
            PositionCallback::HandleOrderUpdates0(c) => ("handleOrderUpdates_0", c),
            PositionCallback::ApplyRealizedPnlToCurrentPosition0(c) => {
                ("applyRealizedPnlToCurrentPosition_0", c)
            }
            PositionCallback::GenerateFakePositionsFromMarkets0(c) => {
                ("generateFakePositionsFromMarkets_0", c)
            }
            PositionCallback::FetchHip3Positions0(c) => ("fetchHip3Positions_0", c),
            PositionCallback::FetchHip3Positions1(c) => ("fetchHip3Positions_1", c),
            PositionCallback::FetchHip3Positions2(c) => ("fetchHip3Positions_2", c),
            PositionCallback::FetchHip3Positions3(c) => ("fetchHip3Positions_3", c),
            PositionCallback::FetchHip3Positions4(c) => ("fetchHip3Positions_4", c),
            PositionCallback::ParseOpenOrders0(c) => ("parseOpenOrders_0", c),
            PositionCallback::ParseOpenOrders1(c) => ("parseOpenOrders_1", c),
            PositionCallback::ParseOpenOrders2(c) => ("parseOpenOrders_2", c),
        };
        let f = self.frame("positionCallback");
        assert_eq!(
            f["args"]["kind"], kind,
            "DISPATCH native callback kind {}",
            self.case
        );
        let expected = f["args"]["captures"].as_array().unwrap();
        assert_eq!(
            captures.len(),
            expected.len(),
            "native callback capture count"
        );
        for (a, b) in captures.iter().zip(expected) {
            assert_native(a, &Native::decode(b), &self.case);
        }
        let value = self.finish(
            f,
            json!({"kind":kind,"captures":captures.iter().map(Native::wire).collect::<Vec<_>>()}),
        )?;
        self.position_callbacks.insert(value.id(), callback);
        Ok(value)
    }
    fn set_position_index(
        &mut self,
        object: &Native,
        key: &Native,
        value: &Native,
    ) -> Result<(), Native> {
        let f = self.frame("setIndex");
        self.arg(&f, "object", object);
        self.arg(&f, "key", key);
        self.arg(&f, "value", value);
        self.finish(
            f,
            json!({"object":object.wire(),"key":key.wire(),"value":value.wire()}),
        )
        .map(|_| ())
    }
    fn append_position_spread(&mut self, array: &Native, value: &Native) -> Result<(), Native> {
        let iterator = self.iterator(value)?;
        while let Some(value) = self.next(&iterator)? {
            let index = self.arrays[&array.id()].len();
            if let Err(error) = self.array_define(array, index, &value) {
                self.close_after_throw(&iterator);
                return Err(error);
            }
        }
        Ok(())
    }
}
