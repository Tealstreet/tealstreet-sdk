use safe_cex_exchange_server::{envelope, http, order_contract};
use serde_json::Value;
#[test]
fn action_frames_decode_the_shared_versioned_contract() {
    let fixture: Value =
        serde_json::from_str(include_str!("data/delegated-actions-v1.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        for kind in ["request", "cancelQueued"] {
            assert!(
                serde_json::from_value::<envelope::Inbound>(case[kind].clone()).is_ok(),
                "{} {kind}",
                case["name"]
            );
        }
    }
}
#[test]
fn action_capability_requires_compiled_and_startup_admission() {
    let startup = std::env::var("TEALSTREET_DELEGATED_ORDER_FRAMES").as_deref() == Ok("1");
    assert_eq!(
        http::capabilities_json()["transports"]["accountWorkerWebSocket"]["delegatedOrderActionsV1"],
        order_contract::enabled() && startup
    );
}

#[test]
fn action_fingerprints_match_browser_vectors_and_bind_targets() {
    let fixture: Value =
        serde_json::from_str(include_str!("data/delegated-actions-v1.json")).unwrap();
    let id = fixture["key"].as_str().unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let action = &case["action"];
        let fp = order_contract::action_fingerprint("main", id, action, 1).unwrap();
        assert_eq!(fp, case["fingerprint"]);
        assert_ne!(
            fp,
            order_contract::fingerprint("main", id, action, 1).unwrap()
        );
        assert_ne!(
            fp,
            order_contract::action_fingerprint("other", id, action, 1).unwrap()
        );
        assert_ne!(
            fp,
            order_contract::action_fingerprint("main", id, action, 2).unwrap()
        );
        let decoded: envelope::Inbound = serde_json::from_value(case["request"].clone()).unwrap();
        let envelope::Inbound::OrderAction(request) = decoded else {
            panic!("action shape")
        };
        assert_eq!(serde_json::to_value(request).unwrap(), case["request"]["d"]);
        if action.get("order").is_some() {
            let mut other = action.clone();
            other["order"]["id"] = "different-target".into();
            assert_ne!(
                fp,
                order_contract::action_fingerprint("main", id, &other, 1).unwrap()
            );
        }
    }
}
#[test]
fn unsupported_action_fields_and_invalid_updates_do_not_get_a_fingerprint() {
    use serde_json::json;
    for action in [
        json!({"kind":"cancelAll","extra":true}),
        json!({"kind":"amend","order":{"id":"one","symbol":"BTCUSDT"},"update":{"price":null,"amount":1}}),
        json!({"kind":"cancel","order":{"id":"","symbol":"BTCUSDT"}}),
        json!({"kind":"amend","order":{"id":"one","symbol":"BTCUSDT"},"update":{"price":1e21}}),
    ] {
        assert!(
            order_contract::action_fingerprint(
                "main",
                "018bcfe5-6800-7101-8101-010101010101",
                &action,
                1
            )
            .is_err(),
            "{action}"
        );
    }
}
