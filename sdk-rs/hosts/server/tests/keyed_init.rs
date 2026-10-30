//! Every core venue's keyed init, exactly as NX `buildInitPayload` sends it,
//! builds a private remote-signed engine through the server's own `prepare`
//! (#2450 failed here for every venue). No network is used, so only venues
//! that sign before loading markets are asserted to send a sign-req.
use safe_cex_engine::{HttpRequest, SocketId};
use safe_cex_exchange_server::accounts::{Prepared, prepare};
use safe_cex_exchange_server::envelope::{Inbound, InitPayload};
use safe_cex_native::{Events, HostEvent, ServerConfig, Transport, spawn_venue_account};
use safe_cex_remote_signer::SignerLink;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

const WALLET: &str = "0x1111111111111111111111111111111111111111";
const BUILDER: &str = "0x2222222222222222222222222222222222222222";

/// Sockets open at once; HTTP fails, so only signing can make progress.
struct Offline;
impl Transport for Offline {
    fn connect(
        &self,
        socket: SocketId,
        _: String,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        let _ = events.send(HostEvent::WsOpened(socket));
        mpsc::unbounded_channel().0
    }
    fn connect_with_headers(
        &self,
        socket: SocketId,
        url: String,
        _: Vec<(&'static str, String)>,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        self.connect(socket, url, events)
    }
    fn request(&self, request: HttpRequest, events: Events) {
        let _ = events.send(HostEvent::HttpFailed(request.id, "offline".into()));
    }
}

/// The venue-specific fields `accountToExchangeOptions` forwards, then
/// `buildInitPayload`'s allowlist, as the browser serializes them.
fn browser_init(venue: &str, proxied: bool) -> String {
    let mut fields = json!({
        "accountName": format!("{venue}-main"),
        "exchangeName": venue,
        "key": "api-key",
        "tealstreetProxy": proxied,
        "tealstreetTunnel": proxied,
        // JSON numbers from JS carry no integer type; 10 is the default tier.
        "hyperliquidBuilder": { "address": BUILDER, "feeTenthsBps": 10 },
    });
    let extra = match venue {
        "okx" | "bitget" | "blofin" => json!({ "passphrase": "pass" }),
        "hyperliquid" | "hyperliquidspot" => json!({ "key": WALLET }),
        "lighter" => json!({ "applicationId": "281474976710654", "apiKeyIndex": "3" }),
        "paradex" => json!({ "paradexAddress": WALLET, "subkeyAddress": WALLET }),
        _ => json!({}),
    };
    for (field, value) in extra.as_object().unwrap() {
        fields[field] = value.clone();
    }
    if proxied {
        fields["proxyRegion"] = "SG".into();
    }
    json!({ "t": "init", "d": fields }).to_string()
}

fn parse(text: &str) -> InitPayload {
    match serde_json::from_str::<Inbound>(text) {
        Ok(Inbound::Init(init)) => *init,
        Ok(_) => panic!("not an init"),
        Err(error) => panic!("init did not parse: {error}: {text}"),
    }
}

/// Venues whose private startup signs before any market data loads.
const SIGNS_FIRST: [&str; 2] = ["hyperliquid", "paradex"];

/// Builds the account; for `SIGNS_FIRST` venues, also awaits the first sign-req.
async fn build_private(init: &InitPayload) -> Result<(), String> {
    let (sender, mut frames) = mpsc::unbounded_channel::<String>();
    let link = SignerLink::new(1, Arc::new(move |frame| sender.send(frame).is_ok()));
    let Prepared {
        options, remote, ..
    } = prepare(init, false, Some(link.clone()))?;
    let remote = remote.expect("a keyed init signs remotely");
    let built: Value = serde_json::from_str(&options).unwrap();
    assert!(built.get("key").is_none(), "{built}");
    assert!(built["signing"]["primary"].is_object(), "{built}");
    assert_ne!(built["publicMode"]["requested"], true, "{built}");
    let (handle, _task) = spawn_venue_account(
        &init.exchange_name,
        &options,
        Offline,
        Some(remote.provider.clone()),
        ServerConfig::default(),
    )
    .map_err(|error| format!("engine did not build: {error}"))?;
    let signed = if SIGNS_FIRST.contains(&init.exchange_name.as_str()) {
        let frame = tokio::time::timeout(Duration::from_secs(5), frames.recv())
            .await
            .map_err(|_| "no sign-req within 5s".to_owned())?
            .ok_or("signer link closed")?;
        let frame: Value = serde_json::from_str(&frame).unwrap();
        (frame["t"] == "sign-req" && frame["d"]["exchangeName"] == init.exchange_name)
            .then_some(())
            .ok_or(format!("not this venue's sign-req: {frame}"))
    } else {
        Ok(())
    };
    handle.dispose();
    signed
}

#[tokio::test(flavor = "current_thread")]
async fn every_core_venue_keyed_init_builds_a_private_engine() {
    const VENUES: [&str; 13] = [
        "binance",
        "binancespot",
        "bybit",
        "bybitspot",
        "okx",
        "bitget",
        "hyperliquid",
        "hyperliquidspot",
        "blofin",
        "bingx",
        "lighter",
        "paradex",
        "phemex",
    ];
    let local = tokio::task::LocalSet::new();
    let failures = Arc::new(Mutex::new(Vec::new()));
    let record = failures.clone();
    local
        .run_until(async move {
            for venue in VENUES {
                for proxied in [false, true] {
                    let init = parse(&browser_init(venue, proxied));
                    if let Err(error) = build_private(&init).await {
                        record
                            .lock()
                            .unwrap()
                            .push(format!("{venue} (proxied {proxied}): {error}"));
                    }
                }
            }
        })
        .await;
    let failures = failures.lock().unwrap();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
