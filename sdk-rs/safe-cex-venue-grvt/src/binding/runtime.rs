//! GRVT's existing value-level bodies over the shared JavaScript runtime.
use crate::{
    builder_workflow::{JsPromise, JsResult},
    session_host::{Callback, SessionHost},
};
use safe_cex_exchange::js_runtime::{
    promise::{Promise, await_js},
    scope::{Scope, ScopeShared},
    value::{Js, Thrown, arg},
};
use std::rc::Rc;

pub(super) struct Extra {
    pub(super) builder_error_proto: Js,
    pub(super) nonce_ids: std::cell::RefCell<Option<crate::composition::NonceIds>>,
    pub(super) nonce_times: std::cell::RefCell<Option<crate::composition::NonceTimes>>,
    pub(super) public: std::cell::RefCell<Vec<Rc<super::public::Public>>>,
    pub(super) private: std::cell::RefCell<Vec<Rc<super::private::Private>>>,
    pub(super) name: safe_cex_types::ExchangeName,
    pub(super) options: safe_cex_types::ExchangeOptions,
    pub(super) external_public_data:
        Option<safe_cex_exchange::js_runtime::base_exchange::ExternalPublicDataFactory>,
}
pub(super) type Shared = ScopeShared<Extra>;

#[derive(Clone)]
pub(super) struct Host(pub(super) Scope<Extra>);

pub(super) fn value<T>(result: Result<T, Thrown>) -> JsResult<Js, T> {
    result.map_err(|error| error.value)
}
pub(super) fn thrown<T>(result: JsResult<Js, T>) -> Result<T, Thrown> {
    result.map_err(Thrown::new)
}

impl Host {
    pub(super) fn from_shared(shared: Rc<Shared>, this: Js) -> Self {
        Self(Scope { shared, this })
    }
    pub(super) fn spawn(&self, task: JsPromise<Js>) -> Js {
        self.0.spawn(Box::pin(async move { thrown(task.await) }))
    }
    pub(super) fn global_call(&self, name: &str, args: Vec<Js>) -> JsResult<Js> {
        value(value(self.0.global(name))?.call(Js::Undefined, args))
    }
    pub(super) fn log(&self, message: &str, severity: &str) -> Result<(), Thrown> {
        self.0
            .this
            .call_method("log", vec![Js::str(message), Js::str(severity)])?;
        Ok(())
    }
}

impl SessionHost for Host {
    type Value = Js;
    fn get(&self, object: &Js, key: &str) -> JsResult<Js> {
        value(object.get(key))
    }
    fn set(&self, object: &Js, key: &str, field: Js) -> JsResult<Js, ()> {
        value(object.set(key, field))
    }
    fn call(&self, function: Js, receiver: Js, args: Vec<Js>) -> JsResult<Js> {
        value(function.call(receiver, args))
    }
    fn function(&self, callback: Callback<Js>) -> Js {
        Js::function("grvtCallback", move |_, args| {
            thrown(callback(arg(&args, 0)))
        })
    }
    fn object(&self, fields: Vec<(&str, Js)>) -> Js {
        Js::object(fields)
    }
    fn define(&self, object: &Js, key: &str, field: Js) -> JsResult<Js, ()> {
        value(object.define(key, field))
    }
    fn array(&self, values: Vec<Js>) -> Js {
        Js::array(values)
    }
    fn spread(&self, target: &Js, source: Js) -> JsResult<Js, ()> {
        if !source.is_nullish() {
            for (key, field) in value(source.own_entries())? {
                value(target.define_key(&key, field))?;
            }
        }
        Ok(())
    }
    fn string(&self, text: &str) -> Js {
        Js::str(text)
    }
    fn number(&self, number: f64) -> Js {
        Js::Number(number)
    }
    fn boolean(&self, boolean: bool) -> Js {
        Js::Bool(boolean)
    }
    fn undefined(&self) -> Js {
        Js::Undefined
    }
    fn null(&self) -> Js {
        Js::Null
    }
    fn truthy(&self, v: &Js) -> bool {
        v.truthy()
    }
    fn nullish(&self, v: &Js) -> bool {
        v.is_nullish()
    }
    fn is_array(&self, v: &Js) -> bool {
        v.is_array()
    }
    fn is_string(&self, v: &Js) -> bool {
        matches!(v, Js::String(_))
    }
    fn is_function(&self, v: &Js) -> bool {
        v.is_callable()
    }
    fn equals_number(&self, v: &Js, number: f64) -> JsResult<Js, bool> {
        Ok(v.strict_equals(&Js::Number(number)))
    }
    fn template(&self, parts: Vec<Js>) -> JsResult<Js> {
        let mut units = vec![];
        for part in parts {
            units.extend(value(part.to_js_string())?.iter().copied());
        }
        Ok(Js::utf16(&units))
    }
    fn error(&self, message: Js) -> JsResult<Js> {
        let error = safe_cex_exchange::js_runtime::value::error_object("Error", "");
        value(error.define("message", Js::String(value(message.to_js_string())?)))?;
        Ok(error)
    }
    fn reject(&self, error: Js) -> Js {
        Js::Promise(Promise::settled(
            &self.0.exchange_runtime(),
            Err(Thrown::new(error)),
        ))
    }
    fn start(&self, task: JsPromise<Js>) -> JsPromise<Js> {
        // Promise::spawn first-polls now and owns the continuation independently
        // of the returned waiter, matching the GRVT eager host contract.
        let promise = self.spawn(task);
        self.await_value(promise)
    }
    fn promise(&self, task: JsPromise<Js>) -> Js {
        self.spawn(task)
    }
    fn await_value(&self, v: Js) -> JsPromise<Js> {
        {
            let pending = await_js(&self.0.exchange_runtime(), v);
            Box::pin(async move { value(pending.await) })
        }
    }
}

#[cfg(test)]
mod accepted_clone_tests {
    use super::*;
    #[allow(dead_code)]
    mod fixture {
        use crate as safe_cex_venue_grvt;
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/binding_harness.rs"
        ));
    }

    #[test]
    fn host_clone_retains_shared_scope_and_receiver_identity() {
        let runtime = Rc::new(fixture::Runtime::default());
        let http = Rc::new(fixture::Http::default());
        let store = Rc::new(safe_cex_store::store::DefaultStore::new());
        let shared = Rc::new(ScopeShared::new(
            runtime,
            http,
            store,
            Js::object(vec![]),
            Extra {
                builder_error_proto: Js::object(vec![]),
                nonce_ids: Default::default(),
                nonce_times: Default::default(),
                public: Default::default(),
                private: Default::default(),
                name: safe_cex_types::ExchangeName::Grvt,
                options: fixture::options(),
                external_public_data: None,
            },
        ));
        let original = Host::from_shared(shared, Js::object(vec![("marker", Js::Number(7.0))]));
        let cloned = original.clone();
        assert!(Rc::ptr_eq(&original.0.shared, &cloned.0.shared));
        assert!(original.0.this.same_value_zero(&cloned.0.this));
        cloned.0.this.set("marker", Js::Number(-0.0)).unwrap();
        let Js::Number(marker) = original.0.this.get("marker").unwrap() else {
            panic!("GRVT Host clone lost its receiver");
        };
        assert_eq!(marker.to_bits(), (-0.0_f64).to_bits());
        cloned
            .0
            .shared
            .extra
            .nonce_ids
            .borrow_mut()
            .replace(Default::default());
        assert!(original.0.shared.extra.nonce_ids.borrow().is_some());
    }
}
