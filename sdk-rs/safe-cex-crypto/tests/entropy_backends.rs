//! Pin the resolved browser backend of every locked getrandom generation.
#[test]
fn all_getrandom_generations_have_browser_entropy_backends() {
    for (version, feature) in [("0.2.17", "js"), ("0.3.4", "wasm_js"), ("0.4.3", "wasm_js")] {
        let output = std::process::Command::new("cargo")
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .args([
                "tree",
                "--locked",
                "--target",
                "wasm32-unknown-unknown",
                "-p",
                "safe-cex-crypto",
                "-e",
                "features",
                "-i",
                &format!("getrandom@{version}"),
            ])
            .output()
            .expect("Cargo feature graph required");
        assert!(output.status.success(), "getrandom {version} graph failed");
        let graph = String::from_utf8(output.stdout).unwrap();
        assert!(
            graph.contains(&format!("getrandom feature \"{feature}\"")),
            "getrandom {version} browser backend {feature} missing from resolved wasm graph"
        );
    }
}
