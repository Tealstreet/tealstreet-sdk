use safe_cex_types::PositionMode;
use safe_cex_venue_cryptocom::{account_admin as bodies, api_guards};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    process::Command,
    rc::{Rc, Weak},
    task::{Context, Poll, Waker},
};
fn units(v: &Value) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|n| u16::try_from(n.as_u64().unwrap()).unwrap())
        .collect()
}
fn text(v: &Value) -> Vec<u16> {
    if let Some(s) = v.as_str() {
        s.encode_utf16().collect()
    } else {
        units(&v["units"])
    }
}
fn number(v: &Value) -> f64 {
    match v.as_str() {
        Some("NaN") => f64::NAN,
        Some("Infinity") => f64::INFINITY,
        Some("neg0") => -0.0,
        None => v.as_f64().unwrap(),
        other => panic!("unexpected number {other:?}"),
    }
}
fn observed_number(v: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap())
}
fn native_number(actual: f64, expected: f64, label: &str) {
    assert!(
        actual.is_nan() && expected.is_nan() || actual.to_bits() == expected.to_bits(),
        "CRYPTOCOM-ADMIN-NATIVE-NUMBER {label}: actual {actual:?}, expected {expected:?}"
    )
}

#[derive(Clone, Debug)]
enum Js {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(Vec<u16>),
    Object(String),
    Symbol,
}
type Native = Rc<Js>;
type Error = Native;
fn string(s: &str) -> Native {
    Rc::new(Js::String(s.encode_utf16().collect()))
}
fn input(v: &Value, default: Native) -> Native {
    if v.is_null() {
        return default;
    }
    if v["undefined"] == true {
        return Rc::new(Js::Undefined);
    }
    if v["symbol"] == true {
        return Rc::new(Js::Symbol);
    }
    if let Some(tag) = v["object"].as_str() {
        return Rc::new(Js::Object(tag.into()));
    }
    if v["units"].is_array() {
        return Rc::new(Js::String(text(v)));
    }
    match v {
        Value::Bool(v) => Rc::new(Js::Bool(*v)),
        Value::Number(v) => Rc::new(Js::Number(v.as_f64().unwrap())),
        Value::String(v) => string(v),
        _ => panic!("unsupported native fixture"),
    }
}
fn same(a: &Native, b: &Native) -> bool {
    match (&**a, &**b) {
        (Js::Undefined, Js::Undefined) | (Js::Null, Js::Null) => true,
        (Js::Bool(a), Js::Bool(b)) => a == b,
        (Js::Number(a), Js::Number(b)) => a == b,
        (Js::String(a), Js::String(b)) => a == b,
        (Js::Object(_), Js::Object(_)) | (Js::Symbol, Js::Symbol) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
fn observed(v: &Value) -> Native {
    Rc::new(match v["kind"].as_str().unwrap() {
        "undefined" => Js::Undefined,
        "null" => Js::Null,
        "bool" => Js::Bool(v["value"].as_bool().unwrap()),
        "number" => Js::Number(observed_number(&v["bits"])),
        "string" => Js::String(units(&v["units"])),
        "object" => Js::Object(v["tag"].as_str().unwrap().into()),
        "symbol" => Js::Symbol,
        kind => panic!("unexpected source kind {kind}"),
    })
}
fn assert_native(actual: &Native, source: &Value, label: &str) {
    let expected = observed(source);
    match (&**actual, &*expected) {
        (Js::Undefined, Js::Undefined) | (Js::Null, Js::Null) | (Js::Symbol, Js::Symbol) => {}
        (Js::Bool(a), Js::Bool(b)) => assert_eq!(a, b, "CRYPTOCOM-ADMIN-NATIVE-BOOL {label}"),
        (Js::Number(a), Js::Number(b)) => native_number(*a, *b, label),
        (Js::String(a), Js::String(b)) => assert_eq!(a, b, "CRYPTOCOM-ADMIN-NATIVE-TEXT {label}"),
        (Js::Object(a), Js::Object(b)) => assert_eq!(a, b, "CRYPTOCOM-ADMIN-NATIVE-OBJECT {label}"),
        _ => {
            panic!("CRYPTOCOM-ADMIN-NATIVE-KIND {label}: actual {actual:?}, expected {expected:?}")
        }
    }
}
#[derive(Clone)]
struct Post {
    path: Vec<u16>,
    params: bodies::AdminParams,
    flag: Native,
}
struct Snapshot {
    flag: Native,
    flag_original: bool,
    disposed: bool,
    events: Vec<String>,
    posts: Vec<Post>,
    logs: Vec<(Vec<u16>, bodies::AdminSeverity)>,
    emitted: Vec<Native>,
    catch_returns: Vec<bodies::BalanceResult<Native>>,
}
struct Host {
    spec: Value,
    owner: RefCell<Weak<Host>>,
    flag: RefCell<Native>,
    original_flag: Native,
    disposed: Cell<bool>,
    events: RefCell<Vec<String>>,
    posts: RefCell<Vec<Post>>,
    logs: RefCell<Vec<(Vec<u16>, bodies::AdminSeverity)>>,
    emitted: RefCell<Vec<Native>>,
    catch_returns: RefCell<Vec<bodies::BalanceResult<Native>>>,
    phase_error: Native,
    post_error: Native,
    thrown_post: Native,
    type_error: Native,
    constructed: Native,
    constructed_message: RefCell<Vec<u16>>,
    prepared: Rc<()>,
    response: Native,
    data: Native,
    account: Native,
    uuid: Native,
    balance: Native,
    post_ready: Cell<bool>,
    balance_ready: Cell<bool>,
    balance_created: Cell<bool>,
    balance_settled: Cell<bool>,
    response_reads: Cell<usize>,
    response_message: Native,
    root_message: Native,
    code: Native,
    second_code: Option<Native>,
    server_response: Native,
    server_data: Native,
}
impl Host {
    fn event(&self, phase: &str) -> Result<(), Error> {
        self.events.borrow_mut().push(phase.into());
        if self.spec["fail"] == phase {
            Err(self.phase_error.clone())
        } else {
            Ok(())
        }
    }
    // The dependency driver owns the original promise even when catch lookup fails.
    fn drive_background_balance(&self) {
        if self.balance_created.get() && !self.balance_settled.replace(true) {
            self.event("balance-resolve")
                .expect("background fixture promise resolves");
        }
    }
    fn snapshot(&self) -> Snapshot {
        let flag = self.flag.borrow().clone();
        Snapshot {
            flag_original: same(&flag, &self.original_flag),
            flag,
            disposed: self.disposed.get(),
            events: self.events.borrow().clone(),
            posts: self.posts.borrow().clone(),
            logs: self.logs.borrow().clone(),
            emitted: self.emitted.borrow().clone(),
            catch_returns: self.catch_returns.borrow().clone(),
        }
    }
    fn value_string(&self, value: &Native) -> Vec<u16> {
        match &**value {
            Js::Undefined => "undefined".encode_utf16().collect(),
            Js::Null => "null".encode_utf16().collect(),
            Js::Bool(v) => v.to_string().encode_utf16().collect(),
            Js::Number(v) => safe_cex_num::js_number_string(*v).encode_utf16().collect(),
            Js::String(v) => v.clone(),
            Js::Symbol => "Symbol(symbol)".encode_utf16().collect(),
            Js::Object(_) => "[object Object]".encode_utf16().collect(),
        }
    }
}
struct PostFuture {
    host: Rc<Host>,
}
impl Future for PostFuture {
    type Output = Result<Native, Error>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        let h = &self.host;
        if !h.post_ready.get() {
            return Poll::Pending;
        }
        if h.spec["postReject"] == true || h.spec["fail"] == "post-reject" {
            match h.event("post-reject") {
                Err(e) => Poll::Ready(Err(e)),
                Ok(()) => Poll::Ready(Err(h.thrown_post.clone())),
            }
        } else {
            match h.event("post-resolve") {
                Err(e) => Poll::Ready(Err(e)),
                Ok(()) => Poll::Ready(Ok(h.response.clone())),
            }
        }
    }
}
type Catch = Box<dyn FnMut() -> bodies::BalanceResult<Native>>;
struct BalanceFuture {
    host: Rc<Host>,
    catch: Option<Catch>,
    custom: bool,
}
impl Future for BalanceFuture {
    type Output = Result<bodies::BalanceResult<Native>, Error>;
    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        let h = self.host.clone();
        if self.custom {
            h.drive_background_balance();
            return Poll::Ready(Ok(bodies::BalanceResult::Value(h.balance.clone())));
        }
        if !h.balance_ready.get() {
            return Poll::Pending;
        }
        h.balance_settled.set(true);
        if h.spec["balanceReject"] == true {
            if let Err(e) = h.event("balance-reject") {
                return Poll::Ready(Err(e));
            }
            if let Err(e) = h.event("catch-handler") {
                return Poll::Ready(Err(e));
            }
            let result = self.catch.as_mut().unwrap()();
            h.catch_returns.borrow_mut().push(result.clone());
            Poll::Ready(Ok(result))
        } else {
            match h.event("balance-resolve") {
                Err(e) => Poll::Ready(Err(e)),
                Ok(()) => Poll::Ready(Ok(bodies::BalanceResult::Value(h.balance.clone()))),
            }
        }
    }
}
impl bodies::ExchangeFields for Host {
    type Error = Error;
    type Flag = Native;
    type PublicReturn = Rc<()>;
    type PrivateReturn = Rc<()>;
    fn suppress_flag(&self) -> Result<Native, Error> {
        self.event("read-flag")?;
        Ok(self.flag.borrow().clone())
    }
    fn set_disposed(&self) -> Result<(), Error> {
        self.event("set-disposed")?;
        self.disposed.set(true);
        Ok(())
    }
    fn dispose_public(&self) -> Result<Rc<()>, Error> {
        self.event("public-ws-get")?;
        self.event("public-dispose-get")?;
        self.event("public-dispose")?;
        Ok(self.prepared.clone())
    }
    fn dispose_private(&self) -> Result<Rc<()>, Error> {
        self.event("private-ws-get")?;
        self.event("private-dispose-get")?;
        self.event("private-dispose")?;
        Ok(self.prepared.clone())
    }
}
impl bodies::AdminHost for Host {
    type Error = Error;
    type Prepared = Rc<()>;
    type Response = Native;
    type PostFuture = PostFuture;
    type Log = Rc<()>;
    type LogReturn = Rc<()>;
    fn prepare_post(&self) -> Result<Rc<()>, Error> {
        self.event("prepare-post")?;
        Ok(self.prepared.clone())
    }
    fn endpoint(&self, key: &'static str) -> Result<Vec<u16>, Error> {
        self.event("endpoint")?;
        Ok(safe_cex_venue_cryptocom::types::ENDPOINTS
            .iter()
            .find(|(name, _)| *name == key)
            .unwrap()
            .1
            .encode_utf16()
            .collect())
    }
    fn number_to_string(&self, value: f64) -> Result<Vec<u16>, Error> {
        self.event("number-string")?;
        Ok(safe_cex_num::js_number_string(value)
            .encode_utf16()
            .collect())
    }
    fn post(
        &self,
        prepared: Rc<()>,
        path: Vec<u16>,
        params: bodies::AdminParams,
    ) -> Result<PostFuture, Error> {
        assert!(
            Rc::ptr_eq(&prepared, &self.prepared),
            "CRYPTOCOM-ADMIN-NATIVE-POST-CALLEE"
        );
        self.posts.borrow_mut().push(Post {
            path,
            params,
            flag: self.flag.borrow().clone(),
        });
        self.event("post-call")?;
        Ok(PostFuture {
            host: self.owner.borrow().upgrade().unwrap(),
        })
    }
    fn prepare_log(&self) -> Result<Rc<()>, Error> {
        self.event("resolve-log")?;
        Ok(self.prepared.clone())
    }
    fn error_as_template(&self, error: &Error) -> Result<Vec<u16>, Error> {
        if let Js::Symbol = &**error {
            return Err(self.type_error.clone());
        }
        self.event("error-string")?;
        let tag = match &**error {
            Js::Object(tag) => tag.as_str(),
            _ => return Ok(self.value_string(error)),
        };
        Ok(
            format!("error {tag} {}", self.spec["name"].as_str().unwrap())
                .encode_utf16()
                .collect(),
        )
    }
    fn log(
        &self,
        prepared: Rc<()>,
        message: Vec<u16>,
        severity: bodies::AdminSeverity,
    ) -> Result<Rc<()>, Error> {
        assert!(
            Rc::ptr_eq(&prepared, &self.prepared),
            "CRYPTOCOM-ADMIN-NATIVE-LOG-CALLEE"
        );
        self.event("log-call")?;
        self.logs.borrow_mut().push((message, severity));
        Ok(self.prepared.clone())
    }
}
impl bodies::MarginErrorHost for Host {
    type Error = Error;
    type Constructor = Rc<()>;
    fn prepare_error_constructor(&self) -> Result<Rc<()>, Error> {
        self.event("error-constructor-get")?;
        Ok(self.prepared.clone())
    }
    fn construct_error(&self, constructor: Rc<()>, message: Vec<u16>) -> Result<Error, Error> {
        assert!(
            Rc::ptr_eq(&constructor, &self.prepared),
            "CRYPTOCOM-ADMIN-NATIVE-ERROR-CONSTRUCTOR"
        );
        self.event("new-error")?;
        self.constructed_message.replace(message);
        Ok(self.constructed.clone())
    }
}
impl bodies::AccountHost for Host {
    type Data = Native;
    type ThisAccount = Native;
    type UserId = Native;
    fn response_data(&self, response: Native) -> Result<Native, Error> {
        assert!(Rc::ptr_eq(&response, &self.response));
        self.event("response-data")?;
        Ok(self.data.clone())
    }
    fn this_account(&self, data: Native) -> Result<Native, Error> {
        assert!(Rc::ptr_eq(&data, &self.data));
        self.event("this-account")?;
        Ok(self.account.clone())
    }
    fn uuid(&self, account: Native) -> Result<Native, Error> {
        assert!(Rc::ptr_eq(&account, &self.account));
        self.event("uuid")?;
        Ok(self.uuid.clone())
    }
}
impl bodies::ValidationHost for Host {
    type Flag = Native;
    type Value = Native;
    type Balance = Native;
    type BalancePromise = Rc<()>;
    type BalanceFuture = BalanceFuture;
    type EmitReturn = Rc<()>;
    fn suppress_flag(&self) -> Result<Native, Error> {
        bodies::ExchangeFields::suppress_flag(self)
    }
    fn set_suppress_true(&self) -> Result<(), Error> {
        self.event("write-true")?;
        self.flag.replace(Rc::new(Js::Bool(true)));
        Ok(())
    }
    fn restore_suppress_flag(&self, original: Native) -> Result<(), Error> {
        assert!(
            Rc::ptr_eq(&original, &self.original_flag),
            "CRYPTOCOM-ADMIN-NATIVE-ORIGINAL-FLAG"
        );
        self.event("restore")?;
        self.flag.replace(original);
        Ok(())
    }
    fn error_value(&self, error: Error) -> Native {
        error
    }
    fn is_nullish(&self, value: &Native) -> bool {
        matches!(&**value, Js::Undefined | Js::Null)
    }
    fn truthy(&self, value: &Native) -> bool {
        match &**value {
            Js::Undefined | Js::Null => false,
            Js::Bool(v) => *v,
            Js::Number(v) => *v != 0.0 && !v.is_nan(),
            Js::String(v) => !v.is_empty(),
            Js::Object(_) | Js::Symbol => true,
        }
    }
    fn undefined_value(&self) -> Native {
        Rc::new(Js::Undefined)
    }
    fn string_value(&self, value: Vec<u16>) -> Native {
        Rc::new(Js::String(value))
    }
    fn get(&self, value: Native, key: &'static str) -> Result<Native, Error> {
        let tag = match &*value {
            Js::Object(tag) => tag.as_str(),
            _ => return Ok(Rc::new(Js::Undefined)),
        };
        match (tag, key) {
            ("post-error", "response") => {
                self.event("get-response")?;
                self.response_reads.set(self.response_reads.get() + 1);
                Ok(
                    if !self.spec["code"].is_null()
                        || !self.spec["responseMessage"].is_null()
                        || !self.spec["secondCode"].is_null()
                    {
                        self.server_response.clone()
                    } else {
                        Rc::new(Js::Undefined)
                    },
                )
            }
            ("server-response", "data") => {
                self.event("get-data")?;
                Ok(self.server_data.clone())
            }
            ("server-data", "message") => {
                self.event("get-message")?;
                Ok(self.response_message.clone())
            }
            ("server-data", "code") => {
                self.event("get-code")?;
                Ok(if self.spec["fail"] == "code-string" {
                    Rc::new(Js::Object("code-error".into()))
                } else if self.response_reads.get() >= 2 {
                    self.second_code.as_ref().unwrap_or(&self.code).clone()
                } else {
                    self.code.clone()
                })
            }
            ("post-error", "message") => {
                self.event("get-message")?;
                Ok(self.root_message.clone())
            }
            ("phase-error", "message") => Ok(string(&format!(
                "phase {}",
                self.spec["name"].as_str().unwrap()
            ))),
            _ => Ok(Rc::new(Js::Undefined)),
        }
    }
    fn authentication_error_code(&self, code: Native) -> Result<bool, Error> {
        api_guards::is_cryptocom_authentication_error_code(code, |code| {
            if matches!(&*code,Js::Object(tag)if tag=="code-error") {
                self.event("code-string")?;
            }
            Ok(self.value_string(&code))
        })
    }
    fn authentication_failure_message(&self, message: &Native) -> Result<bool, Error> {
        Ok(api_guards::is_cryptocom_authentication_failure_message(
            match &**message {
                Js::String(v) => Some(v),
                _ => None,
            },
        ))
    }
    fn emit_error(&self, message: Native) -> Result<Rc<()>, Error> {
        self.event("emit-get")?;
        self.event("emit-call")?;
        if matches!(&*message,Js::Object(tag)if tag=="message") {
            assert!(
                Rc::ptr_eq(&message, &self.response_message),
                "CRYPTOCOM-ADMIN-NATIVE-EMIT-IDENTITY"
            );
        }
        self.emitted.borrow_mut().push(message);
        Ok(self.prepared.clone())
    }
    fn fetch_balance(&self) -> Result<Rc<()>, Error> {
        self.event("balance-get")?;
        self.event("balance-call")?;
        self.balance_created.set(true);
        Ok(self.prepared.clone())
    }
    fn catch_balance<F: FnMut() -> bodies::BalanceResult<Native> + 'static>(
        &self,
        promise: Rc<()>,
        mut catch: F,
    ) -> Result<BalanceFuture, Error> {
        assert!(
            Rc::ptr_eq(&promise, &self.prepared),
            "CRYPTOCOM-ADMIN-NATIVE-BALANCE-PROMISE-IDENTITY"
        );
        self.event("catch-get")?;
        self.event("catch-call")?;
        let custom = self.spec["customCatch"] == true;
        if custom {
            for _ in 0..2 {
                self.event("catch-handler")?;
                self.catch_returns.borrow_mut().push(catch());
            }
        }
        Ok(BalanceFuture {
            host: self.owner.borrow().upgrade().unwrap(),
            catch: Some(Box::new(catch)),
            custom,
        })
    }
}
enum Outcome {
    Void,
    Value(Native),
    Position(PositionMode),
    Account(bodies::Account<Native>),
    Validation(bodies::ValidationResult<Native, Native>),
}
fn assert_snapshot(actual: &Snapshot, expected: &Value, name: &str, phase: &str) {
    let label = format!("{name} {phase}");
    assert_native(&actual.flag, &expected["flag"], &format!("{label} flag"));
    assert_eq!(
        actual.flag_original,
        expected["flagIsOriginal"].as_bool().unwrap(),
        "CRYPTOCOM-ADMIN-NATIVE-FLAG-IDENTITY {label}"
    );
    assert_eq!(
        actual.disposed,
        expected["disposed"].as_bool().unwrap(),
        "CRYPTOCOM-ADMIN-NATIVE-DISPOSED {label}"
    );
    for (i, (post, s)) in actual
        .posts
        .iter()
        .zip(expected["posts"].as_array().unwrap())
        .enumerate()
    {
        assert_eq!(
            post.path,
            units(&s["endpoint"]),
            "CRYPTOCOM-ADMIN-NATIVE-ENDPOINT {label} post{i}"
        );
        assert_native(&post.flag, &s["flag"], &format!("{label} post{i} flag"));
        match &post.params {
            bodies::AdminParams::Empty => assert_eq!(
                s["params"].as_array().unwrap().len(),
                0,
                "CRYPTOCOM-ADMIN-NATIVE-EMPTY-PARAMS {label}"
            ),
            bodies::AdminParams::Leverage {
                instrument_name,
                leverage,
            } => {
                assert_eq!(
                    s["params"][0][0], "instrument_name",
                    "CRYPTOCOM-ADMIN-NATIVE-PARAM-KEY {label}"
                );
                assert_eq!(
                    s["params"][1][0], "leverage",
                    "CRYPTOCOM-ADMIN-NATIVE-PARAM-KEY {label}"
                );
                assert_native(
                    &Rc::new(Js::String(instrument_name.clone())),
                    &s["params"][0][1],
                    &format!("{label} instrument"),
                );
                assert_native(
                    &Rc::new(Js::String(leverage.clone())),
                    &s["params"][1][1],
                    &format!("{label} leverage"),
                );
                assert_eq!(
                    s["params"].as_array().unwrap().len(),
                    2,
                    "independent leverage field floor"
                );
            }
        }
    }
    for (i, ((message, severity), s)) in actual
        .logs
        .iter()
        .zip(expected["logs"].as_array().unwrap())
        .enumerate()
    {
        assert_eq!(
            *message,
            units(&s["message"]),
            "CRYPTOCOM-ADMIN-NATIVE-LOG-MESSAGE {label} log{i}"
        );
        let severity = match severity {
            bodies::AdminSeverity::Error => "error",
            bodies::AdminSeverity::Warning => "warning",
        };
        assert_eq!(
            severity,
            s["severity"].as_str().unwrap(),
            "CRYPTOCOM-ADMIN-NATIVE-LOG-SEVERITY {label} log{i}"
        );
    }
    for (i, (message, s)) in actual
        .emitted
        .iter()
        .zip(expected["emitted"].as_array().unwrap())
        .enumerate()
    {
        assert_native(message, s, &format!("{label} emitted{i}"));
    }
    for (i, (result, s)) in actual
        .catch_returns
        .iter()
        .zip(expected["catchReturns"].as_array().unwrap())
        .enumerate()
    {
        match result {
            bodies::BalanceResult::Undefined => {
                assert_native(&Rc::new(Js::Undefined), s, &format!("{label} callback{i}"))
            }
            bodies::BalanceResult::Value(v) => assert_native(v, s, &format!("{label} callback{i}")),
        }
    }
    assert_eq!(
        actual.events,
        expected["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>(),
        "CRYPTOCOM-ADMIN-NATIVE-EFFECTS {label}"
    );
}
fn assert_inventory(actual: &Snapshot, expected: &Value, name: &str, phase: &str) {
    let label = format!("{name} {phase}");
    assert_eq!(
        actual.posts.len(),
        expected["posts"].as_array().unwrap().len(),
        "ADMIN source post floor {label}"
    );
    assert_eq!(
        actual.logs.len(),
        expected["logs"].as_array().unwrap().len(),
        "ADMIN source log floor {label}"
    );
    assert_eq!(
        actual.emitted.len(),
        expected["emitted"].as_array().unwrap().len(),
        "ADMIN source emit floor {label}"
    );
    assert_eq!(
        actual.catch_returns.len(),
        expected["catchReturns"].as_array().unwrap().len(),
        "ADMIN source callback floor {label}"
    );
}
fn assert_outcome(result: Result<Outcome, Error>, source: &Value, host: &Host, name: &str) {
    let expected = &source["result"];
    match result {
        Err(error) => {
            assert_eq!(
                expected["kind"], "throw",
                "CRYPTOCOM-ADMIN-NATIVE-RETURN {name}"
            );
            assert_native(&error, &source["thrown"], &format!("{name} thrown"));
            let tag = if let Js::Object(tag) = &*error {
                tag.as_str()
            } else {
                ""
            };
            let original = match tag {
                "phase-error" => Some(&host.phase_error),
                "post-error" => Some(&host.post_error),
                "constructed" => Some(&host.constructed),
                "unknown" => Some(&host.type_error),
                _ => None,
            };
            if let Some(original) = original {
                assert!(
                    Rc::ptr_eq(&error, original),
                    "CRYPTOCOM-ADMIN-NATIVE-ERROR-IDENTITY {name}"
                );
            }
            if tag == "constructed" {
                assert_eq!(
                    *host.constructed_message.borrow(),
                    units(&source["thrown"]["message"]),
                    "CRYPTOCOM-ADMIN-NATIVE-MARGIN-MESSAGE {name}"
                );
                assert_eq!(source["thrown"]["name"], "Error");
            }
        }
        Ok(Outcome::Void) => {
            assert_eq!(
                expected["kind"], "value",
                "CRYPTOCOM-ADMIN-NATIVE-RETURN {name}"
            );
            assert_native(
                &Rc::new(Js::Undefined),
                &expected["value"],
                &format!("{name} void"),
            );
        }
        Ok(Outcome::Value(value)) => {
            assert_eq!(expected["kind"], "value");
            assert_native(&value, &expected["value"], &format!("{name} getter"));
            assert!(
                Rc::ptr_eq(&value, &host.original_flag),
                "CRYPTOCOM-ADMIN-NATIVE-GETTER-IDENTITY {name}"
            );
        }
        Ok(Outcome::Position(mode)) => {
            let source = observed(&expected["value"]);
            match &*source {
                Js::String(s) if *s == "hedged".encode_utf16().collect::<Vec<_>>() => assert_eq!(
                    mode,
                    PositionMode::Hedged,
                    "CRYPTOCOM-ADMIN-NATIVE-POSITION-MODE {name}"
                ),
                _ => panic!("CRYPTOCOM-ADMIN-NATIVE-POSITION-MODE {name}"),
            }
        }
        Ok(Outcome::Account(account)) => {
            assert_eq!(expected["kind"], "account");
            assert_native(
                &account.user_id,
                &expected["userId"],
                &format!("{name} userId"),
            );
            assert!(
                Rc::ptr_eq(&account.user_id, &host.uuid),
                "CRYPTOCOM-ADMIN-NATIVE-UID-IDENTITY {name}"
            );
            assert!(expected["userIsOriginal"].as_bool().unwrap());
            assert_eq!(
                account.affiliate_id,
                bodies::AffiliateId::Null,
                "CRYPTOCOM-ADMIN-NATIVE-AFFILIATE {name}"
            );
            assert_native(
                &Rc::new(Js::Null),
                &expected["affiliateId"],
                &format!("{name} affiliate"),
            );
            assert_eq!(
                expected["keys"],
                serde_json::json!(["userId", "affiliateId"]),
                "independent account field inventory"
            );
        }
        Ok(Outcome::Validation(result)) => {
            assert_eq!(
                expected["kind"], "validation",
                "CRYPTOCOM-ADMIN-NATIVE-RETURN {name}"
            );
            match result {
                bodies::ValidationResult::Success { error, balance } => {
                    assert_native(
                        &Rc::new(Js::String(error)),
                        &expected["error"],
                        &format!("{name} success error"),
                    );
                    assert!(
                        expected["hasBalance"].as_bool().unwrap(),
                        "CRYPTOCOM-ADMIN-NATIVE-BALANCE-PRESENCE {name}"
                    );
                    match balance {
                        bodies::BalanceResult::Value(balance) => {
                            assert_native(
                                &balance,
                                &expected["balance"],
                                &format!("{name} balance"),
                            );
                            assert!(
                                Rc::ptr_eq(&balance, &host.balance),
                                "CRYPTOCOM-ADMIN-NATIVE-BALANCE-IDENTITY {name}"
                            );
                            assert!(expected["balanceIsOriginal"].as_bool().unwrap());
                        }
                        bodies::BalanceResult::Undefined => assert_native(
                            &Rc::new(Js::Undefined),
                            &expected["balance"],
                            &format!("{name} rejected balance"),
                        ),
                    }
                    assert_eq!(
                        expected["keys"],
                        serde_json::json!(["error", "balance"]),
                        "independent success field inventory"
                    );
                }
                bodies::ValidationResult::Failure { error } => {
                    assert_native(&error, &expected["error"], &format!("{name} failure error"));
                    if matches!(&*error,Js::Object(tag)if tag=="message") {
                        assert!(
                            Rc::ptr_eq(&error, &host.response_message),
                            "CRYPTOCOM-ADMIN-NATIVE-FAILURE-MESSAGE-IDENTITY {name}"
                        );
                    }
                    assert!(
                        !expected["hasBalance"].as_bool().unwrap(),
                        "CRYPTOCOM-ADMIN-NATIVE-BALANCE-PRESENCE {name}"
                    );
                    assert_eq!(
                        expected["keys"],
                        serde_json::json!(["error"]),
                        "independent failure field inventory"
                    );
                }
            }
        }
    }
}
fn poll(
    future: &mut Pin<Box<dyn Future<Output = Result<Outcome, Error>>>>,
) -> Poll<Result<Outcome, Error>> {
    future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
}
#[test]
fn account_admin_matches_actual_typescript_native() {
    let fixtures: Value =
        serde_json::from_str(include_str!("account_admin_fixtures.json")).unwrap();
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/account_admin_oracle.cjs"
        ))
        .env(
            "SAFE_CEX_TYPESCRIPT_PATH",
            std::env::var("SAFE_CEX_TYPESCRIPT_PATH").expect("compiler path required"),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = &corpus["rows"];
    for (index, spec) in fixtures.as_array().unwrap().iter().enumerate() {
        let source = &rows[index];
        let name = spec["name"].as_str().unwrap();
        let original_flag = if spec["flag"] == "undefined" {
            Rc::new(Js::Undefined)
        } else {
            input(&spec["flag"], Rc::new(Js::Bool(false)))
        };
        let post_error = Rc::new(Js::Object("post-error".into()));
        let thrown_post = if spec.as_object().unwrap().contains_key("throwValue") {
            if spec["throwValue"].is_null() {
                Rc::new(Js::Null)
            } else {
                input(&spec["throwValue"], Rc::new(Js::Undefined))
            }
        } else {
            post_error.clone()
        };
        let host = Rc::new(Host {
            spec: spec.clone(),
            owner: RefCell::new(Weak::new()),
            flag: RefCell::new(original_flag.clone()),
            original_flag,
            disposed: Cell::new(false),
            events: RefCell::new(vec![]),
            posts: RefCell::new(vec![]),
            logs: RefCell::new(vec![]),
            emitted: RefCell::new(vec![]),
            catch_returns: RefCell::new(vec![]),
            phase_error: Rc::new(Js::Object("phase-error".into())),
            post_error,
            thrown_post,
            type_error: Rc::new(Js::Object("unknown".into())),
            constructed: Rc::new(Js::Object("constructed".into())),
            constructed_message: RefCell::new(vec![]),
            prepared: Rc::new(()),
            response: Rc::new(Js::Object("response".into())),
            data: Rc::new(Js::Object("data".into())),
            account: Rc::new(Js::Object("this-account".into())),
            uuid: input(&spec["uid"], string("USER")),
            balance: Rc::new(Js::Object("balance".into())),
            post_ready: Cell::new(spec["heldPost"] != true),
            balance_created: Cell::new(false),
            balance_settled: Cell::new(false),
            balance_ready: Cell::new(spec["heldBalance"] != true),
            response_reads: Cell::new(0),
            response_message: input(&spec["responseMessage"], Rc::new(Js::Undefined)),
            root_message: input(&spec["rootMessage"], string(&format!("post {name}"))),
            code: input(&spec["code"], Rc::new(Js::Undefined)),
            second_code: (!spec["secondCode"].is_null())
                .then(|| input(&spec["secondCode"], Rc::new(Js::Undefined))),
            server_response: Rc::new(Js::Object("server-response".into())),
            server_data: Rc::new(Js::Object("server-data".into())),
        });
        host.owner.replace(Rc::downgrade(&host));
        let op = spec["op"].as_str().unwrap();
        let mut future: Pin<Box<dyn Future<Output = Result<Outcome, Error>>>> = match op {
            "getter" => {
                let result = bodies::suppress_cryptocom_validation_error_emission(&*host)
                    .map(Outcome::Value);
                Box::pin(std::future::ready(result))
            }
            "dispose" => {
                let result = bodies::dispose(&*host).map(|_| Outcome::Void);
                Box::pin(std::future::ready(result))
            }
            "position" => {
                let f = bodies::get_position_mode();
                Box::pin(async move { Ok(Outcome::Position(f.await)) })
            }
            "leverage" => {
                let f = bodies::set_leverage(
                    host.clone(),
                    text(&spec["symbol"]),
                    number(&spec["leverage"]),
                    None,
                );
                Box::pin(async move { f.await.map(|_| Outcome::Void) })
            }
            "cancel" => {
                let f = bodies::cancel_all_orders(host.clone());
                Box::pin(async move { f.await.map(|_| Outcome::Void) })
            }
            "mode" => {
                let f = bodies::change_position_mode(&*host, spec["hedged"].as_bool());
                Box::pin(async move { f.await.map(|_| Outcome::Void) })
            }
            "margin" => {
                let f = bodies::change_margin_type(
                    &*host,
                    spec["isolated"].as_bool().unwrap(),
                    (!spec["symbol"].is_null()).then(|| text(&spec["symbol"])),
                );
                Box::pin(async move { f.await.map(|_| Outcome::Void) })
            }
            "account" => {
                let f = bodies::get_account(host.clone());
                Box::pin(async move { f.await.map(Outcome::Account) })
            }
            "validate" => {
                let f = bodies::validate_account(host.clone());
                Box::pin(async move { f.await.map(Outcome::Validation) })
            }
            _ => panic!("unknown op"),
        };
        let immediate = host.snapshot();
        let mut waiting = None;
        assert_snapshot(&immediate, &source["immediate"], name, "before-poll");
        if spec["heldPost"] == true {
            assert!(
                matches!(poll(&mut future), Poll::Pending),
                "ADMIN held RPC must be pending"
            );
            waiting = Some((host.snapshot(), "waiting-post"));
            assert_snapshot(
                &waiting.as_ref().unwrap().0,
                &source["waiting"],
                name,
                "waiting-post",
            );
            host.post_ready.set(true);
        }
        if spec["heldBalance"] == true {
            assert!(
                matches!(poll(&mut future), Poll::Pending),
                "ADMIN held balance must be pending"
            );
            waiting = Some((host.snapshot(), "waiting-balance"));
            assert_snapshot(
                &waiting.as_ref().unwrap().0,
                &source["waiting"],
                name,
                "waiting-balance",
            );
            host.balance_ready.set(true);
        }
        let result = match poll(&mut future) {
            Poll::Ready(result) => result,
            Poll::Pending => panic!("ADMIN released promises must finish"),
        };
        host.drive_background_balance();
        let final_snapshot = host.snapshot();
        assert_snapshot(&final_snapshot, &source["final"], name, "final");
        assert_outcome(result, source, &host, name);
        assert_inventory(&immediate, &source["immediate"], name, "before-poll");
        if let Some((snapshot, phase)) = waiting {
            assert_inventory(&snapshot, &source["waiting"], name, phase);
        }
        assert_inventory(&final_snapshot, &source["final"], name, "final");
        assert_eq!(
            host.posts.borrow().len() as u64,
            spec["expectedPosts"].as_u64().unwrap(),
            "ADMIN independent RPC count {name}"
        );
        assert_eq!(source["name"], spec["name"]);
    }
    assert_eq!(
        corpus["positionMode"], "hedged",
        "independent current mode constant"
    );
    assert_eq!(corpus["severity"]["Warning"], "warning");
    assert_eq!(corpus["severity"]["Error"], "error");
    assert_eq!(
        fixtures.as_array().unwrap().len(),
        93,
        "ADMIN independent input floor"
    );
    assert_eq!(
        rows.as_array().unwrap().len(),
        93,
        "ADMIN independent output floor"
    );
    assert_eq!(fixtures[92]["name"], "validation-restore-overrides-error");
}
