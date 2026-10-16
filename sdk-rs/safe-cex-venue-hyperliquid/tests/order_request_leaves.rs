use safe_cex_venue_hyperliquid::{
    balance_state::*, order_request_leaves::*, spot_json::JsString, ws_ping_lifecycle::*,
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
                    .map(|v| v.as_u64().unwrap() as u16)
                    .collect(),
            )),
            "ref" => Self::Handle(
                v[1].as_u64().unwrap(),
                v[2].as_str().unwrap().into(),
                v[3].as_str().unwrap().into(),
            ),
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
            Self::Handle(id, tag, kind) => json!(["ref", id, tag, kind]),
        }
    }
    fn tag(&self) -> &str {
        if let Self::Handle(_, tag, _) = self {
            tag
        } else {
            ""
        }
    }
    fn id(&self) -> u64 {
        if let Self::Handle(id, _, _) = self {
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
    refresh_count: u64,
    invalidate_count: u64,
    fetch_count: u64,
    operations: std::rc::Rc<std::cell::Cell<usize>>,
    suspensions: std::rc::Rc<std::cell::RefCell<Vec<usize>>>,
}
fn eq(a: &Native, b: &Native) -> bool {
    match (a, b) {
        (Native::Null, Native::Null) | (Native::Undefined, Native::Undefined) => true,
        (Native::Bool(a), Native::Bool(b)) => a == b,
        (Native::Number(a), Native::Number(b)) => a == b,
        (Native::Text(a), Native::Text(b)) => a == b,
        (Native::Handle(a, _, _), Native::Handle(b, _, _)) => a == b,
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
        self.operations.set(self.operations.get() + 1);
        let f = self
            .script
            .pop_front()
            .unwrap_or_else(|| panic!("REQUEST native operation exhausted {} {op}", self.case));
        assert_eq!(f["op"], op, "REQUEST native operation {}", self.case);
        assert_eq!(
            f["args"], args,
            "REQUEST native argument {} {op}",
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
        if let Native::Handle(id, _, _) = v {
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
    fn snapshot(&self) -> Value {
        json!({"approved":self.fields["_builderFeeApproved"].native(),"refreshCount":self.refresh_count,"invalidateCount":self.invalidate_count,"fetchCount":self.fetch_count})
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
                    "REQUEST native tracked getter {}",
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
        self.put(o, JsString(n.encode_utf16().collect()), v);
        Ok(())
    }
    fn call(&mut self, f: &Native, o: &Native, args: &[Native]) -> Result<Native, Native> {
        let mut input = json!({"function":f.native(),"receiver":o.native(),"args":args.iter().map(Native::native).collect::<Vec<_>>()});
        if ["post", "signActionOrDefer", "signL1Action"].contains(&f.tag()) {
            input["payloads"] = json!(args.iter().map(|v| self.payload(v)).collect::<Vec<_>>());
        }
        let frame = self.frame("call", input);
        if frame["invoked"] == true {
            match f.tag() {
                "scheduleOrderRefresh" => self.refresh_count += 1,
                "invalidateFakePositionsCache" => self.invalidate_count += 1,
                "fetchBuilderFeeApproval" => self.fetch_count += 1,
                _ => {}
            }
        }
        Self::decode(&frame)
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
        if let Native::Handle(id, _, _) = v {
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
                "REQUEST native owned array iteration {}",
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
                    "REQUEST native numeric comparison {}",
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
                    "REQUEST native arithmetic bits {}",
                    self.case
                );
            }
        }
        Ok(value)
    }
    fn spread_into(&mut self, o: &Native, source: &Native) -> Result<(), Native> {
        self.operations.set(self.operations.get() + 1);
        let f = self.script.pop_front().unwrap();
        assert_eq!(
            f["op"], "spread",
            "REQUEST native spread operation {}",
            self.case
        );
        assert_eq!(
            f["args"]["object"],
            o.native(),
            "REQUEST native spread target {}",
            self.case
        );
        assert_eq!(
            f["args"]["source"],
            source.native(),
            "REQUEST native spread source {}",
            self.case
        );
        let members = f["args"]["members"].as_array().unwrap();
        if let Native::Handle(id, _, _) = source {
            if let Some(object) = self.objects.get(id) {
                let own = object
                    .fields
                    .iter()
                    .map(|(k, v)| json!([String::from_utf16(&k.0).unwrap(), v.native()]))
                    .collect::<Vec<_>>();
                assert_eq!(
                    json!(own),
                    json!(members),
                    "REQUEST native owned spread fields {}",
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

impl OrderRequestHost for Host {
    fn await_value(
        &mut self,
        value: Native,
    ) -> impl std::future::Future<Output = Result<Native, Native>> {
        let frame = self.frame("await", json!({"value":value.native()}));
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
                "REQUEST native no continuation before await resolution {}",
                self.case
            );
            self.fields.insert(
                "_builderFeeApproved".into(),
                Native::decode(&frame["state"]["approved"]),
            );
            assert_eq!(
                self.snapshot(),
                frame["state"],
                "REQUEST native await state {}",
                self.case
            );
            std::task::Poll::Ready(Self::decode(&frame))
        })
    }
    fn construct_error(&mut self, c: &Native, args: &[Native]) -> Result<Native, Native> {
        self.step("construct",json!({"constructor":c.native(),"args":args.iter().map(Native::native).collect::<Vec<_>>()}))
    }
    fn instance_of(&mut self, v: &Native, c: &Native) -> Result<bool, Native> {
        let r = self.step(
            "instanceof",
            json!({"value":v.native(),"constructor":c.native()}),
        )?;
        let Native::Bool(b) = r else {
            panic!("native instanceof bool")
        };
        Ok(b)
    }
    fn type_is(&self, v: &Native, k: &str) -> bool {
        match v {
            Native::Undefined => k == "undefined",
            Native::Null => k == "object",
            Native::Bool(_) => k == "boolean",
            Native::Number(_) => k == "number",
            Native::Text(_) => k == "string",
            Native::Handle(_, _, kind) => k == kind,
        }
    }
    fn unary_number(&mut self, v: &Native) -> Result<Native, Native> {
        self.step("unary", json!({"value":v.native()}))
    }
    fn compare(&mut self, op: RequestComparison, a: &Native, b: &Native) -> Result<Native, Native> {
        let r=self.step("compare",json!({"operator":if op==RequestComparison::Less{"less"}else{"greaterEqual"},"left":a.native(),"right":b.native()}))?;
        if let (Native::Number(a), Native::Number(b)) = (a, b) {
            assert_eq!(
                r.native(),
                Native::Bool(if op == RequestComparison::Less {
                    a < b
                } else {
                    a >= b
                })
                .native(),
                "REQUEST native numeric comparison {}",
                self.case
            );
        }
        Ok(r)
    }
}
fn run<F: std::future::Future>(future: F) -> F::Output {
    let waker = std::task::Waker::noop().clone();
    let mut cx = std::task::Context::from_waker(&waker);
    let mut f = std::pin::pin!(future);
    for _ in 0..8 {
        if let std::task::Poll::Ready(r) = f.as_mut().poll(&mut cx) {
            return r;
        }
    }
    panic!("REQUEST native future exceeded independent eight-poll bound")
}
#[test]
fn native_ten_order_request_roots_match_actual_source() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = std::process::Command::new("node")
        .arg(root.join("scripts/order-request-oracle.cjs"))
        .output()
        .expect("source oracle");
    assert!(
        output.status.success(),
        "source oracle failed {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 143, "independent143sourcecases");
    assert_eq!(oracle["iterationStop"], 142);
    let names = [
        "core.getBuilderInfo",
        "core.getOrderErrorTelemetry",
        "core.postOrderAction",
        "core.validateResponseAndExtractOrderIds",
        "core.signActionOrDefer",
        "core.scheduleCancel",
        "core.setLeverage",
        "core.changeMarginType",
        "spot.setLeverage",
        "spot.changeMarginType",
    ];
    assert_eq!(oracle["names"], json!(names), "independent exact ten roots");
    let mut counts = [0; 10];
    let mut pending_counts = [0; 10];
    let mut failures = Vec::new();
    for case in cases {
        let name = format!(
            "{}.{}",
            case["kind"].as_str().unwrap(),
            case["name"].as_str().unwrap()
        );
        let i = names.iter().position(|n| *n == name).unwrap();
        counts[i] += 1;
        let verdict = catch_unwind(AssertUnwindSafe(|| {
            let e = Native::decode(&case["input"]);
            let args = case["args"]
                .as_array()
                .unwrap()
                .iter()
                .map(Native::decode)
                .collect::<Vec<_>>();
            let arg = |i: usize| args.get(i).cloned().unwrap_or(Native::Undefined);
            assert_eq!(case["initial"]["refreshCount"], 0);
            assert_eq!(case["initial"]["invalidateCount"], 0);
            assert_eq!(case["initial"]["fetchCount"], 0);
            let operations = std::rc::Rc::new(std::cell::Cell::new(0));
            let suspensions = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
            let mut host = Host {
                script: case["calls"].as_array().unwrap().iter().cloned().collect(),
                case: format!("{name} {}", case["label"]),
                objects: HashMap::new(),
                arrays: HashMap::new(),
                iterators: HashMap::new(),
                predicates: HashMap::new(),
                fields: HashMap::from([(
                    "_builderFeeApproved".into(),
                    Native::decode(&case["initial"]["approved"]),
                )]),
                refresh_count: 0,
                invalidate_count: 0,
                fetch_count: 0,
                operations: operations.clone(),
                suspensions: suspensions.clone(),
            };
            let result = match i {
                0 => run(get_builder_info(&mut host, &e)),
                1 => get_order_error_telemetry(&mut host, &e, &arg(0), &arg(1)),
                2 => run(post_order_action(&mut host, &e, &arg(0))),
                3 => validate_response_and_extract_order_ids(&mut host, &e, &arg(0)),
                4 => run(sign_action_or_defer(&mut host, &e, &arg(0), &arg(1))),
                5 => run(schedule_cancel(&mut host, &e, &arg(0))).map(|_| Native::Undefined),
                6 => run(core_set_leverage(&mut host, &e, &arg(0), &arg(1), &arg(2)))
                    .map(|_| Native::Undefined),
                7 => run(core_change_margin_type(&mut host, &e, &arg(0), &arg(1)))
                    .map(|_| Native::Undefined),
                8 => run(spot_set_leverage(&mut host, &e, &arg(0), &arg(1), &arg(2)))
                    .map(|_| Native::Undefined),
                9 => run(spot_change_margin_type(&mut host, &e, &arg(0), &arg(1)))
                    .map(|_| Native::Undefined),
                _ => unreachable!(),
            };
            let actual = match result {
                Ok(v) => json!({"value":host.payload(&v)}),
                Err(e) => json!({"throw":e.native()}),
            };
            assert_eq!(
                actual, case["result"],
                "REQUEST native result/field tree/aliases {name}"
            );
            assert_eq!(
                host.snapshot(),
                case["final"],
                "REQUEST native final state {name}"
            );
            assert!(
                host.script.is_empty(),
                "REQUEST native unconsumed effects {name}"
            );
            let expected = case["calls"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .filter_map(|(i, e)| (e["op"] == "await").then_some(i + 1))
                .collect::<Vec<_>>();
            assert_eq!(
                *suspensions.borrow(),
                expected,
                "REQUEST native await suspension cutpoints {name}"
            );
            pending_counts[i] += suspensions.borrow().len();
        }));
        if verdict.is_err() && !failures.contains(&name) {
            failures.push(name);
        }
    }
    assert_eq!(
        counts,
        [12, 25, 9, 12, 8, 17, 15, 15, 15, 15],
        "independent tenroot fixturecounts"
    );
    assert!(
        failures.is_empty(),
        "REQUEST named native failures:{failures:?}"
    );
    // Independently count valid/rejected awaits from fixture definitions:
    // 3approval +5POST +3sign +15schedule +16eachcoreaction +18eachspotaction.
    assert_eq!(
        pending_counts,
        [3, 0, 5, 0, 3, 15, 16, 16, 18, 18],
        "independent94await-boundary suspensions"
    );
}
