use serde_json::Value;
use std::process::{Command, Output};
fn oracle(control: Option<&str>) -> Output {
    let mut command = Command::new("node");
    command.arg(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/scripts/dispatch-findings.mjs"
    ));
    if let Some(control) = control {
        command.arg(control);
    }
    command.output().expect("Node source witness must execute")
}
#[test]
#[ignore = "mandatory live source witness: yarn venue-oracle:binance"]
fn held_dispatch_inherited_arity_and_stats_findings_are_observable() {
    let output = oracle(None);
    assert!(
        output.status.success(),
        "source dispatch witness: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 192, "all twelve inherited members and crosses");
    assert!(cases.iter().any(|c| {
        c["key"] == "__proto__"
            && c["prototypeWrites"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "lastMessageTimestamp")
    }));
    for (control, assertion) in [
        (
            "--own-only",
            "dispatch constructor exact=false prefix=false malformed=false time=1000: stats before parse",
        ),
        (
            "--arity-as-truthiness",
            "dispatch toString exact=false prefix=false malformed=false time=1000: stats before parse",
        ),
        (
            "--stats-own-only",
            "dispatch constructor exact=false prefix=false malformed=false time=1000: inherited stats entry stays inherited",
        ),
        (
            "--skip-reconcile",
            "dispatch __proto__ exact=false prefix=true malformed=false time=6000: inherited iteration failure",
        ),
    ] {
        let output = oracle(Some(control));
        assert_eq!(output.status.code(), Some(1), "control {control} must fail");
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("AssertionError [ERR_ASSERTION]") && error.contains(assertion),
            "control {control} must fail its named source assertion: {error}"
        );
    }
}
