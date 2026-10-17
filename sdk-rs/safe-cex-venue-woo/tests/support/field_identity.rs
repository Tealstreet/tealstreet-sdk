use std::process::Command;

pub fn capture(oracle: &str) -> String {
    let output = Command::new("node")
        .arg(format!(
            "{}/scripts/{oracle}-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .arg("--field-identity")
        .output()
        .expect("field-identity source oracle must execute");
    assert!(
        output.status.success(),
        "field-identity source oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("field-identity source capture must be JSON");
    assert!(
        envelope["capture"]["architecture"].as_str().is_some(),
        "capture architecture missing"
    );
    let fixture = &envelope["fixture"];
    match oracle {
        "types" => assert_eq!(
            fixture.as_array().unwrap().len(),
            145,
            "WOO-IDENTITY legacy source total"
        ),
        "contracts" => assert_eq!(
            fixture["cases"].as_array().unwrap().len(),
            312,
            "WOO-IDENTITY contract source total"
        ),
        "public" => assert_eq!(
            fixture.as_object().unwrap().len(),
            6,
            "WOO-IDENTITY public record total"
        ),
        unknown => panic!("unregistered identity corpus {unknown}"),
    }
    fixture.to_string()
}
