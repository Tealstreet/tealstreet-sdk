//! The real module-owned generator consumed by constructed Spot placement hosts.
use super::{
    KrakenDependencies, KrakenExchange, SpotIds,
    accepted_read_receivers::terminal::{Harness, harness, options},
    spot_client_ids,
};
use crate::trading::TradingHost;
use safe_cex_exchange::exchange::Exchange;
use safe_cex_num::JsCoercionError;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{ExchangeName, PlaceOrderOpts};
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn constructed(h: &Harness, ids: Rc<RefCell<SpotIds>>) -> Rc<KrakenExchange> {
    KrakenExchange::new(
        ExchangeName::Krakenspot,
        options(),
        h.store.clone(),
        KrakenDependencies {
            runtime: h.runtime.clone(),
            http: h.http.clone(),
            external_public_data: None,
            journal: None,
            futures_nonces: Rc::new(crate::api::FuturesModule::default()),
            spot_client_ids: Some(ids),
        },
    )
    .unwrap()
}
fn prepared() -> Harness {
    let h = harness(ExchangeName::Krakenspot, options(), |http| {
        http.ok(
            "GET",
            "/0/public/AssetPairs",
            json!({"error":[],"result":{"XXBTZUSD":{
                "wsname":"XBT/USD","pair_decimals":1,"lot_decimals":8,"ordermin":"0.0001",
                "status":"online","altname":"XBTUSD"
            }}}),
        );
        for id in ["TX-1", "TX-2", "TX-3"] {
            http.ok(
                "POST",
                "/0/private/AddOrder",
                json!({"error":[],"result":{"txid":[id]}}),
            );
        }
    });
    let markets = h.drive(h.venue().fetch_markets()).unwrap();
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    h
}
fn place(
    h: &Harness,
    exchange: &KrakenExchange,
) -> Result<Vec<String>, safe_cex_exchange::runtime::ExchangeError> {
    let order: PlaceOrderOpts = serde_json::from_value(json!({
        "symbol":"XBT/USD", "type":"limit", "side":"buy", "amount":0.5, "price":30000
    }))
    .unwrap();
    h.drive(Box::pin(exchange.base().place_order(exchange, order)))
}
#[test]
fn spot_ids_share_real_entropy_pool_across_constructed_placement_clients() {
    let h = prepared();
    let fills = Rc::new(Cell::new(0u8));
    let sizes = Rc::new(RefCell::new(Vec::new()));
    let count = fills.clone();
    let seen = sizes.clone();
    let ids: Rc<RefCell<SpotIds>> = spot_client_ids(move |bytes| {
        count.set(count.get() + 1);
        seen.borrow_mut().push(bytes.len());
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = count.get() + (index / 32) as u8;
        }
        Ok(())
    });
    let first = constructed(&h, ids.clone());
    let second = constructed(&h, ids.clone());
    assert!(Rc::ptr_eq(&first.rt.shared.extra.spot_ids, &ids));
    assert!(Rc::ptr_eq(&second.rt.shared.extra.spot_ids, &ids));
    // Independently spell the callable type; this is the actual installed box,
    // not a replacement callback with the same apparent output.
    let mut generator = ids.borrow_mut();
    let typed: &mut Box<dyn FnMut() -> Result<String, JsCoercionError>> = &mut generator;
    assert_eq!(typed().unwrap(), "1".repeat(32));
    drop(generator);
    for (exchange, expected) in [(&first, "TX-1"), (&second, "TX-2"), (&first, "TX-3")] {
        assert_eq!(place(&h, exchange).unwrap(), [expected]);
    }
    assert_eq!(*sizes.borrow(), [32, 512]);
    assert_eq!(fills.get(), 2);
    let calls = h.http.calls("/0/private/AddOrder");
    assert_eq!(calls.len(), 3);
    for (call, digit) in calls.iter().zip(["2", "3", "4"]) {
        let body = call.data.as_str().expect("signed form body");
        assert!(
            body.split('&')
                .any(|part| part == format!("cl_ord_id={}", digit.repeat(32))),
            "{body}"
        );
        assert!(body.contains("volume=0.5"));
        assert!(!call.headers["API-Sign"].as_str().unwrap().is_empty());
    }
}
#[test]
fn spot_ids_entropy_failure_reaches_native_host_and_prevents_order_http() {
    let h = prepared();
    let attempts = Rc::new(Cell::new(0));
    let count = attempts.clone();
    let ids = spot_client_ids(move |_| {
        count.set(count.get() + 1);
        Err(JsCoercionError {
            name: "RangeError",
            message: "entropy unavailable",
        })
    });
    let exchange = constructed(&h, ids);
    let error = TradingHost::spot_client_id(&exchange.rt).unwrap_err();
    assert_eq!(
        error.value.get("name").unwrap().as_str().unwrap().as_ref(),
        &"RangeError".encode_utf16().collect::<Vec<_>>()
    );
    assert_eq!(error.message(), "entropy unavailable");
    let error = place(&h, &exchange).unwrap_err();
    assert!(
        error.message.contains("entropy unavailable"),
        "{}",
        error.message
    );
    assert_eq!(attempts.get(), 2);
    assert!(h.http.calls("/0/private/AddOrder").is_empty());
}
