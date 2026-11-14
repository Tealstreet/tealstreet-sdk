use base64::{Engine, engine::general_purpose::STANDARD};
use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::request_context::{
    Claim, GroupClaim, GroupPacket, HttpAction, Preparation, Prepared,
};
use safe_cex_engine::signing::{
    Algorithm, Identities, Identity, KeyRef, Provider, SignError, SignRequest, SignResult,
};
use safe_cex_engine::{
    Action, Engine as VenueEngine, Event, HttpRequest, Method, Notice, RequestId, SocketChannel,
    Venue,
};
use safe_cex_exchange_server::{accounts, envelope::InitPayload};
use safe_cex_remote_signer::{RemoteAccount, SignerLink};
use safe_cex_venues::hosted_signing::{self, Input, OutputFormat};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    future::Future,
    pin::Pin,
    sync::Arc,
};
use tokio::sync::mpsc;

pub type Error = Box<dyn std::error::Error>;
pub type Result<T = ()> = std::result::Result<T, Error>;
pub const NOW: f64 = 1_700_000_000_123.75;
pub const SYMBOL: &str = "SOL_USDC_PERP";
pub const MARKETS: &str = r#"[{"symbol":"SOL_USDC_PERP","marketType":"PERP","quantityIncrement":"0.01","priceIncrement":"0.01"},{"symbol":"BTC_USDC_PERP","marketType":"PERP","quantityIncrement":"0.001","priceIncrement":"0.1"}]"#;
pub const ORDER: &str = r#"{"id":"123","clientId":42,"symbol":"SOL_USDC_PERP","side":"Bid","orderType":"Limit","price":"140","quantity":"5","executedQuantity":"0","timeInForce":"GTC","status":"New","createdAt":1000}"#;
pub const STOP: &str = r#"{"id":"123","clientId":42,"symbol":"SOL_USDC_PERP","side":"Ask","orderType":"Market","triggerPrice":"130","triggerQuantity":"5","triggerBy":"LastPrice","quantity":"5","executedQuantity":"0","reduceOnly":true,"status":"New","createdAt":1000}"#;

pub fn init(key: &str) -> InitPayload {
    InitPayload {
        account_name: "backpack-proof".into(),
        exchange_name: "backpack".into(),
        key: key.into(),
        ..Default::default()
    }
}

pub struct Device {
    pub link: Arc<SignerLink>,
    pub frames: mpsc::UnboundedReceiver<String>,
    local: LocalSigner,
    identities: Identities,
    pub payloads: Vec<String>,
    pub signatures: Vec<String>,
    pub refuse: bool,
}
impl Device {
    pub fn new() -> Result<Self> {
        let (sender, frames) = mpsc::unbounded_channel();
        let link = SignerLink::new(7, Arc::new(move |frame| sender.send(frame).is_ok()));
        let mut local = LocalSigner::new();
        let identity = Identity::new(
            KeyRef("device-public".into()),
            Some(KeyRef("device-signing".into())),
        );
        local.insert(identity.credential.clone(), b"fixture-key".to_vec());
        local.insert_declared(
            identity.signing_ref()?.clone(),
            vec![7; 32],
            Algorithm::Ed25519,
        )?;
        Ok(Self {
            link,
            frames,
            local,
            identities: Identities {
                primary: Some(identity),
                secondary: None,
            },
            payloads: vec![],
            signatures: vec![],
            refuse: false,
        })
    }
    pub fn remote(&self) -> Result<(RemoteAccount, String)> {
        let prepared = accounts::prepare(&init("fixture-key"), false, Some(self.link.clone()))?;
        let remote = prepared.remote.ok_or("no remote provider")?;
        Ok((remote, prepared.options))
    }
    pub fn answer(&mut self, frame: &str) -> Result<()> {
        let response = self.response(frame)?;
        assert!(self.link.resolve(7, response));
        Ok(())
    }
    pub fn response(&mut self, frame: &str) -> Result<Value> {
        let envelope: Value = serde_json::from_str(frame)?;
        assert_eq!(envelope["t"], "sign-req");
        let request = &envelope["d"];
        assert_eq!(request["exchangeName"], "backpack");
        assert_eq!(request["accountName"], "backpack-proof");
        assert_eq!(request["wsAuth"]["outputFormat"], "backpack-ed25519");
        let descriptor = request["wsAuth"]["toSign"]
            .as_str()
            .ok_or("missing descriptor")?;
        let decoded: Value = serde_json::from_str(descriptor)?;
        assert_eq!(decoded["slot"], "primary");
        let payload = decoded["payload"].as_str().ok_or("missing payload")?;
        self.payloads.push(payload.into());
        if self.refuse {
            return Ok(json!({"id":request["id"],"error":"SYNTHETIC-DEVICE-ERROR"}));
        }
        let (work, output) = hosted_signing::prepare(
            "backpack",
            &self.identities,
            Input::WsAuth {
                to_sign: descriptor.into(),
                output_format: OutputFormat::BackpackEd25519,
            },
        )?;
        let signed = self.local.sign_now(&work)?;
        let output = hosted_signing::finish(output, &signed)?;
        let output = output.signature.ok_or("no device output")?;
        let decoded: Value = serde_json::from_str(&output)?;
        let signature = decoded["signature"].as_str().ok_or("missing signature")?;
        self.signatures.push(signature.into());
        Ok(json!({"id":request["id"], "wsAuthSignature":output}))
    }
    pub async fn sign(
        &mut self,
        provider: &Provider,
        work: SignRequest,
    ) -> Result<std::result::Result<SignResult, SignError>> {
        let mut pending = provider.sign(work);
        let frame = tokio::select! {
            _ = &mut pending => return Err("completed before device round-trip".into()),
            frame = self.frames.recv() => frame.ok_or("device closed")?,
        };
        self.answer(&frame)?;
        Ok(pending.await)
    }
}

pub trait Driver {
    fn event(&mut self, event: Event<'_>);
    fn start(&mut self);
    fn set_time(&mut self, now: f64);
    fn skew_next_signature(&mut self);
    fn pump<'a>(
        &'a mut self,
        device: &'a mut Device,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Action>>> + 'a>>;
    fn store(&self) -> &safe_cex_engine::Store;
    fn ids(&self) -> &Identities;
    fn signer(&self) -> Option<Provider>;
}

pub struct Drive<V: Venue> {
    engine: VenueEngine<V>,
    now: f64,
    skew: bool,
    resources: HashMap<RequestId, (Preparation, u64)>,
    groups: HashMap<RequestId, GroupPacket>,
}
impl<V: Venue> Drive<V> {
    pub fn new(engine: VenueEngine<V>) -> Self {
        Self {
            engine,
            now: NOW,
            skew: false,
            resources: HashMap::new(),
            groups: HashMap::new(),
        }
    }
    async fn drain(&mut self, device: &mut Device) -> Result<Vec<Action>> {
        self.engine.prepare_dispatch(self.now);
        let mut queue = VecDeque::from(self.engine.drain_actions().collect::<Vec<_>>());
        let mut out = vec![];
        while let Some(action) = queue.pop_front() {
            match action {
                Action::ManagedHttp(action) => match *action {
                    HttpAction::Prepare {
                        preparation,
                        request,
                        ..
                    } => {
                        if self.engine.http_preparation_pending(preparation) {
                            self.resources
                                .insert(request.id, (preparation, request.id.0));
                            self.engine.http_prepared(
                                self.now,
                                preparation,
                                Prepared {
                                    id: request.id.0,
                                    expires_at: None,
                                },
                            );
                        }
                    }
                    HttpAction::Bind {
                        preparation,
                        permit,
                        request,
                    } => {
                        assert_eq!(
                            self.resources.remove(&request.id),
                            Some((preparation, permit.prepared_id()))
                        );
                        match self.engine.claim_http(self.now, permit) {
                            Ok(Claim::Released(_)) => out.push(Action::Http(request)),
                            Ok(Claim::WaitUntil(_)) => return Err("unexpected pacing wait".into()),
                            Err(_) => {}
                        }
                    }
                    HttpAction::BindGroup { permit, members } => {
                        if self.engine.http_group_pending(&permit) {
                            for mut member in members {
                                if let Some(request) = member.request.take() {
                                    self.groups.insert(
                                        request.id,
                                        GroupPacket {
                                            preparation: member.preparation,
                                            request: Some(request),
                                        },
                                    );
                                }
                            }
                            match self.engine.claim_http_group(self.now, permit.clone()) {
                                Ok(GroupClaim::Released(_)) => {
                                    for token in permit.members() {
                                        let request = token.attempt().request;
                                        self.resources.remove(&request);
                                        let mut packet = self
                                            .groups
                                            .remove(&request)
                                            .ok_or("missing group packet")?;
                                        out.push(Action::Http(
                                            packet.request.take().ok_or("missing bound packet")?,
                                        ));
                                    }
                                }
                                Ok(GroupClaim::WaitUntil(_)) => {
                                    return Err("unexpected group pacing wait".into());
                                }
                                Err(_) => {}
                            }
                        }
                    }
                    HttpAction::Cancel { preparation, .. } => {
                        self.resources.remove(&preparation.attempt().request);
                        self.groups.remove(&preparation.attempt().request);
                    }
                },
                Action::Sign { attempt, work, .. } => {
                    if self.engine.signing_pending(attempt) {
                        let provider = self.engine.signer().ok_or("missing provider")?;
                        let result = device.sign(&provider, work).await?;
                        if self.skew {
                            assert!(self.engine.sync_host_time(self.now, self.now + 1000.));
                            self.skew = false;
                        }
                        self.engine
                            .handle(self.now, Event::Signed { attempt, result });
                    }
                }
                Action::WsConnect { socket, .. } => {
                    self.engine.handle(self.now, Event::WsOpened { socket });
                    out.push(action);
                }
                _ => out.push(action),
            }
            for action in self
                .engine
                .drain_actions()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
            {
                queue.push_front(action);
            }
        }
        Ok(out)
    }
}
impl<V: Venue> Driver for Drive<V> {
    fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
    }
    fn start(&mut self) {
        self.engine.start(self.now);
    }
    fn set_time(&mut self, now: f64) {
        self.now = now;
    }
    fn skew_next_signature(&mut self) {
        self.skew = true;
    }
    fn store(&self) -> &safe_cex_engine::Store {
        self.engine.store()
    }
    fn ids(&self) -> &Identities {
        self.engine.identities()
    }
    fn signer(&self) -> Option<Provider> {
        self.engine.signer()
    }
    fn pump<'a>(
        &'a mut self,
        device: &'a mut Device,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Action>>> + 'a>> {
        Box::pin(self.drain(device))
    }
}
struct Build;
impl safe_cex_venues::Visit for Build {
    type Output = Box<dyn Driver>;
    fn visit<V: Venue + 'static>(self, engine: VenueEngine<V>) -> Self::Output {
        Box::new(Drive::new(engine))
    }
}
pub fn build(device: &Device, public: bool) -> Result<Box<dyn Driver>> {
    build_window(device, public, None)
}
fn build_window(device: &Device, public: bool, window: Option<f64>) -> Result<Box<dyn Driver>> {
    let init = init(if public { "" } else { "fixture-key" });
    let prepared = accounts::prepare(
        &init,
        public,
        if public {
            None
        } else {
            Some(device.link.clone())
        },
    )?;
    let provider = prepared.remote.map(|remote| remote.provider);
    let mut options: safe_cex_venues::EngineOptions = serde_json::from_str(&prepared.options)?;
    options.recv_window = window;
    Ok(safe_cex_venues::build_with_signer(
        "backpack",
        options,
        Some(safe_cex_engine::test_support::seeded_entropy(5)),
        provider,
        Build,
    )?)
}
pub fn response(driver: &mut dyn Driver, request: &HttpRequest, status: u16, body: &str) {
    driver.event(Event::HttpResponse {
        request: request.id,
        response: safe_cex_engine::Response {
            status,
            headers: "",
            body,
        },
    });
}
pub fn startup_body(url: &str, orders: &str) -> String {
    if url.ends_with("/markets") {
        MARKETS.into()
    } else if url.ends_with("/tickers") {
        r#"[{"symbol":"SOL_USDC_PERP","lastPrice":"140","priceChangePercent":"0","volume":"10","quoteVolume":"1400"}]"#.into()
    } else if url.ends_with("/account") {
        r#"{"leverageLimit":"10","accountId":"proof-account"}"#.into()
    } else if url.ends_with("/position") {
        r#"[{"symbol":"SOL_USDC_PERP","netQuantity":"5","entryPrice":"140","netExposureNotional":"700","pnlUnrealized":"0","estLiquidationPrice":"100"}]"#.into()
    } else if url.ends_with("/collateral") {
        r#"{"netEquity":"1005","netEquityAvailable":"905","netEquityLocked":"100"}"#.into()
    } else if url.ends_with("/orders") {
        orders.into()
    } else {
        "[]".into()
    }
}
pub async fn ready(device: &mut Device, orders: &str) -> Result<(Box<dyn Driver>, Vec<Action>)> {
    ready_at(device, orders, None, NOW).await
}
pub async fn ready_at(
    device: &mut Device,
    orders: &str,
    window: Option<f64>,
    now: f64,
) -> Result<(Box<dyn Driver>, Vec<Action>)> {
    let mut driver = build_window(device, false, window)?;
    driver.set_time(now);
    driver.start();
    let mut all = vec![];
    for _ in 0..12 {
        let actions = driver.pump(device).await?;
        for action in &actions {
            if let Action::Http(request) = action {
                response(
                    &mut *driver,
                    request,
                    200,
                    &startup_body(&request.url, orders),
                );
            }
        }
        let empty = actions.is_empty();
        all.extend(actions);
        if empty {
            break;
        }
    }
    let loaded = driver.store().loaded();
    assert!(
        loaded.markets && loaded.tickers && loaded.orders && loaded.positions && loaded.balance
    );
    Ok((driver, all))
}
pub fn requests(actions: Vec<Action>) -> Vec<HttpRequest> {
    actions
        .into_iter()
        .filter_map(|action| {
            if let Action::Http(request) = action {
                Some(request)
            } else {
                None
            }
        })
        .collect()
}
pub fn failure(actions: &[Action], command: u32) -> Result<&safe_cex_engine::VenueError> {
    actions
        .iter()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: id, error }) if *id == command => {
                Some(error)
            }
            _ => None,
        })
        .ok_or_else(|| "missing command failure".into())
}
pub fn command(driver: &mut dyn Driver, value: Value) -> Result<()> {
    driver.event(Event::Command(safe_cex_engine::command::decode_command(
        &value.to_string(),
    )?));
    Ok(())
}
pub fn signature(request: &HttpRequest) -> Result<&str> {
    request
        .headers
        .iter()
        .find_map(|(name, value)| (*name == "X-Signature").then_some(value.as_str()))
        .ok_or_else(|| "missing signature header".into())
}
pub fn assert_signature_shape(request: &HttpRequest) -> Result<()> {
    assert_eq!(STANDARD.decode(signature(request)?)?.len(), 64);
    Ok(())
}
pub fn private_socket(actions: &[Action]) -> bool {
    actions.iter().any(|action| {
        matches!(
            action,
            Action::WsConnect {
                channel: Some(SocketChannel::Private),
                ..
            }
        )
    })
}
pub fn writes(actions: &[Action]) -> usize {
    actions
        .iter()
        .filter(|action| matches!(action,Action::Http(request) if request.method!=Method::Get))
        .count()
}
