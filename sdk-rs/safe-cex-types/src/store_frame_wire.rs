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
        for unit in value { out.extend_from_slice(&unit.to_le_bytes()); }
    }
    fn write(out: &mut Vec<u8>, value: &Input) {
        match value {
            Input::Undefined => out.push(0),
            Input::Null => out.push(1),
            Input::Bool(false) => out.push(2),
            Input::Bool(true) => out.push(3),
            Input::Float(value) => { out.push(4); out.extend_from_slice(&value.to_le_bytes()); }
            Input::Lexeme(value) => {
                out.push(4);
                out.extend_from_slice(&value.parse::<f64>().expect("native numeric lexeme").to_le_bytes());
            }
            Input::Text(value) => { out.push(5); text(out, value); }
            Input::Array(values) => {
                out.push(6); out.extend_from_slice(&(values.len() as u32).to_le_bytes());
                for value in values { write(out, value); }
            }
            Input::Object(values) => {
                out.push(7); out.extend_from_slice(&(values.len() as u32).to_le_bytes());
                for (key, value) in values { text(out, key); write(out, value); }
            }
        }
    }
    let mut out = vec![0; 4];
    write(&mut out, value);
    let length = (out.len() - 4) as u32;
    out[..4].copy_from_slice(&length.to_le_bytes());
    while !out.len().is_multiple_of(8) { out.push(0); }
    out
}

/// Append a complete section to an existing SCXF buffer; its string table and
/// existing section offsets stay untouched. This is also used for host-only
/// updates, in an otherwise empty frame, with the same sequence as venue frames.
pub fn append(frame: &mut Vec<u8>, section: Section, payload: &[u8]) {
    assert!(frame.len() >= 16 && &frame[..4] == b"SCXF");
    let count = u16::from_le_bytes([frame[6], frame[7]]).checked_add(1).expect("SCXF section count");
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_numbers_text_and_padding_have_independent_bytes() {
        assert_eq!(encode(&Input::Undefined), [1, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(encode(&Input::Text(vec![0xd800, 0x61])), [9, 0, 0, 0, 5, 2, 0, 0, 0, 0, 0xd8, 0x61, 0, 0, 0, 0]);
        let value = encode(&Input::Float(-0.0));
        assert_eq!(&value[5..13], &(-0.0f64).to_le_bytes());
        let mut frame = empty(42.0);
        append(&mut frame, Section::BalanceInfo, &value);
        assert_eq!(&frame[6..8], &[1, 0]);
        assert_eq!(&frame[16..24], &[32, 0, 0, 0, 16, 0, 0, 0]);
        assert_eq!(&frame[24..], &value);
    }
}
