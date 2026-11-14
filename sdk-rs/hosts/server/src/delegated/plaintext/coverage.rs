use super::*;
use proptest::{prelude::*, test_runner::RngSeed};
use serde::Deserialize;
use std::panic::{AssertUnwindSafe, catch_unwind};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JsonFields {
    key: String,
    secret: String,
    #[serde(default)]
    passphrase: Option<String>,
    #[serde(default, rename = "signingKey")]
    signing_key: Option<String>,
}

fn compare(bytes: &[u8]) {
    let expected = serde_json::from_slice::<JsonFields>(bytes);
    let mut input = bytes.to_vec();
    let actual = catch_unwind(AssertUnwindSafe(|| parse(&mut input)));
    assert!(actual.is_ok(), "decoder panicked");
    assert!(input.iter().all(|byte| *byte == 0), "input not cleared");
    let actual = actual.unwrap();
    if bytes.len() > MAX_PLAINTEXT {
        assert!(actual.is_err(), "oversize input accepted");
        return;
    }
    match (actual, expected) {
        (Ok(actual), Ok(expected)) => {
            assert!(
                actual.signing_key.as_deref().map(String::as_str)
                    == expected.signing_key.as_deref()
            );
            assert!(actual.key.as_str() == expected.key);
            assert!(actual.secret.as_str() == expected.secret);
            assert!(
                actual.passphrase.as_deref().map(String::as_str) == expected.passphrase.as_deref()
            );
        }
        (Err(_), Err(_)) => {}
        _ => panic!("decoder and serde disagree"),
    }
}

fn refuse(bytes: &[u8]) {
    let mut input = bytes.to_vec();
    let result = catch_unwind(AssertUnwindSafe(|| parse(&mut input)));
    assert!(result.is_ok(), "decoder panicked");
    assert!(result.unwrap().is_err(), "malformed input accepted");
    assert!(input.iter().all(|byte| *byte == 0), "input not cleared");
}

fn quoted(text: &str, unicode: bool) -> String {
    if !unicode {
        return serde_json::to_string(text).unwrap();
    }
    use std::fmt::Write;
    let mut output = String::from("\"");
    for character in text.chars() {
        let code = u32::from(character);
        if code <= 0xffff {
            write!(output, "\\u{code:04x}").unwrap();
        } else {
            let offset = code - 0x10000;
            write!(
                output,
                "\\u{:04x}\\u{:04x}",
                0xd800 + (offset >> 10),
                0xdc00 + (offset & 0x3ff)
            )
            .unwrap();
        }
    }
    output.push('"');
    output
}

#[test]
fn differential_corpus_matches_serde() {
    let values = [
        "",
        "synthetic",
        "quote\" and slash\\",
        "\0\u{8}\u{c}\n\r\t",
        "é€😀𝄞",
        "\\\"/\\end",
    ];
    for value in values {
        for unicode in [false, true] {
            let key = quoted("synthetic-key", unicode);
            let secret = quoted(value, unicode);
            let phrase = quoted(&format!("synthetic-{value}"), unicode);
            for passphrase in [None, Some("null"), Some(phrase.as_str())] {
                let mut fields = vec![
                    format!("{}:{key}", quoted("key", unicode)),
                    format!("{}:{secret}", quoted("secret", unicode)),
                ];
                if let Some(passphrase) = passphrase {
                    fields.push(format!("{}:{passphrase}", quoted("passphrase", unicode)));
                }
                fields.push(format!("{}:{phrase}", quoted("signingKey", unicode)));
                for rotation in 0..fields.len() {
                    fields.rotate_left(rotation);
                    for reverse in [false, true] {
                        if reverse {
                            fields.reverse();
                        }
                        let json = format!(" \n{{ {} }}\t", fields.join(",\r\n"));
                        assert!(serde_json::from_slice::<JsonFields>(json.as_bytes()).is_ok());
                        compare(json.as_bytes());
                        let unknown = format!(
                            "{{{},{}:{phrase}}}",
                            fields.join(","),
                            quoted("other", unicode)
                        );
                        assert!(serde_json::from_slice::<JsonFields>(unknown.as_bytes()).is_err());
                        compare(unknown.as_bytes());
                    }
                }
            }
        }
    }
    for json in [
        br#"{"key":"synthetic-key","secret":"\"\\\/\b\f\n\r\t\u0000","passphrase":"quote\"slash\\"}"#.as_slice(),
        br#"{"k\u0065y":"synthetic-key","secret":"\u00E9\u20AC\uD83D\uDE00","passphrase":null}"#.as_slice(),
        br#"["synthetic-key","synthetic-secret","quote\"slash\\"]"#.as_slice(),
    ] {
        assert!(serde_json::from_slice::<JsonFields>(json).is_ok());
        compare(json);
    }
}

#[test]
fn sealed_handover_preserves_quoted_passphrase() {
    use crate::delegated::{Handovers, tests::seal};
    let mut owner = Handovers::default();
    let offer = owner.issue("synthetic-account", "bybit", 100).unwrap();
    let sealed = seal(&offer, "bybit", "synthetic-key", br#"{"key":"synthetic-key","secret":"synthetic-secret","passphrase":"synthetic-\"quoted\"\\phrase"}"#);
    let fields = owner
        .take("synthetic-account", &sealed, 101)
        .unwrap()
        .open("bybit", "synthetic-key", &sealed, 102)
        .unwrap();
    assert!(fields.key.as_str() == "synthetic-key");
    assert!(fields.secret.as_str() == "synthetic-secret");
    assert!(
        fields.passphrase.as_deref().map(String::as_str) == Some("synthetic-\"quoted\"\\phrase")
    );
}

#[test]
fn every_truncation_refuses_and_clears() {
    for json in [
        br#"{"key":"synthetic-key","secret":"","signingKey":"synthetic-\u0073calar"}"#.as_slice(),
        br#"{"key":"synthetic-key","secret":"synthetic-secret"}"#.as_slice(),
        br#"{"key":"synthetic-\u006bey","secret":"\uD83D\uDE00","passphrase":"quote\"slash\\"}"#
            .as_slice(),
        br#"{"secret":"synthetic-secret","passphrase":null,"key":"synthetic-key"}"#.as_slice(),
    ] {
        compare(json);
        for end in 0..json.len() {
            let prefix = json.get(..end).unwrap();
            assert!(serde_json::from_slice::<JsonFields>(prefix).is_err());
            refuse(prefix);
        }
    }
}

#[test]
fn malformed_corpus_refuses_and_clears() {
    for value in [
        r#""synthetic-\uD800""#,
        r#""synthetic-\uDC00""#,
        r#""synthetic-\uD800\u0041""#,
        r#""synthetic-\uDBFF\uD800""#,
        r#""synthetic-\x""#,
        r#""synthetic-\u12xz""#,
        r#""synthetic-\u00""#,
        r#""synthetic-\"#,
        "true",
        "0",
        "{}",
        "[]",
        r#"{"nested":"synthetic"}"#,
        r#"["synthetic"]"#,
    ] {
        for field in ["key", "secret", "passphrase", "signingKey"] {
            let mut fields = vec![
                r#""key":"synthetic-key""#.to_owned(),
                r#""secret":"synthetic-secret""#.to_owned(),
            ];
            fields.retain(|entry| !entry.starts_with(&format!("\"{field}\":")));
            fields.push(format!("\"{field}\":{value}"));
            let json = format!("{{{}}}", fields.join(","));
            assert!(serde_json::from_slice::<JsonFields>(json.as_bytes()).is_err());
            refuse(json.as_bytes());
        }
    }
    for duplicate in [
        r#""key":"synthetic-second""#,
        r#""secret":"synthetic-second""#,
        r#""passphrase":null,"passphrase":null"#,
        r#""signingKey":null,"signingKey":null"#,
        r#""signingKey":"synthetic-one","signingKey":"synthetic-two"#,
        r#""secr\u0065t":"synthetic-second""#,
    ] {
        let json = format!(r#"{{"key":"synthetic-key","secret":"synthetic-secret",{duplicate}}}"#);
        assert!(serde_json::from_slice::<JsonFields>(json.as_bytes()).is_err());
        refuse(json.as_bytes());
    }
    let valid = r#"{"key":"synthetic-key","secret":"synthetic-secret"}"#;
    for suffix in ["null", "{}", "[]", "x", "\0", ","] {
        refuse(format!("{valid}{suffix}").as_bytes());
    }
    for depth in [1, 16, 128, 1024] {
        let json = format!(
            r#"{{"key":"synthetic-key","secret":{}"synthetic"{}}}"#,
            "[".repeat(depth),
            "]".repeat(depth)
        );
        refuse(json.as_bytes());
        let json = format!(
            r#"{{"key":"synthetic-key","secret":{}null{}}}"#,
            r#"{"nested":"#.repeat(depth),
            "}".repeat(depth)
        );
        refuse(json.as_bytes());
    }
    for json in [
        "null",
        "true",
        r#""synthetic""#,
        "{}",
        r#"{"key":"synthetic-key"}"#,
        r#"{"secret":"synthetic-secret"}"#,
        r#"{"key":"synthetic-key","secret":"synthetic-secret","other":{}}"#,
        "[]",
        r#"[null,"synthetic-secret"]"#,
        r#"["synthetic-key"]"#,
        r#"["synthetic-key","synthetic-secret",null,null,null]"#,
        r#"{"key" "synthetic-key","secret":"synthetic-secret"}"#,
        r#"{"key":"synthetic-key","secret":"synthetic-secret",}"#,
    ] {
        assert!(serde_json::from_slice::<JsonFields>(json.as_bytes()).is_err());
        refuse(json.as_bytes());
    }
    refuse(b"{\"key\":\"synthetic-key\",\"secret\":\"\xff\"}");
    refuse(b"{\"key\":\"synthetic-key\",\"secret\":\"\n\"}");
}

#[test]
fn input_size_boundary() {
    let mut exact = br#"{"key":"synthetic-key","secret":"synthetic-secret"}"#.to_vec();
    exact.resize(MAX_PLAINTEXT, b' ');
    compare(&exact);
    exact.push(b' ');
    assert!(serde_json::from_slice::<JsonFields>(&exact).is_ok());
    refuse(&exact);
    refuse(&vec![b' '; MAX_PLAINTEXT * 2]);
    let large_field = format!(
        r#"{{"key":"synthetic-key","secret":"{}"}}"#,
        "s".repeat(MAX_PLAINTEXT)
    );
    refuse(large_field.as_bytes());
    assert!(serde_json::from_str::<Credentials>(&large_field).is_err());
}

#[test]
fn invalid_ranges_and_positions_are_refused() {
    for position in [0, 16, usize::MAX] {
        let mut bytes = b"synthetic".to_vec();
        let result = catch_unwind(AssertUnwindSafe(|| {
            let mut input = Input {
                bytes: &mut bytes,
                position,
            };
            assert!(input.advance(usize::MAX).is_err());
            assert!(input.text(0..usize::MAX).is_err());
            assert!(input.text(usize::MAX..usize::MAX).is_err());
            assert!(input.take(b'"').is_err());
            assert!(input.string().is_err());
            assert!(input.hex().is_err());
        }));
        assert!(result.is_ok(), "invalid cursor or range panicked");
        assert!(bytes.iter().all(|byte| *byte == 0));
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 256,
        failure_persistence: None,
        rng_seed: RngSeed::Fixed(3409),
        .. ProptestConfig::default()
    })]

    #[test]
    fn arbitrary_bytes_match_serde_without_panics(bytes in prop::collection::vec(any::<u8>(), 0..=MAX_PLAINTEXT + 64)) {
        compare(&bytes);
    }

    #[test]
    fn generated_strings_match_serde(
        key in prop::collection::vec(prop_oneof![Just('"'), Just('\\'), Just('\n'), any::<char>()], 0..48),
        secret in prop::collection::vec(prop_oneof![Just('"'), Just('\\'), Just('\t'), any::<char>()], 0..48),
        passphrase in prop::option::of(prop::collection::vec(prop_oneof![Just('"'), Just('\\'), Just('\0'), any::<char>()], 0..48)),
        signing_key in prop::option::of(prop::collection::vec(prop_oneof![Just('"'), Just('\\'), Just('\0'), any::<char>()], 0..48)),
        unicode in any::<bool>(),
    ) {
        let fields = JsonFields { key:key.into_iter().collect(), secret:secret.into_iter().collect(), passphrase:passphrase.map(|value| value.into_iter().collect()), signing_key:signing_key.map(|value| value.into_iter().collect()) };
        let key = quoted(&fields.key, unicode);
        let secret = quoted(&fields.secret, unicode);
        let passphrase = fields.passphrase.as_deref().map(|value| quoted(value, unicode)).unwrap_or_else(|| "null".to_owned());
        let signing_key = fields.signing_key.as_deref().map(|value| quoted(value, unicode)).unwrap_or_else(|| "null".to_owned());
        let json = format!(r#"{{"key":{key},"secret":{secret},"passphrase":{passphrase},"signingKey":{signing_key}}}"#);
        compare(json.as_bytes());
    }

    #[test]
    fn string_chunks_do_not_panic(bytes in prop::collection::vec(prop_oneof![Just(b'\\'), Just(b'"'), Just(b'u'), b'0'..=b'f', any::<u8>()], 0..1024)) {
        let mut input = vec![b'"'];
        input.extend(bytes);
        let result = catch_unwind(AssertUnwindSafe(|| {
            let mut reader = Input { bytes: &mut input, position: 0 };
            if let Ok(range) = reader.string() {
                let _ = reader.text(range);
            }
        }));
        prop_assert!(result.is_ok());
        prop_assert!(input.iter().all(|byte| *byte == 0));
    }
}
