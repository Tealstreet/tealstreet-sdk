use safe_cex_venue_paradex::order_commands::{self as api, OrderCommand, OrderCommandHost as Host};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
type Raw = Rc<Node>;
#[derive(Debug)]
enum Node {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    Text(Vec<u16>),
    Object(&'static str, RefCell<Vec<(String, Raw)>>),
    Array(RefCell<Vec<Raw>>),
}
#[derive(Debug)]
struct Failure {
    value: Raw,
    primary: bool,
}
type Error = Rc<Failure>;
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn su(s: &str) -> Vec<u16> {
    u(s).into_iter().chain([0xd800]).collect()
}
fn raw(s: &str) -> Raw {
    Rc::new(Node::Text(u(s)))
}
fn surrogate(s: &str) -> Raw {
    Rc::new(Node::Text(su(s)))
}
fn undef() -> Raw {
    Rc::new(Node::Undefined)
}
fn n(v: f64) -> Raw {
    Rc::new(Node::Number(v))
}
fn obj(label: &'static str, values: Vec<(&str, Raw)>) -> Raw {
    Rc::new(Node::Object(
        label,
        RefCell::new(values.into_iter().map(|(k, v)| (k.into(), v)).collect()),
    ))
}
fn arr(values: Vec<Raw>) -> Raw {
    Rc::new(Node::Array(RefCell::new(values)))
}
fn field(v: &Raw, key: &str) -> Raw {
    if let Node::Object(_, values) = v.as_ref() {
        values
            .borrow()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(undef)
    } else {
        undef()
    }
}
fn put(v: &Raw, k: &str, x: Raw) {
    let Node::Object(_, values) = v.as_ref() else {
        panic!("not object")
    };
    let mut values = values.borrow_mut();
    if let Some((_, v)) = values.iter_mut().find(|(key, _)| key == k) {
        *v = x;
    } else {
        values.push((k.into(), x));
    }
}
fn bits(v: f64) -> String {
    format!("{:016x}", v.to_bits())
}
fn text(v: &Raw) -> Vec<u16> {
    match v.as_ref() {
        Node::Undefined => u("undefined"),
        Node::Null => u("null"),
        Node::Bool(v) => u(if *v { "true" } else { "false" }),
        Node::Number(v) => u(&safe_cex_num::js_number_string(*v)),
        Node::Text(v) => v.clone(),
        Node::Object("error", _) => u("Error: ")
            .into_iter()
            .chain(text(&field(v, "message")))
            .collect(),
        Node::Object(_, _) => u("[object Object]"),
        Node::Array(a) => a
            .borrow()
            .iter()
            .enumerate()
            .flat_map(|(i, v)| {
                u(if i == 0 { "" } else { "," }).into_iter().chain(
                    if matches!(v.as_ref(), Node::Undefined | Node::Null) {
                        vec![]
                    } else {
                        text(v)
                    },
                )
            })
            .collect(),
    }
}
fn atom(v: &Raw) -> Value {
    match v.as_ref() {
        Node::Undefined => json!({"undefined":true}),
        Node::Null => Value::Null,
        Node::Bool(v) => json!(v),
        Node::Number(v) => json!({"number":bits(*v)}),
        Node::Text(v) => json!({"string":v}),
        Node::Object(_, values) => Value::Object(
            values
                .borrow()
                .iter()
                .map(|(k, v)| (k.clone(), atom(v)))
                .collect(),
        ),
        Node::Array(values) => Value::Array(values.borrow().iter().map(atom).collect()),
    }
}
fn block_on<F: Future>(future: F) -> F::Output {
    let mut cx = Context::from_waker(Waker::noop());
    let mut f = pin!(future);
    loop {
        match f.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => continue,
        }
    }
}
struct Iter {
    values: Raw,
    index: Cell<usize>,
    custom: bool,
    finished: Cell<bool>,
}
struct Fixture {
    profile: String,
    failure: String,
    primary: Error,
    trace: RefCell<Vec<Vec<u16>>>,
    counts: RefCell<Vec<(String, usize)>>,
    logs: RefCell<Vec<Value>>,
    emitted: RefCell<Vec<Raw>>,
    requests: RefCell<Vec<Value>>,
    commands: RefCell<Vec<Value>>,
    closed: RefCell<Vec<bool>>,
    opts: Raw,
    orders: Raw,
    refs: RefCell<Vec<Raw>>,
    response_reads: Cell<usize>,
    selected_log: Cell<usize>,
    post_headers: RefCell<Raw>,
    awaited: Cell<usize>,
}
impl Fixture {
    fn new(kind: &str, profile: &str, failure: &str) -> Self {
        let opts = obj(
            "opts",
            vec![
                ("symbol", surrogate("BTC-")),
                (
                    "takeProfit",
                    n(if profile == "tp" || profile == "both" {
                        110.0
                    } else {
                        0.0
                    }),
                ),
                (
                    "stopLoss",
                    n(if profile == "sl" || profile == "both" {
                        90.0
                    } else {
                        0.0
                    }),
                ),
                (
                    "clientOrderId",
                    if profile == "caller-empty" {
                        raw("")
                    } else if profile == "caller-null" {
                        Rc::new(Node::Null)
                    } else if profile == "caller-id" {
                        surrogate("caller-")
                    } else {
                        undef()
                    },
                ),
            ],
        );
        let count = if profile == "empty" {
            0
        } else if kind == "cancelOrders" && profile == "one" {
            1
        } else if kind == "cancelOrders" && profile == "even" {
            4
        } else if profile == "native-12" {
            12
        } else if profile == "native-10" {
            10
        } else if profile == "native-9" {
            9
        } else {
            5
        };
        let values: Vec<_> = (0..count)
            .map(|i| {
                if kind == "cancelOrders" {
                    obj("order", vec![("id", surrogate(&format!("id{i}-")))])
                } else {
                    obj(
                        "order",
                        vec![
                            ("id", raw(&format!("i{i}"))),
                            (
                                "native",
                                Rc::new(Node::Bool(
                                    profile.starts_with("native")
                                        || profile == "grow"
                                        || i % 2 == 0,
                                )),
                            ),
                        ],
                    )
                }
            })
            .collect();
        if profile == "grow" && kind == "placeOrders" {
            put(&values[0], "native", Rc::new(Node::Bool(false)));
        }
        Self {
            profile: profile.into(),
            failure: failure.into(),
            primary: Rc::new(Failure {
                value: obj("error", vec![("message", surrogate("primary-"))]),
                primary: true,
            }),
            trace: RefCell::new(vec![]),
            counts: RefCell::new(vec![]),
            logs: RefCell::new(vec![]),
            emitted: RefCell::new(vec![]),
            requests: RefCell::new(vec![]),
            commands: RefCell::new(vec![]),
            closed: RefCell::new(vec![]),
            opts,
            orders: arr(values.clone()),
            refs: RefCell::new(values),
            response_reads: Cell::new(0),
            selected_log: Cell::new(0),
            post_headers: RefCell::new(undef()),
            awaited: Cell::new(0),
        }
    }
    fn touch(&self, key: &str) -> Result<(), Error> {
        let mut counts = self.counts.borrow_mut();
        let idx = counts
            .iter()
            .position(|(k, _)| k == key)
            .unwrap_or_else(|| {
                counts.push((key.into(), 0));
                counts.len() - 1
            });
        let label = format!("{key}#{}", counts[idx].1);
        counts[idx].1 += 1;
        self.trace.borrow_mut().push(u(&label));
        if label == self.failure {
            Err(self.primary.clone())
        } else {
            Ok(())
        }
    }
    fn generated(&self, msg: Vec<u16>) -> Error {
        Rc::new(Failure {
            value: obj("error", vec![("message", Rc::new(Node::Text(msg)))]),
            primary: false,
        })
    }
    fn return_future(&self, value: Raw) -> api::CommandFuture<'_, Raw, Error> {
        Box::pin(async move { Ok(value) })
    }
    fn outcome(&self, result: &Result<Raw, Error>) -> Value {
        match result {
            Ok(v) => json!({"ok":true,"value":atom(v)}),
            Err(e) => {
                if e.primary {
                    assert!(
                        Rc::ptr_eq(e, &self.primary),
                        "command_native_error_identity"
                    );
                }
                json!({"ok":false,"primary":e.primary,"message":text(&field(&e.value,"message"))})
            }
        }
    }
    fn native_success(&self, result: &Result<Raw, Error>, name: &str) {
        assert!(result.is_ok(), "{name}: {result:?}");
    }
}
impl Host for Fixture {
    type Value = Raw;
    type Error = Error;
    type Log = usize;
    type Iterator = Iter;
    fn iterator(&self, v: &Raw) -> Result<Iter, Error> {
        assert!(Rc::ptr_eq(v, &self.orders));
        Ok(Iter {
            values: v.clone(),
            index: Cell::new(0),
            custom: self.profile == "custom-iterator",
            finished: Cell::new(false),
        })
    }
    fn next(&self, it: &Iter) -> Result<Option<Raw>, Error> {
        let i = it.index.get();
        it.index.set(i + 1);
        let indices = if it.custom {
            vec![3, 1]
        } else {
            let Node::Array(values) = it.values.as_ref() else {
                panic!()
            };
            (0..values.borrow().len()).collect()
        };
        let Some(index) = indices.get(i) else {
            if it.custom && !it.finished.replace(true) {
                self.closed.borrow_mut().push(true);
            }
            return Ok(None);
        };
        let Node::Array(values) = it.values.as_ref() else {
            panic!()
        };
        Ok(Some(values.borrow()[*index].clone()))
    }
    fn close_iterator(&self, it: &Iter, e: Error) -> Error {
        if it.custom && !it.finished.replace(true) {
            self.closed.borrow_mut().push(true);
        }
        e
    }
    fn entries(&self, v: &Raw) -> Result<Iter, Error> {
        Ok(Iter {
            values: v.clone(),
            index: Cell::new(0),
            custom: false,
            finished: Cell::new(false),
        })
    }
    fn next_entry(&self, it: &Iter) -> Result<Option<(Raw, Raw)>, Error> {
        let i = it.index.get();
        it.index.set(i + 1);
        let Node::Array(values) = it.values.as_ref() else {
            panic!("not array")
        };
        Ok(values.borrow().get(i).cloned().map(|v| (n(i as f64), v)))
    }
    fn get(&self, v: &Raw, k: &str) -> Result<Raw, Error> {
        if let Node::Object("response", _) = v.as_ref() {
            if self.profile == "changing-id" && k == "id" {
                self.touch("response-id")?;
                let i = self.response_reads.get();
                self.response_reads.set(i + 1);
                return Ok(surrogate(["canon-", "canon-", "log-", "return-"][i.min(3)]));
            }
        }
        if matches!(v.as_ref(), Node::Undefined | Node::Null) {
            return Err(self.generated(u("fixture nullish property")));
        }
        Ok(field(v, k))
    }
    fn optional_get(&self, v: &Raw, k: &str) -> Result<Raw, Error> {
        if self.is_undefined(v) || self.is_null(v) {
            Ok(undef())
        } else {
            self.get(v, k)
        }
    }
    fn truthy(&self, v: &Raw) -> bool {
        match v.as_ref() {
            Node::Undefined | Node::Null => false,
            Node::Bool(v) => *v,
            Node::Number(v) => *v != 0.0 && !v.is_nan(),
            Node::Text(v) => !v.is_empty(),
            _ => true,
        }
    }
    fn is_undefined(&self, v: &Raw) -> bool {
        matches!(v.as_ref(), Node::Undefined)
    }
    fn is_null(&self, v: &Raw) -> bool {
        matches!(v.as_ref(), Node::Null)
    }
    fn is_string(&self, v: &Raw) -> bool {
        matches!(v.as_ref(), Node::Text(_))
    }
    fn is_object(&self, v: &Raw) -> bool {
        matches!(v.as_ref(), Node::Object(..) | Node::Array(_) | Node::Null)
    }
    fn is_array(&self, v: &Raw) -> bool {
        matches!(v.as_ref(), Node::Array(_))
    }
    fn equal(&self, a: &Raw, b: &Raw) -> bool {
        match (a.as_ref(), b.as_ref()) {
            (Node::Undefined, Node::Undefined) | (Node::Null, Node::Null) => true,
            (Node::Text(a), Node::Text(b)) => a == b,
            (Node::Number(a), Node::Number(b)) => a == b,
            (Node::Bool(a), Node::Bool(b)) => a == b,
            _ => Rc::ptr_eq(a, b),
        }
    }
    fn literal(&self, s: &str) -> Raw {
        raw(s)
    }
    fn array(&self, v: Vec<Raw>) -> Raw {
        arr(v)
    }
    fn object(&self, v: Vec<(&'static str, Raw)>) -> Raw {
        obj("built", v)
    }
    fn template(&self, v: &Raw) -> Result<Vec<u16>, Error> {
        Ok(text(v))
    }
    fn string(&self, v: &Raw) -> Result<Vec<u16>, Error> {
        Ok(text(v))
    }
    fn text_value(&self, v: Vec<u16>) -> Raw {
        Rc::new(Node::Text(v))
    }
    fn trim_length(&self, v: &Raw) -> Result<f64, Error> {
        let s = text(v);
        let trim = s
            .iter()
            .position(|c| !matches!(*c, 9 | 10 | 13 | 32))
            .map(|a| {
                (
                    a,
                    s.iter()
                        .rposition(|c| !matches!(*c, 9 | 10 | 13 | 32))
                        .unwrap(),
                )
            });
        Ok(trim.map(|(a, b)| (b - a + 1) as f64).unwrap_or(0.0))
    }
    fn length(&self, v: &Raw) -> Result<f64, Error> {
        Ok(match v.as_ref() {
            Node::Array(a) => a.borrow().len() as f64,
            Node::Text(s) => s.len() as f64,
            _ => panic!("not array"),
        })
    }
    fn at(&self, v: &Raw, i: f64) -> Result<Raw, Error> {
        let Node::Array(a) = v.as_ref() else { panic!() };
        Ok(a.borrow().get(i as usize).cloned().unwrap_or_else(undef))
    }
    fn spread(&self, v: &Raw) -> Result<Vec<Raw>, Error> {
        let Node::Array(a) = v.as_ref() else {
            panic!("not iterable")
        };
        Ok(a.borrow().clone())
    }
    fn slice(&self, v: &Raw, a: f64, b: f64) -> Result<Raw, Error> {
        let Node::Array(values) = v.as_ref() else {
            panic!()
        };
        Ok(arr(values
            .borrow()
            .iter()
            .skip(a as usize)
            .take((b - a) as usize)
            .cloned()
            .collect()))
    }
    fn map_ids(&self, v: &Raw) -> Result<Raw, Error> {
        Ok(arr(self
            .spread(v)?
            .iter()
            .map(|v| field(v, "id"))
            .collect()))
    }
    fn state(&self, k: &str) -> Result<Raw, Error> {
        Ok(if k == "sdkInitPromise" {
            if self.profile == "init" || self.profile == "init-error" {
                obj("promise", vec![])
            } else {
                Rc::new(Node::Null)
            }
        } else if self.profile == "no-signer" {
            Rc::new(Node::Null)
        } else {
            obj("signer", vec![])
        })
    }
    fn await_value(&self, _: Raw) -> Result<api::CommandFuture<'_, Raw, Error>, Error> {
        Ok(Box::pin(std::future::poll_fn(move |_| {
            if self.awaited.get() == 0 {
                self.awaited.set(1);
                return Poll::Pending;
            }
            let result = self.touch("await-init").and_then(|()| {
                if self.profile == "init-error" {
                    Err(self.primary.clone())
                } else {
                    Ok(undef())
                }
            });
            Poll::Ready(result)
        })))
    }
    fn command(
        &self,
        c: OrderCommand,
        args: Vec<Raw>,
    ) -> Result<api::CommandFuture<'_, Raw, Error>, Error> {
        match c {
            OrderCommand::Main => {
                self.touch("main")?;
                assert!(Rc::ptr_eq(&args[0], &self.opts));
                self.commands.borrow_mut().push(
                    json!({"method":"Main","args":args.iter().map(atom).collect::<Vec<_>>()}),
                );
                Ok(self.return_future(surrogate("main-")))
            }
            OrderCommand::Brackets => {
                self.touch("brackets")?;
                self.commands.borrow_mut().push(
                    json!({"method":"Brackets","args":args.iter().map(atom).collect::<Vec<_>>()}),
                );
                Ok(self.return_future(arr(vec![surrogate("tp-"), surrogate("sl-")])))
            }
            OrderCommand::Build => {
                self.touch("build")?;
                assert!(Rc::ptr_eq(&args[0], &self.opts));
                self.commands.borrow_mut().push(
                    json!({"method":"Build","args":args.iter().map(atom).collect::<Vec<_>>()}),
                );
                let payload = obj(
                    "payload",
                    vec![
                        ("market", field(&args[0], "symbol")),
                        ("client_id", args[1].clone()),
                    ],
                );
                let headers = if self.profile == "headers" {
                    obj("headers", vec![("X-Stub", surrogate("stub-"))])
                } else {
                    undef()
                };
                Ok(self.return_future(obj(
                    "built",
                    vec![("payload", payload), ("headers", headers)],
                )))
            }
            OrderCommand::NativeBatch => {
                self.touch("batch")?;
                let batch = self.spread(&args[0])?;
                let indices: Vec<_> = batch
                    .iter()
                    .map(|v| {
                        self.refs
                            .borrow()
                            .iter()
                            .position(|r| Rc::ptr_eq(r, v))
                            .unwrap()
                    })
                    .collect();
                self.commands
                    .borrow_mut()
                    .push(json!({"method":"NativeBatch","indices":indices}));
                if self.profile == "batch-error" {
                    return Ok(Box::pin(async { Err(self.primary.clone()) }));
                }
                let ids = batch
                    .iter()
                    .map(|o| {
                        self.text_value(
                            u("batch:")
                                .into_iter()
                                .chain(text(&field(o, "id")))
                                .collect(),
                        )
                    })
                    .collect();
                Ok(self.return_future(arr(ids)))
            }
            OrderCommand::Single => {
                self.touch("single")?;
                let index = self
                    .refs
                    .borrow()
                    .iter()
                    .position(|r| Rc::ptr_eq(r, &args[0]))
                    .unwrap();
                self.commands
                    .borrow_mut()
                    .push(json!({"method":"Single","index":index}));
                if self.profile == "single-error" {
                    return Ok(Box::pin(async { Err(self.primary.clone()) }));
                }
                if self.profile == "grow" && self.length(&self.orders)? == 5.0 {
                    let v = obj(
                        "order",
                        vec![("id", raw("i5")), ("native", Rc::new(Node::Bool(false)))],
                    );
                    if let Node::Array(a) = self.orders.as_ref() {
                        a.borrow_mut().push(v.clone());
                    }
                    self.refs.borrow_mut().push(v);
                }
                let id = self.text_value(
                    u("single:")
                        .into_iter()
                        .chain(text(&field(&args[0], "id")))
                        .collect(),
                );
                Ok(self.return_future(arr(vec![id])))
            }
            OrderCommand::CancelBatch | OrderCommand::CancelSingle => {
                let batch = c == OrderCommand::CancelBatch;
                self.touch(if batch {
                    "cancel-batch"
                } else {
                    "cancel-single"
                })?;
                let ids = if batch {
                    self.spread(&args[0])?
                } else {
                    vec![args[0].clone()]
                };
                self.commands.borrow_mut().push(json!({"method":if batch{"CancelBatch"}else{"CancelSingle"},"ids":ids.iter().map(text).collect::<Vec<_>>()}));
                if self.profile == "many-failures"
                    || self.profile == "one-failure" && self.commands.borrow().len() == 1
                    || !batch && self.profile == "last-failure"
                {
                    return Ok(Box::pin(async { Err(self.primary.clone()) }));
                }
                if self.profile == "grow" && self.length(&self.orders)? == 5.0 {
                    if let Node::Array(a) = self.orders.as_ref() {
                        a.borrow_mut()
                            .push(obj("order", vec![("id", surrogate("id5-"))]));
                    }
                }
                Ok(self.return_future(undef()))
            }
        }
    }
    fn resolve_client_id(&self, v: &Raw) -> Result<Raw, Error> {
        self.touch("resolve")?;
        assert!(Rc::ptr_eq(v, &self.opts));
        Ok(field(v, "clientOrderId"))
    }
    fn generate_client_id(&self) -> Result<Raw, Error> {
        self.touch("generate")?;
        Ok(surrogate("ts_generated-"))
    }
    fn assert_client_ids(&self, v: &Raw) -> Result<(), Error> {
        self.touch("assert")?;
        assert!(Rc::ptr_eq(v, &self.orders));
        Ok(())
    }
    fn native_batch_order(&self, v: &Raw) -> Result<bool, Error> {
        self.touch("predicate")?;
        Ok(self.truthy(&field(v, "native")))
    }
    fn post(
        &self,
        endpoint: &'static str,
        payload: Raw,
        headers: Raw,
    ) -> Result<api::CommandFuture<'_, Raw, Error>, Error> {
        self.touch("post")?;
        *self.post_headers.borrow_mut() = headers.clone();
        let configs = if self.truthy(&headers) {
            vec![atom(&obj("config", vec![("headers", headers)]))]
        } else {
            vec![]
        };
        self.requests.borrow_mut().push(json!({"method":"post","endpoint":u(endpoint),"payload":atom(&payload),"configs":configs}));
        if self.profile == "api-error" {
            put(
                &self.primary.value,
                "response",
                obj(
                    "api-response",
                    vec![(
                        "data",
                        obj(
                            "api-data",
                            vec![
                                ("error", surrogate("bad-")),
                                ("message", surrogate("denied-")),
                            ],
                        ),
                    )],
                ),
            );
            return Ok(Box::pin(async { Err(self.primary.clone()) }));
        }
        let data = if self.profile == "bad-envelope" {
            arr(vec![])
        } else {
            let data = obj(
                "response",
                vec![
                    ("id", surrogate("canon-")),
                    ("client_id", field(&payload, "client_id")),
                ],
            );
            if self.profile == "no-id" {
                if let Node::Object(_, m) = data.as_ref() {
                    m.borrow_mut().retain(|(k, _)| k != "id");
                }
            }
            if self.profile == "blank-id" {
                put(&data, "id", raw(" \t"));
            }
            if self.profile == "wrong-echo" {
                put(&data, "client_id", raw("wrong"));
            }
            data
        };
        Ok(self.return_future(obj("http", vec![("data", data)])))
    }
    fn delete(
        &self,
        endpoint: Vec<u16>,
        config: Option<Raw>,
    ) -> Result<api::CommandFuture<'_, Raw, Error>, Error> {
        self.touch("delete")?;
        self.requests.borrow_mut().push(json!({"method":"delete","endpoint":endpoint,"config":atom(&config.unwrap_or_else(undef))}));
        Ok(self.return_future(obj("http", vec![("data", cancel_response(&self.profile))])))
    }
    fn select_log(&self) -> Result<usize, Error> {
        let value = self.selected_log.get();
        self.selected_log.set(value + 1);
        Ok(value)
    }
    fn log(&self, _: usize, t: Vec<u16>, severity: &'static str) -> Result<(), Error> {
        self.touch("log")?;
        self.logs
            .borrow_mut()
            .push(json!({"text":t,"severity":severity}));
        Ok(())
    }
    fn new_error(&self, t: Vec<u16>) -> Result<Error, Error> {
        Ok(self.generated(t))
    }
    fn error_value(&self, e: &Error) -> Raw {
        e.value.clone()
    }
    fn error_is_instance(&self, _: &Error) -> Result<bool, Error> {
        Ok(true)
    }
    fn emit(&self, v: Raw) -> Result<(), Error> {
        self.touch("emit")?;
        self.emitted.borrow_mut().push(v);
        Ok(())
    }
    fn stringify(&self, v: &Raw) -> Result<Vec<u16>, Error> {
        fn quote(s: &[u16]) -> Vec<u16> {
            let mut out = u("\"");
            for (i, c) in s.iter().copied().enumerate() {
                match c {
                    34 => out.extend(u("\\\"")),
                    92 => out.extend(u("\\\\")),
                    9 => out.extend(u("\\t")),
                    c if c < 32
                        || ((0xd800..=0xdbff).contains(&c)
                            && !s.get(i + 1).is_some_and(|c| (0xdc00..=0xdfff).contains(c)))
                        || ((0xdc00..=0xdfff).contains(&c)
                            && (i == 0 || !(0xd800..=0xdbff).contains(&s[i - 1]))) =>
                    {
                        out.extend(u(&format!("\\u{c:04x}")))
                    }
                    _ => out.push(c),
                }
            }
            out.push(34);
            out
        }
        fn serialize(v: &Raw) -> Vec<u16> {
            match v.as_ref() {
                Node::Text(s) => quote(s),
                Node::Undefined | Node::Null => u("null"),
                Node::Bool(v) => u(if *v { "true" } else { "false" }),
                Node::Number(v) => u(&safe_cex_num::js_number_string(*v)),
                Node::Array(a) => u("[")
                    .into_iter()
                    .chain(a.borrow().iter().enumerate().flat_map(|(i, v)| {
                        u(if i == 0 { "" } else { "," })
                            .into_iter()
                            .chain(serialize(v))
                    }))
                    .chain(u("]"))
                    .collect(),
                Node::Object(_, m) => {
                    let mut out = u("{");
                    let mut count = 0;
                    for (k, v) in m.borrow().iter() {
                        if matches!(v.as_ref(), Node::Undefined) {
                            continue;
                        }
                        if count > 0 {
                            out.push(44);
                        }
                        count += 1;
                        out.extend(quote(&u(k)));
                        out.push(58);
                        out.extend(serialize(v));
                    }
                    out.push(125);
                    out
                }
            }
        }
        Ok(serialize(v))
    }
    fn replace_endpoint(
        &self,
        endpoint: &'static str,
        search: &str,
        v: &Raw,
    ) -> Result<Vec<u16>, Error> {
        let replacement = text(v);
        let mut out = Vec::new();
        let (a, b) = endpoint.split_once(search).unwrap();
        out.extend(u(a));
        let mut i = 0;
        while i < replacement.len() {
            if replacement[i] == 36 && replacement.get(i + 1) == Some(&38) {
                out.extend(u(search));
                i += 2;
            } else {
                out.push(replacement[i]);
                i += 1;
            }
        }
        out.extend(u(b));
        Ok(out)
    }
}
fn cancel_response(profile: &str) -> Raw {
    let a = obj(
        "row",
        vec![
            ("id", surrogate("a-")),
            ("status", raw("QUEUED_FOR_CANCELLATION")),
        ],
    );
    let b = obj(
        "row",
        vec![("id", surrogate("b-")), ("status", raw("ALREADY_CLOSED"))],
    );
    if profile == "bad-envelope" {
        return Rc::new(Node::Null);
    }
    if profile == "bad-results" {
        return obj("body", vec![("results", obj("wrong", vec![]))]);
    }
    let rows = match profile {
        "missing" => vec![a],
        "extra" => vec![
            a,
            b,
            obj(
                "row",
                vec![("id", raw("c")), ("status", raw("QUEUED_FOR_CANCELLATION"))],
            ),
        ],
        "unknown" => vec![
            a,
            obj(
                "row",
                vec![("id", raw("c")), ("status", raw("QUEUED_FOR_CANCELLATION"))],
            ),
        ],
        "repeat" => vec![a.clone(), a],
        "uncorrelated" => vec![a, obj("row", vec![("status", raw("NOT_FOUND"))])],
        "bad-row" => vec![a, arr(vec![])],
        "empty-result" => vec![],
        "reversed" => vec![b, a],
        _ => {
            if profile == "not-found" {
                put(&b, "status", raw("NOT_FOUND"));
            }
            if profile == "unsupported" {
                put(&b, "status", surrogate("WHAT-"));
            }
            vec![a, b]
        }
    };
    obj("body", vec![("results", arr(rows))])
}
fn check(row: &Value) {
    let kind = row["kind"].as_str().unwrap();
    let profile = row["profile"].as_str().unwrap();
    let failure = row["failure"].as_str().unwrap();
    let h = Fixture::new(kind, profile, failure);
    let result = match kind {
        "generateClientId" => api::generate_client_id(
            || {
                h.touch("uuid")?;
                Ok(match profile {
                    "uuid-empty" => vec![],
                    "uuid-long" => u(&"x".repeat(62)),
                    "uuid-boundary" => u(&"x".repeat(61)),
                    "uuid-astral" => u(&("𐐀".repeat(30) + "x")),
                    "uuid-surrogate" => su("a-"),
                    _ => u("fixture-uuid"),
                })
            },
            |message| Ok(h.generated(u(message))),
        )
        .map(|s| h.text_value(s)),
        "placeOrder" => block_on(api::place_order(&h, h.opts.clone())).map(arr),
        "placeOrders" => block_on(api::place_orders(&h, h.orders.clone())).map(arr),
        "placeMainOrder" => block_on(api::place_main_order(&h, h.opts.clone())),
        "cancelOrder" => block_on(api::cancel_order(&h, surrogate("id-$&-"))).map(|()| undef()),
        "cancelAllOrders" => block_on(api::cancel_all_orders(&h)).map(|()| undef()),
        "cancelSymbolOrders" => {
            block_on(api::cancel_symbol_orders(&h, surrogate("BTC-"))).map(|()| undef())
        }
        "cancelOrders" => block_on(api::cancel_orders(&h, h.orders.clone())).map(|()| undef()),
        "cancelNativeBatchChunk" => {
            let ids = match profile {
                "empty" => vec![],
                "one" => vec![surrogate("a-")],
                "three" => vec![surrogate("a-"), raw("b"), raw("c")],
                "duplicate" => vec![surrogate("a-"), surrogate("a-")],
                "blank" => vec![surrogate("a-"), raw(" \t")],
                "non-string" => vec![surrogate("a-"), n(7.0)],
                _ => vec![surrogate("a-"), surrogate("b-")],
            };
            block_on(api::cancel_native_batch_chunk(&h, arr(ids))).map(|()| undef())
        }
        _ => panic!("unexpected kind"),
    };
    if failure == "none" {
        match (kind, profile) {
            ("generateClientId", "basic") => {
                h.native_success(&result, "client_id_native_success");
                assert_eq!(
                    text(result.as_ref().unwrap()),
                    u("ts_fixture-uuid"),
                    "client_id_native_value"
                );
            }
            ("generateClientId", "uuid-empty" | "uuid-long") => {
                assert!(result.is_err(), "client_id_native_bounds");
                assert_eq!(
                    text(&field(&result.as_ref().unwrap_err().value, "message")),
                    u("Paradex generated an invalid client_id"),
                    "client_id_native_error"
                );
            }
            ("generateClientId", "uuid-boundary" | "uuid-astral") => {
                h.native_success(&result, "client_id_native_boundary");
                assert_eq!(
                    text(result.as_ref().unwrap()).len(),
                    64,
                    "client_id_native_utf16_length"
                );
            }
            ("placeOrder", "basic" | "both") => {
                h.native_success(&result, "place_native_success");
                let ids = h.spread(result.as_ref().unwrap()).unwrap();
                let expected = if profile == "both" {
                    vec![su("main-"), su("tp-"), su("sl-")]
                } else {
                    vec![su("main-")]
                };
                assert_eq!(
                    ids.iter().map(text).collect::<Vec<_>>(),
                    expected,
                    "place_native_ids"
                );
            }
            ("placeOrders", "native-10" | "native-12") => {
                h.native_success(&result, "mixed_native_success");
                let count = if profile == "native-10" { 10 } else { 12 };
                let expected = if count == 10 {
                    vec![json!({"method":"NativeBatch","indices":(0..10).collect::<Vec<_>>()})]
                } else {
                    vec![
                        json!({"method":"NativeBatch","indices":(0..10).collect::<Vec<_>>()}),
                        json!({"method":"NativeBatch","indices":[10,11]}),
                    ]
                };
                assert_eq!(*h.commands.borrow(), expected, "mixed_native_chunks");
                let ids = h.spread(result.as_ref().unwrap()).unwrap();
                assert_eq!(
                    ids.iter().map(text).collect::<Vec<_>>(),
                    (0..count)
                        .map(|i| u(&format!("batch:i{i}")))
                        .collect::<Vec<_>>(),
                    "mixed_native_ids"
                );
            }
            ("placeOrders", "grow") => {
                h.native_success(&result, "mixed_native_growth_success");
                assert_eq!(
                    h.length(&h.orders).unwrap(),
                    6.0,
                    "mixed_native_live_length"
                );
                assert_eq!(
                    h.spread(result.as_ref().unwrap())
                        .unwrap()
                        .iter()
                        .map(text)
                        .collect::<Vec<_>>(),
                    [
                        "single:i0",
                        "batch:i1",
                        "batch:i2",
                        "batch:i3",
                        "batch:i4",
                        "single:i5"
                    ]
                    .iter()
                    .map(|s| u(s))
                    .collect::<Vec<_>>(),
                    "mixed_native_live_results"
                );
            }
            ("placeOrders", "custom-iterator") => {
                h.native_success(&result, "mixed_native_iterator_success");
                assert_eq!(
                    h.spread(result.as_ref().unwrap())
                        .unwrap()
                        .iter()
                        .map(text)
                        .collect::<Vec<_>>(),
                    vec![u("single:i3"), u("single:i1")],
                    "mixed_native_custom_iteration"
                );
                assert_eq!(
                    *h.closed.borrow(),
                    vec![true],
                    "mixed_native_iterator_cleanup"
                );
            }
            ("placeMainOrder", "basic" | "headers") => {
                h.native_success(&result, "main_native_success");
                assert_eq!(
                    text(result.as_ref().unwrap()),
                    su("canon-"),
                    "main_native_id"
                );
                let requests = h.requests.borrow();
                assert_eq!(requests.len(), 1, "main_native_request_count");
                assert_eq!(
                    requests[0]["endpoint"],
                    json!(u("/orders")),
                    "main_native_endpoint"
                );
                assert_eq!(
                    requests[0]["payload"],
                    atom(&obj(
                        "expected",
                        vec![
                            ("market", surrogate("BTC-")),
                            ("client_id", surrogate("ts_generated-"))
                        ]
                    )),
                    "main_native_payload"
                );
                if profile == "headers" {
                    assert_eq!(
                        text(&field(&h.post_headers.borrow(), "X-Stub")),
                        su("stub-"),
                        "main_native_header"
                    );
                }
            }
            ("placeMainOrder", "changing-id") => {
                h.native_success(&result, "main_native_changing_success");
                assert_eq!(
                    text(result.as_ref().unwrap()),
                    su("return-"),
                    "main_native_fresh_id"
                );
                assert_eq!(h.response_reads.get(), 4, "main_native_id_reads");
            }
            ("placeMainOrder", "wrong-echo") => {
                assert!(result.is_err(), "main_native_echo_reject");
                assert_eq!(h.emitted.borrow().len(), 1, "main_native_echo_emit");
            }
            ("cancelOrder", "basic") => {
                h.native_success(&result, "cancel_one_native_success");
                assert_eq!(
                    h.requests.borrow()[0]["endpoint"],
                    json!(su("/orders/id-{id}-")),
                    "cancel_one_native_endpoint"
                );
            }
            ("cancelAllOrders", "basic") => {
                h.native_success(&result, "cancel_all_native_success");
                assert_eq!(
                    h.requests.borrow()[0]["endpoint"],
                    json!(u("/orders")),
                    "cancel_all_native_endpoint"
                );
                assert_eq!(
                    h.requests.borrow()[0]["config"],
                    json!({"undefined":true}),
                    "cancel_all_native_config"
                );
            }
            ("cancelSymbolOrders", "basic") => {
                h.native_success(&result, "cancel_symbol_native_success");
                assert_eq!(
                    h.requests.borrow()[0]["config"],
                    json!({"params":{"market":{"string":su("BTC-")}}}),
                    "cancel_symbol_native_config"
                );
            }
            ("cancelOrders", "basic" | "even") => {
                h.native_success(&result, "cancel_many_native_success");
                assert_eq!(
                    *h.commands.borrow(),
                    if profile == "even" {
                        vec![
                            json!({"method":"CancelBatch","ids":[su("id0-"),su("id1-")]}),
                            json!({"method":"CancelBatch","ids":[su("id2-"),su("id3-")]}),
                        ]
                    } else {
                        vec![
                            json!({"method":"CancelBatch","ids":[su("id0-"),su("id1-")]}),
                            json!({"method":"CancelBatch","ids":[su("id2-"),su("id3-")]}),
                            json!({"method":"CancelSingle","ids":[su("id4-")]}),
                        ]
                    },
                    "cancel_many_native_chunks"
                );
            }
            ("cancelOrders", "one-failure" | "last-failure") => {
                let e = result.as_ref().unwrap_err();
                assert!(
                    Rc::ptr_eq(e, &h.primary),
                    "cancel_many_native_original_error"
                );
                assert_eq!(
                    h.commands.borrow().len(),
                    3,
                    "cancel_many_native_all_attempted"
                );
            }
            ("cancelOrders", "many-failures") => {
                let e = result.as_ref().unwrap_err();
                assert!(!e.primary, "cancel_many_native_aggregate_error");
                assert_eq!(
                    h.commands.borrow().len(),
                    3,
                    "cancel_many_native_all_attempted"
                );
                assert!(
                    text(&field(&e.value, "message")).starts_with(&u(
                        "Paradex cancellation failed for 3 chunks after attempting all orders: "
                    )),
                    "cancel_many_native_aggregate_message"
                );
            }
            ("cancelNativeBatchChunk", "basic" | "reversed") => {
                h.native_success(&result, "batch_cancel_native_success");
                assert_eq!(
                    h.requests.borrow()[0]["endpoint"],
                    json!(u("/orders/batch")),
                    "batch_cancel_native_endpoint"
                );
                assert_eq!(
                    h.requests.borrow()[0]["config"],
                    json!({"data":{"order_ids":[{"string":su("a-")},{"string":su("b-")}]}}),
                    "batch_cancel_native_ids"
                );
                assert_eq!(
                    h.logs.borrow()[0],
                    json!({"text":u("Cancelled order batch [a-").into_iter().chain([0xd800]).chain(u(", b-")).chain([0xd800]).chain(u("]")).collect::<Vec<_>>(),"severity":"debug"}),
                    "batch_cancel_native_log"
                );
            }
            ("cancelNativeBatchChunk", "not-found") => {
                let e = result.as_ref().unwrap_err();
                assert_eq!(
                    text(&field(&e.value, "message")),
                    u("Paradex batch cancel rejected order IDs [b-")
                        .into_iter()
                        .chain([0xd800])
                        .chain(u("]: NOT_FOUND"))
                        .collect::<Vec<_>>(),
                    "batch_cancel_native_rejected"
                );
            }
            _ => {}
        }
    }
    if kind == "placeOrders" && profile == "custom-iterator" && failure == "predicate#0" {
        assert!(
            Rc::ptr_eq(result.as_ref().unwrap_err(), &h.primary),
            "mixed_native_abrupt_error"
        );
        assert_eq!(*h.closed.borrow(), vec![true], "mixed_native_abrupt_close");
        assert!(
            h.commands.borrow().is_empty(),
            "mixed_native_abrupt_before_commands"
        );
    }
    let outcome = h.outcome(&result);
    assert_eq!(
        outcome, row["outcome"],
        "command_supplemental_outcome {kind}/{profile}/{failure}"
    );
    assert_eq!(
        json!(*h.trace.borrow()),
        row["trace"],
        "command_supplemental_trace {kind}/{profile}/{failure}"
    );
    assert_eq!(
        json!(*h.logs.borrow()),
        row["logs"],
        "command_supplemental_logs {kind}/{profile}/{failure}"
    );
    assert_eq!(
        json!(h.emitted.borrow().iter().map(atom).collect::<Vec<_>>()),
        row["emitted"],
        "command_supplemental_emitted {kind}/{profile}/{failure}"
    );
    assert_eq!(
        json!(*h.requests.borrow()),
        row["requests"],
        "command_supplemental_requests {kind}/{profile}/{failure}"
    );
    assert_eq!(
        json!(*h.commands.borrow()),
        row["commands"],
        "command_supplemental_commands {kind}/{profile}/{failure}"
    );
    assert_eq!(json!(*h.closed.borrow()), row["closed"]);
}
#[test]
fn actual_typescript_order_commands() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/order_commands_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source_oracle_exit {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let all: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(all.as_array().unwrap().len(), 1008, "source_case_count");
    for row in all.as_array().unwrap() {
        check(row);
    }
}

#[test]
fn main_awaits_sdk_before_side_effects() {
    let h = Fixture::new("placeMainOrder", "init", "none");
    let mut cx = Context::from_waker(Waker::noop());
    let mut future = pin!(api::place_main_order(&h, h.opts.clone()));
    assert!(
        future.as_mut().poll(&mut cx).is_pending(),
        "main_native_pending"
    );
    assert!(h.logs.borrow().is_empty(), "main_native_pending_logs");
    assert!(
        h.commands.borrow().is_empty(),
        "main_native_pending_commands"
    );
    assert!(
        h.requests.borrow().is_empty(),
        "main_native_pending_requests"
    );
    let Poll::Ready(result) = future.as_mut().poll(&mut cx) else {
        panic!("main did not resume")
    };
    assert_eq!(
        text(&result.unwrap()),
        su("canon-"),
        "main_native_resumed_id"
    );
}
