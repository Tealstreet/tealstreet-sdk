#[path = "support/source_cardinality.rs"]
mod source_cardinality;
use safe_cex_num::{js_split_whitespace, js_trim, js_trim_end, js_trim_start};

#[test]
fn trim_matches_live_javascript_for_every_unicode_scalar() {
    let output = std::process::Command::new("node")
        .args([
            "-e",
            r#"
const sets = [[], [], [], []];
for (let n = 0; n <= 0x10ffff; n++) {
  if (n >= 0xd800 && n <= 0xdfff) continue;
  const c = String.fromCodePoint(n);
  if ((c + 'x' + c).trim() === 'x') sets[0].push(n);
  if ((c + 'x').trimStart() === 'x') sets[1].push(n);
  if (('x' + c).trimEnd() === 'x') sets[2].push(n);
  if (('x' + c + c + 'y').split(/\s+/).length === 2) sets[3].push(n);
}
console.log(JSON.stringify(sets));
"#,
        ])
        .output()
        .expect("Node is required for the live ECMAScript oracle");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<Vec<u32>> = serde_json::from_slice(&output.stdout).unwrap();
    source_cardinality::for_each(
        expected.iter().enumerate(),
        4,
        "trim-sets",
        |(kind, set)| {
            let names = [
                "trim-members",
                "trim-start-members",
                "trim-end-members",
                "split-members",
            ];
            source_cardinality::for_each(set, 25, names[kind], |&code| {
                let c = char::from_u32(code).unwrap();
                match kind {
                    0 => assert_eq!(
                        js_trim(&format!("{c}x{c}")),
                        "x",
                        "TRIM-CARDINALITY member identity"
                    ),
                    1 => assert_eq!(
                        js_trim_start(&format!("{c}x")),
                        "x",
                        "TRIM-CARDINALITY member identity"
                    ),
                    2 => assert_eq!(
                        js_trim_end(&format!("x{c}")),
                        "x",
                        "TRIM-CARDINALITY member identity"
                    ),
                    3 => assert_eq!(
                        js_split_whitespace(&format!("x{c}{c}y")),
                        vec!["x", "y"],
                        "TRIM-CARDINALITY member identity"
                    ),
                    _ => unreachable!(),
                }
            });
        },
    );
    let mut compared = 0;
    for code in 0..=0x10ffff {
        let Some(c) = char::from_u32(code) else {
            continue;
        };
        let text = format!("{c}x{c}");
        assert_eq!(
            js_trim(&text),
            if expected[0].contains(&code) {
                "x"
            } else {
                &text
            },
            "U+{code:04X}"
        );
        let left = format!("{c}x");
        let right = format!("x{c}");
        assert_eq!(
            js_trim_start(&left),
            if expected[1].contains(&code) {
                "x"
            } else {
                &left
            },
            "trimStart U+{code:04X}"
        );
        assert_eq!(
            js_trim_end(&right),
            if expected[2].contains(&code) {
                "x"
            } else {
                &right
            },
            "trimEnd U+{code:04X}"
        );
        let split = format!("x{c}{c}y");
        assert_eq!(
            js_split_whitespace(&split),
            if expected[3].contains(&code) {
                vec!["x", "y"]
            } else {
                vec![split.as_str()]
            },
            "split U+{code:04X}"
        );
        compared += 4;
    }
    assert_eq!(
        compared, 4_448_256,
        "TRIM-CARDINALITY compared scalar operations"
    );
}

#[test]
fn trim_preserves_internal_whitespace_and_non_js_controls() {
    assert_eq!(js_trim("\u{feff}\t a\u{feff} b \r\n"), "a\u{feff} b");
    assert_eq!(js_trim("\u{0085}x\u{0085}"), "\u{0085}x\u{0085}");
    for c in ['\u{001c}', '\u{001d}', '\u{001e}', '\u{001f}'] {
        assert!(
            !c.is_whitespace(),
            "Rust does not treat U+001C–001F as whitespace"
        );
        let text = format!("{c}x{c}");
        assert_eq!(js_trim(&text), text);
    }
    assert_eq!(js_trim(" \u{feff}\n"), "");
}

#[test]
fn whitespace_split_preserves_js_empty_segments_and_coalesces_runs() {
    assert_eq!(js_split_whitespace(""), vec![""]);
    assert_eq!(js_split_whitespace(" \t"), vec!["", ""]);
    assert_eq!(
        js_split_whitespace("\u{feff}alpha \n beta\t"),
        vec!["", "alpha", "beta", ""]
    );
    assert_eq!(js_trim_start(" a "), "a ");
    assert_eq!(js_trim_end(" a "), " a");
}
