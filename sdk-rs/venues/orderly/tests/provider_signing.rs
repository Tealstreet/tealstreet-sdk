//! O1 controls (`docs/plans/RUST_ORDERLY_PROVIDER_READS.md`): the private
//! socket auth and every private read sign through the installed provider.
mod support;
use base64::{Engine as _, engine::general_purpose::URL_SAFE};
use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{Action, Event, Notice, SocketId};
use std::sync::atomic::Ordering;
use support::*;

/// The fixture's Primary seed (`support::options`).
const SEED: [u8; 32] = [7; 32];

/// Answer startup until the private socket connects; returns it.
fn to_private_socket(e: &mut Driver) -> SocketId {
    e.start(1000.0);
    let mut private = None;
    for _ in 0..10 {
        let out = prepared_actions(e, 1000.0);
        if out.is_empty() {
            break;
        }
        for a in out {
            match a {
                Action::Http(r) => {
                    let body = account_body(&r.url, ORDER, "[]", POSITIONS);
                    reply(e, &r, 200, &body);
                }
                Action::WsConnect { socket, url, .. } if url.contains("private") => {
                    private = Some(socket)
                }
                _ => {}
            }
        }
    }
    private.expect("the private socket connects")
}

fn auth_frames(out: &[Action], socket: SocketId) -> Vec<String> {
    out.iter()
        .filter_map(|a| match a {
            Action::WsSend { socket: s, text } if *s == socket && text.contains("\"auth\"") => {
                Some(text.clone())
            }
            _ => None,
        })
        .collect()
}

#[test]
fn the_socket_auth_is_native_exact_and_signed_by_the_provider() {
    let mut e = engine(options());
    let socket = to_private_socket(&mut e);
    e.handle(4321.9, Event::WsOpened { socket });
    let frames = auth_frames(&prepared_actions(&mut e, 4321.9), socket);
    assert_eq!(frames.len(), 1, "one auth frame");
    let raw = &frames[0];
    // Native field order, checked on the raw text (parsed objects sort keys).
    let at = |field: &str| raw.find(&format!("\"{field}\":")).unwrap();
    assert!(at("event") < at("params"));
    assert!(at("orderly_key") < at("sign"));
    assert!(at("sign") < at("timestamp"));
    let frame: serde_json::Value = serde_json::from_str(raw).unwrap();
    assert_eq!(frame["event"], "auth");
    let params = &frame["params"];
    assert_eq!(
        params["orderly_key"],
        "ed25519:11111111111111111111111111111111"
    );
    // The whole-millisecond release stamp, as a JSON number.
    assert_eq!(params["timestamp"].as_u64(), Some(4321));
    // An independent Ed25519 over the stamp's decimal text.
    let expected = URL_SAFE.encode(safe_cex_crypto::ed25519::sign(b"4321", &SEED).unwrap());
    assert_eq!(params["sign"].as_str(), Some(expected.as_str()));
}

#[test]
fn a_provider_refusal_sends_no_auth_frame_and_closes_the_socket() {
    let (mut e, refuse) = refusable_engine();
    let socket = to_private_socket(&mut e);
    refuse.store(true, Ordering::SeqCst);
    e.handle(5000.0, Event::WsOpened { socket });
    let out = prepared_actions(&mut e, 5000.0);
    assert!(auth_frames(&out, socket).is_empty(), "no auth frame");
    assert!(
        out.iter()
            .any(|a| matches!(a, Action::WsClose { socket: s } if *s == socket)),
        "the socket is closed: {out:?}"
    );
}

/// A refused startup read is never silent: it is reported as a failed read and
/// retried natively 5 s later, and nothing unsigned is sent meanwhile.
#[test]
fn a_refused_startup_read_is_reported_and_retried() {
    let (mut e, refuse) = refusable_engine();
    refuse.store(true, Ordering::SeqCst);
    e.start(1000.0);
    let out = prepared_actions(&mut e, 1000.0);
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(out.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::Event(event)) if event.level == safe_cex_engine::Level::Error
    )));
    let retry = out
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 6000.0 => Some(*timer),
            _ => None,
        })
        .expect("the native 5 s startup retry");
    refuse.store(false, Ordering::SeqCst);
    e.handle(6000.0, Event::TimerFired { timer: retry });
    assert!(
        prepared_actions(&mut e, 6000.0)
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/v1/client/info"))),
        "the retried read signs once the provider answers"
    );
}

#[test]
fn every_private_read_signs_at_release_with_the_provider() {
    let mut e = engine(options());
    e.start(1000.0);
    let info = prepared_actions(&mut e, 1234.0)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with("/v1/client/info") => Some(r),
            _ => None,
        })
        .expect("the signed startup read");
    let header = |name: &str| {
        info.headers
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.clone())
            .unwrap()
    };
    assert_eq!(header("orderly-timestamp"), "1234");
    assert_eq!(header("orderly-textsign"), "1234GET/v1/client/info");
    assert_eq!(header("orderly-account-id"), "test-account");
    let expected =
        URL_SAFE.encode(safe_cex_crypto::ed25519::sign(b"1234GET/v1/client/info", &SEED).unwrap());
    assert_eq!(header("orderly-signature"), expected);
}
