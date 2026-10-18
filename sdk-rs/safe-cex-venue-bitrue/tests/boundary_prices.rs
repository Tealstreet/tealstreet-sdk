//! Measured representation losses are separate from ordinary passing examples.
use safe_cex_num::numeric_ingress::js_from_str;
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_types::Market;
use safe_cex_venue_bitrue::{
    account_helpers::*, balance::parse_balance, read_helpers::*, utils::reverse_ws_symbol,
    wire::OpaqueValue,
};
use serde_json::{Value, json};
use std::process::Command;
#[test]
fn exact_units_and_whole_input_prices() {
    // All native assertions precede source capture/transport comparisons.
    let inputs = [
        r#"{"side":"BUY","bidId":"\ud800"}"#,
        r#""\ud800USDT""#,
        r#""\ud800_USDT""#,
        r#"{"data":{"account":[{"marginCoin":"USDT","positionVos":[{"contractName":"\ud800","positions":[{"side":"BUY","volume":1,"openPrice":2,"leverageLevel":3,"positionType":2}]}]}]}}"#,
        r#"{"symbol":"\ud800","multiplier":5}"#,
        r#"{"symbol":"\ud800","maximum":7}"#,
        r#"{"extra":"\ud800","data":{"account":[{"marginCoin":"USDT","totalEquity":31,"accountNormal":7,"accountLock":11,"sumMaintenanceMargin":13,"totalPositionNormal":17,"totalCost":19,"sumOpenRealizedAmount":-23}]}}"#,
    ];
    assert_eq!(inputs.len(), 7, "PRICE-NATIVE-INDEPENDENT-INPUTS");
    for (i, raw) in inputs.iter().enumerate() {
        assert!(
            js_from_str::<OpaqueValue>(raw).is_err(),
            "PRICE-NATIVE-WHOLE-INPUT:{i}"
        );
    }
    assert!(String::from_utf16(&[0xd800]).is_err(), "PRICE-NATIVE-STR");
    let replacement = "\u{fffd}";
    assert_eq!(
        normalize_trade_symbol(Some(&json!(format!("{replacement}USDT")))).unwrap(),
        format!("{replacement}USDT")
    );
    assert_eq!(
        reverse_ws_symbol(&format!("{replacement}_USDT")),
        format!("E-{replacement}-USDT")
    );
    assert_eq!(
        pick_trade_order_id(
            json!({"side":"BUY","bidId":replacement})
                .as_object()
                .unwrap()
        )
        .unwrap()
        .as_deref(),
        Some(replacement)
    );
    let account: OpaqueValue = js_from_str(&inputs[3].replace("\\ud800", replacement)).unwrap();
    let accounts = get_position_accounts(Some(&account.value))
        .unwrap()
        .unwrap();
    assert_eq!(accounts.len(), 1);
    assert!(std::ptr::eq(
        accounts[0],
        &account.value["data"]["account"][0]
    ));
    let positions = get_account_position_snapshots(Some(&account.value))
        .unwrap()
        .unwrap();
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].symbol, replacement);
    let store = DefaultStore::default();
    let market:Market=js_from_str(r#"{"id":"replacement","symbol":"�","base":"BTC","quote":"USDT","active":true,"precision":{"amount":1,"price":0.01},"limits":{"amount":{"min":1,"max":100},"leverage":{"min":1,"max":3}},"info":{"multiplier":7}}"#).unwrap();
    Store::update(
        &store,
        StoreUpdate {
            markets: Some(vec![market]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    assert_eq!(
        resolve_trade_qty(
            json!({"qty":3,"amount":15}).as_object().unwrap(),
            &store,
            replacement,
            1.0
        )
        .unwrap()
        .to_bits(),
        3.0f64.to_bits(),
        "PRICE-NATIVE-REPLACED-KEY-QTY"
    );
    set_market_max_leverage(&store, replacement, 7.0).unwrap();
    assert_eq!(
        store.markets()[0].limits.leverage.max.to_bits(),
        7.0f64.to_bits(),
        "PRICE-NATIVE-REPLACEMENT-UPDATE"
    );
    let raw = inputs[6].replace("\\ud800", replacement);
    let data: OpaqueValue = js_from_str(&raw).unwrap();
    let stored = store.balance_handle();
    let balance = parse_balance(Some(&data.value), &stored, |warning| {
        panic!("unexpected refusal: {warning}")
    })
    .unwrap();
    assert_eq!(balance.total.to_bits(), 31.0f64.to_bits());
    assert_eq!(
        balance.info.as_ref().unwrap().get("extra").unwrap(),
        replacement
    );
    let bracket = r#"{"data":[{"contractName":"�","brackets":[{"initialLeverage":3}]},{"contractName":"\ud800","brackets":[{"initialLeverage":7}]}]}"#;
    assert!(
        js_from_str::<OpaqueValue>(bracket).is_err(),
        "PRICE-NATIVE-WHOLE-BRACKET"
    );
    let replaced: OpaqueValue = js_from_str(&bracket.replace("\\ud800", replacement)).unwrap();
    assert_eq!(
        parse_max_leverage_from_bracket_response(Some(&replaced.value), Some(replacement)).unwrap(),
        Some(3.0),
        "PRICE-NATIVE-REPLACED-BRACKET"
    );
    let scalar:OpaqueValue=js_from_str(r#"{"data":[{"contractName":"A","brackets":[{"initialLeverage":3}]},{"contractName":"B","brackets":[{"initialLeverage":7}]}]}"#).unwrap();
    assert_eq!(
        parse_max_leverage_from_bracket_response(Some(&scalar.value), Some("B")).unwrap(),
        Some(7.0),
        "PRICE-NATIVE-SCALAR-KEY"
    );
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/boundary_prices_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "PRICE-SOURCE:{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source["units"],
        json!({"order":[55296],"normal":[55296,85,83,68,84],"reverse":[69,45,55296,45,85,83,68,84],"account":[55296],"snapshot":[55296],"info":[55296]})
    );
    assert_eq!(source["quantity"], 15);
    assert_eq!(source["selectedMax"], 7);
    assert_eq!(source["otherMax"], 3);
    assert_eq!(source["bracket"], json!([7, 3]));
}
