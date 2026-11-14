#![allow(dead_code)]
use safe_cex_engine::signing::{Identities, Identity, Provider};
use safe_cex_engine::{Engine, Venue};
pub fn agent() -> (Provider, Identities) {
    let policy = serde_json::from_str(r#"{"primary":{"source":"scalarOnly","signingAlgorithm":"secp256k1Digest","secretEncoding":"hex","trimSigning":false}}"#).unwrap();
    let (provider, identities, _) = safe_cex_crypto::signer::LocalSigner::from_json_with_facts(
        serde_json::json!({"primary":{"secret":format!("0x{}", "ab".repeat(32))}}).to_string(),
        policy,
        false,
    )
    .unwrap();
    (std::sync::Arc::new(provider), identities)
}
pub fn identity() -> Option<Identity> {
    agent().1.primary
}
pub fn start<V: Venue>(e: &mut Engine<V>, now: f64) {
    let (provider, identities) = agent();
    e.set_signer(Some(provider)).unwrap();
    e.set_identities(identities).unwrap();
    e.initialize_address_nonce(now - 1.).unwrap();
    e.start(now);
}

/// Most replay tapes test account/trading behavior after an already-approved agent.
/// Authorization failures have separate tests that deliver actual missing-agent responses.
pub fn prepared_actions<V: Venue>(e: &mut Engine<V>, now: f64) -> Vec<safe_cex_engine::Action> {
    use safe_cex_engine::{Action, Event, Response};
    let mut visible = vec![];
    for _ in 0..8 {
        let actions = safe_cex_engine::test_support::prepared_actions(e, now);
        if actions.is_empty() {
            break;
        }
        let mut replied = false;
        for action in actions {
            if let Action::Http(ref r) = action {
                let body: serde_json::Value =
                    serde_json::from_str(r.body.as_deref().unwrap_or("{}")).unwrap();
                if body["type"] == "approvedAgents" {
                    let response = serde_json::json!({"success":true,"code":0,"data":{"user":body["user"],"agents":[{"agentAddress": "0xe239cdc5fbe977a8a141b72194d3cf8c41bc5bc6", "agentName":"fixture"}]}}).to_string();
                    e.handle(
                        now,
                        Event::HttpResponse {
                            request: r.id,
                            response: Response {
                                status: 200,
                                headers: "",
                                body: &response,
                            },
                        },
                    );
                    replied = true;
                    continue;
                }
            }
            visible.push(action);
        }
        if !replied {
            break;
        }
    }
    visible
}
