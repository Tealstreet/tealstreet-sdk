use safe_cex_venue_hyperliquid::{
    core_account_snapshots::*, core_position_order_state::*, core_rest_reads::*,
    helper_tasks::HelperTaskHost, private_message_dispatch::*, spot_json::JsString,
    ws_ping_lifecycle::*,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    future::Future,
    panic::{AssertUnwindSafe, catch_unwind},
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
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
    Map(Native, Native, Vec<(Native, Tree)>),
    Array(Native, Native, Vec<Tree>),
    Atom(Native),
    Literal(Native, Native, Vec<(JsString, Tree)>),
    Buffer(Native, Vec<(JsString, Tree)>),
}
impl Tree {
    fn decode(v: &Value) -> Self {
        match v[0].as_str().unwrap() {
            "map" => Self::Map(
                Native::decode(&v[1]),
                Native::decode(&v[2]),
                v[3].as_array()
                    .unwrap()
                    .iter()
                    .map(|p| (Native::decode(&p[0]), Self::decode(&p[1])))
                    .collect(),
            ),
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
            (Self::Map(a, ap, av), Self::Map(b, bp, bv)) => {
                assert_native(a, b, label);
                assert_native(ap, bp, label);
                assert_eq!(av.len(), bv.len(), "REST native map count {label}");
                for ((ak, av), (bk, bv)) in av.iter().zip(bv) {
                    assert_native(ak, bk, label);
                    av.assert(bv, label);
                }
            }
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
type Task = Pin<Box<dyn Future<Output = ()>>>;
struct Replay {
    script: VecDeque<Value>,
    case: String,
    wire: Vec<(Value, Value)>,
    operations: usize,
    owner_ops: HashMap<usize, usize>,
    next_owner: usize,
    arrays: HashMap<u64, Vec<Native>>,
    prototypes: HashMap<u64, Native>,
    objects: HashMap<u64, Vec<(JsString, Tree)>>,
    maps: HashMap<u64, Vec<(Native, Native)>>,
    sets: HashMap<u64, Vec<Native>>,
    values_iterators: HashMap<u64, VecDeque<Native>>,
    callbacks: HashMap<u64, RestCallback<Native>>,
    tasks: Vec<(usize, Task)>,
    root_result: Option<Result<Native, Native>>,
    jobs: VecDeque<Value>,
    awaits: usize,
    suspensions: usize,
    effects: Vec<(String, Native, Vec<Tree>)>,
    effect_names: Vec<String>,
}
#[derive(Clone)]
struct Host {
    r: Rc<RefCell<Replay>>,
    owner: usize,
    kind: String,
}
impl Host {
    fn fork(&self, kind: &str) -> Self {
        let mut r = self.r.borrow_mut();
        r.next_owner += 1;
        Self {
            r: self.r.clone(),
            owner: r.next_owner,
            kind: kind.into(),
        }
    }
    fn frame(&mut self, name: &str) -> Value {
        let mut r = self.r.borrow_mut();
        r.operations += 1;
        *r.owner_ops.entry(self.owner).or_default() += 1;
        let f = r
            .script
            .pop_front()
            .unwrap_or_else(|| panic!("REST native operation exhausted {} {name}", r.case));
        assert_eq!(f["op"], name, "REST native operation {}", r.case);
        f
    }
    fn arg(&self, f: &Value, name: &str, value: &Native) {
        assert_native(
            value,
            &Native::decode(&f["args"][name]),
            &self.r.borrow().case,
        );
    }
    fn finish(&mut self, f: Value, args: Value) -> Result<Native, Native> {
        self.r.borrow_mut().wire.push((f["args"].clone(), args));
        if let Some(e) = f.get("throw") {
            Err(Native::decode(e))
        } else {
            Ok(Native::decode(&f["result"]))
        }
    }
    fn tree(&self, value: &Native) -> Tree {
        if let Native::Handle(id, tag, _) = value {
            let r = self.r.borrow();
            if let Some(fields) = r.objects.get(id) {
                return Tree::Literal(value.clone(), r.prototypes[id].clone(), fields.clone());
            }
            if tag == "array" {
                let vals = r.arrays[id].clone();
                let proto = r.prototypes[id].clone();
                drop(r);
                return Tree::Array(
                    value.clone(),
                    proto,
                    vals.iter().map(|v| self.tree(v)).collect(),
                );
            }
            if tag == "map" {
                let vals = r.maps[id].clone();
                let proto = r.prototypes[id].clone();
                drop(r);
                return Tree::Map(
                    value.clone(),
                    proto,
                    vals.iter()
                        .map(|(k, v)| (k.clone(), self.tree(v)))
                        .collect(),
                );
            }
        }
        Tree::Atom(value.clone())
    }
    fn put(fields: &mut Vec<(JsString, Tree)>, name: &str, value: Tree) {
        let key = JsString(name.encode_utf16().collect());
        if let Some(e) = fields.iter_mut().find(|(k, _)| k.0 == key.0) {
            e.1 = value;
        } else {
            fields.push((key, value));
        }
    }
    fn invoke(&mut self, entry: &Value) -> Result<Native, Native> {
        let function = Native::decode(&entry["args"]["function"]);
        let callback = self.r.borrow().callbacks[&function.id()].clone();
        let mut body = self.fork(callback.kind());
        let f = body.frame("invoke");
        body.arg(&f, "function", &function);
        let receiver = Native::Undefined;
        body.arg(&f, "receiver", &receiver);
        let args = entry["args"]["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(Native::decode)
            .collect::<Vec<_>>();
        assert_eq!(args.len(), f["args"]["args"].as_array().unwrap().len());
        for (a, b) in args.iter().zip(f["args"]["args"].as_array().unwrap()) {
            assert_native(a, &Native::decode(b), &body.r.borrow().case);
        }
        let actual = if callback.is_async() {
            let observer = start_rest_callback(&body, callback, args.clone());
            drop(observer);
            Ok(Native::decode(&f["result"]))
        } else {
            run_rest_callback(&mut body, &callback, &args)
        };
        match (&actual, f.get("throw")) {
            (Ok(a), None) => assert_native(a, &Native::decode(&f["result"]), &body.r.borrow().case),
            (Err(a), Some(b)) => assert_native(a, &Native::decode(b), &body.r.borrow().case),
            _ => panic!("REST native invocation outcome {}", body.r.borrow().case),
        }
        body.r.borrow_mut().wire.push((f["args"].clone(),json!({"function":function.wire(),"receiver":receiver.wire(),"args":args.iter().map(Native::wire).collect::<Vec<_>>()})));
        actual
    }
    fn drive(&mut self, cx: &mut Context<'_>) {
        let mut tasks = std::mem::take(&mut self.r.borrow_mut().tasks);
        tasks.sort_by_key(|(owner, _)| std::cmp::Reverse(*owner));
        let mut pending = vec![];
        // Newest child first: caller remains suspended while its owned callee resolves.
        for (owner, mut task) in tasks {
            if task.as_mut().poll(cx).is_pending() {
                pending.push((owner, task));
            }
        }

        let mut r = self.r.borrow_mut();
        pending.append(&mut r.tasks);
        r.tasks = pending;
        drop(r);
        loop {
            let next = self.r.borrow().script.front().cloned();
            let Some(next) = next else { break };
            if next["op"] != "invoke" {
                break;
            }
            let job = self
                .r
                .borrow_mut()
                .jobs
                .pop_front()
                .expect("runtime supplied deferred invocation");

            let _ = self.invoke(&job);
        }
    }
    fn await_frame(&mut self, value: Native) -> impl Future<Output = Result<Native, Native>> {
        let f = self.frame("await");
        self.arg(&f, "value", &value);
        self.r
            .borrow_mut()
            .wire
            .push((f["args"].clone(), json!({"value":value.wire()})));
        let h = self.clone();
        let ordinal = *h.r.borrow().owner_ops.get(&h.owner).unwrap();
        let mut pending = true;
        std::future::poll_fn(move |cx| {
            if pending {
                pending = false;
                h.r.borrow_mut().suspensions += 1;
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            // A pending imported helper's callbacks run after the owned request task completes.
            if h.r.borrow().script.front().is_some_and(|f| {
                f["op"] == "invoke" || (f["op"] == "settle" && f["args"]["kind"] != h.kind)
            }) {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            assert_eq!(
                *h.r.borrow().owner_ops.get(&h.owner).unwrap(),
                ordinal,
                "REST native no owned continuation before resolution {}",
                h.r.borrow().case
            );
            h.r.borrow_mut().awaits += 1;
            Poll::Ready(if let Some(e) = f.get("throw") {
                Err(Native::decode(e))
            } else {
                Ok(Native::decode(&f["result"]))
            })
        })
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
            Native::Bool(b) => *b,
            Native::Number(n) => *n != 0.0 && !n.is_nan(),
            Native::Text(s) => !s.0.is_empty(),
            Native::Handle(..) => true,
        }
    }
    fn global(&mut self, name: &str) -> Result<Native, Native> {
        let f = self.frame("global");
        assert_eq!(
            f["args"]["name"],
            name,
            "REST native global {}",
            self.r.borrow().case
        );
        self.finish(f, json!({"name":name}))
    }
    fn get(&mut self, object: &Native, name: &str) -> Result<Native, Native> {
        let f = self.frame("get");
        self.arg(&f, "object", object);
        assert_eq!(
            f["args"]["name"],
            name,
            "REST native getter {}",
            self.r.borrow().case
        );
        self.finish(f, json!({"object":object.wire(),"name":name}))
    }
    fn set(&mut self, object: &Native, name: &str, value: &Native) -> Result<(), Native> {
        let f = self.frame("set");
        self.arg(&f, "object", object);
        self.arg(&f, "value", value);
        assert_eq!(
            f["args"]["name"],
            name,
            "REST native setter {}",
            self.r.borrow().case
        );
        self.finish(
            f,
            json!({"object":object.wire(),"name":name,"value":value.wire()}),
        )
        .map(|_| ())
    }
    fn object(&mut self) -> Result<Native, Native> {
        let f = self.frame("object");
        let proto = Native::decode(&f["prototype"]);
        let value = self.finish(f, json!({}))?;
        self.r.borrow_mut().prototypes.insert(value.id(), proto);
        self.r.borrow_mut().objects.insert(value.id(), vec![]);
        Ok(value)
    }
    fn define_data(&mut self, object: &Native, name: &str, value: &Native) -> Result<(), Native> {
        let f = self.frame("define");
        self.arg(&f, "object", object);
        self.arg(&f, "value", value);
        assert_eq!(
            f["args"]["name"],
            name,
            "REST native field {}",
            self.r.borrow().case
        );
        self.finish(
            f,
            json!({"object":object.wire(),"name":name,"value":value.wire()}),
        )?;
        let tree = self.tree(value);
        Self::put(
            self.r.borrow_mut().objects.get_mut(&object.id()).unwrap(),
            name,
            tree,
        );
        Ok(())
    }
    fn callback(&mut self, _: PingCallback<Native>) -> Result<Native, Native> {
        panic!("unused ping")
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
            "REST native call arity {}",
            self.r.borrow().case
        );
        for (a, b) in args.iter().zip(expected) {
            assert_native(a, &Native::decode(b), &self.r.borrow().case);
        }
        if self
            .r
            .borrow()
            .effect_names
            .iter()
            .any(|n| n == function.tag())
            && f.get("throw").is_none()
        {
            let trees = args.iter().map(|a| self.tree(a)).collect();
            self.r
                .borrow_mut()
                .effects
                .push((function.tag().into(), receiver.clone(), trees));
        }
        let mut callback_results = vec![];
        for entry in f["invokes"].as_array().unwrap() {
            if self.r.borrow().script.front() == Some(entry) {
                let result = self.invoke(entry);
                if let Ok(value) = result {
                    callback_results.push((
                        entry["args"]["args"]
                            .as_array()
                            .unwrap()
                            .first()
                            .map(Native::decode)
                            .unwrap_or(Native::Undefined),
                        value,
                    ));
                }
            } else {
                self.r.borrow_mut().jobs.push_back(entry.clone());
            }
        }
        let elements = f.get("elements").cloned();
        let prototype = f.get("prototype").cloned();
        let result=self.finish(f,json!({"function":function.wire(),"receiver":receiver.wire(),"args":args.iter().map(Native::wire).collect::<Vec<_>>()}))?;
        if function.tag() == "push" {
            self.r
                .borrow_mut()
                .arrays
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
                "map" => callback_results.into_iter().map(|(_, v)| v).collect(),
                "filter" if self.r.borrow().callbacks.contains_key(&args[0].id()) => {
                    callback_results
                        .into_iter()
                        .filter(|(_, v)| self.truthy(v))
                        .map(|(input, _)| input)
                        .collect()
                }
                _ => expected.clone(),
            };
            assert_eq!(
                actual.len(),
                expected.len(),
                "REST native owned array count {}",
                self.r.borrow().case
            );
            for (a, b) in actual.iter().zip(&expected) {
                assert_native(a, b, &self.r.borrow().case);
            }
            self.r.borrow_mut().arrays.insert(result.id(), actual);
            self.r
                .borrow_mut()
                .prototypes
                .insert(result.id(), Native::decode(&prototype.unwrap()));
        }
        if receiver.tag() == "map" {
            match function.tag() {
                "set" => {
                    let mut r = self.r.borrow_mut();
                    let entries = r.maps.get_mut(&receiver.id()).unwrap();
                    if let Some((_, value)) = entries.iter_mut().find(|(k, _)| k.equal(&args[0])) {
                        *value = args[1].clone();
                    } else {
                        entries.push((args[0].clone(), args[1].clone()));
                    }
                    assert_native(&result, receiver, &r.case);
                }
                "get" => {
                    let actual = self.r.borrow().maps[&receiver.id()]
                        .iter()
                        .find(|(k, _)| k.equal(&args[0]))
                        .map(|(_, v)| v.clone())
                        .unwrap_or(Native::Undefined);
                    assert_native(&actual, &result, &self.r.borrow().case);
                }
                "values" => {
                    let values = self.r.borrow().maps[&receiver.id()]
                        .iter()
                        .map(|(_, v)| v.clone())
                        .collect();
                    self.r
                        .borrow_mut()
                        .values_iterators
                        .insert(result.id(), values);
                }
                _ => {}
            }
        }
        if receiver.tag() == "set" && function.tag() == "has" {
            let actual = Native::Bool(
                self.r.borrow().sets[&receiver.id()]
                    .iter()
                    .any(|v| v.equal(&args[0])),
            );
            assert_native(&actual, &result, &self.r.borrow().case);
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
            Native::Handle(_, _, k) => k,
        }
    }
    fn has_property(&mut self, _: &str, _: &Native) -> Result<bool, Native> {
        panic!("unused has")
    }
    fn valid_fill_predicate(&mut self) -> Result<Native, Native> {
        panic!("unused predicate")
    }
    fn for_in(&mut self, _: &Native) -> Result<Native, Native> {
        panic!("unused forin")
    }
    fn for_in_next(&mut self, _: &Native) -> Result<Option<Native>, Native> {
        panic!("unused forin")
    }
    fn construct(&mut self, c: &Native, args: &[Native]) -> Result<Native, Native> {
        let f = self.frame("construct");
        self.arg(&f, "constructor", c);
        assert_eq!(
            args.len(),
            f["args"]["args"].as_array().unwrap().len(),
            "REST native construct arity"
        );
        for (a, b) in args.iter().zip(f["args"]["args"].as_array().unwrap()) {
            assert_native(a, &Native::decode(b), &self.r.borrow().case);
        }
        let prototype = f.get("prototype").map(Native::decode);
        let v=self.finish(f,json!({"constructor":c.wire(),"args":args.iter().map(Native::wire).collect::<Vec<_>>()}))?;
        if let Some(p) = prototype {
            self.r.borrow_mut().prototypes.insert(v.id(), p);
        }
        if c.tag() == "Map" {
            self.r.borrow_mut().maps.insert(v.id(), vec![]);
        }
        if c.tag() == "Set" {
            let values = self.r.borrow().arrays[&args[0].id()].clone();
            self.r.borrow_mut().sets.insert(v.id(), values);
        }
        Ok(v)
    }
    fn regex(&mut self, pattern: &str) -> Result<Native, Native> {
        let f = self.frame("regex");
        assert_eq!(
            f["args"]["pattern"],
            pattern,
            "REST native regex {}",
            self.r.borrow().case
        );
        self.finish(f, json!({"pattern":pattern}))
    }
    fn number(&self, n: f64) -> Native {
        Native::Number(n)
    }
    fn boolean(&self, b: bool) -> Native {
        Native::Bool(b)
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
        let p = Native::decode(&f["prototype"]);
        let v = self.finish(f, json!({}))?;
        self.r.borrow_mut().arrays.insert(v.id(), vec![]);
        self.r.borrow_mut().prototypes.insert(v.id(), p);
        Ok(v)
    }
    fn template_string(&mut self, v: &Native) -> Result<JsString, Native> {
        let f = self.frame("template");
        self.arg(&f, "value", v);
        let result = self.finish(f, json!({"value":v.wire()}))?;
        if let Native::Text(s) = result {
            Ok(s)
        } else {
            panic!("REST native template type")
        }
    }
    fn operator(&mut self, op: DispatchOperator, a: &Native, b: &Native) -> Result<Native, Native> {
        self.position_operator(
            match op {
                DispatchOperator::Less => "less",
                DispatchOperator::LessEqual => "lessEqual",
                DispatchOperator::Greater => "greater",
                DispatchOperator::Subtract => "subtract",
                DispatchOperator::Divide => "divide",
            },
            a,
            b,
        )
    }
}
impl CoreSnapshotHost for Host {
    fn null_value(&self) -> Native {
        Native::Null
    }
    fn add_values(&mut self, a: &Native, b: &Native) -> Result<Native, Native> {
        self.position_operator("add", a, b)
    }
    // The sibling methods in this impl return a future built elsewhere
    // (`self.await_frame(v)`), so every method keeps the `-> impl Future` shape
    // rather than half the impl being `async fn`.
    #[allow(clippy::manual_async_fn)]
    fn return_await_value(&mut self, _: Native) -> impl Future<Output = Result<Native, Native>> {
        async { panic!("unused returnawait") }
    }
    fn await_value(&mut self, v: Native) -> impl Future<Output = Result<Native, Native>> {
        self.await_frame(v)
    }
    fn error_value(&self, v: &Native) -> Native {
        v.clone()
    }
    fn construct_error(&mut self, c: &Native, args: &[Native]) -> Result<Native, Native> {
        self.construct(c, args)
    }
    fn instance_of(&mut self, _: &Native, _: &Native) -> Result<bool, Native> {
        panic!("unused instanceof")
    }
    fn snapshot_predicate(&mut self, _: SnapshotPredicate<Native>) -> Result<Native, Native> {
        panic!("unused snapshot predicate")
    }
    fn array_define(&mut self, array: &Native, index: usize, value: &Native) -> Result<(), Native> {
        let f = self.frame("arrayDefine");
        self.arg(&f, "object", array);
        self.arg(&f, "value", value);
        assert_eq!(
            f["args"]["index"],
            index,
            "REST native array index {}",
            self.r.borrow().case
        );
        self.finish(
            f,
            json!({"object":array.wire(),"index":index,"value":value.wire()}),
        )?;
        let mut r = self.r.borrow_mut();
        let vals = r.arrays.get_mut(&array.id()).unwrap();
        if vals.len() <= index {
            vals.resize(index + 1, Native::Undefined);
        }
        vals[index] = value.clone();
        Ok(())
    }
    fn spread_into(&mut self, object: &Native, source: &Native) -> Result<(), Native> {
        let f = self.frame("spread");
        self.arg(&f, "object", object);
        self.arg(&f, "source", source);
        let members = f["args"]["members"].as_array().unwrap().clone();
        let result = self.finish(
            f,
            json!({"object":object.wire(),"source":source.wire(),"members":members}),
        );
        for p in members {
            let value = Native::decode(&p[1]);
            let tree = self.tree(&value);
            Self::put(
                self.r.borrow_mut().objects.get_mut(&object.id()).unwrap(),
                p[0].as_str().unwrap(),
                tree,
            );
        }
        result.map(|_| ())
    }
    fn iterator(&mut self, value: &Native) -> Result<Native, Native> {
        let f = self.frame("iterator");
        self.arg(&f, "value", value);
        let result = self.finish(f, json!({"value":value.wire()}))?;
        let known = self.r.borrow().values_iterators.get(&value.id()).cloned();
        if let Some(v) = known {
            self.r.borrow_mut().values_iterators.insert(result.id(), v);
        }
        Ok(result)
    }
    fn next(&mut self, iterator: &Native) -> Result<Option<Native>, Native> {
        let f = self.frame("next");
        self.arg(&f, "iterator", iterator);
        if f["result"][0] == "done" {
            self.r
                .borrow_mut()
                .wire
                .push((f["args"].clone(), json!({"iterator":iterator.wire()})));
            if let Some(v) = self.r.borrow().values_iterators.get(&iterator.id()) {
                assert!(v.is_empty(), "REST native map value iterator count");
            }
            return Ok(None);
        }
        let value = self.finish(f, json!({"iterator":iterator.wire()}))?;
        let actual = self
            .r
            .borrow_mut()
            .values_iterators
            .get_mut(&iterator.id())
            .map(|v| v.pop_front().unwrap());
        if let Some(actual) = actual {
            assert_native(&actual, &value, &self.r.borrow().case);
        }
        Ok(Some(value))
    }
    fn close_after_throw(&mut self, i: &Native) {
        let _ = self.close_after_return(i);
    }
    fn close_after_return(&mut self, i: &Native) -> Result<(), Native> {
        let f = self.frame("close");
        self.arg(&f, "iterator", i);
        self.finish(f, json!({"iterator":i.wire()})).map(|_| ())
    }
    fn pair(&mut self, _: &Native) -> Result<[Native; 2], Native> {
        panic!("use masked binding")
    }
}
impl PositionOrderHost for Host {
    fn position_operator(
        &mut self,
        operator: &str,
        a: &Native,
        b: &Native,
    ) -> Result<Native, Native> {
        let f = self.frame("operator");
        assert_eq!(
            f["args"]["operator"],
            operator,
            "REST native operator {}",
            self.r.borrow().case
        );
        self.arg(&f, "left", a);
        self.arg(&f, "right", b);
        self.finish(
            f,
            json!({"operator":operator,"left":a.wire(),"right":b.wire()}),
        )
    }
    fn position_unary(&mut self, operator: &str, value: &Native) -> Result<Native, Native> {
        let f = self.frame("positionUnary");
        assert_eq!(
            f["args"]["operator"],
            operator,
            "REST native unary {}",
            self.r.borrow().case
        );
        self.arg(&f, "value", value);
        self.finish(f, json!({"operator":operator,"value":value.wire()}))
    }
    fn position_callback(&mut self, _: PositionCallback<Native>) -> Result<Native, Native> {
        panic!("unused position callback")
    }
    fn set_position_index(&mut self, _: &Native, _: &Native, _: &Native) -> Result<(), Native> {
        panic!("unused setindex")
    }
    fn append_position_spread(&mut self, array: &Native, value: &Native) -> Result<(), Native> {
        let iterator = self.iterator(value)?;
        while let Some(value) = self.next(&iterator)? {
            let index = self.r.borrow().arrays[&array.id()].len();
            if let Err(e) = self.array_define(array, index, &value) {
                self.close_after_throw(&iterator);
                return Err(e);
            }
        }
        Ok(())
    }
}
impl RestReadHost for Host {
    fn rest_callback(&mut self, callback: RestCallback<Native>) -> Result<Native, Native> {
        let captures: &[Native] = match &callback {
            RestCallback::FetchHip3Dexs0(c) => c,
            RestCallback::FetchHip3Dexs1(c) => c,
            RestCallback::FetchHip3Dexs2(c) => c,
            RestCallback::FetchHip3Markets0(c) => c,
            RestCallback::FetchOHLCV0(c) => c,
            RestCallback::FetchOHLCV1(c) => c,
            RestCallback::FetchIncomeHistory0(c) => c,
            RestCallback::FetchIncomeHistory1(c) => c,
            RestCallback::FetchIncomeHistory2(c) => c,
            RestCallback::FetchIncomeHistory3(c) => c,
            RestCallback::FetchIncomeHistory4(c) => c,
            RestCallback::FetchIncomeHistory5(c) => c,
            RestCallback::FetchIncomeHistory6(c) => c,
            RestCallback::FetchHistoricalOrders0(c) => c,
            RestCallback::FetchHistoricalOrders1(c) => c,
            RestCallback::FetchHistoricalOrders2(c) => c,
            RestCallback::FetchHistoricalOrders3(c) => c,
            RestCallback::FetchHistoricalOrders4(c) => c,
            RestCallback::DeriveClosedPositions0(c) => c,
            RestCallback::DeriveClosedPositions1(c) => c,
            RestCallback::DeriveClosedPositions2(c) => c,
            RestCallback::DeriveClosedPositions3(c) => c,
            RestCallback::FetchHistoricalFills0(c) => c,
            RestCallback::FetchHistoricalFills1(c) => c,
            RestCallback::FetchHistoricalFills2(c) => c,
            RestCallback::FetchHistoricalFills3(c) => c,
            RestCallback::FetchOHLCV2(c) => c,
        };
        let f = self.frame("restCallback");
        assert_eq!(
            f["args"]["kind"],
            callback.kind(),
            "REST native callback kind {}",
            self.r.borrow().case
        );
        assert_eq!(
            f["args"]["isAsync"],
            callback.is_async(),
            "REST native callback async {}",
            self.r.borrow().case
        );
        let expected = f["args"]["captures"].as_array().unwrap();
        assert_eq!(
            captures.len(),
            expected.len(),
            "REST native callback captures"
        );
        for (a, b) in captures.iter().zip(expected) {
            assert_native(a, &Native::decode(b), &self.r.borrow().case);
        }
        let value=self.finish(f,json!({"kind":callback.kind(),"captures":captures.iter().map(Native::wire).collect::<Vec<_>>(),"isAsync":callback.is_async()}))?;
        self.r.borrow_mut().callbacks.insert(value.id(), callback);
        Ok(value)
    }
    fn rest_exception(&self, value: Native) -> Native {
        value
    }
    fn rest_array_binding(&mut self, value: &Native, mask: &[bool]) -> Result<Vec<Native>, Native> {
        let f = self.frame("arrayBinding");
        self.arg(&f, "value", value);
        assert_eq!(
            f["args"]["mask"],
            json!(mask),
            "REST native array binding mask {}",
            self.r.borrow().case
        );
        self.r
            .borrow_mut()
            .wire
            .push((f["args"].clone(), json!({"value":value.wire(),"mask":mask})));
        if let Some(error) = f.get("throw") {
            return Err(Native::decode(error));
        }
        assert_eq!(f["result"].as_array().unwrap().len(), mask.len());
        // Production Realm::array_pattern returns bound slots only; holes are consumed, not returned.
        Ok(f["result"]
            .as_array()
            .unwrap()
            .iter()
            .zip(mask)
            .filter(|(_, bound)| **bound)
            .map(|(value, _)| Native::decode(value))
            .collect())
    }
    fn rest_argument_spread(
        &mut self,
        args: &mut Vec<Native>,
        value: &Native,
    ) -> Result<(), Native> {
        let iterator = self.iterator(value)?;
        while let Some(value) = self.next(&iterator)? {
            args.push(value);
        }
        Ok(())
    }
}
impl HelperTaskHost<Native, Native> for Host {
    fn start_task(&self, mut task: Task) {
        let waker = std::task::Waker::noop().clone();
        let mut cx = Context::from_waker(&waker);
        if task.as_mut().poll(&mut cx).is_pending() {
            self.r.borrow_mut().tasks.push((self.owner, task));
        }
    }
    fn settle_return(
        &self,
        result: Result<Native, Native>,
    ) -> Pin<Box<dyn Future<Output = Result<Native, Native>>>> {
        let mut h = self.clone();
        Box::pin(async move {
            let mut pending = true;
            std::future::poll_fn(|cx| {
                if pending {
                    pending = false;
                    cx.waker().wake_by_ref();
                    Poll::Pending
                } else {
                    Poll::Ready(())
                }
            })
            .await;
            let f = h.frame("settle");
            assert_eq!(
                f["args"]["kind"],
                h.kind,
                "REST native task kind {}",
                h.r.borrow().case
            );
            let (success, value) = match &result {
                Ok(v) => (true, v),
                Err(e) => (false, e),
            };
            assert_eq!(
                f["args"]["success"],
                success,
                "REST native task outcome {}",
                h.r.borrow().case
            );
            h.arg(&f, "value", value);
            h.finish(
                f,
                json!({"kind":h.kind,"success":success,"value":value.wire()}),
            )?;
            if h.kind.starts_with("root.") {
                h.r.borrow_mut().root_result = Some(result.clone());
            }
            result
        })
    }
}
fn run_task<F: Future>(h: &mut Host, future: F) -> F::Output {
    let waker = std::task::Waker::noop().clone();
    let mut cx = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    for _ in 0..1000 {
        h.drive(&mut cx);
        if let Poll::Ready(result) = future.as_mut().poll(&mut cx) {
            return result;
        }
    }
    panic!("REST native task stalled {}", h.r.borrow().case)
}
#[test]
fn actual_source_core_rest_reads_native_values() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let output = std::process::Command::new("node")
        .arg(root.join("sdk-rs/safe-cex-venue-hyperliquid/scripts/core-rest-oracle.cjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 94);
    assert_eq!(oracle["iterationStop"], 93);
    let names = [
        "fetchMarketsHelper",
        "fetchHip3Dexs",
        "fetchHip3Markets",
        "fetchOHLCV",
        "fetchIncomeHistory",
        "fetchHistoricalOrders",
        "deriveClosedPositions",
        "fetchIncomeHistoryRange",
        "fetchHistoricalFills",
        "fetchFillsForOrderIds",
    ];
    let counts = [8, 8, 8, 10, 10, 8, 8, 6, 20, 8];
    let awaits = [0, 8, 8, 10, 26, 7, 10, 5, 20, 7];
    assert_eq!(awaits.iter().sum::<usize>(), 101);
    for ((name, count), aw) in names.iter().zip(counts).zip(awaits) {
        assert_eq!(
            cases.iter().filter(|c| c["name"] == *name).count(),
            count,
            "REST independent count {name}"
        );
        assert_eq!(
            cases
                .iter()
                .filter(|c| c["name"] == *name)
                .map(|c| c["expectedAwaits"].as_u64().unwrap() as usize)
                .sum::<usize>(),
            aw,
            "REST independent await count {name}"
        );
    }
    let mut wires = vec![];
    let mut replayed_awaits = 0;
    let mut failed = vec![];
    let drop_witnesses = [
        ("fetchHip3Dexs", "production"),
        ("fetchIncomeHistory", "both"),
        ("fetchHistoricalFills", "windows"),
    ];
    let extra = drop_witnesses
        .iter()
        .map(|(name, label)| {
            let rows = cases
                .iter()
                .filter(|c| c["name"] == *name && c["label"] == *label)
                .collect::<Vec<_>>();
            assert_eq!(
                rows.len(),
                1,
                "independent root-drop witness {name}/{label}"
            );
            rows[0]
        })
        .collect::<Vec<_>>();
    assert_eq!(extra.len(), 3);
    for (c, expected) in extra.iter().zip([1, 4, 2]) {
        assert_eq!(
            c["expectedAwaits"], expected,
            "independent root-drop await floor"
        );
    }
    let runs = cases
        .iter()
        .map(|c| (c, false))
        .chain(extra.iter().map(|c| (*c, true)))
        .collect::<Vec<_>>();
    assert_eq!(runs.len(), 97);
    for (c, drop_root) in runs {
        let label = format!(
            "core.{} {}{}",
            c["name"].as_str().unwrap(),
            c["label"].as_str().unwrap(),
            if drop_root {
                " root observer dropped"
            } else {
                ""
            }
        );
        let run = catch_unwind(AssertUnwindSafe(|| {
            let exchange = Native::decode(&c["input"]);
            let replay = Replay {
                script: c["calls"].as_array().unwrap().iter().cloned().collect(),
                case: label.clone(),
                wire: vec![],
                operations: 0,
                owner_ops: HashMap::new(),
                next_owner: 0,
                arrays: HashMap::new(),
                prototypes: HashMap::new(),
                objects: HashMap::new(),
                maps: HashMap::new(),
                sets: HashMap::new(),
                values_iterators: HashMap::new(),
                callbacks: HashMap::new(),
                tasks: vec![],
                root_result: None,
                jobs: VecDeque::new(),
                awaits: 0,
                suspensions: 0,
                effects: vec![],
                effect_names: oracle["effectNames"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().into())
                    .collect(),
            };
            let mut h = Host {
                r: Rc::new(RefCell::new(replay)),
                owner: 0,
                kind: format!("root.{}", c["name"].as_str().unwrap()),
            };
            for array in c["seedArrays"].as_array().unwrap() {
                h.r.borrow_mut().arrays.insert(
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
                "fetchMarketsHelper" => {
                    fetch_markets_helper(&mut h, &exchange, &arg(0), &arg(1), &arg(2))
                }
                name => {
                    let observer = match name {
                        "fetchHip3Dexs" => fetch_hip3_dexs(&h, &exchange),
                        "fetchHip3Markets" => {
                            fetch_hip3_markets(&h, &exchange, &arg(0), &arg(1), &arg(2))
                        }
                        "fetchOHLCV" => fetch_ohlcv(&h, &exchange, &arg(0)),
                        "fetchIncomeHistory" => fetch_income_history(&h, &exchange, &arg(0)),
                        "fetchHistoricalOrders" => fetch_historical_orders(&h, &exchange, &arg(0)),
                        "deriveClosedPositions" => derive_closed_positions(&h, &exchange, &arg(0)),
                        "fetchIncomeHistoryRange" => {
                            fetch_income_history_range(&h, &exchange, &arg(0), &arg(1), &arg(2))
                        }
                        "fetchHistoricalFills" => fetch_historical_fills(&h, &exchange, &arg(0)),
                        "fetchFillsForOrderIds" => {
                            fetch_fills_for_order_ids(&h, &exchange, &arg(0))
                        }
                        _ => panic!("unknown root"),
                    };
                    assert_eq!(
                        h.r.borrow().operations,
                        c["prefixOps"].as_u64().unwrap() as usize,
                        "REST native eager prefix {label}"
                    );
                    if drop_root {
                        drop(observer);
                        let waker = std::task::Waker::noop().clone();
                        let mut cx = Context::from_waker(&waker);
                        for _ in 0..1000 {
                            if h.r.borrow().tasks.is_empty() {
                                break;
                            }
                            h.drive(&mut cx);
                        }
                        assert!(
                            h.r.borrow().tasks.is_empty(),
                            "REST native dropped root continuation {label}"
                        );
                        let result = h.r.borrow().root_result.clone();
                        assert!(
                            result.is_some(),
                            "REST native dropped root settled result {label}"
                        );
                        result.unwrap()
                    } else {
                        run_task(&mut h, observer)
                    }
                }
            };
            match (&result, c["result"].get("throw")) {
                (Ok(a), None) => h
                    .tree(a)
                    .assert(&Tree::decode(&c["result"]["value"]), &label),
                (Err(a), Some(b)) => assert_native(a, &Native::decode(b), &label),
                _ => panic!("REST native outcome {label}"),
            };
            let r = h.r.borrow();
            assert!(
                r.script.is_empty(),
                "REST native remaining operations {label}"
            );
            assert!(r.tasks.is_empty(), "REST native tasks retained {label}");
            assert!(r.jobs.is_empty(), "REST native jobs retained {label}");
            assert_eq!(
                r.awaits,
                c["expectedAwaits"].as_u64().unwrap() as usize,
                "REST native awaits {label}"
            );
            assert_eq!(r.suspensions, r.awaits, "REST native suspensions {label}");
            assert_eq!(
                r.effects.len(),
                c["effects"].as_array().unwrap().len(),
                "REST native effects count {label}"
            );
            for ((name, receiver, args), expected) in
                r.effects.iter().zip(c["effects"].as_array().unwrap())
            {
                assert_eq!(name, expected["name"].as_str().unwrap());
                assert_native(receiver, &Native::decode(&expected["receiver"]), &label);
                assert_eq!(args.len(), expected["args"].as_array().unwrap().len());
                for (a, b) in args.iter().zip(expected["args"].as_array().unwrap()) {
                    a.assert(&Tree::decode(b), &label);
                }
            }
            (r.wire.clone(), r.awaits)
        }));
        match run {
            Ok((w, awaits)) => {
                wires.extend(w);
                replayed_awaits += awaits;
            }
            Err(_) => failed.push(label),
        }
    }
    assert!(failed.is_empty(), "REST native failed cases {failed:?}");
    assert_eq!(replayed_awaits, 108, "independent total replay await floor");
    for (expected, actual) in wires {
        assert_eq!(actual, expected, "supplemental wire arguments");
    }
}
