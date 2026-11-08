#![cfg(feature = "bitrue")]
use safe_cex_engine::{Engine, ErrorKind, Outcome, Venue};
use safe_cex_venues::{EngineOptions, Visit, build, build_with_signer};
use serde_json::json;

struct Reject;
impl Visit for Reject {
    type Output = ();
    fn visit<V: Venue + 'static>(self, _: Engine<V>) {
        panic!("Bitrue is outside beta admission");
    }
}

#[test]
fn bitrue_catalogue_and_private_accounts_require_beta_admission_before_dispatch() {
    for value in [
        json!({}),
        json!({"publicDataOnly":true}),
        json!({"key":"", "secret":"", "publicDataOnly":true}),
        json!({"key":"fixture-key", "secret":"", "publicDataOnly":true}),
        json!({"key":"", "secret":"fixture-material", "publicDataOnly":true}),
        json!({"key":"fixture-key", "secret":"fixture-material"}),
        json!({"key":"fixture-key", "secret":"fixture-material", "publicDataOnly":true}),
        json!({"recvWindow":0, "publicDataOnly":true}),
        json!({"recvWindow":100000, "publicDataOnly":true}),
        json!({"signing":{"primary":{"credential":"fixture-ref", "signing":"opaque-key"}}, "publicDataOnly":true}),
        json!({"signing":{"primary":{"credential":"fixture-ref", "signing":"opaque-key", "stableId":"\n"}}, "publicDataOnly":true}),
    ] {
        for injected in [false, true] {
            let options: EngineOptions = serde_json::from_value(value.clone()).unwrap();
            let error = if injected {
                build_with_signer(
                    "bitrue",
                    options,
                    None,
                    Some(std::sync::Arc::new(
                        safe_cex_crypto::signer::LocalSigner::new(),
                    )),
                    Reject,
                )
            } else {
                build("bitrue", options, None, Reject)
            }
            .expect_err("OUT venue cannot construct or dispatch its catalogue");
            assert_eq!(error.kind, Some(ErrorKind::Unsupported));
            assert_eq!(error.outcome, Some(Outcome::NotSent));
        }
    }
}
