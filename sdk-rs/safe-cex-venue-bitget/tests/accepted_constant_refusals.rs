#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod harness;
mod proof {
    use super::harness as io;
    use safe_cex_venue_bitget as venue_crate;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../safe-cex-parity/accepted-set/bitget-refusal-fixture.rs"
    ));
}
use safe_cex_exchange::exchange::Exchange;
use safe_cex_types::ExchangeName;
use safe_cex_venue_bitget::BitgetExchange;
use serde_json::json;

fn expected() -> serde_json::Value {
    json!({"error":"Not implemented","error_name":"Error","http":0,
        "state_unchanged":true,"options_unchanged":true,"notifications":0,"events":0,
        "reports_unchanged":true,"sockets_unchanged":true,"now_unchanged":true})
}

#[test]
fn inherited_max_leverage_rejects_without_effects() {
    for symbol in ["BTCUSDT", ""] {
        let h = harness::harness(ExchangeName::Bitget, harness::options(), |_| {});
        let actual = proof::observe(&h, |venue| {
            Box::pin(async move {
                <BitgetExchange as Exchange>::fetch_symbol_max_leverage(venue, symbol.into())
                    .await
                    .map(|_| ())
            })
        });
        assert_eq!(actual, expected(), "actual native refusal for {symbol:?}");
    }
}

#[test]
fn inherited_public_trades_rejects_without_effects() {
    for (symbol, limit) in [("BTCUSDT", None), ("", Some(3.0))] {
        let h = harness::harness(ExchangeName::Bitget, harness::options(), |_| {});
        let actual = proof::observe(&h, |venue| {
            Box::pin(async move {
                <BitgetExchange as Exchange>::fetch_public_trades(venue, symbol.into(), limit)
                    .await
                    .map(|_| ())
            })
        });
        assert_eq!(actual, expected(), "actual native refusal for {symbol:?}");
    }
}
