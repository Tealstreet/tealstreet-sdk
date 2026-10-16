use safe_cex_venue_lighter::types::LighterDeploymentProfile;
use safe_cex_venue_lighter::ws_policy::{
    LighterPrivateCloseClassification, LighterPublicCloseClassification,
};
use serde::Deserialize;
use serde_json::json;

#[test]
fn deployment_profile_rejects_positional_array_on_public_decoder() {
    let value = json!(["livenet", "https://x", "wss://x", 304, "USDC", true, true]);
    assert!(serde_json::from_value::<LighterDeploymentProfile>(value.clone()).is_err());
    assert!(LighterDeploymentProfile::deserialize(value).is_err());
}
#[test]
fn public_close_rejects_positional_array_on_public_decoder() {
    let value = json!([false, false]);
    assert!(serde_json::from_value::<LighterPublicCloseClassification>(value.clone()).is_err());
    assert!(LighterPublicCloseClassification::deserialize(value).is_err());
}
#[test]
fn private_close_rejects_positional_array_on_public_decoder() {
    let value = json!([false, false, false, false, false, false]);
    assert!(serde_json::from_value::<LighterPrivateCloseClassification>(value.clone()).is_err());
    assert!(LighterPrivateCloseClassification::deserialize(value).is_err());
}
