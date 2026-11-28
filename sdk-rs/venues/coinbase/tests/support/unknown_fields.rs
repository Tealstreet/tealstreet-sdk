use safe_cex_engine::{LogEvent, Value};

pub fn assert_field_rules(event: &LogEvent) {
    assert_eq!(event.level, safe_cex_engine::Level::Warn);
    assert_eq!(event.code, "order.outcome_unknown");
    assert_eq!(event.message, safe_cex_engine::UNKNOWN_OUTCOME_TEXT);
    let text = |name| {
        event
            .fields
            .iter()
            .find(|(key, _)| *key == name)
            .unwrap()
            .1
            .as_str()
            .unwrap()
    };
    let request = text("request");
    assert!(
        (1..=20).contains(&request.len()) && request.bytes().all(|b| b.is_ascii_digit()),
        "request must match ^[0-9]{{1,20}}$"
    );
    let command = text("command");
    assert!((1..=64).contains(&command.len()) && command.as_bytes()[0].is_ascii_alphabetic());
    assert!(
        command
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
    );
    assert_eq!(text("outcome"), "unknown");
    for (name, max_length, max_value) in [
        ("placedOrderIds", 32, None),
        ("unknownIndices", 5, Some(65_535)),
    ] {
        let value = text(name);
        let entries: Vec<_> = if value.is_empty() {
            vec![]
        } else {
            value.split(',').collect()
        };
        assert!(entries.len() <= 16);
        for entry in entries {
            assert!(
                (1..=max_length).contains(&entry.len())
                    && entry.bytes().all(|b| b.is_ascii_digit())
            );
            if let Some(max) = max_value {
                assert!(entry.parse::<u32>().unwrap() <= max);
            }
        }
    }
    assert_eq!(
        event.fields.len(),
        if event.fields.iter().any(|(key, _)| *key == "status") {
            6
        } else {
            5
        }
    );
    for (key, value) in &event.fields {
        assert!(
            [
                "request",
                "command",
                "outcome",
                "placedOrderIds",
                "unknownIndices",
                "status"
            ]
            .contains(key)
        );
        if *key == "status" {
            let Value::Num(status) = value else {
                panic!("numeric HTTP status")
            };
            assert!((100.0..=599.0).contains(status) && status.fract() == 0.);
        }
        let field = value.as_str().unwrap_or("").to_ascii_lowercase();
        for forbidden in [
            "http",
            "/",
            "?",
            "=",
            "{",
            "}",
            "signature",
            "sig",
            "api_key",
            "payload",
            "body",
        ] {
            assert!(!field.contains(forbidden), "unsafe emitted field {key}");
        }
    }
}
