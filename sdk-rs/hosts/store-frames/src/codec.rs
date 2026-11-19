//! Inspection decoder for native record sections; production encoding is typed.
use serde_json::Value;
pub fn decode(payload: &[u8]) -> Result<Value, &'static str> {
    struct Reader<'a> {
        bytes: &'a [u8],
        at: usize,
    }
    impl Reader<'_> {
        fn take(&mut self, n: usize) -> Result<&[u8], &'static str> {
            let end = self.at.checked_add(n).ok_or("native length overflow")?;
            let bytes = self
                .bytes
                .get(self.at..end)
                .ok_or("truncated native value")?;
            self.at = end;
            Ok(bytes)
        }
        fn count(&mut self) -> Result<usize, &'static str> {
            Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()) as usize)
        }
        fn text(&mut self) -> Result<String, &'static str> {
            let count = self.count()?;
            String::from_utf8(self.take(count)?.to_vec()).map_err(|_| "invalid native text")
        }
        fn value(&mut self, depth: usize) -> Result<Value, &'static str> {
            if depth > 128 {
                return Err("native nesting limit");
            }
            Ok(match self.take(1)?[0] {
                0 => return Err("unsupported native tag"),
                1 => Value::Null,
                2 => Value::Bool(false),
                3 => Value::Bool(true),
                4 => serde_json::Number::from_f64(f64::from_le_bytes(
                    self.take(8)?.try_into().unwrap(),
                ))
                .map(Value::Number)
                .unwrap_or(Value::Null),
                5 => Value::String(self.text()?),
                tag @ (6 | 7) => {
                    let count = self.count()?;
                    if count > (self.bytes.len() - self.at) / if tag == 7 { 5 } else { 1 } {
                        return Err("invalid native count");
                    }
                    if tag == 6 {
                        Value::Array(
                            (0..count)
                                .map(|_| self.value(depth + 1))
                                .collect::<Result<_, _>>()?,
                        )
                    } else {
                        let mut rows = serde_json::Map::new();
                        let mut keys = std::collections::HashSet::new();
                        for _ in 0..count {
                            let key = self.text()?;
                            if !keys.insert(key.clone()) {
                                return Err("duplicate native key");
                            }
                            rows.insert(key, self.value(depth + 1)?);
                        }
                        Value::Object(rows)
                    }
                }
                _ => return Err("invalid native tag"),
            })
        }
    }
    let count = u32::from_le_bytes(
        payload
            .get(..4)
            .ok_or("truncated native length")?
            .try_into()
            .unwrap(),
    ) as usize;
    let end = 4usize.checked_add(count).ok_or("native length overflow")?;
    let bytes = payload.get(4..end).ok_or("truncated native section")?;
    if payload.len() != end.div_ceil(8) * 8 || payload[end..].iter().any(|&b| b != 0) {
        return Err("invalid native padding");
    }
    let mut reader = Reader { bytes, at: 0 };
    let value = reader.value(0)?;
    if reader.at != bytes.len() {
        return Err("trailing native bytes");
    }
    Ok(value)
}

/// Rewrite only the holder labels. All other value bytes, including native
/// non-finite numbers, are forwarded without materializing a data tree.
pub(super) fn relabel(payload: &[u8], label: &str, field: &str) -> Result<Vec<u8>, &'static str> {
    struct Cursor<'a> {
        data: &'a [u8],
        at: usize,
    }
    impl<'a> Cursor<'a> {
        fn take(&mut self, n: usize) -> Result<&'a [u8], &'static str> {
            let end = self.at.checked_add(n).ok_or("size overflow")?;
            let b = self.data.get(self.at..end).ok_or("truncated value")?;
            self.at = end;
            Ok(b)
        }
        fn count(&mut self) -> Result<usize, &'static str> {
            Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()) as usize)
        }
        fn text(&mut self) -> Result<&'a str, &'static str> {
            let n = self.count()?;
            std::str::from_utf8(self.take(n)?).map_err(|_| "invalid text")
        }
        fn value(&mut self, depth: usize) -> Result<(), &'static str> {
            if depth > 128 {
                return Err("nesting limit");
            }
            match self.take(1)?[0] {
                1..=3 => {}
                4 => {
                    self.take(8)?;
                }
                5 => {
                    self.text()?;
                }
                6 => {
                    let n = self.count()?;
                    for _ in 0..n {
                        self.value(depth + 1)?;
                    }
                }
                7 => {
                    let n = self.count()?;
                    for _ in 0..n {
                        self.text()?;
                        self.value(depth + 1)?;
                    }
                }
                _ => return Err("invalid tag"),
            }
            Ok(())
        }
    }
    fn text(out: &mut Vec<u8>, v: &str) {
        out.extend_from_slice(&(v.len() as u32).to_le_bytes());
        out.extend_from_slice(v.as_bytes());
    }
    let mut cur = Cursor {
        data: payload,
        at: 0,
    };
    let end = 4 + cur.count()?;
    if end > payload.len() || cur.take(1)?[0] != 6 {
        return Err("invalid record array");
    }
    let count = cur.count()?;
    let mut out = payload[..cur.at].to_vec();
    for _ in 0..count {
        let start = cur.at;
        if cur.take(1)?[0] != 7 {
            return Err("invalid record");
        }
        let fields = cur.count()?;
        let values = cur.at;
        let mut original = "";
        for _ in 0..fields {
            let key = cur.text()?;
            if key == field {
                if cur.take(1)?[0] != 5 {
                    return Err("invalid account label");
                }
                original = cur.text()?;
            } else {
                cur.value(0)?;
            }
        }
        let row_end = cur.at;
        out.extend_from_slice(&payload[start..values]);
        cur.at = values;
        for _ in 0..fields {
            let key = cur.text()?;
            text(&mut out, key);
            let value_start = cur.at;
            cur.value(0)?;
            if key == field {
                out.push(5);
                text(&mut out, label);
            } else if key == "key" && field == "account" {
                let mut value = Cursor {
                    data: payload,
                    at: value_start,
                };
                if value.take(1)?[0] != 5 {
                    return Err("invalid position key");
                }
                let key = value.text()?;
                if let Some(suffix) = key.strip_prefix(&format!("{original}:")) {
                    out.push(5);
                    text(&mut out, &format!("{label}:{suffix}"));
                } else {
                    out.extend_from_slice(&payload[value_start..cur.at]);
                }
            } else {
                out.extend_from_slice(&payload[value_start..cur.at]);
            }
        }
        assert_eq!(cur.at, row_end);
    }
    if cur.at != end {
        return Err("trailing record data");
    }
    let n = (out.len() - 4) as u32;
    out[..4].copy_from_slice(&n.to_le_bytes());
    while !out.len().is_multiple_of(8) {
        out.push(0);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn relabel_preserves_unrelated_number_bits_and_native_text() {
        #[derive(serde::Serialize)]
        struct Row {
            account: String,
            key: String,
            value: f64,
        }
        let bits = 0x7ff8_0000_0000_0011u64;
        let rows = vec![Row {
            account: "original".into(),
            key: "original:BTC:long".into(),
            value: f64::from_bits(bits),
        }];
        let bytes = safe_cex_engine::store_frame_wire::encode(&rows);
        let changed = super::relabel(&bytes, "账户🌊", "account").unwrap();
        assert!(changed.windows(8).any(|b| b == bits.to_le_bytes()));
        let row = &super::decode(&changed).unwrap()[0];
        assert_eq!(row["account"], "账户🌊");
        assert_eq!(row["key"], "账户🌊:BTC:long");
    }
}
