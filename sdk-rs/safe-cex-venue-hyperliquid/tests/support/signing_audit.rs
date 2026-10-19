//! Exercise the installed signer from a constructed production exchange.
use super::{HyperliquidDependencies, HyperliquidExchange, RuntimeClock};
use crate::{binding_test_runtime as host, rate_limit::RateLimiter};
use safe_cex_exchange::{native_store::DefaultStore, js_runtime::value::Js};
use serde_json::{Value as Json, json};
use std::{
    process::Command,
    rc::Rc,
    sync::{Arc, OnceLock},
};

fn source() -> &'static Vec<Json> {
    static SOURCE: OnceLock<Vec<Json>> = OnceLock::new();
    SOURCE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/signing-audit.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let rows: Vec<Json> = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(rows.len(), 14);
        rows
    })
}

#[allow(clippy::arc_with_non_send_sync)]
fn fixture() -> Rc<HyperliquidExchange> {
    let runtime = Rc::new(host::Host::default());
    HyperliquidExchange::new(
        serde_json::from_value(json!({"key":"","secret":"","applicationId":"0x1111111111111111111111111111111111111111"})).unwrap(),
        Rc::new(DefaultStore::new()),
        HyperliquidDependencies {
            runtime: runtime.clone(), http: host::Routes::new(vec![]),
            limiter: Rc::new(RateLimiter::new(Arc::new(RuntimeClock::new(runtime.clone())), 1200., 60_000.)),
            signers: Default::default(), external_public_data: None,
        },
    ).unwrap()
}

fn run(success: bool) {
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&executor, async {
        let exchange = fixture();
        let mut r = exchange.binding.borrow().as_ref().unwrap().realm.clone();
        let make_wallet = r.import("createWalletFromPrivateKey").unwrap();
        let wallet = r.call(&make_wallet, &Js::Undefined, &[Js::str(&format!("0x{}", "11".repeat(32)))]).unwrap();
        let signer = r.import("signL1Action").unwrap();
        let action = r.encode_json(&json!({"type":"cancel","cancels":[{"a":1,"o":1234}]}));
        let mut completed = 0;
        for row in source().iter().filter(|row| row["ok"] == success) {
            let id = row["id"].as_str().unwrap();
            let nonce = match id {
                "zero" => 0., "negative-zero" => -0., "negative" => -1.,
                "safe-max" => 9007199254740991., "wrap64" => 18446744073709551616.,
                "fraction-positive" => 1.25, "fraction-negative" => -0.25,
                _ => panic!("unknown source nonce {id}"),
            };
            let bytes = f64::to_le_bytes(nonce);
            let spelling = bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
            assert_eq!(spelling, row["nonceBits"], "native nonce bits {id}");
            let returned = r.call(&signer, &Js::Undefined, &[wallet.clone(), action.clone(), Js::Null, Js::Number(nonce), Js::Bool(row["mainnet"].as_bool().unwrap())]);
            assert_eq!(returned.is_err(), row["syncThrow"].as_bool().unwrap(), "HL_SIGNING sync boundary {id}");
            let pending = returned.unwrap();
            assert_eq!(matches!(&pending, Js::Promise(_)), row["promise"], "HL_SIGNING Promise boundary {id}");
            let result = r.await_value(pending).await;
            if success {
                let signature = result.unwrap();
                for key in ["r", "s"] {
                    let actual = r.get_str(&signature, key).unwrap();
                    assert_eq!(r.to_rust_string(&actual).unwrap(), row["signature"][key], "HL_SIGNING {id}/{key}");
                }
                let v = r.get_str(&signature, "v").unwrap();
                assert_eq!(v.as_f64().unwrap().to_bits(), row["signature"]["v"].as_f64().unwrap().to_bits(), "HL_SIGNING {id}/v");
            } else {
                let error = result.expect_err("fractional nonce rejects the installed signer");
                let name = r.get_str(&error.value, "name").unwrap();
                let message = r.get_str(&error.value, "message").unwrap();
                let base = r.global_binding("Error").unwrap();
                let range = r.global_binding("RangeError").unwrap();
                let profile = json!({
                    "name": r.to_rust_string(&name).unwrap(), "message": r.to_rust_string(&message).unwrap(),
                    "display": r.to_rust_string(&error.value).unwrap(),
                    "isError": r.instance_of(&error.value, &base).unwrap(), "isRangeError": r.instance_of(&error.value, &range).unwrap(),
                });
                assert_eq!(profile, row["error"], "HL_SIGNING native nonce rejection {id}");
            }
            completed += 1;
        }
        assert_eq!(completed, if success {10} else {4});
    });
}

#[test]
fn signing_audit_bound_nonce_signatures_match_maintained_source() {
    run(true);
}

#[test]
fn signing_audit_bound_fractional_nonce_retains_range_error() {
    run(false);
}

#[test]
#[allow(clippy::arc_with_non_send_sync)]
fn signing_audit_api_nonce_error_retains_transport_cause() {
    use safe_cex_exchange::{
        js_runtime::value::owned::OwnedCallbackError, runtime::ExchangeRuntime,
    };
    use safe_cex_transport::TransportError;
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&executor, async {
        let runtime = safe_cex_exchange::native::native_runtime();
        let reporter = runtime.clone();
        let client = crate::api::create_exchange_api(
            serde_json::from_value(json!({"key":"key", "secret":format!("0x{}", "11".repeat(32))}))
                .unwrap(),
            Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
            Rc::new(RateLimiter::with_defaults(
                Arc::new(RuntimeClock::new(runtime)),
                None,
                None,
            )),
            Arc::new(move |message| reporter.console("error", message)),
        );
        let mut completed = 0;
        for row in source()
            .iter()
            .filter(|row| row["ok"] == false && row["mainnet"] == true)
        {
            let nonce = if row["id"] == "fraction-positive" {
                1.25
            } else {
                -0.25
            };
            let body =
                json!({"action":{"type":"cancel","cancels":[{"a":1,"o":1234}]},"nonce":nonce});
            let request = client.build_request("/exchange", &body).unwrap();
            let error = client
                .prepare_request(request, true, &|_| Ok(()))
                .await
                .unwrap_err();
            let TransportError::Rejected(origin) = error else {
                panic!("HL_SIGNING typed transport cause missing: {error:?}")
            };
            let cause = origin
                .error()
                .downcast_ref::<OwnedCallbackError>()
                .expect("canonical callback cause");
            let actual =
                safe_cex_exchange::js_runtime::value::to_json(&cause.value().to_js()).unwrap();
            assert_eq!(
                actual,
                json!({"name":row["error"]["name"],"message":row["error"]["message"]}),
                "HL_SIGNING API nonce rejection"
            );
            completed += 1;
        }
        assert_eq!(completed, 2);
    });
}
