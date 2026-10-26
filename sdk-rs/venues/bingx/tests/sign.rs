use safe_cex_engine::Method;
use safe_cex_venue_bingx::paths;
#[allow(dead_code)]
#[path = "support/oracle.rs"]
mod oracle;
use oracle::{Credentials, Params};
use serde::Deserialize;

fn credentials() -> Credentials {
    Credentials {
        read: None,
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

#[test]
fn secondary_get_keeps_primary_posted_reads_and_mutations() {
    let mut c = credentials();
    c.read = Some(safe_cex_engine::ReadCredentials {
        key: "read-key".into(),
        secret: "read-secret".into(),
        passphrase: None,
    });
    for method in [Method::Get, Method::Post, Method::Put, Method::Delete] {
        let (url, headers, body) = params().into_request(
            method,
            "/openApi/swap/v2/trade/order",
            Some(&c),
            1700000000000.0,
        );
        let expected = if method == Method::Get {
            "read-key"
        } else {
            "replay-key"
        };
        assert!(headers.contains(&("X-BX-APIKEY", expected.into())));
        let expected_credentials = Credentials {
            key: expected.into(),
            secret: if method == Method::Get {
                "read-secret"
            } else {
                "replay-secret"
            }
            .into(),
            read: None,
        };
        let expected_request = params().into_request(
            method,
            "/openApi/swap/v2/trade/order",
            Some(&expected_credentials),
            1700000000000.0,
        );
        assert_eq!((url, headers, body), expected_request);
    }
    let (_, headers, _) = params().into_request(
        Method::Post,
        "/openApi/user/auth/userDataStream",
        Some(&c),
        1700000000000.0,
    );
    assert!(headers.contains(&("X-BX-APIKEY", "replay-key".into())));
}

#[test]
fn invalid_secondary_identity_is_rejected_even_without_a_primary_private_account() {
    use safe_cex_engine::Action;
    use safe_cex_engine::signing::{Identity, KeyRef};
    use safe_cex_venue_bingx::{Bingx, Options};
    for public_data_only in [true, false] {
        let options = Options {
            public_data_only,
            read_credentials: Some(Identity::new(
                KeyRef("ref:owned-read".into()),
                Some(KeyRef(String::new())),
            )),
            ..Options::default()
        };
        let mut engine = safe_cex_engine::test_support::seeded_engine(Bingx::new(options));
        engine.start(1700000000000.0);
        assert!(
            !engine
                .drain_actions()
                .any(|a| matches!(a, Action::Http(_) | Action::WsConnect { .. }))
        );
    }
}
