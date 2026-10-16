//! Concrete production exchange entrypoints over a recording HTTP/socket host.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod binding_harness;
use binding_harness::{Harness, Http, harness, options};
use safe_cex_store::StoreUpdate;
use safe_cex_types::*;
use serde_json::json;
use std::{cell::RefCell, rc::Rc};

fn markets(http: &Http) {
    http.ok("GET","/markets",json!([{"conditionId":"condition","slug":"event","question":"A question?","description":"description","endDate":"2026-12-01","active":true,"closed":false,"enableOrderBook":true,"acceptingOrders":true,"clobTokenIds":"[\"123\",\"456\"]","outcomes":"[\"Yes\",\"No\"]","orderPriceMinTickSize":"0.01","orderMinSize":"5","negRisk":false}]));
}
#[test]
fn concrete_constructor_account_and_source_controls() {
    let h = harness(ExchangeName::Polymarket, options(), |_| {});
    assert_eq!(*h.venue().base().name(), ExchangeName::Polymarket);
    let account = h.drive(h.venue().get_account()).unwrap();
    assert_eq!(account.user_id, "unknown");
    assert_eq!(
        h.drive(h.venue().validate_account()).unwrap().error,
        "No wallet configured".into()
    );
    assert_eq!(
        h.drive(h.venue().fetch_symbol_max_leverage("event-Yes".into()))
            .unwrap(),
        1.
    );
    h.drive(h.venue().set_leverage("event-Yes".into(), 15., None))
        .unwrap();
    h.drive(h.venue().change_position_mode(true, None)).unwrap();
    h.drive(h.venue().change_margin_type(true, None)).unwrap();
    assert_eq!(
        &*h.runtime.logs.borrow(),
        &[
            (
                "warning".into(),
                "Leverage is not supported on Polymarket (always 1x)".into()
            ),
            (
                "warning".into(),
                "Position mode changes not supported on Polymarket".into()
            ),
            (
                "warning".into(),
                "Margin type changes not supported on Polymarket".into()
            ),
        ],
        "source no-op controls retain their exact warnings"
    );
}
#[test]
fn concrete_markets_and_public_start() {
    let h = harness(ExchangeName::Polymarket, options(), |http| {
        markets(http);
        http.ok("POST", "/midpoints", json!({"123":"0.62","456":"0.38"}));
    });
    let rows = h.drive(h.venue().fetch_markets()).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].id, "123");
    assert_eq!(rows[0].symbol, "event-Yes");
    assert_eq!(rows[0].precision.price, 2.);
    assert_eq!(rows[0].limits.amount.min, 5.);
    let tickers = h.drive(h.venue().fetch_tickers()).unwrap();
    assert_eq!(tickers.len(), 2);
    assert_eq!(tickers[0].last, 0.62);
    h.drive(h.venue().start()).unwrap();
    assert!(
        h.store.loaded().markets
            && h.store.loaded().tickers
            && h.store.loaded().positions
            && h.store.loaded().orders
            && h.store.loaded().balance,
        "start commits public snapshots and private loaded flags"
    );
    assert_eq!(
        h.http.calls("/markets")[0].params,
        json!({"active":true,"closed":false,"limit":500.0,"offset":0.0})
    );
    assert!(h.runtime.sockets.borrow().is_empty());
}

fn wallet_options() -> ExchangeOptions {
    let mut opts = options();
    opts.key = format!("0x{}", "01".repeat(32));
    opts.secret = "unused-by-polymarket".into();
    opts
}
fn private_routes(http: &Http) {
    http.ok("GET", "/time", json!(1700000000));
    http.ok(
        "GET",
        "/auth/derive-api-key",
        json!({"apiKey":"account-key","secret":"c2VjcmV0","passphrase":"account-passphrase"}),
    );
    http.ok(
        "GET",
        "/balance-allowance",
        json!({"balance":"123456789","allowance":"999999999"}),
    );
}
#[test]
fn concrete_validate_derives_real_l1_signature_and_l2_headers() {
    let h = harness(ExchangeName::Polymarket, wallet_options(), private_routes);
    let result = h.drive(h.venue().validate_account()).unwrap();
    assert_eq!(
        result.error,
        "".into(),
        "validate_account must bind real EIP712 signer; logs={:?}",
        h.runtime.logs.borrow()
    );
    let l1 = &h.http.calls("/auth/derive-api-key")[0];
    assert_eq!(
        l1.headers["POLY_ADDRESS"],
        "0x1a642f0E3c3aF545E7AcBD38b07251B3990914F1"
    );
    assert_eq!(l1.headers["POLY_TIMESTAMP"], "1700000000");
    assert_eq!(l1.headers["POLY_NONCE"], "0");
    assert_eq!(l1.headers["POLY_SIGNATURE"].as_str().unwrap().len(), 132);
    let l2 = &h.http.calls("/balance-allowance")[0];
    assert_eq!(l2.headers["POLY_API_KEY"], "account-key");
    assert_eq!(l2.headers["POLY_PASSPHRASE"], "account-passphrase");
    assert_eq!(
        l2.params,
        json!({"asset_type":"COLLATERAL","signature_type":0.0})
    );
}
#[test]
fn concrete_wallet_account_matches_ethers_address() {
    let mut opts = options();
    opts.key = format!("0x{}", "01".repeat(32));
    let h = harness(ExchangeName::Polymarket, opts, |_| {});
    let account = h.drive(h.venue().get_account()).unwrap();
    assert_eq!(
        account.user_id,
        "0x1a642f0E3c3aF545E7AcBD38b07251B3990914F1"
    );
}

fn typed<T: serde::de::DeserializeOwned>(v: serde_json::Value) -> T {
    serde_json::from_value(v).unwrap()
}
fn source_order() -> serde_json::Value {
    json!({"id":"rest-order","asset_id":"123","status":"LIVE","order_type":"GTC","side":"BUY","price":"0.57","original_size":"10.25","size_matched":"2.5"})
}
fn normalized_order() -> Order {
    typed(
        json!({"id":"rest-order","symbol":"event-Yes","type":"limit","side":"buy","status":"open","price":0.57,"amount":10.25,"filled":2.5,"remaining":7.75,"reduceOnly":false,"lastUpdate":1700000000000.0}),
    )
}
fn private_harness(script: impl FnOnce(&Http)) -> Harness {
    let h = harness(ExchangeName::Polymarket, wallet_options(), |http| {
        private_routes(http);
        markets(http);
        http.ok("POST", "/midpoints", json!({"123":"0.62","456":"0.38"}));
        script(http);
    });
    let validation = h.drive(h.venue().validate_account()).unwrap();
    assert_eq!(
        validation.error,
        "".into(),
        "private fixture validates: {:?}",
        h.runtime.logs.borrow()
    );
    let markets = h.drive(h.venue().fetch_markets()).unwrap();
    let tickers = h.drive(h.venue().fetch_tickers()).unwrap();
    h.store
        .update_full(
            StoreUpdate {
                markets: Some(markets),
                tickers: Some(tickers),
                ..Default::default()
            },
            false,
        )
        .unwrap();
    h
}
fn place(h: &Harness, v: serde_json::Value) -> Result<Vec<String>, String> {
    h.drive(Box::pin(
        h.venue()
            .base()
            .place_order(h.venue(), typed::<PlaceOrderOpts>(v)),
    ))
    .map_err(|e| e.message)
}
#[test]
fn concrete_fetch_positions_uses_holdings_and_keeps_snapshot_pure() {
    let h = private_harness(|http| {
        http.ok("GET","/positions",json!([
        {"asset":"123","size":10.0,"avgPrice":0.4,"curPrice":0.0,"cashPnl":2.25,"conditionId":"condition","redeemable":false,"mergeable":true},
        {"asset":"unknown","size":20.0,"avgPrice":0.1,"curPrice":0.2,"cashPnl":2.0},
        {"asset":"456","size":0.0,"avgPrice":0.6,"curPrice":0.4,"cashPnl":0.0}
    ]))
    });
    let before = h.store.serialize();
    let positions = h.drive(h.venue().fetch_positions(None)).unwrap();
    assert_eq!(
        positions.len(),
        1,
        "fetch_positions filters unknown and empty holdings"
    );
    assert_eq!(positions[0].symbol, "event-Yes");
    assert_eq!(positions[0].contracts, 10.);
    assert_eq!(positions[0].notional, 6.2);
    assert_eq!(positions[0].unrealized_pnl, 2.25);
    assert_eq!(h.store.serialize(), before, "fetch_positions is pure");
    assert_eq!(
        h.http.calls("/positions")[0].base_url,
        "https://data-api.polymarket.com"
    );
    assert_eq!(
        h.http.calls("/positions")[0].params["user"],
        "0x1a642f0E3c3aF545E7AcBD38b07251B3990914F1"
    );
}
#[test]
fn concrete_fetch_orders_maps_all_pages_without_store_commit() {
    let h = private_harness(|http| {
        http.ok(
            "GET",
            "/data/orders",
            json!({"data":[source_order()],"next_cursor":"page-two"}),
        );
        let mut row = source_order();
        row["id"] = json!("second");
        row["asset_id"] = json!("456");
        http.ok(
            "GET",
            "/data/orders",
            json!({"data":[row],"next_cursor":"LTE="}),
        );
    });
    let before = h.store.serialize();
    let orders = h.drive(h.venue().fetch_orders(None)).unwrap();
    assert_eq!(orders.len(), 2, "fetch_orders retains pages");
    assert_eq!(orders[0].id, "rest-order");
    assert_eq!(orders[1].symbol, "event-No");
    assert_eq!(orders[0].filled, 2.5);
    assert_eq!(orders[0].remaining, 7.75);
    assert_eq!(orders[0].status, OrderStatus::Open);
    assert_eq!(
        h.http.calls("/data/orders")[0].params["next_cursor"],
        "MA=="
    );
    assert_eq!(
        h.http.calls("/data/orders")[1].params["next_cursor"],
        "page-two"
    );
    assert_eq!(h.store.serialize(), before, "fetch_orders is pure");
}
#[test]
fn concrete_place_order_posts_real_signed_gtc_payload() {
    let h = private_harness(|http| {
        http.ok(
            "POST",
            "/order",
            json!({"success":true,"orderID":"placed-id"}),
        )
    });
    let ids = place(
        &h,
        json!({"symbol":"event-Yes","type":"limit","side":"buy","amount":10.259,"price":0.573}),
    )
    .unwrap();
    assert_eq!(ids, vec!["placed-id"], "place_order returns venue ID");
    let request = &h.http.calls("/order")[0];
    let payload: serde_json::Value = serde_json::from_slice(&wire(request)).unwrap();
    assert_eq!(payload["owner"], "account-key");
    assert_eq!(payload["orderType"], "GTC");
    assert_eq!(payload["order"]["tokenId"], "123");
    assert_eq!(payload["order"]["makerAmount"], "5848200");
    assert_eq!(payload["order"]["takerAmount"], "10260000");
    assert_eq!(payload["order"]["side"], "BUY");
    assert_eq!(payload["order"]["signature"].as_str().unwrap().len(), 132);
    let bytes = wire(request);
    let body = String::from_utf8(bytes).unwrap();
    assert!(
        body.starts_with("{\"deferExec\":false,\"postOnly\":false,\"order\":{\"salt\":"),
        "official V2 payload own-key order reaches native wire"
    );
    let preimage = format!(
        "{}POST/order{}",
        request.headers["POLY_TIMESTAMP"].as_str().unwrap(),
        body
    );
    let digest = safe_cex_crypto::hmac_sha256(
        b"secret",
        preimage.as_bytes(),
        safe_cex_crypto::Encoding::Base64,
    )
    .replace('+', "-")
    .replace('/', "_");
    assert_eq!(
        request.headers["POLY_SIGNATURE"], digest,
        "L2 HMAC covers exact transmitted body bytes"
    );
    assert!(
        request.headers["POLY_SIGNATURE"]
            .as_str()
            .unwrap()
            .ends_with('='),
        "canonical URL-safe base64 digest retains padding"
    );
}
#[test]
fn concrete_update_order_cancels_then_signs_replacement() {
    let h = private_harness(|http| {
        http.ok("DELETE", "/order", json!({}));
        http.ok(
            "POST",
            "/order",
            json!({"success":true,"orderID":"replacement"}),
        );
    });
    let update = typed(json!({"order":normalized_order(),"update":{"price":0.55,"amount":12.0}}));
    assert_eq!(
        h.drive(h.venue().update_order(update)).unwrap(),
        vec!["replacement"],
        "update_order replaces"
    );
    let writes = h.http.calls("/order");
    assert_eq!(
        writes.iter().map(|r| r.method.as_str()).collect::<Vec<_>>(),
        vec!["DELETE", "POST"]
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&wire(&writes[0])).unwrap(),
        json!({"orderID":"rest-order"})
    );
    let payload: serde_json::Value = serde_json::from_slice(&wire(&writes[1])).unwrap();
    assert_eq!(payload["order"]["makerAmount"], "6600000");
    assert_eq!(payload["order"]["takerAmount"], "12000000");
}
#[test]
fn concrete_cancel_orders_batches_only_ids() {
    let h = private_harness(|http| http.ok("DELETE", "/orders", json!({})));
    h.drive(h.venue().cancel_orders(vec![normalized_order()]))
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&wire(&h.http.calls("/orders")[0])).unwrap(),
        json!(["rest-order"]),
        "cancel_orders body"
    );
    h.drive(h.venue().cancel_orders(vec![])).unwrap();
    assert_eq!(
        h.http.calls("/orders").len(),
        1,
        "empty cancellation skips network"
    );
}
#[test]
fn concrete_inherited_rejections_match_source() {
    let h = harness(ExchangeName::Polymarket, options(), |_| {});
    assert_eq!(
        h.drive(h.venue().schedule_cancel(Some(Some(123.))))
            .unwrap_err()
            .message,
        "Not implemented",
        "schedule_cancel source rejection"
    );
    assert_eq!(
        h.drive(h.venue().fetch_public_trades("event-Yes".into(), Some(5.)))
            .unwrap_err()
            .message,
        "Not implemented",
        "fetch_public_trades source rejection"
    );
    assert!(h.http.requests.borrow().is_empty());
}
#[test]
fn concrete_client_id_preflight_is_first_and_fail_closed() {
    let h = harness(ExchangeName::Polymarket, options(), |_| {});
    let error=place(&h,json!({"symbol":"unknown","type":"market","side":"buy","amount":1.0,"clientOrderId":"caller-1"})).unwrap_err();
    assert!(
        error.contains("clientOrderId"),
        "client ID wins over unsupported kind: {error}"
    );
    assert!(!error.contains("market orders"));
    assert!(h.http.requests.borrow().is_empty());
}
#[test]
fn concrete_public_ohlcv_and_orderbook_socket() {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_transport::socket::SocketFrame;
    let h = harness(ExchangeName::Polymarket, options(), |http| {
        markets(http);
        http.ok(
            "GET",
            "/prices-history",
            json!({"history":[{"t":1700000000,"p":"0.61"}]}),
        );
    });
    h.drive(h.venue().fetch_markets()).unwrap();
    let candles = h
        .drive(h.venue().fetch_ohlcv(typed(
            json!({"symbol":"event-Yes","interval":"1m","from":1699999990,"to":1700000001}),
        )))
        .unwrap();
    assert_eq!(candles.len(), 1);
    assert_eq!(candles[0].timestamp, 1700000000000.);
    assert_eq!(candles[0].close, 0.61);
    let books = Rc::new(RefCell::new(vec![]));
    let out = books.clone();
    let cleanup = h
        .venue()
        .listen_order_book(
            "event-Yes".into(),
            Rc::new(move |b| out.borrow_mut().push(b)),
        )
        .unwrap();
    h.runtime.pump();
    assert_eq!(h.runtime.sockets.borrow().len(), 1);
    let socket = h.runtime.sockets.borrow()[0].1.clone();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&socket.sent.borrow()[0]).unwrap(),
        json!({"type":"market","assets_ids":["123"],"custom_feature_enabled":true})
    );
    assert_eq!(socket.sent.borrow()[1], "PING");
    socket.events.borrow_mut().push_back(WsEvent::Message(SocketFrame::Text(json!({"event_type":"book","asset_id":"123","bids":[{"price":"0.5","size":"2"}],"asks":[{"price":"0.6","size":"3"}]}).to_string())));
    h.runtime.pump();
    assert_eq!(
        books.borrow().len(),
        1,
        "book callback executes source handler; logs={:?}",
        h.runtime.logs.borrow()
    );
    assert_eq!(books.borrow()[0].bids[0].amount, 2.);
    assert_eq!(books.borrow()[0].asks[0].total, 3.);
    cleanup().unwrap();
    h.venue().dispose().unwrap();
    assert!(h.venue().base().is_disposed());
    assert!(socket.closed.get());
}

fn wire(request: &binding_harness::Request) -> Vec<u8> {
    safe_cex_transport::RestBody::Untransformed(request.data.clone())
        .wire_bytes_for_content_type(Some("application/json"))
        .unwrap()
}

#[test]
fn concrete_private_start_authenticates_and_commits_authoritative_tick() {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_transport::socket::SocketFrame;
    let h = private_harness(|http| {
        http.ok("GET","/positions",json!([{"asset":"123","size":10.0,"avgPrice":0.4,"curPrice":0.62,"cashPnl":2.25,"conditionId":"condition","redeemable":false,"mergeable":false}]));
        http.ok("GET", "/data/orders", json!([source_order()]));
    });
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    assert!(
        h.store.loaded().balance && h.store.loaded().positions && h.store.loaded().orders,
        "private startup publishes all loaded flags"
    );
    assert_eq!(h.store.balance().total, 123.456789);
    assert_eq!(h.store.balance().upnl, 2.25);
    assert_eq!(h.store.positions().len(), 1);
    assert_eq!(h.store.orders().len(), 1);
    assert_eq!(
        h.runtime.sockets.borrow().len(),
        1,
        "private startup leaves public socket lazy"
    );
    let (url, socket) = h.runtime.sockets.borrow()[0].clone();
    assert!(url.ends_with("/ws/user"));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&socket.sent.borrow()[0]).unwrap(),
        json!({"type":"user","auth":{"apiKey":"account-key","secret":"c2VjcmV0","passphrase":"account-passphrase"}})
    );
    assert_eq!(socket.sent.borrow()[1], "PING");
    let reads = h.http.calls("/positions").len();
    socket
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text(
            json!({"authenticated":true}).to_string(),
        )));
    h.runtime.pump();
    assert_eq!(
        h.http.calls("/positions").len(),
        reads + 1,
        "authentication refreshes authoritative holdings"
    );
    h.venue().dispose().unwrap();
    assert!(socket.closed.get());
    let total = h.http.requests.borrow().len();
    h.runtime.advance(20000.);
    assert_eq!(
        h.http.requests.borrow().len(),
        total,
        "disposed polling does not send another request"
    );
}

#[test]
fn concrete_private_read_failure_rethrows_and_preserves_snapshot() {
    let h = private_harness(|http| {
        http.route(
            "GET",
            "/data/orders",
            503.0,
            json!({"error":"unavailable","message":"try later"}),
        )
    });
    h.store
        .update_full(
            StoreUpdate {
                orders: Some(vec![normalized_order()]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
    let before = h.store.serialize();
    let error = h.drive(h.venue().fetch_orders(None)).unwrap_err();
    assert!(
        error.message.contains("try later"),
        "source failure survives: {}",
        error.message
    );
    assert_eq!(
        h.store.serialize(),
        before,
        "failed private read preserves authoritative snapshot"
    );
    assert_eq!(
        h.http.calls("/data/orders").len(),
        1,
        "status error is not retried as network failure"
    );
}
