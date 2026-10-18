use safe_cex_venue_hyperliquid::signing::*;
use serde_json::{Value, json};

#[test]
fn audit_nonfinite_asset_survives_before_authenticated_encoding() {
    let order = json!({"is_buy":true,"limit_px":1,"sz":2,"reduce_only":false,"order_type":{"limit":{"tif":"Gtc"}}});
    let cases = [
        (
            f64::NAN,
            "0xe9f0a4e8af1e99db533347296421d20e94b3cdc8a9e601dd3bd5500c226cdda6",
        ),
        (
            f64::INFINITY,
            "0xeb2d01dc492c5009aa40613263fa5962739d64f1fe4e656eb8076fc9b0c0cda8",
        ),
        (
            f64::NEG_INFINITY,
            "0xe732171695eb794b06fdb333948b168d0ba33b7fdf80a65c89e8fe28e07db074",
        ),
    ];
    let mut completed = 0;
    for (asset, expected_hash) in cases {
        let wire = order_request_to_order_wire(&order, asset).unwrap();
        let view = wire.encoding_view();
        let safe_cex_num::JsJsonValue::Object(fields) = view.as_ref() else {
            panic!("AUDIT native prepared order object");
        };
        let Some((_, safe_cex_num::JsJsonValue::Number(native))) =
            fields.iter().find(|(key, _)| key == "a")
        else {
            panic!("AUDIT native asset remains Number before hashing");
        };
        let native = *native;
        assert!(
            native.to_bits() == asset.to_bits() || native.is_nan() && asset.is_nan(),
            "AUDIT native asset bits before wire"
        );
        assert_eq!(
            action_hash(&wire, None, &json!(1)).unwrap(),
            expected_hash,
            "AUDIT native nonfinite asset authenticated hash"
        );
        completed += 1;
    }
    assert_eq!(completed, 3);
}

#[test]
fn audit_json_array_user_action_signs_like_equivalent_indexed_object() {
    let key = format!("0x{}", "11".repeat(32));
    let fields = json!([{"name":"0","type":"bool"},{"name":"signatureChainId","type":"string"},{"name":"hyperliquidChain","type":"string"}]);
    let mut array = json!([false]);
    let signature = sign_user_signed_action(&key, &mut array, fields.clone(), "Case", true)
        .expect("AUDIT native JSON array action must reach wallet signing");
    assert!(array.is_array(), "AUDIT native action keeps array shape");
    assert_eq!(array[0], Value::Bool(false));
    let mut object = json!({"0":false});
    let expected = sign_user_signed_action(&key, &mut object, fields, "Case", true).unwrap();
    assert_eq!(signature.r, expected.r, "AUDIT native array signature r");
    assert_eq!(signature.s, expected.s, "AUDIT native array signature s");
    assert_eq!(signature.v, expected.v, "AUDIT native array signature v");
}

fn native_asset(value: &safe_cex_num::JsJsonValue) -> f64 {
    use safe_cex_num::JsJsonValue as J;
    let J::Object(fields) = value else {
        panic!("AUDIT native signed object");
    };
    if let Some((_, J::Number(asset))) = fields.iter().find(|(key, _)| key == "a") {
        return *asset;
    }
    let Some((_, J::Array(orders))) = fields.iter().find(|(key, _)| key == "orders") else {
        panic!("AUDIT native wrapped order array");
    };
    assert_eq!(orders.len(), 1, "AUDIT native wrapped order cardinality");
    native_asset(&orders[0])
}

#[test]
fn audit_source_signing_projection_native_values_before_all_wire() {
    let mut prepared = Vec::new();
    for asset in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.0] {
        for form in ["direct", "na", "normalTpsl", "positionTpsl"] {
            let input = json!({"is_buy":true,"limit_px":1,"sz":2,"reduce_only":false,"order_type":{"limit":{"tif":"Gtc"}}});
            let order = order_request_to_order_wire(&input, asset).unwrap();
            let action = match form {
                "direct" => order,
                "na" => order_wires_to_order_action(vec![order], None),
                "normalTpsl" => order_wires_to_normal_tpsl_action(vec![order], None),
                _ => order_wires_to_positional_tpsl_action(vec![order], None),
            };
            let actual = native_asset(&action.encoding_view());
            assert_eq!(
                actual.to_bits(),
                asset.to_bits(),
                "AUDIT native asset bits in vector/group {form}"
            );
            prepared.push(action);
        }
    }
    assert_eq!(
        prepared.len(),
        16,
        "AUDIT independent four assets by four forms"
    );
    let key = format!("0x{}", "11".repeat(32));
    let mut fields = json!([{"name":"0","type":"bool"},{"name":"signatureChainId","type":"string"},{"name":"hyperliquidChain","type":"string"}]);
    let expected = [
        (
            "0x6c4809f850d42ab9f17af75a0622d40960f7c8b061a6555854b3b99c6f542d0e",
            "0x51132346bdc2cdb2fb054e9ef311ee5c3f52c51851b039513542d956a12de667",
            28.,
        ),
        (
            "0xb43bfc231332112bcdf39263f132188dea5d8c12b97d1cfe4ac1d607eb1c5bd7",
            "0x3d8250dcdb088288184e4c4fbc5d13c5f5c41a7f766015f5345e3ef2c42c6788",
            28.,
        ),
        (
            "0x6a78aa307188573e477bf3f0a530fef917db1c541986a0183b5acd8180e69002",
            "0x12d972cc9d35ccf9409800f2911597f1edfa0555347ff7590e53f3898c5ef3fe",
            27.,
        ),
    ];
    let mut signatures = Vec::new();
    for (index, (r, s, v)) in expected.into_iter().enumerate() {
        if index == 1 {
            fields
                .as_array_mut()
                .unwrap()
                .push(json!({"name":"length","type":"uint256"}));
        }
        if index == 2 {
            fields
                .as_array_mut()
                .unwrap()
                .push(json!({"name":"constructor","type":"bool"}));
        }
        let mut action = json!([false]);
        let signature = sign_user_signed_action(&key, &mut action, fields.clone(), "Case", true)
            .expect(
                "AUDIT native array preserves indexed, length and inherited fields for signing",
            );
        let Value::Array(items) = &action else {
            panic!("AUDIT native signed action remains Array");
        };
        assert_eq!(items.len(), 1, "AUDIT native array length before wire");
        assert_eq!(
            items[0],
            Value::Bool(false),
            "AUDIT native indexed field before wire"
        );
        assert_eq!(
            signature.r, r,
            "AUDIT native array signature r before wire {index}"
        );
        assert_eq!(
            signature.s, s,
            "AUDIT native array signature s before wire {index}"
        );
        assert_eq!(
            signature.v, v,
            "AUDIT native array signature v before wire {index}"
        );
        signatures.push(signature);
    }
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/divergence-signing-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "AUDIT actual signing source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source["independentCount"]["total"], 19);
    let rows = source["observations"]["orders"].as_array().unwrap();
    assert_eq!(rows.len(), 16);
    for (index, action) in prepared.iter().enumerate() {
        let row = &rows[index];
        assert_eq!(
            action_hash(action, None, &json!(1)).unwrap(),
            row["hash"].as_str().unwrap(),
            "AUDIT actual source authenticated hash {index}"
        );
        let preimage = signing_preimage(action, None, &json!(1))
            .unwrap()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            preimage,
            row["preimage"].as_str().unwrap(),
            "AUDIT actual source authenticated preimage {index}"
        );
        assert_eq!(
            serde_json::to_string(action).unwrap(),
            row["wireJson"].as_str().unwrap(),
            "AUDIT JSON-only nonfinite projection {index}"
        );
    }
    assert_eq!(signatures.len(), 3);
}

#[test]
fn audit_prepared_limit_is_an_owned_snapshot_of_the_input() {
    use safe_cex_num::JsJsonValue as J;
    let mut input = json!({"is_buy":true,"limit_px":1,"sz":2,"reduce_only":false,"order_type":{"limit":{"tif":"Gtc"}}});
    let action = order_request_to_order_wire(&input, 7.).unwrap();
    input["order_type"]["limit"]["tif"] = "Alo".into();
    let view = action.encoding_view();
    let J::Object(fields) = view.as_ref() else {
        panic!("AUDIT native prepared object");
    };
    let (_, J::Object(kind)) = fields.iter().find(|(key, _)| key == "t").unwrap() else {
        panic!("AUDIT native order kind");
    };
    let (_, J::Object(limit)) = kind.iter().find(|(key, _)| key == "limit").unwrap() else {
        panic!("AUDIT native limit object");
    };
    assert_eq!(limit[0].0, "tif");
    assert_eq!(
        limit[0].1,
        J::String("Gtc".into()),
        "AUDIT chosen native snapshot does not follow input alias"
    );
    assert_eq!(input["order_type"]["limit"]["tif"], "Alo");
}

#[test]
fn audit_messagepack_source_depth_limit_is_enforced_before_signing() {
    fn nested(count: usize) -> Value {
        let mut value = json!(0);
        for _ in 0..count {
            value = Value::Array(vec![value]);
        }
        value
    }
    let accepted = nested(99);
    assert!(
        action_hash(&accepted, None, &json!(1)).is_ok(),
        "AUDIT native MessagePack value at depth100 accepted"
    );
    let rejected = nested(100);
    let error = action_hash(&rejected, None, &json!(1))
        .expect_err("AUDIT native MessagePack value at depth101 must refuse signing");
    assert_eq!(
        error.to_string(),
        "Too deep objects in depth 101",
        "AUDIT native MessagePack source depth error"
    );
}

#[test]
fn audit_array_wallet_view_preserves_length_and_inherited_constructor() {
    let fields = json!([{"name":"0","type":"bool"},{"name":"signatureChainId","type":"string"},{"name":"hyperliquidChain","type":"string"},{"name":"length","type":"uint256"},{"name":"constructor","type":"bool"}]);
    let mut action = json!([false]);
    let signature = sign_user_signed_action(
        &format!("0x{}", "11".repeat(32)),
        &mut action,
        fields,
        "Case",
        true,
    )
    .expect("AUDIT native array reflection reaches signer with length and inherited constructor");
    let Value::Array(items) = action else {
        panic!("AUDIT native action array retained");
    };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0], Value::Bool(false));
    assert_eq!(
        signature.r, "0x6a78aa307188573e477bf3f0a530fef917db1c541986a0183b5acd8180e69002",
        "AUDIT native reflected array signature r"
    );
    assert_eq!(
        signature.s, "0x12d972cc9d35ccf9409800f2911597f1edfa0555347ff7590e53f3898c5ef3fe",
        "AUDIT native reflected array signature s"
    );
    assert_eq!(signature.v.to_bits(), 27f64.to_bits());
}
