//! Response provenance is a core-owned FINDING, not a venue parser workaround.
use std::process::{Command, Output};

fn source(args: &[&str]) -> Output {
    Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/response-phase-oracle.mjs"
        ))
        .args(args)
        .output()
        .expect("Node and isolated runtime packages are mandatory")
}

#[test]
#[ignore = "requires the isolated TypeScript compiler and actual Axios; mandatory in venue CI"]
fn source_factory_pins_how_response_double_parse_before_serialization() {
    let baseline = source(&[]);
    assert!(
        baseline.status.success(),
        "{}",
        String::from_utf8_lossy(&baseline.stderr)
    );
    let cases: Vec<serde_json::Value> = serde_json::from_slice(&baseline.stdout).unwrap();
    assert_eq!(cases.len(), 14);
    assert_eq!(cases[0]["result"]["dataType"], "string");
    assert_eq!(cases[1]["result"]["dataType"], "number");
    assert_eq!(cases[1]["result"]["data"]["numberBits"], "405ec00000000000");
    let mutation = source(&["--mutate-how-parser"]);
    let diagnostic = String::from_utf8_lossy(&mutation.stderr);
    assert_eq!(mutation.status.code(), Some(1));
    assert!(
        diagnostic.contains("AssertionError")
            && diagnostic.contains("source HOW double parse retains response phase"),
        "{diagnostic}"
    );
    let missing = source(&["--missing-retry-import"]);
    assert_eq!(missing.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("MODULE_NOT_FOUND"));
}
