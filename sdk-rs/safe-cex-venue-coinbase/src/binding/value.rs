use super::error::failure;
use crate::{
    portfolios_balances::PortfolioFailure,
    starbase_public_frames::CoinbaseStarbaseFrameValue as Frame,
};
use safe_cex_exchange::{js_runtime::value::Js, runtime::ExchangeError};
use std::{cell::RefCell, rc::Rc};

pub(super) fn text(value: &str) -> Frame {
    Frame::String(value.encode_utf16().collect())
}

pub(super) fn to_js(value: &Frame) -> Js {
    match value {
        Frame::Undefined => Js::Undefined,
        Frame::Null => Js::Null,
        Frame::Bool(value) => Js::Bool(*value),
        Frame::Number(value) => Js::Number(*value),
        Frame::String(value) => Js::utf16(value),
        Frame::Array(values) => Js::array(values.borrow().iter().map(to_js).collect()),
        Frame::Object(values) => {
            let result = Js::object(vec![]);
            for key in value.own_keys() {
                result
                    .define_key(&key, to_js(&values.borrow()[&key]))
                    .expect("fresh object data property");
            }
            result
        }
    }
}

pub(super) fn from_js(value: &Js) -> Result<Option<Frame>, PortfolioFailure> {
    Ok(Some(match value {
        Js::Undefined => return Ok(None),
        Js::Null => Frame::Null,
        Js::Bool(value) => Frame::Bool(*value),
        Js::Number(value) => Frame::Number(*value),
        Js::String(value) => Frame::String(value.to_vec()),
        Js::Array(values) => {
            let values = values.borrow().clone();
            let mut captured = Vec::with_capacity(values.len());
            for value in values {
                let value = value
                    .and_then(|value| from_js(&value).transpose())
                    .transpose()?
                    .unwrap_or(Frame::Undefined);
                captured.push(value);
            }
            Frame::Array(Rc::new(RefCell::new(captured)))
        }
        Js::Object(_) => {
            let mut captured = indexmap::IndexMap::new();
            for key in value.own_keys() {
                let field = from_js(&value.get_key(&key).map_err(super::error::from_thrown)?)?
                    .unwrap_or(Frame::Undefined);
                captured.insert(key.to_vec(), field);
            }
            Frame::Object(Rc::new(RefCell::new(captured)))
        }
        _ => {
            return Err(failure(ExchangeError::new(
                "Non-JSON value at the Coinbase native value boundary",
            )));
        }
    }))
}

pub(super) fn same(left: &Frame, right: &Frame) -> bool {
    match (left, right) {
        (Frame::Object(left), Frame::Object(right)) => Rc::ptr_eq(left, right),
        (Frame::Array(left), Frame::Array(right)) => Rc::ptr_eq(left, right),
        _ => false,
    }
}

pub(super) fn query(value: &Frame) -> Result<safe_cex_types::QueryValue, PortfolioFailure> {
    use safe_cex_types::{QueryValue as Q, object_map::JsObjectMap};
    Ok(match value {
        Frame::Undefined => Q::Undefined,
        Frame::Null => Q::Null,
        Frame::Bool(value) => Q::Bool(*value),
        Frame::Number(value) => Q::Number(*value),
        Frame::String(value) => Q::String(String::from_utf16(value).map_err(|_| {
            failure(ExchangeError::new(
                "Lone UTF-16 surrogate at the native Coinbase query boundary",
            ))
        })?),
        Frame::Array(values) => Q::Array(
            values
                .borrow()
                .iter()
                .map(query)
                .collect::<Result<_, _>>()?,
        ),
        Frame::Object(values) => {
            let mut fields = JsObjectMap::default();
            for key in value.own_keys() {
                fields.insert(
                    String::from_utf16(&key)
                        .map_err(|_| failure(ExchangeError::new("Lone UTF-16 query key")))?,
                    query(&values.borrow()[&key])?,
                );
            }
            Q::Object(fields)
        }
    })
}
