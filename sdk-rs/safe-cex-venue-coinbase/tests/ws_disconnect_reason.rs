use safe_cex_venue_coinbase::starbase_session::{
    CoinbaseStarbaseCancelOnDisconnectPolicy as Policy,
    CoinbaseStarbaseCancelOnDisconnectScope as Scope,
    CoinbaseStarbaseWebsocketDisconnectReason as Reason,
    CoinbaseStarbaseWebsocketSessionState as State,
};
use serde_json::Value;
use std::process::Command;

fn oracle() -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/ws-disconnect-reasons.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn websocket_session_literal_unions_match_closed_typescript_sets() {
    let oracle = oracle();
    let accepted = oracle["accepted"].as_array().unwrap();
    assert_eq!(accepted.len(), 3, "source accepted-spelling total");
    let expected = [
        ("heartbeat-timeout", Reason::HeartbeatTimeout),
        ("rate-limit", Reason::RateLimit),
        ("transport", Reason::Transport),
    ];
    assert_eq!(
        accepted.len(),
        expected.len(),
        "Rust accepted-set inventory"
    );
    for (source, (spelling, variant)) in accepted.iter().zip(expected) {
        assert_eq!(source.as_str(), Some(spelling), "source spelling");
        assert_eq!(
            serde_json::from_value::<Reason>(source.clone()).unwrap(),
            variant
        );
        assert_eq!(serde_json::to_value(variant).unwrap(), *source);
        assert_eq!(
            serde_json::from_str::<Reason>(&serde_json::to_string(source).unwrap()).unwrap(),
            variant,
            "qualified public Deserialize path for {spelling}"
        );
    }

    let rejected = oracle["rejected"].as_array().unwrap();
    assert_eq!(rejected.len(), 10, "source rejected-shape total");
    for value in rejected {
        assert!(
            serde_json::from_value::<Reason>(value.clone()).is_err(),
            "TypeScript-rejected disconnect reason accepted by Rust: {value}"
        );
    }

    let states = oracle["sessionStates"].as_array().unwrap();
    assert_eq!(states.len(), 6, "source session-state accepted total");
    let expected_states = [
        ("idle", State::Idle),
        ("activating", State::Activating),
        ("ready", State::Ready),
        ("disconnected", State::Disconnected),
        ("rate-limit-backoff", State::RateLimitBackoff),
        ("failed", State::Failed),
    ];
    assert_eq!(
        states.len(),
        expected_states.len(),
        "Rust session-state inventory"
    );
    for (source, (spelling, variant)) in states.iter().zip(expected_states) {
        assert_eq!(source.as_str(), Some(spelling), "source state spelling");
        assert_eq!(
            serde_json::from_value::<State>(source.clone()).unwrap(),
            variant
        );
        assert_eq!(serde_json::to_value(variant).unwrap(), *source);
    }
    let rejected_states = oracle["sessionStateRejected"].as_array().unwrap();
    assert_eq!(
        rejected_states.len(),
        13,
        "source rejected session-state total"
    );
    for value in rejected_states {
        assert!(
            serde_json::from_value::<State>(value.clone()).is_err(),
            "TypeScript-rejected session state accepted by Rust: {value}"
        );
    }

    assert_eq!(
        oracle["policyFields"],
        serde_json::json!([
            {"name":"enabled","optional":false,"type":"boolean"},
            {"name":"scope","optional":true,"type":"CoinbaseStarbaseCancelOnDisconnectScope"}
        ]),
        "source policy declaration shape"
    );
    let omitted_scope = Policy {
        enabled: true,
        scope: None,
    };
    assert!(omitted_scope.enabled, "native policy enabled identity");
    assert_eq!(omitted_scope.scope, None, "native optional scope identity");
    assert_eq!(
        serde_json::to_value(&omitted_scope).unwrap(),
        serde_json::json!({"enabled":true}),
        "absent optional scope serialization"
    );
    let account_scope = Policy {
        enabled: false,
        scope: Some(Scope::Account),
    };
    assert!(!account_scope.enabled, "native policy enabled identity");
    assert_eq!(
        account_scope.scope,
        Some(Scope::Account),
        "native policy scope identity"
    );
    assert_eq!(
        serde_json::to_value(&account_scope).unwrap(),
        serde_json::json!({"enabled":false,"scope":"account"}),
        "present optional scope serialization"
    );
}
