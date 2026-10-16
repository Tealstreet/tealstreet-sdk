use safe_cex_venue_hyperliquid::{spot_json::JsString, spot_models::*};
use serde::de::DeserializeOwned;
use serde_json::Value;

trait Native {
    fn native(&self) -> Value;
}
fn number(value: &f64) -> Value {
    serde_json::json!(format!("{:016x}", value.to_bits()))
}
impl Native for JsString {
    fn native(&self) -> Value {
        serde_json::json!(self.0)
    }
}
impl<T: Native> Native for Vec<T> {
    fn native(&self) -> Value {
        Value::Array(self.iter().map(Native::native).collect())
    }
}
include!("fixtures/spot-model-native.rs");
include!("fixtures/spot-model-dispatch.rs");

fn source() -> Value {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/spot-models-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "SPOTMODEL source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn check<T: DeserializeOwned + Native>(row: &Value) {
    let raw = row["input"].as_str().unwrap();
    let name = row["name"].as_str().unwrap();
    if row["accepted"].as_bool().unwrap() {
        let native = serde_json::from_str::<T>(raw)
            .unwrap_or_else(|error| panic!("SPOTMODEL native accepted {name}: {error}; {raw}"));
        assert_eq!(
            native.native(),
            row["native"],
            "SPOTMODEL native {name} from_str mode {}",
            row["mode"]
        );
        let native = serde_json::from_slice::<T>(raw.as_bytes())
            .unwrap_or_else(|error| panic!("SPOTMODEL native accepted slice {name}: {error}"));
        assert_eq!(
            native.native(),
            row["native"],
            "SPOTMODEL native {name} from_slice mode {}",
            row["mode"]
        );
    } else {
        assert!(
            serde_json::from_str::<T>(raw).is_err(),
            "SPOTMODEL native rejection {name}: {raw}"
        );
        assert!(
            serde_json::from_slice::<T>(raw.as_bytes()).is_err(),
            "SPOTMODEL native slice rejection {name}: {raw}"
        );
    }
}
#[test]
fn native_spot_models_match_actual_typed_source() {
    const ROOTS: [&str; 50] = [
        "Cloid",
        "OidOrCloid",
        "Meta",
        "ClearinghouseState",
        "UserFills",
        "OrderResponse",
        "WsTrade",
        "WsBook",
        "WsLevel",
        "WsOrder",
        "WsFill",
        "WsUserFunding",
        "WsLiquidation",
        "WsNonUserCancel",
        "SpotClearinghouseState",
        "FrontendOpenOrders",
        "L2Book",
        "CandleSnapshot",
        "AssetCtx",
        "MetaAndAssetCtxs",
        "UserFundingDelta",
        "UserFundingEntry",
        "UserFunding",
        "UserNonFundingLedgerDelta",
        "UserNonFundingLedgerEntry",
        "UserNonFundingLedgerUpdates",
        "FundingHistoryEntry",
        "FundingHistory",
        "SpotToken",
        "SpotMarket",
        "SpotMeta",
        "SpotAssetCtx",
        "SpotMetaAndAssetCtxs",
        "UserOpenOrder",
        "UserOpenOrders",
        "OrderRequest",
        "OrderWire",
        "CancelOrderRequest",
        "CancelByCloidRequest",
        "ModifyRequest",
        "ModifyWire",
        "Signature",
        "Notification",
        "Candle",
        "HyperliquidCandle",
        "WsUserFill",
        "WsUserFills",
        "WsUserFundings",
        "WsUserNonFundingLedgerUpdate",
        "WsUserNonFundingLedgerUpdates",
    ];
    let source = source();
    let cases = source["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 400, "SPOTMODEL independent source case floor");
    assert_eq!(
        source["iterationStop"], 399,
        "SPOTMODEL source iteration stop"
    );
    let actual_roots: Vec<_> = source["roots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    assert_eq!(
        actual_roots, ROOTS,
        "SPOTMODEL independent source root enumeration"
    );
    let witnesses = source["witnesses"].as_array().unwrap();
    assert_eq!(
        witnesses.len(),
        5,
        "SPOTMODEL independent extra witness floor"
    );
    let mut failures = Vec::new();
    let mut checked = 0;
    for name in ROOTS {
        let rows: Vec<_> = cases.iter().filter(|row| row["name"] == name).collect();
        assert_eq!(
            rows.len(),
            8,
            "SPOTMODEL independent per-root case floor {name}"
        );
        assert_eq!(
            rows.iter().filter(|row| row["accepted"] == true).count(),
            4,
            "SPOTMODEL independent accepted floor {name}"
        );
        checked += 1;
        if std::panic::catch_unwind(|| {
            for row in rows {
                dispatch(row);
            }
        })
        .is_err()
        {
            failures.push(name);
        }
    }
    assert_eq!(checked, 50, "SPOTMODEL native root iteration stop");
    // Native projections at each nested caller; these run after the fixed root floor.
    for row in witnesses {
        dispatch(row);
    }

    assert!(
        failures.is_empty(),
        "SPOTMODEL named native failures: {failures:?}"
    );
}

// Manual projections pin the two most easily swapped field pairs independently
// of the generator's field-name transformation and enum metadata.
#[test]
fn native_manual_string_number_and_tag_pairs() {
    let level: WsLevel = serde_json::from_str(r#"{"px":"\ud800X","sz":"\udfffY","n":-0}"#).unwrap();
    assert_eq!(
        level.px.0,
        vec![0xd800, 0x58],
        "SPOTMODEL manual WsLevel price units"
    );
    assert_eq!(
        level.sz.0,
        vec![0xdfff, 0x59],
        "SPOTMODEL manual WsLevel size units"
    );
    assert_eq!(
        level.n.to_bits(),
        (-0.0f64).to_bits(),
        "SPOTMODEL manual WsLevel count bits"
    );
    let order:OrderRequest=serde_json::from_str(r#"{"coin":"\ud800","is_buy":true,"sz":17.5,"limit_px":91.25,"order_type":{},"reduce_only":false}"#).unwrap();
    assert_eq!(
        order.coin.0,
        vec![0xd800],
        "SPOTMODEL manual OrderRequest coin units"
    );
    assert_eq!(
        order.sz.to_bits(),
        17.5f64.to_bits(),
        "SPOTMODEL manual OrderRequest size bits"
    );
    assert_eq!(
        order.limit_px.to_bits(),
        91.25f64.to_bits(),
        "SPOTMODEL manual OrderRequest price bits"
    );
    assert!(
        order.is_buy && !order.reduce_only,
        "SPOTMODEL manual OrderRequest flags"
    );
    let oid: OidOrCloid = serde_json::from_str("1e400").unwrap();
    match oid {
        OidOrCloid::Number(value) => assert_eq!(
            value.to_bits(),
            f64::INFINITY.to_bits(),
            "SPOTMODEL manual numeric union overflow"
        ),
        other => panic!("SPOTMODEL manual numeric union variant {other:?}"),
    }
    let cloid: OidOrCloid = serde_json::from_str(r#""\ud800""#).unwrap();
    match cloid {
        OidOrCloid::Cloid(value) => {
            assert_eq!(value.0, vec![0xd800], "SPOTMODEL manual string union units")
        }
        other => panic!("SPOTMODEL manual string union variant {other:?}"),
    }
}

#[test]
fn native_declared_fields_ignore_unknown_utf16_keys_and_take_last_duplicate() {
    let level: WsLevel = serde_json::from_str(
        r#"{"px":"old","px":"\ud800X","sz":"\udfffY","n":-0,"\ud800":{"ignored":"\udfff"}}"#,
    )
    .unwrap();
    assert_eq!(
        level.px.0,
        vec![0xd800, 0x58],
        "SPOTMODEL native duplicate last price units"
    );
    assert_eq!(
        level.sz.0,
        vec![0xdfff, 0x59],
        "SPOTMODEL native unknown-key size units"
    );
    assert_eq!(
        level.n.to_bits(),
        (-0.0f64).to_bits(),
        "SPOTMODEL native unknown-key number bits"
    );
    let escaped: WsLevel =
        serde_json::from_str(r#" { "\u0070x":"A", "sz":"B", "n":1e400 } "#).unwrap();
    assert_eq!(
        escaped.px.0,
        vec![0x41],
        "SPOTMODEL native escaped property-name units"
    );
    assert_eq!(
        escaped.n.to_bits(),
        f64::INFINITY.to_bits(),
        "SPOTMODEL native whitespace overflow bits"
    );
}

#[test]
fn native_nested_closed_orders_take_last_duplicate_and_ignore_unknown_keys() {
    let order: OrderRequest = serde_json::from_str(r#"{"coin":"X","is_buy":true,"sz":17.5,"limit_px":91.25,"order_type":{"limit":{"tif":"bad"},"limit":{"tif":"Alo","\ud800":0},"trigger":{"triggerPx":-0,"isMarket":false,"tpsl":"tp","\udfff":1},"\ud800":2},"reduce_only":false}"#).unwrap();
    assert_eq!(
        order.order_type.limit.unwrap().tif,
        Tif::Alo,
        "SPOTMODEL native nested last TIF"
    );
    let trigger = order.order_type.trigger.unwrap();
    assert_eq!(
        trigger.trigger_px.to_bits(),
        (-0.0f64).to_bits(),
        "SPOTMODEL native nested trigger bits"
    );
    assert_eq!(
        trigger.tpsl,
        Tpsl::Tp,
        "SPOTMODEL native nested trigger tag"
    );
    assert!(!trigger.is_market, "SPOTMODEL native nested trigger flag");
    assert_eq!(
        order.sz.to_bits(),
        17.5f64.to_bits(),
        "SPOTMODEL native nested size bits"
    );
    assert_eq!(
        order.limit_px.to_bits(),
        91.25f64.to_bits(),
        "SPOTMODEL native nested price bits"
    );
}

#[test]
fn malformed_raw_json_is_rejected_before_native_decode() {
    for input in [
        "\"\\u12xy\"",
        "\"\\x41\"",
        "\"literal\nnewline\"",
        "\"unterminated",
        "\"X\" trailing",
    ] {
        assert!(
            serde_json::from_str::<Cloid>(input).is_err(),
            "SPOTMODEL malformed string {input:?}"
        );
    }
    for input in [
        "{\"px\":\"X\",\"sz\":\"Y\",\"n\":0,}",
        "{\"px\":\"X\",\"sz\":\"Y\",\"n\":01}",
    ] {
        assert!(
            serde_json::from_str::<WsLevel>(input).is_err(),
            "SPOTMODEL malformed object {input:?}"
        );
    }
}

#[test]
fn native_frontend_boolean_fields_are_distinct() {
    let order: FrontendOpenOrders = serde_json::from_str(r#"{"coin":"X","isPositionTpsl":true,"isTrigger":true,"limitPx":"91.25","oid":42,"orderType":"limit","origSz":"19","reduceOnly":false,"side":"B","sz":"17.5","timestamp":7,"triggerCondition":"none","triggerPx":"0"}"#).unwrap();
    assert!(
        order.is_position_tpsl,
        "SPOTMODEL native frontend position flag"
    );
    assert!(order.is_trigger, "SPOTMODEL native frontend trigger flag");
    assert!(!order.reduce_only, "SPOTMODEL native frontend reduce flag");
}
