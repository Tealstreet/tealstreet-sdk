//! Private WS auth stamps `expires` on Bybit's clock, not a slow host's.
mod support;
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::Event;
use safe_cex_venue_bybit::{Endpoints, Variant};
use support::{Harness, Options, untimed};

/// The host clock runs this far behind Bybit's.
const HOST_SLOW_MS: f64 = 30_000.0;
const REFUSED: &str =
    r#"{"success":false,"ret_msg":"Params Error","op":"auth","conn_id":"cejreaspqfh3sjdnldmg-p"}"#;

fn stamped(body: &str, time: f64) -> String {
    let body = untimed(body);
    format!("{},\"time\":{time}}}", &body[..body.len() - 1])
}

fn market_time(time: f64) -> String {
    format!(
        r#"{{"retCode":0,"retMsg":"OK","result":{{"timeSecond":"{}","timeNano":"{}000000"}},"retExtInfo":{{}},"time":{time}}}"#,
        (time / 1000.0).floor(),
        time
    )
}

/// Startup answered on the host's own clock, or every answer stamped `time`.
fn startup(variant: Variant, time: Option<f64>) -> Harness {
    let mut h = Harness::started(Options {
        variant,
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        endpoints: variant.is_hyro().then(|| Endpoints {
            rest: "https://hyro.example".into(),
            public_ws: "wss://hyro.example/v5/public/linear".into(),
            private_ws: "wss://hyro.example/v5/private".into(),
        }),
        ..Options::default()
    });
    let answer = |body: &str| match time {
        Some(time) => stamped(body, time),
        None => untimed(body),
    };
    if h.has_request("/v5/account/fee-rate") {
        h.respond("/v5/account/fee-rate", 200, &answer(support::FEE_RATES));
    }
    if h.has_request("/v5/account/info") {
        h.respond("/v5/account/info", 200, &answer(support::ACCOUNT_INFO));
    }
    if h.has_request("status=PreLaunch") {
        h.respond("status=PreLaunch", 200, &answer(support::PRELAUNCH));
    }
    let (instruments, tickers) = if variant.is_inverse() {
        (
            support::INSTRUMENTS
                .replace("USDT", "USD")
                .replace("LinearPerpetual", "InversePerpetual"),
            support::TICKERS.replace("USDT", "USD"),
        )
    } else {
        (support::INSTRUMENTS.to_owned(), support::TICKERS.to_owned())
    };
    h.respond("/v5/market/instruments-info", 200, &answer(&instruments));
    h.respond("/v5/market/tickers", 200, &answer(&tickers));
    h
}

fn time_reads(h: &Harness) -> usize {
    h.requests
        .iter()
        .filter(|r| r.url.contains("/v5/market/time"))
        .count()
}

fn open_private(h: &mut Harness) -> safe_cex_engine::SocketId {
    let socket = h.socket("/v5/private").expect("private socket");
    h.event(Event::WsOpened { socket });
    socket
}

/// The `expires` of the only auth sent on the newest private socket.
fn auth_expires(h: &Harness) -> f64 {
    let sent = h.sent_on("/v5/private");
    assert_eq!(sent.len(), 1, "one auth per socket: {sent:?}");
    #[derive(serde::Deserialize)]
    struct Auth {
        op: String,
        args: (String, String, String),
    }
    let auth: Auth = serde_json::from_str(&sent[0]).unwrap();
    assert_eq!(auth.op, "auth");
    let signature = hmac_sha256(
        b"secret",
        format!("GET/realtime{}", auth.args.1).as_bytes(),
        Encoding::Hex,
    );
    assert_eq!(auth.args.2, signature, "the signed expires is the sent one");
    auth.args.1.parse().unwrap()
}

/// Refuse the first auth; the reconnect is due a second later.
fn refuse_first_auth(h: &mut Harness) -> safe_cex_engine::SocketId {
    let socket = open_private(h);
    assert_eq!(auth_expires(h), h.now + 10_000.0, "uncalibrated first auth");
    h.requests.clear();
    h.message("/v5/private", REFUSED);
    assert!(h.closed.contains(&socket));
    socket
}

fn reconnect(h: &mut Harness, refused: safe_cex_engine::SocketId) {
    h.advance(1_000.0);
    assert_ne!(h.socket("/v5/private"), Some(refused), "reconnected");
    open_private(h);
}

#[test]
fn an_auth_refusal_reads_bybit_time_once_and_reauths_on_its_clock() {
    for (variant, origin) in [
        (Variant::Linear, "https://api.bybit.com"),
        (Variant::Demo, "https://api-demo.bybit.com"),
        (Variant::Inverse, "https://api.bybit.com"),
        (Variant::Spot, "https://api.bybit.com"),
    ] {
        let mut h = startup(variant, None);
        let refused = refuse_first_auth(&mut h);
        assert_eq!(time_reads(&h), 1, "{variant:?}: one time read per refusal");
        let read = h.take("/v5/market/time");
        assert_eq!(read.url, format!("{origin}/v5/market/time"), "{variant:?}");
        assert_eq!(read.method, safe_cex_engine::Method::Get);
        assert!(Harness::header(&read, "X-BAPI-SIGN").is_none(), "unsigned");
        let server = h.now + HOST_SLOW_MS;
        h.reply(read.id, 200, &market_time(server));
        reconnect(&mut h, refused);
        assert_eq!(
            auth_expires(&h),
            h.now + HOST_SLOW_MS + 10_000.0,
            "{variant:?}: re-auth on Bybit's clock"
        );
        assert_eq!(time_reads(&h), 0, "{variant:?}: no further time read");
    }
}

#[test]
fn the_reauth_waits_for_a_time_read_still_in_flight() {
    let mut h = startup(Variant::Linear, None);
    let refused = refuse_first_auth(&mut h);
    let read = h.take("/v5/market/time");
    reconnect(&mut h, refused);
    assert!(
        h.sent_on("/v5/private").is_empty(),
        "auth waits for the clock"
    );
    h.reply(read.id, 200, &market_time(h.now + HOST_SLOW_MS));
    assert_eq!(auth_expires(&h), h.now + HOST_SLOW_MS + 10_000.0);
    assert_eq!(time_reads(&h), 0);
}

#[test]
fn a_failed_time_read_still_reauths_on_the_existing_clock() {
    for failure in ["status", "transport", "timeout"] {
        let mut h = startup(Variant::Linear, None);
        let refused = refuse_first_auth(&mut h);
        let read = h.take("/v5/market/time");
        let issued_at = h.now;
        reconnect(&mut h, refused);
        let sent_at = match failure {
            "status" => {
                h.reply(
                    read.id,
                    403,
                    "<html>blocked access from your country</html>",
                );
                h.now
            }
            // The read's own 5 s window bounds the wait.
            "timeout" => {
                h.advance(3_900.0);
                assert!(h.sent_on("/v5/private").is_empty(), "still waiting");
                h.advance(200.0);
                issued_at + 5_000.0
            }
            _ => {
                h.event(Event::HttpFailed {
                    request: read.id,
                    error: "network",
                });
                h.now
            }
        };
        assert_eq!(auth_expires(&h), sent_at + 10_000.0, "{failure}");
        assert_eq!(time_reads(&h), 0, "{failure}: no loop of time reads");
    }
}

#[test]
fn a_rest_answer_time_calibrates_the_first_auth() {
    for variant in [
        Variant::Linear,
        Variant::Demo,
        Variant::Inverse,
        Variant::Spot,
        Variant::Hyrotrader,
    ] {
        let mut h = startup(variant, Some(1_000.0 + HOST_SLOW_MS));
        open_private(&mut h);
        assert_eq!(
            auth_expires(&h),
            h.now + HOST_SLOW_MS + 10_000.0,
            "{variant:?}: first auth on Bybit's clock"
        );
    }
}

#[test]
fn answer_jitter_under_a_second_leaves_the_calibrated_clock() {
    let mut h = startup(Variant::Linear, Some(1_000.0 + HOST_SLOW_MS));
    let socket = open_private(&mut h);
    assert_eq!(auth_expires(&h), h.now + HOST_SLOW_MS + 10_000.0);
    // A later answer reads 500 ms late: ordinary network delay, not drift.
    h.message(
        "/v5/private",
        r#"{"success":true,"ret_msg":"","op":"auth"}"#,
    );
    h.message(
        "/v5/private",
        r#"{"success":true,"ret_msg":"","op":"subscribe"}"#,
    );
    let orders = h.take("/v5/order/realtime");
    let late = h.now + HOST_SLOW_MS - 500.0;
    h.reply(
        orders.id,
        200,
        &stamped(
            r#"{"retCode":0,"retMsg":"OK","result":{"list":[],"nextPageCursor":"","category":"linear"},"retExtInfo":{}}"#,
            late,
        ),
    );
    h.event(Event::WsClosed { socket, code: 1006 });
    h.advance(1_000.0);
    open_private(&mut h);
    assert_eq!(
        auth_expires(&h),
        h.now + HOST_SLOW_MS + 10_000.0,
        "offset unchanged"
    );
}
