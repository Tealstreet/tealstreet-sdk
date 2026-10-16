use safe_cex_venue_coinbase::utils::{
    CoinbasePemDecodeError, asn1_wrap, base64_url_encode, base64_url_encode_string,
    pem_to_pkcs8_der,
};
use serde_json::Value;
use std::{process::Command, sync::OnceLock};

const LENGTHS: [usize; 10] = [0, 1, 2, 3, 127, 128, 255, 256, 65535, 65536];
const TAGS: [f64; 8] = [
    4.0,
    48.0,
    -1.0,
    256.0,
    257.75,
    -257.75,
    f64::NAN,
    f64::INFINITY,
];
fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/signing-helpers.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "live signing helper oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
fn bytes(length: usize) -> Vec<u8> {
    (0..length)
        .map(|i| ((i * 37 + length) % 256) as u8)
        .collect()
}
fn expected_bytes(value: &Value) -> Vec<u8> {
    serde_json::from_value(value.clone()).unwrap()
}
#[test]
fn base64_helpers_match_live_native_strings() {
    let cases = oracle()["encoded"].as_array().unwrap();
    assert_eq!(cases.len(), 10, "byte encoding corpus floor");
    for (case, length) in cases.iter().zip(LENGTHS) {
        assert_eq!(case["length"], length, "byte encoding input identity");
        assert_eq!(
            base64_url_encode(&bytes(length)),
            case["result"].as_str().unwrap(),
            "native base64url bytes"
        );
    }
    let inputs = ["", "f", "fo", "foo", "\0+/=", "你好", "é", "💰", "\u{feff}"];
    let cases = oracle()["texts"].as_array().unwrap();
    assert_eq!(cases.len(), 9, "string encoding corpus floor");
    for (case, input) in cases.iter().zip(inputs) {
        assert_eq!(case["input"], input, "string encoding input identity");
        assert_eq!(
            base64_url_encode_string(input),
            case["result"].as_str().unwrap(),
            "native base64url string"
        );
    }
}
#[test]
fn asn1_wrap_matches_live_native_bytes() {
    let cases = oracle()["wrapped"].as_array().unwrap();
    assert_eq!(cases.len(), 80, "ASN.1 corpus floor");
    for (case, (length, (tag_index, tag))) in cases.iter().zip(
        LENGTHS
            .into_iter()
            .flat_map(|length| TAGS.into_iter().enumerate().map(move |tag| (length, tag))),
    ) {
        assert_eq!(case["length"], length, "ASN.1 length identity");
        assert_eq!(case["tagIndex"], tag_index, "ASN.1 tag index");
        assert_eq!(
            case["tag"],
            [
                "4", "48", "-1", "256", "257.75", "-257.75", "NaN", "Infinity"
            ][tag_index],
            "ASN.1 tag identity"
        );
        assert_eq!(
            asn1_wrap(tag, &bytes(length)),
            expected_bytes(&case["result"]),
            "native ASN.1 tag/length/payload"
        );
    }
}
#[test]
fn pem_conversion_matches_live_native_bytes_and_rejection() {
    let inputs = [
        "",
        "AQID",
        "-----BEGIN PRIVATE KEY-----\nAQID\n-----END PRIVATE KEY-----",
        "-----BEGIN EC PRIVATE KEY-----\nAQID\n-----END EC PRIVATE KEY-----",
        "-----BEGIN EC PRIVATE KEY-----\u{feff} Zh==\u{2028}-----END PRIVATE KEY-----",
        "A",
        "A!",
        "Zg=",
        "Zg===",
        "-___",
        "-----BEGIN PRIVATE KEY-----AQID-----BEGIN PRIVATE KEY----------END PRIVATE KEY-----",
        "-----BEGIN PRIVATE KEY-----AQID-----END EC PRIVATE KEY-----",
        "AQID\u{0085}",
    ];
    let cases = oracle()["pem"].as_array().unwrap();
    assert_eq!(cases.len(), 13, "PEM corpus floor");
    for (case, input) in cases.iter().zip(inputs) {
        assert_eq!(case["input"], input, "PEM input identity");
        let actual = pem_to_pkcs8_der(input);
        if let Some(expected) = case.get("result") {
            assert_eq!(
                actual.unwrap(),
                expected_bytes(expected),
                "native PEM DER bytes"
            );
        } else {
            assert_eq!(actual, Err(CoinbasePemDecodeError), "native PEM rejection");
            assert_eq!(
                case["error"]["name"], "InvalidCharacterError",
                "source PEM rejection family"
            );
        }
    }
}
