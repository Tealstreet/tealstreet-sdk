//! Source requirements for pending lifecycle rows, not a Rust lifecycle parity claim.
use serde_json::{Value, json};
#[test]
fn pending_cleanup_rows_require_partial_cancels_optional_fills_and_fallible_events() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/chaser_lifecycle_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 10);
    for row in rows {
        assert_eq!(
            row["cancels"][0]["orders"],
            json!([{"id":"absent","symbol":"BTC"}]),
            "PARTIAL-CANCEL source must not fabricate full order fields"
        );
        assert!(
            serde_json::from_value::<safe_cex_types::Order>(row["cancels"][0]["orders"][0].clone())
                .is_err(),
            "FINDING current full Order ABI refuses source cancellation stub"
        );
        assert_eq!(row["cancels"].as_array().unwrap().len(), 2);
        for cancel in row["cancels"].as_array().unwrap() {
            assert_eq!(cancel["ids"], json!([]));
            assert_eq!(cancel["managerCount"], 0);
            assert_eq!(cancel["disposed"], true, "DETACH-BEFORE-CANCEL");
        }
        let callable = matches!(row["mode"].as_str(), Some("ok" | "error" | "empty"));
        assert_eq!(
            row["fetches"].as_array().unwrap().len(),
            usize::from(callable),
            "OPTIONAL-FILL-HOOK checks function presence, not a default empty success"
        );
        if callable {
            assert_eq!(
                row["fetches"][0],
                json!({"ids":["absent","known"],"bound":true})
            );
        }
        let events = row["events"].as_array().unwrap();
        let completes: Vec<_> = events
            .iter()
            .filter(|e| e["event"] == "chaser:complete")
            .collect();
        assert_eq!(completes.len(), 1);
        assert_eq!(
            completes[0]["payload"]["filled"],
            if row["mode"] == "ok" {
                json!(1.5)
            } else if row["mode"] == "empty" {
                json!(1)
            } else {
                json!(2)
            },
            "RECONCILED-VS-POSITION-FALLBACK"
        );
        let errors: Vec<_> = events
            .iter()
            .filter(|e| e["event"] == "log")
            .map(|e| e["payload"].as_str().unwrap())
            .collect();
        assert_eq!(
            errors
                .iter()
                .any(|s| s.contains("fill reconciliation failed: Error: reconcile fixture")),
            row["mode"] == "error"
        );
        assert_eq!(
            errors
                .iter()
                .any(|s| s.contains("listener threw: Error: listener fixture")),
            row["listenerThrows"] == true,
            "FALLIBLE-COMPLETE-EVENT is caught and logged by the caller"
        );
        assert_eq!(
            errors.len(),
            usize::from(row["mode"] == "error") + usize::from(row["listenerThrows"] == true),
            "FILL-AND-EVENT-ERROR-COUNT"
        );
        assert_eq!(row["completionClaimedAgain"], false);
        assert_eq!(row["ids"], json!([]));
        assert_eq!(row["managerCount"], 0);
    }
}
