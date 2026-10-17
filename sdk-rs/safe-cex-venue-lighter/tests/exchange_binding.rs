//! Production LighterNativeExchange through the Exchange trait over a scripted transport.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod harness;
use harness::*;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::*;
use serde_json::{Value, json};

const DETAILS: &str = "/api/v1/orderBookDetails";
const ACCOUNT: &str = "/api/v1/account";
const LIMITS: &str = "/api/v1/accountLimits";
const ACTIVE_ORDERS: &str = "/api/v1/accountActiveOrders";
const NEXT_NONCE: &str = "/api/v1/nextNonce";
const SEND_TX: &str = "/api/v1/sendTx";
const EXCHANGE_STATS: &str = "/api/v1/exchangeStats";
const INTEGRATOR: u64 = 158_385;
const NONCE: u64 = 7;

fn perp_row() -> Value {
    json!({
        "symbol": "BTC", "market_id": 1, "market_type": "perp", "status": "active",
        "supported_size_decimals": 4, "supported_price_decimals": 1,
        "min_base_amount": "0.0001", "min_quote_amount": "10",
        "order_quote_limit": "1000", "min_initial_margin_fraction": 200,
        "market_config": {"market_margin_mode": 0}
    })
}

fn spot_row() -> Value {
    json!({
        "symbol": "ETH/USDC", "market_id": 2048, "market_type": "spot", "status": "active",
        "supported_size_decimals": 4, "supported_price_decimals": 2,
        "min_base_amount": "0.001", "min_quote_amount": "10"
    })
}

fn account() -> Value {
    json!({
        "code": 200, "total": 1,
        "accounts": [{
            "account_index": ACCOUNT_INDEX, "index": ACCOUNT_INDEX, "referrer": "ref-7",
            "collateral": "1000", "total_asset_value": "1010", "available_balance": "800",
            "positions": [{
                "market_id": 1, "symbol": "BTC", "sign": 1, "position": "0.5",
                "avg_entry_price": "50000", "unrealized_pnl": "10", "realized_pnl": "0",
                "initial_margin_fraction": "10.00", "liquidation_price": "40000",
                "margin_mode": 0
            }],
            "approved_integrators": [{"account_index": INTEGRATOR, "approval_expiry": 4_102_444_800_000u64}]
        }]
    })
}

fn active_order() -> Value {
    json!({
        "order_index": 777, "client_order_index": 777, "market_index": 1, "is_ask": false,
        "price": "49000.0", "initial_base_amount": "0.5000", "remaining_base_amount": "0.5000",
        "filled_base_amount": "0.0000", "type": "limit", "time_in_force": "good-till-time",
        "reduce_only": false, "status": "open", "timestamp": 1_700_000_000
    })
}

fn lighter(http: &Http) {
    http.ok(
        "GET",
        DETAILS,
        json!({"code": 200, "order_book_details": [perp_row(), spot_row()]}),
    );
    http.ok("GET", ACCOUNT, account());
    http.ok(
        "GET",
        LIMITS,
        json!({"code": 200, "user_tier": "premium", "user_tier_name": "Premium"}),
    );
    http.ok(
        "GET",
        ACTIVE_ORDERS,
        json!({"code": 200, "orders": [active_order()]}),
    );
    http.ok("GET", NEXT_NONCE, json!({"code": 200, "nonce": NONCE}));
    http.ok("POST", SEND_TX, json!({"code": 200, "tx_hash": "0xfeed"}));
    http.ok(
        "GET",
        EXCHANGE_STATS,
        json!({"code": 200, "order_book_stats": []}),
    );
}

fn loaded(h: &Harness) {
    let markets = h.drive(h.venue().fetch_markets()).expect("markets");
    Store::update(
        &*h.store,
        StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .expect("store markets");
}

fn limit_order(extra: Value) -> PlaceOrderOpts {
    let mut opts = json!({
        "symbol": "BTC", "type": "limit", "side": "buy", "amount": 0.5, "price": 50000
    });
    opts.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    serde_json::from_value(opts).expect("PlaceOrderOpts")
}

/// The signed transactions sent since `from`, as `(tx_type, tx_info)`.
fn sent_txs(h: &Harness, from: usize) -> Vec<(String, Value)> {
    h.http.requests.borrow()[from..]
        .iter()
        .filter(|r| r.url == SEND_TX)
        .map(|r| {
            assert_eq!(r.method, "POST");
            (
                r.data["tx_type"].as_str().expect("tx_type").to_owned(),
                r.data["tx_info"].clone(),
            )
        })
        .collect()
}

fn assert_signed_by_account(info: &Value) {
    assert_eq!(info["AccountIndex"], json!(ACCOUNT_INDEX), "{info}");
    assert_eq!(info["ApiKeyIndex"], json!(API_KEY_INDEX), "{info}");
    let sig = info["Sig"].as_str().expect("Sig present");
    assert_eq!(
        sig.len(),
        108,
        "base64 of a 80-byte Schnorr signature: {sig}"
    );
}

/// The first nonce comes from nextNonce; later ones from the client's cache.
fn assert_nonce_read(h: &Harness) {
    let reads = h.http.calls(NEXT_NONCE);
    assert_eq!(reads.len(), 1, "one nextNonce read seeds the cache");
    assert_eq!(reads[0].params["account_index"], json!("42"));
    assert_eq!(
        reads[0].params["api_key_index"].as_f64(),
        Some(API_KEY_INDEX as f64)
    );
}

fn auth_token_prefix() -> String {
    format!(":{ACCOUNT_INDEX}:{API_KEY_INDEX}:")
}

#[test]
fn fetch_markets_maps_perps_and_drops_spot_rows() {
    let h = harness(ExchangeName::Lighter, options(), lighter);
    let markets = h.drive(h.venue().fetch_markets()).expect("fetchMarkets");
    assert_eq!(h.http.urls(), vec![DETAILS], "one orderBookDetails read");
    assert_eq!(h.http.calls(DETAILS)[0].method, "GET");
    assert_eq!(markets.len(), 1, "the spot row is filtered: {markets:?}");
    let btc = &markets[0];
    assert_eq!(btc.symbol, "BTC");
    assert_eq!(btc.id, "1");
    assert_eq!(btc.base, "BTC");
    assert_eq!(btc.quote, "USDC");
    assert_eq!(btc.precision.amount, 0.0001);
    assert_eq!(btc.precision.price, 0.1);
    assert_eq!(btc.limits.amount.min, 0.0001);
    assert_eq!(btc.limits.amount.max, 1000.0);
    assert_eq!(btc.limits.leverage.max, 50.0);
}

#[test]
fn get_account_resolves_the_application_id_account() {
    let h = harness(ExchangeName::Lighter, options(), lighter);
    let account = h.drive(h.venue().get_account()).expect("getAccount");
    assert_eq!(account.user_id, "42");
    assert_eq!(account.affiliate_id, Some(Some("ref-7".to_owned())));
    let calls = h.http.calls(ACCOUNT);
    assert_eq!(h.http.count(), 1);
    assert_eq!(calls[0].params, json!({"by": "index", "value": "42"}));
}

#[test]
fn get_account_resolves_an_l1_address_key_by_l1_lookup() {
    let address = "0x00000000000000000000000000000000000000aB";
    let options: ExchangeOptions = serde_json::from_value(json!({
        "key": format!(" {address} "), "secret": TEST_SECRET, "apiKeyIndex": "3"
    }))
    .unwrap();
    let h = harness(ExchangeName::Lighter, options, lighter);
    let account = h.drive(h.venue().get_account()).expect("getAccount");
    assert_eq!(account.user_id, "42");
    let calls = h.http.calls(ACCOUNT);
    assert_eq!(
        calls[0].params,
        json!({"by": "l1_address", "value": address}),
        "the trimmed key is the L1 lookup candidate"
    );
}

#[test]
fn validate_account_looks_up_the_account_and_warms_the_signer() {
    let h = harness(ExchangeName::Lighter, options(), lighter);
    let result = h
        .drive(h.venue().validate_account())
        .expect("validateAccount");
    assert_eq!(serde_json::to_value(&result.error).unwrap(), json!(""));
    assert_eq!(h.http.urls(), vec![ACCOUNT]);
    assert_eq!(
        h.http.calls(ACCOUNT)[0].params,
        json!({"by": "index", "value": "42"})
    );
}

#[test]
fn validate_account_refuses_an_out_of_range_api_key_index_without_a_request() {
    let mut opts = serde_json::to_value(options()).unwrap();
    opts["apiKeyIndex"] = json!("256");
    let h = harness(
        ExchangeName::Lighter,
        serde_json::from_value(opts).unwrap(),
        lighter,
    );
    let result = h
        .drive(h.venue().validate_account())
        .expect("validateAccount");
    assert_eq!(
        serde_json::to_value(&result.error).unwrap(),
        json!("Invalid API key index. Expected an integer from 0 to 255.")
    );
    assert_eq!(h.http.count(), 0);
}

#[test]
fn fetch_positions_maps_the_account_snapshot_and_placeholders() {
    let h = harness(ExchangeName::Lighter, options(), lighter);
    loaded(&h);
    let before = h.http.count();
    let positions = h
        .drive(h.venue().fetch_positions(None))
        .expect("fetchPositions");
    let reads = &h.http.requests.borrow()[before..];
    assert!(reads.iter().all(|r| r.url == ACCOUNT), "{reads:?}");
    assert!(
        reads
            .iter()
            .all(|r| r.params == json!({"by": "index", "value": "42"}))
    );
    assert_eq!(
        positions.len(),
        1,
        "the real position wins its placeholder id"
    );
    let p = &positions[0];
    assert_eq!(p.symbol, "BTC");
    assert_eq!(p.side, PositionSide::Long);
    assert_eq!(p.contracts, 0.5);
    assert_eq!(p.entry_price, 50000.0);
    assert_eq!(p.notional, 25000.0);
    assert_eq!(p.unrealized_pnl, 10.0);
    assert_eq!(p.leverage, 10.0);
    assert_eq!(p.liquidation_price, 40000.0);
    assert_eq!(p.margin_type, Some(MarginType::Cross));
    assert_eq!(p.position_mode, Some(PositionMode::OneWay));
    assert!(
        h.store.positions().is_empty(),
        "fetchPositions is a pure read"
    );
}

#[test]
fn fetch_orders_reads_all_markets_with_a_signed_auth_token() {
    let h = harness(ExchangeName::Lighter, options(), lighter);
    loaded(&h);
    let orders = h.drive(h.venue().fetch_orders(None)).expect("fetchOrders");
    let calls = h.http.calls(ACTIVE_ORDERS);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].method, "GET");
    assert_eq!(calls[0].params["account_index"], json!("42"));
    assert_eq!(calls[0].params["market_id"].as_f64(), Some(255.0));
    let auth = calls[0].params["auth"].as_str().expect("auth token");
    assert!(auth.contains(&auth_token_prefix()), "{auth}");
    assert_eq!(orders.len(), 1);
    let o = &orders[0];
    assert_eq!(o.id, "777");
    assert_eq!(o.symbol, "BTC");
    assert_eq!(o.side, OrderSide::Buy);
    assert_eq!(o.r#type, OrderType::Limit);
    assert_eq!(o.price, 49000.0);
    assert_eq!(o.amount, 0.5);
    assert_eq!(o.status, OrderStatus::Open);
    assert!(h.store.orders().is_empty(), "fetchOrders is a pure read");
}

#[test]
fn set_leverage_signs_an_update_leverage_transaction() {
    let h = harness(ExchangeName::Lighter, options(), lighter);
    loaded(&h);
    let before = h.http.count();
    h.drive(h.venue().set_leverage("BTC".into(), 5.0, None))
        .expect("setLeverage");
    assert_nonce_read(&h);
    let txs = sent_txs(&h, before);
    assert_eq!(txs.len(), 1);
    let (tx_type, info) = &txs[0];
    assert_eq!(tx_type, "20");
    assert_signed_by_account(info);
    assert_eq!(info["MarketIndex"], json!(1));
    assert_eq!(info["InitialMarginFraction"], json!(2000), "10000 / 5x");
    assert_eq!(info["MarginMode"], json!(0), "market config: cross");
    assert_eq!(info["Nonce"], json!(NONCE));
    let last = h.http.requests.borrow().last().cloned().unwrap();
    assert_eq!(last.url, ACCOUNT, "leverage state is re-read after the tx");
}

#[test]
fn change_margin_type_signs_isolated_mode_for_the_symbol() {
    let h = harness(ExchangeName::Lighter, options(), lighter);
    loaded(&h);
    let before = h.http.count();
    h.drive(h.venue().change_margin_type(true, Some("BTC".into())))
        .expect("changeMarginType");
    assert_nonce_read(&h);
    let txs = sent_txs(&h, before);
    assert_eq!(txs.len(), 1);
    let (tx_type, info) = &txs[0];
    assert_eq!(tx_type, "20");
    assert_signed_by_account(info);
    assert_eq!(info["MarketIndex"], json!(1));
    assert_eq!(info["MarginMode"], json!(1));
    assert_eq!(info["InitialMarginFraction"], json!(200), "50x market max");
    assert_eq!(info["Nonce"], json!(NONCE));

    let before = h.http.count();
    let error = h
        .drive(h.venue().change_margin_type(true, None))
        .unwrap_err();
    assert_eq!(
        error.message,
        "Lighter margin mode update requires a symbol."
    );
    assert_eq!(h.http.count(), before);
}

#[test]
fn place_order_signs_an_attributed_create_order() {
    let h = harness(ExchangeName::Lighter, options(), lighter);
    loaded(&h);
    let before = h.http.count();
    let ids = h
        .drive(Box::pin(
            h.venue()
                .base()
                .place_order(h.venue(), limit_order(json!({}))),
        ))
        .expect("placeOrder");
    let limits = h.http.calls(LIMITS);
    assert_eq!(limits.len(), 1, "partner attribution checks the tier");
    assert_eq!(limits[0].params["account_index"], json!("42"));
    let authorization = limits[0].headers["authorization"].as_str().unwrap();
    assert!(authorization.contains(&auth_token_prefix()));
    assert_nonce_read(&h);
    let txs = sent_txs(&h, before);
    assert_eq!(txs.len(), 1);
    let (tx_type, info) = &txs[0];
    assert_eq!(tx_type, "14");
    assert_signed_by_account(info);
    assert_eq!(info["MarketIndex"], json!(1));
    assert_eq!(info["BaseAmount"], json!(5000), "0.5 at 4 size decimals");
    assert_eq!(info["Price"], json!(500_000), "50000 at 1 price decimal");
    assert_eq!(info["IsAsk"], json!(0));
    assert_eq!(info["Type"], json!(0), "limit");
    assert_eq!(info["ReduceOnly"], json!(0));
    assert_eq!(info["Nonce"], json!(NONCE));
    assert_eq!(
        info["L2TxAttributes"],
        json!({"1": INTEGRATOR, "2": 250, "3": 0})
    );
    assert_eq!(ids, vec![info["ClientOrderIndex"].to_string()]);
}

fn assert_rejected_placement(code: i64, message: &str, expected: &str) {
    let h = harness(ExchangeName::Lighter, options(), lighter);
    loaded(&h);
    h.http
        .routes
        .borrow_mut()
        .retain(|(method, path, _)| method != "POST" || path != SEND_TX);
    h.http
        .ok("POST", SEND_TX, json!({"code": code, "message": message}));
    let error = h
        .drive(Box::pin(
            h.venue()
                .base()
                .place_order(h.venue(), limit_order(json!({}))),
        ))
        .expect_err("venue rejected the real signed transaction");
    assert_eq!(
        h.http.calls(SEND_TX).len(),
        1,
        "one signed transaction reached the venue"
    );
    assert_eq!(
        error.message, expected,
        "production order rejection retains source guidance"
    );
    assert!(
        h.runtime
            .reports
            .borrow()
            .iter()
            .any(|report| report.contains(expected)),
        "normalized venue rejection reaches the real error reporter: {:?}",
        h.runtime.reports.borrow()
    );
}

#[test]
fn partner_tier_rejection_keeps_source_guidance() {
    assert_rejected_placement(
        21520,
        "venue tier refused",
        "Lighter requires Plus or Premium account tier for Tealstreet partner-attributed orders with non-zero integrator fees. Switch this account to Plus or Premium, then retry.",
    );
}

#[test]
fn partner_approval_rejection_keeps_source_guidance() {
    assert_rejected_placement(
        21149,
        "venue approval refused",
        "Tealstreet partner attribution is not yet approved for this Lighter account. Open Account Settings, edit this Lighter account, then click \"Approve Builder Fee\" and retry.",
    );
}

#[test]
fn ordinary_order_rejection_keeps_source_message() {
    assert_rejected_placement(
        29999,
        "venue rejected this order",
        "venue rejected this order",
    );
}

#[test]
fn place_order_refuses_a_caller_client_order_id_before_any_request() {
    let h = harness(ExchangeName::Lighter, options(), lighter);
    loaded(&h);
    let before = h.http.count();
    let error = h
        .drive(Box::pin(h.venue().base().place_order(
            h.venue(),
            limit_order(json!({"clientOrderId": "mine-1"})),
        )))
        .unwrap_err();
    assert_eq!(error.message, "lighter cannot honor a caller clientOrderId");
    assert_eq!(h.http.count(), before, "{:?}", h.http.urls());
}

#[test]
fn place_orders_places_a_single_order_batch() {
    let h = harness(ExchangeName::Lighter, options(), lighter);
    loaded(&h);
    let before = h.http.count();
    let ids = h
        .drive(Box::pin(
            h.venue()
                .base()
                .place_orders(h.venue(), vec![limit_order(json!({}))]),
        ))
        .expect("placeOrders");
    let txs = sent_txs(&h, before);
    assert_eq!(txs.len(), 1);
    assert_eq!(txs[0].0, "14");
    assert_eq!(ids, vec![txs[0].1["ClientOrderIndex"].to_string()]);
}

fn stored_orders(h: &Harness) -> Vec<Order> {
    let orders = h.drive(h.venue().fetch_orders(None)).expect("fetchOrders");
    Store::update(
        &*h.store,
        StoreUpdate {
            orders: Some(orders.clone()),
            ..Default::default()
        },
        false,
    )
    .expect("store orders");
    orders
}

#[test]
fn cancel_orders_signs_a_cancel_for_the_order_index() {
    let h = harness(ExchangeName::Lighter, options(), lighter);
    loaded(&h);
    let orders = stored_orders(&h);
    let before = h.http.count();
    h.drive(h.venue().cancel_orders(orders))
        .expect("cancelOrders");
    assert_nonce_read(&h);
    let txs = sent_txs(&h, before);
    assert_eq!(txs.len(), 1);
    let (tx_type, info) = &txs[0];
    assert_eq!(tx_type, "15");
    assert_signed_by_account(info);
    assert_eq!(info["MarketIndex"], json!(1));
    assert_eq!(info["Index"], json!(777));
    assert_eq!(info["Nonce"], json!(NONCE));
    assert!(
        h.store.orders().is_empty(),
        "cancelled orders leave the store"
    );
}

#[test]
fn update_order_cancels_then_replaces_at_the_new_price() {
    let h = harness(ExchangeName::Lighter, options(), lighter);
    loaded(&h);
    let orders = stored_orders(&h);
    let update: UpdateOrderOpts = serde_json::from_value(json!({
        "order": orders[0], "update": {"price": 48000}
    }))
    .expect("UpdateOrderOpts");
    let before = h.http.count();
    let ids = h
        .drive(h.venue().update_order(update))
        .expect("updateOrder");
    let txs = sent_txs(&h, before);
    let types: Vec<&str> = txs.iter().map(|(t, _)| t.as_str()).collect();
    assert_eq!(types, vec!["15", "14"], "cancel, then create");
    assert_eq!(txs[0].1["Index"], json!(777));
    assert_eq!(txs[0].1["Nonce"], json!(NONCE));
    let create = &txs[1].1;
    assert_signed_by_account(create);
    assert_eq!(create["Price"], json!(480_000));
    assert_eq!(create["BaseAmount"], json!(5000));
    assert_eq!(create["IsAsk"], json!(0));
    assert_eq!(
        create["Nonce"],
        json!(NONCE + 1),
        "the cached nonce advances"
    );
    assert_eq!(ids, vec![create["ClientOrderIndex"].to_string()]);
}

#[test]
fn start_dials_both_sockets_and_subscribes() {
    let h = harness(ExchangeName::Lighter, options(), lighter);
    h.drive(h.venue().start()).expect("start");
    h.runtime.advance(50.0);
    {
        let loaded = h.store.loaded();
        assert!(loaded.markets && loaded.tickers);
        assert!(loaded.positions && loaded.orders && loaded.balance);
    }
    assert_eq!(
        h.http.urls(),
        vec![ACCOUNT, DETAILS, EXCHANGE_STATS, ACCOUNT],
        "signer account lookup, markets, the public ticker snapshot, the private snapshot"
    );
    for read in h.http.calls(ACCOUNT) {
        assert_eq!(read.params, json!({"by": "index", "value": "42"}));
    }
    assert!(
        h.runtime.reports.borrow().is_empty(),
        "{:?}",
        h.runtime.reports
    );
    let sockets = h.runtime.sockets.borrow();
    let urls: Vec<&str> = sockets.iter().map(|(u, _)| u.as_str()).collect();
    assert_eq!(urls.len(), 2, "{urls:?}");
    assert!(
        urls[0].starts_with("wss://mainnet.zklighter.elliot.ai/stream?requestId="),
        "{urls:?}"
    );
    assert_eq!(urls[1], "wss://mainnet.zklighter.elliot.ai/stream");
    let public = sockets[0].1.subscriptions();
    assert!(
        public.contains(&json!({"type": "subscribe", "channel": "market_stats/all"})),
        "{public:?}"
    );
    let private = sockets[1].1.subscriptions();
    assert_eq!(
        private[0],
        json!({"type": "subscribe", "channel": "account_all/42"})
    );
    assert_eq!(private[1]["channel"], json!("account_all_orders/42"));
    let auth = private[1]["auth"].as_str().expect("orders auth token");
    assert!(auth.contains(&auth_token_prefix()), "{auth}");
    drop(sockets);
    h.venue().dispose().expect("dispose");
}

#[test]
fn inherited_base_methods_reject_without_a_request() {
    let h = harness(ExchangeName::Lighter, options(), lighter);
    loaded(&h);
    let before = h.http.count();
    for (method, result) in [
        (
            "fetchTickers",
            h.drive(h.venue().fetch_tickers()).map(|_| ()),
        ),
        (
            "fetchSymbolMaxLeverage",
            h.drive(h.venue().fetch_symbol_max_leverage("BTC".into()))
                .map(|_| ()),
        ),
        (
            "changePositionMode",
            h.drive(h.venue().change_position_mode(true, None)),
        ),
        ("scheduleCancel", h.drive(h.venue().schedule_cancel(None))),
        (
            "fetchPublicTrades",
            h.drive(h.venue().fetch_public_trades("BTC".into(), None))
                .map(|_| ()),
        ),
    ] {
        let error = result.expect_err(method);
        assert_eq!(error.message, "Not implemented", "{method}");
    }
    assert_eq!(h.http.count(), before);
}

#[test]
fn injected_entropy_repeats_the_signed_wire_payload() {
    fn wire() -> Vec<(String, Value)> {
        let h = harness(ExchangeName::Lighter, options(), lighter);
        loaded(&h);
        let before = h.http.count();
        h.drive(h.venue().set_leverage("BTC".into(), 5.0, None))
            .unwrap();
        sent_txs(&h, before)
    }
    let first = wire();
    let second = wire();
    assert_eq!(first.len(), 1);
    assert_eq!(
        first, second,
        "fixed clock, nonce and entropy reproduce the wire signature"
    );
}

fn trait_candles(http: &Http) {
    lighter(http);
    http.ok(
        "GET",
        "/api/v1/candles",
        json!({"code":200,"c":[
            {"t":1700000060000.0,"o":"103","h":"107","l":"102","c":"106","v":"11"},
            {"t":1700000000000.0,"o":"101","h":"105","l":"100","c":"104","v":"7"}
        ]}),
    );
    http.ok(
        "GET",
        "/api/v1/orderBookOrders",
        json!({"code":200,
        "bids":[{"price":"100","remaining_base_amount":"2","size":"2"}],
        "asks":[{"price":"101","remaining_base_amount":"3","size":"3"}]}),
    );
}

#[test]
fn real_trait_candles_reach_rest_and_preserve_sorted_fields() {
    let h = harness(ExchangeName::Lighter, options(), trait_candles);
    loaded(&h);
    let rows = h
        .drive(
            h.venue().fetch_ohlcv(
                serde_json::from_value(json!({
                    "symbol":"BTC","interval":"1m","from":1699999900000.0,"to":1700000100000.0
                }))
                .unwrap(),
            ),
        )
        .unwrap();
    assert_eq!(rows.len(), 2, "actual REST candles");
    assert_eq!(rows[0].timestamp, 1700000000.0);
    assert_eq!(
        (
            rows[0].open,
            rows[0].high,
            rows[0].low,
            rows[0].close,
            rows[0].volume
        ),
        (101.0, 105.0, 100.0, 104.0, 7.0)
    );
    assert_eq!((rows[1].timestamp, rows[1].close), (1700000060.0, 106.0));
    let calls = h.http.calls("/api/v1/candles");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].params["market_id"].as_f64(), Some(1.0));
    assert_eq!(calls[0].params["resolution"], json!("1m"));
    assert_eq!(
        calls[0].params["start_timestamp"].as_f64(),
        Some(1699999900000.0)
    );
}

#[test]
fn real_trait_candles_refuse_unknown_market() {
    let h = harness(ExchangeName::Lighter, options(), lighter);
    loaded(&h);
    let error = h
        .drive(h.venue().fetch_ohlcv(
            serde_json::from_value(json!({"symbol":"UNKNOWN","interval":"1m"})).unwrap(),
        ))
        .expect_err("source rejects missing market");
    assert!(
        error
            .message
            .contains("Market not found for symbol: UNKNOWN"),
        "{}",
        error.message
    );
    assert!(h.http.calls("/api/v1/candles").is_empty());
}

#[test]
fn real_trait_fills_reach_signed_history_and_keep_both_rows() {
    let mut opts = options();
    opts.key = "0x1111111111111111111111111111111111111111".into();
    let h = harness(ExchangeName::Lighter, opts, |http| {
        lighter(http);
        http.ok("GET","/api/v1/accountInactiveOrders",json!({"code":200,"fills":[
            {"market_id":1,"order_id":"open","side":"BUY","price":"100","size":"2","timestamp":1700000000,"fee":"0.1","feeAsset":"USDC"},
            {"market_id":1,"order_id":"close","side":"SELL","price":"110","size":"3","timestamp":1700000060,"fee":"0.2","feeAsset":"USDC"}
        ]}));
    });
    loaded(&h);
    let rows = h
        .drive(
            h.venue().fetch_historical_fills(
                serde_json::from_value(
                    json!({"symbol":"BTC","from":1699999999.0,"to":1700000100.0,"limit":10}),
                )
                .unwrap(),
            ),
        )
        .unwrap();
    assert_eq!(
        rows.len(),
        2,
        "actual authenticated fill collection; urls={:?}, logs={:?}, reports={:?}",
        h.http.urls(),
        h.runtime.logs.borrow(),
        h.runtime.reports.borrow()
    );
    assert_eq!(rows[0].order_id.as_deref(), Some("close"));
    assert_eq!(
        (rows[0].timestamp, rows[0].price, rows[0].qty),
        (1700000060.0, 110.0, 3.0)
    );
    assert_eq!(rows[0].side, OrderSide::Sell);
    assert_eq!(rows[0].commission, Some(0.2));
    assert_eq!(rows[1].order_id.as_deref(), Some("open"));
    assert_eq!(rows[1].side, OrderSide::Buy);
    let calls = h.http.calls("/api/v1/accountInactiveOrders");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].params["account_index"], json!("42"));
    assert_eq!(calls[0].params["market_id"].as_f64(), Some(1.0));
    assert!(
        calls[0].params["auth"]
            .as_str()
            .unwrap()
            .contains(&auth_token_prefix())
    );
}

#[test]
fn real_trait_private_snapshot_updates_positions_orders_and_balance() {
    let h = harness(ExchangeName::Lighter, options(), lighter);
    loaded(&h);
    h.drive(h.venue().fetch_and_set_private_snapshot(1))
        .unwrap();
    assert_eq!(
        h.store.positions().len(),
        1,
        "venue snapshot loads positions"
    );
    assert_eq!(h.store.positions()[0].contracts, 0.5);
    assert_eq!(h.store.orders().len(), 1);
    assert_eq!(h.store.orders()[0].id, "777");
    let state = h.store.loaded();
    assert!(
        state.positions && state.orders && state.balance,
        "full venue snapshot flags"
    );
    assert_eq!(h.store.balance().free, 800.0);
}

#[test]
fn real_trait_book_delivers_socket_changes_and_cleans_up() {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_transport::socket::SocketFrame;
    use std::{cell::RefCell, rc::Rc};
    let h = harness(ExchangeName::Lighter, options(), trait_candles);
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    let seen = Rc::new(RefCell::new(Vec::<OrderBook>::new()));
    let sink = seen.clone();
    let cleanup = h
        .venue()
        .listen_order_book(
            "BTC".into(),
            Rc::new(move |book| sink.borrow_mut().push(book)),
        )
        .unwrap();
    h.runtime.pump();
    let socket = h.runtime.sockets.borrow()[0].1.clone();
    assert!(
        socket
            .subscriptions()
            .contains(&json!({"type":"subscribe","channel":"order_book/1"})),
        "actual subscription"
    );
    let frame = json!({"channel":"order_book:1","type":"update/order_book","order_book":{"bids":[{"price":"102","size":"4"}],"asks":[{"price":"103","size":"5"}]}});
    socket
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text(frame.to_string())));
    h.runtime.pump();
    assert!(
        seen.borrow().iter().any(|book| book
            .bids
            .iter()
            .any(|level| level.price == 102.0 && level.amount == 4.0)),
        "actual websocket book callback"
    );
    cleanup().unwrap();
    let count = seen.borrow().len();
    let mut after_cleanup = frame.clone();
    after_cleanup["order_book"]["bids"][0]["price"] = json!("108");
    socket
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text(
            after_cleanup.to_string(),
        )));
    h.runtime.advance(250.0);
    assert_eq!(seen.borrow().len(), count, "cleanup removes actual parser");
    assert!(
        socket
            .subscriptions()
            .contains(&json!({"type":"unsubscribe","channel":"order_book/1"}))
    );
    h.venue().dispose().unwrap();
}

#[test]
fn real_trait_candle_listener_polls_deduplicates_and_cleans_up() {
    use std::{cell::RefCell, rc::Rc};
    let h = harness(ExchangeName::Lighter, options(), trait_candles);
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    let seen = Rc::new(RefCell::new(Vec::<Candle>::new()));
    let sink = seen.clone();
    let cleanup = h
        .venue()
        .listen_ohlcv(
            serde_json::from_value(json!({"symbol":"BTC","interval":"1m"})).unwrap(),
            Rc::new(move |row| sink.borrow_mut().push(row)),
        )
        .unwrap();
    h.runtime.pump();
    assert_eq!(seen.borrow().len(), 1, "initial real fallback candle");
    assert_eq!(seen.borrow()[0].close, 106.0);
    let calls = h.http.calls("/api/v1/candles").len();
    h.runtime.advance(5000.0);
    assert!(
        h.http.calls("/api/v1/candles").len() > calls,
        "actual repeated polling"
    );
    assert_eq!(
        seen.borrow().len(),
        1,
        "unchanged fallback candle is deduplicated"
    );
    cleanup().unwrap();
    let calls = h.http.calls("/api/v1/candles").len();
    h.runtime.advance(5000.0);
    assert_eq!(
        h.http.calls("/api/v1/candles").len(),
        calls,
        "cleanup stops actual polling"
    );
    assert_eq!(seen.borrow().len(), 1);
    h.venue().dispose().unwrap();
}

#[test]
fn offline_recovery_requests_account_reload_for_source_owned_sockets() {
    let h = harness(ExchangeName::Lighter, options(), |_| {});
    let sockets = h.venue().base().get_all_sockets();
    assert_eq!(sockets.len(), 2, "both real source sockets are registered");
    for socket in &sockets {
        assert!(
            !socket.recover_after_offline(true).unwrap(),
            "the source socket requests an account reload"
        );
    }
    assert!(
        !h.venue().recover_after_offline().unwrap(),
        "the production Exchange must expose the reload policy"
    );
    assert!(
        h.runtime.sockets.borrow().is_empty(),
        "recovery must not dial a second retry chain"
    );
    h.venue().dispose().unwrap();
    assert!(
        h.venue().recover_after_offline().unwrap(),
        "disposed accounts do not request another reload"
    );
}

#[test]
fn private_close_cancels_native_heartbeat_and_receive_deadlines() {
    use safe_cex_exchange::{socket_lifecycle::SocketSlot, websocket::WsEvent};
    use safe_cex_transport::socket::SocketFrame;
    let h = harness(ExchangeName::Lighter, options(), lighter);
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    let private = h.runtime.sockets.borrow()[1].1.clone();
    private
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text(
            json!({"type":"pong"}).to_string(),
        )));
    h.runtime.pump();
    let core = h.venue().base().get_socket(SocketSlot::Private).unwrap();
    assert_eq!(
        core.last_message_timestamp(),
        Some(h.runtime.now.get()),
        "real private pong starts the native watchdog"
    );
    core.handle_pong_event(3217).unwrap();
    h.runtime.pump();
    private.closed.set(true);
    private.events.borrow_mut().push_back(WsEvent::Close {
        code: Some(1008),
        reason: "rate limit exceeded".into(),
    });
    h.runtime.pump();
    assert!(
        h.runtime
            .logs
            .borrow()
            .iter()
            .any(|line| line.contains("closed (code 1008")),
        "real source close handler ran"
    );
    h.runtime.completed_sleeps.borrow_mut().clear();
    h.runtime.advance(5000.0);
    let completed = h.runtime.completed_sleeps.borrow();
    assert!(
        !completed.contains(&3217),
        "source close must cancel the actual native ping deadline: {completed:?}"
    );
    assert!(
        !completed.contains(&1000),
        "source close must cancel the actual native receive deadline: {completed:?}"
    );
    assert_eq!(
        h.runtime.sockets.borrow().len(),
        2,
        "source rate-limit backoff owns the next dial"
    );
    assert!(
        h.runtime.reports.borrow().is_empty(),
        "{:?}",
        h.runtime.reports.borrow()
    );
    drop(completed);
    h.venue().dispose().unwrap();
}

fn private_account_frame_for_deployment(name: ExchangeName) {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_transport::socket::SocketFrame;
    use std::{cell::RefCell, rc::Rc};
    let h = harness(name.clone(), options(), lighter);
    assert_eq!(h.venue().base().name, name);
    h.drive(h.venue().start()).unwrap();
    // The source suppresses fills for five seconds after its real VirtualClock connect.
    std::thread::sleep(std::time::Duration::from_millis(5100));
    let fills = Rc::new(RefCell::new(vec![]));
    let observed = fills.clone();
    h.venue().base().on(
        "fill",
        Rc::new(move |_, fill| {
            observed.borrow_mut().push(fill.clone());
            Ok(())
        }),
    );
    let socket = h.runtime.sockets.borrow()[1].1.clone();
    let mut order = active_order();
    order["order_index"] = json!(888);
    order["client_order_index"] = json!(888);
    let mut position = account()["accounts"][0]["positions"][0].clone();
    position["position"] = json!("0.75");
    let mut refreshed = account();
    refreshed["accounts"][0]["available_balance"] = json!("645");
    h.http
        .routes
        .borrow_mut()
        .retain(|(_, path, _)| path != ACCOUNT);
    h.http.ok("GET", ACCOUNT, refreshed);
    socket.events.borrow_mut().push_back(WsEvent::Message(SocketFrame::Text(json!({
        "channel":"account_all:42","type":"update/account_all",
        "orders":{"1":[order]},"positions":{"1":[position]},
        "trades":{"1":[{"trade_id":71,"order_id":888,"symbol":"BTC","side":"sell","price":"49000","size":"0.1","timestamp":h.runtime.now.get(),"ignored":"SURROGATE_FIELD"}]},
        "account":{"collateral":"1000"}
    }).to_string().replace("\"SURROGATE_FIELD\"", "\"\\ud800\""))));
    h.runtime.pump();
    assert!(
        h.store.orders().iter().any(|order| order.id == "888"),
        "private order goes through real venue mapper"
    );
    assert_eq!(
        h.store.positions()[0].contracts,
        0.75,
        "private position uses real venue mapper and Store"
    );
    assert_eq!(
        *fills.borrow(),
        vec![json!({"id":"888","side":"buy","symbol":"BTC","price":49000.0,"amount":0.1})],
        "private trade produces source fill payload"
    );
    assert_eq!(
        h.store.balance().free,
        800.0,
        "balance refresh waits for the source throttle"
    );
    h.runtime.advance(500.0);
    assert_eq!(
        h.store.balance().free,
        645.0,
        "private account frame triggers real balance refresh"
    );
    assert!(
        h.runtime.reports.borrow().is_empty(),
        "{:?}",
        h.runtime.reports.borrow()
    );
    h.venue().dispose().unwrap();
}

#[test]
fn private_fill_timestamp_types_match_the_maintained_handler() {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_transport::socket::SocketFrame;
    use std::{cell::RefCell, process::Command, rc::Rc};
    let h = harness(ExchangeName::Lighter, options(), lighter);
    h.drive(h.venue().start()).unwrap();
    let source = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/fill-timestamps-oracle.cjs"
        ))
        .arg(h.runtime.now.get().to_string())
        .output()
        .expect("maintained Lighter handler oracle");
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(cases.len(), 15);
    // The production socket uses real VirtualClock time for its connect grace period.
    std::thread::sleep(std::time::Duration::from_millis(5100));
    let fills = Rc::new(RefCell::new(vec![]));
    let observed = fills.clone();
    h.venue().base().on(
        "fill",
        Rc::new(move |_, fill| {
            observed.borrow_mut().push(fill.clone());
            Ok(())
        }),
    );
    let socket = h.runtime.sockets.borrow()[1].1.clone();
    for case in cases {
        fills.borrow_mut().clear();
        socket
            .events
            .borrow_mut()
            .push_back(WsEvent::Message(SocketFrame::Text(
                json!({
                    "channel":"account_all:42", "type":"update/account_all",
                    "data":{"fills":[{"orderId":case["label"], "side":"BUY", "symbol":"BTC",
                        "price":"49000", "size":"0.1", "timestamp":case["timestamp"]}]}
                })
                .to_string(),
            )));
        h.runtime.pump();
        let mut expected = case["expected"].as_array().unwrap().clone();
        for fill in &mut expected {
            for field in ["price", "amount"] {
                fill[field] = json!(fill[field].as_f64().unwrap());
            }
        }
        assert_eq!(*fills.borrow(), expected, "real private frame: {case}");
    }
    assert!(
        h.runtime.reports.borrow().is_empty(),
        "{:?}",
        h.runtime.reports.borrow()
    );
    h.venue().dispose().unwrap();
}

#[test]
fn private_account_frame_reaches_real_order_position_fill_and_balance_handlers() {
    for name in [ExchangeName::Lighter, ExchangeName::LighterRobinhood] {
        private_account_frame_for_deployment(name);
    }
}
