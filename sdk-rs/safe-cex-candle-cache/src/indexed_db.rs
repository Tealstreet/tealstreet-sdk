use crate::{
    CacheRuntime, CandleCacheBackend, CandleCacheChunkAccessOptions, Chunks, LoadedChunks,
    LocalFuture, types::validate_chunk_size,
};
use futures_channel::oneshot;
use futures_util::future::{FutureExt, Shared};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use web_sys::{IdbDatabase, IdbRequest, IdbTransaction, IdbTransactionMode};

#[derive(Clone)]
pub struct IndexedDBCandleCacheBackendOptions {
    pub db_name: String,
    pub db_version: u32,
    pub store_name: String,
    pub max_age_days: f64,
    pub report_error: Option<Rc<dyn Fn(String)>>,
}
impl Default for IndexedDBCandleCacheBackendOptions {
    fn default() -> Self {
        Self {
            db_name: "tealstreet-candles".into(),
            db_version: 1,
            store_name: "chunks".into(),
            max_age_days: 7.,
            report_error: None,
        }
    }
}
type OpenDatabase = Shared<LocalFuture<'static, Result<IdbDatabase, String>>>;
#[derive(Clone)]
pub struct IndexedDBCandleCacheBackend {
    options: IndexedDBCandleCacheBackendOptions,
    runtime: CacheRuntime,
    unavailable: Rc<Cell<bool>>,
    database: Rc<RefCell<Option<OpenDatabase>>>,
}
pub(super) fn error(value: JsValue) -> String {
    format!("IndexedDB: {value:?}")
}
// Abort inside the active cursor callback, before its recovered error escapes.
fn cursor_failure(transaction: &IdbTransaction, write: bool, failure: String) -> String {
    if write && let Err(abort) = transaction.abort() {
        format!("{failure}; cursor rollback failed: {}", error(abort))
    } else {
        failure
    }
}
struct RequestHandlers {
    request: IdbRequest,
    _success: Closure<dyn FnMut(web_sys::Event)>,
    _failure: Closure<dyn FnMut(web_sys::Event)>,
}
impl Drop for RequestHandlers {
    fn drop(&mut self) {
        self.request.set_onsuccess(None);
        self.request.set_onerror(None);
    }
}
struct TransactionHandlers {
    transaction: IdbTransaction,
    _done: Closure<dyn FnMut(web_sys::Event)>,
    _fail: Closure<dyn FnMut(web_sys::Event)>,
}
impl Drop for TransactionHandlers {
    fn drop(&mut self) {
        self.transaction.set_oncomplete(None);
        self.transaction.set_onerror(None);
        self.transaction.set_onabort(None);
    }
}
struct UpgradeHandler(web_sys::IdbOpenDbRequest);
impl Drop for UpgradeHandler {
    fn drop(&mut self) {
        self.0.set_onupgradeneeded(None);
    }
}
fn request_result(request: IdbRequest) -> LocalFuture<'static, Result<JsValue, String>> {
    request_result_with(request, Box::new(Ok))
}
fn request_result_with(
    request: IdbRequest,
    consume: Box<dyn Fn(JsValue) -> Result<JsValue, String>>,
) -> LocalFuture<'static, Result<JsValue, String>> {
    let (sender, receiver) = oneshot::channel();
    let sender = Rc::new(RefCell::new(Some(sender)));
    let success_sender = sender.clone();
    let success_request = request.clone();
    let success = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
        if let Some(sender) = success_sender.borrow_mut().take() {
            let _ = sender.send(success_request.result().map_err(error).and_then(&consume));
        }
    });
    let failure = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
        if let Some(sender) = sender.borrow_mut().take() {
            let _ = sender.send(Err("IndexedDB request failed".into()));
        }
    });
    request.set_onsuccess(Some(success.as_ref().unchecked_ref()));
    request.set_onerror(Some(failure.as_ref().unchecked_ref()));
    let handlers = RequestHandlers {
        request: request.clone(),
        _success: success,
        _failure: failure,
    };
    Box::pin(async move {
        let _handlers = handlers;
        let result = receiver
            .await
            .map_err(|_| "IndexedDB request cancelled".to_string())?;
        request.set_onsuccess(None);
        request.set_onerror(None);
        drop(_handlers);
        result
    })
}
fn transaction_done(
    transaction: IdbTransaction,
    watch_abort: bool,
) -> LocalFuture<'static, Result<(), String>> {
    let (sender, receiver) = oneshot::channel();
    let sender = Rc::new(RefCell::new(Some(sender)));
    let done_sender = sender.clone();
    let done = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
        if let Some(sender) = done_sender.borrow_mut().take() {
            let _ = sender.send(Ok(()));
        }
    });
    let fail = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
        if let Some(sender) = sender.borrow_mut().take() {
            let _ = sender.send(Err("IndexedDB transaction failed or aborted".into()));
        }
    });
    transaction.set_oncomplete(Some(done.as_ref().unchecked_ref()));
    transaction.set_onerror(Some(fail.as_ref().unchecked_ref()));
    if watch_abort {
        transaction.set_onabort(Some(fail.as_ref().unchecked_ref()));
    }
    let handlers = TransactionHandlers {
        transaction: transaction.clone(),
        _done: done,
        _fail: fail,
    };
    Box::pin(async move {
        let _handlers = handlers;
        let result = receiver
            .await
            .map_err(|_| "IndexedDB transaction cancelled".to_string())?;
        transaction.set_oncomplete(None);
        transaction.set_onerror(None);
        transaction.set_onabort(None);
        drop(_handlers);
        result
    })
}
impl IndexedDBCandleCacheBackend {
    pub fn new(options: IndexedDBCandleCacheBackendOptions, runtime: CacheRuntime) -> Self {
        Self {
            options,
            runtime,
            unavailable: Rc::new(Cell::new(false)),
            database: Rc::new(RefCell::new(None)),
        }
    }
    // CHOSEN: null short-circuits here; TS enters open, double-reports and latches.
    // A throwing accessor rejects the TS pre-try guard without reporting/latching;
    // this Reflect guard instead returns unavailable without rejecting.
    fn available(&self) -> bool {
        !self.unavailable.get()
            && js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("indexedDB"))
                .is_ok_and(|value| !value.is_undefined() && !value.is_null())
    }
    fn handle_error(&self, error: String) {
        if let Some(report) = &self.options.report_error {
            report(error);
        }
    }
    async fn open(&self) -> Result<IdbDatabase, String> {
        if self.unavailable.get() {
            return Err("IndexedDB unavailable".into());
        }
        let cached = self.database.borrow().clone();
        if let Some(database) = cached {
            return database.await;
        }
        let options = self.options.clone();
        let runtime = self.runtime.clone();
        let unavailable = self.unavailable.clone();
        let database = Rc::downgrade(&self.database);
        let future: LocalFuture<'static, Result<IdbDatabase, String>> = Box::pin(async move {
            let result = async {
                let factory: web_sys::IdbFactory =
                    js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("indexedDB"))
                        .map_err(error)?
                        .dyn_into()
                        .map_err(error)?;
                let request = factory
                    .open_with_u32(&options.db_name, options.db_version)
                    .map_err(error)?;
                let upgrade_request = request.clone();
                let store_name = options.store_name.clone();
                let upgrade_error = Rc::new(RefCell::new(None));
                let failed_upgrade = upgrade_error.clone();
                let upgrade = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
                    if let Ok(value) = upgrade_request.result() {
                        let db: IdbDatabase = value.unchecked_into();
                        if !db.object_store_names().contains(&store_name)
                            && let Err(failure) = db.create_object_store(&store_name)
                        {
                            failed_upgrade.replace(Some(error(failure)));
                            // A caught JS exception no longer aborts versionchange
                            // automatically: abort explicitly, then reject/latch open.
                            if let Some(transaction) = upgrade_request.transaction()
                                && let Err(abort_error) = transaction.abort()
                            {
                                failed_upgrade.replace(Some(error(abort_error)));
                            }
                        }
                    }
                });
                request.set_onupgradeneeded(Some(upgrade.as_ref().unchecked_ref()));
                let _upgrade_handler = UpgradeHandler(request.clone());
                let result = request_result(request.clone().unchecked_into()).await;
                request.set_onupgradeneeded(None);
                drop(upgrade);
                if let Some(failure) = upgrade_error.borrow_mut().take() {
                    return Err(failure);
                }
                result.map(|value| value.unchecked_into::<IdbDatabase>())
            }
            .await;
            if let Err(error) = &result {
                // ts-bug: IndexedDBCandleCacheBackend.open reports again in the public caller catch.
                if let Some(report) = &options.report_error {
                    report(error.clone());
                }
                unavailable.set(true);
                if let Some(database) = database.upgrade() {
                    database.borrow_mut().take();
                }
            } else {
                let Some(database) = database.upgrade() else {
                    return result;
                };
                let eviction = Self {
                    options: options.clone(),
                    runtime: runtime.clone(),
                    unavailable: unavailable.clone(),
                    database,
                };
                let sleep = (runtime.sleep)(5000.);
                (runtime.spawn)(Box::pin(async move {
                    sleep.await;
                    eviction.evict_old_chunks().await;
                }));
            }
            result
        });
        let future = future.shared();
        self.database.replace(Some(future.clone()));
        future.await
    }
    async fn records(
        &self,
        prefix: Option<String>,
        delete: bool,
        cutoff: Option<f64>,
        records: Rc<RefCell<Vec<(String, JsValue)>>>,
    ) -> Result<(), String> {
        let db = self.open().await?;
        let tx = db
            .transaction_with_str_and_mode(
                &self.options.store_name,
                if delete {
                    IdbTransactionMode::Readwrite
                } else {
                    IdbTransactionMode::Readonly
                },
            )
            .map_err(error)?;
        // ts-bug: clear/persist ignore abort-only events; eviction handles them.
        let completion = delete.then(|| transaction_done(tx.clone(), cutoff.is_some()));
        let store = tx.object_store(&self.options.store_name).map_err(error)?;
        let request = if let Some(prefix) = prefix {
            let range = web_sys::IdbKeyRange::bound(
                &JsValue::from_str(&prefix),
                &JsValue::from_str(&format!("{prefix}\u{ffff}")),
            )
            .map_err(error)?;
            store.open_cursor_with_range(&range).map_err(error)?
        } else {
            store.open_cursor().map_err(error)?
        };
        let (sender, receiver) = oneshot::channel();
        let sender = Rc::new(RefCell::new(Some(sender)));
        let cursor_sender = sender.clone();
        let cursor_records = records.clone();
        let cursor_request = request.clone();
        let cursor_transaction = tx.clone();
        let failure_transaction = tx.clone();
        let success = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            let result = (|| -> Result<bool, String> {
                let value = cursor_request.result().map_err(error)?;
                if value.is_null() {
                    return Ok(true);
                }
                let cursor: web_sys::IdbCursorWithValue = value.unchecked_into();
                let key = cursor
                    .key()
                    .map_err(error)?
                    .as_string()
                    .ok_or("non-string IndexedDB key")?;
                let value = cursor.value().map_err(error)?;
                let old = cutoff.is_none_or(|cutoff| {
                    js_sys::Reflect::get(&value, &JsValue::from_str("lastAccessed"))
                        .ok()
                        .and_then(|v| v.as_f64())
                        .is_some_and(|v| v < cutoff)
                });
                if delete && old {
                    cursor.delete().map_err(error)?;
                } else if !delete {
                    cursor_records.borrow_mut().push((key, value));
                }
                cursor.continue_().map_err(error)?;
                Ok(false)
            })();
            match result {
                Ok(false) => {}
                Ok(true) => {
                    if let Some(sender) = cursor_sender.borrow_mut().take() {
                        let _ = sender.send(Ok(()));
                    }
                }
                Err(error) => {
                    // ts-bug: TS cursor callback exceptions leave its Promise pending.
                    // Reads retain their prefix; writes must roll back before reporting.
                    let error = cursor_failure(&cursor_transaction, delete, error);
                    if let Some(sender) = cursor_sender.borrow_mut().take() {
                        let _ = sender.send(Err(error));
                    }
                }
            }
        });
        let failure = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            let error = cursor_failure(
                &failure_transaction,
                delete,
                "IndexedDB cursor failed".into(),
            );
            if let Some(sender) = sender.borrow_mut().take() {
                let _ = sender.send(Err(error));
            }
        });
        request.set_onsuccess(Some(success.as_ref().unchecked_ref()));
        request.set_onerror(Some(failure.as_ref().unchecked_ref()));
        let _handlers = RequestHandlers {
            request: request.clone(),
            _success: success,
            _failure: failure,
        };
        let result = receiver
            .await
            .map_err(|_| "IndexedDB cursor cancelled".to_string())?;
        request.set_onsuccess(None);
        request.set_onerror(None);
        drop(_handlers);
        result?;
        if let Some(completion) = completion {
            completion.await?;
        }
        Ok(())
    }
    async fn evict_old_chunks(&self) {
        if let Err(e) = self
            .records(
                None,
                true,
                Some((self.runtime.now_ms)() - self.options.max_age_days * 86400000.),
                Rc::default(),
            )
            .await
        {
            self.handle_error(e);
        }
    }
    fn sparse(
        value: JsValue,
        options: CandleCacheChunkAccessOptions,
    ) -> Result<Vec<Option<safe_cex_types::Candle>>, String> {
        validate_chunk_size(options.chunk_size)?;
        if options.chunk_size == 0 {
            return Ok(Vec::new());
        }
        let candles = js_sys::Reflect::get(&value, &JsValue::from_str("candles")).map_err(error)?;
        if !js_sys::Array::is_array(&candles) {
            return Err("invalid candles record".into());
        }
        let candles: js_sys::Array = candles.unchecked_into();
        let mut sparse = vec![None; options.chunk_size];
        for value in candles.iter() {
            let Some(candle) = crate::codec::decode_js_candle(&value)? else {
                continue;
            };
            let index = (candle.timestamp / 1000. / options.interval_seconds).floor();
            if index.is_finite() {
                sparse[(index as i64).rem_euclid(options.chunk_size as i64) as usize] =
                    Some(candle);
            }
        }
        Ok(sparse)
    }
    pub fn load_all_chunks(
        &self,
        key: String,
        options: CandleCacheChunkAccessOptions,
    ) -> Option<LocalFuture<'static, Result<LoadedChunks, String>>> {
        if !self.available() {
            return Some(Box::pin(async { Ok(LoadedChunks::default()) }));
        }
        let backend = self.clone();
        Some(Box::pin(async move {
            let mut chunks = Chunks::new();
            let mut refused_keys = Vec::new();
            let records = Rc::new(RefCell::new(Vec::new()));
            let prefix = format!("{key}::");
            let result = backend
                .records(Some(prefix.clone()), false, None, records.clone())
                .await;
            // Retain the successful cursor prefix even if a later request fails.
            for (key, value) in std::mem::take(&mut *records.borrow_mut()) {
                let Some(suffix) = key.strip_prefix(&prefix) else {
                    continue;
                };
                let suffix = match crate::backends::parse_chunk_key(suffix) {
                    Ok(Some(index)) => index,
                    Ok(None) => continue,
                    Err(()) => {
                        refused_keys.push(key);
                        continue;
                    }
                };
                {
                    match Self::sparse(value, options) {
                        Ok(chunk) => {
                            chunks.insert(suffix, chunk);
                        }
                        Err(e) => {
                            backend.handle_error(e);
                            break;
                        }
                    }
                }
            }
            if let Err(e) = result {
                backend.handle_error(e);
            }
            Ok(LoadedChunks {
                chunks,
                refused_keys,
            })
        }))
    }
    pub fn load_chunks(
        &self,
        key: String,
        keys: Vec<i64>,
        options: CandleCacheChunkAccessOptions,
    ) -> Option<LocalFuture<'static, Result<Chunks, String>>> {
        if !self.available() {
            return Some(Box::pin(async { Ok(Chunks::new()) }));
        }
        let backend = self.clone();
        Some(Box::pin(async move {
            let chunks = Rc::new(RefCell::new(Chunks::new()));
            let result = async {
                if keys.is_empty() {
                    return Ok(());
                }
                let db = backend.open().await?;
                let tx = db
                    .transaction_with_str(&backend.options.store_name)
                    .map_err(error)?;
                let store = tx
                    .object_store(&backend.options.store_name)
                    .map_err(error)?;
                let requests = keys
                    .into_iter()
                    .map(|chunk| {
                        let chunks = chunks.clone();
                        store
                            .get(&JsValue::from_str(&format!("{key}::{chunk}")))
                            .map(|r| {
                                request_result_with(
                                    r,
                                    Box::new(move |value| {
                                        if !value.is_undefined() {
                                            chunks.borrow_mut().insert(
                                                chunk,
                                                Self::sparse(value.clone(), options)?,
                                            );
                                        }
                                        Ok(value)
                                    }),
                                )
                            })
                            .map_err(error)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                // ts-bug: IndexedDBCandleCacheBackend.loadChunks missing candles hangs TS onsuccess.
                // CORRECTED: errors settle with a report;
                // TS throws in onsuccess and its Promise remains pending forever.
                // Each success handler inserts independently, like TS Promise.all.
                futures_util::future::try_join_all(requests).await?;
                Ok::<(), String>(())
            }
            .await;
            if let Err(e) = result {
                backend.handle_error(e);
            }
            let chunks = chunks.borrow().clone();
            Ok(chunks)
        }))
    }
    pub fn persist_chunks(
        &self,
        key: String,
        chunks: Chunks,
        _: CandleCacheChunkAccessOptions,
    ) -> LocalFuture<'static, Result<(), String>> {
        if !self.available() {
            return Box::pin(async { Ok(()) });
        }
        let backend = self.clone();
        Box::pin(async move {
            let result = async {
                let db = backend.open().await?;
                let tx = db
                    .transaction_with_str_and_mode(
                        &backend.options.store_name,
                        IdbTransactionMode::Readwrite,
                    )
                    .map_err(error)?;
                // ts-bug: TS persist ignores an abort-only event and remains pending.
                let completion = transaction_done(tx.clone(), false);
                let store = tx
                    .object_store(&backend.options.store_name)
                    .map_err(error)?;
                let now = (backend.runtime.now_ms)();
                for (chunk, candles) in chunks {
                    let candles: Vec<_> = candles.into_iter().flatten().collect();
                    if candles.is_empty() {
                        continue;
                    }
                    let value = crate::codec::encode_js_record(candles, now).map_err(error)?;
                    store
                        .put_with_key(&value, &JsValue::from_str(&format!("{key}::{chunk}")))
                        .map_err(error)?;
                }
                completion.await
            }
            .await;
            if let Err(e) = result {
                backend.handle_error(e);
            }
            Ok(())
        })
    }
    pub fn clear(&self, key: Option<String>) -> Option<LocalFuture<'static, Result<(), String>>> {
        if !self.available() {
            return Some(Box::pin(async { Ok(()) }));
        }
        let backend = self.clone();
        Some(Box::pin(async move {
            let result = backend
                .records(
                    key.filter(|v| !v.is_empty()).map(|v| format!("{v}::")),
                    true,
                    None,
                    Rc::default(),
                )
                .await;
            if let Err(e) = result {
                backend.handle_error(e);
            }
            Ok(())
        }))
    }
}
impl CandleCacheBackend for IndexedDBCandleCacheBackend {
    fn load_all_chunks(
        &self,
        k: String,
        o: CandleCacheChunkAccessOptions,
    ) -> Option<LocalFuture<'static, Result<LoadedChunks, String>>> {
        self.load_all_chunks(k, o)
    }
    fn load_chunks(
        &self,
        k: String,
        keys: Vec<i64>,
        o: CandleCacheChunkAccessOptions,
    ) -> Option<LocalFuture<'static, Result<Chunks, String>>> {
        self.load_chunks(k, keys, o)
    }
    fn persist_chunks(
        &self,
        k: String,
        c: Chunks,
        o: CandleCacheChunkAccessOptions,
    ) -> LocalFuture<'static, Result<(), String>> {
        self.persist_chunks(k, c, o)
    }
    fn clear(&self, k: Option<String>) -> Option<LocalFuture<'static, Result<(), String>>> {
        self.clear(k)
    }
}
