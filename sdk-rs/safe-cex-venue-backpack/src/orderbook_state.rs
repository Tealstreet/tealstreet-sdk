//! Public socket order-book bodies over native state and compatible host operations.
use crate::public_control::JsString;
use safe_cex_num::parse_float;
use safe_cex_types::OrderBookOrders;
use std::{cell::RefCell, rc::Rc};

pub type StateRef<U, H> = Rc<RefCell<OrderBookState<U, H>>>;
#[derive(Debug)]
pub struct HandlerSet<H> {
    slots: Vec<Option<Rc<H>>>,
}
impl<H> Default for HandlerSet<H> {
    fn default() -> Self {
        Self { slots: Vec::new() }
    }
}
impl<H> HandlerSet<H> {
    pub fn insert(&mut self, handler: Rc<H>) {
        if !self.iter().any(|h| Rc::ptr_eq(h, &handler)) {
            self.slots.push(Some(handler));
        }
    }
    pub fn remove(&mut self, handler: &Rc<H>) {
        for slot in &mut self.slots {
            if slot.as_ref().is_some_and(|h| Rc::ptr_eq(h, handler)) {
                *slot = None;
            }
        }
    }
    pub fn iter(&self) -> impl Iterator<Item = &Rc<H>> {
        self.slots.iter().filter_map(Option::as_ref)
    }
    pub fn is_empty(&self) -> bool {
        self.iter().next().is_none()
    }
    pub fn slots(&self) -> &[Option<Rc<H>>] {
        &self.slots
    }
}
pub type TerminalHandlers<H> = Rc<RefCell<HandlerSet<H>>>;
#[derive(Debug)]
pub struct OrderBookState<U, H> {
    pub updates: Vec<U>,
    pub is_snapshot_loaded: bool,
    pub is_terminal: bool,
    pub last_update_id: f64,
    pub is_snapshot_fetch_in_flight: bool,
    pub is_recovery_scheduled: bool,
    pub recovery_attempts: f64,
    pub terminal_error_emitted: bool,
    pub terminal_handlers: TerminalHandlers<H>,
}
// ts: BackpackPublicWebSocket#createOrderBookState
pub fn create_order_book_state<U, H>() -> OrderBookState<U, H> {
    OrderBookState {
        updates: Vec::new(),
        is_snapshot_loaded: false,
        is_terminal: false,
        last_update_id: 0.0,
        is_snapshot_fetch_in_flight: false,
        is_recovery_scheduled: false,
        recovery_attempts: 0.0,
        terminal_error_emitted: false,
        terminal_handlers: Rc::new(RefCell::new(HandlerSet::default())),
    }
}
#[derive(Debug, Clone, Copy)]
pub struct SequenceUpdate {
    pub first: Option<f64>,
    pub last: Option<f64>,
}
// ts: BackpackPublicWebSocket#isInitialOrderBookBridge
pub fn is_initial_order_book_bridge<U, H>(
    state: &OrderBookState<U, H>,
    update: SequenceUpdate,
) -> bool {
    let (Some(first), Some(last)) = (update.first, update.last) else {
        return false;
    };
    if last < first {
        return false;
    }
    let expected_update_id = state.last_update_id + 1.0;
    first <= expected_update_id && expected_update_id <= last
}
// ts: BackpackPublicWebSocket#isContinuousOrderBookUpdate
pub fn is_continuous_order_book_update<U, H>(
    state: &OrderBookState<U, H>,
    update: SequenceUpdate,
) -> bool {
    let (Some(first), Some(last)) = (update.first, update.last) else {
        return false;
    };
    last >= first && first == state.last_update_id + 1.0
}

pub type DepthLevels = Rc<RefCell<Vec<[JsString; 2]>>>;
#[derive(Debug, Default)]
pub struct DepthUpdate {
    pub bids: Option<DepthLevels>,
    pub asks: Option<DepthLevels>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BookSide {
    Bids,
    Asks,
}
fn apply_levels<B, E>(
    book: &B,
    levels: Option<DepthLevels>,
    side: BookSide,
    update: &mut impl FnMut(&B, BookSide, OrderBookOrders) -> Result<(), E>,
) -> Result<(), E> {
    if let Some(levels) = levels {
        let mut index = 0;
        while index < levels.borrow().len() {
            let row = levels.borrow()[index].clone();
            update(
                book,
                side,
                OrderBookOrders {
                    price: parse_float(&String::from_utf16_lossy(&row[0])),
                    amount: parse_float(&String::from_utf16_lossy(&row[1])),
                    total: 0.0,
                },
            )?;
            index += 1;
        }
    }
    Ok(())
}
// ts: BackpackPublicWebSocket#applyOrderBookUpdate
pub fn apply_order_book_update<B, E>(
    book: &B,
    data: &Rc<RefCell<DepthUpdate>>,
    mut update: impl FnMut(&B, BookSide, OrderBookOrders) -> Result<(), E>,
) -> Result<(), E> {
    let bids = data.borrow().bids.clone();
    apply_levels(book, bids, BookSide::Bids, &mut update)?;
    let asks = data.borrow().asks.clone();
    apply_levels(book, asks, BookSide::Asks, &mut update)
}

pub trait ResetOrderBooks<U, H, F, E> {
    fn state_keys(&mut self) -> Result<Vec<JsString>, E>;
    fn create_state(&mut self) -> Result<StateRef<U, H>, E>;
    fn terminal_handlers(&mut self, key: &[u16]) -> Result<TerminalHandlers<H>, E>;
    fn replace_state(&mut self, key: &[u16], state: StateRef<U, H>) -> Result<(), E>;
    fn fetcher_keys(&mut self) -> Result<Vec<JsString>, E>;
    fn snapshot_fetcher(&mut self, key: &[u16]) -> Result<Option<F>, E>;
    fn fetch_snapshot(&mut self, fetcher: F) -> Result<(), E>;
}
// ts: BackpackPublicWebSocket#resetAndRefetchOrderBooks
pub fn reset_and_refetch_order_books<U, H, F, E>(
    host: &mut impl ResetOrderBooks<U, H, F, E>,
) -> Result<(), E> {
    for key in host.state_keys()? {
        let next_state = host.create_state()?;
        let handlers = host.terminal_handlers(&key)?;
        next_state.borrow_mut().terminal_handlers = handlers;
        host.replace_state(&key, next_state)?;
    }
    for key in host.fetcher_keys()? {
        if let Some(fetcher) = host.snapshot_fetcher(&key)? {
            host.fetch_snapshot(fetcher)?;
        }
    }
    Ok(())
}

pub trait DisposePublicSocket<T, W, E> {
    fn clear_subscriptions(&mut self) -> Result<(), E>;
    fn clear_order_book_state(&mut self) -> Result<(), E>;
    fn clear_snapshot_fetchers(&mut self) -> Result<(), E>;
    fn throttle_values(&mut self) -> Result<Vec<T>, E>;
    fn cancel_throttle(&mut self, throttle: T) -> Result<(), E>;
    fn clear_depth_throttles(&mut self) -> Result<(), E>;
    fn socket(&mut self) -> Result<Option<W>, E>;
    /// Evaluates the current value's close access/call, including an absent-value property error.
    fn close_socket(&mut self, socket: Option<W>) -> Result<(), E>;
}
// ts: BackpackPublicWebSocket.dispose
pub fn dispose<T, W, E>(host: &mut impl DisposePublicSocket<T, W, E>) -> Result<(), E> {
    host.clear_subscriptions()?;
    host.clear_order_book_state()?;
    host.clear_snapshot_fetchers()?;
    for throttle in host.throttle_values()? {
        host.cancel_throttle(throttle)?;
    }
    host.clear_depth_throttles()?;
    if host.socket()?.is_some() {
        let socket = host.socket()?;
        host.close_socket(socket)?;
    }
    Ok(())
}
