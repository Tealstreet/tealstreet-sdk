//! Production adapter to the already ported Poseidon module index and crypto exports.
use crate::key_runtime::{KeyFuture, KeyHost};
use safe_cex_crypto::poseidon_loaders::{CryptoValue, LoaderError, PoseidonCrypto, PoseidonIndex};
use std::{future::Future, pin::Pin, rc::Rc};
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeCryptoValue {
    Undefined,
    Text(Vec<u16>),
    Bool(bool),
    Keys {
        private_key: Vec<u16>,
        public_key: Vec<u16>,
    },
    Error(Vec<u16>),
}
#[derive(Debug)]
pub struct NativeKeyError {
    pub name: Vec<u16>,
    pub message: Vec<u16>,
    pub cause: Option<Rc<NativeKeyError>>,
}
pub trait KeyScheduler {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()>>>);
}
pub struct PoseidonKeyHost {
    pub index: Rc<PoseidonIndex>,
    pub scheduler: Rc<dyn KeyScheduler>,
}
fn fault(error: LoaderError) -> Rc<NativeKeyError> {
    Rc::new(NativeKeyError {
        name: error.name.encode_utf16().collect(),
        message: error.message.encode_utf16().collect(),
        cause: None,
    })
}
fn value(value: CryptoValue) -> NativeCryptoValue {
    match value {
        CryptoValue::Text(s) => NativeCryptoValue::Text(s.encode_utf16().collect()),
        CryptoValue::Bool(b) => NativeCryptoValue::Bool(b),
        CryptoValue::Keys(k) => NativeCryptoValue::Keys {
            private_key: k.private_key.encode_utf16().collect(),
            public_key: k.public_key.encode_utf16().collect(),
        },
        CryptoValue::Error(e) => NativeCryptoValue::Error(e.error.encode_utf16().collect()),
    }
}
impl KeyHost for PoseidonKeyHost {
    type Value = NativeCryptoValue;
    type Error = Rc<NativeKeyError>;
    type Api = Rc<PoseidonCrypto>;
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()>>>) {
        self.scheduler.spawn(future)
    }
    fn promise_resolve(&self, value: Self::Value) -> KeyFuture<Self::Value, Self::Error> {
        Box::pin(async move { Ok(value) })
    }
    fn is_ready(&self) -> Result<bool, Self::Error> {
        Ok(self.index.is_wasm_ready())
    }
    fn init_wasm(&self) -> Result<KeyFuture<(), Self::Error>, Self::Error> {
        let future = self.index.init_wasm();
        Ok(Box::pin(async move { future.await.map_err(fault) }))
    }
    fn get_crypto_api(&self) -> Result<Self::Api, Self::Error> {
        self.index.get_crypto_api().map_err(fault)
    }
    fn generate_keys(&self, api: &Self::Api) -> Result<Self::Value, Self::Error> {
        api.schnorr_generate_keys().map(value).map_err(fault)
    }
    fn derive_public_key(&self, api: &Self::Api, key: &[u16]) -> Result<Self::Value, Self::Error> {
        let key = String::from_utf16(key).map_err(|_| {
            self.key_error("Invalid UTF-16 crypto key".encode_utf16().collect(), None)
        })?;
        api.schnorr_pk_from_sk(key).map(value).map_err(fault)
    }
    fn is_object(&self, value: &Self::Value) -> bool {
        matches!(
            value,
            NativeCryptoValue::Keys { .. } | NativeCryptoValue::Error(_)
        )
    }
    fn is_null(&self, _: &Self::Value) -> bool {
        false
    }
    fn has_error(&self, value: &Self::Value) -> Result<bool, Self::Error> {
        Ok(matches!(value, NativeCryptoValue::Error(_)))
    }
    fn property(&self, value: &Self::Value, key: &str) -> Result<Self::Value, Self::Error> {
        Ok(match (value, key) {
            (NativeCryptoValue::Keys { private_key, .. }, "privateKey") => {
                NativeCryptoValue::Text(private_key.clone())
            }
            (NativeCryptoValue::Keys { public_key, .. }, "publicKey") => {
                NativeCryptoValue::Text(public_key.clone())
            }
            (NativeCryptoValue::Error(message), "error") => {
                NativeCryptoValue::Text(message.clone())
            }
            _ => NativeCryptoValue::Undefined,
        })
    }
    fn string(&self, value: &Self::Value) -> Result<Vec<u16>, Self::Error> {
        Ok(match value {
            NativeCryptoValue::Text(s) => s.clone(),
            NativeCryptoValue::Undefined => "undefined".encode_utf16().collect(),
            NativeCryptoValue::Bool(true) => "true".encode_utf16().collect(),
            NativeCryptoValue::Bool(false) => "false".encode_utf16().collect(),
            _ => "[object Object]".encode_utf16().collect(),
        })
    }
    fn key_error(&self, message: Vec<u16>, cause: Option<Self::Error>) -> Self::Error {
        Rc::new(NativeKeyError {
            name: "KeyManagerError".encode_utf16().collect(),
            message,
            cause,
        })
    }
}
