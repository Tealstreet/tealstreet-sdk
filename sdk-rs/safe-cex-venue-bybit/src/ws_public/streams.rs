use crate::book_mapping::{BookState, Outcome};
use safe_cex_exchange::{
    OwnedOperation,
    runtime::{ExchangeError, ListenerCleanup},
    websocket::{BaseWebSocket, MessageHandler},
};
use safe_cex_types::OrderBook;
use serde_json::{Map, Value, json};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

pub(crate) type Topics = Rc<RefCell<Map<String, Value>>>;

fn send(socket: &Rc<BaseWebSocket>, op: &str, topic: &str) -> Result<(), ExchangeError> {
    socket.send(&json!({"op":op,"args":[topic]}).to_string())
}

pub(super) fn append(socket: &BaseWebSocket, topic: &str, parser: MessageHandler) {
    let mut handlers = socket.message_handlers.borrow_mut();
    let mut next = handlers
        .get(topic)
        .map(|list| list.borrow().clone())
        .unwrap_or_default();
    next.push(parser);
    handlers.insert(topic.into(), Rc::new(RefCell::new(next)));
}

pub(super) fn remove(socket: &BaseWebSocket, topic: &str, parser: &MessageHandler) -> bool {
    let mut handlers = socket.message_handlers.borrow_mut();
    let next: Vec<_> = handlers
        .get(topic)
        .map(|list| {
            list.borrow()
                .iter()
                .filter(|f| !Rc::ptr_eq(f, parser))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let empty = next.is_empty();
    handlers.insert(topic.into(), Rc::new(RefCell::new(next)));
    empty
}

struct BookSubscription {
    socket: Weak<BaseWebSocket>,
    topics: Topics,
    topic: String,
    parser: MessageHandler,
    retry: RefCell<Option<OwnedOperation>>,
}

impl BookSubscription {
    fn connect(self: &Rc<Self>) -> Result<(), ExchangeError> {
        let Some(socket) = self.socket.upgrade() else {
            return Ok(());
        };
        if socket.is_connected() {
            if !socket.is_disposed() {
                append(&socket, &self.topic, self.parser.clone());
                send(&socket, "subscribe", &self.topic)?;
                self.topics
                    .borrow_mut()
                    .insert(self.topic.clone(), Value::String(self.topic.clone()));
            }
        } else {
            let subscription = self.clone();
            *self.retry.borrow_mut() =
                Some(socket.start_timeout(100, move || subscription.connect())?);
        }
        Ok(())
    }
}

pub(crate) fn listen_order_book(
    socket: Rc<BaseWebSocket>,
    topics: Topics,
    symbol: &str,
    depth: usize,
    callback: Rc<dyn Fn(OrderBook)>,
) -> Result<ListenerCleanup, ExchangeError> {
    let topic = format!("orderbook.{depth}.{symbol}");
    let state = Rc::new(RefCell::new(BookState::new()));
    let parsed = state.clone();
    let weak = Rc::downgrade(&socket);
    let resync_topic = topic.clone();
    let parser: MessageHandler = Rc::new(move |frame| {
        let outcome = parsed
            .borrow_mut()
            .apply(frame)
            .map_err(|message| ExchangeError {
                name: "TypeError".into(),
                ..ExchangeError::new(message)
            })?;
        match outcome {
            Outcome::Publish => {
                let book = parsed.borrow().book.clone();
                callback(book);
            }
            Outcome::Suppress => {}
            Outcome::Resync => {
                if let Some(socket) = weak.upgrade() {
                    send(&socket, "unsubscribe", &resync_topic)?;
                    let target = weak.clone();
                    let topic = resync_topic.clone();
                    let state = parsed.clone();
                    socket.start_timeout(100, move || {
                        if let Some(socket) = target.upgrade()
                            && !socket.is_disposed()
                        {
                            send(&socket, "subscribe", &topic)?;
                        }
                        state.borrow_mut().resyncing = false;
                        Ok(())
                    })?;
                }
            }
        }
        Ok(())
    });
    let subscription = Rc::new(BookSubscription {
        socket: Rc::downgrade(&socket),
        topics,
        topic,
        parser,
        retry: RefCell::new(None),
    });
    subscription.connect()?;
    Ok(Box::new(move || {
        let Some(socket) = subscription.socket.upgrade() else {
            return Ok(());
        };
        let empty = remove(&socket, &subscription.topic, &subscription.parser);
        if empty {
            subscription
                .topics
                .borrow_mut()
                .shift_remove(&subscription.topic);
        }
        socket.cleanup_message_handler_stats(&subscription.topic)?;
        {
            let mut state = state.borrow_mut();
            state.book.asks.clear();
            state.book.bids.clear();
        }
        if let Some(timer) = subscription.retry.borrow_mut().take() {
            timer.cancel();
        }
        if socket.is_connected() && empty {
            send(&socket, "unsubscribe", &subscription.topic)?;
        }
        Ok(())
    }))
}
