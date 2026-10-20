//! WebAssembly host surface for browser workers and Cloudflare Workers.
//!
//! JavaScript owns sockets, fetch and timers; it forwards each event here and
//! executes the returned actions. Every call returns the pending actions as a
//! JSON array string, or an empty string when there are none. `wsMessageActions`
//! and `wsMessageBytes` return `undefined` instead, avoiding empty-string decoding.
#[cfg(not(any(
    feature = "grvt",
    feature = "paradex",
    feature = "bitrue",
    feature = "coinbase",
    feature = "binance",
    feature = "apexomni",
    feature = "asterdex",
    feature = "breakoutprop",
    feature = "kucoin",
    feature = "kucoinspot",
    feature = "polymarket",
    feature = "blofin",
    feature = "bybit",
    feature = "woo",
    feature = "cryptocom",
    feature = "bingx",
    feature = "mexc",
    feature = "bitget",
    feature = "backpack",
    feature = "phemex",
    feature = "weex",
    feature = "hyperliquid",
    feature = "kraken",
    feature = "txflow",
    feature = "orderly",
    feature = "lighter",
    feature = "okx"
)))]
compile_error!(
    "safe-cex-wasm requires at least one venue feature: breakoutprop, paradex, apexomni, asterdex, bitrue, binance, kucoin, kucoinspot, bybit, cryptocom, bingx, blofin, mexc, backpack, bitget, grvt, hyperliquid, kraken, txflow, coinbase, lighter, orderly, polymarket, okx, phemex, woo or weex"
);

use safe_cex_engine::{
    Action, Engine, Event, LogLevel, Notice, RequestId, SocketId, TimerId, Venue,
};
#[cfg(feature = "apexomni")]
use safe_cex_venue_apexomni::{ApexOmni, Options as ApexOmniOptions};
#[cfg(feature = "asterdex")]
use safe_cex_venue_asterdex::{AsterDex, Options as AsterOptions};
#[cfg(feature = "backpack")]
use safe_cex_venue_backpack::{Backpack, Options as BackpackOptions};
#[cfg(feature = "binance")]
use safe_cex_venue_binance::{
    BinanceFutures, Options as BinanceOptions, Variant as BinanceVariant,
};
#[cfg(feature = "bingx")]
use safe_cex_venue_bingx::{Bingx, Options as BingxOptions};
#[cfg(feature = "bitget")]
use safe_cex_venue_bitget::{BitgetFutures, Options as BitgetOptions, Product};
#[cfg(feature = "bitrue")]
use safe_cex_venue_bitrue::{Bitrue, Options as BitrueOptions};
#[cfg(feature = "blofin")]
use safe_cex_venue_blofin::{Blofin, Options as BlofinOptions};
#[cfg(feature = "breakoutprop")]
use safe_cex_venue_breakoutprop::{BreakoutProp, Options as BreakoutOptions};
#[cfg(feature = "bybit")]
use safe_cex_venue_bybit::{
    BybitLinear, Endpoints as BybitEndpoints, Options as BybitOptions, Variant as BybitVariant,
};
#[cfg(feature = "coinbase")]
use safe_cex_venue_coinbase::{Coinbase, Options as CoinbaseOptions};
#[cfg(feature = "cryptocom")]
use safe_cex_venue_cryptocom::{Cryptocom, Options as CryptocomOptions};
#[cfg(feature = "grvt")]
use safe_cex_venue_grvt::{Grvt, Options as GrvtOptions};
#[cfg(feature = "hyperliquid")]
use safe_cex_venue_hyperliquid::{Builder, Hyperliquid, Options as HyperliquidOptions};
#[cfg(feature = "kraken")]
use safe_cex_venue_kraken::{Kraken, KrakenSpot, Options as KrakenOptions};
#[cfg(feature = "kucoin")]
use safe_cex_venue_kucoin::Kucoin;
#[cfg(any(feature = "kucoin", feature = "kucoinspot"))]
use safe_cex_venue_kucoin::Options as KucoinOptions;
#[cfg(feature = "kucoinspot")]
use safe_cex_venue_kucoin::spot::KucoinSpot;
#[cfg(feature = "lighter")]
use safe_cex_venue_lighter::{Lighter, Options as LighterOptions};
#[cfg(feature = "mexc")]
use safe_cex_venue_mexc::{MexcFutures, Options as MexcOptions};
#[cfg(feature = "okx")]
use safe_cex_venue_okx::{Okx, Options as OkxOptions};
#[cfg(feature = "orderly")]
use safe_cex_venue_orderly::{Broker as OrderlyBroker, Options as OrderlyOptions, Orderly};
#[cfg(feature = "paradex")]
use safe_cex_venue_paradex::{Options as ParadexOptions, Paradex};
#[cfg(feature = "phemex")]
use safe_cex_venue_phemex::{Options as PhemexOptions, PhemexPerpetuals};
#[cfg(feature = "polymarket")]
use safe_cex_venue_polymarket::{
    Options as PolymarketOptions, Polymarket, PolymarketOptions as PolymarketAccountOptions,
};
#[cfg(feature = "weex")]
use safe_cex_venue_weex::{Options as WeexOptions, WeexFutures};
#[cfg(feature = "woo")]
use safe_cex_venue_woo::{Options as WooOptions, Woo};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

trait AnyEngine {
    #[cfg(feature = "coinbase")]
    fn account_id(&self) -> Option<String>;
    fn set_public_feed_policy(
        &mut self,
        policy: safe_cex_engine::PublicFeedPolicy,
    ) -> Result<(), &'static str>;
    fn apply_public_feed(
        &mut self,
        now: f64,
        update: safe_cex_engine::PublicFeedUpdate,
    ) -> Result<(), &'static str>;
    fn start(&mut self, now: f64);
    fn handle(&mut self, now: f64, event: Event<'_>);
    fn drain(&mut self) -> Vec<Action>;
    fn frame(&mut self, now: f64, market_only: bool) -> Option<&[u8]>;
    fn snapshot(&mut self, now: f64) -> &[u8];
}

impl<V: Venue> AnyEngine for Engine<V> {
    #[cfg(feature = "coinbase")]
    fn account_id(&self) -> Option<String> {
        Engine::account_id(self).map(str::to_owned)
    }
    fn set_public_feed_policy(
        &mut self,
        policy: safe_cex_engine::PublicFeedPolicy,
    ) -> Result<(), &'static str> {
        Engine::set_public_feed_policy(self, policy)
    }
    fn apply_public_feed(
        &mut self,
        now: f64,
        update: safe_cex_engine::PublicFeedUpdate,
    ) -> Result<(), &'static str> {
        Engine::apply_public_feed(self, now, update)
    }
    fn start(&mut self, now: f64) {
        Engine::start(self, now);
    }

    fn handle(&mut self, now: f64, event: Event<'_>) {
        Engine::handle(self, now, event);
    }

    fn drain(&mut self) -> Vec<Action> {
        self.drain_actions().collect()
    }

    fn frame(&mut self, now: f64, market_only: bool) -> Option<&[u8]> {
        if market_only {
            self.take_market_frame(now)
        } else {
            self.take_frame(now)
        }
    }

    fn snapshot(&mut self, now: f64) -> &[u8] {
        self.snapshot_frame(now)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PublicFeedOptions {
    mode: safe_cex_engine::PublicFeedPolicy,
}

#[cfg(any(feature = "kucoin", feature = "kucoinspot"))]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct KucoinExtra {
    #[serde(default)]
    kucoin_broker: Option<safe_cex_venue_kucoin::sign::BrokerOptions>,
}

#[cfg(any(
    feature = "coinbase",
    feature = "paradex",
    feature = "binance",
    feature = "apexomni",
    feature = "asterdex",
    feature = "breakoutprop",
    feature = "kucoin",
    feature = "kucoinspot",
    feature = "polymarket",
    feature = "blofin",
    feature = "bybit",
    feature = "woo",
    feature = "cryptocom",
    feature = "bingx",
    feature = "mexc",
    feature = "bitget",
    feature = "grvt",
    feature = "bitrue",
    feature = "backpack",
    feature = "phemex",
    feature = "weex",
    feature = "hyperliquid",
    feature = "kraken",
    feature = "txflow",
    feature = "orderly",
    feature = "lighter",
    feature = "okx"
))]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EngineOptions {
    #[serde(default)]
    private_only: bool,
    #[serde(default)]
    public_feed: Option<PublicFeedOptions>,
    #[serde(default)]
    read_credentials: Option<safe_cex_engine::ReadCredentials>,
    #[cfg(feature = "bybit")]
    #[serde(default)]
    enable_inverse_markets: bool,
    #[cfg(feature = "bybit")]
    #[serde(default)]
    enable_option_markets: bool,
    #[cfg(feature = "coinbase")]
    #[serde(default)]
    spot_only: bool,
    #[cfg(feature = "polymarket")]
    #[serde(default)]
    polymarket: Option<PolymarketAccountOptions>,
    #[cfg(feature = "lighter")]
    #[serde(default)]
    api_key_index: Option<String>,
    #[cfg(feature = "lighter")]
    #[serde(default)]
    #[serde(deserialize_with = "optional_lighter_index")]
    lighter_integrator_account_index: Option<u64>,
    #[cfg(feature = "lighter")]
    #[serde(default)]
    lighter_stop_slippage_pct: Option<f64>,
    #[cfg(feature = "lighter")]
    #[serde(default)]
    lighter_remote_signer: bool,
    #[cfg(feature = "lighter")]
    lighter_signer_provider: Option<String>,
    #[cfg(any(
        feature = "grvt",
        feature = "coinbase",
        feature = "paradex",
        feature = "binance",
        feature = "apexomni",
        feature = "asterdex",
        feature = "kucoin",
        feature = "kucoinspot",
        feature = "blofin",
        feature = "bybit",
        feature = "woo",
        feature = "cryptocom",
        feature = "hyperliquid",
        feature = "kraken",
        feature = "txflow",
        feature = "phemex",
        feature = "orderly",
        feature = "lighter",
        feature = "bitget",
        feature = "okx"
    ))]
    #[serde(default)]
    testnet: bool,
    #[serde(default)]
    public_data_only: bool,
    #[serde(default)]
    key: Option<String>,
    #[cfg(any(
        feature = "grvt",
        feature = "coinbase",
        feature = "binance",
        feature = "apexomni",
        feature = "asterdex",
        feature = "breakoutprop",
        feature = "kucoin",
        feature = "kucoinspot",
        feature = "blofin",
        feature = "bybit",
        feature = "cryptocom",
        feature = "bingx",
        feature = "mexc",
        feature = "bitget",
        feature = "bitrue",
        feature = "backpack",
        feature = "phemex",
        feature = "weex",
        feature = "hyperliquid",
        feature = "txflow",
        feature = "kraken",
        feature = "lighter",
        feature = "woo",
        feature = "orderly",
        feature = "okx"
    ))]
    #[serde(default)]
    secret: Option<String>,
    #[cfg(feature = "paradex")]
    #[serde(default)]
    paradex_address: Option<String>,
    #[cfg(feature = "paradex")]
    #[serde(default)]
    subkey_address: Option<String>,
    #[cfg(feature = "paradex")]
    #[serde(default)]
    payload_signer: bool,
    #[cfg(any(feature = "backpack", feature = "bitrue", feature = "breakoutprop"))]
    #[serde(default)]
    recv_window: Option<f64>,
    #[cfg(any(
        feature = "kucoin",
        feature = "kucoinspot",
        feature = "okx",
        feature = "weex",
        feature = "blofin",
        feature = "bitget",
        feature = "apexomni"
    ))]
    #[serde(default)]
    passphrase: Option<String>,
    #[cfg(feature = "apexomni")]
    #[serde(default)]
    omni_key_seed: Option<String>,
    #[cfg(any(
        feature = "hyperliquid",
        feature = "txflow",
        feature = "asterdex",
        feature = "orderly",
        feature = "lighter",
        feature = "woo"
    ))]
    #[serde(default)]
    application_id: Option<String>,
    #[cfg(any(feature = "hyperliquid", feature = "txflow"))]
    #[serde(default)]
    vault: Option<String>,
    #[cfg(feature = "hyperliquid")]
    #[serde(default)]
    hyperliquid_builder: Option<HyperliquidBuilder>,
    #[cfg(feature = "hyperliquid")]
    #[serde(default)]
    enable_spot_markets: bool,
    #[cfg(feature = "asterdex")]
    #[serde(default)]
    asterdex: AsterConfig,

    #[cfg(any(feature = "kucoin", feature = "kucoinspot"))]
    #[serde(default)]
    extra: Option<KucoinExtra>,
    #[cfg(feature = "weex")]
    #[serde(default)]
    private_ws_headers: bool,
    #[cfg(feature = "grvt")]
    #[serde(default)]
    grvt_trading_account_id: Option<String>,
    #[cfg(feature = "grvt")]
    #[serde(default)]
    grvt_signer_address: Option<String>,
    #[cfg(feature = "grvt")]
    #[serde(default)]
    grvt_session_proxy: bool,
    #[cfg(feature = "bybit")]
    #[serde(default)]
    endpoints: Option<EndpointOptions>,
    #[cfg(any(feature = "orderly", feature = "breakoutprop"))]
    #[serde(default)]
    tick_interval: Option<f64>,
    #[cfg(feature = "breakoutprop")]
    #[serde(default)]
    subaccount: Option<String>,
    #[cfg(feature = "breakoutprop")]
    #[serde(default)]
    polling_enabled: Option<bool>,
}
#[cfg(feature = "asterdex")]
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct AsterConfig {
    api_version: Option<safe_cex_venue_asterdex::sign::Version>,
    user_address: Option<String>,
    signer_address: Option<String>,
    private_key: Option<String>,
    builder_address: Option<String>,
    builder_fee_rate_maker: Option<String>,
    builder_fee_rate_taker: Option<String>,
    read_key: Option<String>,
    read_secret: Option<String>,
}

#[cfg(feature = "bybit")]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EndpointOptions {
    rest: String,
    public_ws: String,
    private_ws: String,
}
#[cfg(feature = "hyperliquid")]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HyperliquidBuilder {
    address: String,
    fee_tenths_bps: u32,
}

#[cfg(feature = "lighter")]
fn optional_lighter_index<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Index {
        Number(u64),
        Text(String),
    }
    Option::<Index>::deserialize(d)?
        .map(|index| match index {
            Index::Number(n) => Ok(n),
            Index::Text(s) => s.parse::<u64>().map_err(serde::de::Error::custom),
        })
        .transpose()
}

#[derive(Serialize)]
#[serde(tag = "op", rename_all = "camelCase")]
enum ActionOut<'a> {
    Deactivate,
    WsConnect {
        #[serde(skip_serializing_if = "is_false")]
        binary: bool,
        socket: u32,
        url: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        channel: Option<&'static str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        context: Option<&'a safe_cex_engine::SocketContext>,
        #[serde(skip_serializing_if = "Option::is_none")]
        headers: Option<Vec<(&'static str, &'a str)>>,
    },
    WsSend {
        socket: u32,
        text: &'a str,
    },
    WsClose {
        socket: u32,
    },
    Http {
        request: u32,
        method: &'static str,
        #[serde(skip_serializing_if = "Option::is_none")]
        route: Option<&'static str>,
        #[serde(rename = "timeoutMs", skip_serializing_if = "Option::is_none")]
        timeout_ms: Option<u32>,
        url: &'a str,
        headers: Vec<(&'a str, &'a str)>,
        #[serde(skip_serializing_if = "Option::is_none")]
        body: Option<&'a str>,
    },
    SignPayload {
        request: u32,
        payload: &'a safe_cex_engine::PayloadSignRequest,
        #[serde(rename = "timeoutMs")]
        timeout_ms: u32,
    },
    SetTimer {
        timer: u32,
        at: f64,
    },
    ClearTimer {
        timer: u32,
    },
    Log {
        level: &'static str,
        message: &'a str,
    },
    Done {
        command: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<&'a str>,
    },
}

fn socket_channel(channel: &safe_cex_engine::SocketChannel) -> &'static str {
    match channel {
        safe_cex_engine::SocketChannel::Public => "public",
        safe_cex_engine::SocketChannel::Private => "private",
        safe_cex_engine::SocketChannel::Business => "business",
    }
}

fn encode(actions: &[Action]) -> String {
    if actions.is_empty() {
        return String::new();
    }
    let out: Vec<ActionOut<'_>> = actions
        .iter()
        .map(|action| match action {
            Action::WsConnect {
                binary,
                socket,
                url,
                channel,
                context,
            } => ActionOut::WsConnect {
                binary: *binary,
                socket: socket.0,
                url,
                channel: channel.as_ref().map(socket_channel),
                context: context.as_ref(),
                headers: None,
            },
            Action::WsConnectWithHeaders {
                socket,
                url,
                headers,
                channel,
                context,
            } => ActionOut::WsConnect {
                binary: false,
                socket: socket.0,
                url,
                channel: channel.as_ref().map(socket_channel),
                context: context.as_ref(),
                headers: Some(headers.iter().map(|(k, v)| (*k, v.as_str())).collect()),
            },
            Action::WsSend { socket, text } => ActionOut::WsSend {
                socket: socket.0,
                text,
            },
            Action::WsClose { socket } => ActionOut::WsClose { socket: socket.0 },
            Action::Http(request) => ActionOut::Http {
                request: request.id.0,
                method: request.method.as_str(),
                route: request.route.map(safe_cex_engine::RequestRoute::as_str),
                timeout_ms: request.timeout_ms,
                url: &request.url,
                headers: request
                    .headers
                    .iter()
                    .map(|(name, value)| (*name, value.as_str()))
                    .chain(
                        request
                            .signer_headers
                            .iter()
                            .map(|(name, value)| (name.as_str(), value.as_str())),
                    )
                    .collect(),
                body: request.body.as_deref(),
            },
            Action::SignPayload {
                request,
                payload,
                timeout_ms,
            } => ActionOut::SignPayload {
                request: request.0,
                payload,
                timeout_ms: *timeout_ms,
            },
            Action::SetTimer { timer, at_ms } => ActionOut::SetTimer {
                timer: timer.0,
                at: *at_ms,
            },
            Action::ClearTimer { timer } => ActionOut::ClearTimer { timer: timer.0 },
            Action::Notice(Notice::Deactivate) => ActionOut::Deactivate,
            Action::Notice(Notice::Log { level, message }) => ActionOut::Log {
                level: match level {
                    LogLevel::Info => "info",
                    LogLevel::Warning => "warning",
                    LogLevel::Error => "error",
                },
                message,
            },
            Action::Notice(Notice::CommandDone { command, result }) => ActionOut::Done {
                command: *command,
                result: result.as_ref().ok().map(String::as_str),
                error: result.as_ref().err().map(String::as_str),
            },
        })
        .collect();
    serde_json::to_string(&out).unwrap_or_default()
}

/// Bytes of WebAssembly linear memory currently reserved by this module.
#[wasm_bindgen(js_name = wasmMemoryBytes)]
pub fn wasm_memory_bytes() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        (core::arch::wasm32::memory_size(0) * 65_536) as f64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        0.0
    }
}

#[wasm_bindgen]
pub struct WasmEngine {
    inner: Box<dyn AnyEngine>,
}

#[wasm_bindgen]
impl WasmEngine {
    #[cfg(any(
        feature = "coinbase",
        feature = "paradex",
        feature = "binance",
        feature = "apexomni",
        feature = "asterdex",
        feature = "breakoutprop",
        feature = "kucoin",
        feature = "kucoinspot",
        feature = "polymarket",
        feature = "blofin",
        feature = "bybit",
        feature = "woo",
        feature = "cryptocom",
        feature = "bingx",
        feature = "mexc",
        feature = "bitget",
        feature = "grvt",
        feature = "bitrue",
        feature = "backpack",
        feature = "phemex",
        feature = "weex",
        feature = "hyperliquid",
        feature = "kraken",
        feature = "txflow",
        feature = "orderly",
        feature = "lighter",
        feature = "okx"
    ))]
    #[wasm_bindgen(constructor)]
    pub fn new(venue: &str, options_json: &str) -> Result<WasmEngine, JsError> {
        let options: EngineOptions = serde_json::from_str(options_json)?;
        if let Some(read) = &options.read_credentials {
            read.validate().map_err(JsError::new)?;
        }
        #[cfg(feature = "weex")]
        if venue == "weex"
            && !options.public_data_only
            && options.passphrase.as_deref().is_none_or(str::is_empty)
        {
            return Err(JsError::new("WEEX requires a passphrase"));
        }
        #[cfg(feature = "kraken")]
        if matches!(venue, "kraken" | "krakenspot")
            && !options.public_data_only
            && options.key.as_deref().is_some_and(|key| !key.is_empty())
            && let Some(secret) = options.secret.as_deref()
        {
            safe_cex_venue_kraken::sign::Signer::new(secret)
                .map_err(|error| JsError::new(&error))?;
        }
        let policy = options
            .public_feed
            .as_ref()
            .map(|feed| feed.mode)
            .unwrap_or(if options.private_only {
                safe_cex_engine::PublicFeedPolicy::Disabled
            } else {
                safe_cex_engine::PublicFeedPolicy::Direct
            });
        if options.private_only && policy == safe_cex_engine::PublicFeedPolicy::Direct {
            return Err(JsError::new("privateOnly cannot use direct public feeds"));
        }
        let mut inner: Box<dyn AnyEngine> = match venue {
            #[cfg(feature = "breakoutprop")]
            "breakoutprop" => Box::new(Engine::new(BreakoutProp::new(BreakoutOptions {
                key: options.key.unwrap_or_default(),
                secret: options.secret.unwrap_or_default(),
                subaccount: options.subaccount,
                public_data_only: options.public_data_only,
                tick_interval: if options.polling_enabled == Some(false) {
                    f64::INFINITY
                } else {
                    options.tick_interval.unwrap_or(2_000.0)
                },
                timeout_ms: options
                    .recv_window
                    .unwrap_or(20_000.0)
                    .clamp(1.0, u32::MAX as f64) as u32,
            }))),
            #[cfg(feature = "grvt")]
            "grvt" => Box::new(Engine::new(Grvt::new(GrvtOptions {
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
                trading_account_id: options.grvt_trading_account_id,
                signer_address: options.grvt_signer_address,
                session_proxy: options.grvt_session_proxy,
            }))),
            #[cfg(feature = "coinbase")]
            "coinbase" | "coinbasespot" => Box::new(Engine::new(
                Coinbase::new(CoinbaseOptions {
                    key: options.key.unwrap_or_default(),
                    secret: options.secret.unwrap_or_default(),
                    public_data_only: options.public_data_only,
                    spot_only: venue == "coinbasespot" || options.spot_only,
                    testnet: options.testnet,
                })
                .map_err(|error| JsError::new(&error))?,
            )),
            #[cfg(feature = "polymarket")]
            "polymarket" => Box::new(Engine::new(Polymarket::new(PolymarketOptions {
                public_data_only: options.public_data_only,
                key: options.key,
                polymarket: options.polymarket,
            }))),
            #[cfg(feature = "paradex")]
            "paradex" => Box::new(Engine::new(Paradex::new(ParadexOptions {
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key,
                paradex_address: options.paradex_address,
                subkey_address: options.subkey_address,
                payload_signer: options.payload_signer,
            }))),
            #[cfg(feature = "apexomni")]
            "apexomni" => Box::new(Engine::new(ApexOmni::new(ApexOmniOptions {
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
                passphrase: options.passphrase,
                omni_key_seed: options.omni_key_seed,
                tick_interval_ms: None,
            }))),
            #[cfg(feature = "txflow")]
            "txflow" => {
                let options = safe_cex_venue_txflow::Options {
                    testnet: options.testnet,
                    public_data_only: options.public_data_only,
                    key: options.key,
                    secret: options.secret,
                    application_id: options.application_id,
                    vault: options.vault,
                };
                options.validate().map_err(|e| JsError::new(&e))?;
                Box::new(Engine::new(safe_cex_venue_txflow::TxFlow::new(options)))
            }
            #[cfg(feature = "kraken")]
            "kraken" => Box::new(Engine::new(Kraken::new(KrakenOptions {
                read_credentials: options.read_credentials.clone(),
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
            }))),
            #[cfg(feature = "kraken")]
            "krakenspot" => Box::new(Engine::new(KrakenSpot::new(KrakenOptions {
                // Spot private reads are POSTs and retain primary credentials.
                read_credentials: None,
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
            }))),
            #[cfg(feature = "asterdex")]
            "asterdex" => Box::new(Engine::new(AsterDex::new(AsterOptions {
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
                application_id: options.application_id,
                auth_user_address: options.asterdex.user_address,
                auth_signer_address: options.asterdex.signer_address,
                auth_private_key: options.asterdex.private_key,
                api_version: options.asterdex.api_version,
                builder_address: options.asterdex.builder_address,
                builder_fee_rate_maker: options.asterdex.builder_fee_rate_maker,
                builder_fee_rate_taker: options.asterdex.builder_fee_rate_taker,
                read_key: options.asterdex.read_key,
                read_secret: options.asterdex.read_secret,
            }))),
            #[cfg(feature = "orderly")]
            "orderly" | "tealstreet-dex" | "kodiakfi" | "mode" | "salsadex" | "what" | "woofi" => {
                Box::new(Engine::new(Orderly::new(OrderlyOptions {
                    broker: match venue {
                        "kodiakfi" => OrderlyBroker::KodiakFi,
                        "mode" => OrderlyBroker::Mode,
                        "salsadex" => OrderlyBroker::SalsaDex,
                        "what" => OrderlyBroker::What,
                        "woofi" => OrderlyBroker::WooFi,
                        _ => OrderlyBroker::Tealstreet,
                    },
                    public_data_only: options.public_data_only,
                    testnet: options.testnet,
                    key: options.key,
                    secret: options.secret,
                    application_id: options.application_id,
                    tick_interval: options.tick_interval,
                })))
            }
            #[cfg(feature = "lighter")]
            "lighter" | "lighter-robinhood" => {
                Box::new(Engine::new(Lighter::new(LighterOptions {
                    robinhood: venue == "lighter-robinhood",
                    remote_signer: options.lighter_remote_signer,
                    signer_provider: options.lighter_signer_provider,
                    testnet: options.testnet,
                    public_data_only: options.public_data_only,
                    key: options.key,
                    secret: options.secret,
                    application_id: options.application_id,
                    api_key_index: options.api_key_index,
                    integrator_account_index: options.lighter_integrator_account_index,
                    stop_slippage_pct: options.lighter_stop_slippage_pct.unwrap_or(1.0),
                })))
            }
            #[cfg(feature = "bitrue")]
            "bitrue" => Box::new(Engine::new(Bitrue::new(BitrueOptions {
                read_credentials: options.read_credentials.clone(),
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
                recv_window: options.recv_window.unwrap_or(20_000.0).clamp(1.0, 60_000.0) as u32,
            }))),
            #[cfg(feature = "backpack")]
            "backpack" => Box::new(Engine::new(Backpack::new(BackpackOptions {
                read_credentials: options.read_credentials.clone(),
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
                recv_window: options.recv_window,
            }))),
            #[cfg(feature = "blofin")]
            "blofin" => Box::new(Engine::new(Blofin::new(BlofinOptions {
                read_credentials: options.read_credentials.clone(),
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
                passphrase: options.passphrase,
            }))),
            #[cfg(feature = "mexc")]
            "mexc" => Box::new(Engine::new(MexcFutures::new(MexcOptions {
                read_credentials: options.read_credentials.clone(),
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
            }))),
            #[cfg(feature = "phemex")]
            "phemex" => Box::new(Engine::new(PhemexPerpetuals::new(PhemexOptions {
                read_credentials: options.read_credentials.clone(),
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
            }))),
            #[cfg(feature = "binance")]
            "binance" | "binancepm" | "binancespot" | "binanceinverse" => {
                Box::new(Engine::new(BinanceFutures::new(BinanceOptions {
                    read_credentials: options.read_credentials.clone(),
                    testnet: options.testnet,
                    public_data_only: options.public_data_only,
                    key: options.key,
                    secret: options.secret,
                    portfolio_margin: venue == "binancepm",
                    variant: match venue {
                        "binancepm" => BinanceVariant::PortfolioMargin,
                        "binancespot" => BinanceVariant::Spot,
                        "binanceinverse" => BinanceVariant::CoinM,
                        _ => BinanceVariant::UsdM,
                    },
                })))
            }
            #[cfg(feature = "kucoinspot")]
            "kucoinspot" => Box::new(Engine::new(KucoinSpot::new(KucoinOptions {
                read_credentials: options.read_credentials.clone(),
                broker: options.extra.and_then(|e| e.kucoin_broker),
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
                passphrase: options.passphrase,
            }))),
            #[cfg(feature = "kucoin")]
            "kucoin" => Box::new(Engine::new(Kucoin::new(KucoinOptions {
                read_credentials: options.read_credentials.clone(),
                broker: options.extra.and_then(|e| e.kucoin_broker),
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
                passphrase: options.passphrase,
            }))),
            #[cfg(feature = "bingx")]
            "bingx" => Box::new(Engine::new(Bingx::new(BingxOptions {
                read_credentials: options.read_credentials.clone(),
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
            }))),
            #[cfg(feature = "weex")]
            "weex" => Box::new(Engine::new(WeexFutures::new(WeexOptions {
                read_credentials: options.read_credentials.clone(),
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
                passphrase: options.passphrase,
                private_ws_headers: options.private_ws_headers,
            }))),
            #[cfg(feature = "woo")]
            "woo" => Box::new(Engine::new(Woo::new(WooOptions {
                read_credentials: options.read_credentials.clone(),
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
                application_id: options.application_id,
            }))),
            #[cfg(feature = "bybit")]
            "bybit" | "bybitinverse" | "bybitspot" | "bybitdemo" | "hyrotrader" => {
                let options = BybitOptions {
                    read_credentials: options.read_credentials.clone(),
                    enable_inverse_markets: options.enable_inverse_markets,
                    enable_option_markets: options.enable_option_markets,
                    variant: match venue {
                        "bybitinverse" => BybitVariant::Inverse,
                        "bybitspot" => BybitVariant::Spot,
                        "bybitdemo" => BybitVariant::Demo,
                        "hyrotrader" => BybitVariant::Hyrotrader,
                        _ => BybitVariant::Linear,
                    },
                    testnet: options.testnet,
                    public_data_only: options.public_data_only,
                    key: options.key,
                    secret: options.secret,
                    endpoints: options.endpoints.map(|e| BybitEndpoints {
                        rest: e.rest,
                        public_ws: e.public_ws,
                        private_ws: e.private_ws,
                    }),
                };
                options.validate().map_err(|e| JsError::new(&e))?;
                Box::new(Engine::new(BybitLinear::new(options)))
            }
            #[cfg(feature = "bitget")]
            "bitget" | "bitgetusdc" | "bitgetinverse" => {
                if !options.public_data_only
                    && options.key.as_ref().is_some_and(|s| !s.is_empty())
                    && options.secret.as_ref().is_some_and(|s| !s.is_empty())
                    && options.passphrase.as_ref().is_none_or(|s| s.is_empty())
                {
                    return Err(JsError::new("Bitget requires a passphrase"));
                }
                Box::new(Engine::new(BitgetFutures::new(BitgetOptions {
                    read_credentials: options.read_credentials.clone(),
                    testnet: options.testnet,
                    public_data_only: options.public_data_only,
                    key: options.key,
                    secret: options.secret,
                    passphrase: options.passphrase,
                    product: if venue == "bitgetinverse" {
                        Product::Inverse
                    } else if venue == "bitgetusdc" {
                        Product::Usdc
                    } else {
                        Product::Usdt
                    },
                })))
            }
            #[cfg(feature = "cryptocom")]
            "cryptocom" => Box::new(Engine::new(Cryptocom::new(CryptocomOptions {
                read_credentials: options.read_credentials.clone(),
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
            }))),
            #[cfg(feature = "hyperliquid")]
            "hyperliquid" | "hyperliquidspot" => {
                if venue == "hyperliquid" && options.enable_spot_markets {
                    return Err(JsError::new(
                        "Rust Hyperliquid currently supports perpetual markets only",
                    ));
                }
                let config = HyperliquidOptions {
                    testnet: options.testnet,
                    public_data_only: options.public_data_only,
                    key: options.key,
                    secret: options.secret,
                    application_id: options.application_id,
                    vault: options.vault,
                    builder: options.hyperliquid_builder.map(|b| Builder {
                        b: b.address,
                        f: b.fee_tenths_bps,
                    }),
                };
                Box::new(Engine::new(if venue == "hyperliquidspot" {
                    Hyperliquid::new_spot(config)
                } else {
                    Hyperliquid::new(config)
                }))
            }
            #[cfg(feature = "okx")]
            "okx" | "okxeu" | "okxdemo" | "okxeudemo" => {
                if !options.public_data_only
                    && options.key.as_ref().is_some_and(|s| !s.is_empty())
                    && options.secret.as_ref().is_some_and(|s| !s.is_empty())
                    && options.passphrase.as_ref().is_none_or(|s| s.is_empty())
                {
                    return Err(JsError::new("OKX requires a passphrase"));
                }
                Box::new(Engine::new(Okx::new(OkxOptions {
                    read_credentials: options.read_credentials.clone(),
                    testnet: options.testnet || venue.ends_with("demo"),
                    eea: venue.starts_with("okxeu"),
                    public_data_only: options.public_data_only,
                    key: options.key,
                    secret: options.secret,
                    passphrase: options.passphrase,
                })))
            }
            other => return Err(JsError::new(&format!("unsupported venue: {other}"))),
        };
        inner.set_public_feed_policy(policy).map_err(JsError::new)?;
        Ok(Self { inner })
    }

    fn flush(&mut self) -> String {
        encode(&self.inner.drain())
    }

    pub fn start(&mut self, now: f64) -> String {
        self.inner.start(now);
        self.flush()
    }

    #[wasm_bindgen(js_name = wsOpen)]
    pub fn ws_open(&mut self, socket: u32, now: f64) -> String {
        let socket = SocketId(socket);
        self.inner.handle(now, Event::WsOpened { socket });
        self.flush()
    }

    #[wasm_bindgen(js_name = wsMessage)]
    pub fn ws_message(&mut self, socket: u32, text: &str, now: f64) -> String {
        let socket = SocketId(socket);
        self.inner.handle(now, Event::WsMessage { socket, text });
        self.flush()
    }

    /// Like `wsMessage`, returning `undefined` when there are no host actions.
    /// Hosts can avoid decoding an empty UTF-8 string on market-data messages.
    #[cfg(feature = "text-ingress")]
    #[wasm_bindgen(js_name = wsMessageActions)]
    pub fn ws_message_actions(&mut self, socket: u32, text: &str, now: f64) -> Option<String> {
        let actions = self.ws_message(socket, text, now);
        (!actions.is_empty()).then_some(actions)
    }

    /// A byte-input alternative that rejects invalid UTF-8 before handling any event.
    #[cfg(feature = "text-ingress")]
    #[wasm_bindgen(js_name = wsMessageBytes)]
    pub fn ws_message_bytes(&mut self, socket: u32, bytes: &[u8], now: f64) -> Option<String> {
        let text = std::str::from_utf8(bytes).ok()?;
        self.ws_message_actions(socket, text, now)
    }

    #[wasm_bindgen(js_name = wsBinary)]
    pub fn ws_binary(&mut self, socket: u32, data: &[u8], now: f64) -> String {
        self.inner.handle(
            now,
            Event::WsBinary {
                socket: SocketId(socket),
                data,
            },
        );
        self.flush()
    }

    #[wasm_bindgen(js_name = wsClose)]
    pub fn ws_close(&mut self, socket: u32, code: u16, now: f64) -> String {
        let socket = SocketId(socket);
        self.inner.handle(now, Event::WsClosed { socket, code });
        self.flush()
    }

    /// A response begins; `headers` is `name: value` lines. The body follows
    /// as chunks so a large list never has to fit in memory at once.
    #[wasm_bindgen(js_name = httpStart)]
    pub fn http_start(&mut self, request: u32, status: u16, headers: &str, now: f64) -> String {
        let request = RequestId(request);
        self.inner.handle(
            now,
            Event::HttpStart {
                request,
                status,
                headers,
            },
        );
        self.flush()
    }

    #[wasm_bindgen(js_name = httpChunk)]
    pub fn http_chunk(&mut self, request: u32, data: &[u8], now: f64) -> String {
        let request = RequestId(request);
        self.inner.handle(now, Event::HttpChunk { request, data });
        self.flush()
    }

    #[wasm_bindgen(js_name = httpEnd)]
    pub fn http_end(&mut self, request: u32, now: f64) -> String {
        let request = RequestId(request);
        self.inner.handle(now, Event::HttpEnd { request });
        self.flush()
    }

    #[wasm_bindgen(js_name = httpFailed)]
    pub fn http_failed(&mut self, request: u32, error: &str, now: f64) -> String {
        let request = RequestId(request);
        self.inner.handle(now, Event::HttpFailed { request, error });
        self.flush()
    }

    #[wasm_bindgen(js_name = payloadSigned)]
    pub fn payload_signed(
        &mut self,
        request: u32,
        signature: &str,
        headers: &str,
        now: f64,
    ) -> String {
        let request = RequestId(request);
        match payload_headers(headers) {
            Some(headers) => self.inner.handle(
                now,
                Event::PayloadSigned {
                    request,
                    signature,
                    headers,
                },
            ),
            None => self.inner.handle(
                now,
                Event::PayloadSignFailed {
                    request,
                    error: "Invalid payload signer headers",
                },
            ),
        }
        self.flush()
    }

    #[wasm_bindgen(js_name = payloadSignFailed)]
    pub fn payload_sign_failed(&mut self, request: u32, error: &str, now: f64) -> String {
        self.inner.handle(
            now,
            Event::PayloadSignFailed {
                request: RequestId(request),
                error,
            },
        );
        self.flush()
    }

    pub fn timer(&mut self, timer: u32, now: f64) -> String {
        let timer = TimerId(timer);
        self.inner.handle(now, Event::TimerFired { timer });
        self.flush()
    }

    pub fn command(&mut self, command_json: &str, now: f64) -> Result<String, JsError> {
        #[cfg(feature = "cold-subscriptions")]
        {
            if let Some(command) = safe_cex_engine::command::decode_subscription(command_json)? {
                self.inner.handle(now, Event::Command(command));
                return Ok(self.flush());
            }
            self.other_command(command_json, now)
        }
        #[cfg(not(feature = "cold-subscriptions"))]
        {
            let event = match safe_cex_engine::command::decode_host_input(command_json)? {
                safe_cex_engine::command::HostInput::PublicFeedUpdate(update) => {
                    self.inner
                        .apply_public_feed(now, update)
                        .map_err(JsError::new)?;
                    return Ok(self.flush());
                }
                safe_cex_engine::command::HostInput::Command(command) => Event::Command(command),
                safe_cex_engine::command::HostInput::TickIntervalChanged { interval_ms } => {
                    Event::TickIntervalChanged { interval_ms }
                }
            };
            self.inner.handle(now, event);
            Ok(self.flush())
        }
    }

    // Keep cold decoding and dispatch together: returning the large command enum
    // across this boundary would copy every variant into the startup path.
    #[cfg(feature = "cold-subscriptions")]
    #[inline(never)]
    fn other_command(&mut self, command_json: &str, now: f64) -> Result<String, JsError> {
        let event = match safe_cex_engine::command::decode_host_input(command_json)? {
            safe_cex_engine::command::HostInput::PublicFeedUpdate(update) => {
                self.inner
                    .apply_public_feed(now, update)
                    .map_err(JsError::new)?;
                return Ok(self.flush());
            }
            safe_cex_engine::command::HostInput::Command(command) => Event::Command(command),
            safe_cex_engine::command::HostInput::TickIntervalChanged { interval_ms } => {
                Event::TickIntervalChanged { interval_ms }
            }
        };
        self.inner.handle(now, event);
        Ok(self.flush())
    }

    // Only Coinbase exposes its selected portfolio through the SDK account API.
    // Keep the optional ABI out of single-venue artifacts that cannot use it.
    #[cfg(feature = "coinbase")]
    #[wasm_bindgen(js_name = accountId)]
    pub fn account_id(&self) -> Option<String> {
        self.inner.account_id()
    }

    /// Everything that changed since the previous call as one flush frame
    /// (layout: docs/plans/RUST_REWRITE.md), or `undefined` when nothing did.
    /// The whole current state as one frame, for a consumer that starts late
    /// or skipped frames. Frames taken afterwards apply on top of it.
    #[wasm_bindgen(js_name = snapshotFrame)]
    pub fn snapshot_frame(&mut self, now: f64) -> js_sys::Uint8Array {
        js_sys::Uint8Array::from(self.inner.snapshot(now))
    }

    /// Transfer `frame.buffer` to the consumer: it is copied once, straight
    /// out of the engine's reused frame buffer.
    /// With `market_only`, just books, candles and fills (for the venue's
    /// book cadence); store sections wait for the next full frame.
    #[wasm_bindgen(js_name = takeFrame)]
    pub fn take_frame(&mut self, now: f64, market_only: bool) -> Option<js_sys::Uint8Array> {
        self.inner
            .frame(now, market_only)
            .map(js_sys::Uint8Array::from)
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn payload_headers(text: &str) -> Option<Vec<(String, String)>> {
    let mut headers = Vec::new();
    if !text.is_empty() {
        for line in text.split('\n') {
            let (name, value) = line.split_once(':')?;
            headers.push((name.into(), value.into()));
        }
    }
    Some(headers)
}

#[cfg(test)]
mod tests {
    #[test]
    fn host_commands_dispatch_once_and_flush_once_with_original_clock() {
        use safe_cex_engine::command::HostInput;
        use std::cell::{Cell, RefCell};
        use std::rc::Rc;

        struct Commands {
            events: Rc<RefCell<Vec<(f64, HostInput)>>>,
            drains: Rc<Cell<usize>>,
        }
        impl AnyEngine for Commands {
            #[cfg(feature = "coinbase")]
            fn account_id(&self) -> Option<String> {
                None
            }
            fn set_public_feed_policy(
                &mut self,
                _: safe_cex_engine::PublicFeedPolicy,
            ) -> Result<(), &'static str> {
                Err("command-only test engine")
            }
            fn apply_public_feed(
                &mut self,
                now: f64,
                update: safe_cex_engine::PublicFeedUpdate,
            ) -> Result<(), &'static str> {
                self.events
                    .borrow_mut()
                    .push((now, HostInput::PublicFeedUpdate(update)));
                Ok(())
            }
            fn start(&mut self, _: f64) {}
            fn handle(&mut self, now: f64, event: Event<'_>) {
                let input = match event {
                    Event::Command(command) => HostInput::Command(command),
                    Event::TickIntervalChanged { interval_ms } => {
                        HostInput::TickIntervalChanged { interval_ms }
                    }
                    _ => panic!("command boundary"),
                };
                self.events.borrow_mut().push((now, input));
            }
            fn drain(&mut self) -> Vec<Action> {
                self.drains.set(self.drains.get() + 1);
                Vec::new()
            }
            fn frame(&mut self, _: f64, _: bool) -> Option<&[u8]> {
                None
            }
            fn snapshot(&mut self, _: f64) -> &[u8] {
                &[]
            }
        }
        let events = Rc::new(RefCell::new(Vec::new()));
        let drains = Rc::new(Cell::new(0));
        let mut engine = WasmEngine {
            inner: Box::new(Commands {
                events: Rc::clone(&events),
                drains: Rc::clone(&drains),
            }),
        };
        let commands = [
            r#"{"op":"subscribeOrderBook","symbol":"BTCUSDT"}"#,
            r#"{"op":"unsubscribeOrderBook","symbol":"BTCUSDT"}"#,
            r#"{"op":"subscribeCandles","symbol":"BTCUSDT","interval":"1m"}"#,
            r#"{"op":"unsubscribeCandles","symbol":"BTCUSDT","interval":"1m"}"#,
            r#"{"op":"setTickerSymbols","symbols":["BTCUSDT"]}"#,
            r#"{"op":"setTickInterval","interval":2000}"#,
            r#"{"op":"setTickInterval","interval":null}"#,
            r#"{"op":"publicFeedUpdate","tickers":[]}"#,
        ];
        let mut dispatched = 0;
        for (index, json) in commands.iter().enumerate() {
            let now = index as f64 + 0.5;
            match safe_cex_engine::command::decode_host_input(json) {
                Ok(expected) => {
                    assert_eq!(engine.command(json, now).unwrap(), "");
                    assert_eq!(events.borrow()[dispatched], (now, expected));
                    dispatched += 1;
                }
                Err(_) => {
                    // JsError construction invokes JS and cannot run on a native target.
                    // Native tests cover decoder rejection; WASM also checks the host ABI.
                    #[cfg(target_arch = "wasm32")]
                    assert!(engine.command(json, now).is_err());
                }
            }
            assert_eq!(drains.get(), dispatched);
            assert_eq!(events.borrow().len(), dispatched);
        }
    }

    #[test]
    fn account_command_aliases_share_the_engine_read() {
        for (op, enabled) in [
            (
                "getAccount",
                cfg!(any(feature = "apexomni", feature = "kucoinspot")),
            ),
            (
                "fetchAccount",
                cfg!(any(feature = "asterdex", feature = "polymarket")),
            ),
        ] {
            let parsed = safe_cex_engine::command::decode_command(&format!(
                r#"{{"op":"{op}","command":77}}"#
            ));
            if enabled {
                assert!(matches!(
                    parsed.unwrap(),
                    Command::FetchAccount { command: 77 }
                ));
            } else {
                assert!(
                    parsed.is_err(),
                    "{op} must reject without its venue feature"
                );
            }
        }
    }

    #[test]
    fn secondary_read_options_decode_without_changing_primary_identity() {
        let options: EngineOptions = serde_json::from_str(r#"{"key":"primary","secret":"primary-secret","readCredentials":{"key":"read","secret":"read-secret","passphrase":"read-pass"}}"#).unwrap();
        assert_eq!(options.key.as_deref(), Some("primary"));
        let read = options.read_credentials.unwrap();
        assert_eq!(read.key, "read");
        assert_eq!(read.secret, "read-secret");
        assert_eq!(read.passphrase.as_deref(), Some("read-pass"));
        assert!(
            serde_json::from_str::<EngineOptions>(r#"{"readCredentials":{"key":"read"}}"#).is_err()
        );
    }

    #[test]
    fn http_route_metadata_is_optional_and_has_stable_host_names() {
        let actions = [
            None,
            Some(safe_cex_engine::RequestRoute::Direct),
            Some(safe_cex_engine::RequestRoute::PublicCors),
            Some(safe_cex_engine::RequestRoute::Primary),
        ]
        .into_iter()
        .map(|route| {
            Action::Http(safe_cex_engine::HttpRequest {
                id: safe_cex_engine::RequestId(1),
                method: safe_cex_engine::Method::Get,
                route,
                timeout_ms: route.map(|_| 5_000),
                url: "https://venue.test/time".into(),
                headers: vec![],
                signer_headers: vec![],
                body: None,
            })
        })
        .collect::<Vec<_>>();
        let encoded = encode(&actions);
        let rows: Vec<serde_json::Value> = serde_json::from_str(&encoded).unwrap();
        assert!(rows[0].get("route").is_none());
        assert_eq!(rows[1]["route"], "direct");
        assert_eq!(rows[2]["route"], "publicCors");
        assert_eq!(rows[3]["route"], "primary");
        assert!(rows[0].get("timeoutMs").is_none());
        assert_eq!(rows[1]["timeoutMs"], 5_000);
        assert_eq!(rows[2]["timeoutMs"], 5_000);
        assert!(
            rows.iter()
                .all(|row| row["headers"] == serde_json::json!([]))
        );
    }

    use super::*;
    use safe_cex_engine::Command;
    #[cfg(any(feature = "asterdex", feature = "kucoinspot"))]
    use safe_cex_engine::model::CancelOrderRef;
    #[cfg(feature = "asterdex")]
    use safe_cex_engine::model::StpMode;

    #[test]
    fn delegated_header_lines_preserve_value_bytes_and_reject_malformed_lines() {
        assert_eq!(payload_headers(""), Some(vec![]));
        assert_eq!(
            payload_headers("X-Empty:\nX-Exact:  a:b:c \t"),
            Some(vec![
                ("X-Empty".into(), String::new()),
                ("X-Exact".into(), "  a:b:c \t".into()),
            ])
        );
        for invalid in ["missing-colon", "\n", "X-Proof:a\n", "X-Proof:a\ninvalid"] {
            assert_eq!(payload_headers(invalid), None);
        }
    }

    #[test]
    fn amendment_symbol_context_is_preserved_for_spot_builds() {
        let wire = safe_cex_engine::command::decode_command(
            r#"{"op":"amendOrderWithSymbol","command":19,"id":"unobserved","symbol":"BTC-USDT","price":100}"#,
        );
        #[cfg(feature = "kucoinspot")]
        assert!(
            matches!(wire.unwrap(), Command::AmendOrderWithSymbol { command: 19, id, symbol, price: Some(100.0), amount: None }
            if id == "unobserved" && symbol == "BTC-USDT")
        );
        #[cfg(not(feature = "kucoinspot"))]
        assert!(wire.is_err());
        let legacy = safe_cex_engine::command::decode_command(
            r#"{"op":"amendOrder","command":19,"id":"legacy","symbol":"ignored","price":100}"#,
        )
        .unwrap();
        assert!(
            matches!(legacy, Command::AmendOrder { command: 19, id, price: Some(100.0), amount: None } if id == "legacy")
        );
    }

    #[cfg(feature = "text-ingress")]
    #[derive(Default)]
    struct Messages {
        actions: Vec<Action>,
    }

    #[cfg(feature = "text-ingress")]
    impl AnyEngine for Messages {
        #[cfg(feature = "coinbase")]
        fn account_id(&self) -> Option<String> {
            None
        }

        fn set_public_feed_policy(
            &mut self,
            _policy: safe_cex_engine::PublicFeedPolicy,
        ) -> Result<(), &'static str> {
            Err("message-only test engine")
        }
        fn apply_public_feed(
            &mut self,
            _now: f64,
            _update: safe_cex_engine::PublicFeedUpdate,
        ) -> Result<(), &'static str> {
            Err("message-only test engine")
        }
        fn start(&mut self, _now: f64) {}
        fn handle(&mut self, _now: f64, event: Event<'_>) {
            if let Event::WsMessage { socket, text } = event
                && text != "quiet"
            {
                for prefix in ["first", "second"] {
                    self.actions.push(Action::WsSend {
                        socket,
                        text: format!("{prefix}:{text}"),
                    });
                }
            }
        }
        fn drain(&mut self) -> Vec<Action> {
            std::mem::take(&mut self.actions)
        }
        fn frame(&mut self, _now: f64, _market_only: bool) -> Option<&[u8]> {
            None
        }
        fn snapshot(&mut self, _now: f64) -> &[u8] {
            &[]
        }
    }

    #[cfg(feature = "text-ingress")]
    #[test]
    fn optional_websocket_actions_preserve_legacy_output_order_and_drain_once() {
        let engine = || WasmEngine {
            inner: Box::<Messages>::default(),
        };
        let mut legacy = engine();
        let mut optional = engine();
        assert_eq!(legacy.ws_message(7, "quiet", 1.), "");
        assert_eq!(optional.ws_message_actions(7, "quiet", 1.), None);
        let expected = legacy.ws_message(7, "payload", 2.);
        assert_eq!(
            expected,
            r#"[{"op":"wsSend","socket":7,"text":"first:payload"},{"op":"wsSend","socket":7,"text":"second:payload"}]"#
        );
        assert_eq!(
            optional.ws_message_actions(7, "payload", 2.),
            Some(expected)
        );
        assert_eq!(legacy.ws_message(7, "quiet", 3.), "");
        assert_eq!(optional.ws_message_actions(7, "quiet", 3.), None);
    }

    #[cfg(feature = "text-ingress")]
    #[test]
    fn byte_websocket_actions_validate_utf8_before_handling_or_draining() {
        let mut legacy = WasmEngine {
            inner: Box::<Messages>::default(),
        };
        let mut bytes = WasmEngine {
            inner: Box::<Messages>::default(),
        };
        // A rejected byte message must neither handle an event nor drain existing actions.
        bytes.inner.handle(
            1.,
            Event::WsMessage {
                socket: SocketId(7),
                text: "pending",
            },
        );
        for invalid in [
            &[0xff][..],
            &[0xc0, 0xaf],
            &[0xed, 0xa0, 0x80],
            &[0xf0, 0x9f],
        ] {
            assert_eq!(bytes.ws_message_bytes(7, invalid, 2.), None);
        }
        assert_eq!(
            bytes.ws_message_bytes(7, b"quiet", 3.),
            Some(legacy.ws_message(7, "pending", 1.))
        );
        for text in ["", "ASCII", "é漢🦀\u{fffd}", "a\0b"] {
            assert_eq!(
                bytes.ws_message_bytes(7, text.as_bytes(), 4.),
                Some(legacy.ws_message(7, text, 4.))
            );
            assert_eq!(bytes.ws_message_bytes(7, b"quiet", 5.), None);
        }
    }

    #[cfg(any(feature = "asterdex", feature = "kucoinspot"))]
    #[test]
    fn cancellation_with_symbols_preserves_caller_identity() {
        let wire = safe_cex_engine::command::decode_command(
            r#"{"op":"cancelOrdersWithSymbols","command":19,"orders":[{"id":"22542179","symbol":"BTCUSDT"}]}"#,
        ).unwrap();
        assert!(
            matches!(wire, Command::CancelOrdersWithSymbols { command: 19, orders }
            if orders == vec![CancelOrderRef { id: "22542179".into(), symbol: "BTCUSDT".into() }])
        );
    }

    // Compile command parsers only for venues that expose each SDK method.
    #[test]
    fn aster_commands_follow_the_compiled_venue_feature() {
        let commands = [
            r#"{"op":"cancelOrdersWithSymbols","command":19,"orders":[{"id":"7","symbol":"BTCUSDT"}]}"#,
            r#"{"op":"fetchStpMode","command":19}"#,
            r#"{"op":"fetchAccount","command":19}"#,
            r#"{"op":"fetchOrderBook","command":19,"symbol":"BTCUSDT","limit":50}"#,
            r#"{"op":"setStpMode","command":19,"mode":"EXPIRE_MAKER"}"#,
        ];
        for (index, json) in commands.into_iter().enumerate() {
            if index == 0 {
                assert_eq!(
                    safe_cex_engine::command::decode_command(json).is_ok(),
                    cfg!(any(feature = "asterdex", feature = "kucoinspot"))
                );
                continue;
            }
            let parsed = safe_cex_engine::command::decode_command(json);
            #[cfg(feature = "asterdex")]
            match (index, parsed.unwrap()) {
                (
                    0,
                    Command::CancelOrdersWithSymbols {
                        command: 19,
                        orders,
                    },
                ) => {
                    assert_eq!(
                        orders,
                        vec![CancelOrderRef {
                            id: "7".into(),
                            symbol: "BTCUSDT".into()
                        }]
                    );
                }
                (1, Command::FetchStpMode { command: 19 })
                | (2, Command::FetchAccount { command: 19 })
                | (
                    4,
                    Command::SetStpMode {
                        command: 19,
                        mode: StpMode::ExpireMaker,
                    },
                ) => {}
                (
                    3,
                    Command::FetchOrderBook {
                        command: 19,
                        symbol,
                        limit: Some(50),
                    },
                ) => {
                    assert_eq!(symbol, "BTCUSDT");
                }
                _ => panic!("Aster command {index} changed its wire mapping"),
            }
            #[cfg(not(feature = "asterdex"))]
            if cfg!(feature = "polymarket") && matches!(index, 2 | 3) {
                let command = parsed.unwrap();
                assert!(matches!(
                    (index, command),
                    (2, Command::FetchAccount { command: 19 })
                        | (
                            3,
                            Command::FetchOrderBook {
                                command: 19,
                                symbol: _,
                                limit: Some(50)
                            }
                        )
                ));
            } else {
                assert!(
                    parsed.is_err(),
                    "unsupported venue command {index} must reject"
                );
            }
        }
    }

    #[test]
    fn forwards_leverage_metadata_command() {
        let wire = safe_cex_engine::command::decode_command(
            r#"{"op":"fetchLeverage","command":17,"symbol":"BTCUSDT"}"#,
        )
        .unwrap();
        assert!(matches!(
            wire,
            Command::FetchLeverage { command: 17, symbol } if symbol == "BTCUSDT"
        ));
    }
}
