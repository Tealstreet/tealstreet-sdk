//! Test-only observations at the typed String / retained UTF-16 boundary.
use safe_cex_types::retained_json::RetainedJsonInput as Input;

#[allow(dead_code)] // Some targets already execute their own production decoder.
pub fn decode<T: serde::de::DeserializeOwned + serde::Serialize>(raw: &str) -> T {
    let input = Input::parse_json(&raw.encode_utf16().collect::<Vec<_>>()).unwrap();
    let value = input
        .decode_native(false)
        .expect("canonical typed record retains the frame");
    assert_projection(raw, &value);
    value
}

#[allow(dead_code)]
pub fn retained(raw: &str) -> Input {
    let retained = Input::parse_json(&raw.encode_utf16().collect::<Vec<_>>()).unwrap();
    let native_raw: Input = retained.clone().decode_native(false).unwrap();
    assert_eq!(
        native_raw, retained,
        "retained native input keeps original UTF-16"
    );
    retained
}

pub fn assert_projection<T: serde::Serialize>(raw: &str, typed: &T) {
    let retained = retained(raw);
    // Capture native String callbacks and retained payloads before a JSON writer
    // could project numbers or reject an otherwise valid opaque string.
    let observed = safe_cex_num::numeric_ingress::js_capture_output(typed).unwrap();
    let count = compare(&retained, &observed, "");
    assert!(
        count > 0,
        "typed/retained surrogate field must be observed: {raw}"
    );
}

fn compare(source: &Input, typed: &Input, path: &str) -> usize {
    match (source, typed) {
        (Input::Text(units), Input::Text(actual)) if String::from_utf16(units).is_err() => {
            let replaced: Vec<u16> = String::from_utf16_lossy(units).encode_utf16().collect();
            // A native String serializes the replacement. An opaque retained
            // field instead serializes its original units; both stay observable.
            assert!(
                actual == &replaced || actual == units,
                "typed U+FFFD or opaque retained units at {path}: {actual:?} != {units:?}"
            );
            1
        }
        (Input::Array(source), Input::Array(actual)) => source
            .iter()
            .zip(actual)
            .enumerate()
            .map(|(i, (a, b))| compare(a, b, &format!("{path}/{i}")))
            .sum(),
        (Input::Object(source), Input::Object(actual)) => source
            .iter()
            .map(|(key, value)| {
                let normalized: Vec<u16> = String::from_utf16_lossy(key).encode_utf16().collect();
                let Some((actual_key, actual_value)) =
                    actual.iter().find(|(k, _)| k == key || k == &normalized)
                else {
                    return 0; // Erased unknown fields are not native record members.
                };
                let key_count = usize::from(String::from_utf16(key).is_err());
                if key_count != 0 {
                    assert!(
                        actual_key == key || actual_key == &normalized,
                        "typed key replacement or retained key units at {path}"
                    );
                }
                key_count
                    + compare(
                        value,
                        actual_value,
                        &format!("{path}/{}", String::from_utf16_lossy(key)),
                    )
            })
            .sum(),
        _ => 0,
    }
}
