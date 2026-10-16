use safe_cex_types::{MarginType, OrderStatus, PositionMode, PositionSide};
use safe_cex_venue_paradex::private_ingress::{self as api, PrivateIngressHost as Host};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    process::Command,
    rc::Rc,
};
#[derive(Debug)]
struct Failure {
    tag: &'static str,
    message: Vec<u16>,
}
type Error = Rc<Failure>;
type Raw = Rc<Node>;
#[derive(Debug)]
enum Node {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    Text(Vec<u16>),
    Object(&'static str, RefCell<BTreeMap<String, Raw>>),
    Array(RefCell<Vec<Raw>>),
    Function(usize),
    Prototype(&'static str),
}
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn surrogate(s: &str) -> Vec<u16> {
    u(s).into_iter().chain([0xd800]).collect()
}
fn raw(s: &str) -> Raw {
    Rc::new(Node::Text(u(s)))
}
fn raw_surrogate(s: &str) -> Raw {
    Rc::new(Node::Text(surrogate(s)))
}
fn obj(label: &'static str, fields: Vec<(&str, Raw)>) -> Raw {
    Rc::new(Node::Object(
        label,
        RefCell::new(fields.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()),
    ))
}
fn arr(values: Vec<Raw>) -> Raw {
    Rc::new(Node::Array(RefCell::new(values)))
}
fn undef() -> Raw {
    Rc::new(Node::Undefined)
}
fn bits(n: f64) -> String {
    format!(
        "{:016x}",
        if n.is_nan() {
            0x7ff8000000000000
        } else {
            n.to_bits()
        }
    )
}
fn number(v: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap())
}
fn decode(v: &Value) -> Vec<u16> {
    serde_json::from_value(v.clone()).unwrap()
}
fn atom(v: &Raw) -> Value {
    match v.as_ref() {
        Node::Undefined => json!({"kind":"undefined"}),
        Node::Null => json!({"kind":"null"}),
        Node::Bool(v) => json!({"kind":"boolean","value":v}),
        Node::Number(n) => num(*n),
        Node::Text(v) => json!({"kind":"string","units":v}),
        Node::Object(label, _) => json!({"kind":"reference","tag":label}),
        Node::Prototype(k) => json!({"kind":"reference","tag":format!("prototype:{k}")}),
        _ => panic!("unexpected native atom"),
    }
}
fn num(n: f64) -> Value {
    json!({"kind":"number","bits":bits(n)})
}
fn outcome(result: &Result<(), Error>, expected: &Value, primary: &Error) {
    match result {
        Ok(()) => assert_eq!(expected["ok"], true, "ingress_native_success"),
        Err(e) => {
            assert_eq!(expected["ok"], false, "ingress_native_failure");
            assert_eq!(e.tag, expected["error"], "ingress_native_error_tag");
            if e.tag == "primary" {
                assert!(Rc::ptr_eq(e, primary), "ingress_native_error_identity");
            }
        }
    }
}
fn status_text(s: OrderStatus) -> &'static str {
    match s {
        OrderStatus::Open => "open",
        OrderStatus::Closed => "closed",
        OrderStatus::Canceled => "canceled",
        _ => panic!("unexpected status"),
    }
}
fn proto(key: &[u16]) -> Option<(Raw, bool)> {
    for name in ["constructor", "toString", "valueOf", "__proto__"] {
        if key == u(name) {
            return Some((Rc::new(Node::Prototype(name)), true));
        }
    }
    if key == u("__truthyFixture") {
        return Some((raw_surrogate("proto-"), true));
    }
    if key == u("__falsyFixture") {
        return Some((Rc::new(Node::Number(0.0)), false));
    }
    None
}
fn lower(input: Vec<u16>) -> Vec<u16> {
    let mut result = vec![];
    let mut i = 0;
    while i < input.len() {
        if input[i] == 304 {
            result.extend([105, 775]);
        } else if input[i] == 0xd801 && input.get(i + 1) == Some(&0xdc00) {
            result.extend([0xd801, 0xdc28]);
            i += 1;
        } else {
            result.push(if (65..=90).contains(&input[i]) {
                input[i] + 32
            } else {
                input[i]
            });
        }
        i += 1;
    }
    result
}
fn pure(row: &Value) {
    let key = decode(&row["input"]);
    let inherited = RefCell::new(None);
    let lookup = |key: &[u16]| -> Result<Option<(Raw, bool)>, Error> {
        let value = proto(key);
        if let Some((ref handle, true)) = value {
            *inherited.borrow_mut() = Some(handle.clone());
        }
        Ok(value)
    };
    let value = if row["kind"] == "mapOrderStatus" {
        match api::map_private_status::<Raw, Error>(&key, lookup).unwrap() {
            api::PrivateStatus::Status(status) => raw(status_text(status)),
            api::PrivateStatus::Inherited(v) => {
                assert!(proto(&key).unwrap().1, "status_native_inherited_truthiness");
                v
            }
        }
    } else {
        match api::format_cancel_reason::<Raw, Error>(&key, lookup, |replaced| {
            assert_eq!(
                replaced,
                key.iter()
                    .map(|c| if *c == 95 { 32 } else { *c })
                    .collect::<Vec<_>>(),
                "reason_native_replacement"
            );
            Ok(lower(replaced))
        })
        .unwrap()
        {
            api::CancelReason::Text(value) => Rc::new(Node::Text(value)),
            api::CancelReason::Inherited(value) => value,
        }
    };
    if let Some(expected) = inherited.borrow().as_ref() {
        assert!(
            Rc::ptr_eq(&value, expected),
            "helper_native_inherited_identity"
        );
    }
    assert_eq!(
        atom(&value),
        row["outcome"]["value"],
        "{}",
        if row["kind"] == "mapOrderStatus" {
            "status_native_value"
        } else {
            "reason_native_value"
        }
    );
}
struct Frame {
    kind: String,
    profile: String,
    failure: String,
    trace: RefCell<Vec<Vec<u16>>>,
    counts: RefCell<BTreeMap<Vec<u16>, usize>>,
    primary: Error,
    data: Raw,
    raw: Raw,
    logs: RefCell<Vec<Value>>,
    orders: RefCell<Vec<Value>>,
    positions: RefCell<Vec<Value>>,
    balances: RefCell<Vec<Value>>,
    rejections: RefCell<Vec<Vec<u16>>>,
    pending: Cell<bool>,
    refresh_orders: Cell<usize>,
    refresh_positions: Cell<usize>,
    authenticated: Cell<bool>,
    auth_id: Raw,
    connected: bool,
    disposed: bool,
    closed: Cell<bool>,
    subscribed: Cell<usize>,
    handlers: Raw,
    stats: RefCell<Vec<Vec<u16>>>,
    calls: RefCell<Vec<Value>>,
}
impl Frame {
    fn touch(&self, base: &str) -> Result<(), Error> {
        self.touch_units(&u(base))
    }
    fn touch_units(&self, base: &[u16]) -> Result<(), Error> {
        let mut counts = self.counts.borrow_mut();
        let n = counts.entry(base.to_vec()).or_default();
        let label: Vec<u16> = base.iter().copied().chain(u(&format!("#{n}"))).collect();
        *n += 1;
        drop(counts);
        self.trace.borrow_mut().push(label.clone());
        if label == u(&self.failure) {
            Err(self.primary.clone())
        } else {
            Ok(())
        }
    }
    fn units(&self, v: &Raw) -> Vec<u16> {
        match v.as_ref() {
            Node::Text(v) => v.clone(),
            Node::Undefined => u("undefined"),
            Node::Null => u("null"),
            Node::Bool(v) => u(if *v { "true" } else { "false" }),
            Node::Number(v) => u(if v.is_nan() {
                "NaN".to_owned()
            } else if *v == f64::INFINITY {
                "Infinity".to_owned()
            } else if *v == f64::NEG_INFINITY {
                "-Infinity".to_owned()
            } else if *v == 0.0 {
                "0".to_owned()
            } else {
                v.to_string()
            }
            .as_str()),
            Node::Prototype("constructor") => u("function Object() { [native code] }"),
            Node::Prototype("__proto__") => u("[object Object]"),
            Node::Prototype("toString") => u("function toString() { [native code] }"),
            Node::Prototype("valueOf") => u("function valueOf() { [native code] }"),
            Node::Object(_, _) => u("[object Object]"),
            _ => panic!("unobserved template receipt"),
        }
    }
    fn decimal(&self, v: &Raw) -> f64 {
        let value = self.units(v);
        for (s, n) in [
            ("101.25", 101.25),
            ("99.5", 99.5),
            ("5.5", 5.5),
            ("-5.5", -5.5),
            ("7.25", 7.25),
            ("2.5", 2.5),
            ("3.125", 3.125),
            ("12.5", 12.5),
            ("3.25", 3.25),
            ("8", 8.0),
            ("50.125", 50.125),
            ("100", 100.0),
            ("70", 70.0),
            ("12", 12.0),
            ("123.5", 123.5),
            ("80.25", 80.25),
            ("20", 20.0),
            ("0", 0.0),
            ("-0", -0.0),
            ("Infinity", f64::INFINITY),
        ] {
            if value == u(s) {
                return n;
            }
        }
        f64::NAN
    }
    fn read_field(&self, value: &Raw, key: &str) -> Result<Raw, Error> {
        match value.as_ref() {
            Node::Object(label, fields) => {
                let read = self
                    .counts
                    .borrow()
                    .get(&u(&format!("{label}.{key}")))
                    .copied()
                    .unwrap_or(0);
                self.touch(&format!("{label}.{key}"))?;
                if *label == "raw" {
                    if self.profile == "reduce-changing" && key == "reduce_only" && read > 0 {
                        return Ok(raw_surrogate("retained-"));
                    }
                    if self.profile == "flags-changing" && key == "flags" && read > 0 {
                        return Ok(obj(
                            "flags-replacement",
                            vec![("includes", Rc::new(Node::Function(0)))],
                        ));
                    }
                    if self.profile == "changing-size" && key == "size" && read > 0 {
                        return Ok(raw("7.25"));
                    }
                    if self.profile == "changing-remaining" && key == "remaining_size" && read > 0 {
                        return Ok(raw("3.125"));
                    }
                    if self.profile == "changing-type" && key == "type" && read > 0 {
                        return Ok(raw("STOP_MARKET"));
                    }
                    if self.profile == "changing-side" && key == "side" && read > 0 {
                        return Ok(raw("SHORT"));
                    }
                }
                Ok(fields.borrow().get(key).cloned().unwrap_or_else(undef))
            }
            Node::Undefined | Node::Null => Err(Rc::new(Failure {
                tag: "TypeError",
                message: u(&format!(
                    "TypeError: Cannot read properties of {} (reading '{key}')",
                    if matches!(value.as_ref(), Node::Null) {
                        "null"
                    } else {
                        "undefined"
                    }
                )),
            })),
            _ => Ok(undef()),
        }
    }
    fn new(kind: &str, profile: &str, failure: &str) -> Self {
        let mut f = Self {
            kind: kind.to_owned(),
            profile: profile.to_owned(),
            failure: failure.to_owned(),
            trace: RefCell::new(vec![]),
            counts: RefCell::new(BTreeMap::new()),
            primary: Rc::new(Failure {
                tag: "primary",
                message: surrogate("cause-"),
            }),
            data: undef(),
            raw: undef(),
            logs: RefCell::new(vec![]),
            orders: RefCell::new(vec![]),
            positions: RefCell::new(vec![]),
            balances: RefCell::new(vec![]),
            rejections: RefCell::new(vec![]),
            pending: Cell::new(false),
            refresh_orders: Cell::new(0),
            refresh_positions: Cell::new(0),
            authenticated: Cell::new(false),
            auth_id: undef(),
            connected: true,
            disposed: false,
            closed: Cell::new(false),
            subscribed: Cell::new(0),
            handlers: arr(vec![]),
            stats: RefCell::new(vec![]),
            calls: RefCell::new(vec![]),
        };
        if kind == "order" {
            let fields = vec![
                ("id", raw_surrogate("id-")),
                ("market", raw_surrogate("BTC-")),
                ("side", raw("BUY")),
                (
                    "status",
                    raw(if profile == "status-prototype" {
                        "constructor"
                    } else {
                        "NEW"
                    }),
                ),
                (
                    "type",
                    raw(if ["stop", "stop-zero"].contains(&profile) {
                        "STOP_MARKET"
                    } else {
                        "LIMIT"
                    }),
                ),
                (
                    "reduce_only",
                    if profile == "reduce-false" {
                        Rc::new(Node::Bool(false))
                    } else if ["reduce-flag", "reduce-missing", "flags-changing"].contains(&profile)
                    {
                        undef()
                    } else {
                        Rc::new(Node::Bool(true))
                    },
                ),
                (
                    "flags",
                    if profile == "reduce-missing" {
                        undef()
                    } else {
                        arr(vec![raw("REDUCE_ONLY")])
                    },
                ),
                (
                    "price",
                    raw(if profile == "no-price" { "" } else { "101.25" }),
                ),
                (
                    "trigger_price",
                    raw(if profile == "stop-zero" { "0" } else { "99.5" }),
                ),
                ("size", raw("5.5")),
                ("remaining_size", raw("2.5")),
                ("time_in_force", raw_surrogate("GTC-")),
                (
                    "cancel_reason",
                    raw(if profile == "cancel" {
                        "INSUFFICIENT_MARGIN"
                    } else {
                        ""
                    }),
                ),
            ];
            f.raw = obj("raw", fields);
            f.data = obj("data", vec![("data", f.raw.clone())]);
        } else if kind == "position" {
            f.raw = if profile == "missing" {
                Rc::new(Node::Null)
            } else {
                obj(
                    "raw",
                    vec![
                        (
                            "market",
                            if profile == "missing-market" {
                                raw("")
                            } else {
                                raw_surrogate("BTC-")
                            },
                        ),
                        (
                            "side",
                            raw(if profile == "short" { "SHORT" } else { "LONG" }),
                        ),
                        (
                            "status",
                            raw(if profile == "closed" {
                                "CLOSED"
                            } else {
                                "OPEN"
                            }),
                        ),
                        (
                            "size",
                            raw(match profile {
                                "negative" => "-5.5",
                                "zero" => "-0",
                                "nan" => "NaN",
                                _ => "5.5",
                            }),
                        ),
                        (
                            "average_entry_price",
                            raw(if profile == "nan" { "NaN" } else { "101.25" }),
                        ),
                        ("unrealized_pnl", raw("12.5")),
                        (
                            "realized_positional_pnl",
                            raw(if profile == "fallback" { "" } else { "3.25" }),
                        ),
                        (
                            "leverage",
                            if profile == "fallback" {
                                Rc::new(Node::Null)
                            } else {
                                raw("8")
                            },
                        ),
                        (
                            "liquidation_price",
                            if profile == "fallback" {
                                undef()
                            } else {
                                raw("50.125")
                            },
                        ),
                        (
                            "last_updated_at",
                            if profile == "fallback" {
                                Rc::new(Node::Number(0.0))
                            } else {
                                raw_surrogate("updated-")
                            },
                        ),
                        ("created_at", raw_surrogate("created-")),
                    ],
                )
            };
            f.data = obj("data", vec![("data", f.raw.clone())]);
        } else if kind == "balance" {
            let total = match profile {
                "fallback" => "bad",
                "negative-zero" => "-0",
                "negative-difference" => "20",
                "nonfinite" => "Infinity",
                _ => "100",
            };
            let free = match profile {
                "fallback" => "Infinity",
                "negative-zero" => "0",
                "nonfinite" => "NaN",
                _ => "70",
            };
            f.raw = obj(
                "raw",
                vec![
                    ("total_collateral", raw(total)),
                    (
                        "account_value",
                        if profile == "fallback" {
                            raw("123.5")
                        } else {
                            undef()
                        },
                    ),
                    ("equity", undef()),
                    ("margin_balance", undef()),
                    ("wallet_balance", undef()),
                    ("free_collateral", raw(free)),
                    (
                        "free",
                        if profile == "fallback" {
                            raw("80.25")
                        } else {
                            undef()
                        },
                    ),
                    ("available_margin", undef()),
                    ("available_collateral", undef()),
                    (
                        "initial_margin_requirement",
                        if profile == "explicit" {
                            raw("12")
                        } else {
                            undef()
                        },
                    ),
                    ("position_margin", undef()),
                    ("used_margin", undef()),
                ],
            );
            f.data = match profile {
                "wrapped" => obj("data", vec![("data", f.raw.clone())]),
                "array" => obj(
                    "data",
                    vec![(
                        "data",
                        arr(vec![
                            Rc::new(Node::Null),
                            Rc::new(Node::Number(0.0)),
                            raw("skip"),
                            f.raw.clone(),
                        ]),
                    )],
                ),
                "array-empty" => obj(
                    "data",
                    vec![(
                        "data",
                        arr(vec![
                            Rc::new(Node::Null),
                            Rc::new(Node::Number(0.0)),
                            raw("skip"),
                        ]),
                    )],
                ),
                "invalid" => undef(),
                "nested" => obj(
                    "data",
                    vec![("data", obj("wrapper", vec![("data", f.raw.clone())]))],
                ),
                _ => f.raw.clone(),
            };
        } else {
            let params = obj(
                "params",
                vec![(
                    "channel",
                    if profile == "unknown-channel" {
                        raw_surrogate("unknown-")
                    } else {
                        raw("orders.ALL")
                    },
                )],
            );
            let result = obj(
                "result",
                if profile == "result-channel" {
                    vec![("channel", raw_surrogate("orders.ALL-"))]
                } else {
                    vec![]
                },
            );
            let error = obj("error", vec![("message", raw_surrogate("reason-"))]);
            let subscription = [
                "subscription",
                "growing",
                "replacing",
                "unknown-channel",
                "handler-error",
            ]
            .contains(&profile);
            let auth = [
                "auth",
                "auth-disconnected",
                "auth-no-socket",
                "auth-already",
                "wrong-id",
                "auth-error",
                "result-null",
                "result-channel",
            ]
            .contains(&profile);
            f.disposed = profile == "disposed";
            f.connected = !["auth-disconnected", "auth-no-socket"].contains(&profile);
            f.authenticated.set(profile == "auth-already");
            f.auth_id = if auth && profile != "result-channel" {
                Rc::new(Node::Number(5.0))
            } else {
                undef()
            };
            f.data = if profile == "empty" {
                Rc::new(Node::Null)
            } else {
                obj(
                    "data",
                    vec![
                        (
                            "jsonrpc",
                            if profile == "legacy" {
                                undef()
                            } else {
                                raw("2.0")
                            },
                        ),
                        (
                            "method",
                            raw(if subscription { "subscription" } else { "none" }),
                        ),
                        ("params", if subscription { params } else { undef() }),
                        (
                            "id",
                            Rc::new(Node::Number(if profile == "wrong-id" { 6.0 } else { 5.0 })),
                        ),
                        (
                            "result",
                            if profile == "result-null" {
                                Rc::new(Node::Null)
                            } else if auth {
                                result
                            } else {
                                undef()
                            },
                        ),
                        (
                            "error",
                            if ["error", "auth-error"].contains(&profile) {
                                error
                            } else {
                                undef()
                            },
                        ),
                        (
                            "type",
                            if profile == "legacy" {
                                raw("ping")
                            } else {
                                undef()
                            },
                        ),
                        ("channel", undef()),
                    ],
                )
            };
            f.handlers = arr((0..if ["growing", "replacing"].contains(&profile) {
                2
            } else {
                1
            })
                .map(|i| Rc::new(Node::Function(i)))
                .collect());
        }
        f
    }
}
fn native_n(actual: f64, expected: f64, label: &str) {
    assert_eq!(bits(actual), bits(expected), "{label}");
}
impl Host for Frame {
    type Value = Raw;
    type Error = Error;
    type Log = Rc<()>;
    fn get(&self, v: &Raw, k: &str) -> Result<Raw, Error> {
        self.read_field(v, k)
    }
    fn truthy(&self, v: &Raw) -> bool {
        match v.as_ref() {
            Node::Undefined | Node::Null => false,
            Node::Bool(v) => *v,
            Node::Number(n) => *n != 0.0 && !n.is_nan(),
            Node::Text(v) => !v.is_empty(),
            _ => true,
        }
    }
    fn nullish(&self, v: &Raw) -> bool {
        matches!(v.as_ref(), Node::Undefined | Node::Null)
    }
    fn undefined(&self, v: &Raw) -> bool {
        matches!(v.as_ref(), Node::Undefined)
    }
    fn object(&self, v: &Raw) -> bool {
        matches!(
            v.as_ref(),
            Node::Object(..) | Node::Array(..) | Node::Null | Node::Prototype("__proto__")
        )
    }
    fn array(&self, v: &Raw) -> Result<bool, Error> {
        Ok(matches!(v.as_ref(), Node::Array(..)))
    }
    fn boolean(&self, v: &Raw) -> Option<bool> {
        if let Node::Bool(v) = v.as_ref() {
            Some(*v)
        } else {
            None
        }
    }
    fn literal(&self, s: &str) -> Raw {
        raw(s)
    }
    fn boolean_value(&self, v: bool) -> Raw {
        Rc::new(Node::Bool(v))
    }
    fn number_template(&self, v: f64) -> Result<Vec<u16>, Error> {
        Ok(self.units(&Rc::new(Node::Number(v))))
    }
    fn equals(&self, a: &Raw, b: &Raw) -> bool {
        match (a.as_ref(), b.as_ref()) {
            (Node::Number(a), Node::Number(b)) => a == b,
            (Node::Bool(a), Node::Bool(b)) => a == b,
            (Node::Text(a), Node::Text(b)) => a == b,
            (Node::Null, Node::Null) | (Node::Undefined, Node::Undefined) => true,
            _ => Rc::ptr_eq(a, b),
        }
    }
    fn parse_float(&self, v: &Raw) -> Result<f64, Error> {
        self.touch(if self.kind == "balance" {
            "number-parse"
        } else {
            "parse-float"
        })?;
        Ok(self.decimal(v))
    }
    fn string(&self, v: &Raw) -> Result<Raw, Error> {
        self.touch("string")?;
        Ok(Rc::new(Node::Text(self.units(v))))
    }
    fn template(&self, v: &Raw) -> Result<Vec<u16>, Error> {
        Ok(self.units(v))
    }
    fn select_log(&self) -> Result<Rc<()>, Error> {
        self.touch("log-read")?;
        Ok(Rc::new(()))
    }
    fn log(&self, _bound: Rc<()>, text: Vec<u16>, severity: &'static str) -> Result<(), Error> {
        self.touch("log")?;
        self.logs
            .borrow_mut()
            .push(json!({"units":text,"severity":severity}));
        Ok(())
    }
    fn log_caught(&self, prefix: &'static str, e: &Error) -> Result<(), Error> {
        let log = self.select_log()?;
        if Rc::ptr_eq(e, &self.primary) {
            self.touch("error-string")?;
        }
        let msg = u(prefix)
            .into_iter()
            .chain(e.message.iter().copied())
            .collect();
        self.log(log, msg, "error")
    }
    fn disposed(&self) -> Result<bool, Error> {
        self.touch("disposed")?;
        Ok(self.disposed)
    }
    fn track_received(&self) -> Result<(), Error> {
        self.touch("track")
    }
    fn parse_event(&self) -> Result<Raw, Error> {
        self.touch("event-data")?;
        self.touch("parse-event")?;
        Ok(self.data.clone())
    }
    fn handlers(&self, topic: &Raw) -> Result<Raw, Error> {
        self.touch("handlers-read")?;
        let units = self.units(topic);
        self.touch_units(
            &u("handlers:")
                .into_iter()
                .chain(units.iter().copied())
                .collect::<Vec<_>>(),
        )?;
        if ["orders.ALL", "ping"].iter().any(|k| units == u(k)) {
            Ok(self.handlers.clone())
        } else {
            Ok(undef())
        }
    }
    fn stats(&self, topic: &Raw) -> Result<(), Error> {
        self.touch("stats")?;
        self.stats.borrow_mut().push(self.units(topic));
        Ok(())
    }
    fn for_each_handler(
        &self,
        handlers: &Raw,
        mut callback: impl FnMut(&Raw) -> Result<(), Error>,
    ) -> Result<(), Error> {
        assert!(
            Rc::ptr_eq(handlers, &self.handlers),
            "message_native_handlers_identity"
        );
        let Node::Array(values) = handlers.as_ref() else {
            panic!("handlers receipt")
        };
        let length = values.borrow().len();
        for i in 0..length {
            let handler = values.borrow().get(i).cloned();
            if let Some(handler) = handler {
                callback(&handler)?;
            }
        }
        Ok(())
    }
    fn call_handler(&self, handler: &Raw, data: &Raw) -> Result<(), Error> {
        let Node::Function(i) = handler.as_ref() else {
            panic!("handler receipt")
        };
        let expected = if self.profile == "legacy" {
            "data"
        } else if self.profile == "replacing" && *i == 1 {
            "replacement"
        } else {
            "params"
        };
        let Node::Object(tag, _) = data.as_ref() else {
            panic!("message_native_argument_object")
        };
        assert_eq!(*tag, expected, "message_native_argument_identity");
        self.touch(&format!("handler-call:{i}"))?;
        if self.profile == "handler-error" {
            return Err(self.primary.clone());
        }
        self.calls.borrow_mut().push(json!({"index":i,"arg":tag}));
        if self.profile == "growing" && *i == 0 {
            let Node::Array(values) = self.handlers.as_ref() else {
                unreachable!()
            };
            values.borrow_mut().push(Rc::new(Node::Function(2)));
        }
        if self.profile == "replacing" && *i == 0 {
            let Node::Object(_, fields) = self.data.as_ref() else {
                unreachable!()
            };
            fields.borrow_mut().insert(
                "params".to_owned(),
                obj("replacement", vec![("channel", raw("orders.ALL"))]),
            );
        }
        Ok(())
    }
    fn auth_id(&self) -> Result<Raw, Error> {
        self.touch("auth-id")?;
        Ok(self.auth_id.clone())
    }
    fn authenticated(&self) -> Result<bool, Error> {
        self.touch("authenticated-read")?;
        Ok(self.authenticated.get())
    }
    fn write_authenticated(&self, v: bool) -> Result<(), Error> {
        self.touch("authenticated-write")?;
        self.authenticated.set(v);
        Ok(())
    }
    fn connected(&self) -> Result<bool, Error> {
        self.touch("connected")?;
        Ok(self.connected)
    }
    fn close_socket(&self) -> Result<(), Error> {
        self.touch("socket")?;
        if self.profile != "auth-no-socket" {
            self.touch("close-read")?;
            self.touch("close")?;
            self.closed.set(true);
        }
        Ok(())
    }
    fn subscribe_private(&self) -> Result<(), Error> {
        self.touch("subscribe")?;
        self.subscribed.set(self.subscribed.get() + 1);
        Ok(())
    }
    fn side_lookup(&self, side: &Raw) -> Result<Raw, Error> {
        assert_eq!(self.units(side), u("BUY"), "order_native_side_argument");
        Ok(raw("buy"))
    }
    fn map_status(&self, order: &Raw) -> Result<Raw, Error> {
        self.touch("map-status-read")?;
        let status = self.get(order, "status")?;
        self.touch("map-status-call")?;
        let result = api::map_private_status::<Raw, Error>(&self.units(&status), |k| Ok(proto(k)))?;
        Ok(match result {
            api::PrivateStatus::Status(status) => raw(status_text(status)),
            api::PrivateStatus::Inherited(value) => value,
        })
    }
    fn flags_include_reduce_only(&self, flags: &Raw) -> Result<Raw, Error> {
        if matches!(flags.as_ref(), Node::Object("flags-replacement", _)) {
            self.get(flags, "includes")?;
            self.touch("flags-includes")?;
            return Ok(raw_surrogate("flag-retained-"));
        }
        let Node::Array(values) = flags.as_ref() else {
            panic!("flags receipt")
        };
        Ok(Rc::new(Node::Bool(
            values
                .borrow()
                .iter()
                .any(|v| self.equals(v, &raw("REDUCE_ONLY"))),
        )))
    }
    fn reverse_type(&self, order: &Raw, reduce: &Raw) -> Result<Raw, Error> {
        self.touch("reverse-read")?;
        let kind = self.get(order, "type")?;
        self.touch("reverse-call")?;
        let mut name = u(if self.truthy(reduce) {
            "reduce:"
        } else {
            "plain:"
        });
        name.extend(self.units(&kind));
        Ok(Rc::new(Node::Text(name)))
    }
    fn is_trigger_type(&self, v: &Raw) -> Result<bool, Error> {
        Ok([
            "STOP_LIMIT",
            "STOP_MARKET",
            "STOP_LOSS_LIMIT",
            "STOP_LOSS_MARKET",
            "TAKE_PROFIT_LIMIT",
            "TAKE_PROFIT_MARKET",
        ]
        .iter()
        .any(|s| self.units(v) == u(s)))
    }
    fn now(&self) -> Result<f64, Error> {
        self.touch("now")?;
        Ok(1000.0)
    }
    fn format_reason(&self, order: &Raw) -> Result<Raw, Error> {
        self.touch("format-reason-read")?;
        let reason = self.get(order, "cancel_reason")?;
        self.touch("format-reason-call")?;
        let result = api::format_cancel_reason::<Raw, Error>(
            &self.units(&reason),
            |k| Ok(proto(k)),
            |v| Ok(lower(v)),
        )?;
        Ok(match result {
            api::CancelReason::Text(v) => Rc::new(Node::Text(v)),
            api::CancelReason::Inherited(v) => v,
        })
    }
    fn emit_rejection(&self, message: Vec<u16>) -> Result<(), Error> {
        let expected = u("Order ")
            .into_iter()
            .chain(surrogate("BTC-"))
            .chain(u(" BUY rejected: Insufficient margin/balance"))
            .collect::<Vec<_>>();
        assert_eq!(message, expected, "order_native_rejection");
        self.touch("emit-read")?;
        self.touch("emit")?;
        self.rejections.borrow_mut().push(message);
        Ok(())
    }
    fn store_order(&self, v: api::PrivateOrder<Raw>) -> Result<(), Error> {
        assert!(Rc::ptr_eq(&v.info, &self.raw), "order_native_info_identity");
        assert_eq!(self.units(&v.id), surrogate("id-"), "order_native_id");
        assert_eq!(
            self.units(&v.symbol),
            surrogate("BTC-"),
            "order_native_symbol"
        );
        assert_eq!(self.units(&v.side), u("buy"), "order_native_side");
        assert_eq!(
            atom(&v.status),
            if self.profile == "status-prototype" {
                atom(&Rc::new(Node::Prototype("constructor")))
            } else {
                atom(&raw("open"))
            },
            "order_native_status"
        );
        let prefix = if ["reduce-false", "reduce-missing"].contains(&self.profile.as_str()) {
            "plain:"
        } else {
            "reduce:"
        };
        let first_kind = if ["stop", "stop-zero"].contains(&self.profile.as_str()) {
            "STOP_MARKET"
        } else {
            "LIMIT"
        };
        assert_eq!(
            self.units(&v.order_type),
            u(&format!("{prefix}{first_kind}")),
            "order_native_type"
        );
        native_n(
            v.price,
            match self.profile.as_str() {
                "stop" | "changing-type" => 99.5,
                "stop-zero" | "no-price" => 0.0,
                _ => 101.25,
            },
            "order_native_price",
        );
        native_n(v.amount, 5.5, "order_native_amount");
        native_n(
            v.filled,
            if self.profile == "changing-size" {
                4.75
            } else {
                3.0
            },
            "order_native_filled",
        );
        native_n(
            v.remaining,
            if self.profile == "changing-remaining" {
                3.125
            } else {
                2.5
            },
            "order_native_remaining",
        );
        assert_eq!(
            atom(&v.reduce_only),
            if self.profile == "flags-changing" {
                atom(&raw_surrogate("flag-retained-"))
            } else if self.profile == "reduce-changing" {
                atom(&raw_surrogate("retained-"))
            } else {
                atom(&Rc::new(Node::Bool(
                    !["reduce-false", "reduce-missing"].contains(&self.profile.as_str()),
                )))
            },
            "order_native_reduce_only"
        );
        assert_eq!(
            self.units(&v.time_in_force),
            surrogate("GTC-"),
            "order_native_tif"
        );
        native_n(v.last_update, 1000.0, "order_native_last_update");
        self.touch("store-read")?;
        self.touch("store-order-read")?;
        self.touch("store-order")?;
        self.orders.borrow_mut().push(json!({"id":atom(&v.id),"symbol":atom(&v.symbol),"side":atom(&v.side),"type":atom(&v.order_type),"status":atom(&v.status),"price":num(v.price),"amount":num(v.amount),"filled":num(v.filled),"remaining":num(v.remaining),"reduceOnly":atom(&v.reduce_only),"timeInForce":atom(&v.time_in_force),"lastUpdate":num(v.last_update),"info":atom(&v.info)}));
        Ok(())
    }
    fn refresh_orders(&self) -> Result<(), Error> {
        self.touch("refresh-orders")?;
        self.refresh_orders.set(self.refresh_orders.get() + 1);
        Ok(())
    }
    fn write_pending_positions(&self, v: bool) -> Result<(), Error> {
        assert!(v, "position_native_pending_value");
        self.touch("pending-write")?;
        self.pending.set(v);
        Ok(())
    }
    fn position_id(
        &self,
        symbol: &Raw,
        side: PositionSide,
        mode: PositionMode,
    ) -> Result<Raw, Error> {
        assert_eq!(
            self.units(symbol),
            surrogate("BTC-"),
            "position_native_id_symbol"
        );
        assert_eq!(
            side,
            if self.profile == "short" {
                PositionSide::Short
            } else {
                PositionSide::Long
            },
            "position_native_id_side"
        );
        assert_eq!(mode, PositionMode::OneWay, "position_native_id_mode");
        Ok(symbol.clone())
    }
    fn store_position(&self, v: api::PrivatePosition<Raw>) -> Result<(), Error> {
        assert!(
            Rc::ptr_eq(&v.info, &self.raw),
            "position_native_info_identity"
        );
        assert!(Rc::ptr_eq(&v.id, &v.symbol), "position_native_id_identity");
        assert_eq!(self.units(&v.symbol), surrogate("BTC-"));
        assert_eq!(
            v.side,
            if ["short", "changing-side"].contains(&self.profile.as_str()) {
                PositionSide::Short
            } else {
                PositionSide::Long
            },
            "position_native_side"
        );
        let nan = self.profile == "nan";
        let fallback = self.profile == "fallback";
        native_n(
            v.contracts,
            if nan { f64::NAN } else { 5.5 },
            "position_native_contracts",
        );
        native_n(
            v.notional,
            if nan { f64::NAN } else { 556.875 },
            "position_native_notional",
        );
        native_n(v.unrealized_pnl, 12.5, "position_native_unrealized");
        native_n(
            v.realized_pnl,
            if fallback { 0.0 } else { 3.25 },
            "position_native_realized",
        );
        native_n(
            v.leverage,
            if fallback { 0.0 } else { 8.0 },
            "position_native_leverage",
        );
        native_n(
            v.entry_price,
            if nan { f64::NAN } else { 101.25 },
            "position_native_entry",
        );
        native_n(
            v.liquidation_price,
            if fallback { 0.0 } else { 50.125 },
            "position_native_liquidation",
        );
        assert_eq!(v.margin_type, MarginType::Cross);
        assert_eq!(v.position_mode, PositionMode::OneWay);
        assert_eq!(
            self.units(&v.last_update),
            surrogate(if fallback { "created-" } else { "updated-" }),
            "position_native_last_update"
        );
        self.touch("store-read")?;
        self.touch("store-position-read")?;
        self.touch("store-position")?;
        self.positions.borrow_mut().push(json!({"id":atom(&v.id),"symbol":atom(&v.symbol),"side":atom(&raw(if v.side==PositionSide::Long{"long"}else{"short"})),"contracts":num(v.contracts),"notional":num(v.notional),"unrealizedPnl":num(v.unrealized_pnl),"realizedPnl":num(v.realized_pnl),"leverage":num(v.leverage),"entryPrice":num(v.entry_price),"liquidationPrice":num(v.liquidation_price),"marginType":atom(&raw("cross")),"positionMode":atom(&raw("oneway")),"lastUpdate":atom(&v.last_update),"info":atom(&v.info)}));
        Ok(())
    }
    fn refresh_positions(&self) -> Result<(), Error> {
        self.touch("refresh-positions")?;
        self.refresh_positions.set(self.refresh_positions.get() + 1);
        Ok(())
    }
    fn find_object(
        &self,
        array: &Raw,
        mut predicate: impl FnMut(&Raw) -> Result<bool, Error>,
    ) -> Result<Raw, Error> {
        let Node::Array(values) = array.as_ref() else {
            panic!("find receipt")
        };
        let length = values.borrow().len();
        for i in 0..length {
            let value = values.borrow().get(i).cloned().unwrap_or_else(undef);
            if predicate(&value)? {
                return Ok(value);
            }
        }
        Ok(undef())
    }
    fn stringify_data(&self, data: &Raw) -> Result<Vec<u16>, Error> {
        self.touch("json-stringify")?;
        if self.undefined(data) {
            Ok(u("undefined"))
        } else {
            self.get(data, "toJSON")?;
            self.get(data, "data")?;
            assert_eq!(self.profile, "array-empty");
            Ok(u("{\"data\":[null,0,\"skip\"]}"))
        }
    }
    fn positions_length(&self) -> Result<f64, Error> {
        self.touch("store-read")?;
        self.touch("positions-read")?;
        self.touch("positions-length")?;
        Ok(1.0)
    }
    fn store_balance(&self, v: api::PrivateBalance) -> Result<(), Error> {
        let (total, free, used) = match self.profile.as_str() {
            "fallback" => (123.5, 80.25, 43.25),
            "explicit" => (100.0, 70.0, 12.0),
            "negative-zero" => (-0.0, 0.0, 0.0),
            "negative-difference" => (20.0, 70.0, 0.0),
            "nonfinite" => (0.0, 0.0, 0.0),
            _ => (100.0, 70.0, 30.0),
        };
        native_n(v.total, total, "balance_native_total");
        native_n(v.free, free, "balance_native_free");
        native_n(v.used, used, "balance_native_used");
        native_n(v.upnl, 0.0, "balance_native_upnl");
        self.touch("store-read")?;
        self.touch("store-balance-read")?;
        self.touch("store-balance")?;
        self.balances.borrow_mut().push(
            json!({"total":num(v.total),"free":num(v.free),"used":num(v.used),"upnl":num(v.upnl)}),
        );
        Ok(())
    }
}
fn update(row: &Value) {
    let kind = row["kind"].as_str().unwrap();
    let f = Frame::new(
        kind,
        row["profile"].as_str().unwrap(),
        row["failure"].as_str().unwrap(),
    );
    let result = match kind {
        "order" => api::handle_order_update(&f, &f.data),
        "position" => api::handle_position_update(&f, &f.data),
        "balance" => api::handle_balance_update(&f, &f.data),
        _ => panic!("operation floor"),
    };
    outcome(&result, &row["outcome"], &f.primary);
    assert_eq!(
        f.pending.get(),
        row["pending"],
        "position_native_pending_state"
    );
    assert_eq!(
        f.refresh_orders.get(),
        row["refreshOrders"],
        "order_native_refresh_count"
    );
    assert_eq!(
        f.refresh_positions.get(),
        row["refreshPositions"],
        "position_native_refresh_count"
    );
    assert_eq!(json!(*f.orders.borrow()), row["orders"]);
    assert_eq!(json!(*f.positions.borrow()), row["positions"]);
    assert_eq!(json!(*f.balances.borrow()), row["balances"]);
    assert_eq!(json!(*f.rejections.borrow()), row["rejections"]);
    assert_eq!(
        json!(*f.logs.borrow()),
        row["logs"],
        "update_native_logs_{kind}_{}",
        f.profile
    );
    assert_eq!(
        json!(*f.trace.borrow()),
        row["trace"],
        "update_native_trace_{kind}_{}_{}",
        f.profile,
        f.failure
    );
}
fn message(row: &Value) {
    let f = Frame::new(
        "message",
        row["profile"].as_str().unwrap(),
        row["failure"].as_str().unwrap(),
    );
    let result = api::on_private_message(&f);
    outcome(&result, &row["outcome"], &f.primary);
    assert_eq!(
        f.authenticated.get(),
        row["authenticated"],
        "message_native_authenticated"
    );
    assert_eq!(f.closed.get(), row["closed"], "message_native_closed");
    assert_eq!(
        f.subscribed.get(),
        row["subscribed"],
        "message_native_subscription_count"
    );
    let Node::Array(values) = f.handlers.as_ref() else {
        unreachable!()
    };
    assert_eq!(
        values.borrow().len(),
        row["handlerCount"],
        "message_native_handler_growth"
    );
    assert_eq!(
        json!(*f.calls.borrow()),
        row["calls"],
        "message_native_calls_{}_{}",
        f.profile,
        f.failure
    );
    assert_eq!(json!(*f.stats.borrow()), row["stats"]);
    assert_eq!(json!(*f.logs.borrow()), row["logs"]);
    assert_eq!(
        json!(*f.trace.borrow()),
        row["trace"],
        "message_native_trace_{}_{}",
        f.profile,
        f.failure
    );
}
fn constructor(row: &Value) {
    let testnet = row["testnet"].as_bool().unwrap();
    let failure = row["failure"].as_str().unwrap();
    let epoch = row["epoch"].as_u64().unwrap();
    let f = Frame::new("constructor", "none", failure);
    let parent = Rc::new(());
    let urls = Rc::new(());
    let base = Cell::new(false);
    let written_urls = RefCell::new(vec![]);
    let bindings = RefCell::new(vec![]);
    let calls = RefCell::new(vec![]);
    let result = api::initialize_private_socket(
        &parent,
        |p| {
            assert!(Rc::ptr_eq(p, &parent), "constructor_native_parent_identity");
            f.touch("base")?;
            base.set(true);
            Ok(())
        },
        || {
            f.touch("urls-read")?;
            Ok(urls.clone())
        },
        |p| {
            assert!(Rc::ptr_eq(p, &parent));
            f.touch("options")?;
            f.touch("testnet")?;
            Ok(testnet)
        },
        |map, network| {
            assert!(Rc::ptr_eq(&map, &urls));
            assert_eq!(network, testnet);
            f.touch("url-lookup")?;
            Ok(u(if network {
                "wss://ws.api.testnet.paradex.trade/v1?cancel-on-disconnect=false"
            } else {
                "wss://ws.api.prod.paradex.trade/v1?cancel-on-disconnect=false"
            }))
        },
        |v| {
            assert_eq!(
                v,
                u(if testnet {
                    "wss://ws.api.testnet.paradex.trade/v1?cancel-on-disconnect=false"
                } else {
                    "wss://ws.api.prod.paradex.trade/v1?cancel-on-disconnect=false"
                }),
                "constructor_native_url"
            );
            f.touch("url-write")?;
            written_urls.borrow_mut().push(v);
            Ok(())
        },
        |v| {
            let expected = [
                ("ping", api::PrivateHandler::Ping),
                ("pong", api::PrivateHandler::Pong),
                ("orders.ALL", api::PrivateHandler::Orders),
                ("positions", api::PrivateHandler::Positions),
                ("balance_events", api::PrivateHandler::Balance),
            ];
            assert_eq!(
                v.iter().map(|b| (b.channel, b.handler)).collect::<Vec<_>>(),
                expected,
                "constructor_native_bindings"
            );
            f.touch("handlers-write")?;
            *bindings.borrow_mut() = v;
            Ok(())
        },
    );
    outcome(&result, &row["outcome"], &f.primary);
    if result.is_ok() {
        for binding in bindings.borrow().iter() {
            let invoke = |name: &str, tag: &str, arg: Option<&Rc<()>>| -> Result<(), Error> {
                f.touch(&format!("handler-read:{name}"))?;
                f.touch(&format!("handler-call:{name}"))?;
                assert!(
                    arg.is_none_or(|p| Rc::ptr_eq(p, &parent)),
                    "constructor_native_callback_argument"
                );
                calls.borrow_mut().push(
                    json!({"tag":tag,"epoch":epoch,"argc":if arg.is_some(){1}else{0},"same":true}),
                );
                Ok(())
            };
            api::invoke_private_handler(
                binding.handler,
                &parent,
                || invoke("handlePongEvent", "pong", None),
                |p| invoke("handleOrderUpdate", "orders", Some(p)),
                |p| invoke("handlePositionUpdate", "positions", Some(p)),
                |p| invoke("handleBalanceUpdate", "balance", Some(p)),
            )
            .unwrap();
        }
    }
    assert_eq!(base.get(), row["baseParent"]);
    assert_eq!(json!(*written_urls.borrow()), row["urls"]);
    assert_eq!(
        json!(
            bindings
                .borrow()
                .iter()
                .map(|b| json!({"channel":b.channel,"count":1}))
                .collect::<Vec<_>>()
        ),
        row["bindings"]
    );
    assert_eq!(json!(*calls.borrow()), row["calls"]);
    assert_eq!(
        json!(*f.trace.borrow()),
        row["trace"],
        "constructor_native_trace"
    );
}
fn throttle(row: &Value) {
    let input = &row["input"];
    let f = Frame::new("throttle", "none", input["failure"].as_str().unwrap());
    let now = number(&input["now"]);
    let last = number(&input["last"]);
    let period = number(&input["period"]);
    let next_period = number(&input["nextPeriod"]);
    let disposed = Cell::new(input["disposed"].as_bool().unwrap());
    let scheduled = Cell::new(input["scheduled"].as_bool().unwrap());
    let pending = Cell::new(true);
    let periods = Cell::new(0);
    let delay = RefCell::new(Vec::new());
    let executions = Cell::new(0);
    let callback = RefCell::new(None);
    let result = api::throttled_fetch_positions(
        || {
            f.touch("now")?;
            Ok(now)
        },
        || {
            f.touch("disposed")?;
            Ok(disposed.get())
        },
        || {
            f.touch("scheduled-read")?;
            Ok(scheduled.get())
        },
        || {
            f.touch("last")?;
            Ok(last)
        },
        || {
            f.touch("period")?;
            let i = periods.get();
            periods.set(i + 1);
            Ok(if i == 0 { period } else { next_period })
        },
        |value| {
            f.touch("scheduled-write")?;
            scheduled.set(value);
            Ok(())
        },
        || {
            f.touch("pending")?;
            Ok(pending.get())
        },
        || {
            f.touch("execute")?;
            executions.set(executions.get() + 1);
            Ok(())
        },
        |cb, ms| {
            native_n(ms, next_period - (now - last), "throttle_native_delay");
            f.touch("schedule")?;
            delay.borrow_mut().push(bits(ms));
            *callback.borrow_mut() = Some(cb);
            Ok(())
        },
    );
    outcome(&result, &row["outcome"], &f.primary);
    assert_eq!(
        json!({"scheduled":scheduled.get(),"trace":*f.trace.borrow(),"delays":*delay.borrow(),"executions":executions.get()}),
        row["before"],
        "throttle_native_before_state"
    );
    disposed.set(input["afterDisposed"].as_bool().unwrap());
    pending.set(input["afterPending"].as_bool().unwrap());
    let cb = callback.borrow_mut().take();
    if let Some(cb) = cb {
        outcome(&cb(), &row["callbackOutcome"], &f.primary);
    } else {
        assert!(row["callbackOutcome"].is_null());
    }
    assert_eq!(
        scheduled.get(),
        row["scheduled"],
        "throttle_native_scheduled_state"
    );
    assert_eq!(
        executions.get(),
        row["executions"],
        "throttle_native_executions"
    );
    assert_eq!(json!(*delay.borrow()), row["delays"]);
    assert_eq!(json!(*f.trace.borrow()), row["trace"]);
}
#[test]
fn original_private_ingress_bodies() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/private_ingress_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "ingress_source_observer: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    for (name, floor) in [
        ("pure", 54),
        ("constructors", 24),
        ("throttles", 960),
        ("updates", 900),
        ("messages", 408),
    ] {
        assert_eq!(
            data[name].as_array().unwrap().len(),
            floor,
            "private_ingress_independent_floor_{name}"
        );
    }
    for row in data["pure"].as_array().unwrap() {
        pure(row);
    }
    for row in data["constructors"].as_array().unwrap() {
        constructor(row);
    }
    for row in data["throttles"].as_array().unwrap() {
        throttle(row);
    }
    for row in data["updates"].as_array().unwrap() {
        update(row);
    }
    for row in data["messages"].as_array().unwrap() {
        message(row);
    }
}
