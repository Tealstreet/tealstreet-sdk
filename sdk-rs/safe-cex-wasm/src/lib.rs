//! WebAssembly host surface for browser workers and Cloudflare Workers.
//!
//! JavaScript owns sockets, fetch and timers; it forwards each event here and
//! executes the returned actions. Every call returns the pending actions as a
//! JSON array string, or an empty string when there are none.
use safe_cex_engine::model::{Candle, PlaceOrderOpts};
use safe_cex_engine::{
    Action, Command, Engine, Event, LogLevel, Notice, RequestId, SocketId, TimerId, Venue,
};
use safe_cex_venue_binance::{BinanceFutures, Options as BinanceOptions};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

trait AnyEngine {
    fn start(&mut self, now: f64);
    fn handle(&mut self, now: f64, event: Event<'_>);
    fn drain(&mut self) -> Vec<Action>;
    fn store_update(&mut self) -> String;
    fn ticker_deltas(&mut self) -> Vec<f64>;
    fn changed_books(&mut self) -> Vec<String>;
    fn book_ops(&mut self, symbol: &str) -> Vec<f64>;
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

    fn store_update(&mut self) -> String {
        let update = self.store_mut().take_update();
        if update.is_empty() {
            String::new()
        } else {
            serde_json::to_string(&update).unwrap_or_default()
        }
    }

    fn ticker_deltas(&mut self) -> Vec<f64> {
        let mut out = Vec::new();
        self.store_mut().take_ticker_deltas(&mut out);
        out
    }

    fn changed_books(&mut self) -> Vec<String> {
        self.take_changed_books()
    }

    fn book_ops(&mut self, symbol: &str) -> Vec<f64> {
        let mut out = Vec::new();
        self.take_book_ops(symbol, &mut out);
        out
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EngineOptions {
    #[serde(default)]
    testnet: bool,
    #[serde(default)]
    public_data_only: bool,
}

#[derive(Serialize)]
#[serde(tag = "op", rename_all = "camelCase")]
enum ActionOut<'a> {
    WsConnect {
        socket: u32,
        url: &'a str,
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
    Fill {
        order_id: &'a str,
        symbol: &'a str,
        side: safe_cex_engine::model::OrderSide,
        price: f64,
        amount: f64,
    },
    Candle {
        symbol: &'a str,
        interval: &'a str,
        candle: &'a Candle,
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
            Action::Notice(Notice::Fill(fill)) => ActionOut::Fill {
                order_id: &fill.order_id,
                symbol: &fill.symbol,
                side: fill.side,
                price: fill.price,
                amount: fill.amount,
            },
            Action::Notice(Notice::Candle {
                symbol,
                interval,
                candle,
            }) => ActionOut::Candle {
                symbol,
                interval,
                candle,
            },
        })
        .collect();
    serde_json::to_string(&out).unwrap_or_default()
}

/// Commands as JavaScript sends them.
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
enum CommandIn {
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
    #[wasm_bindgen(constructor)]
    pub fn new(venue: &str, options_json: &str) -> Result<WasmEngine, JsError> {
        let options: EngineOptions = serde_json::from_str(options_json)?;
        let inner: Box<dyn AnyEngine> = match venue {
            "binance" => Box::new(Engine::new(BinanceFutures::new(BinanceOptions {
                testnet: options.testnet,
                public_data_only: options.public_data_only,
            }))),
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

    #[wasm_bindgen(js_name = wsClose)]
    pub fn ws_close(&mut self, socket: u32, code: u16, now: f64) -> String {
        let socket = SocketId(socket);
        self.inner.handle(now, Event::WsClosed { socket, code });
        self.flush()
    }

    #[wasm_bindgen(js_name = httpResponse)]
    pub fn http_response(&mut self, request: u32, status: u16, body: &str, now: f64) -> String {
        let request = RequestId(request);
        self.inner.handle(
            now,
            Event::HttpResponse {
                request,
                status,
                body,
            },
        );
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

    /// The Store delta since the previous call as JSON, or "" when unchanged.
    #[wasm_bindgen(js_name = storeUpdate)]
    pub fn store_update(&mut self) -> String {
        self.inner.store_update()
    }

    /// Symbols whose book changed since the previous call, as a JSON array.
    #[wasm_bindgen(js_name = changedBooks)]
    pub fn changed_books(&mut self) -> String {
        let symbols = self.inner.changed_books();
        if symbols.is_empty() {
            String::new()
        } else {
            serde_json::to_string(&symbols).unwrap_or_default()
        }
    }

    /// Packed ticker deltas since the previous call: `[index, mask, ...values]`
    /// repeated, in `PACKED_TICKER_FIELDS` order.
    #[wasm_bindgen(js_name = tickerDeltas)]
    pub fn ticker_deltas(&mut self) -> Vec<f64> {
        self.inner.ticker_deltas()
    }

    /// Book ops for `symbol` since the previous call: `[code, price, amount]`
    /// triples (0 bid, 1 ask, 2 reset, 3 reset bids, 4 reset asks).
    #[wasm_bindgen(js_name = bookOps)]
    pub fn book_ops(&mut self, symbol: &str) -> Vec<f64> {
        self.inner.book_ops(symbol)
    }
}
