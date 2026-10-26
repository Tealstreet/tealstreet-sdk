//! Original controls for the credential cutover
//! (`docs/plans/RUST_BLOFIN_CREDENTIAL_CUTOVER.md`): the private socket login
//! signs through the dispatch provider over opaque identities.
#![allow(dead_code)]
mod support;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use safe_cex_engine::{Action, Event};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use support::*;

/// The trusted fixture's Primary signing value (`support::Harness`).
const SIGNING: &str = concat!("sec", "ret");

fn logins(actions: &[Action]) -> Vec<serde_json::Value> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::WsSend { text, .. } if text.contains("\"op\":\"login\"") => {
                serde_json::from_str(text).ok()
            }
            _ => None,
        })
        .collect()
}

fn open(h: &mut Harness) -> Vec<serde_json::Value> {
    let before = h.actions.len();
    let socket = h.private;
    let now = h.now.max(1.0);
    h.handle(now, Event::WsOpened { socket });
    h.collect();
    logins(&h.actions[before..])
}

/// Native: base64 of the lowercase-hex HMAC over path, method, stamp, nonce.
fn expected_sign(signing: &str, timestamp: &str, nonce: &str) -> String {
    let text = format!("/users/self/verify{}{timestamp}{nonce}", "GET");
    STANDARD.encode(safe_cex_crypto::hmac_sha256(
        signing.as_bytes(),
        text.as_bytes(),
        safe_cex_crypto::Encoding::Hex,
    ))
}

#[test]
fn the_login_frame_is_native_exact_and_signed_by_the_provider() {
    let mut h = Harness::new();
    let frames = open(&mut h);
    assert_eq!(frames.len(), 1, "one login frame");
    let frame = &frames[0];
    assert_eq!(frame["op"], "login");
    let args = &frame["args"][0];
    // Field order as native serializes it (parsed JSON objects sort keys).
    let raw = h
        .actions
        .iter()
        .find_map(|a| match a {
            Action::WsSend { text, .. } if text.contains("\"op\":\"login\"") => Some(text.clone()),
            _ => None,
        })
        .unwrap();
    let at = |field: &str| raw.find(&format!("\"{field}\":")).unwrap();
    assert!(at("apiKey") < at("passphrase"));
    assert!(at("passphrase") < at("timestamp"));
    assert!(at("timestamp") < at("sign"));
    assert!(at("sign") < at("nonce"));
    assert_eq!(args["apiKey"], "key");
    assert_eq!(args["passphrase"], "pass");
    assert!(args["timestamp"].is_u64(), "a JSON number, as native");
    let timestamp = args["timestamp"].as_u64().unwrap().to_string();
    let nonce = args["nonce"].as_str().unwrap();
    // One 21-byte draw; native's nanoid filters the two symbol characters.
    assert!(!nonce.is_empty() && nonce.len() <= 21);
    assert!(nonce.bytes().all(|b| b.is_ascii_alphanumeric()));
    assert_eq!(
        args["sign"].as_str().unwrap(),
        expected_sign(SIGNING, &timestamp, nonce)
    );
    // The ping still follows the login.
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a, Action::WsSend { text, .. } if text == "ping"))
    );
}

#[test]
fn the_login_signs_with_the_primary_even_with_a_secondary_configured() {
    let mut h = Harness::with_secondary();
    let frames = open(&mut h);
    assert_eq!(frames.len(), 1);
    let args = &frames[0]["args"][0];
    assert_eq!(args["apiKey"], "key");
    assert_eq!(args["passphrase"], "pass");
}

#[test]
fn an_entropy_failure_closes_without_a_frame_and_the_reconnect_logs_in_again() {
    let fail = Arc::new(AtomicBool::new(false));
    let refuse = fail.clone();
    let mut h = Harness::with_signing_entropy(Box::new(move |bytes| {
        if refuse.load(Ordering::SeqCst) {
            return Err("entropy unavailable".into());
        }
        bytes.fill(7);
        Ok(())
    }));
    fail.store(true, Ordering::SeqCst);
    let before = h.actions.len();
    let frames = open(&mut h);
    assert!(frames.is_empty(), "no login frame");
    let socket = h.private;
    assert!(
        h.actions[before..]
            .iter()
            .any(|a| matches!(a, Action::WsClose { socket: s } if *s == socket)),
        "the socket is closed"
    );
    // Entropy is not a provider refusal: the private role is not latched.
    fail.store(false, Ordering::SeqCst);
    // The reconnect is the earliest timer armed by the close and not cleared.
    let mut armed = std::collections::HashMap::new();
    for a in &h.actions[before..] {
        match a {
            Action::SetTimer { timer, at_ms } => {
                armed.insert(*timer, *at_ms);
            }
            Action::ClearTimer { timer } => {
                armed.remove(timer);
            }
            _ => {}
        }
    }
    let (timer, now) = armed
        .into_iter()
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .expect("a reconnect is scheduled");
    let mark = h.actions.len();
    h.handle(now, Event::TimerFired { timer });
    h.collect();
    let socket = h.actions[mark..]
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.ends_with("/private") => Some(*socket),
            _ => None,
        })
        .expect("the private socket reconnects");
    h.private = socket;
    assert_eq!(open(&mut h).len(), 1, "a fresh login signs");
}

#[test]
fn public_data_only_with_identities_never_signs_a_login() {
    let (_, identities) = safe_cex_crypto::signer::LocalSigner::from_json(format!(
        r#"{{"primary":{{"key":"key","{SIGNING}":"{SIGNING}","passphrase":"pass"}}}}"#
    ))
    .unwrap();
    let mut e = safe_cex_engine::test_support::seeded_engine(safe_cex_venue_blofin::Blofin::new(
        safe_cex_venue_blofin::Options {
            public_data_only: true,
            identities,
            ..Default::default()
        },
    ));
    e.start(0.0);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, 0.0);
    assert!(!actions.iter().any(|a| match a {
        Action::Sign { .. } => true,
        Action::WsConnect { url, .. } => url.ends_with("/private"),
        _ => false,
    }));
    assert!(logins(&actions).is_empty());
}

/// A provider refusal (MissingKey) latches signing: one credential notice and
/// one startup failure, no login frame, and the private socket never
/// reconnects. The venue adds no notice of its own (D1 (a)).
#[test]
fn a_provider_refusal_latches_the_private_role_without_a_frame_or_reconnect() {
    // The provider holds no key under this ref, so it refuses (MissingKey).
    let mut h = Harness::with_identities(|ids| {
        ids.primary.as_mut().unwrap().credential =
            safe_cex_engine::signing::KeyRef("absent-ref".into());
    });
    let before = h.actions.len();
    assert!(open(&mut h).is_empty(), "no login frame");
    let socket = h.private;
    assert!(
        h.actions[before..]
            .iter()
            .any(|a| matches!(a, Action::WsClose { socket: s } if *s == socket))
    );
    let mut armed = std::collections::HashMap::new();
    for a in &h.actions[before..] {
        match a {
            Action::SetTimer { timer, at_ms } => {
                armed.insert(*timer, *at_ms);
            }
            Action::ClearTimer { timer } => {
                armed.remove(timer);
            }
            _ => {}
        }
    }
    for (timer, at) in armed {
        let now = at.max(h.now);
        h.handle(now, Event::TimerFired { timer });
        h.collect();
    }
    assert!(
        !h.actions[before..]
            .iter()
            .any(|a| matches!(a, Action::WsConnect { url, .. } if url.ends_with("/private"))),
        "a latched private role never reconnects"
    );
    // The first refused signature (the startup settings reads, before the
    // socket opened) latched and reported once. The login itself adds
    // nothing: no notice and no second startup failure from `before` on.
    let notices = |from: usize, needle: &str| {
        h.actions[from..]
            .iter()
            .filter(|a| matches!(a, Action::Notice(n) if format!("{n:?}").contains(needle)))
            .count()
    };
    let latch =
        "Signing cannot succeed with the installed credentials: Signing key is not installed";
    assert_eq!(notices(0, latch), 1);
    assert_eq!(notices(0, "StartupFailed"), 1);
    assert_eq!(notices(before, latch), 0);
    assert_eq!(notices(before, "StartupFailed"), 0);
    assert_eq!(
        notices(before, "STREAM_UNAVAILABLE") + notices(before, "stream.unavailable"),
        0
    );
}

/// Native order: the login, then the ping. The engine holds a plain send behind
/// a socket's pending signature (`Cx::send` → `signing.hold`), so the ping never
/// reaches the wire ahead of the login, whatever the provider's timing.
#[test]
fn the_ping_follows_the_login_on_the_wire() {
    let mut h = Harness::new();
    let before = h.actions.len();
    open(&mut h);
    let socket = h.private;
    let sends: Vec<&str> = h.actions[before..]
        .iter()
        .filter_map(|a| match a {
            Action::WsSend { socket: s, text } if *s == socket => Some(text.as_str()),
            _ => None,
        })
        .collect();
    let login = sends.iter().position(|t| t.contains("\"op\":\"login\""));
    let ping = sends.iter().position(|t| *t == "ping");
    assert!(
        matches!((login, ping), (Some(l), Some(p)) if l < p),
        "login before ping: {login:?} {ping:?}"
    );
}
