//! Public listener bodies with captured callbacks, handler-map and timer bindings.
use crate::stream_updates::{Lookup, StreamValues};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::TableHit;
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Feed {
    Book,
    Ohlc,
}
pub enum Key<V: ?Sized> {
    Value(Rc<V>),
    Units(Vec<u16>),
}
pub enum Frame<V: ?Sized> {
    Futures {
        event: &'static str,
        feed: &'static str,
        products: Vec<Rc<V>>,
    },
    SpotBook {
        method: &'static str,
        channel: &'static str,
        symbols: Vec<Rc<V>>,
        depth: f64,
    },
    SpotOhlc {
        method: &'static str,
        channel: &'static str,
        symbols: Vec<Rc<V>>,
        interval: TableHit<f64>,
    },
}
pub trait ListenerHost: StreamValues + Sized + 'static {
    type Callback: ?Sized;
    fn string_units(&self, value: &Self::Value) -> Result<Vec<u16>, ExchangeError>;
    fn interval(&self, value: &Self::Value) -> Result<TableHit<f64>, ExchangeError>;
    fn append(
        &self,
        feed: Feed,
        key: &Key<Self::Value>,
        callback: Rc<Self::Callback>,
    ) -> Result<(), ExchangeError>;
    fn filter(
        &self,
        feed: Feed,
        key: &Key<Self::Value>,
        callback: Rc<Self::Callback>,
    ) -> Result<(), ExchangeError>;
    fn clear_idle(&self, feed: Feed) -> Result<(), ExchangeError>;
    fn current_length(
        &self,
        feed: Feed,
        key: &Key<Self::Value>,
    ) -> Result<Option<usize>, ExchangeError>;
    fn connected(&self) -> Result<bool, ExchangeError>;
    fn send(&self, frame: Rc<RefCell<Frame<Self::Value>>>) -> Result<(), ExchangeError>;
    fn send_error(&self, prefix: &'static str, error: ExchangeError) -> Result<(), ExchangeError>;
    fn schedule(&self, poll: Rc<Listener<Self>>, delay: u32) -> Result<(), ExchangeError>;
}
#[derive(Clone, Copy)]
enum Kind {
    FuturesBook,
    SpotBook,
    SpotOhlc,
}
pub struct Listener<H: ListenerHost> {
    host: Rc<H>,
    kind: Kind,
    key: Key<H::Value>,
    symbol: Option<Rc<H::Value>>,
    options: Option<Rc<H::Value>>,
    callback: Rc<H::Callback>,
    interval: Option<TableHit<f64>>,
    frame: Option<Rc<RefCell<Frame<H::Value>>>>,
}
impl<H: ListenerHost> Listener<H> {
    fn feed(&self) -> Feed {
        if matches!(self.kind, Kind::SpotOhlc) {
            Feed::Ohlc
        } else {
            Feed::Book
        }
    }
    fn spot_frame(
        &self,
        method: &'static str,
    ) -> Result<Rc<RefCell<Frame<H::Value>>>, ExchangeError> {
        let frame = if matches!(self.kind, Kind::SpotOhlc) {
            let symbol =
                self.host
                    .field(self.options.as_ref().unwrap(), "symbol", Lookup::Direct)?;
            Frame::SpotOhlc {
                method,
                channel: "ohlc",
                symbols: vec![symbol],
                interval: self.interval.unwrap(),
            }
        } else {
            Frame::SpotBook {
                method,
                channel: "book",
                symbols: vec![self.symbol.as_ref().unwrap().clone()],
                depth: 100.0,
            }
        };
        Ok(Rc::new(RefCell::new(frame)))
    }
    fn send_subscribe(&self) -> Result<(), ExchangeError> {
        if matches!(self.kind, Kind::FuturesBook) {
            if let Err(error) = self.host.send(self.frame.as_ref().unwrap().clone()) {
                self.host
                    .send_error("[Kraken WS] Failed to send book subscription:", error)?;
            }
            Ok(())
        } else {
            self.host.send(self.spot_frame("subscribe")?)
        }
    }
    pub fn poll(self: &Rc<Self>) -> Result<(), ExchangeError> {
        if self.host.connected()? {
            self.send_subscribe()
        } else {
            self.host.schedule(self.clone(), 100)
        }
    }
    pub fn dispose(&self) -> Result<(), ExchangeError> {
        let feed = self.feed();
        self.host.filter(feed, &self.key, self.callback.clone())?;
        self.host.clear_idle(feed)?;
        if !matches!(self.kind, Kind::FuturesBook)
            && self.host.current_length(feed, &self.key)? == Some(0)
            && self.host.connected()?
        {
            self.host.send(self.spot_frame("unsubscribe")?)?;
        }
        Ok(())
    }
}
pub fn futures_listen_book<H: ListenerHost>(
    host: Rc<H>,
    symbol: Rc<H::Value>,
    callback: Rc<H::Callback>,
) -> Result<Rc<Listener<H>>, ExchangeError> {
    let key = Key::Value(symbol.clone());
    host.append(Feed::Book, &key, callback.clone())?;
    let frame = Rc::new(RefCell::new(Frame::Futures {
        event: "subscribe",
        feed: "book",
        products: vec![symbol.clone()],
    }));
    let listener = Rc::new(Listener {
        host,
        kind: Kind::FuturesBook,
        key,
        symbol: Some(symbol),
        options: None,
        callback,
        interval: None,
        frame: Some(frame),
    });
    if listener.host.connected()? {
        listener.send_subscribe()?;
    } else {
        listener.host.schedule(listener.clone(), 100)?;
    }
    Ok(listener)
}
pub fn spot_listen_book<H: ListenerHost>(
    host: Rc<H>,
    symbol: Rc<H::Value>,
    callback: Rc<H::Callback>,
) -> Result<Rc<Listener<H>>, ExchangeError> {
    let key = Key::Value(symbol.clone());
    host.append(Feed::Book, &key, callback.clone())?;
    let listener = Rc::new(Listener {
        host,
        kind: Kind::SpotBook,
        key,
        symbol: Some(symbol),
        options: None,
        callback,
        interval: None,
        frame: None,
    });
    if listener.host.connected()? {
        listener.send_subscribe()?;
    } else {
        listener.host.schedule(listener.clone(), 100)?;
    }
    Ok(listener)
}
pub fn spot_listen_ohlcv<H: ListenerHost>(
    host: Rc<H>,
    options: Rc<H::Value>,
    callback: Rc<H::Callback>,
) -> Result<Rc<Listener<H>>, ExchangeError> {
    let symbol = host.field(&options, "symbol", Lookup::Direct)?;
    let mut key = host.string_units(&symbol)?;
    key.extend([58, 58]);
    let timeframe = host.field(&options, "interval", Lookup::Direct)?;
    key.extend(host.string_units(&timeframe)?);
    let key = Key::Units(key);
    host.append(Feed::Ohlc, &key, callback.clone())?;
    let timeframe = host.field(&options, "interval", Lookup::Direct)?;
    let interval = host.interval(&timeframe)?;
    let listener = Rc::new(Listener {
        host,
        kind: Kind::SpotOhlc,
        key,
        symbol: None,
        options: Some(options),
        callback,
        interval: Some(interval),
        frame: None,
    });
    if listener.host.connected()? {
        listener.send_subscribe()?;
    }
    Ok(listener)
}
