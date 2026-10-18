use safe_cex_num::JsCoercionError;
use safe_cex_num::JsJsonValue;
use safe_cex_venue_okx::position_adapter::{OkxPositionAdapterDeps, map_okx_position_history};
use serde_json::json;

fn native_number(value: impl Into<JsJsonValue>) -> f64 {
    match value.into() {
        JsJsonValue::Number(value) => value,
        _ => panic!("OKX JOURNAL native numeric kind retained"),
    }
}
#[test]
fn journal_computed_numbers_survive_before_wire_projection() {
    let row = json!({"instId":"BTC-USDT-SWAP","closeTotalPos":"1e308","openMaxPos":"1e308","fee":"0","fundingFee":"0","realizedPnl":"0","openAvgPx":"100","uTime":"1000","cTime":"1","posSide":"long","type":"1"});
    let deps = OkxPositionAdapterDeps {
        account: "fixture",
        to_base: &|number, _| Ok::<_, JsCoercionError>(number * 2.0),
        resolve_symbol: &|_| Ok::<_, JsCoercionError>(Some("BTCUSDT".into())),
        resolve_settle_currency: &|_| Ok::<_, JsCoercionError>(Some("USDT".into())),
    };
    let record = map_okx_position_history(row.as_object().unwrap(), &deps)
        .unwrap()
        .unwrap();
    assert_eq!(
        native_number(record["closedSize"].clone()).to_bits(),
        f64::INFINITY.to_bits(),
        "OKX JOURNAL native closedSize overflow"
    );
    assert_eq!(
        native_number(record["maxSize"].clone()).to_bits(),
        f64::INFINITY.to_bits(),
        "OKX JOURNAL native maxSize overflow"
    );
    assert_eq!(
        native_number(record["fees"].clone()).to_bits(),
        (-0.0_f64).to_bits(),
        "OKX JOURNAL native signed fee zero"
    );
    let mut row = row;
    row["closeTotalPos"] = json!("1");
    row["openMaxPos"] = json!("2");
    let deps = OkxPositionAdapterDeps {
        to_base: &|number, _| Ok::<_, JsCoercionError>(if number == 1.0 { 1.0 } else { f64::NAN }),
        ..deps
    };
    let record = map_okx_position_history(row.as_object().unwrap(), &deps)
        .unwrap()
        .unwrap();
    assert!(
        native_number(record["maxSize"].clone()).is_nan(),
        "OKX JOURNAL native callback NaN"
    );
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/journal-numbers-source.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "OKX JOURNAL actual source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
