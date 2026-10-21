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
    Action, Engine, Event, Health, LogLevel, Notice, RequestId, SocketId, TimerId, Value, Venue,
};
use serde::Serialize;
use wasm_bindgen::prelude::*;

trait AnyEngine {
    fn capabilities(&self) -> safe_cex_engine::Capabilities;
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
    fn health(&self) -> Health {
        Health::default()
    }
}

impl<V: Venue> AnyEngine for Engine<V> {
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        Engine::capabilities(self)
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

    fn health(&self) -> Health {
        Engine::health(self).clone()
    }
}

/// Owns whichever venue engine the registry builds.
struct Boxed;

impl safe_cex_venues::Visit for Boxed {
    type Output = Box<dyn AnyEngine>;
    fn visit<V: Venue + 'static>(self, engine: Engine<V>) -> Box<dyn AnyEngine> {
        Box::new(engine)
    }
}

#[derive(Serialize)]
#[serde(tag = "op", rename_all = "camelCase")]
enum ActionOut<'a> {
    Deactivate,
    StartupFailed {
        detail: &'a safe_cex_engine::VenueError,
    },
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
    Event {
        level: &'static str,
        code: &'static str,
        message: &'a str,
        #[serde(skip_serializing_if = "Fields::is_empty")]
        fields: Fields<'a>,
    },
    CommandError {
        command: u32,
        message: &'a str,
    },
    Done {
        command: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<&'a str>,
        /// The message, or the partial-batch JSON `host.mjs` unpacks.
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<std::borrow::Cow<'a, str>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<&'a safe_cex_engine::VenueError>,
    },
    CommandProgress {
        command: u32,
        receipts: &'a [safe_cex_engine::OrderIdentityReceipt],
    },
}

struct Fields<'a>(&'a [(&'static str, Value)]);

impl Fields<'_> {
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl serde::Serialize for Fields<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, value) in self.0 {
            match value {
                Value::Str(text) => map.serialize_entry(key, text)?,
                // JSON has no non-finite numbers.
                Value::Num(number) if number.is_finite() => map.serialize_entry(key, number)?,
                Value::Num(_) => map.serialize_entry(key, &())?,
                Value::Bool(flag) => map.serialize_entry(key, flag)?,
                Value::Redacted => map.serialize_entry(key, safe_cex_engine::log::REDACTED)?,
            }
        }
        map.end()
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
            Action::Notice(Notice::CommandError { command, message }) => ActionOut::CommandError {
                command: *command,
                message,
            },
            Action::Notice(Notice::StartupFailed { error }) => {
                ActionOut::StartupFailed { detail: error }
            }
            Action::Notice(Notice::Log { level, message }) => ActionOut::Log {
                level: match level {
                    LogLevel::Info => "info",
                    LogLevel::Warning => "warning",
                    LogLevel::Error => "error",
                },
                message,
            },
            Action::Notice(Notice::Event(event)) => ActionOut::Event {
                level: event.level.as_str(),
                code: event.code,
                message: &event.message,
                fields: Fields(&event.fields),
            },
            Action::Notice(Notice::CommandDone { command, result }) => ActionOut::Done {
                command: *command,
                result: result.as_ref().ok().map(String::as_str),
                error: result.as_ref().err().map(|error| error.as_str().into()),
                detail: None,
            },
            Action::Notice(Notice::CommandFailed { command, error }) => ActionOut::Done {
                command: *command,
                result: None,
                error: Some(error.legacy_text().into()),
                detail: Some(error),
            },
            Action::Notice(Notice::CommandProgress { command, receipts }) => {
                ActionOut::CommandProgress {
                    command: *command,
                    receipts,
                }
            }
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
        let options: safe_cex_venues::EngineOptions = serde_json::from_str(options_json)
            .map_err(|_| JsError::new("Invalid engine options"))?;
        let inner = safe_cex_venues::build(venue, options, Boxed)
            .map_err(|error| JsError::new(&error.message))?;
        Ok(Self { inner })
    }

    fn flush(&mut self) -> String {
        encode(&self.inner.drain())
    }

    /// Health counters as JSON (`safe_cex_engine::Health`, camelCase).
    pub fn health(&self) -> String {
        let h = self.inner.health();
        format!(
            r#"{{"wsOpened":{},"wsClosed":{},"reconnects":{},"authFailures":{},"httpFailures":{},"rateLimited":{},"snapshotsDiscarded":{},"rowsSkipped":{}}}"#,
            h.ws_opened,
            h.ws_closed,
            h.reconnects,
            h.auth_failures,
            h.http_failures,
            h.rate_limited,
            h.snapshots_discarded,
            h.rows_skipped
        )
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

    /// The venue's capability record as JSON; fixed for the engine's lifetime.
    /// Snapshot frames carry the same record for consumers without the engine.
    pub fn capabilities(&self) -> Result<String, JsError> {
        Ok(serde_json::to_string(&self.inner.capabilities())?)
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
    /// Each venue's record equals the name lists `rust.exchange.ts` used before
    /// the engine reported it.
    #[cfg(all(
        feature = "apexomni",
        feature = "asterdex",
        feature = "backpack",
        feature = "binance",
        feature = "bingx",
        feature = "bitget",
        feature = "bitrue",
        feature = "blofin",
        feature = "breakoutprop",
        feature = "bybit",
        feature = "coinbase",
        feature = "cryptocom",
        feature = "grvt",
        feature = "hyperliquid",
        feature = "kraken",
        feature = "kucoin",
        feature = "kucoinspot",
        feature = "lighter",
        feature = "mexc",
        feature = "okx",
        feature = "orderly",
        feature = "paradex",
        feature = "phemex",
        feature = "polymarket",
        feature = "txflow",
        feature = "weex",
        feature = "woo"
    ))]
    #[test]
    fn capabilities_match_the_former_bridge_name_lists() {
        use safe_cex_engine::{
            AccountLookup, CancelAll, Capabilities, LeverageQuery, MaxLeverage, QuantityUnits,
        };
        const ORDERLY: [&str; 6] = [
            "tealstreet-dex",
            "kodiakfi",
            "mode",
            "salsadex",
            "what",
            "woofi",
        ];
        const OKX: [&str; 4] = ["okx", "okxeu", "okxdemo", "okxeudemo"];
        let venues = [
            "coinbase",
            "coinbasespot",
            "apexomni",
            "asterdex",
            "breakoutprop",
            "binance",
            "binancespot",
            "binanceinverse",
            "backpack",
            "binancepm",
            "bitget",
            "bitgetusdc",
            "bitgetinverse",
            "blofin",
            "bitrue",
            "grvt",
            "mexc",
            "woo",
            "bingx",
            "bybit",
            "bybitinverse",
            "bybitspot",
            "bybitdemo",
            "hyrotrader",
            "cryptocom",
            "kucoin",
            "kucoinspot",
            "polymarket",
            "hyperliquid",
            "hyperliquidspot",
            "paradex",
            "txflow",
            "kraken",
            "krakenspot",
            "lighter",
            "lighter-robinhood",
            "weex",
            "phemex",
        ]
        .into_iter()
        .chain(ORDERLY)
        .chain(OKX);
        let any = |names: &[&str], venue: &str| names.contains(&venue);
        let engine_options = |venue: &str, extra: &str| {
            let endpoints = if venue == "txflow" {
                r#","testnet":true"#
            } else if venue == "hyrotrader" {
                r#","endpoints":{"rest":"https://hyro.test","publicWs":"wss://hyro.test/public","privateWs":"wss://hyro.test/private"}"#
            } else {
                ""
            };
            format!(r#"{{"publicDataOnly":true{endpoints}{extra}}}"#)
        };
        for venue in venues {
            let no_history = any(
                &[
                    "breakoutprop",
                    "hyperliquidspot",
                    "krakenspot",
                    "binancespot",
                    "binanceinverse",
                ],
                venue,
            );
            let no_orders_or_income =
                no_history || any(&["apexomni", "bitrue", "kucoinspot"], venue);
            let expected = Capabilities {
                account: if any(&["apexomni", "kucoinspot", "asterdex", "polymarket"], venue) {
                    AccountLookup::Query
                } else if any(&["coinbase", "coinbasespot"], venue) || any(&ORDERLY, venue) {
                    AccountLookup::AfterLoad
                } else {
                    AccountLookup::Unsupported
                },
                client_order_lookup: any(
                    &[
                        "hyperliquid",
                        "grvt",
                        "paradex",
                        "bitrue",
                        "kucoinspot",
                        "phemex",
                        "binance",
                        "binancepm",
                    ],
                    venue,
                ) || any(&OKX, venue),
                fill_history: !no_history,
                order_history: !no_orders_or_income,
                income_history: !no_orders_or_income,
                fills_by_order_id: venue == "grvt",
                cancel_all: if any(
                    &[
                        "polymarket",
                        "breakoutprop",
                        "woo",
                        "grvt",
                        "kucoin",
                        "kucoinspot",
                    ],
                    venue,
                ) || any(&ORDERLY, venue)
                {
                    CancelAll::Native
                } else if any(
                    &[
                        "phemex",
                        "asterdex",
                        "bitget",
                        "bitgetinverse",
                        "bitgetusdc",
                        "blofin",
                        "bitrue",
                    ],
                    venue,
                ) || any(&OKX, venue)
                {
                    CancelAll::EachPlacedSymbol
                } else {
                    CancelAll::EachOpenSymbol
                },
                leverage_query: if any(
                    &[
                        "asterdex",
                        "blofin",
                        "bitrue",
                        "grvt",
                        "paradex",
                        "txflow",
                        "coinbase",
                        "coinbasespot",
                        "polymarket",
                    ],
                    venue,
                ) || any(&OKX, venue)
                    || any(&ORDERLY, venue)
                {
                    LeverageQuery::OnRead
                } else if any(&["mexc", "woo", "kraken"], venue) {
                    LeverageQuery::OnMetadata
                } else {
                    LeverageQuery::Settings
                },
                max_leverage: match venue {
                    "bitrue" => MaxLeverage::Query,
                    "polymarket" => MaxLeverage::Rails,
                    "breakoutprop" => MaxLeverage::Unsupported,
                    _ => MaxLeverage::Market,
                },
                account_leverage: any(&ORDERLY, venue),
                account_margin_type: any(&["blofin", "kucoinspot"], venue),
                cancel_with_symbols: any(&["asterdex", "kucoinspot"], venue),
                amend_with_symbol: venue == "kucoinspot",
                order_book_query: any(&["asterdex", "polymarket"], venue),
                options: false,
                quantity_units: match venue {
                    "mexc" | "blofin" | "kucoin" | "coinbase" | "coinbasespot" => {
                        QuantityUnits::NativePlacement
                    }
                    "bitgetinverse" => QuantityUnits::InverseNative,
                    "okx" | "okxeu" | "okxdemo" | "okxeudemo" => QuantityUnits::ContractCount,
                    "binanceinverse" => QuantityUnits::InverseContractSized,
                    _ => QuantityUnits::Base,
                },
                account_tier: any(&["lighter", "lighter-robinhood"], venue),
                closed_positions: any(
                    &["bybit", "bybitdemo", "hyrotrader", "bybitinverse", "blofin"],
                    venue,
                ) || any(&OKX, venue),
                balance_info: any(
                    &[
                        "bybit",
                        "bybitdemo",
                        "hyrotrader",
                        "bybitinverse",
                        "bybitspot",
                    ],
                    venue,
                ),
            };
            let Ok(engine) = super::WasmEngine::new(venue, &engine_options(venue, "")) else {
                panic!("{venue} engine constructs");
            };
            assert_eq!(engine.inner.capabilities(), expected, "{venue}");
        }
        for (venue, options) in [
            ("bybit", true),
            ("bybitinverse", false),
            ("hyrotrader", false),
        ] {
            let Ok(engine) = super::WasmEngine::new(
                venue,
                &engine_options(venue, r#","enableOptionMarkets":true"#),
            ) else {
                panic!("{venue} engine constructs");
            };
            assert_eq!(engine.inner.capabilities().options, options, "{venue}");
        }
    }

    #[test]
    fn command_progress_serializes_native_identity_namespaces_in_action_order() {
        use safe_cex_engine::{Action, Notice, OrderIdentityKind, OrderIdentityReceipt};
        let receipts = [
            (OrderIdentityKind::ClientOrderId, "client"),
            (OrderIdentityKind::ClientOrderIndex, "9007199254740993"),
            (OrderIdentityKind::OrderCode, "code"),
        ]
        .into_iter()
        .map(|(kind, identity)| OrderIdentityReceipt {
            symbol: "BTC-USD".into(),
            kind,
            identity: identity.into(),
        })
        .collect::<Vec<_>>();
        let encoded = super::encode(&[
            Action::Notice(Notice::CommandProgress {
                command: 19,
                receipts,
            }),
            Action::Notice(Notice::CommandDone {
                command: 19,
                result: Ok("[]".into()),
            }),
        ]);
        assert_eq!(
            encoded,
            r#"[{"op":"commandProgress","command":19,"receipts":[{"symbol":"BTC-USD","kind":"clientOrderId","identity":"client"},{"symbol":"BTC-USD","kind":"clientOrderIndex","identity":"9007199254740993"},{"symbol":"BTC-USD","kind":"orderCode","identity":"code"}]},{"op":"done","command":19,"result":"[]"}]"#
        );
    }

    #[test]
    fn attributed_command_errors_preserve_command_and_precede_result() {
        use safe_cex_engine::{Action, Notice};
        let encoded = super::encode(&[
            Action::Notice(Notice::CommandError {
                command: 19,
                message: "placement rejected".into(),
            }),
            Action::Notice(Notice::CommandDone {
                command: 19,
                result: Err("placement rejected".into()),
            }),
        ]);
        assert_eq!(
            encoded,
            r#"[{"op":"commandError","command":19,"message":"placement rejected"},{"op":"done","command":19,"error":"placement rejected"}]"#
        );
    }

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
            fn capabilities(&self) -> safe_cex_engine::Capabilities {
                safe_cex_engine::Capabilities::BASE
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

    #[test]
    fn structured_failures_keep_the_legacy_error_text_beside_their_detail() {
        let ids = ["a".to_owned()];
        let actions = [
            Action::Notice(Notice::CommandFailed {
                command: 1,
                error: safe_cex_engine::VenueError::new("leg 2 rejected").partial(&ids, 2, 1),
            }),
            Action::Notice(Notice::CommandDone {
                command: 2,
                result: Err("plain".into()),
            }),
            Action::Notice(Notice::CommandDone {
                command: 3,
                result: Ok("[]".into()),
            }),
        ];
        assert_eq!(
            encode(&actions),
            concat!(
                r#"[{"op":"done","command":1,"#,
                r#""error":"{\"message\":\"leg 2 rejected\",\"partialOrderIds\":[\"a\"],\"partialOrderCount\":1,\"totalOrderCount\":2,\"failedOrderIndex\":1}","#,
                r#""detail":{"message":"leg 2 rejected","partial":{"orderIds":["a"],"total":2,"failedIndex":1}}},"#,
                r#"{"op":"done","command":2,"error":"plain"},"#,
                r#"{"op":"done","command":3,"result":"[]"}]"#,
            )
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
        fn capabilities(&self) -> safe_cex_engine::Capabilities {
            safe_cex_engine::Capabilities::BASE
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

    #[test]
    fn events_and_health_encode_as_the_documented_json() {
        let event = Action::Notice(Notice::Event(safe_cex_engine::LogEvent {
            level: safe_cex_engine::Level::Warn,
            code: "auth.rejected",
            message: "rejected \"key\"".into(),
            fields: vec![
                ("key", Value::Redacted),
                ("status", Value::Num(401.)),
                ("ratio", Value::Num(f64::NAN)),
                ("retry", Value::Bool(false)),
                ("venue", Value::Str("bybit".into())),
            ],
        }));
        assert_eq!(
            encode(&[event]),
            r#"[{"op":"event","level":"warn","code":"auth.rejected","message":"rejected \"key\"","fields":{"key":"[redacted]","status":401.0,"ratio":null,"retry":false,"venue":"bybit"}}]"#
        );

        struct Counted;
        impl AnyEngine for Counted {
            fn capabilities(&self) -> safe_cex_engine::Capabilities {
                safe_cex_engine::Capabilities::BASE
            }
            fn apply_public_feed(
                &mut self,
                _: f64,
                _: safe_cex_engine::PublicFeedUpdate,
            ) -> Result<(), &'static str> {
                Ok(())
            }
            fn start(&mut self, _: f64) {}
            fn handle(&mut self, _: f64, _: Event<'_>) {}
            fn drain(&mut self) -> Vec<Action> {
                Vec::new()
            }
            fn frame(&mut self, _: f64, _: bool) -> Option<&[u8]> {
                None
            }
            fn snapshot(&mut self, _: f64) -> &[u8] {
                &[]
            }
            fn health(&self) -> Health {
                Health {
                    ws_opened: 1,
                    ws_closed: 2,
                    reconnects: 3,
                    auth_failures: 4,
                    http_failures: 5,
                    rate_limited: 6,
                    snapshots_discarded: 7,
                    rows_skipped: 8,
                }
            }
        }
        let health: serde_json::Value = serde_json::from_str(
            &WasmEngine {
                inner: Box::new(Counted),
            }
            .health(),
        )
        .unwrap();
        assert_eq!(
            health,
            serde_json::json!({"wsOpened":1,"wsClosed":2,"reconnects":3,"authFailures":4,
                "httpFailures":5,"rateLimited":6,"snapshotsDiscarded":7,"rowsSkipped":8})
        );
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
