//! Order-book posts retain binary64 until the host structured-clone sink.
use crate::payload::StoreWorkerOp;
use indexmap::IndexMap;
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::JsJsonValue;
use safe_cex_store::StoreMutationError;
use safe_cex_types::{OrderBook, object_map::JsObjectMap};
use serde::{Serialize, Serializer, ser::SerializeMap};
use serde_json::Value;
use std::{
    cell::{Ref, RefCell, RefMut},
    rc::Rc,
};

#[derive(Debug, Clone, PartialEq)]
pub enum OrderBookPostField {
    Metadata(JsJsonValue),
    Book(Rc<RefCell<OrderBook>>),
}
// Serialization is for explicitly lossy JSON traces, not the postMessage adapter.
impl Serialize for OrderBookPostField {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Metadata(value) => {
                let text =
                    safe_cex_num::js_json_stringify(value).map_err(serde::ser::Error::custom)?;
                match text {
                    Some(text) => serde_json::from_str::<Value>(&text)
                        .map_err(serde::ser::Error::custom)?
                        .serialize(serializer),
                    None => serializer.serialize_none(),
                }
            }
            Self::Book(value) => value.borrow().serialize(serializer),
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct OrderBookPostData(IndexMap<String, OrderBookPostField>);
impl OrderBookPostData {
    fn new(data: &JsObjectMap<JsJsonValue>, book: Rc<RefCell<OrderBook>>) -> Self {
        let mut fields: IndexMap<_, _> = safe_cex_num::js_own_keys(data.keys().map(String::as_str))
            .into_iter()
            .map(|key| {
                (
                    key.to_owned(),
                    OrderBookPostField::Metadata(data[key].clone()),
                )
            })
            .collect();
        fields.insert("orderbook".into(), OrderBookPostField::Book(book));
        Self(fields)
    }
    pub fn fields(&self) -> impl Iterator<Item = (&str, &OrderBookPostField)> {
        self.0.iter().map(|(key, value)| (key.as_str(), value))
    }
    pub fn get(&self, key: &str) -> Option<&OrderBookPostField> {
        self.0.get(key)
    }
    pub fn get_mut(&mut self, key: &str) -> Option<&mut OrderBookPostField> {
        self.0.get_mut(key)
    }
    pub fn book(&self) -> Option<Ref<'_, OrderBook>> {
        match self.0.get("orderbook")? {
            OrderBookPostField::Book(book) => Some(book.borrow()),
            OrderBookPostField::Metadata(_) => None,
        }
    }
    pub fn book_mut(&self) -> Option<RefMut<'_, OrderBook>> {
        match self.0.get("orderbook")? {
            OrderBookPostField::Book(book) => Some(book.borrow_mut()),
            OrderBookPostField::Metadata(_) => None,
        }
    }
}
impl Serialize for OrderBookPostData {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, value) in &self.0 {
            if !matches!(value, OrderBookPostField::Metadata(JsJsonValue::Undefined)) {
                map.serialize_entry(key, value)?;
            }
        }
        map.end()
    }
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OrderBookWorkerPost {
    pub op: StoreWorkerOp,
    pub data: OrderBookPostData,
}
impl OrderBookWorkerPost {
    pub(crate) fn new(data: &JsObjectMap<JsJsonValue>, book: Rc<RefCell<OrderBook>>) -> Self {
        Self {
            op: StoreWorkerOp::ListenOrderBook,
            data: OrderBookPostData::new(data, book),
        }
    }
    /// Snapshot for a receiving worker. Unlike Clone, this detaches the book.
    /// Binary64 values, including NaN, infinities and signed zero, stay native.
    pub fn structured_clone(&self) -> Self {
        Self {
            op: self.op,
            data: OrderBookPostData(
                self.data
                    .0
                    .iter()
                    .map(|(key, value)| {
                        let value = match value {
                            OrderBookPostField::Metadata(value) => {
                                OrderBookPostField::Metadata(value.clone())
                            }
                            OrderBookPostField::Book(book) => OrderBookPostField::Book(Rc::new(
                                RefCell::new(book.borrow().clone()),
                            )),
                        };
                        (key.clone(), value)
                    })
                    .collect(),
            ),
        }
    }
}
type BookPost = Rc<dyn Fn(OrderBookWorkerPost) -> Result<(), ExchangeError>>;
/// The mandatory book sink performs structured clone or binds postMessage.
#[derive(Clone)]
pub struct OrderBookPost {
    pub(crate) book: BookPost,
}
impl OrderBookPost {
    pub fn new(book: BookPost) -> Self {
        Self { book }
    }
}
/// Leaf failures keep their original Store variant and subscriber cause.
#[derive(Debug, Clone, PartialEq)]
pub enum OrderBookUpdateError {
    Post(ExchangeError),
    Store(StoreMutationError),
}
impl From<ExchangeError> for OrderBookUpdateError {
    fn from(error: ExchangeError) -> Self {
        Self::Post(error)
    }
}
impl From<StoreMutationError> for OrderBookUpdateError {
    fn from(error: StoreMutationError) -> Self {
        Self::Store(error)
    }
}
impl OrderBookUpdateError {
    // Only the infallible shared listener adapter projects an error for reporting.
    pub(crate) fn into_exchange_error(self) -> ExchangeError {
        match self {
            Self::Post(error) => error,
            Self::Store(error) => error.into(),
        }
    }
}
impl std::fmt::Display for OrderBookUpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Post(e) => e.fmt(f),
            Self::Store(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for OrderBookUpdateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Post(e) => e,
            Self::Store(e) => e,
        })
    }
}
