use safe_cex_exchange::{
    js_runtime::{
        client,
        value::{Js, arg},
    },
    native::native_runtime,
};
use serde_json::{Value, json};
use std::{
    cell::Cell,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub fn capture() -> Value {
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&executor, async {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(AtomicUsize::new(0));
        let served = requests.clone();
        let server = tokio::spawn(async move {
            for status in ["429 Too Many Requests", "200 OK"] {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                    let mut bytes = [0; 1024];
                    let count = socket.read(&mut bytes).await.unwrap();
                    assert!(count > 0 && request.len() < 8192, "complete bounded HTTP request");
                    request.extend_from_slice(&bytes[..count]);
                }
                assert!(request.starts_with(b"GET /retry HTTP/1.1\r\n"));
                served.fetch_add(1, Ordering::SeqCst);
                socket.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}").as_bytes()).await.unwrap();
            }
        });
        let runtime = native_runtime();
        let host = client::rest_http_host(runtime, Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()));
        let client = client::create(host, Js::object(vec![
            ("baseURL", Js::str(&format!("http://{address}"))),
            ("timeout", Js::Number(5000.0)),
        ]));
        let armed = Rc::new(tokio::sync::Notify::new());
        let complete = Rc::new(tokio::sync::Notify::new());
        let before_replay = Rc::new(Cell::new(None));
        let (notify, before, seen) = (armed.clone(), before_replay.clone(), requests.clone());
        client.call_method("__configureRetry", vec![Js::object(vec![
            ("retries", Js::Number(1.0)),
            ("retryCondition", Js::function("retryCondition", |_, _| Ok(Js::Bool(true)))),
            ("retryDelay", Js::function("retryDelay", move |_, _| {
                before.set(Some(seen.load(Ordering::SeqCst)));
                notify.notify_one();
                Ok(Js::Number(25.0))
            })),
        ])]).unwrap();
        let notify = complete.clone();
        client.get("interceptors").unwrap().get("response").unwrap().call_method("use", vec![Js::function("complete", move |_, args| {
            notify.notify_one();
            Ok(arg(&args, 0))
        })]).unwrap();
        let observer = client.call_method("get", vec![Js::str("/retry")]).unwrap();
        let immediate = requests.load(Ordering::SeqCst);
        tokio::time::timeout(Duration::from_secs(5), armed.notified()).await.unwrap();
        assert_eq!(requests.load(Ordering::SeqCst), 1, "retry timer precedes replay");
        drop(observer);
        tokio::time::timeout(Duration::from_secs(5), complete.notified()).await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), server).await.unwrap().unwrap();
        json!({"immediate":immediate,"beforeReplay":before_replay.get().unwrap(),"afterReplay":requests.load(Ordering::SeqCst)})
    })
}
