mod support;
use safe_cex_engine::market_facts::NotionalBasis;
use safe_cex_venue_binance::Variant;
use support::{Harness, Options};

#[test]
fn native_minimum_units_and_market_exemption_follow_discovery() {
    for variant in [
        Variant::UsdM,
        Variant::PortfolioMargin,
        Variant::CoinM,
        Variant::Spot,
    ] {
        let mut h = Harness::started(Options {
            variant,
            public_data_only: true,
            ..Default::default()
        });
        let mut catalogue = support::EXCHANGE_INFO.replace(
            "\"contractType\":\"PERPETUAL\"",
            "\"contractType\":\"PERPETUAL\",\"contractSize\":100",
        );
        if variant == Variant::Spot {
            catalogue = catalogue.replace(
                "\"filterType\":\"MIN_NOTIONAL\",\"notional\":\"5\"",
                "\"filterType\":\"NOTIONAL\",\"minNotional\":\"5\",\"applyMinToMarket\":false",
            );
        }
        h.respond("/exchangeInfo", 200, &catalogue);
        let facts = h.engine.store().market_order_facts()["BTCUSDT"];
        assert_eq!(facts.min_notional, Some(5.0));
        let market_amount = facts.market_amount.expect("native market-lot facts");
        assert_eq!(market_amount.step, Some(0.001));
        assert_eq!(market_amount.maximum, Some(120.0));
        assert_eq!(facts.reduce_only_exempt, variant != Variant::Spot);
        assert_eq!(facts.market_notional_exempt, variant == Variant::Spot);
        assert_eq!(
            facts.notional_basis,
            if variant == Variant::CoinM {
                NotionalBasis::Quote { per_amount: 100.0 }
            } else {
                NotionalBasis::Base
            }
        );
        let market_json = serde_json::to_value(&h.engine.store().markets()[0]).unwrap();
        assert!(market_json.get("minNotional").is_none());
        assert!(market_json.get("marketOrderFacts").is_none());
    }
}
