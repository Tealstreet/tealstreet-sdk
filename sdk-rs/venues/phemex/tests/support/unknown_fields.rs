use safe_cex_engine::{Level, LogEvent, UNKNOWN_OUTCOME_TEXT, Value};

fn decimal(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && value.bytes().all(|byte| byte.is_ascii_digit())
}

pub fn assert_event(event: &LogEvent, command: u32, request: u64, status: Option<u16>) {
    assert_eq!(event.code, "order.outcome_unknown");
    assert_eq!(event.message, UNKNOWN_OUTCOME_TEXT);
    assert_eq!(event.level, Level::Warn);
    let allowed = [
        "command",
        "outcome",
        "placedOrderIds",
        "unknownIndices",
        "request",
        "status",
    ];
    assert_eq!(event.fields.len(), if status.is_some() { 6 } else { 5 });
    for (key, value) in &event.fields {
        assert!(allowed.contains(key), "unexpected field {key}");
        assert_eq!(
            event
                .fields
                .iter()
                .filter(|(other, _)| other == key)
                .count(),
            1
        );
        let text = value
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{value:?}"));
        for forbidden in [
            "https://",
            "http://",
            "/g-orders",
            "?",
            "&",
            "=",
            "signature",
            "api_key",
            "apikey",
            "sentinel",
            "payload",
            "body",
        ] {
            assert!(
                !text.to_ascii_lowercase().contains(forbidden),
                "field {key} contains {forbidden}"
            );
            assert!(!key.to_ascii_lowercase().contains(forbidden));
        }
        match *key {
            "command" => {
                let label = value.as_str().unwrap();
                assert_eq!(label, format!("phemex.{command}"));
                assert!((1..=64).contains(&label.len()));
                assert!(label.as_bytes()[0].is_ascii_alphabetic());
                assert!(
                    label
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
                );
            }
            "request" => {
                let id = value.as_str().unwrap();
                // Exact language of ^[0-9]{1,20}$; never a request URL.
                assert!(decimal(id, 20));
                assert_eq!(id, request.to_string());
            }
            "outcome" => assert_eq!(value.as_str(), Some("unknown")),
            "placedOrderIds" | "unknownIndices" => {
                let text = value.as_str().unwrap();
                assert!(text.len() <= safe_cex_engine::log::MAX_LOG_BYTES);
                let entries: Vec<_> = if text.is_empty() {
                    vec![]
                } else {
                    text.split(',').collect()
                };
                assert!(entries.len() <= 16);
                for entry in entries {
                    if *key == "placedOrderIds" {
                        assert!(decimal(entry, 32));
                    } else {
                        assert!(decimal(entry, 5));
                        assert!(entry.parse::<u32>().unwrap() <= 65535);
                    }
                }
            }
            "status" => {
                let Value::Num(number) = value else {
                    panic!("numeric status")
                };
                assert_eq!(*number, f64::from(status.unwrap()));
                assert_eq!(number.fract(), 0.);
                assert!((100.0..=599.0).contains(number));
            }
            _ => unreachable!(),
        }
    }
    assert_eq!(
        event.fields.iter().any(|(key, _)| *key == "status"),
        status.is_some()
    );
}
