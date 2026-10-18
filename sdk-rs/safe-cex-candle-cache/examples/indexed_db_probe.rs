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

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub async fn nan_codec_probe(storage: js_sys::Function) -> Result<String, JsValue> {
    use safe_cex_candle_cache::{
        CacheRuntime, CandleCache, CandleCacheBackendReadMode, CandleCacheChunkAccessOptions,
        CandleCacheFetchRequest, CandleCacheOptions, Chunks, Fetcher,
        indexed_db::{IndexedDBCandleCacheBackend, IndexedDBCandleCacheBackendOptions},
    };
    use safe_cex_types::{Candle, OrderSide};
    use std::{cell::Cell, rc::Rc};
    let runtime = CacheRuntime {
        now_ms: Rc::new(|| 1_700_004_200_000.),
        sleep: Rc::new(|_| Box::pin(std::future::pending())),
        spawn: Rc::new(wasm_bindgen_futures::spawn_local),
    };
    let options = CandleCacheChunkAccessOptions {
        chunk_size: 100,
        interval_seconds: 60.,
    };
    let mut checked = 0;
    for (mode_index, mode) in [
        CandleCacheBackendReadMode::ReadThrough,
        CandleCacheBackendReadMode::HydrateOnce,
    ]
    .into_iter()
    .enumerate()
    {
        for (profile, (scenario, field)) in [
            ("nan", "volume"),
            ("nan", "open"),
            ("nan", "high"),
            ("nan", "low"),
            ("nan", "close"),
            ("null", "volume"),
            ("null", "open"),
            ("null", "high"),
            ("null", "low"),
            ("null", "close"),
            ("missing", "volume"),
            ("string", "open"),
            ("timestamp", "timestamp"),
        ]
        .into_iter()
        .enumerate()
        {
            let database = format!("port-txflow-nan-codec-{mode_index}-{profile}");
            let reports = Rc::new(Cell::new(0));
            let count = reports.clone();
            let config = IndexedDBCandleCacheBackendOptions {
                db_name: database.clone(),
                report_error: Some(Rc::new(move |_| count.set(count.get() + 1))),
                ..Default::default()
            };
            let writer = IndexedDBCandleCacheBackend::new(config.clone(), runtime.clone());
            writer
                .clear(None)
                .unwrap()
                .await
                .map_err(|e| JsValue::from_str(&e))?;
            let mut stored = Candle {
                timestamp: 1_700_004_000_000.,
                open: 1.,
                high: 3.,
                low: 0.5,
                close: 2.,
                volume: 4.,
                last_side: match profile % 3 {
                    0 => None,
                    1 => Some(OrderSide::Buy),
                    _ => Some(OrderSide::Sell),
                },
            };
            if scenario == "nan" || scenario == "null" {
                match field {
                    "open" => stored.open = f64::NAN,
                    "high" => stored.high = f64::NAN,
                    "low" => stored.low = f64::NAN,
                    "close" => stored.close = f64::NAN,
                    _ => stored.volume = f64::NAN,
                }
            }
            writer
                .persist_chunks(
                    "cache".into(),
                    Chunks::from([(283334, vec![Some(stored.clone())])]),
                    options,
                )
                .await
                .map_err(|e| JsValue::from_str(&e))?;
            drop(writer);
            if scenario != "nan" {
                wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&storage.call3(
                    &JsValue::NULL,
                    &JsValue::from_str(&database),
                    &JsValue::from_str(scenario),
                    &JsValue::from_str(field),
                )?))
                .await?;
            }
            let backend = Rc::new(IndexedDBCandleCacheBackend::new(config, runtime.clone()));
            let cache = CandleCache::new(
                CandleCacheOptions {
                    backend: Some(backend.clone()),
                    backend_read_mode: Some(mode),
                    chunk_size: Some(100),
                    ..Default::default()
                },
                runtime.clone(),
            );
            let requests = Rc::new(Cell::new(0));
            let count = requests.clone();
            let fallback = Candle {
                timestamp: 1_700_004_000_000.,
                open: 1.,
                high: 3.,
                low: 0.5,
                close: 2.,
                volume: 4.,
                last_side: stored.last_side,
            };
            let expected_fallback = fallback.clone();
            let fetcher: Fetcher = Rc::new(move |_| {
                count.set(count.get() + 1);
                let value = fallback.clone();
                Box::pin(async move { Ok(vec![value]) })
            });
            let result = cache
                .fetch(CandleCacheFetchRequest {
                    cache_key: "cache".into(),
                    interval: serde_json::from_str("\"1m\"").unwrap(),
                    from_ms: 1_700_004_000_000.,
                    to_ms: 1_700_004_000_000.,
                    limit: 1.,
                    now_ms: Some(1_700_004_200_000.),
                    fetcher,
                    use_persistent_backend: None,
                })
                .await
                .map_err(|e| JsValue::from_str(&e))?;
            let hit = scenario == "nan" || scenario == "null";
            if requests.get() != usize::from(!hit) {
                return Err(JsValue::from_str(&format!(
                    "NAN-CACHE-IDB {scenario}/{field}/mode{mode_index}: venue requests {}, expected {}",
                    requests.get(),
                    usize::from(!hit)
                )));
            }
            if result.len() != 1 {
                return Err(JsValue::from_str("cache must return exactly one candle"));
            }
            let actual = &result[0];
            if hit {
                for (name, value, expected) in [
                    ("open", actual.open, stored.open),
                    ("high", actual.high, stored.high),
                    ("low", actual.low, stored.low),
                    ("close", actual.close, stored.close),
                    ("volume", actual.volume, stored.volume),
                ] {
                    if (name == field && !value.is_nan())
                        || (name != field && value.to_bits() != expected.to_bits())
                    {
                        return Err(JsValue::from_str(&format!(
                            "native field {scenario}/{field}/{name}"
                        )));
                    }
                }
                if actual.timestamp.to_bits() != stored.timestamp.to_bits()
                    || actual.last_side != stored.last_side
                {
                    return Err(JsValue::from_str("native timestamp/lastSide changed"));
                }
                for loaded in [
                    backend
                        .load_chunks("cache".into(), vec![283334], options)
                        .unwrap()
                        .await
                        .map_err(|e| JsValue::from_str(&e))?,
                    backend
                        .load_all_chunks("cache".into(), options)
                        .unwrap()
                        .await
                        .map_err(|e| JsValue::from_str(&e))?
                        .chunks,
                ] {
                    if loaded.get(&283334).and_then(|c| c[0].as_ref()).is_none() {
                        return Err(JsValue::from_str(
                            "both IndexedDB read APIs must retain the candle",
                        ));
                    }
                }
                wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&storage.call3(
                    &JsValue::NULL,
                    &JsValue::from_str(&database),
                    &JsValue::from_str(if scenario == "nan" {
                        "inspect-nan"
                    } else {
                        "inspect-null"
                    }),
                    &JsValue::from_str(field),
                )?))
                .await?;
            } else if actual != &expected_fallback {
                return Err(JsValue::from_str(
                    "malformed row must return the fetched candle",
                ));
            }
            if reports.get() != 0 {
                return Err(JsValue::from_str(
                    "codec/malformed rows must not report backend failures",
                ));
            }
            checked += 1;
        }
    }
    if checked != 26 {
        return Err(JsValue::from_str("IndexedDB profile floor"));
    }
    Ok(format!(
        "IndexedDB NaN/null reload hits and malformed misses: {checked} real profiles"
    ))
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub async fn fix_audit_probe() -> Result<String, JsValue> {
    use safe_cex_candle_cache::{
        CacheRuntime, CandleCacheChunkAccessOptions, Chunks,
        indexed_db::{IndexedDBCandleCacheBackend, IndexedDBCandleCacheBackendOptions},
    };
    use safe_cex_types::Candle;
    use std::{
        rc::Rc,
        task::{Context, Waker},
    };
    let runtime = CacheRuntime {
        now_ms: Rc::new(|| 1_700_004_200_000.),
        sleep: Rc::new(|_| Box::pin(std::future::pending())),
        spawn: Rc::new(drop),
    };
    let marker = Rc::new(());
    let weak = Rc::downgrade(&marker);
    let captured = marker.clone();
    drop(marker);
    let config = IndexedDBCandleCacheBackendOptions {
        db_name: "port-txflow-fix-audit".into(),
        report_error: Some(Rc::new(move |_| {
            let _ = &captured;
        })),
        ..Default::default()
    };
    let backend = IndexedDBCandleCacheBackend::new(config.clone(), runtime.clone());
    backend
        .clear(None)
        .unwrap()
        .await
        .map_err(|e| JsValue::from_str(&e))?;
    let options = CandleCacheChunkAccessOptions {
        chunk_size: 100,
        interval_seconds: 60.,
    };
    let mut slots = vec![None; 100];
    slots[99] = Some(Candle {
        timestamp: -60_000.,
        open: 11.,
        high: 17.,
        low: 7.,
        close: 13.,
        volume: 19.,
        last_side: None,
    });
    // Warm database: the future's first poll starts the actual put transaction.
    let mut write = backend.persist_chunks("negative".into(), Chunks::from([(-1, slots)]), options);
    assert!(
        write
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    drop(write);
    drop(backend);
    drop(config);
    assert!(
        weak.upgrade().is_none(),
        "CACHE-AUDIT dropped persistence must release callback owner"
    );
    // Dropping the waiter detaches callbacks; an already-issued browser put still commits.
    let reader = IndexedDBCandleCacheBackend::new(
        IndexedDBCandleCacheBackendOptions {
            db_name: "port-txflow-fix-audit".into(),
            ..Default::default()
        },
        runtime,
    );
    let chunks = reader
        .load_chunks("negative".into(), vec![-1], options)
        .unwrap()
        .await
        .map_err(|e| JsValue::from_str(&e))?;
    let candle = chunks
        .get(&-1)
        .and_then(|c| c[99].as_ref())
        .ok_or_else(|| JsValue::from_str("CACHE-AUDIT cancelled waiter lost issued put"))?;
    for (actual, expected) in [
        (candle.timestamp, -60_000_f64),
        (candle.open, 11.),
        (candle.high, 17.),
        (candle.low, 7.),
        (candle.close, 13.),
        (candle.volume, 19.),
    ] {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "CACHE-AUDIT IndexedDB native field"
        );
    }
    Ok("CACHE-AUDIT IndexedDB pre-epoch persistence and cancelled waiter ownership passed".into())
}
