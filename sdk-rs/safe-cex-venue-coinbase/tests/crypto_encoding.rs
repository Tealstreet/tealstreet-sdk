use safe_cex_venue_coinbase::utils::{
    asn1_wrap, base64_url_encode, base64_url_encode_string, pem_to_pkcs8_der,
};
use serde_json::Value;
use std::{process::Command, sync::OnceLock};

fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/crypto-encoding.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}

#[test]
fn coinbase_crypto_encoding_helpers_match_live_source() {
    assert_eq!(oracle()["binary"].as_array().unwrap().len(), 12);
    assert_eq!(oracle()["strings"].as_array().unwrap().len(), 8);
    assert_eq!(oracle()["lengths"].as_array().unwrap().len(), 8);
    assert_eq!(oracle()["tags"].as_array().unwrap().len(), 6);
    assert_eq!(oracle()["pem"].as_array().unwrap().len(), 13);

    for case in oracle()["binary"].as_array().unwrap() {
        let bytes = case["bytes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|byte| byte.as_u64().unwrap() as u8)
            .collect::<Vec<_>>();
        assert_eq!(
            base64_url_encode(&bytes),
            case["result"].as_str().unwrap(),
            "base64UrlEncode {}",
            case["label"]
        );
    }
    for case in oracle()["strings"].as_array().unwrap() {
        assert_eq!(
            base64_url_encode_string(case["input"].as_str().unwrap()),
            case["result"].as_str().unwrap(),
            "base64UrlEncodeString {:?}",
            case["input"]
        );
    }
    for case in oracle()["lengths"].as_array().unwrap() {
        let bytes = case["bytes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|byte| byte.as_u64().unwrap() as u8)
            .collect::<Vec<_>>();
        assert_eq!(
            asn1_wrap(case["tag"].as_f64().unwrap(), &bytes),
            serde_json::from_value::<Vec<u8>>(case["result"].clone()).unwrap(),
            "asn1Wrap {}",
            case["label"]
        );
    }
    for case in oracle()["tags"].as_array().unwrap() {
        let tag = case["tag"].as_f64().unwrap_or(f64::NAN);
        let bytes = case["bytes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|byte| byte.as_u64().unwrap() as u8)
            .collect::<Vec<_>>();
        assert_eq!(
            asn1_wrap(tag, &bytes),
            serde_json::from_value::<Vec<u8>>(case["result"].clone()).unwrap(),
            "asn1Wrap {}",
            case["label"]
        );
    }
    for case in oracle()["pem"].as_array().unwrap() {
        let actual = pem_to_pkcs8_der(case["input"].as_str().unwrap());
        if case["result"].is_null() {
            assert!(actual.is_err(), "native PEM rejection {}", case["label"]);
        } else {
            assert_eq!(
                actual.unwrap(),
                serde_json::from_value::<Vec<u8>>(case["result"].clone()).unwrap(),
                "native PEM bytes {}",
                case["label"]
            );
        }
    }
}
