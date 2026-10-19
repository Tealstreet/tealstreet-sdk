use safe_cex_engine::Method;
use safe_cex_venue_bingx::sign::{Credentials, Params};
use serde::Deserialize;

fn credentials() -> Credentials {
    Credentials {
        key: "replay-key".into(),
        secret: "replay-secret".into(),
    }
}
fn params() -> Params {
    Params::new()
        .str("type", "LIMIT")
        .str("symbol", "BTC-USDT")
        .num("quantity", 0.01)
        .str("side", "BUY")
        .num("price", 30000.)
}
#[test]
fn sorted_hmac_matches_independent_sha256_vector_and_post_keeps_numbers() {
    let (url, headers, body) = params().into_request(
        Method::Post,
        "/openApi/swap/v2/trade/order",
        Some(&credentials()),
        1700000000000.,
    );
    assert!(!url.contains('?'));
    assert!(headers.contains(&("X-BX-APIKEY", "replay-key".into())));
    #[derive(Deserialize)]
    struct Body {
        signature: String,
        quantity: f64,
        price: f64,
    }
    let body: Body = serde_json::from_str(&body.unwrap()).unwrap();
    assert_eq!(
        body.signature,
        "344193f8520600dcf9591fa0ed776c1b522f6277d01fac57d15dcd664d28f46a"
    );
    assert_eq!((body.quantity, body.price), (0.01, 30000.));
}
#[test]
fn production_listen_keys_are_unsigned_but_keep_the_api_key_header() {
    for method in [Method::Post, Method::Put] {
        let (url, headers, body) = Params::new()
            .str("listenKey", "public-test-key")
            .into_request(
                method,
                "/openApi/user/auth/userDataStream",
                Some(&credentials()),
                1700000000000.,
            );
        assert!(!url.contains("signature"));
        assert!(!body.unwrap_or_default().contains("timestamp"));
        assert!(headers.contains(&("X-BX-APIKEY", "replay-key".into())));
    }
}
