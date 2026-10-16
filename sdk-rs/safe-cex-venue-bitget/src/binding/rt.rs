//! Shared runtime handle passed to every ported body as its host.
//! `this` is the receiver the body runs on (exchange or socket object).
use super::{
    promise::{JsFuture, Promise},
    value::{Js, Thrown},
};
use safe_cex_exchange::{
    exchange::{BaseExchange, Exchange},
    runtime::ExchangeRuntime,
    websocket::WsRuntime,
};
use safe_cex_store::Store;
use std::{
    cell::{OnceCell, RefCell},
    rc::{Rc, Weak},
};

/// Wire-level operations the host supplies; everything above them is ported code.
pub trait BitgetImports {
    /// Send one HTTP request whose config already passed the request interceptors.
    /// Resolves the Axios response object `{ data, status, headers, config }`.
    fn send(&self, config: Js) -> JsFuture<'static>;
}

pub(crate) struct Shared {
    pub runtime: Rc<dyn WsRuntime>,
    pub venue: RefCell<Option<Weak<dyn Exchange>>>,
    pub imports: Rc<dyn BitgetImports>,
    pub store: Rc<dyn Store>,
    pub base: OnceCell<Rc<BaseExchange>>,
    pub globals: Js,
    pub exchange: RefCell<Js>,
    pub variant: Variant,
    pub name: safe_cex_types::ExchangeName,
    pub options: safe_cex_types::ExchangeOptions,
    pub external_public_data: Option<ExternalPublicDataFactory>,
    pub timers: RefCell<Vec<(Js, Box<dyn safe_cex_exchange::runtime::ExchangeTask>)>>,
    pub socket_ids: std::cell::Cell<u64>,
    pub socket_cores: RefCell<Vec<(Js, Rc<super::sockets::SocketCore>)>>,
}

pub type ExternalPublicDataFactory = Rc<
    dyn Fn(&safe_cex_types::ExchangeOptionsExternalPublicData) -> Rc<dyn safe_cex_exchange::external::ExternalPublicData>,
>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Variant {
    Classic,
    Inverse,
    Usdc,
}

#[derive(Clone)]
pub(crate) struct Rt {
    pub shared: Rc<Shared>,
    pub this: Js,
}

impl Rt {
    pub(crate) fn with_this(&self, this: Js) -> Rt {
        Rt {
            shared: self.shared.clone(),
            this,
        }
    }
    pub(crate) fn base(&self) -> &Rc<BaseExchange> {
        self.shared.base.get().expect("BaseExchange is constructed first")
    }
    pub(crate) fn exchange_runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.shared.runtime.clone()
    }
    pub(crate) fn global(&self, name: &str) -> Result<Js, Thrown> {
        let globals = &self.shared.globals;
        if globals.has_own(name) {
            return globals.get(name);
        }
        let mut parts = name.split('.');
        let first = parts.next().unwrap_or_default();
        if !globals.has_own(first) {
            return Err(Thrown::error("ReferenceError", &format!("{first} is not defined")));
        }
        let mut value = globals.get(first)?;
        for part in parts {
            value = super::hosts::assembly::bound_member(&value, part)?;
        }
        Ok(value)
    }
    /// Resolve `a.b.c` from `this`; a first segment `this` lacks resolves from globals.
    pub(crate) fn path(&self, path: &str) -> Result<Js, Thrown> {
        let mut parts = path.split('.');
        let first = parts.next().unwrap_or_default();
        let mut value = if self.this.has_own(first) || self.this.get(first).is_ok_and(|v| !v.is_undefined()) {
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
    pub(crate) fn owner_and_member(&self, path: &str) -> Result<(Js, Js), Thrown> {
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
    pub(crate) fn set_path(&self, path: &str, value: Js) -> Result<(), Thrown> {
        match path.rsplit_once('.') {
            Some((owner, member)) => self.path(owner)?.set(member, value),
            None => self.this.set(path, value),
        }
    }
    /// Call a member of `this` by dotted path with its owner as receiver.
    pub(crate) fn call_path(&self, path: &str, args: Vec<Js>) -> Result<Js, Thrown> {
        let (owner, member) = self.owner_and_member(path)?;
        if !member.is_callable() {
            return Err(Thrown::type_error(&format!("this.{path} is not a function")));
        }
        member.call(owner, args)
    }
    pub(crate) fn spawn(&self, body: JsFuture<'static>) -> Js {
        Js::Promise(Promise::spawn(&self.exchange_runtime(), body))
    }
    pub(crate) fn now(&self) -> f64 {
        self.shared.runtime.now_ms()
    }
    /// setTimeout: returns a timer handle object; clearTimeout drops the task.
    pub(crate) fn set_timeout(&self, callback: Rc<dyn Fn() -> Result<(), Thrown>>, delay_ms: f64) -> Js {
        let runtime = self.shared.runtime.clone();
        let delay = if delay_ms.is_finite() && delay_ms > 0.0 { delay_ms as u64 } else { 0 };
        let reporter = runtime.clone();
        let task = runtime.spawn_local(Box::pin(async move {
            reporter.sleep(delay).await.ok();
            if let Err(error) = callback() {
                reporter.report_exception(&error.to_exchange_error(), &serde_json::json!({"source": "setTimeout"}));
            }
        }));
        let handle = Js::object_with_class("Timeout", vec![]);
        self.shared.timers.borrow_mut().push((handle.clone(), task));
        handle
    }
    pub(crate) fn clear_timeout(&self, handle: &Js) {
        self.shared.timers.borrow_mut().retain(|(h, _)| !h.same_reference(handle));
    }
    pub(crate) fn report(&self, error: &Thrown, source: &str) {
        self.shared
            .runtime
            .report_exception(&error.to_exchange_error(), &serde_json::json!({"source": source}));
    }
}

