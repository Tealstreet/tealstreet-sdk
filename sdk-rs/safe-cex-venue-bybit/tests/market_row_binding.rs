#[path = "support/binding_harness.rs"]
mod support;

use safe_cex_types::ExchangeName;
use serde_json::{Value, json};

fn instrument(symbol: &str, base: &str) -> Value {
    json!({"symbol":symbol,"baseCoin":base,"quoteCoin":if symbol.ends_with("USDT") {"USDT"} else {"USD"},"status":"Trading",
        "contractType":if symbol.ends_with("USDT") {"LinearPerpetual"} else {"InversePerpetual"},"settleCoin":if symbol.ends_with("USDT") {"USDT"} else {base},"deliveryTime":"0",
        "priceFilter":{"tickSize":"0.1"},
        "lotSizeFilter":{"qtyStep":"0.001","minOrderQty":"0.001","maxOrderQty":"100"},
        "leverageFilter":{"minLeverage":"1","maxLeverage":"100"}})
}

fn mixed_rows(name: ExchangeName) {
    let inverse = name == ExchangeName::Bybitinverse;
    let (first, bad_symbol, last) = if inverse {
        ("BTCUSD", "BADUSD", "ETHUSD")
    } else {
        ("BTCUSDT", "BADUSDT", "ETHUSDT")
    };
    for (field, value) in [
        ("baseCoin", Some(json!({"unexpected":"asset"}))),
        ("quoteCoin", Some(json!(false))),
        ("baseCoin", Some(Value::Null)),
        ("quoteCoin", Some(json!(17))),
        ("baseCoin", None),
    ] {
        let mut bad = instrument(bad_symbol, "BAD");
        if let Some(value) = &value {
            bad[field] = value.clone();
        } else {
            bad.as_object_mut().unwrap().remove(field);
        }
        let h = support::harness_as(name.clone(), support::options(), |wire| {
            wire.ok("GET", "/v5/account/fee-rate", json!({"list":[]}));
            wire.ok(
                "GET",
                "/v5/market/instruments-info",
                json!({"list":[instrument(first,"BTC"),bad,instrument(last,"ETH")]}),
            );
            wire.route_when(
                "GET",
                "/v5/market/instruments-info",
                "status",
                "PreLaunch",
                json!({"retCode":0,"retMsg":"OK","result":{"list":[]}}),
            );
        });
        let markets = h
            .drive(h.venue().fetch_markets())
            .expect("malformed asset row must not reject its valid siblings");
        assert_eq!(
            markets
                .iter()
                .map(|m| m.symbol.as_str())
                .collect::<Vec<_>>(),
            [first, last],
            "{name:?} {field}"
        );
        assert_eq!(markets[0].base, "BTC");
        assert_eq!(markets[1].base, "ETH");
        let logs = h.runtime.logs.borrow();
        let report = logs
            .iter()
            .find(|line| line.contains("Refused Bybit market row"))
            .expect("typed row refusal must visibly report its field/value");
        assert!(report.contains(bad_symbol), "{report}");
        assert!(
            report.contains(if field == "baseCoin" {
                "base="
            } else {
                "quote="
            }),
            "{report}"
        );
        let expected = match &value {
            Some(Value::Object(_)) => "asset".into(),
            Some(value) => value.to_string(),
            None => "undefined".into(),
        };
        assert!(report.contains(&expected), "{report}");
        drop(logs);
        h.drive(h.venue().refresh_markets())
            .expect("refresh admits the same good rows");
        assert_eq!(
            h.store
                .markets()
                .iter()
                .map(|m| m.symbol.as_str())
                .collect::<Vec<_>>(),
            [first, last]
        );
    }
}

#[test]
fn primary_market_rows_keep_valid_siblings_and_report_refusals() {
    mixed_rows(ExchangeName::Bybit);
}
#[test]
fn demo_market_rows_keep_valid_siblings_and_report_refusals() {
    mixed_rows(ExchangeName::Bybitdemo);
}
#[test]
fn inverse_market_rows_keep_valid_siblings_and_report_refusals() {
    mixed_rows(ExchangeName::Bybitinverse);
}
#[test]
fn spot_market_rows_keep_valid_siblings_and_report_refusals() {
    mixed_rows(ExchangeName::Bybitspot);
}
