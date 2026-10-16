// Per-case expectation tables. `if i == 0 { n } else { n }` and `[V][i % 1]`
// keep each corpus case's expectation INDEPENDENT even where the values
// currently coincide - the sibling line `if i == 2 { 0 } else { 2 }` is the same
// shape with arms that differ. Collapsing the equal ones to a single constant
// merges two independent expectations, which is the homogenized-corpus false
// green ZIP_CARDINALITY_AUDIT.md exists to prevent.
#![allow(clippy::if_same_then_else, clippy::modulo_one)]
use safe_cex_venue_polymarket::{
    market_records::*,
    ws_records::{Decode, NativeValue, Optional, OrderType, Side},
};
use serde_json::Value;
use std::{path::PathBuf, process::Command, sync::OnceLock};
fn oracle() -> &'static Value {
    static RESULT: OnceLock<Value> = OnceLock::new();
    RESULT.get_or_init(|| {
        let output = Command::new("node")
            .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/market_records_oracle.cjs"))
            .output()
            .expect("source oracle start");
        assert!(
            output.status.success(),
            "source market record oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result: Value =
            serde_json::from_slice(&output.stdout).expect("source record corpus JSON");
        assert_eq!(
            result["cases"].as_array().unwrap().len(),
            54,
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
    let mut v = format!("{role}/{i}/").encode_utf16().collect::<Vec<_>>();
    v.extend(match i {
        0 => "plain".encode_utf16().collect::<Vec<_>>(),
        1 => vec![0xd83e, 0xdd80, 0xd800],
        3 => vec![0xdfff],
        4 => vec![120, 0, 121],
        5 => vec![32, 9],
        _ => panic!("fixture index"),
    });
    v
}
fn expected_flag(role: &str, i: usize) -> bool {
    let key = role.rsplit('.').next().unwrap();
    let pattern = match key {
        "active" | "redeemable" | "negRisk" => 0b010101,
        "closed" | "mergeable" => 0b001100,
        "enableOrderBook" => 0b101010,
        "archived" | "negativeRisk" => 0b110010,
        "acceptingOrders" => 0b001010,
        "winner" => 0b101010,
        _ => panic!("independent boolean fixture role {key}"),
    };
    ((pattern >> i) & 1 != 0) != role.contains("[1]")
}
fn lane(role: &str) -> usize {
    usize::from(role.contains("[1]"))
}
fn expected_number(role: &str, i: usize) -> f64 {
    let seed = role
        .bytes()
        .fold(0_u32, |s, c| s.wrapping_mul(31).wrapping_add(u32::from(c)));
    [
        17.25 + f64::from(seed) / 8.0,
        -0.0,
        f64::NAN,
        f64::INFINITY,
        9007199254740992.0,
        -17.75,
    ][i]
}
fn check_number(actual: f64, i: usize, role: &str) {
    let expected = expected_number(role, i);
    if expected.is_nan() {
        assert!(actual.is_nan(), "native NaN {role}");
    } else {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "native Number bits {role}"
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
fn token_native() {
    for i in 0..6 {
        let input = row("PolymarketToken", i);
        let NativeValue::Object(fields) = &input else {
            panic!("native record kind");
        };
        assert_eq!(
            fields.len(),
            if i == 0 { 2 } else { 3 },
            "native independent root fields PolymarketToken/{i}"
        );
        let decoded = PolymarketToken::decode(&input).expect("native decoded PolymarketToken");
        assert_eq!(
            decoded.token_id.0,
            expected_text("PolymarketToken.token_id", i),
            "native text PolymarketToken.token_id/{i}"
        );
        assert_eq!(
            decoded.outcome,
            [Outcome::Yes, Outcome::No][(i + lane("PolymarketToken.outcome")) % 2],
            "native literal PolymarketToken.outcome/{i}"
        );
        match (&decoded.winner, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 2) => {}
            (Optional::Value(value), 1 | 3 | 4 | 5) => {
                assert_eq!(
                    *value,
                    expected_flag("PolymarketToken.winner", i),
                    "native bool PolymarketToken.winner/{i}"
                );
            }
            _ => panic!("native optional presence PolymarketToken.winner/{i}"),
        }
    }
}

#[test]
fn gamma_market_native() {
    for i in 0..6 {
        let input = row("PolymarketGammaMarket", i);
        let NativeValue::Object(fields) = &input else {
            panic!("native record kind");
        };
        assert_eq!(
            fields.len(),
            if i == 0 { 12 } else { 17 },
            "native independent root fields PolymarketGammaMarket/{i}"
        );
        let decoded =
            PolymarketGammaMarket::decode(&input).expect("native decoded PolymarketGammaMarket");
        assert_eq!(
            decoded.condition_id.0,
            expected_text("PolymarketGammaMarket.condition_id", i),
            "native text PolymarketGammaMarket.condition_id/{i}"
        );
        assert_eq!(
            decoded.question_id.0,
            expected_text("PolymarketGammaMarket.question_id", i),
            "native text PolymarketGammaMarket.question_id/{i}"
        );
        assert_eq!(
            decoded.tokens.len(),
            if i == 4 { 0 } else { 2 },
            "native array count PolymarketGammaMarket.tokens/{i}"
        );
        if i != 4 {
            assert_eq!(
                decoded.tokens[0].token_id.0,
                expected_text("PolymarketGammaMarket.tokens[0].token_id", i),
                "native text PolymarketGammaMarket.tokens[0].token_id/{i}"
            );
            assert_eq!(
                decoded.tokens[0].outcome,
                [Outcome::Yes, Outcome::No]
                    [(i + lane("PolymarketGammaMarket.tokens[0].outcome")) % 2],
                "native literal PolymarketGammaMarket.tokens[0].outcome/{i}"
            );
            match (&decoded.tokens[0].winner, i) {
                (Optional::Absent, 0) | (Optional::Undefined, 2) => {}
                (Optional::Value(value), 1 | 3 | 4 | 5) => {
                    assert_eq!(
                        *value,
                        expected_flag("PolymarketGammaMarket.tokens[0].winner", i),
                        "native bool PolymarketGammaMarket.tokens[0].winner/{i}"
                    );
                }
                _ => panic!("native optional presence PolymarketGammaMarket.tokens[0].winner/{i}"),
            }
            assert_eq!(
                decoded.tokens[1].token_id.0,
                expected_text("PolymarketGammaMarket.tokens[1].token_id", i),
                "native text PolymarketGammaMarket.tokens[1].token_id/{i}"
            );
            assert_eq!(
                decoded.tokens[1].outcome,
                [Outcome::Yes, Outcome::No]
                    [(i + lane("PolymarketGammaMarket.tokens[1].outcome")) % 2],
                "native literal PolymarketGammaMarket.tokens[1].outcome/{i}"
            );
            match (&decoded.tokens[1].winner, i) {
                (Optional::Absent, 0) | (Optional::Undefined, 2) => {}
                (Optional::Value(value), 1 | 3 | 4 | 5) => {
                    assert_eq!(
                        *value,
                        expected_flag("PolymarketGammaMarket.tokens[1].winner", i),
                        "native bool PolymarketGammaMarket.tokens[1].winner/{i}"
                    );
                }
                _ => panic!("native optional presence PolymarketGammaMarket.tokens[1].winner/{i}"),
            }
        }
        assert_eq!(
            decoded.question.0,
            expected_text("PolymarketGammaMarket.question", i),
            "native text PolymarketGammaMarket.question/{i}"
        );
        assert_eq!(
            decoded.description.0,
            expected_text("PolymarketGammaMarket.description", i),
            "native text PolymarketGammaMarket.description/{i}"
        );
        assert_eq!(
            decoded.market_slug.0,
            expected_text("PolymarketGammaMarket.market_slug", i),
            "native text PolymarketGammaMarket.market_slug/{i}"
        );
        assert_eq!(
            decoded.end_date_iso.0,
            expected_text("PolymarketGammaMarket.end_date_iso", i),
            "native text PolymarketGammaMarket.end_date_iso/{i}"
        );
        assert_eq!(
            decoded.active,
            expected_flag("PolymarketGammaMarket.active", i),
            "native bool PolymarketGammaMarket.active/{i}"
        );
        assert_eq!(
            decoded.closed,
            expected_flag("PolymarketGammaMarket.closed", i),
            "native bool PolymarketGammaMarket.closed/{i}"
        );
        assert_eq!(
            decoded.archived,
            expected_flag("PolymarketGammaMarket.archived", i),
            "native bool PolymarketGammaMarket.archived/{i}"
        );
        assert_eq!(
            decoded.minimum_order_size.0,
            expected_text("PolymarketGammaMarket.minimum_order_size", i),
            "native text PolymarketGammaMarket.minimum_order_size/{i}"
        );
        assert_eq!(
            decoded.minimum_tick_size.0,
            expected_text("PolymarketGammaMarket.minimum_tick_size", i),
            "native text PolymarketGammaMarket.minimum_tick_size/{i}"
        );
        match (&decoded.tags, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 2) => {}
            (Optional::Value(value), 1 | 3 | 4 | 5) => {
                assert_eq!(
                    value.len(),
                    if i == 4 { 0 } else { 2 },
                    "native array count PolymarketGammaMarket.tags/{i}"
                );
                if i != 4 {
                    assert_eq!(
                        value[0].0,
                        expected_text("PolymarketGammaMarket.tags[0]", i),
                        "native text PolymarketGammaMarket.tags[0]/{i}"
                    );
                    assert_eq!(
                        value[1].0,
                        expected_text("PolymarketGammaMarket.tags[1]", i),
                        "native text PolymarketGammaMarket.tags[1]/{i}"
                    );
                }
            }
            _ => panic!("native optional presence PolymarketGammaMarket.tags/{i}"),
        }
        match (&decoded.volume, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 2) => {}
            (Optional::Value(value), 1 | 3 | 4 | 5) => {
                assert_eq!(
                    value.0,
                    expected_text("PolymarketGammaMarket.volume", i),
                    "native text PolymarketGammaMarket.volume/{i}"
                );
            }
            _ => panic!("native optional presence PolymarketGammaMarket.volume/{i}"),
        }
        match (&decoded.volume_24h, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 2) => {}
            (Optional::Value(value), 1 | 3 | 4 | 5) => {
                assert_eq!(
                    value.0,
                    expected_text("PolymarketGammaMarket.volume_24h", i),
                    "native text PolymarketGammaMarket.volume_24h/{i}"
                );
            }
            _ => panic!("native optional presence PolymarketGammaMarket.volume_24h/{i}"),
        }
        match (&decoded.liquidity, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 2) => {}
            (Optional::Value(value), 1 | 3 | 4 | 5) => {
                assert_eq!(
                    value.0,
                    expected_text("PolymarketGammaMarket.liquidity", i),
                    "native text PolymarketGammaMarket.liquidity/{i}"
                );
            }
            _ => panic!("native optional presence PolymarketGammaMarket.liquidity/{i}"),
        }
        match (&decoded.unique_traders, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 2) => {}
            (Optional::Value(value), 1 | 3 | 4 | 5) => {
                check_number(*value, i, "PolymarketGammaMarket.unique_traders");
            }
            _ => panic!("native optional presence PolymarketGammaMarket.unique_traders/{i}"),
        }
    }
}

#[test]
fn position_native() {
    for i in 0..6 {
        let input = row("PolymarketPosition", i);
        let NativeValue::Object(fields) = &input else {
            panic!("native record kind");
        };
        assert_eq!(
            fields.len(),
            if i == 0 { 5 } else { 9 },
            "native independent root fields PolymarketPosition/{i}"
        );
        let decoded =
            PolymarketPosition::decode(&input).expect("native decoded PolymarketPosition");
        assert_eq!(
            decoded.token_id.0,
            expected_text("PolymarketPosition.token_id", i),
            "native text PolymarketPosition.token_id/{i}"
        );
        assert_eq!(
            decoded.condition_id.0,
            expected_text("PolymarketPosition.condition_id", i),
            "native text PolymarketPosition.condition_id/{i}"
        );
        assert_eq!(
            decoded.outcome.0,
            expected_text("PolymarketPosition.outcome", i),
            "native text PolymarketPosition.outcome/{i}"
        );
        assert_eq!(
            decoded.size.0,
            expected_text("PolymarketPosition.size", i),
            "native text PolymarketPosition.size/{i}"
        );
        assert_eq!(
            decoded.avg_price.0,
            expected_text("PolymarketPosition.avg_price", i),
            "native text PolymarketPosition.avg_price/{i}"
        );
        match (&decoded.market_slug, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 2) => {}
            (Optional::Value(value), 1 | 3 | 4 | 5) => {
                assert_eq!(
                    value.0,
                    expected_text("PolymarketPosition.market_slug", i),
                    "native text PolymarketPosition.market_slug/{i}"
                );
            }
            _ => panic!("native optional presence PolymarketPosition.market_slug/{i}"),
        }
        match (&decoded.question, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 2) => {}
            (Optional::Value(value), 1 | 3 | 4 | 5) => {
                assert_eq!(
                    value.0,
                    expected_text("PolymarketPosition.question", i),
                    "native text PolymarketPosition.question/{i}"
                );
            }
            _ => panic!("native optional presence PolymarketPosition.question/{i}"),
        }
        match (&decoded.current_price, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 2) => {}
            (Optional::Value(value), 1 | 3 | 4 | 5) => {
                assert_eq!(
                    value.0,
                    expected_text("PolymarketPosition.current_price", i),
                    "native text PolymarketPosition.current_price/{i}"
                );
            }
            _ => panic!("native optional presence PolymarketPosition.current_price/{i}"),
        }
        match (&decoded.unrealized_pnl, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 2) => {}
            (Optional::Value(value), 1 | 3 | 4 | 5) => {
                assert_eq!(
                    value.0,
                    expected_text("PolymarketPosition.unrealized_pnl", i),
                    "native text PolymarketPosition.unrealized_pnl/{i}"
                );
            }
            _ => panic!("native optional presence PolymarketPosition.unrealized_pnl/{i}"),
        }
    }
}

#[test]
fn signed_order_native() {
    for i in 0..6 {
        let input = row("PolymarketSignedOrder", i);
        let NativeValue::Object(fields) = &input else {
            panic!("native record kind");
        };
        assert_eq!(
            fields.len(),
            if i == 0 { 13 } else { 13 },
            "native independent root fields PolymarketSignedOrder/{i}"
        );
        let decoded =
            PolymarketSignedOrder::decode(&input).expect("native decoded PolymarketSignedOrder");
        assert_eq!(
            decoded.salt.0,
            expected_text("PolymarketSignedOrder.salt", i),
            "native text PolymarketSignedOrder.salt/{i}"
        );
        assert_eq!(
            decoded.maker.0,
            expected_text("PolymarketSignedOrder.maker", i),
            "native text PolymarketSignedOrder.maker/{i}"
        );
        assert_eq!(
            decoded.signer.0,
            expected_text("PolymarketSignedOrder.signer", i),
            "native text PolymarketSignedOrder.signer/{i}"
        );
        assert_eq!(
            decoded.token_id.0,
            expected_text("PolymarketSignedOrder.tokenId", i),
            "native text PolymarketSignedOrder.tokenId/{i}"
        );
        assert_eq!(
            decoded.maker_amount.0,
            expected_text("PolymarketSignedOrder.makerAmount", i),
            "native text PolymarketSignedOrder.makerAmount/{i}"
        );
        assert_eq!(
            decoded.taker_amount.0,
            expected_text("PolymarketSignedOrder.takerAmount", i),
            "native text PolymarketSignedOrder.takerAmount/{i}"
        );
        assert_eq!(
            decoded.expiration.0,
            expected_text("PolymarketSignedOrder.expiration", i),
            "native text PolymarketSignedOrder.expiration/{i}"
        );
        assert_eq!(
            decoded.side,
            [Side::Buy, Side::Sell][(i + lane("PolymarketSignedOrder.side")) % 2],
            "native literal PolymarketSignedOrder.side/{i}"
        );
        check_number(
            decoded.signature_type,
            i,
            "PolymarketSignedOrder.signatureType",
        );
        assert_eq!(
            decoded.timestamp.0,
            expected_text("PolymarketSignedOrder.timestamp", i),
            "native text PolymarketSignedOrder.timestamp/{i}"
        );
        assert_eq!(
            decoded.metadata.0,
            expected_text("PolymarketSignedOrder.metadata", i),
            "native text PolymarketSignedOrder.metadata/{i}"
        );
        assert_eq!(
            decoded.builder.0,
            expected_text("PolymarketSignedOrder.builder", i),
            "native text PolymarketSignedOrder.builder/{i}"
        );
        assert_eq!(
            decoded.signature.0,
            expected_text("PolymarketSignedOrder.signature", i),
            "native text PolymarketSignedOrder.signature/{i}"
        );
    }
}

#[test]
fn create_order_request_native() {
    for i in 0..6 {
        let input = row("PolymarketCreateOrderRequest", i);
        let NativeValue::Object(fields) = &input else {
            panic!("native record kind");
        };
        assert_eq!(
            fields.len(),
            if i == 0 { 3 } else { 3 },
            "native independent root fields PolymarketCreateOrderRequest/{i}"
        );
        let decoded = PolymarketCreateOrderRequest::decode(&input)
            .expect("native decoded PolymarketCreateOrderRequest");
        assert_eq!(
            decoded.order.salt.0,
            expected_text("PolymarketCreateOrderRequest.order.salt", i),
            "native text PolymarketCreateOrderRequest.order.salt/{i}"
        );
        assert_eq!(
            decoded.order.maker.0,
            expected_text("PolymarketCreateOrderRequest.order.maker", i),
            "native text PolymarketCreateOrderRequest.order.maker/{i}"
        );
        assert_eq!(
            decoded.order.signer.0,
            expected_text("PolymarketCreateOrderRequest.order.signer", i),
            "native text PolymarketCreateOrderRequest.order.signer/{i}"
        );
        assert_eq!(
            decoded.order.token_id.0,
            expected_text("PolymarketCreateOrderRequest.order.tokenId", i),
            "native text PolymarketCreateOrderRequest.order.tokenId/{i}"
        );
        assert_eq!(
            decoded.order.maker_amount.0,
            expected_text("PolymarketCreateOrderRequest.order.makerAmount", i),
            "native text PolymarketCreateOrderRequest.order.makerAmount/{i}"
        );
        assert_eq!(
            decoded.order.taker_amount.0,
            expected_text("PolymarketCreateOrderRequest.order.takerAmount", i),
            "native text PolymarketCreateOrderRequest.order.takerAmount/{i}"
        );
        assert_eq!(
            decoded.order.expiration.0,
            expected_text("PolymarketCreateOrderRequest.order.expiration", i),
            "native text PolymarketCreateOrderRequest.order.expiration/{i}"
        );
        assert_eq!(
            decoded.order.side,
            [Side::Buy, Side::Sell][(i + lane("PolymarketCreateOrderRequest.order.side")) % 2],
            "native literal PolymarketCreateOrderRequest.order.side/{i}"
        );
        check_number(
            decoded.order.signature_type,
            i,
            "PolymarketCreateOrderRequest.order.signatureType",
        );
        assert_eq!(
            decoded.order.timestamp.0,
            expected_text("PolymarketCreateOrderRequest.order.timestamp", i),
            "native text PolymarketCreateOrderRequest.order.timestamp/{i}"
        );
        assert_eq!(
            decoded.order.metadata.0,
            expected_text("PolymarketCreateOrderRequest.order.metadata", i),
            "native text PolymarketCreateOrderRequest.order.metadata/{i}"
        );
        assert_eq!(
            decoded.order.builder.0,
            expected_text("PolymarketCreateOrderRequest.order.builder", i),
            "native text PolymarketCreateOrderRequest.order.builder/{i}"
        );
        assert_eq!(
            decoded.order.signature.0,
            expected_text("PolymarketCreateOrderRequest.order.signature", i),
            "native text PolymarketCreateOrderRequest.order.signature/{i}"
        );
        assert_eq!(
            decoded.owner.0,
            expected_text("PolymarketCreateOrderRequest.owner", i),
            "native text PolymarketCreateOrderRequest.owner/{i}"
        );
        assert_eq!(
            decoded.order_type,
            [
                OrderType::Gtc,
                OrderType::Fok,
                OrderType::Gtd,
                OrderType::Fak
            ][(i + lane("PolymarketCreateOrderRequest.orderType")) % 4],
            "native literal PolymarketCreateOrderRequest.orderType/{i}"
        );
    }
}

#[test]
fn cancel_request_native() {
    for i in 0..6 {
        let input = row("PolymarketCancelRequest", i);
        let NativeValue::Object(fields) = &input else {
            panic!("native record kind");
        };
        assert_eq!(
            fields.len(),
            if i == 0 { 0 } else { 3 },
            "native independent root fields PolymarketCancelRequest/{i}"
        );
        let decoded = PolymarketCancelRequest::decode(&input)
            .expect("native decoded PolymarketCancelRequest");
        match (&decoded.order_ids, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 2) => {}
            (Optional::Value(value), 1 | 3 | 4 | 5) => {
                assert_eq!(
                    value.len(),
                    if i == 4 { 0 } else { 2 },
                    "native array count PolymarketCancelRequest.orderIds/{i}"
                );
                if i != 4 {
                    assert_eq!(
                        value[0].0,
                        expected_text("PolymarketCancelRequest.orderIds[0]", i),
                        "native text PolymarketCancelRequest.orderIds[0]/{i}"
                    );
                    assert_eq!(
                        value[1].0,
                        expected_text("PolymarketCancelRequest.orderIds[1]", i),
                        "native text PolymarketCancelRequest.orderIds[1]/{i}"
                    );
                }
            }
            _ => panic!("native optional presence PolymarketCancelRequest.orderIds/{i}"),
        }
        match (&decoded.market, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 2) => {}
            (Optional::Value(value), 1 | 3 | 4 | 5) => {
                assert_eq!(
                    value.0,
                    expected_text("PolymarketCancelRequest.market", i),
                    "native text PolymarketCancelRequest.market/{i}"
                );
            }
            _ => panic!("native optional presence PolymarketCancelRequest.market/{i}"),
        }
        match (&decoded.asset_id, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 2) => {}
            (Optional::Value(value), 1 | 3 | 4 | 5) => {
                assert_eq!(
                    value.0,
                    expected_text("PolymarketCancelRequest.asset_id", i),
                    "native text PolymarketCancelRequest.asset_id/{i}"
                );
            }
            _ => panic!("native optional presence PolymarketCancelRequest.asset_id/{i}"),
        }
    }
}

#[test]
fn exchange_options_native() {
    for i in 0..6 {
        let input = row("PolymarketExchangeOptions", i);
        let NativeValue::Object(fields) = &input else {
            panic!("native record kind");
        };
        assert_eq!(
            fields.len(),
            if i == 0 { 0 } else { 5 },
            "native independent root fields PolymarketExchangeOptions/{i}"
        );
        let decoded = PolymarketExchangeOptions::decode(&input)
            .expect("native decoded PolymarketExchangeOptions");
        match (&decoded.api_key, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 2) => {}
            (Optional::Value(value), 1 | 3 | 4 | 5) => {
                assert_eq!(
                    value.0,
                    expected_text("PolymarketExchangeOptions.apiKey", i),
                    "native text PolymarketExchangeOptions.apiKey/{i}"
                );
            }
            _ => panic!("native optional presence PolymarketExchangeOptions.apiKey/{i}"),
        }
        match (&decoded.api_secret, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 2) => {}
            (Optional::Value(value), 1 | 3 | 4 | 5) => {
                assert_eq!(
                    value.0,
                    expected_text("PolymarketExchangeOptions.apiSecret", i),
                    "native text PolymarketExchangeOptions.apiSecret/{i}"
                );
            }
            _ => panic!("native optional presence PolymarketExchangeOptions.apiSecret/{i}"),
        }
        match (&decoded.api_passphrase, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 2) => {}
            (Optional::Value(value), 1 | 3 | 4 | 5) => {
                assert_eq!(
                    value.0,
                    expected_text("PolymarketExchangeOptions.apiPassphrase", i),
                    "native text PolymarketExchangeOptions.apiPassphrase/{i}"
                );
            }
            _ => panic!("native optional presence PolymarketExchangeOptions.apiPassphrase/{i}"),
        }
        match (&decoded.signer_type, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 2) => {}
            (Optional::Value(value), 1 | 3 | 4 | 5) => {
                check_number(*value, i, "PolymarketExchangeOptions.signerType");
            }
            _ => panic!("native optional presence PolymarketExchangeOptions.signerType/{i}"),
        }
        match (&decoded.funder_address, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 2) => {}
            (Optional::Value(value), 1 | 3 | 4 | 5) => {
                assert_eq!(
                    value.0,
                    expected_text("PolymarketExchangeOptions.funderAddress", i),
                    "native text PolymarketExchangeOptions.funderAddress/{i}"
                );
            }
            _ => panic!("native optional presence PolymarketExchangeOptions.funderAddress/{i}"),
        }
    }
}

#[test]
fn gamma_market_info_native() {
    for i in 0..6 {
        let input = row("GammaMarketInfo", i);
        let NativeValue::Object(fields) = &input else {
            panic!("native record kind");
        };
        assert_eq!(
            fields.len(),
            if i == 0 { 19 } else { 22 },
            "native independent root fields GammaMarketInfo/{i}"
        );
        let decoded = GammaMarketInfo::decode(&input).expect("native decoded GammaMarketInfo");
        assert_eq!(
            decoded.id.0,
            expected_text("GammaMarketInfo.id", i),
            "native text GammaMarketInfo.id/{i}"
        );
        assert_eq!(
            decoded.question.0,
            expected_text("GammaMarketInfo.question", i),
            "native text GammaMarketInfo.question/{i}"
        );
        assert_eq!(
            decoded.condition_id.0,
            expected_text("GammaMarketInfo.conditionId", i),
            "native text GammaMarketInfo.conditionId/{i}"
        );
        assert_eq!(
            decoded.slug.0,
            expected_text("GammaMarketInfo.slug", i),
            "native text GammaMarketInfo.slug/{i}"
        );
        assert_eq!(
            decoded.resolution_source.0,
            expected_text("GammaMarketInfo.resolutionSource", i),
            "native text GammaMarketInfo.resolutionSource/{i}"
        );
        assert_eq!(
            decoded.end_date.0,
            expected_text("GammaMarketInfo.endDate", i),
            "native text GammaMarketInfo.endDate/{i}"
        );
        assert_eq!(
            decoded.liquidity.0,
            expected_text("GammaMarketInfo.liquidity", i),
            "native text GammaMarketInfo.liquidity/{i}"
        );
        assert_eq!(
            decoded.start_date.0,
            expected_text("GammaMarketInfo.startDate", i),
            "native text GammaMarketInfo.startDate/{i}"
        );
        assert_eq!(
            decoded.fee.0,
            expected_text("GammaMarketInfo.fee", i),
            "native text GammaMarketInfo.fee/{i}"
        );
        assert_eq!(
            decoded.description.0,
            expected_text("GammaMarketInfo.description", i),
            "native text GammaMarketInfo.description/{i}"
        );
        match (&decoded.outcomes, i) {
            (Nullable::Null, 3) => {}
            (Nullable::Value(value), 0 | 1 | 2 | 4 | 5) => {
                assert_eq!(
                    value.0,
                    expected_text("GammaMarketInfo.outcomes", i),
                    "native text GammaMarketInfo.outcomes/{i}"
                );
            }
            _ => panic!("native nullable arm GammaMarketInfo.outcomes/{i}"),
        }
        match (&decoded.outcome_prices, i) {
            (Nullable::Null, 3) => {}
            (Nullable::Value(value), 0 | 1 | 2 | 4 | 5) => {
                assert_eq!(
                    value.0,
                    expected_text("GammaMarketInfo.outcomePrices", i),
                    "native text GammaMarketInfo.outcomePrices/{i}"
                );
            }
            _ => panic!("native nullable arm GammaMarketInfo.outcomePrices/{i}"),
        }
        assert_eq!(
            decoded.volume.0,
            expected_text("GammaMarketInfo.volume", i),
            "native text GammaMarketInfo.volume/{i}"
        );
        assert_eq!(
            decoded.active,
            expected_flag("GammaMarketInfo.active", i),
            "native bool GammaMarketInfo.active/{i}"
        );
        assert_eq!(
            decoded.closed,
            expected_flag("GammaMarketInfo.closed", i),
            "native bool GammaMarketInfo.closed/{i}"
        );
        assert_eq!(
            decoded.market_type.0,
            expected_text("GammaMarketInfo.marketType", i),
            "native text GammaMarketInfo.marketType/{i}"
        );
        match (&decoded.clob_token_ids, i) {
            (Nullable::Null, 3) => {}
            (Nullable::Value(value), 0 | 1 | 2 | 4 | 5) => {
                assert_eq!(
                    value.0,
                    expected_text("GammaMarketInfo.clobTokenIds", i),
                    "native text GammaMarketInfo.clobTokenIds/{i}"
                );
            }
            _ => panic!("native nullable arm GammaMarketInfo.clobTokenIds/{i}"),
        }
        match (&decoded.order_price_min_tick_size, i) {
            (Nullable::Null, 3) => {}
            (Nullable::Value(value), 0 | 1 | 2 | 4 | 5) => match (&value, i) {
                (NumberOrText::Text(value), 1 | 4) => {
                    assert_eq!(
                        value.0,
                        expected_text("GammaMarketInfo.orderPriceMinTickSize", i),
                        "native text GammaMarketInfo.orderPriceMinTickSize/{i}"
                    );
                }
                (NumberOrText::Number(value), 0 | 2 | 5) => {
                    check_number(*value, i, "GammaMarketInfo.orderPriceMinTickSize");
                }
                _ => panic!("native number/string arm GammaMarketInfo.orderPriceMinTickSize/{i}"),
            },
            _ => panic!("native nullable arm GammaMarketInfo.orderPriceMinTickSize/{i}"),
        }
        match (&decoded.order_min_size, i) {
            (Nullable::Null, 3) => {}
            (Nullable::Value(value), 0 | 1 | 2 | 4 | 5) => match (&value, i) {
                (NumberOrText::Text(value), 1 | 4) => {
                    assert_eq!(
                        value.0,
                        expected_text("GammaMarketInfo.orderMinSize", i),
                        "native text GammaMarketInfo.orderMinSize/{i}"
                    );
                }
                (NumberOrText::Number(value), 0 | 2 | 5) => {
                    check_number(*value, i, "GammaMarketInfo.orderMinSize");
                }
                _ => panic!("native number/string arm GammaMarketInfo.orderMinSize/{i}"),
            },
            _ => panic!("native nullable arm GammaMarketInfo.orderMinSize/{i}"),
        }
        match (&decoded.neg_risk, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 2) => {}
            (Optional::Value(value), 1 | 3 | 4 | 5) => match (&value, i) {
                (Nullable::Null, 3) => {}
                (Nullable::Value(value), 0 | 1 | 2 | 4 | 5) => {
                    assert_eq!(
                        *value,
                        expected_flag("GammaMarketInfo.negRisk", i),
                        "native bool GammaMarketInfo.negRisk/{i}"
                    );
                }
                _ => panic!("native nullable arm GammaMarketInfo.negRisk/{i}"),
            },
            _ => panic!("native optional presence GammaMarketInfo.negRisk/{i}"),
        }
        match (&decoded.enable_order_book, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 2) => {}
            (Optional::Value(value), 1 | 3 | 4 | 5) => match (&value, i) {
                (Nullable::Null, 3) => {}
                (Nullable::Value(value), 0 | 1 | 2 | 4 | 5) => {
                    assert_eq!(
                        *value,
                        expected_flag("GammaMarketInfo.enableOrderBook", i),
                        "native bool GammaMarketInfo.enableOrderBook/{i}"
                    );
                }
                _ => panic!("native nullable arm GammaMarketInfo.enableOrderBook/{i}"),
            },
            _ => panic!("native optional presence GammaMarketInfo.enableOrderBook/{i}"),
        }
        match (&decoded.accepting_orders, i) {
            (Optional::Absent, 0) | (Optional::Undefined, 2) => {}
            (Optional::Value(value), 1 | 3 | 4 | 5) => match (&value, i) {
                (Nullable::Null, 3) => {}
                (Nullable::Value(value), 0 | 1 | 2 | 4 | 5) => {
                    assert_eq!(
                        *value,
                        expected_flag("GammaMarketInfo.acceptingOrders", i),
                        "native bool GammaMarketInfo.acceptingOrders/{i}"
                    );
                }
                _ => panic!("native nullable arm GammaMarketInfo.acceptingOrders/{i}"),
            },
            _ => panic!("native optional presence GammaMarketInfo.acceptingOrders/{i}"),
        }
    }
}

#[test]
fn data_position_native() {
    for i in 0..6 {
        let input = row("PolymarketDataPosition", i);
        let NativeValue::Object(fields) = &input else {
            panic!("native record kind");
        };
        assert_eq!(
            fields.len(),
            if i == 0 { 14 } else { 14 },
            "native independent root fields PolymarketDataPosition/{i}"
        );
        let decoded =
            PolymarketDataPosition::decode(&input).expect("native decoded PolymarketDataPosition");
        assert_eq!(
            decoded.proxy_wallet.0,
            expected_text("PolymarketDataPosition.proxyWallet", i),
            "native text PolymarketDataPosition.proxyWallet/{i}"
        );
        assert_eq!(
            decoded.asset.0,
            expected_text("PolymarketDataPosition.asset", i),
            "native text PolymarketDataPosition.asset/{i}"
        );
        assert_eq!(
            decoded.condition_id.0,
            expected_text("PolymarketDataPosition.conditionId", i),
            "native text PolymarketDataPosition.conditionId/{i}"
        );
        check_number(decoded.size, i, "PolymarketDataPosition.size");
        check_number(decoded.avg_price, i, "PolymarketDataPosition.avgPrice");
        check_number(
            decoded.current_value,
            i,
            "PolymarketDataPosition.currentValue",
        );
        check_number(decoded.cash_pnl, i, "PolymarketDataPosition.cashPnl");
        check_number(decoded.cur_price, i, "PolymarketDataPosition.curPrice");
        assert_eq!(
            decoded.redeemable,
            expected_flag("PolymarketDataPosition.redeemable", i),
            "native bool PolymarketDataPosition.redeemable/{i}"
        );
        assert_eq!(
            decoded.mergeable,
            expected_flag("PolymarketDataPosition.mergeable", i),
            "native bool PolymarketDataPosition.mergeable/{i}"
        );
        assert_eq!(
            decoded.title.0,
            expected_text("PolymarketDataPosition.title", i),
            "native text PolymarketDataPosition.title/{i}"
        );
        assert_eq!(
            decoded.slug.0,
            expected_text("PolymarketDataPosition.slug", i),
            "native text PolymarketDataPosition.slug/{i}"
        );
        assert_eq!(
            decoded.outcome.0,
            expected_text("PolymarketDataPosition.outcome", i),
            "native text PolymarketDataPosition.outcome/{i}"
        );
        assert_eq!(
            decoded.negative_risk,
            expected_flag("PolymarketDataPosition.negativeRisk", i),
            "native bool PolymarketDataPosition.negativeRisk/{i}"
        );
    }
}

fn replace(v: &mut NativeValue, key: &str, value: NativeValue) {
    let NativeValue::Object(fields) = v else {
        panic!("fixture object")
    };
    fields.iter_mut().find(|(name, _)| name == key).unwrap().1 = value;
}
#[test]
fn declared_union_refusals() {
    let mut v = row("GammaMarketInfo", 1);
    replace(&mut v, "orderMinSize", NativeValue::Bool(false));
    let e = GammaMarketInfo::decode(&v).unwrap_err();
    assert_eq!(
        e.field, "orderMinSize",
        "native number/string/null boolean refusal"
    );
    assert_eq!(
        e.expected, "number or string",
        "native number/string/null expected type"
    );
    let mut v = row("PolymarketGammaMarket", 1);
    replace(&mut v, "volume", NativeValue::Null);
    let e = PolymarketGammaMarket::decode(&v).unwrap_err();
    assert_eq!(
        e.field, "volume",
        "native nonnullable optional null refusal"
    );
    let mut v = row("PolymarketToken", 0);
    replace(
        &mut v,
        "outcome",
        NativeValue::Text("yes".encode_utf16().collect()),
    );
    let e = PolymarketToken::decode(&v).unwrap_err();
    assert_eq!(e.field, "outcome", "native case sensitive outcome refusal");
}
#[test]
fn array_and_required_refusals() {
    let mut v = row("PolymarketGammaMarket", 1);
    replace(
        &mut v,
        "tokens",
        NativeValue::Array(vec![NativeValue::Undefined]),
    );
    let e = PolymarketGammaMarket::decode(&v).unwrap_err();
    assert_eq!(e.field, "tokens.0", "native undefined token refusal");
    let mut v = row("PolymarketSignedOrder", 1);
    replace(&mut v, "signatureType", NativeValue::Text(vec![]));
    let e = PolymarketSignedOrder::decode(&v).unwrap_err();
    assert_eq!(
        e.field, "signatureType",
        "native numeric enum string refusal"
    );
    assert_eq!(e.expected, "number", "native numeric enum expected type");
    let mut v = row("PolymarketDataPosition", 1);
    let NativeValue::Object(fields) = &mut v else {
        unreachable!()
    };
    fields.retain(|(name, _)| name != "curPrice");
    let e = PolymarketDataPosition::decode(&v).unwrap_err();
    assert_eq!(
        e.field, "curPrice",
        "native required current price omission refusal"
    );
}
