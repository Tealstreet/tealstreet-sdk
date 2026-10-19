//! lighter-sdk `LighterApiClient`, `OrderApi`, `CandlestickApi` and
//! `LighterApiError` as JS classes assembled from the ported SDK/venue bodies.
use super::{
    ops::{self, Arithmetic, JsIterator},
    realm::Realm,
    value::{js_to_native, native_to_js, to_account, to_js, to_signing, to_thrown},
};
use crate::{
    http_models::Utf16Record,
    http_number_helpers::{is_safe_integer_bigint, to_safe_number},
    sdk_account_api::{
        AccountGet, AccountHttpClient, AccountRequestConfig, AccountSemantics, AccountValue,
    },
    sdk_http_access::{self as access, AccessCall, HttpAccessHost, HttpCall, HttpVerb},
    sdk_market_api::{CandlestickApi, MarketApiErrors},
    sdk_order_queries::{OrderApi, OrderQuerySemantics},
    transaction_promise::{NativePromise, PromiseResolver, TransactionRuntime},
    transaction_signing::{HostThrownValue, SigningResult},
};
use num_bigint::BigInt;
use safe_cex_exchange::js_runtime::{
    client,
    promise::{Promise, await_js},
    value::{Js, Thrown, arg},
};
use safe_cex_lighter_sdk::{
    api_reads::{self, Fields, ReadHost},
    client_runtime::{ClientHost, LighterApiClient, NativeMethod},
    http_types::LighterApiError,
    key_runtime::KeyFuture,
    signature_wire::{Base64Host, to_base64_from_hex},
};
use std::{future::Future, pin::Pin, rc::Rc};

// ts: TX_TYPE_BY_NAME (api-client.ts)
const TX_TYPE_BY_NAME: [(&str, f64); 7] = [
    ("create_order", 14.0),
    ("cancel_order", 15.0),
    ("transfer", 12.0),
    ("withdrawal", 13.0),
    ("update_leverage", 20.0),
    ("approve_integrator", 45.0),
    ("create_grouped_orders", 28.0),
];

/// The SDK body host over one receiver: the client for its own reads, or an
/// `OrderApi` whose `client.get` the order-book read goes through.
struct SdkHost {
    realm: Realm,
    owner: Js,
    prototype: Js,
}

impl SdkHost {
    fn with_owner(&self, owner: Js) -> Rc<Self> {
        Rc::new(Self {
            realm: self.realm.clone(),
            owner,
            prototype: self.prototype.clone(),
        })
    }
    fn global_call(&self, name: &str, args: Vec<Js>) -> Result<Js, Thrown> {
        self.realm.global(name)?.call(Js::Undefined, args)
    }
}

fn object(fields: Fields<Js>) -> Js {
    let out = Js::object(vec![]);
    for (key, value) in fields {
        let _ = out.define_key(&key, value);
    }
    out
}

fn rejected(realm: &Realm, error: Thrown) -> Js {
    Js::Promise(Promise::settled(&realm.exchange_runtime(), Err(error)))
}

fn spawn(realm: &Realm, future: KeyFuture<Js, Thrown>) -> Js {
    Js::Promise(Promise::spawn(&realm.exchange_runtime(), future))
}

impl ReadHost for SdkHost {
    type Value = Js;
    type Error = Thrown;
    fn spawn(&self, task: Pin<Box<dyn Future<Output = ()>>>) {
        // The spawned promise owns the task until it completes.
        let _ = Promise::spawn(
            &self.realm.exchange_runtime(),
            Box::pin(async move {
                task.await;
                Ok(Js::Undefined)
            }),
        );
    }
    fn get(&self, value: &Js, key: &str) -> Result<Js, Thrown> {
        value.get(key)
    }
    fn truthy(&self, value: &Js) -> bool {
        value.truthy()
    }
    fn undefined(&self, value: &Js) -> bool {
        value.is_undefined()
    }
    fn nullish(&self, value: &Js) -> bool {
        value.is_nullish()
    }
    fn number_200(&self, value: &Js) -> bool {
        value.strict_equals(&Js::Number(200.0))
    }
    fn string_200(&self, value: &Js) -> bool {
        value.strict_equals(&Js::str("200"))
    }
    fn is_array(&self, value: &Js) -> bool {
        value.is_array()
    }
    fn string(&self, value: &Js) -> Result<Vec<u16>, Thrown> {
        Ok(ops::to_string(value)?.to_vec())
    }
    fn bigint(&self, value: &Js) -> Result<BigInt, Thrown> {
        match self.global_call("BigInt", vec![value.clone()])? {
            Js::BigInt(b) => Ok((*b).clone()),
            other => Err(Thrown::type_error(&format!(
                "BigInt returned {}",
                other.type_of()
            ))),
        }
    }
    fn text(&self, value: Vec<u16>) -> Js {
        Js::utf16(&value)
    }
    fn number(&self, value: f64) -> Js {
        Js::Number(value)
    }
    fn object(&self, fields: Fields<Js>) -> Js {
        object(fields)
    }
    fn undefined_value(&self) -> Js {
        Js::Undefined
    }
    fn bigint_value(&self, value: BigInt) -> Js {
        Js::BigInt(Rc::new(value))
    }
    fn empty_array(&self) -> Js {
        Js::array(vec![])
    }
    fn copy_properties(&self, value: &Js) -> Result<Fields<Js>, Thrown> {
        if value.is_nullish() {
            return Ok(vec![]);
        }
        Ok(value
            .own_entries()?
            .into_iter()
            .map(|(k, v)| (k.to_vec(), v))
            .collect())
    }
    fn now(&self) -> Result<f64, Thrown> {
        self.global_call("Date.now", vec![])?.to_number()
    }
    fn api_error(&self, message: Js, code: Js) -> Result<Thrown, Thrown> {
        construct_api_error(&self.realm, vec![message, code])
    }
    fn error(&self, message: Js) -> Result<Thrown, Thrown> {
        self.global_call("Error", vec![message]).map(Thrown::new)
    }
    fn http_get(
        &self,
        axios: bool,
        url: Vec<u16>,
        config: Js,
    ) -> Result<KeyFuture<Js, Thrown>, Thrown> {
        let target = self
            .owner
            .get(if axios { "axiosInstance" } else { "client" })?;
        let pending = target.call_method("get", vec![Js::utf16(&url), config])?;
        Ok(Box::pin(await_js(&self.realm.exchange_runtime(), pending)))
    }
    fn promise_resolve(&self, value: Js) -> KeyFuture<Js, Thrown> {
        Box::pin(await_js(&self.realm.exchange_runtime(), value))
    }
}

/// `typeof btoa === 'function'` holds in every runtime the TS client targets.
struct Base64;

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

impl Base64Host for Base64 {
    type Error = Thrown;
    fn has_btoa(&self) -> bool {
        true
    }
    fn btoa(&self, bytes: &[u8]) -> Result<Vec<u16>, Thrown> {
        let mut out = Vec::with_capacity(bytes.len().div_ceil(3) * 4);
        for chunk in bytes.chunks(3) {
            let n = chunk
                .iter()
                .enumerate()
                .fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
            for i in 0..4 {
                out.push(if i <= chunk.len() {
                    u16::from(BASE64[((n >> (18 - 6 * i)) & 63) as usize])
                } else {
                    u16::from(b'=')
                });
            }
        }
        Ok(out)
    }
    fn has_buffer(&self) -> bool {
        false
    }
    fn buffer_base64(&self, bytes: &[u8]) -> Result<Vec<u16>, Thrown> {
        self.btoa(bytes)
    }
    fn error(&self, message: &'static str) -> Thrown {
        Thrown::error("Error", message)
    }
}

fn build_error(error: crate::transaction_builders::TransactionBuildError) -> Thrown {
    Thrown::error(error.name, &String::from_utf16_lossy(&error.message))
}

impl ClientHost for SdkHost {
    fn boolean(&self, value: bool) -> Js {
        Js::Bool(value)
    }
    fn function(&self, body: NativeMethod<Self>) -> Js {
        Js::function("", move |this, args| body(this, args))
    }
    fn call(&self, function: &Js, args: Vec<Js>) -> Result<Js, Thrown> {
        function.call(Js::Undefined, args)
    }
    fn construct(&self, constructor: &Js, args: Vec<Js>) -> Result<Js, Thrown> {
        constructor.construct(args, None)
    }
    fn call_method(&self, receiver: &Js, member: &str, args: Vec<Js>) -> Result<Js, Thrown> {
        receiver.call_method(member, args)
    }
    // Class methods live on the shared prototype, as on the TS class.
    fn receiver(&self, methods: Fields<Js>) -> Result<Js, Thrown> {
        if self.prototype.own_keys().is_empty() {
            for (key, method) in methods {
                self.prototype.define_key(&key, method)?;
            }
        }
        let instance = Js::object(vec![]);
        if let Js::Object(o) = &instance {
            *o.proto.borrow_mut() = Some(self.prototype.clone());
        }
        Ok(instance)
    }
    fn existing_method(&self, name: &str) -> Result<Js, Thrown> {
        client_method(self.with_owner(Js::Undefined), name)
    }
    fn define_field(&self, receiver: &Js, key: &str, value: Js) -> Result<(), Thrown> {
        receiver.define(key, value)
    }
    fn set(&self, receiver: &Js, key: &str, value: Js) -> Result<(), Thrown> {
        receiver.set(key, value)
    }
    fn axios_factory(&self) -> Result<Js, Thrown> {
        let http = self.realm.0.http.clone();
        Ok(Js::function("create", move |_, a| {
            Ok(client::create(http.clone(), arg(&a, 0)))
        }))
    }
    fn call_bound(&self, function: &Js, receiver: &Js, args: Vec<Js>) -> Result<Js, Thrown> {
        function.call(receiver.clone(), args)
    }
    fn iterator(&self, value: Js) -> Result<Js, Thrown> {
        let iterator = JsIterator::open(&value)?;
        Ok(Js::function("next", move |_, _| {
            Ok(match iterator.step()? {
                Some(value) => Js::object(vec![("done", Js::Bool(false)), ("value", value)]),
                None => Js::object(vec![("done", Js::Bool(true))]),
            })
        }))
    }
    fn next(&self, iterator: &Js) -> Result<Option<Js>, Thrown> {
        let step = iterator.call(Js::Undefined, vec![])?;
        if step.get("done")?.truthy() {
            Ok(None)
        } else {
            step.get("value").map(Some)
        }
    }
    fn logger(&self, error: bool) -> Result<Js, Thrown> {
        self.realm.global(if error {
            "console.error"
        } else {
            "console.log"
        })
    }
    fn reject(&self, reason: Js) -> Result<Js, Thrown> {
        Ok(rejected(&self.realm, Thrown::new(reason)))
    }
    fn publish(&self, future: KeyFuture<Js, Thrown>) -> Js {
        spawn(&self.realm, future)
    }
    fn api_error_value(
        &self,
        message: Js,
        code: Js,
        details: Js,
        request_id: Js,
        status: Js,
    ) -> Result<Js, Thrown> {
        construct_api_error(
            &self.realm,
            vec![message, code, details, request_id, status],
        )
        .map(|thrown| thrown.value)
    }
    fn is_string(&self, value: &Js) -> bool {
        value.is_string()
    }
    fn string_length(&self, value: &Js) -> Result<f64, Thrown> {
        value.get("length")?.to_number()
    }
    fn strict_number(&self, value: &Js, n: f64) -> bool {
        value.strict_equals(&Js::Number(n))
    }
    fn strict_text(&self, value: &Js, s: &str) -> bool {
        value.strict_equals(&Js::str(s))
    }
    fn template_string(&self, value: &Js) -> Result<Vec<u16>, Thrown> {
        Ok(ops::to_string(value)?.to_vec())
    }
    // ts: toSafeNumber — the bigint domain runs the ported helper; any other
    // value takes the source's mixed comparison and Number() conversion.
    fn safe_number(&self, value: Js, label: &str) -> Result<Js, Thrown> {
        if let Js::BigInt(b) = &value {
            let label: Vec<u16> = label.encode_utf16().collect();
            return to_safe_number(b, &label)
                .map(Js::Number)
                .map_err(build_error);
        }
        if !self.safe_integer_bigint(value.clone())?.truthy() {
            return Err(Thrown::error(
                "Error",
                &format!("{label} exceeds Number safe integer range"),
            ));
        }
        self.number_call(value)
    }
    // ts: isSafeIntegerBigInt
    fn safe_integer_bigint(&self, value: Js) -> Result<Js, Thrown> {
        if let Js::BigInt(b) = &value {
            return Ok(Js::Bool(is_safe_integer_bigint(b)));
        }
        let max = Js::BigInt(Rc::new(BigInt::from(9007199254740991u64)));
        let min = Js::BigInt(Rc::new(-BigInt::from(9007199254740991u64)));
        Ok(Js::Bool(
            ops::relation(ops::Relation::LessOrEqual, &value, &max)?
                && ops::relation(ops::Relation::GreaterOrEqual, &value, &min)?,
        ))
    }
    // ts: toBase64FromHex
    fn signature_base64(&self, value: Js) -> Result<Js, Thrown> {
        let Js::String(hex) = &value else {
            value.get("startsWith")?;
            return Err(Thrown::type_error("hexValue.startsWith is not a function"));
        };
        to_base64_from_hex(&Base64, hex).map(|units| Js::utf16(&units))
    }
    fn tx_type_table(&self) -> Js {
        Js::object(
            TX_TYPE_BY_NAME
                .iter()
                .map(|(k, v)| (*k, Js::Number(*v)))
                .collect(),
        )
    }
    fn computed_get(&self, object: &Js, key: Js) -> Result<Js, Thrown> {
        object.get_key(&ops::to_string(&key)?)
    }
    fn now_value(&self) -> Result<Js, Thrown> {
        self.global_call("Date.now", vec![])
    }
    fn add_number(&self, value: Js, number: f64) -> Result<Js, Thrown> {
        ops::arithmetic(Arithmetic::Add, &value, &Js::Number(number))
    }
    fn bigint_call(&self, value: Js) -> Result<Js, Thrown> {
        self.global_call("BigInt", vec![value])
    }
    fn bigint_zero(&self) -> Js {
        Js::BigInt(Rc::new(BigInt::from(0)))
    }
    // Number(value): a BigInt converts exactly, which the realm's unary-plus
    // ToNumber refuses.
    fn number_call(&self, value: Js) -> Result<Js, Thrown> {
        if let Js::BigInt(b) = &value {
            return safe_cex_num::js_number_primitive(&safe_cex_num::JsPrimitive::BigInt(
                (**b).clone(),
            ))
            .map(Js::Number)
            .map_err(Thrown::from);
        }
        self.global_call("Number", vec![value])
    }
    fn stringify(&self, value: Js) -> Result<Js, Thrown> {
        self.global_call("JSON.stringify", vec![value])
    }
    fn url_search_params_factory(&self) -> Result<Js, Thrown> {
        self.realm.global("URLSearchParams")
    }
    fn json_stringifier(&self) -> Result<Js, Thrown> {
        self.realm.global("JSON.stringify")
    }
    fn every_string(&self, value: &Js) -> Result<bool, Thrown> {
        let predicate = Js::function("", |_, a| Ok(Js::Bool(arg(&a, 0).is_string())));
        Ok(value.call_method("every", vec![predicate])?.truthy())
    }
}

/// `new LighterApiError(...)`, through the realm's binding.
fn construct_api_error(realm: &Realm, args: Vec<Js>) -> Result<Thrown, Thrown> {
    realm
        .global("LighterApiError")?
        .construct(args, None)
        .map(Thrown::new)
}

// ts: LighterApiError.constructor — Error(message) coercion, the default code,
// then the brand and raw details/requestId/statusCode assignments.
fn lighter_api_error(
    superclass: &Js,
    constructor: Js,
    target: Js,
    args: Vec<Js>,
) -> Result<Js, Thrown> {
    let error = superclass.construct(vec![arg(&args, 0)], Some(&target))?;
    let shape = LighterApiError::<Js>::new(vec![], None, None, None, None);
    for key in ["code", "details", "requestId", "statusCode"] {
        error.define(key, Js::Undefined)?;
    }
    error.set("name", Js::str(shape.name))?;
    let code = arg(&args, 1);
    error.set(
        "code",
        if code.is_undefined() {
            Js::utf16(&shape.code)
        } else {
            code
        },
    )?;
    error.set("details", arg(&args, 2))?;
    error.set("requestId", arg(&args, 3))?;
    error.set("statusCode", arg(&args, 4))?;
    if superclass.get("captureStackTrace")?.truthy() {
        let capture = superclass.get("captureStackTrace")?;
        if !capture.is_callable() {
            return Err(Thrown::type_error(
                "Error.captureStackTrace is not a function",
            ));
        }
        capture.call(superclass.clone(), vec![error.clone(), constructor])?;
    }
    Ok(error)
}

/// One receiver's `HttpAccessHost`/`AccountHttpClient`/semantics view.
struct Access {
    realm: Realm,
    owner: Js,
}

struct Call {
    callee: Js,
    this: Js,
}

impl Call {
    fn invoke(&self, args: Vec<Js>) -> Result<Js, Thrown> {
        if !self.callee.is_callable() {
            return Err(Thrown::type_error(&format!(
                "{} is not a function",
                self.callee.to_display()
            )));
        }
        self.callee.call(self.this.clone(), args)
    }
}

struct RealmCall {
    call: Call,
    realm: Realm,
}

impl AccessCall for Call {
    fn call(&self, args: &[AccountValue]) -> SigningResult<AccountValue> {
        self.invoke(args.iter().map(to_js).collect())
            .map(|v| to_account(&v))
            .map_err(to_signing)
    }
}

impl HttpCall for RealmCall {
    fn call(&self, args: &[AccountValue]) -> SigningResult<NativePromise<AccountValue>> {
        let value = self
            .call
            .invoke(args.iter().map(to_js).collect())
            .map_err(to_signing)?;
        Ok(js_to_native(
            &self.realm.exchange_runtime(),
            self.realm.0.extra.microtasks.clone(),
            value,
        ))
    }
}

impl AccountGet for RealmCall {
    fn call(
        &self,
        url: Vec<u16>,
        config: Option<AccountRequestConfig>,
    ) -> SigningResult<NativePromise<AccountValue>> {
        let mut args = vec![Js::utf16(&url)];
        if let Some(config) = config {
            args.push(Js::object(vec![("params", record(config.params))]));
        }
        let value = self.call.invoke(args).map_err(to_signing)?;
        Ok(js_to_native(
            &self.realm.exchange_runtime(),
            self.realm.0.extra.microtasks.clone(),
            value,
        ))
    }
}

fn record(params: Utf16Record<AccountValue>) -> Js {
    object(params.into_iter().map(|(k, v)| (k, to_js(&v))).collect())
}

impl HttpAccessHost for Access {
    fn runtime(&self) -> Rc<dyn TransactionRuntime> {
        self.realm.0.extra.microtasks.clone()
    }
    fn semantics(&self) -> Rc<dyn AccountSemantics> {
        Rc::new(Semantics(self.realm.clone()))
    }
    fn http_callback(&self, verb: HttpVerb) -> SigningResult<Rc<dyn HttpCall>> {
        let axios = self.owner.get("axiosInstance").map_err(to_signing)?;
        let member = match verb {
            HttpVerb::Get => "get",
            HttpVerb::Post => "post",
            HttpVerb::Put => "put",
            HttpVerb::Delete => "delete",
        };
        let callee = axios.get(member).map_err(to_signing)?;
        Ok(Rc::new(RealmCall {
            call: Call {
                callee,
                this: axios,
            },
            realm: self.realm.clone(),
        }))
    }
    fn interceptors(&self) -> SigningResult<AccountValue> {
        self.owner
            .get("interceptors")
            .map(|v| to_account(&v))
            .map_err(to_signing)
    }
    fn property(&self, value: &AccountValue, name: &str) -> SigningResult<AccountValue> {
        to_js(value)
            .get(name)
            .map(|v| to_account(&v))
            .map_err(to_signing)
    }
    fn set_property(
        &self,
        value: &AccountValue,
        name: &str,
        new_value: AccountValue,
    ) -> SigningResult<()> {
        to_js(value)
            .set(name, to_js(&new_value))
            .map_err(to_signing)
    }
    fn empty_array(&self) -> SigningResult<AccountValue> {
        Ok(to_account(&Js::array(vec![])))
    }
    fn push_callback(&self, value: &AccountValue) -> SigningResult<Rc<dyn AccessCall>> {
        let this = to_js(value);
        let callee = this.get("push").map_err(to_signing)?;
        Ok(Rc::new(Call { callee, this }))
    }
    fn config(&self) -> SigningResult<AccountValue> {
        self.owner
            .get("config")
            .map(|v| to_account(&v))
            .map_err(to_signing)
    }
    fn copy_data_properties(&self, value: &AccountValue) -> SigningResult<AccountValue> {
        ops::copy_data(&[to_js(value)])
            .map(|v| to_account(&v))
            .map_err(to_signing)
    }
}

impl AccountHttpClient for Access {
    fn get_callback(&self) -> SigningResult<Rc<dyn AccountGet>> {
        let client = self.owner.get("client").map_err(to_signing)?;
        let callee = client.get("get").map_err(to_signing)?;
        Ok(Rc::new(RealmCall {
            call: Call {
                callee,
                this: client,
            },
            realm: self.realm.clone(),
        }))
    }
}

struct Semantics(Realm);

impl AccountSemantics for Semantics {
    fn get_property(&self, value: &AccountValue, key: &str) -> SigningResult<AccountValue> {
        to_js(value)
            .get(key)
            .map(|v| to_account(&v))
            .map_err(to_signing)
    }
    fn error_message(&self, value: &AccountValue) -> SigningResult<Vec<u16>> {
        ops::to_string(&to_js(value))
            .map(|s| s.to_vec())
            .map_err(to_signing)
    }
    fn resolve_return(
        self: Rc<Self>,
        value: AccountValue,
        resolver: PromiseResolver<AccountValue>,
        runtime: Rc<dyn TransactionRuntime>,
    ) {
        let js = to_js(&value);
        let thenable = match &js {
            Js::Promise(_) => Ok(true),
            Js::Object(_) | Js::Function(_) => js.get("then").map(|t| t.is_callable()),
            _ => Ok(false),
        };
        match thenable {
            Ok(false) => resolver.settle(Ok(value)),
            Err(e) => resolver.settle(Err(to_signing(e))),
            Ok(true) => {
                let adopted = js_to_native(&self.0.exchange_runtime(), runtime, js);
                adopted.subscribe(move |result| {
                    resolver.settle(result.map(|v| (*v).clone()));
                });
            }
        }
    }
}

impl OrderQuerySemantics for Semantics {
    fn to_string_method(&self, value: &AccountValue) -> SigningResult<AccountValue> {
        to_js(value)
            .call_method("toString", vec![])
            .map(|v| to_account(&v))
            .map_err(to_signing)
    }
}

impl MarketApiErrors for Semantics {
    fn construct(
        &self,
        message: AccountValue,
        code: AccountValue,
    ) -> SigningResult<HostThrownValue> {
        construct_api_error(&self.0, vec![to_js(&message), to_js(&code)])
            .map(HostThrownValue::new)
            .map_err(to_signing)
    }
}

fn bigint_arg(value: &Js, name: &str) -> Result<BigInt, Thrown> {
    match value {
        Js::BigInt(b) => Ok((**b).clone()),
        other => Err(Thrown::type_error(&format!(
            "{name} must be a bigint, got {}",
            other.type_of()
        ))),
    }
}

fn number_arg(value: &Js, name: &str) -> Result<f64, Thrown> {
    value.as_f64().ok_or_else(|| {
        Thrown::type_error(&format!("{name} must be a number, got {}", value.type_of()))
    })
}

fn optional_number(value: &Js, name: &str) -> Result<Option<f64>, Thrown> {
    match value {
        Js::Undefined => Ok(None),
        other => number_arg(other, name).map(Some),
    }
}

fn string_arg(value: &Js, name: &str) -> Result<Vec<u16>, Thrown> {
    match value {
        Js::String(s) => Ok(s.to_vec()),
        other => Err(Thrown::type_error(&format!(
            "{name} must be a string, got {}",
            other.type_of()
        ))),
    }
}

/// An async method: argument failures reject, as inside the source body.
fn async_method(
    realm: &Realm,
    name: &str,
    body: impl Fn(Js, Vec<Js>) -> Result<Js, Thrown> + 'static,
) -> Js {
    let realm = realm.clone();
    Js::function(name, move |this, a| {
        Ok(body(this, a).unwrap_or_else(|e| rejected(&realm, e)))
    })
}

fn bigint_future(realm: &Realm, future: KeyFuture<BigInt, Thrown>) -> Js {
    spawn(
        realm,
        Box::pin(async move { future.await.map(|b| Js::BigInt(Rc::new(b))) }),
    )
}

/// The client members `LighterApiClient::new` takes from their own rows.
fn client_method(host: Rc<SdkHost>, name: &str) -> Result<Js, Thrown> {
    let realm = host.realm.clone();
    let access = move |this: Js| Access {
        realm: realm.clone(),
        owner: this,
    };
    let interceptor = |adder: fn(&dyn HttpAccessHost, AccountValue) -> SigningResult<()>| {
        let access = access.clone();
        Js::function(name, move |this, a| {
            adder(&access(this), to_account(&arg(&a, 0)))
                .map(|()| Js::Undefined)
                .map_err(to_thrown)
        })
    };
    let r = host.realm.clone();
    Ok(match name {
        // ts: LighterApiClient.addRequestInterceptor
        "addRequestInterceptor" => interceptor(access::add_request_interceptor),
        // ts: LighterApiClient.addResponseInterceptor
        "addResponseInterceptor" => interceptor(access::add_response_interceptor),
        // ts: LighterApiClient.addErrorInterceptor
        "addErrorInterceptor" => interceptor(access::add_error_interceptor),
        // ts: LighterApiClient.getNextNonce
        "getNextNonce" => async_method(&r.clone(), name, move |this, a| {
            let account = bigint_arg(&arg(&a, 0), "accountIndex")?;
            let key = number_arg(&arg(&a, 1), "apiKeyIndex")?;
            let future = api_reads::get_next_nonce(host.with_owner(this), account, key);
            Ok(bigint_future(&r, future))
        }),
        // ts: LighterApiClient.getApiKeys
        "getApiKeys" => async_method(&r.clone(), name, move |this, a| {
            let account = bigint_arg(&arg(&a, 0), "accountIndex")?;
            let key = number_arg(&arg(&a, 1), "apiKeyIndex")?;
            Ok(spawn(
                &r,
                api_reads::get_api_keys(host.with_owner(this), account, key),
            ))
        }),
        // ts: LighterApiClient.getTransferFeeInfo
        "getTransferFeeInfo" => async_method(&r.clone(), name, move |this, a| {
            let account = bigint_arg(&arg(&a, 0), "accountIndex")?;
            let to = bigint_arg(&arg(&a, 1), "toAccountIndex")?;
            let auth = ops::to_string(&arg(&a, 2))?.to_vec();
            Ok(spawn(
                &r,
                api_reads::get_transfer_fee_info(host.with_owner(this), account, to, auth),
            ))
        }),
        // ts: LighterApiClient.get
        "get" => async_method(&r.clone(), name, move |this, a| {
            let path = ops::to_string(&arg(&a, 0))?.to_vec();
            let promise = access::get(&access(this), path, to_account(&arg(&a, 1)));
            Ok(native_to_js(&r.exchange_runtime(), promise))
        }),
        // ts: LighterApiClient.delete
        "delete" => async_method(&r.clone(), name, move |this, a| {
            let path = ops::to_string(&arg(&a, 0))?.to_vec();
            let promise = access::delete(&access(this), path, to_account(&arg(&a, 1)));
            Ok(native_to_js(&r.exchange_runtime(), promise))
        }),
        // ts: LighterApiClient.post
        "post" => async_method(&r.clone(), name, move |this, a| {
            let path = ops::to_string(&arg(&a, 0))?.to_vec();
            let (data, config) = (to_account(&arg(&a, 1)), to_account(&arg(&a, 2)));
            let promise = access::post(&access(this), path, data, config);
            Ok(native_to_js(&r.exchange_runtime(), promise))
        }),
        // ts: LighterApiClient.put
        "put" => async_method(&r.clone(), name, move |this, a| {
            let path = ops::to_string(&arg(&a, 0))?.to_vec();
            let (data, config) = (to_account(&arg(&a, 1)), to_account(&arg(&a, 2)));
            let promise = access::put(&access(this), path, data, config);
            Ok(native_to_js(&r.exchange_runtime(), promise))
        }),
        // ts: LighterApiClient.getBaseUrl
        "getBaseUrl" => Js::function(name, move |this, _| {
            access::get_base_url(&access(this))
                .map(|v| to_js(&v))
                .map_err(to_thrown)
        }),
        // ts: LighterApiClient.getConfig
        "getConfig" => Js::function(name, move |this, _| {
            access::get_config(&access(this))
                .map(|v| to_js(&v))
                .map_err(to_thrown)
        }),
        other => {
            return Err(Thrown::type_error(&format!(
                "LighterApiClient has no member {other}"
            )));
        }
    })
}

/// `class X { constructor(private readonly client) {} }` with `methods` on its prototype.
fn api_class(name: &str, methods: Vec<(&str, Js)>) -> Result<Js, Thrown> {
    let prototype = Js::object(vec![]);
    for (key, method) in methods {
        prototype.define(key, method)?;
    }
    let class = Js::constructor(name, None, move |target, a| {
        let instance = target.allocate_instance()?;
        instance.define("client", arg(&a, 0))?;
        Ok(instance)
    });
    class.define("prototype", prototype)?;
    Ok(class)
}

fn order_api(realm: &Realm, owner: Js) -> OrderApi {
    let semantics = Rc::new(Semantics(realm.clone()));
    OrderApi::new(
        Rc::new(Access {
            realm: realm.clone(),
            owner,
        }),
        realm.0.extra.microtasks.clone(),
        semantics.clone(),
        semantics,
    )
}

fn optional_options(value: &Js) -> Option<AccountValue> {
    (!value.is_undefined()).then(|| to_account(value))
}

fn order_api_class(realm: &Realm, sdk: Rc<SdkHost>) -> Result<Js, Thrown> {
    let method =
        |name: &'static str,
         body: fn(&OrderApi, &[Js]) -> Result<NativePromise<AccountValue>, Thrown>| {
            let realm = realm.clone();
            (
                name,
                async_method(&realm.clone(), name, move |this, a| {
                    let promise = body(&order_api(&realm, this), &a)?;
                    Ok(native_to_js(&realm.exchange_runtime(), promise))
                }),
            )
        };
    let book_realm = realm.clone();
    let methods = vec![
        // ts: OrderApi.getAccountActiveOrders
        method("getAccountActiveOrders", |api, a| {
            let index = bigint_arg(&arg(a, 0), "accountIndex")?;
            let market = optional_number(&arg(a, 1), "marketId")?;
            Ok(api.get_account_active_orders(&index, market))
        }),
        // ts: OrderApi.getAccountInactiveOrders
        method("getAccountInactiveOrders", |api, a| {
            let index = bigint_arg(&arg(a, 0), "accountIndex")?;
            let options = optional_options(&arg(a, 1));
            Ok(api.get_account_inactive_orders(&index, options.as_ref()))
        }),
        // ts: OrderApi.getOrderBooks
        method("getOrderBooks", |api, a| {
            Ok(api.get_order_books(optional_number(&arg(a, 0), "marketId")?))
        }),
        // ts: OrderApi.getOrderBookOrders — the body lives in the SDK crate.
        (
            "getOrderBookOrders",
            async_method(realm, "getOrderBookOrders", move |this, a| {
                let market = number_arg(&arg(&a, 0), "marketId")?;
                let limit = match arg(&a, 1) {
                    Js::Number(n) => Some(n),
                    other if !other.truthy() => None,
                    other => {
                        return Err(Thrown::type_error(&format!(
                            "limit must be a number, got {}",
                            other.type_of()
                        )));
                    }
                };
                let future = api_reads::get_order_book_orders(
                    sdk.with_owner(this),
                    market,
                    limit,
                    arg(&a, 2),
                );
                Ok(spawn(&book_realm, future))
            }),
        ),
        // ts: OrderApi.getOrderBookDetails
        method("getOrderBookDetails", |api, a| {
            Ok(api.get_order_book_details(optional_number(&arg(a, 0), "marketId")?))
        }),
        // ts: OrderApi.getRecentTrades
        method("getRecentTrades", |api, a| {
            let market = number_arg(&arg(a, 0), "marketId")?;
            Ok(api.get_recent_trades(market, optional_number(&arg(a, 1), "limit")?))
        }),
        // ts: OrderApi.getTrades
        method("getTrades", |api, a| {
            Ok(api.get_trades(optional_options(&arg(a, 0)).as_ref()))
        }),
        // ts: OrderApi.getExchangeStats
        method("getExchangeStats", |api, _| Ok(api.get_exchange_stats())),
    ];
    api_class("OrderApi", methods)
}

type CandleRead = fn(&CandlestickApi, f64, Vec<u16>, f64, f64, f64) -> NativePromise<AccountValue>;

fn candlestick_api_class(realm: &Realm) -> Result<Js, Thrown> {
    let method = |name: &'static str, read: CandleRead| {
        let realm = realm.clone();
        (
            name,
            async_method(&realm.clone(), name, move |this, a| {
                let semantics = Rc::new(Semantics(realm.clone()));
                let api = CandlestickApi::new(
                    Rc::new(Access {
                        realm: realm.clone(),
                        owner: this,
                    }),
                    realm.0.extra.microtasks.clone(),
                    semantics.clone(),
                    semantics,
                );
                let promise = read(
                    &api,
                    number_arg(&arg(&a, 0), "marketId")?,
                    string_arg(&arg(&a, 1), "resolution")?,
                    number_arg(&arg(&a, 2), "startTs")?,
                    number_arg(&arg(&a, 3), "endTs")?,
                    number_arg(&arg(&a, 4), "countBack")?,
                );
                Ok(native_to_js(&realm.exchange_runtime(), promise))
            }),
        )
    };
    api_class(
        "CandlestickApi",
        vec![
            // ts: CandlestickApi.getCandlesticks
            method("getCandlesticks", CandlestickApi::get_candlesticks),
            // ts: CandlestickApi.getFundings
            method("getFundings", CandlestickApi::get_fundings),
        ],
    )
}

pub(crate) fn install(realm: &Realm) -> Result<(), Thrown> {
    let globals = &realm.0.globals;
    let superclass = realm.global("Error")?;
    let error_proto = Js::object(vec![]);
    if let Js::Object(data) = &error_proto {
        *data.proto.borrow_mut() = Some(superclass.get("prototype")?);
    }
    let identity = Rc::new(std::cell::RefCell::new(std::rc::Weak::new()));
    let class_identity = identity.clone();
    let error_class = Js::constructor("LighterApiError", None, move |target, args| {
        let constructor = class_identity
            .borrow()
            .upgrade()
            .ok_or_else(|| Thrown::type_error("LighterApiError constructor was released"))?;
        lighter_api_error(&superclass, Js::Function(constructor), target, args)
    });
    if let Js::Function(function) = &error_class {
        *identity.borrow_mut() = Rc::downgrade(function);
    }
    error_class.define("prototype", error_proto)?;
    globals.define("LighterApiError", error_class)?;

    let sdk = Rc::new(SdkHost {
        realm: realm.clone(),
        owner: Js::Undefined,
        prototype: Js::object(vec![]),
    });
    let client_host = sdk.clone();
    // ts: LighterApiClient.constructor
    let client_class = Js::constructor("LighterApiClient", None, move |_, a| {
        LighterApiClient::new(client_host.clone(), arg(&a, 0)).map(|client| client.receiver)
    });
    client_class.define("prototype", sdk.prototype.clone())?;
    globals.define("LighterApiClient", client_class)?;
    globals.define("OrderApi", order_api_class(realm, sdk)?)?;
    globals.define("CandlestickApi", candlestick_api_class(realm)?)?;
    Ok(())
}

#[cfg(test)]
mod tests;
