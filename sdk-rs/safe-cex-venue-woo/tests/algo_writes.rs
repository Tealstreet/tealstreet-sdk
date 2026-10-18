#[path = "support/retained_input_observations.rs"]
mod retained_input_observations;
use safe_cex_num::numeric_ingress::{JsNumberInput, js_from_str};
use safe_cex_types::object_map::JsObjectMap;
use safe_cex_venue_woo::v3_private_orders::{
    WooV3AlgoOrder, WooV3AlgoType, WooV3GatewayAlgoQuantity, WooV3MarginMode, WooV3OrderSide,
    WooV3OrderStatus, WooV3RegularOrderType, WooV3TriggerPriceType, WooV3TriggerStatus,
};
use safe_cex_venue_woo::{
    v3_algo_writes::*,
    v3_contracts::{WooV3Endpoint, WooV3HttpMethod},
    v3_private::{WooV3AccountPositionMode, WooV3PositionSide},
    v3_request::WooV3RequestInput,
};
use serde_json::Value;
use std::process::Command;
fn source() -> Value {
    let o = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/algo-writes-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "ALGO-WRITES source oracle {}",
        String::from_utf8_lossy(&o.stderr)
    );
    serde_json::from_slice(&o.stdout).unwrap()
}
#[test]
fn native_algo_declarations_before_source() {
    use serde_json::json;
    fn captured(value: &Value) -> JsNumberInput {
        js_from_str(&value.to_string()).unwrap()
    }
    fn expected(value: &Value) -> Value {
        let mut expectation = value.clone();
        expectation["clientAlgoOrderId"] = json!({"$number":"83"});
        expectation
    }
    let common = json!({"symbol":"PERP_BTC_USDT","side":"BUY","positionMode":"HEDGE_MODE","clientAlgoOrderId":83,"algoOrderTag":"decl-19"});
    let mut stop = common.clone();
    stop["family"] = json!("STOP_MARKET");
    stop["quantity"] = json!("3.25");
    stop["triggerPrice"] = json!("101.25");
    stop["triggerPriceType"] = json!("MARKET_PRICE");
    let stop_capture = captured(&stop);
    let mut native = stop_capture
        .clone()
        .decode::<WooV3StopMarketPlaceInput>()
        .unwrap();
    stop_input(&native, &expected(&stop));
    assert_eq!(
        native.quantity, "3.25",
        "WOO-ALGO-DECL native stop quantity"
    );
    assert_eq!(native.trigger_price, "101.25");
    native.quantity = "9.75".into();
    let retained = stop_capture
        .clone()
        .decode::<JsObjectMap<JsNumberInput>>()
        .unwrap();
    assert_eq!(
        retained["quantity"].clone().decode::<String>().unwrap(),
        "3.25",
        "WOO-ALGO-DECL native retained stop capture"
    );
    native.family = "PUBLIC_OTHER".into();
    assert_eq!(native.family, "PUBLIC_OTHER");
    let mut trailing = common.clone();
    trailing["family"] = json!("TRAILING_STOP");
    trailing["quantity"] = json!("4.125");
    trailing["callback"] = json!({"source":"WOO_NATIVE_CALLBACK_VALUE","value":".25"});
    trailing["activatedPrice"] = json!("97.5");
    let trailing_capture = captured(&trailing);
    let mut native = trailing_capture
        .clone()
        .decode::<WooV3TrailingPlaceInput>()
        .unwrap();
    trailing_input(&native, &expected(&trailing));
    native.callback.value = ".75".into();
    let retained = trailing_capture
        .clone()
        .decode::<JsObjectMap<JsNumberInput>>()
        .unwrap();
    let callback = retained["callback"]
        .clone()
        .decode::<JsObjectMap<JsNumberInput>>()
        .unwrap();
    assert_eq!(
        callback["value"].clone().decode::<String>().unwrap(),
        ".25",
        "WOO-ALGO-DECL native retained callback capture"
    );
    native.callback.source = "PUBLIC_OTHER".into();
    assert_eq!(native.callback.source, "PUBLIC_OTHER");
    let trigger = json!({"triggerPrice":"121.25","triggerPriceType":"MARKET_PRICE"});
    trigger_input(
        &captured(&trigger)
            .decode::<WooV3PositionalTriggerInput>()
            .unwrap(),
        &trigger,
    );
    let mut positional = common.clone();
    positional["family"] = json!("POSITIONAL_TP_SL");
    positional["takeProfit"] = trigger.clone();
    positional["stopLoss"] = json!({"triggerPrice":"81.5","triggerPriceType":"INDEX_PRICE"});
    positional_input(
        &captured(&positional)
            .decode::<WooV3PositionalPlaceInput>()
            .unwrap(),
        &expected(&positional),
    );
    for (value, kind) in [(&stop, 0), (&trailing, 1), (&positional, 2)] {
        match (
            captured(value).decode::<WooV3AlgoPlaceInput>().unwrap(),
            kind,
        ) {
            (WooV3AlgoPlaceInput::StopMarket(v), 0) => stop_input(&v, &expected(value)),
            (WooV3AlgoPlaceInput::TrailingStop(v), 1) => trailing_input(&v, &expected(value)),
            (WooV3AlgoPlaceInput::PositionalTpSl(v), 2) => positional_input(&v, &expected(value)),
            _ => panic!("WOO-ALGO-DECL native union family"),
        }
    }
    let row =
        json!({"algoOrderId":71,"clientAlgoOrderId":83,"algoType":"STOP_LOSS","quantity":"5.375"});
    let native = captured(&row)
        .decode::<WooV3AlgoPlaceResponseRow>()
        .unwrap();
    assert_eq!(
        native.algo_order_id.to_bits(),
        71.0f64.to_bits(),
        "WOO-ALGO-DECL native response algo ID"
    );
    assert_eq!(native.client_algo_order_id.to_bits(), 83.0f64.to_bits());
    assert_eq!(native.algo_type, WooV3AlgoPlaceType::StopLoss);
    assert_eq!(native.quantity, "5.375");
    for (raw, is_algo, bits) in [
        (r#"{"algoOrderId":-0}"#, true, (-0.0f64).to_bits()),
        (
            r#"{"clientAlgoOrderId":1e999}"#,
            false,
            f64::INFINITY.to_bits(),
        ),
    ] {
        match (js_from_str::<WooV3AlgoWriteTarget>(raw).unwrap(), is_algo) {
            (WooV3AlgoWriteTarget::AlgoOrderId(value), true)
            | (WooV3AlgoWriteTarget::ClientAlgoOrderId(value), false) => {
                assert_eq!(value.to_bits(), bits)
            }
            _ => panic!("WOO-ALGO-DECL native target variant"),
        }
    }
    assert!(
        js_from_str::<WooV3AlgoWriteTarget>(r#"{"algoOrderId":71,"clientAlgoOrderId":83}"#)
            .is_err()
    );
    let mut absent = stop.clone();
    absent.as_object_mut().unwrap().remove("clientAlgoOrderId");
    absent.as_object_mut().unwrap().remove("algoOrderTag");
    let native = captured(&absent)
        .decode::<WooV3StopMarketPlaceInput>()
        .unwrap();
    assert!(native.client_algo_order_id.is_none() && native.algo_order_tag.is_none());
    let mut null = absent.clone();
    null["algoOrderTag"] = Value::Null;
    assert!(
        captured(&null)
            .decode::<WooV3StopMarketPlaceInput>()
            .is_err()
    );
    let map = captured(&stop)
        .decode::<JsObjectMap<JsNumberInput>>()
        .unwrap();
    let WooV3AlgoPlaceBuildResult::Ready {
        request,
        family,
        trigger_semantics,
    } = build_woo_v3_algo_place_request(&map).unwrap()
    else {
        panic!("WOO-ALGO-DECL native ready result")
    };
    assert_eq!(family, WooV3AlgoPlaceFamily::StopMarket);
    assert_eq!(trigger_semantics, "official-last-price-trigger");
    assert_eq!(request.method, WooV3HttpMethod::Post);
    assert_eq!(request.path, WooV3Endpoint::TradeAlgoOrder);
    assert!(request.query.is_none());
    let body = request.body.unwrap();
    assert_eq!(body.len(), 12);
    assert_eq!(
        body["algoOrderTag"].clone().decode::<String>().unwrap(),
        "decl-19"
    );
    for value in [&stop, &trailing, &positional] {
        let raw = value.to_string().replace("decl-19", r"\ud800");
        typed_utf16::assert_projection(&raw, &js_from_str::<WooV3AlgoPlaceInput>(&raw).unwrap());
        if value["family"] == "STOP_MARKET" {
            typed_utf16::assert_projection(
                &raw,
                &js_from_str::<WooV3StopMarketPlaceInput>(&raw).unwrap(),
            );
        }
        if value["family"] == "TRAILING_STOP" {
            typed_utf16::assert_projection(
                &raw,
                &js_from_str::<WooV3TrailingPlaceInput>(&raw).unwrap(),
            );
        }
        if value["family"] == "POSITIONAL_TP_SL" {
            typed_utf16::assert_projection(
                &raw,
                &js_from_str::<WooV3PositionalPlaceInput>(&raw).unwrap(),
            );
        }
        let retained: JsObjectMap<JsNumberInput> = js_from_str(&raw).unwrap();
        let tag = retained["algoOrderTag"]
            .clone()
            .decode::<safe_cex_types::retained_json::RetainedJsonInput>()
            .unwrap();
        assert_eq!(
            tag.as_text().unwrap(),
            &[0xd800],
            "WOO-ALGO-DECL native retained tag"
        );
    }
    let raw = r#"{"triggerPrice":"\ud800","triggerPriceType":"MARKET_PRICE"}"#;
    typed_utf16::assert_projection(
        raw,
        &js_from_str::<WooV3PositionalTriggerInput>(raw).unwrap(),
    );
    let raw = r#"{"algoOrderId":71,"clientAlgoOrderId":83,"algoType":"STOP","quantity":"\ud800"}"#;
    typed_utf16::assert_projection(raw, &js_from_str::<WooV3AlgoPlaceResponseRow>(raw).unwrap());
    for expected in ["\u{10000}", r"\ud800"] {
        let mut value = trigger.clone();
        value["triggerPrice"] = json!(expected);
        assert_eq!(
            captured(&value)
                .decode::<WooV3PositionalTriggerInput>()
                .unwrap()
                .trigger_price,
            expected
        );
        let mut value = row.clone();
        value["quantity"] = json!(expected);
        assert_eq!(
            captured(&value)
                .decode::<WooV3AlgoPlaceResponseRow>()
                .unwrap()
                .quantity,
            expected
        );
        for value in [&stop, &trailing, &positional] {
            let mut value = value.clone();
            value["algoOrderTag"] = json!(expected);
            match captured(&value).decode::<WooV3AlgoPlaceInput>().unwrap() {
                WooV3AlgoPlaceInput::StopMarket(v) => {
                    assert_eq!(v.algo_order_tag.as_deref(), Some(expected))
                }
                WooV3AlgoPlaceInput::TrailingStop(v) => {
                    assert_eq!(v.algo_order_tag.as_deref(), Some(expected))
                }
                WooV3AlgoPlaceInput::PositionalTpSl(v) => {
                    assert_eq!(v.algo_order_tag.as_deref(), Some(expected))
                }
            }
        }
    }
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/algo-writes-oracle.cjs"
        ))
        .arg("--declaration-audit")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "WOO-ALGO-DECL source observations: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let observation: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(observation["groups"], 8);
    assert_eq!(observation["typedLoneFields"], 2);
    assert_eq!(observation["checkerControlErrors"], 4);
}
#[test]
fn native_algo_guard_choices_before_source() {
    fn capture(raw: &str) -> JsNumberInput {
        js_from_str(raw).unwrap()
    }
    let mut policy = WOO_V3_ALGO_WRITE_PARITY;
    assert!(policy.request_and_response_fixtures);
    assert!(
        !policy.runtime_imported,
        "WOO-ALGO-GUARD native runtime remains inert"
    );
    assert!(!policy.credentialed_demo_lifecycle);
    assert_eq!(policy.trigger_price_type, "MARKET_PRICE-only");
    assert_eq!(
        policy.trailing_input,
        "vendor-native-callbackValue-no-safe-cex-conversion"
    );
    assert_eq!(
        policy.positional_response_identity,
        "blocked-pending-demo-evidence"
    );
    assert_eq!(
        policy.positional_update,
        "blocked-official-edit-schema-has-no-childOrders"
    );
    assert_eq!(
        policy.partial_fill_and_quantity_amend,
        "blocked-pending-demo-evidence"
    );
    assert_eq!(
        policy.symbol_pending_cancel,
        "blocked-published-query-schema-conflicts-with-prose"
    );
    assert_eq!(policy.cancel_response_identity, "request-target-only");
    policy.runtime_imported = true;
    assert!(policy.runtime_imported);
    let fresh_policy = WOO_V3_ALGO_WRITE_PARITY;
    assert!(!fresh_policy.runtime_imported);
    let record_input = capture(r#"{"kept":41,"z":-0,"nested":{"count":43}}"#);
    let mut record = require_record(Some(&record_input), "probe").unwrap();
    assert_eq!(record.len(), 3);
    assert_eq!(
        record["kept"].clone().decode::<f64>().unwrap().to_bits(),
        41.0f64.to_bits()
    );
    assert_eq!(
        record["z"].clone().decode::<f64>().unwrap().to_bits(),
        (-0.0f64).to_bits()
    );
    let nested = record["nested"]
        .clone()
        .decode::<JsObjectMap<JsNumberInput>>()
        .unwrap();
    assert_eq!(nested.len(), 1);
    assert_eq!(
        nested["count"].clone().decode::<f64>().unwrap().to_bits(),
        43.0f64.to_bits()
    );
    record.insert("kept".into(), capture("47"));
    let original = record_input
        .clone()
        .decode::<JsObjectMap<JsNumberInput>>()
        .unwrap();
    assert_eq!(
        original["kept"].clone().decode::<f64>().unwrap().to_bits(),
        41.0f64.to_bits(),
        "WOO-ALGO-GUARD native record snapshot"
    );
    let array_input = capture("[-0,1e999,{\"count\":53}]");
    let mut array = require_array(Some(&array_input), "probe").unwrap();
    assert_eq!(array.len(), 3);
    assert_eq!(
        array[0].clone().decode::<f64>().unwrap().to_bits(),
        (-0.0f64).to_bits()
    );
    assert_eq!(
        array[1].clone().decode::<f64>().unwrap().to_bits(),
        f64::INFINITY.to_bits()
    );
    let nested = array[2]
        .clone()
        .decode::<JsObjectMap<JsNumberInput>>()
        .unwrap();
    assert_eq!(
        nested["count"].clone().decode::<f64>().unwrap().to_bits(),
        53.0f64.to_bits()
    );
    array[0] = capture("59");
    assert_eq!(
        array_input.clone().decode::<Vec<JsNumberInput>>().unwrap()[0]
            .clone()
            .decode::<f64>()
            .unwrap()
            .to_bits(),
        (-0.0f64).to_bits(),
        "WOO-ALGO-GUARD native array snapshot"
    );
    for (raw, message) in [
        ("null", "probe must be an object"),
        ("[]", "probe must be an object"),
    ] {
        assert_eq!(
            require_record(Some(&capture(raw)), "probe").unwrap_err(),
            message
        );
    }
    assert_eq!(
        require_array(Some(&capture("{}")), "probe").unwrap_err(),
        "probe must be an array"
    );
    assert!(require_record(None, "probe").is_err());
    assert!(require_array(None, "probe").is_err());
    for (raw, expected) in [("true", true), ("false", false)] {
        assert_eq!(
            require_boolean(Some(&capture(raw)), "probe").unwrap(),
            expected,
            "WOO-ALGO-GUARD native boolean identity"
        );
    }
    for raw in ["0", "1", "null", r#""true""#] {
        assert_eq!(
            require_boolean(Some(&capture(raw)), "probe").unwrap_err(),
            "probe must be a boolean"
        );
    }
    for (raw, minimum, expected) in [
        ("-0", 0.0, -0.0f64),
        ("9007199254740991", 1.0, 9007199254740991.0),
        ("1", f64::NAN, 1.0),
    ] {
        assert_eq!(
            require_integer(Some(&capture(raw)), "probe", minimum)
                .unwrap()
                .to_bits(),
            expected.to_bits(),
            "WOO-ALGO-GUARD native integer bits"
        );
    }
    for raw in ["1.25", "9007199254740992", "1e999", r#""1""#] {
        assert_eq!(
            require_integer(Some(&capture(raw)), "probe", 0.0).unwrap_err(),
            "probe must be a safe integer of at least 0"
        );
    }
    assert_eq!(
        require_integer(Some(&capture("0")), "probe", 1.0).unwrap_err(),
        "probe must be a safe integer of at least 1"
    );
    assert_eq!(
        require_string(Some(&capture(r#""  ""#)), "probe").unwrap(),
        "  "
    );
    for raw in [r#""""#, "0", "null"] {
        assert_eq!(
            require_string(Some(&capture(raw)), "probe").unwrap_err(),
            "probe must be a non-empty string"
        );
    }
    assert_eq!(
        require_enum(Some(&capture(r#""B""#)), &["A", "B"], "probe").unwrap(),
        "B"
    );
    assert_eq!(
        require_enum(Some(&capture(r#""C""#)), &["B", "A"], "probe").unwrap_err(),
        "probe must be one of B, A"
    );
    assert_eq!(
        require_enum(Some(&capture(r#""C""#)), &["A"], "probe").unwrap_err(),
        "probe must be one of A",
        "WOO-ALGO-GUARD native fixed membership refuses C"
    );
    assert_eq!(
        require_perpetual_symbol("NOT_PERP").unwrap_err(),
        "WOO V3 symbol must use PERP_BASE_QUOTE form",
        "WOO-ALGO-GUARD native fixed grammar refuses NOT_PERP"
    );
    assert_eq!(require_perpetual_symbol("PERP_1_2").unwrap(), "PERP_1_2");
    for symbol in [
        "SPOT_BTC_USDT",
        "PERP_btc_USDT",
        "PERP_BTC_USDT\n",
        "PERP__USDT",
    ] {
        assert_eq!(
            require_perpetual_symbol(symbol).unwrap_err(),
            "WOO V3 symbol must use PERP_BASE_QUOTE form"
        );
    }
    for (trigger, expected) in [
        (WooV3AlgoTriggerPriceType::MarketPrice, false),
        (WooV3AlgoTriggerPriceType::MarkPrice, true),
        (WooV3AlgoTriggerPriceType::IndexPrice, true),
        (WooV3AlgoTriggerPriceType::LastPrice, true),
    ] {
        assert_eq!(
            has_unsupported_trigger(trigger),
            expected,
            "WOO-ALGO-GUARD native trigger predicate"
        );
    }
    assert!(js_from_str::<WooV3AlgoTriggerPriceType>(r#""UNKNOWN""#).is_err());
    for raw in [r#"{"kept":"\ud800"}"#, r#"["\ud800"]"#, r#""\ud800""#] {
        use safe_cex_types::retained_json::RetainedJsonInput as Input;
        let retained = retained_input_observations::capture(raw);
        let units = match &retained {
            Input::Text(units) => units.as_slice(),
            Input::Array(values) => values[0].as_text().unwrap(),
            Input::Object(_) => retained.get("kept").unwrap().as_text().unwrap(),
            other => panic!("unexpected retained guard shape {other:?}"),
        };
        assert_eq!(units, &[0xd800], "WOO-ALGO-GUARD native captured units");
    }
    for expected in ["\u{10000}", r"\ud800"] {
        let value = safe_cex_num::numeric_ingress::js_from_str::<JsNumberInput>(
            &serde_json::to_string(expected).unwrap(),
        )
        .unwrap();
        assert_eq!(require_string(Some(&value), "probe").unwrap(), expected);
        assert_eq!(
            require_enum(Some(&value), &[expected], "probe").unwrap(),
            expected
        );
        let record = require_record(
            Some(&capture(&serde_json::json!({"kept":expected}).to_string())),
            "probe",
        )
        .unwrap();
        assert_eq!(record["kept"].clone().decode::<String>().unwrap(), expected);
        let array = require_array(
            Some(&capture(&serde_json::json!([expected]).to_string())),
            "probe",
        )
        .unwrap();
        assert_eq!(array[0].clone().decode::<String>().unwrap(), expected);
    }
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/algo-writes-oracle.cjs"
        ))
        .arg("--guard-audit")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "WOO-ALGO-GUARD source observations: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let observation: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(observation["groups"], 8);
    assert_eq!(observation["returnedLoneRows"], 4);
}
#[test]
fn native_algo_request_choices_before_source() {
    use serde_json::json;
    fn map(v: Value) -> JsObjectMap<JsNumberInput> {
        js_from_str(&v.to_string()).unwrap()
    }
    fn captured(v: Value) -> JsNumberInput {
        js_from_str(&v.to_string()).unwrap()
    }
    fn text(v: &JsObjectMap<JsNumberInput>, key: &str) -> String {
        v.get(key).unwrap().clone().decode().unwrap()
    }
    fn common_body(body: &JsObjectMap<JsNumberInput>) {
        assert_eq!(text(body, "symbol"), "PERP_BTC_USDT");
        assert_eq!(text(body, "positionSide"), "SHORT");
        assert_eq!(text(body, "algoOrderTag"), "audit-19");
        assert_eq!(
            body.get("clientAlgoOrderId")
                .unwrap()
                .clone()
                .decode::<f64>()
                .unwrap()
                .to_bits(),
            83.0f64.to_bits()
        );
    }
    fn common() -> Value {
        json!({"symbol":"PERP_BTC_USDT","side":"BUY","positionMode":"HEDGE_MODE","clientAlgoOrderId":83,"algoOrderTag":"audit-19"})
    }
    fn stop() -> Value {
        let mut v = common();
        v["family"] = json!("STOP_MARKET");
        v["quantity"] = json!("0003.2500");
        v["triggerPrice"] = json!("101.25");
        v["triggerPriceType"] = json!("MARKET_PRICE");
        v
    }
    fn order() -> Value {
        json!({"algoOrderId":71,"clientAlgoOrderId":83,"algoType":"STOP","totalExecutedQuantity":"0","algoStatus":"NEW","rootAlgoStatus":"NEW","isTriggered":false,"triggerStatus":"USELESS","triggerPriceType":"MARKET_PRICE","triggerPrice":"101.25"})
    }
    fn amend(o: Value) -> Value {
        json!({"target":{"algoOrderId":71},"currentOrder":o,"update":{"triggerPrice":"102.5","triggerPriceType":"MARKET_PRICE"}})
    }
    fn blocked(v: Value, expected: &str) {
        let WooV3AlgoAmendBuildResult::Blocked { blocker, target } =
            build_woo_v3_algo_amend_request(&map(v)).unwrap()
        else {
            panic!("WOO-ALGO-REQUEST native expected amend blocker")
        };
        assert_eq!(
            blocker, expected,
            "WOO-ALGO-REQUEST native amend blocker precedence"
        );
        assert!(
            matches!(target,WooV3AlgoWriteTarget::AlgoOrderId(n) if n.to_bits()==71.0f64.to_bits())
        );
    }
    assert_eq!(
        require_decimal(Some(&captured(json!("0003.2500"))), "probe", false).unwrap(),
        "0003.2500"
    );
    assert_eq!(
        require_decimal(Some(&captured(json!("0"))), "probe", true).unwrap(),
        "0"
    );
    assert_eq!(
        require_decimal(Some(&captured(json!("0"))), "probe", false).unwrap_err(),
        "probe must be greater than zero",
        "WOO-ALGO-REQUEST native zero quantity refusal"
    );
    assert_eq!(
        require_decimal(Some(&captured(json!("not-decimal"))), "probe", false).unwrap_err(),
        "probe must be a finite non-negative decimal string"
    );
    assert!(
        matches!(validate_target(&map(json!({"algoOrderId":71}))).unwrap(),WooV3AlgoWriteTarget::AlgoOrderId(n) if n.to_bits()==71.0f64.to_bits())
    );
    assert!(
        matches!(validate_target(&map(json!({"clientAlgoOrderId":83}))).unwrap(),WooV3AlgoWriteTarget::ClientAlgoOrderId(n) if n.to_bits()==83.0f64.to_bits())
    );
    assert_eq!(
        validate_target(&map(json!({"algoOrderId":0,"clientAlgoOrderId":83}))).unwrap_err(),
        "WOO V3 algo target requires exactly one ID"
    );
    assert_eq!(
        validate_target(&map(json!({}))).unwrap_err(),
        "WOO V3 algo target requires exactly one ID"
    );
    let normalized = validate_common(&map(common())).unwrap();
    assert_eq!(normalized.len(), 5);
    assert_eq!(text(&normalized, "symbol"), "PERP_BTC_USDT");
    assert_eq!(text(&normalized, "side"), "BUY");
    assert_eq!(
        text(&normalized, "positionSide"),
        "SHORT",
        "WOO-ALGO-REQUEST native hedge closing side"
    );
    assert_eq!(text(&normalized, "algoOrderTag"), "audit-19");
    assert_eq!(
        normalized
            .get("clientAlgoOrderId")
            .unwrap()
            .clone()
            .decode::<f64>()
            .unwrap()
            .to_bits(),
        83.0f64.to_bits()
    );
    for (side, mode, expected) in [("SELL", "HEDGE_MODE", "LONG"), ("BUY", "ONE_WAY", "BOTH")] {
        let mut v = common();
        v["side"] = json!(side);
        v["positionMode"] = json!(mode);
        assert_eq!(
            text(&validate_common(&map(v)).unwrap(), "positionSide"),
            expected
        );
    }
    let mut invalid = common();
    invalid["algoOrderTag"] = json!({"kept":41});
    assert_eq!(
        validate_common(&map(invalid)).unwrap_err(),
        "WOO V3 algoOrderTag must contain 1 to 64 characters"
    );
    let mut wide = common();
    wide["algoOrderTag"] = json!("😀".repeat(32));
    assert_eq!(
        text(
            &validate_common(&map(wide.clone())).unwrap(),
            "algoOrderTag"
        )
        .encode_utf16()
        .count(),
        64
    );
    wide["algoOrderTag"] = json!("😀".repeat(33));
    assert!(validate_common(&map(wide)).is_err());
    let stop_input = map(stop());
    let WooV3AlgoPlaceBuildResult::Ready {
        request,
        family,
        trigger_semantics,
    } = build_woo_v3_algo_place_request(&stop_input).unwrap()
    else {
        panic!("WOO-ALGO-REQUEST native stop ready")
    };
    assert!(matches!(family, WooV3AlgoPlaceFamily::StopMarket));
    assert_eq!(trigger_semantics, "official-last-price-trigger");
    assert_eq!(request.method, WooV3HttpMethod::Post);
    assert_eq!(request.path, WooV3Endpoint::TradeAlgoOrder);
    assert!(request.query.is_none());
    let mut body = request.body.unwrap();
    common_body(&body);
    assert_eq!(text(&body, "side"), "BUY");
    assert_eq!(body.len(), 12);
    assert_eq!(text(&body, "quantity"), "0003.2500");
    assert_eq!(text(&body, "triggerPrice"), "101.25");
    assert_eq!(text(&body, "positionSide"), "SHORT");
    assert_eq!(text(&body, "algoType"), "STOP");
    assert_eq!(text(&body, "type"), "MARKET");
    assert_eq!(text(&body, "marginMode"), "CROSS");
    assert_eq!(text(&body, "triggerPriceType"), "MARKET_PRICE");
    assert!(
        body.get("reduceOnly")
            .unwrap()
            .clone()
            .decode::<bool>()
            .unwrap()
    );
    body.insert("quantity".into(), captured(json!("0")));
    assert_eq!(text(&body, "quantity"), "0");
    assert_eq!(
        text(&stop_input, "quantity"),
        "0003.2500",
        "WOO-ALGO-REQUEST native retained input independence"
    );
    let mut unsupported = stop();
    unsupported["triggerPriceType"] = json!("INDEX_PRICE");
    unsupported["quantity"] = json!("0");
    assert!(matches!(
        build_woo_v3_algo_place_request(&map(unsupported)).unwrap(),
        WooV3AlgoPlaceBuildResult::Blocked {
            blocker: "only-market-price-trigger-is-documented",
            ..
        }
    ));
    let mut trailing = common();
    trailing["family"] = json!("TRAILING_STOP");
    trailing["quantity"] = json!("4.125");
    trailing["callback"] = json!({"source":"WOO_NATIVE_CALLBACK_VALUE","value":".25"});
    trailing["activatedPrice"] = json!("97.5");
    let WooV3AlgoPlaceBuildResult::Ready {
        request,
        family,
        trigger_semantics,
    } = build_woo_v3_algo_place_request(&map(trailing)).unwrap()
    else {
        panic!("WOO-ALGO-REQUEST native trailing ready")
    };
    assert!(matches!(family, WooV3AlgoPlaceFamily::TrailingStop));
    assert_eq!(
        trigger_semantics,
        "vendor-native-callback-value-no-safe-cex-conversion"
    );
    assert_eq!(request.method, WooV3HttpMethod::Post);
    assert_eq!(request.path, WooV3Endpoint::TradeAlgoOrder);
    assert!(request.query.is_none());
    let body = request.body.unwrap();
    common_body(&body);
    assert_eq!(body.len(), 12);
    assert_eq!(
        text(&body, "algoType"),
        "TRAILING_STOP",
        "WOO-ALGO-REQUEST native trailing root type"
    );
    assert_eq!(text(&body, "side"), "BUY");
    assert_eq!(text(&body, "quantity"), "4.125");
    assert_eq!(text(&body, "marginMode"), "CROSS");
    assert_eq!(text(&body, "type"), "MARKET");
    assert!(
        body.get("reduceOnly")
            .unwrap()
            .clone()
            .decode::<bool>()
            .unwrap()
    );
    assert_eq!(text(&body, "callbackValue"), ".25");
    assert_eq!(text(&body, "activatedPrice"), "97.5");
    let mut positional = common();
    positional["family"] = json!("POSITIONAL_TP_SL");
    positional["takeProfit"] = json!({"triggerPrice":"121.25","triggerPriceType":"MARKET_PRICE"});
    positional["stopLoss"] = json!({"triggerPrice":"81.5","triggerPriceType":"MARKET_PRICE"});
    let WooV3AlgoPlaceBuildResult::Ready {
        request,
        family,
        trigger_semantics,
    } = build_woo_v3_algo_place_request(&map(positional)).unwrap()
    else {
        panic!("WOO-ALGO-REQUEST native positional ready")
    };
    assert!(matches!(family, WooV3AlgoPlaceFamily::PositionalTpSl));
    assert_eq!(trigger_semantics, "official-last-price-positional-children");
    assert_eq!(request.method, WooV3HttpMethod::Post);
    assert_eq!(request.path, WooV3Endpoint::TradeAlgoOrder);
    assert!(request.query.is_none());
    let body = request.body.unwrap();
    common_body(&body);
    assert_eq!(body.len(), 7);
    assert_eq!(
        text(&body, "algoType"),
        "POSITIONAL_TP_SL",
        "WOO-ALGO-REQUEST native positional root type"
    );
    assert_eq!(text(&body, "marginMode"), "CROSS");
    assert!(body.get("side").is_none());
    let children = body
        .get("childOrders")
        .unwrap()
        .clone()
        .decode::<Vec<JsObjectMap<JsNumberInput>>>()
        .unwrap();
    assert_eq!(children.len(), 2);
    for (i, kind, price) in [(0, "TAKE_PROFIT", "121.25"), (1, "STOP_LOSS", "81.5")] {
        assert_eq!(children[i].len(), 5);
        assert_eq!(
            text(&children[i], "algoType"),
            kind,
            "WOO-ALGO-REQUEST native child order"
        );
        assert_eq!(text(&children[i], "triggerPrice"), price);
        assert_eq!(text(&children[i], "side"), "BUY");
        assert_eq!(text(&children[i], "type"), "CLOSE_POSITION");
        assert_eq!(text(&children[i], "triggerPriceType"), "MARKET_PRICE");
    }
    let ids = map(json!({"algoOrderId":71,"clientAlgoOrderId":83}));
    assert!(target_matches_order(
        &WooV3AlgoWriteTarget::AlgoOrderId(71.0),
        &ids
    ));
    assert!(target_matches_order(
        &WooV3AlgoWriteTarget::ClientAlgoOrderId(83.0),
        &ids
    ));
    assert!(!target_matches_order(
        &WooV3AlgoWriteTarget::AlgoOrderId(7.0),
        &ids
    ));
    assert!(!target_matches_order(
        &WooV3AlgoWriteTarget::AlgoOrderId(71.0),
        &map(json!({"algoOrderId":"71"}))
    ));
    assert!(target_matches_order(
        &WooV3AlgoWriteTarget::AlgoOrderId(-0.0),
        &map(json!({"algoOrderId":0}))
    ));
    let infinite: JsObjectMap<JsNumberInput> = js_from_str(r#"{"algoOrderId":1e999}"#).unwrap();
    assert!(target_matches_order(
        &WooV3AlgoWriteTarget::AlgoOrderId(f64::INFINITY),
        &infinite
    ));
    assert!(!target_matches_order(
        &WooV3AlgoWriteTarget::AlgoOrderId(f64::NAN),
        &infinite
    ));
    let WooV3AlgoAmendBuildResult::Ready {
        request,
        target,
        update_semantics,
    } = build_woo_v3_algo_amend_request(&map(amend(order()))).unwrap()
    else {
        panic!("WOO-ALGO-REQUEST native amend ready")
    };
    assert_eq!(request.method, WooV3HttpMethod::Put);
    assert_eq!(request.path, WooV3Endpoint::TradeAlgoOrder);
    assert!(matches!(target,WooV3AlgoWriteTarget::AlgoOrderId(n) if n==71.0));
    assert_eq!(update_semantics, "untriggered-zero-fill-stop-trigger-only");
    let body = request.body.unwrap();
    assert_eq!(body.len(), 3);
    assert_eq!(text(&body, "triggerPrice"), "102.5");
    assert_eq!(text(&body, "triggerPriceType"), "MARKET_PRICE");
    assert_eq!(
        body.get("algoOrderId")
            .unwrap()
            .clone()
            .decode::<f64>()
            .unwrap()
            .to_bits(),
        71.0f64.to_bits()
    );
    let mut partial = order();
    partial["totalExecutedQuantity"] = json!("1");
    partial["algoStatus"] = json!("FILLED");
    blocked(
        amend(partial),
        "partial-fill-algo-amend-semantics-require-demo-evidence",
    );
    let mut mismatched = order();
    mismatched["algoOrderId"] = json!(99);
    mismatched["totalExecutedQuantity"] = json!("1");
    blocked(amend(mismatched), "target-id-mismatch");
    let mut unchanged = amend(order());
    unchanged["update"]["triggerPrice"] = json!("00101.2500");
    assert_eq!(
        build_woo_v3_algo_amend_request(&map(unchanged)).unwrap_err(),
        "WOO V3 algo amend must change triggerPrice"
    );
    let input = map(json!({"clientAlgoOrderId":83}));
    let mut cancel = build_woo_v3_algo_cancel_request(&input).unwrap();
    assert_eq!(cancel.method, WooV3HttpMethod::Delete);
    assert_eq!(cancel.path, WooV3Endpoint::TradeAlgoOrder);
    assert!(cancel.body.is_none());
    let query = cancel.query.as_mut().unwrap();
    assert_eq!(query.len(), 1);
    assert_eq!(
        query
            .get("clientAlgoOrderId")
            .unwrap()
            .clone()
            .decode::<f64>()
            .unwrap()
            .to_bits(),
        83.0f64.to_bits()
    );
    query.insert("clientAlgoOrderId".into(), captured(json!(89)));
    assert_eq!(
        input
            .get("clientAlgoOrderId")
            .unwrap()
            .clone()
            .decode::<f64>()
            .unwrap()
            .to_bits(),
        83.0f64.to_bits()
    );
    let WooV3AlgoCancelPendingBuildResult::Ready { request, scope } =
        build_woo_v3_algo_cancel_pending_request(None).unwrap()
    else {
        panic!("WOO-ALGO-REQUEST native pending ready")
    };
    assert_eq!(scope, "all-algo-orders-all-symbols");
    assert_eq!(request.method, WooV3HttpMethod::Delete);
    assert_eq!(
        request.path,
        WooV3Endpoint::TradeAlgoOrders,
        "WOO-ALGO-REQUEST native pending plural endpoint"
    );
    assert!(request.query.is_none() && request.body.is_none());
    let WooV3AlgoCancelPendingBuildResult::Blocked { blocker, symbol } =
        build_woo_v3_algo_cancel_pending_request(Some("PERP_SOL_USDT")).unwrap()
    else {
        panic!("WOO-ALGO-REQUEST native scope blocker")
    };
    assert_eq!(
        blocker,
        "symbol-scoped-pending-cancel-query-schema-conflicts-with-description"
    );
    assert_eq!(symbol, "PERP_SOL_USDT");
    let tag_raw = stop().to_string().replace("audit-19", r"\ud800");
    assert_eq!(
        retained_input_observations::text_at(&tag_raw, &["algoOrderTag"]),
        vec![0xd800],
        "WOO-ALGO-REQUEST native retained tag units"
    );
    let comparison_raw = amend(order()).to_string().replace(
        r#""totalExecutedQuantity":"0""#,
        r#""totalExecutedQuantity":"\ud800""#,
    );
    assert!(comparison_raw.contains(r"\ud800"));
    assert_eq!(
        retained_input_observations::text_at(
            &comparison_raw,
            &["currentOrder", "totalExecutedQuantity"]
        ),
        vec![0xd800],
        "WOO-ALGO-REQUEST native retained stored units"
    );
    for (raw, expected) in [(r"\ud800\udc00", "\u{10000}"), (r"\\ud800", r"\ud800")] {
        let input: JsObjectMap<JsNumberInput> =
            js_from_str(&stop().to_string().replace("audit-19", raw)).unwrap();
        let WooV3AlgoPlaceBuildResult::Ready { request, .. } =
            build_woo_v3_algo_place_request(&input).unwrap()
        else {
            panic!("WOO-ALGO-REQUEST native Unicode positive")
        };
        assert_eq!(text(&request.body.unwrap(), "algoOrderTag"), expected);
    }
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/algo-writes-oracle.cjs"
        ))
        .arg("--request-audit")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "WOO-ALGO-REQUEST source observations: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let observations: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(observations["groups"], 8);
    assert_eq!(observations["loneUnitRows"], 3);
}
#[test]
fn native_algo_response_choices_are_visible_before_source_projection() {
    let gateway: JsObjectMap<JsNumberInput> = js_from_str(r#"{"nested":{"value":81}}"#).unwrap();
    let mut error = WooV3AlgoWriteResponseError::new("probe".into(), &gateway, Some(-0.0));
    assert_eq!(
        error.name, "WooV3AlgoWriteResponseError",
        "WOO-ALGO-CHOICE native error name"
    );
    assert_eq!(error.message, "probe");
    assert_eq!(error.contract_status, "vendor-error-envelope-undocumented");
    assert_eq!(error.code.unwrap().to_bits(), (-0_f64).to_bits());
    error
        .gateway_response
        .insert("added".into(), js_from_str("89").unwrap());
    assert!(
        !gateway.contains_key("added"),
        "WOO-ALGO-CHOICE native independent mutable gateway snapshot"
    );
    let envelope: JsNumberInput =
        js_from_str(r#"{"success":true,"timestamp":-0,"data":{"nested":{"value":81}}}"#).unwrap();
    let mut success = decode_success_data(Some(&envelope), "probe").unwrap();
    assert_eq!(success.timestamp.to_bits(), (-0_f64).to_bits());
    success
        .data
        .insert("added".into(), js_from_str("89").unwrap());
    let mut row=decode_place_row(Some(&js_from_str(r#"{"algoOrderId":81,"clientAlgoOrderId":83,"algoType":"STOP","quantity":"3.25000"}"#).unwrap()),"probe").unwrap();
    assert_eq!(row.algo_order_id.to_bits(), 81_f64.to_bits());
    assert_eq!(row.client_algo_order_id.to_bits(), 83_f64.to_bits());
    assert!(matches!(row.algo_type, WooV3AlgoPlaceType::Stop));
    assert_eq!(row.quantity, "3.25000");
    row.quantity = "0".into();
    assert_eq!(
        row.quantity, "0",
        "WOO-ALGO-CHOICE native mutable decoded row"
    );
    let target = js_from_str(r#"{"algoOrderId":83}"#).unwrap();
    for mut value in [
        decode_woo_v3_algo_amend_response(Some(&envelope), &target).unwrap(),
        decode_woo_v3_algo_cancel_response(Some(&envelope), &target).unwrap(),
    ] {
        assert!(value.accepted);
        assert_eq!(value.timestamp.to_bits(), (-0_f64).to_bits());
        let WooV3AlgoWriteTarget::AlgoOrderId(id) = value.target else {
            panic!("WOO-ALGO-CHOICE native target variant")
        };
        assert_eq!(id.to_bits(), 83_f64.to_bits());
        assert_eq!(
            value.identity_evidence,
            "request-target-only-response-does-not-echo-id-or-status"
        );
        let nested: JsObjectMap<JsNumberInput> = value
            .gateway_data
            .get("nested")
            .unwrap()
            .clone()
            .decode()
            .unwrap();
        assert_eq!(
            nested
                .get("value")
                .unwrap()
                .clone()
                .decode::<f64>()
                .unwrap()
                .to_bits(),
            81_f64.to_bits()
        );
        value
            .gateway_data
            .insert("added".into(), js_from_str("89").unwrap());
    }
    let mut pending = decode_woo_v3_algo_cancel_pending_response(Some(&envelope)).unwrap();
    assert!(pending.accepted);
    assert_eq!(pending.scope, "all-algo-orders-all-symbols");
    assert_eq!(pending.timestamp.to_bits(), (-0_f64).to_bits());
    assert!(
        !pending.gateway_data.contains_key("added"),
        "WOO-ALGO-CHOICE native independent mutation result"
    );
    pending
        .gateway_data
        .insert("added".into(), js_from_str("89").unwrap());
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/algo-writes-oracle.cjs"
        ))
        .arg("--boundary-audit")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source["cases"], 6,
        "WOO-ALGO-CHOICE source floor ends at pending response"
    );
}
#[test]
fn live_algo_membership_follows_complete_row_decode_and_precedes_duplicates() {
    use std::{cell::RefCell, rc::Rc};
    let build = WooV3AlgoPlaceBuildResult::Ready {
        request: WooV3RequestInput {
            method: WooV3HttpMethod::Post,
            path: WooV3Endpoint::TradeAlgoOrder,
            query: None,
            body: Some(js_from_str(r#"{"quantity":"3.25","clientAlgoOrderId":83}"#).unwrap()),
        },
        family: WooV3AlgoPlaceFamily::StopMarket,
        trigger_semantics: "official-last-price-trigger",
    };
    let first =
        r#"{"algoOrderId":81,"clientAlgoOrderId":83,"algoType":"STOP","quantity":"3.25000"}"#;
    let second =
        r#"{"algoOrderId":89,"clientAlgoOrderId":91,"algoType":"STOP_LOSS","quantity":"0"}"#;
    let response = |rows: &str, timestamp: &str| -> JsNumberInput {
        js_from_str(&format!(
            r#"{{"success":true,"timestamp":{timestamp},"data":{{"rows":[{rows}]}}}}"#
        ))
        .unwrap()
    };
    let calls = RefCell::new(Vec::new());
    let actual =
        decode_woo_v3_algo_place_response_with(Some(&response(first, "97")), &build, |id| {
            calls.borrow_mut().push(id);
            Ok::<_, Rc<String>>(false)
        })
        .expect("WOO-ALGO-MEMBERSHIP native false override must allow known ID");
    let WooV3AlgoPlaceDecodeResult::Ready {
        row,
        timestamp,
        identity_evidence,
    } = actual
    else {
        panic!("WOO-ALGO-MEMBERSHIP native ready variant");
    };
    assert_eq!(
        *calls.borrow(),
        vec![81.0],
        "WOO-ALGO-MEMBERSHIP native one lookup"
    );
    for (value, expected) in [
        (row.algo_order_id, 81_f64),
        (row.client_algo_order_id, 83_f64),
        (timestamp, 97_f64),
    ] {
        assert_eq!(
            value.to_bits(),
            expected.to_bits(),
            "WOO-ALGO-MEMBERSHIP native decoded number"
        );
    }
    assert!(matches!(row.algo_type, WooV3AlgoPlaceType::Stop));
    assert_eq!(row.quantity, "3.25000");
    assert_eq!(identity_evidence, "response-echoed-algo-order-id");
    let validation =
        |result: Result<WooV3AlgoPlaceDecodeResult, WooV3AlgoPlaceResponseError<Rc<String>>>,
         expected: &str| {
            let WooV3AlgoPlaceResponseError::Response(WooV3AlgoWriteError::Validation(message)) =
                result.unwrap_err()
            else {
                panic!("WOO-ALGO-MEMBERSHIP native validation classification");
            };
            assert_eq!(
                message, expected,
                "WOO-ALGO-MEMBERSHIP native first refusal"
            );
        };
    calls.borrow_mut().clear();
    validation(
        decode_woo_v3_algo_place_response_with(
            Some(&response(&format!("{first},{first}"), "97")),
            &build,
            |id| {
                calls.borrow_mut().push(id);
                Ok(calls.borrow().len() == 2)
            },
        ),
        "WOO V3 algo place response reused a known algoOrderId",
    );
    assert_eq!(
        *calls.borrow(),
        vec![81.0, 81.0],
        "WOO-ALGO-MEMBERSHIP native known before duplicate"
    );
    calls.borrow_mut().clear();
    validation(
        decode_woo_v3_algo_place_response_with(
            Some(&response(&format!("{first},{first}"), "97")),
            &build,
            |id| {
                calls.borrow_mut().push(id);
                Ok(false)
            },
        ),
        "WOO V3 algo place response repeated an algoOrderId",
    );
    assert_eq!(
        *calls.borrow(),
        vec![81.0, 81.0],
        "WOO-ALGO-MEMBERSHIP native duplicate still calls known"
    );
    let mut positional = build.clone();
    let WooV3AlgoPlaceBuildResult::Ready { family, .. } = &mut positional else {
        unreachable!()
    };
    *family = WooV3AlgoPlaceFamily::PositionalTpSl;
    let sentinel = Rc::new("original algo membership".to_owned());
    calls.borrow_mut().clear();
    let error = decode_woo_v3_algo_place_response_with(
        Some(&response(&format!("{first},{second}"), "97")),
        &positional,
        |id| {
            calls.borrow_mut().push(id);
            if calls.borrow().len() == 2 {
                Err(sentinel.clone())
            } else {
                Ok(false)
            }
        },
    )
    .unwrap_err();
    let WooV3AlgoPlaceResponseError::Membership(original) = error else {
        panic!("WOO-ALGO-MEMBERSHIP native error before positional blocker");
    };
    assert!(
        Rc::ptr_eq(&original, &sentinel),
        "WOO-ALGO-MEMBERSHIP native original error identity"
    );
    assert_eq!(*calls.borrow(), vec![81.0, 89.0]);
    let malformed = second.replace("\"quantity\":\"0\"", "\"quantity\":\"bad\"");
    calls.borrow_mut().clear();
    validation(
        decode_woo_v3_algo_place_response_with(
            Some(&response(&format!("{first},{malformed}"), "97")),
            &build,
            |id| {
                calls.borrow_mut().push(id);
                Err(sentinel.clone())
            },
        ),
        "WOO V3 place algo order.data.rows[1].quantity must be a finite non-negative decimal string",
    );
    assert!(
        calls.borrow().is_empty(),
        "WOO-ALGO-MEMBERSHIP native decode all rows before lookup"
    );
    validation(
        decode_woo_v3_algo_place_response_with(Some(&response("", "97")), &build, |id| {
            calls.borrow_mut().push(id);
            Err(sentinel.clone())
        }),
        "WOO V3 place algo order.data.rows must not be empty",
    );
    let mut wrong = build.clone();
    let WooV3AlgoPlaceBuildResult::Ready { request, .. } = &mut wrong else {
        unreachable!()
    };
    request.method = WooV3HttpMethod::Get;
    validation(
        decode_woo_v3_algo_place_response_with(Some(&response(first, "97")), &wrong, |id| {
            calls.borrow_mut().push(id);
            Err(sentinel.clone())
        }),
        "WOO V3 algo place response requires its matching place request",
    );
    assert!(
        calls.borrow().is_empty(),
        "WOO-ALGO-MEMBERSHIP native earlier refusal suppresses lookup"
    );
    let blocked = decode_woo_v3_algo_place_response_with(
        Some(&response(&format!("{first},{second}"), "-0")),
        &positional,
        |id| {
            calls.borrow_mut().push(id);
            Ok::<_, Rc<String>>(false)
        },
    )
    .unwrap();
    let WooV3AlgoPlaceDecodeResult::Blocked {
        blocker,
        rows,
        timestamp,
    } = blocked
    else {
        panic!("WOO-ALGO-MEMBERSHIP native positional blocker after lookup");
    };
    assert_eq!(
        blocker,
        "positional-response-parent-child-identity-requires-demo-evidence"
    );
    assert_eq!(*calls.borrow(), vec![81.0, 89.0]);
    assert_eq!(rows.len(), 2);
    assert_eq!(timestamp.to_bits(), (-0_f64).to_bits());
    for (row, id, client, quantity) in [
        (&rows[0], 81_f64, 83_f64, "3.25000"),
        (&rows[1], 89_f64, 91_f64, "0"),
    ] {
        assert_eq!(row.algo_order_id.to_bits(), id.to_bits());
        assert_eq!(row.client_algo_order_id.to_bits(), client.to_bits());
        assert_eq!(row.quantity, quantity);
    }
    assert!(matches!(rows[0].algo_type, WooV3AlgoPlaceType::Stop));
    assert!(matches!(rows[1].algo_type, WooV3AlgoPlaceType::StopLoss));
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/algo-writes-oracle.cjs"
        ))
        .arg("--membership-audit")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source["cases"], 9,
        "WOO-ALGO-MEMBERSHIP source floor ends at live has getter"
    );
}
fn arg(row: &Value, i: usize) -> Option<JsNumberInput> {
    row["args"][i].as_str().map(|s| js_from_str(s).unwrap())
}
fn record(v: &JsNumberInput) -> JsObjectMap<JsNumberInput> {
    v.clone().decode().unwrap()
}
fn number(a: f64, e: &Value, label: &str) {
    assert_eq!(
        a.to_bits(),
        e["$number"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap()
            .to_bits(),
        "ALGO-WRITES native {label}"
    );
}
fn input(a: &JsNumberInput, e: &Value) {
    if e.get("$number").is_some() {
        number(a.clone().decode::<f64>().unwrap(), e, "captured number");
    } else {
        match e {
            Value::Null => assert!(
                a.clone().decode::<Option<bool>>().unwrap().is_none(),
                "ALGO-WRITES native null"
            ),
            Value::Bool(b) => assert_eq!(
                a.clone().decode::<bool>().unwrap(),
                *b,
                "ALGO-WRITES native captured boolean"
            ),
            Value::String(s) => assert_eq!(
                a.clone().decode::<String>().unwrap(),
                *s,
                "ALGO-WRITES native captured string"
            ),
            Value::Array(xs) => {
                let vs = a.clone().decode::<Vec<JsNumberInput>>().unwrap();
                assert_eq!(vs.len(), xs.len(), "ALGO-WRITES native array length");
                for (v, x) in vs.iter().zip(xs) {
                    input(v, x);
                }
            }
            Value::Object(_) => map(&record(a), e),
            _ => panic!("number must use oracle marker"),
        }
    }
}
fn map(a: &JsObjectMap<JsNumberInput>, e: &Value) {
    let object = e.as_object().unwrap();
    assert_eq!(a.len(), object.len(), "ALGO-WRITES native key count");
    for (k, v) in object {
        input(
            a.get(k)
                .unwrap_or_else(|| panic!("ALGO-WRITES native missing key {k}")),
            v,
        );
    }
}
fn checked<T>(r: Result<T, String>, e: &Value, f: impl FnOnce(T, &Value)) {
    match e.get("error") {
        Some(error) => {
            assert!(r.is_err(), "ALGO-WRITES native expected refusal");
            assert_eq!(
                r.err().unwrap(),
                error["message"].as_str().unwrap(),
                "ALGO-WRITES native first error"
            );
            assert_eq!(error["name"], "Error");
        }
        None => {
            assert!(
                r.is_ok(),
                "ALGO-WRITES native expected success {r:?}",
                r = r.as_ref().map(|_| ())
            );
            f(r.ok().unwrap(), &e["ok"]);
        }
    }
}
fn write_checked<T>(r: Result<T, WooV3AlgoWriteError>, e: &Value, f: impl FnOnce(T, &Value)) {
    if let Some(error) = e.get("error") {
        match r.err().expect("ALGO-WRITES native expected write refusal") {
            WooV3AlgoWriteError::Validation(s) => {
                assert_eq!(
                    error["name"], "Error",
                    "ALGO-WRITES native validation class"
                );
                assert_eq!(s, error["message"], "ALGO-WRITES native first write error");
            }
            WooV3AlgoWriteError::Gateway(g) => {
                assert_eq!(g.name, error["name"], "ALGO-WRITES native gateway name");
                assert_eq!(
                    g.message, error["message"],
                    "ALGO-WRITES native gateway message"
                );
                assert_eq!(
                    g.contract_status, error["contractStatus"],
                    "ALGO-WRITES native gateway contract status"
                );
                assert_eq!(
                    g.code.is_some(),
                    error.get("code").is_some(),
                    "ALGO-WRITES native gateway code presence"
                );
                if let Some(n) = g.code {
                    number(n, &error["code"], "gateway code");
                }
                map(&g.gateway_response, &error["gatewayResponse"]);
            }
        }
    } else {
        assert!(r.is_ok(), "ALGO-WRITES native expected write success");
        f(r.ok().unwrap(), &e["ok"]);
    }
}
fn side(e: &Value) -> WooV3PositionSide {
    match e.as_str().unwrap() {
        "BOTH" => WooV3PositionSide::Both,
        "LONG" => WooV3PositionSide::Long,
        "SHORT" => WooV3PositionSide::Short,
        _ => panic!("side"),
    }
}
fn expected_woo_v3_order_side(s: &str) -> WooV3OrderSide {
    match s {
        "BUY" => WooV3OrderSide::Buy,
        "SELL" => WooV3OrderSide::Sell,
        _ => panic!("source enum value"),
    }
}
fn expected_woo_v3_order_status(s: &str) -> WooV3OrderStatus {
    match s {
        "NEW" => WooV3OrderStatus::New,
        "CANCELLED" => WooV3OrderStatus::Cancelled,
        "PARTIAL_FILLED" => WooV3OrderStatus::PartialFilled,
        "FILLED" => WooV3OrderStatus::Filled,
        "REJECTED" => WooV3OrderStatus::Rejected,
        _ => panic!("source enum value"),
    }
}
fn expected_woo_v3_regular_order_type(s: &str) -> WooV3RegularOrderType {
    match s {
        "LIMIT" => WooV3RegularOrderType::Limit,
        "MARKET" => WooV3RegularOrderType::Market,
        "IOC" => WooV3RegularOrderType::Ioc,
        "POST_ONLY" => WooV3RegularOrderType::PostOnly,
        "AC" => WooV3RegularOrderType::Ac,
        "FOK" => WooV3RegularOrderType::Fok,
        "LIQUIDATE" => WooV3RegularOrderType::Liquidate,
        "BID" => WooV3RegularOrderType::Bid,
        "ASK" => WooV3RegularOrderType::Ask,
        "RPI" => WooV3RegularOrderType::Rpi,
        "LIQUIDATE_BLP" => WooV3RegularOrderType::LiquidateBlp,
        "ADL" => WooV3RegularOrderType::Adl,
        _ => panic!("source enum value"),
    }
}
fn expected_woo_v3_algo_type(s: &str) -> WooV3AlgoType {
    match s {
        "STOP" => WooV3AlgoType::Stop,
        "OCO" => WooV3AlgoType::Oco,
        "TRAILING_STOP" => WooV3AlgoType::TrailingStop,
        "POSITIONAL_TP_SL" => WooV3AlgoType::PositionalTpSl,
        "TP_SL" => WooV3AlgoType::TpSl,
        "BRACKET" => WooV3AlgoType::Bracket,
        "STOP_BRACKET" => WooV3AlgoType::StopBracket,
        "TAKE_PROFIT" => WooV3AlgoType::TakeProfit,
        "STOP_LOSS" => WooV3AlgoType::StopLoss,
        _ => panic!("source enum value"),
    }
}
fn expected_woo_v3_trigger_price_type(s: &str) -> WooV3TriggerPriceType {
    match s {
        "MARKET_PRICE" => WooV3TriggerPriceType::MarketPrice,
        "INDEX_PRICE" => WooV3TriggerPriceType::IndexPrice,
        "LAST_PRICE" => WooV3TriggerPriceType::LastPrice,
        _ => panic!("source enum value"),
    }
}
fn expected_woo_v3_trigger_status(s: &str) -> WooV3TriggerStatus {
    match s {
        "USELESS" => WooV3TriggerStatus::Useless,
        "TRIGGERED" => WooV3TriggerStatus::Triggered,
        "CANCELLED" => WooV3TriggerStatus::Cancelled,
        "SUCCESS" => WooV3TriggerStatus::Success,
        _ => panic!("source enum value"),
    }
}
fn expected_woo_v3_margin_mode(s: &str) -> WooV3MarginMode {
    match s {
        "ISOLATED" => WooV3MarginMode::Isolated,
        "CROSSED" => WooV3MarginMode::Crossed,
        _ => panic!("source enum value"),
    }
}
fn quantity(v: &WooV3GatewayAlgoQuantity, e: &Value) {
    match v {
        WooV3GatewayAlgoQuantity::Decimal(s) => assert_eq!(
            Some(s.as_str()),
            e.as_str(),
            "ALGO-WRITES native quantity variant/text"
        ),
        WooV3GatewayAlgoQuantity::Number(n) => number(*n, e, "quantity bits"),
    }
}
fn inspect_woo_v3_algo_order(v: &WooV3AlgoOrder, e: &Value) {
    number(v.algo_order_id, &e["algoOrderId"], "algoOrderId");
    number(
        v.client_algo_order_id,
        &e["clientAlgoOrderId"],
        "clientAlgoOrderId",
    );
    number(
        v.root_algo_order_id,
        &e["rootAlgoOrderId"],
        "rootAlgoOrderId",
    );
    number(
        v.parent_algo_order_id,
        &e["parentAlgoOrderId"],
        "parentAlgoOrderId",
    );
    assert_eq!(
        &v.symbol,
        e["symbol"].as_str().unwrap(),
        "ALGO-WRITES native symbol"
    );
    assert_eq!(
        &v.algo_order_tag,
        e["algoOrderTag"].as_str().unwrap(),
        "ALGO-WRITES native algoOrderTag"
    );
    assert_eq!(
        v.algo_type,
        expected_woo_v3_algo_type(e["algoType"].as_str().unwrap()),
        "ALGO-WRITES native algoType"
    );
    assert_eq!(
        v.side,
        expected_woo_v3_order_side(e["side"].as_str().unwrap()),
        "ALGO-WRITES native side"
    );
    assert_eq!(v.position_side, side(&e["positionSide"]));
    quantity(&v.quantity, &e["quantity"]);
    assert_eq!(
        v.is_triggered,
        e["isTriggered"].as_bool().unwrap(),
        "ALGO-WRITES native isTriggered"
    );
    assert_eq!(
        v.trigger_status,
        expected_woo_v3_trigger_status(e["triggerStatus"].as_str().unwrap()),
        "ALGO-WRITES native triggerStatus"
    );
    assert_eq!(
        v.r#type.is_some(),
        e.get("type").is_some(),
        "ALGO-WRITES native type presence"
    );
    if let Some(value) = &v.r#type {
        assert_eq!(
            *value,
            expected_woo_v3_regular_order_type(e["type"].as_str().unwrap()),
            "ALGO-WRITES native type"
        );
    }
    assert_eq!(
        v.root_algo_status,
        expected_woo_v3_order_status(e["rootAlgoStatus"].as_str().unwrap()),
        "ALGO-WRITES native rootAlgoStatus"
    );
    assert_eq!(
        v.algo_status,
        expected_woo_v3_order_status(e["algoStatus"].as_str().unwrap()),
        "ALGO-WRITES native algoStatus"
    );
    assert_eq!(
        v.trigger_price_type,
        expected_woo_v3_trigger_price_type(e["triggerPriceType"].as_str().unwrap()),
        "ALGO-WRITES native triggerPriceType"
    );
    assert_eq!(
        &v.trigger_time,
        e["triggerTime"].as_str().unwrap(),
        "ALGO-WRITES native triggerTime"
    );
    assert_eq!(
        v.trigger_price.is_some(),
        e.get("triggerPrice").is_some(),
        "ALGO-WRITES native triggerPrice presence"
    );
    if let Some(value) = &v.trigger_price {
        assert_eq!(
            value,
            e["triggerPrice"].as_str().unwrap(),
            "ALGO-WRITES native triggerPrice"
        );
    }
    assert_eq!(
        v.price.is_some(),
        e.get("price").is_some(),
        "ALGO-WRITES native price presence"
    );
    if let Some(value) = &v.price {
        assert_eq!(
            value,
            e["price"].as_str().unwrap(),
            "ALGO-WRITES native price"
        );
    }
    assert_eq!(
        v.visible_quantity.is_some(),
        e.get("visibleQuantity").is_some(),
        "ALGO-WRITES native visibleQuantity presence"
    );
    if let Some(value) = &v.visible_quantity {
        assert_eq!(
            value,
            e["visibleQuantity"].as_str().unwrap(),
            "ALGO-WRITES native visibleQuantity"
        );
    }
    assert_eq!(
        &v.total_executed_quantity,
        e["totalExecutedQuantity"].as_str().unwrap(),
        "ALGO-WRITES native totalExecutedQuantity"
    );
    assert_eq!(
        &v.average_executed_price,
        e["averageExecutedPrice"].as_str().unwrap(),
        "ALGO-WRITES native averageExecutedPrice"
    );
    assert_eq!(
        &v.total_fee,
        e["totalFee"].as_str().unwrap(),
        "ALGO-WRITES native totalFee"
    );
    assert_eq!(
        &v.fee_asset,
        e["feeAsset"].as_str().unwrap(),
        "ALGO-WRITES native feeAsset"
    );
    number(v.total_rebate, &e["totalRebate"], "totalRebate");
    assert_eq!(
        &v.rebate_currency,
        e["rebateCurrency"].as_str().unwrap(),
        "ALGO-WRITES native rebateCurrency"
    );
    assert_eq!(
        v.reduce_only,
        e["reduceOnly"].as_bool().unwrap(),
        "ALGO-WRITES native reduceOnly"
    );
    number(v.created_time, &e["createdTime"], "createdTime");
    number(v.updated_time, &e["updatedTime"], "updatedTime");
    assert_eq!(
        v.is_activated,
        e["isActivated"].as_bool().unwrap(),
        "ALGO-WRITES native isActivated"
    );
    assert_eq!(
        v.callback_rate.is_some(),
        e.get("callbackRate").is_some(),
        "ALGO-WRITES native callbackRate presence"
    );
    if let Some(value) = &v.callback_rate {
        assert_eq!(
            value,
            e["callbackRate"].as_str().unwrap(),
            "ALGO-WRITES native callbackRate"
        );
    }
    assert_eq!(
        v.activated_price.is_some(),
        e.get("activatedPrice").is_some(),
        "ALGO-WRITES native activatedPrice presence"
    );
    if let Some(value) = &v.activated_price {
        assert_eq!(
            value,
            e["activatedPrice"].as_str().unwrap(),
            "ALGO-WRITES native activatedPrice"
        );
    }
    assert_eq!(
        v.leverage.is_some(),
        e.get("leverage").is_some(),
        "ALGO-WRITES native leverage presence"
    );
    if let Some(value) = &v.leverage {
        number(*value, &e["leverage"], "leverage");
    }
    assert_eq!(
        v.margin_mode.is_some(),
        e.get("marginMode").is_some(),
        "ALGO-WRITES native marginMode presence"
    );
    if let Some(value) = &v.margin_mode {
        assert_eq!(
            *value,
            expected_woo_v3_margin_mode(e["marginMode"].as_str().unwrap()),
            "ALGO-WRITES native marginMode"
        );
    }
    let rows = e["childOrders"].as_array().unwrap();
    assert_eq!(
        v.child_orders.len(),
        rows.len(),
        "ALGO-WRITES native childOrders count"
    );
    for (v, e) in v.child_orders.iter().zip(rows) {
        inspect_woo_v3_algo_order(v, e);
    }
}
fn target(v: &WooV3AlgoWriteTarget, e: &Value) {
    match v {
        WooV3AlgoWriteTarget::AlgoOrderId(n) => {
            assert!(
                e.get("algoOrderId").is_some(),
                "ALGO-WRITES native target variant"
            );
            number(*n, &e["algoOrderId"], "algoOrderId");
        }
        WooV3AlgoWriteTarget::ClientAlgoOrderId(n) => {
            assert!(
                e.get("clientAlgoOrderId").is_some(),
                "ALGO-WRITES native client target variant"
            );
            number(*n, &e["clientAlgoOrderId"], "clientAlgoOrderId");
        }
    }
}
fn family(e: &Value) -> WooV3AlgoPlaceFamily {
    match e.as_str().unwrap() {
        "STOP_MARKET" => WooV3AlgoPlaceFamily::StopMarket,
        "TRAILING_STOP" => WooV3AlgoPlaceFamily::TrailingStop,
        "POSITIONAL_TP_SL" => WooV3AlgoPlaceFamily::PositionalTpSl,
        _ => panic!("source family"),
    }
}
fn trigger_type(e: &Value) -> WooV3AlgoTriggerPriceType {
    match e.as_str().unwrap() {
        "MARKET_PRICE" => WooV3AlgoTriggerPriceType::MarketPrice,
        "MARK_PRICE" => WooV3AlgoTriggerPriceType::MarkPrice,
        "INDEX_PRICE" => WooV3AlgoTriggerPriceType::IndexPrice,
        "LAST_PRICE" => WooV3AlgoTriggerPriceType::LastPrice,
        _ => panic!("source trigger"),
    }
}
fn place_type(e: &Value) -> WooV3AlgoPlaceType {
    match e.as_str().unwrap() {
        "STOP" => WooV3AlgoPlaceType::Stop,
        "TRAILING_STOP" => WooV3AlgoPlaceType::TrailingStop,
        "POSITIONAL_TP_SL" => WooV3AlgoPlaceType::PositionalTpSl,
        "TAKE_PROFIT" => WooV3AlgoPlaceType::TakeProfit,
        "STOP_LOSS" => WooV3AlgoPlaceType::StopLoss,
        _ => panic!("source row type"),
    }
}
fn request(v: &WooV3RequestInput, e: &Value) {
    assert_eq!(
        v.method,
        match e["method"].as_str().unwrap() {
            "POST" => WooV3HttpMethod::Post,
            "PUT" => WooV3HttpMethod::Put,
            "DELETE" => WooV3HttpMethod::Delete,
            _ => panic!("source method"),
        },
        "ALGO-WRITES native request method"
    );
    assert_eq!(
        v.path,
        match e["path"].as_str().unwrap() {
            "/v3/trade/algoOrder" => WooV3Endpoint::TradeAlgoOrder,
            "/v3/trade/algoOrders" => WooV3Endpoint::TradeAlgoOrders,
            _ => panic!("source endpoint"),
        },
        "ALGO-WRITES native request endpoint"
    );
    for (v, e) in [(&v.body, e.get("body")), (&v.query, e.get("query"))] {
        assert_eq!(
            v.is_some(),
            e.is_some(),
            "ALGO-WRITES native request field presence"
        );
        if let (Some(v), Some(e)) = (v, e) {
            map(v, e);
        }
    }
}
fn place_row(v: &WooV3AlgoPlaceResponseRow, e: &Value) {
    number(v.algo_order_id, &e["algoOrderId"], "row algoOrderId");
    number(
        v.client_algo_order_id,
        &e["clientAlgoOrderId"],
        "row clientAlgoOrderId",
    );
    assert_eq!(
        v.algo_type,
        place_type(&e["algoType"]),
        "ALGO-WRITES native row algoType"
    );
    assert_eq!(v.quantity, e["quantity"], "ALGO-WRITES native row quantity");
}
fn place_build(v: WooV3AlgoPlaceBuildResult, e: &Value) {
    match v {
        WooV3AlgoPlaceBuildResult::Ready {
            request: r,
            family: f,
            trigger_semantics,
        } => {
            assert_eq!(e["ready"], true, "ALGO-WRITES native place ready variant");
            request(&r, &e["request"]);
            assert_eq!(f, family(&e["family"]), "ALGO-WRITES native ready family");
            assert_eq!(
                trigger_semantics, e["triggerSemantics"],
                "ALGO-WRITES native trigger semantics"
            );
        }
        WooV3AlgoPlaceBuildResult::Blocked { blocker, family: f } => {
            assert_eq!(
                e["ready"], false,
                "ALGO-WRITES native place blocked variant"
            );
            assert_eq!(blocker, e["blocker"], "ALGO-WRITES native place blocker");
            assert_eq!(f, family(&e["family"]), "ALGO-WRITES native blocked family");
        }
    }
}
fn mutation(v: &WooV3AlgoMutationResult, e: &Value) {
    assert_eq!(
        v.accepted,
        e["accepted"].as_bool().unwrap(),
        "ALGO-WRITES native accepted"
    );
    target(&v.target, &e["target"]);
    number(v.timestamp, &e["timestamp"], "mutation timestamp");
    assert_eq!(
        v.identity_evidence, e["identityEvidence"],
        "ALGO-WRITES native mutation identity"
    );
    map(&v.gateway_data, &e["gatewayData"]);
}
fn pending(v: &WooV3AlgoCancelPendingResult, e: &Value) {
    assert_eq!(
        v.accepted,
        e["accepted"].as_bool().unwrap(),
        "ALGO-WRITES native pending accepted"
    );
    assert_eq!(v.scope, e["scope"], "ALGO-WRITES native pending scope");
    number(v.timestamp, &e["timestamp"], "pending timestamp");
    map(&v.gateway_data, &e["gatewayData"]);
}
#[test]
fn native_validators() {
    let s = source();
    let rows = s["validators"].as_array().unwrap();
    assert_eq!(rows.len(), 105, "ALGO-WRITES validator floor");
    for e in rows {
        let a = arg(e, 0);
        let b = arg(e, 1);
        match e["name"].as_str().unwrap() {
            "requireRecord" => checked(require_record(a.as_ref(), "probe"), e, |v, x| map(&v, x)),
            "requireBoolean" => checked(require_boolean(a.as_ref(), "probe"), e, |v, x| {
                assert_eq!(
                    v,
                    x.as_bool().unwrap(),
                    "ALGO-WRITES native validator boolean"
                )
            }),
            "requireInteger" => checked(
                require_integer(a.as_ref(), "probe", b.unwrap().decode::<f64>().unwrap()),
                e,
                |v, x| number(v, x, "validator integer"),
            ),
            "requireString" => checked(require_string(a.as_ref(), "probe"), e, |v, x| {
                assert_eq!(
                    v,
                    x.as_str().unwrap(),
                    "ALGO-WRITES native validator string"
                )
            }),
            "requireEnum" => checked(require_enum(a.as_ref(), &["A", "B"], "probe"), e, |v, x| {
                assert_eq!(v, x.as_str().unwrap(), "ALGO-WRITES native validator enum")
            }),
            "requireDecimal" => checked(
                require_decimal(a.as_ref(), "probe", b.unwrap().decode::<bool>().unwrap()),
                e,
                |v, x| {
                    assert_eq!(
                        v,
                        x.as_str().unwrap(),
                        "ALGO-WRITES native validator decimal"
                    )
                },
            ),
            "decimalParts" => {
                let (l, r) = decimal_parts(&a.unwrap().decode::<String>().unwrap());
                assert_eq!(l, e["ok"][0], "ALGO-WRITES native decimal whole");
                assert_eq!(r, e["ok"][1], "ALGO-WRITES native decimal fraction");
            }
            "compareDecimals" => number(
                f64::from(compare_decimals(
                    &a.unwrap().decode::<String>().unwrap(),
                    &b.unwrap().decode::<String>().unwrap(),
                )),
                &e["ok"],
                "decimal comparison",
            ),
            "requirePerpetualSymbol" => checked(
                require_perpetual_symbol(&a.unwrap().decode::<String>().unwrap()),
                e,
                |v, x| assert_eq!(v, x.as_str().unwrap(), "ALGO-WRITES native symbol"),
            ),
            "validateTarget" => checked(validate_target(&record(&a.unwrap())), e, |v, x| {
                target(&v, x)
            }),
            "requireArray" => checked(require_array(a.as_ref(), "probe"), e, |v, x| {
                let xs = x.as_array().unwrap();
                assert_eq!(v.len(), xs.len(), "ALGO-WRITES native helper array length");
                for (a, e) in v.iter().zip(xs) {
                    input(a, e);
                }
            }),
            "hasUnsupportedTrigger" => {
                let v = a.unwrap().decode::<WooV3AlgoTriggerPriceType>().unwrap();
                assert_eq!(
                    has_unsupported_trigger(v),
                    e["ok"].as_bool().unwrap(),
                    "ALGO-WRITES native unsupported trigger"
                );
            }
            "validateCommon" => {
                checked(validate_common(&record(&a.unwrap())), e, |v, x| map(&v, x))
            }
            "targetMatchesOrder" => {
                let v = a.unwrap().decode::<WooV3AlgoWriteTarget>().unwrap();
                assert_eq!(
                    target_matches_order(&v, &record(&b.unwrap())),
                    e["ok"].as_bool().unwrap(),
                    "ALGO-WRITES native target match"
                );
            }
            _ => panic!("source helper"),
        }
    }
}
#[test]
fn native_place_builders() {
    let s = source();
    let rows = s["places"].as_array().unwrap();
    assert_eq!(rows.len(), 31, "ALGO-WRITES place floor");
    for e in rows {
        checked(
            build_woo_v3_algo_place_request(&record(&arg(e, 0).unwrap())),
            e,
            place_build,
        );
    }
}
#[test]
fn native_amend_builders() {
    let s = source();
    let rows = s["amends"].as_array().unwrap();
    assert_eq!(rows.len(), 32, "ALGO-WRITES amend floor");
    for e in rows {
        checked(
            build_woo_v3_algo_amend_request(&record(&arg(e, 0).unwrap())),
            e,
            |v, x| match v {
                WooV3AlgoAmendBuildResult::Ready {
                    request: r,
                    target: t,
                    update_semantics,
                } => {
                    assert_eq!(x["ready"], true, "ALGO-WRITES native amend ready variant");
                    request(&r, &x["request"]);
                    target(&t, &x["target"]);
                    assert_eq!(
                        update_semantics, x["updateSemantics"],
                        "ALGO-WRITES native update semantics"
                    );
                }
                WooV3AlgoAmendBuildResult::Blocked { blocker, target: t } => {
                    assert_eq!(
                        x["ready"], false,
                        "ALGO-WRITES native amend blocked variant"
                    );
                    assert_eq!(
                        blocker, x["blocker"],
                        "ALGO-WRITES native first amend blocker"
                    );
                    target(&t, &x["target"]);
                }
            },
        );
    }
}
#[test]
fn native_cancel_builders() {
    let s = source();
    let rows = s["cancels"].as_array().unwrap();
    assert_eq!(rows.len(), 9, "ALGO-WRITES cancel floor");
    for e in rows {
        let a = arg(e, 0);
        if e["name"] == "single" {
            checked(
                build_woo_v3_algo_cancel_request(&record(&a.unwrap())),
                e,
                |v, x| request(&v, x),
            );
        } else {
            let symbol = match a.as_ref() {
                None => None,
                Some(v) if v.clone().decode::<String>().ok().as_deref() == Some("all") => None,
                Some(v) => Some(
                    record(v)
                        .get("symbol")
                        .unwrap()
                        .clone()
                        .decode::<String>()
                        .unwrap(),
                ),
            };
            checked(
                build_woo_v3_algo_cancel_pending_request(symbol.as_deref()),
                e,
                |v, x| match v {
                    WooV3AlgoCancelPendingBuildResult::Ready { request: r, scope } => {
                        assert_eq!(x["ready"], true, "ALGO-WRITES native cancel ready variant");
                        request(&r, &x["request"]);
                        assert_eq!(scope, x["scope"], "ALGO-WRITES native cancel all scope");
                    }
                    WooV3AlgoCancelPendingBuildResult::Blocked { blocker, symbol } => {
                        assert_eq!(
                            x["ready"], false,
                            "ALGO-WRITES native cancel blocked variant"
                        );
                        assert_eq!(blocker, x["blocker"], "ALGO-WRITES native cancel blocker");
                        assert_eq!(
                            symbol, x["scope"]["symbol"],
                            "ALGO-WRITES native cancel symbol"
                        );
                    }
                },
            );
        }
    }
}
#[test]
fn native_response_decoders() {
    let s = source();
    let rows = s["successes"].as_array().unwrap();
    assert_eq!(rows.len(), 10, "ALGO-WRITES success floor");
    for e in rows {
        write_checked(
            decode_success_data(arg(e, 0).as_ref(), "probe"),
            e,
            |v, x| {
                map(&v.data, &x["data"]);
                number(v.timestamp, &x["timestamp"], "envelope timestamp");
            },
        );
    }
    let rows = s["placeRows"].as_array().unwrap();
    assert_eq!(rows.len(), 8, "ALGO-WRITES row floor");
    for e in rows {
        checked(decode_place_row(arg(e, 0).as_ref(), "probe"), e, |v, x| {
            place_row(&v, x)
        });
    }
    let rows = s["placeResponses"].as_array().unwrap();
    assert_eq!(rows.len(), 17, "ALGO-WRITES response floor");
    for e in rows {
        let b = record(&arg(e, 1).unwrap());
        let r = b
            .get("request")
            .unwrap()
            .clone()
            .decode::<WooV3RequestInput>()
            .unwrap();
        let f = b
            .get("family")
            .unwrap()
            .clone()
            .decode::<WooV3AlgoPlaceFamily>()
            .unwrap();
        let b = WooV3AlgoPlaceBuildResult::Ready {
            request: r,
            family: f,
            trigger_semantics: "not inspected by source decoder",
        };
        let ids = arg(e, 2).unwrap().decode::<Vec<f64>>().unwrap();
        write_checked(
            decode_woo_v3_algo_place_response(arg(e, 0).as_ref(), &b, &ids),
            e,
            |v, x| match v {
                WooV3AlgoPlaceDecodeResult::Ready {
                    row,
                    timestamp,
                    identity_evidence,
                } => {
                    assert_eq!(x["ready"], true, "ALGO-WRITES native decode ready variant");
                    place_row(&row, &x["row"]);
                    number(timestamp, &x["timestamp"], "place timestamp");
                    assert_eq!(
                        identity_evidence, x["identityEvidence"],
                        "ALGO-WRITES native place identity"
                    );
                }
                WooV3AlgoPlaceDecodeResult::Blocked {
                    blocker,
                    rows,
                    timestamp,
                } => {
                    assert_eq!(
                        x["ready"], false,
                        "ALGO-WRITES native decode blocked variant"
                    );
                    assert_eq!(blocker, x["blocker"], "ALGO-WRITES native decode blocker");
                    let xs = x["rows"].as_array().unwrap();
                    assert_eq!(rows.len(), xs.len(), "ALGO-WRITES native decoded row count");
                    for (a, e) in rows.iter().zip(xs) {
                        place_row(a, e);
                    }
                    number(timestamp, &x["timestamp"], "blocked timestamp");
                }
            },
        );
    }
    let rows = s["mutations"].as_array().unwrap();
    assert_eq!(rows.len(), 22, "ALGO-WRITES mutation floor");
    for e in rows {
        let a = arg(e, 0);
        if e["name"] == "pending" {
            write_checked(
                decode_woo_v3_algo_cancel_pending_response(a.as_ref()),
                e,
                |v, x| pending(&v, x),
            );
        } else {
            let t = record(&arg(e, 1).unwrap());
            let r = match e["name"].as_str().unwrap() {
                "amend" => decode_woo_v3_algo_amend_response(a.as_ref(), &t),
                "cancel" => decode_woo_v3_algo_cancel_response(a.as_ref(), &t),
                "direct" => decode_mutation(a.as_ref(), "probe", &t),
                _ => panic!("mutation kind"),
            };
            write_checked(r, e, |v, x| mutation(&v, x));
        }
    }
    let mut gateway: JsObjectMap<JsNumberInput> =
        js_from_str("{\"message\":\"original\",\"nested\":{\"x\":1},\"n\":1e999}").unwrap();
    let v = WooV3AlgoWriteResponseError::new("direct-59".into(), &gateway, Some(-0.0));
    let e = &s["directError"];
    assert_eq!(v.name, e["name"], "ALGO-WRITES native direct error name");
    assert_eq!(
        v.message, e["message"],
        "ALGO-WRITES native direct error message"
    );
    assert_eq!(
        v.contract_status, e["contractStatus"],
        "ALGO-WRITES native direct error contract"
    );
    number(v.code.unwrap(), &e["code"], "direct error code");
    map(&v.gateway_response, &e["gatewayResponse"]);
    gateway.insert("message".into(), js_from_str("\"changed\"").unwrap());
    assert_eq!(
        v.gateway_response
            .get("message")
            .unwrap()
            .clone()
            .decode::<String>()
            .unwrap(),
        "original",
        "ALGO-WRITES native error snapshot"
    );
}
fn common_input(
    symbol: &str,
    side: WooV3OrderSide,
    mode: WooV3AccountPositionMode,
    id: Option<f64>,
    tag: Option<&str>,
    e: &Value,
) {
    assert_eq!(symbol, e["symbol"], "ALGO-WRITES native declaration symbol");
    assert_eq!(
        side,
        expected_woo_v3_order_side(e["side"].as_str().unwrap()),
        "ALGO-WRITES native declaration side"
    );
    assert_eq!(
        mode,
        if e["positionMode"] == "ONE_WAY" {
            WooV3AccountPositionMode::OneWay
        } else {
            WooV3AccountPositionMode::HedgeMode
        },
        "ALGO-WRITES native declaration mode"
    );
    assert_eq!(
        id.is_some(),
        e.get("clientAlgoOrderId").is_some(),
        "ALGO-WRITES native declaration ID presence"
    );
    if let Some(n) = id {
        number(n, &e["clientAlgoOrderId"], "declaration clientAlgoOrderId");
    }
    assert_eq!(
        tag,
        e["algoOrderTag"].as_str(),
        "ALGO-WRITES native declaration tag"
    );
}
fn stop_input(v: &WooV3StopMarketPlaceInput, e: &Value) {
    common_input(
        &v.symbol,
        v.side,
        v.position_mode,
        v.client_algo_order_id,
        v.algo_order_tag.as_deref(),
        e,
    );
    assert_eq!(v.family, e["family"], "ALGO-WRITES native stop family");
    assert_eq!(
        v.quantity, e["quantity"],
        "ALGO-WRITES native declaration quantity"
    );
    assert_eq!(
        v.trigger_price, e["triggerPrice"],
        "ALGO-WRITES native declaration triggerPrice"
    );
    assert_eq!(
        v.trigger_price_type,
        trigger_type(&e["triggerPriceType"]),
        "ALGO-WRITES native declaration triggerPriceType"
    );
}
fn trailing_input(v: &WooV3TrailingPlaceInput, e: &Value) {
    common_input(
        &v.symbol,
        v.side,
        v.position_mode,
        v.client_algo_order_id,
        v.algo_order_tag.as_deref(),
        e,
    );
    assert_eq!(v.family, e["family"], "ALGO-WRITES native trailing family");
    assert_eq!(
        v.quantity, e["quantity"],
        "ALGO-WRITES native trailing quantity"
    );
    assert_eq!(
        v.callback.source, e["callback"]["source"],
        "ALGO-WRITES native callback source"
    );
    assert_eq!(
        v.callback.value, e["callback"]["value"],
        "ALGO-WRITES native callback value"
    );
    assert_eq!(
        v.activated_price.as_deref(),
        e["activatedPrice"].as_str(),
        "ALGO-WRITES native activatedPrice"
    );
}
fn trigger_input(v: &WooV3PositionalTriggerInput, e: &Value) {
    assert_eq!(
        v.trigger_price, e["triggerPrice"],
        "ALGO-WRITES native positional triggerPrice"
    );
    assert_eq!(
        v.trigger_price_type,
        trigger_type(&e["triggerPriceType"]),
        "ALGO-WRITES native positional trigger type"
    );
}
fn positional_input(v: &WooV3PositionalPlaceInput, e: &Value) {
    common_input(
        &v.symbol,
        v.side,
        v.position_mode,
        v.client_algo_order_id,
        v.algo_order_tag.as_deref(),
        e,
    );
    assert_eq!(
        v.family, e["family"],
        "ALGO-WRITES native positional family"
    );
    for (v, e) in [
        (&v.take_profit, e.get("takeProfit")),
        (&v.stop_loss, e.get("stopLoss")),
    ] {
        assert_eq!(
            v.is_some(),
            e.is_some(),
            "ALGO-WRITES native trigger presence"
        );
        if let (Some(v), Some(e)) = (v, e) {
            trigger_input(v, e);
        }
    }
}
#[test]
fn native_declarations() {
    let s = source();
    let rows = s["declarations"].as_array().unwrap();
    assert_eq!(rows.len(), 14, "ALGO-WRITES declaration floor");
    for e in rows {
        let a: JsNumberInput = js_from_str(e["input"].as_str().unwrap()).unwrap();
        let x = &e["expected"];
        match e["type"].as_str().unwrap() {
            "stop" => stop_input(&a.decode::<WooV3StopMarketPlaceInput>().unwrap(), x),
            "trailing" => trailing_input(&a.decode::<WooV3TrailingPlaceInput>().unwrap(), x),
            "positional" => positional_input(&a.decode::<WooV3PositionalPlaceInput>().unwrap(), x),
            "trigger" => trigger_input(&a.decode::<WooV3PositionalTriggerInput>().unwrap(), x),
            "row" => place_row(&a.decode::<WooV3AlgoPlaceResponseRow>().unwrap(), x),
            "amend" => {
                let v = a.decode::<WooV3AlgoAmendInput>().unwrap();
                inspect_woo_v3_algo_order(&v.current_order, &x["currentOrder"]);
                target(&v.target, &x["target"]);
                assert_eq!(
                    v.update.trigger_price.as_deref(),
                    x["update"]["triggerPrice"].as_str(),
                    "ALGO-WRITES native update triggerPrice"
                );
                assert_eq!(
                    v.update.trigger_price_type,
                    Some(trigger_type(&x["update"]["triggerPriceType"])),
                    "ALGO-WRITES native update triggerPriceType"
                );
                assert_eq!(
                    v.update.quantity.as_deref(),
                    x["update"]["quantity"].as_str(),
                    "ALGO-WRITES native update quantity"
                );
                assert_eq!(
                    v.update.callback_value.as_deref(),
                    x["update"]["callbackValue"].as_str(),
                    "ALGO-WRITES native update callbackValue"
                );
            }
            "mutation" => mutation(&a.decode::<WooV3AlgoMutationResult>().unwrap(), x),
            "pending" => pending(&a.decode::<WooV3AlgoCancelPendingResult>().unwrap(), x),
            "target" => target(&a.decode::<WooV3AlgoWriteTarget>().unwrap(), x),
            "union" => match a.decode::<WooV3AlgoPlaceInput>().unwrap() {
                WooV3AlgoPlaceInput::StopMarket(v) => {
                    assert_eq!(
                        x["family"], "STOP_MARKET",
                        "ALGO-WRITES native union stop variant"
                    );
                    stop_input(&v, x);
                }
                WooV3AlgoPlaceInput::TrailingStop(v) => {
                    assert_eq!(
                        x["family"], "TRAILING_STOP",
                        "ALGO-WRITES native union trailing variant"
                    );
                    trailing_input(&v, x);
                }
                WooV3AlgoPlaceInput::PositionalTpSl(v) => {
                    assert_eq!(
                        x["family"], "POSITIONAL_TP_SL",
                        "ALGO-WRITES native union positional variant"
                    );
                    positional_input(&v, x);
                }
            },
            _ => panic!("declaration kind"),
        }
    }
    let rows = s["declarationRefusals"].as_array().unwrap();
    assert_eq!(rows.len(), 9, "ALGO-WRITES declaration refusal floor");
    for e in rows {
        let a: JsNumberInput = js_from_str(e["input"].as_str().unwrap()).unwrap();
        let bad = match e["type"].as_str().unwrap() {
            "stop" => a.decode::<WooV3StopMarketPlaceInput>().is_err(),
            "trailing" => a.decode::<WooV3TrailingPlaceInput>().is_err(),
            "positional" => a.decode::<WooV3PositionalPlaceInput>().is_err(),
            "mutation" => a.decode::<WooV3AlgoMutationResult>().is_err(),
            "pending" => a.decode::<WooV3AlgoCancelPendingResult>().is_err(),
            "target" => a.decode::<WooV3AlgoWriteTarget>().is_err(),
            "union" => a.decode::<WooV3AlgoPlaceInput>().is_err(),
            _ => panic!("refusal kind"),
        };
        assert!(bad, "ALGO-WRITES native declaration refusal");
    }
    for text in ["[]", "{}"] {
        assert!(
            js_from_str::<WooV3AlgoPlaceInput>(text).is_err(),
            "ALGO-WRITES native root union refusal"
        );
        assert!(
            js_from_str::<WooV3StopMarketPlaceInput>(text).is_err(),
            "ALGO-WRITES native root record refusal"
        );
    }
    let variants = [
        WooV3AlgoTriggerPriceType::MarketPrice,
        WooV3AlgoTriggerPriceType::MarkPrice,
        WooV3AlgoTriggerPriceType::IndexPrice,
        WooV3AlgoTriggerPriceType::LastPrice,
    ];
    let values = s["triggerTypes"].as_array().unwrap();
    assert_eq!(values.len(), 4, "ALGO-WRITES trigger union floor");
    for (e, v) in variants.iter().zip(values) {
        let a: JsNumberInput = js_from_str(&serde_json::to_string(v).unwrap()).unwrap();
        assert_eq!(
            a.decode::<WooV3AlgoTriggerPriceType>().unwrap(),
            *e,
            "ALGO-WRITES native declared trigger enum"
        );
    }
    let p = WOO_V3_ALGO_WRITE_PARITY;
    let e = &s["parity"];
    assert_eq!(
        p.request_and_response_fixtures,
        e["requestAndResponseFixtures"].as_bool().unwrap()
    );
    assert_eq!(p.runtime_imported, e["runtimeImported"].as_bool().unwrap());
    assert_eq!(
        p.credentialed_demo_lifecycle,
        e["credentialedDemoLifecycle"].as_bool().unwrap()
    );
    assert_eq!(p.trigger_price_type, e["triggerPriceType"]);
    assert_eq!(p.trailing_input, e["trailingInput"]);
    assert_eq!(
        p.positional_response_identity,
        e["positionalResponseIdentity"]
    );
    assert_eq!(p.positional_update, e["positionalUpdate"]);
    assert_eq!(
        p.partial_fill_and_quantity_amend,
        e["partialFillAndQuantityAmend"]
    );
    assert_eq!(p.symbol_pending_cancel, e["symbolPendingCancel"]);
    assert_eq!(p.cancel_response_identity, e["cancelResponseIdentity"]);
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;
