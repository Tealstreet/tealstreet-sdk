fn main() {
    let source = "../../../packages/safe-cex/src/exchange-display-policy.json";
    println!("cargo:rerun-if-changed={source}");
    let rules: std::collections::BTreeMap<String, bool> =
        serde_json::from_str(&std::fs::read_to_string(source).expect("shared display metadata"))
            .expect("valid shared display metadata");
    let names = rules
        .into_iter()
        .filter(|(_, enabled)| *enabled)
        .map(|(name, _)| format!("{name:?}"))
        .collect::<Vec<_>>()
        .join(" | ");
    let expression = if names.is_empty() {
        "false".into()
    } else {
        format!("matches!(venue, {names})")
    };
    let code = format!("fn calculate_upnl_from_last(venue: &str) -> bool {{ {expression} }}");
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(out.join("display_policy.rs"), code).unwrap();
}
