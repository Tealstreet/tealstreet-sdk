use serde_json::Value;

fn oracle(args: &[&str]) -> std::process::Output {
    std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/dispose-notification.mjs"
        ))
        .args(args)
        .output()
        .expect("Node actual RWS disposal witness")
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn actual_rws_disposal_removes_notification_edge() {
    let output = oracle(&[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 32);
    for case in cases {
        assert_eq!(case["immediate"]["closeCallbacks"], 0);
        assert_eq!(case["immediate"]["notifications"], 0);
        assert_eq!(case["closeCallbacks"], 0);
        assert_eq!(case["notifications"], 0);
        assert_eq!(case["trace"][0], "child");
        assert_eq!(
            case["trace"][1],
            if case["terminates"] == true {
                "terminate"
            } else {
                "close"
            }
        );
    }
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn actual_rws_disposal_notification_controls() {
    for (arg, assertion) in [
        (
            "--keep-rws-listeners",
            "RWS disposal detaches close forwarding before host termination",
        ),
        (
            "--skip-parent-dispose",
            "venue child then actual parent termination",
        ),
        ("--missing-runtime-import", "MODULE_NOT_FOUND"),
    ] {
        let output = oracle(&[arg]);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "control {arg} survived");
        assert!(stderr.contains(assertion), "control {arg}: {stderr}");
    }
}
