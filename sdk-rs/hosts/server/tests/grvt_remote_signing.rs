//! Native server init, managed transport and the actual device wallet codec.
use safe_cex_engine::model::{OrderSide, OrderType, PlaceOrderOpts, TimeInForce};
use safe_cex_engine::signing::{Identities, Provider};
use safe_cex_engine::{Command, HttpRequest, SocketChannel, SocketId};
use safe_cex_exchange_server::accounts::{engine_options, prepare};
use safe_cex_exchange_server::envelope::InitPayload;
use safe_cex_exchange_server::methods::until_account_loaded;
use safe_cex_exchange_server::session::unauthenticated_init_refusal;
use safe_cex_native::{
    BoundHttp, Events, HostEvent, HttpPrepareError, HttpPrepareFuture, Outgoing, PreparedHttp,
    ServerConfig, Transport, spawn_venue_account,
};
use safe_cex_remote_signer::{SignerLink, remote_account};
use safe_cex_venues::hosted_wallet::{self, Account};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

type Check<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
const ADDRESS: &str = "0x7E5F4552091A69125d5DfCb7b8C2659029395Bdf";
const SUBACCOUNT: &str = "9007199254740993";
const SOCKET: u64 = 7;
const WAIT: Duration = Duration::from_secs(3);

fn init() -> Check<InitPayload> {
    Ok(serde_json::from_value(json!({
        "accountName": "main", "exchangeName": "grvt", "key": "grvt-fixture-api",
        "grvtTradingAccountId": SUBACCOUNT, "grvtSignerAddress": ADDRESS,
    }))?)
}

struct Accept;
impl safe_cex_venues::Visit for Accept {
    type Output = bool;
    fn visit<V: safe_cex_engine::Venue + 'static>(
        self,
        engine: safe_cex_engine::Engine<V>,
    ) -> bool {
        !engine.account_startup()
    }
}

fn construct(options: Value, provider: Option<Provider>) -> Result<bool, String> {
    let options = serde_json::from_value(options).map_err(|error| error.to_string())?;
    let entropy = Some(safe_cex_native::production_entropy());
    let result = match provider {
        Some(provider) => {
            safe_cex_venues::build_with_signer("grvt", options, entropy, Some(provider), Accept)
        }
        None => safe_cex_venues::build("grvt", options, entropy, Accept),
    };
    result.map_err(|error| error.to_string())
}

#[test]
fn grvt_server_routing_preserves_mainnet_and_testnet_origins() -> Check {
    for (testnet, expected) in [
        (false, "https://trades.grvt.io"),
        (true, "https://trades.testnet.grvt.io"),
    ] {
        assert_eq!(
            safe_cex_native::resolve::rest_base_url("grvt", testnet).ok(),
            Some(expected)
        );
        let mut init = init()?;
        init.testnet = Some(testnet);
        assert!(prepare(&init, false, None).is_ok());
    }
    Ok(())
}

#[test]
fn grvt_private_init_forwards_signer_address_and_preserves_subaccount() -> Check {
    let init = init()?;
    let options = engine_options(&init, false);
    assert_eq!(options["grvtSignerAddress"], ADDRESS);
    assert_eq!(options["grvtTradingAccountId"], SUBACCOUNT);
    assert_eq!(serde_json::to_value(&init)?["grvtSignerAddress"], ADDRESS);
    let mut unrelated = init;
    unrelated.exchange_name = "bybit".into();
    assert!(
        engine_options(&unrelated, false)
            .get("grvtSignerAddress")
            .is_none()
    );
    Ok(())
}

#[test]
fn grvt_private_fingerprint_changes_with_signer_or_subaccount() -> Check {
    let init = init()?;
    let mut changed: Value = serde_json::to_value(&init)?;
    changed["grvtSignerAddress"] = "0x1111111111111111111111111111111111111111".into();
    let changed: InitPayload = serde_json::from_value(changed)?;
    assert_ne!(init.fingerprint(), changed.fingerprint());
    let mut changed = init.clone();
    changed.grvt_trading_account_id = Some("9007199254740994".into());
    assert_ne!(init.fingerprint(), changed.fingerprint());
    Ok(())
}

#[test]
fn grvt_unauthenticated_public_init_rejects_signer_metadata() -> Check {
    let public =
        json!({ "accountName": "__pub_bybit", "exchangeName": "bybit", "publicOnly": true });
    assert!(unauthenticated_init_refusal(&serde_json::from_value(public.clone())?).is_none());
    for value in [json!(ADDRESS), json!(""), json!(123)] {
        let mut fields = public.clone();
        fields["grvtSignerAddress"] = value;
        assert!(unauthenticated_init_refusal(&serde_json::from_value(fields)?).is_some());
    }
    Ok(())
}

#[test]
fn grvt_signer_metadata_uses_the_established_text_decoder() {
    for invalid in [json!(true), json!({ "address": ADDRESS }), json!([])] {
        assert!(
            serde_json::from_value::<InitPayload>(json!({
                "accountName": "main", "exchangeName": "grvt", "publicOnly": true,
                "grvtSignerAddress": invalid,
            }))
            .is_err()
        );
    }
}

#[test]
fn grvt_missing_or_invalid_private_signer_address_refuses_before_order_dispatch() -> Check {
    let mut fields = serde_json::to_value(init()?)?;
    for address in [Value::Null, json!(""), json!("stale-invalid-address")] {
        fields["grvtSignerAddress"] = address.clone();
        let init: InitPayload = serde_json::from_value(fields.clone())?;
        let link = SignerLink::new(
            SOCKET,
            Arc::new(|_| panic!("construction contacted device")),
        );
        let prepared = prepare(&init, false, Some(link))?;
        let remote = prepared.remote.ok_or("missing remote provider")?;
        let result = construct(
            serde_json::from_str(&prepared.options)?,
            Some(remote.provider),
        );
        let error = result.err().ok_or("invalid hosted signer was accepted")?;
        if address == json!("stale-invalid-address") {
            assert!(error.contains("Invalid GRVT signer address"));
        } else {
            assert!(error.contains("Hosted GRVT requires grvtSignerAddress"));
        }
    }
    Ok(())
}

#[test]
fn grvt_public_and_keyless_construction_never_require_wallet_metadata() -> Check {
    for (index, options) in [
        json!({}),
        json!({ "key": "grvt-fixture-api" }),
        json!({ "publicDataOnly": true, "grvtSignerAddress": "stale-invalid-address" }),
        json!({ "key": "grvt-fixture-api", "publicDataOnly": true,
            "publicMode": { "requested": true }, "grvtSignerAddress": "stale-invalid-address" }),
        json!({ "key": "grvt-fixture-api", "publicDataOnly": false,
            "publicMode": { "requested": true }, "grvtSignerAddress": "stale-invalid-address" }),
    ]
    .into_iter()
    .enumerate()
    {
        construct(options, None)
            .map_err(|error| format!("public construction case {index}: {error}"))?;
    }
    let mut options = json!({
        "key": "grvt-fixture-api", "secret": format!("{:064x}", 1u8),
        "grvtSignerAddress": "stale-invalid-address", "publicDataOnly": false,
        "publicMode": { "requested": true },
    });
    assert!(construct(options.clone(), None)?);
    options["publicMode"]["requested"] = false.into();
    assert!(!construct(options, None)?);
    Ok(())
}

#[test]
fn grvt_explicit_public_ignores_unused_signer_metadata_and_has_no_private_link_work() -> Check {
    let mut fields = serde_json::to_value(init()?)?;
    fields["publicOnly"] = true.into();
    fields["grvtSignerAddress"] = "stale-invalid-address".into();
    let public: InitPayload = serde_json::from_value(fields.clone())?;
    let prepared = prepare(&public, true, None)?;
    let options: Value = serde_json::from_str(&prepared.options)?;
    assert!(options.get("key").is_none());
    assert!(options.get("grvtSignerAddress").is_none());
    assert!(options.get("grvtTradingAccountId").is_none());
    assert!(prepared.remote.is_none());
    assert!(construct(options, None)?);
    fields["grvtSignerAddress"] = ADDRESS.into();
    fields["grvtTradingAccountId"] = "other-unused-account".into();
    let other: InitPayload = serde_json::from_value(fields)?;
    assert_eq!(public.fingerprint(), other.fingerprint());
    Ok(())
}

#[test]
fn grvt_private_payload_and_secondary_custody_stay_unsupported() -> Check {
    assert!(
        construct(
            json!({ "key": "grvt-fixture-api", "payloadSigner": true }),
            None
        )
        .is_err()
    );
    let link = SignerLink::new(SOCKET, Arc::new(|_| panic!("setup contacted device")));
    let mut options = engine_options(&init()?, false);
    options["secondary_key"] = "synthetic-secondary-api".into();
    assert!(
        remote_account(
            link.clone(),
            "grvt",
            Some("main".into()),
            &options.to_string()
        )
        .is_err()
    );
    let remote = remote_account(
        link,
        "grvt",
        Some("main".into()),
        &engine_options(&init()?, false).to_string(),
    )?;
    let mut options: Value = serde_json::from_str(&remote.options_json)?;
    options["signing"]["secondary"] = options["signing"]["primary"].clone();
    assert!(construct(options, Some(remote.provider)).is_err());
    Ok(())
}

fn device() -> Check<(Provider, Identities)> {
    // The public golden fixture identifies this synthetic scalar by ordinal.
    let mut options = serde_json::from_value(json!({
        "key": "grvt-fixture-api", "secret": format!("{:064x}", 1u8),
    }))?;
    safe_cex_venues::prepare_local_signer_for("grvt", &mut options)
        .map_err(|error| std::io::Error::other(error.to_string()).into())
}

#[test]
fn grvt_real_device_codec_matches_fixed_browser_goldens() -> Check {
    let fixtures: Value = serde_json::from_str(include_str!(
        "../../remote-signer/src/tests/fixtures/grvt-browser-goldens.json"
    ))?;
    let (provider, identities) = device()?;
    for golden in fixtures["vectors"].as_array().ok_or("missing goldens")? {
        let account = Account {
            grvt_trading_account_id: Some(SUBACCOUNT.into()),
            testnet: golden["testnet"].as_bool().ok_or("missing network")?,
            ..Account::default()
        };
        let wire = hosted_wallet::sign(
            "grvt",
            &identities,
            &account,
            &provider,
            golden["statement"].as_str().ok_or("missing statement")?,
            "grvt-order",
            1_700_000_000_000.,
        )?;
        assert_eq!(wire, golden["wire"].as_str().ok_or("missing wire")?);
    }
    Ok(())
}

#[derive(Default)]
struct Recording {
    requests: Vec<HttpRequest>,
    private_headers: Vec<Vec<(&'static str, String)>>,
    subscriptions: Vec<Value>,
}

#[derive(Clone, Default)]
struct Tape {
    recording: Arc<Mutex<Recording>>,
    ambiguous: bool,
}

struct Packet {
    tape: Tape,
    request: HttpRequest,
}
impl std::fmt::Debug for Packet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CannedGrvtPacket")
    }
}
impl PreparedHttp for Packet {
    fn bind(
        mut self: Box<Self>,
        packet: HttpRequest,
    ) -> Result<Box<dyn BoundHttp>, HttpPrepareError> {
        self.request = packet;
        Ok(self)
    }
}
impl BoundHttp for Packet {
    fn ready(&self) -> bool {
        true
    }
    fn dispatch(self: Box<Self>, events: Events) -> tokio::task::AbortHandle {
        self.tape.request(self.request, events);
        tokio::spawn(async {}).abort_handle()
    }
}

impl Transport for Tape {
    fn prepare_http(
        &self,
        request: HttpRequest,
        _: safe_cex_engine::request_context::RoutingIntent,
    ) -> HttpPrepareFuture {
        let packet: Box<dyn PreparedHttp> = Box::new(Packet {
            tape: self.clone(),
            request,
        });
        Box::pin(async move { Ok(packet) })
    }
    fn connect(
        &self,
        socket: SocketId,
        url: String,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        self.connect_channel(socket, url, vec![], None, events)
    }
    fn connect_channel(
        &self,
        socket: SocketId,
        _: String,
        headers: Vec<(&'static str, String)>,
        channel: Option<SocketChannel>,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        let (sender, mut frames) = mpsc::unbounded_channel::<String>();
        if channel == Some(SocketChannel::Private) {
            self.recording
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .private_headers
                .push(headers);
        }
        let recording = self.recording.clone();
        let _ = events.send(HostEvent::WsOpened(socket));
        tokio::task::spawn_local(async move {
            if channel == Some(SocketChannel::Private) {
                let _ = events.send(HostEvent::WsMessage(
                    socket,
                    r#"{"type":"auth_success"}"#.into(),
                ));
            }
            while let Some(frame) = frames.recv().await {
                if let Ok(value) = serde_json::from_str::<Value>(&frame) {
                    recording
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .subscriptions
                        .push(value);
                }
            }
        });
        sender
    }
    fn request(&self, request: HttpRequest, events: Events) {
        let (status, headers, body) = if request.url.ends_with("all_instruments") {
            (
                200,
                "",
                r#"{"r":[{"i":"BTC_USDT_Perp","ih":"0x030501","b":"BTC","q":"USDT","k":"PERPETUAL","ts":"0.1","ms":"0.001","mp":"1000","bd":9,"qd":6}]}"#,
            )
        } else if request.url.ends_with("auth/api_key/login") {
            (
                200,
                "Set-Cookie: gravity=synthetic-session; Path=/\nX-Grvt-Account-Id: synthetic-account",
                r#"{"sub_account_id":"9007199254740993"}"#,
            )
        } else if request.url.ends_with("aggregated_account_summary") {
            (
                200,
                "",
                r#"{"r":{"te":"1000","ts2":"1000","tu":"0","sas":[{"si":"9007199254740993"}]}}"#,
            )
        } else if request.url.ends_with("get_authorized_builders") {
            (
                200,
                "",
                r#"{"results":[{"builder_account_id":"0xee8f3559f298fffa79b8253437969d15b5dccff3","max_futures_fee_rate":"0.03"}]}"#,
            )
        } else if request.url.ends_with("get_all_initial_leverage") {
            (200, "", r#"{"r":[{"i":"BTC_USDT_Perp","l":"10"}]}"#)
        } else if request.url.ends_with("create_order") && self.ambiguous {
            (
                429,
                "",
                r#"{"code":1000,"message":"ambiguous synthetic response"}"#,
            )
        } else if request.url.ends_with("create_order") {
            (200, "", r#"{"r":{"oi":"0x1234"}}"#)
        } else {
            (200, "", r#"{"r":[]}"#)
        };
        let id = request.id;
        self.recording
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .requests
            .push(request);
        let _ = events.send(HostEvent::HttpStart {
            request: id,
            status,
            headers: headers.into(),
        });
        let _ = events.send(HostEvent::HttpChunk(id, body.as_bytes().to_vec()));
        let _ = events.send(HostEvent::HttpEnd(id));
    }
}

fn order() -> PlaceOrderOpts {
    PlaceOrderOpts {
        symbol: "BTC_USDT_PERP".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        amount: 1.001,
        price: Some(50000.1),
        time_in_force: Some(TimeInForce::GoodTillCancel),
        reduce_only: Some(false),
        client_order_id: Some("9223372036854775807".into()),
        trigger_price: None,
        stop_loss: None,
        take_profit: None,
        trigger_type: None,
        tp_trigger_type: None,
        sl_trigger_type: None,
        max_slippage_pct: None,
        bybit_bbo: None,
        bybit_slippage_tolerance_ticks: None,
        implied_volatility: None,
        close: None,
        stp_mode: None,
    }
}

async fn done(client: &mut safe_cex_native::ClientLink) -> Check<Value> {
    tokio::time::timeout(WAIT, async {
        while let Some(message) = client.outgoing.recv().await {
            if let Outgoing::Text(text) = message {
                let value: Value = serde_json::from_str(&text)?;
                if value["type"] == "done" {
                    return Ok(value);
                }
                if value["type"] == "startupFailed" {
                    return Err("canned startup failed".into());
                }
            }
        }
        Err("client closed before completion".into())
    })
    .await?
}

async fn exercise(mode: &str) -> Check {
    let (sender, mut frames) = mpsc::unbounded_channel::<String>();
    let link = SignerLink::new(SOCKET, Arc::new(move |frame| sender.send(frame).is_ok()));
    let answering = Arc::downgrade(&link);
    let seen = Arc::new(Mutex::new(Vec::<Value>::new()));
    let record = seen.clone();
    let mode = mode.to_owned();
    let device_mode = mode.clone();
    let responder = tokio::spawn(async move {
        let (provider, identities) = device()?;
        let account = Account {
            grvt_trading_account_id: Some(SUBACCOUNT.into()),
            ..Account::default()
        };
        while let Some(frame) = frames.recv().await {
            let request: Value = serde_json::from_str(&frame)?;
            let request = &request["d"];
            record
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .push(request.clone());
            let mut signature = hosted_wallet::sign(
                "grvt",
                &identities,
                &account,
                &provider,
                request["wsAuth"]["toSign"]
                    .as_str()
                    .ok_or("missing statement")?,
                request["wsAuth"]["outputFormat"]
                    .as_str()
                    .ok_or("missing format")?,
                safe_cex_native::now_ms(),
            )?;
            if device_mode == "wrong-address" {
                signature.replace_range(2..42, "1111111111111111111111111111111111111111");
            } else if device_mode == "wrong-signature" {
                signature.replace_range(42..172, &"00".repeat(65));
            }
            if let Some(link) = answering.upgrade() {
                assert!(link.resolve(
                    SOCKET,
                    json!({ "id": request["id"], "wsAuthSignature": signature })
                ));
            }
        }
        Check::Ok(())
    });
    let tape = Tape {
        ambiguous: mode == "ambiguous",
        ..Tape::default()
    };
    let prepared = prepare(&init()?, false, Some(link.clone()))?;
    let options: Value = serde_json::from_str(&prepared.options)?;
    for field in ["key", "secret", "localSigner", "omniKeySeed"] {
        assert!(options.get(field).is_none());
    }
    let remote = prepared.remote.ok_or("missing remote provider")?;
    let provider = remote.provider.clone();
    drop(remote);
    let (handle, actor) = spawn_venue_account(
        "grvt",
        &prepared.options,
        tape.clone(),
        Some(provider),
        ServerConfig::default(),
    )?;
    assert_eq!(options["grvtSignerAddress"], ADDRESS);
    assert_eq!(options["grvtTradingAccountId"], SUBACCOUNT);
    let mut client = handle.join(1, true).await?;
    until_account_loaded(&handle, WAIT)
        .await
        .map_err(|_| "private reads did not load")?;
    assert!(handle.command(
        1,
        Command::PlaceOrders {
            command: 9,
            orders: vec![order()]
        }
    ));
    let outcome = done(&mut client).await?;
    if mode == "valid" {
        assert!(
            outcome.get("error").is_none(),
            "valid native signing refused"
        );
    } else {
        assert!(outcome["error"].is_string());
        let expected = if mode == "ambiguous" {
            "unknown"
        } else {
            "notSent"
        };
        assert_eq!(outcome["detail"]["outcome"], expected);
    }
    // Give queued wakes another turn: none may admit another physical create.
    tokio::time::sleep(Duration::from_millis(50)).await;
    handle.dispose();
    tokio::time::timeout(WAIT, actor).await???;
    drop(handle);
    drop(link);
    tokio::time::timeout(WAIT, responder).await???;
    let recording = tape.recording.lock().unwrap_or_else(|p| p.into_inner());
    let creates: Vec<_> = recording
        .requests
        .iter()
        .filter(|r| r.url.ends_with("create_order"))
        .collect();
    assert_eq!(
        creates.len(),
        usize::from(mode == "valid" || mode == "ambiguous")
    );
    assert_eq!(
        recording
            .requests
            .iter()
            .filter(|r| r.url.ends_with("auth/api_key/login"))
            .count(),
        1
    );
    assert_eq!(recording.private_headers.len(), 1);
    let login = recording
        .requests
        .iter()
        .find(|r| r.url.ends_with("auth/api_key/login"))
        .ok_or("missing login")?;
    let login_body: Value =
        serde_json::from_str(login.body.as_deref().ok_or("missing login body")?)?;
    assert_eq!(login_body, json!({ "api_key": "grvt-fixture-api" }));
    assert!(
        recording.private_headers[0]
            .iter()
            .any(|(name, value)| name.eq_ignore_ascii_case("cookie")
                && value == "rm=true; gravity=synthetic-session")
    );
    assert!(recording.private_headers[0].iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("x-grvt-account-id") && value == "synthetic-account"
    }));
    assert!(
        recording.private_headers[0]
            .iter()
            .all(|(name, _)| !name.to_ascii_lowercase().starts_with("x-ts-grvt"))
    );
    for stream in ["v1.order", "v1.position", "v1.fill"] {
        assert!(
            recording
                .subscriptions
                .iter()
                .any(|v| v["stream"] == stream && v["feed"] == json!([SUBACCOUNT]))
        );
    }
    let seen = seen.lock().unwrap_or_else(|p| p.into_inner());
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0]["exchangeName"], "grvt");
    assert_eq!(seen[0]["accountName"], "main");
    assert_eq!(seen[0]["wsAuth"]["outputFormat"], "grvt-order");
    if let Some(create) = creates.first() {
        assert!(
            create
                .headers
                .iter()
                .any(|(name, value)| name.eq_ignore_ascii_case("cookie")
                    && value == "rm=true; gravity=synthetic-session")
        );
        let body: Value =
            serde_json::from_str(create.body.as_deref().ok_or("missing create body")?)?;
        assert_eq!(body["o"]["sa"], SUBACCOUNT);
        assert_eq!(body["o"]["m"]["co"], "9223372036854775807");
        assert_eq!(body["o"]["s"]["s"], ADDRESS);
        assert_eq!(body["o"]["s"]["ci"], "325");
        let statement: Value = serde_json::from_str(
            seen[0]["wsAuth"]["toSign"]
                .as_str()
                .ok_or("missing statement")?,
        )?;
        assert_eq!(body["o"]["s"]["e"], statement["expiration"]);
        assert_eq!(body["o"]["s"]["n"], statement["nonce"]);
    }
    Ok(())
}

#[tokio::test]
async fn a_keyless_grvt_engine_places_through_server_init_remote_link_and_real_device_codec()
-> Check {
    tokio::task::LocalSet::new()
        .run_until(exercise("valid"))
        .await
}

#[tokio::test]
async fn grvt_wrong_device_address_or_signature_never_releases_http() -> Check {
    tokio::task::LocalSet::new()
        .run_until(async {
            exercise("wrong-address").await?;
            exercise("wrong-signature").await
        })
        .await
}

#[tokio::test]
async fn grvt_ambiguous_native_create_stays_unknown_without_resend() -> Check {
    tokio::task::LocalSet::new()
        .run_until(exercise("ambiguous"))
        .await
}
