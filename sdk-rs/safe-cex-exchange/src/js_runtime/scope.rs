//! The host handle a venue's ported bodies run on: shared runtime state plus
//! `this`, the receiver object (exchange or socket). `X` holds venue-only fields.
use super::{
    client::JsHttpHost,
    promise::{JsFuture, Promise},
    sockets::JsSocketContext,
    value::{Js, ObjectData, Prop, Thrown, utf16},
};
use crate::{
    exchange::BaseExchange,
    runtime::{ExchangeRuntime, ExchangeTask},
    websocket::WsRuntime,
};
use safe_cex_store::Store;
use std::{
    cell::{OnceCell, RefCell},
    collections::HashMap,
    rc::Rc,
};

pub struct ScopeShared<X> {
    pub runtime: Rc<dyn WsRuntime>,
    pub http: Rc<dyn JsHttpHost>,
    pub sockets: Rc<JsSocketContext>,
    pub store: Rc<dyn Store>,
    pub base: OnceCell<Rc<BaseExchange>>,
    pub globals: Js,
    pub exchange: RefCell<Js>,
    pub timers: Timers,
    pub extra: X,
    native_modules: RefCell<HashMap<&'static str, Rc<ObjectData>>>,
}

impl<X> ScopeShared<X> {
    pub fn new(
        runtime: Rc<dyn WsRuntime>,
        http: Rc<dyn JsHttpHost>,
        store: Rc<dyn Store>,
        globals: Js,
        extra: X,
    ) -> Self {
        let sockets = JsSocketContext::new(runtime.clone());
        *sockets.http.borrow_mut() = Some(http.clone());
        let mut native_modules = HashMap::new();
        if let Js::Object(data) = &globals {
            let props = data.props.borrow();
            for name in super::library::NATIVE_MODULE_ROOTS {
                if let Some(
                    Prop::Data(Js::Object(module))
                    | Prop::DataDescriptor {
                        value: Js::Object(module),
                        ..
                    },
                ) = props.get(&utf16(name))
                {
                    native_modules.insert(name, module.clone());
                }
            }
        }
        Self {
            sockets,
            runtime,
            http,
            store,
            base: OnceCell::new(),
            globals,
            exchange: RefCell::new(Js::Undefined),
            timers: RefCell::default(),
            extra,
            native_modules: RefCell::new(native_modules),
        }
    }
    pub(crate) fn native_module(&self, name: &str) -> Option<Js> {
        self.native_modules
            .borrow()
            .get(name)
            .cloned()
            .map(Js::Object)
    }
    pub(crate) fn retain_native_module(&self, name: &'static str, module: Rc<ObjectData>) {
        self.native_modules
            .borrow_mut()
            .entry(name)
            .or_insert(module);
    }
}

pub struct Scope<X> {
    pub shared: Rc<ScopeShared<X>>,
    pub this: Js,
}

impl<X> Clone for Scope<X> {
    fn clone(&self) -> Self {
        Self {
            shared: self.shared.clone(),
            this: self.this.clone(),
        }
    }
}

/// `owner.member` read as a callable that keeps `owner` as its receiver.
pub fn bound_member(owner: &Js, name: &str) -> Result<Js, Thrown> {
    let value = owner.get(name)?;
    if !value.is_callable() {
        return Ok(value);
    }
    let owner = owner.clone();
    let f = value.clone();
    Ok(Js::function(name, move |_, a| f.call(owner.clone(), a)))
}

impl<X> Scope<X> {
    pub fn with_this(&self, this: Js) -> Self {
        Self {
            shared: self.shared.clone(),
            this,
        }
    }
    pub fn base(&self) -> &Rc<BaseExchange> {
        self.shared
            .base
            .get()
            .expect("BaseExchange is constructed first")
    }
    pub fn exchange_runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.shared.runtime.clone()
    }
    /// A module-scope name; a dotted name binds each member to its owner.
    pub fn global(&self, name: &str) -> Result<Js, Thrown> {
        let globals = &self.shared.globals;
        if globals.has_own(name) {
            return globals.get(name);
        }
        let mut parts = name.split('.');
        let first = parts.next().unwrap_or_default();
        if !globals.has_own(first) {
            return Err(Thrown::error(
                "ReferenceError",
                &format!("{first} is not defined"),
            ));
        }
        let mut value = globals.get(first)?;
        for part in parts {
            value = bound_member(&value, part)?;
        }
        Ok(value)
    }
    /// Resolve `a.b.c` from `this`; a first segment `this` lacks resolves from globals.
    pub fn path(&self, path: &str) -> Result<Js, Thrown> {
        let mut parts = path.split('.');
        let first = parts.next().unwrap_or_default();
        let mut value =
            if self.this.has_own(first) || self.this.get(first).is_ok_and(|v| !v.is_undefined()) {
                self.this.get(first)?
            } else if self.shared.globals.has_own(first) {
                self.shared.globals.get(first)?
            } else {
                self.this.get(first)?
            };
        for part in parts {
            value = value.get(part)?;
        }
        Ok(value)
    }
    /// Resolve the owner and the member of a dotted path (owner is `this` for one segment).
    pub fn owner_and_member(&self, path: &str) -> Result<(Js, Js), Thrown> {
        match path.rsplit_once('.') {
            Some((owner, member)) => {
                let owner = self.path(owner)?;
                let value = owner.get(member)?;
                Ok((owner, value))
            }
            None => {
                let value = self.path(path)?;
                let owner = if self.this.has_own(path) || !self.shared.globals.has_own(path) {
                    self.this.clone()
                } else {
                    Js::Undefined
                };
                Ok((owner, value))
            }
        }
    }
    pub fn set_path(&self, path: &str, value: Js) -> Result<(), Thrown> {
        match path.rsplit_once('.') {
            Some((owner, member)) => self.path(owner)?.set(member, value),
            None => self.this.set(path, value),
        }
    }
    /// Call a member of `this` by dotted path with its owner as receiver.
    pub fn call_path(&self, path: &str, args: Vec<Js>) -> Result<Js, Thrown> {
        let (owner, member) = self.owner_and_member(path)?;
        if !member.is_callable() {
            return Err(Thrown::type_error(&format!(
                "this.{path} is not a function"
            )));
        }
        member.call(owner, args)
    }
    pub fn spawn(&self, body: JsFuture<'static>) -> Js {
        Js::Promise(Promise::spawn(&self.exchange_runtime(), body))
    }
    pub fn now(&self) -> f64 {
        self.shared.runtime.now_ms()
    }
    /// setTimeout: returns a timer handle object; clearTimeout cancels the task.
    /// A fired timer removes its own entry, so handles do not accumulate.
    pub fn set_timeout(&self, callback: Rc<dyn Fn() -> Result<(), Thrown>>, delay_ms: f64) -> Js
    where
        X: 'static,
    {
        let runtime = self.shared.runtime.clone();
        let delay = if delay_ms.is_finite() && delay_ms > 0.0 {
            delay_ms as u64
        } else {
            0
        };
        let handle = Js::object_with_class("Timeout", vec![]);
        let fired = Rc::new(std::cell::Cell::new(false));
        let (shared, own, done) = (Rc::downgrade(&self.shared), handle.clone(), fired.clone());
        let reporter = runtime.clone();
        let deadline = runtime.monotonic_now_ms() + delay as f64;
        let task = super::jobs::for_runtime(&(runtime.clone() as Rc<dyn ExchangeRuntime>)).timer(
            deadline,
            Box::pin(async move {
                let remaining = (deadline - reporter.monotonic_now_ms()).max(0.0).ceil() as u64;
                let slept = if remaining == 0 {
                    Ok(())
                } else {
                    reporter.sleep(remaining).await
                };
                done.set(true);
                if let Some(shared) = shared.upgrade() {
                    drop(take_timers(&shared.timers, &own));
                }
                let outcome = match slept {
                    // A host timer that fails never fires; say so rather than run early.
                    Err(error) => Err((error, "setTimeout sleep")),
                    Ok(()) => callback().map_err(|error| (error.to_exchange_error(), "setTimeout")),
                };
                if let Err((error, source)) = outcome {
                    reporter.report_exception(&error, &serde_json::json!({ "source": source }));
                }
            }),
        );
        // An inline host may already have run the timer: never register a dead task.
        if !fired.get() {
            self.shared.timers.borrow_mut().push((handle.clone(), task));
        }
        handle
    }
    pub fn clear_timeout(&self, handle: &Js) {
        self.shared.sockets.clear_timeout(handle);
        let removed = take_timers(&self.shared.timers, handle);
        // Host cancellation runs after the timer list's borrow is released.
        for (_, task) in removed {
            task.cancel();
        }
    }
    pub fn report(&self, error: &Thrown, source: &str) {
        self.shared.runtime.report_exception(
            &error.to_exchange_error(),
            &serde_json::json!({"source": source}),
        );
    }
}

/// A scope over the assembled exchange receiver, for builtins that spawn work.
pub fn scope_of<X>(weak: &std::rc::Weak<ScopeShared<X>>) -> Result<Scope<X>, Thrown> {
    let shared = weak
        .upgrade()
        .ok_or_else(|| Thrown::error("Error", "exchange runtime disposed"))?;
    let this = shared.exchange.borrow().clone();
    Ok(Scope { shared, this })
}

type Timers = RefCell<Vec<(Js, Box<dyn ExchangeTask>)>>;
fn take_timers(timers: &Timers, handle: &Js) -> Vec<(Js, Box<dyn ExchangeTask>)> {
    let mut timers = timers.borrow_mut();
    let (taken, kept) = std::mem::take(&mut *timers)
        .into_iter()
        .partition(|(h, _)| h.same_reference(handle));
    *timers = kept;
    taken
}
