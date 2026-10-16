use safe_cex_venue_kucoin::{public_streams::StreamOps, spot_native_accounts};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
type V = Rc<Native>;
type E = V;
type Task = Pin<Box<dyn Future<Output = Result<V, E>>>>;
enum Native {
    U,
    Null,
    B(bool),
    N(f64),
    S(Vec<u16>),
    O(RefCell<Vec<(String, V)>>),
    A(Vec<V>),
    F(String, usize, Rc<dyn Fn(V, Vec<V>) -> Result<V, E>>),
    P(Promise),
}
struct Promise {
    result: RefCell<Option<Result<V, E>>>,
    task: RefCell<Option<Task>>,
}
fn u() -> V {
    Rc::new(Native::U)
}
fn null() -> V {
    Rc::new(Native::Null)
}
fn s(value: &str) -> V {
    units(value.encode_utf16().collect())
}
fn units(value: Vec<u16>) -> V {
    Rc::new(Native::S(value))
}
fn n(value: f64) -> V {
    Rc::new(Native::N(value))
}
fn b(value: bool) -> V {
    Rc::new(Native::B(value))
}
fn object(fields: Vec<(&str, V)>) -> V {
    Rc::new(Native::O(RefCell::new(
        fields.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    )))
}
fn array(values: Vec<V>) -> V {
    Rc::new(Native::A(values))
}
fn function(name: &str, length: usize, body: impl Fn(V, Vec<V>) -> Result<V, E> + 'static) -> V {
    Rc::new(Native::F(name.into(), length, Rc::new(body)))
}
fn ready(result: Result<V, E>) -> V {
    Rc::new(Native::P(Promise {
        result: RefCell::new(Some(result)),
        task: RefCell::new(None),
    }))
}
fn deferred() -> V {
    Rc::new(Native::P(Promise {
        result: RefCell::new(None),
        task: RefCell::new(None),
    }))
}
fn task(body: Task) -> V {
    Rc::new(Native::P(Promise {
        result: RefCell::new(None),
        task: RefCell::new(Some(body)),
    }))
}
fn keys(value: &V) -> Vec<String> {
    let Native::O(fields) = value.as_ref() else {
        panic!("object expected")
    };
    fields.borrow().iter().map(|(k, _)| k.clone()).collect()
}
fn at(value: &V, key: &str) -> V {
    let Native::O(fields) = value.as_ref() else {
        panic!("object expected")
    };
    fields
        .borrow()
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
        .unwrap_or_else(u)
}
fn put(value: &V, key: &str, new: V) {
    let Native::O(fields) = value.as_ref() else {
        panic!("object expected")
    };
    let mut fields = fields.borrow_mut();
    if let Some((_, v)) = fields.iter_mut().find(|(k, _)| k == key) {
        *v = new
    } else {
        fields.push((key.into(), new));
    }
}
fn rows(value: &V) -> &[V] {
    let Native::A(rows) = value.as_ref() else {
        panic!("array expected")
    };
    rows
}
fn call(value: V, receiver: V, args: Vec<V>) -> Result<V, E> {
    let Native::F(_, _, body) = value.as_ref() else {
        panic!("callable expected")
    };
    body(receiver, args)
}
fn truthy(value: &V) -> bool {
    match value.as_ref() {
        Native::U | Native::Null => false,
        Native::B(v) => *v,
        Native::N(v) => *v != 0. && !v.is_nan(),
        Native::S(v) => !v.is_empty(),
        _ => true,
    }
}
fn equal(a: &V, b: &V) -> bool {
    match (a.as_ref(), b.as_ref()) {
        (Native::S(a), Native::S(b)) => a == b,
        (Native::N(a), Native::N(b)) => a == b,
        (Native::B(a), Native::B(b)) => a == b,
        (Native::U, Native::U) | (Native::Null, Native::Null) => true,
        _ => Rc::ptr_eq(a, b),
    }
}
fn error(message: &str) -> V {
    object(vec![("name", s("TypeError")), ("message", s(message))])
}
fn assert_s(value: &V, want: &str) {
    assert_units(value, &want.encode_utf16().collect::<Vec<_>>());
}
fn assert_units(value: &V, want: &[u16]) {
    let Native::S(actual) = value.as_ref() else {
        panic!("KUCOIN-ACCOUNT native string kind")
    };
    assert_eq!(actual, want, "KUCOIN-ACCOUNT native string units");
}
fn assert_n(value: &V, want: f64) {
    let Native::N(actual) = value.as_ref() else {
        panic!("KUCOIN-ACCOUNT native numeric kind")
    };
    assert_eq!(
        actual.to_bits(),
        want.to_bits(),
        "KUCOIN-ACCOUNT native numeric bits"
    );
}
fn assert_same(a: &V, b: &V) {
    assert!(Rc::ptr_eq(a, b), "KUCOIN-ACCOUNT native reference identity");
}
fn pack(value: &V) -> Value {
    match value.as_ref() {
        Native::U => json!(["u"]),
        Native::Null => json!(["null"]),
        Native::B(v) => json!(["b", v]),
        Native::N(v) => json!(["n", format!("{:016x}", v.to_bits())]),
        Native::S(v) => json!(["s", v]),
        Native::O(fields) => json!([
            "o",
            fields
                .borrow()
                .iter()
                .map(|(k, v)| json!([k, pack(v)]))
                .collect::<Vec<_>>()
        ]),
        Native::A(values) => json!(["a", values.iter().map(pack).collect::<Vec<_>>()]),
        _ => panic!("no fixture implementation values in projection"),
    }
}
fn poll_value(value: &V, cx: &mut Context<'_>) -> Poll<Result<V, E>> {
    let Native::P(promise) = value.as_ref() else {
        return Poll::Ready(Ok(value.clone()));
    };
    if let Some(result) = promise.result.borrow().clone() {
        return Poll::Ready(result);
    }
    let Some(mut pending) = promise.task.borrow_mut().take() else {
        return Poll::Pending;
    };
    match pending.as_mut().poll(cx) {
        Poll::Ready(result) => {
            *promise.result.borrow_mut() = Some(result.clone());
            Poll::Ready(result)
        }
        Poll::Pending => {
            *promise.task.borrow_mut() = Some(pending);
            Poll::Pending
        }
    }
}
struct Await(V);
impl Future for Await {
    type Output = Result<V, E>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        poll_value(&self.0, cx)
    }
}
fn poll(task: &mut Task) -> Poll<Result<V, E>> {
    task.as_mut().poll(&mut Context::from_waker(Waker::noop()))
}
const NULL_BINDING: &str =
    "Cannot destructure property 'data' of '(intermediate value)' as it is null.";
struct Fixture {
    kind: &'static str,
    receiver: V,
    xhr: V,
    endpoints: V,
    envelope: V,
    raw: V,
    balance: V,
    failure: Option<V>,
    balance_failure: Option<V>,
    sync_failure: Option<V>,
    catch_failure: Option<V>,
    reporter: V,
    pending_balance: V,
    emit_failures: Cell<usize>,
    emitted: RefCell<Vec<V>>,
    requests: RefCell<Vec<V>>,
    callbacks: RefCell<Vec<(String, usize)>>,
    reads: Cell<usize>,
    balance_calls: Cell<usize>,
}
impl Fixture {
    fn new(kind: &'static str) -> Rc<Self> {
        let raw = object(vec![("marker", n(29.))]);
        let mut envelope = object(vec![
            ("code", s("200000")),
            (
                "data",
                object(vec![
                    ("userId", s("user-11")),
                    ("subUserId", s("sub-13")),
                    ("referrerUserId", s("affiliate-17")),
                ]),
            ),
        ]);
        let raw_failure = || {
            object(vec![
                (
                    "response",
                    object(vec![("data", object(vec![("msg", raw.clone())]))]),
                ),
                ("message", s("transport")),
            ])
        };
        let mut failure = None;
        let mut balance_failure = None;
        let mut sync_failure = None;
        let mut catch_failure = None;
        let mut emit_failures = 0;
        match kind {
            "strings" | "callee-order" | "live-data" | "validate-alias" | "validate-pending"
            | "null-response" => {}
            "native-identities" => put(
                &envelope,
                "data",
                object(vec![
                    ("userId", n(31.)),
                    ("subUserId", b(true)),
                    ("referrerUserId", raw.clone()),
                ]),
            ),
            "falsy-uid" => put(
                &envelope,
                "data",
                object(vec![
                    ("userId", n(0.)),
                    ("uid", n(53.)),
                    ("subUserId", n(0.)),
                    ("referrerUserId", b(false)),
                ]),
            ),
            "missing-data" => envelope = object(vec![("code", s("200000"))]),
            "raw-rejection" => envelope = object(vec![("code", s("no")), ("msg", raw.clone())]),
            "surrogate-id" => put(
                &envelope,
                "data",
                object(vec![
                    ("userId", units(vec![0xd800])),
                    ("subUserId", units(vec![0xdfff])),
                    ("referrerUserId", units(vec![0x61, 0xd800, 0x62])),
                ]),
            ),
            "raw-failure" | "validate-raw-catch" => failure = Some(raw_failure()),
            "emit-throws" => {
                envelope = object(vec![("code", s("no")), ("msg", s("first"))]);
                emit_failures = 1;
            }
            "emit-propagates" => {
                failure = Some(object(vec![("message", s("first"))]));
                emit_failures = 2;
            }
            "validate-numeric-code" => {
                envelope = object(vec![("code", n(200000.)), ("msg", raw.clone())])
            }
            "validate-infinity" => {
                envelope = object(vec![("code", s("no")), ("msg", n(f64::INFINITY))])
            }
            "balance-rejection" => balance_failure = Some(object(vec![("message", s("balance"))])),
            "balance-sync-throw" => sync_failure = Some(raw_failure()),
            "catch-getter" => catch_failure = Some(raw_failure()),
            _ => panic!("unknown fixture"),
        }
        Rc::new(Self {
            kind,
            receiver: object(vec![]),
            xhr: object(vec![]),
            endpoints: object(vec![
                ("ACCOUNT_SUMMARY", s("/api/v2/user-info")),
                ("ACCOUNTS", s("/api/v1/accounts")),
            ]),
            envelope,
            raw: raw.clone(),
            balance: object(vec![("free", n(2.))]),
            failure,
            balance_failure,
            sync_failure,
            catch_failure,
            reporter: object(vec![
                (
                    "response",
                    object(vec![("data", object(vec![("msg", raw)]))]),
                ),
                ("message", s("reporter")),
            ]),
            pending_balance: deferred(),
            emit_failures: Cell::new(emit_failures),
            emitted: RefCell::new(vec![]),
            requests: RefCell::new(vec![]),
            callbacks: RefCell::new(vec![]),
            reads: Cell::new(0),
            balance_calls: Cell::new(0),
        })
    }
    fn read(self: &Rc<Self>, value: &V, key: &str) -> Result<V, E> {
        if Rc::ptr_eq(value, &self.receiver) {
            return match key {
                "xhr" => Ok(self.xhr.clone()),
                "emitAndLogError" => {
                    let f = self.clone();
                    Ok(function("", 1, move |receiver, args| {
                        assert_same(&receiver, &f.receiver);
                        f.emitted.borrow_mut().push(args[0].clone());
                        if f.emit_failures.get() > 0 {
                            f.emit_failures.set(f.emit_failures.get() - 1);
                            Err(f.reporter.clone())
                        } else {
                            Ok(u())
                        }
                    }))
                }
                "fetchBalance" => {
                    let f = self.clone();
                    Ok(function("", 0, move |receiver, _| {
                        assert_same(&receiver, &f.receiver);
                        f.balance_calls.set(f.balance_calls.get() + 1);
                        if let Some(error) = &f.sync_failure {
                            return Err(error.clone());
                        }
                        Ok(if f.kind == "validate-pending" {
                            f.pending_balance.clone()
                        } else {
                            ready(
                                f.balance_failure
                                    .as_ref()
                                    .map_or_else(|| Ok(f.balance.clone()), |e| Err(e.clone())),
                            )
                        })
                    }))
                }
                _ => Ok(u()),
            };
        }
        if Rc::ptr_eq(value, &self.xhr) && key == "get" {
            if self.kind == "callee-order" {
                put(&self.endpoints, "ACCOUNT_SUMMARY", s("/probe-summary"));
            }
            let f = self.clone();
            return Ok(function("", 1, move |receiver, args| {
                assert_same(&receiver, &f.xhr);
                f.requests.borrow_mut().push(array(args));
                Ok(ready(if let Some(error) = &f.failure {
                    Err(error.clone())
                } else {
                    Ok(if f.kind == "null-response" {
                        null()
                    } else {
                        object(vec![("data", f.envelope.clone())])
                    })
                }))
            }));
        }
        if Rc::ptr_eq(value, &self.envelope) && key == "data" && self.kind == "live-data" {
            self.reads.set(self.reads.get() + 1);
            return Ok(match self.reads.get() {
                1 => object(vec![("userId", n(0.))]),
                2 => object(vec![("uid", n(61.))]),
                3 => object(vec![("subUserId", n(67.))]),
                4 => object(vec![("referrerUserId", n(71.))]),
                _ => panic!("unexpected data reread"),
            });
        }
        match value.as_ref() {
            Native::U | Native::Null => Err(error(&format!(
                "Cannot read properties of {} (reading '{key}')",
                if matches!(value.as_ref(), Native::Null) {
                    "null"
                } else {
                    "undefined"
                }
            ))),
            Native::O(_) => Ok(at(value, key)),
            Native::F(name, len, _) => Ok(match key {
                "name" => s(name),
                "length" => n(*len as f64),
                _ => u(),
            }),
            Native::P(_) => {
                if key != "catch" {
                    return Ok(u());
                }
                if let Some(error) = &self.catch_failure {
                    return Err(error.clone());
                }
                Ok(function("catch", 1, move |receiver, args| {
                    assert!(
                        matches!(receiver.as_ref(), Native::P(_)),
                        "KUCOIN-ACCOUNT catch receiver"
                    );
                    let handler = args[0].clone();
                    Ok(task(Box::pin(async move {
                        match Await(receiver).await {
                            Ok(value) => Ok(value),
                            Err(error) => call(handler, u(), vec![error]),
                        }
                    })))
                }))
            }
            _ => Ok(u()),
        }
    }
    fn frame(self: &Rc<Self>) -> Rc<StreamOps<V, E>> {
        let read = self.clone();
        let capture = self.clone();
        let intrinsic = self.clone();
        let callbacks = self.clone();
        Rc::new(StreamOps {
            get: Rc::new(move |value, key| read.read(value, key)),
            set: Rc::new(|_, _, _| panic!("no assignments")),
            capture: Rc::new(move |path| {
                let mut parts = path.split('.');
                let first = parts.next().unwrap();
                let mut value = match first {
                    "this" => capture.receiver.clone(),
                    "KUCOIN_SPOT_ENDPOINTS" => capture.endpoints.clone(),
                    _ => panic!("unbound global"),
                };
                let mut receiver = u();
                for key in parts {
                    receiver = value.clone();
                    value = capture.read(&value, key)?;
                }
                if matches!(value.as_ref(), Native::F(..)) {
                    let f = value;
                    Ok(function("", 0, move |_, args| {
                        call(f.clone(), receiver.clone(), args)
                    }))
                } else {
                    Ok(value)
                }
            }),
            invoke: Rc::new(|value, args| call(value, u(), args)),
            intrinsic: Rc::new(move |name, args| match name {
                "null" => Ok(null()),
                "destructure-data" => {
                    if matches!(args[0].as_ref(), Native::Null) {
                        Err(error(NULL_BINDING))
                    } else {
                        intrinsic.read(&args[0], "data")
                    }
                }
                "call-with-this" => call(args[0].clone(), args[1].clone(), rows(&args[2]).to_vec()),
                _ => panic!("unused intrinsic {name}"),
            }),
            truthy: Rc::new(truthy),
            nullish: Rc::new(|value| matches!(value.as_ref(), Native::U | Native::Null)),
            equal: Rc::new(equal),
            literal: Rc::new(s),
            number: Rc::new(n),
            undefined: Rc::new(u),
            array: Rc::new(array),
            object: Rc::new(object),
            callback: Rc::new(move |name, len, body| {
                callbacks.callbacks.borrow_mut().push((name.into(), len));
                function(name, len, move |_, args| {
                    body(args.first().cloned().unwrap_or_else(u))?;
                    Ok(u())
                })
            }),
            map: Rc::new(|_, _| panic!("no map")),
            filter: Rc::new(|_, _| panic!("no filter")),
            for_of: Rc::new(|_, _| panic!("no iteration")),
            await_value: Rc::new(|value| Box::pin(Await(value))),
            spawn: Rc::new(|_| panic!("no spawn")),
            error_value: Rc::new(Clone::clone),
        })
    }
    fn validate(&self) -> bool {
        self.kind.starts_with("validate-")
            || ["balance-rejection", "balance-sync-throw", "catch-getter"].contains(&self.kind)
    }
    fn run(self: &Rc<Self>) -> Result<V, E> {
        let mut task: Task = if self.validate() {
            Box::pin(spot_native_accounts::validate_account(self.frame()))
        } else {
            Box::pin(spot_native_accounts::get_account(self.frame()))
        };
        let mut result = poll(&mut task);
        if self.kind == "validate-pending" {
            assert!(
                matches!(result, Poll::Pending),
                "KUCOIN-ACCOUNT pending balance await"
            );
            assert_eq!(self.balance_calls.get(), 1);
            assert!(self.emitted.borrow().is_empty());
            put(&self.balance, "free", n(77.));
            let Native::P(promise) = self.pending_balance.as_ref() else {
                unreachable!()
            };
            *promise.result.borrow_mut() = Some(Ok(self.balance.clone()));
            result = poll(&mut task);
        }
        let Poll::Ready(result) = result else {
            panic!("unexpected pending")
        };
        result
    }
    fn assert_native(&self, result: &Result<V, E>) {
        assert_eq!(
            self.requests.borrow().len(),
            1,
            "KUCOIN-ACCOUNT one request"
        );
        let requests = self.requests.borrow();
        let args = rows(&requests[0]);
        assert_eq!(
            args.len(),
            if self.validate() { 2 } else { 1 },
            "KUCOIN-ACCOUNT request arity"
        );
        assert_s(
            &args[0],
            if self.validate() {
                "/api/v1/accounts"
            } else if self.kind == "callee-order" {
                "/probe-summary"
            } else {
                "/api/v2/user-info"
            },
        );
        if self.validate() {
            assert_eq!(keys(&args[1]), ["params"]);
            let params = at(&args[1], "params");
            assert_eq!(keys(&params), ["type"]);
            assert_s(&at(&params, "type"), "trade");
        }
        drop(requests);
        if self.kind == "emit-propagates" {
            let Err(error) = result else {
                panic!("KUCOIN-ACCOUNT reporting failure escaped")
            };
            assert_same(error, &self.reporter);
            assert_eq!(self.emitted.borrow().len(), 1);
            assert_s(&self.emitted.borrow()[0], "first");
            return;
        }
        let Ok(value) = result else {
            panic!("unexpected error")
        };
        let expected_emissions = if [
            "raw-rejection",
            "raw-failure",
            "validate-raw-catch",
            "balance-sync-throw",
            "catch-getter",
            "null-response",
        ]
        .contains(&self.kind)
        {
            1
        } else if self.kind == "emit-throws" {
            2
        } else {
            0
        };
        assert_eq!(
            self.emitted.borrow().len(),
            expected_emissions,
            "KUCOIN-ACCOUNT independent emission count"
        );
        match self.kind {
            "strings" => {
                assert_eq!(keys(value), ["userId", "subId", "affiliateId"]);
                for (k, want) in [
                    ("userId", "user-11"),
                    ("subId", "sub-13"),
                    ("affiliateId", "affiliate-17"),
                ] {
                    assert_s(&at(value, k), want);
                }
            }
            "native-identities" => {
                assert_eq!(keys(value), ["userId", "subId", "affiliateId"]);
                assert_n(&at(value, "userId"), 31.);
                assert!(matches!(at(value, "subId").as_ref(), Native::B(true)));
                assert_same(&at(value, "affiliateId"), &self.raw);
            }
            "falsy-uid" | "live-data" => {
                assert_eq!(keys(value), ["userId", "subId", "affiliateId"]);
                assert_n(
                    &at(value, "userId"),
                    if self.kind == "live-data" { 61. } else { 53. },
                );
                if self.kind == "live-data" {
                    assert_n(&at(value, "subId"), 67.);
                    assert_n(&at(value, "affiliateId"), 71.);
                    assert_eq!(self.reads.get(), 4, "KUCOIN-ACCOUNT live data getter count");
                } else {
                    for key in ["subId", "affiliateId"] {
                        assert!(matches!(at(value, key).as_ref(), Native::Null));
                    }
                }
            }
            "surrogate-id" => {
                assert_eq!(keys(value), ["userId", "subId", "affiliateId"]);
                for (key, want) in [
                    ("userId", vec![0xd800]),
                    ("subId", vec![0xdfff]),
                    ("affiliateId", vec![0x61, 0xd800, 0x62]),
                ] {
                    assert_units(&at(value, key), &want);
                }
            }
            "validate-alias" | "validate-pending" | "balance-rejection" => {
                assert_eq!(
                    keys(value),
                    ["error", "balance"],
                    "KUCOIN-ACCOUNT balance property presence"
                );
                assert_s(&at(value, "error"), "");
                if self.kind == "balance-rejection" {
                    assert!(matches!(at(value, "balance").as_ref(), Native::U));
                } else {
                    assert_same(&at(value, "balance"), &self.balance);
                    if self.kind == "validate-alias" {
                        put(&self.balance, "free", n(77.));
                    }
                    assert_n(&at(&at(value, "balance"), "free"), 77.);
                }
            }
            "validate-infinity" => {
                assert_eq!(
                    keys(value),
                    ["error"],
                    "KUCOIN-ACCOUNT native rejection shape"
                );
                assert_n(&at(value, "error"), f64::INFINITY);
            }
            "validate-numeric-code"
            | "validate-raw-catch"
            | "balance-sync-throw"
            | "catch-getter" => {
                assert_eq!(
                    keys(value),
                    ["error"],
                    "KUCOIN-ACCOUNT native rejection shape"
                );
                assert_same(&at(value, "error"), &self.raw);
                if self.kind != "validate-numeric-code" {
                    assert_same(&self.emitted.borrow()[0], &self.raw);
                }
                if self.kind == "validate-raw-catch" {
                    put(&self.raw, "marker", n(97.));
                    assert_n(&at(&at(value, "error"), "marker"), 97.);
                    assert_n(&at(&self.emitted.borrow()[0], "marker"), 97.);
                }
            }
            _ => {
                assert_eq!(keys(value), ["userId", "subId", "affiliateId"]);
                assert_s(
                    &at(value, "userId"),
                    if self.kind == "callee-order" {
                        "user-11"
                    } else {
                        ""
                    },
                );
                for key in ["subId", "affiliateId"] {
                    if self.kind == "callee-order" {
                        assert_s(
                            &at(value, key),
                            if key == "subId" {
                                "sub-13"
                            } else {
                                "affiliate-17"
                            },
                        );
                    } else {
                        assert!(matches!(at(value, key).as_ref(), Native::Null));
                    }
                }
                if ["raw-rejection", "raw-failure"].contains(&self.kind) {
                    assert_same(&self.emitted.borrow()[0], &self.raw);
                }
                if self.kind == "emit-throws" {
                    assert_s(&self.emitted.borrow()[0], "first");
                    assert_same(&self.emitted.borrow()[1], &self.raw);
                }
                if self.kind == "null-response" {
                    assert_s(&self.emitted.borrow()[0], NULL_BINDING);
                }
            }
        }
        let balance_called = [
            "validate-alias",
            "validate-pending",
            "balance-rejection",
            "balance-sync-throw",
            "catch-getter",
        ]
        .contains(&self.kind);
        assert_eq!(
            self.balance_calls.get(),
            usize::from(balance_called),
            "KUCOIN-ACCOUNT independent balance calls"
        );
        let catches =
            ["validate-alias", "validate-pending", "balance-rejection"].contains(&self.kind);
        assert_eq!(
            *self.callbacks.borrow(),
            if catches {
                vec![(String::new(), 0)]
            } else {
                vec![]
            },
            "KUCOIN-ACCOUNT catch handler metadata/allocation order"
        );
    }
    fn projection(&self, result: &Result<V, E>) -> Value {
        json!({"result":pack(&result.as_ref().map_or_else(|_|u(),Clone::clone)),"thrown":pack(&result.as_ref().err().cloned().unwrap_or_else(u)),"emitted":self.emitted.borrow().iter().map(pack).collect::<Vec<_>>(),"requests":self.requests.borrow().iter().map(pack).collect::<Vec<_>>(),"callbacks":*self.callbacks.borrow(),"reads":self.reads.get(),"balanceCalls":self.balance_calls.get()})
    }
}
#[test]
fn native_spot_account_values_precede_source_projection() {
    let cases = [
        "strings",
        "native-identities",
        "falsy-uid",
        "missing-data",
        "raw-rejection",
        "surrogate-id",
        "raw-failure",
        "emit-throws",
        "emit-propagates",
        "callee-order",
        "live-data",
        "validate-alias",
        "validate-numeric-code",
        "validate-infinity",
        "validate-raw-catch",
        "balance-rejection",
        "balance-sync-throw",
        "catch-getter",
        "null-response",
        "validate-pending",
    ];
    assert_eq!(cases.len(), 20);
    let native = cases.map(|kind| {
        let fixture = Fixture::new(kind);
        let result = fixture.run();
        fixture.assert_native(&result);
        (fixture, result)
    });
    for (index, (fixture, result)) in native.iter().enumerate() {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/spot-native-accounts.cjs"
            ))
            .arg(cases[index])
            .output()
            .expect("source oracle");
        assert!(
            output.status.success(),
            "source {}: {}",
            cases[index],
            String::from_utf8_lossy(&output.stderr)
        );
        let source: Value = serde_json::from_slice(&output.stdout).expect("source capture");
        assert_eq!(
            fixture.projection(result),
            source,
            "source parity case {index}/{}",
            cases[index]
        );
    }
}
