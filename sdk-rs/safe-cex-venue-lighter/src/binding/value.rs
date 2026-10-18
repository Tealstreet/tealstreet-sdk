//! Bridge between the crate's host-boundary `AccountValue` and the shared
//! `js_runtime::Js` model. Object identity survives both directions.
use crate::{
    sdk_account_api::{AccountObject, AccountValue},
    transaction_builders::TransactionBuildError,
    transaction_promise::{NativePromise, TransactionRuntime},
    transaction_signing::{
        AuthTokenEncoding, HostThrownValue, SigningResult, TransactionSigningError,
    },
};
use safe_cex_exchange::{
    js_runtime::{
        promise::Promise,
        value::{Js, Thrown},
    },
    runtime::{ExchangeRuntime, ExchangeTask},
};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, VecDeque},
    rc::{Rc, Weak},
};

/// A JS object (or array/map/set/promise) seen through the host boundary.
pub(crate) struct JsObject(pub(crate) Js);

impl AccountObject for JsObject {
    fn get(&self, key: &str) -> SigningResult<AccountValue> {
        self.0.get(key).map(|v| to_account(&v)).map_err(to_signing)
    }
}

struct Tables {
    // js function allocation -> the one host handle for it (ptr_eq identity).
    functions: HashMap<usize, Weak<dyn std::any::Any>>,
    // js allocation -> wrapper, so one JS object always maps to one wrapper.
    forward: HashMap<usize, Weak<JsObject>>,
    // wrapper allocation -> wrapper, to recover the Js behind a host object.
    reverse: HashMap<usize, Weak<JsObject>>,
}

fn tables<R>(f: impl FnOnce(&mut Tables) -> R) -> R {
    thread_local! {
        static TABLES: RefCell<Tables> = RefCell::new(Tables {
            functions: HashMap::new(),
            forward: HashMap::new(),
            reverse: HashMap::new(),
        });
    }
    TABLES.with(|t| f(&mut t.borrow_mut()))
}

fn identity(value: &Js) -> Option<usize> {
    Some(match value {
        Js::Array(v) => Rc::as_ptr(v) as *const () as usize,
        Js::Object(v) => Rc::as_ptr(v) as *const () as usize,
        Js::Map(v) => Rc::as_ptr(v) as *const () as usize,
        Js::Set(v) => Rc::as_ptr(v) as *const () as usize,
        _ => return None,
    })
}

fn prune(map: &mut HashMap<usize, Weak<JsObject>>) {
    if map.len() > 4096 {
        map.retain(|_, w| w.strong_count() > 0);
    }
}

fn wrap(value: &Js) -> AccountValue {
    let key = identity(value);
    tables(|t| {
        if let Some(existing) = key.and_then(|k| t.forward.get(&k).and_then(Weak::upgrade)) {
            return AccountValue::Object(existing);
        }
        let wrapper = Rc::new(JsObject(value.clone()));
        if let Some(k) = key {
            prune(&mut t.forward);
            t.forward.insert(k, Rc::downgrade(&wrapper));
        }
        prune(&mut t.reverse);
        t.reverse.insert(
            Rc::as_ptr(&wrapper) as *const () as usize,
            Rc::downgrade(&wrapper),
        );
        AccountValue::Object(wrapper)
    })
}

pub(crate) fn to_account(value: &Js) -> AccountValue {
    match value {
        Js::Undefined => AccountValue::Undefined,
        Js::Null => AccountValue::Null,
        Js::Bool(b) => AccountValue::Boolean(*b),
        Js::Number(n) => AccountValue::Number(*n),
        Js::BigInt(b) => AccountValue::BigInt((**b).clone()),
        Js::String(s) => AccountValue::String(s.to_vec()),
        Js::Function(f) => {
            AccountValue::Function(function_handle(value, Rc::as_ptr(f) as *const () as usize))
        }
        other => wrap(other),
    }
}

pub(crate) fn to_js(value: &AccountValue) -> Js {
    match value {
        AccountValue::Undefined | AccountValue::HtmlDda => Js::Undefined,
        AccountValue::Null => Js::Null,
        AccountValue::Boolean(b) => Js::Bool(*b),
        AccountValue::Number(n) => Js::Number(*n),
        AccountValue::BigInt(b) => Js::BigInt(Rc::new(b.clone())),
        AccountValue::String(s) => Js::utf16(s),
        AccountValue::Function(f) | AccountValue::Symbol(f) => f
            .value()
            .downcast_ref::<Js>()
            .cloned()
            .expect("host functions are js_runtime values"),
        AccountValue::Object(o) => object_js(o),
    }
}

/// One host handle per JS function, so ported identity checks (`f !== g`) hold.
fn function_handle(value: &Js, key: usize) -> HostThrownValue {
    tables(|t| {
        if let Some(existing) = t.functions.get(&key).and_then(Weak::upgrade) {
            return HostThrownValue::from_shared(existing);
        }
        if t.functions.len() > 4096 {
            t.functions.retain(|_, w| w.strong_count() > 0);
        }
        let handle = HostThrownValue::new(value.clone());
        t.functions.insert(key, Rc::downgrade(&handle.shared()));
        handle
    })
}

fn object_js(object: &Rc<dyn AccountObject>) -> Js {
    let ptr = Rc::as_ptr(object) as *const () as usize;
    tables(|t| t.reverse.get(&ptr).and_then(Weak::upgrade))
        .map(|w| w.0.clone())
        .expect("host objects originate in js_runtime")
}

/// A JS throw crossing into a body keeps its original value for rethrow.
pub(crate) fn to_signing(thrown: Thrown) -> TransactionSigningError {
    TransactionSigningError::Thrown(HostThrownValue::new(thrown))
}

pub(crate) fn to_thrown(error: TransactionSigningError) -> Thrown {
    match error {
        TransactionSigningError::Thrown(value) => value
            .value()
            .downcast_ref::<Thrown>()
            .cloned()
            .unwrap_or_else(|| Thrown::new(Js::Undefined)),
        TransactionSigningError::Generated(TransactionBuildError { name, message }) => {
            Thrown::error(name, &String::from_utf16_lossy(&message))
        }
    }
}

/// FIFO microtasks for the crate's native promises, drained by one runtime task
/// that owns its own handle until the queue empties.
pub(crate) struct Microtasks {
    runtime: Rc<dyn ExchangeRuntime>,
    queue: Rc<RefCell<VecDeque<Box<dyn FnOnce()>>>>,
    running: Rc<Cell<bool>>,
    drainer: Rc<RefCell<Option<Box<dyn ExchangeTask>>>>,
}

impl Microtasks {
    pub(crate) fn new(runtime: Rc<dyn ExchangeRuntime>) -> Rc<Self> {
        Rc::new(Self {
            runtime,
            queue: Rc::default(),
            running: Rc::default(),
            drainer: Rc::default(),
        })
    }
}

impl TransactionRuntime for Microtasks {
    fn enqueue_microtask(&self, task: Box<dyn FnOnce()>) {
        self.queue.borrow_mut().push_back(task);
        if self.running.replace(true) {
            return;
        }
        let (queue, running, slot) = (
            self.queue.clone(),
            self.running.clone(),
            self.drainer.clone(),
        );
        let handle = self.runtime.spawn_local(Box::pin(async move {
            loop {
                let next = queue.borrow_mut().pop_front();
                match next {
                    Some(task) => task(),
                    None => break,
                }
            }
            running.set(false);
            drop(slot.borrow_mut().take());
        }));
        // An inline host has already drained; keep the handle only while it runs.
        if self.running.get() {
            *self.drainer.borrow_mut() = Some(handle);
        }
    }
    fn now_millis(&self) -> f64 {
        self.runtime.now_ms()
    }
    fn auth_token_encoding(&self) -> AuthTokenEncoding {
        AuthTokenEncoding::TextEncoder
    }
}

pub(crate) fn native_to_js(
    runtime: &Rc<dyn ExchangeRuntime>,
    value: NativePromise<AccountValue>,
) -> Js {
    let promise = Promise::pending(runtime);
    let settle = promise.clone();
    value.subscribe(move |result| {
        settle.settle(match result {
            Ok(v) => Ok(to_js(&v)),
            Err(e) => Err(to_thrown(e)),
        })
    });
    Js::Promise(promise)
}

/// Await any JS value (promises adopt; other values settle) as a native promise.
pub(crate) fn js_to_native(
    runtime: &Rc<dyn ExchangeRuntime>,
    microtasks: Rc<dyn TransactionRuntime>,
    value: Js,
) -> NativePromise<AccountValue> {
    let (promise, resolver) = NativePromise::pending(microtasks);
    let wait = safe_cex_exchange::js_runtime::promise::await_js(runtime, value);
    Promise::spawn(
        runtime,
        Box::pin(async move {
            resolver.settle(match wait.await {
                Ok(v) => Ok(to_account(&v)),
                Err(e) => Err(to_signing(e)),
            });
            Ok(Js::Undefined)
        }),
    );
    promise
}
