use super::*;

fn hosted_replies(reads: &[(u32, &str, &str)], notices: &[Value]) -> Vec<Value> {
    let (out, mut sent, _) = Out::channel(16);
    let pending = Arc::new(Mutex::new(HashMap::new()));
    for &(command, request_id, method) in reads {
        pending.lock().unwrap().insert(
            command,
            Pending {
                request_id: request_id.into(),
                method: method.into(),
                reply: methods::Reply::Result,
                group: None,
                incomplete: None,
                audit: None,
            },
        );
    }
    for notice in notices {
        notice_to_msg(&out, "history-account", notice, &pending);
    }
    let mut replies = Vec::new();
    while let Ok(text) = sent.try_recv() {
        let frame: Value = serde_json::from_str(&text).unwrap();
        if frame["d"]["op"] == "callMethod:done" {
            replies.push(frame["d"]["data"].clone());
        }
    }
    assert!(pending.lock().unwrap().is_empty());
    replies
}

fn incomplete(fields: Value) -> Value {
    json!({"type":"event", "level":"info", "code":"history.incomplete",
        "message":"partial history", "fields":fields})
}

#[test]
fn hosted_history_later_page_metadata_stays_with_its_interleaved_reply() {
    let replies = hosted_replies(
        &[
            (11, "orders", "fetchHistoricalOrders"),
            (22, "closed", "fetchHistoricalPositions"),
            (33, "complete", "fetchHistoricalFills"),
        ],
        &[
            incomplete(json!({"command":11,"reason":"laterPageFailed",
                "historyOperation":"bybitOrders","historyFailureKind":"rateLimited",
                "historyVenueCode":"10006","nativeMessage":"PRIVATE-CANARY"})),
            incomplete(json!({"command":22,"reason":"laterPageFailed",
                "historyOperation":"bybitClosedPnl","historyFailureKind":"network"})),
            json!({"type":"done","command":33,"result":"[{\"id\":\"fill-row\"}]"}),
            json!({"type":"done","command":22,"result":"[{\"id\":\"closed-row\"}]"}),
            json!({"type":"done","command":11,"result":"[{\"id\":\"order-row\"}]"}),
        ],
    );
    assert_eq!(
        replies[0],
        json!({"requestId":"complete","success":true,"result":[{"id":"fill-row"}]})
    );
    assert_eq!(
        replies[1],
        json!({"requestId":"closed","success":true,"result":[{"id":"closed-row"}],
            "completeness":{"incomplete":true,"reason":"laterPageFailed",
                "failure":{"operation":"bybitClosedPnl","kind":"network"}}})
    );
    assert_eq!(
        replies[2],
        json!({"requestId":"orders","success":true,"result":[{"id":"order-row"}],
            "completeness":{"incomplete":true,"reason":"laterPageFailed",
                "failure":{"operation":"bybitOrders","kind":"rateLimited","venueCode":"10006"}}})
    );
    assert!(
        !serde_json::to_string(&replies)
            .unwrap()
            .contains("PRIVATE-CANARY")
    );
}

#[test]
fn hosted_history_native_float_command_keeps_its_owner_and_metadata() {
    for command in [0_u32, 77, u32::MAX] {
        // Native NoticeFields serializes engine Value::Num as an f64.
        let wire_command = serde_json::to_value(f64::from(command)).unwrap();
        assert!(wire_command.as_u64().is_none());
        let replies = hosted_replies(
            &[(command, "native-history", "fetchHistoricalOrders")],
            &[
                incomplete(json!({"command":wire_command,"reason":"laterPageFailed",
                    "historyOperation":"bybitOrders","historyFailureKind":"timeout"})),
                json!({"type":"done","command":command,"result":"[]"}),
            ],
        );
        assert_eq!(
            replies[0]["completeness"],
            json!({"incomplete":true,"reason":"laterPageFailed",
                "failure":{"operation":"bybitOrders","kind":"timeout"}})
        );
    }
}

#[test]
fn hosted_history_closed_failure_categories_survive_native_json_reply() {
    for operation in [
        "bybitClosedPnl",
        "bybitOrders",
        "bybitFills",
        "bybitIncome",
        "bybitTrades",
    ] {
        for kind in [
            "rejected",
            "invalidInput",
            "insufficientBalance",
            "authInvalid",
            "authExpired",
            "authPermission",
            "rateLimited",
            "network",
            "timeout",
            "venueUnavailable",
            "unsupported",
            "internal",
        ] {
            for code in [None, Some("10006"), Some("other")] {
                let mut fields = json!({"command":1,"reason":"laterPageFailed",
                    "historyOperation":operation,"historyFailureKind":kind});
                let mut failure = json!({"operation":operation,"kind":kind});
                if let Some(code) = code {
                    fields["historyVenueCode"] = code.into();
                    failure["venueCode"] = code.into();
                }
                let replies = hosted_replies(
                    &[(1, "history", "fetchHistoricalFills")],
                    &[
                        incomplete(fields),
                        json!({"type":"done","command":1,"result":"[]"}),
                    ],
                );
                assert_eq!(
                    replies[0]["completeness"],
                    json!({"incomplete":true,"reason":"laterPageFailed","failure":failure})
                );
            }
        }
    }
}

#[test]
fn hosted_history_replacement_and_invalid_detail_keep_only_the_closed_assertion() {
    for (fields, reason) in [
        (json!({"command":1,"reason":"PRIVATE-CANARY"}), None),
        (json!({"command":1}), None),
        (
            json!({"command":1,"reason":"coverageGap","historyOperation":"bybitOrders",
            "historyFailureKind":"network"}),
            Some("coverageGap"),
        ),
        (
            json!({"command":1,"reason":"laterPageFailed","historyOperation":"PRIVATE-CANARY",
            "historyFailureKind":"network"}),
            Some("laterPageFailed"),
        ),
        (
            json!({"command":1,"reason":"laterPageFailed","historyOperation":"bybitOrders",
            "historyFailureKind":"PRIVATE-CANARY"}),
            Some("laterPageFailed"),
        ),
        (
            json!({"command":1,"reason":"laterPageFailed","historyOperation":"bybitOrders",
            "historyFailureKind":"network","historyVenueCode":"PRIVATE-CANARY"}),
            Some("laterPageFailed"),
        ),
        (
            json!({"command":1,"reason":"laterPageFailed","historyOperation":7,
            "historyFailureKind":"network"}),
            Some("laterPageFailed"),
        ),
        (
            json!({"command":1,"reason":"laterPageFailed","historyOperation":"bybitOrders",
            "historyFailureKind":"network","historyVenueCode":10006}),
            Some("laterPageFailed"),
        ),
    ] {
        let replies = hosted_replies(
            &[(1, "history", "fetchHistoricalOrders")],
            &[
                incomplete(json!({"command":1,"reason":"laterPageFailed",
                    "historyOperation":"bybitOrders","historyFailureKind":"timeout"})),
                incomplete(fields),
                json!({"type":"done","command":1,"result":"[]"}),
            ],
        );
        let mut expected = json!({"incomplete":true});
        if let Some(reason) = reason {
            expected["reason"] = reason.into();
        }
        assert_eq!(replies[0]["completeness"], expected);
        assert!(
            !serde_json::to_string(&replies)
                .unwrap()
                .contains("PRIVATE-CANARY")
        );
    }
}

#[test]
fn hosted_history_unknown_owner_cannot_mark_a_complete_reply() {
    for command in [
        json!(2),
        json!("1"),
        json!(-1),
        json!(1.5),
        json!(4_294_967_296_u64),
        Value::Null,
        json!(true),
        json!({}),
        json!([]),
    ] {
        for owner in [0_u32, 1] {
            let replies = hosted_replies(
                &[(owner, "complete", "fetchHistoricalFills")],
                &[
                    incomplete(json!({"command":command,"reason":"laterPageFailed",
                        "historyOperation":"bybitFills","historyFailureKind":"network"})),
                    json!({"type":"done","command":owner,"result":"[]"}),
                ],
            );
            assert_eq!(
                replies,
                vec![json!({"requestId":"complete","success":true,"result":[]})]
            );
        }
    }
}
