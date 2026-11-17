//! SCXF native store sections. Each payload is a u32 byte length followed by
//! tagged native values and zero padding to eight bytes. Numbers are binary64;
//! text is length-prefixed UTF-16, preserving lone units and own undefined.
use crate::retained_json::RetainedJsonInput as Input;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u16)]
pub enum Section {
    SocketHealth = 26,
    ActiveChasers = 27,
    ActiveTwaps = 28,
    OrdersRisk = 29,
    DisplayedPositions = 30,
    DisplayedMargin = 31,
    BalanceInfo = 32,
}

pub fn encode(value: &Input) -> Vec<u8> {
    fn text(out: &mut Vec<u8>, value: &[u16]) {
        out.extend_from_slice(&(value.len() as u32).to_le_bytes());
        for unit in value {
            out.extend_from_slice(&unit.to_le_bytes());
        }
    }
    fn write(out: &mut Vec<u8>, value: &Input) {
        match value {
            Input::Undefined => out.push(0),
            Input::Null => out.push(1),
            Input::Bool(false) => out.push(2),
            Input::Bool(true) => out.push(3),
            Input::Float(value) => {
                out.push(4);
                out.extend_from_slice(&value.to_le_bytes());
            }
            Input::Lexeme(value) => {
                out.push(4);
                out.extend_from_slice(
                    &value
                        .parse::<f64>()
                        .expect("native numeric lexeme")
                        .to_le_bytes(),
                );
            }
            Input::Text(value) => {
                out.push(5);
                text(out, value);
            }
            Input::Array(values) => {
                out.push(6);
                out.extend_from_slice(&(values.len() as u32).to_le_bytes());
                for value in values {
                    write(out, value);
                }
            }
            Input::Object(values) => {
                out.push(7);
                out.extend_from_slice(&(values.len() as u32).to_le_bytes());
                for (key, value) in values {
                    text(out, key);
                    write(out, value);
                }
            }
        }
    }
    let mut out = vec![0; 4];
    write(&mut out, value);
    let length = (out.len() - 4) as u32;
    out[..4].copy_from_slice(&length.to_le_bytes());
    while !out.len().is_multiple_of(8) {
        out.push(0);
    }
    out
}

/// Append a complete section to an existing SCXF buffer; its string table and
/// existing section offsets stay untouched. This is also used for host-only
/// updates, in an otherwise empty frame, with the same sequence as venue frames.
pub fn append(frame: &mut Vec<u8>, section: Section, payload: &[u8]) {
    assert!(frame.len() >= 16 && &frame[..4] == b"SCXF");
    let count = u16::from_le_bytes([frame[6], frame[7]])
        .checked_add(1)
        .expect("SCXF section count");
    frame[6..8].copy_from_slice(&count.to_le_bytes());
    frame.extend_from_slice(&(section as u16).to_le_bytes());
    frame.extend_from_slice(&0u16.to_le_bytes());
    frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    frame.extend_from_slice(payload);
}

pub fn empty(time: f64) -> Vec<u8> {
    let mut frame = b"SCXF\x01\0\0\0".to_vec();
    frame.extend_from_slice(&time.to_le_bytes());
    frame
}

/// Decode a native section for host-side account-label projection and tests.
/// Length/count guards are checked before allocating or recursing.
pub fn decode(payload: &[u8]) -> Result<Input, &'static str> {
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
        fn text(&mut self) -> Result<Vec<u16>, &'static str> {
            let count = self.count()?;
            Ok(self
                .take(count.checked_mul(2).ok_or("text overflow")?)?
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect())
        }
        fn value(&mut self, depth: usize) -> Result<Input, &'static str> {
            if depth > 128 {
                return Err("native nesting limit");
            }
            Ok(match self.take(1)?[0] {
                0 => Input::Undefined,
                1 => Input::Null,
                2 => Input::Bool(false),
                3 => Input::Bool(true),
                4 => Input::Float(f64::from_le_bytes(self.take(8)?.try_into().unwrap())),
                5 => Input::Text(self.text()?),
                tag @ (6 | 7) => {
                    let count = self.count()?;
                    if count > (self.bytes.len() - self.at) / if tag == 7 { 5 } else { 1 } {
                        return Err("invalid native count");
                    }
                    if tag == 6 {
                        Input::Array(
                            (0..count)
                                .map(|_| self.value(depth + 1))
                                .collect::<Result<_, _>>()?,
                        )
                    } else {
                        let mut rows = Vec::with_capacity(count);
                        let mut keys = std::collections::HashSet::new();
                        for _ in 0..count {
                            let key = self.text()?;
                            if !keys.insert(key.clone()) {
                                return Err("duplicate native key");
                            }
                            rows.push((key, self.value(depth + 1)?));
                        }
                        Input::Object(rows)
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_numbers_text_and_padding_have_independent_bytes() {
        assert_eq!(encode(&Input::Undefined), [1, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(
            encode(&Input::Text(vec![0xd800, 0x61])),
            [9, 0, 0, 0, 5, 2, 0, 0, 0, 0, 0xd8, 0x61, 0, 0, 0, 0]
        );
        let value = encode(&Input::Float(-0.0));
        assert_eq!(&value[5..13], &(-0.0f64).to_le_bytes());
        let mut frame = empty(42.0);
        append(&mut frame, Section::BalanceInfo, &value);
        assert_eq!(&frame[6..8], &[1, 0]);
        assert_eq!(&frame[16..24], &[32, 0, 0, 0, 16, 0, 0, 0]);
        assert_eq!(&frame[24..], &value);
    }
}
