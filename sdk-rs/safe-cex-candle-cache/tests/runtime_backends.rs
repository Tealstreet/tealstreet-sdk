#![cfg(not(target_arch = "wasm32"))]
use safe_cex_candle_cache::{
    CandleCacheChunkAccessOptions, Chunks,
    native::{FilesystemCandleCacheBackend, RedisCandleCacheBackend, local_runtime},
};
use safe_cex_types::Candle;
use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

fn fixture() -> (CandleCacheChunkAccessOptions, Chunks) {
    let mut sparse = vec![None; 100];
    sparse[0] = Some(Candle {
        timestamp: 6_000_000.,
        open: 1.,
        high: 3.,
        low: 0.5,
        close: 2.,
        volume: 4.,
        last_side: None,
    });
    (
        CandleCacheChunkAccessOptions {
            chunk_size: 100,
            interval_seconds: 60.,
        },
        Chunks::from([(1, sparse)]),
    )
}
#[test]
fn filesystem_reopen_namespace_and_atomic_replacement() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(tokio::task::LocalSet::new().run_until(async {
        let directory = PathBuf::from(format!(
            "/tmp/p3b-files-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let backend = FilesystemCandleCacheBackend::open(directory.clone(), Some("n".into()))
            .await
            .unwrap();
        let (options, chunks) = fixture();
        backend
            .persist_chunks("../../escaped::key".into(), chunks.clone(), options)
            .await
            .unwrap();
        backend
            .persist_chunks("keep".into(), chunks.clone(), options)
            .await
            .unwrap();
        let fresh = FilesystemCandleCacheBackend::open(directory.clone(), Some("n".into()))
            .await
            .unwrap();
        assert_eq!(
            fresh
                .load_all_chunks("../../escaped::key".into(), options)
                .unwrap()
                .await
                .unwrap()
                .chunks,
            chunks
        );
        fresh
            .clear(Some("../../escaped::key".into()))
            .unwrap()
            .await
            .unwrap();
        assert!(
            fresh
                .load_all_chunks("../../escaped::key".into(), options)
                .unwrap()
                .await
                .unwrap()
                .chunks
                .is_empty()
        );
        assert_eq!(
            fresh
                .load_chunks("keep".into(), vec![1], options)
                .unwrap()
                .await
                .unwrap(),
            chunks
        );
        fresh.clear(None).unwrap().await.unwrap();
        assert!(std::fs::read_dir(&directory).unwrap().next().is_none());
        std::fs::remove_dir_all(directory).unwrap();
    }));
}
#[test]
#[ignore = "requires P3B_REDIS_URL pointing at an isolated test Redis"]
fn redis_persist_reopen_scan_clear_and_namespace_isolation() {
    let url = std::env::var("P3B_REDIS_URL").expect("include-ignored requires isolated Redis URL");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(tokio::task::LocalSet::new().run_until(async {
        let namespace = format!(
            "p3b-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let backend =
            RedisCandleCacheBackend::connect(&url, Some(namespace.clone()), local_runtime(), false)
                .await
                .unwrap();
        let (options, chunks) = fixture();
        backend
            .persist_chunks("k".into(), chunks.clone(), options)
            .await
            .unwrap();
        backend
            .persist_chunks("keep".into(), chunks.clone(), options)
            .await
            .unwrap();
        let fresh = RedisCandleCacheBackend::connect(&url, Some(namespace), local_runtime(), false)
            .await
            .unwrap();
        assert_eq!(
            fresh
                .load_all_chunks("k".into(), options)
                .unwrap()
                .await
                .unwrap()
                .chunks,
            chunks
        );
        assert_eq!(
            fresh
                .load_chunks("k".into(), vec![1], options)
                .unwrap()
                .await
                .unwrap(),
            chunks
        );
        fresh.clear(Some("k".into())).unwrap().await.unwrap();
        assert!(
            fresh
                .load_all_chunks("k".into(), options)
                .unwrap()
                .await
                .unwrap()
                .chunks
                .is_empty()
        );
        assert_eq!(
            fresh
                .load_all_chunks("keep".into(), options)
                .unwrap()
                .await
                .unwrap()
                .chunks,
            chunks
        );
        fresh.clear(None).unwrap().await.unwrap();
        assert!(
            fresh
                .load_all_chunks("keep".into(), options)
                .unwrap()
                .await
                .unwrap()
                .chunks
                .is_empty()
        );
    }));
}

#[test]
#[ignore = "requires P3B_REDIS_SERVER executable for isolated restart proof"]
fn redis_restart_recovers_both_read_and_bulk_connections() {
    use std::process::{Child, Command, Stdio};
    struct Server(Child);
    impl Drop for Server {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let binary = std::env::var("P3B_REDIS_SERVER")
        .expect("include-ignored requires Redis server executable");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let start = || {
        Server(
            Command::new(&binary)
                .args([
                    "--bind",
                    "127.0.0.1",
                    "--port",
                    &port.to_string(),
                    "--save",
                    "",
                    "--appendonly",
                    "no",
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        )
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(tokio::task::LocalSet::new().run_until(async {
        let url = format!("redis://127.0.0.1:{port}");
        let ready = || async {
            for _ in 0..100 {
                if tokio::net::TcpStream::connect(("127.0.0.1", port))
                    .await
                    .is_ok()
                {
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            panic!("Redis did not start");
        };
        let server = start();
        ready().await;
        let backend =
            RedisCandleCacheBackend::connect(&url, Some("restart".into()), local_runtime(), false)
                .await
                .unwrap();
        let (options, chunks) = fixture();
        backend
            .persist_chunks("k".into(), chunks.clone(), options)
            .await
            .unwrap();
        assert_eq!(
            backend
                .load_chunks("k".into(), vec![1], options)
                .unwrap()
                .await
                .unwrap(),
            chunks
        );
        drop(server);
        assert!(
            backend
                .persist_chunks("k".into(), chunks.clone(), options)
                .await
                .is_err()
        );
        assert!(
            backend
                .load_chunks("k".into(), vec![1], options)
                .unwrap()
                .await
                .is_err()
        );
        let _server = start();
        ready().await;
        backend
            .persist_chunks("k".into(), chunks.clone(), options)
            .await
            .unwrap();
        assert_eq!(
            backend
                .load_chunks("k".into(), vec![1], options)
                .unwrap()
                .await
                .unwrap(),
            chunks
        );
        backend.clear(None).unwrap().await.unwrap();
    }));
}

#[test]
#[ignore = "requires P3B_REDIS_URL pointing at an isolated test Redis with ACL support"]
fn best_effort_read_keeps_successful_chunks_when_one_get_is_denied() {
    let url = std::env::var("P3B_REDIS_URL").unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(tokio::task::LocalSet::new().run_until(async {
        let namespace = format!(
            "p3b-partial-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let user = namespace.clone();
        let backend =
            RedisCandleCacheBackend::connect(&url, Some(namespace.clone()), local_runtime(), false)
                .await
                .unwrap();
        let (options, expected) = fixture();
        let mut all = expected.clone();
        let mut earlier = vec![None; 100];
        let mut candle = expected[&1][0].clone().unwrap();
        candle.timestamp = 5_940_000.;
        earlier[99] = Some(candle);
        all.insert(0, earlier);
        backend
            .persist_chunks("k".into(), all, options)
            .await
            .unwrap();
        let mut admin = redis::Client::open(url.as_str())
            .unwrap()
            .get_multiplexed_async_connection()
            .await
            .unwrap();
        redis::cmd("ACL")
            .arg("SETUSER")
            .arg(&user)
            .arg("reset")
            .arg("on")
            .arg(">test-only")
            .arg(format!("~{namespace}::k::1"))
            .arg("+@all")
            .query_async::<()>(&mut admin)
            .await
            .unwrap();
        let (scheme, address) = url.split_once("://").unwrap();
        let limited_url = format!("{scheme}://{user}:test-only@{address}");
        let limited = RedisCandleCacheBackend::connect(
            &limited_url,
            Some(namespace.clone()),
            local_runtime(),
            true,
        )
        .await
        .unwrap();
        assert_eq!(
            limited
                .load_chunks("k".into(), vec![0, 1], options)
                .unwrap()
                .await
                .unwrap(),
            expected
        );
        let strict =
            RedisCandleCacheBackend::connect(&limited_url, Some(namespace), local_runtime(), false)
                .await
                .unwrap();
        assert!(
            strict
                .load_chunks("k".into(), vec![0, 1], options)
                .unwrap()
                .await
                .is_err()
        );
        redis::cmd("ACL")
            .arg("DELUSER")
            .arg(user)
            .query_async::<i64>(&mut admin)
            .await
            .unwrap();
        backend.clear(None).unwrap().await.unwrap();
    }));
}
