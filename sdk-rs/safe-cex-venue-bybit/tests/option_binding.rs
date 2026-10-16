//! Option metadata across the real Bybit receiver and typed Exchange boundary.
#[path = "support/binding_harness.rs"]
mod support;
use safe_cex_types::{OptionRight, OptionTermsSettlement, OptionTermsStyle};
use serde_json::json;

#[test]
fn bound_market_terms_and_position_greeks_survive_typed_results() {
    let symbol = "SOL-6NOV26-112d5-P";
    let mut opts = support::options();
    opts.enable_option_markets = Some(true);
    opts.public_data_only = Some(true);
    let h = support::harness(opts, |wire| {
        wire.ok(
            "GET",
            "/v5/market/instruments-info",
            json!({"list":[],"nextPageCursor":""}),
        );
        wire.ok(
            "GET",
            "/v5/market/option-base-coins",
            json!({"list":[{"baseCoin":"SOL"}]}),
        );
        wire.route_when(
            "GET",
            "/v5/market/instruments-info",
            "category",
            "option",
            json!({
                "retCode":0,"retMsg":"OK","result":{"list":[{
                    "symbol":symbol,"status":"Trading","optionsType":"Put","baseCoin":"SOL",
                    "quoteCoin":"USDC","settleCoin":"USDC","deliveryTime":"1793952000000",
                    "lotSizeFilter":{"qtyStep":"0.1","minOrderQty":"0.1","maxOrderQty":"500"},
                    "priceFilter":{"tickSize":"0.01","minPrice":"0.01"}
                }],"nextPageCursor":""}
            }),
        );
        wire.ok(
            "GET",
            "/v5/position/list",
            json!({"list":[],"nextPageCursor":""}),
        );
        wire.route_when(
            "GET",
            "/v5/position/list",
            "category",
            "option",
            json!({
                "retCode":0,"retMsg":"OK","result":{"list":[{
                    "symbol":symbol,"side":"Buy","size":"2","avgPrice":"3","positionValue":"6",
                    "leverage":"10","unrealisedPnl":"1","curRealisedPnl":"0","liqPrice":"100",
                    "positionIdx":0,"positionIM":"6","delta":"-0.4","gamma":"0.03",
                    "vega":"0.12","theta":"-0.08"
                }],"nextPageCursor":""}
            }),
        );
    });
    let markets = h.drive(h.venue().fetch_markets()).unwrap();
    assert_eq!(markets.len(), 1);
    let market = &markets[0];
    assert_eq!(market.symbol, symbol);
    let terms = market.option.as_ref().expect("typed option terms");
    assert_eq!(terms.underlying, "SOL");
    assert_eq!(terms.expiry, 1793952000000.);
    assert_eq!(terms.strike, 112.5);
    assert_eq!(terms.right, OptionRight::Put);
    assert_eq!(terms.style, OptionTermsStyle::European);
    assert_eq!(terms.settlement, OptionTermsSettlement::Cash);
    assert_eq!(terms.premium_asset, "USDC");
    assert_eq!(terms.settle_asset, "USDC");
    assert_eq!(market.contract_size, Some(1.));
    assert_eq!(market.is_expiring, Some(true));
    safe_cex_store::Store::update(
        h.store.as_ref(),
        safe_cex_store::StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let positions = h.drive(h.venue().fetch_positions(None)).unwrap();
    assert_eq!(positions.len(), 1);
    let position = &positions[0];
    assert_eq!(position.symbol, symbol);
    assert_eq!(position.leverage, 1.);
    assert_eq!(position.liquidation_price, 0.);
    let greeks = position
        .option
        .as_ref()
        .expect("typed position option")
        .greeks
        .as_ref()
        .expect("typed Greeks");
    assert_eq!(
        (greeks.delta, greeks.gamma, greeks.vega, greeks.theta),
        (-0.4, 0.03, 0.12, -0.08)
    );
    assert_eq!(
        greeks.units,
        safe_cex_venue_bybit::options::BYBIT_GREEK_UNITS
    );
    assert!(
        h.wire
            .calls("/v5/position/list")
            .iter()
            .any(|request| request.params["category"] == "option")
    );
}
