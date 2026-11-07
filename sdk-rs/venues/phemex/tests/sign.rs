//! Fixed HMAC vectors generated independently with Python hashlib/hmac.
#[path = "support/native_sign.rs"]
mod sign;
use sign::Credentials;
#[test]
fn rest_path_query_body_and_expiry_match_independent_vectors() {
    let c = Credentials {
        key: "key".into(),
        secret: "secret".into(),
    };
    let headers = c.headers(
        "/g-accounts/accountPositions",
        "currency=USDT",
        "",
        1_700_000_000_000.,
    );
    assert_eq!(headers[1].1, "1700000060");
    assert_eq!(
        headers[2].1,
        "f5c2373fe7fd936754713178bcd405a0f810fb23b2ff086a50529815189a036f"
    );
    let headers = c.headers(
        "/g-orders/cancel",
        "symbol=BTCUSDT&orderID=caller%2Fa%2Cb%20+",
        "",
        1_700_000_000_500.,
    );
    assert_eq!(headers[1].1, "1700000061");
    assert_eq!(
        headers[2].1,
        "94637541060fb9a62cfe1f57bd3b900136c45056e39946f0cb3d62d4e8d28159"
    );
    let headers = c.headers(
        "/g-orders/create",
        "",
        r#"{"symbol":"BTCUSDT","clOrdID":"a/b"}"#,
        1_700_000_000_000.,
    );
    assert_eq!(
        headers[2].1,
        "c71903ea2be3eb4bbb67d46bb836b2ac377a5dcb0ed01632735d3d8e6210029b"
    );
}
#[test]
fn websocket_key_expiry_signature_is_distinct_from_rest() {
    let c = Credentials {
        key: "key".into(),
        secret: "secret".into(),
    };
    assert_eq!(
        c.websocket_auth(1_700_000_000_000., 999),
        r#"{"method":"user.auth","params":["API","key","46a48d7b73f809f511182a4807f8301321bf3efe6f397c40f50b393e649a1419",1700000060],"id":999}"#
    );
    assert_ne!(
        c.websocket_auth(1_700_000_000_000., 999),
        c.websocket_auth(1_700_000_060_000., 999)
    );
}
