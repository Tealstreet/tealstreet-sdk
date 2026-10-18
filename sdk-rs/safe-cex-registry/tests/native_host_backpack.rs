//! BackpackExchange built with the production native inputs, reading markets
//! from a local HTTP server through the real reqwest adapter.
#![cfg(not(target_arch = "wasm32"))]
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{
    exchange::{BaseExchange, Exchange, ExchangeSettings},
    websocket::WsRuntime,
};
use safe_cex_transport::{
    RestAdapter, RestFuture, RestRequest, RestResponse, native::ReqwestAdapter,
};
use safe_cex_types::ExchangeName;
use safe_cex_venue_backpack::{adapter::BackpackExchange, native_host::native_inputs};
use serde_json::json;
use std::{cell::RefCell, rc::Rc, sync::Arc};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

struct LocalOrigin {
    inner: ReqwestAdapter,
    origin: String,
}
impl RestAdapter for LocalOrigin {
    fn execute(&self, mut request: RestRequest) -> RestFuture<'_, RestResponse> {
        request.base_url = Some(self.origin.clone());
        self.inner.execute(request)
    }
}

#[test]
fn backpack_constructs_on_the_native_host_and_reads_markets_over_http() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&runtime, async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let paths = Rc::new(RefCell::new(vec![]));
        let seen = paths.clone();
        tokio::task::spawn_local(async move {
            let markets = json!([
                {"symbol":"SOL_USDC_PERP","marketType":"PERP","visible":true,"imfFunction":{"base":"0.05"},
                 "filters":{"price":{"tickSize":"0.01","minPrice":"0.01","maxPrice":"1000"},
                            "quantity":{"stepSize":"0.1","minQuantity":"0.1","maxQuantity":"500"}}},
                {"symbol":"HIDDEN_USDC","marketType":"SPOT","visible":false}
            ])
            .to_string();
            while let Ok((mut stream, _)) = listener.accept().await {
                let mut head = vec![0u8; 8192];
                let n = stream.read(&mut head).await.unwrap_or(0);
                let line = String::from_utf8_lossy(&head[..n]).lines().next().unwrap_or_default().to_owned();
                seen.borrow_mut().push(line);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{markets}",
                    markets.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
            }
        });

        let host: Rc<dyn WsRuntime> = safe_cex_exchange::native::native_runtime();
        let adapter = Arc::new(LocalOrigin { inner: safe_cex_transport::native::reqwest_adapter().unwrap(), origin });
        let inputs = native_inputs(host.clone(), adapter);
        let options = serde_json::from_value(json!({"key":"","secret":"","publicDataOnly":true})).unwrap();
        let base = BaseExchange::new(
            ExchangeName::Backpack,
            options,
            Rc::new(DefaultStore::new()),
            host,
            ExchangeSettings::default(),
        );
        let exchange = BackpackExchange::new(base, inputs.build_http, inputs.install_transport, inputs.runtime, inputs.date_parse)
            .expect("BackpackExchange::new with the production native inputs");
        let markets = exchange.as_ref().fetch_markets().await.expect("fetchMarkets over HTTP");
        assert_eq!(markets.iter().map(|m| m.symbol.as_str()).collect::<Vec<_>>(), ["SOL_USDC_PERP"]);
        assert_eq!(markets[0].limits.leverage.max, 20.0, "1 / imf base from the wire");
        assert!(paths.borrow().iter().any(|l| l.starts_with("GET /api/v1/markets")), "{:?}", paths.borrow());
        exchange.as_ref().dispose().expect("dispose");
    });
}
