//! GRVT through the registry, common HTTP factory and real local reqwest IO.
#![cfg(not(target_arch = "wasm32"))]
use safe_cex_registry::{
    native_host::{hyperliquid_rate_limiter, native_venue_host_with},
    rust_venues::rust_constructor,
};
use safe_cex_store::store::DefaultStore;
use safe_cex_transport::{
    RestAdapter, RestFuture, RestRequest, RestResponse, native::ReqwestAdapter,
};
use safe_cex_types::ExchangeName;
use serde_json::json;
use std::{cell::RefCell, rc::Rc, sync::Arc};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

struct LocalOrigin {
    inner: ReqwestAdapter,
    origin: String,
    seen: Rc<RefCell<Vec<RestRequest>>>,
}
impl RestAdapter for LocalOrigin {
    fn execute(&self, mut request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.seen.borrow_mut().push(request.clone());
        request.base_url = Some(self.origin.clone());
        self.inner.execute(request)
    }
}

#[test]
#[expect(clippy::arc_with_non_send_sync)]
fn grvt_registry_uses_real_http_and_preserves_direct_sdk_session_headers() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        tokio::task::spawn_local(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let mut bytes = Vec::new();
                let mut chunk = [0;4096];
                while !bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = stream.read(&mut chunk).await.unwrap();
                    if n == 0 { return; }
                    bytes.extend_from_slice(&chunk[..n]);
                }
                let line = String::from_utf8_lossy(&bytes).lines().next().unwrap().to_owned();
                let path = line.split(' ').nth(1).unwrap();
                let body = match path {
                    "/lite/v1/all_instruments" => json!({"r":[{"i":"BTC_USDT_Perp","k":"PERPETUAL","b":"BTC","q":"USDT","ms":"0.001","ts":"0.1","ih":"0x030501","bd":3,"qd":6}]}),
                    "/auth/api_key/login" | "/lite/v1/account_summary" => json!({"r":{}}),
                    "/lite/v1/funding_account_summary" => json!({"r":{"ma":"account-local","te":"123","tc":"100"}}),
                    _ => panic!("unexpected GRVT native wire route {path}"),
                }.to_string();
                let auth = if path == "/auth/api_key/login" {
                    "Set-Cookie: gravity=session-local; Expires=Wed, 21 Oct 2037 07:28:00 GMT\r\nX-Grvt-Account-Id: account-local\r\n"
                } else { "" };
                let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n{auth}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
                stream.write_all(response.as_bytes()).await.unwrap();
                let _ = stream.shutdown().await;
            }
        });
        let seen = Rc::new(RefCell::new(vec![]));
        let recorded = seen.clone();
        let host = native_venue_host_with(safe_cex_exchange::native::native_runtime(), Rc::new(move || {
            Ok(Arc::new(LocalOrigin {
                inner: safe_cex_transport::native::reqwest_adapter().unwrap(),
                origin: origin.clone(), seen: recorded.clone(),
            }) as Arc<dyn RestAdapter>)
        }), hyperliquid_rate_limiter());
        let ctor = rust_constructor(&ExchangeName::Grvt, host).unwrap();
        let options = serde_json::from_value(json!({"key":"native-key","secret":"0000000000000000000000000000000000000000000000000000000000000001","grvtTradingAccountId":"42"})).unwrap();
        let exchange = ctor.construct(ExchangeName::Grvt, options, Rc::new(DefaultStore::new())).unwrap();
        let markets = exchange.fetch_markets().await.unwrap();
        assert_eq!(markets[0].symbol,"BTC_USDT_PERP");
        assert_eq!(serde_json::to_value(exchange.validate_account().await.unwrap().error).unwrap(), json!(""));
        assert_eq!(exchange.get_account().await.unwrap().user_id,"account-local");
        let requests = seen.borrow();
        assert_eq!(requests.len(),4,"one login seeds the SDK cookie");
        assert!(requests.iter().all(|r| r.timeout_ms == Some(20_000)), "common factory timeout overrides venue 30s");
        let funding = requests.iter().find(|r| r.url == "/lite/v1/funding_account_summary").unwrap();
        assert_eq!(funding.base_url.as_deref(),Some("https://trades.grvt.io"));
        assert_eq!(funding.headers["Cookie"],"gravity=session-local");
        assert_eq!(funding.headers["X-Grvt-Account-Id"],"account-local");
        assert!(funding.body.is_none(),"SDK funding call preserves null body");
        let account = requests.iter().find(|r| r.url == "/lite/v1/account_summary").unwrap();
        assert_eq!(account.headers["X-Ts-Grvt-Session"],"session-local");
        assert!(!account.headers.contains_key("Cookie"),"direct source never promotes custom grvtCookie");
        assert_eq!(account.config_extensions["grvtCookie"].clone().into_value().unwrap(),json!("rm=true; gravity=session-local"));
        assert_eq!(requests[0].base_url.as_deref(),Some("https://market-data.grvt.io"));
        drop(requests);
        exchange.dispose().unwrap();
    });
}

#[test]
fn grvt_common_factory_refuses_proxy_options_on_direct_transport_during_construction() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
        let host = safe_cex_registry::native_host::native_venue_host(hyperliquid_rate_limiter());
        let ctor = rust_constructor(&ExchangeName::Grvt, host).unwrap();
        let options =
            serde_json::from_value(json!({"key":"","secret":"","tealstreetProxy":true})).unwrap();
        let result = ctor.construct(ExchangeName::Grvt, options, Rc::new(DefaultStore::new()));
        let error = result
            .err()
            .expect("source common factory must reject unsupported routing before I/O");
        assert!(
            error.message.contains("direct transport is active"),
            "{}",
            error.message
        );
    });
}
