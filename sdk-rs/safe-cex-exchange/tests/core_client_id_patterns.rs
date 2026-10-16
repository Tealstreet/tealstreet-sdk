use safe_cex_exchange::client_order_id::{
    ClientOrderIdRuleOverrides, pattern_client_order_id_rule,
};
use std::rc::Rc;

fn predicate(display: &str) -> Rc<dyn Fn(&str) -> bool> {
    if display == "/^\\S+$/" {
        return Rc::new(|id| {
            !id.is_empty()
                && !id.chars().any(|character| {
                    let mut bytes = [0; 4];
                    safe_cex_num::js_trim(character.encode_utf8(&mut bytes)).is_empty()
                })
        });
    }
    let dot_bound = display
        .strip_prefix("/^.{1,")
        .or_else(|| display.strip_prefix("/^(?!-).{1,"))
        .and_then(|suffix| suffix.strip_suffix("}$/"))
        .map(|bound| bound.parse::<usize>().unwrap());
    if let Some(bound) = dot_bound {
        let reject_hyphen = display.starts_with("/^(?!-)");
        return Rc::new(move |id| {
            let length = id.encode_utf16().count();
            (1..=bound).contains(&length)
                && !(reject_hyphen && id.starts_with('-'))
                && !id
                    .chars()
                    .any(|character| matches!(character, '\n' | '\r' | '\u{2028}' | '\u{2029}'))
        });
    }
    assert!(
        display.starts_with("/^["),
        "unreviewed JS pattern family: {display}"
    );
    let (characters, bound) = display
        .strip_prefix("/^[")
        .unwrap()
        .split_once("]{1,")
        .unwrap();
    let bound: usize = bound.strip_suffix("}$/").unwrap().parse().unwrap();
    let characters = characters.as_bytes();
    let mut allowed = [false; 128];
    let mut index = 0;
    while index < characters.len() {
        if index + 2 < characters.len() && characters[index + 1] == b'-' {
            for byte in characters[index]..=characters[index + 2] {
                allowed[usize::from(byte)] = true;
            }
            index += 3;
        } else {
            allowed[usize::from(characters[index])] = true;
            index += 1;
        }
    }
    Rc::new(move |id| {
        (1..=bound).contains(&id.len())
            && id
                .bytes()
                .all(|byte| byte.is_ascii() && allowed[usize::from(byte)])
    })
}

#[test]
fn callback_pattern_api_keeps_requirement_and_supplied_semantics() {
    let rule = pattern_client_order_id_rule(
        "Orderly".into(),
        "/^(?!-).{1,36}$/".into(),
        predicate("/^(?!-).{1,36}$/"),
        ClientOrderIdRuleOverrides::default(),
    );
    assert_eq!(rule.requirement, "match /^(?!-).{1,36}$/");
    assert!(!(rule.is_valid)("-caller"));
    assert!((rule.is_valid)(&"😀".repeat(18)));
    assert!(!(rule.is_valid)(&"😀".repeat(19)));
    let whitespace = predicate("/^\\S+$/");
    assert!(!whitespace("\u{FEFF}"));
    assert!(whitespace("\u{0085}"));
}

#[test]
#[ignore = "live JS pattern inventory/oracle gate; CI runs --include-ignored"]
fn all_venue_client_id_patterns_match_live_javascript_predicates() {
    let output = std::process::Command::new("node")
        .arg("scripts/client-id-patterns.mjs")
        .arg("--oracle")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Node is required for the live client-ID pattern gate");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.len(), 11, "CLIENT-ID-CARDINALITY source patterns");
    assert_eq!(
        rows.iter()
            .map(|row| row["cases"].as_array().unwrap().len())
            .sum::<usize>(),
        880,
        "CLIENT-ID-CARDINALITY source total samples"
    );
    for row in rows {
        let display = row["pattern"].as_str().unwrap();
        let rule = pattern_client_order_id_rule(
            "Venue".into(),
            display.into(),
            predicate(display),
            ClientOrderIdRuleOverrides::default(),
        );
        assert_eq!(
            row["cases"].as_array().unwrap().len(),
            80,
            "CLIENT-ID-CARDINALITY source per-pattern samples"
        );
        for index in 0..80 {
            let case = &row["cases"][index];
            let id = case["value"].as_str().unwrap();
            assert_eq!(
                (rule.is_valid)(id),
                case["expected"].as_bool().unwrap(),
                "pattern={display} value={id:?}"
            );
        }
    }
}
