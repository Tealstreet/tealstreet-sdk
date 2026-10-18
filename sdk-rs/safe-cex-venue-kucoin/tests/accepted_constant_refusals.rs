#[path = "support/exchange_runtime.rs"]
#[allow(dead_code)]
mod harness;
mod proof {
    use super::harness as io;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../safe-cex-parity/accepted-set/kucoin-refusal-fixture.rs"
    ));
}
use safe_cex_exchange::exchange::Exchange;
use safe_cex_venue_kucoin::kucoin_exchange::{KucoinExchange, KucoinSpotExchange};
use serde_json::json;

fn expected() -> serde_json::Value {
    json!({"error":"Not implemented","error_name":"Error","http":0,
        "state_unchanged":true,"options_unchanged":true,"notifications":0,"events":0,
        "reports_unchanged":true,"sockets_unchanged":true,"now_unchanged":true})
}

#[test]
fn futures_inherited_max_leverage_rejects_without_effects() {
    for symbol in ["BTCUSDT", ""] {
        let (venue, fixture) = harness::futures();
        let actual = proof::observe(venue.as_ref(), &fixture, |venue| {
            Box::pin(async move {
                <KucoinExchange as Exchange>::fetch_symbol_max_leverage(venue, symbol.into())
                    .await
                    .map(|_| ())
            })
        });
        assert_eq!(actual, expected(), "actual native refusal for {symbol:?}");
    }
}

#[test]
fn futures_inherited_public_trades_rejects_without_effects() {
    for (symbol, limit) in [("BTCUSDT", None), ("", Some(3.0))] {
        let (venue, fixture) = harness::futures();
        let actual = proof::observe(venue.as_ref(), &fixture, |venue| {
            Box::pin(async move {
                <KucoinExchange as Exchange>::fetch_public_trades(venue, symbol.into(), limit)
                    .await
                    .map(|_| ())
            })
        });
        assert_eq!(actual, expected(), "actual native refusal for {symbol:?}");
    }
}

#[test]
fn spot_inherited_public_trades_rejects_without_effects() {
    for (symbol, limit) in [("BTC-USDT", None), ("", Some(7.0))] {
        let (venue, fixture) = harness::spot();
        let actual = proof::observe(venue.as_ref(), &fixture, |venue| {
            Box::pin(async move {
                <KucoinSpotExchange as Exchange>::fetch_public_trades(venue, symbol.into(), limit)
                    .await
                    .map(|_| ())
            })
        });
        assert_eq!(actual, expected(), "actual native refusal for {symbol:?}");
    }
}
