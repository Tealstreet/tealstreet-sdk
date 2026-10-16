use serde_json::Value;
use std::{path::Path, process::Command};

pub fn observe(script: &str, profile: &str, assertion: &str) -> Vec<Value> {
    let run = |control| {
        let mut command = Command::new("node");
        command
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join(script))
            .arg(profile);
        if control {
            command.arg("--control");
        }
        command
            .output()
            .expect("maintained TypeScript must execute")
    };
    let baseline = run(false);
    assert!(
        baseline.status.success(),
        "WEEX source baseline: {}",
        String::from_utf8_lossy(&baseline.stderr)
    );
    let mutant = run(true);
    assert_eq!(mutant.status.code(), Some(1), "WEEX source control exit");
    assert!(
        String::from_utf8_lossy(&mutant.stderr).contains(assertion),
        "WEEX named source control: {}",
        String::from_utf8_lossy(&mutant.stderr)
    );
    let restored = run(false);
    assert!(
        restored.status.success(),
        "WEEX restored source: {}",
        String::from_utf8_lossy(&restored.stderr)
    );
    let mut results = Vec::new();
    for output in [baseline, restored] {
        let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(envelope["node"].as_str().is_some_and(|s| !s.is_empty()));
        assert!(envelope["arch"].as_str().is_some_and(|s| !s.is_empty()));
        let observations = envelope["observations"].as_array().unwrap();
        assert_eq!(observations.len(), 1, "WEEX one selected source profile");
        assert_eq!(
            observations[0]["id"], profile,
            "WEEX source profile identity"
        );
        results.push(observations[0].clone());
    }
    assert_eq!(
        results.len(),
        2,
        "WEEX baseline and restored observation floor"
    );
    results
}
