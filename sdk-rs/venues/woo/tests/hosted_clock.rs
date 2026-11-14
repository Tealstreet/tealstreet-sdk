use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::signing::{
    Algorithm, Capability, Identities, Identity, KeyRef, SignFuture, SignRequest, Signer,
};
use safe_cex_engine::{
    Action, Command, Cx, Engine, Event, Method, RequestId, Response, SocketId, TimerId, Venue,
};
use safe_cex_venue_woo::dispatch_signing::{Http, WsAuth};
use std::sync::Arc;
const NOW: f64 = 1_700_000_000_000.;
#[derive(Clone, serde::Deserialize)]
struct Vector {
    method: String,
    path: String,
    params: Vec<(String, String)>,
    body: Option<String>,
    payload: String,
}
fn vectors() -> Vec<Option<Vector>> {
    let values: Vec<Vector> =
        serde_json::from_str(include_str!("fixtures/hosted-signing.json")).unwrap();
    values
        .into_iter()
        .map(Some)
        .chain(std::iter::once(None))
        .collect()
}
fn method(value: &str) -> Method {
    match value {
        "GET" => Method::Get,
        "POST" => Method::Post,
        "PUT" => Method::Put,
        "DELETE" => Method::Delete,
        _ => panic!("fixture method"),
    }
}
struct Parked(Arc<LocalSigner>);
impl Signer for Parked {
    fn sign(&self, _: SignRequest) -> SignFuture {
        Box::pin(std::future::pending())
    }
    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        self.0.can_sign(key, algorithm)
    }
}
struct Probe {
    identity: Identity,
    vector: Option<Vector>,
}
impl Venue for Probe {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        if let Some(vector) = &self.vector {
            let method = method(&vector.method);
            let recipe = Http::new(
                self.identity.clone(),
                "https://api.woox.io",
                method,
                vector.path.clone(),
                vector.params.clone(),
                vector.body.clone(),
            );
            let (url, headers, body) = recipe.unsigned();
            let request = cx.request(method, url, headers, body, ());
            cx.sign_http(request, Box::new(recipe));
        } else {
            cx.connect("wss://fixture.invalid");
        }
    }

    fn on_open(&mut self, cx: &mut Cx<'_, ()>, socket: SocketId) {
        cx.send_signed(
            socket,
            String::new(),
            Box::new(WsAuth::new(self.identity.clone(), socket)),
        );
    }
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, cx: &mut Cx<'_, ()>, _: Command) {
        assert!(cx.observe_protocol_time(NOW + 10_000.));
    }
    fn book(&self, _: &str) -> Option<&safe_cex_engine::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn setup(vector: Option<&Vector>) -> (Engine<Probe>, Arc<LocalSigner>) {
    let (local, identities): (LocalSigner, Identities) = LocalSigner::from_setup(LocalSetup {
        primary: Some(LocalIdentity {
            key: Secret::new("fixture-public".into()),
            secret: Secret::new("fixture-signing-material".into()),
            ..Default::default()
        }),
        ..Default::default()
    })
    .unwrap();
    let local = Arc::new(local);
    let mut engine = safe_cex_engine::test_support::seeded_engine(Probe {
        identity: identities.primary.clone().unwrap(),
        vector: vector.cloned(),
    });
    engine
        .set_signer(Some(Arc::new(Parked(local.clone()))))
        .unwrap();
    engine.set_identities(identities).unwrap();
    engine.start(NOW);
    if vector.is_none() {
        let socket = engine
            .drain_actions()
            .find_map(|action| match action {
                Action::WsConnect { socket, .. } => Some(socket),
                _ => None,
            })
            .unwrap();
        engine.handle(NOW, Event::WsOpened { socket });
    }
    (engine, local)
}
fn take_work(engine: &mut Engine<Probe>, now: f64) -> (RequestId, SignRequest) {
    engine.prepare_dispatch(now);
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::WsSend { .. })),
        "unexpected physical release: {actions:?}"
    );
    actions
        .into_iter()
        .find_map(|action| match action {
            Action::Sign { attempt, work, .. } => Some((attempt, work)),
            _ => None,
        })
        .expect("fresh signing attempt")
}
#[test]
fn clock_skew_discards_the_old_mac_and_resigns_http_and_ws_with_the_corrected_stamp() {
    for vector in vectors() {
        let websocket = vector.is_none();
        let (mut engine, local) = setup(vector.as_ref());
        let (old_id, old) = take_work(&mut engine, NOW);
        engine.handle(
            NOW + 1.,
            Event::Command(Command::SetTickerSymbols {
                symbols: vec!["fixture-clock".into()],
            }),
        );
        engine.handle(
            NOW + 2.,
            Event::Signed {
                attempt: old_id,
                result: local.sign_now(&old),
            },
        );
        let (fresh_id, fresh) = take_work(&mut engine, NOW + 2.);
        assert_ne!(old_id, fresh_id);
        assert_ne!(old.receipt, fresh.receipt);
        let stamp = fresh.receipt.stamp_ms.unwrap();
        assert!(stamp >= NOW + 10_000.);
        let text = match &vector {
            Some(vector) if vector.path.contains("/v1/") => {
                format!("{}|{stamp:.0}", vector.payload.rsplit_once('|').unwrap().0)
            }
            Some(vector) => format!(
                "{stamp:.0}{}",
                vector.payload.strip_prefix("1700000000000").unwrap()
            ),
            None => format!("|{stamp:.0}"),
        };
        assert_eq!(
            fresh.operations[1],
            safe_cex_engine::signing::Operation::Sign {
                algorithm: Algorithm::HmacSha256,
                key: engine
                    .identities()
                    .primary
                    .as_ref()
                    .unwrap()
                    .signing_ref()
                    .unwrap()
                    .clone(),
                payload: vec![safe_cex_engine::signing::PayloadPart::Bytes {
                    bytes: text.as_bytes().to_vec()
                }],
            }
        );
        engine.handle(
            NOW + 2.,
            Event::Signed {
                attempt: fresh_id,
                result: local.sign_now(&fresh),
            },
        );
        let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, NOW + 2.);
        let expected = safe_cex_crypto::hmac_sha256(
            b"fixture-signing-material",
            text.as_bytes(),
            safe_cex_crypto::Encoding::Hex,
        );
        if websocket {
            let frames: Vec<_> = actions
                .iter()
                .filter_map(|a| match a {
                    Action::WsSend { text, .. } => Some(text),
                    _ => None,
                })
                .collect();
            assert_eq!(frames.len(), 1);
            let auth: serde_json::Value = serde_json::from_str(frames[0]).unwrap();
            assert_eq!(auth["params"]["timestamp"], stamp);
            assert_eq!(auth["params"]["sign"], expected);
        } else {
            let requests: Vec<_> = actions
                .iter()
                .filter_map(|a| match a {
                    Action::Http(r) => Some(r),
                    _ => None,
                })
                .collect();
            assert_eq!(requests.len(), 1);
            let vector = vector.as_ref().unwrap();
            let expected_body = if vector.path.contains("/v1/") && vector.method != "GET" {
                Some(vector.payload.rsplit_once('|').unwrap().0)
            } else {
                vector.body.as_deref()
            };
            assert_eq!(requests[0].body.as_deref(), expected_body);
            assert!(requests[0].headers.contains(&("x-api-signature", expected)));
        }
    }
}
#[test]
fn wrong_receipts_and_malformed_provider_results_publish_neither_http_nor_ws() {
    for vector in vectors() {
        for bad_receipt in [false, true] {
            let (mut engine, local) = setup(vector.as_ref());
            let (attempt, work) = take_work(&mut engine, NOW);
            let mut result = local.sign_now(&work).unwrap();
            if bad_receipt {
                result.receipt.stamp_ms = Some(NOW + 1.);
            } else {
                result.values[1].pop();
            }
            engine.handle(
                NOW,
                Event::Signed {
                    attempt,
                    result: Ok(result),
                },
            );
            engine.prepare_dispatch(NOW);
            let actions: Vec<_> = engine.drain_actions().collect();
            assert!(
                !actions
                    .iter()
                    .any(|a| matches!(a, Action::Http(_) | Action::WsSend { .. }))
            );
            assert!(!format!("{actions:?}").contains("fixture-signing-material"));
        }
    }
}
