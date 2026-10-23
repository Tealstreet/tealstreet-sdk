//! Original REDs for dispatch-time signing adoption, plus passing controls for
//! nonce and entropy behaviour that the cutover must preserve.
//! `setup()` is the one construction point the custody cutover rewrites.
#[allow(dead_code)]
mod support;
use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::signing::{
    Operation, Provider, SignError, SignFuture, SignRequest, SignResult, Signer,
};
use safe_cex_engine::{Action, Command, Event, Outcome, ReadCredentials};
use safe_cex_venue_blofin::Options;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use support::*;

/// Records each provider work's signed path and presented credential slot.
struct Recorder {
    inner: LocalSigner,
    seen: Arc<Mutex<Vec<(String, String)>>>,
}
impl Recorder {
    fn record(&self, request: &SignRequest) {
        // Every presented and signing key must come from one slot.
        let keys: Vec<&str> = request
            .operations
            .iter()
            .filter_map(|operation| match operation {
                Operation::Present { key } | Operation::Sign { key, .. } => Some(key.0.as_str()),
                _ => None,
            })
            .collect();
        let prefix = |key: &str| key.split('.').next().unwrap_or_default().to_owned();
        let slot = match keys.first() {
            Some(first) if keys.iter().all(|key| prefix(key) == prefix(first)) => {
                format!("{}.credential", prefix(first))
            }
            Some(_) => "MIXED".into(),
            None => String::new(),
        };
        self.seen
            .lock()
            .unwrap()
            .push((request.context.path.clone(), slot));
    }
}
impl Signer for Recorder {
    fn immediate(&self) -> bool {
        true
    }
    fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        self.record(request);
        Some(self.inner.sign_now(request))
    }
    fn sign(&self, request: SignRequest) -> SignFuture {
        self.record(&request);
        Box::pin(std::future::ready(self.inner.sign_now(&request)))
    }
}

type Seen = Arc<Mutex<Vec<(String, String)>>>;

/// Primary and secondary material, provisioned for both custody shapes.
fn setup() -> (Harness, Seen) {
    let (inner, _) = LocalSigner::from_json(
        r#"{"primary":{"key":"key","secret":"secret","passphrase":"pass"},"secondary":{"key":"read-key","secret":"read-secret","passphrase":"read-pass"}}"#
            .into(),
    )
    .unwrap();
    let seen = Seen::default();
    let provider: Provider = Arc::new(Recorder {
        inner,
        seen: seen.clone(),
    });
    let options = Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("pass".into()),
        read_credentials: Some(ReadCredentials {
            key: "read-key".into(),
            secret: "read-secret".into(),
            passphrase: Some("read-pass".into()),
        }),
        ..Options::default()
    };
    (
        Harness::with_options_and_signer(options, Some(provider)),
        seen,
    )
}

fn signed(seen: &Seen, prefix: &str) -> Vec<(String, String)> {
    seen.lock()
        .unwrap()
        .iter()
        .filter(|(path, _)| path.starts_with(prefix))
        .cloned()
        .collect()
}

fn slot(slot: &str) -> String {
    format!("{slot}.credential")
}

fn place(h: &mut Harness, raw: &str) {
    let orders: Vec<PlaceOrderOpts> = serde_json::from_str(raw).unwrap();
    h.e.handle(
        1.0,
        Event::Command(Command::PlaceOrders { command: 7, orders }),
    );
    h.collect();
}

fn nonce(request: &safe_cex_engine::HttpRequest) -> &str {
    &request
        .headers
        .iter()
        .find(|(name, _)| *name == "ACCESS-NONCE")
        .expect("nonce header")
        .1
}

#[test]
#[ignore = "original RED: startup settings reads are legacy-signed, not provider work on the secondary; enabled by the S3-S6 cutover"]
fn startup_settings_reads_are_provider_work_on_the_secondary() {
    let (_, seen) = setup();
    assert_eq!(
        signed(&seen, "/api/v1/account/"),
        [
            ("/api/v1/account/margin-mode".into(), slot("secondary")),
            ("/api/v1/account/position-mode".into(), slot("secondary")),
        ]
    );
}

#[test]
#[ignore = "original RED: the WS login is sent immediately with legacy-signed text; enabled by the S3-S6 cutover"]
fn ws_login_is_primary_provider_work_and_ping_follows_it() {
    let (mut h, seen) = setup();
    h.actions.clear();
    h.e.handle(0.0, Event::WsOpened { socket: h.private });
    h.collect_at(0.0);
    assert_eq!(
        signed(&seen, "/users/self/verify"),
        [("/users/self/verify".into(), slot("primary"))]
    );
    let sent: Vec<_> = h
        .actions
        .iter()
        .filter_map(|action| match action {
            Action::WsSend { socket, text } if *socket == h.private => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(sent.len(), 2, "{sent:?}");
    assert!(sent[0].contains("\"op\":\"login\""), "{sent:?}");
    assert_eq!(sent[1], "ping");
}

#[test]
#[ignore = "original RED: trading POSTs are legacy-signed, not primary provider work; enabled by the S3-S6 cutover"]
fn trading_post_is_provider_work_on_the_primary() {
    let (mut h, seen) = setup();
    h.seed();
    place(
        &mut h,
        r#"[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":60000}]"#,
    );
    assert_eq!(
        signed(&seen, "/api/v1/trade/"),
        [("/api/v1/trade/batch-orders".into(), slot("primary"))]
    );
}

#[test]
#[ignore = "original RED: the replacement Verify GET is legacy-signed, not secondary provider work; enabled by the S3-S6 cutover"]
fn replacement_verify_get_is_provider_work_on_the_secondary() {
    let (mut h, seen) = setup();
    h.seed();
    h.ws(r#"{"arg":{"channel":"orders"},"data":[{"instId":"BTC-USDT","orderCategory":"normal","orderId":"12","orderType":"limit","side":"buy","marginMode":"cross","positionSide":"net","size":"10","price":"60000","state":"live"}]}"#);
    h.e.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 7,
            id: "12".into(),
            price: Some(61000.0),
            amount: None,
        }),
    );
    h.collect_at(1.0);
    h.reply(
        "cancel-batch-orders",
        r#"{"code":"0","data":[{"orderId":"12","code":"0"}]}"#,
    );
    let pause = h
        .actions
        .iter()
        .rev()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == 350.0 => Some(*timer),
            _ => None,
        })
        .expect("cancel pause");
    h.e.handle(350.0, Event::TimerFired { timer: pause });
    h.collect_at(350.0);
    assert!(h.pending.iter().any(|r| r.url.contains("order-detail")));
    assert_eq!(
        signed(&seen, "/api/v1/trade/"),
        [
            ("/api/v1/trade/cancel-batch-orders".into(), slot("primary")),
            (
                "/api/v1/trade/order-detail?instId=BTC-USDT&orderId=12".into(),
                slot("secondary")
            ),
        ]
    );
}

/// Passing control: joined hedged-leverage legs each draw one fresh nonce, in
/// the order the legs are sent.
#[test]
fn joined_leverage_legs_draw_fresh_nonces_in_send_order() {
    let draws = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let counter = Arc::new(AtomicU8::new(0));
    let recorded = draws.clone();
    let mut h = Harness::with_signing_entropy(Box::new(move |bytes| {
        let draw = counter.fetch_add(1, Ordering::Relaxed);
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = draw.wrapping_mul(7).wrapping_add(index as u8);
        }
        recorded.lock().unwrap().push(bytes.to_vec());
        Ok(())
    }));
    h.modes();
    h.reply("account/positions", r#"{"code":"0","data":[]}"#);
    h.reply(
        "balances",
        r#"{"code":"0","data":[{"available":"1000","frozen":"0"}]}"#,
    );
    for path in [
        "orders-pending",
        "orders-tpsl-pending",
        "orders-algo-pending",
    ] {
        h.reply(path, r#"{"code":"0","data":[]}"#);
    }
    for command in [
        Command::SetMarginType {
            command: 8,
            symbol: "BTCUSDT".into(),
            isolated: true,
        },
        Command::SetPositionMode {
            command: 9,
            hedged: true,
            symbol: None,
        },
    ] {
        h.e.handle(1.0, Event::Command(command));
        h.collect();
        let path = if h.pending.iter().any(|r| r.url.contains("set-margin-mode")) {
            (
                "set-margin-mode",
                r#"{"code":"0","data":{"marginMode":"isolated"}}"#,
            )
        } else {
            (
                "set-position-mode",
                r#"{"code":"0","data":{"positionMode":"long_short_mode"}}"#,
            )
        };
        h.reply(path.0, path.1);
    }
    let before = draws.lock().unwrap().len();
    h.e.handle(
        2.0,
        Event::Command(Command::SetLeverage {
            command: 7,
            symbol: "BTCUSDT".into(),
            leverage: 30.0,
            side: None,
        }),
    );
    h.collect();
    let legs: Vec<_> = h
        .pending
        .iter()
        .filter(|r| r.url.ends_with("set-leverage"))
        .collect();
    let draws = draws.lock().unwrap();
    assert_eq!(legs.len(), 2);
    assert_eq!(draws.len(), before + 2, "exactly one draw per leg");
    for (leg, draw) in legs.iter().zip(&draws[before..]) {
        let expected = safe_cex_crypto::uuid::uuid_with_entropy(|bytes| {
            bytes.copy_from_slice(draw);
            Ok(())
        })
        .unwrap();
        assert_eq!(nonce(leg), expected);
    }
    assert_ne!(nonce(legs[0]), nonce(legs[1]));
}

/// Passing control: HTTP entropy failure is NotSent before any write; WS
/// entropy failure closes the private socket without a login.
#[test]
fn entropy_failure_is_not_sent_for_http_and_closes_the_private_socket() {
    let refuse = Arc::new(AtomicBool::new(false));
    let flag = refuse.clone();
    let mut h = Harness::with_signing_entropy(Box::new(move |bytes| {
        if flag.load(Ordering::Relaxed) {
            return Err("signing entropy unavailable".into());
        }
        bytes.fill(1);
        Ok(())
    }));
    h.seed();
    refuse.store(true, Ordering::Relaxed);
    place(
        &mut h,
        r#"[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":60000}]"#,
    );
    assert!(!h.pending.iter().any(|r| r.url.contains("batch-orders")));
    let [error] = h.failures.as_slice() else {
        panic!("{:?}", h.failures);
    };
    assert_eq!(error.outcome, Some(Outcome::NotSent));

    h.actions.clear();
    h.e.handle(2.0, Event::WsOpened { socket: h.private });
    h.collect();
    assert!(
        !h.actions
            .iter()
            .any(|a| matches!(a, Action::WsSend { text, .. } if text.contains("login")))
    );
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a, Action::WsClose { socket, .. } if *socket == h.private))
    );
}
