use num_bigint::BigInt;
use safe_cex_venue_lighter::{
    deployment, signer_helpers, signer_runtime as s,
    transaction_builders::TransactionBuildError,
    transaction_promise::{NativePromise, PromiseResolver, PromiseResult, TransactionRuntime},
    transaction_signing::{AuthTokenEncoding, HostThrownValue, TransactionSigningError as E},
};
use serde_json::Value as J;
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    process::Command,
    rc::Rc,
};
#[derive(Clone, Debug)]
enum V {
    U,
    Null,
    Bool(bool),
    Num(f64),
    Big(BigInt),
    Text(Vec<u16>),
    Obj(Rc<Obj>),
    Array(Rc<RefCell<Vec<Option<V>>>>),
    Method(&'static str, Box<V>),
}
#[derive(Debug)]
struct Obj {
    fields: RefCell<Vec<(String, V)>>,
}
impl PartialEq for V {
    fn eq(&self, b: &Self) -> bool {
        match (self, b) {
            (Self::U, Self::U) | (Self::Null, Self::Null) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Num(a), Self::Num(b)) => a.to_bits() == b.to_bits(),
            (Self::Big(a), Self::Big(b)) => a == b,
            (Self::Text(a), Self::Text(b)) => a == b,
            (Self::Obj(a), Self::Obj(b)) => *a.fields.borrow() == *b.fields.borrow(),
            (Self::Array(a), Self::Array(b)) => *a.borrow() == *b.borrow(),
            _ => false,
        }
    }
}
fn obj(v: Vec<(&str, V)>) -> V {
    V::Obj(Rc::new(Obj {
        fields: RefCell::new(v.into_iter().map(|(k, v)| (k.into(), v)).collect()),
    }))
}
fn array(v: Vec<V>) -> V {
    V::Array(Rc::new(RefCell::new(v.into_iter().map(Some).collect())))
}
fn text(v: &str) -> V {
    V::Text(v.encode_utf16().collect())
}
fn string(v: &V) -> String {
    match v {
        V::Text(v) => String::from_utf16(v).unwrap(),
        V::Big(v) => v.to_string(),
        V::Num(v) => safe_cex_num::js_number_string(*v),
        V::U => "undefined".into(),
        V::Null => "null".into(),
        V::Bool(v) => v.to_string(),
        _ => panic!("fixture String conversion"),
    }
}
fn raw(v: &V, k: &str) -> V {
    match v {
        V::Obj(o) => o
            .fields
            .borrow()
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.clone())
            .unwrap_or(V::U),
        V::Array(v) if k == "length" => V::Num(v.borrow().len() as f64),
        V::Array(v) => k
            .parse::<usize>()
            .ok()
            .and_then(|i| v.borrow().get(i).cloned().flatten())
            .unwrap_or(V::U),
        _ => V::U,
    }
}
fn same(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Obj(a), V::Obj(b)) => Rc::ptr_eq(a, b),
        (V::Array(a), V::Array(b)) => Rc::ptr_eq(a, b),
        _ => a == b,
    }
}
fn truthy(v: &V) -> bool {
    match v {
        V::U | V::Null => false,
        V::Bool(v) => *v,
        V::Num(v) => *v != 0.0 && !v.is_nan(),
        V::Big(v) => *v != BigInt::from(0),
        V::Text(v) => !v.is_empty(),
        _ => true,
    }
}
fn decode(j: &J) -> V {
    match j["kind"].as_str().unwrap() {
        "undefined" => V::U,
        "null" => V::Null,
        "boolean" => V::Bool(j["value"].as_bool().unwrap()),
        "number" => V::Num(f64::from_bits(
            u64::from_str_radix(j["bits"].as_str().unwrap(), 16).unwrap(),
        )),
        "bigint" => V::Big(j["decimal"].as_str().unwrap().parse().unwrap()),
        "string" => V::Text(serde_json::from_value(j["units"].clone()).unwrap()),
        "object" => V::Obj(Rc::new(Obj {
            fields: RefCell::new(
                j["fields"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|f| (f[0].as_str().unwrap().into(), decode(&f[1])))
                    .collect(),
            ),
        })),
        "array" => V::Array(Rc::new(RefCell::new(
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
        ))),
        k => panic!("decode {k}"),
    }
}
fn json(v: &V) -> J {
    match v {
        V::U | V::Null => J::Null,
        V::Bool(v) => (*v).into(),
        V::Num(v) => serde_json::Number::from_f64(*v)
            .map(J::Number)
            .unwrap_or(J::Null),
        V::Text(v) => String::from_utf16(v).unwrap().into(),
        V::Big(v) => v.to_string().into(),
        V::Array(v) => J::Array(
            v.borrow()
                .iter()
                .map(|x| x.as_ref().map(json).unwrap_or(J::Null))
                .collect(),
        ),
        V::Obj(v) => J::Object(
            v.fields
                .borrow()
                .iter()
                .filter(|(_, v)| !matches!(v, V::U))
                .map(|(k, v)| (k.clone(), json(v)))
                .collect(),
        ),
        _ => panic!("fixture imported JSON helper"),
    }
}
fn error(name: &'static str, message: &str) -> E {
    TransactionBuildError {
        name,
        message: message.encode_utf16().collect(),
    }
    .into()
}

#[derive(Default)]
struct Engine {
    queue: RefCell<VecDeque<Box<dyn FnOnce()>>>,
}
impl TransactionRuntime for Engine {
    fn enqueue_microtask(&self, callback: Box<dyn FnOnce()>) {
        self.queue.borrow_mut().push_back(callback);
    }
    fn now_millis(&self) -> f64 {
        panic!("no clock in this slice")
    }
    fn auth_token_encoding(&self) -> AuthTokenEncoding {
        panic!("imported signer capability")
    }
}
fn drain(engine: &Engine) {
    for _ in 0..200 {
        let callback = engine.queue.borrow_mut().pop_front();
        match callback {
            Some(callback) => callback(),
            None => return,
        }
    }
    panic!("signer::microtask floor exhausted");
}
#[derive(Debug)]
struct Injected {
    stage: String,
    value: V,
}
struct State {
    row: J,
    engine: Rc<Engine>,
    options: V,
    request: V,
    input: V,
    http: V,
    crypto: V,
    events: Vec<String>,
    calls: Vec<V>,
    constructors: Vec<V>,
    clients: Vec<V>,
    counts: BTreeMap<String, usize>,
    errors: BTreeMap<String, HostThrownValue>,
    secret_index: usize,
    held_once: bool,
    held: Option<(String, V, PromiseResolver<V>)>,
}
type H = Rc<RefCell<State>>;
fn injected(h: &H, stage: &str, message: &str, code: Option<f64>) -> E {
    let mut fields = vec![("message", text(message))];
    if let Some(code) = code {
        fields.push(("code", V::Num(code)));
    }
    let e = HostThrownValue::new(Injected {
        stage: stage.into(),
        value: obj(fields),
    });
    h.borrow_mut().errors.insert(stage.into(), e.clone());
    e.into()
}
fn plain_error(h: &H, stage: &str) -> E {
    injected(h, stage, &format!("injected:{stage}"), None)
}
fn thrown(e: &E) -> V {
    match e {
        E::Generated(e) => obj(vec![("message", V::Text(e.message.clone()))]),
        E::Thrown(e) => e.value().downcast_ref::<Injected>().unwrap().value.clone(),
    }
}
fn signing(stage: &str) -> bool {
    matches!(
        stage,
        "getCreateOrderTransaction"
            | "getCreateOrderTransactions"
            | "getCreateGroupedOrdersTransaction"
            | "getCancelOrderTransaction"
            | "getUpdateLeverageTransaction"
            | "getApproveIntegratorTransaction"
    )
}
fn step(h: &H, stage: &str, args: Vec<V>) -> Result<(), E> {
    let (n, mode, method) = {
        let mut state = h.borrow_mut();
        let n = state.counts.entry(stage.into()).or_default();
        *n += 1;
        let n = *n;
        state.events.push(stage.into());
        if !args.is_empty() {
            let identity = if signing(stage) {
                V::Bool(same(
                    &args[0],
                    if stage == "getCreateOrderTransactions" {
                        &state.input
                    } else {
                        &state.request
                    },
                ))
            } else {
                V::U
            };
            state.calls.push(obj(vec![
                ("name", text(stage)),
                ("args", array(args)),
                ("requestIdentity", identity),
            ]));
        }
        (
            n,
            state.row["mode"].as_str().unwrap().to_owned(),
            state.row["method"].as_str().unwrap().to_owned(),
        )
    };
    if stage == "crypto" && mode == "ordinary" {
        return Err(plain_error(h, "crypto"));
    }
    if stage == "crypto"
        && (mode == "race" && n == 1 || mode == "retry-failure" && method == "initialize")
    {
        return Err(injected(
            h,
            &format!("crypto:{n}"),
            "WASM NOT INITIALIZED",
            None,
        ));
    }
    if stage == "init" && mode == "init-matching" && n == 1 {
        return Err(injected(h, "init:1", "wasm not initialized", None));
    }
    if stage == "TransactionClient" && mode == "constructor-race" && n == 1 {
        return Err(injected(h, "construct:1", "wasm not initialized", None));
    }
    if stage == "resetNonce" && (mode == "reset-throw" || mode == "retry-reset-throw" && n == 2) {
        return Err(plain_error(h, &format!("reset:{n}")));
    }
    if stage == "delay" && mode == "delay-throw" {
        return Err(plain_error(h, "delay"));
    }
    if signing(stage) && (mode == "sign-failure" || mode == "sign-rejection") {
        return Err(plain_error(h, "sign"));
    }
    if matches!(stage, "sendRawTx" | "sendRawTxBatch")
        && ["nonce", "retry-failure", "retry-reset-throw", "delay-throw"].contains(&mode.as_str())
        && (n == 1 || mode == "retry-failure" || mode == "retry-reset-throw")
    {
        return Err(injected(
            h,
            &format!("send:{n}"),
            if n == 1 {
                "bad nonce"
            } else {
                "retry rejected"
            },
            if n == 1 { Some(21104.0) } else { None },
        ));
    }
    if matches!(stage, "sendRawTx" | "sendRawTxBatch")
        && (matches!(
            mode.as_str(),
            "non-nonce" | "reset-throw" | "classifier-throw"
        ) || method == "queue" && mode == "error")
        && n == 1
    {
        return Err(plain_error(h, "send:1"));
    }
    Ok(())
}
fn get(h: &H, value: &V, key: &str) -> Result<V, E> {
    if key == "secret" {
        let mut state = h.borrow_mut();
        if state.row["mode"] == "secret-sequence" && same(value, &state.options) {
            let n = state.secret_index;
            state.secret_index += 1;
            state.events.push(format!("secret:{n}"));
            return Ok(text(if n == 0 { "initial" } else { "stored" }));
        }
    }
    if key == "length" && h.borrow().row["mode"] == "length-throw" {
        return Err(plain_error(h, "length"));
    }
    Ok(raw(value, key))
}
fn finish_send(h: &H, stage: &str, tx: V) -> Result<V, E> {
    step(h, stage, vec![tx])?;
    Ok(if stage == "sendRawTxBatch" {
        obj(vec![
            ("txHashes", array(vec![text("batch-a"), text("batch-b")])),
            ("extra", text("retained")),
        ])
    } else {
        text(&format!("hash:{}", h.borrow().counts[stage]))
    })
}
fn call_async(h: &H, method: V, args: Vec<V>) -> Result<NativePromise<V>, E> {
    let V::Method(stage, receiver) = method else {
        panic!("method")
    };
    let engine = h.borrow().engine.clone();
    if matches!(stage, "sendRawTx" | "sendRawTxBatch") {
        assert!(
            same(&receiver, &h.borrow().http),
            "signer::native http receiver"
        );
        let hold = {
            let mut state = h.borrow_mut();
            let hold = state.row["method"] == "queue" && !state.held_once;
            if hold {
                state.held_once = true;
                state.events.push("held-send".into());
            }
            hold
        };
        if hold {
            let (promise, out) = NativePromise::pending(engine);
            h.borrow_mut().held = Some((stage.into(), args[0].clone(), out));
            return Ok(promise);
        }
        return finish_send(h, stage, args[0].clone())
            .map(|value| NativePromise::settled(engine, Ok(value)));
    }
    assert!(
        same(&receiver, h.borrow().clients.last().unwrap()),
        "signer::native signing receiver {}",
        h.borrow().row["id"]
    );
    let result = step(h, stage, args).map(|_| {
        if stage == "getCreateOrderTransactions" {
            array(vec![
                obj(vec![("tx", text("a"))]),
                obj(vec![("tx", text("b"))]),
            ])
        } else {
            obj(vec![("tx", text(stage))])
        }
    });
    let sign_failure = h.borrow().row["mode"] == "sign-failure";
    let result = match result {
        Err(error) if sign_failure => return Err(error),
        other => other,
    };
    Ok(NativePromise::settled(engine, result))
}
fn runtime(h: H) -> Rc<s::LighterSignerRuntime<H, V>> {
    Rc::new(s::LighterSignerRuntime {
        promise_runtime: h.borrow().engine.clone(),
        host: h.clone(),
        get,
        method: |_, receiver, name| Ok(V::Method(name, Box::new(receiver))),
        call: |h, method, _| {
            let V::Method(name, receiver) = method else {
                panic!("method")
            };
            assert_eq!(name, "resetNonce");
            assert!(
                same(&receiver, h.borrow().clients.last().unwrap()),
                "signer::native reset receiver {}",
                h.borrow().row["id"]
            );
            step(h, name, vec![])?;
            Ok(V::U)
        },
        call_async,
        construct_transaction_client: |h, options| {
            let state = h.borrow();
            assert!(
                same(&raw(&options, "httpClient"), &state.http),
                "signer::native constructor http identity"
            );
            assert!(
                same(&raw(&options, "crypto"), &state.crypto),
                "signer::native constructor crypto identity"
            );
            drop(state);
            h.borrow_mut().constructors.push(options.clone());
            step(h, "TransactionClient", vec![])?;
            let client = obj(vec![("options", options)]);
            h.borrow_mut().clients.push(client.clone());
            Ok(client)
        },
        object: obj,
        array,
        undefined: || V::U,
        text,
        truthy,
        nullish: |v| matches!(v, V::U | V::Null),
        strict_zero: |v| matches!(v,V::Num(n) if *n == 0.0),
        wasm_ready: |h| {
            step(h, "ready", vec![])?;
            Ok(!matches!(
                h.borrow().row["mode"].as_str().unwrap(),
                "load" | "init-matching"
            ))
        },
        init_wasm: |h| {
            let result = step(h, "init", vec![]).map(|_| V::U);
            let engine = h.borrow().engine.clone();
            Ok(NativePromise::settled(engine, result))
        },
        crypto_api: |h| {
            step(h, "crypto", vec![])?;
            Ok(h.borrow().crypto.clone())
        },
        deployment_profile: |_, params| {
            let name: safe_cex_types::ExchangeName =
                serde_json::from_value(json(&raw(&params, "exchangeName"))).unwrap();
            let profile = deployment::resolve_lighter_deployment_profile(
                Some(&name),
                Some(truthy(&raw(&params, "testnet"))),
            );
            Ok(obj(vec![(
                "signerChainId",
                V::Num(profile.signer_chain_id as f64),
            )]))
        },
        error_value: thrown,
        lowercase_string: |_, value| {
            let s = if matches!(value, V::Obj(_) | V::Array(_)) {
                safe_cex_num::js_string(Some(&json(&value))).unwrap()
            } else {
                string(&value)
            };
            Ok(s.to_lowercase().encode_utf16().collect())
        },
        invalid_nonce: |h, error| {
            if h.borrow().row["mode"] == "classifier-throw" {
                return Err(plain_error(h, "classifier"));
            }
            let value = safe_cex_exchange::js_runtime::value::from_json(&json(&thrown(error)));
            signer_helpers::is_lighter_invalid_nonce_error(Some(&value))
                .map_err(|e| super_error(&e.message()))
        },
        delay: |h, milliseconds| {
            let result = step(h, "delay", vec![V::Num(milliseconds as f64)]).map(|_| V::U);
            let engine = h.borrow().engine.clone();
            Ok(NativePromise::settled(engine, result))
        },
        new_error: |_, message| error("Error", message),
        number: V::Num,
        add: |_, _, _| panic!("auth deadline capability outside this slice"),
        big_int: |_, _| panic!("auth deadline capability outside this slice"),
    })
}
fn super_error(message: &str) -> E {
    error("Error", message)
}
fn source() -> &'static J {
    static DATA: std::sync::OnceLock<J> = std::sync::OnceLock::new();
    DATA.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/signer-runtime-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "signer::source oracle {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result: J = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["count"], 62, "signer::independent total floor");
        result
    })
}
fn host(row: &J) -> H {
    Rc::new(RefCell::new(State {
        row: row.clone(),
        engine: Default::default(),
        options: decode(&row["options"]),
        request: decode(&row["request"]),
        input: if row["mode"] == "length-throw" {
            obj(vec![])
        } else {
            decode(&row["input"])
        },
        http: obj(vec![("imported", text("http"))]),
        crypto: obj(vec![("imported", text("crypto"))]),
        events: vec![],
        calls: vec![],
        constructors: vec![],
        clients: vec![],
        counts: Default::default(),
        errors: Default::default(),
        secret_index: 0,
        held_once: false,
        held: None,
    }))
}
type Adapter = s::TealstreetLighterSignerAdapter<H, V>;
fn adapter(h: &H) -> Rc<Adapter> {
    let state = h.borrow();
    let r = runtime(h.clone());
    let a = Adapter::new(
        r,
        state.options.clone(),
        decode(&state.row["exchangeName"]),
        state.http.clone(),
        V::Big("123456789012345678901".parse().unwrap()),
        V::Num(7.0),
    );
    assert!(
        same(&a.options, &state.options),
        "signer::native constructor options identity"
    );
    assert!(
        same(&a.api_client, &state.http),
        "signer::native constructor api identity"
    );
    assert_eq!(
        *a.transaction_client.borrow(),
        V::U,
        "signer::native constructor uninitialized"
    );
    assert_eq!(
        a.account_index,
        V::Big("123456789012345678901".parse().unwrap()),
        "signer::native constructor account index"
    );
    assert_eq!(
        a.api_key_index,
        V::Num(7.0),
        "signer::native constructor key index"
    );
    drop(state);
    if h.borrow().row["mode"] == "existing" {
        a.transaction_client
            .replace(obj(vec![("existing", V::Bool(true))]));
    }
    a
}
fn invoke(a: &Rc<Adapter>, method: &str, request: V) -> NativePromise<V> {
    match method {
        "initialize" => a.initialize(),
        "createOrder" => a.create_order(request),
        "createOrders" => a.create_orders(request),
        "createGroupedOrders" => a.create_grouped_orders(request),
        "cancelOrder" => a.cancel_order(request),
        "updateLeverage" => a.update_leverage(request),
        "approveIntegrator" => a.approve_integrator(request),
        _ => panic!("method {method}"),
    }
}
fn observe(p: &NativePromise<V>) -> Rc<RefCell<Option<PromiseResult<V>>>> {
    let value = Rc::new(RefCell::new(None));
    let captured = value.clone();
    p.subscribe(move |v| *captured.borrow_mut() = Some(v));
    value
}
fn native_outcome(row: &J, index: usize, h: &H, value: PromiseResult<V>) {
    let expected = &row["result"][index];
    let id = &row["id"];
    assert_eq!(
        value.is_err(),
        expected["kind"] == "error",
        "signer::native outcome {id}:{index}"
    );
    match value {
        Ok(value) => assert_eq!(
            *value,
            decode(&expected["value"]),
            "signer::native result {id}:{index}"
        ),
        Err(e) => {
            let message = raw(&thrown(&e), "message");
            assert_eq!(
                message,
                decode(&expected["message"]),
                "signer::native error {id}:{index}"
            );
            let name = match &e {
                E::Generated(e) => e.name,
                E::Thrown(_) => "Error",
            };
            assert_eq!(
                name,
                expected["name"].as_str().unwrap(),
                "signer::native error name {id}:{index}"
            );
            if let Some(stage) = expected["original"].as_str() {
                let E::Thrown(e) = e else {
                    panic!("signer::native original error {id}:{index}")
                };
                let original = h.borrow().errors[stage].clone();
                assert!(
                    e.ptr_eq(&original),
                    "signer::native original error {id}:{index}"
                );
                assert_eq!(e.value().downcast_ref::<Injected>().unwrap().stage, stage);
            }
        }
    }
}
fn before_release(row: &J, h: &H, observers: &[Rc<RefCell<Option<PromiseResult<V>>>>]) {
    let state = h.borrow();
    let id = &row["id"];
    assert_eq!(
        array(state.constructors.clone()),
        decode(&row["beforeRelease"]["constructors"]),
        "signer::native blocked constructors {id}"
    );
    assert_eq!(
        array(state.calls.clone()),
        decode(&row["beforeRelease"]["calls"]),
        "signer::native blocked requests {id}"
    );
    if row["method"] == "queue" || row["mode"] != "throw" {
        assert!(
            observers.iter().all(|v| v.borrow().is_none()),
            "signer::native blocked outcomes {id}"
        );
        assert_eq!(
            state
                .counts
                .get("getCancelOrderTransaction")
                .copied()
                .unwrap_or(0),
            0,
            "signer::native blocked peer {id}"
        );
    }
}
fn run(row: &J, detached: bool) {
    let h = host(row);
    let a = adapter(&h);
    let engine = h.borrow().engine.clone();
    let method = row["method"].as_str().unwrap();
    let mut observers = vec![];
    let mut native_before = None;
    if method == "queue" {
        let request = h.borrow().request.clone();
        let first = a.create_order(request.clone());
        if !detached {
            observers.push(observe(&first));
        }
        drop(first);
        let second = a.cancel_order(request);
        observers.push(observe(&second));
        assert!(
            h.borrow().events.is_empty(),
            "signer::native lock first suspension {}",
            row["id"]
        );
        drain(&engine);
        before_release(row, &h, &observers);
        native_before = Some(h.borrow().events.clone());
        let (stage, tx, out) = h.borrow_mut().held.take().unwrap();
        let result = finish_send(&h, &stage, tx);
        out.settle(result);
    } else if method == "lock" {
        let first_h = h.clone();
        let first = a.run_with_submission_lock(move || {
            first_h.borrow_mut().events.push("callback:first".into());
            if first_h.borrow().row["mode"] == "throw" {
                return Err(plain_error(&first_h, "callback"));
            }
            let engine = first_h.borrow().engine.clone();
            let (promise, out) = NativePromise::pending(engine);
            first_h.borrow_mut().held = Some(("callback".into(), V::U, out));
            Ok(promise)
        });
        observers.push(observe(&first));
        let second_h = h.clone();
        let second = a.run_with_submission_lock(move || {
            second_h.borrow_mut().events.push("callback:second".into());
            let engine = second_h.borrow().engine.clone();
            Ok(NativePromise::settled(engine, Ok(text("second"))))
        });
        observers.push(observe(&second));
        drain(&engine);
        before_release(row, &h, &observers);
        native_before = Some(h.borrow().events.clone());
        let held = h.borrow_mut().held.take();
        if let Some((_, _, out)) = held {
            let result = if row["mode"] == "error" {
                Err(plain_error(&h, "callback"))
            } else {
                Ok(text("first"))
            };
            out.settle(result);
        }
    } else {
        let request = if method == "createOrders" {
            h.borrow().input.clone()
        } else {
            h.borrow().request.clone()
        };
        let promise = invoke(&a, method, request);
        observers.push(observe(&promise));
    }
    drain(&engine);
    for (i, observed) in observers.iter().enumerate() {
        let result = observed.borrow_mut().take();
        assert!(result.is_some(), "signer::native settled {}:{i}", row["id"]);
        native_outcome(row, if detached { i + 1 } else { i }, &h, result.unwrap());
    }
    let state = h.borrow();
    let id = &row["id"];
    assert_eq!(
        state.counts.get("init").copied().unwrap_or(0),
        row["initCount"].as_u64().unwrap() as usize,
        "signer::native init receipts {id}"
    );
    assert_eq!(
        state.counts.get("resetNonce").copied().unwrap_or(0),
        row["resetCount"].as_u64().unwrap() as usize,
        "signer::native nonce resets {id}"
    );
    assert_eq!(
        truthy(&a.transaction_client.borrow()),
        row["clientPresent"].as_bool().unwrap(),
        "signer::native initialized state {id}"
    );
    assert_eq!(
        array(state.constructors.clone()),
        decode(&row["constructors"]),
        "signer::native constructors {id}"
    );
    assert_eq!(
        array(state.calls.clone()),
        decode(&row["calls"]),
        "signer::native requests {id}"
    );
    if let Some(actual_before) = native_before {
        let expected_before: Vec<String> =
            serde_json::from_value(row["beforeRelease"]["events"].clone()).unwrap();
        assert_eq!(
            actual_before, expected_before,
            "signer::blocked event trace {id}"
        );
    }
    let expected: Vec<String> = serde_json::from_value(row["events"].clone()).unwrap();
    assert_eq!(state.events, expected, "signer::event trace {id}");
}
fn family(methods: &[&str], floor: usize) {
    let rows: Vec<_> = source()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| methods.contains(&r["method"].as_str().unwrap()))
        .collect();
    assert_eq!(rows.len(), floor, "signer::independent family floor");
    for row in rows {
        run(row, false);
    }
}
#[test]
fn signer_initialize() {
    family(&["initialize"], 12);
}
#[test]
fn signer_submissions() {
    family(
        &[
            "createOrder",
            "createOrders",
            "createGroupedOrders",
            "cancelOrder",
            "updateLeverage",
            "approveIntegrator",
        ],
        44,
    );
}
#[test]
fn signer_submission_lock() {
    family(&["queue", "lock"], 6);
}
#[test]
fn signer_detached() {
    let row = source()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "queue-ok")
        .unwrap();
    run(row, true);
}
