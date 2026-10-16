use serde_json::Value;

fn oracle(args: &[&str]) -> std::process::Output {
    std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/send-store.mjs"
        ))
        .args(args)
        .output()
        .expect("Node actual-source socket notification witness")
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn private_ping_store_notification_reachability_finding() {
    let output = oracle(&[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 64);
    let cases: Vec<_> = cases
        .into_iter()
        .filter(|case| case["method"] == "ping")
        .collect();
    assert_eq!(cases.len(), 12);
    for group in cases.as_chunks::<4>().0 {
        assert_eq!(group[0]["settlement"], "resolved");
        assert_eq!(group[0]["reconnecting"], true);
        assert_eq!(group[0]["onCloseCount"], 101);
        assert_eq!(group[1]["settlement"], "rejected");
        assert_eq!(group[1]["reconnecting"], false);
        assert_eq!(group[1]["onCloseCount"], 100);
        assert_eq!(group[1]["health"]["publicHealthy"], true);
        assert_eq!(
            group[1]["trace"],
            serde_json::json!([["first", true], "diagnostic", "caller-returned"])
        );
        assert_eq!(group[2]["settlement"], Value::Null);
        assert_eq!(group[3]["settlement"], Value::Null);
    }
    // Source-only witness retained independently of the native Store/core
    // continuation and cause-pointer assertions in send_notification.rs.
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn private_ping_store_notification_witness_controls() {
    for (arg, assertion) in [
        (
            "--bypass-health-write",
            "send reaches health notification and skips later subscriber on throw",
        ),
        (
            "--skip-reconnect",
            "send reaches health notification and skips later subscriber on throw",
        ),
        ("--missing-runtime-import", "MODULE_NOT_FOUND"),
        (
            "--await-spot-reconnect",
            "send reaches health notification and skips later subscriber on throw",
        ),
        (
            "--skip-reset",
            "public open resets orderbook despite background rejection",
        ),
        (
            "--force-public-send",
            "send reaches health notification and skips later subscriber on throw",
        ),
        (
            "--skip-open-continuation",
            "private open continues after discarded background rejection",
        ),
    ] {
        let output = oracle(&[arg]);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "control {arg} survived");
        assert!(stderr.contains(assertion), "control {arg}: {stderr}");
    }
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn private_open_store_notification_reachability_finding() {
    let output = oracle(&[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let cases: Vec<_> = cases
        .into_iter()
        .filter(|case| {
            case["method"] == "onOpen"
                && case["scope"] == "private"
                && case["variant"] != "binancespot"
        })
        .collect();
    assert_eq!(cases.len(), 8);
    for group in cases.as_chunks::<4>().0 {
        assert_eq!(group[0]["settlement"], "resolved");
        assert_eq!(group[1]["settlement"], "rejected");
        assert_eq!(group[1]["reconnecting"], false);
        assert_eq!(group[1]["onCloseCount"], 100);
        assert_eq!(group[1]["hasOpened"], true);
        assert_eq!(group[1]["resyncOrdersOnOpen"], false);
        assert_eq!(
            group[1]["trace"],
            serde_json::json!([
                ["first", true],
                "diagnostic",
                "fetch-orders",
                "caller-returned"
            ])
        );
        assert_eq!(group[2]["hasOpened"], false);
        assert_eq!(group[3]["settlement"], Value::Null);
    }
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn public_open_store_notification_reachability_finding() {
    let output = oracle(&[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let cases: Vec<_> = cases
        .into_iter()
        .filter(|case| case["scope"] == "public")
        .collect();
    assert_eq!(cases.len(), 40);
    for case in cases {
        let disposed = case["disposed"].as_bool().unwrap();
        let market = case["market"].as_bool().unwrap();
        let skips_send =
            !market && case["variant"] != "binancespot" && case["publicDataOnly"] == true;
        if disposed || skips_send {
            assert_eq!(case["settlement"], Value::Null);
        } else if case["listenerThrows"] == true {
            assert_eq!(case["settlement"], "rejected");
            assert_eq!(case["reconnecting"], false);
            assert_eq!(case["onCloseCount"], 100);
            assert_eq!(case["health"]["publicHealthy"], true);
            assert_eq!(
                case["trace"],
                serde_json::json!([["first", true], "subscribe", "caller-returned"])
            );
        } else {
            assert_eq!(case["settlement"], "resolved");
        }
        if !disposed && !market {
            assert_eq!(case["book"]["updates"], serde_json::json!([]));
            assert_eq!(case["book"]["isSnapshotLoaded"], false);
        }
    }
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn spot_private_open_store_notification_reachability_finding() {
    let output = oracle(&[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let cases: Vec<_> = cases
        .into_iter()
        .filter(|case| {
            case["scope"] == "private"
                && case["variant"] == "binancespot"
                && case["method"] == "onOpen"
        })
        .collect();
    assert_eq!(cases.len(), 4);
    assert_eq!(cases[0]["settlement"], "resolved");
    assert_eq!(cases[1]["settlement"], "rejected");
    assert_eq!(
        cases[1]["trace"],
        serde_json::json!([
            "caller-returned",
            ["first", true],
            "diagnostic",
            "caller-resolved"
        ])
    );
    assert_eq!(cases[1]["reconnecting"], false);
    assert_eq!(cases[1]["onCloseCount"], 100);
    assert_eq!(cases[1]["health"]["publicHealthy"], true);
    for case in &cases[2..] {
        assert_eq!(case["settlement"], Value::Null);
        assert_eq!(
            case["trace"],
            serde_json::json!(["caller-returned", "caller-resolved"])
        );
    }
}
