use safe_cex_types::{MarginType, OptionGreeks, Position, PositionMode, PositionSide};
use std::{path::Path, process::Command};
use ts_rs::{Config, TS};

#[test]
fn committed_position_bindings_match_the_current_rust_model() {
    fn check<T: TS + 'static>(name: &str) {
        let raw = Path::new(env!("CARGO_MANIFEST_DIR")).join("bindings/raw");
        let config = Config::new().with_out_dir(&raw);
        let generated = T::export_to_string(&config).unwrap();
        let committed = std::fs::read_to_string(raw.join(format!("{name}.ts"))).unwrap();
        assert_eq!(generated, committed, "regenerate {name} from Rust");
    }
    check::<Position>("Position");
    check::<PositionSide>("PositionSide");
    check::<PositionMode>("PositionMode");
    check::<MarginType>("MarginType");
    check::<OptionGreeks>("OptionGreeks");
}

#[test]
fn public_position_compiles_and_matches_the_generated_dependency_closure() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new("node")
        .current_dir(root)
        .args([
            "--test",
            "sdk-rs/safe-cex-types/tests/position-contract.cjs",
        ])
        .output()
        .expect("Node is required for the public Position contract gate");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
