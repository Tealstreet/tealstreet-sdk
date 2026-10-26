//! Single JSON grammar for native-number parsing and retained lexical capture.
//! Container frames avoid a Rust recursion limit on valid nested JSON.
use super::RetainedJsonInput as Input;
use serde::de::Error as _;
use serde_json::Error;

enum Frame {
    Array(Vec<Input>),
    Object(Vec<(Vec<u16>, Input)>, Vec<u16>),
}
struct Parser<'a> {
    text: &'a [u16],
    cursor: usize,
    lexical: bool,
}
impl Parser<'_> {
    fn error(&self) -> Error {
        Error::custom(format!("Invalid JSON at position {}", self.cursor))
    }
    fn space(&mut self) {
        while self
            .text
            .get(self.cursor)
            .is_some_and(|unit| matches!(unit, 0x20 | 0x09 | 0x0a | 0x0d))
        {
            self.cursor += 1;
        }
    }
    fn take(&mut self, byte: u8) -> bool {
        if self.text.get(self.cursor) == Some(&u16::from(byte)) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }
    fn literal(&mut self, word: &str) -> bool {
        if self
            .text
            .get(self.cursor..self.cursor + word.len())
            .is_some_and(|units| units.iter().copied().eq(word.encode_utf16()))
        {
            self.cursor += word.len();
            true
        } else {
            false
        }
    }
    fn string(&mut self) -> Result<Vec<u16>, Error> {
        if !self.take(b'"') {
            return Err(self.error());
        }
        let mut units = Vec::new();
        loop {
            let start = self.cursor;
            while let Some(unit) = self.text.get(self.cursor) {
                if matches!(unit, 0..=0x1f | 0x22 | 0x5c) {
                    break;
                }
                self.cursor += 1;
            }
            units.extend_from_slice(&self.text[start..self.cursor]);
            let Some(&unit) = self.text.get(self.cursor) else {
                return Err(self.error());
            };
            self.cursor += 1;
            match unit {
                0x22 => return Ok(units),
                0x5c => {
                    let Some(&escape) = self.text.get(self.cursor) else {
                        return Err(self.error());
                    };
                    self.cursor += 1;
                    units.push(match escape {
                        0x22 | 0x5c | 0x2f => escape,
                        0x62 => 0x08,
                        0x66 => 0x0c,
                        0x6e => 0x0a,
                        0x72 => 0x0d,
                        0x74 => 0x09,
                        0x75 => {
                            let mut value = 0;
                            for _ in 0..4 {
                                let Some(&digit) = self.text.get(self.cursor) else {
                                    return Err(self.error());
                                };
                                self.cursor += 1;
                                let digit = match digit {
                                    0x30..=0x39 => digit - 0x30,
                                    0x41..=0x46 => digit - 0x41 + 10,
                                    0x61..=0x66 => digit - 0x61 + 10,
                                    _ => return Err(self.error()),
                                };
                                value = value * 16 + digit;
                            }
                            value
                        }
                        _ => return Err(self.error()),
                    });
                }
                0..=0x1f => return Err(self.error()),
                _ => unreachable!("unescaped string chunk already consumed"),
            }
        }
    }
    fn digits(&mut self) -> bool {
        let start = self.cursor;
        while self
            .text
            .get(self.cursor)
            .is_some_and(|digit| matches!(digit, 0x30..=0x39))
        {
            self.cursor += 1;
        }
        self.cursor > start
    }
    fn number(&mut self) -> Result<Input, Error> {
        let start = self.cursor;
        self.take(b'-');
        if !self.take(b'0') && !self.digits() {
            return Err(self.error());
        }
        if self.take(b'.') && !self.digits() {
            return Err(self.error());
        }
        if self.take(b'e') || self.take(b'E') {
            if !self.take(b'+') {
                self.take(b'-');
            }
            if !self.digits() {
                return Err(self.error());
            }
        }
        let text = String::from_utf16(&self.text[start..self.cursor])
            .expect("JSON number grammar is ASCII");
        if self.lexical {
            Ok(Input::Lexeme(text))
        } else {
            text.parse().map(Input::Float).map_err(|_| self.error())
        }
    }
    fn key(&mut self) -> Result<Vec<u16>, Error> {
        self.space();
        let key = self.string()?;
        self.space();
        if !self.take(b':') {
            return Err(self.error());
        }
        Ok(key)
    }
    fn start_value(&mut self, stack: &mut Vec<Frame>) -> Result<Option<Input>, Error> {
        self.space();
        Ok(match self.text.get(self.cursor) {
            Some(0x22) => Some(Input::Text(self.string()?)),
            Some(0x6e) if self.literal("null") => Some(Input::Null),
            Some(0x74) if self.literal("true") => Some(Input::Bool(true)),
            Some(0x66) if self.literal("false") => Some(Input::Bool(false)),
            Some(0x2d | 0x30..=0x39) => Some(self.number()?),
            Some(0x5b) => {
                self.cursor += 1;
                self.space();
                if self.take(b']') {
                    Some(Input::Array(Vec::new()))
                } else {
                    stack.push(Frame::Array(Vec::new()));
                    None
                }
            }
            Some(0x7b) => {
                self.cursor += 1;
                self.space();
                let object = Vec::new();
                if self.take(b'}') {
                    Some(Input::Object(object))
                } else {
                    stack.push(Frame::Object(object, self.key()?));
                    None
                }
            }
            _ => return Err(self.error()),
        })
    }
}

pub(super) fn parse(text: &[u16], lexical: bool, initial_depth: usize) -> Result<Input, Error> {
    let mut parser = Parser {
        text,
        cursor: 0,
        lexical,
    };
    let mut stack = Vec::new();
    loop {
        let Some(mut value) = parser.start_value(&mut stack)? else {
            continue;
        };
        loop {
            parser.space();
            match stack.pop() {
                None => {
                    return if parser.cursor == text.len() {
                        if lexical {
                            let mut pending = vec![(&value, initial_depth)];
                            while let Some((node, depth)) = pending.pop() {
                                if depth >= 128 {
                                    return Err(Error::custom(
                                        "numeric ingress recursion limit exceeded",
                                    ));
                                }
                                match node {
                                    Input::Array(values) => {
                                        pending.extend(values.iter().map(|v| (v, depth + 1)))
                                    }
                                    Input::Object(values) => {
                                        pending.extend(values.iter().map(|(_, v)| (v, depth + 1)))
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Ok(value)
                    } else {
                        Err(parser.error())
                    };
                }
                Some(Frame::Array(mut values)) => {
                    values.push(value);
                    if parser.take(b']') {
                        value = Input::Array(values);
                    } else if parser.take(b',') {
                        stack.push(Frame::Array(values));
                        break;
                    } else {
                        return Err(parser.error());
                    }
                }
                Some(Frame::Object(mut object, key)) => {
                    // JSON.parse creates own data properties, including __proto__,
                    // and replaces duplicate values without moving the first key.
                    if let Some((_, old)) = object.iter_mut().find(|(name, _)| name == &key) {
                        *old = value;
                    } else {
                        object.push((key, value));
                    }
                    if parser.take(b'}') {
                        value = Input::Object(object);
                    } else if parser.take(b',') {
                        stack.push(Frame::Object(object, parser.key()?));
                        break;
                    } else {
                        return Err(parser.error());
                    }
                }
            }
        }
    }
}
