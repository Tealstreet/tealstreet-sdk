use serde_json::Value;
use std::process::Command;

pub fn run(script: &str, args: &[&str]) -> Value {
    let output = Command::new("node")
        .arg(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("scripts")
                .join(script),
        )
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

pub fn verify(file: &str, captured_hash: Option<&str>) {
    let mut args = vec!["--verify-source", file];
    if let Some(hash) = captured_hash {
        args.push(hash);
    }
    let result = run("lighter-source-provenance.cjs", &args);
    assert_eq!(result["verified"], file);
}
