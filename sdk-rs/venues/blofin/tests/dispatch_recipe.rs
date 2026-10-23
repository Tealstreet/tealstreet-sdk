//! S1 component oracles: the dispatch recipe through the injected provider,
//! byte-for-byte against the legacy signer. This is not production adoption.
#[path = "../src/sign/dispatch.rs"]
mod dispatch_signing;
#[allow(dead_code)] // Shared across suites; this one needs only the seeded harness.
mod support;

use base64::Engine as _;
use dispatch_signing::{Http, Login, UNSIGNED_HEADERS, VERIFY_PATH};
use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::book::Book;
use safe_cex_engine::signing::{Identities, Identity, Operation, PayloadPart, SignError};
use safe_cex_engine::{
    Action, Command, Cx, Engine, Event, HttpRequest, Method, ReadCredentials, Response, SocketId,
    TimerId, Venue, VenueError,
};
use safe_cex_venue_blofin::{Blofin, Options};

/// A fractional protocol instant: both signers must floor it to the same ms.
const NOW: f64 = 1_700_000_000_123.875;
const ORIGIN: &str = "wss://openapi.blofin.com/ws/private";
const INSTRUMENTS: &str = r#"{"code":"0","data":[{"instId":"BTC-USDT","instType":"SWAP","contractType":"linear","baseCurrency":"BTC","quoteCurrency":"USDT","state":"live","contractValue":"0.001","minSize":"0.1","lotSize":"0.1","tickSize":"0.1","maxLimitSize":"1000","maxMarketSize":"500","maxLeverage":"150"}]}"#;
/// `fill` through the 21-byte nanoid mapping; one `-`/`_` byte is filtered out.
const NONCE: &str = "aMiCq7f8FUeYrAlXb9Lm";

/// Constant signing entropy, so every draw by either signer yields `NONCE`.
fn fill(bytes: &mut [u8]) -> Result<(), String> {
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = (index as u8).wrapping_mul(29).wrapping_add(3);
    }
    Ok(())
}

/// The legacy primitive (crypto HMAC hex, then base64), outside the provider path.
fn independent(secret: &str, text: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(safe_cex_crypto::hmac_sha256(
        secret.as_bytes(),
        text.as_bytes(),
        safe_cex_crypto::Encoding::Hex,
    ))
}

fn provider(primary: (&str, &str, &str)) -> (LocalSigner, Identities) {
    LocalSigner::from_json(
        serde_json::json!({
            "primary": {"key": primary.0, "secret": primary.1, "passphrase": primary.2},
            "secondary": {"key": "secondary-key", "secret": "secondary-secret", "passphrase": "secondary-pass"},
        })
        .to_string(),
    )
    .unwrap()
}

enum Shape {
    Http {
        identity: Identity,
        method: Method,
        url: String,
        path: String,
        sent_body: Option<String>,
        signed_body: Option<String>,
    },
    Login(Identity),
}
struct Probe {
    shape: Shape,
    sign_errors: Vec<VenueError>,
    closed: Vec<u16>,
}
impl Venue for Probe {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        match &self.shape {
            Shape::Http {
                identity,
                method,
                url,
                path,
                sent_body,
                signed_body,
            } => {
                let headers = UNSIGNED_HEADERS
                    .iter()
                    .map(|&(name, value)| (name, value.to_owned()))
                    .collect();
                let id = cx.request(*method, url.clone(), headers, sent_body.clone(), ());
                cx.sign_http(
                    id,
                    Box::new(Http::new(
                        identity.clone(),
                        *method,
                        path.clone(),
                        signed_body.clone(),
                    )),
                );
            }
            Shape::Login(identity) => {
                let socket = cx.connect(ORIGIN);
                cx.send_signed(
                    socket,
                    String::new(),
                    Box::new(Login::new(identity.clone(), ORIGIN.into())),
                );
                cx.send(socket, "ping");
            }
        }
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, code: u16) {
        self.closed.push(code);
    }
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_sign_error(&mut self, _: &mut Cx<'_, ()>, _: (), error: VenueError) {
        self.sign_errors.push(error);
    }
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn http(identity: Identity, method: Method, url: &str, body: Option<&str>) -> Shape {
    let path_start = url.find("://").unwrap() + 3;
    let path_start = path_start + url[path_start..].find('/').unwrap();
    Shape::Http {
        identity,
        method,
        url: url.into(),
        path: url[path_start..].into(),
        sent_body: body.map(Into::into),
        signed_body: body.map(Into::into),
    }
}

fn staged(
    shape: Shape,
    entropy: safe_cex_engine::entropy::Fill,
    now: f64,
) -> (Engine<Probe>, Vec<Action>) {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Probe {
        shape,
        sign_errors: vec![],
        closed: vec![],
    });
    engine.set_signing_entropy(entropy).unwrap();
    engine.start(now - 1_000.);
    engine.prepare_dispatch(now);
    let actions = engine.drain_actions().collect();
    (engine, actions)
}

/// Prepares at `now`, signs with `provider`, and returns the work and the output.
fn signed(
    shape: Shape,
    provider: &LocalSigner,
    now: f64,
) -> (safe_cex_engine::signing::SignRequest, Vec<Action>) {
    let (mut engine, actions) = staged(shape, Box::new(fill), now);
    let (attempt, work) = actions
        .into_iter()
        .find_map(|action| match action {
            Action::Sign { attempt, work, .. } => Some((attempt, work)),
            _ => None,
        })
        .expect("recipe staged provider work");
    let result = provider.sign_now(&work).unwrap();
    engine.handle(
        now,
        Event::Signed {
            attempt,
            result: Ok(result),
        },
    );
    (work, engine.drain_actions().collect())
}

fn sent_http(actions: &[Action]) -> &HttpRequest {
    actions
        .iter()
        .find_map(|action| match action {
            Action::Http(http) => Some(http),
            _ => None,
        })
        .expect("signed request released")
}

fn bytes(text: &str) -> PayloadPart {
    PayloadPart::Bytes {
        bytes: text.as_bytes().to_vec(),
    }
}

/// Legacy startup: the margin-mode GET signed by `account::headers` at `NOW`.
fn legacy_margin_mode(options: Options) -> HttpRequest {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Blofin::new(options));
    engine.set_signing_entropy(Box::new(fill)).unwrap();
    engine.start(NOW - 1_000.);
    let instruments = safe_cex_engine::test_support::prepared_actions(&mut engine, NOW - 1_000.)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(http) if http.url.contains("instruments") => Some(http),
            _ => None,
        })
        .unwrap();
    engine.handle(
        NOW,
        Event::HttpResponse {
            request: instruments.id,
            response: Response {
                status: 200,
                headers: "",
                body: INSTRUMENTS,
            },
        },
    );
    safe_cex_engine::test_support::prepared_actions(&mut engine, NOW)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(http) if http.url.ends_with("/api/v1/account/margin-mode") => Some(http),
            _ => None,
        })
        .expect("legacy margin-mode read")
}

fn legacy_options(read_passphrase: Option<&str>, read: bool) -> Options {
    Options {
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        passphrase: Some("primary-pass".into()),
        read_credentials: read.then(|| ReadCredentials {
            key: "secondary-key".into(),
            secret: "secondary-secret".into(),
            passphrase: read_passphrase.map(Into::into),
        }),
        ..Options::default()
    }
}

#[test]
fn legacy_fixed_vector_through_the_provider_including_the_nonce_filter() {
    // `sign.rs` pins ("secret", "/test", "POST", "1", "n", "{}"). Byte 0 maps to
    // `n`; the other twenty map to `-`/`_` and are filtered out.
    let entropy: safe_cex_engine::entropy::Fill = Box::new(|bytes| {
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = match index {
                0 => 4,
                odd if odd % 2 == 1 => 8,
                _ => 44,
            };
        }
        Ok(())
    });
    let (provider, identities) = provider(("key", "secret", "pass"));
    let shape = http(
        identities.primary.unwrap(),
        Method::Post,
        "https://openapi.blofin.com/test",
        Some("{}"),
    );
    let (mut engine, actions) = staged(shape, entropy, 1.);
    let (attempt, work) = actions
        .into_iter()
        .find_map(|action| match action {
            Action::Sign { attempt, work, .. } => Some((attempt, work)),
            _ => None,
        })
        .unwrap();
    assert_eq!(work.receipt.stamp_ms, Some(1.));
    assert_eq!(work.context.host, "openapi.blofin.com");
    assert!(
        matches!(&work.operations[0], Operation::Present { key } if key.0 == "primary.credential")
    );
    assert!(matches!(
        &work.operations[1],
        Operation::Sign { key, payload, .. }
            if key.0 == "primary.signing"
                && *payload == vec![bytes("/test"), bytes("POST"), bytes("1"), bytes("n"), bytes("{}")]
    ));
    assert!(
        matches!(&work.operations[2], Operation::Present { key } if key.0 == "primary.passphrase")
    );
    let result = provider.sign_now(&work).unwrap();
    engine.handle(
        1.,
        Event::Signed {
            attempt,
            result: Ok(result),
        },
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    assert_eq!(
        sent_http(&actions).headers,
        vec![
            ("ACCESS-KEY", "key".into()),
            (
                "ACCESS-SIGN",
                "ZmIwNzhmM2MzZDRjMmI2Y2M3ZjlkZWEwMDUwNGE5ZDFiMTY3MzcwZjMyODliOTgxYjZjMTA4OTAzNTJmYjJkZA=="
                    .into()
            ),
            ("ACCESS-NONCE", "n".into()),
            ("ACCESS-TIMESTAMP", "1".into()),
            ("ACCESS-PASSPHRASE", "pass".into()),
            ("Content-Type", "application/json".into()),
        ]
    );
}

#[test]
fn get_on_the_secondary_matches_legacy_headers_byte_for_byte() {
    let (provider, identities) = provider(("primary-key", "primary-secret", "primary-pass"));
    let legacy = legacy_margin_mode(legacy_options(Some("secondary-pass"), true));
    let (work, actions) = signed(
        http(
            identities.secondary.unwrap(),
            Method::Get,
            &legacy.url,
            None,
        ),
        &provider,
        NOW,
    );
    assert_eq!(work.receipt.stamp_ms, Some(1_700_000_000_123.));
    let native = sent_http(&actions);
    assert_eq!(native.headers, legacy.headers);
    assert_eq!(
        (native.method, &native.url, &native.body),
        (legacy.method, &legacy.url, &None)
    );
    assert_eq!(
        native.headers,
        vec![
            ("ACCESS-KEY", "secondary-key".into()),
            (
                "ACCESS-SIGN",
                // Independent: Python hmac/hashlib/base64 over the same preimage.
                "MTNjOTVlZDIwOTRjNDQwYWU1MTI3NTI3NWQwOWI0OTIyM2ZhMmIzZGExMmZmZjA2OTFjOGU0MjlmNTAxNzgzZQ=="
                    .into()
            ),
            ("ACCESS-NONCE", NONCE.into()),
            ("ACCESS-TIMESTAMP", "1700000000123".into()),
            ("ACCESS-PASSPHRASE", "secondary-pass".into()),
            ("Content-Type", "application/json".into()),
        ]
    );
}

#[test]
fn get_without_a_secondary_and_inherited_secondary_passphrase_match_legacy() {
    let (provider, identities) = provider(("primary-key", "primary-secret", "primary-pass"));
    let primary = identities.primary.unwrap();
    let secondary = identities.secondary.unwrap();
    // Setup policy, not this recipe, resolves an absent secondary passphrase.
    let inherited = Identity {
        passphrase: primary.passphrase.clone(),
        ..secondary
    };
    for (options, identity, key, passphrase) in [
        (
            legacy_options(None, false),
            primary,
            "primary-key",
            "primary-pass",
        ),
        (
            legacy_options(None, true),
            inherited,
            "secondary-key",
            "primary-pass",
        ),
    ] {
        let legacy = legacy_margin_mode(options);
        let (_, actions) = signed(
            http(identity, Method::Get, &legacy.url, None),
            &provider,
            NOW,
        );
        let native = sent_http(&actions);
        assert_eq!(native.headers, legacy.headers);
        assert_eq!(native.headers[0], ("ACCESS-KEY", key.into()));
        assert_eq!(native.headers[4], ("ACCESS-PASSPHRASE", passphrase.into()));
    }
}

#[test]
fn post_on_the_primary_matches_legacy_batch_placement_byte_for_byte() {
    let mut h = support::Harness::with_signing_entropy(Box::new(fill));
    h.seed();
    let orders = serde_json::from_str(
        r#"[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":60000,"clientOrderId":"Caller1"}]"#,
    )
    .unwrap();
    h.e.handle(
        NOW,
        Event::Command(Command::PlaceOrders { command: 7, orders }),
    );
    h.collect();
    let legacy = h.take("batch-orders");
    let body = legacy.body.clone().unwrap();
    let (provider, identities) = provider(("key", "secret", "pass"));
    let (work, actions) = signed(
        http(
            identities.primary.unwrap(),
            Method::Post,
            &legacy.url,
            Some(&body),
        ),
        &provider,
        NOW,
    );
    assert!(
        matches!(&work.operations[1], Operation::Sign { key, .. } if key.0 == "primary.signing")
    );
    let native = sent_http(&actions);
    assert_eq!(native.headers, legacy.headers);
    assert_eq!(native.body, legacy.body);
    assert_eq!(
        native.headers[1].1,
        independent(
            "secret",
            &format!("/api/v1/trade/batch-ordersPOST1700000000123{NONCE}{body}")
        )
    );
}

#[test]
fn ws_login_matches_legacy_text_and_precedes_ping() {
    let mut h = support::Harness::with_signing_entropy(Box::new(fill));
    h.e.handle(NOW, Event::WsOpened { socket: h.private });
    h.collect();
    let legacy = h
        .actions
        .iter()
        .find_map(|action| match action {
            Action::WsSend { text, .. } if text.contains("\"op\":\"login\"") => Some(text.clone()),
            _ => None,
        })
        .unwrap();
    let (provider, identities) = provider(("key", "secret", "pass"));
    let (work, actions) = signed(Shape::Login(identities.primary.unwrap()), &provider, NOW);
    assert_eq!(work.receipt.stamp_ms, Some(1_700_000_000_123.));
    assert_eq!(work.context.path, VERIFY_PATH);
    let sent: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::WsSend { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(sent, [legacy.as_str(), "ping"]);
    let sign = independent("secret", &format!("{VERIFY_PATH}GET1700000000123{NONCE}"));
    assert_eq!(
        legacy,
        format!(
            r#"{{"op":"login","args":[{{"apiKey":"key","passphrase":"pass","timestamp":1700000000123,"sign":"{sign}","nonce":"{NONCE}"}}]}}"#
        )
    );
}

#[test]
fn ws_login_independent_vector_on_distinct_primary_material() {
    let (provider, identities) = provider(("primary-key", "primary-secret", "primary-pass"));
    let (_, actions) = signed(Shape::Login(identities.primary.unwrap()), &provider, NOW);
    let login = actions
        .iter()
        .find_map(|action| match action {
            Action::WsSend { text, .. } if text != "ping" => Some(text.as_str()),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        login,
        format!(
            // Independent: Python hmac/hashlib/base64 over the same preimage.
            r#"{{"op":"login","args":[{{"apiKey":"primary-key","passphrase":"primary-pass","timestamp":1700000000123,"sign":"ZGE0YzQ0MWE5NGI0NDFiOTQ3Yjc2ZjBjMzU3N2JhZDg5ODMyNTE0OTc4ZmU4N2MyMzc2OGNjMDk0ZGEyYTQzMA==","nonce":"{NONCE}"}}]}}"#
        )
    );
}

#[test]
fn entropy_failure_is_a_typed_sign_error_before_provider_or_transport() {
    let (_, identities) = provider(("key", "secret", "pass"));
    let primary = identities.primary.unwrap();
    let refuse = || -> safe_cex_engine::entropy::Fill { Box::new(|_| Err("no entropy".into())) };
    let (engine, actions) = staged(
        http(
            primary.clone(),
            Method::Post,
            "https://openapi.blofin.com/api/v1/trade/batch-orders",
            Some("[]"),
        ),
        refuse(),
        NOW,
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::Sign { .. }))
    );
    assert_eq!(
        engine.venue().sign_errors,
        [SignError::Unavailable.venue_error()]
    );
    assert_eq!(
        engine.venue().sign_errors[0].outcome,
        Some(safe_cex_engine::Outcome::NotSent)
    );

    let (engine, actions) = staged(Shape::Login(primary), refuse(), NOW);
    assert!(!actions.iter().any(|a| matches!(a, Action::Sign { .. })));
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::WsSend { text, .. } if text.contains("login")))
    );
    assert!(actions.iter().any(|a| matches!(a, Action::WsClose { .. })));
    assert_eq!(engine.venue().closed, [1006]);
}

#[test]
fn unsigned_intent_mismatch_or_missing_passphrase_never_reaches_the_provider() {
    let (_, identities) = provider(("key", "secret", "pass"));
    let primary = identities.primary.unwrap();
    let url = "https://openapi.blofin.com/api/v1/trade/batch-orders";
    let Shape::Http {
        identity,
        method,
        url: full,
        path,
        ..
    } = http(primary.clone(), Method::Post, url, Some("[1]"))
    else {
        unreachable!()
    };
    let altered = Shape::Http {
        identity,
        method,
        url: full,
        path,
        sent_body: Some("[2]".into()),
        signed_body: Some("[1]".into()),
    };
    let no_passphrase = Identity {
        passphrase: None,
        ..primary
    };
    for shape in [altered, http(no_passphrase, Method::Post, url, Some("[1]"))] {
        let (engine, actions) = staged(shape, Box::new(fill), NOW);
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Http(_) | Action::Sign { .. }))
        );
        assert_eq!(
            engine.venue().sign_errors,
            [SignError::InvalidRequest.venue_error()]
        );
    }
}
