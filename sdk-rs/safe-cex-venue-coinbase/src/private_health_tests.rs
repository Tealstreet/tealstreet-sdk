use super::CoinbasePrivateUserStreamHealth;
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_store::{Store, StoreMutationError, store::DefaultStore};
use safe_cex_types::StoreSubscriberError;
use serde_json::{Map, Value, json};
use std::{cell::RefCell, error::Error, process::Command, rc::Rc};

#[path = "../tests/support/compare.rs"]
mod comparison;

#[test]
fn private_user_stream_health_matches_live_source_and_keeps_notification_finding() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/private-health.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut expected: Value = serde_json::from_slice(&output.stdout).unwrap();

    for case in expected["healthIdentityCases"].as_array().unwrap() {
        assert_eq!(case["label"], "field-identity-supplement");
        let values = case["value"].as_object().unwrap();
        let store = DefaultStore::new();
        let initial_health = values
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<Map<String, Value>>();
        Store::update_socket_health(&store, &initial_health).unwrap();
        let current = Store::socket_health(&store);
        for (key, value) in values {
            assert_eq!(
                current.as_ref().and_then(|current| current.get(key)),
                Some(value),
                "socketHealth.{key} identity"
            );
        }
    }
    expected
        .as_object_mut()
        .unwrap()
        .remove("healthIdentityCases");

    let store = DefaultStore::new();
    let mut initial_health = Map::new();
    initial_health.insert("publicHealthy".into(), json!(true));
    initial_health.insert("publicInitialized".into(), json!(true));
    initial_health.insert("privateDegradedReason".into(), json!("stale"));
    Store::update_socket_health(&store, &initial_health).unwrap();
    let notifications = Rc::new(RefCell::new(Vec::<Value>::new()));
    let observed = notifications.clone();
    let _unsubscribe = Store::subscribe(
        &store,
        Box::new(move |data, _| {
            observed
                .borrow_mut()
                .push(data.socket_health.clone().unwrap());
            Ok(())
        }),
    );
    let health = CoinbasePrivateUserStreamHealth::default();
    let effects = Rc::new(RefCell::new(Vec::new()));
    let mut checkpoints = Vec::new();
    let snapshot = || {
        json!({
            "lastMessageAt": health.last_message_at(),
            "hasWarned": health.has_warned_degraded(),
            "socketHealth": serde_json::to_value(Store::serialize(&store)).unwrap()["socketHealth"],
        })
    };

    health
        .mark_private_user_stream_message_received(&store, 100.0)
        .unwrap();
    checkpoints.push(snapshot());
    health
        .mark_private_user_stream_degraded(
            &store,
            Some("private stream stalled"),
            true,
            |message, severity| {
                effects.borrow_mut().push(json!(["log", message, severity]));
                Ok(())
            },
            |message| {
                effects
                    .borrow_mut()
                    .push(json!(["warning", message, {"forceToast":true}]));
                Ok(())
            },
        )
        .unwrap();
    checkpoints.push(snapshot());
    health
        .mark_private_user_stream_degraded(
            &store,
            Some("private stream still stalled"),
            true,
            |message, severity| {
                effects.borrow_mut().push(json!(["log", message, severity]));
                Ok(())
            },
            |message| {
                effects
                    .borrow_mut()
                    .push(json!(["warning", message, {"forceToast":true}]));
                Ok(())
            },
        )
        .unwrap();
    checkpoints.push(snapshot());
    health
        .mark_private_user_stream_message_received(&store, 400.0)
        .unwrap();
    checkpoints.push(snapshot());
    health
        .mark_private_user_stream_degraded(
            &store,
            None,
            true,
            |message, severity| {
                effects.borrow_mut().push(json!(["log", message, severity]));
                Ok(())
            },
            |message| {
                effects
                    .borrow_mut()
                    .push(json!(["warning", message, {"forceToast":true}]));
                Ok(())
            },
        )
        .unwrap();
    checkpoints.push(snapshot());

    health
        .mark_private_user_stream_auth_failed(
            &store,
            Some("invalid signature"),
            |message, severity| {
                effects.borrow_mut().push(json!(["log", message, severity]));
                Ok(())
            },
            |message| {
                effects
                    .borrow_mut()
                    .push(json!(["warning", message, {"forceToast":true}]));
                Ok(())
            },
        )
        .unwrap();
    checkpoints.push(snapshot());
    health
        .mark_private_user_stream_auth_failed(
            &store,
            None,
            |message, severity| {
                effects.borrow_mut().push(json!(["log", message, severity]));
                Ok(())
            },
            |message| {
                effects
                    .borrow_mut()
                    .push(json!(["warning", message, {"forceToast":true}]));
                Ok(())
            },
        )
        .unwrap();
    checkpoints.push(snapshot());
    health
        .mark_private_user_stream_auth_failed(
            &store,
            Some(""),
            |message, severity| {
                effects.borrow_mut().push(json!(["log", message, severity]));
                Ok(())
            },
            |message| {
                effects
                    .borrow_mut()
                    .push(json!(["warning", message, {"forceToast":true}]));
                Ok(())
            },
        )
        .unwrap();
    checkpoints.push(snapshot());

    let actual = json!({
        "lastMessageAt": health.last_message_at(),
        "hasWarned": health.has_warned_degraded(),
        "socketHealth": serde_json::to_value(Store::serialize(&store)).unwrap()["socketHealth"],
        "notifications": notifications.borrow().clone(),
        "effects": effects.borrow().clone(),
        "checkpoints": checkpoints,
    });
    comparison::compare(&actual, &expected, "Coinbase private stream health");
    assert_eq!(notifications.borrow().len(), 8);
}

#[test]
fn subscriber_failure_propagates_after_health_commit_and_before_callbacks() {
    let store = DefaultStore::new();
    let mut initial = Map::new();
    initial.insert("publicHealthy".into(), json!(true));
    Store::update_socket_health(&store, &initial).unwrap();

    let notifications = Rc::new(RefCell::new(Vec::new()));
    let first_calls = notifications.clone();
    let _first = Store::subscribe(
        &store,
        Box::new(move |data, _| {
            first_calls
                .borrow_mut()
                .push(data.socket_health.clone().unwrap());
            Err(StoreSubscriberError::new("subscriber failed"))
        }),
    );
    let later_calls = notifications.clone();
    let _second = Store::subscribe(
        &store,
        Box::new(move |_, _| {
            later_calls
                .borrow_mut()
                .push(json!("second subscriber called"));
            Ok(())
        }),
    );

    let effects = Rc::new(RefCell::new(Vec::new()));
    let logged = effects.clone();
    let warned = effects.clone();
    let health = CoinbasePrivateUserStreamHealth::default();
    let result = health.mark_private_user_stream_degraded(
        &store,
        Some("private stream stalled"),
        true,
        move |message, severity| {
            logged.borrow_mut().push(json!(["log", message, severity]));
            Ok(())
        },
        move |message| {
            warned.borrow_mut().push(json!(["warning", message]));
            Ok(())
        },
    );

    let failure = result.expect_err("health publication retains the Store subscriber failure");
    assert!(
        failure.value.is_none(),
        "Store failure is retained without JSON conversion"
    );
    match failure.store_mutation() {
        Some(StoreMutationError::Notification(error)) => {
            assert_eq!(
                error.subscriber_id(),
                0,
                "first failing Store-local subscriber"
            );
            assert_eq!(error.message(), "subscriber failed");
            assert_eq!(error.cause().message(), "subscriber failed");
        }
        other => panic!("expected original Store notification failure, got {other:?}"),
    }
    let committed = Store::socket_health(&store).unwrap();
    assert_eq!(committed["privateDegradedReason"], "private stream stalled");
    assert_eq!(
        notifications.borrow().len(),
        1,
        "later subscriber is skipped"
    );
    assert!(
        effects.borrow().is_empty(),
        "throwing TS update skips later effects"
    );
    assert!(!health.has_warned_degraded());

    let message_store = DefaultStore::new();
    let mut stale = Map::new();
    stale.insert("privateDegradedReason".into(), json!("stale"));
    Store::update_socket_health(&message_store, &stale).unwrap();
    let _failing_subscriber = Store::subscribe(
        &message_store,
        Box::new(|_, _| Err(StoreSubscriberError::new("message subscriber failed"))),
    );
    let message_health = CoinbasePrivateUserStreamHealth::default();
    let result = message_health.mark_private_user_stream_message_received(&message_store, 25.0);
    assert!(matches!(
        result,
        Err(StoreMutationError::Notification(error))
            if error.subscriber_id() == 0 && error.cause().message() == "message subscriber failed"
    ));
    assert_eq!(message_health.last_message_at(), 25.0);
    assert!(
        Store::socket_health(&message_store)
            .unwrap()
            .get("privateDegradedReason")
            .is_none()
    );
}

#[test]
fn subscriber_id_survives_store_to_exchange_error_conversion() {
    let store = DefaultStore::new();
    let mut cause = ExchangeError::new("subscriber failed");
    cause.response = Some(json!({"status": 503}));
    let subscriber_cause: Rc<dyn Error> = Rc::new(cause);
    let _subscriber = Store::subscribe(
        &store,
        Box::new(move |_, _| {
            Err(StoreSubscriberError::new("subscriber failed").with_cause(subscriber_cause.clone()))
        }),
    );

    let mutation = CoinbasePrivateUserStreamHealth::default()
        .mark_private_user_stream_degraded(
            &store,
            Some("degraded"),
            false,
            |_, _| Ok(()),
            |_| Ok(()),
        )
        .expect_err("subscriber failure is returned after the Store mutation");
    assert!(
        mutation.value.is_none(),
        "original Store failure retained without encoding"
    );
    let exchange_error = ExchangeError::from(mutation.store_mutation().unwrap().clone());

    assert_eq!(exchange_error.message, "subscriber failed");
    assert_eq!(exchange_error.store_subscriber_ids, [0]);
    assert_eq!(exchange_error.response, Some(json!({"status": 503})));
    assert_eq!(
        Store::socket_health(&store).unwrap()["privateDegradedReason"],
        "degraded",
        "Store state commits before subscriber failure is returned"
    );
}
