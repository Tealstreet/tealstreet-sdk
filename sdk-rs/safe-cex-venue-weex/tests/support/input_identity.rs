use serde_json::Value;

/// Fixed ordered input census; FNV is a regression checksum, not a security hash.
/// Pin raw inputs as well as labels so relabelled duplicate cases cannot erase coverage.
pub fn check(corpus: &Value, expected: u64, label: &str) {
    let mut hash = 0xcbf29ce484222325_u64;
    for case in corpus["cases"].as_array().unwrap() {
        for key in ["id", "raw"] {
            for byte in case[key]
                .as_str()
                .unwrap()
                .bytes()
                .chain(std::iter::once(0))
            {
                hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
            }
        }
    }
    assert_eq!(hash, expected, "{label}::input_identity");
}
