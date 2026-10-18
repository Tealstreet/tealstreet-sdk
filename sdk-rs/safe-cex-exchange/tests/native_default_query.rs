//! Actual HTTP request-target bytes through the production JS host and native terminal.
#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    js_runtime::{client, promise::await_js, value::Js},
    native::native_runtime,
};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::Arc,
    time::Duration,
};

fn request_target(params: Js) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        let mut chunk = [0; 1024];
        while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
            let n = stream.read(&mut chunk).unwrap();
            assert!(n > 0, "request headers must complete");
            request.extend_from_slice(&chunk[..n]);
        }
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
            .unwrap();
        String::from_utf8(request)
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap()
            .to_owned()
    });
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&executor, async move {
        let runtime: Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime> = native_runtime();
        let terminal = Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap());
        let host = client::rest_http_host(runtime.clone(), terminal);
        let xhr = client::create(
            host,
            Js::object(vec![
                ("baseURL", Js::str(&format!("http://{address}"))),
                ("timeout", Js::Number(2000.0)),
            ]),
        );
        let pending = xhr
            .call_method(
                "get",
                vec![Js::str("/witness"), Js::object(vec![("params", params)])],
            )
            .unwrap();
        await_js(&runtime, pending).await.unwrap();
    });
    server.join().unwrap()
}

#[test]
fn native_http_keeps_axios_query_punctuation_on_the_wire() {
    let actual = request_target(Js::object(vec![("a", Js::str("x~y!z(1)"))]));
    assert_eq!(
        actual, "/witness?a=x~y!z(1)",
        "actual Axios HTTP adapter preserves this request target"
    );
}

#[test]
fn js_http_keeps_nonfinite_default_query_values_on_the_wire() {
    let actual = request_target(Js::object(vec![
        ("p", Js::Number(f64::NAN)),
        ("i", Js::Number(f64::INFINITY)),
        ("n", Js::Number(f64::NEG_INFINITY)),
        ("z", Js::Number(-0.0)),
        ("skip", Js::Undefined),
        ("null", Js::Null),
    ]));
    assert_eq!(actual, "/witness?p=NaN&i=Infinity&n=-Infinity&z=0");
}

#[test]
fn native_http_keeps_nested_axios_keys_on_the_wire() {
    assert_eq!(
        request_target(Js::object(vec![(
            "a",
            Js::array(vec![Js::Number(1.0), Js::Number(2.0)])
        )])),
        "/witness?a%5B%5D=1&a%5B%5D=2"
    );
    assert_eq!(
        request_target(Js::object(vec![(
            "a",
            Js::object(vec![("b", Js::Number(1.0))])
        )])),
        "/witness?a%5Bb%5D=1"
    );
}
