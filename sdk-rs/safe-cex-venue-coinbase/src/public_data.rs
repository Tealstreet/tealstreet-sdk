//! Object-only view of Coinbase's Data record. Parsed object roots and the
//! native candle roots supplied by the public dispatcher stay shared.
use crate::starbase_public_frames::CoinbaseStarbaseFrameValue as Frame;
use indexmap::IndexMap;
use safe_cex_types::{Candle, OrderSide};
use std::{cell::RefCell, rc::Rc};

#[derive(Debug, Clone)]
pub enum PublicData {
    Json(Rc<RefCell<IndexMap<Vec<u16>, Frame>>>),
    Candle(Rc<RefCell<Candle>>),
    /// None is an own undefined property, distinct from an absent map entry.
    Generated(Rc<RefCell<IndexMap<Vec<u16>, Option<Frame>>>>),
}
impl PublicData {
    pub(crate) fn get(&self, name: &str) -> Option<Frame> {
        let key = name.encode_utf16().collect::<Vec<_>>();
        match self {
            Self::Json(v) => v.borrow().get(&key).cloned(),
            Self::Generated(v) => v.borrow().get(&key).cloned().flatten(),
            Self::Candle(v) => {
                let v = v.borrow();
                let number = match name {
                    "timestamp" => v.timestamp,
                    "open" => v.open,
                    "high" => v.high,
                    "low" => v.low,
                    "close" => v.close,
                    "volume" => v.volume,
                    "lastSide" => {
                        return v.last_side.map(|side| {
                            Frame::String(
                                match side {
                                    OrderSide::Buy => "buy",
                                    OrderSide::Sell => "sell",
                                }
                                .encode_utf16()
                                .collect(),
                            )
                        });
                    }
                    _ => return None,
                };
                Some(Frame::Number(number))
            }
        }
    }
    pub(crate) fn fields(&self) -> IndexMap<Vec<u16>, Option<Frame>> {
        match self {
            Self::Json(v) => v
                .borrow()
                .iter()
                .map(|(k, v)| (k.clone(), Some(v.clone())))
                .collect(),
            Self::Generated(v) => v.borrow().clone(),
            Self::Candle(_) => [
                "timestamp",
                "open",
                "high",
                "low",
                "close",
                "volume",
                "lastSide",
            ]
            .into_iter()
            .filter_map(|key| {
                self.get(key)
                    .map(|v| (key.encode_utf16().collect(), Some(v)))
            })
            .collect(),
        }
    }
}

// Inert JSON has no callable own valueOf/toString methods. Arrays use the
// inherited join method; an own non-callable toString shadows the object method.
pub(crate) fn primitive(
    value: Option<Frame>,
) -> Result<Option<Frame>, safe_cex_num::JsCoercionError> {
    Ok(match value {
        Some(Frame::Object(v)) => {
            if v.borrow()
                .contains_key(&"toString".encode_utf16().collect::<Vec<_>>())
            {
                return Err(safe_cex_num::JsCoercionError {
                    name: "TypeError",
                    message: "Cannot convert object to primitive value",
                });
            }
            Some(Frame::String("[object Object]".encode_utf16().collect()))
        }
        Some(Frame::Array(v)) => {
            let mut text = Vec::new();
            for (i, value) in v.borrow().iter().enumerate() {
                if i > 0 {
                    text.push(44);
                }
                if !matches!(value, Frame::Null | Frame::Undefined) {
                    text.extend(string(Some(value.clone()))?);
                }
            }
            Some(Frame::String(text))
        }
        value => value,
    })
}
pub(crate) fn string(value: Option<Frame>) -> Result<Vec<u16>, safe_cex_num::JsCoercionError> {
    Ok(match primitive(value)? {
        None | Some(Frame::Undefined) => "undefined".encode_utf16().collect(),
        Some(Frame::Null) => "null".encode_utf16().collect(),
        Some(Frame::String(v)) => v,
        Some(Frame::Number(v)) => safe_cex_num::js_number_string(v).encode_utf16().collect(),
        Some(Frame::Bool(v)) => v.to_string().encode_utf16().collect(),
        _ => unreachable!("primitive conversion"),
    })
}
pub(crate) fn number(value: Option<Frame>) -> Result<f64, safe_cex_num::JsCoercionError> {
    Ok(match primitive(value)? {
        None | Some(Frame::Undefined) => f64::NAN,
        Some(Frame::Null) => 0.0,
        Some(Frame::Number(v)) => v,
        Some(Frame::Bool(v)) => {
            if v {
                1.0
            } else {
                0.0
            }
        }
        Some(Frame::String(v)) => safe_cex_num::number_from_string(&String::from_utf16_lossy(&v)),
        _ => unreachable!("primitive conversion"),
    })
}
