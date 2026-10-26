//! Exercise the native recipe through shared dispatch and a trusted provider.
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::book::Book;
use safe_cex_engine::signing::{Identity, Provider};
use safe_cex_engine::{
    Action, Command, Cx, HttpRequest, Method, Response, SocketId, TimerId, Venue,
};
use safe_cex_venue_bingx::paths;
use safe_cex_venue_bingx::sign::{Params, dispatch::Http};
#[allow(dead_code)]
#[path = "support/oracle.rs"]
mod oracle;
use oracle::Credentials;

struct Probe {
    identity: Identity,
    method: Method,
    path: &'static str,
    params: Params,
    alter: u8,
}
impl Venue for Probe {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        let step = Http::new(
            self.identity.clone(),
            self.method,
            self.path,
            self.params.clone(),
        );
        let (url, headers, body) = step.unsigned();
        let id = cx.request(self.method, url, headers, body, ());
        cx.sign_http(
            id,
            Box::new(Alter {
                inner: step,
                kind: self.alter,
            }),
        );
    }
    fn on_sign_error(&mut self, cx: &mut Cx<'_, ()>, _: (), error: safe_cex_engine::VenueError) {
        cx.fail(1, error);
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), error: &str) {
        panic!("{error}");
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
fn setup() -> (Identity, Provider) {
    let (local, identities) = LocalSigner::from_setup(LocalSetup {
        primary: Some(LocalIdentity {
            key: Secret::new("replay-key".into()),
            secret: Secret::new("replay-secret".into()),
            ..Default::default()
        }),
        ..Default::default()
    })
    .unwrap();
    (identities.primary.unwrap(), std::sync::Arc::new(local))
}
fn request(method: Method, path: &'static str, params: Params, now: f64) -> HttpRequest {
    let (identity, local) = setup();
    let mut engine = safe_cex_engine::test_support::seeded_engine(Probe {
        identity,
        method,
        path,
        params,
        alter: 0,
    });
    engine.set_signer(Some(local)).unwrap();
    engine.start(now);
    let requests: Vec<_> = engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::Http(http) => Some(http),
            Action::Sign { .. } | Action::SignedOutbound { .. } => {
                panic!("local provider must release inline")
            }
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 1);
    requests.into_iter().next().unwrap()
}
fn params() -> Params {
    Params::new()
        .str("symbol", "BTC-USDT")
        .str("side", "BUY")
        .str("type", "LIMIT")
        .num("quantity", 0.01)
        .num("price", 30000.)
        .boolean("reduceOnly", true)
        .integer("orderId", 9007199254740993)
        .str("clientOrderID", "a b+/%")
}
#[test]
fn provider_recipe_preserves_every_native_method_and_typed_body() {
    for method in [Method::Get, Method::Post, Method::Put, Method::Delete] {
        let http = request(
            method,
            "/openApi/swap/v2/trade/order",
            params(),
            1700000000000.875,
        );
        // The unmigrated construction path is an independent legacy oracle for
        // this additive checkpoint; it becomes test-only at builder adoption.
        let expected = oracle::Params::new()
            .str("symbol", "BTC-USDT")
            .str("side", "BUY")
            .str("type", "LIMIT")
            .num("quantity", 0.01)
            .num("price", 30000.)
            .boolean("reduceOnly", true)
            .integer("orderId", 9007199254740993)
            .str("clientOrderID", "a b+/%")
            .into_request(
                method,
                "/openApi/swap/v2/trade/order",
                Some(&Credentials {
                    read: None,
                    key: "replay-key".into(),
                    secret: "replay-secret".into(),
                }),
                1700000000000.875,
            );
        assert_eq!((http.url, http.headers, http.body.clone()), expected);
        if method == Method::Post {
            let body: serde_json::Value = serde_json::from_str(&http.body.unwrap()).unwrap();
            assert_eq!(body["orderId"].as_u64(), Some(9007199254740993));
            assert_eq!(body["reduceOnly"], true);
            assert_eq!(body["quantity"], 0.01);
            assert_eq!(body["timestamp"], 1700000000000_f64);
        }
    }
}
#[test]
fn listen_presentation_has_neither_timestamp_nor_signature() {
    for method in [Method::Post, Method::Put] {
        let http = request(
            method,
            "/openApi/user/auth/userDataStream",
            Params::new().str("listenKey", "session"),
            1700000000000.875,
        );
        let wire = format!("{}{}", http.url, http.body.unwrap_or_default());
        assert!(!wire.contains("timestamp"));
        assert!(!wire.contains("signature"));
        assert!(http.headers.contains(&("X-BX-APIKEY", "replay-key".into())));
    }
}

struct Alter {
    inner: Http,
    kind: u8,
}
impl safe_cex_engine::SignStep for Alter {
    fn prepare(
        &mut self,
        outbound: &mut Action,
        cx: &safe_cex_engine::DispatchContext<'_>,
    ) -> Result<safe_cex_engine::signing::SignRequest, safe_cex_engine::signing::SignError> {
        if let Action::Http(http) = outbound {
            match self.kind {
                1 => http.url.push_str("/different"),
                2 => http.method = Method::Delete,
                3 => http.headers.push(("X-Untrusted", "changed".into())),
                4 => http.body = Some("{}".into()),
                _ => (),
            }
        }
        self.inner.prepare(outbound, cx)
    }
    fn apply(
        &mut self,
        outbound: &mut Action,
        result: &safe_cex_engine::signing::SignResult,
    ) -> Result<(), safe_cex_engine::signing::SignError> {
        self.inner.apply(outbound, result)
    }
    fn freshness_ms(&self) -> f64 {
        self.inner.freshness_ms()
    }
}
struct Parked(Provider);
impl safe_cex_engine::signing::Signer for Parked {
    fn immediate(&self) -> bool {
        false
    }
    fn sign(
        &self,
        work: safe_cex_engine::signing::SignRequest,
    ) -> safe_cex_engine::signing::SignFuture {
        self.0.sign(work)
    }
    fn sign_ready(
        &self,
        _: &safe_cex_engine::signing::SignRequest,
    ) -> Option<Result<safe_cex_engine::signing::SignResult, safe_cex_engine::signing::SignError>>
    {
        None
    }
}
fn parked(alter: u8) -> (safe_cex_engine::Engine<Probe>, Provider) {
    let (identity, local) = setup();
    let mut engine = safe_cex_engine::test_support::seeded_engine(Probe {
        identity,
        method: Method::Post,
        path: "/openApi/swap/v2/trade/order",
        params: params(),
        alter,
    });
    engine
        .set_signer(Some(std::sync::Arc::new(Parked(local.clone()))))
        .unwrap();
    engine.start(1700000000000.875);
    (engine, local)
}
fn work(
    engine: &mut safe_cex_engine::Engine<Probe>,
) -> (
    safe_cex_engine::RequestId,
    safe_cex_engine::signing::SignRequest,
) {
    let mut works = vec![];
    for action in engine.drain_actions() {
        match action {
            Action::Sign { attempt, work, .. } => works.push((attempt, work)),
            Action::Http(_) => panic!("parked provider must not send"),
            _ => (),
        }
    }
    assert_eq!(works.len(), 1);
    works.pop().unwrap()
}
#[test]
fn changed_intent_fails_before_provider_or_http() {
    for kind in 1..=4 {
        let (mut engine, _) = parked(kind);
        let actions: Vec<_> = engine.drain_actions().collect();
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Http(_) | Action::Sign { .. }))
        );
        assert!(actions.iter().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 1, error }) if error.outcome == Some(safe_cex_engine::Outcome::NotSent))));
    }
}
#[test]
fn malformed_provider_results_never_release_http() {
    for kind in 0..4 {
        let (mut engine, local) = parked(0);
        let (attempt, work) = work(&mut engine);
        let mut result = local.sign_ready(&work).unwrap().unwrap();
        match kind {
            0 => result.values[0].clear(),
            1 => result.values[0] = b"key\r\nInjected: yes".to_vec(),
            2 => {
                result.values[1].pop();
            }
            _ => {
                result.values.pop();
            }
        }
        engine.handle(
            1700000000001.,
            safe_cex_engine::Event::Signed {
                attempt,
                result: Ok(result),
            },
        );
        let actions: Vec<_> = engine.drain_actions().collect();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(actions.iter().any(|a| matches!(
            a,
            Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 1, .. })
        )));
    }
}
#[test]
fn stale_provider_reprepares_from_original_intent_and_old_attempt_cannot_send() {
    let (mut engine, local) = parked(0);
    let (old, old_work) = work(&mut engine);
    let result = local.sign_ready(&old_work).unwrap().unwrap();
    engine.handle(
        1700000005001.,
        safe_cex_engine::Event::Signed {
            attempt: old,
            result: Ok(result.clone()),
        },
    );
    let (fresh, fresh_work) = work(&mut engine);
    assert_ne!(old, fresh);
    assert_eq!(fresh_work.receipt.stamp_ms, Some(1700000005001.));
    engine.handle(
        1700000005001.,
        safe_cex_engine::Event::Signed {
            attempt: old,
            result: Ok(result),
        },
    );
    assert!(engine.drain_actions().next().is_none());
    engine.handle(
        1700000005002.,
        safe_cex_engine::Event::Signed {
            attempt: fresh,
            result: Ok(local.sign_ready(&fresh_work).unwrap().unwrap()),
        },
    );
    let requests: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| {
            if let Action::Http(http) = a {
                Some(http)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_str(requests[0].body.as_ref().unwrap()).unwrap();
    assert_eq!(body["timestamp"], 1700000005001_f64);
    assert_eq!(body["clientOrderID"], "a b+/%");
    assert_eq!(body["orderId"].as_u64(), Some(9007199254740993));
}
