//! Canonical retained JSON admission; the carrier is observed before Value projection.
use safe_cex_types::retained_json::RetainedJsonInput as Input;

#[test]
fn retained_json_admits_valid_lone_surrogate_values() {
    let result = serde_json::from_str::<Input>(r#"{"label":"tp \ud800","rows":[1,2]}"#);
    assert!(
        result.is_ok(),
        "valid source JSON must enter the retained carrier without UTF-8 projection: {result:?}"
    );
}
#[test]
fn retained_json_admits_valid_lone_surrogate_keys() {
    let result = serde_json::from_str::<Input>(r#"{"\udfff":"kept","rows":[1,2]}"#);
    assert!(
        result.is_ok(),
        "valid source UTF-16 own key must enter the retained carrier: {result:?}"
    );
}
