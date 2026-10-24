//! Native trading events retain their clocks; managed preparation uses the last event.
#[allow(unused_imports)]
pub use super::{BALANCE, MARKETS, ORDER, POSITIONS, SYMBOL, jittered};
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, SocketId};
use safe_cex_venue_backpack::{Backpack, Options};
use std::ops::{Deref, DerefMut};

pub struct Driver {
    inner: Engine<Backpack>,
    now: f64,
}
impl Driver {
    pub fn new(mut options: Options) -> Self {
        let (provider, identities) = safe_cex_crypto::signer::LocalSigner::from_setup_with_policy(
            safe_cex_crypto::signer::LocalSetup {
                primary: Some(safe_cex_crypto::signer::LocalIdentity {
                    key: safe_cex_crypto::signer::Secret::new(options.key.clone().unwrap()),
                    secret: safe_cex_crypto::signer::Secret::new(options.secret.clone().unwrap()),
                    secret_encoding: safe_cex_engine::signing::KeyEncoding::Base64,
                    ..Default::default()
                }),
                secondary: None,
            },
            safe_cex_engine::signing::CredentialSetupPolicy {
                primary: Some(safe_cex_engine::signing::IdentitySetupPolicy {
                    signing_algorithm: Some(safe_cex_engine::signing::Algorithm::Ed25519),
                    ..Default::default()
                }),
                secondary: None,
            },
        )
        .unwrap();
        options.identity = identities.primary.clone();
        let mut inner = safe_cex_engine::test_support::seeded_engine(Backpack::new(options));
        inner
            .set_signer(Some(std::sync::Arc::new(provider)))
            .unwrap();
        inner.set_identities(identities).unwrap();
        Self { inner, now: 1000.0 }
    }
    pub fn handle(&mut self, now: f64, event: Event<'_>) {
        self.now = now;
        self.inner.handle(now, event);
    }
    pub fn start(&mut self, now: f64) {
        self.now = now;
        self.inner.start(now);
    }
}
impl Deref for Driver {
    type Target = Engine<Backpack>;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
impl DerefMut for Driver {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
pub fn ready() -> (Driver, SocketId) {
    let (inner, socket) = super::ready();
    (Driver { inner, now: 1000.0 }, socket)
}
pub fn ready_orders(orders: &str) -> (Driver, SocketId) {
    let (inner, socket) = super::ready_orders(orders);
    (Driver { inner, now: 1000.0 }, socket)
}
pub fn actions(engine: &mut Driver) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(&mut engine.inner, engine.now)
}
pub fn requests(engine: &mut Driver) -> Vec<HttpRequest> {
    actions(engine)
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect()
}
pub fn reply(engine: &mut Driver, request: &HttpRequest, status: u16, body: &str) {
    engine.handle(
        1000.0,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
}
pub fn ws(engine: &mut Driver, socket: SocketId, stream: &str, data: &str) {
    let text = format!(r#"{{"stream":"{stream}","data":{data}}}"#);
    engine.handle(
        2000.0,
        Event::WsMessage {
            socket,
            text: &text,
        },
    );
}

pub fn identity_only_orders(
    orders: &str,
    identity: safe_cex_engine::signing::Identity,
    provider: std::sync::Arc<dyn safe_cex_engine::signing::Signer>,
    recv_window: Option<f64>,
) -> Driver {
    identity_only_orders_mode(orders, identity, provider, recv_window, false)
}
pub fn identity_only_orders_mode(
    orders: &str,
    identity: safe_cex_engine::signing::Identity,
    provider: std::sync::Arc<dyn safe_cex_engine::signing::Signer>,
    recv_window: Option<f64>,
    public_data_only: bool,
) -> Driver {
    let (snapshot, _) = super::ready_orders(orders);
    let options = Options {
        identity: Some(identity.clone()),
        public_data_only,
        recv_window,
        ..Default::default()
    };
    assert!(options.key.is_none() && options.secret.is_none());
    let mut inner = safe_cex_engine::test_support::seeded_engine(Backpack::new(options));
    inner.set_signer(Some(provider)).unwrap();
    inner
        .set_identities(safe_cex_engine::signing::Identities {
            primary: Some(identity),
            secondary: None,
        })
        .unwrap();
    inner.start(1000.);
    inner.drain_actions().for_each(drop);
    inner
        .store_mut()
        .set_markets(snapshot.store().markets().to_vec());
    for order in snapshot.store().orders() {
        inner.store_mut().upsert_order(order.clone());
    }
    Driver { inner, now: 1000. }
}
