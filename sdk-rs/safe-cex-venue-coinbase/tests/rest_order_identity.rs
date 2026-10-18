//! Production fetchOrders keeps REST open orders whose client_order_id is null or absent.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod binding_harness;
use binding_harness::{Http, harness, options};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::*;
use serde_json::{Value, json};

const LIST_ORDERS: &str = "/api/v3/brokerage/orders/historical/batch";

fn product() -> Value {
    json!({"product_id":"BTC-USDC","base_currency_id":"BTC","quote_currency_id":"USDC","price_increment":"0.01","base_increment":"0.001","base_min_size":"0.001","base_max_size":"100","trading_disabled":false,"view_only":false,"price":"123.5","price_percentage_change_24h":"2.5","volume_24h":"1000"})
}
fn rest_order(id: &str) -> Value {
    json!({"order_id":id,"product_id":"BTC-USDC","side":"BUY","status":"OPEN","filled_size":"0","order_configuration":{"limit_limit_gtc":{"base_size":"2.5","limit_price":"123.45","post_only":false}}})
}
fn script(http: &Http) {
    http.ok("GET", "/api/v3/brokerage/portfolios", json!({"portfolios":[{"name":"Synthetic Spot","uuid":"portfolio-synthetic","type":"DEFAULT","deleted":false}]}));
    http.ok(
        "GET",
        "/api/v3/brokerage/portfolios/portfolio-synthetic",
        json!({"breakdown":{"spot_positions":[]}}),
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
    let mut absent = rest_order("rest-absent-client");
    absent.as_object_mut().unwrap().remove("client_order_id");
    let mut nulled = rest_order("rest-null-client");
    nulled["client_order_id"] = Value::Null;
    let mut named = rest_order("rest-named-client");
    named["client_order_id"] = json!("caller-id");
    http.ok(
        "GET",
        LIST_ORDERS,
        json!({"orders":[nulled, absent, named],"has_next":false}),
    );
    http.ok("GET", LIST_ORDERS, json!({"orders":[],"has_next":false}));
}

#[test]
fn rest_snapshot_keeps_orders_without_client_order_id() {
    let h = harness(ExchangeName::Coinbase, options(), script);
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
    let orders = h.drive(h.venue().fetch_orders(None)).unwrap();
    assert!(
        !h.http.calls(LIST_ORDERS).is_empty(),
        "fetchOrders must read the real REST list"
    );
    let identity: Vec<(&str, Option<&str>)> = orders
        .iter()
        .map(|o| (o.id.as_str(), o.parent_id.as_deref()))
        .collect();
    assert_eq!(
        identity,
        [
            ("rest-null-client", None),
            ("rest-absent-client", None),
            ("rest-named-client", Some("caller-id")),
        ],
        "REST open orders without client_order_id must map with an undefined parentId"
    );
    assert!(
        orders
            .iter()
            .all(|o| o.status == OrderStatus::Open && o.amount == 2.5)
    );
}
