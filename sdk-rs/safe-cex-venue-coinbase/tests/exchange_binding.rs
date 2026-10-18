//! Production Coinbase class graph over scripted HTTP/WebSocket leaves.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod binding_harness;
use binding_harness::{Http, harness, options};
use safe_cex_exchange::exchange::Exchange;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::*;
use serde_json::{Value, json};

fn script(http: &Http) {
    http.ok("GET", "/api/v3/brokerage/portfolios", json!({"portfolios":[{"name":"Synthetic Spot", "uuid":"portfolio-synthetic", "type":"DEFAULT", "deleted":false}]}));
    http.ok(
        "GET",
        "/api/v3/brokerage/portfolios/portfolio-synthetic",
        json!({"breakdown":{"spot_positions":[{"asset":"USDC","available_to_trade_crypto":13.25,"total_balance_crypto":17.5}]}}),
    );
    http.route(
        "GET",
        "/api/v3/brokerage/cfm/balance_summary",
        403.0,
        json!({"message":"No CFM permissions"}),
    );
    http.ok(
        "GET",
        "/api/v3/brokerage/products",
        json!({"products":[product()]}),
    );
    http.ok(
        "GET",
        "/api/v3/brokerage/orders/historical/batch",
        json!({"orders":[],"has_next":false}),
    );
}
fn product() -> Value {
    json!({"product_id":"BTC-USDC","base_currency_id":"BTC","quote_currency_id":"USDC","price_increment":"0.01","base_increment":"0.001","base_min_size":"0.001","base_max_size":"100", "trading_disabled":false,"view_only":false,"price":"123.5","price_percentage_change_24h":"2.5","volume_24h":"1000"})
}
fn place() -> PlaceOrderOpts {
    serde_json::from_value(json!({"symbol":"BTC-USDC","type":"limit","side":"buy","amount":2.5,"price":123.45,"clientOrderId":"synthetic-caller-id"})).unwrap()
}
fn order() -> Order {
    serde_json::from_value(json!({"id":"order-synthetic","parentId":"synthetic-caller-id","status":"open","symbol":"BTC-USDC","type":"limit","side":"buy","price":123.45,"amount":2.5,"filled":0,"remaining":2.5,"reduceOnly":false,"lastUpdate":1700000000000.0,"info":{"product_id":"BTC-USDC","order_configuration":{"limit_limit_gtc":{"base_size":"2.5","limit_price":"123.45","post_only":false}}}})).unwrap()
}

#[test]
fn constructs_both_variants_and_selects_the_real_portfolio() {
    for name in [ExchangeName::Coinbase, ExchangeName::Coinbasespot] {
        let h = harness(name.clone(), options(), script);
        assert_eq!(h.venue().name(), &name);
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/binding-metadata.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let metadata: Value = serde_json::from_slice(&output.stdout).unwrap();
        let source = &metadata[if name == ExchangeName::Coinbase {
            "coinbase"
        } else {
            "coinbasespot"
        }];
        assert_eq!(
            h.venue().base().settings.supports_public_only,
            source["supportsPublicOnly"].as_bool().unwrap()
        );
        assert_eq!(
            h.venue().base().settings.tick_interval,
            source["tickInterval"].as_f64()
        );
        assert_eq!(
            h.venue().base().settings.positions_supported,
            source["positions"].as_bool()
        );
        let account = h.drive(h.venue().get_account()).unwrap();
        assert_eq!(account.user_id, "portfolio-synthetic");
        assert_eq!(h.http.calls("/api/v3/brokerage/portfolios").len(), 1);
        let request = &h.http.calls("/api/v3/brokerage/portfolios")[0];
        assert!(
            request.headers["Coinbase-Authorization"]
                .as_str()
                .unwrap()
                .starts_with("Bearer eyJ")
        );
        assert_eq!(
            h.venue().class_name(),
            if name == ExchangeName::Coinbase {
                "CoinbaseExchange"
            } else {
                "CoinbaseSpotExchange"
            }
        );
    }
}
#[test]
fn reads_are_pure_and_preserve_native_fields() {
    let h = harness(ExchangeName::Coinbase, options(), script);
    let markets = h.drive(h.venue().fetch_markets()).unwrap();
    assert_eq!(markets.len(), 1);
    assert_eq!(markets[0].symbol, "BTC-USDC");
    assert_eq!(markets[0].precision.price, 0.01);
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let before = h.store.orders().unwrap().len();
    let tickers = h.drive(h.venue().fetch_tickers()).unwrap();
    assert_eq!(tickers[0].last, 123.5);
    assert_eq!(tickers[0].percentage, 2.5);
    let positions = h.drive(h.venue().fetch_positions(None)).unwrap();
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].symbol, "BTC-USDC");
    assert_eq!(positions[0].contracts, 0.0);
    let orders = h.drive(h.venue().fetch_orders(None)).unwrap();
    assert!(orders.is_empty());
    assert_eq!(
        h.http
            .calls("/api/v3/brokerage/orders/historical/batch")
            .len(),
        5
    );
    assert_eq!(h.store.orders().unwrap().len(), before);
    assert!(h.store.tickers().unwrap().is_empty());
    let balance = h.drive(h.exchange.fetch_balance()).unwrap();
    assert_eq!(balance.total, 0.0);
    assert_eq!(
        balance.currencies.as_ref().unwrap()["USDC"]
            .as_balance()
            .unwrap()
            .free,
        13.25
    );
    assert_eq!(
        balance.currencies.as_ref().unwrap()["USDC"]
            .as_balance()
            .unwrap()
            .used,
        4.25
    );
}
#[test]
fn validates_account_with_the_source_error_policy() {
    let h = harness(ExchangeName::Coinbase, options(), script);
    let validation = h.drive(h.venue().validate_account()).unwrap();
    assert_eq!(validation.error.as_value(), "");
    assert_eq!(validation.balance.unwrap().total, 0.0);
    let h = harness(ExchangeName::Coinbase, options(), |http| {
        http.route(
            "GET",
            "/api/v3/brokerage/products",
            401.0,
            json!({"msg":"denied synthetic"}),
        )
    });
    assert_eq!(
        h.drive(h.venue().validate_account())
            .unwrap()
            .error
            .as_value(),
        "denied synthetic"
    );
}
#[test]
fn writes_use_native_client_id_and_real_reconciliation() {
    let h = harness(ExchangeName::Coinbase, options(), |http| {
        script(http);
        http.ok("POST", "/api/v3/brokerage/orders", json!({"success":true,"success_response":{"order_id":"order-synthetic","product_id":"BTC-USDC","side":"BUY","client_order_id":"synthetic-caller-id"}}));
        http.ok(
            "POST",
            "/api/v3/brokerage/orders/batch_cancel",
            json!({"results":[{"success":true}]}),
        );
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
    let ids = h
        .drive(Box::pin(
            h.exchange.base().place_order(h.exchange.as_ref(), place()),
        ))
        .unwrap();
    assert_eq!(ids, vec!["order-synthetic"]);
    let request = &h.http.calls("/api/v3/brokerage/orders")[0];
    assert_eq!(request.data["client_order_id"], "synthetic-caller-id");
    assert_eq!(
        request.data["order_configuration"]["limit_limit_gtc"]["base_size"],
        "2.5"
    );
    assert_eq!(
        h.store.orders().unwrap()[0].parent_id.as_deref(),
        Some("synthetic-caller-id")
    );
    h.drive(h.venue().cancel_orders(vec![order()])).unwrap();
    assert_eq!(h.store.orders().unwrap()[0].status, OrderStatus::Canceled);
    assert_eq!(
        h.http.calls("/api/v3/brokerage/orders/batch_cancel")[0].data,
        json!({"order_ids":["order-synthetic"]})
    );
}
#[test]
fn inherited_unsupported_methods_keep_the_exact_base_error() {
    let h = harness(ExchangeName::Coinbase, options(), |_| {});
    assert_eq!(
        h.drive(h.venue().fetch_symbol_max_leverage("BTC-USDC".into()))
            .unwrap_err()
            .message,
        "Not implemented"
    );
    assert_eq!(
        h.drive(h.venue().set_leverage("BTC-USDC".into(), 5.0, None))
            .unwrap_err()
            .message,
        "Not implemented"
    );
    assert_eq!(
        h.drive(h.venue().change_position_mode(true, None))
            .unwrap_err()
            .message,
        "Not implemented"
    );
    assert_eq!(
        h.drive(h.venue().change_margin_type(true, None))
            .unwrap_err()
            .message,
        "Not implemented"
    );
    assert!(h.http.requests.borrow().is_empty());
}
#[test]
fn starts_both_sockets_and_disposes_the_same_owned_connections() {
    let h = harness(ExchangeName::Coinbase, options(), script);
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    assert!(h.store.loaded().unwrap().markets);
    let sockets = h.runtime.sockets.borrow();
    assert_eq!(sockets.len(), 2);
    let public = sockets
        .iter()
        .find(|(url, _)| url == "wss://advanced-trade-ws.coinbase.com")
        .unwrap()
        .1
        .clone();
    let private = sockets
        .iter()
        .find(|(url, _)| url == "wss://advanced-trade-ws-user.coinbase.com")
        .unwrap()
        .1
        .clone();
    drop(sockets);
    let messages: Vec<Value> = public
        .sent
        .borrow()
        .iter()
        .map(|value| serde_json::from_str(value).unwrap())
        .collect();
    assert_eq!(
        messages[0],
        json!({"type":"subscribe","channel":"heartbeats","product_ids":["BTC-USDC"]})
    );
    let messages: Vec<Value> = private
        .sent
        .borrow()
        .iter()
        .map(|value| serde_json::from_str(value).unwrap())
        .collect();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["channel"], "user");
    assert_eq!(messages[1]["channel"], "heartbeats");
    assert!(messages.iter().all(|message| {
        message["jwt"]
            .as_str()
            .is_some_and(|jwt| jwt.starts_with("eyJ"))
    }));
    h.venue().dispose().unwrap();
    assert!(h.venue().base().is_disposed());
    assert!(public.closed.get());
    assert!(private.closed.get());
}
#[test]
fn edits_only_a_proven_reconciled_native_spot_order() {
    let h = harness(ExchangeName::Coinbase, options(), |http| {
        script(http);
        http.ok(
            "POST",
            "/api/v3/brokerage/orders/edit",
            json!({"success":true,"errors":[]}),
        );
    });
    let mut markets = h.drive(h.venue().fetch_markets()).unwrap();
    markets[0]
        .info
        .as_mut()
        .unwrap()
        .as_object_mut()
        .unwrap()
        .push((
            "product_type".encode_utf16().collect(),
            json!("SPOT").into(),
        ));
    let mut canonical = order();
    let info = canonical.info.as_mut().unwrap();
    info.as_object_mut().unwrap().extend(
        [
            ("order_id", "order-synthetic"),
            ("status", "OPEN"),
            ("order_type", "LIMIT"),
            ("order_side", "BUY"),
            ("filled_size", "0"),
        ]
        .into_iter()
        .map(|(key, value)| (key.encode_utf16().collect(), json!(value).into())),
    );
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            markets: Some(markets),
            orders: Some(vec![canonical.clone()]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let ids = h
        .drive(h.venue().update_order(UpdateOrderOpts {
            order: canonical,
            update: UpdateOrderOptsUpdate::Variant1(UpdateOrderOptsUpdateVariant1 {
                price: 125.0,
                amount: None,
            }),
        }))
        .unwrap();
    assert_eq!(ids, vec!["order-synthetic"]);
    assert_eq!(
        h.http.calls("/api/v3/brokerage/orders/edit")[0].data,
        json!({"order_id":"order-synthetic","price":"125","size":"2.5"})
    );
}
#[test]
fn historical_candles_resolve_aliases_and_preserve_the_native_result() {
    let h = harness(ExchangeName::Coinbase, options(), |http| {
        http.ok("GET", "/api/v3/brokerage/products/BTC-USD/candles", json!({"candles":[{"start":"1700000000","open":"100","high":"130","low":"90","close":"123.5","volume":"4.5"}]}))
    });
    let candles = h
        .drive(
            h.venue().fetch_ohlcv(
                serde_json::from_value(
                    json!({"symbol":"BTC-USDC","interval":"1m","limit":2,"to":1700000120000.0}),
                )
                .unwrap(),
            ),
        )
        .unwrap();
    assert_eq!(candles.len(), 1);
    assert_eq!(candles[0].timestamp, 1700000000.0);
    assert_eq!(candles[0].close, 123.5);
    assert_eq!(candles[0].volume, 4.5);
    let params = &h.http.calls("/api/v3/brokerage/products/BTC-USD/candles")[0].params;
    assert_eq!(params["granularity"], "ONE_MINUTE");
    assert_eq!(params["start"], 1700000000.0);
    assert_eq!(params["end"], 1700000120.0);
}

#[derive(Default)]
struct Hosted {
    requests: std::cell::RefCell<Vec<Value>>,
    fail: std::cell::Cell<bool>,
}
impl safe_cex_transport::signing::RemoteSigner for Hosted {
    fn sign(
        &self,
        request: safe_cex_transport::signing::RemoteSignerRequest,
    ) -> safe_cex_transport::TransportFuture<'_, safe_cex_transport::signing::RemoteSignerResponse>
    {
        let value = serde_json::to_value(&request).unwrap();
        self.requests.borrow_mut().push(value.clone());
        let result = safe_cex_crypto::WsAuthResponse {
            id: value.get("id").cloned(),
            ws_auth_signature: (!self.fail.get()).then(|| "synthetic-hosted-jwt".into()),
            error: self
                .fail
                .get()
                .then(|| "synthetic signer unavailable".into()),
        };
        Box::pin(async move {
            Ok(safe_cex_transport::signing::RemoteSignerResponse::WsAuth(
                result,
            ))
        })
    }
}
impl safe_cex_exchange::runtime::RemoteSigner for Hosted {
    fn is_available(&self) -> bool {
        true
    }
    fn on_availability_change(
        &self,
        _: std::rc::Rc<dyn Fn(bool)>,
    ) -> safe_cex_exchange::runtime::Cleanup {
        Box::new(|| {})
    }
}
#[test]
fn private_ws_uses_shared_hosted_signer_and_sends_nothing_on_failure() {
    for fail in [false, true] {
        let signer = std::rc::Rc::new(Hosted::default());
        signer.fail.set(fail);
        let mut options = options();
        options.secret.clear();
        let h = harness(ExchangeName::Coinbase, options, |http| {
            script(http);
            *http.hosted_signer.borrow_mut() =
                Some((signer.clone(), Some("synthetic-account".into())));
        });
        assert!(!h.venue().is_public_data_only());
        h.drive(h.venue().start()).unwrap();
        h.runtime.pump();
        let sockets = h.runtime.sockets.borrow();
        let private = sockets
            .iter()
            .find(|(url, _)| url == "wss://advanced-trade-ws-user.coinbase.com")
            .unwrap()
            .1
            .clone();
        drop(sockets);
        let requests = signer.requests.borrow();
        assert_eq!(requests.len(), if fail { 1 } else { 2 });
        assert_eq!(
            requests[0],
            json!({"id":"coinbase-ws-auth.user","type":"ws-auth","exchangeName":"coinbase","accountName":"synthetic-account","wsAuth":{"toSign":"coinbase-websocket-jwt.user","outputFormat":"coinbase-jwt","extraData":{"channel":"user"}}})
        );
        if fail {
            assert!(private.sent.borrow().is_empty());
            assert!(private.closed.get());
        } else {
            assert_eq!(private.sent.borrow().len(), 2);
            assert!(
                private
                    .sent
                    .borrow()
                    .iter()
                    .all(|message| message.contains("synthetic-hosted-jwt"))
            );
        }
    }
}
#[test]
fn native_number_bits_survive_reads_and_tick_store_publication() {
    for price in ["-0", "1e999"] {
        let h = harness(ExchangeName::Coinbase, options(), |http| {
            script(http);
            let mut product = product();
            product["price"] = json!(price);
            http.ok(
                "GET",
                "/api/v3/brokerage/products",
                json!({"products":[product]}),
            );
        });
        h.drive(h.venue().start()).unwrap();
        for _ in 0..20 {
            h.runtime.advance(250.0);
        }
        let tickers = h.store.tickers().unwrap();
        assert!(!tickers.is_empty());
        if price == "-0" {
            assert_eq!(tickers[0].last.to_bits(), (-0.0_f64).to_bits());
        } else {
            assert_eq!(tickers[0].last, f64::INFINITY);
        }
        drop(tickers);
        h.venue().dispose().unwrap();
    }
}
#[test]
fn concrete_ws_auth_hook_preserves_request_id_and_source_errors() {
    let h = harness(ExchangeName::Coinbase, options(), |_| {});
    let request = serde_json::from_value(
        json!({"id":"ws-synthetic","wsAuth":{"outputFormat":"coinbase-jwt"}}),
    )
    .unwrap();
    let response = h.drive(h.venue().sign_ws_auth(&request)).unwrap();
    assert_eq!(response.id, Some(json!("ws-synthetic")));
    assert!(response.error.is_none());
    assert!(response.ws_auth_signature.unwrap().starts_with("eyJ"));
    let request =
        serde_json::from_value(json!({"id":17,"wsAuth":{"outputFormat":"unsupported-fixture"}}))
            .unwrap();
    let response = h.drive(h.venue().sign_ws_auth(&request)).unwrap();
    assert_eq!(response.id.as_ref().and_then(Value::as_f64), Some(17.0));
    assert_eq!(
        response.error.as_deref(),
        Some("Unsupported Coinbase WS auth outputFormat: unsupported-fixture")
    );
}
#[test]
fn public_stream_listeners_use_real_parser_and_shared_book_operations() {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_transport::socket::SocketFrame;
    use std::{cell::RefCell, rc::Rc};
    let h = harness(ExchangeName::Coinbase, options(), script);
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    let books = Rc::new(RefCell::new(Vec::new()));
    let capture = books.clone();
    let stop_book = h
        .venue()
        .listen_order_book(
            "BTC-USDC".into(),
            Rc::new(move |book| capture.borrow_mut().push(book)),
        )
        .unwrap();
    let candles = Rc::new(RefCell::new(Vec::new()));
    let capture = candles.clone();
    let stop_candle = h
        .venue()
        .listen_ohlcv(
            serde_json::from_value(json!({"symbol":"BTC-USDC","interval":"5m"})).unwrap(),
            Rc::new(move |candle| capture.borrow_mut().push(candle)),
        )
        .unwrap();
    let public = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url == "wss://advanced-trade-ws.coinbase.com")
        .unwrap()
        .1
        .clone();
    for frame in [
        json!({"channel":"l2_data","events":[{"type":"snapshot","product_id":"BTC-USD","updates":[{"side":"bid","price_level":"40","new_quantity":"2"},{"side":"bid","price_level":"42","new_quantity":"3"},{"side":"offer","price_level":"43","new_quantity":"4"}]}]}),
        json!({"channel":"candles","events":[{"type":"update","candles":[{"product_id":"BTC-USD","start":"1700000000","open":"40","high":"45","low":"39","close":"44","volume":"7"}]}]}),
    ] {
        public
            .events
            .borrow_mut()
            .push_back(WsEvent::Message(SocketFrame::Text(frame.to_string())));
    }
    h.runtime.pump();
    assert_eq!(books.borrow().len(), 1);
    assert_eq!(books.borrow()[0].bids[0].price, 42.0);
    assert_eq!(books.borrow()[0].bids[1].total, 5.0);
    assert_eq!(books.borrow()[0].asks[0].amount, 4.0);
    assert_eq!(candles.borrow().len(), 1);
    assert_eq!(candles.borrow()[0].timestamp, 1700000000.0);
    assert_eq!(candles.borrow()[0].close, 44.0);
    stop_book().unwrap();
    stop_candle().unwrap();
    assert!(
        public
            .sent
            .borrow()
            .iter()
            .any(|message| message.contains("unsubscribe"))
    );
    h.venue().dispose().unwrap();
}
#[test]
fn public_book_copyback_preserves_duplicate_price_levels() {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_transport::socket::SocketFrame;
    use std::{
        cell::RefCell,
        io::Write,
        process::{Command, Stdio},
        rc::Rc,
    };
    let frames = json!([
        {"channel":"l2_data","events":[{"type":"snapshot","product_id":"BTC-USD","updates":[
            {"side":"bid","price_level":"42","new_quantity":"2"},
            {"side":"bid","price_level":"42","new_quantity":"3"},
            {"side":"bid","price_level":"40","new_quantity":"7"},
            {"side":"offer","price_level":"43","new_quantity":"4"},
            {"side":"offer","price_level":"43","new_quantity":"5"}]}]},
        {"channel":"l2_data","events":[{"type":"update","product_id":"BTC-USD","updates":[
            {"side":"bid","price_level":"42","new_quantity":"8"},
            {"side":"offer","price_level":"43","new_quantity":"0"}]}]},
        {"channel":"l2_data","events":[{"type":"update","product_id":"BTC-USD","updates":[
            {"side":"bid","price_level":"41","new_quantity":"6"},
            {"side":"offer","price_level":"44","new_quantity":"9"}]}]}
    ]);
    let mut source = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/book-copyback-binding.cjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    source
        .stdin
        .take()
        .unwrap()
        .write_all(frames.to_string().as_bytes())
        .unwrap();
    let source = source.wait_with_output().unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let expected: Value = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(expected[0]["bids"][0]["amount"], 2);
    assert_eq!(expected[0]["bids"][1]["amount"], 3);
    assert_eq!(expected[0]["bids"][1]["total"], 5);
    let expected_books: Vec<OrderBook> = serde_json::from_value(expected).unwrap();
    for name in [ExchangeName::Coinbase, ExchangeName::Coinbasespot] {
        let h = harness(name, options(), script);
        h.drive(h.venue().start()).unwrap();
        h.runtime.pump();
        let captures = Rc::new(RefCell::new(Vec::new()));
        let observed = captures.clone();
        let cleanup = h
            .venue()
            .listen_order_book(
                "BTC-USDC".into(),
                Rc::new(move |book| {
                    observed.borrow_mut().push(book);
                }),
            )
            .unwrap();
        let public = h
            .runtime
            .sockets
            .borrow()
            .iter()
            .find(|(url, _)| url == "wss://advanced-trade-ws.coinbase.com")
            .unwrap()
            .1
            .clone();
        for frame in frames.as_array().unwrap() {
            public
                .events
                .borrow_mut()
                .push_back(WsEvent::Message(SocketFrame::Text(frame.to_string())));
            h.runtime.pump();
        }
        assert_eq!(
            captures.borrow().len(),
            3,
            "every real public frame reaches the listener"
        );
        assert_eq!(
            &*captures.borrow(),
            &expected_books,
            "real book copyback must retain each source level and running total"
        );
        cleanup().unwrap();
        h.venue().dispose().unwrap();
    }
}
#[test]
fn startup_publication_keeps_store_subscriber_cause_after_commit() {
    use std::{cell::Cell, error::Error, rc::Rc};
    let h = harness(ExchangeName::Coinbase, options(), script);
    let cause: Rc<dyn Error> = Rc::new(std::io::Error::other("synthetic subscriber cause"));
    let observed = cause.clone();
    let _first = Store::subscribe(
        h.store.as_ref(),
        Box::new(move |data, _| {
            assert_eq!(data.markets.len(), 1);
            assert!(data.loaded.markets);
            Err(StoreSubscriberError::new("synthetic subscriber failure")
                .with_cause(observed.clone()))
        }),
    );
    let later = Rc::new(Cell::new(0));
    let capture = later.clone();
    let _later = Store::subscribe(
        h.store.as_ref(),
        Box::new(move |_, _| {
            capture.set(capture.get() + 1);
            Ok(())
        }),
    );
    let error = h.drive(h.venue().start()).unwrap_err();
    assert_eq!(error.message, "synthetic subscriber failure");
    assert!(!error.store_subscriber_ids.is_empty());
    assert_eq!(
        error.store_subscriber_cause.as_ref().unwrap(),
        &StoreSubscriberError::new("synthetic subscriber failure").with_cause(cause)
    );
    assert_eq!(later.get(), 0);
    assert!(h.store.loaded().unwrap().markets);
    assert_eq!(h.store.markets().unwrap().len(), 1);
    assert!(h.runtime.sockets.borrow().is_empty());
}
#[test]
fn cached_balance_fallback_keeps_native_optional_fields_and_nested_currencies() {
    let h = harness(ExchangeName::Coinbase, options(), |http| {
        script(http);
        http.routes
            .borrow_mut()
            .retain(|(_, path, _)| path != "/api/v3/brokerage/cfm/balance_summary");
        http.ok(
            "GET",
            "/api/v3/brokerage/cfm/balance_summary",
            json!({"balance_summary":{}}),
        );
        http.route(
            "GET",
            "/api/v3/brokerage/cfm/balance_summary",
            503.0,
            json!({"message":"synthetic unavailable"}),
        );
        // First breakdown selects the portfolio; the next rejected balance read
        // must retain the original Store balance's complete native shape.
        http.route(
            "GET",
            "/api/v3/brokerage/portfolios/portfolio-synthetic",
            503.0,
            json!({"message":"synthetic unavailable"}),
        );
    });
    h.drive(h.venue().get_account()).unwrap();
    let mut cached: Balance = serde_json::from_value(json!({"total":100,"free":70,"used":30,"upnl":2,"info":{"literal":"retained"},"coins":{"BTC":{"base":"BTC","amount":1,"notional":3}},"currencies":{"USDC":{"total":9,"free":7,"used":2,"upnl":0,"info":{"nested":true}}}})).unwrap();
    cached
        .coins
        .as_mut()
        .unwrap()
        .get_mut("BTC")
        .unwrap()
        .amount = f64::INFINITY;
    cached
        .coins
        .as_mut()
        .unwrap()
        .get_mut("BTC")
        .unwrap()
        .notional = -0.0;
    cached.currencies.as_mut().unwrap().insert(
        "native-alternate".into(),
        safe_cex_types::CurrencyBalance::AvailableLocked {
            available: f64::INFINITY,
            locked: -0.0,
        },
    );
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            balance: Some(cached),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let balance = h.drive(h.exchange.fetch_balance()).unwrap();
    let safe_cex_types::CurrencyBalance::AvailableLocked { available, locked } =
        &balance.currencies.as_ref().unwrap()["native-alternate"]
    else {
        panic!("cached native row was normalized")
    };
    assert_eq!(*available, f64::INFINITY);
    assert_eq!(locked.to_bits(), (-0.0_f64).to_bits());
    assert_eq!(balance.info, Some(json!({"literal":"retained"}).into()));
    assert_eq!(balance.coins.as_ref().unwrap()["BTC"].amount, f64::INFINITY);
    assert_eq!(
        balance.coins.as_ref().unwrap()["BTC"].notional.to_bits(),
        (-0.0_f64).to_bits()
    );
    assert_eq!(
        balance.currencies.as_ref().unwrap()["USDC"]
            .as_balance()
            .unwrap()
            .info,
        Some(json!({"nested":true}).into())
    );
}

#[test]
fn portfolios_do_not_require_unobserved_name_or_deleted_fields() {
    for name in [ExchangeName::Coinbase, ExchangeName::Coinbasespot] {
        for extra in [
            json!({}),
            json!({"name":null}),
            json!({"deleted":"unobserved"}),
            json!({"name":[],"deleted":{}}),
        ] {
            let h = harness(name.clone(), options(), |http| {
                script(http);
                http.routes
                    .borrow_mut()
                    .retain(|(_, path, _)| path != "/api/v3/brokerage/portfolios");
                let mut portfolio = json!({"uuid":"portfolio-synthetic","type":"DEFAULT"});
                portfolio
                    .as_object_mut()
                    .unwrap()
                    .extend(extra.as_object().unwrap().clone());
                http.ok(
                    "GET",
                    "/api/v3/brokerage/portfolios",
                    json!({"portfolios":[portfolio]}),
                );
            });
            let account = h
                .drive(h.venue().get_account())
                .expect("unused portfolio metadata must not reject selection");
            assert_eq!(account.user_id, "portfolio-synthetic");
            let balance = h.drive(h.exchange.fetch_balance()).unwrap();
            let usdc = balance.currencies.as_ref().unwrap()["USDC"]
                .as_balance()
                .unwrap();
            assert_eq!(
                (usdc.free, usdc.used),
                (13.25, 4.25),
                "real breakdown must load after selecting the sparse portfolio"
            );
            h.venue().dispose().unwrap();
        }
    }
}

fn source_uuid_profile() -> Value {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/uuid-binding.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn uuid_entropy(h: &binding_harness::Harness) {
    // nanoid's raw21 starts with both removable separators, then19 'M'.
    h.runtime
        .random_values
        .borrow_mut()
        .extend([8.0 / 64.0, 44.0 / 64.0]);
}
fn load_uuid_market(h: &binding_harness::Harness) {
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
}
#[test]
fn uuid_binding_mints_source_client_ids() {
    let expected = source_uuid_profile();
    for name in [ExchangeName::Coinbase, ExchangeName::Coinbasespot] {
        let h = harness(name, options(), script);
        load_uuid_market(&h);
        uuid_entropy(&h);
        let before = h.runtime.random_calls.get();
        assert_eq!(
            h.venue()
                .base()
                .mint_client_order_id(place())
                .unwrap()
                .unwrap(),
            expected["minted"].as_str().unwrap()
        );
        assert_eq!(h.runtime.random_calls.get() - before, 21);
        assert!(h.runtime.random_values.borrow().is_empty());
    }
}
#[test]
fn uuid_binding_generated_order_ids_reach_actual_http() {
    let expected = source_uuid_profile();
    for name in [ExchangeName::Coinbase, ExchangeName::Coinbasespot] {
        let h = harness(name, options(), |http| {
            script(http);
            http.ok("POST", "/api/v3/brokerage/orders", json!({"success":true,"success_response":{"order_id":"uuid-order","product_id":"BTC-USDC","side":"BUY"}}));
        });
        load_uuid_market(&h);
        uuid_entropy(&h);
        let mut opts = place();
        opts.client_order_id = None;
        let ids = h
            .drive(Box::pin(
                h.exchange.base().place_order(h.exchange.as_ref(), opts),
            ))
            .unwrap();
        assert_eq!(ids, ["uuid-order"]);
        let calls = h.http.calls("/api/v3/brokerage/orders");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].data["client_order_id"], expected["minted"]);
        assert!(h.runtime.random_values.borrow().is_empty());
        assert_eq!(
            h.store.orders().unwrap()[0].parent_id.as_deref(),
            expected["minted"].as_str()
        );
    }
}
#[test]
fn uuid_binding_jwt_nonce_reaches_actual_signed_http() {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    let expected = source_uuid_profile();
    for name in [ExchangeName::Coinbase, ExchangeName::Coinbasespot] {
        let h = harness(name, options(), script);
        uuid_entropy(&h);
        h.drive(h.venue().get_account()).unwrap();
        let calls = h.http.calls("/api/v3/brokerage/portfolios");
        assert_eq!(calls.len(), 1);
        let headers = calls[0].headers.as_object().unwrap();
        let token = headers
            .iter()
            .find(|(k, _)| k.ends_with("Authorization"))
            .unwrap()
            .1
            .as_str()
            .unwrap()
            .strip_prefix("Bearer ")
            .unwrap();
        let header: Value = serde_json::from_slice(
            &URL_SAFE_NO_PAD
                .decode(token.split('.').next().unwrap())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(header["nonce"], expected["nonce"]);
        assert_eq!(header["alg"], "EdDSA");
        let (payload, signature) = token.rsplit_once('.').unwrap();
        let public_key: Vec<u8> = serde_json::from_value(expected["publicKey"].clone()).unwrap();
        assert!(
            safe_cex_crypto::ed25519::verify(
                payload.as_bytes(),
                &public_key,
                &URL_SAFE_NO_PAD.decode(signature).unwrap()
            )
            .unwrap(),
            "actual JWT is verifiably signed"
        );
        assert!(h.runtime.random_values.borrow().is_empty());
    }
}

fn source_credential_admission() -> Vec<Value> {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/credential-binding.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
#[test]
fn credential_binding_ws_auth_matches_source_organization_admission() {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    let source = source_credential_admission();
    assert_eq!(source.len(), 8);
    for name in [ExchangeName::Coinbase, ExchangeName::Coinbasespot] {
        for (index, (id, key)) in [
            ("valid", "organizations/org/apiKeys/id".to_owned()),
            ("org-slash", "organizations/org/child/apiKeys/id".to_owned()),
            (
                "org-double-slash",
                "organizations/org//child/apiKeys/id".to_owned(),
            ),
            ("key-slash", "organizations/org/apiKeys/id/child".to_owned()),
            ("empty-org", "organizations//apiKeys/id".to_owned()),
            ("unicode", "organizations/雪/apiKeys/clé".to_owned()),
            ("org-space", "organizations/org child/apiKeys/id".to_owned()),
            (
                "json-org-slash",
                json!({"name":"organizations/org/child/apiKeys/id"}).to_string(),
            ),
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(source[index]["id"], id);
            assert_eq!(source[index]["key"], key);
            let mut opts = options();
            opts.key = key;
            let h = harness(name.clone(), opts, |_| {});
            let request =
                serde_json::from_value(json!({"id":id,"wsAuth":{"outputFormat":"coinbase-jwt"}}))
                    .unwrap();
            let response = h.drive(h.venue().sign_ws_auth(&request)).unwrap();
            assert_eq!(response.id, Some(json!(id)));
            if let Some(error) = source[index]["error"].as_str() {
                assert_eq!(
                    response.error.as_deref(),
                    Some(error),
                    "{name:?}/{id}: source preflight refusal"
                );
                assert!(
                    response.ws_auth_signature.is_none(),
                    "{id}: rejected credentials cannot mint a token"
                );
            } else {
                assert!(response.error.is_none(), "{id}");
                let token = response.ws_auth_signature.unwrap();
                let header: Value = serde_json::from_slice(
                    &URL_SAFE_NO_PAD
                        .decode(token.split('.').next().unwrap())
                        .unwrap(),
                )
                .unwrap();
                assert_eq!(header["kid"], source[index]["keyName"], "{id}");
            }
            assert!(h.http.requests.borrow().is_empty(), "WS signing is local");
        }
    }
}
#[test]
fn credential_binding_refuses_organization_slash_before_http() {
    let source = source_credential_admission();
    assert!(
        source[1]["error"]
            .as_str()
            .unwrap()
            .contains("Coinbase API key")
    );
    for name in [ExchangeName::Coinbase, ExchangeName::Coinbasespot] {
        let mut opts = options();
        opts.key = "organizations/org/child/apiKeys/id".into();
        let h = harness(name, opts, script);
        let _ = h.drive(h.venue().get_account());
        assert!(
            h.http.requests.borrow().is_empty(),
            "source credential validation runs before any actual HTTP request: {:?}",
            h.http.requests.borrow()
        );
    }
}

#[path = "../../parity/test-support/ticker_coercion.rs"]
mod ticker_coercion;

#[test]
fn production_ticker_coercion_matches_constructed_typescript() {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_transport::socket::SocketFrame;
    let source = ticker_coercion::source("coinbase");
    assert_eq!(source["cases"].as_array().unwrap().len(), 56);
    for name in [ExchangeName::Coinbase, ExchangeName::Coinbasespot] {
        let h = harness(name, options(), script);
        h.drive(h.venue().start()).unwrap();
        h.runtime.pump();
        let socket = h
            .runtime
            .sockets
            .borrow()
            .iter()
            .find(|(url, _)| url == "wss://advanced-trade-ws.coinbase.com")
            .expect("constructed public socket")
            .1
            .clone();
        for case in source["cases"].as_array().unwrap() {
            Store::update(
                h.store.as_ref(),
                StoreUpdate {
                    tickers: Some(ticker_coercion::seed("coinbase")),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
            for frame in case["frames"].as_array().unwrap() {
                socket
                    .events
                    .borrow_mut()
                    .push_back(WsEvent::Message(SocketFrame::Text(
                        frame.as_str().unwrap().into(),
                    )));
                h.runtime.pump();
            }
            ticker_coercion::assert_native(&h.store.tickers().unwrap(), case);
        }
        assert_eq!(
            h.runtime
                .console
                .borrow()
                .iter()
                .filter(|(level, message)| level == "error"
                    && message == "No handler for channel ticker_batch")
                .count(),
            56,
            "native ticker unsupported channel must be reported"
        );
        assert!(
            h.runtime.reports.borrow().is_empty(),
            "{:?}",
            h.runtime.reports.borrow()
        );
        h.venue().dispose().unwrap();
    }
}

fn divergence_binding_source() -> Value {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/divergence-binding.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn divergence_pem_failure_reaches_real_ws_and_rest_bindings() {
    let source = divergence_binding_source();
    let payloads = [
        "A",
        "!",
        "A!",
        "Zg=",
        "Zg===",
        "-___",
        "AQID\u{0085}",
        "AAAAA",
        "!AAAA",
        "AAAA!",
        "A===",
    ];
    let cases = source["pem"].as_array().unwrap();
    assert_eq!(cases.len(), payloads.len(), "separate live signing corpus");
    for name in [ExchangeName::Coinbase, ExchangeName::Coinbasespot] {
        // Obtain native markets through the original valid public/account graph.
        // A malformed credential can otherwise prevent discovery before signing.
        let admitted = harness(name.clone(), options(), script);
        let markets = admitted.drive(admitted.venue().fetch_markets()).unwrap();
        assert!(markets.iter().any(|market| market.symbol == "BTC-USDC"));
        for (case, payload) in cases.iter().zip(payloads) {
            assert_eq!(case["payload"], payload, "signing payload identity");
            assert_eq!(case["name"], "InvalidCharacterError");
            let secret =
                format!("-----BEGIN PRIVATE KEY-----\n{payload}\n-----END PRIVATE KEY-----");
            assert_eq!(case["secret"], secret, "exact original PEM and markers");
            let mut opts = options();
            opts.secret = secret;
            let h = harness(name.clone(), opts, script);
            let request = serde_json::from_value(
                json!({"id":payload,"wsAuth":{"outputFormat":"coinbase-jwt"}}),
            )
            .unwrap();
            let response = h.drive(h.venue().sign_ws_auth(&request)).unwrap();
            assert_eq!(response.id, Some(json!(payload)), "same native request ID");
            assert_eq!(
                response.error.as_deref(),
                case["message"].as_str(),
                "{name:?}/{payload:?}: source PEM error visible at actual WS hook"
            );
            assert!(response.ws_auth_signature.is_none());
            assert!(
                h.http.requests.borrow().is_empty(),
                "malformed PEM sends no HTTP"
            );
            Store::update(
                h.store.as_ref(),
                StoreUpdate {
                    markets: Some(markets.clone()),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
            h.http.requests.borrow_mut().clear();
            let error = h
                .drive(Box::pin(
                    h.exchange.base().place_order(h.exchange.as_ref(), place()),
                ))
                .unwrap_err();
            assert_eq!(
                error.message,
                case["message"].as_str().unwrap(),
                "{name:?}/{payload:?}: source PEM error visible at actual REST placement"
            );
            assert!(
                h.http.requests.borrow().is_empty(),
                "failed signer cannot dispatch placement"
            );
            assert!(
                h.store.orders().unwrap().is_empty(),
                "failed signer creates no optimistic order"
            );
            h.venue().dispose().unwrap();
        }
    }
}

#[test]
fn divergence_starbase_intent_loss_is_outside_live_placement() {
    use safe_cex_venue_coinbase::{
        protocol, starbase_cutover::select_coinbase_starbase_cutover_plan,
    };
    let source = divergence_binding_source();
    assert_eq!(
        source["starbase"]["blocker"],
        "contract-unit-conversion-unverified"
    );
    assert_eq!(source["starbase"]["retained"], json!({"retained":true}));
    assert_eq!(
        source["starbase"]["keys"],
        json!([
            "unmodeledRuntimeField",
            "savedSymbol",
            "side",
            "size",
            "type",
            "price"
        ])
    );
    assert_eq!(source["starbase"]["sourcePlacement"], "legacy-rest");
    let plan = serde_json::to_value(select_coinbase_starbase_cutover_plan().unwrap()).unwrap();
    assert_eq!(plan["authoritative"]["writes"], "legacy-coinbase");
    assert_eq!(plan["httpShadow"]["active"], false);
    assert_eq!(plan["websocket"]["active"], false);
    assert_eq!(plan["cutover"]["runtimeActivationEnabled"], false);
    assert!(!std::hint::black_box(
        protocol::COINBASE_STARBASE_RUNTIME_ACTIVATION_ENABLED
    ));
    for name in [ExchangeName::Coinbase, ExchangeName::Coinbasespot] {
        let h = harness(name, options(), |http| {
            script(http);
            http.ok("POST", "/api/v3/brokerage/orders", json!({"success":true,"success_response":{"order_id":"audit-legacy","product_id":"BTC-USDC","side":"BUY"}}));
        });
        load_uuid_market(&h);
        let ids = h
            .drive(Box::pin(
                h.exchange.base().place_order(h.exchange.as_ref(), place()),
            ))
            .unwrap();
        assert_eq!(ids, ["audit-legacy"]);
        let calls = h.http.calls("/api/v3/brokerage/orders");
        assert_eq!(
            calls.len(),
            1,
            "constructed exchange dispatches authoritative REST placement"
        );
        assert_eq!(calls[0].data["product_id"], "BTC-USDC");
        assert_eq!(calls[0].data["client_order_id"], "synthetic-caller-id");
        assert_eq!(
            calls[0].data["order_configuration"]["limit_limit_gtc"]["base_size"],
            "2.5"
        );
        assert!(
            calls[0].data.get("method").is_none(),
            "no Starbase RPC request"
        );
        assert_eq!(h.store.orders().unwrap()[0].id, "audit-legacy");
        assert_eq!(
            h.store.orders().unwrap()[0].parent_id.as_deref(),
            Some("synthetic-caller-id")
        );
        assert!(
            h.runtime.sockets.borrow().is_empty(),
            "Starbase websocket stays inert"
        );
        h.venue().dispose().unwrap();
    }
}

#[test]
fn divergence_starbase_typed_intent_identity() {
    // The native blocked gate retains the exact typed caller, too. Serialization
    // of that closed DTO has no runtime-only own fields; it is not a wire path.
    use safe_cex_venue_coinbase::starbase_orders::*;
    let intent = CoinbaseStarbasePlaceOrderIntent {
        label: None,
        otoco: None,
        post_only: None,
        price: Some(-0.0),
        reduce_only: Some(false),
        reject_post_only: Some(true),
        saved_symbol: "BTC-PERP-INTX".into(),
        side: CoinbaseStarbaseWriteSide::Buy,
        size: CoinbaseStarbaseWriteAmountIntent::Contracts(CoinbaseStarbaseContractAmountIntent {
            total_contracts: 2.0,
            source: CoinbaseStarbaseContractAmountSource::GatewayContractCount,
            unit: CoinbaseStarbaseContractAmountUnit::Contracts,
        }),
        time_in_force: None,
        trigger: None,
        trigger_offset: None,
        trigger_price: None,
        r#type: CoinbaseStarbaseWriteOrderType::Limit,
        valid_until: None,
    };
    match build_coinbase_starbase_place_order(&intent).unwrap() {
        CoinbaseStarbaseWriteGate::Blocked {
            blocker,
            intent: retained,
        } => {
            assert!(
                std::ptr::eq(retained, &intent),
                "native blocked gate borrows the exact caller"
            );
            assert_eq!(
                blocker,
                CoinbaseStarbaseWriteParityBlocker::ContractUnitConversionUnverified
            );
            assert_eq!(retained.price.unwrap().to_bits(), (-0.0_f64).to_bits());
            assert_eq!(retained.reduce_only, Some(false));
            assert_eq!(retained.reject_post_only, Some(true));
        }
        CoinbaseStarbaseWriteGate::Ready { .. } => panic!("unverified contracts must stay blocked"),
    }
}
