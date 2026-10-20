//! WebAssembly host surface for browser workers and Cloudflare Workers.
//!
//! JavaScript owns sockets, fetch and timers; it forwards each event here and
//! executes the returned actions. Every call returns the pending actions as a
//! JSON array string, or an empty string when there are none.
#[cfg(not(any(
    feature = "binance",
    feature = "blofin",
    feature = "bybit",
    feature = "cryptocom",
    feature = "bingx",
    feature = "mexc",
    feature = "bitget",
    feature = "backpack",
    feature = "phemex",
    feature = "weex",
    feature = "hyperliquid",
    feature = "okx"
)))]
compile_error!(
    "safe-cex-wasm requires at least one venue feature: binance, bybit, cryptocom, bingx, blofin, mexc, backpack, bitget, hyperliquid, okx, phemex or weex"
);

use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{
    Action, Command, Engine, Event, LogLevel, Notice, Query, RequestId, SocketId, TimerId, Venue,
};
#[cfg(feature = "backpack")]
use safe_cex_venue_backpack::{Backpack, Options as BackpackOptions};
#[cfg(feature = "binance")]
use safe_cex_venue_binance::{BinanceFutures, Options as BinanceOptions};
#[cfg(feature = "bingx")]
use safe_cex_venue_bingx::{Bingx, Options as BingxOptions};
#[cfg(feature = "bitget")]
use safe_cex_venue_bitget::{BitgetFutures, Options as BitgetOptions, Product};
#[cfg(feature = "blofin")]
use safe_cex_venue_blofin::{Blofin, Options as BlofinOptions};
#[cfg(feature = "bybit")]
use safe_cex_venue_bybit::{BybitLinear, Options as BybitOptions};
#[cfg(feature = "cryptocom")]
use safe_cex_venue_cryptocom::{Cryptocom, Options as CryptocomOptions};
#[cfg(feature = "hyperliquid")]
use safe_cex_venue_hyperliquid::{Builder, Hyperliquid, Options as HyperliquidOptions};
#[cfg(feature = "mexc")]
use safe_cex_venue_mexc::{MexcFutures, Options as MexcOptions};
#[cfg(feature = "okx")]
use safe_cex_venue_okx::{Okx, Options as OkxOptions};
#[cfg(feature = "phemex")]
use safe_cex_venue_phemex::{Options as PhemexOptions, PhemexPerpetuals};
#[cfg(feature = "weex")]
use safe_cex_venue_weex::{Options as WeexOptions, WeexFutures};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

trait AnyEngine {
    fn start(&mut self, now: f64);
    fn handle(&mut self, now: f64, event: Event<'_>);
    fn drain(&mut self) -> Vec<Action>;
    fn frame(&mut self, now: f64, market_only: bool) -> Option<&[u8]>;
    fn snapshot(&mut self, now: f64) -> &[u8];
}

impl<V: Venue> AnyEngine for Engine<V> {
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

#[cfg(any(
    feature = "binance",
    feature = "blofin",
    feature = "bybit",
    feature = "cryptocom",
    feature = "bingx",
    feature = "mexc",
    feature = "bitget",
    feature = "backpack",
    feature = "phemex",
    feature = "weex",
    feature = "hyperliquid",
    feature = "okx"
))]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EngineOptions {
    #[cfg(any(
        feature = "binance",
        feature = "blofin",
        feature = "bybit",
        feature = "cryptocom",
        feature = "hyperliquid",
        feature = "phemex",
        feature = "bitget",
        feature = "okx"
    ))]
    #[serde(default)]
    testnet: bool,
    #[serde(default)]
    public_data_only: bool,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    secret: Option<String>,
    #[cfg(feature = "backpack")]
    #[serde(default)]
    recv_window: Option<f64>,
    #[cfg(any(
        feature = "okx",
        feature = "weex",
        feature = "blofin",
        feature = "bitget"
    ))]
    #[serde(default)]
    passphrase: Option<String>,
    #[cfg(feature = "hyperliquid")]
    #[serde(default)]
    application_id: Option<String>,
    #[cfg(feature = "hyperliquid")]
    #[serde(default)]
    vault: Option<String>,
    #[cfg(feature = "hyperliquid")]
    #[serde(default)]
    hyperliquid_builder: Option<HyperliquidBuilder>,
    #[cfg(feature = "hyperliquid")]
    #[serde(default)]
    enable_spot_markets: bool,
    #[cfg(feature = "weex")]
    #[serde(default)]
    private_ws_headers: bool,
}
#[cfg(feature = "hyperliquid")]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HyperliquidBuilder {
    address: String,
    fee_tenths_bps: u32,
}

#[derive(Serialize)]
#[serde(tag = "op", rename_all = "camelCase")]
enum ActionOut<'a> {
    WsConnect {
        socket: u32,
        url: &'a str,
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

fn encode(actions: &[Action]) -> String {
    if actions.is_empty() {
        return String::new();
    }
    let out: Vec<ActionOut<'_>> = actions
        .iter()
        .map(|action| match action {
            Action::WsConnect { socket, url } => ActionOut::WsConnect {
                socket: socket.0,
                url,
                headers: None,
            },
            Action::WsConnectWithHeaders {
                socket,
                url,
                headers,
            } => ActionOut::WsConnect {
                socket: socket.0,
                url,
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

/// Commands as JavaScript sends them.
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
enum CommandIn {
    SetTickerSymbols {
        symbols: Vec<String>,
    },
    SubscribeOrderBook {
        symbol: String,
    },
    UnsubscribeOrderBook {
        symbol: String,
    },
    SubscribeCandles {
        symbol: String,
        interval: String,
    },
    UnsubscribeCandles {
        symbol: String,
        interval: String,
    },
    PlaceOrders {
        command: u32,
        orders: Vec<PlaceOrderOpts>,
    },
    CancelOrders {
        command: u32,
        ids: Vec<String>,
    },
    AmendOrder {
        command: u32,
        id: String,
        #[serde(default)]
        price: Option<f64>,
        #[serde(default)]
        amount: Option<f64>,
    },
    CancelSymbolOrders {
        command: u32,
        symbol: String,
    },
    #[serde(rename_all = "camelCase")]
    SetLeverage {
        command: u32,
        symbol: String,
        leverage: f64,
    },
    SetMarginType {
        command: u32,
        symbol: String,
        isolated: bool,
    },
    SetPositionMode {
        command: u32,
        hedged: bool,
    },
    FetchLeverage {
        command: u32,
        symbol: String,
    },
    #[serde(rename_all = "camelCase")]
    FetchFills {
        command: u32,
        #[serde(default)]
        symbol: Option<String>,
        #[serde(default)]
        order_id: Option<String>,
        #[serde(default)]
        from: Option<f64>,
        #[serde(default)]
        to: Option<f64>,
        #[serde(default)]
        limit: Option<u32>,
    },
    FetchOrderHistory {
        command: u32,
        #[serde(default)]
        symbol: Option<String>,
        #[serde(default)]
        from: Option<f64>,
        #[serde(default)]
        to: Option<f64>,
        #[serde(default)]
        limit: Option<u32>,
    },
    #[serde(rename_all = "camelCase")]
    FetchIncome {
        command: u32,
        #[serde(default)]
        symbol: Option<String>,
        #[serde(default)]
        income_type: Option<String>,
        #[serde(default)]
        from: Option<f64>,
        #[serde(default)]
        to: Option<f64>,
        #[serde(default)]
        limit: Option<u32>,
    },
    #[serde(rename_all = "camelCase")]
    FindOrderByClientOrderId {
        command: u32,
        symbol: String,
        client_order_id: String,
    },
    FetchPublicTrades {
        command: u32,
        symbol: String,
        #[serde(default)]
        limit: Option<u32>,
    },
    #[serde(rename_all = "camelCase")]
    FetchCandles {
        command: u32,
        symbol: String,
        interval: String,
        #[serde(default)]
        limit: Option<u32>,
        #[serde(default)]
        from: Option<f64>,
        #[serde(default)]
        to: Option<f64>,
    },
}

impl From<CommandIn> for Command {
    fn from(command: CommandIn) -> Self {
        match command {
            CommandIn::FindOrderByClientOrderId {
                command,
                symbol,
                client_order_id,
            } => Self::Query {
                command,
                query: Query::OrderByClientId {
                    symbol,
                    client_order_id,
                },
            },
            CommandIn::SetTickerSymbols { symbols } => Self::SetTickerSymbols { symbols },
            CommandIn::SubscribeOrderBook { symbol } => Self::SubscribeOrderBook { symbol },
            CommandIn::UnsubscribeOrderBook { symbol } => Self::UnsubscribeOrderBook { symbol },
            CommandIn::SubscribeCandles { symbol, interval } => {
                Self::SubscribeCandles { symbol, interval }
            }
            CommandIn::UnsubscribeCandles { symbol, interval } => {
                Self::UnsubscribeCandles { symbol, interval }
            }
            CommandIn::PlaceOrders { command, orders } => Self::PlaceOrders { command, orders },
            CommandIn::CancelOrders { command, ids } => Self::CancelOrders { command, ids },
            CommandIn::AmendOrder {
                command,
                id,
                price,
                amount,
            } => Self::AmendOrder {
                command,
                id,
                price,
                amount,
            },
            CommandIn::SetMarginType {
                command,
                symbol,
                isolated,
            } => Self::SetMarginType {
                command,
                symbol,
                isolated,
            },
            CommandIn::SetPositionMode { command, hedged } => {
                Self::SetPositionMode { command, hedged }
            }
            CommandIn::FetchLeverage { command, symbol } => Self::FetchLeverage { command, symbol },
            CommandIn::FetchFills {
                command,
                symbol,
                order_id,
                from,
                to,
                limit,
            } => Self::Query {
                command,
                query: Query::Fills {
                    symbol,
                    order_id,
                    from_ms: from,
                    to_ms: to,
                    limit,
                },
            },
            CommandIn::FetchOrderHistory {
                command,
                symbol,
                from,
                to,
                limit,
            } => Self::Query {
                command,
                query: Query::OrderHistory {
                    symbol,
                    from_ms: from,
                    to_ms: to,
                    limit,
                },
            },
            CommandIn::FetchIncome {
                command,
                symbol,
                income_type,
                from,
                to,
                limit,
            } => Self::Query {
                command,
                query: Query::Income {
                    symbol,
                    kind: income_type,
                    from_ms: from,
                    to_ms: to,
                    limit,
                },
            },
            CommandIn::FetchPublicTrades {
                command,
                symbol,
                limit,
            } => Self::Query {
                command,
                query: Query::PublicTrades { symbol, limit },
            },
            CommandIn::CancelSymbolOrders { command, symbol } => {
                Self::CancelSymbolOrders { command, symbol }
            }
            CommandIn::SetLeverage {
                command,
                symbol,
                leverage,
            } => Self::SetLeverage {
                command,
                symbol,
                leverage,
            },
            CommandIn::FetchCandles {
                command,
                symbol,
                interval,
                limit,
                from,
                to,
            } => Self::FetchCandles {
                command,
                symbol,
                interval,
                limit,
                from_ms: from,
                to_ms: to,
            },
        }
    }
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
        feature = "binance",
        feature = "blofin",
        feature = "bybit",
        feature = "cryptocom",
        feature = "bingx",
        feature = "mexc",
        feature = "bitget",
        feature = "backpack",
        feature = "phemex",
        feature = "weex",
        feature = "hyperliquid",
        feature = "okx"
    ))]
    #[wasm_bindgen(constructor)]
    pub fn new(venue: &str, options_json: &str) -> Result<WasmEngine, JsError> {
        let options: EngineOptions = serde_json::from_str(options_json)?;
        #[cfg(feature = "weex")]
        if venue == "weex"
            && !options.public_data_only
            && options.passphrase.as_deref().is_none_or(str::is_empty)
        {
            return Err(JsError::new("WEEX requires a passphrase"));
        }
        let inner: Box<dyn AnyEngine> = match venue {
            #[cfg(feature = "backpack")]
            "backpack" => Box::new(Engine::new(Backpack::new(BackpackOptions {
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
                recv_window: options.recv_window,
            }))),
            #[cfg(feature = "blofin")]
            "blofin" => Box::new(Engine::new(Blofin::new(BlofinOptions {
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
                passphrase: options.passphrase,
            }))),
            #[cfg(feature = "mexc")]
            "mexc" => Box::new(Engine::new(MexcFutures::new(MexcOptions {
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
            }))),
            #[cfg(feature = "phemex")]
            "phemex" => Box::new(Engine::new(PhemexPerpetuals::new(PhemexOptions {
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
            }))),
            #[cfg(feature = "binance")]
            "binance" | "binancepm" => Box::new(Engine::new(BinanceFutures::new(BinanceOptions {
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
                portfolio_margin: venue == "binancepm",
            }))),
            #[cfg(feature = "bingx")]
            "bingx" => Box::new(Engine::new(Bingx::new(BingxOptions {
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
            }))),
            #[cfg(feature = "weex")]
            "weex" => Box::new(Engine::new(WeexFutures::new(WeexOptions {
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
                passphrase: options.passphrase,
                private_ws_headers: options.private_ws_headers,
            }))),
            #[cfg(feature = "bybit")]
            "bybit" => Box::new(Engine::new(BybitLinear::new(BybitOptions {
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
            }))),
            #[cfg(feature = "bitget")]
            "bitget" | "bitgetusdc" => {
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
                    product: if venue == "bitgetusdc" {
                        Product::Usdc
                    } else {
                        Product::Usdt
                    },
                })))
            }
            #[cfg(feature = "cryptocom")]
            "cryptocom" => Box::new(Engine::new(Cryptocom::new(CryptocomOptions {
                testnet: options.testnet,
                public_data_only: options.public_data_only,
                key: options.key,
                secret: options.secret,
            }))),
            #[cfg(feature = "hyperliquid")]
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
        let command: CommandIn = serde_json::from_str(command_json)?;
        self.inner.handle(now, Event::Command(command.into()));
        Ok(self.flush())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Messages {
        actions: Vec<Action>,
    }

    impl AnyEngine for Messages {
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
    fn forwards_leverage_metadata_command() {
        let wire: CommandIn =
            serde_json::from_str(r#"{"op":"fetchLeverage","command":17,"symbol":"BTCUSDT"}"#)
                .unwrap();
        assert!(matches!(
            Command::from(wire),
            Command::FetchLeverage { command: 17, symbol } if symbol == "BTCUSDT"
        ));
    }
}
