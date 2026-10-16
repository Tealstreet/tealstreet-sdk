// Per-method expectation chains. The arms below name DIFFERENT methods that
// happen to share an expected eager-child count in this corpus; keeping them
// separate keeps each method's expectation independent, so one method changing
// is a one-arm edit rather than a rewrite of a collapsed condition.
#![allow(clippy::if_same_then_else)]
use safe_cex_venue_polymarket::{
    account_reads::*, order_adapters::Completion, ws_dispatch::Runtime,
};
use serde_json::{Value as J, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    process::Command,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
#[derive(Clone, Debug)]
enum V {
    U,
    Null,
    Bool(bool),
    Number(f64),
    Text(Vec<u16>),
    Object(Option<u32>, Rc<RefCell<Vec<(String, V)>>>),
    Array(Option<u32>, Rc<RefCell<Vec<Option<V>>>>),
}
#[derive(Debug)]
struct Failure {
    marker: Option<u32>,
    message: V,
}
type E = Rc<Failure>;
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn text(s: &str) -> V {
    V::Text(units(s))
}
fn object(id: Option<u32>, fields: Vec<(String, V)>) -> V {
    V::Object(id, Rc::new(RefCell::new(fields)))
}
fn list(id: Option<u32>, slots: Vec<Option<V>>) -> V {
    V::Array(id, Rc::new(RefCell::new(slots)))
}
fn decode(j: &J) -> V {
    match j["kind"].as_str().unwrap() {
        "undefined" => V::U,
        "null" => V::Null,
        "bool" => V::Bool(j["value"].as_bool().unwrap()),
        "number" => V::Number(f64::from_bits(
            u64::from_str_radix(j["bits"].as_str().unwrap(), 16).unwrap(),
        )),
        "text" => V::Text(
            j["units"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| u16::try_from(x.as_u64().unwrap()).unwrap())
                .collect(),
        ),
        "object" => object(
            j["id"].as_u64().map(|v| v as u32),
            j["fields"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| (x[0].as_str().unwrap().into(), decode(&x[1])))
                .collect(),
        ),
        "array" => list(
            j["id"].as_u64().map(|v| v as u32),
            j["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| {
                    if x["kind"] == "hole" {
                        None
                    } else {
                        Some(decode(x))
                    }
                })
                .collect(),
        ),
        other => panic!("native fixture {other}"),
    }
}
fn raw_field(v: &V, key: &str) -> V {
    if let V::Object(_, f) = v {
        f.borrow()
            .iter()
            .find(|(k, _)| k == key)
            .map_or(V::U, |(_, v)| v.clone())
    } else {
        V::U
    }
}
fn field_set(v: &V, key: &str, value: V) {
    let V::Object(_, f) = v else {
        panic!("native object")
    };
    let mut f = f.borrow_mut();
    if let Some((_, v)) = f.iter_mut().find(|(k, _)| k == key) {
        *v = value
    } else {
        f.push((key.into(), value));
    }
}
fn numeric(v: &V) -> f64 {
    match v {
        V::Number(n) => *n,
        V::Null => 0.,
        V::U => f64::NAN,
        V::Bool(b) => u8::from(*b) as f64,
        V::Text(s) => String::from_utf16_lossy(s).parse().unwrap_or(f64::NAN),
        _ => f64::NAN,
    }
}
fn project(v: &V) -> J {
    match v {
        V::U => json!({"kind":"undefined"}),
        V::Null => json!({"kind":"null"}),
        V::Bool(b) => json!({"kind":"bool","value":b}),
        V::Number(n) => json!({"kind":"number","bits":format!("{:016x}",n.to_bits())}),
        V::Text(t) => json!({"kind":"text","units":t}),
        V::Object(id, f) => {
            json!({"kind":"object","id":id,"fields":f.borrow().iter().map(|(k,v)|json!([k,project(v)])).collect::<Vec<_>>()})
        }
        V::Array(id, slots) => {
            json!({"kind":"array","id":id,"items":slots.borrow().iter().map(|v|v.as_ref().map_or(json!({"kind":"hole"}),project)).collect::<Vec<_>>()})
        }
    }
}
fn same(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Object(_, a), V::Object(_, b)) => Rc::ptr_eq(a, b),
        (V::Array(_, a), V::Array(_, b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
struct Op {
    ready: Cell<bool>,
    result: Result<V, E>,
    tag: &'static str,
}
impl Completion<V, E> for Op {
    fn poll(&self, _: &mut Context<'_>) -> Poll<Result<V, E>> {
        if self.ready.get() {
            Poll::Ready(self.result.clone())
        } else {
            Poll::Pending
        }
    }
}
struct Call {
    client: &'static str,
    method: &'static str,
    args: Vec<V>,
}
struct Host {
    name: String,
    i: usize,
    fixture: J,
    credentials: V,
    wallet: V,
    exchange_credentials: RefCell<V>,
    input: V,
    order: V,
    replies: Vec<V>,
    error: E,
    calls: RefCell<Vec<Call>>,
    logs: RefCell<Vec<(Vec<u16>, &'static str)>>,
    serializer: RefCell<Vec<Vec<V>>>,
    tasks: RefCell<Vec<Pin<Box<dyn Future<Output = ()>>>>>,
    ops: RefCell<Vec<Rc<Op>>>,
    completed: Cell<usize>,
    derivations: Cell<usize>,
    balance_calls: Cell<usize>,
    key_reads: Cell<usize>,
    captures: RefCell<Vec<(&'static str, &'static str)>>,
    assignments: RefCell<Vec<(&'static str, V)>>,
    ready: Cell<bool>,
    logger_changed: Cell<bool>,
    captured_logger_old: Cell<bool>,
    resolves: Cell<usize>,
    resolve_target: RefCell<Option<V>>,
}
impl Host {
    fn new(case: &J) -> Self {
        let f = case["fixture"].clone();
        let i = case["i"].as_u64().unwrap() as usize;
        let message = if i == 9 {
            text("")
        } else if i == 10 {
            V::Number(0.)
        } else if i == 11 && case["name"] == "validateAccount" {
            object(Some(60), vec![("marker".into(), V::Number(60.))])
        } else if i == 11 {
            V::Text(vec![0xd800, 32, 102, 97, 105, 108, 117, 114, 101])
        } else {
            text("original failure")
        };
        Self {
            name: case["name"].as_str().unwrap().into(),
            i,
            credentials: decode(&f["credentials"]),
            wallet: decode(&f["wallet"]),
            exchange_credentials: RefCell::new(decode(&f["exchangeCredentials"])),
            input: decode(&f["input"]),
            order: decode(&f["order"]),
            replies: f["replies"]
                .as_array()
                .unwrap()
                .iter()
                .map(decode)
                .collect(),
            error: Rc::new(Failure {
                marker: Some(47),
                message,
            }),
            fixture: f,
            calls: RefCell::new(vec![]),
            logs: RefCell::new(vec![]),
            serializer: RefCell::new(vec![]),
            tasks: RefCell::new(vec![]),
            ops: RefCell::new(vec![]),
            completed: Cell::new(0),
            derivations: Cell::new(0),
            balance_calls: Cell::new(0),
            key_reads: Cell::new(0),
            captures: RefCell::new(vec![]),
            assignments: RefCell::new(vec![]),
            ready: Cell::new(false),
            logger_changed: Cell::new(false),
            captured_logger_old: Cell::new(false),
            resolves: Cell::new(0),
            resolve_target: RefCell::new(None),
        }
    }
    fn drive(&self) {
        let tasks = std::mem::take(&mut *self.tasks.borrow_mut());
        let mut keep = vec![];
        let mut cx = Context::from_waker(Waker::noop());
        for mut task in tasks {
            if task.as_mut().poll(&mut cx).is_pending() {
                keep.push(task)
            }
        }
        self.tasks.borrow_mut().extend(keep);
    }
    fn finish_one(&self) {
        let op = self.ops.borrow_mut().remove(0);
        self.completed.set(self.completed.get() + 1);
        if op.tag == "deriveApiCredentials" && op.result.is_ok() && self.i != 5 {
            *self.exchange_credentials.borrow_mut() =
                object(Some(22), vec![("apiKey".into(), text("derived"))]);
        }
        op.ready.set(true);
    }
    fn finish_all(&self) {
        for _ in 0..80 {
            if !self.ops.borrow().is_empty() {
                self.finish_one();
            }
            self.drive();
            if self.tasks.borrow().is_empty() {
                return;
            }
        }
        panic!("native 80 scheduler iteration ceiling")
    }
    fn operation(&self, result: Result<V, E>, tag: &'static str) -> Rc<Op> {
        let op = Rc::new(Op {
            ready: Cell::new(self.ready.get()),
            result,
            tag,
        });
        if !op.ready.get() {
            self.ops.borrow_mut().push(op.clone())
        } else {
            self.completed.set(self.completed.get() + 1);
        }
        op
    }
}
impl Runtime for Host {
    type Value = V;
    type Error = E;
    fn undefined(&self) -> V {
        V::U
    }
    fn nullish(&self, v: &V) -> bool {
        matches!(v, V::U | V::Null)
    }
    fn truthy(&self, v: &V) -> bool {
        match v {
            V::U | V::Null => false,
            V::Bool(b) => *b,
            V::Number(n) => *n != 0. && !n.is_nan(),
            V::Text(s) => !s.is_empty(),
            _ => true,
        }
    }
    fn is_text(&self, v: &V, s: &str) -> bool {
        matches!(v,V::Text(t) if *t==units(s))
    }
    fn field(&self, v: &V, key: &str) -> Result<V, E> {
        if self.nullish(v) {
            return Err(self.new_error("nullish property read"));
        }
        if self.name == "postOrders"
            && same(v, &self.credentials)
            && key == "apiKey"
            && (self.i == 7 || self.i == 9)
        {
            let n = self.key_reads.get() + 1;
            self.key_reads.set(n);
            assert!(
                self.captures.borrow().contains(&("xhr", "post")),
                "native post callable captured before map getter"
            );
            if self.i == 9 && n == 1 {
                if let V::Array(_, slots) = &self.input {
                    let mut slots = slots.borrow_mut();
                    slots[1] = None;
                    slots.push(Some(object(
                        Some(16),
                        vec![
                            ("order".into(), object(Some(75), vec![])),
                            ("orderType".into(), text("FOK")),
                        ],
                    )));
                }
            }
            return Ok(text(&format!("owner-live-{n}")));
        }
        Ok(raw_field(v, key))
    }
    fn string(&self, v: &V) -> Result<Vec<u16>, E> {
        if matches!(v, V::Object(Some(60), _)) {
            self.logger_changed.set(true);
        }
        Ok(match v {
            V::Text(t) => t.clone(),
            V::U => units("undefined"),
            V::Null => units("null"),
            V::Bool(b) => units(if *b { "true" } else { "false" }),
            V::Number(n) => units(&safe_cex_num::js_number_string(*n)),
            _ => units("[object Object]"),
        })
    }
    fn log(&self, message: &[u16], severity: &'static str) -> Result<(), E> {
        self.logs.borrow_mut().push((message.to_vec(), severity));
        if self.i == 10 && self.name == "validateAccount" {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
}
impl AccountHost for Host {
    type Callable = (&'static str, &'static str);
    type Operation = Op;
    fn start_task(&self, mut task: Pin<Box<dyn Future<Output = ()>>>) {
        let mut cx = Context::from_waker(Waker::noop());
        if task.as_mut().poll(&mut cx).is_pending() {
            self.tasks.borrow_mut().push(task)
        }
    }
    fn capture(&self, r: &'static str, m: &'static str) -> Result<Self::Callable, E> {
        self.captures.borrow_mut().push((r, m));
        if m == "log" {
            self.captured_logger_old.set(!self.logger_changed.get());
        }
        Ok((r, m))
    }
    fn invoke(&self, (client, method): Self::Callable, args: Vec<V>) -> Result<Rc<Op>, E> {
        let at = self.calls.borrow().len();
        self.calls.borrow_mut().push(Call {
            client,
            method,
            args,
        });
        let (result, fails) = match method {
            "getAddress" => (decode(&self.fixture["address"]), self.i == 8),
            "deriveApiCredentials" => {
                self.derivations.set(self.derivations.get() + 1);
                (V::U, self.i == 6)
            }
            "getBalanceAllowance" => {
                self.balance_calls.set(self.balance_calls.get() + 1);
                (object(Some(210), vec![]), self.i >= 7)
            }
            _ => {
                assert!(at < self.replies.len(), "native fixture page ceiling");
                (
                    object(None, vec![("data".into(), self.replies[at].clone())]),
                    self.fixture_error_at() == Some(at),
                )
            }
        };
        Ok(self.operation(
            if fails {
                Err(self.error.clone())
            } else {
                Ok(result)
            },
            method,
        ))
    }
    fn invoke_log(
        &self,
        callable: Self::Callable,
        message: &[u16],
        severity: &'static str,
    ) -> Result<(), E> {
        assert_eq!(callable, ("exchange", "log"));
        let severity = if self.captured_logger_old.get() {
            severity
        } else {
            "changed"
        };
        self.log(message, severity)
    }
    fn resolve(&self, value: V) -> Result<Rc<Op>, E> {
        self.resolves.set(self.resolves.get() + 1);
        if let Some(target) = self.resolve_target.borrow_mut().take() {
            return Ok(self.operation(Ok(target), "returnThenable"));
        }
        Ok(Rc::new(Op {
            ready: Cell::new(true),
            result: Ok(value),
            tag: "resolve",
        }))
    }
    fn credentials(&self) -> Result<V, E> {
        Ok(self.credentials.clone())
    }
    fn wallet(&self) -> Result<V, E> {
        Ok(self.wallet.clone())
    }
    fn exchange_credentials(&self) -> Result<V, E> {
        Ok(self.exchange_credentials.borrow().clone())
    }
    fn signer_type(&self) -> Result<V, E> {
        Ok(decode(&self.fixture["signer"]))
    }
    fn text(&self, t: &'static str) -> V {
        text(t)
    }
    fn number(&self, n: f64) -> V {
        V::Number(n)
    }
    fn null(&self) -> V {
        V::Null
    }
    fn object(&self, fields: Vec<(&'static str, V)>) -> Result<V, E> {
        Ok(object(
            None,
            fields.into_iter().map(|(k, v)| (k.into(), v)).collect(),
        ))
    }
    fn assign(&self, object: &V, key: &'static str, value: V) -> Result<(), E> {
        if self.i == 11 && matches!(self.name.as_str(), "getOpenOrders" | "getTrades") {
            self.assignments.borrow_mut().push((key, value));
        } else {
            field_set(object, key, value);
        }
        Ok(())
    }
    fn spread_params(&self, source: &V, limit: V, offset: V) -> Result<V, E> {
        let mut fields = if let V::Object(_, f) = source {
            f.borrow().clone()
        } else {
            vec![]
        };
        for (k, v) in [("limit", limit), ("offset", offset)] {
            if let Some((_, value)) = fields.iter_mut().find(|(key, _)| key == k) {
                *value = v
            } else {
                fields.push((k.into(), v));
            }
        }
        Ok(object(None, fields))
    }
    fn array(&self, slots: Vec<Option<V>>) -> V {
        list(None, slots)
    }
    fn map_output(&self, _: &V, length: usize) -> Result<V, E> {
        Ok(list(None, vec![None; length]))
    }
    fn map_set(&self, output: &V, index: usize, value: V) -> Result<(), E> {
        let V::Array(_, slots) = output else {
            return Err(self.new_error("invalid map output"));
        };
        slots.borrow_mut()[index] = Some(value);
        Ok(())
    }
    fn is_array(&self, v: &V) -> bool {
        matches!(v, V::Array(..))
    }
    fn length(&self, v: &V) -> Result<usize, E> {
        if let V::Array(_, s) = v {
            Ok(s.borrow().len())
        } else {
            Err(self.new_error("invalid array length"))
        }
    }
    fn at(&self, v: &V, i: usize) -> Result<Option<V>, E> {
        if let V::Array(_, s) = v {
            Ok(s.borrow()[i].clone())
        } else {
            Err(self.new_error("invalid array slot"))
        }
    }
    fn push_spread(&self, array: &V, iterable: &V) -> Result<(), E> {
        let V::Array(_, out) = array else {
            return Err(self.new_error("invalid push receiver"));
        };
        let V::Array(_, input) = iterable else {
            return Err(self.new_error("invalid iterable"));
        };
        out.borrow_mut().extend(
            input
                .borrow()
                .iter()
                .map(|v| Some(v.clone().unwrap_or(V::U))),
        );
        Ok(())
    }
    fn less(&self, a: &V, b: &V) -> Result<bool, E> {
        Ok(numeric(a) < numeric(b))
    }
    fn greater_equal(&self, a: &V, b: &V) -> Result<bool, E> {
        Ok(numeric(a) >= numeric(b))
    }
    fn add(&self, a: &V, b: &V) -> Result<V, E> {
        Ok(V::Number(numeric(a) + numeric(b)))
    }
    fn slice(&self, array: &V, end: &V) -> Result<V, E> {
        let V::Array(_, s) = array else {
            return Err(self.new_error("invalid slice"));
        };
        let s = s.borrow();
        let n = numeric(end);
        let integer = if n.is_nan() { 0. } else { n.trunc() };
        let end = if integer < 0. {
            (s.len() as f64 + integer).max(0.)
        } else {
            integer.min(s.len() as f64)
        } as usize;
        Ok(list(None, s[..end].to_vec()))
    }
    fn order_to_json(&self, order: V, owner: V, kind: V) -> Result<V, E> {
        self.serializer
            .borrow_mut()
            .push(vec![order.clone(), owner.clone(), kind.clone()]);
        Ok(object(
            Some(90),
            vec![
                ("serialized".into(), order),
                ("owner".into(), owner),
                ("orderType".into(), kind),
            ],
        ))
    }
    fn new_error(&self, message: &'static str) -> E {
        Rc::new(Failure {
            marker: None,
            message: text(message),
        })
    }
    fn error_message(&self, error: &E) -> Result<V, E> {
        Ok(error.message.clone())
    }
}
impl Host {
    fn fixture_error_at(&self) -> Option<usize> {
        match self.name.as_str() {
            "postOrder" | "postOrders" if self.i == 8 => Some(0),
            "getOpenOrders" | "getTrades" | "getPositions" if self.i == 7 => Some(1),
            "getGammaMarkets" if (4..=7).contains(&self.i) => Some(1),
            _ => None,
        }
    }
}
fn oracle() -> &'static J {
    static DATA: OnceLock<J> = OnceLock::new();
    DATA.get_or_init(|| {
        let out = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/account_reads_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "actual eight account bodies: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let data: J = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(
            data["cases"].as_array().unwrap().len(),
            96,
            "independent source account fixture floor"
        );
        data
    })
}
fn method(h: Rc<Host>) -> Pin<Box<dyn Future<Output = Result<V, E>>>> {
    let f = &h.fixture;
    match h.name.as_str() {
        "postOrder" => Box::pin(post_order(h.clone(), h.order.clone(), decode(&f["kind"]))),
        "postOrders" => Box::pin(post_orders(h.clone(), h.input.clone())),
        "getOpenOrders" => Box::pin(get_open_orders(h.clone(), decode(&f["market"]))),
        "getTrades" => Box::pin(get_trades(
            h.clone(),
            decode(&f["market"]),
            decode(&f["limit"]),
        )),
        "getGammaMarkets" => Box::pin(get_gamma_markets(h.clone(), decode(&f["params"]))),
        "getPositions" => Box::pin(get_positions(h.clone(), decode(&f["address"]))),
        "getAccount" => Box::pin(get_account(h)),
        "validateAccount" => Box::pin(validate_account(h)),
        _ => panic!("method"),
    }
}
fn independent_count(name: &str, i: usize) -> usize {
    match name {
        "getOpenOrders" => match i {
            0 | 3 => 3,
            1 | 2 => 0,
            10 => 1,
            11 => 4,
            _ => 5,
        },
        "getTrades" => match i {
            0 | 3 => 3,
            1 | 2 | 8 | 11 => 0,
            4 | 5 | 10 => 1,
            9 => 2,
            _ => 5,
        },
        "getGammaMarkets" => match i {
            0 | 1 => 501,
            2 | 3 => 3,
            _ => 5,
        },
        "getPositions" => match i {
            1 => 0,
            2 => 1,
            _ => 1001,
        },
        _ => panic!("array family"),
    }
}
fn fixture_text(i: usize) -> Vec<u16> {
    match i {
        2 => vec![0xd83e, 0xdd80, 0xd800],
        3 => vec![0xdfff],
        4 => vec![0],
        _ => units(
            [
                "market/one",
                "",
                "",
                "",
                "",
                "x/y?z",
                "#",
                "__proto__",
                "0",
                "constructor",
                " final ",
                "tail",
            ][i],
        ),
    }
}
fn assert_keys(v: &V, keys: &[&str], label: &str) {
    let V::Object(_, fields) = v else {
        panic!("native own-key object {label}")
    };
    let fields = fields.borrow();
    assert_eq!(
        fields.len(),
        keys.len(),
        "native independent own-key count {label}"
    );
    for (index, key) in keys.iter().enumerate() {
        assert_eq!(
            fields[index].0, *key,
            "native own-key order {label}/{index}"
        );
    }
}
fn assert_text(v: &V, expected: &[u16], label: &str) {
    let V::Text(t) = v else {
        panic!("native text arm {label}")
    };
    assert_eq!(t, expected, "native text {label}");
}
fn assert_number(v: &V, expected: f64, label: &str) {
    let V::Number(n) = v else {
        panic!("native Number arm {label}")
    };
    if expected.is_nan() {
        assert!(n.is_nan(), "native NaN {label}")
    } else {
        assert_eq!(
            n.to_bits(),
            expected.to_bits(),
            "native Number bits {label}"
        )
    }
}
fn assert_native(h: &Host, result: &Result<V, E>) {
    let name = h.name.as_str();
    let i = h.i;
    let missing = name.starts_with("post") && (i == 1 || i == 2);
    let expected_error = missing
        || (name.starts_with("post") && i == 8)
        || (["getOpenOrders", "getTrades", "getPositions"].contains(&name) && i == 7)
        || (name == "getGammaMarkets" && (4..=7).contains(&i))
        || (name == "getAccount" && i == 8)
        || (name == "validateAccount" && i == 10);
    assert_eq!(result.is_err(), expected_error, "native outcome {name}/{i}");
    if let Err(error) = result {
        if missing {
            assert_eq!(error.marker, None);
            assert_text(
                &error.message,
                &units("Credentials not initialized"),
                "credentials refusal",
            );
            assert_eq!(
                h.calls.borrow().len(),
                0,
                "native missing credentials stops dispatch"
            );
        } else {
            assert!(
                Rc::ptr_eq(error, &h.error),
                "native original error identity {name}/{i}"
            );
        }
    }
    if let Ok(out) = result {
        assert_eq!(
            h.resolves.get(),
            1,
            "native final async return resolution {name}/{i}"
        );
        if [
            "getOpenOrders",
            "getTrades",
            "getGammaMarkets",
            "getPositions",
        ]
        .contains(&name)
        {
            let V::Array(id, slots) = out else {
                panic!("native array result {name}")
            };
            let count = independent_count(name, i);
            assert_eq!(
                slots.borrow().len(),
                count,
                "native pagination count {name}/{i}"
            );
            let direct =
                (name == "getOpenOrders" || name == "getTrades") && (i == 0 || i == 3 || i == 10);
            if direct {
                assert!(
                    same(out, &h.replies[h.calls.borrow().len() - 1]),
                    "native direct page identity {name}/{i}"
                );
                assert_eq!(*id, Some(if i == 10 { 202 } else { 200 }));
            } else {
                assert_eq!(*id, None, "native fresh accumulated array {name}/{i}");
            }
            for (index, slot) in slots.borrow().iter().enumerate() {
                let row = slot.as_ref().expect("native dense iterator output");
                let expected = if i == 10 && matches!(name, "getOpenOrders" | "getTrades") {
                    400
                } else if name == "getPositions" {
                    if index < 500 {
                        300 + index
                    } else if index < 1000 {
                        900 + index - 500
                    } else {
                        1500
                    }
                } else if name == "getGammaMarkets" && (i == 0 || i == 1) {
                    if index < 500 { 300 + index } else { 900 }
                } else if (name == "getOpenOrders" && i == 11)
                    || (name == "getGammaMarkets" && i == 3)
                {
                    if index < 3 { 300 + index } else { 400 }
                } else {
                    if index < 2 || ((i == 0 || i == 3) && name != "getGammaMarkets") {
                        300 + index
                    } else if index < 4 {
                        400 + index - 2
                    } else {
                        500
                    }
                };
                let V::Object(marker, _) = row else {
                    panic!("native row object")
                };
                assert_eq!(
                    *marker,
                    Some(expected as u32),
                    "native decoded row marker {name}/{i}/{index}"
                );
                assert_number(
                    &raw_field(row, "marker"),
                    expected as f64,
                    "row marker field",
                );
                assert_keys(row, &["marker", "text", "number"], "decoded page row");
                assert_number(&raw_field(row, "number"), -0., "row signed zero");
                assert_text(
                    &raw_field(row, "text"),
                    &fixture_text(expected % 12),
                    "row text UTF-16",
                );
                let original = h
                    .replies
                    .iter()
                    .find_map(|reply| {
                        let rows = if matches!(reply, V::Array(..)) {
                            reply.clone()
                        } else {
                            raw_field(reply, "data")
                        };
                        if let V::Array(_, s) = rows {
                            s.borrow().iter().flatten().find(|v| same(v, row)).cloned()
                        } else {
                            None
                        }
                    })
                    .expect("native original row alias");
                field_set(row, "fromResult", V::Number(17.));
                assert_number(
                    &raw_field(&original, "fromResult"),
                    17.,
                    "native response element two-way alias",
                );
                if let V::Object(_, f) = row {
                    f.borrow_mut().retain(|(k, _)| k != "fromResult");
                }
            }
        } else if name.starts_with("post") {
            assert!(
                same(out, &h.replies[0]),
                "native submitted data alias {name}/{i}"
            );
            assert_keys(out, &["marker", "text", "number"], "submitted data");
            assert_number(&raw_field(out, "marker"), 200., "submitted marker");
            assert_number(&raw_field(out, "number"), -0., "submitted signed zero");
            assert_text(
                &raw_field(out, "text"),
                &fixture_text(200 % 12),
                "submitted text UTF-16",
            );
        } else if name == "getAccount" {
            let address = if i == 1 || i == 2 {
                text("unknown")
            } else {
                decode(&h.fixture["address"])
            };
            let V::Text(t) = address else {
                panic!("fixture address")
            };
            assert_keys(out, &["userId", "affiliateId"], "account result");
            assert_text(&raw_field(out, "userId"), &t, "account userId");
            assert!(
                matches!(raw_field(out, "affiliateId"), V::Null),
                "native account affiliateId null"
            );
        } else {
            let expected = if i == 1 || i == 2 || i == 5 {
                text("No wallet configured")
            } else if i == 6 {
                text("original failure")
            } else if i >= 7 {
                if h.truthy(&h.error.message) {
                    h.error.message.clone()
                } else {
                    text("Failed to validate account")
                }
            } else {
                text("")
            };
            assert_keys(out, &["error"], "validation result");
            if let V::Text(t) = expected {
                assert_text(&raw_field(out, "error"), &t, "validation error field");
            } else {
                assert!(
                    same(&raw_field(out, "error"), &h.error.message),
                    "native validation object error message alias"
                );
                assert_number(
                    &raw_field(&raw_field(out, "error"), "marker"),
                    60.,
                    "native object error marker",
                );
            }
            if i == 6 || i >= 7 {
                assert_eq!(h.logs.borrow().len(), 1, "native validation logger count");
                assert_eq!(
                    h.logs.borrow()[0].1,
                    "error",
                    "native captured logger identity"
                );
            }
            assert_eq!(
                h.derivations.get(),
                usize::from(matches!(i, 4..=6)),
                "native optional derivation count"
            );
            assert_eq!(
                h.balance_calls.get(),
                usize::from(!matches!(i, 1 | 2 | 5 | 6)),
                "native validation balance count"
            );
        }
    }
    let expected_calls = match name {
        "postOrder" | "postOrders" => usize::from(!missing),
        "getAccount" => usize::from(!matches!(i, 1 | 2)),
        "validateAccount" => {
            if matches!(i, 1 | 2) {
                0
            } else if i == 4 {
                2
            } else {
                1
            }
        }
        "getOpenOrders" => {
            if matches!(i, 0..=3) {
                1
            } else if matches!(i, 7 | 10 | 11) {
                2
            } else {
                3
            }
        }
        "getTrades" => {
            if matches!(i, 0 | 1 | 2 | 3 | 4 | 5 | 8 | 9 | 11) {
                1
            } else if matches!(i, 7 | 10) {
                2
            } else {
                3
            }
        }
        "getGammaMarkets" => {
            if i == 3 {
                1
            } else if i <= 7 {
                2
            } else {
                3
            }
        }
        "getPositions" => {
            if matches!(i, 1 | 2) {
                1
            } else if i == 7 {
                2
            } else {
                3
            }
        }
        _ => panic!("family"),
    };
    assert_eq!(
        h.calls.borrow().len(),
        expected_calls,
        "native independent child count {name}/{i}"
    );
    if i == 11 && matches!(name, "getOpenOrders" | "getTrades") {
        let count = 2;
        assert_eq!(
            h.assignments.borrow().len(),
            count,
            "native inherited setter count"
        );
        for (index, (key, value)) in h.assignments.borrow().iter().enumerate() {
            assert_eq!(
                *key,
                if name == "getTrades" && index == 1 {
                    "limit"
                } else {
                    "market"
                },
                "native assignment key"
            );
            if *key == "limit" {
                assert_number(value, 0.5, "inherited limit setter value");
            } else {
                assert_text(value, &units("tail"), "inherited market setter value");
            }
        }
    }
    // Native request assertions precede every projection/oracle comparison.
    for (call_index, call) in h.calls.borrow().iter().enumerate() {
        if [
            "getOpenOrders",
            "getTrades",
            "getGammaMarkets",
            "getPositions",
        ]
        .contains(&name)
        {
            let (client, url) = match name {
                "getOpenOrders" => ("xhr", "/data/orders"),
                "getTrades" => ("xhr", "/data/trades"),
                "getGammaMarkets" => ("gammaXhr", "/markets"),
                _ => ("dataXhr", "/positions"),
            };
            assert_eq!(call.client, client, "native client {name}");
            assert_eq!(call.method, "get", "native method {name}");
            assert_text(&call.args[0], &units(url), "read URL");
            assert_keys(&call.args[1], &["params"], "read config");
            let params = raw_field(&call.args[1], "params");
            if name == "getPositions" {
                assert_keys(
                    &params,
                    &["user", "limit", "offset", "sizeThreshold"],
                    "positions params",
                );
                assert_number(&raw_field(&params, "limit"), 500., "positions limit");
                assert_number(
                    &raw_field(&params, "offset"),
                    (call_index * 500) as f64,
                    "positions offset",
                );
                assert_number(
                    &raw_field(&params, "sizeThreshold"),
                    0.,
                    "positions threshold",
                );
                let V::Text(expected) = decode(&h.fixture["address"]) else {
                    panic!("fixture")
                };
                assert_text(&raw_field(&params, "user"), &expected, "positions user");
            } else if name == "getOpenOrders" || name == "getTrades" {
                let mut keys = vec!["next_cursor"];
                if i >= 2 && i != 11 {
                    keys.push("market");
                    assert_text(
                        &raw_field(&params, "market"),
                        &fixture_text(i),
                        "cursor market",
                    );
                }
                if name == "getTrades" && matches!(i, 3 | 4 | 5 | 7 | 8 | 9 | 10) {
                    keys.push("limit");
                    let expected = match i {
                        3 => 1.,
                        4 => 1.5,
                        5 => -1.,
                        7 => f64::INFINITY,
                        8 => f64::NEG_INFINITY,
                        9 => 2.,
                        10 => 3.,
                        _ => unreachable!(),
                    };
                    assert_number(&raw_field(&params, "limit"), expected, "trades limit");
                }
                assert_keys(&params, &keys, "cursor params");
                let cursor = if call_index == 0 {
                    "MA=="
                } else if call_index == 1 {
                    "cursor/2"
                } else {
                    "cursor/3"
                };
                assert_text(
                    &raw_field(&params, "next_cursor"),
                    &units(cursor),
                    "cursor native value",
                );
            } else {
                let keys = if i == 0 {
                    vec!["limit", "offset"]
                } else if i == 1 {
                    vec!["limit", "offset", "active", "closed"]
                } else {
                    vec!["active", "closed", "archived", "limit", "offset"]
                };
                assert_keys(&params, &keys, "gamma params");
                if i >= 1 {
                    assert!(
                        matches!(raw_field(&params,"active"),V::Bool(b) if b==(i!=1&&i.is_multiple_of(2))),
                        "native copied active flag"
                    );
                    assert!(
                        matches!(raw_field(&params,"closed"),V::Bool(b) if b==(i==1)),
                        "native copied closed flag"
                    );
                }
                if i >= 2 {
                    assert!(
                        matches!(raw_field(&params, "archived"), V::U),
                        "native copied archived undefined"
                    );
                }
                let expected_limit = if i <= 1 {
                    500.
                } else if i == 2 {
                    1.5
                } else if i == 3 {
                    f64::INFINITY
                } else if i == 4 {
                    0.
                } else if i == 5 {
                    -1.
                } else if i == 6 {
                    f64::NAN
                } else if i == 7 {
                    f64::NEG_INFINITY
                } else {
                    2.
                };
                assert_number(&raw_field(&params, "limit"), expected_limit, "gamma limit");
                let offset = if i <= 1 {
                    0.
                } else if i == 8 {
                    -0.
                } else if i == 9 {
                    -2.
                } else {
                    3.
                };
                let expected_offset = if call_index == 0 {
                    offset
                } else {
                    offset + expected_limit * call_index as f64
                };
                assert_number(
                    &raw_field(&params, "offset"),
                    expected_offset,
                    "gamma offset",
                );
            }
        } else if name == "postOrder" {
            assert_eq!(call.client, "xhr");
            assert_eq!(call.method, "post");
            assert_text(&call.args[0], &units("/order"), "postOrder URL");
            assert_keys(
                &call.args[1],
                &["serialized", "owner", "orderType"],
                "serialized payload",
            );
            assert!(
                same(&raw_field(&call.args[1], "serialized"), &h.order),
                "native serializer order alias"
            );
            let V::Text(owner) = raw_field(&h.credentials, "apiKey") else {
                panic!("credentials")
            };
            assert_text(
                &raw_field(&call.args[1], "owner"),
                &owner,
                "postOrder owner",
            );
            let expected = decode(&h.fixture["kind"]);
            let V::Text(expected) = expected else {
                panic!("kind")
            };
            assert_text(
                &raw_field(&call.args[1], "orderType"),
                &expected,
                "postOrder orderType",
            );
            assert_eq!(
                h.serializer.borrow().len(),
                1,
                "native serializer called once"
            );
        } else if name == "postOrders" {
            assert_eq!(call.client, "xhr");
            assert_eq!(call.method, "post");
            assert_text(&call.args[0], &units("/orders"), "postOrders URL");
            assert_keys(&call.args[1], &["orders"], "batch payload");
            let mapped = raw_field(&call.args[1], "orders");
            assert!(!same(&mapped, &h.input), "native fresh mapped payload");
            let V::Array(_, slots) = mapped else {
                panic!("native mapped array")
            };
            let slots = slots.borrow();
            assert_eq!(
                slots.len(),
                if i == 4 { 0 } else { 3 },
                "native fixed Array.map length"
            );
            for (index, slot) in slots.iter().enumerate() {
                let hole = (i == 3 || i == 9) && index == 1;
                assert_eq!(slot.is_none(), hole, "native map hole {i}/{index}");
                if let Some(row) = slot {
                    assert_keys(row, &["order", "owner", "orderType"], "batch row");
                    let V::Array(_, input) = &h.input else {
                        panic!("native original batch")
                    };
                    let item = input.borrow()[index].clone().unwrap();
                    assert!(!same(row, &item), "native mapped row fresh identity");
                    assert!(
                        same(&raw_field(row, "order"), &raw_field(&item, "order")),
                        "native nested order original pointer alias"
                    );
                    let owner = if i == 7 || i == 9 {
                        format!(
                            "owner-live-{}",
                            if i == 9 && index == 2 { 2 } else { index + 1 }
                        )
                    } else {
                        let V::Text(t) = raw_field(&h.credentials, "apiKey") else {
                            panic!("owner")
                        };
                        String::from_utf16_lossy(&t)
                    };
                    let expected = if i == 7 || i == 9 {
                        units(&owner)
                    } else {
                        let V::Text(t) = raw_field(&h.credentials, "apiKey") else {
                            panic!("owner")
                        };
                        t
                    };
                    assert_text(&raw_field(row, "owner"), &expected, "postOrders owner");
                    let V::Object(id, _) = raw_field(row, "order") else {
                        panic!("native nested order")
                    };
                    assert_eq!(
                        id,
                        Some(if index == 1 { 73 } else { 71 }),
                        "native nested order identity"
                    );
                    let expected_kind = match index {
                        0 => "GTC",
                        1 => "FAK",
                        _ => "GTD",
                    };
                    assert_text(
                        &raw_field(row, "orderType"),
                        &units(expected_kind),
                        "postOrders kind",
                    );
                }
            }
        } else if call.method == "getBalanceAllowance" {
            let V::Number(n) = decode(&h.fixture["signer"]) else {
                panic!("signer")
            };
            assert_number(&call.args[0], n, "validation signer type");
        }
    }
}
fn family(name: &str) {
    let mut comparisons = vec![];
    for i in 0..12 {
        let case = oracle()["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name && c["i"] == i)
            .unwrap();
        let h = Rc::new(Host::new(case));
        let mut observer = method(h.clone());
        let eager = h.calls.borrow().len();
        let expected = if name.starts_with("post") && (i == 1 || i == 2) {
            0
        } else if name == "getAccount" && (i == 1 || i == 2)
            || name == "validateAccount" && (i == 1 || i == 2)
        {
            0
        } else {
            1
        };
        assert_eq!(eager, expected, "native eager child count {name}/{i}");
        h.finish_all();
        let mut cx = Context::from_waker(Waker::noop());
        let Poll::Ready(result) = observer.as_mut().poll(&mut cx) else {
            panic!("native completion")
        };
        assert_native(&h, &result);
        comparisons.push((h, result, case));
    }
    // Every native assertion in this family finishes before encoded oracle projections.
    for (h, result, case) in comparisons {
        let projected = match result {
            Ok(v) => json!({"value":project(&v)}),
            Err(e) => json!({"error":{"marker":e.marker,"message":project(&e.message)}}),
        };
        assert_eq!(projected, case["result"], "source result {name}/{}", h.i);
        assert_eq!(
            json!(
                h.assignments
                    .borrow()
                    .iter()
                    .map(|(k, v)| json!([k, project(v)]))
                    .collect::<Vec<_>>()
            ),
            case["assignments"],
            "source inherited setters"
        );
        let calls: Vec<J> = h
            .calls
            .borrow()
            .iter()
            .map(|c| {
                json!([
                    c.client,
                    c.method,
                    c.args.iter().map(project).collect::<Vec<_>>()
                ])
            })
            .collect();
        assert_eq!(json!(calls), case["calls"], "source calls {name}/{}", h.i);
        assert_eq!(
            json!(*h.logs.borrow()),
            case["logs"],
            "source logs {name}/{}",
            h.i
        );
        assert_eq!(
            json!(
                h.serializer
                    .borrow()
                    .iter()
                    .map(|a| a.iter().map(project).collect::<Vec<_>>())
                    .collect::<Vec<_>>()
            ),
            case["serializerCalls"],
            "source serializer {name}/{}",
            h.i
        );
        assert_eq!(
            h.completed.get(),
            case["completed"].as_u64().unwrap() as usize,
            "source completed child count"
        );
    }
}
#[test]
fn post_order_native() {
    family("postOrder")
}
#[test]
fn post_orders_native() {
    family("postOrders")
}
#[test]
fn open_orders_native() {
    family("getOpenOrders")
}
#[test]
fn trades_native() {
    family("getTrades")
}
#[test]
fn gamma_markets_native() {
    family("getGammaMarkets")
}
#[test]
fn positions_native() {
    family("getPositions")
}
#[test]
fn account_native() {
    family("getAccount")
}
#[test]
fn validation_native() {
    family("validateAccount")
}
#[test]
fn observer_retains_whole_method() {
    let case = &oracle()["cases"][5];
    assert_eq!(case["name"], "postOrder");
    let case = oracle()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "getOpenOrders" && c["i"] == 5)
        .unwrap();
    let h = Rc::new(Host::new(case));
    let observer = method(h.clone());
    assert_eq!(h.calls.borrow().len(), 1);
    drop(observer);
    h.finish_all();
    assert_eq!(
        h.calls.borrow().len(),
        3,
        "native dropped observer completes all three pages"
    );
    assert_eq!(h.completed.get(), 3);
    assert_eq!(h.resolves.get(), 1);
    assert!(h.tasks.borrow().is_empty());
    assert_eq!(oracle()["detachedStarted"], 2);
}
#[test]
fn ready_await_yields() {
    let case = oracle()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "getOpenOrders" && c["i"] == 5)
        .unwrap();
    let h = Rc::new(Host::new(case));
    h.ready.set(true);
    let observer = method(h.clone());
    assert_eq!(
        h.calls.borrow().len(),
        1,
        "native ready await defers page two"
    );
    h.finish_all();
    assert_eq!(h.calls.borrow().len(), 3);
    drop(observer);
    assert_eq!(oracle()["readyCalls"], 2);
}
#[test]
fn return_resolution_is_awaited() {
    let case = &oracle()["cases"][0];
    let h = Rc::new(Host::new(case));
    let target = object(Some(999), vec![("marker".into(), V::Number(999.))]);
    *h.resolve_target.borrow_mut() = Some(target.clone());
    let mut observer = method(h.clone());
    h.finish_one();
    h.drive();
    h.drive();
    assert_eq!(h.resolves.get(), 1, "native return resolver invoked");
    let mut cx = Context::from_waker(Waker::noop());
    assert!(
        observer.as_mut().poll(&mut cx).is_pending(),
        "native pending thenable return remains pending"
    );
    h.finish_all();
    let Poll::Ready(Ok(out)) = observer.as_mut().poll(&mut cx) else {
        panic!("native assimilated return")
    };
    assert!(same(&out, &target), "native resolved return alias");
    assert_number(
        &raw_field(&out, "marker"),
        999.,
        "native assimilated marker",
    );
}

#[test]
fn immediate_return_has_no_await() {
    let case = oracle()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "getAccount" && c["i"] == 1)
        .unwrap();
    let h = Rc::new(Host::new(case));
    let mut observer = method(h.clone());
    let mut cx = Context::from_waker(Waker::noop());
    let Poll::Ready(Ok(out)) = observer.as_mut().poll(&mut cx) else {
        panic!("native non-thenable return has no await turn")
    };
    assert_text(
        &raw_field(&out, "userId"),
        &units("unknown"),
        "immediate account result",
    );
    assert!(h.tasks.borrow().is_empty());
    assert_eq!(oracle()["immediateTurns"], json!(["return", "marker"]));
}
