#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub async fn probe() -> Result<String, JsValue> {
    use safe_cex_candle_cache::{
        CacheRuntime, CandleCacheChunkAccessOptions, Chunks, LocalFuture,
        indexed_db::{IndexedDBCandleCacheBackend, IndexedDBCandleCacheBackendOptions},
    };
    use safe_cex_types::Candle;
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };
    let clock = Rc::new(Cell::new(1_700_000_000_000.));
    let now = clock.clone();
    let tasks = Rc::new(RefCell::new(Vec::<LocalFuture<'static, ()>>::new()));
    let spawn = tasks.clone();
    let delays = Rc::new(RefCell::new(Vec::new()));
    let sleep_delays = delays.clone();
    let runtime = CacheRuntime {
        now_ms: Rc::new(move || now.get()),
        sleep: Rc::new(move |ms| {
            sleep_delays.borrow_mut().push(ms);
            Box::pin(async {})
        }),
        spawn: Rc::new(move |task| spawn.borrow_mut().push(task)),
    };
    let backend = IndexedDBCandleCacheBackend::new(
        IndexedDBCandleCacheBackendOptions {
            db_name: "p3b-browser-proof".into(),
            ..Default::default()
        },
        runtime.clone(),
    );
    let options = CandleCacheChunkAccessOptions {
        chunk_size: 100,
        interval_seconds: 60.,
    };
    backend
        .clear(None)
        .unwrap()
        .await
        .map_err(|e| JsValue::from_str(&e))?;
    let candle = Candle {
        timestamp: 6_000_000.,
        open: 1.,
        high: 3.,
        low: 0.5,
        close: 2.,
        volume: 4.,
        last_side: None,
    };
    let mut sparse = vec![None; 100];
    sparse[0] = Some(candle.clone());
    let chunks = Chunks::from([(1, sparse)]);
    backend
        .persist_chunks("k".into(), chunks.clone(), options)
        .await
        .map_err(|e| JsValue::from_str(&e))?;
    backend
        .persist_chunks("keep".into(), chunks.clone(), options)
        .await
        .map_err(|e| JsValue::from_str(&e))?;
    let fresh = IndexedDBCandleCacheBackend::new(
        IndexedDBCandleCacheBackendOptions {
            db_name: "p3b-browser-proof".into(),
            ..Default::default()
        },
        runtime.clone(),
    );
    assert_eq!(
        fresh
            .load_chunks("k".into(), vec![1], options)
            .unwrap()
            .await
            .unwrap(),
        chunks
    );
    assert_eq!(
        fresh
            .load_all_chunks("k".into(), options)
            .unwrap()
            .await
            .unwrap()
            .chunks,
        chunks
    );
    fresh.clear(Some("k".into())).unwrap().await.unwrap();
    if !(fresh
        .load_all_chunks("k".into(), options)
        .unwrap()
        .await
        .unwrap()
        .chunks
        .is_empty())
    {
        return Err(JsValue::from_str("persistent clear/age expectation failed"));
    }
    assert_eq!(
        fresh
            .load_all_chunks("keep".into(), options)
            .unwrap()
            .await
            .unwrap()
            .chunks,
        chunks
    );
    if *delays.borrow() != vec![5000., 5000.] {
        return Err(JsValue::from_str("IndexedDB eviction delay changed"));
    }
    clock.set(clock.get() + 6. * 86_400_000.);
    let queued = std::mem::take(&mut *tasks.borrow_mut());
    for task in queued {
        task.await;
    }
    if fresh
        .load_all_chunks("keep".into(), options)
        .unwrap()
        .await
        .unwrap()
        .chunks
        != chunks
    {
        return Err(JsValue::from_str("IndexedDB expired a six-day record"));
    }
    clock.set(clock.get() + 2. * 86_400_000.);
    let reopened = IndexedDBCandleCacheBackend::new(
        IndexedDBCandleCacheBackendOptions {
            db_name: "p3b-browser-proof".into(),
            ..Default::default()
        },
        runtime,
    );
    reopened
        .load_all_chunks("keep".into(), options)
        .unwrap()
        .await
        .unwrap();
    let queued = std::mem::take(&mut *tasks.borrow_mut());
    for task in queued {
        task.await;
    }

    if !(fresh
        .load_all_chunks("keep".into(), options)
        .unwrap()
        .await
        .unwrap()
        .chunks
        .is_empty())
    {
        return Err(JsValue::from_str("persistent clear/age expectation failed"));
    }
    fresh.clear(None).unwrap().await.unwrap();
    Ok("IndexedDB: persist/reopen/load-all/selective-clear/age-eviction passed".into())
}
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub async fn async_storage_probe(storage: JsValue) -> Result<String, JsValue> {
    use safe_cex_candle_cache::{
        CacheRuntime, CandleCacheChunkAccessOptions, Chunks,
        async_storage::ReactNativeAsyncStorageCandleCacheBackend,
    };
    use std::rc::Rc;
    let runtime = CacheRuntime {
        now_ms: Rc::new(|| 100.),
        sleep: Rc::new(|_| Box::pin(async {})),
        spawn: Rc::new(wasm_bindgen_futures::spawn_local),
    };
    let backend = ReactNativeAsyncStorageCandleCacheBackend::from_js(
        storage.clone(),
        Some("rn".into()),
        runtime.clone(),
    );
    let options = CandleCacheChunkAccessOptions {
        chunk_size: 100,
        interval_seconds: 60.,
    };
    let candle = safe_cex_types::Candle {
        timestamp: 6_000_000.,
        open: 1.,
        high: 3.,
        low: 0.5,
        close: 2.,
        volume: 4.,
        last_side: None,
    };
    let mut sparse = vec![None; 100];
    sparse[0] = Some(candle);
    let chunks = Chunks::from([(1, sparse)]);
    backend
        .persist_chunks("k".into(), chunks.clone(), options)
        .await
        .unwrap();
    let fresh =
        ReactNativeAsyncStorageCandleCacheBackend::from_js(storage, Some("rn".into()), runtime);
    assert_eq!(
        fresh
            .load_all_chunks("k".into(), options)
            .unwrap()
            .await
            .unwrap()
            .chunks,
        chunks
    );
    fresh.clear(None).unwrap().await.unwrap();
    if !(fresh
        .load_all_chunks("k".into(), options)
        .unwrap()
        .await
        .unwrap()
        .chunks
        .is_empty())
    {
        return Err(JsValue::from_str("persistent clear/age expectation failed"));
    }
    Ok("AsyncStorage: JS promises/persist/reopen/prefix-clear passed".into())
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub async fn cancellation_probe() -> Result<String, JsValue> {
    use safe_cex_candle_cache::{
        CacheRuntime, CandleCacheChunkAccessOptions,
        indexed_db::{IndexedDBCandleCacheBackend, IndexedDBCandleCacheBackendOptions},
    };
    use std::{
        rc::Rc,
        task::{Context, Waker},
    };
    let runtime = CacheRuntime {
        now_ms: Rc::new(|| 0.),
        sleep: Rc::new(|_| Box::pin(std::future::pending())),
        spawn: Rc::new(drop),
    };
    let marker = Rc::new(());
    let weak = Rc::downgrade(&marker);
    let captured = marker.clone();
    drop(marker);
    let backend = IndexedDBCandleCacheBackend::new(
        IndexedDBCandleCacheBackendOptions {
            db_name: "p3b-cancel-open".into(),
            report_error: Some(Rc::new(move |_| {
                let _ = &captured;
            })),
            ..Default::default()
        },
        runtime.clone(),
    );
    let options = CandleCacheChunkAccessOptions {
        chunk_size: 100,
        interval_seconds: 60.,
    };
    let mut read = backend.load_all_chunks("k".into(), options).unwrap();
    assert!(
        read.as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    drop(read);
    drop(backend);
    assert!(weak.upgrade().is_none(), "pending open owns an Rc cycle");
    let backend = IndexedDBCandleCacheBackend::new(
        IndexedDBCandleCacheBackendOptions {
            db_name: "p3b-browser-proof".into(),
            ..Default::default()
        },
        runtime,
    );
    backend
        .load_all_chunks("k".into(), options)
        .unwrap()
        .await
        .unwrap();
    // Open is ready; both request and transaction listeners now exist when the read is cancelled.
    let mut read = backend
        .load_chunks("k".into(), vec![1, 2], options)
        .unwrap();
    assert!(
        read.as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    drop(read);
    drop(backend);
    let wait = js_sys::Promise::new(&mut |resolve, _| {
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 50)
            .unwrap();
    });
    wasm_bindgen_futures::JsFuture::from(wait).await?;
    Ok("IndexedDB: cancelled open/read detach handlers and release owner passed".into())
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub async fn error_path_probe(patch: js_sys::Function) -> Result<String, JsValue> {
    use safe_cex_candle_cache::{
        CacheRuntime, CandleCacheChunkAccessOptions,
        indexed_db::{IndexedDBCandleCacheBackend, IndexedDBCandleCacheBackendOptions},
    };
    use std::{cell::Cell, rc::Rc};
    let reports = Rc::new(Cell::new(0));
    let count = reports.clone();
    let backend = IndexedDBCandleCacheBackend::new(
        IndexedDBCandleCacheBackendOptions {
            db_name: "p3b-error-proof".into(),
            report_error: Some(Rc::new(move |_| count.set(count.get() + 1))),
            ..Default::default()
        },
        CacheRuntime {
            now_ms: Rc::new(|| 0.),
            sleep: Rc::new(|_| Box::pin(std::future::pending())),
            spawn: Rc::new(drop),
        },
    );
    let options = CandleCacheChunkAccessOptions {
        chunk_size: 100,
        interval_seconds: 60.,
    };
    backend
        .load_chunks("fault".into(), vec![1], options)
        .unwrap()
        .await
        .unwrap();
    patch.call1(&JsValue::NULL, &JsValue::from_str("cursor"))?;
    let prefix = backend
        .load_all_chunks("fault".into(), options)
        .unwrap()
        .await
        .unwrap()
        .chunks;
    if prefix.len() != 1 || !prefix.contains_key(&1) {
        return Err(JsValue::from_str("cursor error lost successful prefix"));
    }
    patch.call1(&JsValue::NULL, &JsValue::from_str("throw"))?;
    let prefix = backend
        .load_all_chunks("fault".into(), options)
        .unwrap()
        .await
        .unwrap()
        .chunks;
    if prefix.len() != 1 {
        return Err(JsValue::from_str("callback exception lost cursor prefix"));
    }
    patch.call1(&JsValue::NULL, &JsValue::from_str("get"))?;
    let siblings = backend
        .load_chunks("fault".into(), vec![1, 2], options)
        .unwrap()
        .await
        .unwrap();
    if siblings.len() != 1 || !siblings.contains_key(&2) {
        return Err(JsValue::from_str("GET error lost successful sibling"));
    }
    let before = reports.get();
    patch.call1(&JsValue::NULL, &JsValue::from_str("missing"))?;
    let siblings = backend
        .load_chunks("fault".into(), vec![1, 2], options)
        .unwrap()
        .await
        .unwrap();
    if siblings.len() != 1 || !siblings.contains_key(&2) || reports.get() != before + 1 {
        return Err(JsValue::from_str(
            "missing candles must settle, report once and retain sibling",
        ));
    }
    patch.call1(&JsValue::NULL, &JsValue::from_str("abort"))?;
    let mut persist = backend.persist_chunks("fault".into(), Default::default(), options);
    let mut clear = backend.clear(Some("fault".into())).unwrap();
    futures_util::future::poll_fn(|cx| {
        if persist.as_mut().poll(cx).is_ready() || clear.as_mut().poll(cx).is_ready() {
            return std::task::Poll::Ready(Err(JsValue::from_str(
                "abort-only operation resolved before event",
            )));
        }
        std::task::Poll::Ready(Ok(()))
    })
    .await?;
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        let timer: js_sys::Function =
            js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("setTimeout"))
                .unwrap()
                .unchecked_into();
        timer
            .call2(&JsValue::NULL, &resolve, &JsValue::from_f64(20.))
            .unwrap();
    });
    wasm_bindgen_futures::JsFuture::from(promise).await?;
    futures_util::future::poll_fn(|cx| {
        if persist.as_mut().poll(cx).is_ready() || clear.as_mut().poll(cx).is_ready() {
            return std::task::Poll::Ready(Err(JsValue::from_str(
                "TS abort-only pending bug was changed",
            )));
        }
        std::task::Poll::Ready(Ok(()))
    })
    .await?;
    drop(persist);
    drop(clear);
    patch.call1(&JsValue::NULL, &JsValue::from_str("restore"))?;
    Ok("IndexedDB: cursor prefix/independent GET success/abort-only pending passed".into())
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub async fn upgrade_failure_probe() -> Result<String, JsValue> {
    use safe_cex_candle_cache::{
        CacheRuntime, CandleCacheChunkAccessOptions,
        indexed_db::{IndexedDBCandleCacheBackend, IndexedDBCandleCacheBackendOptions},
    };
    use std::{cell::Cell, rc::Rc};
    let reports = Rc::new(Cell::new(0));
    let count = reports.clone();
    let backend = IndexedDBCandleCacheBackend::new(
        IndexedDBCandleCacheBackendOptions {
            db_name: "p3b-upgrade-failure-proof".into(),
            report_error: Some(Rc::new(move |_| count.set(count.get() + 1))),
            ..Default::default()
        },
        CacheRuntime {
            now_ms: Rc::new(|| 0.),
            sleep: Rc::new(|_| Box::pin(async {})),
            spawn: Rc::new(drop),
        },
    );
    let options = CandleCacheChunkAccessOptions {
        chunk_size: 100,
        interval_seconds: 60.,
    };
    let result = backend
        .load_chunks("k".into(), vec![0], options)
        .unwrap()
        .await
        .map_err(|e| JsValue::from_str(&e))?;
    if !result.is_empty() || reports.get() != 2 {
        return Err(JsValue::from_str(
            "upgrade must reject open and double-report",
        ));
    }
    backend
        .load_chunks("k".into(), vec![0], options)
        .unwrap()
        .await
        .map_err(|e| JsValue::from_str(&e))?;
    if reports.get() != 2 {
        return Err(JsValue::from_str("failed upgrade must latch unavailable"));
    }
    Ok("upgrade failure rejected, double-reported and latched".into())
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub async fn availability_probe(patch: js_sys::Function) -> Result<(), JsValue> {
    use safe_cex_candle_cache::{
        CacheRuntime, CandleCacheChunkAccessOptions,
        indexed_db::{IndexedDBCandleCacheBackend, IndexedDBCandleCacheBackendOptions},
    };
    use std::{cell::Cell, rc::Rc};
    for mode in ["null", "accessor"] {
        let reports = Rc::new(Cell::new(0));
        let count = reports.clone();
        let backend = IndexedDBCandleCacheBackend::new(
            IndexedDBCandleCacheBackendOptions {
                db_name: format!("p3b-rust-availability-{mode}"),
                report_error: Some(Rc::new(move |_| count.set(count.get() + 1))),
                ..Default::default()
            },
            CacheRuntime {
                now_ms: Rc::new(|| 0.),
                sleep: Rc::new(|_| Box::pin(std::future::pending())),
                spawn: Rc::new(drop),
            },
        );
        let options = CandleCacheChunkAccessOptions {
            chunk_size: 100,
            interval_seconds: 60.,
        };
        patch.call1(&JsValue::NULL, &JsValue::from_str(mode))?;
        backend
            .load_chunks("k".into(), vec![0], options)
            .unwrap()
            .await
            .unwrap();
        if reports.get() != 0 {
            return Err(JsValue::from_str("unavailable guard unexpectedly reported"));
        }
        patch.call1(&JsValue::NULL, &JsValue::from_str("restore"))?;
        backend
            .load_chunks("k".into(), vec![0], options)
            .unwrap()
            .await
            .unwrap();
        if reports.get() != 0 {
            return Err(JsValue::from_str(
                "guard latched instead of allowing restored factory",
            ));
        }
    }
    Ok(())
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub async fn loaded_chunks_probe() -> Result<String, JsValue> {
    use safe_cex_candle_cache::{
        CacheRuntime, CandleCacheChunkAccessOptions,
        indexed_db::{IndexedDBCandleCacheBackend, IndexedDBCandleCacheBackendOptions},
    };
    use std::rc::Rc;
    let backend = IndexedDBCandleCacheBackend::new(
        IndexedDBCandleCacheBackendOptions {
            db_name: "p3b-loaded-chunks-proof".into(),
            ..Default::default()
        },
        CacheRuntime {
            now_ms: Rc::new(|| 0.),
            sleep: Rc::new(|_| Box::pin(std::future::pending())),
            spawn: Rc::new(drop),
        },
    );
    let loaded = backend
        .load_all_chunks(
            "k".into(),
            CandleCacheChunkAccessOptions {
                chunk_size: 100,
                interval_seconds: 60.,
            },
        )
        .ok_or_else(|| JsValue::from_str("load-all operation absent"))?
        .await
        .map_err(|e| JsValue::from_str(&e))?;
    if loaded.chunks.len() != 2
        || loaded
            .chunks
            .get(&1)
            .and_then(|chunk| chunk[0].as_ref())
            .map(|c| c.timestamp)
            != Some(6_000_000.)
        || loaded
            .chunks
            .get(&2)
            .and_then(|chunk| chunk[0].as_ref())
            .map(|c| c.timestamp)
            != Some(12_000_000.)
    {
        return Err(JsValue::from_str(
            "LoadedChunks must retain both usable cursor siblings",
        ));
    }
    if loaded.refused_keys != ["k::9223372036854775808"] {
        return Err(JsValue::from_str(
            "LoadedChunks must expose the exact refused key",
        ));
    }
    Ok(
        "IndexedDB: executable LoadedChunks usable siblings and refused-key observability passed"
            .into(),
    )
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub async fn cursor_rollback_probe(patch: js_sys::Function) -> Result<String, JsValue> {
    use safe_cex_candle_cache::{
        CacheRuntime, CandleCacheChunkAccessOptions, Chunks,
        indexed_db::{IndexedDBCandleCacheBackend, IndexedDBCandleCacheBackendOptions},
    };
    use safe_cex_types::Candle;
    use std::{cell::RefCell, rc::Rc};
    let errors = Rc::new(RefCell::new(Vec::new()));
    let reported = errors.clone();
    let backend = IndexedDBCandleCacheBackend::new(
        IndexedDBCandleCacheBackendOptions {
            db_name: "p3b-cursor-rollback-proof".into(),
            report_error: Some(Rc::new(move |error| reported.borrow_mut().push(error))),
            ..Default::default()
        },
        CacheRuntime {
            now_ms: Rc::new(|| 1_700_000_000_000.),
            sleep: Rc::new(|_| Box::pin(std::future::pending())),
            spawn: Rc::new(drop),
        },
    );
    let options = CandleCacheChunkAccessOptions {
        chunk_size: 1,
        interval_seconds: 60.,
    };
    backend
        .clear(None)
        .unwrap()
        .await
        .map_err(|error| JsValue::from_str(&error))?;
    let original = Chunks::from([1, 2].map(|key| {
        (
            key,
            vec![Some(Candle {
                timestamp: key as f64 * 60_000.,
                open: 1.,
                high: 2.,
                low: 0.5,
                close: 1.5,
                volume: 3.,
                last_side: None,
            })],
        )
    }));
    backend
        .persist_chunks("k".into(), original.clone(), options)
        .await
        .map_err(|error| JsValue::from_str(&error))?;
    patch.call1(&JsValue::NULL, &JsValue::from_str("rollback"))?;
    let cleared = backend.clear(Some("k".into())).unwrap().await;
    patch.call1(&JsValue::NULL, &JsValue::from_str("restore"))?;
    cleared.map_err(|error| JsValue::from_str(&error))?;
    let loaded = backend
        .load_all_chunks("k".into(), options)
        .unwrap()
        .await
        .map_err(|error| JsValue::from_str(&error))?;
    if loaded.chunks != original || !loaded.refused_keys.is_empty() {
        return Err(JsValue::from_str(
            "cursor write failure must roll back both original chunks",
        ));
    }
    if errors.borrow().len() != 1 {
        return Err(JsValue::from_str(
            "cursor rollback failure must report once",
        ));
    }
    Ok("IndexedDB cursor write rollback passed".into())
}
