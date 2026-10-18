//! Values crossing between js_runtime and the DataValue bodies (ticker_readers,
//! public_data_recovery). Objects keep their entries; identity does not cross.
use super::value::{Js, Thrown};
use crate::ticker_readers::TickerValue as DataValue;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

pub(crate) fn to_data(value: &Js) -> Result<DataValue, Thrown> {
    Ok(match value {
        Js::Undefined => DataValue::Undefined,
        Js::Null => DataValue::Null,
        Js::Bool(b) => DataValue::Bool(*b),
        Js::Number(n) => DataValue::Number(*n),
        Js::String(s) => DataValue::String(s.to_vec()),
        Js::Array(_) => DataValue::Array(Rc::new(RefCell::new(
            value
                .array_items()?
                .unwrap_or_default()
                .iter()
                .map(|item| item.as_ref().map_or(Ok(DataValue::Undefined), to_data))
                .collect::<Result<_, _>>()?,
        ))),
        _ => DataValue::Object(Rc::new(RefCell::new(
            value
                .own_entries()?
                .into_iter()
                .map(|(k, v)| Ok((k.to_vec(), to_data(&v)?)))
                .collect::<Result<BTreeMap<_, _>, Thrown>>()?,
        ))),
    })
}

pub(crate) fn from_data(value: &DataValue) -> Js {
    match value {
        DataValue::Undefined => Js::Undefined,
        DataValue::Null => Js::Null,
        DataValue::Bool(b) => Js::Bool(*b),
        DataValue::Number(n) => Js::Number(*n),
        DataValue::String(s) => Js::utf16(s),
        DataValue::Array(items) => Js::array(items.borrow().iter().map(from_data).collect()),
        DataValue::Object(fields) => {
            let object = Js::object(vec![]);
            for (key, value) in fields.borrow().iter() {
                object
                    .define_key(key, from_data(value))
                    .expect("fresh data object");
            }
            object
        }
    }
}
