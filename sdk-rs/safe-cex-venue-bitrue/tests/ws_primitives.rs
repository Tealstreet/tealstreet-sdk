use safe_cex_venue_bitrue::{ws_private, ws_public};
use serde::Deserialize;
use std::process::Command;

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Number,
    String,
    Other,
}

#[derive(Deserialize)]
struct CodeCase {
    id: String,
    kind: Kind,
    bits: Option<String>,
    units: Option<Vec<u16>>,
    value: bool,
    coercions: usize,
}

#[derive(Deserialize)]
struct PingCase {
    id: String,
    kind: String,
    touches: usize,
}

#[derive(Deserialize)]
struct Capture {
    codes: Vec<CodeCase>,
    pings: Vec<PingCase>,
}

#[test]
fn ws_primitives_match_actual_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/ws-primitives-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "WS-PRIMITIVES source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Capture = serde_json::from_slice(&output.stdout).unwrap();

    // Actual decoded booleans and native unit results precede all inventory checks.
    for case in &capture.codes {
        let code = match case.kind {
            Kind::Number => ws_private::SuccessCode::Number(f64::from_bits(
                u64::from_str_radix(case.bits.as_deref().unwrap(), 16).unwrap(),
            )),
            Kind::String => ws_private::SuccessCode::String(case.units.as_deref().unwrap()),
            Kind::Other => ws_private::SuccessCode::Other,
        };
        let native = ws_private::is_successful_code(code);
        let independent = matches!(
            case.id.as_str(),
            "n-zero" | "n-neg-zero" | "n-200" | "s-zero" | "s-200"
        );
        assert_eq!(
            native, independent,
            "BITRUE-WS-CODE-NATIVE independent {}",
            case.id
        );
        assert_eq!(
            native, case.value,
            "BITRUE-WS-CODE-NATIVE actual source {}",
            case.id
        );
    }
    for case in &capture.pings {
        let source_unit: Result<(), ()> = match case.kind.as_str() {
            "undefined" => Ok(()),
            other => panic!("unexpected source native ping return {other}"),
        };
        let ping = match case.id.as_str() {
            "private" => ws_private::ping,
            "public" => ws_public::ping,
            other => panic!("unexpected source ping {other}"),
        };
        let native = std::panic::catch_unwind(ping).map_err(|_| ());
        assert_eq!(native, source_unit, "BITRUE-WS-PING-NATIVE {}", case.id);
    }

    const IDS: [&str; 46] = [
        "n-zero",
        "n-neg-zero",
        "n-200",
        "n-one",
        "n-negative",
        "n-201",
        "n-nan",
        "n-pos-inf",
        "n-neg-inf",
        "n-max",
        "n-min",
        "n-subnormal",
        "n-next-200",
        "s-zero",
        "s-200",
        "s-empty",
        "s-doublezero",
        "s-spacezero",
        "s-pluszero",
        "s-minuszero",
        "s-2e2",
        "s-decimal",
        "s-nullbyte",
        "s-surrogate-high",
        "s-surrogate-low",
        "s-pair",
        "s-zero-surrogate",
        "s-surrogate-200",
        "s-200-surrogate",
        "s-fullwidthzero",
        "s-200-space",
        "s-zero-space",
        "o-undefined",
        "o-null",
        "o-false",
        "o-true",
        "o-bigint-zero",
        "o-bigint-200",
        "o-symbol",
        "o-object",
        "o-array",
        "o-function",
        "o-number-object",
        "o-string-object",
        "o-coercion-trap",
        "o-proxy-trap",
    ];
    assert_eq!(capture.codes.len(), 46, "BITRUE-WS-CODE-INVENTORY");
    assert_eq!(
        capture
            .codes
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        IDS,
        "BITRUE-WS-CODE-IDENTITIES"
    );
    assert!(
        capture.codes.iter().all(|c| c.coercions == 0),
        "BITRUE-WS-CODE-NO-COERCION"
    );
    assert_eq!(capture.pings.len(), 2, "BITRUE-WS-PING-INVENTORY");
    assert_eq!(
        capture
            .pings
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        ["private", "public"],
        "BITRUE-WS-PING-IDENTITIES"
    );
    assert!(
        capture.pings.iter().all(|c| c.touches == 0),
        "BITRUE-WS-PING-NO-RECEIVER-EFFECTS"
    );
    println!("BITRUE-WS floor: code 46/46 through o-proxy-trap; ping 2/2 through public");
}
