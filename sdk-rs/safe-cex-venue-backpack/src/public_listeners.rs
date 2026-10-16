//! Candle and snapshot/delta subscriptions with shared market state.
use crate::{
    orderbook_state::{self, DepthUpdate, SequenceUpdate, StateRef},
    public_control::JsString,
};
use safe_cex_types::{OrderBook, OrderBookOrders};
use std::{cell::RefCell, future::Future, pin::Pin, rc::Rc};
pub type Task<T, E> = Pin<Box<dyn Future<Output = Result<T, E>>>>;
pub type Action<E> = Rc<dyn Fn() -> Result<(), E>>;
pub type Book = Rc<RefCell<OrderBook>>;
pub type BookCallback<E> = Rc<dyn Fn(Book) -> Result<(), E>>;
pub type CandleCallback<E> = Rc<dyn Fn(crate::rest_reads::Candle) -> Result<(), E>>;
#[derive(Clone, Debug)]
pub enum Field {
    Null,
    Undefined,
    Text(JsString),
}
#[derive(Clone, Debug)]
pub struct Kline {
    pub symbol: JsString,
    pub time: JsString,
    pub open: Field,
    pub high: Field,
    pub low: Field,
    pub close: Field,
    pub volume: Field,
}
#[derive(Clone, Debug)]
pub struct Delta {
    pub symbol: JsString,
    pub sequence: SequenceUpdate,
    pub levels: Rc<RefCell<DepthUpdate>>,
}
#[derive(Clone, Debug)]
pub enum Data {
    Kline(Kline),
    Delta(Rc<RefCell<Delta>>),
}
pub struct Handler<E> {
    pub call: Rc<dyn Fn(Data) -> Result<(), E>>,
}
pub struct Terminal<E> {
    pub call: Action<E>,
}
pub type State<E> = StateRef<Rc<RefCell<Delta>>, Terminal<E>>;
pub type Fetcher<E> = Rc<dyn Fn() -> Task<(), E>>;
pub struct ListenerState<E, T> {
    pub handlers: RefCell<Vec<(JsString, Rc<RefCell<Vec<Rc<Handler<E>>>>>)>>,
    pub topics: RefCell<Vec<(JsString, f64)>>,
    pub states: RefCell<Vec<(JsString, State<E>)>>,
    pub fetchers: RefCell<Vec<(JsString, Fetcher<E>)>>,
    pub throttles: RefCell<Vec<(JsString, T)>>,
}
impl<E, T> Default for ListenerState<E, T> {
    fn default() -> Self {
        Self {
            handlers: RefCell::new(vec![]),
            topics: RefCell::new(vec![]),
            states: RefCell::new(vec![]),
            fetchers: RefCell::new(vec![]),
            throttles: RefCell::new(vec![]),
        }
    }
}
#[derive(Clone)]
pub enum LastId {
    Undefined,
    Null,
    Number(f64),
    Text(JsString),
}
pub struct Snapshot {
    pub bids: Vec<[JsString; 2]>,
    pub asks: Vec<[JsString; 2]>,
    pub last: LastId,
}
pub trait Host: 'static {
    type Error: 'static;
    type Throttle: Clone;
    fn state(&self) -> Rc<ListenerState<Self::Error, Self::Throttle>>;
    fn market(&mut self, symbol: &[u16]) -> Result<Option<JsString>, Self::Error>;
    fn connected(&mut self) -> Result<bool, Self::Error>;
    fn disposed(&mut self) -> Result<bool, Self::Error>;
    fn emit(&mut self, message: JsString) -> Result<(), Self::Error>;
    fn error_text(&mut self, error: &Self::Error) -> Result<JsString, Self::Error>;
    fn timeout(&mut self, callback: Action<Self::Error>, delay: f64) -> Result<(), Self::Error>;
    fn subscribe_kline(&mut self, id: JsString, interval: JsString) -> Result<(), Self::Error>;
    fn unsubscribe_kline(&mut self, id: JsString, interval: JsString) -> Result<(), Self::Error>;
    fn subscribe_depth(&mut self, id: JsString) -> Result<(), Self::Error>;
    fn unsubscribe(&mut self, topic: JsString) -> Result<(), Self::Error>;
    fn cleanup_stats(&mut self, topic: JsString) -> Result<(), Self::Error>;
    fn date(&mut self, text: JsString) -> Result<f64, Self::Error>;
    fn snapshot(&mut self, id: JsString) -> Result<Task<Snapshot, Self::Error>, Self::Error>;
    fn spawn(&mut self, task: Task<(), Self::Error>) -> Result<(), Self::Error>;
    fn apply(&mut self, book: Book, data: Rc<RefCell<Delta>>) -> Result<(), Self::Error>;
    fn totals(&mut self, book: Book) -> Result<(), Self::Error>;
    fn recover(&mut self, id: JsString, reason: JsString) -> Result<(), Self::Error>;
    fn terminate(
        &mut self,
        id: JsString,
        state: State<Self::Error>,
        reason: JsString,
    ) -> Result<(), Self::Error>;
    fn cancel_throttle(&mut self, throttle: Self::Throttle) -> Result<(), Self::Error>;
}
fn u(s: &str) -> JsString {
    s.encode_utf16().collect()
}
fn join(parts: &[&[u16]]) -> JsString {
    parts.iter().flat_map(|p| p.iter().copied()).collect()
}
fn get<V: Clone>(map: &RefCell<Vec<(JsString, V)>>, key: &[u16]) -> Option<V> {
    map.borrow()
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
}
fn set<V>(map: &RefCell<Vec<(JsString, V)>>, key: JsString, value: V) {
    let mut m = map.borrow_mut();
    if let Some((_, v)) = m.iter_mut().find(|(k, _)| k == &key) {
        *v = value;
    } else {
        m.push((key, value));
    }
}
fn remove<V>(map: &RefCell<Vec<(JsString, V)>>, key: &[u16]) -> Option<V> {
    let mut m = map.borrow_mut();
    m.iter().position(|(k, _)| k == key).map(|i| m.remove(i).1)
}
fn count<E, T>(state: &ListenerState<E, T>, topic: &[u16], add: bool) {
    let old = get(&state.topics, topic).unwrap_or(f64::NAN);
    let n = if add {
        if old == 0.0 || old.is_nan() {
            1.0
        } else {
            old + 1.0
        }
    } else {
        old - 1.0
    };
    set(&state.topics, topic.to_vec(), n);
}
fn register<H: Host>(h: &mut H, topic: &[u16], parser: Rc<Handler<H::Error>>) {
    let s = h.state();
    let mut handlers = get(&s.handlers, topic)
        .map(|v| v.borrow().clone())
        .unwrap_or_default();
    handlers.push(parser);
    set(&s.handlers, topic.to_vec(), Rc::new(RefCell::new(handlers)));
    count(&s, topic, true);
}
fn unregister<H: Host>(h: &mut H, topic: &[u16], parser: &Rc<Handler<H::Error>>) {
    let s = h.state();
    let handlers = get(&s.handlers, topic)
        .map(|v| v.borrow().clone())
        .unwrap_or_default()
        .into_iter()
        .filter(|p| !Rc::ptr_eq(p, parser))
        .collect::<Vec<_>>();
    set(&s.handlers, topic.to_vec(), Rc::new(RefCell::new(handlers)));
    count(&s, topic, false);
}
fn float(f: &Field) -> f64 {
    match f {
        Field::Text(s) => safe_cex_num::parse_float(&String::from_utf16_lossy(s)),
        Field::Undefined | Field::Null => f64::NAN,
    }
}
fn wait_kline<H: Host>(
    host: Rc<RefCell<H>>,
    id: JsString,
    interval: JsString,
    topic: JsString,
    parser: Rc<Handler<H::Error>>,
) -> Result<(), H::Error> {
    if host.borrow_mut().connected()? {
        let mut h = host.borrow_mut();
        register(&mut *h, &topic, parser);
        h.subscribe_kline(id, interval)
    } else {
        let h = host.clone();
        host.borrow_mut().timeout(
            Rc::new(move || {
                wait_kline(
                    h.clone(),
                    id.clone(),
                    interval.clone(),
                    topic.clone(),
                    parser.clone(),
                )
            }),
            100.0,
        )
    }
}
// ts: BackpackPublicWebSocket.listenOHLCV
pub fn listen_ohlcv<H: Host>(
    host: Rc<RefCell<H>>,
    symbol: JsString,
    interval: JsString,
    callback: CandleCallback<H::Error>,
) -> Result<Action<H::Error>, H::Error> {
    let market = host.borrow_mut().market(&symbol)?;
    let Some(id) = market else {
        host.borrow_mut().emit(join(&[
            &u("Market with symbol "),
            &symbol,
            &u(" not found"),
        ]))?;
        return Ok(Rc::new(|| Ok(())));
    };
    let topic = join(&[&u("kline."), &interval, &u("."), &id]);
    let h = host.clone();
    let market = id.clone();
    let parser = Rc::new(Handler {
        call: Rc::new(move |data| {
            let Data::Kline(data) = data else {
                return Ok(());
            };
            if data.symbol != market
                || [&data.open, &data.high, &data.low, &data.close, &data.volume]
                    .iter()
                    .any(|v| matches!(v, Field::Null))
            {
                return Ok(());
            }
            let timestamp = h.borrow_mut().date(join(&[&data.time, &u("Z")]))?;
            callback(crate::rest_reads::Candle {
                timestamp,
                open: float(&data.open),
                high: float(&data.high),
                low: float(&data.low),
                close: float(&data.close),
                volume: float(&data.volume),
            })
        }),
    });
    wait_kline(
        host.clone(),
        id.clone(),
        interval.clone(),
        topic.clone(),
        parser.clone(),
    )?;
    Ok(Rc::new(move || {
        {
            let mut h = host.borrow_mut();
            unregister(&mut *h, &topic, &parser);
        }
        let connected = host.borrow_mut().connected()?;
        let current = host.borrow().state();
        let empty =
            get(&current.handlers, &topic).is_none_or(|handlers| handlers.borrow().is_empty());
        if connected && empty {
            host.borrow_mut().cleanup_stats(topic.clone())?;
            host.borrow_mut()
                .unsubscribe_kline(id.clone(), interval.clone())?;
        }
        Ok(())
    }))
}
fn same_state<H: Host>(h: &H, id: &[u16], s: &State<H::Error>) -> bool {
    get(&h.state().states, id).is_some_and(|current| Rc::ptr_eq(&current, s))
}
fn last(v: LastId) -> f64 {
    let n = match v {
        LastId::Undefined => f64::NAN,
        LastId::Null => 0.0,
        LastId::Number(n) => n,
        LastId::Text(s) => safe_cex_num::number_from_string(&String::from_utf16_lossy(&s)),
    };
    if n == 0.0 || n.is_nan() { 0.0 } else { n }
}
fn fetch_snapshot<H: Host>(
    host: Rc<RefCell<H>>,
    id: JsString,
    book: Book,
    callback: BookCallback<H::Error>,
) -> Task<(), H::Error> {
    let state = get(&host.borrow().state().states, &id);
    let Some(state) = state else {
        return Box::pin(std::future::ready(Ok(())));
    };
    if state.borrow().is_snapshot_fetch_in_flight || state.borrow().is_terminal {
        return Box::pin(std::future::ready(Ok(())));
    };
    state.borrow_mut().is_snapshot_fetch_in_flight = true;
    let response = host.borrow_mut().snapshot(id.clone());
    let response = match response {
        Ok(response) => response,
        Err(error) => {
            return Box::pin(std::future::ready(finish_snapshot(
                host,
                id,
                state,
                Err(error),
                false,
            )));
        }
    };
    Box::pin(async move {
        let mut recover = false;
        let result = async {
            let snapshot = response.await?;
            if host.borrow_mut().disposed()?
                || !same_state(&*host.borrow(), &id, &state)
                || state.borrow().is_terminal
            {
                return Ok(());
            }
            let side = |levels: Vec<[JsString; 2]>| {
                levels
                    .into_iter()
                    .map(|p| OrderBookOrders {
                        price: safe_cex_num::parse_float(&String::from_utf16_lossy(&p[0])),
                        amount: safe_cex_num::parse_float(&String::from_utf16_lossy(&p[1])),
                        total: 0.0,
                    })
                    .collect::<Vec<_>>()
            };
            book.borrow_mut().bids = side(snapshot.bids);
            book.borrow_mut().asks = side(snapshot.asks);
            let last_id = last(snapshot.last);
            let updates = state
                .borrow()
                .updates
                .iter()
                .filter(|u| u.borrow().sequence.last.is_none_or(|n| n > last_id))
                .cloned()
                .collect::<Vec<_>>();
            {
                let mut s = state.borrow_mut();
                s.updates.clear();
                s.last_update_id = last_id;
                s.is_snapshot_loaded = true;
            }
            let mut initial = true;
            for (i, update) in updates.iter().enumerate() {
                let seq = update.borrow().sequence;
                if seq.last.is_some_and(|n| n <= state.borrow().last_update_id) {
                    continue;
                }
                let valid = if initial {
                    orderbook_state::is_initial_order_book_bridge(&state.borrow(), seq)
                } else {
                    orderbook_state::is_continuous_order_book_update(&state.borrow(), seq)
                };
                if !valid {
                    let mut s = state.borrow_mut();
                    s.is_snapshot_loaded = false;
                    s.updates = updates[i..].to_vec();
                    recover = true;
                    break;
                }
                host.borrow_mut().apply(book.clone(), update.clone())?;
                state.borrow_mut().last_update_id =
                    update.borrow().sequence.last.unwrap_or(f64::NAN);
                initial = false;
            }
            if recover {
                return Ok(());
            }
            state.borrow_mut().recovery_attempts = 0.0;
            host.borrow_mut().totals(book.clone())?;
            callback(book.clone())
        }
        .await;
        finish_snapshot(host, id, state, result, recover)
    })
}
fn finish_snapshot<H: Host>(
    host: Rc<RefCell<H>>,
    id: JsString,
    state: State<H::Error>,
    result: Result<(), H::Error>,
    mut recover: bool,
) -> Result<(), H::Error> {
    let caught = match result {
        Ok(()) => Ok(()),
        Err(error) => {
            let report = (|| {
                let mut h = host.borrow_mut();
                let text = h.error_text(&error)?;
                h.emit(join(&[&u("Error fetching orderbook snapshot: "), &text]))
            })();
            if report.is_ok()
                && same_state(&*host.borrow(), &id, &state)
                && !state.borrow().is_terminal
            {
                recover = true;
            }
            report
        }
    };
    let finally = if same_state(&*host.borrow(), &id, &state) {
        state.borrow_mut().is_snapshot_fetch_in_flight = false;
        if recover {
            host.borrow_mut().recover(
                id,
                u("snapshot did not provide a continuous delta sequence"),
            )
        } else {
            Ok(())
        }
    } else {
        Ok(())
    };
    finally?;
    caught
}
fn wait_depth<H: Host>(
    host: Rc<RefCell<H>>,
    id: JsString,
    topic: JsString,
    parser: Rc<Handler<H::Error>>,
    fetcher: Fetcher<H::Error>,
) -> Result<(), H::Error> {
    if host.borrow_mut().connected()? {
        let s = host.borrow().state();
        {
            let mut h = host.borrow_mut();
            register(&mut *h, &topic, parser);
            h.subscribe_depth(id.clone())?;
        }
        set(&s.fetchers, id, fetcher.clone());
        let task = fetcher();
        host.borrow_mut().spawn(task)
    } else {
        let h = host.clone();
        host.borrow_mut().timeout(
            Rc::new(move || {
                wait_depth(
                    h.clone(),
                    id.clone(),
                    topic.clone(),
                    parser.clone(),
                    fetcher.clone(),
                )
            }),
            100.0,
        )
    }
}
// ts: BackpackPublicWebSocket.listenOrderBook
pub fn listen_order_book<H: Host>(
    host: Rc<RefCell<H>>,
    symbol: JsString,
    callback: BookCallback<H::Error>,
) -> Result<Action<H::Error>, H::Error> {
    let market = host.borrow_mut().market(&symbol)?;
    let Some(id) = market else {
        host.borrow_mut().emit(join(&[
            &u("Market with symbol "),
            &symbol,
            &u(" not found"),
        ]))?;
        return Ok(Rc::new(|| Ok(())));
    };
    let topic = join(&[&u("depth."), &id]);
    let s = host.borrow().state();
    if get(&s.states, &id).is_none() {
        set(
            &s.states,
            id.clone(),
            Rc::new(RefCell::new(orderbook_state::create_order_book_state())),
        );
    }
    let book = Rc::new(RefCell::new(OrderBook {
        bids: vec![],
        asks: vec![],
    }));
    let b = book.clone();
    let cb = callback.clone();
    let terminal = Rc::new(Terminal {
        call: Rc::new(move || {
            b.borrow_mut().bids.clear();
            b.borrow_mut().asks.clear();
            cb(b.clone())
        }),
    });
    get(&s.states, &id)
        .unwrap()
        .borrow()
        .terminal_handlers
        .borrow_mut()
        .insert(terminal.clone());
    let h = host.clone();
    let market = id.clone();
    let b = book.clone();
    let cb = callback.clone();
    let fetcher: Fetcher<H::Error> =
        Rc::new(move || fetch_snapshot(h.clone(), market.clone(), b.clone(), cb.clone()));
    let h = host.clone();
    let market = id.clone();
    let parser = Rc::new(Handler {
        call: Rc::new(move |data| {
            let Data::Delta(data) = data else {
                return Ok(());
            };
            if data.borrow().symbol != market {
                return Ok(());
            }
            let Some(state) = get(&h.borrow().state().states, &market) else {
                return Ok(());
            };
            if state.borrow().is_terminal {
                return Ok(());
            }
            if !state.borrow().is_snapshot_loaded {
                if state.borrow().updates.len() >= 1000 {
                    h.borrow_mut().terminate(
                        market.clone(),
                        state,
                        u("too many deltas buffered while awaiting a snapshot"),
                    )?;
                } else {
                    state.borrow_mut().updates.push(data);
                }
                return Ok(());
            }
            let seq = data.borrow().sequence;
            if seq.last.is_some_and(|n| n <= state.borrow().last_update_id) {
                return Ok(());
            }
            if !orderbook_state::is_continuous_order_book_update(&state.borrow(), seq) {
                {
                    let mut s = state.borrow_mut();
                    s.is_snapshot_loaded = false;
                    s.updates = vec![data];
                }
                let number = |n: Option<f64>| {
                    n.map_or_else(|| "undefined".into(), safe_cex_num::js_number_string)
                };
                let reason = u(&format!(
                    "expected update {}, received {}-{}",
                    safe_cex_num::js_number_string(state.borrow().last_update_id + 1.0),
                    number(seq.first),
                    number(seq.last)
                ));
                return h.borrow_mut().recover(market.clone(), reason);
            }
            h.borrow_mut().apply(book.clone(), data.clone())?;
            state.borrow_mut().last_update_id = data.borrow().sequence.last.unwrap_or(f64::NAN);
            h.borrow_mut().totals(book.clone())?;
            callback(book.clone())
        }),
    });
    wait_depth(
        host.clone(),
        id.clone(),
        topic.clone(),
        parser.clone(),
        fetcher,
    )?;
    Ok(Rc::new(move || {
        {
            let mut h = host.borrow_mut();
            unregister(&mut *h, &topic, &parser);
        }
        let connected = host.borrow_mut().connected()?;
        let current = host.borrow().state();
        let empty =
            get(&current.handlers, &topic).is_none_or(|handlers| handlers.borrow().is_empty());
        if connected && empty {
            host.borrow_mut().cleanup_stats(topic.clone())?;
            host.borrow_mut().unsubscribe(topic.clone())?;
            if let Some(t) = get(&s.throttles, &id) {
                host.borrow_mut().cancel_throttle(t)?;
            }
            remove(&s.throttles, &id);
            if let Some(state) = get(&s.states, &id) {
                state
                    .borrow()
                    .terminal_handlers
                    .borrow_mut()
                    .remove(&terminal);
            }
            remove(&s.states, &id);
            remove(&s.fetchers, &id);
        } else if let Some(state) = get(&s.states, &id) {
            state
                .borrow()
                .terminal_handlers
                .borrow_mut()
                .remove(&terminal);
        }
        Ok(())
    }))
}
