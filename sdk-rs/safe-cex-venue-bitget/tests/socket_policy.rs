use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_venue_bitget::{
    account_predicates::promote_uta_routing_from_classic_unsupported,
    socket_ping::{ping_private, ping_public, ping_secondary_public},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    process::Command,
};

#[derive(Deserialize)]
struct PingInput {
    kind: String,
    disposed: bool,
    fail: bool,
    now: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PromotionInput {
    present: bool,
    unsupported: bool,
    supports: bool,
    testnet: bool,
    initial_uta: bool,
    initial_detected: bool,
    failure: Option<String>,
}
enum Observation {
    Ping {
        at: f64,
        events: Vec<&'static str>,
        error: bool,
    },
    Promotion {
        uta: bool,
        detected: bool,
        returned: Option<bool>,
        events: Vec<&'static str>,
        error: bool,
    },
}
impl Observation {
    fn wire(self) -> Value {
        match self {
            Self::Ping { at, events, error } => {
                json!({"at":if at.to_bits()==(-0.0_f64).to_bits(){"-0".into()}else{at.to_string()},"events":events,"error":error})
            }
            Self::Promotion {
                uta,
                detected,
                returned,
                events,
                error,
            } => {
                json!({"uta":uta,"detected":detected,"returned":returned,"events":events,"error":error})
            }
        }
    }
}
fn native_ping(input: PingInput) -> Observation {
    let now: f64 = input.now.parse().unwrap();
    let expected_now = if input.now == "-0" { -0.0_f64 } else { 103.5 };
    assert_eq!(
        now.to_bits(),
        expected_now.to_bits(),
        "PING decoded clock bits"
    );
    let at = Cell::new(37.5_f64);
    let events = RefCell::new(Vec::new());
    let method = if input.kind == "private" {
        "sendWithLog"
    } else {
        "send"
    };
    let original = Box::new(
        ExchangeError::new("send failed").with_retained_response(json!({"marker":59}).into()),
    );
    let pointer = std::ptr::from_ref(original.as_ref());
    let clock = || {
        events.borrow_mut().push("clock");
        now
    };
    let send = |text: &str| {
        assert_eq!(text, "ping", "PING native text {}", input.kind);
        assert_eq!(
            at.get().to_bits(),
            expected_now.to_bits(),
            "PING native timestamp before send {}",
            input.kind
        );
        events.borrow_mut().push(method);
        if input.fail { Err(original) } else { Ok(()) }
    };
    let result = match input.kind.as_str() {
        "public" => ping_public(input.disposed, &at, clock, send),
        "secondary" => ping_secondary_public(input.disposed, &at, clock, send),
        "private" => ping_private(input.disposed, &at, clock, send),
        _ => panic!("PING unknown fixture"),
    };
    assert_eq!(
        result.is_err(),
        !input.disposed && input.fail,
        "PING native disposition {}",
        input.kind
    );
    if let Err(error) = result {
        assert_eq!(
            std::ptr::from_ref(error.as_ref()),
            pointer,
            "PING native original error"
        );
        assert_eq!(error.message, "send failed", "PING native error message");
        assert_eq!(
            error.response_json_view(),
            Some(json!({"marker":59})),
            "PING native error metadata"
        );
    }
    assert_eq!(
        at.get().to_bits(),
        if input.disposed {
            37.5_f64.to_bits()
        } else {
            expected_now.to_bits()
        },
        "PING native final timestamp {}",
        input.kind
    );
    let events = events.into_inner();
    let expected_events = if input.disposed {
        Vec::new()
    } else {
        vec!["clock", method]
    };
    assert_eq!(
        events, expected_events,
        "PING native effects {}",
        input.kind
    );
    Observation::Ping {
        at: at.get(),
        events,
        error: !input.disposed && input.fail,
    }
}
fn native_promotion(input: PromotionInput) -> Observation {
    let payload = input.present.then(|| json!({"code":"40085","marker":59}));
    let uta = Cell::new(input.initial_uta);
    let detected = Cell::new(input.initial_detected);
    let events = RefCell::new(Vec::new());
    let original = Box::new(ExchangeError::new("route read failed"));
    let pointer = std::ptr::from_ref(original.as_ref());
    let error = RefCell::new(Some(original));
    let step = |name, value| {
        events.borrow_mut().push(name);
        if input.failure.as_deref() == Some(name) {
            Err(error.borrow_mut().take().unwrap())
        } else {
            Ok(value)
        }
    };
    let result = promote_uta_routing_from_classic_unsupported(
        payload.as_ref(),
        |received| {
            match (received, payload.as_ref()) {
                (Some(actual), Some(expected)) => {
                    assert!(
                        std::ptr::eq(actual, expected),
                        "PROMOTION native payload identity"
                    );
                    assert_eq!(
                        actual.get("code"),
                        Some(&json!("40085")),
                        "PROMOTION decoded payload code"
                    );
                    assert_eq!(
                        actual.get("marker"),
                        Some(&json!(59)),
                        "PROMOTION decoded payload marker"
                    );
                }
                (None, None) => {}
                _ => panic!("PROMOTION native payload presence"),
            }
            step("predicate", input.unsupported)
        },
        || step("supports", input.supports),
        || step("testnet", input.testnet),
        &uta,
        &detected,
    );
    let promoted = input.failure.is_none() && input.unsupported && input.supports && !input.testnet;
    assert_eq!(
        uta.get(),
        promoted || input.initial_uta,
        "PROMOTION native UTA flag"
    );
    assert_eq!(
        detected.get(),
        promoted || input.initial_detected,
        "PROMOTION native detected flag"
    );
    let returned = match result {
        Ok(value) => {
            assert!(input.failure.is_none(), "PROMOTION native missing error");
            assert_eq!(value, promoted, "PROMOTION native result");
            Some(value)
        }
        Err(error) => {
            assert!(input.failure.is_some(), "PROMOTION native unexpected error");
            assert_eq!(
                std::ptr::from_ref(error.as_ref()),
                pointer,
                "PROMOTION native original error"
            );
            assert_eq!(
                error.message, "route read failed",
                "PROMOTION native error message"
            );
            None
        }
    };
    let mut expected_events = vec!["predicate"];
    if input.unsupported && input.failure.as_deref() != Some("predicate") {
        expected_events.push("supports");
        if input.supports && input.failure.as_deref() != Some("supports") {
            expected_events.push("testnet");
        }
    }
    let events = events.into_inner();
    assert_eq!(events, expected_events, "PROMOTION native lazy reads");
    Observation::Promotion {
        uta: uta.get(),
        detected: detected.get(),
        returned,
        events,
        error: input.failure.is_some(),
    }
}
fn inputs() -> Vec<Value> {
    let mut inputs = Vec::new();
    for kind in ["public", "secondary", "private"] {
        for disposed in [false, true] {
            for fail in [false, true] {
                for now in ["-0", "103.5"] {
                    inputs.push(json!({"kind":kind,"disposed":disposed,"fail":fail,"now":now}));
                }
            }
        }
    }
    for present in [false, true] {
        for unsupported in [false, true] {
            for supports in [false, true] {
                for testnet in [false, true] {
                    for initial_uta in [false, true] {
                        inputs.push(json!({"kind":"promotion","present":present,"unsupported":unsupported,"supports":supports,"testnet":testnet,"initialUta":initial_uta,"initialDetected":!initial_uta,"failure":null}));
                    }
                }
            }
        }
    }
    for failure in ["predicate", "supports", "testnet"] {
        inputs.push(json!({"kind":"promotion","present":true,"unsupported":true,"supports":true,"testnet":false,"initialUta":false,"initialDetected":true,"failure":failure}));
    }
    inputs
}
fn native(input: Value) -> Observation {
    if input["kind"] == "promotion" {
        native_promotion(serde_json::from_value(input).unwrap())
    } else {
        native_ping(serde_json::from_value(input).unwrap())
    }
}
#[test]
fn socket_policy_native() {
    let inputs = inputs();
    assert_eq!(inputs.len(), 59, "POLICY independent native floor");
    for input in inputs {
        native(input);
    }
}
#[test]
#[ignore = "requires Node and TypeScript; included in venue oracle"]
fn socket_policy_matches_actual_source() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/socket-policy-capture.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "POLICY actual source capture {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = capture["cases"].as_array().unwrap();
    let mut native_results = Vec::new();
    for (index, expected_input) in inputs().into_iter().enumerate() {
        let case = cases.get(index).expect("POLICY required native input");
        assert_eq!(
            case.get("input"),
            Some(&expected_input),
            "POLICY source input identity {index}"
        );
        native_results.push(native(case["input"].clone()));
    }
    assert_eq!(native_results.len(), 59, "POLICY native consumed floor");
    assert_eq!(cases.len(), 59, "POLICY source consumed floor");
    // All decoded native state/identity assertions above finish before any
    // native observation is projected to JSON or compared with source output.
    for (index, result) in native_results.into_iter().enumerate() {
        assert_eq!(
            cases[index].get("output"),
            Some(&result.wire()),
            "POLICY source output {index}"
        );
    }
}
