use safe_cex_venue_coinbase::{
    starbase::CoinbaseStarbaseProtocolError, starbase_orders::*,
    starbase_private::decode_coinbase_starbase_order,
};
use serde_json::{Value, json};
use std::{process::Command, sync::OnceLock};
#[path = "support/capture_metadata.rs"]
mod capture_metadata;
#[path = "support/compare.rs"]
mod comparison;
#[path = "support/private_projection.rs"]
mod private_projection;
#[path = "support/special_numbers.rs"]
mod special_numbers;
fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/write-results.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
        capture_metadata::assert_capture_metadata(&oracle, 22);
        oracle
    })
}
fn project(result: &CoinbaseStarbaseWriteResult) -> Value {
    let mut value = json!(result);
    // Verify actual envelope serialization before replacing numeric projections:
    // missing/reordered/narrowed nested output must not be repaired by this test.
    comparison::compare(
        &value["order"],
        &json!(result.order),
        "serialized nested order",
    );
    comparison::compare(
        &value["trades"],
        &json!(result.trades),
        "serialized nested trades",
    );
    value["order"] = private_projection::project_order(&result.order);
    value["trades"] = Value::Array(
        result
            .trades
            .iter()
            .map(private_projection::project_trade)
            .collect(),
    );
    value
}
fn capture(result: Result<CoinbaseStarbaseWriteResult, CoinbaseStarbaseProtocolError>) -> Value {
    match result {
        Ok(v) => json!({"ok":project(&v)}),
        Err(e) => json!({"error":e}),
    }
}
#[test]
fn write_result_decoders_live_validation_identity_and_serialization_order() {
    let identity = &oracle()["sharedOrderFixtureIdentitySupplement"];
    let decoded = decode_coinbase_starbase_write_result(Some(&identity["input"]))
        .expect("shared order fixture identity supplement");
    assert!(decoded.order.post_only, "shared fixture postOnly identity");
    assert_eq!(
        decoded.order.reduce_only,
        Some(false),
        "shared fixture reduceOnly identity"
    );
    for case in oracle()["cases"].as_array().unwrap() {
        let input = case
            .get("inputJson")
            .map(|v| serde_json::from_str::<Value>(v.as_str().unwrap()).unwrap());
        let result = match case["name"].as_str().unwrap() {
            "decodeCoinbaseStarbaseWriteResult" => {
                decode_coinbase_starbase_write_result(input.as_ref())
            }
            "decodeCoinbaseStarbaseCancelResult" => {
                decode_coinbase_starbase_cancel_result(input.as_ref())
            }
            name => panic!("unclassified decoder {name}"),
        };
        comparison::compare(&capture(result), &case["result"], &case.to_string());
    }
    let mut order = decode_coinbase_starbase_order(Some(&oracle()["orderBase"])).unwrap();
    order.id = "canonical".into();
    order.label = "correlation".into();
    for case in oracle()["identities"].as_array().unwrap() {
        let raw: Value = serde_json::from_str(case["rawJson"].as_str().unwrap()).unwrap();
        let result = match decode_order_identity(raw.as_object().unwrap(), &order) {
            Ok(v) => json!({"ok":v}),
            Err(e) => json!({"error":e}),
        };
        comparison::compare(&result, &case["result"], &case.to_string());
    }
    let supplement = &oracle()["sharedOrderFixtureIdentitySupplement"];
    assert_eq!(
        supplement["label"],
        "field-identity-supplement: starbase-orders fixture"
    );
    let decoded = decode_coinbase_starbase_write_result(Some(&supplement["input"]))
        .expect("shared order fixture identity supplement");
    assert!(decoded.order.post_only, "shared fixture postOnly identity");
    assert_eq!(
        decoded.order.reduce_only,
        Some(false),
        "shared fixture reduceOnly identity"
    );
    assert_eq!(
        decoded.order.gateway_amount,
        Some(0.14),
        "shared fixture amount identity"
    );
    assert_eq!(
        decoded.order.gateway_filled_amount,
        Some(0.03),
        "shared fixture filledAmount identity"
    );
    assert_eq!(
        decoded.order.creation_timestamp_ms, 1_800_000_000_123.0,
        "shared fixture creationTimestamp identity"
    );
    assert_eq!(
        decoded.order.last_update_timestamp_ms, 1_800_000_000_456.0,
        "shared fixture lastUpdateTimestamp identity"
    );
    comparison::compare(
        &capture(Ok(decoded)),
        &supplement["result"],
        "shared fixture identity supplement output",
    );
}
#[test]
fn write_result_assessment_live_blocker_precedence_float_bits_and_borrow() {
    for case in oracle()["assessments"].as_array().unwrap() {
        let trades = if case["hasTrade"].as_bool().unwrap() {
            json!([oracle()["tradeBase"]])
        } else {
            json!([])
        };
        let mut input = decode_coinbase_starbase_write_result(Some(
            &json!({"order":oracle()["orderBase"],"trades":trades}),
        ))
        .unwrap();
        input.order.gateway_filled_amount = case
            .get("amountBits")
            .map(|v| f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap()));
        let gate = assess_coinbase_starbase_write_result_parity(&input);
        let mut value = json!(gate);
        let (key, borrow) = match gate {
            CoinbaseStarbaseWriteGate::Ready { value } => ("value", value),
            CoinbaseStarbaseWriteGate::Blocked { intent, .. } => ("intent", intent),
        };
        assert!(std::ptr::eq(borrow, &input));
        comparison::compare(&value[key], &json!(borrow), "serialized assessor payload");
        value[key] = project(borrow);
        comparison::compare(&value, &case["result"], &case.to_string());
    }
}

#[test]
fn write_result_assessment_live_return_contract() {
    let file = syn::parse_file(include_str!("../src/starbase_orders.rs")).unwrap();
    let function = file
        .items
        .iter()
        .find_map(|item| match item {
            syn::Item::Fn(f) if f.sig.ident == "assess_coinbase_starbase_write_result_parity" => {
                Some(f)
            }
            _ => None,
        })
        .unwrap();
    let syn::ReturnType::Type(_, output) = &function.sig.output else {
        panic!("missing return type")
    };
    let syn::Type::Path(output) = output.as_ref() else {
        panic!("expected gate type")
    };
    assert_eq!(output.path.segments.len(), 1);
    let segment = output.path.segments.last().unwrap();
    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        panic!("missing gate arguments")
    };
    let args: Vec<String> = args
        .args
        .iter()
        .map(|arg| {
            let syn::GenericArgument::Type(syn::Type::Reference(reference)) = arg else {
                panic!("gate payload must borrow original")
            };
            assert!(reference.mutability.is_none());
            let syn::Type::Path(path) = reference.elem.as_ref() else {
                panic!("expected payload type")
            };
            assert_eq!(path.path.segments.len(), 1);
            path.path.segments.last().unwrap().ident.to_string()
        })
        .collect();
    assert_eq!(
        json!({"name":segment.ident.to_string(),"args":args}),
        oracle()["assessmentReturn"]
    );
}
