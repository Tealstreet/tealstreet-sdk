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
    /// Like `Path`, validating but omitting every other array in the envelope.
    /// Use when product catalogs carry large unrelated lists beside the target.
    PathOmitLists(&'static [&'static str]),
    /// Stream the first matching path, retaining metadata from any native
    /// response shape. An empty path selects a root array.
    Envelope(&'static [&'static [&'static str]]),
    /// Either `field: []` or `field: { nested: [] }`.
    EnvelopeFieldOrNested(&'static str, &'static str),
    /// A direct array or one of several accepted nested array names.
    EnvelopeFieldOrNestedAny(&'static str, &'static [&'static str]),
    /// First matching array field at any depth in a versioned envelope.
    #[cfg(feature = "any-field-envelope")]
    AnyFieldEnvelope(&'static [&'static str]),
}

/// Who rejects a malformed array element.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemCheck {
    /// The splitter checks each element's JSON syntax before handing it over.
    Splitter,
    /// The venue deserializes every element with serde_json and fails the
    /// request on any error, so a second syntax pass would only repeat it.
    Venue,
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
    /// Seeking containers: named path segments must all belong to objects.
    containers: Vec<u8>,
    capturing_key: bool,
    /// The wanted key and its colon were just seen.
    armed: bool,
    item: Vec<u8>,
    // Keep metadata and separators, replacing streamed rows with null. This
    // validates the complete document without retaining the large array.
    envelope: Vec<u8>,
    // Metadata delivered to venues has the selected array emptied.
    outside: Vec<u8>,
    // Only the opt-in catalog mode creates this validator for an unrelated
    // list. Its Root mode keeps at most one element and a bounded skeleton.
    ignored: Option<Box<ArraySplitter>>,
    item_check: ItemCheck,
    invalid: bool,
}

impl ArraySplitter {
    pub fn new(at: ArrayAt) -> Self {
        Self::with_item_check(at, ItemCheck::Splitter)
    }

    pub fn with_item_check(at: ArrayAt, item_check: ItemCheck) -> Self {
        Self {
            at,
            phase: Phase::Seeking,
            depth: 0,
            array_depth: 0,
            in_string: false,
            escaped: false,
            key: Vec::new(),
            keys: Vec::new(),
            containers: Vec::new(),
            capturing_key: false,
            armed: false,
            item: Vec::new(),
            envelope: Vec::new(),
            outside: Vec::new(),
            ignored: None,
            item_check,
            invalid: false,
        }
    }

    /// Whether the array was found and closed.
    pub fn finished(&self) -> bool {
        self.phase == Phase::Done
            && self.ignored.is_none()
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

    pub fn feed(&mut self, mut data: &[u8], mut on_item: impl FnMut(&str)) {
        loop {
            if self.invalid {
                return;
            }
            if self.phase == Phase::InArray && self.ignored.is_none() {
                data = self.feed_array(data, &mut on_item);
                if self.invalid {
                    return;
                }
            }
            let Some((&byte, tail)) = data.split_first() else {
                return;
            };
            data = tail;
            if let Some(ignored) = self.ignored.as_mut() {
                // A function pointer keeps this recursive generic call from
                // generating a new callback type at each instantiation.
                ignored.feed(std::slice::from_ref(&byte), ignore_item as fn(&str));
                if ignored.invalid {
                    self.invalid = true;
                    return;
                }
                if ignored.phase == Phase::Done {
                    if !ignored.finished() {
                        self.invalid = true;
                        return;
                    }
                    self.ignored = None;
                    if self.phase == Phase::Seeking {
                        self.containers.pop();
                    }
                    self.depth = self.depth.saturating_sub(1);
                    self.keep(byte);
                }
                continue;
            }
            // Array elements are scanned by `feed_array`; this loop only sees
            // the envelope around the array.
            self.keep(byte);
            if self.invalid {
                return;
            }
            if self.phase == Phase::Done && !matches!(self.at, ArrayAt::PathOmitLists(_)) {
                continue;
            }
            if self.in_string {
                if self.capturing_key {
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
                    self.armed = self.containers.iter().all(|&kind| kind == b'{')
                        && match self.at {
                            ArrayAt::Root => false,
                            ArrayAt::Field(name) | ArrayAt::EnvelopeField(name) => {
                                self.depth == 1 && name.as_bytes() == self.key
                            }
                            ArrayAt::Path(path) | ArrayAt::PathOmitLists(path) => matches(path),
                            ArrayAt::Envelope(paths) => paths.iter().any(|path| matches(path)),
                            ArrayAt::EnvelopeFieldOrNested(field, nested) => {
                                matches(&[field]) || matches(&[field, nested])
                            }
                            ArrayAt::EnvelopeFieldOrNestedAny(field, nested) => {
                                matches(&[field])
                                    || nested.iter().any(|name| matches(&[field, name]))
                            }
                            #[cfg(feature = "any-field-envelope")]
                            ArrayAt::AnyFieldEnvelope(_) => false,
                        };
                    // Versioned envelopes may wrap the target in arrays.
                    #[cfg(feature = "any-field-envelope")]
                    if let ArrayAt::AnyFieldEnvelope(names) = self.at {
                        self.armed = self.containers.last() == Some(&b'{')
                            && names.iter().any(|name| name.as_bytes() == self.key);
                    }
                    continue;
                }
                b'[' | b'{' => {
                    self.depth += 1;
                    self.keys.resize_with(self.depth as usize, Vec::new);
                    if self.phase == Phase::Seeking {
                        self.containers.push(byte);
                        let found = byte == b'['
                            && match self.at {
                                ArrayAt::Root => self.depth == 1,
                                ArrayAt::Field(_) | ArrayAt::EnvelopeField(_) => {
                                    self.armed && self.depth == 2
                                }
                                ArrayAt::Path(path) | ArrayAt::PathOmitLists(path) => {
                                    self.armed || (path.is_empty() && self.depth == 1)
                                }
                                ArrayAt::Envelope(paths) => {
                                    self.armed
                                        || (self.depth == 1 && paths.iter().any(|p| p.is_empty()))
                                }
                                ArrayAt::EnvelopeFieldOrNested(..)
                                | ArrayAt::EnvelopeFieldOrNestedAny(..) => self.armed,
                                #[cfg(feature = "any-field-envelope")]
                                ArrayAt::AnyFieldEnvelope(_) => self.armed,
                            };
                        self.armed = false;
                        if found {
                            self.phase = Phase::InArray;
                            self.array_depth = self.depth;
                            continue;
                        }
                    }
                    if byte == b'[' && matches!(self.at, ArrayAt::PathOmitLists(_)) {
                        let mut ignored = Box::new(Self::new(ArrayAt::Root));
                        ignored.feed(b"[", ignore_item as fn(&str));
                        self.ignored = Some(ignored);
                        continue;
                    }
                }
                b']' | b'}' => {
                    if self.phase == Phase::Seeking {
                        self.containers.pop();
                    }
                    self.depth = self.depth.saturating_sub(1);
                }
                b' ' | b'\n' | b'\r' | b'\t' => {}
                _ if self.phase == Phase::Seeking => self.armed = false,
                _ => {}
            }
        }
    }

    /// Scans array elements, copying runs of ordinary bytes at once and
    /// stopping only at bytes that change string, nesting or element state.
    /// Returns the input after the array's closing bracket, if it closed.
    fn feed_array<'d>(&mut self, mut data: &'d [u8], on_item: &mut impl FnMut(&str)) -> &'d [u8] {
        while !data.is_empty() {
            if self.in_string {
                if self.escaped {
                    self.escaped = false;
                    self.item.push(data[0]);
                    data = &data[1..];
                    continue;
                }
                let len = data
                    .iter()
                    .position(|&byte| byte == b'\\' || byte == b'"')
                    .unwrap_or(data.len());
                self.item.extend_from_slice(&data[..len]);
                data = &data[len..];
                if let Some((&byte, tail)) = data.split_first() {
                    self.item.push(byte);
                    data = tail;
                    if byte == b'\\' {
                        self.escaped = true;
                    } else {
                        self.in_string = false;
                    }
                }
                continue;
            }
            let at_array_level = self.depth == self.array_depth;
            let len = data
                .iter()
                .position(|&byte| {
                    matches!(byte, b'"' | b'[' | b'{' | b']' | b'}')
                        || (at_array_level && matches!(byte, b',' | b' ' | b'\n' | b'\r' | b'\t'))
                })
                .unwrap_or(data.len());
            self.item.extend_from_slice(&data[..len]);
            data = &data[len..];
            let Some((&byte, tail)) = data.split_first() else {
                break;
            };
            data = tail;
            match byte {
                b'"' => self.in_string = true,
                b'[' | b'{' => {
                    self.depth += 1;
                    self.keys.resize_with(self.depth as usize, Vec::new);
                }
                b']' if at_array_level => {
                    self.emit(on_item);
                    self.phase = Phase::Done;
                    self.depth = self.depth.saturating_sub(1);
                    self.keep(byte);
                    return data;
                }
                b']' | b'}' => self.depth = self.depth.saturating_sub(1),
                b',' if at_array_level => {
                    self.emit(on_item);
                    // Collapse completed rows; preserve malformed separators.
                    if self.envelope.ends_with(b"null,null") {
                        self.envelope.truncate(self.envelope.len() - 5);
                    }
                    if self.invalid || self.envelope.len() >= METADATA_LIMIT {
                        self.invalid = true;
                        return &[];
                    }
                    self.envelope.push(b',');
                    continue;
                }
                b' ' | b'\n' | b'\r' | b'\t' if self.item.is_empty() => continue,
                _ => {}
            }
            self.item.push(byte);
        }
        data
    }

    fn emit(&mut self, on_item: &mut impl FnMut(&str)) {
        if !self.item.is_empty() {
            match std::str::from_utf8(&self.item) {
                Ok(text)
                    if self.item_check == ItemCheck::Venue
                        || serde_json::from_str::<IgnoredAny>(text).is_ok() =>
                {
                    // JSON whitespace only: Unicode trim would accept a row
                    // that the venue-checked mode never syntax-checks whole.
                    on_item(text.trim_matches([' ', '\n', '\r', '\t']))
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

fn ignore_item(_item: &str) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_spans_preserve_rows_and_metadata_at_every_chunk_boundary() {
        let row = format!(
            r#"{{"long":"{}","utf8":"{}","escaped":"\\\"\u263a\uD83D\uDE00\n\t","nested":["[}},]",{{"k":"v"}}]}}"#,
            "ordinary ascii ".repeat(64),
            "é中😀".repeat(32),
        );
        assert!(serde_json::from_str::<IgnoredAny>(&row).is_ok());
        for (at, prefix, suffix) in [
            (ArrayAt::Root, "[", "]"),
            (ArrayAt::Field("data"), r#"{"data":["#, r#"],"tail":"ok"}"#),
            (
                ArrayAt::EnvelopeField("data"),
                r#"{"data":["#,
                r#"],"tail":"ok"}"#,
            ),
            (
                ArrayAt::Path(&["result", "data"]),
                r#"{"result":{"data":["#,
                r#"]},"tail":"ok"}"#,
            ),
            (
                ArrayAt::PathOmitLists(&["result", "data"]),
                r#"{"skip":["ignored"],"result":{"data":["#,
                r#"]},"after":["ignored"]}"#,
            ),
            (ArrayAt::Envelope(&[&["data"]]), r#"{"data":["#, "]}"),
            (
                ArrayAt::EnvelopeFieldOrNested("result", "data"),
                r#"{"result":{"data":["#,
                "]}}",
            ),
            (
                ArrayAt::EnvelopeFieldOrNestedAny("result", &["data", "other"]),
                r#"{"result":{"other":["#,
                "]}}",
            ),
        ] {
            let body = format!("{prefix}{row}{suffix}");
            let expected_outside = if matches!(at, ArrayAt::PathOmitLists(_)) {
                r#"{"skip":[],"result":{"data":[]},"after":[]}"#.to_owned()
            } else {
                format!("{prefix}{suffix}")
            };
            for boundary in 0..=body.len() {
                let mut splitter = ArraySplitter::new(at);
                let mut rows = Vec::new();
                for bytes in [&body.as_bytes()[..boundary], &body.as_bytes()[boundary..]] {
                    splitter.feed(bytes, |item| rows.push(item.to_owned()));
                }
                assert!(splitter.finished(), "{at:?} at byte {boundary}");
                assert_eq!(rows, [row.as_str()], "{at:?} at byte {boundary}");
                assert_eq!(splitter.outside(), expected_outside.as_bytes());
            }
        }
    }

    #[test]
    fn string_spans_keep_malformed_items_invalid_at_every_chunk_boundary() {
        for row in [
            b"\"ordinary text\x00tail\"".as_slice(),
            b"\"ordinary text\ntail\"",
            b"\"ordinary text\xfftail\"",
            br#""ordinary text\qtail""#,
            br#""ordinary text\u12x4tail""#,
            br#""ordinary text\""#,
            br#"{"key":"ordinary text",}"#,
            br#""ordinary text" "second value""#,
        ] {
            assert!(
                std::str::from_utf8(row)
                    .ok()
                    .is_none_or(|text| serde_json::from_str::<IgnoredAny>(text).is_err()),
                "{row:?}"
            );
            let mut body = b"[".to_vec();
            body.extend_from_slice(row);
            body.extend_from_slice(b"]");
            for boundary in 0..=body.len() {
                let mut splitter = ArraySplitter::new(ArrayAt::Root);
                for bytes in [&body[..boundary], &body[boundary..]] {
                    splitter.feed(bytes, |_| panic!("malformed row emitted"));
                }
                assert!(!splitter.finished(), "{row:?} at byte {boundary}");
            }
        }
    }

    #[test]
    fn element_scanner_frames_nesting_whitespace_and_escapes_at_every_chunk_boundary() {
        let body = concat!(
            "{\"data\": [ {\"a\":[1, {\"b\":\"]},\\\"x\"}], \"c\" : \"\\\\\"} ,\n",
            "\t\"s\\\\\" , [ ] ,12 ,true,{\"d\":{\"e\":[[]]}} ],\"tail\":[1]}"
        );
        let expected = [
            r#"{"a":[1, {"b":"]},\"x"}], "c" : "\\"}"#,
            r#""s\\""#,
            "[ ]",
            "12",
            "true",
            r#"{"d":{"e":[[]]}}"#,
        ];
        for check in [ItemCheck::Splitter, ItemCheck::Venue] {
            for boundary in 0..=body.len() {
                for size in [1, 3, body.len()] {
                    let mut splitter =
                        ArraySplitter::with_item_check(ArrayAt::Field("data"), check);
                    let mut rows = Vec::new();
                    for part in [&body.as_bytes()[..boundary], &body.as_bytes()[boundary..]] {
                        for bytes in part.chunks(size) {
                            splitter.feed(bytes, |item| rows.push(item.to_owned()));
                        }
                    }
                    assert!(splitter.finished(), "{check:?} {boundary}/{size}");
                    assert_eq!(rows, expected, "{check:?} {boundary}/{size}");
                    assert_eq!(splitter.outside(), br#"{"data": [],"tail":[1]}"#);
                }
            }
        }
    }

    #[test]
    fn venue_checked_items_skip_only_the_syntax_pass() {
        let body = b"[{\"a\":1,},\"ok\",{\"b\" 2}]";
        let mut splitter = ArraySplitter::with_item_check(ArrayAt::Root, ItemCheck::Venue);
        let mut rows = Vec::new();
        splitter.feed(body, |item| rows.push(item.to_owned()));
        assert!(splitter.finished());
        assert_eq!(rows, [r#"{"a":1,}"#, r#""ok""#, r#"{"b" 2}"#]);
        // UTF-8, framing and the envelope are still the splitter's to reject.
        for body in [b"[\"\xff\",1]".as_slice(), b"[1,2", b"[1,2]]"] {
            let mut splitter = ArraySplitter::with_item_check(ArrayAt::Root, ItemCheck::Venue);
            let mut rows = 0;
            splitter.feed(body, |_| rows += 1);
            assert!(!splitter.finished(), "{body:?}");
        }
        let mut splitter = ArraySplitter::new(ArrayAt::Root);
        let mut rows = 0;
        splitter.feed(body, |_| rows += 1);
        assert_eq!(rows, 0);
        assert!(!splitter.finished());
        // Non-JSON whitespace around a row stays in the text the venue parses.
        for row in [
            "{\"a\":1}\u{c}",
            "\u{a0}{\"a\":1}",
            "{\"a\":1}\u{2028}",
            "\u{b}1",
        ] {
            for check in [ItemCheck::Splitter, ItemCheck::Venue] {
                let mut splitter = ArraySplitter::with_item_check(ArrayAt::Root, check);
                let mut rows = Vec::new();
                splitter.feed(format!("[{row}]").as_bytes(), |item| {
                    rows.push(item.to_owned())
                });
                assert!(
                    rows.iter()
                        .all(|item| serde_json::from_str::<IgnoredAny>(item).is_err()),
                    "{check:?} {row:?} {rows:?}"
                );
            }
        }
    }

    #[test]
    fn string_spans_do_not_relax_the_metadata_limit() {
        let row = format!("\"{}\"", "é".repeat(METADATA_LIMIT));
        // A streamed row is not metadata; its size cannot consume that budget.
        let prefix = format!(r#"{{"data":[{row}],"metadata":""#);
        let suffix = "\"}";
        let metadata_len = METADATA_LIMIT - br#"{"data":[null],"metadata":""#.len() - suffix.len();
        for extra in [0, 1] {
            let mut splitter = ArraySplitter::new(ArrayAt::Field("data"));
            let mut rows = 0;
            splitter.feed(prefix.as_bytes(), |_| rows += 1);
            splitter.feed(&vec![b'x'; metadata_len + extra], |_| {
                panic!("metadata emitted")
            });
            splitter.feed(suffix.as_bytes(), |_| panic!("metadata emitted"));
            assert_eq!(rows, 1);
            assert_eq!(splitter.finished(), extra == 0);
            assert!(splitter.envelope.len() <= METADATA_LIMIT);
            assert!(splitter.outside.len() <= METADATA_LIMIT);
        }
    }

    #[test]
    fn catalog_mode_omits_only_unrelated_arrays_and_retains_objects_scalars() {
        let body = r#"{"currencies":[{"nested":[1,2],"name":"[x]"}],"data":{"before":[[3]],"catalog":[{"symbol":"a","risk":[4]}],"after":[{"v":5}],"cursor":{"page":"next"}},"tail":[6],"msg":"[]","code":0}"#;
        for size in [1, 2, 7, 1024] {
            let mut splitter = ArraySplitter::new(ArrayAt::PathOmitLists(&["data", "catalog"]));
            let mut rows = Vec::new();
            for bytes in body.as_bytes().chunks(size) {
                splitter.feed(bytes, |row| rows.push(row.to_owned()));
            }
            assert!(splitter.finished(), "chunk {size}");
            assert_eq!(rows, [r#"{"symbol":"a","risk":[4]}"#]);
            assert_eq!(
                std::str::from_utf8(splitter.outside()).unwrap(),
                r#"{"currencies":[],"data":{"before":[],"catalog":[],"after":[],"cursor":{"page":"next"}},"tail":[],"msg":"[]","code":0}"#
            );
            assert!(serde_json::from_str::<IgnoredAny>(splitter.envelope()).is_ok());
        }
        // Existing modes retain those lists exactly as before.
        let mut splitter = ArraySplitter::new(ArrayAt::Path(&["data", "catalog"]));
        splitter.feed(body.as_bytes(), |_| {});
        assert!(splitter.finished());
        assert!(
            std::str::from_utf8(splitter.outside())
                .unwrap()
                .contains(r#""tail":[6]"#)
        );
    }

    #[test]
    fn catalog_mode_fails_closed_for_invalid_omitted_arrays_and_envelopes() {
        for array in [
            b"[,1]".as_slice(),
            b"[1,]",
            b"[1 2]",
            b"[tru e]",
            b"[\"bad\\q\"]",
            b"[\"\xff\"]",
            b"[{\"x\":1,}]",
        ] {
            for before in [false, true] {
                let mut body = if before {
                    b"{\"ignored\":".to_vec()
                } else {
                    b"{\"data\":{\"catalog\":[1]},\"ignored\":".to_vec()
                };
                body.extend_from_slice(array);
                body.extend_from_slice(if before {
                    b",\"data\":{\"catalog\":[1]}}"
                } else {
                    b"}"
                });
                let mut splitter = ArraySplitter::new(ArrayAt::PathOmitLists(&["data", "catalog"]));
                for byte in body {
                    splitter.feed(&[byte], |_| {});
                }
                assert!(!splitter.finished());
            }
        }
        for body in [
            r#"{"data":{"catalog":[1]},"ignored":[1"#,
            r#"{"data":{"catalog":[1]},"ignored":[],}"#,
            r#"{"data":{"catalog":[1]},"ignored":[]}garbage"#,
            r#"{"ignored":[{"data":{"catalog":[1]}}]}"#,
        ] {
            let mut splitter = ArraySplitter::new(ArrayAt::PathOmitLists(&["data", "catalog"]));
            splitter.feed(body.as_bytes(), |_| {});
            assert!(!splitter.finished(), "{body}");
        }
    }

    #[test]
    fn catalog_mode_keeps_large_lists_bounded_without_relaxing_metadata_cap() {
        // One catalog row and an unrelated list exceeding the metadata cap.
        // This is a bounded-memory correctness fixture, not a throughput test.
        let row = format!(r#"{{"description":"{}"}}"#, "x".repeat(32 * 1024));
        let mut splitter = ArraySplitter::new(ArrayAt::PathOmitLists(&["data", "catalog"]));
        splitter.feed(br#"{"ignored":["#, |_| panic!("not selected"));
        for i in 0..33 {
            if i > 0 {
                splitter.feed(b",", |_| panic!("not selected"));
            }
            splitter.feed(row.as_bytes(), |_| panic!("not selected"));
            assert!(splitter.envelope.len() < 64);
            assert!(splitter.outside.len() < 64);
            assert!(splitter.ignored.as_ref().unwrap().item.capacity() <= 64 * 1024);
        }
        let mut rows = Vec::new();
        splitter.feed(
            br#"],"data":{"catalog":[{"symbol":"a"}]},"code":0}"#,
            |row| rows.push(row.to_owned()),
        );
        assert!(splitter.finished());
        assert_eq!(rows, [r#"{"symbol":"a"}"#]);
        assert_eq!(
            splitter.outside(),
            br#"{"ignored":[],"data":{"catalog":[]},"code":0}"#
        );
        let oversized = format!(
            r#"{{"data":{{"catalog":[]}},"scalar":"{}"}}"#,
            "x".repeat(METADATA_LIMIT)
        );
        let mut splitter = ArraySplitter::new(ArrayAt::PathOmitLists(&["data", "catalog"]));
        splitter.feed(oversized.as_bytes(), |_| {});
        assert!(!splitter.finished());
        assert!(splitter.envelope.len() <= METADATA_LIMIT);
        assert!(splitter.outside.len() <= METADATA_LIMIT);
    }

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

    #[cfg(feature = "any-field-envelope")]
    #[test]
    fn any_field_selector_preserves_array_wrapped_versioned_envelopes() {
        let body = br#"{"version":[{"rows":[1,2],"cursor":"next"}],"code":0}"#;
        for size in [1, 2, 9, 1024] {
            let mut splitter = ArraySplitter::new(ArrayAt::AnyFieldEnvelope(&["rows"]));
            let mut items = Vec::new();
            for chunk in body.chunks(size) {
                splitter.feed(chunk, |item| items.push(item.to_owned()));
            }
            assert!(splitter.finished());
            assert_eq!(items, ["1", "2"]);
            assert_eq!(
                splitter.outside(),
                br#"{"version":[{"rows":[],"cursor":"next"}],"code":0}"#
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
    fn named_paths_require_object_parents_not_array_string_elements() {
        for (at, body) in [
            (
                ArrayAt::Path(&["result", "list"]),
                r#"["result",{"list":[1]}]"#,
            ),
            (
                ArrayAt::Envelope(&[&["result", "list"], &["data"]]),
                r#"["result",{"list":[1]}]"#,
            ),
            (
                ArrayAt::EnvelopeFieldOrNested("result", "list"),
                r#"["result",{"list":[1]}]"#,
            ),
            (
                ArrayAt::EnvelopeFieldOrNestedAny("result", &["list", "rows"]),
                r#"["result",{"rows":[1]}]"#,
            ),
            (
                ArrayAt::Path(&["result", "nested", "list"]),
                r#"{"result":["nested",{"list":[1]}]}"#,
            ),
        ] {
            for size in [1, 2, 9, 1024] {
                let mut splitter = ArraySplitter::new(at);
                let mut items = Vec::new();
                for chunk in body.as_bytes().chunks(size) {
                    splitter.feed(chunk, |item| items.push(item.to_owned()));
                }
                assert!(!splitter.finished(), "{at:?}: {body}");
                assert!(items.is_empty(), "{at:?}: {items:?}");
                assert_eq!(splitter.outside(), body.as_bytes());
            }
        }
    }

    #[test]
    fn explicit_root_paths_still_stream_arrays_with_string_elements() {
        for at in [
            ArrayAt::Root,
            ArrayAt::Path(&[]),
            ArrayAt::Envelope(&[&["data"], &[]]),
        ] {
            let mut splitter = ArraySplitter::new(at);
            let mut items = Vec::new();
            for byte in br#"["result",{"list":[1]}]"# {
                splitter.feed(&[*byte], |item| items.push(item.to_owned()));
            }
            assert!(splitter.finished(), "{at:?}");
            assert_eq!(items, [r#""result""#, r#"{"list":[1]}"#]);
            assert_eq!(splitter.outside(), b"[]");
        }
    }

    #[test]
    fn alternative_envelopes_and_catalog_omission_share_exact_parent_matching() {
        let body = r#"{"shadow":{"data":{"perpProductsV2":[{"symbol":"wrong"}]}},"data":{"before":[{"perpProductsV2":[0]}],"perpProductsV2":[{"symbol":"BTCUSDT","risk":[1,2]}],"cursor":"next","after":[3]},"result":{"perpProductsV2":[{"symbol":"later"}]},"code":0}"#;
        for at in [
            ArrayAt::Envelope(&[&["data", "perpProductsV2"], &["result", "perpProductsV2"]]),
            ArrayAt::PathOmitLists(&["data", "perpProductsV2"]),
        ] {
            for size in [1, 2, 7, 1024] {
                let mut splitter = ArraySplitter::new(at);
                let mut rows = Vec::new();
                for chunk in body.as_bytes().chunks(size) {
                    splitter.feed(chunk, |row| rows.push(row.to_owned()));
                }
                assert!(splitter.finished(), "{at:?}, chunk {size}");
                assert_eq!(rows, [r#"{"symbol":"BTCUSDT","risk":[1,2]}"#]);
                let expected = if matches!(at, ArrayAt::PathOmitLists(_)) {
                    r#"{"shadow":{"data":{"perpProductsV2":[]}},"data":{"before":[],"perpProductsV2":[],"cursor":"next","after":[]},"result":{"perpProductsV2":[]},"code":0}"#.to_owned()
                } else {
                    body.replace(r#"[{"symbol":"BTCUSDT","risk":[1,2]}]"#, "[]")
                };
                assert_eq!(splitter.outside(), expected.as_bytes());
            }
        }
    }

    #[test]
    fn alternative_envelopes_and_catalog_omission_reject_invalid_siblings() {
        for body in [
            r#"{"ignored":[1,],"data":{"perpProductsV2":[1]},"code":0}"#,
            r#"{"data":{"perpProductsV2":[1]},"ignored":[1,],"code":0}"#,
            r#"{"data":{"perpProductsV2":[1]},"ignored":[1],"code":0}garbage"#,
        ] {
            for at in [
                ArrayAt::Envelope(&[&["data", "perpProductsV2"], &["result", "perpProductsV2"]]),
                ArrayAt::PathOmitLists(&["data", "perpProductsV2"]),
            ] {
                for size in [1, 7, 1024] {
                    let mut splitter = ArraySplitter::new(at);
                    for chunk in body.as_bytes().chunks(size) {
                        splitter.feed(chunk, |_| {});
                    }
                    assert!(!splitter.finished(), "{at:?}: {body}");
                }
            }
        }
    }

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

#[cfg(test)]
mod mexc_regressions {
    use super::*;

    #[test]
    fn nested_envelope_streaming_matches_only_the_selected_parent() {
        let body = r#"{"other":{"resultList":[0]},"success":true,"code":0,"data":{"pageSize":2,"resultList":[{"id":1},{"id":2}],"currentPage":1}}"#;
        for chunk in 1..body.len() {
            let mut split = ArraySplitter::new(ArrayAt::Path(&["data", "resultList"]));
            let mut rows = vec![];
            for part in body.as_bytes().chunks(chunk) {
                split.feed(part, |s| rows.push(s.to_owned()));
            }
            assert!(split.finished());
            assert_eq!(rows, [r#"{"id":1}"#, r#"{"id":2}"#]);
            assert_eq!(
                std::str::from_utf8(split.outside()).ok(),
                Some(
                    r#"{"other":{"resultList":[0]},"success":true,"code":0,"data":{"pageSize":2,"resultList":[],"currentPage":1}}"#
                )
            );
        }
    }

    #[test]
    fn malformed_streamed_lists_are_not_successful_empty_snapshots() {
        for body in [
            br#"{"data":[,]}"#.as_slice(),
            br#"{"data":[1,]}"#.as_slice(),
            b"{\"data\":[\"\xff\"]}".as_slice(),
        ] {
            let mut split = ArraySplitter::new(ArrayAt::Field("data"));
            split.feed(body, |_| {});
            assert!(!split.finished());
        }
        for body in [
            br#"{"data":{"resultList":[,]}}"#.as_slice(),
            br#"{"data":{"resultList":[1,]}}"#.as_slice(),
            b"{\"data\":{\"resultList\":[\"\xff\"]}}".as_slice(),
        ] {
            let mut split = ArraySplitter::new(ArrayAt::Path(&["data", "resultList"]));
            split.feed(body, |_| {});
            assert!(!split.finished());
        }
    }

    #[test]
    fn oversized_envelopes_are_rejected_instead_of_truncated() {
        let body = format!(
            r#"{{"data":[],"message":"{}"}}"#,
            "x".repeat(METADATA_LIMIT)
        );
        let mut split = ArraySplitter::new(ArrayAt::EnvelopeField("data"));
        split.feed(body.as_bytes(), |_| {});
        assert!(!split.finished());
        assert_eq!(split.outside().len(), METADATA_LIMIT);
    }

    #[test]
    fn streamed_envelope_retains_fields_before_and_after_the_array() {
        let body = r#"{"code":510,"data":[{"x":[1,2]},3],"success":false}"#;
        for chunk in 1..body.len() {
            let mut split = ArraySplitter::new(ArrayAt::Field("data"));
            let mut items = Vec::new();
            for bytes in body.as_bytes().chunks(chunk) {
                split.feed(bytes, |s| items.push(s.to_owned()));
            }
            assert!(split.finished());
            assert_eq!(items, [r#"{"x":[1,2]}"#, "3"]);
            assert_eq!(
                std::str::from_utf8(split.outside()).ok(),
                Some(r#"{"code":510,"data":[],"success":false}"#)
            );
        }
    }
}

#[cfg(test)]
mod blofin_envelope_tests {
    use super::*;
    #[test]
    fn streamed_envelope_retains_status_after_array_at_every_chunk_boundary() {
        let body = br#"{"msg":"a[\"b","data":[{"nested":[1,2]},3],"code":"152401"}"#;
        for size in 1..body.len() {
            let mut splitter = ArraySplitter::new(ArrayAt::EnvelopeField("data"));
            let mut items = Vec::new();
            for chunk in body.chunks(size) {
                splitter.feed(chunk, |s| items.push(s.to_owned()));
            }
            assert!(splitter.finished());
            assert_eq!(items, [r#"{"nested":[1,2]}"#, "3"]);
            assert_eq!(
                std::str::from_utf8(splitter.outside()).unwrap(),
                r#"{"msg":"a[\"b","data":[],"code":"152401"}"#
            );
        }
    }
}

#[cfg(test)]
mod malformed_stream_tests {
    use super::*;
    #[test]
    fn rejects_empty_elements_trailing_commas_and_invalid_utf8() {
        for body in [
            br#"{"code":"0","data":[,]}"#.as_slice(),
            br#"{"data":[1,]}"#,
            br#"{"data":[1 2]}"#,
            b"{\"data\":[\"\xff\"]}",
        ] {
            let mut splitter = ArraySplitter::new(ArrayAt::EnvelopeField("data"));
            splitter.feed(body, |_| {});
            assert!(!splitter.finished());
        }
    }
}

#[cfg(test)]
mod merged_stream_tests {
    use super::*;

    #[test]
    fn large_venue_lists_keep_status_and_cursor_outside_the_streamed_array() {
        let rows = (0..4096)
            .map(|i| format!(r#"{{"id":{i},"name":"{}"}}"#, "x".repeat(128)))
            .collect::<Vec<_>>()
            .join(",");
        for (at, body, expected) in [
            (
                ArrayAt::EnvelopeField("data"),
                format!(r#"{{"data":[{rows}],"code":"152401"}}"#),
                r#"{"data":[],"code":"152401"}"#,
            ),
            (
                ArrayAt::Path(&["result", "list"]),
                format!(
                    r#"{{"result":{{"list":[{rows}],"nextPageCursor":"cursor"}},"retCode":10001}}"#
                ),
                r#"{"result":{"list":[],"nextPageCursor":"cursor"},"retCode":10001}"#,
            ),
        ] {
            for chunk in [7, 4096, 65_536] {
                let mut splitter = ArraySplitter::new(at);
                let mut count = 0;
                for bytes in body.as_bytes().chunks(chunk) {
                    splitter.feed(bytes, |_| count += 1);
                }
                assert!(splitter.finished());
                assert_eq!(count, 4096);
                assert_eq!(std::str::from_utf8(splitter.outside()).unwrap(), expected);
                assert!(splitter.item.capacity() < 1024);
            }
        }
    }

    #[test]
    fn nested_paths_reject_invalid_items_and_incomplete_or_oversized_envelopes() {
        for body in [
            br#"{"retCode":0,"result":{"list":[,]}}"#.as_slice(),
            br#"{"retCode":0,"result":{"list":[1,]}}"#,
            br#"{"retCode":0,"result":{"list":[1 2]}}"#,
            br#"{"retCode":0,"result":{"list":[{"a":tru}]}}"#,
            br#"{"retCode":0,"result":{"list":[]}"#,
            b"{\"retCode\":0,\"result\":{\"list\":[\"\xff\"]}}",
        ] {
            for chunk in [1, 7, 1024] {
                let mut splitter = ArraySplitter::new(ArrayAt::Path(&["result", "list"]));
                for bytes in body.chunks(chunk) {
                    splitter.feed(bytes, |_| {});
                }
                assert!(!splitter.finished());
            }
        }
        let body = format!(
            r#"{{"result":{{"list":[]}},"metadata":"{}"}}"#,
            "x".repeat(METADATA_LIMIT)
        );
        let mut splitter = ArraySplitter::new(ArrayAt::Path(&["result", "list"]));
        splitter.feed(body.as_bytes(), |_| {});
        assert!(!splitter.finished());
        assert_eq!(splitter.outside().len(), METADATA_LIMIT);
    }
}
