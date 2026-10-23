//! Component vectors through the injected provider, not production adoption.
use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::signing::{Identity, Operation, PayloadPart, SignRequest};
use safe_cex_engine::{
    Action, Command, Cx, Engine, Event, Method, Response, SocketId, TimerId, Venue,
};
use safe_cex_venue_okx::dispatch_signing::{Http, WsLogin};

struct Probe {
    identity: Identity,
    origin: String,
    ws: bool,
    mismatch: bool,
}
impl Venue for Probe {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        if self.ws {
            let socket = cx.connect(self.origin.clone());
            cx.send_signed(
                socket,
                String::new(),
                Box::new(WsLogin::new(self.identity.clone(), self.origin.clone())),
            );
        } else {
            let path = "/api/v5/trade/batch-orders?tag=a%2Bb";
            let request = cx.request(
                Method::Post,
                format!("{}{path}", self.origin),
                vec![
                    ("x-simulated-trading", "1".into()),
                    ("Content-Type", "application/json".into()),
                ],
                Some(r#"[{"instId":"BTC-USDT-SWAP"}]"#.into()),
                (),
            );
            cx.sign_http(
                request,
                Box::new(Http::new(
                    Some(self.identity.clone()),
                    if self.mismatch { "/different" } else { path }.into(),
                )),
            );
        }
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn book(&self, _: &str) -> Option<&safe_cex_engine::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn staged(
    ws: bool,
    stamp: f64,
) -> (
    Engine<Probe>,
    LocalSigner,
    safe_cex_engine::RequestId,
    SignRequest,
) {
    let (provider, identities) = LocalSigner::from_json(
        r#"{"primary":{"key":"test-key","secret":"test-secret","passphrase":"test-passphrase"}}"#
            .into(),
    )
    .unwrap();
    let mut engine = safe_cex_engine::test_support::seeded_engine(Probe {
        identity: identities.primary.unwrap(),
        origin: if ws {
            "wss://wseeapap.okx.com/ws/v5/private"
        } else {
            "https://eea.okx.com"
        }
        .into(),
        ws,
        mismatch: false,
    });
    engine.start(stamp - 1000.);
    engine.prepare_dispatch(stamp);
    let (attempt, request) = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::Sign { attempt, work, .. } => Some((attempt, work)),
            _ => None,
        })
        .unwrap();
    (engine, provider, attempt, request)
}

#[test]
fn http_canonical_bytes_and_passphrase_are_one_atomic_provider_work() {
    let (mut engine, provider, attempt, request) = staged(false, 1_709_209_849_123.9);
    let payload = b"2024-02-29T12:30:49.123ZPOST/api/v5/trade/batch-orders?tag=a%2Bb[{\"instId\":\"BTC-USDT-SWAP\"}]";
    assert_eq!(request.receipt.stamp_ms, Some(1_709_209_849_123.));
    assert_eq!(request.context.host, "eea.okx.com");
    assert!(matches!(&request.operations[0], Operation::Present { .. }));
    assert!(
        matches!(&request.operations[1], Operation::Sign { payload: parts, .. } if parts == &vec![PayloadPart::Bytes { bytes: payload.to_vec() }])
    );
    assert!(matches!(&request.operations[2], Operation::Present { .. }));
    let signed = provider.sign_now(&request).unwrap();
    let expected =
        safe_cex_crypto::hmac_sha256(b"test-secret", payload, safe_cex_crypto::Encoding::Base64);
    engine.handle(
        1_709_209_849_124.,
        Event::Signed {
            attempt,
            result: Ok(signed),
        },
    );
    let output: Vec<_> = engine.drain_actions().collect();
    let http = output
        .iter()
        .find_map(|action| match action {
            Action::Http(http) => Some(http),
            _ => None,
        })
        .unwrap();
    let header = |name| &http.headers.iter().find(|(key, _)| *key == name).unwrap().1;
    assert_eq!(header("OK-ACCESS-SIGN"), &expected);
    assert_eq!(header("OK-ACCESS-PASSPHRASE"), "test-passphrase");
    assert_eq!(header("OK-ACCESS-TIMESTAMP"), "2024-02-29T12:30:49.123Z");
    assert_eq!(header("x-simulated-trading"), "1");
    assert_eq!(
        http.headers,
        vec![
            ("OK-ACCESS-KEY", "test-key".into()),
            ("OK-ACCESS-SIGN", expected),
            ("OK-ACCESS-TIMESTAMP", "2024-02-29T12:30:49.123Z".into()),
            ("OK-ACCESS-PASSPHRASE", "test-passphrase".into()),
            ("x-simulated-trading", "1".into()),
            ("Content-Type", "application/json".into()),
        ]
    );
    assert_eq!(
        http.body.as_deref(),
        Some(r#"[{"instId":"BTC-USDT-SWAP"}]"#)
    );
}

#[test]
fn mismatched_native_path_never_reaches_provider_or_transport() {
    let (_, identities) = LocalSigner::from_json(
        r#"{"primary":{"key":"test-key","secret":"test-secret","passphrase":"test-passphrase"}}"#
            .into(),
    )
    .unwrap();
    let mut engine = safe_cex_engine::test_support::seeded_engine(Probe {
        identity: identities.primary.unwrap(),
        origin: "https://eea.okx.com".into(),
        ws: false,
        mismatch: true,
    });
    engine.start(0.);
    engine.prepare_dispatch(1.);
    assert!(
        !engine
            .drain_actions()
            .any(|action| matches!(action, Action::Http(_) | Action::Sign { .. }))
    );
}

#[test]
fn ws_uses_send_time_seconds_and_base64_without_moving_raw_time() {
    let (mut engine, provider, attempt, request) = staged(true, 1_234_567_890_999.9);
    assert_eq!(request.receipt.stamp_ms, Some(1_234_567_890_000.));
    let signed = provider.sign_now(&request).unwrap();
    // Independent fixed vector retained from the pre-migration signer.
    assert_eq!(
        safe_cex_crypto::encode(&signed.values[1], safe_cex_crypto::Encoding::Base64),
        "qf8dggkdwH7IVTM6AIiNv9i1NS6Boqw3+pNUhrOWjWQ="
    );
    engine.handle(
        1_234_567_891_000.,
        Event::Signed {
            attempt,
            result: Ok(signed),
        },
    );
    let text = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::WsSend { text, .. } => Some(text),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        text,
        r#"{"op":"login","args":[{"apiKey":"test-key","passphrase":"test-passphrase","timestamp":"1234567890","sign":"qf8dggkdwH7IVTM6AIiNv9i1NS6Boqw3+pNUhrOWjWQ="}]}"#
    );
}
