use safe_cex_num::{js_replace, js_replace_all, js_replace_all_utf16, js_replace_utf16};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

#[test]
fn string_replacement_matches_live_node_code_units() {
    let mut child = Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/js_replace.cjs"))
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut count = 0;
    let mut failures = Vec::new();
    for line in BufReader::new(child.stdout.take().unwrap()).lines() {
        let [text, needle, replacement, first, all]: [Vec<u16>; 5] =
            serde_json::from_str(&line.unwrap()).unwrap();
        if (js_replace_utf16(&text, &needle, &replacement) != first
            || js_replace_all_utf16(&text, &needle, &replacement) != all)
            && failures.len() < 10
        {
            failures.push(format!(
                "UTF16 replacement {text:?} / {needle:?} / {replacement:?}"
            ));
        }
        if let (Ok(text), Ok(needle), Ok(replacement)) = (
            String::from_utf16(&text),
            String::from_utf16(&needle),
            String::from_utf16(&replacement),
        ) {
            let actual = js_replace(&text, &needle, &replacement);
            if actual.encode_utf16().collect::<Vec<_>>() != first && failures.len() < 10 {
                failures.push(format!("first string replacement {text:?}"));
            }
            match (
                js_replace_all(&text, &needle, &replacement),
                String::from_utf16(&all),
            ) {
                (Ok(actual), Ok(expected)) => {
                    if actual != expected && failures.len() < 10 {
                        failures.push(format!("all string replacement {text:?}"));
                    }
                }
                (Err(error), Err(_)) => {
                    assert_eq!(error.name, "RepresentationError");
                }
                (actual, expected) => panic!(
                    "representation boundary {text:?} / {needle:?} / {replacement:?}: {actual:?} != {expected:?}"
                ),
            }
        }
        count += 1;
    }
    assert!(
        child.wait().unwrap().success(),
        "live replacement oracle must execute"
    );
    assert_eq!(count, 33780, "complete replacement corpus");
    assert!(
        failures.is_empty(),
        "JavaScript replacement mismatch: {}",
        failures.join("\n")
    );
}

#[test]
fn empty_all_replacement_refuses_only_unrepresentable_output() {
    let error = js_replace_all("😀", "", "_").unwrap_err();
    assert_eq!(error.name, "RepresentationError");
    assert_eq!(
        js_replace_all_utf16(&[0xd83d, 0xde00], &[], &[95]),
        vec![95, 0xd83d, 95, 0xde00, 95]
    );
    assert_eq!(js_replace("😀", "", "_"), "_😀");
    assert_eq!(js_replace_all("😀", "", "").unwrap(), "😀");
    assert_eq!(js_replace_all("😀", "😀", "_").unwrap(), "_");
    assert_eq!(js_replace_all("\u{feff}", "", "_").unwrap(), "_\u{feff}_");
}
