use safe_cex_venue_paradex::auth_initialization::{
    self as api, AuthConstruction as Build, AuthInitializationHost as Host, AuthOperation as Op,
};
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
}
#[derive(Debug)]
struct Failure {
    value: Raw,
    primary: bool,
    history: bool,
    original: Option<Rc<Failure>>,
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
struct Fixture {
    profile: String,
    failure: String,
    primary: Error,
    trace: RefCell<Vec<Vec<u16>>>,
    counts: RefCell<Vec<(String, usize)>>,
    logs: RefCell<Vec<Value>>,
    consoles: RefCell<Vec<Value>>,
    constructs: RefCell<Vec<Value>>,
    operations: RefCell<Vec<Value>>,
    emitted: RefCell<Vec<Raw>>,
    headers: RefCell<Vec<Value>>,
    opts: Raw,
    config: Raw,
    provider: Raw,
    account: Raw,
    sdk_account: Raw,
    signer: Raw,
    manager: Raw,
    states: RefCell<Vec<(String, Raw)>>,
    attempts: Cell<usize>,
    awaited: Cell<usize>,
    headers_read: Cell<usize>,
    pending: RefCell<Vec<(usize, Option<Result<Raw, Error>>)>>,
}
impl Fixture {
    fn new(profile: &str, failure: &str) -> Self {
        let opts = obj(
            "opts",
            vec![
                (
                    "key",
                    if profile == "missing-key" {
                        raw("")
                    } else if profile == "subkey" {
                        raw(&"x".repeat(66))
                    } else {
                        surrogate("private-")
                    },
                ),
                (
                    "paradexAddress",
                    if profile == "missing-address" {
                        raw("")
                    } else {
                        surrogate("0xABC-")
                    },
                ),
                (
                    "subkeyAddress",
                    if profile == "missing-subkey" {
                        raw("")
                    } else {
                        surrogate("sub-")
                    },
                ),
                ("testnet", Rc::new(Node::Bool(profile == "testnet"))),
                (
                    "payloadSigner",
                    if profile == "payload" {
                        obj("signer", vec![])
                    } else {
                        undef()
                    },
                ),
            ],
        );
        let config = obj("config", vec![("paradexFullNodeRpcUrl", surrogate("rpc-"))]);
        let provider = obj("provider", vec![("tag", raw("provider"))]);
        let account = obj("account", vec![("address", surrogate("account-"))]);
        let sdk_account = obj(
            "sdk-account",
            vec![
                ("account", account.clone()),
                ("config", config.clone()),
                ("provider", provider.clone()),
            ],
        );
        Self {
            profile: profile.into(),
            failure: failure.into(),
            primary: Rc::new(Failure {
                value: obj("error", vec![("message", surrogate("primary-"))]),
                primary: true,
                history: false,
                original: None,
            }),
            trace: RefCell::new(vec![]),
            counts: RefCell::new(vec![]),
            logs: RefCell::new(vec![]),
            consoles: RefCell::new(vec![]),
            constructs: RefCell::new(vec![]),
            operations: RefCell::new(vec![]),
            emitted: RefCell::new(vec![]),
            headers: RefCell::new(vec![]),
            opts,
            config,
            provider,
            account,
            sdk_account,
            signer: obj("signer", vec![]),
            manager: obj("manager", vec![]),
            states: RefCell::new(
                [
                    "sdkInitPromise",
                    "sdkAccount",
                    "requestSigner",
                    "jwtManager",
                ]
                .iter()
                .map(|k| ((*k).into(), Rc::new(Node::Null)))
                .collect(),
            ),
            attempts: Cell::new(0),
            awaited: Cell::new(0),
            headers_read: Cell::new(0),
            pending: RefCell::new(vec![]),
        }
    }
    fn touch(&self, key: &str) -> Result<(), Error> {
        let mut counts = self.counts.borrow_mut();
        let i = counts
            .iter()
            .position(|(k, _)| k == key)
            .unwrap_or_else(|| {
                counts.push((key.into(), 0));
                counts.len() - 1
            });
        let label = format!("{key}#{}", counts[i].1);
        counts[i].1 += 1;
        self.trace.borrow_mut().push(u(&label));
        if label == self.failure {
            Err(self.primary.clone())
        } else {
            Ok(())
        }
    }
    fn set(&self, key: &str, value: Raw) {
        let mut state = self.states.borrow_mut();
        state.iter_mut().find(|(k, _)| k == key).unwrap().1 = value;
    }
    fn peek(&self, key: &str) -> Raw {
        self.states
            .borrow()
            .iter()
            .find(|(k, _)| k == key)
            .unwrap()
            .1
            .clone()
    }
    fn error(&self, message: Vec<u16>) -> Error {
        Rc::new(Failure {
            value: obj("error", vec![("message", Rc::new(Node::Text(message)))]),
            primary: false,
            history: false,
            original: None,
        })
    }
    fn future(&self, value: Raw) -> api::AuthFuture<'_, Raw, Error> {
        Box::pin(async move { Ok(value) })
    }
    fn drain(&self, attempt: usize) -> Result<Raw, Error> {
        if let Some(result) = self
            .pending
            .borrow()
            .iter()
            .find(|(a, _)| *a == attempt)
            .and_then(|(_, r)| r.clone())
        {
            return result;
        }
        let result = self.touch("await-init").and_then(|()| {
            if self.profile == "init-error" || self.profile == "retry-success" && attempt == 1 {
                Err(self.primary.clone())
            } else {
                if ![
                    "not-ready",
                    "missing-key",
                    "missing-address",
                    "missing-subkey",
                    "missing-manager",
                ]
                .contains(&self.profile.as_str())
                {
                    self.set("jwtManager", self.manager.clone());
                }
                Ok(undef())
            }
        });
        self.pending
            .borrow_mut()
            .iter_mut()
            .find(|(a, _)| *a == attempt)
            .unwrap()
            .1 = Some(result.clone());
        result
    }
    fn outcome(&self, r: &Result<Raw, Error>) -> Value {
        match r {
            Ok(value) => json!({"ok":true,"value":atom(value)}),
            Err(e) => {
                if e.primary {
                    assert!(Rc::ptr_eq(e, &self.primary), "auth_native_error_identity");
                }
                json!({"ok":false,"primary":e.primary,"message":text(&field(&e.value,"message")),"history":e.history,"original":e.original.as_ref().is_some_and(|v|Rc::ptr_eq(v,&self.primary))})
            }
        }
    }
}
impl Host for Fixture {
    type Value = Raw;
    type Error = Error;
    type Log = ();
    type HeaderSetter = usize;
    fn get(&self, v: &Raw, key: &str) -> Result<Raw, Error> {
        if let Node::Object(label, _) = v.as_ref() {
            if *label == "opts" || *label == "config" {
                self.touch(&format!("{label}.{key}"))?;
            }
        }
        if matches!(v.as_ref(), Node::Null | Node::Undefined) {
            return Err(self.error(u("nullish getter")));
        }
        Ok(field(v, key))
    }
    fn optional_get(&self, v: &Raw, k: &str) -> Result<Raw, Error> {
        if matches!(v.as_ref(), Node::Null | Node::Undefined) {
            Ok(undef())
        } else {
            self.get(v, k)
        }
    }
    fn truthy(&self, v: &Raw) -> bool {
        match v.as_ref() {
            Node::Null | Node::Undefined => false,
            Node::Bool(v) => *v,
            Node::Number(v) => *v != 0.0 && !v.is_nan(),
            Node::Text(v) => !v.is_empty(),
            _ => true,
        }
    }
    fn literal(&self, s: &str) -> Raw {
        raw(s)
    }
    fn null(&self) -> Raw {
        Rc::new(Node::Null)
    }
    fn boolean(&self, v: bool) -> Raw {
        Rc::new(Node::Bool(v))
    }
    fn object(&self, fields: Vec<(&'static str, Raw)>) -> Raw {
        obj("built", fields)
    }
    fn template(&self, v: &Raw) -> Result<Vec<u16>, Error> {
        Ok(text(v))
    }
    fn text_value(&self, v: Vec<u16>) -> Raw {
        Rc::new(Node::Text(v))
    }
    fn lowercase(&self, v: Raw) -> Result<Raw, Error> {
        Ok(self.text_value(
            text(&v)
                .into_iter()
                .map(|c| if (65..=90).contains(&c) { c + 32 } else { c })
                .collect(),
        ))
    }
    fn length(&self, v: &Raw) -> Result<f64, Error> {
        Ok(text(v).len() as f64)
    }
    fn trim_empty(&self, v: &Raw) -> Result<bool, Error> {
        Ok(text(v).iter().all(|c| matches!(*c, 9 | 10 | 13 | 32)))
    }
    fn state(&self, k: &str) -> Result<Raw, Error> {
        if k == "options" {
            return Ok(self.opts.clone());
        }
        self.touch(&format!("state:{k}"))?;
        Ok(self.peek(k))
    }
    fn write_state(&self, k: &str, v: Raw) -> Result<(), Error> {
        self.touch(&format!("write:{k}"))?;
        self.set(k, v);
        Ok(())
    }
    fn operation(&self, op: Op, args: Vec<Raw>) -> Result<api::AuthFuture<'_, Raw, Error>, Error> {
        match op {
            Op::LoadSdk => {
                self.touch("load-sdk")?;
                Ok(self.future(obj("sdk", vec![])))
            }
            Op::Config => {
                self.touch("fetch-config")?;
                self.operations.borrow_mut().push(json!({"op":"Config","environment":String::from_utf16(&text(&args[1])).unwrap()}));
                self.get(&self.config, "then")?;
                self.get(&self.config, "then")?;
                Ok(self.future(self.config.clone()))
            }
            Op::ImportAccount => {
                self.touch("load-account")?;
                Ok(self.future(obj("class", vec![])))
            }
            Op::FromEthereum => {
                self.touch("from-ethereum")?;
                self.operations
                    .borrow_mut()
                    .push(json!({"op":"FromEthereum","args":atom(&args[1])}));
                Ok(self.future(self.account.clone()))
            }
            Op::InitializeSdk => {
                self.touch("initialize-sdk")?;
                assert!(
                    Rc::ptr_eq(&args[0], &self.opts),
                    "sdk_native_options_identity"
                );
                self.operations
                    .borrow_mut()
                    .push(json!({"op":"InitializeSdk","same":true}));
                Ok(self.future(if self.profile == "null-account" {
                    self.null()
                } else {
                    self.sdk_account.clone()
                }))
            }
            Op::Wait => {
                self.touch("wait")?;
                self.operations.borrow_mut().push(json!({"op":"Wait","delay":bits(match args[0].as_ref(){Node::Number(n)=>*n,_=>panic!()})}));
                Ok(self.future(undef()))
            }
            Op::GetAccount => {
                self.touch("get-account")?;
                self.operations.borrow_mut().push(json!({"op":"GetAccount","endpoint":String::from_utf16(&text(&args[0])).unwrap()}));
                if self.profile == "api-error" {
                    put(
                        &self.primary.value,
                        "response",
                        obj(
                            "response",
                            vec![(
                                "data",
                                obj(
                                    "data",
                                    vec![(
                                        "error",
                                        obj("error-data", vec![("message", surrogate("api-"))]),
                                    )],
                                ),
                            )],
                        ),
                    );
                    return Ok(Box::pin(async { Err(self.primary.clone()) }));
                }
                Ok(self.future(obj("response", vec![])))
            }
            Op::GetToken => {
                self.state("jwtManager")?;
                self.touch("get-token")?;
                self.operations.borrow_mut().push(json!({"op":"GetToken"}));
                if self.profile == "held-token" {
                    return Ok(Box::pin(std::future::poll_fn(|_| {
                        if self.awaited.get() == 0 {
                            self.awaited.set(1);
                            Poll::Pending
                        } else {
                            Poll::Ready(Ok(surrogate("token-")))
                        }
                    })));
                }
                if self.profile == "token-error" {
                    return Ok(Box::pin(async { Err(self.primary.clone()) }));
                }
                Ok(self.future(if self.profile == "empty-token" {
                    raw("")
                } else if self.profile == "blank-token" {
                    raw(" \t")
                } else {
                    surrogate("token-")
                }))
            }
        }
    }
    fn construct(&self, kind: Build, args: Vec<Raw>) -> Result<Raw, Error> {
        match kind {
            Build::Provider => {
                self.touch("provider")?;
                assert!(
                    Rc::ptr_eq(&args[1], &self.config),
                    "sdk_native_provider_config_identity"
                );
                self.constructs
                    .borrow_mut()
                    .push(json!({"kind":"Provider","same":true}));
                Ok(self.provider.clone())
            }
            Build::Wallet => {
                self.touch("wallet")?;
                self.constructs
                    .borrow_mut()
                    .push(json!({"kind":"Wallet","key":atom(&args[0])}));
                Ok(obj("wallet", vec![("tag", raw("wallet"))]))
            }
            Build::EthereumAdapter => {
                self.touch("adapter")?;
                self.constructs
                    .borrow_mut()
                    .push(json!({"kind":"EthereumAdapter","wallet":atom(&args[1])}));
                Ok(obj("adapter", vec![("tag", raw("ethereum-signer"))]))
            }
            Build::StarknetAccount => {
                self.touch("stark-account")?;
                self.constructs.borrow_mut().push(json!({"kind":"StarknetAccount","node":atom(&args[1]),"address":atom(&args[2]),"key":atom(&args[3])}));
                Ok(self.account.clone())
            }
            Build::RequestSigner => {
                self.touch("signer")?;
                assert!(
                    Rc::ptr_eq(&args[0], &self.sdk_account) && Rc::ptr_eq(&args[1], &self.opts),
                    "exchange_native_signer_identity"
                );
                self.constructs
                    .borrow_mut()
                    .push(json!({"kind":"RequestSigner","same":true}));
                Ok(self.signer.clone())
            }
            Build::JwtManager => {
                self.touch("manager")?;
                assert!(
                    Rc::ptr_eq(&args[0], &self.sdk_account) && Rc::ptr_eq(&args[3], &self.opts),
                    "exchange_native_manager_identity"
                );
                self.constructs.borrow_mut().push(json!({"kind":"JwtManager","same":true,"testnet":atom(&args[1]),"subkey":atom(&args[2])}));
                Ok(self.manager.clone())
            }
        }
    }
    fn await_value(&self, v: Raw) -> Result<api::AuthFuture<'_, Raw, Error>, Error> {
        let attempt = match field(&v, "attempt").as_ref() {
            Node::Number(n) => *n as usize,
            _ => panic!("not promise"),
        };
        Ok(Box::pin(async move {
            self.awaited.set(self.awaited.get() + 1);
            self.drain(attempt)
        }))
    }
    fn initialize_promise(&self) -> Result<Raw, Error> {
        self.touch("initialize-promise")?;
        let attempt = self.attempts.get() + 1;
        self.attempts.set(attempt);
        self.operations
            .borrow_mut()
            .push(json!({"op":"InitializePromise","attempt":attempt}));
        self.pending.borrow_mut().push((attempt, None));
        Ok(obj("promise", vec![("attempt", n(attempt as f64))]))
    }
    fn observe_rejection(&self, _: Raw) -> Result<(), Error> {
        self.touch("observe")
    }
    fn reset_sdk(&self) -> Result<(), Error> {
        self.touch("reset")?;
        for k in [
            "sdkInitPromise",
            "sdkAccount",
            "requestSigner",
            "jwtManager",
        ] {
            self.set(k, self.null());
        }
        Ok(())
    }
    fn new_error(&self, msg: Vec<u16>) -> Result<Error, Error> {
        Ok(self.error(msg))
    }
    fn history_error(&self, operation: &[u16], last: Option<Error>) -> Result<Error, Error> {
        let original = last.as_ref().map(|e| e.value.clone()).unwrap_or_else(undef);
        let value = api::initialize_history_auth_error(
            operation,
            &original,
            |v| Ok(matches!(v.as_ref(), Node::Object("error", _))),
            |v| Ok(field(v, "message")),
            |v| self.truthy(v),
            |v| Ok(text(&v)),
            |message| Ok::<_, Error>(obj("error", vec![("message", self.text_value(message))])),
            |value, name| {
                put(value, "name", raw(name));
                Ok(())
            },
            |value, original| {
                put(value, "originalError", original.clone());
                Ok(())
            },
        )?;
        Ok(Rc::new(Failure {
            value,
            primary: false,
            history: true,
            original: last,
        }))
    }
    fn error_value(&self, e: &Error) -> Raw {
        e.value.clone()
    }
    fn emit(&self, v: Raw) -> Result<(), Error> {
        self.touch("emit")?;
        self.emitted.borrow_mut().push(v);
        Ok(())
    }
    fn select_log(&self) -> Result<(), Error> {
        Ok(())
    }
    fn log(&self, _: (), t: Vec<u16>, severity: &'static str) -> Result<(), Error> {
        self.touch("log")?;
        self.logs
            .borrow_mut()
            .push(json!({"text":t,"severity":severity}));
        Ok(())
    }
    fn console(&self, error: bool, prefix: &'static str, value: Option<Raw>) -> Result<(), Error> {
        self.touch(if error { "console-error" } else { "console" })?;
        let args: Vec<_> = value
            .iter()
            .map(|v| {
                if Rc::ptr_eq(v, &self.primary.value) {
                    json!({"primary":true})
                } else {
                    atom(v)
                }
            })
            .collect();
        self.consoles
            .borrow_mut()
            .push(json!({"error":error,"prefix":prefix,"args":args}));
        Ok(())
    }
    fn header_setter(&self, _: &Raw) -> Result<usize, Error> {
        self.touch("headers")?;
        let index = self.headers_read.get();
        self.headers_read.set(index + 1);
        Ok(index.min(2))
    }
    fn set_header(&self, setter: usize, name: &'static str, value: Raw) -> Result<Raw, Error> {
        self.touch("set-header")?;
        self.headers
            .borrow_mut()
            .push(json!({"receiver":setter,"name":name,"value":atom(&value)}));
        Ok(undef())
    }
    fn signer_address(&self, _: Raw) -> Result<Raw, Error> {
        self.touch("signer-address")?;
        Ok(field(&self.account, "address"))
    }
    fn select_ethereum(&self, sdk: &Raw) -> Result<Raw, Error> {
        self.touch("select-ethereum")?;
        Ok(sdk.clone())
    }
    fn number(&self, n: f64) -> Raw {
        self::n(n)
    }
}
fn run_case(row: &Value) {
    let kind = row["kind"].as_str().unwrap();
    let profile = row["profile"].as_str().unwrap();
    let failure = row["failure"].as_str().unwrap();
    let h = Fixture::new(profile, failure);
    if profile == "ready" || kind == "authenticate" && profile != "missing-manager" {
        h.set("jwtManager", h.manager.clone());
    }
    if profile == "existing-init" {
        h.attempts.set(1);
        h.pending.borrow_mut().push((1, None));
        h.set("sdkInitPromise", obj("promise", vec![("attempt", n(1.0))]));
    }
    let request = obj("request", vec![]);
    let result = match kind {
        "sdk" => block_on(api::initialize_paradex_sdk(&h, h.opts.clone())),
        "initializeSDK" => block_on(api::initialize_exchange_sdk(&h)).map(|()| undef()),
        "ensurePrivateHistoryAuth" => {
            block_on(api::ensure_private_history_auth(&h, &su("history-"))).map(|()| undef())
        }
        "validateAccount" => block_on(api::validate_account(&h)),
        "authenticate" => block_on(api::authenticate_request(&h, request.clone())),
        _ => panic!("unknown kind"),
    };
    if kind == "ensurePrivateHistoryAuth" {
        let delays: Vec<_> = h
            .operations
            .borrow()
            .iter()
            .filter(|v| v["op"] == "Wait")
            .map(|v| v["delay"].clone())
            .collect();
        assert!(delays.len() <= 2, "history_auth_native_delay_count");
        for (index, delay) in delays.iter().enumerate() {
            assert_eq!(
                *delay,
                json!(bits([500.0, 1500.0][index])),
                "history_auth_native_delay_values"
            );
        }
    }
    if failure == "none" {
        match (kind, profile) {
            ("sdk", "basic" | "subkey" | "testnet") => {
                let value = result.as_ref().unwrap();
                assert!(
                    Rc::ptr_eq(&field(value, "account"), &h.account),
                    "sdk_native_account_identity"
                );
                assert!(
                    Rc::ptr_eq(&field(value, "config"), &h.config),
                    "sdk_native_config_identity"
                );
                assert!(
                    Rc::ptr_eq(&field(value, "provider"), &h.provider),
                    "sdk_native_provider_identity"
                );
                let keys = if let Node::Object(_, m) = value.as_ref() {
                    m.borrow()
                        .iter()
                        .map(|(k, _)| k.clone())
                        .collect::<Vec<_>>()
                } else {
                    panic!()
                };
                assert_eq!(
                    keys,
                    vec!["account", "config", "provider"],
                    "sdk_native_record_fields"
                );
                assert_eq!(
                    h.operations.borrow()[0]["environment"],
                    if profile == "testnet" {
                        "testnet"
                    } else {
                        "prod"
                    },
                    "sdk_native_environment"
                );
                if profile == "subkey" {
                    assert_eq!(
                        h.constructs.borrow()[1]["kind"],
                        "StarknetAccount",
                        "sdk_native_subkey_mode"
                    );
                } else {
                    assert_eq!(
                        h.constructs.borrow()[1]["kind"],
                        "Wallet",
                        "sdk_native_ethereum_mode"
                    );
                }
            }
            ("sdk", "payload") => {
                let value = result.as_ref().unwrap();
                assert_eq!(
                    text(&field(&field(value, "account"), "address")),
                    su("0xabc-"),
                    "sdk_native_payload_address"
                );
                assert_eq!(
                    h.constructs.borrow().len(),
                    1,
                    "sdk_native_payload_no_key_wallet"
                );
                assert!(
                    h.consoles.borrow().is_empty(),
                    "sdk_native_payload_no_account_log"
                );
            }
            ("initializeSDK", "basic" | "testnet") => {
                assert!(result.is_ok(), "exchange_init_native_success");
                assert!(
                    Rc::ptr_eq(&h.peek("sdkAccount"), &h.sdk_account),
                    "exchange_init_native_account"
                );
                assert!(
                    Rc::ptr_eq(&h.peek("requestSigner"), &h.signer),
                    "exchange_init_native_signer"
                );
                assert!(
                    Rc::ptr_eq(&h.peek("jwtManager"), &h.manager),
                    "exchange_init_native_manager"
                );
                assert_eq!(
                    h.constructs.borrow().len(),
                    2,
                    "exchange_init_native_constructors"
                );
            }
            ("ensurePrivateHistoryAuth", "basic") => {
                assert!(result.is_ok(), "history_auth_native_success");
                assert_eq!(h.attempts.get(), 1, "history_auth_native_attempts");
                assert!(
                    Rc::ptr_eq(&h.peek("jwtManager"), &h.manager),
                    "history_auth_native_manager"
                );
            }
            ("ensurePrivateHistoryAuth", "missing-address") => {
                assert!(
                    result.as_ref().unwrap_err().history,
                    "history_auth_native_missing_wrapper"
                );
                assert_eq!(h.attempts.get(), 0, "history_auth_native_missing_attempts");
                let delays: Vec<_> = h
                    .operations
                    .borrow()
                    .iter()
                    .filter(|v| v["op"] == "Wait")
                    .map(|v| v["delay"].clone())
                    .collect();
                assert_eq!(
                    delays,
                    vec![json!(bits(500.0)), json!(bits(1500.0))],
                    "history_auth_native_exhausted_delays"
                );
            }
            ("ensurePrivateHistoryAuth", "retry-success") => {
                assert!(result.is_ok(), "history_auth_native_retry_success");
                assert_eq!(h.attempts.get(), 2, "history_auth_native_retry_count");
                let delays: Vec<_> = h
                    .operations
                    .borrow()
                    .iter()
                    .filter(|v| v["op"] == "Wait")
                    .map(|v| v["delay"].clone())
                    .collect();
                assert_eq!(
                    delays,
                    vec![json!(bits(500.0))],
                    "history_auth_native_retry_delay"
                );
            }
            ("ensurePrivateHistoryAuth", "not-ready" | "init-error") => {
                let e = result.as_ref().unwrap_err();
                assert!(e.history, "history_auth_native_wrapper");
                assert_eq!(
                    h.attempts.get(),
                    3,
                    "history_auth_native_exhausted_attempts"
                );
                let delays: Vec<_> = h
                    .operations
                    .borrow()
                    .iter()
                    .filter(|v| v["op"] == "Wait")
                    .map(|v| v["delay"].clone())
                    .collect();
                assert_eq!(
                    delays,
                    vec![json!(bits(500.0)), json!(bits(1500.0))],
                    "history_auth_native_exhausted_delays"
                );
                if profile == "init-error" {
                    assert!(
                        Rc::ptr_eq(e.original.as_ref().unwrap(), &h.primary),
                        "history_auth_native_original_error"
                    );
                }
            }
            ("validateAccount", "basic" | "ready") => {
                assert!(result.is_ok(), "validation_native_success");
                assert_eq!(
                    text(&field(result.as_ref().unwrap(), "error")),
                    u(""),
                    "validation_native_error_field"
                );
                assert_eq!(
                    h.operations.borrow().last().unwrap()["endpoint"],
                    "/account",
                    "validation_native_account_request"
                );
            }
            ("validateAccount", "api-error") => {
                assert!(result.is_ok(), "validation_native_caught_success");
                assert_eq!(
                    text(&field(result.as_ref().unwrap(), "error")),
                    su("api-"),
                    "validation_native_api_message"
                );
                assert_eq!(h.emitted.borrow().len(), 1, "validation_native_emission");
            }
            ("authenticate", "basic") => {
                assert!(
                    Rc::ptr_eq(result.as_ref().unwrap(), &request),
                    "request_auth_native_config_identity"
                );
                assert_eq!(
                    *h.headers.borrow(),
                    vec![
                        json!({"receiver":0,"name":"Authorization","value":{"string":su("Bearer token-")}}),
                        json!({"receiver":1,"name":"Content-Type","value":{"string":u("application/json")}}),
                        json!({"receiver":2,"name":"Accept","value":{"string":u("application/json")}})
                    ],
                    "request_auth_native_headers"
                );
            }
            ("authenticate", "empty-token" | "blank-token") => {
                assert!(result.is_err(), "request_auth_native_empty_reject");
                assert!(
                    h.headers.borrow().is_empty(),
                    "request_auth_native_empty_headers"
                );
            }
            _ => {}
        }
    }
    if kind == "authenticate" && profile == "basic" && failure == "set-header#1" {
        assert!(result.is_err(), "request_auth_native_partial_failure");
        assert_eq!(
            h.headers.borrow().len(),
            1,
            "request_auth_native_partial_header_count"
        );
        assert_eq!(
            h.headers.borrow()[0]["name"],
            "Authorization",
            "request_auth_native_partial_header"
        );
    }
    let mut outcome = h.outcome(&result);
    if let Ok(value) = &result {
        if kind == "authenticate" {
            assert!(
                Rc::ptr_eq(value, &request),
                "request_auth_native_return_identity"
            );
            outcome = json!({"ok":true,"value":{"same":true}});
        }
        if kind == "sdk" {
            outcome = json!({"ok":true,"value":{"account":if Rc::ptr_eq(&field(value,"account"),&h.account){json!("original")}else{atom(&field(value,"account"))},"config":Rc::ptr_eq(&field(value,"config"),&h.config),"provider":Rc::ptr_eq(&field(value,"provider"),&h.provider),"null":matches!(value.as_ref(),Node::Null)}});
        }
    }
    assert_eq!(
        outcome, row["outcome"],
        "auth_supplemental_outcome {kind}/{profile}/{failure}"
    );
    assert_eq!(
        json!(*h.trace.borrow()),
        row["trace"],
        "auth_supplemental_trace {kind}/{profile}/{failure}"
    );
    for (key, actual) in [
        ("logs", json!(*h.logs.borrow())),
        ("consoles", json!(*h.consoles.borrow())),
        ("constructs", json!(*h.constructs.borrow())),
        ("operations", json!(*h.operations.borrow())),
        ("headers", json!(*h.headers.borrow())),
        (
            "emitted",
            json!(h.emitted.borrow().iter().map(atom).collect::<Vec<_>>()),
        ),
    ] {
        assert_eq!(
            actual, row[key],
            "auth_supplemental_{key} {kind}/{profile}/{failure}"
        );
    }
    assert_eq!(h.attempts.get(), row["attempts"]);
    let slots: serde_json::Map<String, Value> = h
        .states
        .borrow()
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                json!(if matches!(v.as_ref(), Node::Null) {
                    "null"
                } else if Rc::ptr_eq(v, &h.sdk_account) {
                    "account"
                } else if Rc::ptr_eq(v, &h.signer) {
                    "signer"
                } else if Rc::ptr_eq(v, &h.manager) {
                    "manager"
                } else {
                    "promise"
                }),
            )
        })
        .collect();
    assert_eq!(Value::Object(slots), row["slots"]);
}
fn error_case(row: &Value) {
    let profile = row["profile"].as_str().unwrap();
    let h = Fixture::new(profile, row["failure"].as_str().unwrap());
    let original = if profile == "plain" {
        obj("plain", vec![("message", raw("plain"))])
    } else if profile == "absent" {
        undef()
    } else {
        obj("error", vec![("message", surrogate("cause-"))])
    };
    let reads = Cell::new(0);
    let writes = RefCell::new(vec![]);
    let result = api::initialize_history_auth_error(
        &su("history-"),
        &original,
        |v| Ok(matches!(v.as_ref(), Node::Object("error", _))),
        |_| {
            h.touch("message")?;
            let i = reads.get();
            reads.set(i + 1);
            Ok(if profile == "empty" {
                raw("")
            } else if profile == "changing" {
                if i == 0 {
                    raw("first")
                } else {
                    surrogate("second-")
                }
            } else {
                surrogate("cause-")
            })
        },
        |v| h.truthy(v),
        |v| Ok(text(&v)),
        |msg| {
            h.touch("base")?;
            Ok(obj("parent", vec![("message", h.text_value(msg))]))
        },
        |_, name| {
            h.touch("name")?;
            writes.borrow_mut().push(json!(["name", u(name)]));
            Ok(())
        },
        |_, v| {
            h.touch("original")?;
            assert!(
                Rc::ptr_eq(v, &original),
                "history_error_native_original_identity"
            );
            writes.borrow_mut().push(json!(["original", true]));
            Ok(())
        },
    );
    if h.failure == "none" {
        let expected = u("Paradex private auth not ready for history-")
            .into_iter()
            .chain([0xd800])
            .chain(if profile == "basic" {
                u(": cause-")
                    .into_iter()
                    .chain([0xd800])
                    .collect::<Vec<_>>()
            } else if profile == "changing" {
                u(": second-").into_iter().chain([0xd800]).collect()
            } else {
                vec![]
            })
            .collect::<Vec<_>>();
        assert_eq!(
            text(&field(result.as_ref().unwrap(), "message")),
            expected,
            "history_error_native_message"
        );
        assert_eq!(
            writes.borrow()[0],
            json!(["name", u("ParadexPrivateHistoryAuthNotReadyError")]),
            "history_error_native_name"
        );
    }
    let outcome = match result {
        Ok(v) => json!({"ok":true,"message":text(&field(&v,"message"))}),
        Err(e) => {
            assert!(Rc::ptr_eq(&e, &h.primary));
            json!({"ok":false,"primary":true})
        }
    };
    assert_eq!(outcome, row["outcome"]);
    assert_eq!(json!(*h.trace.borrow()), row["trace"]);
    assert_eq!(json!(*writes.borrow()), row["writes"]);
}
fn predicate(row: &Value) {
    let expected = matches!(row["profile"].as_str().unwrap(), "own" | "prototype");
    let calls = Cell::new(0);
    let value = api::is_history_auth_not_ready(|| {
        calls.set(calls.get() + 1);
        Ok::<_, ()>(expected)
    })
    .unwrap();
    assert_eq!(value, expected, "history_predicate_native_brand");
    assert_eq!(calls.get(), 1, "history_predicate_native_calls");
    assert_eq!(value, row["value"]);
}
fn waiting(row: &Value) {
    let ms = f64::from_bits(u64::from_str_radix(row["ms"].as_str().unwrap(), 16).unwrap());
    let failure = row["failure"].as_bool().unwrap();
    let primary = Rc::new(Failure {
        value: obj("error", vec![]),
        primary: true,
        history: false,
        original: None,
    });
    let delay = Cell::new(None);
    let executor_error = RefCell::new(None);
    let settled = Cell::new(false);
    let result = api::wait(
        ms,
        |executor| {
            let run = executor(77);
            if let Err(e) = run {
                settled.set(true);
                *executor_error.borrow_mut() = Some(e);
            }
            Ok::<_, Error>(99)
        },
        |resolve, duration| {
            assert_eq!(resolve, 77, "wait_native_resolver_identity");
            if failure {
                return Err(primary.clone());
            }
            delay.set(Some(duration));
            Ok(88)
        },
    )
    .unwrap();
    assert_eq!(result, 99, "wait_native_promise_identity");
    assert_eq!(
        delay.get().map(bits),
        if failure { None } else { Some(bits(ms)) },
        "wait_native_delay"
    );
    assert_eq!(settled.get(), failure, "wait_native_before_settled");
    assert_eq!(
        json!({"settled":settled.get(),"delays":delay.get().map(bits).into_iter().collect::<Vec<_>>()}),
        row["before"]
    );
    let outcome = if let Some(e) = executor_error.borrow().as_ref() {
        assert!(Rc::ptr_eq(e, &primary), "wait_native_error_identity");
        json!({"ok":false,"primary":true})
    } else {
        settled.set(true);
        json!({"ok":true,"undefined":true})
    };
    assert_eq!(outcome, row["outcome"]);
}
#[test]
fn actual_typescript_auth_initialization() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/auth_initialization_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source_oracle_exit {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let all: Value = serde_json::from_slice(&output.stdout).unwrap();
    let count: usize = all
        .as_object()
        .unwrap()
        .values()
        .map(|v| v.as_array().unwrap().len())
        .sum();
    assert_eq!(count, 846, "source_case_count");
    for row in all["runs"].as_array().unwrap() {
        run_case(row);
    }
    for row in all["errors"].as_array().unwrap() {
        error_case(row);
    }
    for row in all["predicates"].as_array().unwrap() {
        predicate(row);
    }
    for row in all["waits"].as_array().unwrap() {
        waiting(row);
    }
}

#[test]
fn request_waits_for_token_before_headers() {
    let h = Fixture::new("held-token", "none");
    h.set("jwtManager", h.manager.clone());
    let request = obj("request", vec![]);
    let mut cx = Context::from_waker(Waker::noop());
    let mut future = pin!(api::authenticate_request(&h, request.clone()));
    assert!(
        future.as_mut().poll(&mut cx).is_pending(),
        "request_auth_native_pending"
    );
    assert!(
        h.headers.borrow().is_empty(),
        "request_auth_native_pending_headers"
    );
    assert_eq!(
        h.headers_read.get(),
        0,
        "request_auth_native_pending_receivers"
    );
    assert!(
        h.logs.borrow().is_empty(),
        "request_auth_native_pending_logs"
    );
    let Poll::Ready(result) = future.as_mut().poll(&mut cx) else {
        panic!("request did not resume")
    };
    assert!(
        Rc::ptr_eq(&result.unwrap(), &request),
        "request_auth_native_resumed_config"
    );
    assert_eq!(
        h.headers.borrow().len(),
        3,
        "request_auth_native_resumed_headers"
    );
}
