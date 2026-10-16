use safe_cex_venue_cryptocom::{ws_private, ws_public};
use serde::Deserialize;
use std::process::Command;

#[derive(Deserialize)]
struct Row {
    socket: String,
    method: String,
    kind: String,
    bits: Option<String>,
    value: Option<bool>,
    touches: usize,
}

#[derive(Deserialize)]
struct Capture {
    rows: Vec<Row>,
}

#[derive(Debug, PartialEq)]
enum Native {
    Boolean(bool),
    Number(f64),
    Unit,
    Panic,
}

fn native(socket: &str, method: &str) -> Native {
    match (socket, method) {
        ("private", "shouldEnforcePongMissReconnect") => {
            Native::Boolean(ws_private::should_enforce_pong_miss_reconnect())
        }
        ("private", "getWatchdogStaleThresholdMs") => {
            Native::Number(ws_private::get_watchdog_stale_threshold_ms())
        }
        ("private", "getWatchdogPingThresholdMs") => {
            Native::Number(ws_private::get_watchdog_ping_threshold_ms())
        }
        ("private", "ping") => {
            ws_private::ping();
            Native::Unit
        }
        ("public", "shouldEnforcePongMissReconnect") => {
            Native::Boolean(ws_public::should_enforce_pong_miss_reconnect())
        }
        ("public", "getWatchdogStaleThresholdMs") => {
            Native::Number(ws_public::get_watchdog_stale_threshold_ms())
        }
        ("public", "getWatchdogPingThresholdMs") => {
            Native::Number(ws_public::get_watchdog_ping_threshold_ms())
        }
        ("public", "ping") => {
            ws_public::ping();
            Native::Unit
        }
        _ => panic!("unexpected WS policy identity {socket}/{method}"),
    }
}

#[test]
fn ws_policy_matches_actual_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/ws_policy_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "WS-POLICY source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Capture = serde_json::from_slice(&output.stdout).unwrap();
    for row in &capture.rows {
        let expected = match row.kind.as_str() {
            "number" => Native::Number(f64::from_bits(
                u64::from_str_radix(row.bits.as_deref().unwrap(), 16).unwrap(),
            )),
            "boolean" => Native::Boolean(row.value.unwrap()),
            "undefined" => Native::Unit,
            _ => panic!("unexpected source native kind {}", row.kind),
        };
        let independent = match row.method.as_str() {
            "shouldEnforcePongMissReconnect" => Native::Boolean(false),
            "getWatchdogStaleThresholdMs" => Native::Number(90_000.0),
            "getWatchdogPingThresholdMs" => Native::Number(30_000.0),
            "ping" => Native::Unit,
            _ => panic!("unexpected source method {}", row.method),
        };
        let actual =
            std::panic::catch_unwind(|| native(&row.socket, &row.method)).unwrap_or(Native::Panic);
        assert_eq!(
            actual, independent,
            "CRYPTOCOM-WS-NATIVE {}/{} independent",
            row.socket, row.method
        );
        assert_eq!(
            actual, expected,
            "CRYPTOCOM-WS-NATIVE {}/{} actual source",
            row.socket, row.method
        );
    }
    // Every native result comparison precedes inventory/effect checks. No wire comparison.
    const IDS: [(&str, &str); 8] = [
        ("private", "shouldEnforcePongMissReconnect"),
        ("private", "getWatchdogStaleThresholdMs"),
        ("private", "getWatchdogPingThresholdMs"),
        ("private", "ping"),
        ("public", "shouldEnforcePongMissReconnect"),
        ("public", "getWatchdogStaleThresholdMs"),
        ("public", "getWatchdogPingThresholdMs"),
        ("public", "ping"),
    ];
    assert_eq!(
        capture.rows.len(),
        8,
        "CRYPTOCOM-WS-INVENTORY independent field total"
    );
    assert_eq!(
        capture
            .rows
            .iter()
            .map(|r| (r.socket.as_str(), r.method.as_str()))
            .collect::<Vec<_>>(),
        IDS,
        "CRYPTOCOM-WS-IDENTITIES"
    );
    assert!(
        capture.rows.iter().all(|r| r.touches == 0),
        "CRYPTOCOM-WS-NO-RECEIVER-EFFECTS"
    );
    println!("CRYPTOCOM-WS floor: 8/8 fields through public/ping");
}
