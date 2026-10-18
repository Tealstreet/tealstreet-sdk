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
                .unwrap()
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

fn mixed_symbol_rows(name: ExchangeName) {
    let inverse = name == ExchangeName::Bybitinverse;
    let (first, bad_symbol, last) = if inverse {
        ("BTCUSD", "BADUSD", "ETHUSD")
    } else {
        ("BTCUSDT", "BADUSDT", "ETHUSDT")
    };
    for value in [
        Some(json!({"symbol":"wrong"})),
        Some(json!(false)),
        Some(Value::Null),
        Some(json!(17)),
        None,
    ] {
        let mut bad = instrument(bad_symbol, "BAD");
        if let Some(value) = &value {
            bad["symbol"] = value.clone();
        } else {
            bad.as_object_mut().unwrap().remove("symbol");
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
        let markets = h.drive(h.venue().fetch_markets()).unwrap();
        assert_eq!(
            markets
                .iter()
                .map(|m| m.symbol.as_str())
                .collect::<Vec<_>>(),
            [first, last]
        );
        let logs = h.runtime.logs.borrow();
        let reports = logs
            .iter()
            .filter(|line| line.contains("Refused Bybit market row"))
            .collect::<Vec<_>>();
        assert_eq!(reports.len(), 1);
        assert!(reports[0].contains("symbol="), "{}", reports[0]);
        let expected = match &value {
            Some(Value::Object(_)) => "wrong".into(),
            Some(value) => value.to_string(),
            None => "undefined".into(),
        };
        assert!(reports[0].contains(&expected), "{}", reports[0]);
        drop(logs);
        h.drive(h.venue().refresh_markets()).unwrap();
        assert_eq!(
            h.store
                .markets()
                .unwrap()
                .iter()
                .map(|m| m.symbol.as_str())
                .collect::<Vec<_>>(),
            [first, last]
        );
    }
}
#[test]
fn primary_market_symbol_refusals_keep_siblings() {
    mixed_symbol_rows(ExchangeName::Bybit);
}
#[test]
fn demo_market_symbol_refusals_keep_siblings() {
    mixed_symbol_rows(ExchangeName::Bybitdemo);
}
#[test]
fn spot_market_symbol_refusals_keep_siblings() {
    mixed_symbol_rows(ExchangeName::Bybitspot);
}
#[test]
fn inverse_market_symbol_refusals_keep_siblings() {
    mixed_symbol_rows(ExchangeName::Bybitinverse);
}

#[test]
fn symbol_refusal_waits_for_source_projection_errors() {
    for name in [
        ExchangeName::Bybit,
        ExchangeName::Bybitdemo,
        ExchangeName::Bybitspot,
        ExchangeName::Bybitinverse,
    ] {
        let mut bad = instrument("BADUSDT", "BAD");
        bad["symbol"] = json!(17);
        bad["lotSizeFilter"]["qtyStep"] = json!({"toString":null});
        let h = support::harness_as(name, support::options(), |wire| {
            wire.ok("GET", "/v5/account/fee-rate", json!({"list":[]}));
            wire.ok("GET", "/v5/market/instruments-info", json!({"list":[bad]}));
            wire.route_when(
                "GET",
                "/v5/market/instruments-info",
                "status",
                "PreLaunch",
                json!({"retCode":0,"retMsg":"OK","result":{"list":[]}}),
            );
        });
        let error = h.drive(h.venue().fetch_markets()).unwrap_err();
        assert!(
            error
                .message
                .contains("Cannot convert object to primitive value"),
            "{error:?}"
        );
        assert!(
            !h.runtime
                .logs
                .borrow()
                .iter()
                .any(|line| line.contains("Refused Bybit market row"))
        );
    }
}

#[test]
fn null_market_row_retains_the_source_property_error() {
    for name in [
        ExchangeName::Bybit,
        ExchangeName::Bybitdemo,
        ExchangeName::Bybitspot,
        ExchangeName::Bybitinverse,
    ] {
        let h = support::harness_as(name, support::options(), |wire| {
            wire.ok("GET", "/v5/account/fee-rate", json!({"list":[]}));
            wire.ok(
                "GET",
                "/v5/market/instruments-info",
                json!({"list":[instrument("BTCUSDT","BTC"),null,instrument("ETHUSDT","ETH")]}),
            );
            wire.route_when(
                "GET",
                "/v5/market/instruments-info",
                "status",
                "PreLaunch",
                json!({"retCode":0,"retMsg":"OK","result":{"list":[]}}),
            );
        });
        let error = h.drive(h.venue().fetch_markets()).unwrap_err();
        assert_eq!(
            error.message,
            "Cannot read properties of null (reading 'symbol')"
        );
        assert!(
            !h.runtime
                .logs
                .borrow()
                .iter()
                .any(|line| line.contains("Refused Bybit market row"))
        );
    }
}

fn distinct_fee_symbols(name: ExchangeName) {
    for symbol in [json!({"symbol":"wrong"}), json!(["wrong"])] {
        let mut bad = instrument("BADUSDT", "BAD");
        bad["symbol"] = symbol.clone();
        let h = support::harness_as(name.clone(), support::options(), |wire| {
            wire.ok(
                "GET",
                "/v5/account/fee-rate",
                json!({"list":[{
                    "symbol":symbol,"takerFeeRate":{"toString":null},"makerFeeRate":"0.02"
                }]}),
            );
            wire.ok(
                "GET",
                "/v5/market/instruments-info",
                json!({"list":[
                    instrument("BTCUSDT","BTC"),bad,instrument("ETHUSDT","ETH")
                ]}),
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
            .expect("distinct fee/instrument objects must not match");
        assert_eq!(
            markets
                .iter()
                .map(|m| m.symbol.as_str())
                .collect::<Vec<_>>(),
            ["BTCUSDT", "ETHUSDT"]
        );
        assert!(markets.iter().all(|m| m.taker_fee.is_none()));
        assert!(
            h.runtime
                .logs
                .borrow()
                .iter()
                .any(|line| line.contains("Refused Bybit market row") && line.contains("symbol="))
        );
        h.drive(h.venue().refresh_markets()).unwrap();
        assert_eq!(h.store.markets().unwrap().len(), 2);
    }
}
#[test]
fn primary_distinct_fee_symbols_do_not_abort_valid_market_rows() {
    distinct_fee_symbols(ExchangeName::Bybit);
}
#[test]
fn demo_distinct_fee_symbols_do_not_abort_valid_market_rows() {
    distinct_fee_symbols(ExchangeName::Bybitdemo);
}
