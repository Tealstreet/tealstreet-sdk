//! Streaming decode of large REST lists: each element is decoded, handed to
//! the caller and dropped before the next one, so a multi-megabyte response
//! never becomes a vector of wire records. In WebAssembly, linear memory never
//! shrinks, so the peak of a startup response would otherwise stay reserved.
use serde::Deserialize;
use serde::de::{DeserializeSeed, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
use std::fmt;
use std::marker::PhantomData;

struct Each<'f, T, F> {
    each: &'f mut F,
    _item: PhantomData<fn() -> T>,
}

impl<'de, T: Deserialize<'de>, F: FnMut(T)> DeserializeSeed<'de> for Each<'_, T, F> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_seq(self)
    }
}

impl<'de, T: Deserialize<'de>, F: FnMut(T)> Visitor<'de> for Each<'_, T, F> {
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an array")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        while let Some(item) = seq.next_element::<T>()? {
            (self.each)(item);
        }
        Ok(())
    }
}

struct Field<'f, T, F> {
    name: &'f str,
    each: &'f mut F,
    found: bool,
    _item: PhantomData<fn() -> T>,
}

impl<'de, T: Deserialize<'de>, F: FnMut(T)> Visitor<'de> for Field<'_, T, F> {
    type Value = bool;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an object")
    }

    fn visit_map<A: MapAccess<'de>>(mut self, mut map: A) -> Result<bool, A::Error> {
        while let Some(key) = map.next_key::<std::borrow::Cow<'de, str>>()? {
            if key == self.name {
                map.next_value_seed(Each {
                    each: self.each,
                    _item: PhantomData::<fn() -> T>,
                })?;
                self.found = true;
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        Ok(self.found)
    }
}

/// Decode a top-level JSON array one element at a time.
pub fn each<'de, T: Deserialize<'de>>(
    text: &'de str,
    mut each: impl FnMut(T),
) -> serde_json::Result<()> {
    let mut deserializer = serde_json::Deserializer::from_str(text);
    Each {
        each: &mut each,
        _item: PhantomData,
    }
    .deserialize(&mut deserializer)?;
    deserializer.end()
}

/// Decode the array at `field` of a top-level object one element at a time.
/// Returns whether the field was present.
pub fn each_in_field<'de, T: Deserialize<'de>>(
    text: &'de str,
    field: &str,
    mut each: impl FnMut(T),
) -> serde_json::Result<bool> {
    let mut deserializer = serde_json::Deserializer::from_str(text);
    let found = deserializer.deserialize_map(Field {
        name: field,
        each: &mut each,
        found: false,
        _item: PhantomData,
    })?;
    deserializer.end()?;
    Ok(found)
}

/// Where the array to stream sits in a response body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrayAt {
    /// The body is the array.
    Root,
    /// The array is this field of the top-level object.
    Field(&'static str),
    /// Like `Field`, but return the surrounding object (with an empty array)
    /// to the venue so a 200 error envelope cannot become an empty snapshot.
    EnvelopeField(&'static str),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Seeking,
    InArray,
    Done,
}

/// Splits an array out of a body that arrives in chunks, handing over each
/// element's text as soon as it is complete. Only the element in progress is
/// buffered, so memory stays at the size of one element however large the
/// body is.
pub struct ArraySplitter {
    at: ArrayAt,
    phase: Phase,
    depth: u32,
    array_depth: u32,
    in_string: bool,
    escaped: bool,
    /// The most recent string at depth 1 while seeking a field.
    key: Vec<u8>,
    capturing_key: bool,
    /// The wanted key and its colon were just seen.
    armed: bool,
    item: Vec<u8>,
    envelope: Vec<u8>,
    invalid: bool,
    after_separator: bool,
}

impl ArraySplitter {
    pub fn new(at: ArrayAt) -> Self {
        Self {
            at,
            phase: Phase::Seeking,
            depth: 0,
            array_depth: 0,
            in_string: false,
            escaped: false,
            key: Vec::new(),
            capturing_key: false,
            armed: false,
            item: Vec::new(),
            envelope: Vec::new(),
            invalid: false,
            after_separator: false,
        }
    }

    /// Whether the array was found and closed.
    pub fn finished(&self) -> bool {
        self.phase == Phase::Done && !self.invalid
    }

    pub fn envelope(&self) -> Option<&str> {
        matches!(self.at, ArrayAt::EnvelopeField(_))
            .then(|| std::str::from_utf8(&self.envelope).ok())
            .flatten()
    }

    pub fn feed(&mut self, data: &[u8], mut on_item: impl FnMut(&str)) {
        for &byte in data {
            if matches!(self.at, ArrayAt::EnvelopeField(_)) && self.phase != Phase::InArray {
                if self.envelope.len() < 16_384 {
                    self.envelope.push(byte);
                } else {
                    self.invalid = true;
                }
            }
            if self.phase == Phase::Done {
                continue;
            }
            if self.in_string {
                if self.phase == Phase::InArray {
                    self.item.push(byte);
                } else if self.capturing_key {
                    self.key.push(byte);
                }
                if self.escaped {
                    self.escaped = false;
                } else if byte == b'\\' {
                    self.escaped = true;
                } else if byte == b'"' {
                    self.in_string = false;
                    if self.capturing_key {
                        self.capturing_key = false;
                        self.key.pop();
                    }
                }
                continue;
            }
            let at_array_level = self.phase == Phase::InArray && self.depth == self.array_depth;
            match byte {
                b'"' => {
                    self.in_string = true;
                    if self.phase == Phase::Seeking && self.depth == 1 {
                        self.key.clear();
                        self.capturing_key = true;
                        self.armed = false;
                    }
                }
                b':' if self.phase == Phase::Seeking && self.depth == 1 => {
                    self.armed = matches!(self.at, ArrayAt::Field(name) | ArrayAt::EnvelopeField(name) if name.as_bytes() == self.key);
                    continue;
                }
                b'[' | b'{' => {
                    self.depth += 1;
                    if self.phase == Phase::Seeking {
                        let found = byte == b'['
                            && match self.at {
                                ArrayAt::Root => self.depth == 1,
                                ArrayAt::Field(_) | ArrayAt::EnvelopeField(_) => {
                                    self.armed && self.depth == 2
                                }
                            };
                        self.armed = false;
                        if found {
                            self.phase = Phase::InArray;
                            self.array_depth = self.depth;
                            continue;
                        }
                    }
                }
                b']' | b'}' => {
                    if at_array_level && byte == b']' {
                        self.invalid |= self.after_separator;
                        self.emit(&mut on_item);
                        if matches!(self.at, ArrayAt::EnvelopeField(_)) {
                            self.envelope.push(b']');
                        }
                        self.phase = Phase::Done;
                        continue;
                    }
                    self.depth = self.depth.saturating_sub(1);
                }
                b',' if at_array_level => {
                    self.invalid |= self.item.is_empty();
                    self.emit(&mut on_item);
                    self.after_separator = true;
                    continue;
                }
                b' ' | b'\n' | b'\r' | b'\t' if at_array_level => continue,
                b' ' | b'\n' | b'\r' | b'\t' => {}
                _ if self.phase == Phase::Seeking && self.depth == 1 => self.armed = false,
                _ => {}
            }
            if self.phase == Phase::InArray {
                self.item.push(byte);
                self.after_separator = false;
            }
        }
    }

    fn emit(&mut self, on_item: &mut impl FnMut(&str)) {
        if !self.item.is_empty() {
            if let Ok(text) = std::str::from_utf8(&self.item) {
                on_item(text);
            } else {
                self.invalid = true;
            }
            self.item.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_streamed_lists_are_not_successful_empty_snapshots() {
        for body in [
            br#"{"data":[,]}"#.as_slice(),
            br#"{"data":[1,]}"#.as_slice(),
            b"{\"data\":[\"\xff\"]}".as_slice(),
        ] {
            let mut split = ArraySplitter::new(ArrayAt::EnvelopeField("data"));
            split.feed(body, |_| {});
            assert!(!split.finished());
        }
    }

    #[test]
    fn streamed_envelope_retains_fields_before_and_after_the_array() {
        let body = r#"{"code":510,"data":[{"x":[1,2]},3],"success":false}"#;
        for chunk in 1..body.len() {
            let mut split = ArraySplitter::new(ArrayAt::EnvelopeField("data"));
            let mut items = Vec::new();
            for bytes in body.as_bytes().chunks(chunk) {
                split.feed(bytes, |s| items.push(s.to_owned()));
            }
            assert!(split.finished());
            assert_eq!(items, [r#"{"x":[1,2]}"#, "3"]);
            assert_eq!(
                split.envelope(),
                Some(r#"{"code":510,"data":[],"success":false}"#)
            );
        }
    }

    fn split(at: ArrayAt, body: &str, chunk: usize) -> Vec<String> {
        let mut splitter = ArraySplitter::new(at);
        let mut items = Vec::new();
        for part in body.as_bytes().chunks(chunk) {
            splitter.feed(part, |item| items.push(item.to_owned()));
        }
        assert!(splitter.finished());
        items
    }

    #[test]
    fn splits_elements_across_any_chunking() {
        let body = r#"{"timezone":"UTC","rateLimits":[{"a":1}],"symbols" : [ {"s":"a,]\"}","f":[{"x":[1,2]}]} , {"s":"é"} ],"after":[9]}"#;
        for chunk in [1, 2, 3, 7, 1024] {
            assert_eq!(
                split(ArrayAt::Field("symbols"), body, chunk),
                [r#"{"s":"a,]\"}","f":[{"x":[1,2]}]}"#, r#"{"s":"é"}"#],
                "chunk {chunk}"
            );
        }
        assert_eq!(
            split(ArrayAt::Root, r#"[{"v":1},{"v":[2]}]"#, 3),
            [r#"{"v":1}"#, r#"{"v":[2]}"#]
        );
        assert!(split(ArrayAt::Root, "[]", 1).is_empty());
        let mut splitter = ArraySplitter::new(ArrayAt::Field("symbols"));
        splitter.feed(br#"{"other":[1],"symbolsX":[2]}"#, |_| panic!("no match"));
        assert!(!splitter.finished());
    }

    #[derive(Deserialize)]
    struct Row<'a> {
        s: &'a str,
        v: u32,
    }

    #[test]
    fn streams_arrays_and_object_fields() {
        let mut seen = Vec::new();
        each::<Row>(r#"[{"s":"a","v":1},{"s":"b","v":2}]"#, |r| {
            seen.push((r.s.to_owned(), r.v))
        })
        .unwrap();
        assert_eq!(seen, [("a".into(), 1), ("b".into(), 2)]);

        let mut seen = Vec::new();
        let body = r#"{"timezone":"UTC","symbols":[{"s":"x","v":3}],"rateLimits":[]}"#;
        assert!(each_in_field::<Row>(body, "symbols", |r| seen.push(r.v)).unwrap());
        assert_eq!(seen, [3]);
        assert!(!each_in_field::<Row>(r#"{"other":[]}"#, "symbols", |_| {}).unwrap());
        assert!(each::<Row>("[1]", |_| {}).is_err());
    }
}
