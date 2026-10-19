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
    /// Preserve the surrounding object (with an empty array) for envelope validation.
    EnvelopeField(&'static str),
    /// Either `field: []` or `field: { nested: [] }`, preserving its envelope.
    EnvelopeFieldOrNested(&'static str, &'static str),
    /// The same envelope selection with several accepted nested array names.
    EnvelopeFieldOrNestedAny(&'static str, &'static [&'static str]),
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
    envelope_truncated: bool,
    nested_parent: bool,
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
            envelope_truncated: false,
            nested_parent: false,
        }
    }

    /// Whether the array was found and closed.
    pub fn finished(&self) -> bool {
        self.phase == Phase::Done && !self.envelope_truncated
    }

    /// Only opt-in streams retain metadata; old streams keep their empty response body.
    pub fn envelope(&self) -> Option<&[u8]> {
        matches!(
            self.at,
            ArrayAt::EnvelopeField(_)
                | ArrayAt::EnvelopeFieldOrNested(_, _)
                | ArrayAt::EnvelopeFieldOrNestedAny(_, _)
        )
        .then_some(&self.envelope)
    }

    pub fn feed(&mut self, data: &[u8], mut on_item: impl FnMut(&str)) {
        for &byte in data {
            if matches!(
                self.at,
                ArrayAt::EnvelopeField(_)
                    | ArrayAt::EnvelopeFieldOrNested(_, _)
                    | ArrayAt::EnvelopeFieldOrNestedAny(_, _)
            ) && (self.phase != Phase::InArray
                || (!self.in_string && self.depth == self.array_depth && byte == b']'))
            {
                // Oversized metadata fails JSON validation rather than retaining arbitrary bodies.
                if self.envelope.len() < 16_384 {
                    self.envelope.push(byte);
                } else {
                    self.envelope_truncated = true;
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
                    if self.phase == Phase::Seeking
                        && (self.depth == 1 || self.depth == 2 && self.nested_parent)
                    {
                        self.key.clear();
                        self.capturing_key = true;
                        self.armed = false;
                    }
                }
                b':' if self.phase == Phase::Seeking
                    && (self.depth == 1 || self.depth == 2 && self.nested_parent) =>
                {
                    self.armed = match self.at {
                        ArrayAt::Root => false,
                        ArrayAt::Field(name) | ArrayAt::EnvelopeField(name) => {
                            self.depth == 1 && name.as_bytes() == self.key
                        }
                        ArrayAt::EnvelopeFieldOrNested(field, nested) => {
                            (self.depth == 1 && field.as_bytes() == self.key)
                                || (self.depth == 2
                                    && self.nested_parent
                                    && nested.as_bytes() == self.key)
                        }
                        ArrayAt::EnvelopeFieldOrNestedAny(field, nested) => {
                            (self.depth == 1 && field.as_bytes() == self.key)
                                || (self.depth == 2
                                    && self.nested_parent
                                    && nested.iter().any(|key| key.as_bytes() == self.key))
                        }
                    };
                    continue;
                }
                b'[' | b'{' => {
                    if self.phase == Phase::Seeking
                        && self.depth == 1
                        && self.armed
                        && byte == b'{'
                        && matches!(
                            self.at,
                            ArrayAt::EnvelopeFieldOrNested(_, _)
                                | ArrayAt::EnvelopeFieldOrNestedAny(_, _)
                        )
                    {
                        self.nested_parent = true;
                    }
                    self.depth += 1;
                    if self.phase == Phase::Seeking {
                        let found = byte == b'['
                            && match self.at {
                                ArrayAt::Root => self.depth == 1,
                                ArrayAt::Field(_) | ArrayAt::EnvelopeField(_) => {
                                    self.armed && self.depth == 2
                                }
                                ArrayAt::EnvelopeFieldOrNested(_, _)
                                | ArrayAt::EnvelopeFieldOrNestedAny(_, _) => {
                                    self.armed
                                        && (self.depth == 2
                                            || self.depth == 3 && self.nested_parent)
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
                        self.emit(&mut on_item);
                        self.phase = Phase::Done;
                        continue;
                    }
                    self.depth = self.depth.saturating_sub(1);
                    if self.depth < 2 {
                        self.nested_parent = false;
                    }
                }
                b',' if at_array_level => {
                    self.emit(&mut on_item);
                    continue;
                }
                b' ' | b'\n' | b'\r' | b'\t' if at_array_level => continue,
                b' ' | b'\n' | b'\r' | b'\t' => {}
                _ if self.phase == Phase::Seeking && self.depth == 1 => self.armed = false,
                _ => {}
            }
            if self.phase == Phase::InArray {
                self.item.push(byte);
            }
        }
    }

    fn emit(&mut self, on_item: &mut impl FnMut(&str)) {
        if !self.item.is_empty() {
            if let Ok(text) = std::str::from_utf8(&self.item) {
                on_item(text);
            }
            self.item.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn nested_array_aliases_retain_valid_envelopes_across_chunks() {
        for body in [
            r#"{"data":[{"id":1}],"code":0}"#,
            r#"{"data":{"fill_orders":[{"id":1}]},"code":0}"#,
            r#"{"data":{"fill_history_orders":[{"id":1}]},"code":0}"#,
        ] {
            let mut splitter = ArraySplitter::new(ArrayAt::EnvelopeFieldOrNestedAny(
                "data",
                &["fill_orders", "fill_history_orders"],
            ));
            let mut rows = 0;
            for b in body.as_bytes().chunks(2) {
                splitter.feed(b, |_| rows += 1)
            }
            assert!(splitter.finished());
            assert_eq!(rows, 1);
            assert_eq!(
                std::str::from_utf8(splitter.envelope().unwrap()).unwrap(),
                body.replace(r#"{"id":1}"#, "")
            );
        }
    }

    #[test]
    fn streams_either_direct_or_nested_envelope_arrays() {
        for text in [
            r#"{"code":0,"data":[{"n":1}],"msg":""}"#,
            r#"{"code":0,"data":{"orders":[{"n":1}]},"msg":""}"#,
        ] {
            for size in [1, 3, 1000] {
                let mut splitter =
                    ArraySplitter::new(ArrayAt::EnvelopeFieldOrNested("data", "orders"));
                let mut items = Vec::new();
                for chunk in text.as_bytes().chunks(size) {
                    splitter.feed(chunk, |s| items.push(s.to_owned()))
                }
                assert!(splitter.finished());
                assert_eq!(items, [r#"{"n":1}"#]);
                let envelope = std::str::from_utf8(splitter.envelope().unwrap()).unwrap();
                assert_eq!(envelope, text.replace(r#"{"n":1}"#, ""));
            }
        }
    }

    #[test]
    fn rejects_an_oversized_stream_envelope() {
        let mut splitter = ArraySplitter::new(ArrayAt::EnvelopeField("data"));
        splitter.feed(
            format!(r#"{{"code":0,"data":[]}}{}garbage"#, " ".repeat(16_384)).as_bytes(),
            |_| {},
        );
        assert!(!splitter.finished());
    }

    #[test]
    fn retains_envelope_fields_on_either_side_of_the_stream() {
        let text = r#"{"code":0,"data":[{"n":[1,2],"s":"],}"},{"n":3}],"msg":"ok"}"#;
        for size in [1, 2, 7, 1000] {
            let mut splitter = ArraySplitter::new(ArrayAt::EnvelopeField("data"));
            let mut count = 0;
            for chunk in text.as_bytes().chunks(size) {
                splitter.feed(chunk, |_| count += 1)
            }
            assert_eq!(count, 2);
            assert_eq!(
                splitter.envelope(),
                Some(br#"{"code":0,"data":[],"msg":"ok"}"#.as_slice())
            );
        }
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
