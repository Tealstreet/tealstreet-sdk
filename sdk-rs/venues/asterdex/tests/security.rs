//! R6: the venue holds provider refs only and every signed request is
//! materialised by the injected provider at dispatch.
use safe_cex_engine::signing::{
    Identity, KeyRef, Operation, SignError, SignFuture, SignRequest, SignResult, Signer,
};
use safe_cex_engine::{Action, Event, HttpRequest, Method, Response};
use safe_cex_venue_asterdex::{AsterDex, Options, sign::Version};
use std::sync::{Arc, Mutex};

const MARKETS: &str = r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","marginAsset":"USDT","contractType":"PERPETUAL","pricePrecision":1,"quantityPrecision":3,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.1"},{"filterType":"LOT_SIZE","stepSize":"0.001","minQty":"0.001","maxQty":"100"},{"filterType":"MARKET_LOT_SIZE","stepSize":"0.001","minQty":"0.001","maxQty":"100"}]}]}"#;

/// Presents `presented:<ref>` and signs with fixed bytes; records every batch.
#[derive(Default)]
struct Recording(Mutex<Vec<SignRequest>>);
impl Signer for Recording {
    fn sign(&self, request: SignRequest) -> SignFuture {
        let result = self.sign_ready(&request).unwrap();
        Box::pin(async move { result })
    }
    fn immediate(&self) -> bool {
        true
    }
    fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        self.0.lock().unwrap().push(request.clone());
        let values = request
            .operations
            .iter()
            .map(|operation| match operation {
                Operation::Present { key } => format!("presented:{}", key.0).into_bytes(),
                _ => vec![0xab; 32],
            })
            .collect();
        Some(Ok(SignResult {
            values,
            receipt: request.receipt,
            headers: Vec::new(),
        }))
    }
}

fn identity(slot: &str) -> Identity {
    Identity::new(
        KeyRef(format!("{slot}.credential")),
        Some(KeyRef(format!("{slot}.signing"))),
    )
}

fn requests(engine: &mut safe_cex_engine::Engine<AsterDex>, now: f64) -> Vec<HttpRequest> {
    safe_cex_engine::test_support::prepared_actions(engine, now)
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect()
}

fn reply(
    engine: &mut safe_cex_engine::Engine<AsterDex>,
    now: f64,
    request: &HttpRequest,
    body: &str,
) {
    engine.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}

#[test]
fn venue_signs_only_through_the_injected_provider_with_the_selected_slot() {
    let now = 1_700_000_000_000.0;
    let recording = Arc::new(Recording::default());
    let mut engine = safe_cex_engine::test_support::seeded_engine(AsterDex::new(Options {
        credentials: Some(identity("primary")),
        read_credentials: Some(identity("secondary")),
        api_version: Some(Version::V1),
        ..Options::default()
    }));
    engine.set_signer(Some(recording.clone())).unwrap();
    engine.start(now);
    let mut signed = Vec::new();
    let mut pending = requests(&mut engine, now);
    for _ in 0..8 {
        let Some(request) = pending.pop() else { break };
        if request.url.contains("signature=") {
            signed.push(request);
            continue;
        }
        let body = if request.url.ends_with("/time") {
            r#"{"serverTime":1700000000000}"#
        } else if request.url.contains("/exchangeInfo") {
            MARKETS
        } else {
            "[]"
        };
        reply(&mut engine, now, &request, body);
        pending.extend(requests(&mut engine, now));
    }
    signed.extend(pending.into_iter().filter(|r| r.url.contains("signature=")));
    let signature = format!("signature={}", "ab".repeat(32));
    let listen = signed
        .iter()
        .find(|r| r.method == Method::Post && r.url.contains("/listenKey"))
        .expect("listen key requested");
    assert_eq!(
        listen.headers,
        vec![("X-MBX-APIKEY", "presented:primary.credential".into())]
    );
    let read = signed
        .iter()
        .find(|r| r.method == Method::Get)
        .expect("signed read");
    assert_eq!(
        read.headers,
        vec![("X-MBX-APIKEY", "presented:secondary.credential".into())]
    );
    assert!(signed.iter().all(|r| r.url.ends_with(&signature)));
    // Every batch references only the installed refs, never key material.
    let refs = ["primary", "secondary"].map(identity);
    for request in recording.0.lock().unwrap().iter() {
        for operation in &request.operations {
            let key = match operation {
                Operation::Present { key } | Operation::Sign { key, .. } => key,
                _ => panic!("unexpected operation"),
            };
            assert!(
                refs.iter()
                    .any(|id| *key == id.credential || *key == id.signing_ref().unwrap().clone())
            );
        }
    }
    assert_eq!(recording.0.lock().unwrap().len(), signed.len());
}

#[test]
fn signing_refs_without_a_resolved_version_are_refused_not_guessed() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(AsterDex::new(Options {
        credentials: Some(identity("primary")),
        ..Options::default()
    }));
    engine
        .set_signer(Some(Arc::new(Recording::default())))
        .unwrap();
    engine.start(0.0);
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(actions.iter().any(|action| matches!(
        action,
        Action::Notice(safe_cex_engine::Notice::Event(event))
            if event.code == safe_cex_engine::log::code::CONFIG_INVALID
                && event.message.contains("resolved API version"))));
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
}

/// Replaces the deleted oracle's validation test: V3 refs with no main
/// wallet are refused at start with the legacy message, before any I/O.
#[test]
fn v3_refs_without_the_main_wallet_are_refused_before_any_request() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(AsterDex::new(Options {
        credentials: Some(identity("primary")),
        api_version: Some(safe_cex_venue_asterdex::sign::Version::V3),
        signer_address: Some("0x0000000000000000000000000000000000000111".into()),
        ..Options::default()
    }));
    engine
        .set_signer(Some(Arc::new(Recording::default())))
        .unwrap();
    engine.start(0.0);
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(actions.iter().any(|action| matches!(
        action,
        Action::Notice(safe_cex_engine::Notice::Event(event))
            if event.code == safe_cex_engine::log::code::CONFIG_INVALID
                && event.message.starts_with("Missing required Aster v3 auth fields"))));
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
}
