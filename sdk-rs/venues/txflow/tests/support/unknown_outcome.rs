use safe_cex_engine::{Action, Notice};

pub fn unknown_events(actions: &[Action]) -> Vec<&safe_cex_engine::LogEvent> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::Event(event)) if event.code == "order.outcome_unknown" => {
                Some(event)
            }
            _ => None,
        })
        .collect()
}

pub fn assert_unknown_event(
    actions: &[Action],
    request: Option<safe_cex_engine::RequestId>,
    command: u32,
) {
    let events = unknown_events(actions);
    assert_eq!(events.len(), 1);
    let event = events[0];
    assert_eq!(event.level, safe_cex_engine::Level::Warn);
    assert_eq!(event.message, safe_cex_engine::UNKNOWN_OUTCOME_TEXT);
    assert!(
        event
            .fields
            .contains(&("command", format!("txflow-{command}").into()))
    );
    assert!(event.fields.contains(&("outcome", "unknown".into())));
    let label = event
        .fields
        .iter()
        .find(|(key, _)| *key == "command")
        .unwrap()
        .1
        .as_str()
        .unwrap();
    assert!((1..=64).contains(&label.len()) && label.as_bytes()[0].is_ascii_alphabetic());
    assert!(
        label
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
    );
    let observed_request = event
        .fields
        .iter()
        .find(|(key, _)| *key == "request")
        .map(|(_, value)| value.as_str().unwrap());
    if let Some(value) = observed_request {
        // Equivalent to ^[0-9]{1,20}$, including u64::MAX without precision loss.
        assert!((1..=20).contains(&value.len()) && value.bytes().all(|b| b.is_ascii_digit()));
    }
    assert_eq!(
        observed_request.map(str::to_owned),
        request.map(|id| id.0.to_string())
    );
    for (key, value) in &event.fields {
        assert!(
            [
                "command",
                "outcome",
                "placedOrderIds",
                "unknownIndices",
                "request",
                "status"
            ]
            .contains(key)
        );
        if *key == "status" {
            let safe_cex_engine::Value::Num(status) = value else {
                panic!("numeric status required")
            };
            assert!(status.fract() == 0. && (100.0..=599.0).contains(status));
        }
        if matches!(*key, "placedOrderIds" | "unknownIndices") {
            let list = value.as_str().unwrap();
            let entries: Vec<_> = if list.is_empty() {
                vec![]
            } else {
                list.split(',').collect()
            };
            assert!(entries.len() <= 16);
            for entry in entries {
                assert!(!entry.is_empty() && entry.bytes().all(|b| b.is_ascii_digit()));
                if *key == "placedOrderIds" {
                    assert!(entry.len() <= 32);
                } else {
                    assert!(entry.len() <= 5 && entry.parse::<u16>().is_ok());
                }
            }
        }
        let text = format!("{key} {value:?}").to_ascii_lowercase();
        for forbidden in [
            "http",
            "/openapi",
            "?",
            "&",
            "=",
            "{",
            "}",
            "url",
            "endpoint",
            "query",
            "body",
            "payload",
            "signature",
            "apikey",
            "primary-key",
            "primary-secret",
            "hostile",
        ] {
            assert!(
                !text.contains(forbidden),
                "forbidden telemetry content: {key}"
            );
        }
    }
}
