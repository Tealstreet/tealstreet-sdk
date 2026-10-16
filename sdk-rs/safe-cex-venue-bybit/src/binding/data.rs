//! Values crossing between js_runtime and the DataValue bodies (ticker_readers,
//! public_data_recovery). Objects keep their entries; identity does not cross.
use super::value::Js;
use crate::ticker_readers::TickerValue as DataValue;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

pub(crate) fn to_data(value: &Js) -> DataValue {
    match value {
        Js::Undefined => DataValue::Undefined,
        Js::Null => DataValue::Null,
        Js::Bool(b) => DataValue::Bool(*b),
        Js::Number(n) => DataValue::Number(*n),
        Js::String(s) => DataValue::String(String::from_utf16_lossy(s)),
        Js::Array(_) => DataValue::Array(Rc::new(RefCell::new(
            value
                .array_items()
                .unwrap_or_default()
                .iter()
                .map(|item| item.as_ref().map_or(DataValue::Undefined, to_data))
                .collect(),
        ))),
        _ => DataValue::Object(Rc::new(RefCell::new(
            value
                .own_entries()
                .unwrap_or_default()
                .into_iter()
                .map(|(k, v)| (String::from_utf16_lossy(&k), to_data(&v)))
                .collect::<BTreeMap<_, _>>(),
        ))),
    }
}

pub(crate) fn from_data(value: &DataValue) -> Js {
    match value {
        DataValue::Undefined => Js::Undefined,
        DataValue::Null => Js::Null,
        DataValue::Bool(b) => Js::Bool(*b),
        DataValue::Number(n) => Js::Number(*n),
        DataValue::String(s) => Js::str(s),
        DataValue::Array(items) => Js::array(items.borrow().iter().map(from_data).collect()),
        DataValue::Object(fields) => Js::object(
            fields
                .borrow()
                .iter()
                .map(|(k, v)| (k.as_str(), from_data(v)))
                .collect(),
        ),
    }
}
