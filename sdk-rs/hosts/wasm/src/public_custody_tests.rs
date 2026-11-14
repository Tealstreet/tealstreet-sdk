//! The actual native-compiled WasmEngine flag must satisfy the real JS host guard.
use super::WasmEngine;
use serde_json::json;

fn host_admission(venue: &str, options: serde_json::Value, required: bool) {
    let expected_public = options["publicMode"]["requested"].as_bool().unwrap_or(true);
    let engine = WasmEngine::new(venue, &options.to_string(), None)
        .unwrap_or_else(|_| panic!("{venue}: constructor failed"));
    let construction: serde_json::Value = serde_json::from_str(&engine.construction()).unwrap();
    let row = json!({
        "venue": venue,
        "construction": construction,
        "requiresExternalSigner": engine.requires_external_signer(),
        "expectedRequired": required,
        "expectedPublic": expected_public,
    });
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/js/public-custody.proof.mjs"
        ))
        .arg(row.to_string())
        .output()
        .expect("Node is required to exercise the real EngineHost guard");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn public_options(explicit: bool) -> serde_json::Value {
    let mut options = json!({"testnet":true,"payloadSigner":true,
        "publicMode":{"requested":null,"payloadSigner":true},
        "signing":{"primary":{"credential":"unused","signing":"unused"}}});
    if explicit {
        options["publicMode"]["requested"] = true.into();
        options["applicationId"] = "0x1111111111111111111111111111111111111111".into();
    }
    options
}

#[test]
fn explicit_public_refs_need_no_engine_host_provider() {
    host_admission("txflow", public_options(true), false);
}

#[test]
fn inferred_public_refs_need_no_engine_host_provider() {
    host_admission("txflow", public_options(false), false);
}

#[test]
fn private_refs_still_require_an_engine_host_provider() {
    host_admission(
        "txflow",
        json!({"testnet":true,
        "key":"0x0202020202020202020202020202020202020202",
        "applicationId":"0x1111111111111111111111111111111111111111",
        "publicMode":{"requested":false,"remoteSigner":true},
        "signing":{"primary":{"credential":"owned","signing":"owned"}}}),
        true,
    );
}

#[cfg(feature = "backpack")]
#[test]
fn public_read_refs_still_require_an_engine_host_provider() {
    host_admission(
        "backpack",
        json!({"publicMode":{"requested":true},
        "signing":{"secondary":{"credential":"read","signing":"read"}}}),
        true,
    );
}

#[cfg(feature = "backpack")]
#[test]
fn empty_public_read_refs_need_no_engine_host_provider() {
    host_admission(
        "backpack",
        json!({"publicMode":{"requested":true},"signing":{}}),
        false,
    );
}

#[cfg(any(
    feature = "bingx",
    feature = "weex",
    feature = "bitget",
    feature = "okx"
))]
fn venue_owned_private_refs(venue: &str, passphrase: bool, remote: bool) {
    let mut options = json!({"testnet":true,"remoteSigning":remote,
        "publicMode":{"requested":false,"remoteSigner":remote},
        "signing":{"primary":{"credential":"private-account","signing":"private-signing"}}});
    if passphrase {
        options["signing"]["primary"]["passphrase"] = "private-passphrase-ref".into();
    }
    host_admission(venue, options, true);
}

#[cfg(feature = "bingx")]
#[test]
fn bingx_private_refs_require_an_engine_host_provider() {
    venue_owned_private_refs("bingx", false, true);
}

#[cfg(feature = "weex")]
#[test]
fn weex_private_refs_require_an_engine_host_provider() {
    // WEEX callback admission is closed; trusted client refs still require
    // the EngineHost provider whenever native construction retains them.
    venue_owned_private_refs("weex", true, false);
}

#[cfg(feature = "bitget")]
#[test]
fn bitget_private_refs_require_an_engine_host_provider() {
    venue_owned_private_refs("bitget", true, true);
}

#[cfg(feature = "okx")]
#[test]
fn okx_private_refs_require_an_engine_host_provider() {
    venue_owned_private_refs("okx", true, true);
}
