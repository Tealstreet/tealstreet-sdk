//! Offline native login recovery; synthetic credentials and server-time replies only.
mod support;

use safe_cex_engine::{Event, Method, SocketId};
use serde_json::{Value, json};
use support::Harness;

const TIME: &str = "/api/v2/public/time";
const EXPIRED: &str = r#"{"event":"error","code":"30014","msg":"Timestamp request expired"}"#;

fn opening(uta: bool) -> Harness {
    let mut h = Harness::private_started();
    h.respond(
        "/account/settings",
        200,
        if uta {
            r#"{"code":"00000","data":{"holdMode":"one_way_mode"}}"#
        } else {
            r#"{"code":"40084","msg":"Classic Account mode","data":null}"#
        },
    );
    let socket = private_socket(&h);
    h.event(Event::WsOpened { socket });
    h
}

fn private_socket(h: &Harness) -> SocketId {
    h.sockets
        .iter()
        .rev()
        .find(|(_, u)| u.ends_with("/private"))
        .unwrap()
        .0
}

fn reconnect(h: &mut Harness) -> SocketId {
    let previous = private_socket(h);
    for _ in 0..100 {
        if private_socket(h) != previous {
            let socket = private_socket(h);
            h.event(Event::WsOpened { socket });
            return socket;
        }
        let (timer, at) = h
            .timers
            .iter()
            .copied()
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap();
        h.timers.retain(|(id, _)| *id != timer);
        h.now = h.now.max(at);
        h.event(Event::TimerFired { timer });
    }
    panic!("private reconnect did not occur");
}

fn logins(h: &Harness, socket: SocketId) -> Vec<Value> {
    h.sent
        .iter()
        .filter(|(id, _)| *id == socket)
        .filter_map(|(_, text)| serde_json::from_str::<Value>(text).ok())
        .filter(|payload| payload["op"] == "login")
        .collect()
}

fn time_reply(server: f64) -> String {
    json!({"code":"00000", "data":{"serverTime":(server as u64).to_string()}}).to_string()
}

fn assert_stamp(h: &Harness, socket: SocketId, expected_ms: f64) {
    let packets = logins(h, socket);
    assert_eq!(packets.len(), 1);
    let login = &packets[0]["args"][0];
    let seconds = ((expected_ms / 1000.).floor() as u64).to_string();
    assert_eq!(login["timestamp"], seconds);
    assert_eq!(
        login["sign"],
        safe_cex_crypto::hmac_sha256(
            b"secret",
            format!("{seconds}GET/user/verify").as_bytes(),
            safe_cex_crypto::Encoding::Base64,
        )
    );
}

#[test]
fn expired_login_reads_one_unsigned_clock_and_holds_the_next_login_until_correction() {
    for uta in [false, true] {
        let mut h = opening(uta);
        h.private_message(EXPIRED);
        let request = h.take(TIME);
        assert_eq!(request.method, Method::Get);
        assert_eq!(request.url, format!("https://api.bitget.com{TIME}"));
        assert_eq!(
            request.headers,
            [("X-CHANNEL-API-CODE", "Tealstreet".into())]
        );
        let socket = reconnect(&mut h);
        assert!(
            logins(&h, socket).is_empty(),
            "an unresolved correction must not reuse the bad clock"
        );
        let server = (h.now + 90_000.).floor();
        h.reply(request.id, 200, &time_reply(server));
        assert_stamp(&h, socket, server);
        assert_eq!(h.engine.signing_clock(h.now).0, server);
        assert!(h.requests.iter().all(|r| !r.url.contains(TIME)));
        assert!(h.requests.iter().all(|r| !r.url.contains("/place-order")));
        assert!(h.startup_failures.is_empty());
    }
}

#[test]
fn a_delayed_reconnect_signs_the_corrected_send_time_not_the_clock_sample_time() {
    let mut h = opening(false);
    h.private_message(EXPIRED);
    let server = (h.now + 90_000.).floor();
    h.respond(TIME, 200, &time_reply(server));
    h.now += 35_000.;
    let socket = reconnect(&mut h);
    assert_stamp(&h, socket, server + 35_000.);
}

#[test]
fn exact_numeric_and_string_login_expiry_share_one_inflight_sample() {
    for event in ["error", "login"] {
        for code in ["30014", r#""30014""#] {
            let mut h = opening(false);
            h.private_message(&format!(
                r#"{{"event":"{event}","code":{code},"msg":"Timestamp request expired"}}"#
            ));
            let _request = h.take(TIME);
            let _waiting = reconnect(&mut h);
            h.private_message(EXPIRED);
            assert!(h.requests.iter().all(|r| !r.url.contains(TIME)));
        }
    }
}

#[test]
fn failed_or_malformed_clock_samples_resume_with_the_existing_clock() {
    for (status, body) in [
        (503, r#"{"code":"00000","data":{"serverTime":"9000000"}}"#),
        (200, r#"{"code":"40008","data":{"serverTime":"9000000"}}"#),
        (200, r#"{"code":"00000","data":{"serverTime":"0"}}"#),
        (200, r#"{"code":"00000","data":{"serverTime":"NaN"}}"#),
        (200, r#"{"code":"00000","data":null}"#),
        (200, r#"{"code":"00000","data":{"serverTime":"-1"}}"#),
        (200, r#"{"code":"00000","data":{"serverTime":"1000000.5"}}"#),
        (
            200,
            r#"{"code":"00000","data":{"serverTime":"9007199254740992"}}"#,
        ),
        (200, r#"{"code":"00000","data":{"serverTime":"1e100"}}"#),
        (
            200,
            r#"{"code":"00000","code":"40008","data":{"serverTime":"9000000"}}"#,
        ),
    ] {
        let mut h = opening(false);
        h.private_message(EXPIRED);
        let request = h.take(TIME);
        let socket = reconnect(&mut h);
        h.reply(request.id, status, body);
        assert_stamp(&h, socket, h.now);
        assert_eq!(h.engine.signing_clock(h.now).0, h.now);
    }
}

#[test]
fn failed_clock_transport_does_not_block_login_or_remove_the_next_backoff_sample() {
    let mut h = opening(false);
    h.private_message(EXPIRED);
    let request = h.take(TIME);
    let socket = reconnect(&mut h);
    h.event(Event::HttpFailed {
        request: request.id,
        error: "synthetic clock transport failure",
    });
    assert_stamp(&h, socket, h.now);
    h.private_message(EXPIRED);
    let next = h.take(TIME);
    assert_ne!(next.id, request.id);
    h.reply(request.id, 200, &time_reply(9_000_000.));
    assert_eq!(
        h.engine.signing_clock(h.now).0,
        h.now,
        "a late retired read cannot correct the clock"
    );
}

#[test]
fn a_clock_sample_expires_within_five_seconds_and_ignores_its_late_reply() {
    let mut h = opening(false);
    let start = h.now;
    h.private_message(EXPIRED);
    let request = h.take(TIME);
    let socket = reconnect(&mut h);
    let deadline = start + 5_000.;
    let mut fired = 0;
    while let Some((timer, at)) = h
        .timers
        .iter()
        .copied()
        .filter(|(_, at)| *at <= deadline)
        .min_by(|a, b| a.1.total_cmp(&b.1))
    {
        h.timers.retain(|(id, _)| *id != timer);
        h.now = h.now.max(at);
        h.event(Event::TimerFired { timer });
        fired += 1;
    }
    assert!(fired > 1, "heartbeat and HTTP deadlines may coincide");
    assert_eq!(h.now, deadline);
    assert_stamp(&h, socket, deadline);
    h.reply(request.id, 200, &time_reply(9_000_000.));
    assert_eq!(h.engine.signing_clock(h.now).0, h.now);
}

#[test]
fn a_clock_reply_never_sends_login_to_a_retired_waiting_socket() {
    let mut h = opening(false);
    h.private_message(EXPIRED);
    let request = h.take(TIME);
    let waiting = reconnect(&mut h);
    h.event(Event::WsClosed {
        socket: waiting,
        code: 1006,
    });
    h.reply(request.id, 200, &time_reply(h.now + 90_000.));
    assert!(logins(&h, waiting).is_empty());
    let active = reconnect(&mut h);
    assert_ne!(active, waiting);
    assert_stamp(&h, active, h.engine.signing_clock(h.now).0);
}

#[test]
fn unrelated_noncanonical_and_post_login_refusals_do_not_sample_the_clock() {
    for code in [
        "30013",
        "30014.000000000000001",
        r#""30014junk""#,
        "30014,\"code\":30013",
    ] {
        let mut h = opening(false);
        h.private_message(&format!(
            r#"{{"event":"error","code":{code},"msg":"Timestamp request expired"}}"#
        ));
        assert!(h.requests.iter().all(|r| !r.url.contains(TIME)), "{code}");
    }
    let mut h = Harness::classic();
    h.private_message(EXPIRED);
    assert!(h.requests.iter().all(|r| !r.url.contains(TIME)));
}
