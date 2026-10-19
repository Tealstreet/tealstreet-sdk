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
    /// Like `Field`, retaining the surrounding object for `on_response`.
    EnvelopeField(&'static str),
    /// First matching array field at any depth in a versioned envelope.
    AnyFieldEnvelope(&'static [&'static str]),
    /// The array sits under these nested object fields (`result.list`); its
    /// surrounding object is retained for `on_response`.
    Path(&'static [&'static str]),
}

impl ArrayAt {
    pub(crate) fn retains_envelope(self) -> bool {
        matches!(
            self,
            Self::EnvelopeField(_) | Self::AnyFieldEnvelope(_) | Self::Path(_)
        )
    }

    fn len(self) -> usize {
        match self {
            Self::Root | Self::AnyFieldEnvelope(_) => 0,
            Self::Field(_) | Self::EnvelopeField(_) => 1,
            Self::Path(path) => path.len(),
        }
    }

    fn segment(self, index: usize) -> Option<&'static str> {
        match self {
            Self::Root | Self::AnyFieldEnvelope(_) => None,
            Self::Field(name) | Self::EnvelopeField(name) => (index == 0).then_some(name),
            Self::Path(path) => path.get(index).copied(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Seeking,
    InArray,
    Done,
}

/// The bytes outside the streamed array are kept up to this size, so an
/// envelope's other fields (a page cursor, an error) stay readable.
const OUTSIDE_LIMIT: usize = 16 * 1024;

/// Splits an array out of a body that arrives in chunks, handing over each
/// element's text as soon as it is complete. Only the element in progress is
/// buffered, so memory stays at the size of one element however large the
/// body is. Everything around the array is kept, with the array emptied.
pub struct ArraySplitter {
    at: ArrayAt,
    phase: Phase,
    depth: u32,
    array_depth: u32,
    /// Path segments matched so far; the next is sought at depth `matched + 1`.
    matched: u32,
    in_string: bool,
    escaped: bool,
    /// The most recent string at the seeking depth.
    key: Vec<u8>,
    capturing_key: bool,
    /// The wanted key and its colon were just seen.
    armed: bool,
    item: Vec<u8>,
    outside: Vec<u8>,
    invalid: bool,
    after_comma: bool,
}

impl ArraySplitter {
    pub fn new(at: ArrayAt) -> Self {
        Self {
            at,
            phase: Phase::Seeking,
            depth: 0,
            array_depth: 0,
            matched: 0,
            in_string: false,
            escaped: false,
            key: Vec::new(),
            capturing_key: false,
            armed: false,
            item: Vec::new(),
            outside: Vec::new(),
            invalid: false,
            after_comma: false,
        }
    }

    /// Whether the array was found and closed without a framing, UTF-8 or
    /// metadata-overflow error.
    pub fn finished(&self) -> bool {
        self.phase == Phase::Done && !self.invalid
    }

    /// The body without the array's elements (`"list":[]`), up to 16 KiB.
    pub fn outside(&self) -> &[u8] {
        &self.outside
    }

    /// Validated surrounding JSON for modes that opt into response metadata.
    /// Legacy `Root` and `Field` callers continue to receive an empty body.
    pub fn envelope(&self) -> Option<&[u8]> {
        if !self.at.retains_envelope() || !self.finished() {
            return None;
        }
        let text = std::str::from_utf8(&self.outside).ok()?;
        serde_json::from_str::<IgnoredAny>(text).ok()?;
        Some(&self.outside)
    }

    fn keep(&mut self, byte: u8) {
        if self.outside.len() < OUTSIDE_LIMIT {
            self.outside.push(byte);
        } else if self.at.retains_envelope() {
            // Never report a truncated status/cursor envelope as successful.
            self.invalid = true;
        }
    }

    pub fn feed(&mut self, data: &[u8], mut on_item: impl FnMut(&str)) {
        for &byte in data {
            if self.phase == Phase::Done {
                self.keep(byte);
                continue;
            }
            let seek_depth = self.matched + 1;
            if self.in_string {
                if self.phase == Phase::InArray {
                    self.item.push(byte);
                    self.after_comma = false;
                } else {
                    self.keep(byte);
                    if self.capturing_key {
                        self.key.push(byte);
                    }
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
                        && (matches!(self.at, ArrayAt::AnyFieldEnvelope(_))
                            || self.depth == seek_depth)
                    {
                        self.key.clear();
                        self.capturing_key = true;
                        self.armed = false;
                    }
                }
                b':' if self.phase == Phase::Seeking
                    && (matches!(self.at, ArrayAt::AnyFieldEnvelope(_))
                        || self.depth == seek_depth) =>
                {
                    self.armed = match self.at {
                        ArrayAt::AnyFieldEnvelope(names) => {
                            names.iter().any(|name| name.as_bytes() == self.key)
                        }
                        _ => self
                            .at
                            .segment(self.matched as usize)
                            .is_some_and(|name| name.as_bytes() == self.key),
                    };
                    self.keep(byte);
                    continue;
                }
                b'[' | b'{' => {
                    self.depth += 1;
                    if self.phase == Phase::Seeking {
                        let last = self.matched as usize + 1 == self.at.len();
                        let found = byte == b'['
                            && match self.at {
                                ArrayAt::Root => self.depth == 1,
                                ArrayAt::AnyFieldEnvelope(_) => self.armed,
                                _ => self.armed && last && self.depth == seek_depth + 1,
                            };
                        let descend = !matches!(self.at, ArrayAt::AnyFieldEnvelope(_))
                            && byte == b'{'
                            && self.armed
                            && !last
                            && self.depth == seek_depth + 1;
                        self.armed = false;
                        if found {
                            self.phase = Phase::InArray;
                            self.array_depth = self.depth;
                            self.keep(byte);
                            continue;
                        }
                        if descend {
                            self.matched += 1;
                        }
                    }
                }
                b']' | b'}' => {
                    if at_array_level && byte == b']' {
                        if self.item.is_empty() && self.after_comma {
                            self.invalid = true;
                        }
                        self.emit(&mut on_item);
                        self.phase = Phase::Done;
                        self.depth = self.depth.saturating_sub(1);
                        self.keep(byte);
                        continue;
                    }
                    self.depth = self.depth.saturating_sub(1);
                    if self.phase == Phase::Seeking && self.depth < self.matched + 1 {
                        self.matched = self.depth.saturating_sub(1);
                    }
                }
                b',' if at_array_level => {
                    if self.item.is_empty() {
                        self.invalid = true;
                    }
                    self.emit(&mut on_item);
                    self.after_comma = true;
                    continue;
                }
                b' ' | b'\n' | b'\r' | b'\t' if at_array_level && self.item.is_empty() => continue,
                b' ' | b'\n' | b'\r' | b'\t' => {}
                _ if self.phase == Phase::Seeking
                    && (matches!(self.at, ArrayAt::AnyFieldEnvelope(_))
                        || self.depth == seek_depth) =>
                {
                    self.armed = false
                }
                _ => {}
            }
            if self.phase == Phase::InArray {
                self.item.push(byte);
                self.after_comma = false;
            } else {
                self.keep(byte);
            }
        }
    }

    fn emit(&mut self, on_item: &mut impl FnMut(&str)) {
        if !self.item.is_empty() {
            if serde_json::from_slice::<IgnoredAny>(&self.item).is_err() {
                self.invalid = true;
            } else if let Ok(text) = std::str::from_utf8(&self.item) {
                on_item(text.trim());
            } else {
                self.invalid = true;
            }
            self.after_comma = false;
            self.item.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_envelopes_validate_items_and_preserve_suffixes() {
        let mut splitter = ArraySplitter::new(ArrayAt::Path(&["result", "data"]));
        let body = br#"{"unrelated":{"data":[1]},"result":{"data":[{"a":1},{"b":[2]}]},"code":0}"#;
        let mut items = Vec::new();
        for byte in body {
            splitter.feed(&[*byte], |item| items.push(item.to_owned()));
        }
        assert!(splitter.finished());
        assert_eq!(items, [r#"{"a":1}"#, r#"{"b":[2]}"#]);
        assert_eq!(
            std::str::from_utf8(splitter.outside()).unwrap(),
            r#"{"unrelated":{"data":[1]},"result":{"data":[]},"code":0}"#
        );
        for body in [
            b"{\"result\":{\"data\":[,]}}".as_slice(),
            b"{\"result\":{\"data\":[1,]}}",
            b"{\"result\":{\"data\":[\"\xff\"]}}",
        ] {
            let mut splitter = ArraySplitter::new(ArrayAt::Path(&["result", "data"]));
            splitter.feed(body, |_| {});
            assert!(!splitter.finished());
        }
    }

    #[test]
    fn malformed_scalars_and_truncated_envelopes_fail_closed() {
        for body in ["[1 2]", "[1 e2]"] {
            let mut splitter = ArraySplitter::new(ArrayAt::Root);
            splitter.feed(body.as_bytes(), |_| {});
            assert!(!splitter.finished());
        }
        let body = format!(
            "{{\"code\":0,\"result\":{{\"data\":[]}}}}{}garbage",
            " ".repeat(OUTSIDE_LIMIT)
        );
        for chunk in [1, 1024, OUTSIDE_LIMIT * 2] {
            let mut splitter = ArraySplitter::new(ArrayAt::Path(&["result", "data"]));
            for part in body.as_bytes().chunks(chunk) {
                splitter.feed(part, |_| {});
            }
            assert!(!splitter.finished());
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

    #[test]
    fn splits_a_nested_array_and_keeps_its_envelope() {
        let body = r#"{"retCode":0,"list":[9],"result":{"category":"linear","list":[{"s":"a"},{"s":"]"}],"nextPageCursor":"c%3D1"},"time":1}"#;
        for chunk in [1, 4, 1024] {
            let at = ArrayAt::Path(&["result", "list"]);
            assert_eq!(split(at, body, chunk), [r#"{"s":"a"}"#, r#"{"s":"]"}"#]);
            let mut splitter = ArraySplitter::new(at);
            for part in body.as_bytes().chunks(chunk) {
                splitter.feed(part, |_| {});
            }
            assert_eq!(splitter.envelope(), Some(splitter.outside()));
            assert_eq!(
                std::str::from_utf8(splitter.outside()).unwrap(),
                r#"{"retCode":0,"list":[9],"result":{"category":"linear","list":[],"nextPageCursor":"c%3D1"},"time":1}"#
            );
        }
        let mut splitter = ArraySplitter::new(ArrayAt::Path(&["result", "list"]));
        splitter.feed(
            br#"{"retCode":10001,"retMsg":"params error","result":{}}"#,
            |_| {},
        );
        assert!(!splitter.finished());
    }

    #[test]
    fn streams_validated_envelopes_across_chunks() {
        for body in [
            r#"{"code":0,"data":[{"a":"b"}],"msg":"ok"}"#,
            r#"{"data":[],"code":0}"#,
        ] {
            for chunk in 1..=body.len() {
                let mut s = ArraySplitter::new(ArrayAt::EnvelopeField("data"));
                for bytes in body.as_bytes().chunks(chunk) {
                    s.feed(bytes, |_| {});
                }
                assert!(s.finished());
                assert!(s.envelope().is_some());
            }
        }
        for body in [
            r#"{"code":0,"data":[,]}"#,
            r#"{"code":0,"data":[{},]}"#,
            r#"{"code":0,"data":[]"#,
        ] {
            let mut s = ArraySplitter::new(ArrayAt::EnvelopeField("data"));
            s.feed(body.as_bytes(), |_| {});
            assert!(s.envelope().is_none());
        }
        let mut s = ArraySplitter::new(ArrayAt::EnvelopeField("data"));
        s.feed(
            format!(r#"{{"msg":"{}","data":[]}}"#, "x".repeat(70_000)).as_bytes(),
            |_| {},
        );
        assert!(s.envelope().is_none());
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

#[cfg(test)]
mod envelope_tests {
    use super::*;
    #[test]
    fn merged_selectors_preserve_nested_metadata_and_validate_the_whole_envelope() {
        let body = br#"{"code":0,"data":{"result":{"list":[{"s":"a b"},{"s":"\u00e9"}],"cursor":"next"}},"after":true}"#;
        for at in [
            ArrayAt::Path(&["data", "result", "list"]),
            ArrayAt::AnyFieldEnvelope(&["data", "list"]),
        ] {
            for size in 1..=body.len() {
                let mut splitter = ArraySplitter::new(at);
                let mut items = Vec::new();
                for bytes in body.chunks(size) {
                    splitter.feed(bytes, |item| items.push(item.to_owned()));
                }
                assert_eq!(items, [r#"{"s":"a b"}"#, r#"{"s":"\u00e9"}"#]);
                assert_eq!(
                    splitter.envelope().unwrap(),
                    br#"{"code":0,"data":{"result":{"list":[],"cursor":"next"}},"after":true}"#,
                );
            }
        }
        for (at, body) in [
            (
                ArrayAt::EnvelopeField("data"),
                br#"{"data":[],"code":0"#.as_slice(),
            ),
            (
                ArrayAt::Path(&["result", "list"]),
                br#"{"result":{"list":[]},"retCode":0"#,
            ),
            (
                ArrayAt::AnyFieldEnvelope(&["list"]),
                br#"{"data":{"list":[]},"code":0"#,
            ),
        ] {
            let mut splitter = ArraySplitter::new(at);
            for bytes in body.chunks(1) {
                splitter.feed(bytes, |_| {});
            }
            assert!(splitter.envelope().is_none(), "truncated envelope: {at:?}");
        }
    }

    #[test]
    fn selectors_do_not_follow_unrelated_siblings_or_lose_primitive_whitespace() {
        for at in [
            ArrayAt::Path(&["data", "list"]),
            ArrayAt::AnyFieldEnvelope(&["list"]),
        ] {
            let body = br#"{"data":{"list":0,"other":[9]},"other":{"list":[8]}}"#;
            let mut splitter = ArraySplitter::new(at);
            let mut items = Vec::new();
            for bytes in body.chunks(1) {
                splitter.feed(bytes, |item| items.push(item.to_owned()));
            }
            if matches!(at, ArrayAt::Path(_)) {
                assert!(!splitter.finished());
                assert!(items.is_empty());
            } else {
                assert_eq!(items, ["8"]);
                assert!(splitter.envelope().is_some());
            }
        }
        for at in [
            ArrayAt::EnvelopeField("list"),
            ArrayAt::AnyFieldEnvelope(&["list"]),
        ] {
            let mut splitter = ArraySplitter::new(at);
            let mut items = Vec::new();
            splitter.feed(br#"{"list":[1 2]}"#, |item| items.push(item.to_owned()));
            assert!(items.is_empty());
            assert!(!splitter.finished());
            assert!(splitter.envelope().is_none());
        }
    }

    #[test]
    fn preserves_metadata_with_every_chunk_boundary() {
        let body = br#"{"code":"0","data":[{"s":"a"},{"s":"b"}],"msg":"ok"}"#;
        for size in 1..body.len() {
            let mut splitter = ArraySplitter::new(ArrayAt::EnvelopeField("data"));
            let mut items = Vec::new();
            for bytes in body.chunks(size) {
                splitter.feed(bytes, |s| items.push(s.to_owned()));
            }
            assert!(splitter.finished());
            assert_eq!(items, [r#"{"s":"a"}"#, r#"{"s":"b"}"#]);
            assert_eq!(
                splitter.envelope().unwrap(),
                br#"{"code":"0","data":[],"msg":"ok"}"#
            );
        }
    }
    #[test]
    fn legacy_modes_do_not_opt_into_response_metadata() {
        for (at, body) in [
            (ArrayAt::Root, b"[]".as_slice()),
            (ArrayAt::Field("data"), br#"{"data":[]}"#.as_slice()),
        ] {
            let mut splitter = ArraySplitter::new(at);
            splitter.feed(body, |_| {});
            splitter.feed(&[b' '; 16_384], |_| {});
            assert!(splitter.finished());
            assert!(splitter.envelope().is_none());
        }
    }

    #[test]
    fn nested_metadata_modes_keep_array_validation_and_overflow_guards() {
        for body in [
            br#"{"result":{"list":[, ]}}"#.as_slice(),
            br#"{"result":{"list":[1,]}}"#,
            b"{\"result\":{\"list\":[\xff]}}",
        ] {
            let mut splitter = ArraySplitter::new(ArrayAt::Path(&["result", "list"]));
            for byte in body.chunks(1) {
                splitter.feed(byte, |_| {});
            }
            assert!(!splitter.finished());
        }
        let mut splitter = ArraySplitter::new(ArrayAt::Path(&["result", "list"]));
        splitter.feed(br#"{"result":{"list":[]}}"#, |_| {});
        splitter.feed(&[b' '; 16_384], |_| {});
        assert!(!splitter.finished());
    }

    #[test]
    fn rejects_array_grammar_invalid_utf8_and_metadata_overflow() {
        for body in [b"[, ]".as_slice(), b"[1,]", b"[\xff]"] {
            let mut splitter = ArraySplitter::new(ArrayAt::Root);
            splitter.feed(body, |_| {});
            assert!(!splitter.finished());
        }
        let mut splitter = ArraySplitter::new(ArrayAt::EnvelopeField("data"));
        splitter.feed(br#"{"data":[]}"#, |_| {});
        splitter.feed(&[b' '; 16_384], |_| {});
        assert!(!splitter.finished());
    }
}
