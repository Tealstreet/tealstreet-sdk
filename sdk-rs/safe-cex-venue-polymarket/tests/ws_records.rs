// Per-case expectation tables. `if i == 0 { n } else { n }` and `[V][i % 1]`
// keep each corpus case's expectation INDEPENDENT even where the values
// currently coincide - the sibling line `if i == 2 { 0 } else { 2 }` is the same
// shape with arms that differ. Collapsing the equal ones to a single constant
// merges two independent expectations, which is the homogenized-corpus false
// green ZIP_CARDINALITY_AUDIT.md exists to prevent.
#![allow(clippy::if_same_then_else, clippy::modulo_one)]
use safe_cex_venue_polymarket::ws_records::*;
use serde_json::Value;
use std::{path::PathBuf, process::Command, sync::OnceLock};
fn oracle() -> &'static Value {
    static RESULT: OnceLock<Value> = OnceLock::new();
    RESULT.get_or_init(|| {
        let output = Command::new("node")
            .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/ws_records_oracle.cjs"))
            .output()
            .expect("source oracle start");
        assert!(
            output.status.success(),
            "source websocket record oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result: Value =
            serde_json::from_slice(&output.stdout).expect("source record corpus JSON");
        assert_eq!(
            result["cases"].as_array().unwrap().len(),
            32,
            "independent source record corpus count"
        );
        result
    })
}
fn native(v: &Value) -> NativeValue {
    match v["kind"].as_str().unwrap() {
        "undefined" => NativeValue::Undefined,
        "null" => NativeValue::Null,
        "text" => NativeValue::Text(
            v["units"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| u16::try_from(v.as_u64().unwrap()).unwrap())
                .collect(),
        ),
        "number" => NativeValue::Number(f64::from_bits(
            u64::from_str_radix(v["bits"].as_str().unwrap(), 16).unwrap(),
        )),
        "bool" => NativeValue::Bool(v["value"].as_bool().unwrap()),
        "array" => NativeValue::Array(v["items"].as_array().unwrap().iter().map(native).collect()),
        "object" => NativeValue::Object(
            v["fields"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| (v[0].as_str().unwrap().into(), native(&v[1])))
                .collect(),
        ),
        other => panic!("unsupported fixture kind {other}"),
    }
}
fn expected_text(role: &str, i: usize) -> Vec<u16> {
    if i == 2 {
        return vec![];
    }
    let mut value = format!("{role}/{i}/").encode_utf16().collect::<Vec<_>>();
    value.extend(match i {
        0 => "plain".encode_utf16().collect::<Vec<_>>(),
        1 => vec![0xd83e, 0xdd80, 0xd800],
        3 => vec![0xdfff],
        _ => panic!("fixture index"),
    });
    value
}
fn expected_number(role: &str, i: usize) -> f64 {
    let seed = role
        .bytes()
        .fold(0_u32, |s, c| s.wrapping_mul(31).wrapping_add(u32::from(c)));
    [
        17.25 + f64::from(seed) / 8.0,
        -0.0,
        f64::NAN,
        9007199254740992.0,
    ][i]
}
fn check_number(actual: f64, i: usize, label: &str) {
    let expected = expected_number(label, i);
    if expected.is_nan() {
        assert!(actual.is_nan(), "native NaN {label}");
    } else {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "native Number bits {label}"
        );
    }
}
fn row(name: &str, i: usize) -> NativeValue {
    let rows = oracle()["cases"].as_array().unwrap();
    let found = rows
        .iter()
        .filter(|v| v["name"] == name && v["index"] == i)
        .collect::<Vec<_>>();
    assert_eq!(
        found.len(),
        1,
        "independent source fixture selection {name}/{i}"
    );
    native(&found[0]["value"])
}

#[test]
fn apiorder_native() {
    for i in 0..4 {
        let input = row("PolymarketAPIOrder", i);
        let NativeValue::Object(fields) = &input else {
            panic!("native record kind");
        };
        assert_eq!(
            fields.len(),
            if i == 0 { 12 } else { 13 },
            "native independent root fields PolymarketAPIOrder/{i}"
        );
        let decoded =
            PolymarketAPIOrder::decode(&input).expect("native decoded PolymarketAPIOrder");
        assert_eq!(
            decoded.id.0,
            expected_text("PolymarketAPIOrder.id", i),
            "native text PolymarketAPIOrder.id/{i}"
        );
        assert_eq!(
            decoded.owner.0,
            expected_text("PolymarketAPIOrder.owner", i),
            "native text PolymarketAPIOrder.owner/{i}"
        );
        assert_eq!(
            decoded.market.0,
            expected_text("PolymarketAPIOrder.market", i),
            "native text PolymarketAPIOrder.market/{i}"
        );
        assert_eq!(
            decoded.asset_id.0,
            expected_text("PolymarketAPIOrder.asset_id", i),
            "native text PolymarketAPIOrder.asset_id/{i}"
        );
        assert_eq!(
            decoded.side,
            [Side::Buy, Side::Sell][i % 2],
            "native literal PolymarketAPIOrder.side/{i}"
        );
        assert_eq!(
            decoded.original_size.0,
            expected_text("PolymarketAPIOrder.original_size", i),
            "native text PolymarketAPIOrder.original_size/{i}"
        );
        assert_eq!(
            decoded.size_matched.0,
            expected_text("PolymarketAPIOrder.size_matched", i),
            "native text PolymarketAPIOrder.size_matched/{i}"
        );
        assert_eq!(
            decoded.price.0,
            expected_text("PolymarketAPIOrder.price", i),
            "native text PolymarketAPIOrder.price/{i}"
        );
        assert_eq!(
            decoded.status,
            [
                Status::Live,
                Status::Matched,
                Status::Cancelled,
                Status::Delayed
            ][i % 4],
            "native literal PolymarketAPIOrder.status/{i}"
        );
        check_number(decoded.created_at, i, "PolymarketAPIOrder.created_at");
        check_number(decoded.expiration, i, "PolymarketAPIOrder.expiration");
        assert_eq!(
            decoded.order_type,
            [
                OrderType::Gtc,
                OrderType::Fok,
                OrderType::Gtd,
                OrderType::Fak
            ][i % 4],
            "native literal PolymarketAPIOrder.order_type/{i}"
        );
        match (&decoded.associate_trades, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
            (Optional::Value(value), 1 | 2) => {
                assert_eq!(
                    value.len(),
                    if i == 2 { 0 } else { 2 },
                    "native array count PolymarketAPIOrder.associate_trades/{i}"
                );
                if i != 2 {
                    assert_eq!(
                        value[0].id.0,
                        expected_text("PolymarketAPIOrder.associate_trades[0].id", i),
                        "native text PolymarketAPIOrder.associate_trades[0].id/{i}"
                    );
                    assert_eq!(
                        value[0].taker_order_id.0,
                        expected_text("PolymarketAPIOrder.associate_trades[0].taker_order_id", i),
                        "native text PolymarketAPIOrder.associate_trades[0].taker_order_id/{i}"
                    );
                    assert_eq!(
                        value[0].maker_order_id.0,
                        expected_text("PolymarketAPIOrder.associate_trades[0].maker_order_id", i),
                        "native text PolymarketAPIOrder.associate_trades[0].maker_order_id/{i}"
                    );
                    assert_eq!(
                        value[0].market.0,
                        expected_text("PolymarketAPIOrder.associate_trades[0].market", i),
                        "native text PolymarketAPIOrder.associate_trades[0].market/{i}"
                    );
                    assert_eq!(
                        value[0].asset_id.0,
                        expected_text("PolymarketAPIOrder.associate_trades[0].asset_id", i),
                        "native text PolymarketAPIOrder.associate_trades[0].asset_id/{i}"
                    );
                    assert_eq!(
                        value[0].side,
                        [Side::Buy, Side::Sell][i % 2],
                        "native literal PolymarketAPIOrder.associate_trades[0].side/{i}"
                    );
                    assert_eq!(
                        value[0].size.0,
                        expected_text("PolymarketAPIOrder.associate_trades[0].size", i),
                        "native text PolymarketAPIOrder.associate_trades[0].size/{i}"
                    );
                    assert_eq!(
                        value[0].price.0,
                        expected_text("PolymarketAPIOrder.associate_trades[0].price", i),
                        "native text PolymarketAPIOrder.associate_trades[0].price/{i}"
                    );
                    assert_eq!(
                        value[0].status.0,
                        expected_text("PolymarketAPIOrder.associate_trades[0].status", i),
                        "native text PolymarketAPIOrder.associate_trades[0].status/{i}"
                    );
                    check_number(
                        value[0].match_time,
                        i,
                        "PolymarketAPIOrder.associate_trades[0].match_time",
                    );
                    match (&value[0].transaction_hash, i) {
                        (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
                        (Optional::Value(value), 1 | 2) => {
                            assert_eq!(
                                value.0,
                                expected_text(
                                    "PolymarketAPIOrder.associate_trades[0].transaction_hash",
                                    i
                                ),
                                "native text PolymarketAPIOrder.associate_trades[0].transaction_hash/{i}"
                            );
                        }
                        _ => panic!(
                            "native optional presence PolymarketAPIOrder.associate_trades[0].transaction_hash/{i}"
                        ),
                    }
                    match (&value[0].fee_rate_bps, i) {
                        (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
                        (Optional::Value(value), 1 | 2) => {
                            assert_eq!(
                                value.0,
                                expected_text(
                                    "PolymarketAPIOrder.associate_trades[0].fee_rate_bps",
                                    i
                                ),
                                "native text PolymarketAPIOrder.associate_trades[0].fee_rate_bps/{i}"
                            );
                        }
                        _ => panic!(
                            "native optional presence PolymarketAPIOrder.associate_trades[0].fee_rate_bps/{i}"
                        ),
                    }
                    match (&value[0].bucket_index, i) {
                        (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
                        (Optional::Value(value), 1 | 2) => {
                            check_number(
                                *value,
                                i,
                                "PolymarketAPIOrder.associate_trades[0].bucket_index",
                            );
                        }
                        _ => panic!(
                            "native optional presence PolymarketAPIOrder.associate_trades[0].bucket_index/{i}"
                        ),
                    }
                    match (&value[0].maker_address, i) {
                        (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
                        (Optional::Value(value), 1 | 2) => {
                            assert_eq!(
                                value.0,
                                expected_text(
                                    "PolymarketAPIOrder.associate_trades[0].maker_address",
                                    i
                                ),
                                "native text PolymarketAPIOrder.associate_trades[0].maker_address/{i}"
                            );
                        }
                        _ => panic!(
                            "native optional presence PolymarketAPIOrder.associate_trades[0].maker_address/{i}"
                        ),
                    }
                    assert_eq!(
                        value[1].id.0,
                        expected_text("PolymarketAPIOrder.associate_trades[1].id", i),
                        "native text PolymarketAPIOrder.associate_trades[1].id/{i}"
                    );
                    assert_eq!(
                        value[1].taker_order_id.0,
                        expected_text("PolymarketAPIOrder.associate_trades[1].taker_order_id", i),
                        "native text PolymarketAPIOrder.associate_trades[1].taker_order_id/{i}"
                    );
                    assert_eq!(
                        value[1].maker_order_id.0,
                        expected_text("PolymarketAPIOrder.associate_trades[1].maker_order_id", i),
                        "native text PolymarketAPIOrder.associate_trades[1].maker_order_id/{i}"
                    );
                    assert_eq!(
                        value[1].market.0,
                        expected_text("PolymarketAPIOrder.associate_trades[1].market", i),
                        "native text PolymarketAPIOrder.associate_trades[1].market/{i}"
                    );
                    assert_eq!(
                        value[1].asset_id.0,
                        expected_text("PolymarketAPIOrder.associate_trades[1].asset_id", i),
                        "native text PolymarketAPIOrder.associate_trades[1].asset_id/{i}"
                    );
                    assert_eq!(
                        value[1].side,
                        [Side::Buy, Side::Sell][i % 2],
                        "native literal PolymarketAPIOrder.associate_trades[1].side/{i}"
                    );
                    assert_eq!(
                        value[1].size.0,
                        expected_text("PolymarketAPIOrder.associate_trades[1].size", i),
                        "native text PolymarketAPIOrder.associate_trades[1].size/{i}"
                    );
                    assert_eq!(
                        value[1].price.0,
                        expected_text("PolymarketAPIOrder.associate_trades[1].price", i),
                        "native text PolymarketAPIOrder.associate_trades[1].price/{i}"
                    );
                    assert_eq!(
                        value[1].status.0,
                        expected_text("PolymarketAPIOrder.associate_trades[1].status", i),
                        "native text PolymarketAPIOrder.associate_trades[1].status/{i}"
                    );
                    check_number(
                        value[1].match_time,
                        i,
                        "PolymarketAPIOrder.associate_trades[1].match_time",
                    );
                    match (&value[1].transaction_hash, i) {
                        (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
                        (Optional::Value(value), 1 | 2) => {
                            assert_eq!(
                                value.0,
                                expected_text(
                                    "PolymarketAPIOrder.associate_trades[1].transaction_hash",
                                    i
                                ),
                                "native text PolymarketAPIOrder.associate_trades[1].transaction_hash/{i}"
                            );
                        }
                        _ => panic!(
                            "native optional presence PolymarketAPIOrder.associate_trades[1].transaction_hash/{i}"
                        ),
                    }
                    match (&value[1].fee_rate_bps, i) {
                        (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
                        (Optional::Value(value), 1 | 2) => {
                            assert_eq!(
                                value.0,
                                expected_text(
                                    "PolymarketAPIOrder.associate_trades[1].fee_rate_bps",
                                    i
                                ),
                                "native text PolymarketAPIOrder.associate_trades[1].fee_rate_bps/{i}"
                            );
                        }
                        _ => panic!(
                            "native optional presence PolymarketAPIOrder.associate_trades[1].fee_rate_bps/{i}"
                        ),
                    }
                    match (&value[1].bucket_index, i) {
                        (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
                        (Optional::Value(value), 1 | 2) => {
                            check_number(
                                *value,
                                i,
                                "PolymarketAPIOrder.associate_trades[1].bucket_index",
                            );
                        }
                        _ => panic!(
                            "native optional presence PolymarketAPIOrder.associate_trades[1].bucket_index/{i}"
                        ),
                    }
                    match (&value[1].maker_address, i) {
                        (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
                        (Optional::Value(value), 1 | 2) => {
                            assert_eq!(
                                value.0,
                                expected_text(
                                    "PolymarketAPIOrder.associate_trades[1].maker_address",
                                    i
                                ),
                                "native text PolymarketAPIOrder.associate_trades[1].maker_address/{i}"
                            );
                        }
                        _ => panic!(
                            "native optional presence PolymarketAPIOrder.associate_trades[1].maker_address/{i}"
                        ),
                    }
                }
            }
            _ => panic!("native optional presence PolymarketAPIOrder.associate_trades/{i}"),
        }
    }
}

#[test]
fn trade_native() {
    for i in 0..4 {
        let input = row("PolymarketTrade", i);
        let NativeValue::Object(fields) = &input else {
            panic!("native record kind");
        };
        assert_eq!(
            fields.len(),
            if i == 0 { 10 } else { 14 },
            "native independent root fields PolymarketTrade/{i}"
        );
        let decoded = PolymarketTrade::decode(&input).expect("native decoded PolymarketTrade");
        assert_eq!(
            decoded.id.0,
            expected_text("PolymarketTrade.id", i),
            "native text PolymarketTrade.id/{i}"
        );
        assert_eq!(
            decoded.taker_order_id.0,
            expected_text("PolymarketTrade.taker_order_id", i),
            "native text PolymarketTrade.taker_order_id/{i}"
        );
        assert_eq!(
            decoded.maker_order_id.0,
            expected_text("PolymarketTrade.maker_order_id", i),
            "native text PolymarketTrade.maker_order_id/{i}"
        );
        assert_eq!(
            decoded.market.0,
            expected_text("PolymarketTrade.market", i),
            "native text PolymarketTrade.market/{i}"
        );
        assert_eq!(
            decoded.asset_id.0,
            expected_text("PolymarketTrade.asset_id", i),
            "native text PolymarketTrade.asset_id/{i}"
        );
        assert_eq!(
            decoded.side,
            [Side::Buy, Side::Sell][i % 2],
            "native literal PolymarketTrade.side/{i}"
        );
        assert_eq!(
            decoded.size.0,
            expected_text("PolymarketTrade.size", i),
            "native text PolymarketTrade.size/{i}"
        );
        assert_eq!(
            decoded.price.0,
            expected_text("PolymarketTrade.price", i),
            "native text PolymarketTrade.price/{i}"
        );
        assert_eq!(
            decoded.status.0,
            expected_text("PolymarketTrade.status", i),
            "native text PolymarketTrade.status/{i}"
        );
        check_number(decoded.match_time, i, "PolymarketTrade.match_time");
        match (&decoded.transaction_hash, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
            (Optional::Value(value), 1 | 2) => {
                assert_eq!(
                    value.0,
                    expected_text("PolymarketTrade.transaction_hash", i),
                    "native text PolymarketTrade.transaction_hash/{i}"
                );
            }
            _ => panic!("native optional presence PolymarketTrade.transaction_hash/{i}"),
        }
        match (&decoded.fee_rate_bps, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
            (Optional::Value(value), 1 | 2) => {
                assert_eq!(
                    value.0,
                    expected_text("PolymarketTrade.fee_rate_bps", i),
                    "native text PolymarketTrade.fee_rate_bps/{i}"
                );
            }
            _ => panic!("native optional presence PolymarketTrade.fee_rate_bps/{i}"),
        }
        match (&decoded.bucket_index, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
            (Optional::Value(value), 1 | 2) => {
                check_number(*value, i, "PolymarketTrade.bucket_index");
            }
            _ => panic!("native optional presence PolymarketTrade.bucket_index/{i}"),
        }
        match (&decoded.maker_address, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
            (Optional::Value(value), 1 | 2) => {
                assert_eq!(
                    value.0,
                    expected_text("PolymarketTrade.maker_address", i),
                    "native text PolymarketTrade.maker_address/{i}"
                );
            }
            _ => panic!("native optional presence PolymarketTrade.maker_address/{i}"),
        }
    }
}

#[test]
fn wssubscription_native() {
    for i in 0..4 {
        let input = row("PolymarketWSSubscription", i);
        let NativeValue::Object(fields) = &input else {
            panic!("native record kind");
        };
        assert_eq!(
            fields.len(),
            if i == 0 { 1 } else { 5 },
            "native independent root fields PolymarketWSSubscription/{i}"
        );
        let decoded = PolymarketWSSubscription::decode(&input)
            .expect("native decoded PolymarketWSSubscription");
        match (&decoded.auth, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
            (Optional::Value(value), 1 | 2) => {
                assert_eq!(
                    value.api_key.0,
                    expected_text("PolymarketWSSubscription.auth.apiKey", i),
                    "native text PolymarketWSSubscription.auth.apiKey/{i}"
                );
                assert_eq!(
                    value.secret.0,
                    expected_text("PolymarketWSSubscription.auth.secret", i),
                    "native text PolymarketWSSubscription.auth.secret/{i}"
                );
                assert_eq!(
                    value.passphrase.0,
                    expected_text("PolymarketWSSubscription.auth.passphrase", i),
                    "native text PolymarketWSSubscription.auth.passphrase/{i}"
                );
            }
            _ => panic!("native optional presence PolymarketWSSubscription.auth/{i}"),
        }
        match (&decoded.markets, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
            (Optional::Value(value), 1 | 2) => {
                assert_eq!(
                    value.len(),
                    if i == 2 { 0 } else { 2 },
                    "native array count PolymarketWSSubscription.markets/{i}"
                );
                if i != 2 {
                    assert_eq!(
                        value[0].0,
                        expected_text("PolymarketWSSubscription.markets[0]", i),
                        "native text PolymarketWSSubscription.markets[0]/{i}"
                    );
                    assert_eq!(
                        value[1].0,
                        expected_text("PolymarketWSSubscription.markets[1]", i),
                        "native text PolymarketWSSubscription.markets[1]/{i}"
                    );
                }
            }
            _ => panic!("native optional presence PolymarketWSSubscription.markets/{i}"),
        }
        match (&decoded.assets_ids, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
            (Optional::Value(value), 1 | 2) => {
                assert_eq!(
                    value.len(),
                    if i == 2 { 0 } else { 2 },
                    "native array count PolymarketWSSubscription.assets_ids/{i}"
                );
                if i != 2 {
                    assert_eq!(
                        value[0].0,
                        expected_text("PolymarketWSSubscription.assets_ids[0]", i),
                        "native text PolymarketWSSubscription.assets_ids[0]/{i}"
                    );
                    assert_eq!(
                        value[1].0,
                        expected_text("PolymarketWSSubscription.assets_ids[1]", i),
                        "native text PolymarketWSSubscription.assets_ids[1]/{i}"
                    );
                }
            }
            _ => panic!("native optional presence PolymarketWSSubscription.assets_ids/{i}"),
        }
        assert_eq!(
            decoded.r#type,
            [Channel::User, Channel::Market][i % 2],
            "native literal PolymarketWSSubscription.type/{i}"
        );
        match (&decoded.custom_feature_enabled, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
            (Optional::Value(value), 1 | 2) => {
                assert_eq!(
                    *value,
                    i % 2 == 1,
                    "native bool PolymarketWSSubscription.custom_feature_enabled/{i}"
                );
            }
            _ => panic!(
                "native optional presence PolymarketWSSubscription.custom_feature_enabled/{i}"
            ),
        }
    }
}

#[test]
fn wsoperation_native() {
    for i in 0..4 {
        let input = row("PolymarketWSOperation", i);
        let NativeValue::Object(fields) = &input else {
            panic!("native record kind");
        };
        assert_eq!(
            fields.len(),
            if i == 0 { 1 } else { 4 },
            "native independent root fields PolymarketWSOperation/{i}"
        );
        let decoded =
            PolymarketWSOperation::decode(&input).expect("native decoded PolymarketWSOperation");
        match (&decoded.assets_ids, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
            (Optional::Value(value), 1 | 2) => {
                assert_eq!(
                    value.len(),
                    if i == 2 { 0 } else { 2 },
                    "native array count PolymarketWSOperation.assets_ids/{i}"
                );
                if i != 2 {
                    assert_eq!(
                        value[0].0,
                        expected_text("PolymarketWSOperation.assets_ids[0]", i),
                        "native text PolymarketWSOperation.assets_ids[0]/{i}"
                    );
                    assert_eq!(
                        value[1].0,
                        expected_text("PolymarketWSOperation.assets_ids[1]", i),
                        "native text PolymarketWSOperation.assets_ids[1]/{i}"
                    );
                }
            }
            _ => panic!("native optional presence PolymarketWSOperation.assets_ids/{i}"),
        }
        match (&decoded.markets, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
            (Optional::Value(value), 1 | 2) => {
                assert_eq!(
                    value.len(),
                    if i == 2 { 0 } else { 2 },
                    "native array count PolymarketWSOperation.markets/{i}"
                );
                if i != 2 {
                    assert_eq!(
                        value[0].0,
                        expected_text("PolymarketWSOperation.markets[0]", i),
                        "native text PolymarketWSOperation.markets[0]/{i}"
                    );
                    assert_eq!(
                        value[1].0,
                        expected_text("PolymarketWSOperation.markets[1]", i),
                        "native text PolymarketWSOperation.markets[1]/{i}"
                    );
                }
            }
            _ => panic!("native optional presence PolymarketWSOperation.markets/{i}"),
        }
        assert_eq!(
            decoded.operation,
            [Operation::Subscribe, Operation::Unsubscribe][i % 2],
            "native literal PolymarketWSOperation.operation/{i}"
        );
        match (&decoded.custom_feature_enabled, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
            (Optional::Value(value), 1 | 2) => {
                assert_eq!(
                    *value,
                    i % 2 == 1,
                    "native bool PolymarketWSOperation.custom_feature_enabled/{i}"
                );
            }
            _ => {
                panic!("native optional presence PolymarketWSOperation.custom_feature_enabled/{i}")
            }
        }
    }
}

#[test]
fn wspricechange_native() {
    for i in 0..4 {
        let input = row("PolymarketWSPriceChange", i);
        let NativeValue::Object(fields) = &input else {
            panic!("native record kind");
        };
        assert_eq!(
            fields.len(),
            if i == 0 { 3 } else { 3 },
            "native independent root fields PolymarketWSPriceChange/{i}"
        );
        let decoded = PolymarketWSPriceChange::decode(&input)
            .expect("native decoded PolymarketWSPriceChange");
        assert_eq!(
            decoded.event_type,
            [PriceEvent::PriceChange][i % 1],
            "native literal PolymarketWSPriceChange.event_type/{i}"
        );
        assert_eq!(
            decoded.price_changes.len(),
            if i == 2 { 0 } else { 2 },
            "native array count PolymarketWSPriceChange.price_changes/{i}"
        );
        if i != 2 {
            assert_eq!(
                decoded.price_changes[0].asset_id.0,
                expected_text("PolymarketWSPriceChange.price_changes[0].asset_id", i),
                "native text PolymarketWSPriceChange.price_changes[0].asset_id/{i}"
            );
            assert_eq!(
                decoded.price_changes[0].price.0,
                expected_text("PolymarketWSPriceChange.price_changes[0].price", i),
                "native text PolymarketWSPriceChange.price_changes[0].price/{i}"
            );
            assert_eq!(
                decoded.price_changes[0].size.0,
                expected_text("PolymarketWSPriceChange.price_changes[0].size", i),
                "native text PolymarketWSPriceChange.price_changes[0].size/{i}"
            );
            assert_eq!(
                decoded.price_changes[0].side,
                [Side::Buy, Side::Sell][i % 2],
                "native literal PolymarketWSPriceChange.price_changes[0].side/{i}"
            );
            assert_eq!(
                decoded.price_changes[0].best_bid.0,
                expected_text("PolymarketWSPriceChange.price_changes[0].best_bid", i),
                "native text PolymarketWSPriceChange.price_changes[0].best_bid/{i}"
            );
            assert_eq!(
                decoded.price_changes[0].best_ask.0,
                expected_text("PolymarketWSPriceChange.price_changes[0].best_ask", i),
                "native text PolymarketWSPriceChange.price_changes[0].best_ask/{i}"
            );
            assert_eq!(
                decoded.price_changes[1].asset_id.0,
                expected_text("PolymarketWSPriceChange.price_changes[1].asset_id", i),
                "native text PolymarketWSPriceChange.price_changes[1].asset_id/{i}"
            );
            assert_eq!(
                decoded.price_changes[1].price.0,
                expected_text("PolymarketWSPriceChange.price_changes[1].price", i),
                "native text PolymarketWSPriceChange.price_changes[1].price/{i}"
            );
            assert_eq!(
                decoded.price_changes[1].size.0,
                expected_text("PolymarketWSPriceChange.price_changes[1].size", i),
                "native text PolymarketWSPriceChange.price_changes[1].size/{i}"
            );
            assert_eq!(
                decoded.price_changes[1].side,
                [Side::Buy, Side::Sell][i % 2],
                "native literal PolymarketWSPriceChange.price_changes[1].side/{i}"
            );
            assert_eq!(
                decoded.price_changes[1].best_bid.0,
                expected_text("PolymarketWSPriceChange.price_changes[1].best_bid", i),
                "native text PolymarketWSPriceChange.price_changes[1].best_bid/{i}"
            );
            assert_eq!(
                decoded.price_changes[1].best_ask.0,
                expected_text("PolymarketWSPriceChange.price_changes[1].best_ask", i),
                "native text PolymarketWSPriceChange.price_changes[1].best_ask/{i}"
            );
        }
        check_number(decoded.timestamp, i, "PolymarketWSPriceChange.timestamp");
    }
}

#[test]
fn wsorderbookupdate_native() {
    for i in 0..4 {
        let input = row("PolymarketWSOrderBookUpdate", i);
        let NativeValue::Object(fields) = &input else {
            panic!("native record kind");
        };
        assert_eq!(
            fields.len(),
            if i == 0 { 4 } else { 7 },
            "native independent root fields PolymarketWSOrderBookUpdate/{i}"
        );
        let decoded = PolymarketWSOrderBookUpdate::decode(&input)
            .expect("native decoded PolymarketWSOrderBookUpdate");
        assert_eq!(
            decoded.event_type,
            [BookEvent::Book][i % 1],
            "native literal PolymarketWSOrderBookUpdate.event_type/{i}"
        );
        assert_eq!(
            decoded.asset_id.0,
            expected_text("PolymarketWSOrderBookUpdate.asset_id", i),
            "native text PolymarketWSOrderBookUpdate.asset_id/{i}"
        );
        assert_eq!(
            decoded.market.0,
            expected_text("PolymarketWSOrderBookUpdate.market", i),
            "native text PolymarketWSOrderBookUpdate.market/{i}"
        );
        match (&decoded.bids, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
            (Optional::Value(value), 1 | 2) => {
                assert_eq!(
                    value.len(),
                    if i == 2 { 0 } else { 2 },
                    "native array count PolymarketWSOrderBookUpdate.bids/{i}"
                );
                if i != 2 {
                    assert_eq!(
                        value[0].price.0,
                        expected_text("PolymarketWSOrderBookUpdate.bids[0].price", i),
                        "native text PolymarketWSOrderBookUpdate.bids[0].price/{i}"
                    );
                    assert_eq!(
                        value[0].size.0,
                        expected_text("PolymarketWSOrderBookUpdate.bids[0].size", i),
                        "native text PolymarketWSOrderBookUpdate.bids[0].size/{i}"
                    );
                    assert_eq!(
                        value[1].price.0,
                        expected_text("PolymarketWSOrderBookUpdate.bids[1].price", i),
                        "native text PolymarketWSOrderBookUpdate.bids[1].price/{i}"
                    );
                    assert_eq!(
                        value[1].size.0,
                        expected_text("PolymarketWSOrderBookUpdate.bids[1].size", i),
                        "native text PolymarketWSOrderBookUpdate.bids[1].size/{i}"
                    );
                }
            }
            _ => panic!("native optional presence PolymarketWSOrderBookUpdate.bids/{i}"),
        }
        match (&decoded.asks, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
            (Optional::Value(value), 1 | 2) => {
                assert_eq!(
                    value.len(),
                    if i == 2 { 0 } else { 2 },
                    "native array count PolymarketWSOrderBookUpdate.asks/{i}"
                );
                if i != 2 {
                    assert_eq!(
                        value[0].price.0,
                        expected_text("PolymarketWSOrderBookUpdate.asks[0].price", i),
                        "native text PolymarketWSOrderBookUpdate.asks[0].price/{i}"
                    );
                    assert_eq!(
                        value[0].size.0,
                        expected_text("PolymarketWSOrderBookUpdate.asks[0].size", i),
                        "native text PolymarketWSOrderBookUpdate.asks[0].size/{i}"
                    );
                    assert_eq!(
                        value[1].price.0,
                        expected_text("PolymarketWSOrderBookUpdate.asks[1].price", i),
                        "native text PolymarketWSOrderBookUpdate.asks[1].price/{i}"
                    );
                    assert_eq!(
                        value[1].size.0,
                        expected_text("PolymarketWSOrderBookUpdate.asks[1].size", i),
                        "native text PolymarketWSOrderBookUpdate.asks[1].size/{i}"
                    );
                }
            }
            _ => panic!("native optional presence PolymarketWSOrderBookUpdate.asks/{i}"),
        }
        check_number(
            decoded.timestamp,
            i,
            "PolymarketWSOrderBookUpdate.timestamp",
        );
        match (&decoded.hash, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
            (Optional::Value(value), 1 | 2) => {
                assert_eq!(
                    value.0,
                    expected_text("PolymarketWSOrderBookUpdate.hash", i),
                    "native text PolymarketWSOrderBookUpdate.hash/{i}"
                );
            }
            _ => panic!("native optional presence PolymarketWSOrderBookUpdate.hash/{i}"),
        }
    }
}

#[test]
fn wstrade_native() {
    for i in 0..4 {
        let input = row("PolymarketWSTrade", i);
        let NativeValue::Object(fields) = &input else {
            panic!("native record kind");
        };
        assert_eq!(
            fields.len(),
            if i == 0 { 11 } else { 15 },
            "native independent root fields PolymarketWSTrade/{i}"
        );
        let decoded = PolymarketWSTrade::decode(&input).expect("native decoded PolymarketWSTrade");
        assert_eq!(
            decoded.trade.id.0,
            expected_text("PolymarketWSTrade.id", i),
            "native text PolymarketWSTrade.id/{i}"
        );
        assert_eq!(
            decoded.trade.taker_order_id.0,
            expected_text("PolymarketWSTrade.taker_order_id", i),
            "native text PolymarketWSTrade.taker_order_id/{i}"
        );
        assert_eq!(
            decoded.trade.maker_order_id.0,
            expected_text("PolymarketWSTrade.maker_order_id", i),
            "native text PolymarketWSTrade.maker_order_id/{i}"
        );
        assert_eq!(
            decoded.trade.market.0,
            expected_text("PolymarketWSTrade.market", i),
            "native text PolymarketWSTrade.market/{i}"
        );
        assert_eq!(
            decoded.trade.asset_id.0,
            expected_text("PolymarketWSTrade.asset_id", i),
            "native text PolymarketWSTrade.asset_id/{i}"
        );
        assert_eq!(
            decoded.trade.side,
            [Side::Buy, Side::Sell][i % 2],
            "native literal PolymarketWSTrade.side/{i}"
        );
        assert_eq!(
            decoded.trade.size.0,
            expected_text("PolymarketWSTrade.size", i),
            "native text PolymarketWSTrade.size/{i}"
        );
        assert_eq!(
            decoded.trade.price.0,
            expected_text("PolymarketWSTrade.price", i),
            "native text PolymarketWSTrade.price/{i}"
        );
        assert_eq!(
            decoded.trade.status.0,
            expected_text("PolymarketWSTrade.status", i),
            "native text PolymarketWSTrade.status/{i}"
        );
        check_number(decoded.trade.match_time, i, "PolymarketWSTrade.match_time");
        match (&decoded.trade.transaction_hash, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
            (Optional::Value(value), 1 | 2) => {
                assert_eq!(
                    value.0,
                    expected_text("PolymarketWSTrade.transaction_hash", i),
                    "native text PolymarketWSTrade.transaction_hash/{i}"
                );
            }
            _ => panic!("native optional presence PolymarketWSTrade.transaction_hash/{i}"),
        }
        match (&decoded.trade.fee_rate_bps, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
            (Optional::Value(value), 1 | 2) => {
                assert_eq!(
                    value.0,
                    expected_text("PolymarketWSTrade.fee_rate_bps", i),
                    "native text PolymarketWSTrade.fee_rate_bps/{i}"
                );
            }
            _ => panic!("native optional presence PolymarketWSTrade.fee_rate_bps/{i}"),
        }
        match (&decoded.trade.bucket_index, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
            (Optional::Value(value), 1 | 2) => {
                check_number(*value, i, "PolymarketWSTrade.bucket_index");
            }
            _ => panic!("native optional presence PolymarketWSTrade.bucket_index/{i}"),
        }
        match (&decoded.trade.maker_address, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
            (Optional::Value(value), 1 | 2) => {
                assert_eq!(
                    value.0,
                    expected_text("PolymarketWSTrade.maker_address", i),
                    "native text PolymarketWSTrade.maker_address/{i}"
                );
            }
            _ => panic!("native optional presence PolymarketWSTrade.maker_address/{i}"),
        }
        assert_eq!(
            decoded.event_type,
            [TradeEvent::Trade][i % 1],
            "native literal PolymarketWSTrade.event_type/{i}"
        );
    }
}

#[test]
fn wsorderupdate_native() {
    for i in 0..4 {
        let input = row("PolymarketWSOrderUpdate", i);
        let NativeValue::Object(fields) = &input else {
            panic!("native record kind");
        };
        assert_eq!(
            fields.len(),
            if i == 0 { 13 } else { 14 },
            "native independent root fields PolymarketWSOrderUpdate/{i}"
        );
        let decoded = PolymarketWSOrderUpdate::decode(&input)
            .expect("native decoded PolymarketWSOrderUpdate");
        assert_eq!(
            decoded.order.id.0,
            expected_text("PolymarketWSOrderUpdate.id", i),
            "native text PolymarketWSOrderUpdate.id/{i}"
        );
        assert_eq!(
            decoded.order.owner.0,
            expected_text("PolymarketWSOrderUpdate.owner", i),
            "native text PolymarketWSOrderUpdate.owner/{i}"
        );
        assert_eq!(
            decoded.order.market.0,
            expected_text("PolymarketWSOrderUpdate.market", i),
            "native text PolymarketWSOrderUpdate.market/{i}"
        );
        assert_eq!(
            decoded.order.asset_id.0,
            expected_text("PolymarketWSOrderUpdate.asset_id", i),
            "native text PolymarketWSOrderUpdate.asset_id/{i}"
        );
        assert_eq!(
            decoded.order.side,
            [Side::Buy, Side::Sell][i % 2],
            "native literal PolymarketWSOrderUpdate.side/{i}"
        );
        assert_eq!(
            decoded.order.original_size.0,
            expected_text("PolymarketWSOrderUpdate.original_size", i),
            "native text PolymarketWSOrderUpdate.original_size/{i}"
        );
        assert_eq!(
            decoded.order.size_matched.0,
            expected_text("PolymarketWSOrderUpdate.size_matched", i),
            "native text PolymarketWSOrderUpdate.size_matched/{i}"
        );
        assert_eq!(
            decoded.order.price.0,
            expected_text("PolymarketWSOrderUpdate.price", i),
            "native text PolymarketWSOrderUpdate.price/{i}"
        );
        assert_eq!(
            decoded.order.status,
            [
                Status::Live,
                Status::Matched,
                Status::Cancelled,
                Status::Delayed
            ][i % 4],
            "native literal PolymarketWSOrderUpdate.status/{i}"
        );
        check_number(
            decoded.order.created_at,
            i,
            "PolymarketWSOrderUpdate.created_at",
        );
        check_number(
            decoded.order.expiration,
            i,
            "PolymarketWSOrderUpdate.expiration",
        );
        assert_eq!(
            decoded.order.order_type,
            [
                OrderType::Gtc,
                OrderType::Fok,
                OrderType::Gtd,
                OrderType::Fak
            ][i % 4],
            "native literal PolymarketWSOrderUpdate.order_type/{i}"
        );
        match (&decoded.order.associate_trades, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
            (Optional::Value(value), 1 | 2) => {
                assert_eq!(
                    value.len(),
                    if i == 2 { 0 } else { 2 },
                    "native array count PolymarketWSOrderUpdate.associate_trades/{i}"
                );
                if i != 2 {
                    assert_eq!(
                        value[0].id.0,
                        expected_text("PolymarketWSOrderUpdate.associate_trades[0].id", i),
                        "native text PolymarketWSOrderUpdate.associate_trades[0].id/{i}"
                    );
                    assert_eq!(
                        value[0].taker_order_id.0,
                        expected_text(
                            "PolymarketWSOrderUpdate.associate_trades[0].taker_order_id",
                            i
                        ),
                        "native text PolymarketWSOrderUpdate.associate_trades[0].taker_order_id/{i}"
                    );
                    assert_eq!(
                        value[0].maker_order_id.0,
                        expected_text(
                            "PolymarketWSOrderUpdate.associate_trades[0].maker_order_id",
                            i
                        ),
                        "native text PolymarketWSOrderUpdate.associate_trades[0].maker_order_id/{i}"
                    );
                    assert_eq!(
                        value[0].market.0,
                        expected_text("PolymarketWSOrderUpdate.associate_trades[0].market", i),
                        "native text PolymarketWSOrderUpdate.associate_trades[0].market/{i}"
                    );
                    assert_eq!(
                        value[0].asset_id.0,
                        expected_text("PolymarketWSOrderUpdate.associate_trades[0].asset_id", i),
                        "native text PolymarketWSOrderUpdate.associate_trades[0].asset_id/{i}"
                    );
                    assert_eq!(
                        value[0].side,
                        [Side::Buy, Side::Sell][i % 2],
                        "native literal PolymarketWSOrderUpdate.associate_trades[0].side/{i}"
                    );
                    assert_eq!(
                        value[0].size.0,
                        expected_text("PolymarketWSOrderUpdate.associate_trades[0].size", i),
                        "native text PolymarketWSOrderUpdate.associate_trades[0].size/{i}"
                    );
                    assert_eq!(
                        value[0].price.0,
                        expected_text("PolymarketWSOrderUpdate.associate_trades[0].price", i),
                        "native text PolymarketWSOrderUpdate.associate_trades[0].price/{i}"
                    );
                    assert_eq!(
                        value[0].status.0,
                        expected_text("PolymarketWSOrderUpdate.associate_trades[0].status", i),
                        "native text PolymarketWSOrderUpdate.associate_trades[0].status/{i}"
                    );
                    check_number(
                        value[0].match_time,
                        i,
                        "PolymarketWSOrderUpdate.associate_trades[0].match_time",
                    );
                    match (&value[0].transaction_hash, i) {
                        (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
                        (Optional::Value(value), 1 | 2) => {
                            assert_eq!(
                                value.0,
                                expected_text(
                                    "PolymarketWSOrderUpdate.associate_trades[0].transaction_hash",
                                    i
                                ),
                                "native text PolymarketWSOrderUpdate.associate_trades[0].transaction_hash/{i}"
                            );
                        }
                        _ => panic!(
                            "native optional presence PolymarketWSOrderUpdate.associate_trades[0].transaction_hash/{i}"
                        ),
                    }
                    match (&value[0].fee_rate_bps, i) {
                        (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
                        (Optional::Value(value), 1 | 2) => {
                            assert_eq!(
                                value.0,
                                expected_text(
                                    "PolymarketWSOrderUpdate.associate_trades[0].fee_rate_bps",
                                    i
                                ),
                                "native text PolymarketWSOrderUpdate.associate_trades[0].fee_rate_bps/{i}"
                            );
                        }
                        _ => panic!(
                            "native optional presence PolymarketWSOrderUpdate.associate_trades[0].fee_rate_bps/{i}"
                        ),
                    }
                    match (&value[0].bucket_index, i) {
                        (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
                        (Optional::Value(value), 1 | 2) => {
                            check_number(
                                *value,
                                i,
                                "PolymarketWSOrderUpdate.associate_trades[0].bucket_index",
                            );
                        }
                        _ => panic!(
                            "native optional presence PolymarketWSOrderUpdate.associate_trades[0].bucket_index/{i}"
                        ),
                    }
                    match (&value[0].maker_address, i) {
                        (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
                        (Optional::Value(value), 1 | 2) => {
                            assert_eq!(
                                value.0,
                                expected_text(
                                    "PolymarketWSOrderUpdate.associate_trades[0].maker_address",
                                    i
                                ),
                                "native text PolymarketWSOrderUpdate.associate_trades[0].maker_address/{i}"
                            );
                        }
                        _ => panic!(
                            "native optional presence PolymarketWSOrderUpdate.associate_trades[0].maker_address/{i}"
                        ),
                    }
                    assert_eq!(
                        value[1].id.0,
                        expected_text("PolymarketWSOrderUpdate.associate_trades[1].id", i),
                        "native text PolymarketWSOrderUpdate.associate_trades[1].id/{i}"
                    );
                    assert_eq!(
                        value[1].taker_order_id.0,
                        expected_text(
                            "PolymarketWSOrderUpdate.associate_trades[1].taker_order_id",
                            i
                        ),
                        "native text PolymarketWSOrderUpdate.associate_trades[1].taker_order_id/{i}"
                    );
                    assert_eq!(
                        value[1].maker_order_id.0,
                        expected_text(
                            "PolymarketWSOrderUpdate.associate_trades[1].maker_order_id",
                            i
                        ),
                        "native text PolymarketWSOrderUpdate.associate_trades[1].maker_order_id/{i}"
                    );
                    assert_eq!(
                        value[1].market.0,
                        expected_text("PolymarketWSOrderUpdate.associate_trades[1].market", i),
                        "native text PolymarketWSOrderUpdate.associate_trades[1].market/{i}"
                    );
                    assert_eq!(
                        value[1].asset_id.0,
                        expected_text("PolymarketWSOrderUpdate.associate_trades[1].asset_id", i),
                        "native text PolymarketWSOrderUpdate.associate_trades[1].asset_id/{i}"
                    );
                    assert_eq!(
                        value[1].side,
                        [Side::Buy, Side::Sell][i % 2],
                        "native literal PolymarketWSOrderUpdate.associate_trades[1].side/{i}"
                    );
                    assert_eq!(
                        value[1].size.0,
                        expected_text("PolymarketWSOrderUpdate.associate_trades[1].size", i),
                        "native text PolymarketWSOrderUpdate.associate_trades[1].size/{i}"
                    );
                    assert_eq!(
                        value[1].price.0,
                        expected_text("PolymarketWSOrderUpdate.associate_trades[1].price", i),
                        "native text PolymarketWSOrderUpdate.associate_trades[1].price/{i}"
                    );
                    assert_eq!(
                        value[1].status.0,
                        expected_text("PolymarketWSOrderUpdate.associate_trades[1].status", i),
                        "native text PolymarketWSOrderUpdate.associate_trades[1].status/{i}"
                    );
                    check_number(
                        value[1].match_time,
                        i,
                        "PolymarketWSOrderUpdate.associate_trades[1].match_time",
                    );
                    match (&value[1].transaction_hash, i) {
                        (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
                        (Optional::Value(value), 1 | 2) => {
                            assert_eq!(
                                value.0,
                                expected_text(
                                    "PolymarketWSOrderUpdate.associate_trades[1].transaction_hash",
                                    i
                                ),
                                "native text PolymarketWSOrderUpdate.associate_trades[1].transaction_hash/{i}"
                            );
                        }
                        _ => panic!(
                            "native optional presence PolymarketWSOrderUpdate.associate_trades[1].transaction_hash/{i}"
                        ),
                    }
                    match (&value[1].fee_rate_bps, i) {
                        (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
                        (Optional::Value(value), 1 | 2) => {
                            assert_eq!(
                                value.0,
                                expected_text(
                                    "PolymarketWSOrderUpdate.associate_trades[1].fee_rate_bps",
                                    i
                                ),
                                "native text PolymarketWSOrderUpdate.associate_trades[1].fee_rate_bps/{i}"
                            );
                        }
                        _ => panic!(
                            "native optional presence PolymarketWSOrderUpdate.associate_trades[1].fee_rate_bps/{i}"
                        ),
                    }
                    match (&value[1].bucket_index, i) {
                        (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
                        (Optional::Value(value), 1 | 2) => {
                            check_number(
                                *value,
                                i,
                                "PolymarketWSOrderUpdate.associate_trades[1].bucket_index",
                            );
                        }
                        _ => panic!(
                            "native optional presence PolymarketWSOrderUpdate.associate_trades[1].bucket_index/{i}"
                        ),
                    }
                    match (&value[1].maker_address, i) {
                        (Optional::Absent, 0) | (Optional::Undefined, 3) => {}
                        (Optional::Value(value), 1 | 2) => {
                            assert_eq!(
                                value.0,
                                expected_text(
                                    "PolymarketWSOrderUpdate.associate_trades[1].maker_address",
                                    i
                                ),
                                "native text PolymarketWSOrderUpdate.associate_trades[1].maker_address/{i}"
                            );
                        }
                        _ => panic!(
                            "native optional presence PolymarketWSOrderUpdate.associate_trades[1].maker_address/{i}"
                        ),
                    }
                }
            }
            _ => panic!("native optional presence PolymarketWSOrderUpdate.associate_trades/{i}"),
        }
        assert_eq!(
            decoded.event_type,
            [OrderEvent::Order][i % 1],
            "native literal PolymarketWSOrderUpdate.event_type/{i}"
        );
    }
}

#[test]
fn optional_null_and_wrong_element_refusals() {
    for name in ["PolymarketWSSubscription", "PolymarketWSOperation"] {
        let mut v = row(name, 1);
        let NativeValue::Object(fields) = &mut v else {
            unreachable!()
        };
        let (_, assets) = fields
            .iter_mut()
            .find(|(key, _)| key == "assets_ids")
            .unwrap();
        *assets = NativeValue::Null;
        let e = if name == "PolymarketWSSubscription" {
            PolymarketWSSubscription::decode(&v).unwrap_err()
        } else {
            PolymarketWSOperation::decode(&v).unwrap_err()
        };
        assert_eq!(
            e.field, "assets_ids",
            "native optional present null refusal"
        );
        assert_eq!(e.expected, "array", "native optional null expected type");
        let NativeValue::Object(fields) = &mut v else {
            unreachable!()
        };
        let (_, assets) = fields
            .iter_mut()
            .find(|(key, _)| key == "assets_ids")
            .unwrap();
        *assets = NativeValue::Array(vec![NativeValue::Text(vec![]), NativeValue::Undefined]);
        let e = if name == "PolymarketWSSubscription" {
            PolymarketWSSubscription::decode(&v).unwrap_err()
        } else {
            PolymarketWSOperation::decode(&v).unwrap_err()
        };
        assert_eq!(
            e.field, "assets_ids.1",
            "native array undefined element refusal"
        );
    }
}
#[test]
fn literal_and_required_field_refusals() {
    let mut v = row("PolymarketWSPriceChange", 0);
    let NativeValue::Object(fields) = &mut v else {
        unreachable!()
    };
    fields[0].1 = NativeValue::Text("book".encode_utf16().collect());
    let e = PolymarketWSPriceChange::decode(&v).unwrap_err();
    assert_eq!(e.field, "event_type", "native event literal refusal");
    assert_eq!(
        e.expected, "declared string literal",
        "native event expected type"
    );
    let mut v = row("PolymarketTrade", 0);
    let NativeValue::Object(fields) = &mut v else {
        unreachable!()
    };
    fields.retain(|(key, _)| key != "maker_order_id");
    let e = PolymarketTrade::decode(&v).unwrap_err();
    assert_eq!(
        e.field, "maker_order_id",
        "native required omission refusal"
    );
    assert_eq!(e.expected, "string", "native required expected type");
}
