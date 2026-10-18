use safe_cex_num::{ProtoMember, TableHit, numeric_ingress::js_from_str};
use safe_cex_types::{OrderStatus, OrderType};
use safe_cex_venue_paradex::order_maps::{map_order_status, reverse_map_order_type};
use serde_json::{Value, json};
use std::process::Command;

fn status_text(value: OrderStatus) -> &'static str {
    match value {
        OrderStatus::Open => "open",
        OrderStatus::Closed => "closed",
        OrderStatus::Canceled => "canceled",
        OrderStatus::Creating => "creating",
        OrderStatus::Updating => "updating",
        OrderStatus::Canceling => "canceling",
    }
}
fn type_text(value: OrderType) -> &'static str {
    match value {
        OrderType::Market => "market",
        OrderType::Limit => "limit",
        OrderType::Conditional => "conditional",
        OrderType::StopLoss => "stop_market",
        OrderType::TakeProfit => "take_profit_market",
        OrderType::TrailingStopLoss => "trailing_stop_market",
    }
}
fn observe<T: Copy>(
    value: TableHit<T>,
    text: impl FnOnce(T) -> &'static str,
    expected: &Value,
    key: &str,
) -> String {
    let (kind, native_text, arity) = match value {
        TableHit::Mapped(v) => ("string", text(v), None),
        TableHit::Inherited(p) => (
            if p == ProtoMember::Prototype {
                "object"
            } else {
                "function"
            },
            p.js_string(),
            p.length(),
        ),
        TableHit::Missing => panic!("maps_missing_fallback {key}"),
    };
    assert_eq!(kind, expected["kind"], "maps_native_kind {key}");
    assert_eq!(native_text, expected["text"], "maps_native_text {key}");
    assert_eq!(json!(arity), expected["arity"], "maps_native_arity {key}");
    assert_eq!(
        expected.get("truthy"),
        Some(&json!(true)),
        "maps_native_truthy {key}"
    );
    // Save serialization expectations, but compare only after every native check.
    match kind {
        "function" => "{}".into(),
        "object" => "{\"value\":{}}".into(),
        _ => serde_json::to_string(&json!({"value":native_text})).unwrap(),
    }
}
#[test]
fn actual_typescript_order_maps() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/order_maps_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "maps_oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let keys = [
        "NEW",
        "OPEN",
        "PENDING",
        "UNTRIGGERED",
        "FILLED",
        "CANCELED",
        "REJECTED",
        "LIMIT",
        "MARKET",
        "STOP_LIMIT",
        "STOP_MARKET",
        "STOP_LOSS_LIMIT",
        "STOP_LOSS_MARKET",
        "TAKE_PROFIT_LIMIT",
        "TAKE_PROFIT_MARKET",
        "constructor",
        "__defineGetter__",
        "__defineSetter__",
        "hasOwnProperty",
        "__lookupGetter__",
        "__lookupSetter__",
        "isPrototypeOf",
        "propertyIsEnumerable",
        "toString",
        "valueOf",
        "__proto__",
        "toLocaleString",
        "",
        "new",
        "unknown",
        "é😀",
        "STOP_MARKET ",
    ];
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 32, "maps_case_count");
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        v => v,
    };
    assert_eq!(corpus["header"]["arch"], arch);
    assert!(corpus["header"]["nodeMajor"].as_u64().unwrap() > 0);
    let mut wires = Vec::new();
    let mut observed = 0;
    for (i, key) in keys.iter().enumerate() {
        let c = &cases[i];
        assert_eq!(
            c.get("key"),
            Some(&json!(key)),
            "maps_live_input_content {i}"
        );
        wires.push((
            observe(map_order_status(key), status_text, &c["status"], key),
            &c["status"]["wire"],
        ));
        observed += 1;
        let args = [None, Some(true), Some(false)];
        let source = c["types"].as_array().unwrap();
        assert_eq!(source.len(), 3, "maps_type_case_count");
        let native: [TableHit<OrderType>; 3] = [
            reverse_map_order_type(key, None),
            reverse_map_order_type(key, Some(true)),
            reverse_map_order_type(key, Some(false)),
        ];
        for j in 0..3 {
            assert_eq!(
                source[j].get("reduce"),
                Some(&json!(args[j])),
                "maps_live_reduce_content"
            );
            wires.push((
                observe(native[j], type_text, &source[j]["out"], key),
                &source[j]["out"]["wire"],
            ));
            observed += 1;
        }
    }
    assert_eq!(observed, 128, "maps_completed_native");
    assert_eq!(
        corpus["surrogate"]["status"]["text"], "open",
        "maps_source_surrogate_status"
    );
    assert_eq!(
        corpus["surrogate"]["type"]["text"], "limit",
        "maps_source_surrogate_type"
    );
    typed_utf16::assert_projection(
        corpus["surrogate"]["raw"].as_str().unwrap(),
        &(js_from_str::<String>(corpus["surrogate"]["raw"].as_str().unwrap()))
            .expect("typed surrogate record is retained"),
    );
    for (native, expected) in wires {
        assert_eq!(native, *expected, "maps_wire_after_native");
    }
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;
