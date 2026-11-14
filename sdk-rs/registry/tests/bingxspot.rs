#![cfg(feature = "bingx")]
use safe_cex_engine::{Engine, ErrorKind, Outcome, Venue};
use safe_cex_venues::{EngineOptions, Visit, build, build_with_signer, supported};
struct NeverConstruct;
impl Visit for NeverConstruct {
    type Output = ();
    fn visit<V: Venue + 'static>(self, _: Engine<V>) {
        panic!("fixture-only venue reached a live factory");
    }
}
#[test]
fn fixture_only_spot_is_refused_by_native_and_hosted_factories() {
    assert!(!supported("bingxspot"));
    assert!(supported("bingx"));
    for json in [r#"{}"#, r#"{"publicDataOnly":true}"#, r#"{"testnet":true}"#] {
        for injected in [false, true] {
            let options: EngineOptions = serde_json::from_str(json).unwrap();
            let entropy = Some(safe_cex_engine::test_support::seeded_entropy(17));
            let result = if injected {
                build_with_signer("bingxspot", options, entropy, None, NeverConstruct)
            } else {
                build("bingxspot", options, entropy, NeverConstruct)
            };
            let error = result.unwrap_err();
            assert_eq!(error.kind, Some(ErrorKind::Unsupported));
            assert_eq!(error.outcome, Some(Outcome::NotSent));
            assert_eq!(error.message, "unsupported venue: bingxspot");
        }
    }
}
