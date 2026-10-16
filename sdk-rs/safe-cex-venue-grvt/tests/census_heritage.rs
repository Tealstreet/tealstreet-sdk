#![cfg(not(target_arch = "wasm32"))]
#[test]
fn inherited_required_number_cannot_hide_from_ast_inventories() {
    for (script, variable, mutation, name) in [
        (
            "numeric-records.cjs",
            "GRVT_NUMERIC_HERITAGE_MUTATION",
            "GrvtInstrumentLite",
            "GRVT numeric census refuses inherited fields: GrvtInstrumentLite",
        ),
        (
            "numeric-records.cjs",
            "GRVT_NUMERIC_HERITAGE_MUTATION",
            "GrvtWebSocketMessage",
            "GRVT numeric census refuses inherited fields: GrvtWebSocketMessage",
        ),
        (
            "types.cjs",
            "GRVT_TYPES_MUTATION",
            "heritage-fill",
            "GRVT union census refuses inherited fields: GrvtFill",
        ),
        (
            "types.cjs",
            "GRVT_TYPES_MUTATION",
            "heritage-subscription",
            "GRVT union census refuses inherited fields: GrvtWebSocketSubscription",
        ),
    ] {
        let output = std::process::Command::new("node")
            .arg(format!("{}/tests/{script}", env!("CARGO_MANIFEST_DIR")))
            .env(variable, mutation)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(1),
            "heritage control must refuse: {script}/{mutation}"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(name),
            "named heritage refusal required: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
