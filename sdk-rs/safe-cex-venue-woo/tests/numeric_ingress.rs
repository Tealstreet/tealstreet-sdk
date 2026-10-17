#[path = "support/field_identity.rs"]
mod identity;
use safe_cex_num::numeric_ingress::{
    JsNumberOutput, js_from_str, js_from_value, js_from_value_ref,
};
use safe_cex_venue_woo::legacy_fills::WooLegacyFillsQuery;
use safe_cex_venue_woo::{types::*, v3_contracts::*, v3_public::*};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
fn objects(v: &Value, out: &mut Vec<Value>) {
    match v {
        Value::Object(m) => {
            out.push(v.clone());
            for v in m.values() {
                objects(v, out);
            }
        }
        Value::Array(a) => {
            for v in a {
                objects(v, out);
            }
        }
        _ => {}
    }
}
// Source-bit regressions through the repaired public decoders and shared entry APIs.
// Read the native typed field before serialization can erase non-finite values.
fn probe<T: DeserializeOwned + serde::Serialize + JsNumberOutput>(
    candidates: &[Value],
    name: &str,
    key: &str,
    get: fn(&T) -> f64,
    rows: &mut Vec<String>,
) {
    let seed = candidates
        .iter()
        .find(|v| serde_json::from_value::<T>((*v).clone()).is_ok())
        .unwrap_or_else(|| panic!("missing seed {name}"));

    for (mode, literal) in [
        ("lexical", "-0"),
        ("decimal", "-0.0"),
        ("positive-overflow", "1e400"),
        ("negative-overflow", "-1e400"),
        ("underflow", "-1e-4000"),
    ] {
        let mut input = seed.clone();
        input[key] = serde_json::from_str(literal).unwrap();
        let source_bits = safe_cex_num::js_number(Some(&input[key]))
            .unwrap()
            .to_bits();
        for (path, result) in [
            ("Value", serde_json::from_value::<T>(input.clone())),
            ("raw", serde_json::from_str::<T>(&input.to_string())),
            (
                "raw slice",
                serde_json::from_slice::<T>(input.to_string().as_bytes()),
            ),
            ("borrowed Value", T::deserialize(&input)),
            ("shared raw", js_from_str::<T>(&input.to_string())),
            ("shared owned", js_from_value::<T>(input.clone())),
            ("shared borrowed", js_from_value_ref::<T>(&input)),
        ] {
            let actual = get(&result.unwrap_or_else(|e| panic!("{name}.{key} {path} {mode}: {e}")))
                .to_bits();
            assert_eq!(
                actual, source_bits,
                "{name}.{key} {path} {mode} source bits"
            );
        }
    }
    let mut input = seed.clone();
    input[key] = Value::from(-0.0);
    let x: T = serde_json::from_value(input).unwrap();
    assert_eq!(
        get(&x).to_bits(),
        (-0.0_f64).to_bits(),
        "{name}.{key} native negative zero"
    );
    rows.push(format!("{name}.{key}"));
}
#[test]
fn map_only_numeric_ingress_source_bits() {
    let mut candidates = Vec::new();
    for text in [
        include_str!("fixtures/types.json"),
        &identity::capture("types"),
        &identity::capture("contracts"),
        &identity::capture("public"),
        include_str!("fixtures/contracts.json"),
        include_str!("fixtures/public-records.json"),
        include_str!("fixtures/fills.json"),
    ] {
        objects(
            &serde_json::from_str::<Value>(text).unwrap(),
            &mut candidates,
        );
    }
    let mut rows = Vec::new();
    probe::<AccountResponse>(
        &candidates,
        "AccountResponse",
        "leverage",
        |x| x.leverage,
        &mut rows,
    );
    probe::<AccountResponse>(
        &candidates,
        "AccountResponse",
        "takerFeeRate",
        |x| x.taker_fee_rate,
        &mut rows,
    );
    probe::<AccountResponse>(
        &candidates,
        "AccountResponse",
        "makerFeeRate",
        |x| x.maker_fee_rate,
        &mut rows,
    );
    probe::<AccountResponse>(
        &candidates,
        "AccountResponse",
        "interestRate",
        |x| x.interest_rate,
        &mut rows,
    );
    probe::<AccountResponse>(
        &candidates,
        "AccountResponse",
        "futuresTakerFeeRate",
        |x| x.futures_taker_fee_rate,
        &mut rows,
    );
    probe::<AccountResponse>(
        &candidates,
        "AccountResponse",
        "futuresMakerFeeRate",
        |x| x.futures_maker_fee_rate,
        &mut rows,
    );
    probe::<AccountResponse>(
        &candidates,
        "AccountResponse",
        "marginRatio",
        |x| x.margin_ratio,
        &mut rows,
    );
    probe::<AccountResponse>(
        &candidates,
        "AccountResponse",
        "openMarginRatio",
        |x| x.open_margin_ratio,
        &mut rows,
    );
    probe::<AccountResponse>(
        &candidates,
        "AccountResponse",
        "initialMarginRatio",
        |x| x.initial_margin_ratio,
        &mut rows,
    );
    probe::<AccountResponse>(
        &candidates,
        "AccountResponse",
        "maintenanceMarginRatio",
        |x| x.maintenance_margin_ratio,
        &mut rows,
    );
    probe::<AccountResponse>(
        &candidates,
        "AccountResponse",
        "totalCollateral",
        |x| x.total_collateral,
        &mut rows,
    );
    probe::<AccountResponse>(
        &candidates,
        "AccountResponse",
        "freeCollateral",
        |x| x.free_collateral,
        &mut rows,
    );
    probe::<AccountResponse>(
        &candidates,
        "AccountResponse",
        "totalAccountValue",
        |x| x.total_account_value,
        &mut rows,
    );
    probe::<AccountResponse>(
        &candidates,
        "AccountResponse",
        "totalVaultValue",
        |x| x.total_vault_value,
        &mut rows,
    );
    probe::<AccountResponse>(
        &candidates,
        "AccountResponse",
        "totalStakingValue",
        |x| x.total_staking_value,
        &mut rows,
    );
    probe::<AccountResponse>(
        &candidates,
        "AccountResponse",
        "totalLaunchpadVaultValue",
        |x| x.total_launchpad_vault_value,
        &mut rows,
    );
    probe::<AccountResponse>(
        &candidates,
        "AccountResponse",
        "totalEarnValue",
        |x| x.total_earn_value,
        &mut rows,
    );
    probe::<PlaceOrderResponse>(
        &candidates,
        "PlaceOrderResponse",
        "order_id",
        |x| x.order_id,
        &mut rows,
    );
    probe::<PlaceOrderResponse>(
        &candidates,
        "PlaceOrderResponse",
        "order_price",
        |x| x.order_price,
        &mut rows,
    );
    probe::<PlaceOrderResponse>(
        &candidates,
        "PlaceOrderResponse",
        "order_quantity",
        |x| x.order_quantity,
        &mut rows,
    );
    probe::<PlaceOrderResponse>(
        &candidates,
        "PlaceOrderResponse",
        "order_amount",
        |x| x.order_amount,
        &mut rows,
    );
    probe::<PlaceOrderResponse>(
        &candidates,
        "PlaceOrderResponse",
        "client_order_id",
        |x| x.client_order_id,
        &mut rows,
    );
    probe::<PlaceAlgoOrderResponse>(
        &candidates,
        "PlaceAlgoOrderResponse",
        "timestamp",
        |x| x.timestamp,
        &mut rows,
    );
    probe::<GetFuturesLeverageResponse>(
        &candidates,
        "GetFuturesLeverageResponse",
        "timestamp",
        |x| x.timestamp,
        &mut rows,
    );
    probe::<GetKlineResponseDataRowsItem>(
        &candidates,
        "GetKlineResponseDataRowsItem",
        "open",
        |x| x.open,
        &mut rows,
    );
    probe::<GetKlineResponseDataRowsItem>(
        &candidates,
        "GetKlineResponseDataRowsItem",
        "close",
        |x| x.close,
        &mut rows,
    );
    probe::<GetKlineResponseDataRowsItem>(
        &candidates,
        "GetKlineResponseDataRowsItem",
        "low",
        |x| x.low,
        &mut rows,
    );
    probe::<GetKlineResponseDataRowsItem>(
        &candidates,
        "GetKlineResponseDataRowsItem",
        "high",
        |x| x.high,
        &mut rows,
    );
    probe::<GetKlineResponseDataRowsItem>(
        &candidates,
        "GetKlineResponseDataRowsItem",
        "volume",
        |x| x.volume,
        &mut rows,
    );
    probe::<GetKlineResponseDataRowsItem>(
        &candidates,
        "GetKlineResponseDataRowsItem",
        "amount",
        |x| x.amount,
        &mut rows,
    );
    probe::<GetKlineResponseDataRowsItem>(
        &candidates,
        "GetKlineResponseDataRowsItem",
        "start_timestamp",
        |x| x.start_timestamp,
        &mut rows,
    );
    probe::<GetKlineResponseDataRowsItem>(
        &candidates,
        "GetKlineResponseDataRowsItem",
        "end_timestamp",
        |x| x.end_timestamp,
        &mut rows,
    );
    probe::<GetKlineResponseDataMeta>(
        &candidates,
        "GetKlineResponseDataMeta",
        "total",
        |x| x.total,
        &mut rows,
    );
    probe::<GetKlineResponseDataMeta>(
        &candidates,
        "GetKlineResponseDataMeta",
        "records_per_page",
        |x| x.records_per_page,
        &mut rows,
    );
    probe::<GetKlineResponseDataMeta>(
        &candidates,
        "GetKlineResponseDataMeta",
        "current_page",
        |x| x.current_page,
        &mut rows,
    );
    probe::<GetKlineResponse>(
        &candidates,
        "GetKlineResponse",
        "timestamp",
        |x| x.timestamp,
        &mut rows,
    );
    probe::<WooV3Response<safe_cex_num::numeric_ingress::JsNumberInput>>(
        &candidates,
        "WooV3Response",
        "timestamp",
        |x| x.timestamp,
        &mut rows,
    );
    probe::<WooV3PageMeta>(
        &candidates,
        "WooV3PageMeta",
        "total",
        |x| x.total,
        &mut rows,
    );
    probe::<WooV3PageMeta>(
        &candidates,
        "WooV3PageMeta",
        "recordsPerPage",
        |x| x.records_per_page,
        &mut rows,
    );
    probe::<WooV3PageMeta>(
        &candidates,
        "WooV3PageMeta",
        "currentPage",
        |x| x.current_page,
        &mut rows,
    );
    probe::<WooV3Instrument>(
        &candidates,
        "WooV3Instrument",
        "fundingIntervalHours",
        |x| x.funding_interval_hours.unwrap(),
        &mut rows,
    );
    probe::<WooV3Balance>(
        &candidates,
        "WooV3Balance",
        "timestamp",
        |x| x.timestamp,
        &mut rows,
    );
    probe::<WooV3Position>(
        &candidates,
        "WooV3Position",
        "timestamp",
        |x| x.timestamp,
        &mut rows,
    );
    probe::<WooV3AccountInfo>(
        &candidates,
        "WooV3AccountInfo",
        "leverage",
        |x| x.leverage,
        &mut rows,
    );
    probe::<WooV3PlaceOrderRequest>(
        &candidates,
        "WooV3PlaceOrderRequest",
        "clientOrderId",
        |x| x.client_order_id.unwrap(),
        &mut rows,
    );
    probe::<WooV3PlaceOrderResult>(
        &candidates,
        "WooV3PlaceOrderResult",
        "orderId",
        |x| x.order_id,
        &mut rows,
    );
    probe::<WooV3PlaceOrderResult>(
        &candidates,
        "WooV3PlaceOrderResult",
        "clientOrderId",
        |x| x.client_order_id,
        &mut rows,
    );
    probe::<WooV3PlaceOrderResult>(
        &candidates,
        "WooV3PlaceOrderResult",
        "bidAskLevel",
        |x| x.bid_ask_level,
        &mut rows,
    );
    probe::<WooV3ListenKeyResult>(
        &candidates,
        "WooV3ListenKeyResult",
        "expiredTime",
        |x| x.expired_time,
        &mut rows,
    );
    probe::<WooV3FuturesRow>(
        &candidates,
        "WooV3FuturesRow",
        "nextFundingTime",
        |x| x.next_funding_time,
        &mut rows,
    );
    probe::<WooV3OrderBookSnapshot>(
        &candidates,
        "WooV3OrderBookSnapshot",
        "timestamp",
        |x| x.timestamp,
        &mut rows,
    );
    probe::<WooV3KlineRow>(
        &candidates,
        "WooV3KlineRow",
        "startTimestamp",
        |x| x.start_timestamp,
        &mut rows,
    );
    probe::<WooV3KlineRow>(
        &candidates,
        "WooV3KlineRow",
        "endTimestamp",
        |x| x.end_timestamp,
        &mut rows,
    );
    probe::<WooV3FuturesStats>(
        &candidates,
        "WooV3FuturesStats",
        "index",
        |x| x.index,
        &mut rows,
    );
    probe::<WooV3FuturesStats>(
        &candidates,
        "WooV3FuturesStats",
        "mark",
        |x| x.mark,
        &mut rows,
    );
    probe::<WooV3FuturesStats>(
        &candidates,
        "WooV3FuturesStats",
        "open24h",
        |x| x.open24h,
        &mut rows,
    );
    probe::<WooV3FuturesStats>(
        &candidates,
        "WooV3FuturesStats",
        "close24h",
        |x| x.close24h,
        &mut rows,
    );
    probe::<WooV3FuturesStats>(
        &candidates,
        "WooV3FuturesStats",
        "high24h",
        |x| x.high24h,
        &mut rows,
    );
    probe::<WooV3FuturesStats>(
        &candidates,
        "WooV3FuturesStats",
        "low24h",
        |x| x.low24h,
        &mut rows,
    );
    probe::<WooV3FuturesStats>(
        &candidates,
        "WooV3FuturesStats",
        "percentage24h",
        |x| x.percentage24h,
        &mut rows,
    );
    probe::<WooV3FuturesStats>(
        &candidates,
        "WooV3FuturesStats",
        "openInterest",
        |x| x.open_interest,
        &mut rows,
    );
    probe::<WooV3FuturesStats>(
        &candidates,
        "WooV3FuturesStats",
        "estimatedFundingRate",
        |x| x.estimated_funding_rate,
        &mut rows,
    );
    probe::<WooV3FuturesStats>(
        &candidates,
        "WooV3FuturesStats",
        "lastFundingRate",
        |x| x.last_funding_rate,
        &mut rows,
    );
    probe::<WooV3FuturesStats>(
        &candidates,
        "WooV3FuturesStats",
        "fundingTime",
        |x| x.funding_time,
        &mut rows,
    );
    probe::<WooV3FuturesStats>(
        &candidates,
        "WooV3FuturesStats",
        "volume24h",
        |x| x.volume24h,
        &mut rows,
    );
    probe::<WooV3FuturesStats>(
        &candidates,
        "WooV3FuturesStats",
        "quoteVolume24h",
        |x| x.quote_volume24h,
        &mut rows,
    );
    probe::<WooV3MappedOrderBookSnapshot>(
        &candidates,
        "WooV3MappedOrderBookSnapshot",
        "timestamp",
        |x| x.timestamp,
        &mut rows,
    );
    probe::<WooLegacyFillsQuery>(
        &candidates,
        "WooLegacyFillsQuery",
        "fromMilliseconds",
        |x| x.from_milliseconds.unwrap(),
        &mut rows,
    );
    probe::<WooLegacyFillsQuery>(
        &candidates,
        "WooLegacyFillsQuery",
        "toMilliseconds",
        |x| x.to_milliseconds.unwrap(),
        &mut rows,
    );
    probe::<WooLegacyFillsQuery>(
        &candidates,
        "WooLegacyFillsQuery",
        "limit",
        |x| x.limit,
        &mut rows,
    );
    probe::<WooLegacyFillsQuery>(
        &candidates,
        "WooLegacyFillsQuery",
        "page",
        |x| x.page.unwrap(),
        &mut rows,
    );
    probe::<WooV3Instrument>(
        &candidates,
        "WooV3Instrument",
        "baseAssetMultiplier",
        |x| match &x.base_asset_multiplier.as_ref().unwrap() {
            WooV3InstrumentBaseAssetMultiplier::Number(n) => *n,
            _ => panic!("numeric arm"),
        },
        &mut rows,
    );
    probe::<WooV3Instrument>(
        &candidates,
        "WooV3Instrument",
        "baseMin",
        |x| match &x.base_min {
            WooV3InstrumentBaseMin::Number(n) => *n,
            _ => panic!("numeric arm"),
        },
        &mut rows,
    );
    probe::<WooV3Instrument>(
        &candidates,
        "WooV3Instrument",
        "baseMax",
        |x| match &x.base_max {
            WooV3InstrumentBaseMax::Number(n) => *n,
            _ => panic!("numeric arm"),
        },
        &mut rows,
    );
    probe::<WooV3Instrument>(
        &candidates,
        "WooV3Instrument",
        "baseTick",
        |x| match &x.base_tick {
            WooV3InstrumentBaseTick::Number(n) => *n,
            _ => panic!("numeric arm"),
        },
        &mut rows,
    );
    probe::<WooV3Instrument>(
        &candidates,
        "WooV3Instrument",
        "quoteTick",
        |x| match &x.quote_tick {
            WooV3InstrumentQuoteTick::Number(n) => *n,
            _ => panic!("numeric arm"),
        },
        &mut rows,
    );
    probe::<WooV3Instrument>(
        &candidates,
        "WooV3Instrument",
        "minNotional",
        |x| match &x.min_notional {
            WooV3InstrumentMinNotional::Number(n) => *n,
            _ => panic!("numeric arm"),
        },
        &mut rows,
    );
    probe::<WooV3Instrument>(
        &candidates,
        "WooV3Instrument",
        "baseIMR",
        |x| match &x.base_imr {
            WooV3InstrumentBaseIMR::Number(n) => *n,
            _ => panic!("numeric arm"),
        },
        &mut rows,
    );
    assert_eq!(rows.len(), 78, "numeric field inventory");
    assert_eq!(
        rows.iter()
            .map(|r| r.split('.').next().unwrap())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        22,
        "numeric record inventory"
    );
}

#[test]
fn live_javascript_ingress_bits_before_json_projection() {
    let output = std::process::Command::new("node").args(["-e", r#"
const inputs=['-0','-0.0','1e400','-1e400','-1e-4000'];
process.stdout.write(JSON.stringify(inputs.map(s=>{const n=JSON.parse('{"n":'+s+'}').n;const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex')})));
"#]).output().unwrap();
    assert!(
        output.status.success(),
        "Node must execute: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let bits: Vec<String> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        bits,
        [
            "8000000000000000",
            "8000000000000000",
            "7ff0000000000000",
            "fff0000000000000",
            "8000000000000000"
        ]
    );
}

#[test]
fn rows_container_preserves_page_meta_ingress_bits() {
    let input: Value = serde_json::from_str(
        r#"{"rows":[],"meta":{"total":-0,"recordsPerPage":-0,"currentPage":-0}}"#,
    )
    .unwrap();
    type Rows = WooV3Rows<safe_cex_num::numeric_ingress::JsNumberInput>;
    for (path, result) in [
        ("raw", serde_json::from_str::<Rows>(&input.to_string())),
        ("owned", serde_json::from_value::<Rows>(input.clone())),
        (
            "borrowed",
            <Rows as serde::Deserialize>::deserialize(&input),
        ),
        ("shared raw", js_from_str::<Rows>(&input.to_string())),
        ("shared owned", js_from_value::<Rows>(input.clone())),
        ("shared borrowed", js_from_value_ref::<Rows>(&input)),
    ] {
        let decoded = result.unwrap();
        assert_eq!(
            decoded.rows.len(),
            0,
            "WOO-CARDINALITY metadata-only rows {path}"
        );
        let meta = decoded.meta.unwrap();
        for (field, n) in [
            ("total", meta.total),
            ("recordsPerPage", meta.records_per_page),
            ("currentPage", meta.current_page),
        ] {
            assert_eq!(
                n.to_bits(),
                (-0.0_f64).to_bits(),
                "rows {path} {field} preserves page metadata sign"
            );
        }
    }
}

#[test]
fn mapped_orderbook_flattened_numbers_preserve_bits() {
    for literal in ["-0", "-0.0", "1e400", "-1e400", "-1e-4000"] {
        let raw = format!(
            r#"{{"timestamp":{literal},"bids":[{{"price":{literal},"amount":{literal},"total":{literal}}}],"asks":[{{"price":{literal},"amount":{literal},"total":{literal}}}]}}"#
        );
        let input: Value = serde_json::from_str(&raw).unwrap();
        let expected = safe_cex_num::js_number(Some(&input["timestamp"]))
            .unwrap()
            .to_bits();
        type Book = WooV3MappedOrderBookSnapshot;
        for (path, result) in [
            ("raw", serde_json::from_str::<Book>(&raw)),
            ("slice", serde_json::from_slice::<Book>(raw.as_bytes())),
            ("owned", serde_json::from_value::<Book>(input.clone())),
            ("borrowed", Book::deserialize(&input)),
            ("shared raw", js_from_str::<Book>(&raw)),
            ("shared owned", js_from_value::<Book>(input.clone())),
            ("shared borrowed", js_from_value_ref::<Book>(&input)),
        ] {
            assert!(
                result.is_ok(),
                "flattened book {path} {literal} acceptance: {result:?}"
            );
            let book = result.unwrap();
            assert_eq!(
                book.book.bids.len(),
                1,
                "WOO-CARDINALITY flattened bid count {path}"
            );
            assert_eq!(
                book.book.asks.len(),
                1,
                "WOO-CARDINALITY flattened ask count {path}"
            );
            for (field, n) in [
                ("timestamp", book.timestamp),
                ("bid price", book.book.bids[0].price),
                ("bid amount", book.book.bids[0].amount),
                ("bid total", book.book.bids[0].total),
                ("ask price", book.book.asks[0].price),
                ("ask amount", book.book.asks[0].amount),
                ("ask total", book.book.asks[0].total),
            ] {
                assert_eq!(
                    n.to_bits(),
                    expected,
                    "flattened book {path} {literal} {field} bits"
                );
            }
        }
    }
}

#[test]
fn standalone_numeric_unions_preserve_bits_and_reject_other_shapes() {
    for literal in ["-0", "-0.0", "1e400", "-1e400", "-1e-4000"] {
        let input: Value = serde_json::from_str(literal).unwrap();
        let expected = safe_cex_num::js_number(Some(&input)).unwrap().to_bits();
        for (path, result) in [
            (
                "raw",
                serde_json::from_str::<PlaceAlgoOrderResponseDataRowsItemOrderId>(literal),
            ),
            (
                "public qualified",
                PlaceAlgoOrderResponseDataRowsItemOrderId::deserialize(input.clone()),
            ),
            (
                "Value",
                serde_json::from_value::<PlaceAlgoOrderResponseDataRowsItemOrderId>(input.clone()),
            ),
            (
                "shared raw",
                js_from_str::<PlaceAlgoOrderResponseDataRowsItemOrderId>(literal),
            ),
            (
                "shared owned",
                js_from_value::<PlaceAlgoOrderResponseDataRowsItemOrderId>(input.clone()),
            ),
            (
                "shared borrowed",
                js_from_value_ref::<PlaceAlgoOrderResponseDataRowsItemOrderId>(&input),
            ),
        ] {
            assert!(
                result.is_ok(),
                "PlaceAlgoOrderResponseDataRowsItemOrderId {path} {literal} numeric union acceptance: {result:?}"
            );
            let PlaceAlgoOrderResponseDataRowsItemOrderId::Number(number) = result.unwrap() else {
                panic!("numeric branch")
            };
            assert_eq!(
                number.to_bits(),
                expected,
                "PlaceAlgoOrderResponseDataRowsItemOrderId {path} {literal} bits"
            );
        }
    }
    for input in ["null", "true", "[]", "{}", "{\"Number\":0}"] {
        assert!(
            serde_json::from_str::<PlaceAlgoOrderResponseDataRowsItemOrderId>(input).is_err(),
            "PlaceAlgoOrderResponseDataRowsItemOrderId rejects {input}"
        );
    }
    assert!(serde_json::from_str::<PlaceAlgoOrderResponseDataRowsItemOrderId>(r#""1.25""#).is_ok());
    for literal in ["-0", "-0.0", "1e400", "-1e400", "-1e-4000"] {
        let input: Value = serde_json::from_str(literal).unwrap();
        let expected = safe_cex_num::js_number(Some(&input)).unwrap().to_bits();
        for (path, result) in [
            (
                "raw",
                serde_json::from_str::<WooV3InstrumentBaseAssetMultiplier>(literal),
            ),
            (
                "public qualified",
                WooV3InstrumentBaseAssetMultiplier::deserialize(input.clone()),
            ),
            (
                "Value",
                serde_json::from_value::<WooV3InstrumentBaseAssetMultiplier>(input.clone()),
            ),
            (
                "shared raw",
                js_from_str::<WooV3InstrumentBaseAssetMultiplier>(literal),
            ),
            (
                "shared owned",
                js_from_value::<WooV3InstrumentBaseAssetMultiplier>(input.clone()),
            ),
            (
                "shared borrowed",
                js_from_value_ref::<WooV3InstrumentBaseAssetMultiplier>(&input),
            ),
        ] {
            assert!(
                result.is_ok(),
                "WooV3InstrumentBaseAssetMultiplier {path} {literal} numeric union acceptance: {result:?}"
            );
            let WooV3InstrumentBaseAssetMultiplier::Number(number) = result.unwrap() else {
                panic!("numeric branch")
            };
            assert_eq!(
                number.to_bits(),
                expected,
                "WooV3InstrumentBaseAssetMultiplier {path} {literal} bits"
            );
        }
    }
    for input in ["null", "true", "[]", "{}", "{\"Number\":0}"] {
        assert!(
            serde_json::from_str::<WooV3InstrumentBaseAssetMultiplier>(input).is_err(),
            "WooV3InstrumentBaseAssetMultiplier rejects {input}"
        );
    }
    assert!(serde_json::from_str::<WooV3InstrumentBaseAssetMultiplier>(r#""1.25""#).is_ok());
    for literal in ["-0", "-0.0", "1e400", "-1e400", "-1e-4000"] {
        let input: Value = serde_json::from_str(literal).unwrap();
        let expected = safe_cex_num::js_number(Some(&input)).unwrap().to_bits();
        for (path, result) in [
            (
                "raw",
                serde_json::from_str::<WooV3InstrumentBaseMin>(literal),
            ),
            (
                "public qualified",
                WooV3InstrumentBaseMin::deserialize(input.clone()),
            ),
            (
                "Value",
                serde_json::from_value::<WooV3InstrumentBaseMin>(input.clone()),
            ),
            ("shared raw", js_from_str::<WooV3InstrumentBaseMin>(literal)),
            (
                "shared owned",
                js_from_value::<WooV3InstrumentBaseMin>(input.clone()),
            ),
            (
                "shared borrowed",
                js_from_value_ref::<WooV3InstrumentBaseMin>(&input),
            ),
        ] {
            assert!(
                result.is_ok(),
                "WooV3InstrumentBaseMin {path} {literal} numeric union acceptance: {result:?}"
            );
            let WooV3InstrumentBaseMin::Number(number) = result.unwrap() else {
                panic!("numeric branch")
            };
            assert_eq!(
                number.to_bits(),
                expected,
                "WooV3InstrumentBaseMin {path} {literal} bits"
            );
        }
    }
    for input in ["null", "true", "[]", "{}", "{\"Number\":0}"] {
        assert!(
            serde_json::from_str::<WooV3InstrumentBaseMin>(input).is_err(),
            "WooV3InstrumentBaseMin rejects {input}"
        );
    }
    assert!(serde_json::from_str::<WooV3InstrumentBaseMin>(r#""1.25""#).is_ok());
    for literal in ["-0", "-0.0", "1e400", "-1e400", "-1e-4000"] {
        let input: Value = serde_json::from_str(literal).unwrap();
        let expected = safe_cex_num::js_number(Some(&input)).unwrap().to_bits();
        for (path, result) in [
            (
                "raw",
                serde_json::from_str::<WooV3InstrumentBaseMax>(literal),
            ),
            (
                "public qualified",
                WooV3InstrumentBaseMax::deserialize(input.clone()),
            ),
            (
                "Value",
                serde_json::from_value::<WooV3InstrumentBaseMax>(input.clone()),
            ),
            ("shared raw", js_from_str::<WooV3InstrumentBaseMax>(literal)),
            (
                "shared owned",
                js_from_value::<WooV3InstrumentBaseMax>(input.clone()),
            ),
            (
                "shared borrowed",
                js_from_value_ref::<WooV3InstrumentBaseMax>(&input),
            ),
        ] {
            assert!(
                result.is_ok(),
                "WooV3InstrumentBaseMax {path} {literal} numeric union acceptance: {result:?}"
            );
            let WooV3InstrumentBaseMax::Number(number) = result.unwrap() else {
                panic!("numeric branch")
            };
            assert_eq!(
                number.to_bits(),
                expected,
                "WooV3InstrumentBaseMax {path} {literal} bits"
            );
        }
    }
    for input in ["null", "true", "[]", "{}", "{\"Number\":0}"] {
        assert!(
            serde_json::from_str::<WooV3InstrumentBaseMax>(input).is_err(),
            "WooV3InstrumentBaseMax rejects {input}"
        );
    }
    assert!(serde_json::from_str::<WooV3InstrumentBaseMax>(r#""1.25""#).is_ok());
    for literal in ["-0", "-0.0", "1e400", "-1e400", "-1e-4000"] {
        let input: Value = serde_json::from_str(literal).unwrap();
        let expected = safe_cex_num::js_number(Some(&input)).unwrap().to_bits();
        for (path, result) in [
            (
                "raw",
                serde_json::from_str::<WooV3InstrumentBaseTick>(literal),
            ),
            (
                "public qualified",
                WooV3InstrumentBaseTick::deserialize(input.clone()),
            ),
            (
                "Value",
                serde_json::from_value::<WooV3InstrumentBaseTick>(input.clone()),
            ),
            (
                "shared raw",
                js_from_str::<WooV3InstrumentBaseTick>(literal),
            ),
            (
                "shared owned",
                js_from_value::<WooV3InstrumentBaseTick>(input.clone()),
            ),
            (
                "shared borrowed",
                js_from_value_ref::<WooV3InstrumentBaseTick>(&input),
            ),
        ] {
            assert!(
                result.is_ok(),
                "WooV3InstrumentBaseTick {path} {literal} numeric union acceptance: {result:?}"
            );
            let WooV3InstrumentBaseTick::Number(number) = result.unwrap() else {
                panic!("numeric branch")
            };
            assert_eq!(
                number.to_bits(),
                expected,
                "WooV3InstrumentBaseTick {path} {literal} bits"
            );
        }
    }
    for input in ["null", "true", "[]", "{}", "{\"Number\":0}"] {
        assert!(
            serde_json::from_str::<WooV3InstrumentBaseTick>(input).is_err(),
            "WooV3InstrumentBaseTick rejects {input}"
        );
    }
    assert!(serde_json::from_str::<WooV3InstrumentBaseTick>(r#""1.25""#).is_ok());
    for literal in ["-0", "-0.0", "1e400", "-1e400", "-1e-4000"] {
        let input: Value = serde_json::from_str(literal).unwrap();
        let expected = safe_cex_num::js_number(Some(&input)).unwrap().to_bits();
        for (path, result) in [
            (
                "raw",
                serde_json::from_str::<WooV3InstrumentQuoteTick>(literal),
            ),
            (
                "public qualified",
                WooV3InstrumentQuoteTick::deserialize(input.clone()),
            ),
            (
                "Value",
                serde_json::from_value::<WooV3InstrumentQuoteTick>(input.clone()),
            ),
            (
                "shared raw",
                js_from_str::<WooV3InstrumentQuoteTick>(literal),
            ),
            (
                "shared owned",
                js_from_value::<WooV3InstrumentQuoteTick>(input.clone()),
            ),
            (
                "shared borrowed",
                js_from_value_ref::<WooV3InstrumentQuoteTick>(&input),
            ),
        ] {
            assert!(
                result.is_ok(),
                "WooV3InstrumentQuoteTick {path} {literal} numeric union acceptance: {result:?}"
            );
            let WooV3InstrumentQuoteTick::Number(number) = result.unwrap() else {
                panic!("numeric branch")
            };
            assert_eq!(
                number.to_bits(),
                expected,
                "WooV3InstrumentQuoteTick {path} {literal} bits"
            );
        }
    }
    for input in ["null", "true", "[]", "{}", "{\"Number\":0}"] {
        assert!(
            serde_json::from_str::<WooV3InstrumentQuoteTick>(input).is_err(),
            "WooV3InstrumentQuoteTick rejects {input}"
        );
    }
    assert!(serde_json::from_str::<WooV3InstrumentQuoteTick>(r#""1.25""#).is_ok());
    for literal in ["-0", "-0.0", "1e400", "-1e400", "-1e-4000"] {
        let input: Value = serde_json::from_str(literal).unwrap();
        let expected = safe_cex_num::js_number(Some(&input)).unwrap().to_bits();
        for (path, result) in [
            (
                "raw",
                serde_json::from_str::<WooV3InstrumentMinNotional>(literal),
            ),
            (
                "public qualified",
                WooV3InstrumentMinNotional::deserialize(input.clone()),
            ),
            (
                "Value",
                serde_json::from_value::<WooV3InstrumentMinNotional>(input.clone()),
            ),
            (
                "shared raw",
                js_from_str::<WooV3InstrumentMinNotional>(literal),
            ),
            (
                "shared owned",
                js_from_value::<WooV3InstrumentMinNotional>(input.clone()),
            ),
            (
                "shared borrowed",
                js_from_value_ref::<WooV3InstrumentMinNotional>(&input),
            ),
        ] {
            assert!(
                result.is_ok(),
                "WooV3InstrumentMinNotional {path} {literal} numeric union acceptance: {result:?}"
            );
            let WooV3InstrumentMinNotional::Number(number) = result.unwrap() else {
                panic!("numeric branch")
            };
            assert_eq!(
                number.to_bits(),
                expected,
                "WooV3InstrumentMinNotional {path} {literal} bits"
            );
        }
    }
    for input in ["null", "true", "[]", "{}", "{\"Number\":0}"] {
        assert!(
            serde_json::from_str::<WooV3InstrumentMinNotional>(input).is_err(),
            "WooV3InstrumentMinNotional rejects {input}"
        );
    }
    assert!(serde_json::from_str::<WooV3InstrumentMinNotional>(r#""1.25""#).is_ok());
    for literal in ["-0", "-0.0", "1e400", "-1e400", "-1e-4000"] {
        let input: Value = serde_json::from_str(literal).unwrap();
        let expected = safe_cex_num::js_number(Some(&input)).unwrap().to_bits();
        for (path, result) in [
            (
                "raw",
                serde_json::from_str::<WooV3InstrumentBaseIMR>(literal),
            ),
            (
                "public qualified",
                WooV3InstrumentBaseIMR::deserialize(input.clone()),
            ),
            (
                "Value",
                serde_json::from_value::<WooV3InstrumentBaseIMR>(input.clone()),
            ),
            ("shared raw", js_from_str::<WooV3InstrumentBaseIMR>(literal)),
            (
                "shared owned",
                js_from_value::<WooV3InstrumentBaseIMR>(input.clone()),
            ),
            (
                "shared borrowed",
                js_from_value_ref::<WooV3InstrumentBaseIMR>(&input),
            ),
        ] {
            assert!(
                result.is_ok(),
                "WooV3InstrumentBaseIMR {path} {literal} numeric union acceptance: {result:?}"
            );
            let WooV3InstrumentBaseIMR::Number(number) = result.unwrap() else {
                panic!("numeric branch")
            };
            assert_eq!(
                number.to_bits(),
                expected,
                "WooV3InstrumentBaseIMR {path} {literal} bits"
            );
        }
    }
    for input in ["null", "true", "[]", "{}", "{\"Number\":0}"] {
        assert!(
            serde_json::from_str::<WooV3InstrumentBaseIMR>(input).is_err(),
            "WooV3InstrumentBaseIMR rejects {input}"
        );
    }
    assert!(serde_json::from_str::<WooV3InstrumentBaseIMR>(r#""1.25""#).is_ok());
}
