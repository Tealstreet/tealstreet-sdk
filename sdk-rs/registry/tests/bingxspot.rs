#![cfg(feature = "bingx")]
use safe_cex_engine::{Action, Engine, Venue};
use safe_cex_venues::{EngineOptions, Visit, build};
struct Spot;
impl Visit for Spot {
    type Output = ();
    fn visit<V: Venue + 'static>(self, mut e: Engine<V>) {
        e.start(1702720824000.);
        let a = safe_cex_engine::test_support::prepared_actions(&mut e, 1702720824000.);
        assert!(a.iter().any(|a|matches!(a,Action::Http(r) if r.url=="https://open-api.bingx.com/openApi/spot/v1/common/symbols")));
    }
}
#[test]
fn native_spot_registry_constructs_without_opening_sdk_admission() {
    build(
        "bingxspot",
        serde_json::from_str::<EngineOptions>(r#"{"publicDataOnly":true}"#).unwrap(),
        Some(safe_cex_engine::test_support::seeded_entropy(17)),
        Spot,
    )
    .unwrap();
}

#[test]
fn spot_refuses_testnet_instead_of_silently_using_live_endpoints() {
    let result = build(
        "bingxspot",
        serde_json::from_str::<EngineOptions>(r#"{"publicDataOnly":true,"testnet":true}"#).unwrap(),
        Some(safe_cex_engine::test_support::seeded_entropy(17)),
        Spot,
    );
    let error = result.unwrap_err();
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::Unsupported));
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::NotSent));
    assert!(error.message.contains("testnet"));
}
