use safe_cex_venue_kucoin::{
    account_mode::{KucoinAccountMode, KucoinAccountModeDetection, KucoinClassicRuntimeDecision},
    spot_types::{KucoinSpotWsServer, KucoinSpotWsTokenData, KucoinSpotWsTokenResponse},
    ws_helpers::KucoinPublicRole,
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

fn reject_array<T: DeserializeOwned>(value: Value) {
    assert!(
        serde_json::from_value::<T>(value.clone()).is_err(),
        "array admission must reject a complete valid sequence (owned)"
    );
    assert!(
        <T as serde::Deserialize>::deserialize(value.clone()).is_err(),
        "array admission must reject a complete valid sequence (qualified)"
    );
    assert!(
        <T as serde::Deserialize>::deserialize(&value).is_err(),
        "array admission must reject a complete valid sequence (borrowed)"
    );
    assert!(
        serde_json::from_str::<T>(&value.to_string()).is_err(),
        "array admission must reject a complete valid sequence (text)"
    );
}
fn reject_tags<T: DeserializeOwned>(tags: &[&str]) {
    for tag in tags {
        let value = json!({*tag: null});
        assert!(
            serde_json::from_value::<T>(value.clone()).is_err(),
            "tagged shape must be refused (owned): {tag}"
        );
        assert!(
            <T as serde::Deserialize>::deserialize(value.clone()).is_err(),
            "tagged shape must be refused (qualified): {tag}"
        );
        assert!(
            <T as serde::Deserialize>::deserialize(&value).is_err(),
            "tagged shape must be refused (borrowed): {tag}"
        );
        assert!(
            serde_json::from_str::<T>(&value.to_string()).is_err(),
            "tagged shape must be refused (text): {tag}"
        );
    }
}
fn server() -> Value {
    json!({"endpoint":"wss://native.invalid/a","encrypt":true,"protocol":"websocket","pingInterval":17.5,"pingTimeout":3.25})
}
fn data() -> Value {
    json!({"token":"native-token-a","instanceServers":[server()]})
}

#[test]
fn account_mode_string_shape() {
    assert_eq!(
        serde_json::from_str::<KucoinAccountMode>("\"CLASSIC\"").unwrap(),
        KucoinAccountMode::Classic
    );
    assert_eq!(
        serde_json::from_str::<KucoinAccountMode>("\"UNIFIED\"").unwrap(),
        KucoinAccountMode::Unified
    );
    reject_tags::<KucoinAccountMode>(&["CLASSIC", "UNIFIED"]);
}
#[test]
fn public_role_string_shape() {
    assert_eq!(
        serde_json::from_str::<KucoinPublicRole>("\"detail\"").unwrap(),
        KucoinPublicRole::Detail
    );
    assert_eq!(
        serde_json::from_str::<KucoinPublicRole>("\"snapshot\"").unwrap(),
        KucoinPublicRole::Snapshot
    );
    reject_tags::<KucoinPublicRole>(&["detail", "snapshot"]);
}
#[test]
fn spot_server_rejects_valid_sequence() {
    let first: KucoinSpotWsServer = serde_json::from_value(server()).unwrap();
    assert_eq!(first.endpoint, "wss://native.invalid/a");
    assert!(first.encrypt);
    assert_eq!(first.protocol, "websocket");
    assert_eq!(first.ping_interval.to_bits(), 17.5_f64.to_bits());
    assert_eq!(first.ping_timeout.to_bits(), 3.25_f64.to_bits());
    let second: KucoinSpotWsServer = serde_json::from_str(r#"{"endpoint":"wss://native.invalid/b","encrypt":false,"protocol":"native-b","pingInterval":-0,"pingTimeout":1e400}"#).unwrap();
    assert_eq!(second.endpoint, "wss://native.invalid/b");
    assert!(!second.encrypt);
    assert_eq!(second.protocol, "native-b");
    assert_eq!(second.ping_interval.to_bits(), (-0.0_f64).to_bits());
    assert_eq!(second.ping_timeout.to_bits(), f64::INFINITY.to_bits());
    reject_array::<KucoinSpotWsServer>(json!([
        "wss://native.invalid/a",
        true,
        "websocket",
        17.5,
        3.25
    ]));
}
#[test]
fn spot_token_data_rejects_valid_sequence() {
    let first: KucoinSpotWsTokenData = serde_json::from_value(data()).unwrap();
    assert_eq!(first.token, "native-token-a");
    assert_eq!(first.instance_servers.len(), 1);
    assert_eq!(first.instance_servers[0].endpoint, "wss://native.invalid/a");
    assert_eq!(
        first.instance_servers[0].ping_timeout.to_bits(),
        3.25_f64.to_bits()
    );
    let second: KucoinSpotWsTokenData =
        serde_json::from_value(json!({"token":"native-token-b","instanceServers":[]})).unwrap();
    assert_eq!(second.token, "native-token-b");
    assert!(second.instance_servers.is_empty());
    reject_array::<KucoinSpotWsTokenData>(json!(["native-token-a", [server()]]));
}
#[test]
fn spot_token_response_rejects_valid_sequence() {
    let first: KucoinSpotWsTokenResponse = serde_json::from_value(
        json!({"code":"native-code-a","data":data(),"msg":"native-message-a"}),
    )
    .unwrap();
    assert_eq!(first.code, "native-code-a");
    assert_eq!(first.msg.as_deref(), Some("native-message-a"));
    assert_eq!(first.data.as_ref().unwrap().token, "native-token-a");
    let second: KucoinSpotWsTokenResponse =
        serde_json::from_value(json!({"code":"native-code-b"})).unwrap();
    assert_eq!(second.code, "native-code-b");
    assert!(second.data.is_none());
    assert!(second.msg.is_none());
    assert!(
        serde_json::from_value::<KucoinSpotWsTokenResponse>(
            json!({"code":"native-code-b","data":null})
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<KucoinSpotWsTokenResponse>(
            json!({"code":"native-code-b","msg":null})
        )
        .is_err()
    );
    reject_array::<KucoinSpotWsTokenResponse>(json!(["native-code-a", data(), "native-message-a"]));
}
#[test]
fn classic_runtime_rejects_valid_sequence() {
    for (input, expected) in [
        (
            json!({"eligible":true,"reason":"classic-account-mode-confirmed","route":"existing-classic-runtime","utaRuntimeActivationEnabled":false}),
            KucoinClassicRuntimeDecision::Classic,
        ),
        (
            json!({"eligible":false,"reason":"unified-account-mode-unsupported","route":"blocked","utaRuntimeActivationEnabled":false}),
            KucoinClassicRuntimeDecision::Unified,
        ),
        (
            json!({"eligible":false,"reason":"account-mode-unknown","route":"blocked","utaRuntimeActivationEnabled":false}),
            KucoinClassicRuntimeDecision::Unknown,
        ),
        (
            json!({"eligible":false,"reason":"account-mode-response-malformed","route":"blocked","utaRuntimeActivationEnabled":false}),
            KucoinClassicRuntimeDecision::Malformed,
        ),
    ] {
        assert_eq!(
            serde_json::from_value::<KucoinClassicRuntimeDecision>(input).unwrap(),
            expected
        );
    }
    reject_array::<KucoinClassicRuntimeDecision>(json!([
        true,
        "classic-account-mode-confirmed",
        "existing-classic-runtime",
        false
    ]));
}
#[test]
fn detected_mode_retains_native_list_bits_and_tagged_shape() {
    let decoded: KucoinAccountModeDetection = serde_json::from_str(r#"{"status":"identified","accountMode":"CLASSIC","classicSubAccount":[-0,17.5],"unifiedSubAccount":[1e400]}"#).unwrap();
    let KucoinAccountModeDetection::Identified {
        account_mode,
        classic_sub_account,
        unified_sub_account,
    } = decoded
    else {
        panic!("native identified variant required")
    };
    assert_eq!(account_mode, KucoinAccountMode::Classic);
    assert_eq!(classic_sub_account.len(), 2);
    assert_eq!(classic_sub_account[0].to_bits(), (-0.0_f64).to_bits());
    assert_eq!(classic_sub_account[1].to_bits(), 17.5_f64.to_bits());
    assert_eq!(unified_sub_account[0].to_bits(), f64::INFINITY.to_bits());
    reject_array::<KucoinAccountModeDetection>(json!(["identified", "CLASSIC", [3], [7]]));
}
