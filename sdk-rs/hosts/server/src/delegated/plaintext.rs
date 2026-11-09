//! The handover's fixed JSON schema, decoded within its input allocation.
use super::{Credentials, Refused};
use std::ops::Range;
use zeroize::{Zeroize, Zeroizing};

struct Input<'a> {
    bytes: &'a mut [u8],
    position: usize,
}

impl Drop for Input<'_> {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

impl Input<'_> {
    fn whitespace(&mut self) {
        while matches!(
            self.bytes.get(self.position),
            Some(b' ' | b'\n' | b'\r' | b'\t')
        ) {
            self.position += 1;
        }
    }

    fn take(&mut self, byte: u8) -> Result<(), Refused> {
        self.whitespace();
        if self.bytes.get(self.position) != Some(&byte) {
            return Err(Refused);
        }
        self.position += 1;
        Ok(())
    }

    fn next_byte(&mut self) -> Result<u8, Refused> {
        let byte = *self.bytes.get(self.position).ok_or(Refused)?;
        self.position += 1;
        Ok(byte)
    }

    fn hex(&mut self) -> Result<u32, Refused> {
        let mut code = 0;
        for _ in 0..4 {
            code = code * 16 + char::from(self.next_byte()?).to_digit(16).ok_or(Refused)?;
        }
        Ok(code)
    }

    fn string(&mut self) -> Result<Range<usize>, Refused> {
        self.take(b'"')?;
        let start = self.position;
        let mut written = start;
        loop {
            let byte = self.next_byte()?;
            let decoded = match byte {
                b'"' => return Ok(start..written),
                b'\\' => match self.next_byte()? {
                    b'"' => b'"',
                    b'\\' => b'\\',
                    b'/' => b'/',
                    b'b' => 8,
                    b'f' => 12,
                    b'n' => b'\n',
                    b'r' => b'\r',
                    b't' => b'\t',
                    b'u' => {
                        let mut code = self.hex()?;
                        if (0xd800..=0xdbff).contains(&code) {
                            if self.next_byte()? != b'\\' || self.next_byte()? != b'u' {
                                return Err(Refused);
                            }
                            let low = self.hex()?;
                            if !(0xdc00..=0xdfff).contains(&low) {
                                return Err(Refused);
                            }
                            code = 0x10000 + ((code - 0xd800) << 10) + (low - 0xdc00);
                        }
                        let character = char::from_u32(code).ok_or(Refused)?;
                        let mut encoded = Zeroizing::new([0u8; 4]);
                        let text = character.encode_utf8(&mut *encoded);
                        let end = written + text.len();
                        self.bytes[written..end].copy_from_slice(text.as_bytes());
                        written = end;
                        continue;
                    }
                    _ => return Err(Refused),
                },
                0..=31 => return Err(Refused),
                byte => byte,
            };
            self.bytes[written] = decoded;
            written += 1;
        }
    }

    fn text(&self, range: Range<usize>) -> Result<&str, Refused> {
        std::str::from_utf8(&self.bytes[range]).map_err(|_| Refused)
    }

    fn field(&mut self) -> Result<usize, Refused> {
        let range = self.string()?;
        match self.text(range)? {
            "key" => Ok(0),
            "secret" => Ok(1),
            "passphrase" => Ok(2),
            _ => Err(Refused),
        }
    }
}

pub(super) fn parse(bytes: &mut [u8]) -> Result<Credentials, Refused> {
    let mut input = Input { bytes, position: 0 };
    // RawValue validates JSON syntax without constructing decoded strings.
    // Subsequent string decoding writes only within this guarded input.
    serde_json::from_slice::<&serde_json::value::RawValue>(input.bytes).map_err(|_| Refused)?;
    input.whitespace();
    let sequence = input.bytes.get(input.position) == Some(&b'[');
    let closing = if sequence { b']' } else { b'}' };
    input.take(if sequence { b'[' } else { b'{' })?;
    let mut fields: [Option<Zeroizing<String>>; 3] = [None, None, None];
    let mut seen = [false; 3];
    let mut next_field = 0;
    loop {
        input.whitespace();
        if input.bytes.get(input.position) == Some(&closing) {
            input.position += 1;
            break;
        }
        let field = if sequence { next_field } else { input.field()? };
        if field >= fields.len() || seen[field] {
            return Err(Refused);
        }
        seen[field] = true;
        next_field += 1;
        if !sequence {
            input.take(b':')?;
        }
        input.whitespace();
        if field == 2 && input.bytes.get(input.position..input.position + 4) == Some(b"null") {
            input.position += 4;
        } else {
            let range = input.string()?;
            let mut value = Zeroizing::new(String::with_capacity(range.len()));
            value.push_str(input.text(range)?);
            fields[field] = Some(value);
        }
        input.whitespace();
        if input.bytes.get(input.position) == Some(&closing) {
            input.position += 1;
            break;
        }
        input.take(b',')?;
    }
    input.whitespace();
    if input.position != input.bytes.len() {
        return Err(Refused);
    }
    let [key, secret, passphrase] = fields;
    Ok(Credentials {
        key: key.ok_or(Refused)?,
        secret: secret.ok_or(Refused)?,
        passphrase,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct JsonFields {
        key: String,
        secret: String,
        #[serde(default)]
        passphrase: Option<String>,
    }

    #[test]
    fn json_string_and_schema_compatibility() {
        let mut cases: Vec<&[u8]> = vec![
            br#"{"key":"synthetic-key","secret":"synthetic-secret"}"#,
            br#" { "secret":"synthetic-secret", "key":"synthetic-key", "passphrase":null } "#,
            br#"{"k\u0065y":"synthetic-\u006bey","secret":"synthetic-\u0073ecret"}"#,
            br#"{"key":"synthetic-key","secret":"\"\\\/\b\f\n\r\t\u0000"}"#,
            br#"{"key":"synthetic-key","secret":"\u00e9\u20ac\ud83d\ude00","passphrase":"\uD834\uDD1E"}"#,
            "{\"key\":\"synthetic-key\",\"secret\":\"é€😀\"}".as_bytes(),
            br#"{"key":"synthetic-key","secret":"synthetic","passphrase":""}"#,
            br#"{"key":"","secret":""}"#,
            b"{}",
            b"[]",
            br#"["synthetic-key","synthetic-secret"]"#,
            br#"["synthetic-key","synthetic-secret",null]"#,
            br#"["synthetic-\u006bey","synthetic-\u0073ecret","synthetic-\u0070hrase"]"#,
            br#"["synthetic-key"]"#,
            br#"["synthetic-key","synthetic-secret",null,"extra"]"#,
            br#"{"key":"synthetic-key"}"#,
            br#"{"key":null,"secret":"synthetic"}"#,
            br#"{"key":"synthetic-key","secret":3}"#,
            br#"{"key":"synthetic-key","secret":"synthetic","passphrase":false}"#,
            br#"{"key":"synthetic-key","secret":"synthetic","other":"synthetic"}"#,
            br#"{"key":"synthetic-key","key":"synthetic-key","secret":"synthetic"}"#,
            br#"{"key":"synthetic-key","secret":"synthetic","secret":"synthetic"}"#,
            br#"{"key":"synthetic-key","secret":"synthetic","passphrase":null,"passphrase":null}"#,
            br#"{"key":"synthetic-key","secret":"synthetic",}"#,
            br#"{"key":"synthetic-key","secret":"synthetic"} {}"#,
            br#"{"key":"synthetic-key","secret":"synthetic-\uD800x"}"#,
            br#"{"key":"synthetic-key","secret":"synthetic-\uDC00"}"#,
            br#"{"key":"synthetic-key","secret":"synthetic-\uD800\u0041"}"#,
            br#"{"key":"synthetic-key","secret":"synthetic-\q"}"#,
            br#"{"key":"synthetic-key","secret":"synthetic-\u00"}"#,
            br#"{"key":"synthetic-key","secret":"synthetic"#,
        ];
        cases.push(b"{\"key\":\"synthetic-key\",\"secret\":\"\xff\"}");
        cases.push(b"{\"key\":\"synthetic-key\",\"secret\":\"\n\"}");
        for (index, case) in cases.into_iter().enumerate() {
            let expected = serde_json::from_slice::<JsonFields>(case);
            let mut input = case.to_vec();
            let actual = parse(&mut input);
            assert_eq!(actual.is_ok(), expected.is_ok(), "case {index}");
            if let (Ok(actual), Ok(expected)) = (actual, expected) {
                assert!(actual.key.as_str() == expected.key);
                assert!(actual.secret.as_str() == expected.secret);
                assert!(
                    actual.passphrase.as_deref().map(String::as_str)
                        == expected.passphrase.as_deref()
                );
            }
            assert!(input.iter().all(|byte| *byte == 0), "case {index}");
        }
    }

    #[test]
    fn json_unicode_compatibility() {
        for scalar in (0..=0xffff)
            .chain((0x10000..=0x10ffff).step_by(257))
            .chain([0x10ffff])
        {
            let Some(character) = char::from_u32(scalar) else {
                continue;
            };
            let escaped = if scalar <= 0xffff {
                format!("\\u{scalar:04x}")
            } else {
                let offset = scalar - 0x10000;
                format!(
                    "\\u{:04x}\\u{:04x}",
                    0xd800 + (offset >> 10),
                    0xdc00 + (offset & 0x3ff)
                )
            };
            let mut input =
                format!(r#"{{"key":"synthetic-key","secret":"synthetic-{escaped}-end"}}"#)
                    .into_bytes();
            let actual = parse(&mut input).unwrap();
            assert!(actual.secret.as_str() == format!("synthetic-{character}-end"));
            assert!(input.iter().all(|byte| *byte == 0));
        }
    }

    #[test]
    fn input_lifecycle_after_success_and_refusal() {
        for (case, accepted) in [
            (br#"{"key":"synthetic-\u006bey","secret":"synthetic-\u0073ecret","passphrase":"synthetic-\u0070hrase"}"#.as_slice(), true),
            (br#"{"key":"synthetic-\u006bey","secret":"synthetic-\u0073ecret","secret":"duplicate"}"#.as_slice(), false),
            (br#"{"key":"synthetic-\u006bey","secret":"synthetic-\u0073ecret","passphrase":"synthetic-\u0070refix\uD800x"}"#.as_slice(), false),
            (br#"{"key":"synthetic-\u006bey","secret":"synthetic-\u0073ecret","passphrase":"synthetic-\u0070refix\u00"}"#.as_slice(), false),
        ] {
            // Observe the same live allocation after the parser's guard drops.
            // No freed storage is inspected.
            let mut input = case.to_vec();
            let pointer = input.as_ptr();
            let capacity = input.capacity();
            let result = parse(&mut input);
            assert_eq!(result.is_ok(), accepted);
            assert_eq!(input.as_ptr(), pointer);
            assert_eq!(input.capacity(), capacity);
            assert_eq!(input.len(), case.len());
            assert!(input.iter().all(|byte| *byte == 0));
            if let Ok(fields) = result {
                assert!(fields.key.as_str() == "synthetic-key");
                assert!(fields.secret.as_str() == "synthetic-secret");
                assert!(fields.passphrase.as_deref().map(String::as_str) == Some("synthetic-phrase"));
            }
        }
    }

    #[test]
    fn string_lifecycle_after_partial_decoding() {
        for case in [
            br#""synthetic-\u0070refix\uD800x""#.as_slice(),
            br#""synthetic-\u0070refix\u00""#.as_slice(),
            br#""synthetic-\u0070refix\q""#.as_slice(),
        ] {
            let mut bytes = case.to_vec();
            {
                let mut input = Input {
                    bytes: &mut bytes,
                    position: 0,
                };
                assert!(input.string().is_err());
                assert!(
                    input
                        .bytes
                        .windows(b"synthetic-prefix".len())
                        .any(|window| window == b"synthetic-prefix")
                );
            }
            assert!(bytes.iter().all(|byte| *byte == 0));
        }
    }
}
