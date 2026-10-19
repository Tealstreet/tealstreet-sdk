//! The sans-I/O driver. Hosts feed [`Event`]s in and execute the [`Action`]s
//! that come out; venues never touch sockets, HTTP clients or clocks directly.
use crate::book::Book;
use crate::model::{Candle, OrderSide, PlaceOrderOpts};
use crate::store::Store;
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
    Fill(Fill),
    Candle {
        symbol: String,
        interval: String,
        candle: Candle,
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
    WsConnect { socket: SocketId, url: String },
    WsSend { socket: SocketId, text: String },
    WsClose { socket: SocketId },
    Http(HttpRequest),
    SetTimer { timer: TimerId, at_ms: f64 },
    ClearTimer { timer: TimerId },
    Notice(Notice),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
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
    CancelSymbolOrders {
        command: CommandId,
        symbol: String,
    },
    SetLeverage {
        command: CommandId,
        symbol: String,
        leverage: f64,
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
    HttpResponse {
        request: RequestId,
        status: u16,
        body: &'a str,
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

struct Io<P> {
    actions: Vec<Action>,
    pending: HashMap<RequestId, P>,
    timers: HashMap<TimerId, ()>,
    next_id: u32,
    changed_books: Vec<String>,
}

impl<P> Default for Io<P> {
    fn default() -> Self {
        Self {
            actions: Vec::new(),
            pending: HashMap::new(),
            timers: HashMap::new(),
            next_id: 1,
            changed_books: Vec::new(),
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

    pub fn candle(&mut self, symbol: &str, interval: &str, candle: Candle) {
        self.io.actions.push(Action::Notice(Notice::Candle {
            symbol: symbol.to_owned(),
            interval: interval.to_owned(),
            candle,
        }));
    }

    pub fn fill(&mut self, fill: Fill) {
        self.io.actions.push(Action::Notice(Notice::Fill(fill)));
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
        status: u16,
        body: &str,
    );
    fn on_request_failed(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        error: &str,
    );
    fn on_timer(&mut self, cx: &mut Cx<'_, Self::Pending>, timer: TimerId);
    fn on_command(&mut self, cx: &mut Cx<'_, Self::Pending>, command: Command);
    fn book(&self, symbol: &str) -> Option<&Book>;
    fn book_mut(&mut self, symbol: &str) -> Option<&mut Book>;
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
            Event::HttpResponse {
                request,
                status,
                body,
            } => {
                if let Some(pending) = cx.io.pending.remove(&request) {
                    self.venue.on_response(&mut cx, pending, status, body);
                }
            }
            Event::HttpFailed { request, error } => {
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

    /// Actions produced since the previous drain, in order.
    pub fn drain_actions(&mut self) -> std::vec::Drain<'_, Action> {
        self.io.actions.drain(..)
    }

    pub fn has_actions(&self) -> bool {
        !self.io.actions.is_empty()
    }

    /// Symbols whose order book changed since the previous call.
    pub fn take_changed_books(&mut self) -> Vec<String> {
        std::mem::take(&mut self.io.changed_books)
    }

    pub fn book(&self, symbol: &str) -> Option<&Book> {
        self.venue.book(symbol)
    }

    /// Append `symbol`'s book ops logged since the previous call to `out`.
    pub fn take_book_ops(&mut self, symbol: &str, out: &mut Vec<f64>) {
        if let Some(book) = self.venue.book_mut(symbol) {
            book.take_ops(out);
        }
    }
}
