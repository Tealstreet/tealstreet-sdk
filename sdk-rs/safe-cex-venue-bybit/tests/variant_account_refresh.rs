use safe_cex_venue_bybit::{
    hyro_private_session::{SessionFuture, SessionHost as Host},
    variant_account_refresh::{self as api, AccountRefreshHost},
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
type Raw = Rc<Node>;
#[derive(Debug)]
enum Node {
    Reference {
        tag: String,
        value: Raw,
    },
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    Text(Vec<u16>),
    Array(RefCell<Vec<Raw>>),
    Promise {
        result: RefCell<Option<Result<Raw, Raw>>>,
        handled: RefCell<bool>,
        reaction: RefCell<Option<(Raw, Raw)>>,
        phase: Option<String>,
        aggregate: RefCell<Option<Vec<Raw>>>,
    },
    Object {
        fields: RefCell<Vec<(Vec<u16>, Raw)>>,
        prototype: Option<Raw>,
        tag: Option<String>,
        error: bool,
    },
    Function {
        owner: Raw,
        code: String,
        captures: Vec<Raw>,
    },
}
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn text(s: Vec<u16>) -> Raw {
    Rc::new(Node::Text(s))
}
fn raw(s: &str) -> Raw {
    text(u(s))
}
fn undef() -> Raw {
    Rc::new(Node::Undefined)
}
fn null() -> Raw {
    Rc::new(Node::Null)
}
fn number(n: f64) -> Raw {
    Rc::new(Node::Number(n))
}
fn boolean(b: bool) -> Raw {
    Rc::new(Node::Bool(b))
}
fn object(fields: Vec<(&str, Raw)>) -> Raw {
    Rc::new(Node::Object {
        fields: RefCell::new(fields.into_iter().map(|(k, v)| (u(k), v)).collect()),
        prototype: None,
        tag: None,
        error: false,
    })
}
fn tagged(tag: &str, fields: Vec<(&str, Raw)>) -> Raw {
    Rc::new(Node::Object {
        fields: RefCell::new(fields.into_iter().map(|(k, v)| (u(k), v)).collect()),
        prototype: None,
        tag: Some(tag.into()),
        error: false,
    })
}
fn array(v: Vec<Raw>) -> Raw {
    Rc::new(Node::Array(RefCell::new(v)))
}
fn func(owner: Raw, code: &str, captures: Vec<Raw>) -> Raw {
    Rc::new(Node::Function {
        owner,
        code: code.into(),
        captures,
    })
}
fn tag(v: &Raw) -> String {
    match v.as_ref() {
        Node::Object { tag: Some(s), .. } => s.clone(),
        _ => "this".into(),
    }
}
fn elements(v: &Raw) -> Vec<Raw> {
    if let Node::Reference { value, .. } = v.as_ref() {
        return elements(value);
    }
    let Node::Array(a) = v.as_ref() else {
        panic!("not array")
    };
    a.borrow().clone()
}
fn eq(a: &Raw, b: &Raw) -> bool {
    match (a.as_ref(), b.as_ref()) {
        (Node::Undefined, Node::Undefined) | (Node::Null, Node::Null) => true,
        (Node::Bool(a), Node::Bool(b)) => a == b,
        (Node::Number(a), Node::Number(b)) => a == b,
        (Node::Text(a), Node::Text(b)) => a == b,
        _ => Rc::ptr_eq(a, b),
    }
}
fn get_units(v: &Raw, key: &[u16]) -> Raw {
    match v.as_ref() {
        Node::Reference { value, .. } => get_units(value, key),
        Node::Object {
            fields, prototype, ..
        } => fields
            .borrow()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| {
                prototype
                    .as_ref()
                    .map(|p| get_units(p, key))
                    .unwrap_or_else(undef)
            }),
        Node::Array(a) if key == u("length") => number(a.borrow().len() as f64),
        Node::Array(a) => String::from_utf16(key)
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .and_then(|i| a.borrow().get(i).cloned())
            .unwrap_or_else(undef),
        _ => undef(),
    }
}
fn get(v: &Raw, key: &str) -> Raw {
    get_units(v, &u(key))
}
fn set(v: &Raw, key: &str, value: Raw) {
    let Node::Object { fields, .. } = v.as_ref() else {
        panic!("write nonobject")
    };
    let mut fields = fields.borrow_mut();
    if let Some((_, v)) = fields.iter_mut().find(|(k, _)| *k == u(key)) {
        *v = value
    } else {
        fields.push((u(key), value));
    }
}
fn numeric(v: &Raw) -> f64 {
    match v.as_ref() {
        Node::Number(n) => *n,
        Node::Null => 0.0,
        Node::Bool(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        Node::Text(t) => String::from_utf16(t)
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(f64::NAN),
        _ => f64::NAN,
    }
}
fn js(v: &Raw) -> Vec<u16> {
    match v.as_ref() {
        Node::Text(t) => t.clone(),
        Node::Undefined => u("undefined"),
        Node::Null => u("null"),
        Node::Bool(b) => u(if *b { "true" } else { "false" }),
        Node::Number(n) => u(&safe_cex_num::js_number_string(*n)),
        Node::Object { error: true, .. } => u("Error: ")
            .into_iter()
            .chain(js(&get(v, "message")))
            .collect(),
        _ => u("[object Object]"),
    }
}
fn atom(v: &Raw) -> Value {
    match v.as_ref() {
        Node::Reference { tag, .. } => json!({"ref":tag}),
        Node::Promise { .. } => panic!("observer internal value"),
        Node::Undefined => json!({"u":true}),
        Node::Null => Value::Null,
        Node::Bool(b) => json!(b),
        Node::Number(n) => {
            json!({"n":format!("{:016x}",if n.is_nan(){0x7ff8000000000000}else{n.to_bits()})})
        }
        Node::Text(t) => json!({"s":t}),
        Node::Array(a) => json!(a.borrow().iter().map(atom).collect::<Vec<_>>()),
        Node::Function {
            owner,
            code,
            captures,
        } => {
            json!({"fn":code,"owner":tag(owner),"captures":captures.iter().map(atom).collect::<Vec<_>>()})
        }
        Node::Object { tag: Some(tag), .. } => json!({"ref":tag}),
        Node::Object { error: true, .. } => json!({"error":true,"message":js(&get(v,"message"))}),
        Node::Object { fields, .. } => {
            json!({"o":fields.borrow().iter().map(|(k,v)|json!([k,atom(v)])).collect::<Vec<_>>()})
        }
    }
}
fn decode(v: &Value) -> Raw {
    if v.is_null() {
        return null();
    }
    if let Some(b) = v.as_bool() {
        return boolean(b);
    }
    if v["u"] == true {
        return undef();
    }
    if let Some(t) = v["s"].as_array() {
        return text(t.iter().map(|n| n.as_u64().unwrap() as u16).collect());
    }
    if let Some(n) = v["n"].as_str() {
        return number(f64::from_bits(u64::from_str_radix(n, 16).unwrap()));
    }
    if let Some(tag) = v["ref"].as_str() {
        return tagged(tag, vec![]);
    }
    if let Some(a) = v.as_array() {
        return array(a.iter().map(decode).collect());
    }
    if let Some(code) = v["fn"].as_str() {
        let owner = tagged(v["owner"].as_str().unwrap(), vec![]);
        let captures = v["captures"]
            .as_array()
            .unwrap()
            .iter()
            .map(decode)
            .collect();
        return func(owner, code, captures);
    }
    let fields = v["o"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (serde_json::from_value(p[0].clone()).unwrap(), decode(&p[1])))
        .collect();
    Rc::new(Node::Object {
        fields: RefCell::new(fields),
        prototype: None,
        tag: None,
        error: false,
    })
}
fn reference(tag: &str, value: Raw) -> Raw {
    Rc::new(Node::Reference {
        tag: tag.into(),
        value,
    })
}
fn error(message: Vec<u16>) -> Raw {
    Rc::new(Node::Object {
        fields: RefCell::new(vec![(u("message"), text(message))]),
        prototype: None,
        tag: None,
        error: true,
    })
}
fn promise(phase: Option<&str>) -> Raw {
    Rc::new(Node::Promise {
        result: RefCell::new(None),
        handled: RefCell::new(false),
        reaction: RefCell::new(None),
        phase: phase.map(str::to_owned),
        aggregate: RefCell::new(None),
    })
}
fn state(p: &Raw) -> Option<Result<Raw, Raw>> {
    let Node::Promise { result, .. } = p.as_ref() else {
        panic!("promise")
    };
    result.borrow().clone()
}
fn settle(p: &Raw, v: Result<Raw, Raw>) {
    let Node::Promise { result, .. } = p.as_ref() else {
        panic!("promise")
    };
    if result.borrow().is_none() {
        *result.borrow_mut() = Some(v);
    }
}
struct Harness {
    variant: String,
    kind: String,
    profile: String,
    failure: String,
    counts: RefCell<Vec<(String, usize)>>,
    owner: Raw,
    store: Raw,
    initial_loaded: Raw,
    global: Raw,
    orders: Raw,
    positions: Raw,
    existing: Raw,
    merged: Raw,
    raw_balance: Raw,
    balance: Raw,
    primary: Raw,
    ops: RefCell<Vec<Value>>,
    writes: RefCell<Vec<Value>>,
    deferred: RefCell<Vec<Raw>>,
    promises: RefCell<Vec<Raw>>,
    timers: RefCell<Vec<f64>>,
    loop_callbacks: RefCell<Vec<Raw>>,
    tick_result: Raw,
    clock: RefCell<usize>,
    write_enabled: RefCell<bool>,
}
impl Harness {
    fn new(row: &Value) -> Self {
        let profile = row["profile"].as_str().unwrap();
        let owner = tagged("this", vec![]);
        let initial_loaded = object(vec![
            ("markets", boolean(true)),
            ("orders", boolean(false)),
            ("positions", boolean(false)),
            ("balance", boolean(false)),
            (
                "custom",
                text(u("kept-").into_iter().chain([0xd800]).collect()),
            ),
        ]);
        let existing = reference(
            "existing",
            array(vec![object(vec![("id", raw("old-pos"))])]),
        );
        let store = tagged(
            "store",
            vec![
                ("loaded", initial_loaded.clone()),
                ("orders", array(vec![])),
                ("positions", existing.clone()),
                ("balance", object(vec![("total", number(7.0))])),
            ],
        );
        for (k, v) in [
            ("store", store.clone()),
            ("isDisposed", boolean(row["input"]["disposed"] == true)),
            ("needsReinit", boolean(row["input"]["needs"] == true)),
            ("lastReinitAt", decode(&row["input"]["lastReinit"])),
            ("reinitRetryMs", decode(&row["input"]["retry"])),
            (
                "lastWebsocketPositionsUpdate",
                decode(&row["input"]["last"]),
            ),
        ] {
            set(&owner, k, v);
        }
        Self {
            variant: row["variant"].as_str().unwrap().into(),
            kind: row["name"].as_str().unwrap().into(),
            profile: profile.into(),
            failure: row["failure"].as_str().unwrap().into(),
            counts: RefCell::new(vec![]),
            owner,
            store,
            initial_loaded,
            global: tagged("global", vec![]),
            orders: reference(
                "orders",
                array(vec![object(vec![(
                    "id",
                    text(u("order-").into_iter().chain([0xd800]).collect()),
                )])]),
            ),
            positions: reference(
                "positions",
                array(vec![
                    object(vec![(
                        "id",
                        text(u("pos-").into_iter().chain([0xd800]).collect()),
                    )]),
                    object(vec![("id", raw("pos2"))]),
                ]),
            ),
            existing,
            merged: reference(
                "merged",
                array(vec![object(vec![(
                    "id",
                    text(u("merged-").into_iter().chain([0xd800]).collect()),
                )])]),
            ),
            raw_balance: reference("raw-balance", object(vec![("raw", number(17.0))])),
            balance: reference("balance", object(vec![("total", number(19.0))])),
            primary: error(u("primary-").into_iter().chain([0xd800]).collect()),
            ops: RefCell::new(vec![]),
            writes: RefCell::new(vec![]),
            deferred: RefCell::new(vec![]),
            promises: RefCell::new(vec![]),
            timers: RefCell::new(vec![]),
            loop_callbacks: RefCell::new(vec![]),
            tick_result: reference("tick-result", object(vec![])),
            clock: RefCell::new(0),
            write_enabled: RefCell::new(true),
        }
    }
    fn touch(&self, k: &str) -> Result<(), Raw> {
        let mut counts = self.counts.borrow_mut();
        let n = if let Some((_, n)) = counts.iter_mut().find(|(name, _)| name == k) {
            let old = *n;
            *n += 1;
            old
        } else {
            counts.push((k.into(), 1));
            0
        };
        if self.failure == format!("{k}#{n}") {
            Err(self.primary.clone())
        } else {
            Ok(())
        }
    }
    fn rec(&self, k: &str, args: Vec<Raw>) -> Result<(), Raw> {
        self.touch(k)?;
        self.ops
            .borrow_mut()
            .push(json!({"name":k,"args":args.iter().map(atom).collect::<Vec<_>>()}));
        Ok(())
    }
    fn request(&self, phase: &str) -> Result<Raw, Raw> {
        self.rec(phase, vec![])?;
        let p = promise(Some(phase));
        self.deferred.borrow_mut().push(p.clone());
        self.promises.borrow_mut().push(p.clone());
        Ok(p)
    }
    fn snapshot(&self) -> Value {
        json!({"disposed":atom(&get(&self.owner,"isDisposed")),"needs":atom(&get(&self.owner,"needsReinit")),"last":atom(&get(&self.owner,"lastReinitAt")),"retry":atom(&get(&self.owner,"reinitRetryMs")),"orders":atom(&get(&self.store,"orders")),"positions":atom(&get(&self.store,"positions")),"balance":atom(&get(&self.store,"balance")),"loaded":atom(&get(&self.store,"loaded")),"loadedIdentity":Rc::ptr_eq(&get(&self.store,"loaded"),&self.initial_loaded)})
    }
    fn pump(&self) {
        for _ in 0..12 {
            let mut changed = false;
            for p in self.promises.borrow().clone() {
                let Node::Promise {
                    reaction,
                    aggregate,
                    ..
                } = p.as_ref()
                else {
                    panic!()
                };
                let callback = reaction.borrow().clone();
                if let (Some((callback, derived)), Some(result)) = (callback, state(&p)) {
                    if state(&derived).is_none() {
                        let value = match result {
                            Ok(value) => self.apply(callback, vec![value]),
                            Err(e) => Err(e),
                        };
                        settle(&derived, value);
                        changed = true;
                    }
                }
                let inputs = aggregate.borrow().clone();
                if let Some(inputs) = inputs {
                    if state(&p).is_none() {
                        if let Some(error) =
                            inputs.iter().find_map(|p| state(p).and_then(Result::err))
                        {
                            settle(&p, Err(error));
                            changed = true;
                        } else if inputs.iter().all(|p| state(p).is_some()) {
                            let values =
                                inputs.iter().map(|p| state(p).unwrap().unwrap()).collect();
                            settle(&p, Ok(array(values)));
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                return;
            }
        }
        panic!("native Promise reactions did not drain within 12 turns")
    }
    fn next(&self) -> Option<Raw> {
        self.deferred
            .borrow()
            .iter()
            .find(|p| state(p).is_none())
            .cloned()
    }
    fn resolve(&self, p: &Raw) {
        let Node::Promise {
            phase: Some(phase), ..
        } = p.as_ref()
        else {
            panic!()
        };
        if self.profile == format!("dispose-{phase}") {
            self.set(&self.owner, "isDisposed", boolean(true)).unwrap();
        }
        if matches!(self.profile.as_str(), "reinit-again" | "capped") && phase == "reinit" {
            self.set(&self.owner, "needsReinit", boolean(true)).unwrap();
        }
        let reject = match self.profile.as_str() {
            "orders-error" => "orders",
            "positions-error" => "positions-fetch",
            "raw-error-first" => "raw",
            "balance-error" => "balance-fetch",
            "reinit-error" => "reinit",
            "helper-error" | "raw-helper-error" => "helper-wait",
            _ => "",
        };
        let result = if phase == reject {
            Err(if self.profile == "raw-helper-error" {
                text(u("raw-").into_iter().chain([0xd800]).collect())
            } else {
                self.primary.clone()
            })
        } else {
            Ok(match phase.as_str() {
                "orders" => self.orders.clone(),
                "positions-fetch" => self.positions.clone(),
                "raw" => self.raw_balance.clone(),
                "balance-fetch" => self.balance.clone(),
                _ => undef(),
            })
        };
        settle(p, result);
        self.pump();
    }
    fn unhandled(&self) -> Value {
        json!(
            self.promises
                .borrow()
                .iter()
                .filter_map(|p| {
                    let Node::Promise { handled, .. } = p.as_ref() else {
                        panic!()
                    };
                    if *handled.borrow() {
                        return None;
                    }
                    state(p)
                        .and_then(Result::err)
                        .map(|e| json!({"error":atom(&e),"primary":Rc::ptr_eq(&e,&self.primary)}))
                })
                .collect::<Vec<_>>()
        )
    }
}
impl Host for Harness {
    type Value = Raw;
    type Error = Raw;
    fn get(&self, v: &Raw, k: &str) -> Result<Raw, Raw> {
        Ok(get(v, k))
    }
    fn optional(&self, v: &Raw, k: &str) -> Result<Raw, Raw> {
        self.get(v, k)
    }
    fn set(&self, v: &Raw, k: &str, value: Raw) -> Result<(), Raw> {
        if *self.write_enabled.borrow() {
            self.touch(&format!("write:{k}"))?;
            self.writes
                .borrow_mut()
                .push(json!({"key":k,"value":atom(&value)}));
        }
        set(v, k, value);
        Ok(())
    }
    fn define(&self, _v: &Raw, _k: &str, _value: Raw) -> Result<(), Raw> {
        panic!("unused define")
    }
    fn truthy(&self, v: &Raw) -> bool {
        match v.as_ref() {
            Node::Undefined | Node::Null => false,
            Node::Bool(v) => *v,
            Node::Number(n) => *n != 0.0 && !n.is_nan(),
            Node::Text(t) => !t.is_empty(),
            _ => true,
        }
    }
    fn equal(&self, a: &Raw, b: &Raw) -> bool {
        eq(a, b)
    }
    fn undefined(&self) -> Raw {
        undef()
    }
    fn literal(&self, s: &str) -> Raw {
        raw(s)
    }
    fn boolean(&self, b: bool) -> Raw {
        boolean(b)
    }
    fn number(&self, n: f64) -> Raw {
        number(n)
    }
    fn numeric(&self, v: &Raw) -> Result<f64, Raw> {
        Ok(numeric(v))
    }
    fn units(&self, v: &Raw) -> Result<Vec<u16>, Raw> {
        Ok(js(v))
    }
    fn text(&self, v: Vec<u16>) -> Raw {
        text(v)
    }
    fn object(&self, v: Vec<(&'static str, Raw)>) -> Raw {
        object(v)
    }
    fn array(&self, v: Vec<Raw>) -> Raw {
        array(v)
    }
    fn global(&self) -> Raw {
        self.global.clone()
    }
    fn select(&self, owner: Raw, name: &str) -> Result<Raw, Raw> {
        let selected = get(&owner, name);
        if matches!(selected.as_ref(), Node::Function { .. }) {
            Ok(selected)
        } else {
            Ok(func(owner, name, vec![]))
        }
    }
    fn receiver_call(&self, _owner: Raw, selected: Raw, args: Vec<Raw>) -> Result<Raw, Raw> {
        self.apply(selected, args)
    }
    fn apply(&self, selected: Raw, args: Vec<Raw>) -> Result<Raw, Raw> {
        let Node::Function { owner, code, .. } = selected.as_ref() else {
            panic!("callable")
        };
        match code.as_str() {
            "fetchOrders" => self.request("orders"),
            "fetchPositions" if self.profile == "spot-original" => {
                let p = promise(None);
                let value = api::spot_positions(self, undef());
                assert_eq!(elements(&value).len(), 0, "spot_positions_native_empty");
                settle(&p, Ok(value));
                Ok(p)
            }
            "fetchPositions" => self.request("positions-fetch"),
            "fetchRawBalance" => self.request("raw"),
            "fetchBalance" => self.request("balance-fetch"),
            "fetchAndSetPrivateSnapshotHelper" => {
                self.rec("helper", vec![args.first().cloned().unwrap_or_else(undef)])?;
                self.request("helper-wait")
            }
            "fetchOrdersPositionsAndBalance" => self.request("reinit"),
            "formatBalance" => {
                self.rec("format", args.clone())?;
                assert!(
                    Rc::ptr_eq(&args[0], &self.raw_balance),
                    "native_formatter_raw_identity"
                );
                Ok(self.balance.clone())
            }
            "update" => {
                self.rec("update", args.clone())?;
                let Node::Object { fields, .. } = args[0].as_ref() else {
                    panic!("payload")
                };
                for (k, v) in fields.borrow().iter() {
                    let key = String::from_utf16(k).unwrap();
                    set(owner, &key, v.clone());
                }
                Ok(undef())
            }
            "addOrUpdatePositionsFull" => {
                self.rec("positions", args.clone())?;
                set(owner, "positions", args[0].clone());
                Ok(undef())
            }
            "getCurrentTimeMs" => {
                self.rec("clock", vec![])?;
                Ok(number(100_000.0))
            }
            "mergeUpdatesUsingLastUpdate" => {
                self.rec("merge", args.clone())?;
                assert!(
                    Rc::ptr_eq(&args[0], &self.positions),
                    "native_merge_fetched_identity"
                );
                assert!(
                    Rc::ptr_eq(&args[1], &self.existing),
                    "native_merge_existing_identity"
                );
                Ok(self.merged.clone())
            }
            "emitAndLogError" => {
                self.rec("log", args)?;
                if self.profile == "dispose-log" {
                    self.set(&self.owner, "isDisposed", boolean(true))?;
                }
                Ok(undef())
            }
            "now" => {
                self.rec("now", vec![])?;
                let n = *self.clock.borrow();
                *self.clock.borrow_mut() += 1;
                Ok(number(100_000.0 + n as f64))
            }
            "min" => {
                let a = numeric(&args[0]);
                let b = numeric(&args[1]);
                Ok(number(if a.is_nan() || b.is_nan() {
                    f64::NAN
                } else {
                    a.min(b)
                }))
            }
            "getTickInterval" => {
                self.rec("interval", vec![])?;
                Ok(number(match self.profile.as_str() {
                    "interval-nan" => f64::NAN,
                    "interval-negative-zero" => -0.0,
                    _ => 3000.0,
                }))
            }
            "loop" => {
                self.rec("loop", vec![args[1].clone()])?;
                let Node::Function {
                    owner: lexical,
                    code,
                    ..
                } = args[0].as_ref()
                else {
                    panic!("loop callback")
                };
                assert!(Rc::ptr_eq(lexical, &self.owner), "native_loop_receiver");
                assert_eq!(code, "account-tick-again", "native_loop_body");
                self.loop_callbacks.borrow_mut().push(args[0].clone());
                self.timers.borrow_mut().push(numeric(&args[1]));
                Ok(undef())
            }
            "tick-probe" => {
                assert!(
                    Rc::ptr_eq(owner, &self.owner),
                    "native_tick_callback_receiver"
                );
                assert!(args.is_empty(), "native_tick_callback_argc");
                Ok(self.tick_result.clone())
            }
            "account-orders"
            | "inverse-account-positions"
            | "spot-account-positions"
            | "account-tick-again" => api::dispatch(
                self,
                owner,
                api::Callback::from_name(code).expect("source callback selector"),
                args,
            ),
            _ => panic!("unused native dependency {code}"),
        }
    }
    fn wait(&self, p: Raw) -> SessionFuture<'_, Raw, Raw> {
        if let Node::Promise { handled, .. } = p.as_ref() {
            *handled.borrow_mut() = true;
        }
        let mut first = true;
        Box::pin(std::future::poll_fn(move |cx| {
            if first {
                first = false;
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            self.pump();
            if let Some(result) = state(&p) {
                Poll::Ready(result)
            } else {
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }))
    }
    fn intrinsic(&self, name: &str, _args: Vec<Raw>) -> Result<Raw, Raw> {
        Ok(tagged(name, vec![]))
    }
    fn allocate(&self, _parent: Raw) -> Result<Raw, Raw> {
        panic!("unused allocation")
    }
    fn callback(&self, owner: Raw, code: &'static str, captures: Vec<Raw>) -> Result<Raw, Raw> {
        Ok(func(owner, code, captures))
    }
    fn promise(&self) -> Result<(Raw, Raw, Raw), Raw> {
        panic!("unused promise executor")
    }
    fn error(&self, message: Raw) -> Result<Raw, Raw> {
        Ok(error(js(&message)))
    }
    fn error_value(&self, error: Raw) -> Raw {
        error
    }
    fn normalize_error(&self, error: Raw) -> Result<Raw, Raw> {
        Ok(error)
    }
}
impl AccountRefreshHost for Harness {
    fn spread(&self, base: Raw, changes: Vec<(&'static str, Raw)>) -> Result<Raw, Raw> {
        let Node::Object { fields, .. } = base.as_ref() else {
            panic!("loaded")
        };
        let out = object(vec![]);
        for (k, v) in fields.borrow().iter() {
            set(&out, &String::from_utf16(k).unwrap(), v.clone());
        }
        for (k, v) in changes {
            set(&out, k, v);
        }
        Ok(out)
    }
    fn then(&self, p: Raw, callback: Raw) -> Result<Raw, Raw> {
        let Node::Promise {
            handled, reaction, ..
        } = p.as_ref()
        else {
            panic!("source then")
        };
        *handled.borrow_mut() = true;
        if !self
            .promises
            .borrow()
            .iter()
            .any(|value| Rc::ptr_eq(value, &p))
        {
            self.promises.borrow_mut().push(p.clone());
        }
        let derived = promise(None);
        *reaction.borrow_mut() = Some((callback, derived.clone()));
        self.promises.borrow_mut().push(derived.clone());
        Ok(derived)
    }
    fn all(&self, inputs: Vec<Raw>) -> Result<Raw, Raw> {
        for p in &inputs {
            let Node::Promise { handled, .. } = p.as_ref() else {
                panic!("aggregate input")
            };
            *handled.borrow_mut() = true;
        }
        let aggregate = promise(None);
        let Node::Promise {
            aggregate: rows, ..
        } = aggregate.as_ref()
        else {
            panic!()
        };
        *rows.borrow_mut() = Some(inputs);
        self.promises.borrow_mut().push(aggregate.clone());
        Ok(aggregate)
    }
}
fn outcome(v: Result<Raw, Raw>, h: &Harness) -> Value {
    match v {
        Ok(v) => json!({"ok":true,"value":atom(&v)}),
        Err(e) => json!({"ok":false,"error":atom(&e),"primary":Rc::ptr_eq(&e,&h.primary)}),
    }
}
#[test]
fn variant_account_refresh_source_parity() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/variant-account-refresh-fixtures.cjs"
        ))
        .output()
        .expect("source oracle");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        rows.as_array().unwrap().len(),
        2772,
        "independent declared case floor"
    );
    let mut supplemental = vec![];
    for row in rows.as_array().unwrap() {
        let h = Harness::new(row);
        let inverse = h.variant == "inverse";
        let owner = h.owner.clone();
        let mut future: SessionFuture<'_, Raw, Raw> = match h.kind.as_str() {
            "fetchOrdersPositionsAndBalance" => {
                if inverse {
                    api::inverse_account_refresh(&h, owner)
                } else {
                    api::spot_account_refresh(&h, owner)
                }
            }
            "fetchAndSetPrivateSnapshot" => {
                if inverse {
                    api::inverse_private_snapshot(&h, owner)
                } else {
                    api::spot_private_snapshot(&h, owner)
                }
            }
            "fetchAndSetPrivateSnapshotHelper" => {
                if inverse {
                    Box::pin(api::inverse_snapshot_helper(
                        &h,
                        &h.owner,
                        decode(&row["input"]["fetchOrders"]),
                    ))
                } else {
                    Box::pin(api::spot_snapshot_helper(
                        &h,
                        &h.owner,
                        decode(&row["input"]["fetchOrders"]),
                    ))
                }
            }
            "tick" => {
                if inverse {
                    Box::pin(api::inverse_tick(&h, &h.owner))
                } else {
                    Box::pin(api::spot_tick(&h, &h.owner))
                }
            }
            "fetchPositions" => Box::pin(async { Ok(api::spot_positions(&h, undef())) }),
            _ => panic!("unknown source member"),
        };
        let waker = Waker::noop().clone();
        let mut context = Context::from_waker(&waker);
        let first = future.as_mut().poll(&mut context);
        let eager = json!(*h.ops.borrow());
        let mut result = if let Poll::Ready(v) = first {
            Some(v)
        } else {
            None
        };
        h.pump();
        if h.profile == "positions-first" || h.profile == "raw-error-first" {
            let phase = if h.profile == "positions-first" {
                "positions-fetch"
            } else {
                "raw"
            };
            let p = h
                .deferred
                .borrow()
                .iter()
                .find(|p| matches!(p.as_ref(),Node::Promise{phase:Some(v),..}if v==phase))
                .cloned();
            if let Some(p) = p {
                h.resolve(&p);
            }
        }
        if result.is_none() {
            if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
                result = Some(value);
            }
        }
        for _ in 0..20 {
            if result.is_some() {
                break;
            }
            if let Some(p) = h.next() {
                h.resolve(&p);
            }
            h.pump();
            if let Poll::Ready(v) = future.as_mut().poll(&mut context) {
                result = Some(v);
            }
        }
        let result = result.expect("native body did not settle within 20 controlled turns");
        if h.profile == "loop-return" && !h.loop_callbacks.borrow().is_empty() {
            set(
                &h.owner,
                "tick",
                func(h.owner.clone(), "tick-probe", vec![]),
            );
            let callback = h.loop_callbacks.borrow()[0].clone();
            let returned = h.apply(callback, vec![raw("ignored")]).unwrap();
            assert!(
                Rc::ptr_eq(&returned, &h.tick_result),
                "native_tick_callback_return_identity"
            );
            assert_eq!(
                row["probe"],
                json!({"receiver":true,"argc":0,"identity":true}),
                "source_tick_callback_probe"
            );
        }
        let mid = h.snapshot();
        for _ in 0..20 {
            if let Some(p) = h.next() {
                h.resolve(&p);
            } else {
                break;
            }
        }
        h.pump();
        assert_eq!(elements(&h.orders).len(), 1, "native_orders_reply_count");
        assert_eq!(
            js(&get(&elements(&h.orders)[0], "id")),
            u("order-").into_iter().chain([0xd800]).collect::<Vec<_>>(),
            "native_orders_reply_id"
        );
        assert_eq!(
            elements(&h.positions).len(),
            2,
            "native_positions_reply_count"
        );
        assert_eq!(
            js(&get(&elements(&h.positions)[0], "id")),
            u("pos-").into_iter().chain([0xd800]).collect::<Vec<_>>(),
            "native_positions_reply_id"
        );
        assert_eq!(
            numeric(&get(&h.raw_balance, "raw")).to_bits(),
            17.0f64.to_bits(),
            "native_raw_balance_value"
        );
        assert_eq!(
            numeric(&get(&h.balance, "total")).to_bits(),
            19.0f64.to_bits(),
            "native_balance_reply_value"
        );
        let label = format!("{}/{}/{}/{}", h.variant, h.kind, h.profile, h.failure);
        if h.failure == "none" {
            if matches!(
                h.kind.as_str(),
                "fetchOrdersPositionsAndBalance" | "fetchAndSetPrivateSnapshotHelper"
            ) && h.profile == "old"
            {
                let clocks = h
                    .ops
                    .borrow()
                    .iter()
                    .filter(|v| v["name"] == "clock")
                    .count();
                assert_eq!(
                    clocks,
                    usize::from(inverse),
                    "{}_{}_native_merge_window_read",
                    h.variant,
                    if h.kind == "fetchOrdersPositionsAndBalance" {
                        "refresh"
                    } else {
                        "helper"
                    }
                );
            }
            if h.kind == "fetchPositions" {
                let v = result.as_ref().unwrap();
                assert_eq!(elements(v).len(), 0, "spot_positions_native_empty");
                let a = api::spot_positions(&h, undef());
                let b = api::spot_positions(&h, undef());
                assert!(!Rc::ptr_eq(&a, &b), "spot_positions_native_fresh");
                let Node::Array(values) = a.as_ref() else {
                    panic!()
                };
                values.borrow_mut().push(raw("mutation"));
                assert_eq!(
                    elements(&b).len(),
                    0,
                    "spot_positions_native_independent_mutability"
                );
            }
            if h.kind == "fetchAndSetPrivateSnapshot" {
                let calls = h
                    .ops
                    .borrow()
                    .iter()
                    .filter(|v| v["name"] == "helper")
                    .count();
                assert_eq!(calls, 1, "{}_wrapper_native_call", h.variant);
            }
            if matches!(
                h.kind.as_str(),
                "fetchOrdersPositionsAndBalance" | "fetchAndSetPrivateSnapshotHelper"
            ) && h.profile == "recent"
            {
                let expected = if inverse { &h.merged } else { &h.positions };
                assert!(
                    Rc::ptr_eq(&get(&h.store, "positions"), expected),
                    "{}_{}_native_positions_identity",
                    h.variant,
                    if h.kind == "fetchOrdersPositionsAndBalance" {
                        "refresh"
                    } else {
                        "helper"
                    }
                );
            }
            if h.kind == "tick" && h.profile == "reinit" {
                assert_eq!(
                    numeric(&get(&h.owner, "reinitRetryMs")).to_bits(),
                    15_000.0f64.to_bits(),
                    "{}_tick_native_reset",
                    h.variant
                );
            }
            if h.kind == "tick" && h.profile == "reinit-again" {
                assert_eq!(
                    numeric(&get(&h.owner, "reinitRetryMs")).to_bits(),
                    30_000.0f64.to_bits(),
                    "{}_tick_native_backoff",
                    h.variant
                );
            }
            if h.kind == "tick" && h.profile == "capped" {
                assert_eq!(
                    numeric(&get(&h.owner, "reinitRetryMs")).to_bits(),
                    300_000.0f64.to_bits(),
                    "native_backoff_cap"
                );
            }
            if matches!(
                h.kind.as_str(),
                "fetchOrdersPositionsAndBalance" | "fetchAndSetPrivateSnapshotHelper"
            ) && h.profile == "basic"
            {
                for field in ["orders", "positions", "balance"] {
                    assert_eq!(
                        atom(&get(&get(&h.store, "loaded"), field)),
                        json!(true),
                        "native_loaded_{field}"
                    );
                }
                assert_eq!(
                    js(&get(&get(&h.store, "loaded"), "custom")),
                    u("kept-").into_iter().chain([0xd800]).collect::<Vec<_>>(),
                    "native_spread_retained_units"
                );
                assert!(
                    !Rc::ptr_eq(&get(&h.store, "loaded"), &h.initial_loaded),
                    "native_loaded_fresh_spread"
                );
            }
            if h.kind == "fetchOrdersPositionsAndBalance" && h.profile == "raw-error-first" {
                assert_eq!(
                    mid["loaded"]["o"][1][1],
                    json!(false),
                    "native_first_rejection_before_sibling_publication"
                );
                assert_eq!(
                    atom(&get(&get(&h.store, "loaded"), "orders")),
                    json!(true),
                    "native_sibling_survives_rejection"
                );
            }
        }
        assert_eq!(mid, row["mid"], "native mid-state {label}");
        assert_eq!(h.snapshot(), row["final"], "native final-state {label}");
        assert_eq!(
            json!(*h.writes.borrow()),
            row["writes"],
            "native field writes {label}"
        );
        assert_eq!(
            outcome(result, &h),
            row["result"],
            "native settlement {label}"
        );
        assert_eq!(
            h.unhandled(),
            row["unhandled"],
            "native unhandled rejection {label}"
        );
        assert_eq!(
            h.deferred
                .borrow()
                .iter()
                .filter(|p| state(p).is_none())
                .count(),
            row["pending"].as_u64().unwrap() as usize,
            "native remaining requests {label}"
        );
        assert_eq!(
            json!(
                h.timers
                    .borrow()
                    .iter()
                    .map(|n| format!(
                        "{:016x}",
                        if n.is_nan() {
                            0x7ff8000000000000
                        } else {
                            n.to_bits()
                        }
                    ))
                    .collect::<Vec<_>>()
            ),
            row["timers"],
            "native scheduled delay bits {label}"
        );
        supplemental.push((eager, row["eager"].clone(), format!("eager {label}")));
        supplemental.push((json!(*h.ops.borrow()), row["ops"].clone(), label));
    }
    for (actual, expected, label) in supplemental {
        assert_eq!(actual, expected, "supplemental trace {label}");
    }
}
