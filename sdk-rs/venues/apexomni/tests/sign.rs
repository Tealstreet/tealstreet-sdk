use safe_cex_engine::Method;
use safe_cex_venue_apexomni::sign::{self, LegacyCredentials};

/// The legacy canonicalization oracle's credentials; never venue options.
fn options() -> LegacyCredentials {
    LegacyCredentials {
        key: Some("api-key".into()),
        secret: Some("mysecret".into()),
        passphrase: Some("pass".into()),
    }
}
fn fields(values: &[(&str, &str)]) -> Vec<(String, String)> {
    values
        .iter()
        .map(|(k, v)| ((*k).into(), (*v).into()))
        .collect()
}
fn header<'a>(signed: &'a sign::Signed, name: &str) -> &'a str {
    &signed.headers.iter().find(|(k, _)| *k == name).unwrap().1
}

#[test]
fn canonical_query_keeps_string_zero_false_and_encodes_utf8() {
    assert_eq!(
        sign::query(&fields(&[
            ("z", "false"),
            ("a", "0"),
            ("note", "x + /é!*'()~")
        ])),
        "a=0&note=x%20%2B%20%2F%C3%A9%21%2A%27%28%29~&z=false"
    );
}

#[test]
fn rest_hmac_uses_base64_text_key_and_exact_query_bytes() {
    let request = sign::Request::get(
        "/v3/account",
        fields(&[("symbol", "BTC-USDT"), ("limit", "0")]),
    );
    let signed = sign::request(&options(), 1700000000123.0, &request, false).unwrap();
    assert_eq!(signed.path, "/v3/account?limit=0&symbol=BTC-USDT");
    assert_eq!(header(&signed, "APEX-TIMESTAMP"), "1700000000123");
    assert_eq!(
        header(&signed, "APEX-SIGNATURE"),
        "/RYMhyXhNY/QXuYQaMWcRSTbuozBqYgE8jPnfdrfMxc="
    );
    let legacy = sign::request(&options(), 1700000000123.0, &request, true).unwrap();
    assert_eq!(header(&legacy, "APEX-TIMESTAMP"), "1700000000000");
    assert_eq!(
        header(&legacy, "APEX-SIGNATURE"),
        "psxKpL+elqO3G2UuUqApEK9CKqyn020wUEyQDK/r1mE="
    );
}

#[test]
fn raw_order_legacy_signs_body_once_and_keeps_wire_body() {
    let raw = "clientId=abc&reduceOnly=false&size=1";
    let request = sign::Request::raw("/v3/order", raw.into());
    let signed = sign::request(&options(), 1700000000123.0, &request, false).unwrap();
    let legacy = sign::request(&options(), 1700000000123.0, &request, true).unwrap();
    assert_eq!(signed.body.as_deref(), Some(raw));
    assert_eq!(legacy.body, signed.body);
    assert_eq!(
        header(&signed, "APEX-SIGNATURE"),
        "TCUlpThB4SULuo5m6+3He+bQVN/cDlGZV410nbn1HGk="
    );
    assert_eq!(
        header(&legacy, "APEX-SIGNATURE"),
        "EjkW84sx4BJxzJWtrwzr8BVDvUs9qXFUAV5V9xMrYUs="
    );
}

#[test]
fn legacy_numeric_zero_bound_is_omitted_only_from_signature_not_wire_query() {
    let request = sign::Request::get(
        "/v3/fills",
        fields(&[("beginTimeInclusive", "0"), ("symbol", "BTC-USDT")]),
    );
    let signed = sign::request(&options(), 1700000000123.0, &request, true).unwrap();
    assert_eq!(
        signed.path,
        "/v3/fills?beginTimeInclusive=0&symbol=BTC-USDT"
    );
    assert_eq!(
        header(&signed, "APEX-SIGNATURE"),
        safe_cex_crypto::hmac_sha256(
            b"bXlzZWNyZXQ=",
            b"1700000000000GET/api/v3/fills?symbol=BTC-USDT",
            safe_cex_crypto::Encoding::Base64
        )
    );
}

#[test]
fn delete_signs_query_not_body() {
    let mut request = sign::Request::get(
        "/v3/account",
        fields(&[("limit", "0"), ("symbol", "BTC-USDT")]),
    );
    request.method = Method::Delete;
    request.body = sign::Body::Raw("ignored-for-signing".into());
    let signed = sign::request(&options(), 1700000000123.0, &request, false).unwrap();
    let text = b"1700000000123DELETE/api/v3/account?limit=0&symbol=BTC-USDT";
    assert_eq!(
        header(&signed, "APEX-SIGNATURE"),
        safe_cex_crypto::hmac_sha256(b"bXlzZWNyZXQ=", text, safe_cex_crypto::Encoding::Base64)
    );
}

#[test]
fn leverage_signs_canonical_fields_and_sends_real_multipart() {
    let request = sign::Request::multipart(
        "/v3/set-initial-margin-rate",
        fields(&[("symbol", "BTC-USDT"), ("initialMarginRate", "0.1")]),
    );
    let signed = sign::request(&options(), 1700000000123.0, &request, false).unwrap();
    assert_eq!(
        header(&signed, "APEX-SIGNATURE"),
        "rv5ztBGjZL3l6fJyOBH4loaDlqYbK3NYj0CYQKub1b4="
    );
    assert_eq!(
        header(&signed, "Content-Type"),
        "multipart/form-data; boundary=safe-cex-apexomni"
    );
    assert!(
        signed
            .body
            .unwrap()
            .contains("name=\"initialMarginRate\"\r\n\r\n0.1\r\n")
    );
}

#[test]
fn legacy_resend_preserves_original_cancel_and_leverage_payloads() {
    // TS fix: retain original fields across the response-interceptor resend,
    // rather than signing transformed text or random multipart internals.
    let cancel = sign::Request::form("/v3/delete-order", fields(&[("id", "cancel-id")]));
    let primary = sign::request(&options(), 1700000000123.0, &cancel, false).unwrap();
    let legacy = sign::request(&options(), 1700000000123.0, &cancel, true).unwrap();
    assert_eq!(
        header(&primary, "APEX-SIGNATURE"),
        "emMYBvI0UpVDczbvrbLq708lSjde4iPOAEJrXIV2OBU="
    );
    assert_eq!(
        header(&legacy, "APEX-SIGNATURE"),
        "Y3D7zWAAWJk0csVwTLvq9QeF/NVYiDFCHQt1mR61yfw="
    );
    assert_eq!(primary.body, legacy.body);
    let leverage = sign::Request::multipart(
        "/v3/set-initial-margin-rate",
        fields(&[
            ("symbol", "BTC-USDT"),
            ("token", "USDT"),
            ("initialMarginRate", "0.10000"),
        ]),
    );
    let primary = sign::request(&options(), 1700000000123.0, &leverage, false).unwrap();
    let legacy = sign::request(&options(), 1700000000123.0, &leverage, true).unwrap();
    assert_eq!(
        header(&primary, "APEX-SIGNATURE"),
        "uya4xvcl0ugKy3FbS845nTAgO5zz3KXbIgYbXWpq7Qk="
    );
    assert_eq!(
        header(&legacy, "APEX-SIGNATURE"),
        "tRCiqi5GRQ9wLnUumNNmpNJZ2SWRwShqSm5TuIjNWbM="
    );
    assert_eq!(primary.body, legacy.body);
}

#[test]
fn ws_login_keeps_nested_json_string_and_millisecond_timestamp() {
    let actual = sign::ws_login(&options(), 1700000000123.0).unwrap();
    let expected = r#"{"op":"login","args":["{\"type\":\"login\",\"httpMethod\":\"GET\",\"requestPath\":\"/ws/accounts\",\"apiKey\":\"api-key\",\"passphrase\":\"pass\",\"timestamp\":1700000000123,\"signature\":\"OAr2UVRN6AAgBzP9ag9MqnMXQpDLX29oE0rAg72GNpQ=\"}"]}"#;
    assert_eq!(actual, expected);
}

#[test]
fn public_klines_strip_all_headers_and_symbols_allow_empty_credentials() {
    let unsigned = sign::request(
        &LegacyCredentials::default(),
        1700000000123.0,
        &sign::Request::get("/v3/klines", vec![]),
        false,
    )
    .unwrap();
    assert!(unsigned.headers.is_empty());
    assert!(
        sign::request(
            &LegacyCredentials::default(),
            1700000000123.0,
            &sign::Request::get("/v3/symbols", vec![]),
            false
        )
        .is_ok()
    );
    assert!(
        sign::request(
            &LegacyCredentials::default(),
            1700000000123.0,
            &sign::Request::get("/v3/account", vec![]),
            false
        )
        .is_err()
    );
    assert!(sign::ws_login(&options(), f64::NAN).is_err());
}

#[test]
fn seed_is_exact_eth_signature_bytes_not_text_or_private_key() {
    let seed = format!("{}1b", "01".repeat(64));
    assert!(sign::validate_seed(&seed).is_ok());
    assert!(sign::validate_seed(&format!("0x{seed}")).is_ok());
    for invalid in [
        "",
        "seed",
        &"01".repeat(32),
        &format!("{}zz", "01".repeat(64)),
    ] {
        assert!(sign::validate_seed(invalid).is_err());
    }
}

#[test]
fn decimal_scaling_truncates_at_18_places_without_binary_float() {
    assert_eq!(
        sign::fixed_18("0.1234567891234567899").unwrap(),
        123456789123456789
    );
    assert_eq!(sign::fixed_18("1e-8").unwrap(), 10000000000);
    assert_eq!(sign::fixed_18("1e-100").unwrap(), 0);
    for invalid in ["NaN", "-1", "1e1000", "1.2.3"] {
        assert!(sign::fixed_18(invalid).is_err());
    }
}

#[test]
fn fee_decimal_product_preserves_ts_number_boundaries() {
    // Independent BigNumber oracle: a binary-float product alone gives 0.000011
    // for the second case, and an exact ceil alone loses the Number boundary.
    for (price, size, fee, expected) in [
        ("60000", "0.001", "0.0005", "0.030000"),
        ("0.1", "0.2", "0.0005", "0.000010"),
        ("1.0000000000000001", "1", "0.000001", "0.000002"),
        ("9.999999999999999", "1", "0.000001", "0.000010"),
        (
            "0.123456789123456789",
            "1.23456789123456789",
            "0.0006",
            "0.000092",
        ),
    ] {
        assert_eq!(sign::limit_fee(price, size, fee).unwrap(), expected);
    }
}

#[test]
fn exact_npm_sdk_1_0_2_contract_signatures() {
    // Captured from the installed, unmodified npm WASM. Independent signer inputs
    // cover Unicode IDs, narrowed pair/slot/nonce fields and packed-size rounding.
    let seed = format!("{}1b", "01".repeat(64));
    let cases = [
        (
            "client-1",
            "611779989544632629",
            1,
            "0.001",
            "60000",
            "0.0002",
            "0.0005",
            true,
            "befd7fa78713914a4aedff83b03ae0f80df595c80d8fb916935065f0c3cb048c8b147d1bf6f143482ef439e8f99aa18b87f127fbb300a5ef98531b8dee8ff700",
        ),
        (
            "é/测试+",
            "4294967296",
            258,
            "1.23456789123456789",
            "0.123456789123456789",
            "-0.0002",
            "0.0006",
            false,
            "9df751eb7bdcd59bb957ade218f7edf3b0418c583d258865dbcd0dc31de9fd97bf466f0505ee996e479548b70aed60d32a5870cd0241fd13eaad316e76c6bc02",
        ),
        (
            "same-id",
            "1",
            255,
            "0.000000034359738368",
            "132922799578491587",
            "0",
            "0.0255",
            true,
            "10f270690cc691b450468aa0aa32e5511064a71be787f29d9726f43a0c2407aee3c5ab4e29b47402653f78e82b01feaec49a9280498e86d922d8253c24af3405",
        ),
    ];
    for (id, account, pair, size, price, maker, taker, buy, expected) in cases {
        assert_eq!(
            sign::contract_signature(&seed, id, account, pair, size, price, maker, taker, buy)
                .unwrap(),
            expected
        );
    }
    assert_ne!(
        sign::contract_signature(
            &seed,
            "client-2",
            "611779989544632629",
            1,
            "0.001",
            "60000",
            "0.0002",
            "0.0005",
            true
        )
        .unwrap(),
        cases[0].8
    );
}

#[test]
fn contract_byte_layout_narrows_slot_nonce_pair_and_subsidy() {
    let bytes = sign::contract_payload(
        "client-1",
        "611779989544632629",
        257,
        "0.001",
        "60000",
        "0.0002",
        "0.0005",
        true,
    )
    .unwrap();
    assert_eq!(&bytes[..6], &[0xfe, 0xc5, 0xfd, 0x7b, 0xdd, 0]);
    assert_eq!(&bytes[6..8], &(3864728048u32 as u16).to_be_bytes());
    assert_eq!(&bytes[8..11], &3019828134u32.to_be_bytes()[1..]);
    assert_eq!(bytes[11], 1);
    assert_eq!(bytes[12], 1);
    assert_eq!(&bytes[33..], &[2, 5, 0]);
}
