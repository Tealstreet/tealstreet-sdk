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
    /// Like `Field`, explicitly requesting the surrounding response metadata.
    /// All modes retain metadata; this alias supports venues that opt in.
    EnvelopeField(&'static str),
    /// The array sits under these nested object fields (`result.list`).
    Path(&'static [&'static str]),
    /// Stream the first matching path, retaining metadata from any native
    /// response shape. An empty path selects a root array.
    Envelope(&'static [&'static [&'static str]]),
    /// Either `field: []` or `field: { nested: [] }`.
    EnvelopeFieldOrNested(&'static str, &'static str),
    /// A direct array or one of several accepted nested array names.
    EnvelopeFieldOrNestedAny(&'static str, &'static [&'static str]),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Seeking,
    InArray,
    Done,
}

/// The bytes outside the streamed array are kept up to this size, so an
/// envelope's other fields (a page cursor, an error) stay readable.
const METADATA_LIMIT: usize = 1024 * 1024;

/// Splits an array out of a body that arrives in chunks, handing over each
/// element's text as soon as it is complete. Only the element in progress is
/// buffered, so memory stays at the size of one element however large the
/// body is. Everything around the array is kept, with the array emptied.
pub struct ArraySplitter {
    at: ArrayAt,
    phase: Phase,
    depth: u32,
    array_depth: u32,
    in_string: bool,
    escaped: bool,
    /// The most recent string at the seeking depth.
    key: Vec<u8>,
    keys: Vec<Vec<u8>>,
    capturing_key: bool,
    /// The wanted key and its colon were just seen.
    armed: bool,
    item: Vec<u8>,
    // Keep metadata and separators, replacing streamed rows with null. This
    // validates the complete document without retaining the large array.
    envelope: Vec<u8>,
    // Metadata delivered to venues has the selected array emptied.
    outside: Vec<u8>,
    invalid: bool,
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
            keys: Vec::new(),
            capturing_key: false,
            armed: false,
            item: Vec::new(),
            envelope: Vec::new(),
            outside: Vec::new(),
            invalid: false,
        }
    }

    /// Whether the array was found and closed.
    pub fn finished(&self) -> bool {
        self.phase == Phase::Done
            && !self.invalid
            && std::str::from_utf8(&self.envelope).is_ok()
            && serde_json::from_slice::<IgnoredAny>(&self.envelope).is_ok()
    }

    /// Response metadata with streamed rows replaced by null.
    /// Callers must check [`Self::finished`] before treating it as valid JSON.
    pub fn envelope(&self) -> &str {
        std::str::from_utf8(&self.envelope).unwrap_or("")
    }

    /// Response metadata with the selected array emptied (`"list":[]`).
    /// Callers must check [`Self::finished`] before treating it as valid JSON.
    pub fn outside(&self) -> &[u8] {
        &self.outside
    }

    fn keep(&mut self, byte: u8) {
        if self.outside.len() >= METADATA_LIMIT || self.envelope.len() >= METADATA_LIMIT {
            self.invalid = true;
            return;
        }
        self.outside.push(byte);
        self.envelope.push(byte);
    }

    pub fn feed(&mut self, data: &[u8], mut on_item: impl FnMut(&str)) {
        for &byte in data {
            if self.invalid {
                return;
            }
            if self.phase != Phase::InArray {
                self.keep(byte);
                if self.invalid {
                    return;
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
                        if let Some(key) = self.keys.get_mut(self.depth.saturating_sub(1) as usize)
                        {
                            key.clone_from(&self.key);
                        }
                    }
                }
                continue;
            }
            let at_array_level = self.phase == Phase::InArray && self.depth == self.array_depth;
            match byte {
                b'"' => {
                    self.in_string = true;
                    if self.phase == Phase::Seeking && self.depth > 0 {
                        self.key.clear();
                        self.capturing_key = true;
                        self.armed = false;
                    }
                }
                b':' if self.phase == Phase::Seeking => {
                    let matches = |path: &[&str]| {
                        path.len() == self.depth as usize
                            && path
                                .iter()
                                .zip(&self.keys)
                                .all(|(name, key)| name.as_bytes() == key)
                    };
                    self.armed = match self.at {
                        ArrayAt::Root => false,
                        ArrayAt::Field(name) | ArrayAt::EnvelopeField(name) => {
                            self.depth == 1 && name.as_bytes() == self.key
                        }
                        ArrayAt::Path(path) => matches(path),
                        ArrayAt::Envelope(paths) => paths.iter().any(|path| matches(path)),
                        ArrayAt::EnvelopeFieldOrNested(field, nested) => {
                            matches(&[field]) || matches(&[field, nested])
                        }
                        ArrayAt::EnvelopeFieldOrNestedAny(field, nested) => {
                            matches(&[field]) || nested.iter().any(|name| matches(&[field, name]))
                        }
                    };
                    continue;
                }
                b'[' | b'{' => {
                    self.depth += 1;
                    self.keys.resize_with(self.depth as usize, Vec::new);
                    if self.phase == Phase::Seeking {
                        let found = byte == b'['
                            && match self.at {
                                ArrayAt::Root => self.depth == 1,
                                ArrayAt::Field(_) | ArrayAt::EnvelopeField(_) => {
                                    self.armed && self.depth == 2
                                }
                                ArrayAt::Path(path) => {
                                    self.armed || (path.is_empty() && self.depth == 1)
                                }
                                ArrayAt::Envelope(paths) => {
                                    self.armed
                                        || (self.depth == 1 && paths.iter().any(|p| p.is_empty()))
                                }
                                ArrayAt::EnvelopeFieldOrNested(..)
                                | ArrayAt::EnvelopeFieldOrNestedAny(..) => self.armed,
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
                        self.depth = self.depth.saturating_sub(1);
                        self.keep(byte);
                        continue;
                    }
                    self.depth = self.depth.saturating_sub(1);
                }
                b',' if at_array_level => {
                    self.emit(&mut on_item);
                    // Collapse completed rows; preserve malformed separators.
                    if self.envelope.ends_with(b"null,null") {
                        self.envelope.truncate(self.envelope.len() - 5);
                    }
                    if self.envelope.len() >= METADATA_LIMIT {
                        self.invalid = true;
                        return;
                    }
                    self.envelope.push(b',');
                    continue;
                }
                b' ' | b'\n' | b'\r' | b'\t' if at_array_level && self.item.is_empty() => continue,
                b' ' | b'\n' | b'\r' | b'\t' => {}
                _ if self.phase == Phase::Seeking => self.armed = false,
                _ => {}
            }
            if self.phase == Phase::InArray {
                self.item.push(byte);
            }
        }
    }

    fn emit(&mut self, on_item: &mut impl FnMut(&str)) {
        if !self.item.is_empty() {
            match std::str::from_utf8(&self.item) {
                Ok(text) if serde_json::from_str::<IgnoredAny>(text).is_ok() => {
                    on_item(text.trim())
                }
                _ => self.invalid = true,
            }
            if self.envelope.len() > METADATA_LIMIT - 4 {
                self.invalid = true;
            } else {
                self.envelope.extend_from_slice(b"null");
            }
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
            " ".repeat(METADATA_LIMIT)
        );
        for chunk in [1, 1024, METADATA_LIMIT * 2] {
            let mut splitter = ArraySplitter::new(ArrayAt::Path(&["result", "data"]));
            for part in body.as_bytes().chunks(chunk) {
                splitter.feed(part, |_| {});
            }
            assert!(!splitter.finished());
        }
    }

    #[test]
    fn streamed_metadata_retains_paging_flags_without_rows() {
        let mut splitter = ArraySplitter::new(ArrayAt::Field("items"));
        splitter.feed(
            br#"{"items":[{"id":1},{"id":2}],"hasNextPage":true,"nextKey":{"id":"3"}}"#,
            |_| {},
        );
        assert!(splitter.finished());
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Metadata {
            has_next_page: bool,
            next_key: Option<Key>,
        }
        #[derive(Deserialize)]
        struct Key {
            id: String,
        }
        let metadata: Metadata = serde_json::from_str(splitter.envelope()).unwrap();
        assert!(metadata.has_next_page);
        assert_eq!(metadata.next_key.unwrap().id, "3");
        assert!(!splitter.envelope().contains("\"id\":1"));
    }

    #[test]
    fn rejects_invalid_documents_after_streaming_rows() {
        for text in [
            r#"{"symbols":[{"v":1}]"#,
            r#"{"symbols":[{"v":1},,]}"#,
            r#"{"symbols":[{"v":1}]}x"#,
            r#"[1,]"#,
            r#"[1,,2]"#,
            r#"[1 2]"#,
            r#"[tru e]"#,
        ] {
            let at = if text.starts_with('{') {
                ArrayAt::Field("symbols")
            } else {
                ArrayAt::Root
            };
            let mut splitter = ArraySplitter::new(at);
            for byte in text.bytes() {
                splitter.feed(&[byte], |_| {});
            }
            assert!(!splitter.finished(), "{text}");
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
    fn nested_streams_validate_the_complete_document_and_each_item() {
        for body in [
            r#"{"result":{"list":[1 2]}}"#,
            r#"{"result":{"list":[1,]}}"#,
            r#"{"result":{"list":[1,,2]}}"#,
            r#"{"result":{"list":[tru e]}}"#,
            r#"{"result":{"list":[{"x":1}]}}x"#,
            r#"{"result":{"list":[{"x":1}]}"#,
            r#"{"result":{"list":[{"x":1}]},}"#,
        ] {
            let mut splitter = ArraySplitter::new(ArrayAt::Path(&["result", "list"]));
            for byte in body.bytes() {
                splitter.feed(&[byte], |_| {});
            }
            assert!(!splitter.finished(), "{body}");
        }
        let body =
            r#"{"unrelated":{"list":[0]},"result":{"nested":{"list":[1]},"list":[2]},"list":[3]}"#;
        assert_eq!(split(ArrayAt::Path(&["result", "list"]), body, 1), ["2"]);
    }

    #[test]
    fn keeps_large_metadata_but_rejects_overflow_without_truncation() {
        let body = format!(
            r#"{{"result":{{"list":[1,2],"nextPageCursor":"{}"}}}}"#,
            "x".repeat(100 * 1024)
        );
        let mut splitter = ArraySplitter::new(ArrayAt::Path(&["result", "list"]));
        for part in body.as_bytes().chunks(37) {
            splitter.feed(part, |_| {});
        }
        assert!(splitter.finished());
        assert_eq!(
            std::str::from_utf8(splitter.outside()).unwrap(),
            body.replace("[1,2]", "[]")
        );
        let oversized = format!(
            r#"{{"items":[],"metadata":"{}"}}"#,
            "x".repeat(METADATA_LIMIT)
        );
        let mut splitter = ArraySplitter::new(ArrayAt::Field("items"));
        splitter.feed(oversized.as_bytes(), |_| {});
        assert!(!splitter.finished());
        assert!(splitter.outside.len() <= METADATA_LIMIT);
        assert!(splitter.envelope.len() <= METADATA_LIMIT);
        // Later chunks must not resume an invalid stream or exceed the cap.
        splitter.feed(br#""}"#, |_| panic!("invalid stream resumed"));
        assert!(!splitter.finished());
        assert!(splitter.outside.len() <= METADATA_LIMIT);
        assert!(splitter.envelope.len() <= METADATA_LIMIT);
    }

    #[test]
    fn row_count_does_not_grow_either_metadata_buffer() {
        let mut splitter = ArraySplitter::new(ArrayAt::Root);
        splitter.feed(b"[", |_| {});
        for _ in 0..1_000 {
            splitter.feed(b"1,", |_| {});
        }
        splitter.feed(b"2]", |_| {});
        assert!(splitter.finished());
        assert_eq!(splitter.outside(), b"[]");
        assert!(splitter.envelope().len() < 32);
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
                std::str::from_utf8(splitter.outside()).unwrap(),
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
                let envelope = std::str::from_utf8(splitter.outside()).unwrap();
                assert_eq!(envelope, text.replace(r#"{"n":1}"#, ""));
            }
        }
    }

    #[test]
    fn rejects_an_oversized_stream_envelope() {
        let mut splitter = ArraySplitter::new(ArrayAt::EnvelopeField("data"));
        splitter.feed(
            format!(
                r#"{{"code":0,"data":[]}}{}garbage"#,
                " ".repeat(METADATA_LIMIT)
            )
            .as_bytes(),
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
                splitter.outside(),
                br#"{"code":0,"data":[],"msg":"ok"}"#.as_slice()
            );
        }
    }

    #[test]
    fn path_matching_ignores_sibling_and_wrong_depth_lists() {
        let body = r#"{"result":{"wrong":{"list":[9]},"data":{"list":[1]}},"data":{"list":[8]}}"#;
        for width in [1, 3, 1024] {
            let mut splitter = ArraySplitter::new(ArrayAt::Path(&["result", "data", "list"]));
            let mut rows = Vec::new();
            for chunk in body.as_bytes().chunks(width) {
                splitter.feed(chunk, |row| rows.push(row.to_owned()));
            }
            assert!(splitter.finished());
            assert_eq!(rows, ["1"]);
            assert_eq!(
                std::str::from_utf8(splitter.outside()).unwrap(),
                body.replace("[1]", "[]")
            );
        }
    }

    #[test]
    fn empty_nested_list_keeps_its_pagination_cursor() {
        let body = br#"{"retCode":0,"result":{"list":[],"nextPageCursor":"next"}}"#;
        let mut splitter = ArraySplitter::new(ArrayAt::Path(&["result", "list"]));
        for byte in body.chunks(1) {
            splitter.feed(byte, |_| panic!("empty list"));
        }
        assert!(splitter.finished());
        assert_eq!(splitter.outside(), body);
    }

    #[test]
    fn metadata_selectors_reject_truncation_instead_of_accepting_a_valid_prefix() {
        for (at, body) in [
            (
                ArrayAt::Path(&["result", "list"]),
                r#"{"result":{"list":[1]}}"#,
            ),
            (ArrayAt::EnvelopeField("data"), r#"{"data":[1]}"#),
            (
                ArrayAt::EnvelopeFieldOrNestedAny("data", &["fill_orders", "fill_history_orders"]),
                r#"{"data":{"fill_orders":[1]}}"#,
            ),
        ] {
            let text = format!("{body}{}garbage", " ".repeat(METADATA_LIMIT));
            let mut splitter = ArraySplitter::new(at);
            splitter.feed(text.as_bytes(), |_| {});
            assert!(!splitter.finished(), "{at:?}");
            assert!(splitter.outside().len() <= METADATA_LIMIT);
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

#[cfg(test)]
mod envelope_tests {
    use super::*;
    #[test]
    fn alternative_paths_stream_native_shapes_and_keep_metadata() {
        for body in [
            r#"{"code":"00000","data":{"entrustedList":[{"id":"a"},{"id":"b"}],"endId":"b"},"msg":"ok"}"#,
            r#"{"code":"00000","data":[{"id":"a"},{"id":"b"}],"msg":"ok"}"#,
        ] {
            for size in [1, 2, 9, 1024] {
                let mut splitter =
                    ArraySplitter::new(ArrayAt::Envelope(&[&["data", "entrustedList"], &["data"]]));
                let mut items = vec![];
                for chunk in body.as_bytes().chunks(size) {
                    splitter.feed(chunk, |item| items.push(item.to_owned()));
                }
                assert!(splitter.finished());
                assert_eq!(items, [r#"{"id":"a"}"#, r#"{"id":"b"}"#]);
                assert_eq!(
                    std::str::from_utf8(splitter.outside()).unwrap(),
                    body.replace(r#"[{"id":"a"},{"id":"b"}]"#, "[]")
                );
                assert!(serde_json::from_str::<IgnoredAny>(splitter.envelope()).is_ok());
            }
        }
        let mut root = ArraySplitter::new(ArrayAt::Envelope(&[&["data"], &[]]));
        let mut items = Vec::new();
        root.feed(b"[1,2]", |item| items.push(item.to_owned()));
        assert!(root.finished());
        assert_eq!(items, ["1", "2"]);
        assert_eq!(root.outside(), b"[]");
    }

    #[test]
    fn alternative_paths_require_complete_valid_documents_and_exact_paths() {
        for body in [
            r#"{"data":{"list":[]}"#,
            r#"{"else":{"list":[]}}"#,
            r#"{"data":{"list":[]}garbage}"#,
            r#"{"data":{"list":[1,]}}"#,
            r#"{"data":{"list":[1 2]}}"#,
        ] {
            let mut splitter = ArraySplitter::new(ArrayAt::Envelope(&[&["data", "list"]]));
            for chunk in body.as_bytes().chunks(1) {
                splitter.feed(chunk, |_| {});
            }
            assert!(!splitter.finished(), "{body}");
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
            assert_eq!(splitter.outside(), br#"{"code":"0","data":[],"msg":"ok"}"#);
        }
    }
    #[test]
    fn root_and_field_modes_preserve_bounded_response_metadata() {
        for (at, body) in [
            (ArrayAt::Root, b"[]".as_slice()),
            (ArrayAt::Field("data"), br#"{"data":[]}"#.as_slice()),
        ] {
            let mut splitter = ArraySplitter::new(at);
            splitter.feed(body, |_| {});
            splitter.feed(&[b' '; 16_384], |_| {});
            assert!(splitter.finished());
            assert!(splitter.outside().starts_with(body));
            assert!(serde_json::from_slice::<IgnoredAny>(splitter.outside()).is_ok());
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
        splitter.feed(&vec![b' '; METADATA_LIMIT], |_| {});
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
        splitter.feed(&vec![b' '; METADATA_LIMIT], |_| {});
        assert!(!splitter.finished());
    }
}
