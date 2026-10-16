//! Signer method bodies over current receiver members and original crypto imports.
use crate::key_runtime::{KeyFuture, KeyHost, signer_is_wasm_error};
use std::{
    cell::RefCell,
    rc::Rc,
    task::{Poll, Waker},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HashKind {
    CreateOrder,
    CancelOrder,
    Transfer,
    Withdraw,
}
/// Method Call is receiver-bound; original key/hash/crypto imports retain throws and values.
pub trait SigningHost: KeyHost {
    fn set_property(
        &self,
        receiver: &Self::Value,
        key: &str,
        value: Self::Value,
    ) -> Result<(), Self::Error>;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn boolean(&self, value: bool) -> Self::Value;
    fn text(&self, units: Vec<u16>) -> Self::Value;
    fn number(&self, value: f64) -> Self::Value;
    fn object(&self, fields: Vec<(&'static str, Self::Value)>) -> Self::Value;
    fn call_method(
        &self,
        receiver: &Self::Value,
        member: &str,
        args: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn ensure_wasm_initialized(&self) -> Result<KeyFuture<(), Self::Error>, Self::Error>;
    fn normalize_private_key(&self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn hash_transaction(
        &self,
        kind: HashKind,
        tx: &Self::Value,
        chain: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn hash_auth_token(
        &self,
        account: &Self::Value,
        api_key: &Self::Value,
        deadline: &Self::Value,
        chain: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn poseidon2_hash(
        &self,
        api: &Self::Api,
        message: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn schnorr_sign(
        &self,
        api: &Self::Api,
        hash: &Self::Value,
        key: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn schnorr_verify(
        &self,
        api: &Self::Api,
        hash: &Self::Value,
        public_key: &Self::Value,
        signature: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn remainder_16(&self, value: &Self::Value) -> Result<f64, Self::Error>;
    fn less_than(&self, value: &Self::Value, bound: f64) -> Result<bool, Self::Error>;
    fn greater_than(&self, value: &Self::Value, bound: f64) -> Result<bool, Self::Error>;
    fn template_string(&self, value: &Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn signer_error(&self, message: Vec<u16>, cause: Option<Self::Error>) -> Self::Error;
}
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn completed<T: 'static, E: 'static>(result: Result<T, E>) -> KeyFuture<T, E> {
    Box::pin(async move { result })
}
fn retained<H: SigningHost + 'static, T: Clone + 'static>(
    h: Rc<H>,
    future: KeyFuture<T, H::Error>,
) -> KeyFuture<T, H::Error> {
    let slot = Rc::new(RefCell::new(None));
    let waiters: Rc<RefCell<Vec<Waker>>> = Rc::new(RefCell::new(vec![]));
    let produced = slot.clone();
    let waking = waiters.clone();
    h.spawn(Box::pin(async move {
        *produced.borrow_mut() = Some(future.await);
        for w in waking.take() {
            w.wake();
        }
    }));
    Box::pin(std::future::poll_fn(move |cx| {
        if let Some(result) = slot.borrow().as_ref() {
            return Poll::Ready(result.clone());
        }
        let mut w = waiters.borrow_mut();
        if !w.iter().any(|w| w.will_wake(cx.waker())) {
            w.push(cx.waker().clone());
        }
        Poll::Pending
    }))
}
async fn continuation() {
    let mut yielded = false;
    std::future::poll_fn(move |cx| {
        if yielded {
            Poll::Ready(())
        } else {
            yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    })
    .await;
}
fn resolve<H: SigningHost + 'static>(
    h: Rc<H>,
    result: Result<H::Value, H::Error>,
) -> KeyFuture<H::Value, H::Error> {
    match result {
        Ok(value) => {
            let promise = h.promise_resolve(value);
            retained(h, promise)
        }
        Err(error) => completed(Err(error)),
    }
}
pub fn initialize<H: SigningHost + 'static>(
    h: Rc<H>,
    receiver: H::Value,
) -> KeyFuture<(), H::Error> {
    match h.property(&receiver, "initialized") {
        Err(e) => return completed(Err(e)),
        Ok(v) if h.truthy(&v) => return completed(Ok(())),
        Ok(_) => {}
    }
    let initialized = match h.ensure_wasm_initialized() {
        Ok(promise) => promise,
        Err(error) => {
            return completed(Err(
                h.signer_error(units("Failed to initialize signer"), Some(error))
            ));
        }
    };
    retained(
        h.clone(),
        Box::pin(async move {
            let result = async {
                let initialized = initialized.await;
                continuation().await;
                initialized?;
                h.set_property(&receiver, "initialized", h.boolean(true))
            }
            .await;
            result.map_err(|e| h.signer_error(units("Failed to initialize signer"), Some(e)))
        }),
    )
}
pub fn ensure_initialized<H: SigningHost>(h: &H, receiver: &H::Value) -> Result<(), H::Error> {
    if !h.truthy(&h.property(receiver, "initialized")?) || !h.is_ready()? {
        return Err(h.signer_error(
            units("Signer not initialized. Call initialize() first."),
            None,
        ));
    }
    Ok(())
}
fn clean_message<H: SigningHost>(h: &H, message: H::Value) -> Result<H::Value, H::Error> {
    let starts = h.call_method(&message, "startsWith", vec![h.text(units("0x"))])?;
    let clean = if h.truthy(&starts) {
        h.call_method(&message, "slice", vec![h.number(2.0)])?
    } else {
        message
    };
    if h.remainder_16(&h.property(&clean, "length")?)? != 0.0 {
        let mut message = units("Message must be a multiple of 8 bytes (16 hex chars). Got ");
        message.extend(h.template_string(&h.property(&clean, "length")?)?);
        message.extend(units(" characters."));
        return Err(h.signer_error(message, None));
    }
    Ok(clean)
}
fn wasm_result<H: SigningHost>(
    h: &H,
    result: H::Value,
    prefix: &str,
) -> Result<H::Value, H::Error> {
    if signer_is_wasm_error(h, &result)? {
        let mut message = units(prefix);
        message.extend(h.template_string(&h.property(&result, "error")?)?);
        return Err(h.signer_error(message, None));
    }
    Ok(result)
}
pub fn sign_message<H: SigningHost + 'static>(
    h: Rc<H>,
    receiver: H::Value,
    message: H::Value,
    key: H::Value,
) -> KeyFuture<H::Value, H::Error> {
    let result = (|| {
        h.call_method(&receiver, "ensureInitialized", vec![])?;
        let normalized = h.normalize_private_key(&key)?;
        let clean = clean_message(h.as_ref(), message)?;
        let api = h.get_crypto_api()?;
        let hashed = wasm_result(
            h.as_ref(),
            h.poseidon2_hash(&api, &clean)?,
            "Failed to hash message: ",
        )?;
        let signed = wasm_result(
            h.as_ref(),
            h.schnorr_sign(&api, &hashed, &normalized)?,
            "Failed to sign message: ",
        )?;
        Ok(h.object(vec![("txHash", hashed), ("signature", signed)]))
    })();
    resolve(h, result)
}
fn sign_transaction<H: SigningHost + 'static>(
    h: Rc<H>,
    receiver: H::Value,
    tx: H::Value,
    key: H::Value,
    chain: H::Value,
    kind: HashKind,
) -> KeyFuture<H::Value, H::Error> {
    let result = (|| {
        h.call_method(&receiver, "ensureInitialized", vec![])?;
        let message = h.hash_transaction(kind, &tx, &chain)?;
        h.call_method(&receiver, "signMessage", vec![message, key])
    })();
    resolve(h, result)
}
pub fn sign_create_order<H: SigningHost + 'static>(
    h: Rc<H>,
    receiver: H::Value,
    tx: H::Value,
    key: H::Value,
    chain: H::Value,
) -> KeyFuture<H::Value, H::Error> {
    sign_transaction(h, receiver, tx, key, chain, HashKind::CreateOrder)
}
pub fn sign_cancel_order<H: SigningHost + 'static>(
    h: Rc<H>,
    receiver: H::Value,
    tx: H::Value,
    key: H::Value,
    chain: H::Value,
) -> KeyFuture<H::Value, H::Error> {
    sign_transaction(h, receiver, tx, key, chain, HashKind::CancelOrder)
}
pub fn sign_transfer<H: SigningHost + 'static>(
    h: Rc<H>,
    receiver: H::Value,
    tx: H::Value,
    key: H::Value,
    chain: H::Value,
) -> KeyFuture<H::Value, H::Error> {
    sign_transaction(h, receiver, tx, key, chain, HashKind::Transfer)
}
pub fn sign_withdraw<H: SigningHost + 'static>(
    h: Rc<H>,
    receiver: H::Value,
    tx: H::Value,
    key: H::Value,
    chain: H::Value,
) -> KeyFuture<H::Value, H::Error> {
    sign_transaction(h, receiver, tx, key, chain, HashKind::Withdraw)
}
pub fn create_auth_token<H: SigningHost + 'static>(
    h: Rc<H>,
    receiver: H::Value,
    account: H::Value,
    api_key: H::Value,
    deadline: H::Value,
    key: H::Value,
    chain: H::Value,
) -> KeyFuture<H::Value, H::Error> {
    let prepared = (|| {
        h.call_method(&receiver, "ensureInitialized", vec![])?;
        if h.less_than(&account, 0.0)? || h.greater_than(&account, 255.0)? {
            let mut message = units("Account index must be 0-255, got ");
            message.extend(h.template_string(&account)?);
            return Err(h.signer_error(message, None));
        }
        if h.less_than(&api_key, 0.0)? || h.greater_than(&api_key, 255.0)? {
            let mut message = units("API key index must be 0-255, got ");
            message.extend(h.template_string(&api_key)?);
            return Err(h.signer_error(message, None));
        }
        let message = h.hash_auth_token(&account, &api_key, &deadline, &chain)?;
        let signed = h.call_method(&receiver, "signMessage", vec![message.clone(), key])?;
        let promise = h.promise_resolve(signed);
        Ok((message, promise))
    })();
    let (message, promise) = match prepared {
        Ok(v) => v,
        Err(e) => return completed(Err(e)),
    };
    retained(
        h.clone(),
        Box::pin(async move {
            let signed = promise.await;
            continuation().await;
            let signed = signed?;
            let tx_hash = h.property(&signed, "txHash")?;
            let signature = h.property(&signed, "signature")?;
            let token = h.object(vec![
                ("token", signature),
                ("message", message),
                ("messageHash", tx_hash),
                ("accountIndex", account),
                ("apiKeyIndex", api_key),
                ("deadline", deadline),
            ]);
            h.promise_resolve(token).await
        }),
    )
}
pub fn verify_signature<H: SigningHost + 'static>(
    h: Rc<H>,
    receiver: H::Value,
    message: H::Value,
    public_key: H::Value,
    signature: H::Value,
) -> KeyFuture<H::Value, H::Error> {
    let result = (|| {
        h.call_method(&receiver, "ensureInitialized", vec![])?;
        let clean = clean_message(h.as_ref(), message)?;
        let api = h.get_crypto_api()?;
        let hash = wasm_result(
            h.as_ref(),
            h.poseidon2_hash(&api, &clean)?,
            "Failed to hash message: ",
        )?;
        wasm_result(
            h.as_ref(),
            h.schnorr_verify(&api, &hash, &public_key, &signature)?,
            "Failed to verify signature: ",
        )
    })();
    resolve(h, result)
}
