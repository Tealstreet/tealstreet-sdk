use crate::{CacheRuntime, CandleCacheKeyValueStore, KeyValueCandleCacheBackend, LocalFuture};
use redis::aio::MultiplexedConnection;
use std::{
    path::PathBuf,
    rc::Rc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub fn local_runtime() -> CacheRuntime {
    CacheRuntime {
        now_ms: Rc::new(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as f64
        }),
        sleep: Rc::new(|ms| {
            Box::pin(async move {
                let delay = if ms.is_finite() {
                    ms.clamp(0., u32::MAX as f64) as u64
                } else {
                    0
                };
                tokio::time::sleep(Duration::from_millis(delay)).await
            })
        }),
        spawn: Rc::new(|task| {
            tokio::task::spawn_local(task);
        }),
    }
}

pub struct RedisCandleCacheBackend(KeyValueCandleCacheBackend);
impl RedisCandleCacheBackend {
    pub async fn connect(
        url: &str,
        namespace: Option<String>,
        runtime: CacheRuntime,
        best_effort: bool,
    ) -> Result<Self, String> {
        let client = redis::Client::open(url).map_err(|e| e.to_string())?;
        Ok(Self(KeyValueCandleCacheBackend::new(
            Rc::new(RedisStore {
                client,
                best_effort,
                reads: Rc::new(tokio::sync::Mutex::new(None)),
                bulk: Rc::new(tokio::sync::Mutex::new(None)),
            }),
            namespace,
            runtime,
        )))
    }
}
#[derive(Clone)]
struct RedisStore {
    client: redis::Client,
    best_effort: bool,
    reads: Rc<tokio::sync::Mutex<Option<MultiplexedConnection>>>,
    bulk: Rc<tokio::sync::Mutex<Option<MultiplexedConnection>>>,
}
impl RedisStore {
    async fn connection(&self, bulk: bool) -> Result<MultiplexedConnection, String> {
        let timeout = Duration::from_secs(if bulk { 15 } else { 1 });
        let mut slot = if bulk {
            self.bulk.lock().await
        } else {
            self.reads.lock().await
        };
        if slot.is_none() {
            let mut connection =
                tokio::time::timeout(timeout, self.client.get_multiplexed_async_connection())
                    .await
                    .map_err(|_| "Redis connection timed out".to_string())?
                    .map_err(|e| e.to_string())?;
            connection.set_response_timeout(timeout);
            *slot = Some(connection);
        }
        Ok(slot.as_ref().unwrap().clone())
    }
}
impl RedisStore {
    async fn run<T: redis::FromRedisValue>(
        &self,
        bulk: bool,
        command: redis::Cmd,
    ) -> Result<T, String> {
        // The lane budget starts before waiting for connection initialization, like TS enqueue time.
        let operation = async {
            let mut connection = self.connection(bulk).await?;
            command
                .query_async(&mut connection)
                .await
                .map_err(|e| e.to_string())
        };
        let result =
            tokio::time::timeout(Duration::from_secs(if bulk { 15 } else { 1 }), operation)
                .await
                .unwrap_or_else(|_| Err("Redis command timed out".into()));
        if result.is_err() {
            let slot = if bulk { &self.bulk } else { &self.reads };
            if let Ok(mut cached) = slot.try_lock() {
                *cached = None;
            }
        }
        result
    }
}
impl CandleCacheKeyValueStore for RedisStore {
    fn get(&self, key: String) -> LocalFuture<'static, Result<Option<String>, String>> {
        let store = self.clone();
        Box::pin(async move {
            match store.run(false, redis::cmd("GET").arg(key).take()).await {
                Err(_) if store.best_effort => {
                    eprintln!("Redis candle read failed; falling back to upstream");
                    Ok(None)
                }
                result => result,
            }
        })
    }
    fn set(&self, key: String, value: String) -> LocalFuture<'static, Result<(), String>> {
        let store = self.clone();
        Box::pin(async move {
            let result = store
                .run(true, redis::cmd("SET").arg(key).arg(value).take())
                .await;
            if result.is_err() && store.best_effort {
                eprintln!("Redis candle write failed");
                Ok(())
            } else {
                result
            }
        })
    }
    fn delete(&self, key: String) -> LocalFuture<'static, Result<(), String>> {
        let store = self.clone();
        Box::pin(async move {
            let result = store
                .run::<i64>(true, redis::cmd("DEL").arg(key).take())
                .await
                .map(|_| ());
            if result.is_err() && store.best_effort {
                eprintln!("Redis candle delete failed");
                Ok(())
            } else {
                result
            }
        })
    }
    fn list_keys(&self, prefix: String) -> LocalFuture<'static, Result<Vec<String>, String>> {
        let store = self.clone();
        Box::pin(async move {
            let mut cursor = 0_u64;
            let mut keys = Vec::new();
            loop {
                let page: Result<(u64, Vec<String>), String> = store
                    .run(
                        true,
                        redis::cmd("SCAN")
                            .arg(cursor)
                            .arg("MATCH")
                            .arg(format!("{prefix}*"))
                            .arg("COUNT")
                            .arg(500)
                            .take(),
                    )
                    .await;
                let (next, page) = match page {
                    Err(_) if store.best_effort => {
                        eprintln!("Redis candle scan failed");
                        return Ok(keys);
                    }
                    result => result?,
                };
                keys.extend(page);
                cursor = next;
                if cursor == 0 {
                    return Ok(keys);
                }
            }
        })
    }
}

pub struct FilesystemCandleCacheBackend(KeyValueCandleCacheBackend);
impl FilesystemCandleCacheBackend {
    pub async fn open(directory: PathBuf, namespace: Option<String>) -> Result<Self, String> {
        tokio::fs::create_dir_all(&directory)
            .await
            .map_err(|e| e.to_string())?;
        Ok(Self(KeyValueCandleCacheBackend::new(
            Rc::new(FileStore { directory }),
            namespace,
            local_runtime(),
        )))
    }
}
struct FileStore {
    directory: PathBuf,
}
// Hex encoding is reversible and prevents cache keys from escaping their storage directory.
fn filename(key: &str) -> String {
    key.as_bytes().iter().map(|b| format!("{b:02x}")).collect()
}
fn decode(name: &str) -> Option<String> {
    if !name.len().is_multiple_of(2) {
        return None;
    }
    let bytes = (0..name.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(name.get(i..i + 2)?, 16).ok())
        .collect::<Option<Vec<_>>>()?;
    String::from_utf8(bytes).ok()
}
struct StagingFile(PathBuf);
impl Drop for StagingFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
impl CandleCacheKeyValueStore for FileStore {
    fn get(&self, key: String) -> LocalFuture<'static, Result<Option<String>, String>> {
        let path = self.directory.join(filename(&key));
        Box::pin(async move {
            match tokio::fs::read_to_string(path).await {
                Ok(v) => Ok(Some(v)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(e.to_string()),
            }
        })
    }
    fn set(&self, key: String, value: String) -> LocalFuture<'static, Result<(), String>> {
        let directory = self.directory.clone();
        Box::pin(async move {
            // Each write owns a unique staging file; cancellation cannot corrupt an existing chunk.
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let temporary = directory.join(format!(
                ".tmp-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            tokio::task::spawn_blocking(move || {
                let _staging = StagingFile(temporary.clone());
                std::fs::write(&temporary, value)?;
                std::fs::rename(&temporary, directory.join(filename(&key)))
            })
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e: std::io::Error| e.to_string())
        })
    }
    fn delete(&self, key: String) -> LocalFuture<'static, Result<(), String>> {
        let path = self.directory.join(filename(&key));
        Box::pin(async move {
            match tokio::fs::remove_file(path).await {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.to_string()),
            }
        })
    }
    fn list_keys(&self, prefix: String) -> LocalFuture<'static, Result<Vec<String>, String>> {
        let directory = self.directory.clone();
        Box::pin(async move {
            let mut entries = tokio::fs::read_dir(directory)
                .await
                .map_err(|e| e.to_string())?;
            let mut keys = Vec::new();
            while let Some(entry) = entries.next_entry().await.map_err(|e| e.to_string())? {
                if let Some(key) = entry
                    .file_name()
                    .to_str()
                    .and_then(decode)
                    .filter(|k| k.starts_with(&prefix))
                {
                    keys.push(key);
                }
            }
            Ok(keys)
        })
    }
}

impl std::ops::Deref for RedisCandleCacheBackend {
    type Target = KeyValueCandleCacheBackend;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::Deref for FilesystemCandleCacheBackend {
    type Target = KeyValueCandleCacheBackend;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl crate::CandleCacheBackend for RedisCandleCacheBackend {
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

impl crate::CandleCacheBackend for FilesystemCandleCacheBackend {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::task::{Context, Poll, Waker};
    #[test]
    fn read_deadline_includes_waiting_for_connection_initialization() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(tokio::task::LocalSet::new().run_until(async {
            tokio::time::pause();
            let store = RedisStore {
                client: redis::Client::open("redis://127.0.0.1:1").unwrap(),
                best_effort: false,
                reads: Rc::new(tokio::sync::Mutex::new(None)),
                bulk: Rc::new(tokio::sync::Mutex::new(None)),
            };
            let _initialization = store.reads.lock().await;
            let mut read =
                Box::pin(store.run::<Option<String>>(false, redis::cmd("GET").arg("k").take()));
            let mut bulk =
                Box::pin(store.run::<()>(true, redis::cmd("SET").arg("k").arg("v").take()));
            let _bulk_initialization = store.bulk.lock().await;
            assert!(
                read.as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
            assert!(
                bulk.as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
            tokio::time::advance(Duration::from_secs(2)).await;
            assert!(matches!(
                read.as_mut().poll(&mut Context::from_waker(Waker::noop())),
                Poll::Ready(Err(_))
            ));
            assert!(
                bulk.as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
            tokio::time::advance(Duration::from_secs(14)).await;
            assert!(matches!(
                bulk.as_mut().poll(&mut Context::from_waker(Waker::noop())),
                Poll::Ready(Err(_))
            ));
        }));
    }
}
