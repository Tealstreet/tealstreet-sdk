#[path = "support/exchange_runtime.rs"]
#[allow(dead_code)]
mod harness;
mod proof {
    use super::harness as io;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../safe-cex-parity/accepted-set/kucoin-constant-fixture.rs"
    ));
}
use safe_cex_exchange::exchange::Exchange;
use safe_cex_venue_kucoin::kucoin_exchange::KucoinSpotExchange;
use serde_json::{Value, json};

fn expected(native: Value) -> Value {
    json!({"native":native,"http":0,"state_unchanged":true,"options_unchanged":true,
        "notifications":0,"events":0,"reports_unchanged":true,"sockets_unchanged":true,
        "now_unchanged":true})
}

#[test]
fn spot_max_leverage_returns_one_without_effects() {
    for symbol in ["BTC-USDT", ""] {
        let (venue, fixture) = harness::spot();
        let actual = proof::observe(
            venue.as_ref(),
            &fixture,
            |venue| {
                <KucoinSpotExchange as Exchange>::fetch_symbol_max_leverage(venue, symbol.into())
            },
            |native| json!({"kind":"number","bits":format!("{:016x}",native.to_bits())}),
        );
        assert_eq!(
            actual,
            expected(json!({"kind":"number","bits":"3ff0000000000000"}))
        );
    }
}

#[test]
fn spot_change_margin_returns_unit_without_effects() {
    for (isolated, symbol) in [(true, Some("BTC-USDT")), (false, None)] {
        let (venue, fixture) = harness::spot();
        let actual = proof::observe(
            venue.as_ref(),
            &fixture,
            |venue| {
                <KucoinSpotExchange as Exchange>::change_margin_type(
                    venue,
                    isolated,
                    symbol.map(str::to_owned),
                )
            },
            |()| json!({"kind":"unit"}),
        );
        assert_eq!(actual, expected(json!({"kind":"unit"})));
    }
}

#[test]
fn spot_change_position_returns_unit_without_effects() {
    for (hedged, symbol) in [(true, Some("ETH-USDT")), (false, None)] {
        let (venue, fixture) = harness::spot();
        let actual = proof::observe(
            venue.as_ref(),
            &fixture,
            |venue| {
                <KucoinSpotExchange as Exchange>::change_position_mode(
                    venue,
                    hedged,
                    symbol.map(str::to_owned),
                )
            },
            |()| json!({"kind":"unit"}),
        );
        assert_eq!(actual, expected(json!({"kind":"unit"})));
    }
}
