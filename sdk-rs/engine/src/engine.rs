//! The sans-I/O driver. Hosts feed [`Event`]s in and execute the [`Action`]s
//! that come out; venues never touch sockets, HTTP clients or clocks directly.
use crate::book::Book;
use crate::json::{ArrayAt, ArraySplitter};
use crate::model::{CancelOrderRef, Candle, OrderSide, PlaceOrderOpts};
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
    Patch,
    Delete,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }
}

/// A public request can select a host-owned egress without naming a proxy URL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestRoute {
    Direct,
    PublicCors,
}
impl RequestRoute {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::PublicCors => "publicCors",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HttpRequest {
    pub id: RequestId,
    pub method: Method,
    pub route: Option<RequestRoute>,
    pub timeout_ms: Option<u32>,
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
    /// Definitive credential rejection; transient failures never deactivate.
    Deactivate,
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
pub enum SocketChannel {
    Public,
    Private,
    Business,
}

/// Stable feed identity and protocol recovery; hosts choose transport routes.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct SocketContext {
    pub feed: String,
    pub recover: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    WsConnect {
        socket: SocketId,
        url: String,
        channel: Option<SocketChannel>,
        context: Option<SocketContext>,
    },
    /// Upgrade authentication; hosts must fail closed if headers are unsupported.
    WsConnectWithHeaders {
        socket: SocketId,
        url: String,
        headers: Vec<(&'static str, String)>,
        channel: Option<SocketChannel>,
        context: Option<SocketContext>,
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
    /// Cancel using caller-supplied routing context before Store sees the order.
    CancelOrdersWithSymbols {
        command: CommandId,
        orders: Vec<CancelOrderRef>,
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
    FetchStpMode {
        command: CommandId,
    },
    FetchAccount {
        command: CommandId,
    },
    FetchOrderBook {
        command: CommandId,
        symbol: String,
        limit: Option<u32>,
    },
    SetStpMode {
        command: CommandId,
        mode: crate::model::StpMode,
    },
    /// Refresh account-specific leverage rails and margin mode for a symbol.
    FetchLeverage {
        command: CommandId,
        symbol: String,
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
    /// Resolve an uncertain placement using the venue-native client ID.
    OrderByClientId {
        symbol: String,
        client_order_id: String,
    },
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
    /// streamed body's envelope is kept by its splitter.
    body: Vec<u8>,
}

/// How much of a streamed envelope is quoted to report a response without its array.
const STREAM_PREVIEW_BYTES: usize = 512;

struct Io<P> {
    actions: Vec<Action>,
    pending: HashMap<RequestId, P>,
    streams: HashMap<RequestId, ArrayAt>,
    incoming: HashMap<RequestId, Incoming>,
    timers: HashMap<TimerId, ()>,
    next_id: u32,
    /// Retained symbol buffers; only the active prefix belongs to the next frame.
    changed_books: Vec<String>,
    changed_books_active: usize,
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
            changed_books_active: 0,
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
        self.connect_with_channel(url, None)
    }

    /// Logical feed role lets hosts route identical venue URLs independently.
    pub fn connect_channel(&mut self, url: impl Into<String>, channel: SocketChannel) -> SocketId {
        self.connect_with_channel(url, Some(channel))
    }

    fn connect_with_channel(
        &mut self,
        url: impl Into<String>,
        channel: Option<SocketChannel>,
    ) -> SocketId {
        self.connect_context(url, channel, None)
    }

    pub fn connect_feed(
        &mut self,
        url: impl Into<String>,
        channel: SocketChannel,
        feed: impl Into<String>,
        recover: bool,
    ) -> SocketId {
        self.connect_context(
            url,
            Some(channel),
            Some(SocketContext {
                feed: feed.into(),
                recover,
            }),
        )
    }

    fn connect_context(
        &mut self,
        url: impl Into<String>,
        channel: Option<SocketChannel>,
        context: Option<SocketContext>,
    ) -> SocketId {
        let socket = SocketId(self.io.next());
        self.io.actions.push(Action::WsConnect {
            socket,
            url: url.into(),
            channel,
            context,
        });
        socket
    }

    pub fn connect_with_headers(
        &mut self,
        url: impl Into<String>,
        headers: Vec<(&'static str, String)>,
    ) -> SocketId {
        self.connect_headers_channel(url, headers, None)
    }

    pub fn connect_with_headers_channel(
        &mut self,
        url: impl Into<String>,
        headers: Vec<(&'static str, String)>,
        channel: SocketChannel,
    ) -> SocketId {
        self.connect_headers_channel(url, headers, Some(channel))
    }

    fn connect_headers_channel(
        &mut self,
        url: impl Into<String>,
        headers: Vec<(&'static str, String)>,
        channel: Option<SocketChannel>,
    ) -> SocketId {
        let socket = SocketId(self.io.next());
        self.io.actions.push(Action::WsConnectWithHeaders {
            socket,
            url: url.into(),
            headers,
            channel,
            context: None,
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
            route: None,
            timeout_ms: None,
            url,
            headers,
            body,
        }));
        id
    }

    /// Issue a request whose successful body holds a large array: each element
    /// goes to [`Venue::on_item`] as it arrives, then [`Venue::on_response`]
    /// receives the envelope with its array emptied. Error bodies arrive whole.
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

    /// Stream a POSTed read while retaining its typed request body.
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

    /// Stream the response of an already-issued request, including signed POST reads.
    pub fn stream_response(&mut self, request: RequestId, at: ArrayAt) {
        self.io.streams.insert(request, at);
    }

    /// Set host policy on an outbound action before the host drains it.
    pub fn route_request(&mut self, id: RequestId, route: RequestRoute) {
        if let Some(request) = self
            .io
            .actions
            .iter_mut()
            .rev()
            .find_map(|action| match action {
                Action::Http(request) if request.id == id => Some(request),
                _ => None,
            })
        {
            request.route = Some(route);
        }
    }

    /// Bound the whole host operation, including route setup and response body.
    pub fn request_timeout(&mut self, id: RequestId, timeout_ms: u32) {
        if let Some(request) = self
            .io
            .actions
            .iter_mut()
            .rev()
            .find_map(|action| match action {
                Action::Http(request) if request.id == id => Some(request),
                _ => None,
            })
        {
            request.timeout_ms = Some(timeout_ms.max(1));
        }
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
        let active = self.io.changed_books_active;
        if !self.io.changed_books[..active].iter().any(|s| s == symbol) {
            if let Some(slot) = self.io.changed_books.get_mut(active) {
                slot.clear();
                slot.push_str(symbol);
            } else {
                self.io.changed_books.push(symbol.to_owned());
            }
            self.io.changed_books_active += 1;
        }
    }

    pub fn log(&mut self, level: LogLevel, message: impl Into<String>) {
        self.io.actions.push(Action::Notice(Notice::Log {
            level,
            message: message.into(),
        }));
    }

    pub fn deactivate(&mut self) {
        self.io.actions.push(Action::Notice(Notice::Deactivate));
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
    /// A complete envelope arrived without its expected streamed list. Venues
    /// may recognize business errors; successful missing lists still fail.
    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        response: Response<'_>,
    ) {
        self.on_request_failed(
            cx,
            pending,
            &format!(
                "response did not contain the expected list: {}",
                response.body
            ),
        );
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
    book_deltas: bool,
    book_baselines: HashMap<String, BookBaseline>,
    book_market_rows: Vec<(String, String)>,
}

struct BookBaseline {
    market: u32,
    id: String,
    book: Book,
}

impl<V: Venue> Engine<V> {
    pub fn new(venue: V) -> Self {
        Self {
            venue,
            store: Store::default(),
            io: Io::default(),
            started: false,
            book_deltas: false,
            book_baselines: HashMap::new(),
            book_market_rows: Vec::new(),
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

    /// Networked hosts opt into level deltas. Browser consumers retain full
    /// books by default. Switching modes starts a fresh full-book baseline.
    pub fn set_book_deltas(&mut self, enabled: bool) {
        if self.book_deltas != enabled {
            self.book_deltas = enabled;
            self.book_baselines.clear();
            self.book_market_rows.clear();
            let symbols: Vec<_> = self
                .venue
                .books()
                .map(|(symbol, _)| symbol.to_owned())
                .collect();
            for symbol in symbols {
                self.request_book_resync(&symbol);
            }
        }
    }

    /// Force a full BOOK on the next flush, including when another client
    /// joins an existing upstream subscription.
    pub fn request_book_resync(&mut self, symbol: &str) {
        self.book_baselines.remove(symbol);
        let mut cx = Cx {
            now: 0.0,
            store: &mut self.store,
            io: &mut self.io,
        };
        cx.book_changed(symbol);
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
        match &event {
            Event::Command(Command::SubscribeOrderBook { symbol }) => {
                self.request_book_resync(symbol);
            }
            Event::Command(Command::UnsubscribeOrderBook { symbol }) => {
                self.book_baselines.remove(symbol);
            }
            _ => {}
        }
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
            let outside = splitter.outside();
            if !splitter.finished() {
                let body = splitter.envelope();
                if serde_json::from_str::<serde::de::IgnoredAny>(body).is_ok() {
                    self.venue.on_stream_error(
                        &mut cx,
                        pending,
                        Response {
                            status: incoming.status,
                            headers: &incoming.headers,
                            body,
                        },
                    );
                    return;
                }
                let preview = &outside[..outside.len().min(STREAM_PREVIEW_BYTES)];
                let preview = String::from_utf8_lossy(preview);
                let message = format!("response did not contain the expected list: {preview}");
                self.venue.on_request_failed(&mut cx, pending, &message);
                return;
            }
            // Every selector retains the fully validated envelope, with its array emptied.
            let body = match std::str::from_utf8(outside) {
                Ok(body) => body,
                Err(_) => {
                    self.venue
                        .on_request_failed(&mut cx, pending, "stream envelope is not UTF-8");
                    return;
                }
            };
            let response = Response {
                status: incoming.status,
                headers: &incoming.headers,
                body,
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
    /// afterwards still apply on top: every value in them is absolute. Delta
    /// hosts must flush their existing stream before snapshotting a new client;
    /// this method deliberately leaves the stream's baseline untouched.
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
        let markets_match =
            !self.book_deltas
                || (self.book_market_rows.len() == self.store.markets().len()
                    && self.book_market_rows.iter().zip(self.store.markets()).all(
                        |((symbol, id), market)| symbol == &market.symbol && id == &market.id,
                    ));
        if self.book_deltas {
            self.book_baselines.retain(|symbol, _| {
                self.venue.book(symbol).is_some() && self.store.market_index(symbol).is_some()
            });
            if store && !markets_match {
                self.book_market_rows = self
                    .store
                    .markets()
                    .iter()
                    .map(|market| (market.symbol.clone(), market.id.clone()))
                    .collect();
                let symbols: Vec<_> = self
                    .venue
                    .books()
                    .filter_map(|(symbol, _)| {
                        let market = self.store.market_index(symbol)?;
                        let id = &self.store.markets()[market as usize].id;
                        self.book_baselines
                            .get(symbol)
                            .is_none_or(|baseline| baseline.market != market || &baseline.id != id)
                            .then(|| symbol.to_owned())
                    })
                    .collect();
                for symbol in symbols {
                    self.request_book_resync(&symbol);
                }
            }
        }
        let buffer = std::mem::take(&mut self.io.frame);
        let mut w = FrameWriter::new(buffer, now);
        let pending = store.then(|| self.store.take_pending());
        if let Some(pending) = &pending {
            self.store.write(pending, &mut w);
        }
        // A market-only flush must not publish row indexes ahead of MARKETS.
        let write_books = !self.book_deltas || store || markets_match;
        for symbol in self.io.changed_books[..self.io.changed_books_active]
            .iter()
            .filter(|_| write_books)
        {
            if let (Some(book), Some(market)) =
                (self.venue.book(symbol), self.store.market_index(symbol))
            {
                if self.book_deltas {
                    let id = &self.store.markets()[market as usize].id;
                    match self.book_baselines.get_mut(symbol) {
                        Some(baseline)
                            if baseline.market == market
                                && &baseline.id == id
                                && baseline.book.reset_generation() == book.reset_generation() =>
                        {
                            book.write_delta(&baseline.book, market, &mut w);
                            baseline.book.clone_from(book);
                        }
                        baseline => {
                            book.write(market, &mut w);
                            if let Some(baseline) = baseline {
                                baseline.market = market;
                                baseline.id.clone_from(id);
                                baseline.book.clone_from(book);
                            } else {
                                self.book_baselines.insert(
                                    symbol.clone(),
                                    BookBaseline {
                                        market,
                                        id: id.clone(),
                                        book: book.clone(),
                                    },
                                );
                            }
                        }
                    }
                } else {
                    book.write(market, &mut w);
                }
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
        let mut fills = std::mem::take(&mut self.io.fills);
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
        if write_books {
            self.io.changed_books_active = 0;
        }
        if let Some(pending) = pending {
            self.store.recycle_pending(pending);
        }
        fills.clear();
        self.io.fills = fills;
        drop(candles);
        self.io.candles.clear();
        self.io.frame = frame;
        (!empty).then_some(self.io.frame.as_slice())
    }
}

#[cfg(test)]
mod streaming_tests {
    use super::*;

    struct StreamVenue {
        at: ArrayAt,
        rows: Vec<String>,
        body: Option<String>,
        failure: Option<String>,
        book: Option<Book>,
    }

    impl Venue for StreamVenue {
        type Pending = Vec<String>;
        fn start(&mut self, cx: &mut Cx<'_, Self::Pending>) {
            cx.request_streamed(
                Method::Get,
                "https://example.test/list".into(),
                Vec::new(),
                self.at,
                Vec::new(),
            );
        }
        fn on_open(&mut self, _cx: &mut Cx<'_, Self::Pending>, _socket: SocketId) {}
        fn on_message(&mut self, cx: &mut Cx<'_, Self::Pending>, _socket: SocketId, text: &str) {
            for symbol in text.split(',') {
                cx.book_changed(symbol);
            }
        }
        fn on_close(&mut self, _cx: &mut Cx<'_, Self::Pending>, _socket: SocketId, _code: u16) {}
        fn on_response(
            &mut self,
            _cx: &mut Cx<'_, Self::Pending>,
            pending: Self::Pending,
            response: Response<'_>,
        ) {
            self.rows = pending;
            self.body = Some(response.body.to_owned());
        }
        fn on_item(
            &mut self,
            _cx: &mut Cx<'_, Self::Pending>,
            pending: &mut Self::Pending,
            item: &str,
        ) {
            pending.push(item.to_owned());
        }
        fn on_request_failed(
            &mut self,
            _cx: &mut Cx<'_, Self::Pending>,
            _pending: Self::Pending,
            error: &str,
        ) {
            self.failure = Some(error.to_owned());
        }
        fn on_timer(&mut self, _cx: &mut Cx<'_, Self::Pending>, _timer: TimerId) {}
        fn on_command(&mut self, _cx: &mut Cx<'_, Self::Pending>, _command: Command) {}
        fn book(&self, _symbol: &str) -> Option<&Book> {
            self.book.as_ref()
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            Box::new(self.book.iter().map(|book| ("A", book)))
        }
    }

    fn stream(at: ArrayAt, body: &[u8]) -> StreamVenue {
        let mut engine = Engine::new(StreamVenue {
            at,
            rows: Vec::new(),
            body: None,
            failure: None,
            book: None,
        });
        engine.start(0.0);
        let request = engine
            .drain_actions()
            .find_map(|action| match action {
                Action::Http(request) => Some(request.id),
                _ => None,
            })
            .unwrap();
        engine.handle(
            1.0,
            Event::HttpStart {
                request,
                status: 200,
                headers: "",
            },
        );
        for data in body.chunks(1) {
            engine.handle(1.0, Event::HttpChunk { request, data });
        }
        engine.handle(1.0, Event::HttpEnd { request });
        engine.venue
    }

    fn frame_engine() -> Engine<StreamVenue> {
        Engine::new(StreamVenue {
            at: ArrayAt::Root,
            rows: Vec::new(),
            body: None,
            failure: None,
            book: Some(Book::default()),
        })
    }

    fn market(symbol: &str) -> crate::model::Market {
        use crate::model::{Limits, Market, MinMax, Precision};
        Market {
            id: symbol.into(),
            symbol: symbol.into(),
            pretty_symbol: None,
            base: symbol.into(),
            quote: "USD".into(),
            active: true,
            precision: Precision {
                amount: 1.,
                price: 1.,
            },
            limits: Limits {
                amount: MinMax { min: 1., max: 100. },
                leverage: MinMax { min: 1., max: 10. },
                price: None,
            },
            contract_size: None,
            is_expiring: None,
            is_inverse: None,
            is_spot: None,
            is_perpetual: None,
            is_read_only: None,
            taker_fee: None,
            maker_fee: None,
        }
    }

    fn book_markets(frame: &[u8]) -> Vec<u32> {
        crate::wire::sections(frame)
            .filter(|(kind, _)| *kind == section::BOOK)
            .map(|(_, row)| u32::from_le_bytes(row[..4].try_into().unwrap()))
            .collect()
    }

    fn change_book(
        engine: &mut Engine<StreamVenue>,
        side: crate::book::BookSide,
        price: f64,
        amount: f64,
    ) {
        engine
            .venue
            .book
            .as_mut()
            .unwrap()
            .apply(side, price, amount);
        engine.handle(
            1.,
            Event::WsMessage {
                socket: SocketId(1),
                text: "A",
            },
        );
    }

    fn book_sections(frame: &[u8]) -> Vec<(u16, Vec<u8>)> {
        crate::wire::sections(frame)
            .filter(|(kind, _)| matches!(*kind, section::BOOK | section::BOOK_DELTA))
            .map(|(kind, payload)| (kind, payload.to_vec()))
            .collect()
    }

    #[test]
    fn deltas_coalesce_net_changes_and_snapshots_do_not_consume_the_baseline() {
        use crate::book::BookSide::{Asks, Bids};
        let mut engine = frame_engine();
        engine.set_book_deltas(true);
        engine.store.set_markets(vec![market("A")]);
        change_book(&mut engine, Bids, 100., 1.);
        change_book(&mut engine, Asks, 101., 2.);
        assert_eq!(
            book_sections(engine.take_frame(1.).unwrap())[0].0,
            section::BOOK
        );
        change_book(&mut engine, Bids, 100., 3.);
        change_book(&mut engine, Bids, 99., 4.);
        change_book(&mut engine, Asks, 101., 0.);
        assert_eq!(book_sections(engine.snapshot_frame(2.))[0].0, section::BOOK);
        let sections = book_sections(engine.take_market_frame(2.).unwrap());
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].0, section::BOOK_DELTA);
        let payload = &sections[0].1;
        assert_eq!(
            &payload[..16],
            &[0, 0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]
        );
        let rows: Vec<_> = payload[16..]
            .as_chunks::<8>()
            .0
            .iter()
            .map(|row| f64::from_le_bytes(*row))
            .collect();
        assert_eq!(rows, [100., 3., 99., 4., 101., 0.]);
        change_book(&mut engine, Bids, 100., 7.);
        engine.snapshot_frame(3.);
        change_book(&mut engine, Bids, 100., 3.);
        // Mid-interval snapshots cannot be used as a new stream boundary.
        // The global baseline remains the previously flushed value, 3.
        assert!(engine.take_market_frame(3.).is_none());
        assert!(engine.take_frame(3.).is_none());
    }

    #[test]
    fn deltas_resync_on_book_reset_subscription_and_market_identity_changes() {
        use crate::book::BookSide::Bids;
        let mut engine = frame_engine();
        engine.set_book_deltas(true);
        engine.store.set_markets(vec![market("A")]);
        change_book(&mut engine, Bids, 100., 1.);
        engine.take_frame(0.).unwrap();
        for reset in 0..5 {
            match reset {
                0 => engine.venue.book.as_mut().unwrap().clear(),
                1 => engine
                    .venue
                    .book
                    .as_mut()
                    .unwrap()
                    .load_sorted(Bids, [(100., 1.)]),
                2 => engine
                    .venue
                    .book
                    .as_mut()
                    .unwrap()
                    .load_sorted_owned(Bids, vec![(100., 1.)]),
                3 => engine.venue.book = Some(Book::default()),
                _ => engine.handle(
                    1.,
                    Event::Command(Command::SubscribeOrderBook { symbol: "A".into() }),
                ),
            }
            change_book(&mut engine, Bids, 100., 1.);
            assert_eq!(
                book_sections(engine.take_frame(1.).unwrap())[0].0,
                section::BOOK
            );
        }
        let mut renamed = market("A");
        renamed.id = "new-native-id".into();
        engine.store.set_markets(vec![market("B"), renamed]);
        change_book(&mut engine, Bids, 100., 2.);
        assert!(
            engine.take_market_frame(2.).is_none(),
            "new market indexes wait for MARKETS"
        );
        assert_eq!(book_markets(engine.take_frame(2.).unwrap()), [1]);
        engine.store.set_markets(vec![market("A")]);
        assert_eq!(
            book_markets(engine.take_frame(3.).unwrap()),
            [0],
            "identity changes resync even without a dirty notification"
        );
    }

    #[test]
    fn delta_baselines_are_removed_with_books_and_markets_and_modes_can_switch() {
        use crate::book::BookSide::Bids;
        let mut engine = frame_engine();
        engine.set_book_deltas(true);
        engine.store.set_markets(vec![market("A")]);
        change_book(&mut engine, Bids, 100., 1.);
        engine.take_frame(0.).unwrap();
        assert_eq!(engine.book_baselines.len(), 1);
        engine.handle(
            1.,
            Event::Command(Command::UnsubscribeOrderBook { symbol: "A".into() }),
        );
        assert!(engine.book_baselines.is_empty());
        engine.venue.book = None;
        assert!(engine.take_frame(1.).is_none());
        engine.venue.book = Some(Book::default());
        change_book(&mut engine, Bids, 100., 1.);
        assert_eq!(
            book_sections(engine.take_frame(2.).unwrap())[0].0,
            section::BOOK
        );
        engine.store.set_markets(Vec::new());
        engine.take_frame(3.).unwrap();
        assert!(engine.book_baselines.is_empty());
        engine.store.set_markets(vec![market("A")]);
        assert_eq!(
            book_sections(engine.take_frame(4.).unwrap())[0].0,
            section::BOOK
        );
        engine.set_book_deltas(false);
        change_book(&mut engine, Bids, 100., 2.);
        assert_eq!(
            book_sections(engine.take_frame(5.).unwrap())[0].0,
            section::BOOK
        );
        assert!(engine.book_baselines.is_empty());
        engine.set_book_deltas(true);
        assert_eq!(
            book_sections(engine.take_frame(6.).unwrap())[0].0,
            section::BOOK
        );
        change_book(&mut engine, Bids, 100., 3.);
        assert_eq!(
            book_sections(engine.take_frame(7.).unwrap())[0].0,
            section::BOOK_DELTA
        );
    }

    #[test]
    fn book_flushes_preserve_first_change_order_and_resolve_current_market_identity() {
        let mut engine = frame_engine();
        engine.store.set_markets(vec![market("A"), market("B")]);
        engine.take_frame(0.).unwrap();
        engine.handle(
            1.,
            Event::WsMessage {
                socket: SocketId(1),
                text: "B,A,B",
            },
        );
        engine.snapshot_frame(1.);
        assert_eq!(book_markets(engine.take_market_frame(1.).unwrap()), [1, 0]);
        assert!(engine.take_market_frame(1.).is_none());

        engine.handle(
            2.,
            Event::WsMessage {
                socket: SocketId(1),
                text: "A,A,B",
            },
        );
        engine.store.set_markets(vec![market("B"), market("A")]);
        assert_eq!(book_markets(engine.take_frame(2.).unwrap()), [1, 0]);
        assert!(engine.take_frame(2.).is_none());

        engine.handle(
            3.,
            Event::WsMessage {
                socket: SocketId(1),
                text: "missing",
            },
        );
        assert!(engine.take_market_frame(3.).is_none());
        engine
            .store
            .set_markets(vec![market("missing"), market("A")]);
        assert!(book_markets(engine.take_frame(3.).unwrap()).is_empty());
        engine.handle(
            4.,
            Event::WsMessage {
                socket: SocketId(1),
                text: "missing,A,missing",
            },
        );
        assert_eq!(book_markets(engine.take_market_frame(4.).unwrap()), [0, 1]);
    }

    #[test]
    fn ticker_flushes_do_not_replay_deltas_or_consume_them_during_snapshots() {
        use crate::model::Ticker;
        let mut engine = frame_engine();
        let tickers = || {
            ["A", "B"]
                .into_iter()
                .map(|symbol| Ticker {
                    symbol: symbol.into(),
                    ..Ticker::default()
                })
                .collect()
        };
        engine.store.set_tickers(tickers());
        engine.take_frame(0.).unwrap();
        engine.store.update_ticker("B", |t| t.last = 1.);
        assert!(engine.take_market_frame(1.).is_none());
        engine.snapshot_frame(1.);
        let deltas = |frame: &[u8]| -> Vec<f64> {
            crate::wire::sections(frame)
                .filter(|(kind, _)| *kind == section::TICKER_DELTAS)
                .flat_map(|(_, row)| row.as_chunks::<8>().0)
                .map(|value| f64::from_le_bytes(*value))
                .collect()
        };
        assert_eq!(deltas(engine.take_frame(1.).unwrap()), [1., 4., 1.]);
        assert!(engine.take_frame(1.).is_none());
        engine.store.update_ticker("A", |t| t.bid = 2.);
        engine.store.update_ticker("A", |t| t.bid = 3.);
        assert_eq!(deltas(engine.take_frame(2.).unwrap()), [0., 1., 3.]);

        engine.store.update_ticker("A", |t| t.last = 9.);
        engine.store.set_tickers(vec![Ticker {
            symbol: "B".into(),
            ..Ticker::default()
        }]);
        engine.store.update_ticker("B", |t| t.volume = 3.);
        assert!(deltas(engine.take_frame(3.).unwrap()).is_empty());
        engine.store.update_ticker("B", |t| t.ask = 4.);
        assert_eq!(deltas(engine.take_frame(4.).unwrap()), [0., 2., 4.]);
        assert!(engine.take_frame(4.).is_none());
    }

    #[test]
    fn streamed_completion_preserves_bybit_cursor_and_bingx_alias_envelopes() {
        for (at, body) in [
            (ArrayAt::Root, "[1]"),
            (ArrayAt::Field("symbols"), r#"{"symbols":[1]}"#),
            (
                ArrayAt::Path(&["result", "list"]),
                r#"{"list":[9],"retCode":0,"result":{"list":[1],"nextPageCursor":"next"}}"#,
            ),
            (ArrayAt::EnvelopeField("data"), r#"{"data":[1],"code":0}"#),
            (
                ArrayAt::EnvelopeFieldOrNested("data", "orders"),
                r#"{"data":{"orders":[1]},"code":0}"#,
            ),
            (
                ArrayAt::EnvelopeFieldOrNestedAny("data", &["fill_orders", "fill_history_orders"]),
                r#"{"data":{"fill_orders":[1]},"code":0}"#,
            ),
            (
                ArrayAt::EnvelopeFieldOrNestedAny("data", &["fill_orders", "fill_history_orders"]),
                r#"{"data":{"fill_history_orders":[1]},"code":0}"#,
            ),
        ] {
            let venue = stream(at, body.as_bytes());
            assert_eq!(venue.failure, None, "{at:?}");
            assert_eq!(venue.rows, ["1"]);
            let expected = body.replace("[1]", "[]");
            assert_eq!(venue.body.as_deref(), Some(expected.as_str()));
        }
    }

    #[test]
    fn invalid_utf8_truncated_lists_and_oversized_metadata_discard_staged_rows() {
        let oversized = format!(r#"{{"data":[1]}}{}garbage"#, " ".repeat(1024 * 1024));
        for body in [
            b"{\"data\":[1],\"msg\":\"\xff\"}".as_slice(),
            b"{\"data\":[1,\"\xff\"]}".as_slice(),
            br#"{"data":[1,"#,
            oversized.as_bytes(),
        ] {
            let venue = stream(ArrayAt::EnvelopeField("data"), body);
            assert!(venue.failure.is_some());
            assert!(venue.rows.is_empty());
            assert!(venue.body.is_none());
        }
    }
}

#[cfg(test)]
mod request_route_tests {
    use super::*;
    #[test]
    fn route_hints_mark_only_the_requested_outbound_action() {
        let mut store = Store::default();
        let mut io = Io::<()>::default();
        let mut cx = Cx {
            now: 0.0,
            store: &mut store,
            io: &mut io,
        };
        let normal = cx.request(
            Method::Post,
            "https://venue.test/order".into(),
            vec![],
            None,
            (),
        );
        let direct = cx.request(
            Method::Get,
            "https://venue.test/time".into(),
            vec![],
            None,
            (),
        );
        let cors = cx.request(
            Method::Get,
            "https://venue.test/time".into(),
            vec![],
            None,
            (),
        );
        cx.route_request(direct, RequestRoute::Direct);
        cx.route_request(cors, RequestRoute::PublicCors);
        cx.request_timeout(direct, 5_000);
        cx.request_timeout(cors, 0);
        cx.route_request(RequestId(999), RequestRoute::Direct);
        let requests = io
            .actions
            .iter()
            .filter_map(|a| {
                if let Action::Http(r) = a {
                    Some(r)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            requests.iter().map(|r| (r.id, r.route)).collect::<Vec<_>>(),
            vec![
                (normal, None),
                (direct, Some(RequestRoute::Direct)),
                (cors, Some(RequestRoute::PublicCors))
            ]
        );
        assert!(requests.iter().all(|r| r.headers.is_empty()));
        assert_eq!(
            requests.iter().map(|r| r.timeout_ms).collect::<Vec<_>>(),
            vec![None, Some(5_000), Some(1)]
        );
    }
}
