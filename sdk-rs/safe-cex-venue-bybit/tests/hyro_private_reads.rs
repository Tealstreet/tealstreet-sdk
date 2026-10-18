use safe_cex_venue_bybit::{
    hyro_capabilities, hyro_private_reads as api,
    hyro_private_session::{SessionFuture, SessionHost},
    variant_account_refresh::AccountRefreshHost,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
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
    Array(RefCell<Vec<Raw>>),
    Object {
        fields: RefCell<Vec<(Vec<u16>, Raw)>>,
        prototype: Option<Raw>,
        watched: bool,
        tag: Option<&'static str>,
        coercion: Option<bool>,
    },
    Function {
        owner: Raw,
        code: String,
    },
    Error(Vec<u16>),
    Set(RefCell<Vec<Raw>>),
    Iterator {
        array: Raw,
        index: RefCell<usize>,
    },
    Callback {
        owner: Raw,
        code: String,
        captures: Vec<Raw>,
    },
    Settled(Result<Raw, Raw>),
    Selected {
        owner: Raw,
        callable: Raw,
    },
}
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn undef() -> Raw {
    Rc::new(Node::Undefined)
}
fn null() -> Raw {
    Rc::new(Node::Null)
}
fn boolean(b: bool) -> Raw {
    Rc::new(Node::Bool(b))
}
fn number(n: f64) -> Raw {
    Rc::new(Node::Number(n))
}
fn text(t: Vec<u16>) -> Raw {
    Rc::new(Node::Text(t))
}
fn raw(s: &str) -> Raw {
    text(u(s))
}
fn lone(s: &str) -> Raw {
    text(u(s).into_iter().chain([0xd800]).collect())
}
fn array(a: Vec<Raw>) -> Raw {
    Rc::new(Node::Array(RefCell::new(a)))
}
fn object(fields: Vec<(&str, Raw)>, watched: bool) -> Raw {
    Rc::new(Node::Object {
        fields: RefCell::new(fields.into_iter().map(|(k, v)| (u(k), v)).collect()),
        prototype: None,
        watched,
        tag: None,
        coercion: None,
    })
}
fn with_proto(
    fields: Vec<(Vec<u16>, Raw)>,
    prototype: Option<Raw>,
    watched: bool,
    tag: Option<&'static str>,
    coercion: Option<bool>,
) -> Raw {
    Rc::new(Node::Object {
        fields: RefCell::new(fields),
        prototype,
        watched,
        tag,
        coercion,
    })
}
fn error(message: &str) -> Raw {
    Rc::new(Node::Error(u(message)))
}
fn get_bare(v: &Raw, key: &[u16]) -> Raw {
    match v.as_ref() {
        Node::Object {
            fields, prototype, ..
        } => fields
            .borrow()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .or_else(|| prototype.as_ref().map(|p| get_bare(p, key)))
            .unwrap_or_else(undef),
        Node::Error(message) if key == u("message") => text(message.clone()),
        Node::Array(a) if key == u("length") => number(a.borrow().len() as f64),
        _ => undef(),
    }
}
fn set_bare(v: &Raw, key: &[u16], value: Raw) {
    let Node::Object { fields, .. } = v.as_ref() else {
        panic!("native object write")
    };
    let mut fields = fields.borrow_mut();
    if let Some((_, v)) = fields.iter_mut().find(|(k, _)| k == key) {
        *v = value;
    } else {
        fields.push((key.to_vec(), value));
    }
}
fn truthy(v: &Raw) -> bool {
    match v.as_ref() {
        Node::Undefined | Node::Null => false,
        Node::Bool(b) => *b,
        Node::Number(n) => *n != 0.0 && !n.is_nan(),
        Node::Text(s) => !s.is_empty(),
        _ => true,
    }
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
fn js(v: &Raw) -> Vec<u16> {
    match v.as_ref() {
        Node::Undefined => u("undefined"),
        Node::Null => u("null"),
        Node::Bool(b) => u(if *b { "true" } else { "false" }),
        Node::Number(n) => u(&safe_cex_num::js_number_string(*n)),
        Node::Text(t) => t.clone(),
        Node::Array(_) => vec![],
        Node::Object {
            coercion: Some(false),
            ..
        } => u("object-").into_iter().chain([0xd800]).collect(),
        _ => u("[object Object]"),
    }
}
fn atom(v: &Raw) -> Value {
    match v.as_ref() {
        Node::Undefined => json!({"u":true}),
        Node::Null => Value::Null,
        Node::Bool(b) => json!(b),
        Node::Number(n) => {
            json!({"n":format!("{:016x}",if n.is_nan(){0x7ff8000000000000}else{n.to_bits()})})
        }
        Node::Text(t) => json!({"s":t}),
        Node::Array(a) => json!(a.borrow().iter().map(atom).collect::<Vec<_>>()),
        Node::Object { tag: Some(t), .. } => json!({"ref":t}),
        Node::Object { fields, .. } => {
            json!({"o":fields.borrow().iter().map(|(k,v)|json!([k,atom(v)])).collect::<Vec<_>>()})
        }
        Node::Error(message) => json!({"error":true,"message":message}),
        Node::Function { code, .. } => json!({"fn":code}),
        Node::Set(_)
        | Node::Iterator { .. }
        | Node::Callback { .. }
        | Node::Settled(_)
        | Node::Selected { .. } => panic!("internal selected callable"),
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
    if let Some(a) = v["s"].as_array() {
        return text(a.iter().map(|n| n.as_u64().unwrap() as u16).collect());
    }
    if let Some(n) = v["n"].as_str() {
        return number(f64::from_bits(u64::from_str_radix(n, 16).unwrap()));
    }
    if let Some(a) = v.as_array() {
        return array(a.iter().map(decode).collect());
    }
    let fields = v["o"]
        .as_array()
        .expect("input object")
        .iter()
        .map(|p| (serde_json::from_value(p[0].clone()).unwrap(), decode(&p[1])))
        .collect();
    with_proto(fields, None, false, None, None)
}
fn watch(v: Raw) -> Raw {
    if let Node::Object {
        fields,
        prototype,
        tag,
        coercion,
        ..
    } = v.as_ref()
    {
        with_proto(
            fields.borrow().clone(),
            prototype.clone(),
            true,
            *tag,
            *coercion,
        )
    } else {
        v
    }
}
fn elements(v: &Raw) -> Vec<Raw> {
    let Node::Array(a) = v.as_ref() else {
        panic!("native array")
    };
    a.borrow().clone()
}
fn bare(v: &Raw, k: &str) -> Raw {
    get_bare(v, &u(k))
}
fn num(v: &Raw) -> f64 {
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
            .map(|s| safe_cex_num::js_number(Some(&Value::String(s))).unwrap())
            .unwrap_or(f64::NAN),
        _ => f64::NAN,
    }
}
fn finite_number(v: &Raw) -> bool {
    matches!(v.as_ref(),Node::Number(n)if n.is_finite())
}
fn settle<F: Future>(future: F) -> F::Output {
    let w = Waker::noop().clone();
    let mut cx = Context::from_waker(&w);
    let mut f = pin!(future);
    for _ in 0..512 {
        if let Poll::Ready(result) = f.as_mut().poll(&mut cx) {
            return result;
        }
    }
    panic!("native future exceeded 512 polls")
}
struct Harness {
    method: String,
    profile: String,
    failure: String,
    root: Raw,
    global: Raw,
    xhr: Raw,
    input: Raw,
    row: Raw,
    account: Raw,
    position: Raw,
    order: Raw,
    list: Raw,
    data: Raw,
    symbols: Raw,
    params: Raw,
    counts: RefCell<Vec<(Vec<u16>, usize)>>,
    ops: RefCell<Vec<Value>>,
    logs: RefCell<Vec<Raw>>,
    request_args: RefCell<Vec<Vec<Raw>>>,
    base_args: RefCell<Vec<Raw>>,
    requests: RefCell<usize>,
    maps: RefCell<usize>,
    faulted: RefCell<bool>,
    primary: Raw,
}
impl Harness {
    fn new(case: &Value) -> Self {
        let input = decode(&case["input"]);
        let data = bare(&input, "data");
        let list = bare(&bare(&data, "result"), "list");
        let row = watch(bare(&input, "row"));
        let account = bare(&input, "account");
        let coins = bare(&account, "coin");
        if let Node::Array(a) = coins.as_ref() {
            let watched = a.borrow().iter().cloned().map(watch).collect();
            *a.borrow_mut() = watched;
        }
        let account = watch(account);
        let method = case["method"].as_str().unwrap();
        if let Node::Array(a) = list.as_ref() {
            let mut a = a.borrow_mut();
            if !a.is_empty() {
                a[0] = if method == "fetchRawBalance" {
                    account.clone()
                } else {
                    row.clone()
                };
                for value in a.iter_mut().skip(1) {
                    *value = watch(value.clone());
                }
            }
        }
        Self {
            method: method.into(),
            profile: case["profile"].as_str().unwrap().into(),
            failure: case["failure"].as_str().unwrap().into(),
            root: with_proto(vec![], None, true, Some("this"), None),
            global: object(vec![], false),
            xhr: object(vec![], false),
            row,
            account,
            position: watch(bare(&input, "position")),
            order: watch(bare(&input, "order")),
            data,
            list,
            symbols: bare(&input, "symbols"),
            params: bare(&input, "params"),
            input,
            counts: RefCell::new(vec![]),
            ops: RefCell::new(vec![]),
            logs: RefCell::new(vec![]),
            request_args: RefCell::new(vec![]),
            base_args: RefCell::new(vec![]),
            requests: RefCell::new(0),
            maps: RefCell::new(0),
            faulted: RefCell::new(false),
            primary: with_proto(vec![], None, false, Some("primary"), None),
        }
    }
    fn observe(&self, v: &Raw) -> Value {
        for (name, value) in [
            ("position", &self.position),
            ("order", &self.order),
            ("row", &self.row),
            ("input-list", &self.list),
            ("params", &self.params),
            ("primary", &self.primary),
        ] {
            if Rc::ptr_eq(v, value) && matches!(v.as_ref(), Node::Object { .. } | Node::Array(_)) {
                return json!({"ref":name});
            }
        }
        if let Node::Array(a) = self.list.as_ref() {
            if a.borrow()
                .get(1)
                .is_some_and(|second| Rc::ptr_eq(v, second))
                && !Rc::ptr_eq(v, &self.row)
            {
                return json!({"ref":"row-1"});
            }
        }
        match v.as_ref() {
            Node::Array(a) => json!(
                a.borrow()
                    .iter()
                    .map(|v| self.observe(v))
                    .collect::<Vec<_>>()
            ),
            Node::Object { fields, .. } => {
                json!({"o":fields.borrow().iter().map(|(k,v)|json!([k,self.observe(v)])).collect::<Vec<_>>()})
            }
            _ => atom(v),
        }
    }
    fn touch(&self, name: Vec<u16>, args: Vec<Raw>) -> Result<usize, Raw> {
        let mut counts = self.counts.borrow_mut();
        let n = if let Some((_, n)) = counts.iter_mut().find(|(key, _)| key == &name) {
            let out = *n;
            *n += 1;
            out
        } else {
            counts.push((name.clone(), 1));
            0
        };
        if u(&self.failure)
            == name
                .iter()
                .copied()
                .chain(u(&format!("#{n}")))
                .collect::<Vec<_>>()
        {
            *self.faulted.borrow_mut() = true;
            return Err(self.primary.clone());
        }
        drop(counts);
        self.ops.borrow_mut().push(
            json!({"name":name,"args":args.iter().map(|v|self.observe(v)).collect::<Vec<_>>()}),
        );
        Ok(n)
    }
    fn rec(&self, name: &str, args: Vec<Raw>) -> Result<usize, Raw> {
        self.touch(u(name), args)
    }
    fn read(&self, v: &Raw, k: Vec<u16>) -> Result<Raw, Raw> {
        if matches!(v.as_ref(), Node::Undefined | Node::Null) {
            return Err(error(&format!(
                "Cannot read properties of {} (reading '{}')",
                if matches!(v.as_ref(), Node::Null) {
                    "null"
                } else {
                    "undefined"
                },
                String::from_utf16_lossy(&k)
            )));
        }
        if matches!(v.as_ref(), Node::Object { watched: true, .. }) {
            let n = self.touch(u("get:").into_iter().chain(k.clone()).collect(), vec![])?;
            if self.profile == "equity-flip" && k == u("totalEquity") && n > 0 {
                return Ok(raw(""));
            }
        }
        if Rc::ptr_eq(v, &self.root) && k == u("xhr") {
            return Ok(self.xhr.clone());
        }
        if (Rc::ptr_eq(v, &self.root)
            && matches!(
                String::from_utf16_lossy(&k).as_str(),
                "fail"
                    | "isFiniteNumeric"
                    | "emitAndLogError"
                    | "mapOrder"
                    | "dedupePositionsById"
                    | "assertPrivateListResponse"
                    | "mapPosition"
                    | "fetchOrdersHelper"
                    | "fetchPrivateRows"
                    | "mapValidatedOrders"
            ))
            || (Rc::ptr_eq(v, &self.xhr) && k == u("get"))
            || (Rc::ptr_eq(v, &self.global) && k == u("getBybitResponseMeta"))
            || matches!(
                v.as_ref(),
                Node::Array(_) | Node::Set(_) | Node::Iterator { .. } | Node::Text(_)
            ) && matches!(
                String::from_utf16_lossy(&k).as_str(),
                "push"
                    | "some"
                    | "every"
                    | "map"
                    | "flatMap"
                    | "includes"
                    | "has"
                    | "add"
                    | "next"
                    | "endsWith"
            )
        {
            return Ok(Rc::new(Node::Function {
                owner: v.clone(),
                code: String::from_utf16(&k).unwrap(),
            }));
        }
        if let Node::Array(a) = v.as_ref() {
            if k == u("length") {
                return Ok(number(a.borrow().len() as f64));
            }
            if let Ok(index) = String::from_utf16_lossy(&k).parse::<usize>() {
                return Ok(a.borrow().get(index).cloned().unwrap_or_else(undef));
            }
        }
        if let Node::Text(t) = v.as_ref() {
            if k == u("length") {
                return Ok(number(t.len() as f64));
            }
        }
        Ok(get_bare(v, &k))
    }
    fn reply(&self) -> Result<Raw, Raw> {
        let n = *self.requests.borrow();
        *self.requests.borrow_mut() += 1;
        if self.profile == "xhr-reject" {
            return Err(self.primary.clone());
        }
        if self.method == "fetchPositionsHelper" && self.profile == "growth" && n == 0 {
            let Node::Array(a) = self.symbols.as_ref() else {
                panic!("symbols")
            };
            a.borrow_mut().push(raw("SOLUSDT"));
        }
        if self.method == "fetchPositionsHelper" && self.profile == "replacement" && n == 0 {
            let Node::Array(a) = self.symbols.as_ref() else {
                panic!("symbols")
            };
            a.borrow_mut()[1] = raw("REPLACEDUSDT");
        }
        let data = if self.method == "fetchPrivateRows" {
            let cursor = match self.profile.as_str() {
                "two" => {
                    if n == 0 {
                        raw("next")
                    } else {
                        undef()
                    }
                }
                "duplicate" => raw("same"),
                "next-null" => null(),
                "next-number" => number(1.0),
                "lone-cursor" => {
                    if n == 0 {
                        lone("cursor-")
                    } else {
                        undef()
                    }
                }
                "page-100" => {
                    if n == 99 {
                        undef()
                    } else {
                        raw(&format!("page-{n}"))
                    }
                }
                "page-limit" => raw(&format!("page-{n}")),
                "end-empty" => raw(""),
                _ => undef(),
            };
            let list = get_bare(&bare(&self.data, "result"), &u("list"));
            object(
                vec![
                    ("retCode", bare(&self.data, "retCode")),
                    ("retMsg", bare(&self.data, "retMsg")),
                    (
                        "result",
                        object(vec![("list", list), ("nextPageCursor", cursor)], false),
                    ),
                ],
                false,
            )
        } else {
            self.data.clone()
        };
        Ok(object(vec![("data", data)], false))
    }
    fn count(&self, k: &str) -> usize {
        self.counts
            .borrow()
            .iter()
            .find(|(key, _)| key == &u(k))
            .map(|(_, n)| *n)
            .unwrap_or(0)
    }
    fn promise_result(&self, r: Result<Raw, Raw>) -> Result<Raw, Raw> {
        Ok(Rc::new(Node::Settled(r)))
    }
}
impl SessionHost for Harness {
    type Value = Raw;
    type Error = Raw;
    fn get(&self, v: &Raw, k: &str) -> Result<Raw, Raw> {
        self.read(v, u(k))
    }
    fn optional(&self, v: &Raw, k: &str) -> Result<Raw, Raw> {
        if matches!(v.as_ref(), Node::Undefined | Node::Null) {
            Ok(undef())
        } else {
            self.get(v, k)
        }
    }
    fn set(&self, v: &Raw, k: &str, value: Raw) -> Result<(), Raw> {
        set_bare(v, &u(k), value);
        Ok(())
    }
    fn define(&self, v: &Raw, k: &str, value: Raw) -> Result<(), Raw> {
        self.set(v, k, value)
    }
    fn truthy(&self, v: &Raw) -> bool {
        truthy(v)
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
        Ok(num(v))
    }
    fn units(&self, v: &Raw) -> Result<Vec<u16>, Raw> {
        Ok(js(v))
    }
    fn text(&self, v: Vec<u16>) -> Raw {
        text(v)
    }
    fn object(&self, v: Vec<(&'static str, Raw)>) -> Raw {
        object(v, false)
    }
    fn array(&self, v: Vec<Raw>) -> Raw {
        array(v)
    }
    fn global(&self) -> Raw {
        self.global.clone()
    }
    fn select(&self, owner: Raw, key: &str) -> Result<Raw, Raw> {
        let callable = self.get(&owner, key)?;
        Ok(Rc::new(Node::Selected { owner, callable }))
    }
    fn receiver_call(&self, owner: Raw, callable: Raw, args: Vec<Raw>) -> Result<Raw, Raw> {
        if let Node::Callback {
            owner: lexical,
            code,
            captures,
        } = callable.as_ref()
        {
            assert!(
                Rc::ptr_eq(lexical, &self.root),
                "native_lexical_callback_receiver"
            );
            return api::dispatch(
                self,
                lexical,
                api::Callback::from_name(code).expect("source callback selector"),
                captures,
                args,
            );
        }
        let Node::Function {
            owner: lexical,
            code,
        } = callable.as_ref()
        else {
            panic!("callable")
        };
        assert!(Rc::ptr_eq(&owner, lexical), "native_method_receiver");
        let arg = args.first().cloned().unwrap_or_else(undef);
        match code.as_str() {
            "get" => {
                self.request_args.borrow_mut().push(args.clone());
                let result = self.rec("xhr", args).and_then(|_| self.reply());
                self.promise_result(result)
            }
            "getBybitResponseMeta" => {
                self.rec("meta", args.clone())?;
                if eq(&bare(&arg, "retCode"), &number(0.0)) {
                    Ok(object(
                        vec![
                            ("isSuccess", boolean(true)),
                            ("hasExplicitStatus", boolean(true)),
                            ("retCode", number(0.0)),
                            ("retMsg", raw("OK")),
                            ("rawResponse", undef()),
                        ],
                        false,
                    ))
                } else {
                    Ok(bare(&self.input, "meta"))
                }
            }
            "emitAndLogError" => {
                self.rec("log", args)?;
                self.logs.borrow_mut().push(arg);
                Ok(undef())
            }
            "fail" => hyro_capabilities::fail(self, owner, arg),
            "isFiniteNumeric" => Ok(boolean(
                matches!(arg.as_ref(), Node::Number(_) | Node::Text(_))
                    && !String::from_utf16_lossy(&js(&arg)).trim().is_empty()
                    && num(&arg).is_finite(),
            )),
            "assertPrivateListResponse" => {
                api::assert_private_list_response(self, owner, arg, args[1].clone())
            }
            "mapPosition" => api::map_position(self, owner, arg),
            "fetchOrdersHelper" => self.promise_result(settle(api::fetch_orders_helper(
                self,
                owner,
                arg,
                args.get(1).cloned().unwrap_or_else(undef),
            ))),
            "fetchPrivateRows" => self.promise_result(settle(api::fetch_private_rows(
                self,
                owner,
                arg,
                args[1].clone(),
            ))),
            "mapValidatedOrders" => api::map_validated_orders(self, owner, arg),
            "mapOrder" => {
                self.rec("map-order", args)?;
                if self.profile == "map-reject" {
                    return Err(self.primary.clone());
                }
                let mut n = self.maps.borrow_mut();
                if self.method == "mapValidatedOrders" && self.profile == "growth" && *n == 0 {
                    let Node::Array(a) = self.list.as_ref() else {
                        panic!("list")
                    };
                    a.borrow_mut().push(self.row.clone());
                }
                if self.method == "mapValidatedOrders" && self.profile == "replacement" && *n == 0 {
                    let Node::Array(a) = self.list.as_ref() else {
                        panic!("list")
                    };
                    a.borrow_mut()[1] = self.row.clone();
                }
                *n += 1;
                Ok(array(vec![self.order.clone()]))
            }
            "dedupePositionsById" => {
                self.rec("dedupe", args)?;
                let mut seen = vec![];
                let mut out = vec![];
                for value in elements(&arg) {
                    let id = self.get(&value, "id")?;
                    if let Some(index) = seen.iter().position(|existing| eq(existing, &id)) {
                        out[index] = value;
                    } else {
                        seen.push(id);
                        out.push(value);
                    }
                }
                Ok(array(out))
            }
            "endsWith" => Ok(boolean(js(&owner).ends_with(&js(&arg)))),
            "next" => {
                let Node::Iterator { array, index } = owner.as_ref() else {
                    panic!("iterator")
                };
                let mut index = index.borrow_mut();
                let a = elements(array);
                if *index >= a.len() {
                    Ok(object(vec![("done", boolean(true))], false))
                } else {
                    let value = a[*index].clone();
                    *index += 1;
                    Ok(object(
                        vec![("value", value), ("done", boolean(false))],
                        false,
                    ))
                }
            }
            "has" => {
                let Node::Set(values) = owner.as_ref() else {
                    panic!("set")
                };
                Ok(boolean(values.borrow().iter().any(|v| eq(v, &arg))))
            }
            "add" => {
                let Node::Set(values) = owner.as_ref() else {
                    panic!("set")
                };
                if !values.borrow().iter().any(|v| eq(v, &arg)) {
                    values.borrow_mut().push(arg);
                }
                Ok(owner.clone())
            }
            "push" => {
                let Node::Array(a) = owner.as_ref() else {
                    panic!("array")
                };
                a.borrow_mut().extend(args);
                Ok(number(a.borrow().len() as f64))
            }
            "includes" => Ok(boolean(elements(&owner).iter().any(|v| eq(v, &arg)))),
            "some" | "every" | "map" | "flatMap" => {
                let Node::Array(a) = owner.as_ref() else {
                    panic!("array method")
                };
                let length = a.borrow().len();
                let mut out = vec![];
                for index in 0..length {
                    let value = a.borrow().get(index).cloned();
                    if let Some(value) = value {
                        let result = self.receiver_call(
                            undef(),
                            arg.clone(),
                            vec![value, number(index as f64), owner.clone()],
                        )?;
                        if code == "some" && truthy(&result) {
                            return Ok(boolean(true));
                        }
                        if code == "every" && !truthy(&result) {
                            return Ok(boolean(false));
                        }
                        if code == "map" {
                            out.push(result);
                        } else if code == "flatMap" {
                            if let Node::Array(v) = result.as_ref() {
                                out.extend(v.borrow().clone());
                            } else {
                                out.push(result);
                            }
                        }
                    }
                }
                Ok(if code == "some" {
                    boolean(false)
                } else if code == "every" {
                    boolean(true)
                } else {
                    array(out)
                })
            }
            _ => panic!("unowned callable {code}"),
        }
    }
    fn apply(&self, v: Raw, args: Vec<Raw>) -> Result<Raw, Raw> {
        match v.as_ref() {
            Node::Selected { owner, callable } => {
                self.receiver_call(owner.clone(), callable.clone(), args)
            }
            Node::Callback { .. } => self.receiver_call(undef(), v, args),
            _ => panic!("selected callable"),
        }
    }
    fn wait(&self, value: Raw) -> SessionFuture<'_, Raw, Raw> {
        Box::pin(async move {
            if let Node::Settled(result) = value.as_ref() {
                result.clone()
            } else {
                Ok(value)
            }
        })
    }
    fn intrinsic(&self, name: &str, args: Vec<Raw>) -> Result<Raw, Raw> {
        let arg = args.first().cloned().unwrap_or_else(undef);
        match name {
            "ENDPOINTS" => Ok(decode(&json!({"o":crate_endpoints()}))),
            "Array.isArray" => Ok(boolean(matches!(arg.as_ref(), Node::Array(_)))),
            "Number.isFinite" => Ok(boolean(finite_number(&arg))),
            "Number" => Ok(number(num(&arg))),
            "typeof" => Ok(raw(match arg.as_ref() {
                Node::Text(_) => "string",
                Node::Number(_) => "number",
                Node::Bool(_) => "boolean",
                Node::Undefined => "undefined",
                _ => "object",
            })),
            "null" => Ok(null()),
            "computed property" => self.read(&args[0], js(&args[1])),
            "new Set" => Ok(Rc::new(Node::Set(RefCell::new(vec![])))),
            "iterator" => {
                assert!(
                    matches!(arg.as_ref(), Node::Array(_)),
                    "source array iterator input"
                );
                Ok(Rc::new(Node::Iterator {
                    array: arg,
                    index: RefCell::new(0),
                }))
            }
            "super.mapPosition" => {
                self.rec("base-map", vec![args[1].clone()])?;
                self.base_args.borrow_mut().push(args[1].clone());
                if self.profile == "base-reject" {
                    Err(self.primary.clone())
                } else {
                    Ok(self.position.clone())
                }
            }
            _ => panic!("unused intrinsic {name}"),
        }
    }
    fn allocate(&self, _: Raw) -> Result<Raw, Raw> {
        panic!("unused allocation")
    }
    fn callback(&self, owner: Raw, code: &'static str, captures: Vec<Raw>) -> Result<Raw, Raw> {
        Ok(Rc::new(Node::Callback {
            owner,
            code: code.into(),
            captures,
        }))
    }
    fn promise(&self) -> Result<(Raw, Raw, Raw), Raw> {
        panic!("unused Promise allocation")
    }
    fn error(&self, message: Raw) -> Result<Raw, Raw> {
        Ok(Rc::new(Node::Error(js(&message))))
    }
    fn error_value(&self, error: Raw) -> Raw {
        error
    }
    fn normalize_error(&self, error: Raw) -> Result<Raw, Raw> {
        Ok(error)
    }
}
fn crate_endpoints() -> Vec<Value> {
    safe_cex_venue_bybit::types::ENDPOINTS
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| json!([u(k),{"s":u(v.as_str().unwrap())}]))
        .collect()
}
impl AccountRefreshHost for Harness {
    fn spread(&self, base: Raw, overrides: Vec<(&'static str, Raw)>) -> Result<Raw, Raw> {
        let Node::Object { fields, .. } = base.as_ref() else {
            panic!("source object spread")
        };
        let keys = fields
            .borrow()
            .iter()
            .map(|(k, _)| k.clone())
            .collect::<Vec<_>>();
        let mut copied = vec![];
        for key in keys {
            copied.push((key.clone(), self.read(&base, key)?));
        }
        let out = with_proto(copied, None, false, None, None);
        for (key, value) in overrides {
            set_bare(&out, &u(key), value);
        }
        Ok(out)
    }
    fn then(&self, _: Raw, _: Raw) -> Result<Raw, Raw> {
        panic!("unused then capability")
    }
    fn all(&self, _: Vec<Raw>) -> Result<Raw, Raw> {
        panic!("unused all capability")
    }
}
fn invoke(h: &Harness) -> Result<Raw, Raw> {
    let owner = h.root.clone();
    match h.method.as_str() {
        "fetchRawBalance" => settle(api::fetch_raw_balance(h, owner)),
        "assertPrivateListResponse" => {
            api::assert_private_list_response(h, owner, h.data.clone(), bare(&h.input, "endpoint"))
        }
        "mapPosition" => api::map_position(h, owner, h.row.clone()),
        "fetchOrders" => settle(api::fetch_orders(h, owner)),
        "fetchPrivateRows" => settle(api::fetch_private_rows(
            h,
            owner,
            bare(&h.input, "endpoint"),
            h.params.clone(),
        )),
        "fetchOrdersHelper" => settle(api::fetch_orders_helper(
            h,
            owner,
            bare(&h.input, "category"),
            bare(&h.input, "settle"),
        )),
        "mapValidatedOrders" => api::map_validated_orders(h, owner, h.list.clone()),
        "fetchPositionsHelper" => settle(api::fetch_positions_helper(
            h,
            owner,
            h.symbols.clone(),
            bare(&h.input, "category"),
            bare(&h.input, "settle"),
        )),
        _ => panic!("unknown method"),
    }
}
fn error_message(result: &Result<Raw, Raw>) -> Vec<u16> {
    match result {
        Err(v) => {
            if let Node::Error(t) = v.as_ref() {
                t.clone()
            } else {
                vec![]
            }
        }
        _ => vec![],
    }
}
fn native(h: &Harness, result: &Result<Raw, Raw>) {
    let p = h.profile.as_str();
    let m = h.method.as_str();
    if *h.faulted.borrow() {
        assert!(
            matches!(result,Err(v)if Rc::ptr_eq(v,&h.primary)),
            "native_original_fault_identity"
        );
    }
    if h.failure != "none" {
        return;
    }
    if matches!(p, "xhr-reject" | "base-reject" | "map-reject") {
        assert!(
            matches!(result,Err(v)if Rc::ptr_eq(v,&h.primary)),
            "native_original_dependency_error"
        );
        return;
    }
    if m == "fetchPrivateRows" && matches!(p, "page-100" | "page-limit") {
        assert_eq!(*h.requests.borrow(), 100, "pages_native_request_floor");
    }
    let reason = match m {
        "fetchRawBalance" => match p {
            "failed" => Some("failure-"),
            "failed-empty" => Some("HyroTrader wallet request failed"),
            "missing-list" | "empty-list" | "other-account" | "no-balance" => {
                Some("HyroTrader wallet response is missing the UNIFIED account")
            }
            "whitespace" | "bool-balance" | "null-money" | "bad-money" | "coin-null"
            | "coin-object" | "coin-null-row" | "coin-bad-money" => {
                Some("HyroTrader wallet response contains invalid monetary fields")
            }
            _ => None,
        },
        "assertPrivateListResponse" => match p {
            "failed" => Some("failure-"),
            "failed-empty" => Some("request failed"),
            "missing-list" => Some("response has no list"),
            "order-empty-id" | "symbol-usdc" | "symbol-number" | "row-null"
            | "position-negative" | "position-empty" | "position-space" | "position-bool"
            | "position-idx" => Some("returned a malformed row"),
            "position-no-leverage" | "position-zero-leverage" => {
                Some("HyroTrader position response is missing valid leverage")
            }
            "position-bad-number" | "position-null-number" => {
                Some("HyroTrader position response contains an invalid numeric value")
            }
            "position-no-side" | "position-no-price" => {
                Some("HyroTrader position response is missing side, price or unrealized PnL")
            }
            _ => None,
        },
        "mapPosition" => match p {
            "row-invalid" => Some("returned a malformed row"),
            "base-nan" | "base-string" | "base-null" | "margin-nan" | "margin-null" => {
                Some("HyroTrader position mapping contains invalid numeric fields")
            }
            _ => None,
        },
        "fetchOrders" => {
            if p == "failed" {
                Some("failure-")
            } else {
                None
            }
        }
        "fetchPrivateRows" => match p {
            "duplicate" | "next-null" | "next-number" => {
                Some("HyroTrader private snapshot cursor did not advance")
            }
            "page-limit" => Some("HyroTrader private snapshot page limit exceeded"),
            "invalid-row" => Some("returned a malformed row"),
            _ => None,
        },
        "fetchOrdersHelper" | "mapValidatedOrders" => match p {
            "invalid-row" => Some("returned a malformed row"),
            "mapped-bad" | "mapped-negative" | "mapped-nan" | "mapped-string" | "mapped-null" => {
                Some("HyroTrader order response contains incomplete or invalid order fields")
            }
            _ => None,
        },
        "fetchPositionsHelper" => match p {
            "row-invalid" => Some("returned a malformed row"),
            "base-nan" => Some("HyroTrader position mapping contains invalid numeric fields"),
            _ => None,
        },
        _ => panic!("unknown row"),
    };
    if let Some(reason) = reason {
        assert!(result.is_err(), "{m}_native_rejection");
        let msg = error_message(result);
        let expected = if reason == "failure-" {
            u(reason).into_iter().chain([0xd800]).collect()
        } else if matches!(
            reason,
            "request failed" | "response has no list" | "returned a malformed row"
        ) {
            let endpoint = if matches!(
                m,
                "fetchOrders" | "fetchOrdersHelper" | "mapValidatedOrders"
            ) || m == "assertPrivateListResponse" && !p.starts_with("position")
            {
                "/v5/order/realtime"
            } else {
                "/v5/position/list"
            };
            u(&format!("HyroTrader {endpoint} {reason}"))
        } else {
            u(reason)
        };
        assert_eq!(msg, expected, "{m}_native_failure_reason");
        assert_eq!(h.logs.borrow().len(), 1, "native_visible_failure");
    } else {
        assert!(result.is_ok(), "{m}/{p} native success {:?}", result);
        let out = result.as_ref().unwrap();
        match m {
            "fetchRawBalance" => {
                assert!(Rc::ptr_eq(out, &h.list), "wallet_native_list_identity");
                let rows = elements(out);
                assert_eq!(rows.len(), 1, "wallet_native_row_count");
                assert!(
                    Rc::ptr_eq(&rows[0], &h.account),
                    "wallet_native_account_identity"
                );
                assert!(
                    eq(&bare(&rows[0], "totalWalletBalance"), &raw("90")),
                    "wallet_native_balance_value"
                );
            }
            "assertPrivateListResponse" => assert!(
                matches!(out.as_ref(), Node::Undefined),
                "list_native_undefined"
            ),
            "mapPosition" => {
                assert!(Rc::ptr_eq(out, &h.position), "position_native_identity");
                assert!(
                    eq(&bare(out, "notional"), &number(20.0)),
                    "position_native_notional"
                );
            }
            "fetchPrivateRows" => {
                let count = if p == "page-100" {
                    100
                } else if matches!(p, "two" | "lone-cursor") {
                    2
                } else {
                    1
                };
                let rows = elements(out);
                assert_eq!(
                    rows.len(),
                    count * if p == "two" { 2 } else { 1 },
                    "pages_native_count"
                );
                assert_eq!(*h.requests.borrow(), count, "pages_native_requests");
                let input_rows = elements(&h.list);
                assert!(
                    rows.iter().enumerate().all(|(i, row)| Rc::ptr_eq(
                        row,
                        if p == "two" && i % 2 == 1 {
                            &input_rows[1]
                        } else {
                            &h.row
                        }
                    )),
                    "pages_native_row_aliases"
                );
                assert!(
                    eq(&bare(&rows[0], "orderId"), &lone("ord-")),
                    "pages_native_order_id"
                );
            }
            "fetchOrders" | "fetchOrdersHelper" | "mapValidatedOrders" => {
                let count = if p == "empty" {
                    0
                } else if matches!(p, "two" | "growth" | "replacement") {
                    2
                } else {
                    1
                };
                let rows = elements(out);
                assert_eq!(rows.len(), count, "{m}_native_order_count");
                assert!(
                    rows.iter().all(|row| Rc::ptr_eq(row, &h.order)),
                    "native_mapped_order_aliases"
                );
                for row in rows {
                    assert!(
                        eq(&bare(&row, "id"), &lone("order-")),
                        "native_mapped_order_id"
                    );
                    assert!(
                        eq(&bare(&row, "filled"), &number(9.0)),
                        "native_mapped_filled"
                    );
                }
                if m == "fetchOrders" {
                    assert_eq!(
                        h.count("get:fetchOrdersHelper"),
                        1,
                        "orders_native_helper_call"
                    );
                }
                if m == "fetchOrdersHelper" {
                    assert_eq!(
                        h.count("get:fetchPrivateRows"),
                        1,
                        "orders_helper_native_private_call"
                    );
                }
            }
            "fetchPositionsHelper" => {
                let rows = elements(out);
                assert_eq!(
                    rows.len(),
                    if p == "empty" { 0 } else { 1 },
                    "positions_helper_native_dedupe_count"
                );
                assert!(
                    rows.iter().all(|v| Rc::ptr_eq(v, &h.position)),
                    "positions_helper_native_position_identity"
                );
                let count = if p == "growth" || p == "falsy-symbol" {
                    3
                } else if matches!(p, "two" | "duplicate" | "replacement") {
                    2
                } else {
                    1
                };
                assert_eq!(
                    *h.requests.borrow(),
                    count,
                    "positions_helper_native_request_count"
                );
                assert_eq!(h.count("dedupe"), 1, "positions_helper_native_dedupe_call");
            }
            _ => panic!("unknown method"),
        }
    }
    if result.is_ok()
        && matches!(
            m,
            "fetchOrders"
                | "fetchOrdersHelper"
                | "fetchPrivateRows"
                | "fetchPositionsHelper"
                | "fetchRawBalance"
        )
    {
        let args = h.request_args.borrow();
        assert!(!args.is_empty(), "native_request_arguments_present");
        let params = bare(&args[0][1], "params");
        if m == "fetchRawBalance" {
            assert!(
                eq(&args[0][0], &raw("/v5/account/wallet-balance")),
                "wallet_native_endpoint"
            );
            assert!(
                eq(&bare(&params, "accountType"), &raw("UNIFIED")),
                "wallet_native_account_type"
            );
        }
        if m == "fetchOrders" {
            assert!(
                eq(&bare(&params, "category"), &raw("linear")),
                "orders_native_category"
            );
            assert!(
                eq(&bare(&params, "settleCoin"), &raw("USDT")),
                "orders_native_settle"
            );
        }
        if m == "fetchOrdersHelper" {
            assert!(
                eq(&bare(&params, "category"), &bare(&h.input, "category")),
                "orders_helper_native_category"
            );
            assert!(
                eq(&bare(&params, "settleCoin"), &bare(&h.input, "settle")),
                "orders_helper_native_settle"
            );
        }
        if m != "fetchRawBalance" {
            for request in args.iter() {
                assert!(
                    eq(&bare(&bare(&request[1], "params"), "limit"), &number(50.0)),
                    "native_limit_override"
                );
            }
            assert!(
                eq(&bare(&params, "cursor"), &raw("")),
                "native_initial_cursor_override"
            );
        }
    }
    if m == "mapPosition" && result.is_ok() {
        let args = h.base_args.borrow();
        assert_eq!(args.len(), 1, "position_native_base_call");
        assert!(
            eq(&bare(&args[0], "positionIdx"), &number(0.0)),
            "position_native_idx_override"
        );
        assert!(
            !Rc::ptr_eq(&args[0], &h.row),
            "position_native_spread_fresh"
        );
    }
    if m == "fetchPrivateRows" && p == "page-limit" {
        assert_eq!(*h.requests.borrow(), 100, "pages_native_limit_requests");
    }
}
#[test]
fn original_source_and_native_private_reads() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/hyro-private-reads-fixtures.cjs"
        ))
        .output()
        .expect("source oracle");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).expect("UTF-16 source receipts");
    assert_eq!(cases.len(), 1632, "declared floor");
    assert_eq!(cases.last().unwrap()["profile"], "disposed");
    assert_eq!(cases.last().unwrap()["failure"], "dedupe#0");
    let mut unique = std::collections::BTreeSet::new();
    let mut counts = std::collections::BTreeMap::new();
    for case in &cases {
        let key = (
            case["method"].as_str().unwrap(),
            case["profile"].as_str().unwrap(),
            case["failure"].as_str().unwrap(),
        );
        assert!(unique.insert(key));
        *counts.entry(key.0).or_insert(0_usize) += 1;
    }
    assert_eq!(
        counts,
        std::collections::BTreeMap::from([
            ("fetchRawBalance", 320),
            ("assertPrivateListResponse", 368),
            ("mapPosition", 160),
            ("fetchOrders", 48),
            ("fetchPrivateRows", 208),
            ("fetchOrdersHelper", 112),
            ("mapValidatedOrders", 192),
            ("fetchPositionsHelper", 224)
        ])
    );
    let mut comparisons = vec![];
    for case in cases {
        let h = Harness::new(&case);
        let result = invoke(&h);
        native(&h, &result);
        let observed = match &result {
            Ok(v) => json!({"ok":true,"value":h.observe(v)}),
            Err(e) => json!({"ok":false,"error":h.observe(e),"primary":Rc::ptr_eq(e,&h.primary)}),
        };
        let receipt = json!({"ops":*h.ops.borrow(),"logs":h.logs.borrow().iter().map(|v|h.observe(v)).collect::<Vec<_>>(),"requests":*h.requests.borrow(),"maps":*h.maps.borrow(),"result":observed});
        comparisons.push((case, receipt));
    }
    for (case, receipt) in comparisons {
        for key in ["ops", "logs", "requests", "maps", "result"] {
            assert_eq!(
                receipt[key], case[key],
                "source comparison {}/{}/{} {key}",
                case["method"], case["profile"], case["failure"]
            );
        }
    }
}
