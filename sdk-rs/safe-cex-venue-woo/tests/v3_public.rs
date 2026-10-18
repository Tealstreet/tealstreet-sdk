#[path = "support/field_identity.rs"]
mod identity;
use safe_cex_venue_woo::v3_public::*;
use serde::Serialize;
use serde_json::{Value, json};
fn wire<T: Serialize>(r: Result<T, String>) -> Result<Value, String> {
    r.map(|v| serde_json::to_value(v).unwrap())
}
fn same(a: &Value, b: &Value) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(a.as_f64(), b.as_f64()),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                same(a, b)
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len());
            for (k, v) in a {
                same(v, &b[k])
            }
        }
        _ => assert_eq!(a, b),
    }
}
#[test]
fn strict_public_decoders_and_mappers_match_live_typescript() {
    let cases: Value = serde_json::from_str(include_str!("fixtures/public.json")).unwrap();
    assert_eq!(
        cases.as_array().unwrap().len(),
        2184,
        "WOO-CARDINALITY v3_public source total"
    );
    for case in cases.as_array().unwrap() {
        let v = &case["input"];
        let opt = if case["missing"] == true {
            None
        } else {
            Some(v)
        };
        let result = match case["name"].as_str().unwrap() {
            "isRecord" => Ok(json!(is_record(opt))),
            "requireRecord" => wire(require_record(opt, "probe")),
            "requireArray" => wire(require_array(opt, "probe")),
            "requireString" => wire(require_string(opt, "probe")),
            "optionalString" => wire(optional_string(opt, "probe")),
            "requireInteger" => wire(require_integer(opt, "probe")),
            "requireTimestamp" => wire(require_timestamp(opt, "probe")),
            "requireBoolean" => wire(require_boolean(opt, "probe")),
            "requireDecimalString" => wire(require_decimal_string(opt, "probe")),
            "requireInstrumentDecimal" => wire(require_instrument_decimal(opt, "probe")),
            "optionalInstrumentDecimal" => wire(optional_instrument_decimal(opt, "probe")),
            "requireFiniteNumber" => wire(require_finite_number(opt, "probe", 0.0)),
            "decodeWooV3Instruments" => wire(decode_woo_v3_instruments(v)),
            "decodeWooV3Futures" => wire(decode_woo_v3_futures(v)),
            "decodeWooV3Klines" => wire(decode_woo_v3_klines(v)),
            "decodeWooV3OrderBook" => wire(decode_woo_v3_order_book(v)),
            "mapWooV3Markets" => wire(map_woo_v3_markets(
                &serde_json::from_value::<Vec<safe_cex_venue_woo::v3_contracts::WooV3Instrument>>(
                    v.clone(),
                )
                .unwrap(),
            )),
            "mapWooV3FuturesStats" => {
                let markets = case["extra"][0].as_object().unwrap().iter().map(|(k,v)| {
                    let mut complete = json!({"base":"BTC","quote":"USDT","active":true,"precision":{"amount":1,"price":1},"limits":{"amount":{"min":1,"max":100},"leverage":{"min":1,"max":100}}});
                    complete.as_object_mut().unwrap().extend(v.as_object().unwrap().clone());
                    (k.clone(), serde_json::from_value(complete).unwrap())
                }).collect();
                wire(map_woo_v3_futures_stats(
                    &serde_json::from_value::<Vec<WooV3FuturesRow>>(v.clone()).unwrap(),
                    &markets,
                ))
            }
            "mapWooV3OrderBook" => wire(map_woo_v3_order_book(
                &serde_json::from_value(v.clone()).unwrap(),
            )),
            "mapWooV3Candles" => {
                let mut input = v.clone();
                if case["nonfinite"] == true {
                    input[1]["startTimestamp"] = json!(0);
                }
                let mut rows = serde_json::from_value::<Vec<WooV3KlineRow>>(input).unwrap();
                if case["nonfinite"] == true {
                    rows[1].start_timestamp = f64::NAN;
                }
                wire(map_woo_v3_candles(&rows))
            }
            name => panic!("unclaimed oracle {name}"),
        };
        if let Some(error) = case.get("error") {
            assert_eq!(result, Err(error.as_str().unwrap().into()), "{case}");
        } else {
            let actual = result.unwrap_or_else(|e| panic!("{case}: {e}"));
            same(&actual, &case["ok"]);
        }
    }
}

#[test]
fn public_records_accept_source_objects_and_reject_sequences_and_wrong_fields() {
    use serde::Deserialize;
    let records: Value =
        serde_json::from_str(include_str!("fixtures/public-records.json")).unwrap();
    let identity: Value = serde_json::from_str(&identity::capture("public")).unwrap();
    macro_rules! check { ($ty:ty, $name:literal, $fields:expr) => {{
      for input in [&records[$name], &identity[$name]] {
        assert!(<$ty>::deserialize(input.clone()).is_ok(), "{} valid record", $name);
        assert!(<$ty>::deserialize(Value::Array($fields.iter().map(|k| input[*k].clone()).collect())).is_err(),concat!($name," positional arrays accepted"));
        for key in input.as_object().unwrap().keys() {
            let mut missing=input.clone();missing.as_object_mut().unwrap().remove(key);
            assert!(<$ty>::deserialize(missing).is_err(),"{} missing {key}",$name);
            let mut null=input.clone();null[key]=Value::Null;
            assert!(<$ty>::deserialize(null).is_err(),"{} null {key}",$name);
            let mut wrong=input.clone();wrong[key]=json!({"wrong":true});
            assert!(<$ty>::deserialize(wrong).is_err(),"{} object {key}",$name);
        }
      }
    }}; }
    check!(
        WooV3FuturesRow,
        "WooV3FuturesRow",
        [
            "symbol",
            "indexPrice",
            "markPrice",
            "estFundingRate",
            "lastFundingRate",
            "openInterest",
            "24hOpen",
            "24hClose",
            "24hHigh",
            "24hLow",
            "24hVolume",
            "24hAmount",
            "nextFundingTime"
        ]
    );
    check!(
        WooV3OrderBookLevel,
        "WooV3OrderBookLevel",
        ["price", "quantity"]
    );
    check!(
        WooV3OrderBookSnapshot,
        "WooV3OrderBookSnapshot",
        ["timestamp", "asks", "bids"]
    );
    check!(
        WooV3MappedOrderBookSnapshot,
        "WooV3MappedOrderBookSnapshot",
        ["bids", "asks", "timestamp"]
    );
    check!(
        WooV3KlineRow,
        "WooV3KlineRow",
        [
            "open",
            "close",
            "low",
            "high",
            "volume",
            "amount",
            "symbol",
            "type",
            "startTimestamp",
            "endTimestamp"
        ]
    );
    check!(
        WooV3FuturesStats,
        "WooV3FuturesStats",
        [
            "id",
            "symbol",
            "index",
            "mark",
            "open24h",
            "close24h",
            "high24h",
            "low24h",
            "percentage24h",
            "openInterest",
            "estimatedFundingRate",
            "lastFundingRate",
            "fundingTime",
            "volume24h",
            "quoteVolume24h"
        ]
    );
}

#[test]
fn market_info_snapshot_differs_from_live_source_alias() {
    let witness: Value = serde_json::from_str(include_str!("fixtures/market-alias.json")).unwrap();
    let mut row: safe_cex_venue_woo::v3_contracts::WooV3Instrument =
        serde_json::from_value(witness["input"].clone()).unwrap();
    let markets = map_woo_v3_markets(std::slice::from_ref(&row)).unwrap();
    row.symbol = witness["mutatedSymbol"].as_str().unwrap().to_owned();
    assert_eq!(row.symbol, witness["sourceAfter"]);
    assert_eq!(markets.len(), 1, "WOO-CARDINALITY alias native markets");
    let info = markets[0].info.as_ref().unwrap();
    assert_eq!(
        info.get("symbol").unwrap().as_text().unwrap(),
        witness["snapshotAfter"]
            .as_str()
            .unwrap()
            .encode_utf16()
            .collect::<Vec<_>>()
    );
    assert_ne!(
        info.get("symbol").unwrap().as_text().unwrap(),
        witness["sourceAfter"]
            .as_str()
            .unwrap()
            .encode_utf16()
            .collect::<Vec<_>>(),
        "owned info does not observe the later source mutation"
    );
}
