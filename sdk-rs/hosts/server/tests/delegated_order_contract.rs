use safe_cex_exchange_server::{envelope, order_contract as contract};
use serde_json::{Value, json};

fn fixture() -> Value {
    serde_json::from_str(include_str!("data/delegated-orders-v1.json")).unwrap()
}

#[test]
fn frozen_order_frames_key_epoch_and_canonical_fingerprint() {
    let f = fixture();
    let id = f["key"].as_str().unwrap();
    assert!(id.len() <= contract::KEY_MAX && id.len() == 36);
    assert_eq!(contract::key_time(id), f["createdMs"].as_u64());
    let now = f["createdMs"].as_u64().unwrap();
    assert_eq!(contract::check_key(id, now + 9999), Ok(()));
    assert_eq!(
        contract::check_key(id, now + 10_000),
        Err("not sent: expired")
    );
    assert!(contract::check_key(id, now - 1).is_err());
    assert!(contract::key_time(&id.to_uppercase()).is_none());
    assert!(contract::key_time("01ARZ3NDEKTSV4RRFFQ69G5FAV").is_none());
    assert_eq!(
        contract::fingerprint("main", id, &f["submit"]["d"]["params"], 1).unwrap(),
        f["fingerprint"]
    );
    let mut changed = f["submit"]["d"]["params"].clone();
    changed["amount"] = json!(0.002);
    assert_ne!(
        contract::fingerprint("main", id, &changed, 1).unwrap(),
        f["fingerprint"]
    );
    let submit: envelope::Inbound = serde_json::from_value(f["submit"].clone()).unwrap();
    let envelope::Inbound::OrderSubmit(mut submit) = submit else {
        panic!("submit shape changed");
    };
    assert_eq!(serde_json::to_value(&submit).unwrap(), f["submit"]["d"]);
    for age in [0.0, -0.0, 1.0, 1.25, 9999.5] {
        submit.age_ms = age;
        let encoded = serde_json::to_value(&submit).unwrap();
        assert_eq!(encoded["ageMs"].as_f64(), Some(age));
        if age == 0.0 {
            assert_eq!(encoded["ageMs"], json!(0));
        }
        let decoded: contract::OrderSubmit = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded.age_ms, age);
    }
    let cancel: envelope::Inbound = serde_json::from_value(f["cancelQueued"].clone()).unwrap();
    let envelope::Inbound::OrderCancelQueued(cancel) = cancel else {
        panic!("cancel shape changed");
    };
    assert_eq!(
        serde_json::to_value(cancel).unwrap(),
        f["cancelQueued"]["d"]
    );
    for name in ["ack", "outcome"] {
        let data = f[name]["d"].clone();
        let text = envelope::order_frame(f[name]["t"].as_str().unwrap(), "main", data);
        assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), f[name]);
        assert!(
            text.len()
                <= if name == "ack" {
                    contract::ACK_MAX
                } else {
                    contract::OUTCOME_MAX
                }
        );
    }
}

#[test]
fn capability_advertisement_matches_the_compiled_activation_gate() {
    assert_eq!(
        safe_cex_exchange_server::http::capabilities_json()["transports"]["accountWorkerWebSocket"]
            ["delegatedOrderFramesV1"],
        cfg!(feature = "delegated-order-frames")
    );
    assert_eq!(
        contract::enabled(),
        cfg!(feature = "delegated-order-frames")
    );
}
