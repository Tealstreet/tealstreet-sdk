//! Key-manager orchestration at imported Poseidon and ECMAScript primitive boundaries.
use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Poll, Waker},
};
pub type KeyFuture<T, E> = Pin<Box<dyn Future<Output = Result<T, E>>>>;
#[derive(Clone)]
pub struct GeneratedKeys<V> {
    pub private_key: V,
    pub public_key: V,
}
/// Host imports are crypto exports, JS typeof/null/in/get/ToString and error construction.
pub trait KeyHost {
    type Value: Clone + 'static;
    type Error: Clone + 'static;
    type Api: 'static;
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()>>>);
    fn promise_resolve(&self, value: Self::Value) -> KeyFuture<Self::Value, Self::Error>;
    fn is_ready(&self) -> Result<bool, Self::Error>;
    fn init_wasm(&self) -> Result<KeyFuture<(), Self::Error>, Self::Error>;
    fn get_crypto_api(&self) -> Result<Self::Api, Self::Error>;
    fn generate_keys(&self, api: &Self::Api) -> Result<Self::Value, Self::Error>;
    fn derive_public_key(&self, api: &Self::Api, key: &[u16]) -> Result<Self::Value, Self::Error>;
    fn is_object(&self, value: &Self::Value) -> bool;
    fn is_null(&self, value: &Self::Value) -> bool;
    fn has_error(&self, value: &Self::Value) -> Result<bool, Self::Error>;
    fn property(&self, value: &Self::Value, key: &str) -> Result<Self::Value, Self::Error>;
    fn string(&self, value: &Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn key_error(&self, message: Vec<u16>, cause: Option<Self::Error>) -> Self::Error;
}
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn completed<T: 'static, E: 'static>(result: Result<T, E>) -> KeyFuture<T, E> {
    Box::pin(async move { result })
}
fn retained<H: KeyHost + 'static, T: Clone + 'static>(
    host: Rc<H>,
    future: KeyFuture<T, H::Error>,
) -> KeyFuture<T, H::Error> {
    let slot: Rc<RefCell<Option<Result<T, H::Error>>>> = Rc::new(RefCell::new(None));
    let waiters: Rc<RefCell<Vec<Waker>>> = Rc::new(RefCell::new(Vec::new()));
    let produced = slot.clone();
    let waking = waiters.clone();
    host.spawn(Box::pin(async move {
        *produced.borrow_mut() = Some(future.await);
        for w in waking.take() {
            w.wake();
        }
    }));
    Box::pin(std::future::poll_fn(move |cx| {
        if let Some(result) = slot.borrow().as_ref() {
            return Poll::Ready(result.clone());
        }
        let mut listeners = waiters.borrow_mut();
        if !listeners.iter().any(|w| w.will_wake(cx.waker())) {
            listeners.push(cx.waker().clone())
        }
        Poll::Pending
    }))
}
fn valid_key(key: &[u16]) -> bool {
    key.len() == 80 && key.iter().all(|u| matches!(*u,48..=57|65..=70|97..=102))
}
fn yield_continuation() -> impl Future<Output = ()> {
    let mut yielded = false;
    std::future::poll_fn(move |cx| {
        if yielded {
            std::task::Poll::Ready(())
        } else {
            yielded = true;
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        }
    })
}
// Imports execute eagerly, as they do before the source async function's first await.
pub fn ensure_wasm_initialized<H: KeyHost + 'static>(host: Rc<H>) -> KeyFuture<(), H::Error> {
    match host.is_ready() {
        Err(error) => return completed(Err(error)),
        Ok(true) => return completed(Ok(())),
        Ok(false) => {}
    }
    let init = match host.init_wasm() {
        Ok(future) => future,
        Err(error) => {
            return completed(Err(host.key_error(
                units("Failed to initialize WASM crypto module"),
                Some(error),
            )));
        }
    };
    retained(
        host.clone(),
        Box::pin(async move {
            let result = init.await;
            yield_continuation().await;
            if let Err(error) = result {
                return Err(host.key_error(
                    units("Failed to initialize WASM crypto module"),
                    Some(error),
                ));
            }
            if !host.is_ready()? {
                return Err(host.key_error(
                    units("WASM initialization completed but module is not ready"),
                    None,
                ));
            }
            Ok(())
        }),
    )
}
pub fn key_is_wasm_error<H: KeyHost>(host: &H, value: &H::Value) -> Result<bool, H::Error> {
    if !host.is_object(value) || host.is_null(value) {
        return Ok(false);
    }
    host.has_error(value)
}
pub fn signer_is_wasm_error<H: KeyHost>(host: &H, value: &H::Value) -> Result<bool, H::Error> {
    if !host.is_object(value) || host.is_null(value) {
        return Ok(false);
    }
    host.has_error(value)
}
pub fn generate_key_pair<H: KeyHost + 'static>(
    host: Rc<H>,
) -> KeyFuture<GeneratedKeys<H::Value>, H::Error> {
    let initialized = ensure_wasm_initialized(host.clone());
    retained(
        host.clone(),
        Box::pin(async move {
            initialized.await?;
            yield_continuation().await;
            let api = host.get_crypto_api()?;
            let result = host.generate_keys(&api)?;
            if key_is_wasm_error(host.as_ref(), &result)? {
                let value = host.property(&result, "error")?;
                let mut message = units("Key generation failed: ");
                message.extend(host.string(&value)?);
                return Err(host.key_error(message, None));
            }
            let private_key = host.property(&result, "privateKey")?;
            let public_key = host.property(&result, "publicKey")?;
            Ok(GeneratedKeys {
                private_key,
                public_key,
            })
        }),
    )
}
pub fn derive_public_key<H: KeyHost + 'static>(
    host: Rc<H>,
    private_key: Vec<u16>,
) -> KeyFuture<H::Value, H::Error> {
    let initialized = ensure_wasm_initialized(host.clone());
    retained(
        host.clone(),
        Box::pin(async move {
            initialized.await?;
            yield_continuation().await;
            if !valid_key(&private_key) {
                return Err(host.key_error(
                    units(&format!(
                        "Invalid private key format. Expected 80 hex characters, got {}",
                        private_key.len()
                    )),
                    None,
                ));
            }
            let api = host.get_crypto_api()?;
            let result = host.derive_public_key(&api, &private_key)?;
            if key_is_wasm_error(host.as_ref(), &result)? {
                let value = host.property(&result, "error")?;
                let mut message = units("Public key derivation failed: ");
                message.extend(host.string(&value)?);
                return Err(host.key_error(message, None));
            }
            host.promise_resolve(result).await
        }),
    )
}
