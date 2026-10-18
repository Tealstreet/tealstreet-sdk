//! lighter.signer.ts, the lighter-sdk TransactionClient and the poseidon-crypto
//! index, assembled on the realm globals over the already-ported native bodies.
mod adapter;
mod codec;
mod poseidon;
#[cfg(test)]
mod tests;
mod transaction;

use super::{
    realm::{Realm, Shared},
    value::{to_signing, to_thrown},
};
use crate::transaction_promise::{NativePromise, TransactionRuntime};
use safe_cex_exchange::{
    js_runtime::{
        promise::{Promise, await_js},
        value::{Js, ObjectData, Thrown, error_object},
    },
    runtime::ExchangeRuntime,
};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

pub(crate) use poseidon::Entropy;

pub(crate) fn install(realm: &Realm) -> Result<(), Thrown> {
    install_with_entropy(realm, poseidon::os_entropy())
}

/// The Schnorr nonce source is the only randomness the signer consumes.
pub(crate) fn install_with_entropy(realm: &Realm, entropy: Entropy) -> Result<(), Thrown> {
    let host = Host::new(realm);
    poseidon::install(&host, entropy)?;
    transaction::install(&host)?;
    adapter::install(&host)
}

/// What every signer callable needs, without keeping the realm alive.
#[derive(Clone)]
pub(super) struct Host {
    shared: Weak<Shared>,
    runtime: Rc<dyn ExchangeRuntime>,
    microtasks: Rc<dyn TransactionRuntime>,
}

impl Host {
    fn new(realm: &Realm) -> Self {
        Self {
            shared: Rc::downgrade(&realm.0),
            runtime: realm.exchange_runtime(),
            microtasks: realm.0.extra.microtasks.clone(),
        }
    }
    fn global(&self, name: &str) -> Result<Js, Thrown> {
        let shared = self
            .shared
            .upgrade()
            .ok_or_else(|| Thrown::error("Error", "Lighter scope was dropped"))?;
        Realm(shared).global(name)
    }
    fn define_global(&self, name: &str, value: Js) -> Result<(), Thrown> {
        let shared = self
            .shared
            .upgrade()
            .ok_or_else(|| Thrown::error("Error", "Lighter scope was dropped"))?;
        shared.globals.define(name, value)
    }
    /// A JS promise settled from a native one, mapping the fulfilled value.
    fn promise<T: 'static>(
        &self,
        native: NativePromise<T>,
        map: impl FnOnce(&T) -> Result<Js, Thrown> + 'static,
    ) -> Js {
        let promise = Promise::pending(&self.runtime);
        let settle = promise.clone();
        native.subscribe(move |result| {
            settle.settle(match result {
                Ok(value) => map(&value),
                Err(error) => Err(to_thrown(error)),
            })
        });
        Js::Promise(promise)
    }
    fn rejected(&self, error: Thrown) -> Js {
        Js::Promise(Promise::settled(&self.runtime, Err(error)))
    }
    /// `await value` as a native promise, mapping the fulfilled value.
    fn awaited<T: 'static>(
        &self,
        value: Js,
        map: impl FnOnce(Js) -> Result<T, Thrown> + 'static,
    ) -> NativePromise<T> {
        let (promise, resolver) = NativePromise::pending(self.microtasks.clone());
        let wait = await_js(&self.runtime, value);
        Promise::spawn(
            &self.runtime,
            Box::pin(async move {
                resolver.settle(wait.await.and_then(map).map_err(to_signing));
                Ok(Js::Undefined)
            }),
        );
        promise
    }
}

/// `new Error(message)` with a UTF-16 message.
fn error(message: &[u16]) -> Thrown {
    let value = error_object("Error", "");
    let _ = value.define("message", Js::utf16(message));
    Thrown::new(value)
}

/// Native state behind JS receivers, keyed by object identity, never keeping
/// the receiver alive.
pub(super) struct NativeSlots<T>(RefCell<Vec<(Weak<ObjectData>, Rc<T>)>>);

impl<T> Default for NativeSlots<T> {
    fn default() -> Self {
        Self(RefCell::default())
    }
}

impl<T> NativeSlots<T> {
    fn bind(&self, receiver: &Js, native: Rc<T>) -> Result<(), Thrown> {
        let Js::Object(object) = receiver else {
            return Err(Thrown::type_error("native state needs an object receiver"));
        };
        let mut slots = self.0.borrow_mut();
        slots.retain(|(key, _)| key.strong_count() > 0);
        slots.push((Rc::downgrade(object), native));
        Ok(())
    }
    fn get(&self, receiver: &Js) -> Option<Rc<T>> {
        let Js::Object(object) = receiver else {
            return None;
        };
        self.0
            .borrow()
            .iter()
            .find(|(key, _)| std::ptr::eq(key.as_ptr(), Rc::as_ptr(object)))
            .map(|(_, native)| native.clone())
    }
}
