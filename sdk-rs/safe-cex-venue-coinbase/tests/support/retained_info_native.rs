use safe_cex_exchange::{
    exchange::Exchange,
    js_runtime::{
        client,
        value::{Js, from_typed, to_retained_input, to_typed},
    },
    native::native_runtime,
};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_types::{ExchangeName, ExchangeOptions, Market};
use safe_cex_venue_coinbase::{
    CoinbaseDependencies, CoinbaseExchange, cdp_signing::CdpSigningCache,
};
use std::{
    io::{Read, Write},
    net::TcpListener,
    process::{Command, Stdio},
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
#[path = "retained_info.rs"]
mod observation;

const PRODUCTS: &str = r#"{"products":[{"product_id":"BTC-USD","base_currency_id":"BTC","quote_currency_id":"USD","status":"TRADING","price_increment":"0.01","base_increment":"0.001","base_min_size":"0.001","base_max_size":"100","trading_disabled":false,"view_only":false,"price":"123.5","price_percentage_change_24h":"2.5","volume_24h":"1000","opaque":{"\ud801":{"v":"\udc00"},"positive":1e999,"negative":-1e999,"zero":-0,"text":"\ud800","array":[1e999,-0,"\udfff"]}}]}"#;

#[test]
fn native_http_and_store_keep_opaque_coinbase_info() {
    let mut source = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-market-data.cjs"
        ))
        .arg("--retained-info")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    source
        .stdin
        .take()
        .unwrap()
        .write_all(PRODUCTS.as_bytes())
        .unwrap();
    let source = source.wait_with_output().unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let expected: serde_json::Value = serde_json::from_slice(&source.stdout).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        for (path, body) in [
            (
                "/portfolios",
                r#"{"portfolios":[{"uuid":"retained","type":"DEFAULT"}]}"#,
            ),
            (
                "/portfolios/retained",
                r#"{"breakdown":{"portfolio":{"uuid":"retained","type":"DEFAULT"},"spot_positions":[],"perp_positions":[],"futures_positions":[]}}"#,
            ),
            ("/products", PRODUCTS),
        ] {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(1))
                    }
                    Err(error) => panic!("native HTTP {path}: {error}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut headers = Vec::new();
            let mut chunk = [0; 1024];
            while !headers.windows(4).any(|v| v == b"\r\n\r\n") {
                let count = stream.read(&mut chunk).unwrap();
                assert_ne!(count, 0, "native HTTP headers");
                headers.extend_from_slice(&chunk[..count]);
            }
            let request = String::from_utf8(headers).unwrap();
            assert!(request.lines().next().unwrap().contains(path), "{request}");
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
    });
    let tokio = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&tokio, async {
        let runtime = native_runtime();
        let http = client::rest_http_host(
            runtime.clone(),
            Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
        );
        let store = Rc::new(DefaultStore::new());
        let options: ExchangeOptions = serde_json::from_value(serde_json::json!({
            "key":"organizations/synthetic-org/apiKeys/synthetic-key",
            "secret":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            "overrideProxyUrl":origin,
        }))
        .unwrap();
        let venue = CoinbaseExchange::new(
            ExchangeName::Coinbasespot,
            options,
            store.clone(),
            CoinbaseDependencies {
                runtime,
                http,
                external_public_data: None,
                signing_cache: Rc::new(CdpSigningCache::default()),
                local_wall_time_to_utc: Rc::new(|value| value),
            },
        )
        .unwrap();
        let markets = tokio::time::timeout(Duration::from_secs(10), venue.fetch_markets())
            .await
            .unwrap()
            .expect("native Coinbase markets retain opaque response info");
        assert_eq!(markets.len(), 1, "actual HTTP product admission");
        let market = from_typed(&markets[0]).unwrap();
        market
            .get("info")
            .unwrap()
            .define(
                "native",
                Js::object(vec![
                    ("nan", Js::Number(f64::NAN)),
                    ("undefined", Js::Undefined),
                    (
                        "array",
                        Js::array(vec![
                            Js::Number(f64::NAN),
                            Js::Number(f64::INFINITY),
                            Js::Number(-0.0),
                            Js::Undefined,
                            Js::utf16(&[0xdfff]),
                        ]),
                    ),
                ]),
            )
            .unwrap();
        let market: Market = to_typed(&market).unwrap();
        Store::update(
            store.as_ref(),
            StoreUpdate {
                markets: Some(vec![market]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let stored = from_typed(&store.markets()[0]).unwrap();
        let info = to_retained_input(&stored.get("info").unwrap()).unwrap();
        observation::assert_info(Some(&info), &expected, "native Coinbase opaque info");
        venue.dispose().unwrap();
    });
    server.join().unwrap();
}
