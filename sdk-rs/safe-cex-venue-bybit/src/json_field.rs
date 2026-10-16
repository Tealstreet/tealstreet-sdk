//! Project shared lookup results into the JSON-only venue response boundary.
use safe_cex_num::{
    TableHit,
    get_key::{GetKeyObject, GetKeyValue, v},
};
use serde_json::Value;

pub(crate) fn field<'a>(row: &'a Value, key: &str) -> Result<Option<&'a Value>, &'static str> {
    match v(row.as_object().map(GetKeyObject::Json), key) {
        TableHit::Mapped(GetKeyValue::Json(value)) => Ok(Some(value)),
        TableHit::Mapped(GetKeyValue::Undefined) | TableHit::Missing => Ok(None),
        // Current callers use fixed source field names, never prototype names.
        // Refuse explicitly if a future caller needs a non-JSON inherited value.
        TableHit::Inherited(_) => Err("Bybit JSON response field cannot carry an inherited member"),
    }
}
