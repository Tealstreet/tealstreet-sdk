//! Both Lighter registry names over real local HTTP and the supplied shared limiter.
#![cfg(not(target_arch = "wasm32"))]
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::websocket::WsRuntime;
use safe_cex_registry::{
    native_host::{hyperliquid_rate_limiter, native_venue_host_with},
    rust_venues::{RustVenueHost, rust_constructor},
};
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
    requests: Rc<RefCell<Vec<RestRequest>>>,
}
impl RestAdapter for LocalOrigin {
    fn execute(&self, mut request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.requests.borrow_mut().push(request.clone());
        request.base_url = Some(self.origin.clone());
        self.inner.execute(request)
    }
}
#[test]
#[expect(clippy::arc_with_non_send_sync)]
fn lighter_registry_constructs_both_names_and_reads_each_deployment_over_native_http() {
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
                let mut chunk = [0u8;4096];
                while !bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = stream.read(&mut chunk).await.unwrap();
                    if n == 0 { return; }
                    bytes.extend_from_slice(&chunk[..n]);
                }
                let line = String::from_utf8_lossy(&bytes).lines().next().unwrap().to_owned();
                assert_eq!(line,"GET /api/v1/orderBookDetails HTTP/1.1");
                let body = json!({"code":200,"order_book_details":[{"symbol":"BTC","market_id":1,"market_type":"perp","status":"active","supported_size_decimals":4,"supported_price_decimals":1,"min_base_amount":"0.0001","min_quote_amount":"10","order_quote_limit":"1000","min_initial_margin_fraction":200,"market_config":{"market_margin_mode":0}}]}).to_string();
                let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
                stream.write_all(response.as_bytes()).await.unwrap();
                let _ = stream.shutdown().await;
            }
        });
        let requests = Rc::new(RefCell::new(vec![]));
        let seen = requests.clone();
        let runtime = safe_cex_exchange::native::native_runtime();
        // One LocalSet-owned Hyperliquid budget is passed through the required
        // host API and shared by every account constructed in this test.
        let limiter = hyperliquid_rate_limiter();
        let host = native_venue_host_with(runtime.clone(), Rc::new(move || {
            Ok(Arc::new(LocalOrigin {
                inner: safe_cex_transport::native::reqwest_adapter().unwrap(),
                origin: origin.clone(), requests: seen.clone(),
            }) as Arc<dyn RestAdapter>)
        }), limiter);
        let inputs = host.lighter().unwrap();
        assert!(inputs.signing_entropy.is_none(),"production uses OS entropy");
        assert!(Arc::ptr_eq(&inputs.registry.get_transport(None), &runtime.transport()),"Lighter realm keeps its runtime provider");
        for (name,testnet,base,quote) in [
            (ExchangeName::Lighter,false,"https://mainnet.zklighter.elliot.ai","USDC"),
            (ExchangeName::Lighter,true,"https://testnet-api.lighter.xyz","USDC"),
            (ExchangeName::LighterRobinhood,false,"https://api.rh.lighter.xyz","USDG"),
            (ExchangeName::LighterRobinhood,true,"https://api.rh-testnet.lighter.xyz","USDG"),
        ] {
            let constructor = rust_constructor(&name,host.clone()).expect("bound Lighter registry name");
            // Public test-only key, as used by the venue's signing fixture.
            let options = serde_json::from_value(json!({"key":"","secret":"01000000000000000000000000000000000000000000000000000000000000000000000000000000","applicationId":"42","apiKeyIndex":"3","testnet":testnet})).unwrap();
            let exchange = constructor.construct(name.clone(),options,Rc::new(DefaultStore::new())).unwrap();
            let markets = exchange.fetch_markets().await.unwrap();
            assert_eq!(exchange.name(),&name);
            assert_eq!(markets.len(),1);
            assert_eq!(markets[0].symbol,"BTC");
            assert_eq!(markets[0].quote,quote);
            assert_eq!(markets[0].precision.amount,0.0001);
            let request = requests.borrow().last().unwrap().clone();
            assert_eq!(request.method,"get");
            assert_eq!(request.url,"/api/v1/orderBookDetails");
            assert_eq!(request.base_url.as_deref(),Some(base));
            exchange.dispose().unwrap();
        }
        assert_eq!(requests.borrow().len(),4,"one SDK read per deployment");
    });
}
