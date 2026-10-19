//! The sans-I/O driver. Hosts feed [`Event`]s in and execute the [`Action`]s
//! that come out; venues never touch sockets, HTTP clients or clocks directly.
use crate::book::Book;
use crate::json::{ArrayAt, ArraySplitter};
use crate::model::{Candle, OrderSide, PlaceOrderOpts};
use crate::store::Store;
use crate::wire::{FrameWriter, section};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SocketId(pub u32);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RequestId(pub u32);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TimerId(pub u32);
/// Correlates a [`Command`] with its [`Notice::CommandDone`].
pub type CommandId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Delete,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HttpRequest {
    pub id: RequestId,
    pub method: Method,
    pub url: String,
    pub headers: Vec<(&'static str, String)>,
    pub body: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Notice {
    Log {
        level: LogLevel,
        message: String,
    },
    /// `Ok` carries the command's JSON result.
    CommandDone {
        command: CommandId,
        result: Result<String, String>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Fill {
    pub order_id: String,
    pub symbol: String,
    pub side: OrderSide,
    pub price: f64,
    pub amount: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    WsConnect {
        socket: SocketId,
        url: String,
    },
    /// Upgrade authentication; hosts must fail closed if headers are unsupported.
    WsConnectWithHeaders {
        socket: SocketId,
        url: String,
        headers: Vec<(&'static str, String)>,
    },
    WsSend {
        socket: SocketId,
        text: String,
    },
    WsClose {
        socket: SocketId,
    },
    Http(HttpRequest),
    SetTimer {
        timer: TimerId,
        at_ms: f64,
    },
    ClearTimer {
        timer: TimerId,
    },
    Notice(Notice),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
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
        command: CommandId,
        orders: Vec<PlaceOrderOpts>,
    },
    CancelOrders {
        command: CommandId,
        ids: Vec<String>,
    },
    /// Change an open order's price or amount.
    AmendOrder {
        command: CommandId,
        id: String,
        price: Option<f64>,
        amount: Option<f64>,
    },
    CancelSymbolOrders {
        command: CommandId,
        symbol: String,
    },
    SetLeverage {
        command: CommandId,
        symbol: String,
        leverage: f64,
    },
    SetMarginType {
        command: CommandId,
        symbol: String,
        isolated: bool,
    },
    SetPositionMode {
        command: CommandId,
        hedged: bool,
    },
    /// A rare request/response read (history, income, trades); the result is
    /// JSON in [`Notice::CommandDone`] and never enters the frame.
    Query {
        command: CommandId,
        query: Query,
    },
    FetchCandles {
        command: CommandId,
        symbol: String,
        interval: String,
        limit: Option<u32>,
        from_ms: Option<f64>,
        to_ms: Option<f64>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Query {
    /// The account's own executions.
    Fills {
        symbol: Option<String>,
        order_id: Option<String>,
        from_ms: Option<f64>,
        to_ms: Option<f64>,
        limit: Option<u32>,
    },
    /// Closed and canceled orders.
    OrderHistory {
        symbol: Option<String>,
        from_ms: Option<f64>,
        to_ms: Option<f64>,
        limit: Option<u32>,
    },
    /// Realized PnL, funding and commission ledger rows.
    Income {
        symbol: Option<String>,
        kind: Option<String>,
        from_ms: Option<f64>,
        to_ms: Option<f64>,
        limit: Option<u32>,
    },
    PublicTrades {
        symbol: String,
        limit: Option<u32>,
    },
}

/// An HTTP response as the host received it.
#[derive(Clone, Copy, Debug)]
pub struct Response<'a> {
    pub status: u16,
    /// Raw `name: value` lines separated by `\n`; names in any case.
    pub headers: &'a str,
    pub body: &'a str,
}

impl<'a> Response<'a> {
    pub fn header(&self, name: &str) -> Option<&'a str> {
        self.headers.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.trim().eq_ignore_ascii_case(name).then(|| value.trim())
        })
    }
}

#[derive(Debug)]
pub enum Event<'a> {
    WsOpened {
        socket: SocketId,
    },
    WsMessage {
        socket: SocketId,
        text: &'a str,
    },
    WsClosed {
        socket: SocketId,
        code: u16,
    },
    /// A complete response.
    HttpResponse {
        request: RequestId,
        response: Response<'a>,
    },
    /// A response delivered in parts: start, any number of chunks, end.
    HttpStart {
        request: RequestId,
        status: u16,
        headers: &'a str,
    },
    HttpChunk {
        request: RequestId,
        data: &'a [u8],
    },
    HttpEnd {
        request: RequestId,
    },
    HttpFailed {
        request: RequestId,
        error: &'a str,
    },
    TimerFired {
        timer: TimerId,
    },
    Command(Command),
}

/// What a venue may do while handling an event.
pub struct Cx<'a, P> {
    pub now: f64,
    pub store: &'a mut Store,
    io: &'a mut Io<P>,
}

/// A response arriving in parts.
struct Incoming {
    status: u16,
    headers: String,
    /// Splits a streamed array out of a successful body.
    splitter: Option<ArraySplitter>,
    /// Everything not streamed: error bodies and unstreamed requests. A
    /// streamed body keeps only its start, to explain a missing array.
    body: Vec<u8>,
}

/// How much of a streamed body is kept to report a response without its array.
const STREAM_PREVIEW_BYTES: usize = 512;

struct Io<P> {
    actions: Vec<Action>,
    pending: HashMap<RequestId, P>,
    streams: HashMap<RequestId, ArrayAt>,
    incoming: HashMap<RequestId, Incoming>,
    timers: HashMap<TimerId, ()>,
    next_id: u32,
    changed_books: Vec<String>,
    candles: Vec<(String, String, Candle)>,
    fills: Vec<Fill>,
    frame: Vec<u8>,
    snapshot: Vec<u8>,
}

impl<P> Default for Io<P> {
    fn default() -> Self {
        Self {
            actions: Vec::new(),
            pending: HashMap::new(),
            streams: HashMap::new(),
            incoming: HashMap::new(),
            timers: HashMap::new(),
            next_id: 1,
            changed_books: Vec::new(),
            candles: Vec::new(),
            fills: Vec::new(),
            frame: Vec::new(),
            snapshot: Vec::new(),
        }
    }
}

impl<P> Io<P> {
    fn next(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        id
    }
}

impl<P> Cx<'_, P> {
    pub fn connect(&mut self, url: impl Into<String>) -> SocketId {
        let socket = SocketId(self.io.next());
        self.io.actions.push(Action::WsConnect {
            socket,
            url: url.into(),
        });
        socket
    }

    pub fn connect_with_headers(
        &mut self,
        url: impl Into<String>,
        headers: Vec<(&'static str, String)>,
    ) -> SocketId {
        let socket = SocketId(self.io.next());
        self.io.actions.push(Action::WsConnectWithHeaders {
            socket,
            url: url.into(),
            headers,
        });
        socket
    }

    pub fn send(&mut self, socket: SocketId, text: impl Into<String>) {
        self.io.actions.push(Action::WsSend {
            socket,
            text: text.into(),
        });
    }

    pub fn close(&mut self, socket: SocketId) {
        self.io.actions.push(Action::WsClose { socket });
    }

    /// Issue a request; `pending` comes back with its response.
    pub fn request(
        &mut self,
        method: Method,
        url: String,
        headers: Vec<(&'static str, String)>,
        body: Option<String>,
        pending: P,
    ) -> RequestId {
        let id = RequestId(self.io.next());
        self.io.pending.insert(id, pending);
        self.io.actions.push(Action::Http(HttpRequest {
            id,
            method,
            url,
            headers,
            body,
        }));
        id
    }

    /// Issue a request whose successful body holds a large array: each element
    /// goes to [`Venue::on_item`] as it arrives, then [`Venue::on_response`]
    /// receives metadata with streamed rows replaced by null. Error bodies
    /// arrive whole.
    pub fn request_streamed(
        &mut self,
        method: Method,
        url: String,
        headers: Vec<(&'static str, String)>,
        at: ArrayAt,
        pending: P,
    ) -> RequestId {
        self.request_streamed_body(method, url, headers, None, at, pending)
    }

    pub fn request_streamed_body(
        &mut self,
        method: Method,
        url: String,
        headers: Vec<(&'static str, String)>,
        body: Option<String>,
        at: ArrayAt,
        pending: P,
    ) -> RequestId {
        let id = self.request(method, url, headers, body, pending);
        self.io.streams.insert(id, at);
        id
    }

    pub fn set_timer(&mut self, delay_ms: f64) -> TimerId {
        let timer = TimerId(self.io.next());
        self.io.timers.insert(timer, ());
        self.io.actions.push(Action::SetTimer {
            timer,
            at_ms: self.now + delay_ms,
        });
        timer
    }

    pub fn clear_timer(&mut self, timer: TimerId) {
        if self.io.timers.remove(&timer).is_some() {
            self.io.actions.push(Action::ClearTimer { timer });
        }
    }

    pub fn book_changed(&mut self, symbol: &str) {
        if !self.io.changed_books.iter().any(|s| s == symbol) {
            self.io.changed_books.push(symbol.to_owned());
        }
    }

    pub fn log(&mut self, level: LogLevel, message: impl Into<String>) {
        self.io.actions.push(Action::Notice(Notice::Log {
            level,
            message: message.into(),
        }));
    }

    pub fn done(&mut self, command: CommandId, result: Result<String, String>) {
        self.io
            .actions
            .push(Action::Notice(Notice::CommandDone { command, result }));
    }

    /// Queue a live candle for the next frame; a newer update of the same
    /// candle replaces the queued one.
    pub fn candle(&mut self, symbol: &str, interval: &str, candle: Candle) {
        let same = self
            .io
            .candles
            .iter_mut()
            .find(|(s, i, c)| s == symbol && i == interval && c.timestamp == candle.timestamp);
        match same {
            Some(queued) => queued.2 = candle,
            None => self
                .io
                .candles
                .push((symbol.to_owned(), interval.to_owned(), candle)),
        }
    }

    /// Queue an execution for the next frame.
    pub fn fill(&mut self, fill: Fill) {
        self.io.fills.push(fill);
    }
}

/// One exchange implementation. All methods are synchronous and must not block.
pub trait Venue {
    /// Per-request continuation: what to do with the response.
    type Pending;

    fn start(&mut self, cx: &mut Cx<'_, Self::Pending>);
    fn on_open(&mut self, cx: &mut Cx<'_, Self::Pending>, socket: SocketId);
    fn on_message(&mut self, cx: &mut Cx<'_, Self::Pending>, socket: SocketId, text: &str);
    fn on_close(&mut self, cx: &mut Cx<'_, Self::Pending>, socket: SocketId, code: u16);
    fn on_response(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        response: Response<'_>,
    );
    /// One element of a streamed response's array (see [`Cx::request_streamed`]).
    fn on_item(
        &mut self,
        _cx: &mut Cx<'_, Self::Pending>,
        _pending: &mut Self::Pending,
        _item: &str,
    ) {
    }
    fn on_request_failed(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        error: &str,
    );
    fn on_timer(&mut self, cx: &mut Cx<'_, Self::Pending>, timer: TimerId);
    fn on_command(&mut self, cx: &mut Cx<'_, Self::Pending>, command: Command);
    fn book(&self, symbol: &str) -> Option<&Book>;
    /// Every synchronized book, for snapshot frames.
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_>;
}

pub struct Engine<V: Venue> {
    venue: V,
    store: Store,
    io: Io<V::Pending>,
    started: bool,
}

impl<V: Venue> Engine<V> {
    pub fn new(venue: V) -> Self {
        Self {
            venue,
            store: Store::default(),
            io: Io::default(),
            started: false,
        }
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn store_mut(&mut self) -> &mut Store {
        &mut self.store
    }

    pub fn venue(&self) -> &V {
        &self.venue
    }

    pub fn start(&mut self, now: f64) {
        if std::mem::replace(&mut self.started, true) {
            return;
        }
        let mut cx = Cx {
            now,
            store: &mut self.store,
            io: &mut self.io,
        };
        self.venue.start(&mut cx);
    }

    pub fn handle(&mut self, now: f64, event: Event<'_>) {
        let mut cx = Cx {
            now,
            store: &mut self.store,
            io: &mut self.io,
        };
        match event {
            Event::WsOpened { socket } => self.venue.on_open(&mut cx, socket),
            Event::WsMessage { socket, text } => self.venue.on_message(&mut cx, socket, text),
            Event::WsClosed { socket, code } => self.venue.on_close(&mut cx, socket, code),
            Event::HttpResponse { request, response } => {
                self.http_start(now, request, response.status, response.headers);
                self.http_chunk(now, request, response.body.as_bytes());
                self.http_end(now, request);
            }
            Event::HttpStart {
                request,
                status,
                headers,
            } => self.http_start(now, request, status, headers),
            Event::HttpChunk { request, data } => self.http_chunk(now, request, data),
            Event::HttpEnd { request } => self.http_end(now, request),
            Event::HttpFailed { request, error } => {
                cx.io.streams.remove(&request);
                cx.io.incoming.remove(&request);
                if let Some(pending) = cx.io.pending.remove(&request) {
                    self.venue.on_request_failed(&mut cx, pending, error);
                }
            }
            Event::TimerFired { timer } => {
                if cx.io.timers.remove(&timer).is_some() {
                    self.venue.on_timer(&mut cx, timer);
                }
            }
            Event::Command(command) => self.venue.on_command(&mut cx, command),
        }
    }

    fn http_start(&mut self, _now: f64, request: RequestId, status: u16, headers: &str) {
        if !self.io.pending.contains_key(&request) {
            return;
        }
        let at = self.io.streams.remove(&request);
        let splitter = at
            .filter(|_| (200..300).contains(&status))
            .map(ArraySplitter::new);
        self.io.incoming.insert(
            request,
            Incoming {
                status,
                headers: headers.to_owned(),
                splitter,
                body: Vec::new(),
            },
        );
    }

    fn http_chunk(&mut self, now: f64, request: RequestId, data: &[u8]) {
        let Some(incoming) = self.io.incoming.get_mut(&request) else {
            return;
        };
        let Some(splitter) = incoming.splitter.as_mut() else {
            incoming.body.extend_from_slice(data);
            return;
        };
        let room = STREAM_PREVIEW_BYTES.saturating_sub(incoming.body.len());
        incoming
            .body
            .extend_from_slice(&data[..room.min(data.len())]);
        // The continuation leaves the map while the venue holds it.
        let Some(mut pending) = self.io.pending.remove(&request) else {
            return;
        };
        let mut splitter = std::mem::replace(splitter, ArraySplitter::new(ArrayAt::Root));
        let mut cx = Cx {
            now,
            store: &mut self.store,
            io: &mut self.io,
        };
        let venue = &mut self.venue;
        splitter.feed(data, |item| venue.on_item(&mut cx, &mut pending, item));
        self.io.pending.insert(request, pending);
        if let Some(incoming) = self.io.incoming.get_mut(&request) {
            incoming.splitter = Some(splitter);
        }
    }

    fn http_end(&mut self, now: f64, request: RequestId) {
        let (Some(incoming), Some(pending)) = (
            self.io.incoming.remove(&request),
            self.io.pending.remove(&request),
        ) else {
            return;
        };
        let mut cx = Cx {
            now,
            store: &mut self.store,
            io: &mut self.io,
        };
        // A 2xx without the expected array (an error envelope, a truncated
        // body) must never read as an authoritative empty list.
        if let Some(splitter) = &incoming.splitter {
            if !splitter.finished() {
                let preview = String::from_utf8_lossy(&incoming.body);
                let message = format!("response did not contain the expected list: {preview}");
                self.venue.on_request_failed(&mut cx, pending, &message);
                return;
            }
            let response = Response {
                status: incoming.status,
                headers: &incoming.headers,
                body: splitter.envelope(),
            };
            self.venue.on_response(&mut cx, pending, response);
            return;
        }
        match std::str::from_utf8(&incoming.body) {
            Ok(body) => self.venue.on_response(
                &mut cx,
                pending,
                Response {
                    status: incoming.status,
                    headers: &incoming.headers,
                    body,
                },
            ),
            Err(_) => self
                .venue
                .on_request_failed(&mut cx, pending, "response body is not UTF-8"),
        }
    }

    /// Actions produced since the previous drain, in order.
    pub fn drain_actions(&mut self) -> std::vec::Drain<'_, Action> {
        self.io.actions.drain(..)
    }

    pub fn has_actions(&self) -> bool {
        !self.io.actions.is_empty()
    }

    /// The venue's synchronized book for `symbol`, if any.
    pub fn book(&self, symbol: &str) -> Option<&Book> {
        self.venue.book(symbol)
    }

    /// The whole current state as one frame, for a consumer that starts late
    /// or skipped frames (a new tab, a throttled hidden tab). Frames taken
    /// afterwards still apply on top: every value in them is absolute.
    pub fn snapshot_frame(&mut self, now: f64) -> &[u8] {
        let buffer = std::mem::take(&mut self.io.snapshot);
        let mut w = FrameWriter::new(buffer, now);
        let everything = self.store.everything();
        self.store.write(&everything, &mut w);
        for (symbol, book) in self.venue.books() {
            if let Some(market) = self.store.market_index(symbol) {
                book.write(market, &mut w);
            }
        }
        self.io.snapshot = w.finish();
        &self.io.snapshot
    }

    /// Write everything that changed since the previous call as one flush
    /// frame. Returns `None` when nothing changed.
    pub fn take_frame(&mut self, now: f64) -> Option<&[u8]> {
        self.write_frame(now, true)
    }

    /// Like [`take_frame`](Self::take_frame) but only books, candles and
    /// fills, which consumers want at the venue's book cadence; store
    /// sections wait for the next full frame.
    pub fn take_market_frame(&mut self, now: f64) -> Option<&[u8]> {
        self.write_frame(now, false)
    }

    fn write_frame(&mut self, now: f64, store: bool) -> Option<&[u8]> {
        let buffer = std::mem::take(&mut self.io.frame);
        let mut w = FrameWriter::new(buffer, now);
        let pending = store.then(|| self.store.take_pending());
        if let Some(pending) = &pending {
            self.store.write(pending, &mut w);
        }
        let books = std::mem::take(&mut self.io.changed_books);
        for symbol in &books {
            if let (Some(book), Some(market)) =
                (self.venue.book(symbol), self.store.market_index(symbol))
            {
                book.write(market, &mut w);
            }
        }
        let candles: Vec<_> = self
            .io
            .candles
            .iter()
            .filter_map(|(symbol, interval, candle)| {
                Some((self.store.market_index(symbol)?, interval.as_str(), *candle))
            })
            .collect();
        w.candles(&candles);
        let fills = std::mem::take(&mut self.io.fills);
        if !fills.is_empty() {
            w.begin(section::FILLS);
            w.u32(fills.len() as u32);
            let orders: Vec<u32> = fills.iter().map(|f| w.string(&f.order_id)).collect();
            w.u32s(orders);
            w.u32s(fills.iter().map(|f| {
                self.store
                    .market_index(&f.symbol)
                    .unwrap_or(crate::wire::NONE)
            }));
            w.u32s(fills.iter().map(|f| f.side as u32));
            w.pad();
            for f in &fills {
                w.f64s([f.price, f.amount]);
            }
            w.end();
        }
        let empty = w.is_empty();
        let frame = w.finish();
        drop(candles);
        self.io.candles.clear();
        self.io.frame = frame;
        (!empty).then_some(self.io.frame.as_slice())
    }
}
