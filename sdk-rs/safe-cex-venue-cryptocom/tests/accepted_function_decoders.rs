#[test]
fn native_is_public_endpoint() {
    let cases:Vec<serde_json::Value>=serde_json::from_str(r###"[{"input":"","output":false},{"input":"/public/get-instruments","output":true},{"input":"/public/get-book","output":true},{"input":"/public/get-tickers","output":true},{"input":"/public/get-ticker","output":true},{"input":"/public/get-trades","output":true},{"input":"/public/get-candlestick","output":true},{"input":"/public/get-valuations","output":true},{"input":"/public/get-expired-settlement-price","output":true},{"input":"/public/get-insurance","output":true},{"input":"/private/create-order","output":false},{"input":"/private/advanced/create-order","output":false},{"input":"/private/amend-order","output":false},{"input":"/private/advanced/amend-order","output":false},{"input":"/private/cancel-order","output":false},{"input":"/private/advanced/cancel-order","output":false},{"input":"/private/cancel-all-orders","output":false},{"input":"/private/close-position","output":false},{"input":"/private/change-account-leverage","output":false},{"input":"/private/get-open-orders","output":false},{"input":"/private/user-balance","output":false},{"input":"/private/create-withdrawal","output":false},{"input":"/public/not-real","output":false},{"input":"/PUBLIC/get-book","output":false},{"input":"/private/CREATE-order","output":false},{"input":"prefix/public/get-book-suffix","output":true},{"input":"https://api.crypto.com/exchange/v1/public/get-instruments?x=1","output":true},{"input":"/public/get-book?next=/private/cancel-order","output":true},{"input":"\ud83d\ude42/private/close-position","output":false},{"input":"/private/create-order-extra","output":false},{"input":"\u0000/public/get-insurance","output":true},{"input":"e\u0301/public/get-trades","output":true},{"input":"/private/create\u0000-order","output":false}]"###).unwrap();
    assert_eq!(
        cases.len(),
        33,
        "is_public_endpoint independent fixture count"
    );
    let mut executed = 0;
    for case in cases {
        let value = &case["input"];
        let observed = serde_json::json!(safe_cex_venue_cryptocom::api::is_public_endpoint(
            value.as_str().unwrap()
        ));
        assert_eq!(
            observed, case["output"],
            "is_public_endpoint native output input {value}"
        );
        executed += 1;
    }
    assert_eq!(executed, 33, "is_public_endpoint executed fixture count");
}

#[test]
fn native_is_non_retryable_private_write() {
    let cases:Vec<serde_json::Value>=serde_json::from_str(r###"[{"input":null,"output":false},{"input":"","output":false},{"input":"/public/get-instruments","output":false},{"input":"/public/get-book","output":false},{"input":"/public/get-tickers","output":false},{"input":"/public/get-ticker","output":false},{"input":"/public/get-trades","output":false},{"input":"/public/get-candlestick","output":false},{"input":"/public/get-valuations","output":false},{"input":"/public/get-expired-settlement-price","output":false},{"input":"/public/get-insurance","output":false},{"input":"/private/create-order","output":true},{"input":"/private/advanced/create-order","output":true},{"input":"/private/amend-order","output":true},{"input":"/private/advanced/amend-order","output":true},{"input":"/private/cancel-order","output":true},{"input":"/private/advanced/cancel-order","output":true},{"input":"/private/cancel-all-orders","output":true},{"input":"/private/close-position","output":true},{"input":"/private/change-account-leverage","output":true},{"input":"/private/get-open-orders","output":false},{"input":"/private/user-balance","output":false},{"input":"/private/create-withdrawal","output":false},{"input":"/public/not-real","output":false},{"input":"/PUBLIC/get-book","output":false},{"input":"/private/CREATE-order","output":false},{"input":"prefix/public/get-book-suffix","output":false},{"input":"https://api.crypto.com/exchange/v1/public/get-instruments?x=1","output":false},{"input":"/public/get-book?next=/private/cancel-order","output":true},{"input":"\ud83d\ude42/private/close-position","output":true},{"input":"/private/create-order-extra","output":true},{"input":"\u0000/public/get-insurance","output":false},{"input":"e\u0301/public/get-trades","output":false},{"input":"/private/create\u0000-order","output":false}]"###).unwrap();
    assert_eq!(
        cases.len(),
        34,
        "is_non_retryable_private_write independent fixture count"
    );
    let mut executed = 0;
    for case in cases {
        let value = &case["input"];
        let observed = serde_json::json!(
            safe_cex_venue_cryptocom::api::is_non_retryable_private_write(value.as_str())
        );
        assert_eq!(
            observed, case["output"],
            "is_non_retryable_private_write native output input {value}"
        );
        executed += 1;
    }
    assert_eq!(
        executed, 34,
        "is_non_retryable_private_write executed fixture count"
    );
}

#[test]
fn native_is_cryptocom_signature_rejection() {
    let cases:Vec<serde_json::Value>=serde_json::from_str(r###"[{"input":"null","output":{"ok":false}},{"input":"false","output":{"ok":false}},{"input":"true","output":{"ok":false}},{"input":"40101","output":{"ok":false}},{"input":"\"40101\"","output":{"ok":false}},{"input":"[]","output":{"ok":false}},{"input":"[40101]","output":{"ok":false}},{"input":"{}","output":{"ok":false}},{"input":"{\"code\":40101}","output":{"ok":true}},{"input":"{\"code\":\"40101\"}","output":{"ok":true}},{"input":"{\"code\":\" 40101 \"}","output":{"ok":true}},{"input":"{\"code\":\"0x9ca5\"}","output":{"ok":true}},{"input":"{\"code\":\"+40101\"}","output":{"ok":true}},{"input":"{\"code\":\"\\ufeff40101\\u00a0\"}","output":{"ok":true}},{"input":"{\"code\":[40101]}","output":{"ok":true}},{"input":"{\"code\":[\"40101\"]}","output":{"ok":true}},{"input":"{\"code\":[[40101]]}","output":{"ok":true}},{"input":"{\"code\":40100}","output":{"ok":false}},{"input":"{\"code\":40102}","output":{"ok":false}},{"input":"{\"code\":-0}","output":{"ok":false}},{"input":"{\"code\":null}","output":{"ok":false}},{"input":"{\"code\":false}","output":{"ok":false}},{"input":"{\"code\":true}","output":{"ok":false}},{"input":"{\"code\":\"\"}","output":{"ok":false}},{"input":"{\"code\":\"Infinity\"}","output":{"ok":false}},{"input":"{\"code\":\"NaN\"}","output":{"ok":false}},{"input":"{\"code\":\"40101n\"}","output":{"ok":false}},{"input":"{\"code\":[]}","output":{"ok":false}},{"input":"{\"code\":[null]}","output":{"ok":false}},{"input":"{\"code\":[40101,0]}","output":{"ok":false}},{"input":"{\"code\":{}}","output":{"ok":false}},{"input":"{\"code\":{\"valueOf\":null}}","output":{"ok":false}},{"input":"{\"code\":{\"toString\":null}}","output":{"error":"TypeError","message":"Cannot convert object to primitive value"}},{"input":"{\"code\":{\"toString\":1}}","output":{"error":"TypeError","message":"Cannot convert object to primitive value"}},{"input":"{\"code\":{\"valueOf\":null,\"toString\":null}}","output":{"error":"TypeError","message":"Cannot convert object to primitive value"}},{"input":"{\"code\":40100.999999999999}","output":{"ok":true}},{"input":"{\"code\":40101.00000000001}","output":{"ok":false}}]"###).unwrap();
    assert_eq!(
        cases.len(),
        37,
        "is_cryptocom_signature_rejection independent fixture count"
    );
    let mut executed = 0;
    for case in cases {
        let value = &case["input"];
        let observed = {
            let input: serde_json::Value = serde_json::from_str(value.as_str().unwrap()).unwrap();
            match safe_cex_venue_cryptocom::api::is_cryptocom_signature_rejection(&input) {
                Ok(result) => serde_json::json!({"ok":result}),
                Err(error) => {
                    assert_eq!(error.name, "TypeError", "signature native error name");
                    assert_eq!(
                        error.message, "Cannot convert object to primitive value",
                        "signature native error message"
                    );
                    serde_json::json!({"error":error.name,"message":error.message})
                }
            }
        };
        assert_eq!(
            observed, case["output"],
            "is_cryptocom_signature_rejection native output input {value}"
        );
        executed += 1;
    }
    assert_eq!(
        executed, 37,
        "is_cryptocom_signature_rejection executed fixture count"
    );
}
