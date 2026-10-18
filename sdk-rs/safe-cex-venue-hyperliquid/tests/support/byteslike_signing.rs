use super::*;
use serde_json::{Value as Json, json};
use std::{process::Command, sync::OnceLock};

fn source() -> &'static Vec<Json> {
    static ROWS: OnceLock<Vec<Json>> = OnceLock::new();
    ROWS.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/byteslike-signing-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "source signer: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let rows: Vec<Json> = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(rows.len(), 8);
        rows
    })
}

fn run(success: bool) {
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&executor, async {
        let mut realm = Realm::new(safe_cex_exchange::native::native_runtime());
        super::super::imports::bind(&realm);
        let make_wallet = realm.import("createWalletFromPrivateKey").unwrap();
        let wallet = realm
            .call(
                &make_wallet,
                &Value::Undefined,
                &[Value::str(&format!("0x{}", "11".repeat(32)))],
            )
            .unwrap();
        let signer = realm.import("signL1Action").unwrap();
        let action = realm.from_serde(&json!({"type":"cancel","cancels":[{"a":1,"o":1234}]}));
        let mut completed = 0;
        for row in source().iter().filter(|row| row["ok"] == success) {
            let vault = row["vault"].as_str().unwrap();
            let pending = realm
                .call(
                    &signer,
                    &Value::Undefined,
                    &[
                        wallet.clone(),
                        action.clone(),
                        Value::str(vault),
                        Value::Number(1700000000123.0),
                        Value::Bool(true),
                    ],
                )
                .unwrap();
            let actual = realm.await_value(pending).await;
            if success {
                let signature = actual.unwrap_or_else(|error| {
                    panic!(
                        "native signature must accept {vault}: {:?}",
                        realm.to_serde(&error)
                    )
                });
                for name in ["r", "s"] {
                    let field = realm.get_str(&signature, name).unwrap();
                    assert_eq!(
                        realm.to_rust_string(&field).unwrap(),
                        row["signature"][name].as_str().unwrap(),
                        "native {vault}/{name}"
                    );
                }
                let v = realm.get_str(&signature, "v").unwrap();
                assert_eq!(
                    v.as_number().unwrap().to_bits(),
                    row["signature"]["v"].as_f64().unwrap().to_bits(),
                    "native {vault}/v"
                );
                assert_eq!(
                    crate::signing::action_hash(
                        &json!({"type":"cancel","cancels":[{"a":1,"o":1234}]}),
                        Some(vault),
                        &json!(1700000000123u64)
                    )
                    .unwrap(),
                    row["hash"]
                );
                assert_eq!(
                    crate::signing::address_to_bytes(vault).unwrap(),
                    serde_json::from_value::<Vec<u8>>(row["bytes"].clone()).unwrap()
                );
            } else {
                let error = actual.expect_err("invalid vault rejects the real signer");
                let mut observed = serde_json::Map::new();
                for name in [
                    "name",
                    "message",
                    "code",
                    "argument",
                    "value",
                    "shortMessage",
                ] {
                    let field = realm.get_str(&error, name).unwrap();
                    if !matches!(field, Value::Undefined) {
                        observed.insert(name.into(), realm.to_serde(&field).unwrap());
                    }
                }
                assert_eq!(
                    Json::Object(observed),
                    row["error"],
                    "native rejected signer preserves ethers {vault}"
                );
            }
            completed += 1;
        }
        assert_eq!(completed, if success { 5 } else { 3 });
    });
}

#[test]
fn native_realm_signer_accepts_ethers_byteslike_prefixes() {
    run(true);
}

#[test]
fn native_realm_signer_retains_byteslike_error_protocol() {
    run(false);
}

#[test]
#[allow(clippy::arc_with_non_send_sync)]
fn native_api_signing_retains_original_error_through_transport() {
    use safe_cex_exchange::js_runtime::value::{self as shared, owned::OwnedCallbackError};
    use safe_cex_exchange::runtime::{ExchangeError, ExchangeRuntime};
    use safe_cex_transport::{RestFailure, TransportError};
    use std::{rc::Rc, sync::Arc};
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&executor, async {
        let runtime = safe_cex_exchange::native::native_runtime();
        let clock = Arc::new(crate::binding::RuntimeClock::new(runtime.clone()));
        let reporter = runtime.clone();
        let options = serde_json::from_value(json!({
            "key": "key", "secret": format!("0x{}", "11".repeat(32))
        }))
        .unwrap();
        let client = crate::api::create_exchange_api(
            options,
            Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
            Rc::new(crate::rate_limit::RateLimiter::with_defaults(
                clock, None, None,
            )),
            Arc::new(move |message| reporter.console("error", message)),
        );
        let mut completed = 0;
        for row in source().iter().filter(|row| row["ok"] == false) {
            let body = json!({
                "action": {"type": "cancel", "cancels": [{"a": 1, "o": 1234}]},
                "nonce": 1700000000123u64, "vaultAddress": row["vault"]
            });
            let request = client.build_request("/exchange", &body).unwrap();
            let error = client
                .prepare_request(request.clone(), true)
                .await
                .unwrap_err();
            let TransportError::Rejected(origin) = &error else {
                panic!("native signer must preserve typed origin: {error:?}")
            };
            let cause = origin
                .error()
                .downcast_ref::<OwnedCallbackError>()
                .expect("canonical owned callback cause");
            assert_eq!(
                shared::to_json(&cause.value().to_js()).unwrap(),
                row["error"]
            );
            let mut realm = Realm::new(runtime.clone());
            let projected = super::super::facade_rest::thrown_from(
                &mut realm,
                ExchangeError::from(RestFailure::from_transport(error, request)),
            );
            let mut actual = serde_json::Map::new();
            for name in [
                "name",
                "message",
                "code",
                "argument",
                "value",
                "shortMessage",
            ] {
                let field = realm.get_str(&projected, name).unwrap();
                actual.insert(name.into(), realm.to_serde(&field).unwrap());
            }
            assert_eq!(
                Json::Object(actual),
                row["error"],
                "production Realm REST error projection retains original fields"
            );
            completed += 1;
        }
        assert_eq!(completed, 3);
    });
}
