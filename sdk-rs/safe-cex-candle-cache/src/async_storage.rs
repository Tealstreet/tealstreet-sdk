use crate::LocalFuture;
use crate::{CacheRuntime, CandleCacheKeyValueStore, KeyValueCandleCacheBackend};
use std::rc::Rc;

pub struct ReactNativeAsyncStorageCandleCacheBackend(KeyValueCandleCacheBackend);
impl ReactNativeAsyncStorageCandleCacheBackend {
    pub fn new(
        storage: Rc<dyn CandleCacheKeyValueStore>,
        namespace: Option<String>,
        runtime: CacheRuntime,
    ) -> Self {
        Self(KeyValueCandleCacheBackend::new(storage, namespace, runtime))
    }
}

impl std::ops::Deref for ReactNativeAsyncStorageCandleCacheBackend {
    type Target = KeyValueCandleCacheBackend;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(target_arch = "wasm32")]
impl ReactNativeAsyncStorageCandleCacheBackend {
    pub fn from_js(
        storage: wasm_bindgen::JsValue,
        namespace: Option<String>,
        runtime: CacheRuntime,
    ) -> Self {
        Self::new(Rc::new(JsStorage(storage)), namespace, runtime)
    }
}
#[cfg(target_arch = "wasm32")]
struct JsStorage(wasm_bindgen::JsValue);
#[cfg(target_arch = "wasm32")]
impl JsStorage {
    fn call(
        &self,
        method: &str,
        args: js_sys::Array,
    ) -> crate::LocalFuture<'static, Result<wasm_bindgen::JsValue, String>> {
        use wasm_bindgen::JsCast;
        let result = js_sys::Reflect::get(&self.0, &method.into())
            .and_then(|f| f.dyn_into::<js_sys::Function>())
            .and_then(|f| f.apply(&self.0, &args));
        Box::pin(async move {
            let value = result.map_err(|e| format!("AsyncStorage: {e:?}"))?;
            wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&value))
                .await
                .map_err(|e| format!("AsyncStorage: {e:?}"))
        })
    }
}
#[cfg(target_arch = "wasm32")]
impl CandleCacheKeyValueStore for JsStorage {
    fn get(&self, key: String) -> crate::LocalFuture<'static, Result<Option<String>, String>> {
        let args = js_sys::Array::new();
        args.push(&key.into());
        let result = self.call("getItem", args);
        Box::pin(async move {
            let value = result.await?;
            if value.is_null() || value.is_undefined() {
                Ok(None)
            } else {
                value
                    .as_string()
                    .map(Some)
                    .ok_or_else(|| "AsyncStorage getItem returned a non-string".into())
            }
        })
    }
    fn set(&self, key: String, value: String) -> crate::LocalFuture<'static, Result<(), String>> {
        let args = js_sys::Array::new();
        args.push(&key.into());
        args.push(&value.into());
        let result = self.call("setItem", args);
        Box::pin(async move { result.await.map(|_| ()) })
    }
    fn delete(&self, key: String) -> crate::LocalFuture<'static, Result<(), String>> {
        let args = js_sys::Array::new();
        args.push(&key.into());
        let result = self.call("removeItem", args);
        Box::pin(async move { result.await.map(|_| ()) })
    }
    fn list_keys(
        &self,
        prefix: String,
    ) -> crate::LocalFuture<'static, Result<Vec<String>, String>> {
        let result = self.call("getAllKeys", js_sys::Array::new());
        Box::pin(async move {
            let value = result.await?;
            if !js_sys::Array::is_array(&value) {
                return Err("AsyncStorage getAllKeys returned a non-array".into());
            }
            Ok(js_sys::Array::from(&value)
                .iter()
                .filter_map(|v| v.as_string())
                .filter(|v| v.starts_with(&prefix))
                .collect())
        })
    }
}

impl crate::CandleCacheBackend for ReactNativeAsyncStorageCandleCacheBackend {
    fn load_all_chunks(
        &self,
        key: String,
        options: crate::CandleCacheChunkAccessOptions,
    ) -> Option<LocalFuture<'static, Result<crate::LoadedChunks, String>>> {
        self.0.load_all_chunks(key, options)
    }
    fn load_chunks(
        &self,
        key: String,
        keys: Vec<i64>,
        options: crate::CandleCacheChunkAccessOptions,
    ) -> Option<LocalFuture<'static, Result<crate::Chunks, String>>> {
        self.0.load_chunks(key, keys, options)
    }
    fn persist_chunks(
        &self,
        key: String,
        chunks: crate::Chunks,
        options: crate::CandleCacheChunkAccessOptions,
    ) -> LocalFuture<'static, Result<(), String>> {
        self.0.persist_chunks(key, chunks, options)
    }
    fn clear(&self, key: Option<String>) -> Option<LocalFuture<'static, Result<(), String>>> {
        self.0.clear(key)
    }
}
