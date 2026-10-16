use serde_json::Value;

pub fn assert_capture_metadata(value: &Value, expected_headers: usize) {
    let mut headers = 0;
    assert_capture_metadata_inner(value, &mut headers);
    assert_eq!(headers, expected_headers, "capture metadata header count");
}

fn assert_capture_metadata_inner(value: &Value, headers: &mut usize) {
    match value {
        Value::Array(values) => values
            .iter()
            .for_each(|value| assert_capture_metadata_inner(value, headers)),
        Value::Object(object) => {
            if let Some(architecture) = object.get("captureArchitecture") {
                *headers += 1;
                assert!(
                    architecture.as_str().is_some(),
                    "capture architecture header"
                );
                assert!(
                    object["captureNodeMajor"]
                        .as_u64()
                        .is_some_and(|major| major > 0),
                    "capture Node major header"
                );
            }
            object
                .values()
                .for_each(|value| assert_capture_metadata_inner(value, headers));
        }
        _ => {}
    }
}
