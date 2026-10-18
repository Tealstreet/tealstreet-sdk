// Accepted-set instrumentation only. Reuse the scripted I/O hosts from the
// concrete venue E2Es; the adapters call production methods, then inspect
// native values and effects independently of owner serialization.
#[allow(dead_code)]
pub mod io {
    use crate as safe_cex_venue_weex;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/binding_runtime.rs"
    ));
}
use safe_cex_exchange::js_runtime::{
    globals::{install_builtins, install_enums},
    library::install_library,
    scope::{Scope, ScopeShared},
    value::{Js, Thrown},
};
use std::{rc::Rc, sync::Arc};

pub fn host() -> crate::binding::rt::Host {
    let runtime = Rc::new(io::Runtime::default());
    let http = safe_cex_exchange::js_runtime::client::rest_http_host(
        runtime.clone(),
        Arc::new(io::Http::default()),
    );
    let shared = Rc::new(ScopeShared::new(
        runtime,
        http,
        Rc::new(safe_cex_store::store::DefaultStore::new()),
        Js::object(vec![]),
        crate::binding::rt::Extra {
            name: safe_cex_types::ExchangeName::Weex,
            options: io::options(),
            external_public_data: None,
            local_to_utc: Rc::new(|ms| ms),
            prototypes: Default::default(),
        },
    ));
    install_builtins(&shared.globals, Rc::downgrade(&shared));
    install_enums(&shared.globals);
    install_library(&shared.globals, Rc::downgrade(&shared));
    let this = Js::object(vec![]);
    *shared.exchange.borrow_mut() = this.clone();
    crate::binding::rt::Host(Scope { shared, this })
}

pub fn settle(mut future: safe_cex_exchange::js_runtime::promise::JsFuture<'_>) -> Js {
    use std::task::{Context, Poll, Waker};
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(result) => result.expect("immediately ready scripted result"),
        Poll::Pending => panic!("unexpected pending scripted result"),
    }
}

pub fn text(value: &Js) -> String {
    String::from_utf16(&value.to_js_string().expect("native string")).unwrap()
}

pub fn error(error: Thrown) -> serde_json::Value {
    let error = error.to_exchange_error();
    serde_json::json!({"error": error.message})
}

pub fn endpoint(name: &str) -> &'static str {
    crate::types::ENDPOINTS
        .iter()
        .find(|(key, _)| *key == name)
        .unwrap()
        .1
}

pub fn setup(alternate: bool) -> io::Harness {
    use safe_cex_store::Store;
    use serde_json::json;
    let symbol = if alternate { "ETHUSDT" } else { "BTCUSDT" };
    let price = if alternate { 202.5 } else { 101.5 };
    let max = if alternate { 40.0 } else { 80.0 };
    let h = io::harness(io::options(), |http| {
        for (name, data) in [
            (
                "EXCHANGE_INFO",
                json!({"symbols":[{"symbol":symbol,"baseAsset":if alternate {"ETH"}else{"BTC"},"quoteAsset":"USDT","marginAsset":"USDT","pricePrecision":1,"quantityPrecision":3,"minOrderSize":"0.001","maxOrderSize":"50","minLeverage":"1","maxLeverage":max,"contractVal":"1"}]}),
            ),
            (
                "TICKER_24HR",
                json!([{ "symbol":symbol,"lastPrice":price,"markPrice":price,"indexPrice":price,"volume":"12","quoteVolume":"1200","priceChangePercent":"3.5" }]),
            ),
            (
                "BOOK_TICKER",
                json!([{ "symbol":symbol,"bidPrice":price-0.1,"askPrice":price+0.1 }]),
            ),
            (
                "ACCOUNT_CONFIG",
                json!({"canTrade":true,"dualSidePosition":false}),
            ),
            (
                "BALANCE",
                json!([{ "asset":"USDT","equity":"150","available":"120","unrealizedPnl":"3" }]),
            ),
            (
                "POSITIONS",
                json!([{ "symbol":symbol,"positionSide":"LONG","size":"2","openValue":"200","leverage":"20","marginType":"CROSS","updatedTime":1700000000000.0 }]),
            ),
            (
                "SYMBOL_CONFIG",
                json!([{ "symbol":symbol,"marginType":"CROSS","separatedMode":"COMBINED","crossLeverage":"20","isolatedLongLeverage":"11","isolatedShortLeverage":"13" }]),
            ),
            (
                "OPEN_ORDERS",
                json!([{ "orderId":if alternate {"order-b"}else{"order-a"},"symbol":symbol,"side":"BUY","type":"LIMIT","status":"NEW","origQty":"2","price":price,"executedQty":"0.25","timeInForce":"GTC","createdTime":1700000000000.0 }]),
            ),
            ("OPEN_ALGO_ORDERS", json!([])),
            (
                "DEPTH",
                json!({"bids":[[price, "2"]],"asks":[[price+1.0,"3"]],"lastUpdateId":"17"}),
            ),
            (
                "PUBLIC_TRADES",
                json!([{ "id":if alternate {"trade-b"}else{"trade-a"},"price":price,"qty":"0.5","side":"SELL","time":1700000000000.0 }]),
            ),
            ("USER_TRADES", json!([])),
        ] {
            http.ok("GET", endpoint(name), data);
        }
        for name in ["SET_LEVERAGE", "SET_MARGIN_TYPE"] {
            http.ok("POST", endpoint(name), json!({"success":true}));
        }
        for name in ["CANCEL_ALL_OPEN_ORDERS", "CANCEL_ALL_ALGO_ORDERS"] {
            http.ok("DELETE", endpoint(name), json!({"success":true}));
        }
        http.ok("POST", endpoint("INCOME"), json!([
            {"symbol":symbol,"incomeType":"FUNDING_FEE","income":if alternate {2.5}else{1.25},"asset":"USDT","time":1700000000000.0,"billId":if alternate {"income-b"}else{"income-a"}}
        ]));
    });
    Store::update(h.store.as_ref(), serde_json::from_value(json!({"markets":[{"id":symbol,"symbol":symbol,"base":if alternate {"ETH"}else{"BTC"},"quote":"USDT","active":true,"precision":{"amount":0.001,"price":0.1},"limits":{"amount":{"min":0.001,"max":50},"leverage":{"min":1,"max":max}}}]})).unwrap(), false).unwrap();
    h
}

pub fn request_body(h: &io::Harness, name: &str) -> serde_json::Value {
    let calls = h.http.calls(endpoint(name));
    serde_json::from_slice(
        &calls
            .last()
            .expect("actual native request")
            .body
            .as_ref()
            .unwrap()
            .wire_bytes()
            .unwrap(),
    )
    .unwrap()
}

pub fn query_symbol(h: &io::Harness, name: &str) -> String {
    let calls = h.http.calls(endpoint(name));
    let request = calls.last().expect("actual native query request");
    request
        .url
        .split_once('?')
        .expect("native query string")
        .1
        .split('&')
        .find_map(|part| part.strip_prefix("symbol="))
        .expect("native symbol query parameter")
        .into()
}
