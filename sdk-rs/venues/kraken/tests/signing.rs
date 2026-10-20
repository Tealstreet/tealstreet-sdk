use safe_cex_engine::url::encode_component;
use safe_cex_venue_kraken::sign::{Signer, form};
#[test]
fn independent_hashlib_vectors_cover_challenge_and_both_path_families() {
    // Independent Python hashlib.sha256 + hmac.new(..., hashlib.sha512), secret b"secret".
    let s = Signer::new("c2VjcmV0").unwrap();
    assert_eq!(
        s.challenge("challenge-æ-123"),
        "RrIo22Xn634Pt3sq0W+1h/7OUdrAgbSG4qt+qaycGFEledqV4+k70KrF0smVdYMou8BVj/wi056PrIXGJfjRtQ=="
    );
    let encoded = form(&[("symbol", "PF_XBTUSD".into()), ("note", "é + /~".into())]);
    let signature = s
        .signature(
            &encoded,
            "1700000000000000",
            "/derivatives/api/v3/sendorder",
        )
        .unwrap();
    assert_eq!(
        signature,
        "4TMpQ1hDUBIUn95fYJYlhprS9SpGaPNvX5zdebA2b3LemKgBXCiar+BQwvj/3v0f6go1Qbmylp0YaLR3FUrRiA=="
    );
    assert_eq!(
        s.signature(
            "count=50",
            "1700000000000001",
            "/api/history/v3/account-log"
        )
        .unwrap(),
        "NxNOdcerU3JDkpk81eVA1BWaCNmsoqCCCfL7TbOOoITZdM7elEFmHGTUu3NRcnqfgWmdDXDgq307kNY+0ZlKOg=="
    );
    assert_ne!(
        signature,
        s.signature(
            &encoded.replace('+', "%20"),
            "1700000000000000",
            "/derivatives/api/v3/sendorder"
        )
        .unwrap()
    );
    assert_ne!(
        signature,
        s.signature(
            &encoded,
            "1700000000000001",
            "/derivatives/api/v3/sendorder"
        )
        .unwrap()
    );
    assert_ne!(
        signature,
        s.signature(
            &encoded,
            "1700000000000000",
            "/derivatives/api/v3/editorder"
        )
        .unwrap()
    );
    assert!(s.signature("", "1", "/0/private/AddOrder").is_err());
}
#[test]
fn batch_special_encoding_matches_encode_uri_component() {
    let s = Signer::new("c2VjcmV0").unwrap();
    let batch = format!("json={}", encode_component(r#"{"batchOrder":[]}"#));
    assert_eq!(
        s.signature(&batch, "1700000000000002", "/derivatives/api/v3/batchorder")
            .unwrap(),
        "P76LoYBT5+qIs1XyRVLF84NEZlqejLZXIiy5v+6g+kcvrdb7FnyrcYc/2+/SsEMedQL/FHxSj5Cp02VL4KgnIg=="
    );
    assert_eq!(encode_component("a b()!'*+"), "a%20b()!'*%2B");
    assert_eq!(form(&[("s", "a b()!'*+".into())]), "s=a+b()%21%27%2A%2B");
}
