use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_store::Store;
use safe_cex_store::StoreMutationError;
use safe_cex_types::StoreSubscriberError;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

#[test]
fn first_subscriber_failure_surfaces_after_committed_setting_and_skips_fanout() {
    let out = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../safe-cex-store/tests/notification_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rows.len(), 8);
    for row in rows {
        let real = DefaultStore::new();
        let store: &dyn Store = &real;
        store
            .set_setting("isHedged", row["initial"].as_bool().unwrap())
            .unwrap();
        let trace = Rc::new(RefCell::new(Vec::<Value>::new()));
        let captured = trace.clone();
        let throws = row["listenerThrows"].as_bool().unwrap();
        let first = store.subscribe(Box::new(move |data, _| {
            captured
                .borrow_mut()
                .push(json!(["first", data.options.is_hedged]));
            if throws {
                Err(StoreSubscriberError::new("subscriber failed"))
            } else {
                Ok(())
            }
        }));
        let captured = trace.clone();
        let second = store.subscribe(Box::new(move |data, _| {
            captured
                .borrow_mut()
                .push(json!(["second", data.options.is_hedged]));
            Ok(())
        }));
        trace
            .borrow_mut()
            .push(json!(["post", "/papi/v1/um/positionSide/dual", "true"]));
        let result = store.set_setting("isHedged", true);
        let recovered: Result<(), &str> = match result {
            Ok(()) => Ok(()),
            Err(StoreMutationError::Notification(error)) => {
                assert_eq!(
                    error.subscriber_id(),
                    0,
                    "first failing subscriber retains its identity"
                );
                assert!(
                    store.options().is_hedged,
                    "mutation commits before callback error surfaces"
                );
                let emit = |message: &str| -> Result<(), &str> {
                    trace.borrow_mut().push(json!(["emit", message]));
                    if row["emitterThrows"].as_bool().unwrap() {
                        Err("emitter failed")
                    } else {
                        Ok(())
                    }
                };
                emit(error.message())
            }
            Err(error) => panic!("unexpected mutation error origin: {error}"),
        };
        assert_eq!(
            serde_json::to_value(&*trace.borrow()).unwrap(),
            row["trace"],
            "first subscriber failure must skip second and reach caller recovery"
        );
        assert_eq!(
            recovered.is_err(),
            row["rejected"].as_bool().unwrap(),
            "only caller emitter failure escapes"
        );
        assert!(store.options().is_hedged);
        first();
        second();
    }
}

#[test]
fn external_store_implementor_retains_subscriber_failure_origin() {
    use std::error::Error;
    let original: Rc<dyn Error> = Rc::new(std::io::Error::other("listener failed"));
    let callback_error = StoreSubscriberError::new("listener failed").with_cause(original.clone());
    let notification = safe_cex_store::StoreNotificationError::from_subscriber(197, callback_error);
    assert_eq!(notification.subscriber_id(), 197);
    assert!(
        std::ptr::eq(notification.cause().source().unwrap(), original.as_ref()),
        "external Store original cause identity"
    );
    assert!(
        matches!(
            StoreMutationError::from(notification),
            StoreMutationError::Notification(_)
        ),
        "external Store notification origin"
    );
}
