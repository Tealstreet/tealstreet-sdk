//! Binance's HMAC bytes, proved through the dispatch recipe and the installed
//! provider: the documented vector, and every configured receive window.
mod support;
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::{Action, Command, Event, HttpRequest};
use safe_cex_venue_binance::Variant;
use support::Options;

const NOW: f64 = 1000.;

#[derive(serde::Deserialize)]
struct Vector {
    #[serde(rename = "secret")]
    seed: String,
    payload: String,
    signature: String,
}

fn vector() -> Vector {
    serde_json::from_str(include_str!(
        "../../../crypto/tests/fixtures/hmac/binance.json"
    ))
    .unwrap()
}

/// One provider-signed identity read (FetchAccount) from `options`.
fn signed_read(options: Options) -> HttpRequest {
    let mut engine = support::engine(options);
    engine.store_mut().set_orders(vec![]);
    engine.store_mut().set_positions(vec![]);
    engine.handle(NOW, Event::Command(Command::FetchAccount { command: 7 }));
    safe_cex_engine::test_support::prepared_actions(&mut engine, NOW)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .expect("the signed read")
}

fn keyed(seed: &str, recv_window: Option<f64>) -> Options {
    let mut options = support::keyed(Variant::UsdM, "vector-key", seed);
    options.recv_window = recv_window;
    options
}

/// developers.binance.com, "SIGNED Endpoint Examples for POST /fapi/v1/order":
/// the primitive reproduces the documented signature, and the dispatch path
/// signs its exact query with that same key under the provider.
#[test]
fn the_documented_vector_signs_through_the_provider() {
    let vector = vector();
    assert_eq!(
        hmac_sha256(
            vector.seed.as_bytes(),
            vector.payload.as_bytes(),
            Encoding::Hex
        ),
        vector.signature
    );
    let request = signed_read(keyed(&vector.seed, None));
    let (_, query) = request.url.split_once('?').unwrap();
    let (payload, signature) = query.rsplit_once("&signature=").unwrap();
    assert_eq!(payload, "timestamp=1000&recvWindow=20000");
    assert_eq!(
        signature,
        hmac_sha256(vector.seed.as_bytes(), payload.as_bytes(), Encoding::Hex)
    );
}

/// The configured receive window is signed as given, supported values
/// unchanged; absent, it is native's 20 s.
#[test]
fn configured_receive_windows_are_signed_without_changing_supported_values() {
    for (window, expected) in [
        (None, "20000"),
        (Some(500.0), "500"),
        (Some(0.0), "0"),
        (Some(500.125), "500.125"),
        (Some(60_000.0), "60000"),
    ] {
        let request = signed_read(keyed("s", window));
        let (_, query) = request.url.split_once('?').unwrap();
        let (payload, signature) = query.rsplit_once("&signature=").unwrap();
        assert_eq!(payload, format!("timestamp=1000&recvWindow={expected}"));
        assert_eq!(
            signature,
            hmac_sha256(b"s", payload.as_bytes(), Encoding::Hex)
        );
    }
}
