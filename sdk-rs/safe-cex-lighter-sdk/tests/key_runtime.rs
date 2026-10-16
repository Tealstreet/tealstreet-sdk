use num_bigint::BigInt;
use safe_cex_lighter_sdk::{key_runtime::*, signing_types::*};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
fn corpus() -> &'static Value {
    static C: OnceLock<Value> = OnceLock::new();
    C.get_or_init(|| {
        let o = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/key_runtime_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "SDK-KEY maintained source {}",
            String::from_utf8_lossy(&o.stderr)
        );
        serde_json::from_slice(&o.stdout).unwrap()
    })
}
fn ready<F: Future>(h: &Host, f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    for _ in 0..100 {
        if let Poll::Ready(r) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            return r;
        }
        h.pump();
    }
    panic!("fixture did not settle")
}
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
#[derive(Clone)]
enum V {
    Undefined,
    Null,
    Bool(bool),
    Text(Vec<u16>),
    Number(f64),
    Object(Rc<Object>),
    Function,
}
struct Object {
    fields: Vec<(String, V)>,
    inherited_error: bool,
}
struct E {
    name: String,
    message: Vec<u16>,
    cause: Option<Rc<E>>,
}
struct Host {
    input: Value,
    ready: Cell<bool>,
    calls: RefCell<Vec<Value>>,
    failure: Rc<E>,
    api: Rc<()>,
    result: V,
    tasks: RefCell<Vec<std::pin::Pin<Box<dyn Future<Output = ()>>>>>,
}
fn object(fields: Vec<(&str, V)>) -> V {
    V::Object(Rc::new(Object {
        fields: fields.into_iter().map(|(k, v)| (k.into(), v)).collect(),
        inherited_error: false,
    }))
}
fn value(v: &V) -> Value {
    match v {
        V::Bool(b) => json!(b),
        V::Undefined => json!({"undefined":true}),
        V::Null => Value::Null,
        V::Text(s) => json!({"text":s}),
        V::Number(n) => json!({"bits":format!("{:016x}",n.to_bits())}),
        V::Object(o) => Value::Object(
            o.fields
                .iter()
                .map(|(k, v)| (k.clone(), value(v)))
                .collect(),
        ),
        V::Function => json!({"function":true}),
    }
}
fn same(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Undefined, V::Undefined) | (V::Null, V::Null) => true,
        (V::Text(a), V::Text(b)) => a == b,
        (V::Object(a), V::Object(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
impl Host {
    fn pump(&self) {
        let tasks = self.tasks.take();
        for mut task in tasks {
            if task
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
            {
                self.tasks.borrow_mut().push(task);
            }
        }
    }
    fn call(&self, s: impl Into<Value>) {
        self.calls.borrow_mut().push(s.into())
    }
}
impl KeyHost for Host {
    type Value = V;
    type Error = Rc<E>;
    type Api = Rc<()>;
    fn spawn(&self, mut future: std::pin::Pin<Box<dyn Future<Output = ()>>>) {
        if future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
        {
            self.tasks.borrow_mut().push(future);
        }
    }
    fn promise_resolve(&self, value: V) -> KeyFuture<V, Rc<E>> {
        if matches!(&value,V::Object(o) if o.fields.iter().any(|(k,_)|k=="then")) {
            self.call(json!("then"));
            return Box::pin(async { Ok(V::Text(units("settled-derived"))) });
        }
        Box::pin(async move { Ok(value) })
    }
    fn is_ready(&self) -> Result<bool, Rc<E>> {
        self.call(json!("ready"));
        Ok(self.ready.get())
    }
    fn init_wasm(&self) -> Result<KeyFuture<(), Rc<E>>, Rc<E>> {
        self.call(json!("init"));
        if self.input["initSyncError"] == true {
            return Err(self.failure.clone());
        }
        let failed = self.input["initError"] == true;
        if !failed {
            self.ready.set(self.input["notReady"] != true)
        }
        let error = self.failure.clone();
        Ok(Box::pin(
            async move { if failed { Err(error) } else { Ok(()) } },
        ))
    }
    fn get_crypto_api(&self) -> Result<Rc<()>, Rc<E>> {
        self.call(json!("api"));
        if self.input["apiError"] == true {
            Err(self.failure.clone())
        } else {
            Ok(self.api.clone())
        }
    }
    fn generate_keys(&self, api: &Rc<()>) -> Result<V, Rc<E>> {
        assert!(
            Rc::ptr_eq(api, &self.api),
            "SDK-KEY generation bound receiver"
        );
        self.call(json!("generate"));
        if self.input["operationError"] == true {
            Err(self.failure.clone())
        } else {
            Ok(self.result.clone())
        }
    }
    fn derive_public_key(&self, api: &Rc<()>, key: &[u16]) -> Result<V, Rc<E>> {
        assert!(
            Rc::ptr_eq(api, &self.api),
            "SDK-KEY derivation bound receiver"
        );
        self.call(json!(["derive", key]));
        if self.input["operationError"] == true {
            Err(self.failure.clone())
        } else {
            Ok(self.result.clone())
        }
    }
    fn is_object(&self, v: &V) -> bool {
        matches!(v, V::Object(_) | V::Null)
    }
    fn is_null(&self, v: &V) -> bool {
        matches!(v, V::Null)
    }
    fn has_error(&self, v: &V) -> Result<bool, Rc<E>> {
        Ok(
            matches!(v,V::Object(o) if o.inherited_error||o.fields.iter().any(|(key,_)|key=="error")),
        )
    }
    fn property(&self, v: &V, key: &str) -> Result<V, Rc<E>> {
        match v {
            V::Object(o) => Ok(o
                .fields
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
                .unwrap_or(V::Undefined)),
            V::Null => Err(Rc::new(E {
                name: "TypeError".into(),
                message: units(&format!("Cannot read properties of null (reading '{key}')")),
                cause: None,
            })),
            _ => Ok(V::Undefined),
        }
    }
    fn string(&self, v: &V) -> Result<Vec<u16>, Rc<E>> {
        Ok(match v {
            V::Bool(b) => units(if *b { "true" } else { "false" }),
            V::Undefined => units("undefined"),
            V::Null => units("null"),
            V::Text(s) => s.clone(),
            V::Number(n) => units(&n.to_string()),
            _ => units("[object Object]"),
        })
    }
    fn key_error(&self, message: Vec<u16>, cause: Option<Rc<E>>) -> Rc<E> {
        Rc::new(E {
            name: "KeyManagerError".into(),
            message,
            cause,
        })
    }
}
fn fixture(i: &Value) -> Rc<Host> {
    let method = i["method"].as_str().unwrap();
    let result = match i["resultMode"].as_str().unwrap() {
        "error" => {
            let mut message = units("crypto ");
            message.push(0xd800);
            message.extend(units(" failure"));
            object(vec![("error", V::Text(message))])
        }
        "inherited-error" => V::Object(Rc::new(Object {
            fields: vec![],
            inherited_error: true,
        })),
        "thenable" => object(vec![("then", V::Function)]),
        "identity" => object(vec![("opaque", V::Bool(true))]),
        "null" => V::Null,
        _ => {
            if method == "derive" {
                V::Text(units("derived-public"))
            } else {
                object(vec![
                    ("privateKey", V::Text(units("private-raw"))),
                    ("publicKey", V::Text(units("public-raw"))),
                ])
            }
        }
    };
    Rc::new(Host {
        input: i.clone(),
        ready: Cell::new(i["ready"].as_bool().unwrap()),
        calls: RefCell::default(),
        failure: Rc::new(E {
            name: "Error".into(),
            message: units("original cause"),
            cause: None,
        }),
        api: Rc::new(()),
        result,
        tasks: RefCell::default(),
    })
}
fn key(i: &Value) -> Vec<u16> {
    raw_units(&i["privateKey"])
}

fn run(case: &Value) {
    let i = &case["input"];
    let h = fixture(i);
    let method = i["method"].as_str().unwrap();
    let result=match method{
 "ensure"=>ready(h.as_ref(),ensure_wasm_initialized(h.clone())).map(|_|json!({"ok":{"undefined":true},"same":false})),
 "generate"=>ready(h.as_ref(),generate_key_pair(h.clone())).map(|v|json!({"ok":{"privateKey":value(&v.private_key),"publicKey":value(&v.public_key)},"same":false})),
 "derive"=>ready(h.as_ref(),derive_public_key(h.clone(),key(i))).map(|v|{if i["resultMode"]=="identity"{assert!(same(&v,&h.result),"SDK-KEY derived native object identity");}json!({"ok":value(&v),"same":same(&v,&h.result)})}),
 _=>{let v=match i["probe"].as_str().unwrap(){"null"=>V::Null,"function"=>V::Function,"inherited"=>V::Object(Rc::new(Object{fields:vec![],inherited_error:true})),_=>object(vec![("error",V::Number(0.0))])};let r=if method=="predicate-key"{key_is_wasm_error(h.as_ref(),&v)}else{signer_is_wasm_error(h.as_ref(),&v)};r.map(|b|json!({"ok":b}))}};
    let output = match result {
        Ok(v) => v,
        Err(e) => {
            json!({"error":{"name":e.name,"message":e.message,"original":Rc::ptr_eq(&e,&h.failure),"causeSame":e.cause.as_ref().is_some_and(|c|Rc::ptr_eq(c,&h.failure))}})
        }
    };
    assert_eq!(
        output, case["output"],
        "SDK-KEY native output {method} {}",
        i["variant"]
    );
    assert_eq!(
        json!(h.calls.borrow().clone()),
        case["calls"],
        "SDK-KEY call order {method} {}",
        i["variant"]
    );
}
#[test]
fn key_runtime_matches_maintained_typescript() {
    let c = corpus();
    assert_eq!(
        c["cases"].as_array().unwrap().len(),
        20,
        "SDK-KEY independent runtime floor"
    );
    assert_eq!(
        c["extras"].as_array().unwrap().len(),
        7,
        "SDK-KEY independent extra floor"
    );
    for case in c["cases"]
        .as_array()
        .unwrap()
        .iter()
        .chain(c["extras"].as_array().unwrap())
    {
        run(case)
    }
}
#[test]
fn imports_run_eagerly_and_errors_keep_identity() {
    let i = &corpus()["cases"][5]["input"];
    let h = fixture(i);
    let pending = generate_key_pair(h.clone());
    assert_eq!(
        json!(h.calls.borrow().clone()),
        corpus()["eager"]["before"],
        "SDK-KEY pre-await eager imports"
    );
    drop(pending);
    let mut input = i.clone();
    input["initError"] = json!(true);
    let h = fixture(&input);
    let error = ready(h.as_ref(), ensure_wasm_initialized(h.clone()))
        .err()
        .unwrap();
    assert!(
        Rc::ptr_eq(error.cause.as_ref().unwrap(), &h.failure),
        "SDK-KEY original init cause pointer"
    );
}
fn raw_units(v: &Value) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_u64().unwrap() as u16)
        .collect()
}
fn number(v: &Value) -> f64 {
    let s = v["bits"].as_str().unwrap();
    if s == "NaN" {
        f64::NAN
    } else {
        f64::from_bits(u64::from_str_radix(s, 16).unwrap())
    }
}
#[test]
fn signer_records_have_direct_native_fields() {
    let c = corpus();
    assert_eq!(
        c["records"].as_array().unwrap().len(),
        4,
        "SDK-KEY independent record floor"
    );
    for record in c["records"].as_array().unwrap() {
        let tx = &record["tx"];
        let native = TransactionSignature::new(
            raw_units(&tx["txHash"]["text"]),
            raw_units(&tx["signature"]["text"]),
        );
        assert_eq!(
            native.tx_hash,
            raw_units(&tx["txHash"]["text"]),
            "SDK-KEY native signature hash"
        );
        assert_eq!(
            native.signature,
            raw_units(&tx["signature"]["text"]),
            "SDK-KEY native signature field"
        );
        let a = &record["auth"];
        let native = AuthToken::new(
            raw_units(&a["token"]["text"]),
            raw_units(&a["message"]["text"]),
            raw_units(&a["messageHash"]["text"]),
            number(&a["accountIndex"]),
            number(&a["apiKeyIndex"]),
            a["deadline"]["bigint"]
                .as_str()
                .unwrap()
                .parse::<BigInt>()
                .unwrap(),
        );
        assert_eq!(
            native.token,
            raw_units(&a["token"]["text"]),
            "SDK-KEY native token"
        );
        assert_eq!(
            native.message,
            raw_units(&a["message"]["text"]),
            "SDK-KEY native auth message"
        );
        assert_eq!(
            native.message_hash,
            raw_units(&a["messageHash"]["text"]),
            "SDK-KEY native auth hash"
        );
        for (field, n) in [
            ("accountIndex", native.account_index),
            ("apiKeyIndex", native.api_key_index),
        ] {
            let expected = number(&a[field]);
            assert!(
                if expected.is_nan() {
                    n.is_nan()
                } else {
                    n.to_bits() == expected.to_bits()
                },
                "SDK-KEY native auth {field}"
            );
        }
        assert_eq!(
            native.deadline,
            a["deadline"]["bigint"]
                .as_str()
                .unwrap()
                .parse::<BigInt>()
                .unwrap(),
            "SDK-KEY native unbounded deadline"
        );
    }
}
#[test]
fn signer_error_preserves_fields_and_cause_with_explicit_class_boundary() {
    let c = corpus();
    assert_eq!(
        c["errors"].as_array().unwrap().len(),
        4,
        "SDK-KEY independent constructor floor"
    );
    for case in c["errors"].as_array().unwrap() {
        let token = Rc::new(RefCell::new(7));
        let cause = match case["causeMode"].as_str().unwrap() {
            "undefined" => None,
            "null" => Some(None),
            _ => Some(Some(token.clone())),
        };
        let native = SignerError::new(raw_units(&case["message"]), cause);
        assert_eq!(
            native.name,
            case["name"].as_str().unwrap(),
            "SDK-KEY native signer error name"
        );
        assert_eq!(
            native.message,
            raw_units(&case["messageOut"]),
            "SDK-KEY native signer error message"
        );
        if case["causeMode"] == "object" {
            assert!(
                Rc::ptr_eq(native.cause.as_ref().unwrap().as_ref().unwrap(), &token),
                "SDK-KEY native signer original cause"
            );
            *token.borrow_mut() = 8;
            assert_eq!(
                *native.cause.unwrap().unwrap().borrow(),
                8,
                "SDK-KEY native signer live cause"
            );
        }
        assert_eq!(
            case["isError"], true,
            "SDK-KEY source Error prototype witness"
        );
        assert_eq!(
            case["messageEnumerable"], false,
            "SDK-KEY source Error descriptor witness"
        );
    }
}

#[test]
fn dropped_return_future_keeps_the_owned_sdk_body_running() {
    let mut input = corpus()["cases"][5]["input"].clone();
    input["resultMode"] = json!("normal");
    let h = fixture(&input);
    drop(generate_key_pair(h.clone()));
    for _ in 0..8 {
        h.pump()
    }
    assert_eq!(
        json!(h.calls.borrow().clone()),
        corpus()["droppedCalls"],
        "SDK-KEY dropped promise still dispatches"
    );
    assert!(h.tasks.borrow().is_empty(), "SDK-KEY owned work settled");
}
