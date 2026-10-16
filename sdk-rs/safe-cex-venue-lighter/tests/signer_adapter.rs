use num_bigint::BigInt;
use safe_cex_venue_lighter::{
    signer_adapter::{
        LighterSignerAdapter, SignerAdapterConstructor, SignerAdapterMethod, SignerAdapterRealm,
        create_lighter_signer_adapter,
    },
    signer_runtime::{LighterSignerRuntime, TealstreetLighterSignerAdapter},
    transaction_builders::TransactionBuildError,
    transaction_promise::{NativePromise, PromiseResult, TransactionRuntime},
    transaction_signing::{AuthTokenEncoding, HostThrownValue, TransactionSigningError as E},
};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    fmt,
    rc::Rc,
};

type H = Rc<RefCell<State>>;
type Adapter = TealstreetLighterSignerAdapter<H, V>;

#[derive(Clone)]
enum V {
    U,
    Null,
    Num(f64),
    Big(BigInt),
    Text(String),
    Obj(Rc<Obj>),
    Method(&'static str, Box<V>),
    Callback,
}
struct Obj {
    tag: &'static str,
    fields: RefCell<Vec<(String, V)>>,
    native: RefCell<Option<Rc<Adapter>>>,
}
impl fmt::Debug for V {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            V::U => write!(f, "undefined"),
            V::Null => write!(f, "null"),
            V::Num(n) => write!(f, "{n}"),
            V::Big(n) => write!(f, "{n}n"),
            V::Text(s) => write!(f, "{s:?}"),
            V::Obj(o) => write!(f, "<{}>", o.tag),
            V::Method(name, _) => write!(f, "<method {name}>"),
            V::Callback => write!(f, "<callback>"),
        }
    }
}
fn obj(tag: &'static str, fields: Vec<(&str, V)>) -> V {
    V::Obj(Rc::new(Obj {
        tag,
        fields: RefCell::new(fields.into_iter().map(|(k, v)| (k.into(), v)).collect()),
        native: RefCell::new(None),
    }))
}
fn text(s: &str) -> V {
    V::Text(s.into())
}
fn same(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Obj(a), V::Obj(b)) => Rc::ptr_eq(a, b),
        (V::Text(a), V::Text(b)) => a == b,
        (V::Big(a), V::Big(b)) => a == b,
        (V::Num(a), V::Num(b)) => a.to_bits() == b.to_bits(),
        (V::U, V::U) | (V::Null, V::Null) => true,
        _ => false,
    }
}
fn truthy(v: &V) -> bool {
    match v {
        V::U | V::Null => false,
        V::Num(n) => *n != 0.0 && !n.is_nan(),
        V::Big(n) => *n != BigInt::from(0),
        V::Text(s) => !s.is_empty(),
        _ => true,
    }
}
fn generated(name: &'static str, message: &str) -> E {
    TransactionBuildError {
        name,
        message: message.encode_utf16().collect(),
    }
    .into()
}
fn message_of(e: &E) -> (String, String) {
    match e {
        E::Generated(e) => (e.name.into(), String::from_utf16(&e.message).unwrap()),
        E::Thrown(_) => ("Thrown".into(), String::new()),
    }
}

#[derive(Default)]
struct Engine {
    queue: RefCell<VecDeque<Box<dyn FnOnce()>>>,
    now: Cell<f64>,
}
impl TransactionRuntime for Engine {
    fn enqueue_microtask(&self, task: Box<dyn FnOnce()>) {
        self.queue.borrow_mut().push_back(task);
    }
    fn now_millis(&self) -> f64 {
        self.now.get()
    }
    fn auth_token_encoding(&self) -> AuthTokenEncoding {
        panic!("imported TransactionClient capability")
    }
}

#[derive(Default)]
struct State {
    engine: Rc<Engine>,
    events: Vec<String>,
    client_falsy: bool,
    auth_error: Option<HostThrownValue>,
    realm: Option<Rc<Realm>>,
}
fn log(h: &H, event: String) {
    h.borrow_mut().events.push(event);
}
fn settled(h: &H, result: Result<V, E>) -> NativePromise<V> {
    let engine = h.borrow().engine.clone();
    NativePromise::settled(engine, result)
}
const METHODS: [SignerAdapterMethod; 9] = [
    SignerAdapterMethod::RunWithSubmissionLock,
    SignerAdapterMethod::Initialize,
    SignerAdapterMethod::CreateOrder,
    SignerAdapterMethod::CreateOrders,
    SignerAdapterMethod::CreateGroupedOrders,
    SignerAdapterMethod::CancelOrder,
    SignerAdapterMethod::UpdateLeverage,
    SignerAdapterMethod::ApproveIntegrator,
    SignerAdapterMethod::GetAuthToken,
];
fn call_async(h: &H, callee: V, args: Vec<V>) -> Result<NativePromise<V>, E> {
    if let V::Callback = callee {
        assert!(args.is_empty(), "lock callback is called without arguments");
        log(h, "callback".into());
        return Ok(settled(h, Ok(text("callback-result"))));
    }
    let V::Method(name, receiver) = callee else {
        panic!("callee {callee:?}")
    };
    let V::Obj(target) = receiver.as_ref() else {
        panic!("receiver")
    };
    log(h, format!("call {name} {args:?}"));
    if target.tag == "adapter" {
        let native = target.native.borrow().clone().expect("bound native");
        let realm = h.borrow().realm.clone().expect("realm");
        let method = METHODS
            .into_iter()
            .find(|m| m.metadata().name == name)
            .unwrap();
        let argument = args.into_iter().next().unwrap_or(V::U);
        return Ok(method.invoke(realm.as_ref(), &native, argument));
    }
    let expected = if name == "sendRawTx" {
        "http"
    } else {
        "client"
    };
    assert_eq!(target.tag, expected, "native receiver for {name}");
    match name {
        "getAuthToken" => {
            let thrown = h.borrow().auth_error.clone();
            if let Some(thrown) = thrown {
                return Ok(settled(h, Err(thrown.into())));
            }
            Ok(settled(h, Ok(text(&format!("token:{:?}", args[0])))))
        }
        "getCreateOrderTransaction" | "getCancelOrderTransaction" => {
            Ok(settled(h, Ok(obj("tx", vec![]))))
        }
        "sendRawTx" => Ok(settled(h, Ok(text("hash")))),
        _ => panic!("call {name}"),
    }
}
fn get(h: &H, value: &V, key: &str) -> Result<V, E> {
    let V::Obj(o) = value else {
        return Err(generated("TypeError", &format!("read {key} of {value:?}")));
    };
    log(h, format!("get {key}"));
    Ok(o.fields
        .borrow()
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
        .unwrap_or(V::U))
}
fn runtime(h: &H) -> Rc<LighterSignerRuntime<H, V>> {
    Rc::new(LighterSignerRuntime {
        promise_runtime: h.borrow().engine.clone(),
        host: h.clone(),
        get,
        method: |h, receiver, name| {
            log(h, format!("method {name}"));
            Ok(V::Method(name, Box::new(receiver)))
        },
        call: |_, _, _| panic!("resetNonce is not reached"),
        call_async,
        construct_transaction_client: |h, _| {
            log(h, "TransactionClient".into());
            Ok(if h.borrow().client_falsy {
                V::U
            } else {
                obj("client", vec![])
            })
        },
        object: |fields| obj("object", fields),
        array: |_| obj("array", vec![]),
        undefined: || V::U,
        text,
        truthy,
        nullish: |v| matches!(v, V::U | V::Null),
        strict_zero: |v| matches!(v, V::Num(n) if *n == 0.0),
        wasm_ready: |h| {
            log(h, "ready".into());
            Ok(true)
        },
        init_wasm: |_| panic!("wasm is ready"),
        crypto_api: |h| {
            log(h, "crypto".into());
            Ok(obj("crypto", vec![]))
        },
        deployment_profile: |h, _| {
            log(h, "profile".into());
            Ok(obj("profile", vec![("signerChainId", V::Num(300.0))]))
        },
        error_value: |_| panic!("no initialization error"),
        lowercase_string: |_, _| panic!("no initialization error"),
        invalid_nonce: |_, _| Ok(false),
        delay: |_, _| panic!("no retry"),
        new_error: |_, message| generated("Error", message),
        number: V::Num,
        add: |h, left, right| {
            log(h, format!("add {left:?} {right:?}"));
            match (&left, &right) {
                (V::Num(a), V::Num(b)) => Ok(V::Num(a + b)),
                (V::Num(a), V::Text(b)) => Ok(V::Text(format!("{a}{b}"))),
                _ => Err(generated("TypeError", "add")),
            }
        },
        big_int: |h, value| {
            log(h, format!("BigInt {value:?}"));
            match value {
                V::Num(n) if n.fract() == 0.0 => Ok(V::Big(BigInt::from(n as i64))),
                V::Num(_) => Err(generated("RangeError", "not an integer")),
                V::Text(s) => s
                    .parse()
                    .map(V::Big)
                    .map_err(|_| generated("SyntaxError", "bigint text")),
                _ => Err(generated("TypeError", "bigint")),
            }
        },
    })
}
fn drain(h: &H) {
    let engine = h.borrow().engine.clone();
    for _ in 0..100 {
        let task = engine.queue.borrow_mut().pop_front();
        match task {
            Some(task) => task(),
            None => return,
        }
    }
    panic!("microtask floor exhausted");
}
fn observe(p: &NativePromise<V>) -> Rc<RefCell<Option<PromiseResult<V>>>> {
    let slot = Rc::new(RefCell::new(None));
    let captured = slot.clone();
    p.subscribe(move |v| *captured.borrow_mut() = Some(v));
    slot
}
fn outcome(h: &H, p: &NativePromise<V>) -> PromiseResult<V> {
    let slot = observe(p);
    assert!(slot.borrow().is_none(), "async body settles asynchronously");
    drain(h);
    let result = slot.borrow_mut().take();
    result.expect("settled")
}
fn host() -> H {
    let h: H = Default::default();
    h.borrow().engine.now.set(1_700_000_123_456.0);
    h
}
fn events(h: &H) -> Vec<String> {
    std::mem::take(&mut h.borrow_mut().events)
}
fn options(secret: &str, extra: Option<V>) -> V {
    let mut fields = vec![("secret", text(secret)), ("testnet", V::U)];
    if let Some(extra) = extra {
        fields.push(("extra", extra));
    }
    obj("options", fields)
}
fn native(h: &H, secret: &str) -> Rc<Adapter> {
    Adapter::new(
        runtime(h),
        options(secret, None),
        text("lighter"),
        obj("http", vec![]),
        V::Big(BigInt::from(42)),
        V::Num(3.0),
    )
}
const INIT: [&str; 8] = [
    "get secret",
    "ready",
    "crypto",
    "get testnet",
    "profile",
    "get signerChainId",
    "get secret",
    "TransactionClient",
];
fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

struct Realm {
    h: H,
    runtime: Rc<LighterSignerRuntime<H, V>>,
    body: Cell<Option<SignerAdapterConstructor<H, V>>>,
    class: RefCell<Option<V>>,
}
impl SignerAdapterRealm<H, V> for Realm {
    fn runtime(&self) -> Rc<LighterSignerRuntime<H, V>> {
        self.runtime.clone()
    }
    fn create_class(
        &self,
        name: &'static str,
        length: usize,
        body: SignerAdapterConstructor<H, V>,
    ) -> Result<V, E> {
        log(&self.h, format!("class {name} {length}"));
        self.body.set(Some(body));
        Ok(obj("class", vec![]))
    }
    fn define_prototype_method(&self, _: &V, method: SignerAdapterMethod) -> Result<(), E> {
        let m = method.metadata();
        log(
            &self.h,
            format!("proto {} {} {}", m.name, m.length, m.asynchronous),
        );
        Ok(())
    }
    fn bind_native(&self, receiver: &V, adapter: Rc<Adapter>) -> Result<(), E> {
        log(&self.h, "bind".into());
        let V::Obj(o) = receiver else { panic!() };
        *o.native.borrow_mut() = Some(adapter);
        Ok(())
    }
    fn define_field_fn(
        &self,
        receiver: &V,
        method: SignerAdapterMethod,
        adapter: Rc<Adapter>,
    ) -> Result<(), E> {
        let V::Obj(o) = receiver else { panic!() };
        assert!(Rc::ptr_eq(o.native.borrow().as_ref().unwrap(), &adapter));
        log(&self.h, format!("field {}", method.metadata().name));
        Ok(())
    }
    fn finish_class_binding(&self, class: &V) -> Result<(), E> {
        log(&self.h, "finish".into());
        *self.class.borrow_mut() = Some(class.clone());
        Ok(())
    }
    fn class_binding(&self) -> Result<V, E> {
        log(&self.h, "binding".into());
        Ok(self.class.borrow().clone().expect("class defined"))
    }
    fn construct(&self, class: V, args: Vec<V>) -> Result<V, E> {
        assert!(same(&class, self.class.borrow().as_ref().unwrap()));
        log(&self.h, format!("new {}", args.len()));
        let body = self.body.get().unwrap();
        body(self, obj("adapter", vec![]), &args)
    }
    fn strict_equals_text(&self, value: &V, text: &str) -> bool {
        log(&self.h, format!("strict {value:?}"));
        matches!(value, V::Text(s) if s == text)
    }
    fn template_string(&self, value: V) -> Result<Vec<u16>, E> {
        log(&self.h, format!("template {value:?}"));
        match value {
            V::Text(s) => Ok(s.encode_utf16().collect()),
            V::Num(n) => Ok(n.to_string().encode_utf16().collect()),
            _ => Err(generated(
                "TypeError",
                "Cannot convert a Symbol value to a string",
            )),
        }
    }
    fn new_error(&self, message: Vec<u16>) -> E {
        TransactionBuildError {
            name: "Error",
            message,
        }
        .into()
    }
}
fn realm(h: &H) -> Rc<Realm> {
    let realm = Rc::new(Realm {
        h: h.clone(),
        runtime: runtime(h),
        body: Cell::new(None),
        class: RefCell::new(None),
    });
    h.borrow_mut().realm = Some(realm.clone());
    Adapter::define(realm.as_ref()).unwrap();
    events(h);
    realm
}

#[test]
fn auth_token_with_existing_client() {
    let h = host();
    let a = native(&h, "secret");
    a.transaction_client.replace(obj("client", vec![]));
    let p = a.get_auth_token(V::Num(600.0));
    assert_eq!(
        events(&h),
        strs(&[
            "add 1700000123 600",
            "BigInt 1700000723",
            "method getAuthToken",
            "call getAuthToken [1700000723n]",
        ]),
        "auth::deadline then client call, synchronously"
    );
    let token = outcome(&h, &p).unwrap();
    assert!(same(&token, &text("token:1700000723n")), "auth::token");
    assert!(events(&h).is_empty());
}

#[test]
fn auth_token_initializes_first() {
    let h = host();
    let a = native(&h, "secret");
    let p = a.get_auth_token(V::Num(60.0));
    assert_eq!(events(&h), strs(&INIT), "auth::initialize before deadline");
    let token = outcome(&h, &p).unwrap();
    assert_eq!(
        events(&h),
        strs(&[
            "add 1700000123 60",
            "BigInt 1700000183",
            "method getAuthToken",
            "call getAuthToken [1700000183n]",
        ]),
        "auth::deadline after initialize continuation"
    );
    assert!(same(&token, &text("token:1700000183n")));
}

#[test]
fn auth_token_missing_secret_rejects() {
    let h = host();
    let a = native(&h, "");
    let error = outcome(&h, &a.get_auth_token(V::Num(60.0))).unwrap_err();
    assert_eq!(
        message_of(&error),
        (
            "Error".into(),
            "Missing secret for signer initialization".into()
        )
    );
    assert_eq!(events(&h), strs(&["get secret"]), "auth::no deadline work");
}

#[test]
fn auth_token_uninitialized_client_rejects() {
    let h = host();
    h.borrow_mut().client_falsy = true;
    let a = native(&h, "secret");
    let error = outcome(&h, &a.get_auth_token(V::Num(60.0))).unwrap_err();
    assert_eq!(
        message_of(&error),
        ("Error".into(), "Signer not initialized".into())
    );
    assert_eq!(events(&h), strs(&INIT), "auth::guard precedes deadline");
}

#[test]
fn auth_token_raw_ttl_operand() {
    let h = host();
    let a = native(&h, "secret");
    a.transaction_client.replace(obj("client", vec![]));
    let token = outcome(&h, &a.get_auth_token(text("60"))).unwrap();
    assert_eq!(
        events(&h),
        strs(&[
            "add 1700000123 \"60\"",
            "BigInt \"170000012360\"",
            "method getAuthToken",
            "call getAuthToken [170000012360n]",
        ]),
        "auth::JavaScript + concatenates a string ttl"
    );
    assert!(same(&token, &text("token:170000012360n")));
}

#[test]
fn auth_token_big_int_throw_rejects() {
    let h = host();
    let a = native(&h, "secret");
    a.transaction_client.replace(obj("client", vec![]));
    let error = outcome(&h, &a.get_auth_token(V::Num(1.5))).unwrap_err();
    assert_eq!(message_of(&error).0, "RangeError");
    assert_eq!(
        events(&h),
        strs(&["add 1700000123 1.5", "BigInt 1700000124.5"]),
        "auth::no client call after BigInt throw"
    );
}

#[test]
fn auth_token_rejection_identity() {
    let h = host();
    let thrown = HostThrownValue::new("auth failure");
    h.borrow_mut().auth_error = Some(thrown.clone());
    let a = native(&h, "secret");
    a.transaction_client.replace(obj("client", vec![]));
    let error = outcome(&h, &a.get_auth_token(V::Num(1.0))).unwrap_err();
    let E::Thrown(error) = error else {
        panic!("auth::original rejection")
    };
    assert!(error.ptr_eq(&thrown), "auth::rejection identity");
}

fn params(extra: Option<V>, secret: &str) -> (V, V) {
    let options = options(secret, extra);
    let params = obj(
        "params",
        vec![
            ("exchangeName", text("lighter")),
            ("options", options.clone()),
            ("apiClient", obj("http", vec![])),
            ("accountIndex", V::Big(BigInt::from(9))),
            ("apiKeyIndex", V::Num(4.0)),
        ],
    );
    (params, options)
}
const CONSTRUCTION: [&str; 11] = [
    "binding",
    "get options",
    "get exchangeName",
    "get apiClient",
    "get accountIndex",
    "get apiKeyIndex",
    "new 5",
    "bind",
    "field runWithSubmissionLock",
    "method initialize",
    "call initialize []",
];
fn factory_ok(extra: Option<V>, provider_events: &[&str]) {
    let h = host();
    let realm = realm(&h);
    let (params, options) = params(extra, "secret");
    let p = create_lighter_signer_adapter(realm.as_ref(), params);
    let mut expected = strs(&["get options", "get extra"]);
    expected.extend(strs(provider_events));
    expected.extend(strs(&CONSTRUCTION));
    expected.extend(strs(&INIT));
    assert_eq!(events(&h), expected, "factory::evaluation order");
    let adapter = outcome(&h, &p).unwrap();
    let V::Obj(o) = adapter.as_ref() else {
        panic!("factory::adapter object")
    };
    assert_eq!(o.tag, "adapter");
    let native = o.native.borrow().clone().unwrap();
    assert!(same(&native.options, &options), "factory::options identity");
    assert!(same(&native.exchange_name, &text("lighter")));
    assert!(same(&native.account_index, &V::Big(BigInt::from(9))));
    assert!(same(&native.api_key_index, &V::Num(4.0)));
    assert!(
        truthy(&native.transaction_client.borrow()),
        "factory::initialized"
    );
}

#[test]
fn factory_default_provider() {
    factory_ok(None, &[]);
}

#[test]
fn factory_null_extra() {
    factory_ok(Some(V::Null), &[]);
}

#[test]
fn factory_explicit_sdk_provider() {
    factory_ok(
        Some(obj(
            "extra",
            vec![("lighterSignerProvider", text("tealstreet-sdk"))],
        )),
        &["get lighterSignerProvider", "strict \"tealstreet-sdk\""],
    );
}

#[test]
fn factory_falsy_provider() {
    factory_ok(
        Some(obj("extra", vec![("lighterSignerProvider", text(""))])),
        &["get lighterSignerProvider"],
    );
}

#[test]
fn factory_unsupported_provider_rejects() {
    let h = host();
    let realm = realm(&h);
    let extra = obj("extra", vec![("lighterSignerProvider", V::Num(7.0))]);
    let p = create_lighter_signer_adapter(realm.as_ref(), params(Some(extra), "secret").0);
    let error = outcome(&h, &p).unwrap_err();
    assert_eq!(
        message_of(&error),
        (
            "Error".into(),
            "Unsupported Lighter signer provider \"7\". Legacy third-party signer providers are removed; use tealstreet-sdk."
                .into()
        )
    );
    assert_eq!(
        events(&h),
        strs(&[
            "get options",
            "get extra",
            "get lighterSignerProvider",
            "strict 7",
            "template 7",
        ]),
        "factory::no construction"
    );
}

#[test]
fn factory_template_throw_rejects() {
    let h = host();
    let realm = realm(&h);
    let extra = obj(
        "extra",
        vec![("lighterSignerProvider", obj("symbol", vec![]))],
    );
    let p = create_lighter_signer_adapter(realm.as_ref(), params(Some(extra), "secret").0);
    let error = outcome(&h, &p).unwrap_err();
    assert_eq!(message_of(&error).0, "TypeError");
}

#[test]
fn factory_initialize_rejects() {
    let h = host();
    let realm = realm(&h);
    let p = create_lighter_signer_adapter(realm.as_ref(), params(None, "").0);
    let mut expected = strs(&["get options", "get extra"]);
    expected.extend(strs(&CONSTRUCTION));
    expected.push("get secret".into());
    assert_eq!(events(&h), expected);
    let error = outcome(&h, &p).unwrap_err();
    assert_eq!(
        message_of(&error),
        (
            "Error".into(),
            "Missing secret for signer initialization".into()
        )
    );
}

#[test]
fn class_definition() {
    let h = host();
    let realm = Rc::new(Realm {
        h: h.clone(),
        runtime: runtime(&h),
        body: Cell::new(None),
        class: RefCell::new(None),
    });
    let class = Adapter::define(realm.as_ref()).unwrap();
    assert!(same(&class, realm.class.borrow().as_ref().unwrap()));
    assert_eq!(
        events(&h),
        strs(&[
            "class TealstreetLighterSignerAdapter 5",
            "proto initialize 0 true",
            "proto createOrder 1 true",
            "proto createOrders 1 true",
            "proto createGroupedOrders 1 true",
            "proto cancelOrder 1 true",
            "proto updateLeverage 1 true",
            "proto approveIntegrator 1 true",
            "proto getAuthToken 1 true",
            "finish",
        ]),
        "class::source definition order"
    );
    let receiver = obj("adapter", vec![]);
    let options = options("secret", None);
    let built = Adapter::construct(realm.as_ref(), receiver.clone(), &[options.clone()]).unwrap();
    assert!(
        same(&built, &receiver),
        "class::constructor returns receiver"
    );
    let V::Obj(o) = &receiver else { panic!() };
    let native = o.native.borrow().clone().unwrap();
    assert!(same(&native.options, &options));
    assert!(
        same(&native.exchange_name, &V::U),
        "class::missing args undefined"
    );
    assert!(same(&native.api_key_index, &V::U));
    assert!(same(&native.transaction_client.borrow(), &V::U));
}

#[test]
fn class_method_dispatch() {
    let h = host();
    let realm = realm(&h);
    let receiver = realm.construct(
        realm.class_binding().unwrap(),
        vec![
            options("secret", None),
            text("lighter"),
            obj("http", vec![]),
        ],
    );
    let receiver = receiver.unwrap();
    let V::Obj(o) = &receiver else { panic!() };
    o.native
        .borrow()
        .as_ref()
        .unwrap()
        .transaction_client
        .replace(obj("client", vec![]));
    events(&h);
    let call = |name: &'static str, arg: V| {
        call_async(&h, V::Method(name, Box::new(receiver.clone())), vec![arg]).unwrap()
    };
    let token = outcome(&h, &call("getAuthToken", V::Num(10.0))).unwrap();
    assert!(
        same(&token, &text("token:1700000133n")),
        "class::getAuthToken body"
    );
    let hash = outcome(&h, &call("createOrder", obj("request", vec![]))).unwrap();
    assert!(same(&hash, &text("hash")), "class::createOrder body");
    let locked = outcome(&h, &call("runWithSubmissionLock", V::Callback)).unwrap();
    assert!(
        same(&locked, &text("callback-result")),
        "class::lock field-fn"
    );
    let trace = events(&h);
    assert!(trace.contains(&"call getCreateOrderTransaction [<request>]".into()));
    assert!(trace.contains(&"callback".into()));
}

#[test]
fn interface_dispatch() {
    let h = host();
    let a = native(&h, "secret");
    let dynamic: &dyn LighterSignerAdapter<V> = &a;
    assert!(dynamic.dispose().is_none(), "interface::dispose absent");
    outcome(&h, &dynamic.initialize()).unwrap();
    assert_eq!(events(&h), strs(&INIT), "interface::initialize body");
    let token = outcome(&h, &dynamic.get_auth_token(V::Num(1.0))).unwrap();
    assert!(
        same(&token, &text("token:1700000124n")),
        "interface::getAuthToken body"
    );
    events(&h);
    let hash = outcome(&h, &dynamic.cancel_order(obj("request", vec![]))).unwrap();
    assert!(same(&hash, &text("hash")));
    assert_eq!(
        events(&h)[..2],
        strs(&[
            "method getCancelOrderTransaction",
            "call getCancelOrderTransaction [<request>]"
        ]),
        "interface::cancelOrder body"
    );
}
