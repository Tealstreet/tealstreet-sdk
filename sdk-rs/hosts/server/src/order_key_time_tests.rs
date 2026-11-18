use super::*;

fn key(created: u64) -> String {
    let hex = format!("{created:012x}");
    format!("{}-{}-7101-8101-010101010101", &hex[..8], &hex[8..])
}

#[test]
fn bounded_future_intent_keys_are_accepted() {
    let now = 1_700_000_000_000;
    for ahead in [1, 14, 500, 4_999, 5_000] {
        assert_eq!(check_key(&key(now + ahead), now), Ok(()), "ahead={ahead}");
    }
}

#[test]
fn far_future_intent_keys_still_refuse() {
    let now = 1_700_000_000_000;
    for ahead in [5_001, 6_000, 60_000] {
        assert_eq!(
            check_key(&key(now + ahead), now),
            Err("not sent: invalid key time")
        );
    }
}

#[test]
fn ten_second_key_age_and_uuid_validation_are_unchanged() {
    let now = 1_700_000_000_000;
    for age in [0, 1, 9_999] {
        assert_eq!(check_key(&key(now - age), now), Ok(()));
    }
    for age in [10_000, 10_001, 60_000] {
        assert_eq!(check_key(&key(now - age), now), Err("not sent: expired"));
    }
    assert_eq!(
        check_key("invalid-key-canary", now),
        Err("not sent: invalid intent key")
    );
}

#[test]
fn accepted_key_skew_audit_is_closed_and_bucket_only() {
    let now = 1_700_000_000_000;
    for (ahead, expected) in [
        (0, None),
        (1, Some("le_1s")),
        (1_000, Some("le_1s")),
        (1_001, Some("gt_1s_le_3s")),
        (3_000, Some("gt_1s_le_3s")),
        (3_001, Some("gt_3s_le_5s")),
        (5_000, Some("gt_3s_le_5s")),
        (5_001, None),
    ] {
        let id = key(now + ahead);
        let capture = crate::state::log_capture::capture();
        let _ = check_key(&id, now);
        let lines = capture.finish();
        if let Some(bucket) = expected {
            assert_eq!(lines.len(), 1, "ahead={ahead}");
            assert!(lines[0].len() < 256 && !lines[0].contains(&id));
            let outer: Value = serde_json::from_str(&lines[0]).unwrap();
            assert_eq!(outer["level"], "info");
            let fields: Value = serde_json::from_str(outer["msg"].as_str().unwrap()).unwrap();
            assert_eq!(
                fields,
                json!({"event":"order.intent_clock_skew","skew_bucket":bucket})
            );
        } else {
            assert!(lines.is_empty(), "ahead={ahead}: {lines:?}");
        }
    }
    let capture = crate::state::log_capture::capture();
    assert!(check_key(&key(now - 1), now).is_ok());
    assert!(check_key("invalid-key-canary", now).is_err());
    assert!(capture.finish().is_empty());
}
