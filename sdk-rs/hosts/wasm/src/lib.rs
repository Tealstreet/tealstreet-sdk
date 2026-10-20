//! WebAssembly host surface for browser workers and Cloudflare Workers.
//!
//! JavaScript owns sockets, fetch and timers; it forwards each event here and
//! executes the returned actions. Every call returns the pending actions as a
//! JSON array string, or an empty string when there are none. `wsMessageActions`
//! and `wsMessageBytes` return `undefined` instead, avoiding empty-string decoding.
#[cfg(not(any(
    feature = "venue-grvt",
    feature = "venue-paradex",
    feature = "venue-bitrue",
    feature = "venue-coinbase",
    feature = "venue-binance",
    feature = "venue-apexomni",
    feature = "venue-asterdex",
    feature = "venue-breakoutprop",
    feature = "venue-kucoin",
    feature = "venue-polymarket",
    feature = "venue-blofin",
    feature = "venue-bybit",
    feature = "venue-woo",
    feature = "venue-cryptocom",
    feature = "venue-bingx",
    feature = "venue-mexc",
    feature = "venue-bitget",
    feature = "venue-backpack",
    feature = "venue-phemex",
    feature = "venue-weex",
    feature = "venue-hyperliquid",
    feature = "venue-kraken",
    feature = "venue-orderly",
    feature = "venue-lighter",
    feature = "venue-okx"
)))]
compile_error!(
    "safe-cex-wasm requires at least one venue feature: breakoutprop, paradex, apexomni, asterdex, bitrue, binance, kucoin, bybit, cryptocom, bingx, blofin, mexc, backpack, bitget, grvt, hyperliquid, kraken, coinbase, lighter, orderly, polymarket, okx, phemex, woo or weex"
);

use safe_cex_engine::{
    Action, Engine, Event, LogLevel, Notice, RequestId, SocketId, TimerId, Venue,
};
#[cfg(feature = "venue-apexomni")]
use safe_cex_venue_apexomni::{ApexOmni, Options as ApexOmniOptions};
#[cfg(feature = "venue-asterdex")]
use safe_cex_venue_asterdex::{AsterDex, Options as AsterOptions};
#[cfg(feature = "venue-backpack")]
use safe_cex_venue_backpack::{Backpack, Options as BackpackOptions};
#[cfg(feature = "venue-binance")]
use safe_cex_venue_binance::{
    BinanceFutures, Options as BinanceOptions, Variant as BinanceVariant,
};
#[cfg(feature = "venue-bingx")]
use safe_cex_venue_bingx::{Bingx, Options as BingxOptions};
#[cfg(feature = "venue-bitget")]
use safe_cex_venue_bitget::{BitgetFutures, Options as BitgetOptions, Product};
#[cfg(feature = "venue-bitrue")]
use safe_cex_venue_bitrue::{Bitrue, Options as BitrueOptions};
#[cfg(feature = "venue-blofin")]
use safe_cex_venue_blofin::{Blofin, Options as BlofinOptions};
#[cfg(feature = "venue-breakoutprop")]
use safe_cex_venue_breakoutprop::{BreakoutProp, Options as BreakoutOptions};
#[cfg(feature = "venue-bybit")]
use safe_cex_venue_bybit::{
    BybitLinear, Endpoints as BybitEndpoints, Options as BybitOptions, Variant as BybitVariant,
};
#[cfg(feature = "venue-coinbase")]
use safe_cex_venue_coinbase::{Coinbase, Options as CoinbaseOptions};
#[cfg(feature = "venue-cryptocom")]
use safe_cex_venue_cryptocom::{Cryptocom, Options as CryptocomOptions};
#[cfg(feature = "venue-grvt")]
use safe_cex_venue_grvt::{Grvt, Options as GrvtOptions};
#[cfg(feature = "venue-hyperliquid")]
use safe_cex_venue_hyperliquid::{Builder, Hyperliquid, Options as HyperliquidOptions};
#[cfg(feature = "venue-kraken")]
use safe_cex_venue_kraken::{Kraken, Options as KrakenOptions};
#[cfg(feature = "venue-kucoin")]
use safe_cex_venue_kucoin::{Kucoin, Options as KucoinOptions};
#[cfg(feature = "venue-lighter")]
use safe_cex_venue_lighter::{Lighter, Options as LighterOptions};
#[cfg(feature = "venue-mexc")]
use safe_cex_venue_mexc::{MexcFutures, Options as MexcOptions};
#[cfg(feature = "venue-okx")]
use safe_cex_venue_okx::{Okx, Options as OkxOptions};
#[cfg(feature = "venue-orderly")]
use safe_cex_venue_orderly::{Broker as OrderlyBroker, Options as OrderlyOptions, Orderly};
#[cfg(feature = "venue-paradex")]
use safe_cex_venue_paradex::{Options as ParadexOptions, Paradex};
#[cfg(feature = "venue-phemex")]
use safe_cex_venue_phemex::{Options as PhemexOptions, PhemexPerpetuals};
#[cfg(feature = "venue-polymarket")]
use safe_cex_venue_polymarket::{
    Options as PolymarketOptions, Polymarket, PolymarketOptions as PolymarketAccountOptions,
};
#[cfg(feature = "venue-weex")]
use safe_cex_venue_weex::{Options as WeexOptions, WeexFutures};
#[cfg(feature = "venue-woo")]
use safe_cex_venue_woo::{Options as WooOptions, Woo};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

mod batch;
use batch::{MessageBatch, Payload, dispatch_events};

#[cfg(feature = "profiling")]
mod profile;

trait AnyEngine {
    fn account_id(&self) -> Option<String>;
    fn start(&mut self, now: f64);
    fn handle(&mut self, now: f64, event: Event<'_>);
    fn drain(&mut self) -> Vec<Action>;
    fn frame(&mut self, now: f64, market_only: bool) -> Option<&[u8]>;
    fn snapshot(&mut self, now: f64) -> &[u8];
}

impl<V: Venue> AnyEngine for Engine<V> {
    fn account_id(&self) -> Option<String> {
        Engine::account_id(self).map(str::to_owned)
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

#[cfg(feature = "venue-kucoin")]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct KucoinExtra {
    #[serde(default)]
    kucoin_broker: Option<safe_cex_venue_kucoin::sign::BrokerOptions>,
}

#[cfg(any(
    feature = "venue-coinbase",
    feature = "venue-paradex",
    feature = "venue-binance",
    feature = "venue-apexomni",
    feature = "venue-asterdex",
    feature = "venue-breakoutprop",
    feature = "venue-kucoin",
    feature = "venue-polymarket",
    feature = "venue-blofin",
    feature = "venue-bybit",
    feature = "venue-woo",
    feature = "venue-cryptocom",
    feature = "venue-bingx",
    feature = "venue-mexc",
    feature = "venue-bitget",
    feature = "venue-grvt",
    feature = "venue-bitrue",
    feature = "venue-backpack",
    feature = "venue-phemex",
    feature = "venue-weex",
    feature = "venue-hyperliquid",
    feature = "venue-kraken",
    feature = "venue-orderly",
    feature = "venue-lighter",
    feature = "venue-okx"
))]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EngineOptions {
    #[cfg(feature = "venue-coinbase")]
    #[serde(default)]
    spot_only: bool,
    #[cfg(feature = "venue-polymarket")]
    #[serde(default)]
    polymarket: Option<PolymarketAccountOptions>,
    #[cfg(feature = "venue-lighter")]
    #[serde(default)]
    api_key_index: Option<String>,
    #[cfg(feature = "venue-lighter")]
    #[serde(default)]
    #[serde(deserialize_with = "optional_lighter_index")]
    lighter_integrator_account_index: Option<u64>,
    #[cfg(feature = "venue-lighter")]
    #[serde(default)]
    lighter_stop_slippage_pct: Option<f64>,
    #[cfg(any(
        feature = "venue-grvt",
        feature = "venue-coinbase",
        feature = "venue-paradex",
        feature = "venue-binance",
        feature = "venue-apexomni",
        feature = "venue-asterdex",
        feature = "venue-kucoin",
        feature = "venue-blofin",
        feature = "venue-bybit",
        feature = "venue-woo",
        feature = "venue-cryptocom",
        feature = "venue-hyperliquid",
        feature = "venue-kraken",
        feature = "venue-phemex",
        feature = "venue-orderly",
        feature = "venue-lighter",
        feature = "venue-bitget",
        feature = "venue-okx"
    ))]
    #[serde(default)]
    testnet: bool,
    #[serde(default)]
    public_data_only: bool,
    #[serde(default)]
    key: Option<String>,
    #[cfg(any(
        feature = "venue-grvt",
        feature = "venue-coinbase",
        feature = "venue-binance",
        feature = "venue-apexomni",
        feature = "venue-asterdex",
        feature = "venue-breakoutprop",
        feature = "venue-kucoin",
        feature = "venue-blofin",
        feature = "venue-bybit",
        feature = "venue-cryptocom",
        feature = "venue-bingx",
        feature = "venue-mexc",
        feature = "venue-bitget",
        feature = "venue-bitrue",
        feature = "venue-backpack",
        feature = "venue-phemex",
        feature = "venue-weex",
        feature = "venue-hyperliquid",
        feature = "venue-kraken",
        feature = "venue-lighter",
        feature = "venue-woo",
        feature = "venue-orderly",
        feature = "venue-okx"
    ))]
    #[serde(default)]
    secret: Option<String>,
    #[cfg(feature = "venue-paradex")]
    #[serde(default)]
    paradex_address: Option<String>,
    #[cfg(feature = "venue-paradex")]
    #[serde(default)]
    subkey_address: Option<String>,
    #[cfg(any(
        feature = "venue-backpack",
        feature = "venue-bitrue",
        feature = "venue-breakoutprop"
    ))]
    #[serde(default)]
    recv_window: Option<f64>,
    #[cfg(any(
        feature = "venue-kucoin",
        feature = "venue-okx",
        feature = "venue-weex",
        feature = "venue-blofin",
        feature = "venue-bitget",
        feature = "venue-apexomni"
    ))]
    #[serde(default)]
    passphrase: Option<String>,
    #[cfg(feature = "venue-apexomni")]
    #[serde(default)]
    omni_key_seed: Option<String>,
    #[cfg(any(
        feature = "venue-hyperliquid",
        feature = "venue-asterdex",
        feature = "venue-orderly",
        feature = "venue-lighter",
        feature = "venue-woo"
    ))]
    #[serde(default)]
    application_id: Option<String>,
    #[cfg(feature = "venue-hyperliquid")]
    #[serde(default)]
    vault: Option<String>,
    #[cfg(feature = "venue-hyperliquid")]
    #[serde(default)]
    hyperliquid_builder: Option<HyperliquidBuilder>,
    #[cfg(feature = "venue-hyperliquid")]
    #[serde(default)]
    enable_spot_markets: bool,
    #[cfg(feature = "venue-asterdex")]
    #[serde(default)]
    asterdex: AsterConfig,

    #[cfg(feature = "venue-kucoin")]
    #[serde(default)]
    extra: Option<KucoinExtra>,
    #[cfg(feature = "venue-weex")]
    #[serde(default)]
    private_ws_headers: bool,
    #[cfg(feature = "venue-grvt")]
    #[serde(default)]
    grvt_trading_account_id: Option<String>,
    #[cfg(feature = "venue-grvt")]
    #[serde(default)]
    grvt_signer_address: Option<String>,
    #[cfg(feature = "venue-grvt")]
    #[serde(default)]
    grvt_session_proxy: bool,
    #[cfg(feature = "venue-bybit")]
    #[serde(default)]
    endpoints: Option<EndpointOptions>,
    #[cfg(any(feature = "venue-orderly", feature = "venue-breakoutprop"))]
    #[serde(default)]
    tick_interval: Option<f64>,
    #[cfg(feature = "venue-breakoutprop")]
    #[serde(default)]
    subaccount: Option<String>,
    #[cfg(feature = "venue-breakoutprop")]
    #[serde(default)]
    polling_enabled: Option<bool>,
}
#[cfg(feature = "venue-asterdex")]
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

#[cfg(feature = "venue-bybit")]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EndpointOptions {
    rest: String,
    public_ws: String,
    private_ws: String,
}
#[cfg(feature = "venue-hyperliquid")]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HyperliquidBuilder {
    address: String,
    fee_tenths_bps: u32,
}

#[cfg(feature = "venue-lighter")]
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
        headers: Vec<(&'static str, &'a str)>,
        #[serde(skip_serializing_if = "Option::is_none")]
        body: Option<&'a str>,
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

// Large subscription bursts must not grow a JSON buffer through several
// generations: wasm keeps every page added during those transient reallocations.
fn json_string_length(text: &str) -> usize {
    2 + text
        .bytes()
        .map(|byte| match byte {
            b'"' | b'\\' | b'\n' | b'\r' | b'\t' | 8 | 12 => 2,
            0..=31 => 6,
            _ => 1,
        })
        .sum::<usize>()
}

impl ActionOut<'_> {
    fn capacity_hint(&self) -> usize {
        // Fixed field names, punctuation and numeric identifiers fit in this
        // allowance. Variable strings include JSON escaping and their quotes.
        128 + match self {
            Self::WsConnect {
                url,
                headers,
                channel,
                context,
                ..
            } => {
                json_string_length(url)
                    + channel.map_or(0, json_string_length)
                    + context.map_or(0, |context| json_string_length(&context.feed))
                    + headers.as_ref().map_or(0, |headers| {
                        headers
                            .iter()
                            .map(|(name, value)| {
                                json_string_length(name) + json_string_length(value) + 4
                            })
                            .sum::<usize>()
                    })
            }
            Self::WsSend { text, .. } => json_string_length(text),
            Self::Http {
                url, headers, body, ..
            } => {
                json_string_length(url)
                    + headers
                        .iter()
                        .map(|(name, value)| {
                            json_string_length(name) + json_string_length(value) + 4
                        })
                        .sum::<usize>()
                    + body.map_or(0, json_string_length)
            }
            Self::Log { message, .. } => json_string_length(message),
            Self::Done { result, error, .. } => {
                result.map_or(0, json_string_length) + error.map_or(0, json_string_length)
            }
            _ => 0,
        }
    }
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
                    .collect(),
                body: request.body.as_deref(),
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
    let capacity = 2 + out.iter().map(ActionOut::capacity_hint).sum::<usize>();
    let mut bytes = Vec::with_capacity(capacity);
    if serde_json::to_writer(&mut bytes, &out).is_err() {
        return String::new();
    }
    String::from_utf8(bytes).expect("JSON serialization produces UTF-8")
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
        feature = "venue-coinbase",
        feature = "venue-paradex",
        feature = "venue-binance",
        feature = "venue-apexomni",
        feature = "venue-asterdex",
        feature = "venue-breakoutprop",
        feature = "venue-kucoin",
        feature = "venue-polymarket",
        feature = "venue-blofin",
        feature = "venue-bybit",
        feature = "venue-woo",
        feature = "venue-cryptocom",
        feature = "venue-bingx",
        feature = "venue-mexc",
        feature = "venue-bitget",
        feature = "venue-grvt",
        feature = "venue-bitrue",
        feature = "venue-backpack",
        feature = "venue-phemex",
        feature = "venue-weex",
        feature = "venue-hyperliquid",
        feature = "venue-kraken",
        feature = "venue-orderly",
        feature = "venue-lighter",
        feature = "venue-okx"
    ))]
    #[wasm_bindgen(constructor)]
    pub fn new(venue: &str, options_json: &str) -> Result<WasmEngine, JsError> {
        let options: EngineOptions = serde_json::from_str(options_json)?;
        #[cfg(feature = "venue-weex")]
        if venue == "weex"
            && !options.public_data_only
            && options.passphrase.as_deref().is_none_or(str::is_empty)
        {
            return Err(JsError::new("WEEX requires a passphrase"));
        }
        #[cfg(feature = "venue-kraken")]
        if venue == "kraken"
            && !options.public_data_only
            && options.key.as_deref().is_some_and(|key| !key.is_empty())
            && let Some(secret) = options.secret.as_deref()
        {
            safe_cex_venue_kraken::sign::Signer::new(secret)
                .map_err(|error| JsError::new(&error))?;
        }
        let inner: Box<dyn AnyEngine> = match venue {
            #[cfg(feature = "venue-breakoutprop")]
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
            #[cfg(feature = "venue-grvt")]
            "grvt" => Box::new(Engine::new(Grvt::new(GrvtOptions {
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
                trading_account_id: options.grvt_trading_account_id,
                signer_address: options.grvt_signer_address,
                session_proxy: options.grvt_session_proxy,
            }))),
            #[cfg(feature = "venue-coinbase")]
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
            #[cfg(feature = "venue-polymarket")]
            "polymarket" => Box::new(Engine::new(Polymarket::new(PolymarketOptions {
                public_data_only: options.public_data_only,
                key: options.key,
                polymarket: options.polymarket,
            }))),
            #[cfg(feature = "venue-paradex")]
            "paradex" => Box::new(Engine::new(Paradex::new(ParadexOptions {
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key,
                paradex_address: options.paradex_address,
                subkey_address: options.subkey_address,
            }))),
            #[cfg(feature = "venue-apexomni")]
            "apexomni" => Box::new(Engine::new(ApexOmni::new(ApexOmniOptions {
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
                passphrase: options.passphrase,
                omni_key_seed: options.omni_key_seed,
                tick_interval_ms: None,
            }))),
            #[cfg(feature = "venue-kraken")]
            "kraken" => Box::new(Engine::new(Kraken::new(KrakenOptions {
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
            }))),
            #[cfg(feature = "venue-asterdex")]
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
            #[cfg(feature = "venue-orderly")]
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
            #[cfg(feature = "venue-lighter")]
            "lighter" => Box::new(Engine::new(Lighter::new(LighterOptions {
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
                application_id: options.application_id,
                api_key_index: options.api_key_index,
                integrator_account_index: options.lighter_integrator_account_index,
                stop_slippage_pct: options.lighter_stop_slippage_pct.unwrap_or(1.0),
            }))),
            #[cfg(feature = "venue-bitrue")]
            "bitrue" => Box::new(Engine::new(Bitrue::new(BitrueOptions {
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
                recv_window: options.recv_window.unwrap_or(20_000.0).clamp(1.0, 60_000.0) as u32,
            }))),
            #[cfg(feature = "venue-backpack")]
            "backpack" => Box::new(Engine::new(Backpack::new(BackpackOptions {
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
                recv_window: options.recv_window,
            }))),
            #[cfg(feature = "venue-blofin")]
            "blofin" => Box::new(Engine::new(Blofin::new(BlofinOptions {
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
                passphrase: options.passphrase,
            }))),
            #[cfg(feature = "venue-mexc")]
            "mexc" => Box::new(Engine::new(MexcFutures::new(MexcOptions {
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
            }))),
            #[cfg(feature = "venue-phemex")]
            "phemex" => Box::new(Engine::new(PhemexPerpetuals::new(PhemexOptions {
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
            }))),
            #[cfg(feature = "venue-binance")]
            "binance" | "binancepm" | "binancespot" | "binanceinverse" => {
                Box::new(Engine::new(BinanceFutures::new(BinanceOptions {
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
            #[cfg(feature = "venue-kucoin")]
            "kucoin" => Box::new(Engine::new(Kucoin::new(KucoinOptions {
                broker: options.extra.and_then(|e| e.kucoin_broker),
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
                passphrase: options.passphrase,
            }))),
            #[cfg(feature = "venue-bingx")]
            "bingx" => Box::new(Engine::new(Bingx::new(BingxOptions {
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
            }))),
            #[cfg(feature = "venue-weex")]
            "weex" => Box::new(Engine::new(WeexFutures::new(WeexOptions {
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
                passphrase: options.passphrase,
                private_ws_headers: options.private_ws_headers,
            }))),
            #[cfg(feature = "venue-woo")]
            "woo" => Box::new(Engine::new(Woo::new(WooOptions {
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
                application_id: options.application_id,
            }))),
            #[cfg(feature = "venue-bybit")]
            "bybit" | "bybitinverse" | "bybitspot" | "bybitdemo" | "hyrotrader" => {
                let options = BybitOptions {
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
            #[cfg(feature = "venue-bitget")]
            "bitget" | "bitgetusdc" | "bitgetinverse" => {
                if !options.public_data_only
                    && options.key.as_ref().is_some_and(|s| !s.is_empty())
                    && options.secret.as_ref().is_some_and(|s| !s.is_empty())
                    && options.passphrase.as_ref().is_none_or(|s| s.is_empty())
                {
                    return Err(JsError::new("Bitget requires a passphrase"));
                }
                Box::new(Engine::new(BitgetFutures::new(BitgetOptions {
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
            #[cfg(feature = "venue-cryptocom")]
            "cryptocom" => Box::new(Engine::new(Cryptocom::new(CryptocomOptions {
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
            }))),
            #[cfg(feature = "venue-hyperliquid")]
            "hyperliquid" => {
                if options.enable_spot_markets {
                    return Err(JsError::new(
                        "Rust Hyperliquid currently supports perpetual markets only",
                    ));
                }
                Box::new(Engine::new(Hyperliquid::new(HyperliquidOptions {
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
                })))
            }
            #[cfg(feature = "venue-okx")]
            "okx" | "okxeu" | "okxdemo" | "okxeudemo" => {
                if !options.public_data_only
                    && options.key.as_ref().is_some_and(|s| !s.is_empty())
                    && options.secret.as_ref().is_some_and(|s| !s.is_empty())
                    && options.passphrase.as_ref().is_none_or(|s| s.is_empty())
                {
                    return Err(JsError::new("OKX requires a passphrase"));
                }
                Box::new(Engine::new(Okx::new(OkxOptions {
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
    #[wasm_bindgen(js_name = wsMessageActions)]
    pub fn ws_message_actions(&mut self, socket: u32, text: &str, now: f64) -> Option<String> {
        let actions = self.ws_message(socket, text, now);
        (!actions.is_empty()).then_some(actions)
    }

    /// A byte-input alternative that rejects invalid UTF-8 before handling any event.
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

    /// Records are socket u32, UTF-8 byte length u32, arrival time f64, text.
    /// The payloads borrow this single crossing's buffer without per-message copies.
    #[wasm_bindgen(js_name = wsMessages)]
    pub fn ws_messages(&mut self, data: &[u8]) -> Result<String, JsError> {
        // Validate first: a corrupt batch must never commit a valid prefix.
        for record in MessageBatch::new(data) {
            record.map_err(JsError::new)?;
        }
        for record in MessageBatch::new(data) {
            let (socket, now, text) = record.map_err(JsError::new)?;
            self.inner.handle(now, Event::WsMessage { socket, text });
        }
        Ok(self.flush())
    }

    /// Mixed records: socket u32, byte length u32, arrival time f64, kind u32,
    /// then payload. Kind zero is UTF-8 text; kind one is binary.
    #[wasm_bindgen(js_name = wsEvents)]
    pub fn ws_events(&mut self, data: &[u8]) -> Result<String, JsError> {
        dispatch_events(data, |socket, now, payload| {
            self.inner.handle(
                now,
                match payload {
                    Payload::Text(text) => Event::WsMessage { socket, text },
                    Payload::Binary(data) => Event::WsBinary { socket, data },
                },
            );
        })
        .map_err(JsError::new)?;
        Ok(self.flush())
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

    pub fn timer(&mut self, timer: u32, now: f64) -> String {
        let timer = TimerId(timer);
        self.inner.handle(now, Event::TimerFired { timer });
        self.flush()
    }

    pub fn command(&mut self, command_json: &str, now: f64) -> Result<String, JsError> {
        let event = match safe_cex_engine::command::decode_host_input(command_json)? {
            safe_cex_engine::command::HostInput::Command(command) => Event::Command(command),
            safe_cex_engine::command::HostInput::TickIntervalChanged { interval_ms } => {
                Event::TickIntervalChanged { interval_ms }
            }
        };
        self.inner.handle(now, event);
        Ok(self.flush())
    }

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

#[cfg(test)]
mod action_tests {
    use super::*;
    use safe_cex_engine::Command;
    #[cfg(feature = "venue-asterdex")]
    use safe_cex_engine::model::{CancelOrderRef, StpMode};

    #[test]
    fn subscription_json_preserves_escaped_and_unicode_messages() {
        #[derive(Deserialize)]
        struct Send {
            op: String,
            socket: u32,
            text: String,
        }
        let text = "\"\\\n\r\t\u{0008}\u{000c}\u{0000}\u{001f}é東京🦀".repeat(500);
        let actions = vec![Action::WsSend {
            socket: SocketId(7),
            text: text.clone(),
        }];
        let encoded = encode(&actions);
        let decoded: Vec<Send> = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].op, "wsSend");
        assert_eq!(decoded[0].socket, 7);
        assert_eq!(decoded[0].text, text);
        assert!(encoded.capacity() <= encoded.len() + 128);
    }
    #[test]
    fn account_command_aliases_share_the_engine_read() {
        for (op, enabled) in [
            ("getAccount", cfg!(feature = "venue-apexomni")),
            (
                "fetchAccount",
                cfg!(any(
                    feature = "venue-asterdex",
                    feature = "venue-polymarket"
                )),
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
    fn http_route_metadata_is_optional_and_has_stable_host_names() {
        let actions = [
            None,
            Some(safe_cex_engine::RequestRoute::Direct),
            Some(safe_cex_engine::RequestRoute::PublicCors),
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
                body: None,
            })
        })
        .collect::<Vec<_>>();
        let encoded = encode(&actions);
        let rows: Vec<serde_json::Value> = serde_json::from_str(&encoded).unwrap();
        assert!(rows[0].get("route").is_none());
        assert_eq!(rows[1]["route"], "direct");
        assert_eq!(rows[2]["route"], "publicCors");
        assert!(rows[0].get("timeoutMs").is_none());
        assert_eq!(rows[1]["timeoutMs"], 5_000);
        assert_eq!(rows[2]["timeoutMs"], 5_000);
        assert!(
            rows.iter()
                .all(|row| row["headers"] == serde_json::json!([]))
        );
    }

    #[test]
    fn http_capacity_hint_bounds_maximum_metadata_and_escaped_fields() {
        let escaped = "\"\\\n\u{0000}東京".repeat(500);
        for headers in [
            vec![],
            vec![("x-route", escaped.as_str()), ("x-other", "\"\\")],
        ] {
            let out = ActionOut::Http {
                request: u32::MAX,
                method: safe_cex_engine::Method::Delete.as_str(),
                route: Some(safe_cex_engine::RequestRoute::PublicCors.as_str()),
                timeout_ms: Some(u32::MAX),
                url: &escaped,
                headers,
                body: Some(&escaped),
            };
            let hint = out.capacity_hint();
            let encoded = serde_json::to_vec(&out).unwrap();
            assert!(encoded.len() <= hint);
            let mut reserved = Vec::with_capacity(hint);
            let capacity = reserved.capacity();
            serde_json::to_writer(&mut reserved, &out).unwrap();
            assert_eq!(reserved.capacity(), capacity);
            assert_eq!(reserved, encoded);
            let decoded: serde_json::Value = serde_json::from_slice(&reserved).unwrap();
            assert_eq!(decoded["url"], escaped);
            assert_eq!(decoded["body"], escaped);
            assert_eq!(decoded["route"], "publicCors");
            assert_eq!(decoded["timeoutMs"], u32::MAX);
        }
    }

    #[derive(Default)]
    struct Messages {
        actions: Vec<Action>,
    }

    impl AnyEngine for Messages {
        fn account_id(&self) -> Option<String> {
            None
        }

        fn start(&mut self, _now: f64) {}
        fn handle(&mut self, now: f64, event: Event<'_>) {
            if let Event::WsBinary { socket, data } = event {
                self.actions.push(Action::WsSend {
                    socket,
                    text: format!("binary:{now}:{data:?}"),
                });
            }
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

    #[test]
    fn mixed_websocket_api_matches_single_events_past_host_batch_limit() {
        let mut single = WasmEngine {
            inner: Box::<Messages>::default(),
        };
        let mut batched = WasmEngine {
            inner: Box::<Messages>::default(),
        };
        let mut data = Vec::new();
        let mut expected = Vec::<serde_json::Value>::new();
        // Raw callers may submit more than the host's 256-message limit.
        for index in 0..257_u32 {
            let socket = index % 3;
            let now = 300.5 - f64::from(index);
            let kind = index % 2;
            let payload = if kind == 0 {
                "雪😀".as_bytes()
            } else {
                &[0, 255]
            };
            data.extend(socket.to_le_bytes());
            data.extend((payload.len() as u32).to_le_bytes());
            data.extend(now.to_le_bytes());
            data.extend(kind.to_le_bytes());
            data.extend(payload);
            let actions = if kind == 0 {
                single.ws_message(socket, std::str::from_utf8(payload).unwrap(), now)
            } else {
                single.ws_binary(socket, payload, now)
            };
            expected.extend(serde_json::from_str::<Vec<serde_json::Value>>(&actions).unwrap());
        }
        let actual = batched.ws_events(&data).unwrap();
        assert_eq!(
            serde_json::from_str::<Vec<serde_json::Value>>(&actual).unwrap(),
            expected
        );
        assert_eq!(batched.ws_events(&[]).unwrap(), "");
    }

    #[cfg(feature = "venue-asterdex")]
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
            let parsed = safe_cex_engine::command::decode_command(json);
            #[cfg(feature = "venue-asterdex")]
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
            #[cfg(not(feature = "venue-asterdex"))]
            if cfg!(feature = "venue-polymarket") && matches!(index, 2 | 3) {
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
    #[test]
    fn preserves_socket_routing_identity_and_omits_absent_metadata() {
        let encoded = encode(&[
            Action::WsConnect {
                binary: false,
                socket: SocketId(1),
                url: "wss://example.test".into(),
                channel: Some(safe_cex_engine::SocketChannel::Business),
                context: Some(safe_cex_engine::SocketContext {
                    feed: "swap\nfeed".into(),
                    recover: true,
                }),
            },
            Action::WsConnectWithHeaders {
                socket: SocketId(2),
                url: "wss://example.test/private".into(),
                headers: vec![("x-auth", "signed".into())],
                channel: None,
                context: None,
            },
        ]);
        let rows: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(rows[0]["channel"], "business");
        assert_eq!(rows[0]["context"]["feed"], "swap\nfeed");
        assert_eq!(rows[0]["context"]["recover"], true);
        assert_eq!(
            rows[1]["headers"][0],
            serde_json::json!(["x-auth", "signed"])
        );
        assert!(rows[1].get("channel").is_none());
        assert!(rows[1].get("context").is_none());
    }
    #[test]
    fn forwards_deactivation_to_the_host() {
        let encoded = encode(&[Action::Notice(Notice::Deactivate)]);
        let rows: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(rows, serde_json::json!([{"op": "deactivate"}]));
    }
}
