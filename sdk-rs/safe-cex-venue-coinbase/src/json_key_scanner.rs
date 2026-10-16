//! The source grammar walk over JavaScript UTF-16 units; values are never decoded.
use std::collections::HashSet;

/// Internal source JsonKeyScanResult, absent from the extracted symbol inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonKeyScanResult {
    Duplicate,
    Invalid,
    Valid,
}

// ts: JsonDuplicateKeyScanner
pub struct JsonDuplicateKeyScanner {
    duplicate: bool,
    index: usize,
    raw: Vec<u16>,
}
impl JsonDuplicateKeyScanner {
    // ts: JsonDuplicateKeyScanner.constructor
    pub fn new(raw: &[u16]) -> Self {
        Self {
            duplicate: false,
            index: 0,
            raw: raw.to_vec(),
        }
    }
    // ts: JsonDuplicateKeyScanner.scan
    pub fn scan(&mut self) -> JsonKeyScanResult {
        if self.scan_value(0).is_err() {
            return JsonKeyScanResult::Invalid;
        }
        self.skip_whitespace();
        if self.index != self.raw.len() {
            return JsonKeyScanResult::Invalid;
        }
        if self.duplicate {
            JsonKeyScanResult::Duplicate
        } else {
            JsonKeyScanResult::Valid
        }
    }
    // ts: JsonDuplicateKeyScanner#scanValue
    fn scan_value(&mut self, depth: usize) -> Result<(), ()> {
        if depth > 128 {
            return Err(());
        }
        self.skip_whitespace();
        match self.raw.get(self.index).copied() {
            Some(123) => self.scan_object(depth + 1),
            Some(91) => self.scan_array(depth + 1),
            Some(34) => self.scan_string().map(|_| ()),
            Some(45 | 48..=57) => self.scan_number(),
            _ if self.consume_literal("true")
                || self.consume_literal("false")
                || self.consume_literal("null") =>
            {
                Ok(())
            }
            _ => Err(()),
        }
    }
    // ts: JsonDuplicateKeyScanner#scanObject
    fn scan_object(&mut self, depth: usize) -> Result<(), ()> {
        self.index += 1;
        self.skip_whitespace();
        if self.raw.get(self.index) == Some(&125) {
            self.index += 1;
            return Ok(());
        }
        let mut keys = HashSet::new();
        loop {
            self.skip_whitespace();
            let key = self.scan_string()?;
            if !keys.insert(key) {
                self.duplicate = true;
            }
            self.skip_whitespace();
            if self.raw.get(self.index) != Some(&58) {
                return Err(());
            }
            self.index += 1;
            self.scan_value(depth)?;
            self.skip_whitespace();
            match self.raw.get(self.index) {
                Some(125) => {
                    self.index += 1;
                    return Ok(());
                }
                Some(44) => self.index += 1,
                _ => return Err(()),
            }
        }
    }
    // ts: JsonDuplicateKeyScanner#scanArray
    fn scan_array(&mut self, depth: usize) -> Result<(), ()> {
        self.index += 1;
        self.skip_whitespace();
        if self.raw.get(self.index) == Some(&93) {
            self.index += 1;
            return Ok(());
        }
        loop {
            self.scan_value(depth)?;
            self.skip_whitespace();
            match self.raw.get(self.index) {
                Some(93) => {
                    self.index += 1;
                    return Ok(());
                }
                Some(44) => self.index += 1,
                _ => return Err(()),
            }
        }
    }
    // ts: JsonDuplicateKeyScanner#scanString
    fn scan_string(&mut self) -> Result<Vec<u16>, ()> {
        if self.raw.get(self.index) != Some(&34) {
            return Err(());
        }
        let start = self.index;
        self.index += 1;
        while self.index < self.raw.len() {
            let token = self.raw[self.index];
            if token == 34 {
                self.index += 1;
                return decode_json_string(&self.raw[start..self.index]);
            }
            self.index += if token == 92 { 2 } else { 1 };
        }
        Err(())
    }
    // ts: JsonDuplicateKeyScanner#scanNumber
    fn scan_number(&mut self) -> Result<(), ()> {
        // Exact prefix match of /^-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?/.
        let start = self.index;
        if self.raw.get(self.index) == Some(&45) {
            self.index += 1;
        }
        match self.raw.get(self.index) {
            Some(48) => self.index += 1,
            Some(49..=57) => {
                self.index += 1;
                while self
                    .raw
                    .get(self.index)
                    .is_some_and(|c| (48..=57).contains(c))
                {
                    self.index += 1;
                }
            }
            _ => {
                self.index = start;
                return Err(());
            }
        }
        if self.raw.get(self.index) == Some(&46)
            && self
                .raw
                .get(self.index + 1)
                .is_some_and(|c| (48..=57).contains(c))
        {
            self.index += 2;
            while self
                .raw
                .get(self.index)
                .is_some_and(|c| (48..=57).contains(c))
            {
                self.index += 1;
            }
        }
        if self
            .raw
            .get(self.index)
            .is_some_and(|c| *c == 69 || *c == 101)
        {
            let mut end = self.index + 1;
            if self.raw.get(end).is_some_and(|c| *c == 43 || *c == 45) {
                end += 1;
            }
            if self.raw.get(end).is_some_and(|c| (48..=57).contains(c)) {
                self.index = end + 1;
                while self
                    .raw
                    .get(self.index)
                    .is_some_and(|c| (48..=57).contains(c))
                {
                    self.index += 1;
                }
            }
        }
        Ok(())
    }
    // ts: JsonDuplicateKeyScanner#consumeLiteral
    fn consume_literal(&mut self, literal: &str) -> bool {
        let units: Vec<u16> = literal.encode_utf16().collect();
        if !self.raw[self.index.min(self.raw.len())..].starts_with(&units) {
            return false;
        }
        self.index += units.len();
        true
    }
    // ts: JsonDuplicateKeyScanner#skipWhitespace
    fn skip_whitespace(&mut self) {
        while self
            .raw
            .get(self.index)
            .is_some_and(|c| matches!(c, 32 | 10 | 13 | 9))
        {
            self.index += 1;
        }
    }
}

// JSON.parse of a single string token, retaining unpaired surrogate units and
// avoiding Rust str's narrower domain. No number/value parsing or normalization.
pub(crate) fn decode_json_string(raw: &[u16]) -> Result<Vec<u16>, ()> {
    let mut out = Vec::new();
    let end = raw.len() - 1;
    let mut index = 1;
    while index < end {
        let unit = raw[index];
        if unit < 32 || unit == 34 {
            return Err(());
        }
        index += 1;
        if unit != 92 {
            out.push(unit);
            continue;
        }
        let escape = *raw.get(index).filter(|_| index < end).ok_or(())?;
        index += 1;
        out.push(match escape {
            34 | 47 | 92 => escape,
            98 => 8,
            102 => 12,
            110 => 10,
            114 => 13,
            116 => 9,
            117 => {
                if index + 4 > end {
                    return Err(());
                }
                let mut value = 0u16;
                for &hex in &raw[index..index + 4] {
                    let digit = match hex {
                        48..=57 => hex - 48,
                        65..=70 => hex - 65 + 10,
                        97..=102 => hex - 97 + 10,
                        _ => return Err(()),
                    };
                    value = value * 16 + digit;
                }
                index += 4;
                value
            }
            _ => return Err(()),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decoded_key_units_preserve_surrogates_and_json_escapes() {
        for (raw, expected) in [
            (r#""\ud800""#, vec![0xd800]),
            (r#""\ud83d\ude00""#, vec![0xd83d, 0xde00]),
            (
                r#""\b\f\n\r\t\/\\\"\u0100""#,
                vec![8, 12, 10, 13, 9, 47, 92, 34, 256],
            ),
        ] {
            let units: Vec<u16> = raw.encode_utf16().collect();
            let actual = JsonDuplicateKeyScanner::new(&units).scan_string().unwrap();
            assert_eq!(actual, expected, "decoded native JSON key code units");
        }
    }
}
