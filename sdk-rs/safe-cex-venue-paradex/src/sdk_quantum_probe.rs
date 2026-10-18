//! Actual RequestSigner construction/member dispatch on the production shared host.
use super::*;
#[allow(dead_code)]
mod io {
    use crate as safe_cex_venue_paradex;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/exchange_runtime.rs"
    ));
}
#[test]
fn constructed_sdk_quantum_matches_source_nonfinite_admission() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/quantum-binding-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source quantum: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 10);
    let (venue, runtime, _, wire) = io::fixture_with(
        serde_json::json!({"key":format!("0x{:064x}",1),"secret":"","paradexAddress":"0x123","subkeyAddress":"0x456"}),
        |wire| {
            wire.responses.borrow_mut().insert("/v1/system/config".into(),serde_json::json!({"starknet_fullnode_rpc_url":"https://fixture.invalid/rpc","starknet_chain_id":"PRIVATE_SN_POTC_MAINNET","l1_chain_id":"1","paraclear_account_hash":"0x1","paraclear_account_proxy_hash":"0x2","paraclear_address":"0x3","paraclear_decimals":8,"bridged_tokens":[]}));
        },
    );
    io::drive(&runtime, venue.unit("initializeSDK", vec![])).unwrap();
    let signer = venue.rt.this.get("requestSigner").unwrap();
    assert!(!signer.is_nullish());
    let requests = wire.requests.borrow().len();
    let specs = ["size", "price"]
        .into_iter()
        .flat_map(|field| {
            ["Infinity", "-Infinity", "NaN", "1.234567891"]
                .into_iter()
                .map(move |value| (field, value))
        })
        .chain([("size", "1e400"), ("price", "1e400")]);
    let mut actual = Vec::new();
    for (index, (field, value)) in specs.enumerate() {
        let id = format!("{field}:{value}");
        assert_eq!(rows[index]["id"], id);
        let result = signer.call_method("toChainQuantum", vec![Js::str(value), Js::str(field)]);
        actual.push(match result {
            Ok(value) => serde_json::json!({"id":id,"value":String::from_utf16(&value.to_js_string().unwrap()).unwrap()}),
            Err(error) => {
                let error = error.to_exchange_error();
                serde_json::json!({"id":id,"name":error.name,"message":error.message})
            }
        });
    }
    assert_eq!(
        actual.len(),
        10,
        "all8 original plus2 finite-overflow profiles execute natively"
    );
    assert_eq!(
        wire.requests.borrow().len(),
        requests,
        "quantum admission performs no HTTP"
    );
    venue.dispose().unwrap();
    for (native, source) in actual.iter().zip(rows.iter()) {
        assert_eq!(native, source, "actual SDK {}", source["id"]);
    }
}
