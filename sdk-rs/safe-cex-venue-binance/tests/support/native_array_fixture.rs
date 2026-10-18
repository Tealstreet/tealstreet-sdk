//! Terminal fixture construction/observation over the canonical native array.
use safe_cex_venue_binance::private_event_value::{EventArray, EventValue};
pub fn owner(items: Vec<Option<EventValue>>) -> EventArray {
    let EventValue::Array(owner) = EventValue::holey_array(items) else {
        unreachable!()
    };
    owner
}
pub fn slots(owner: &EventArray) -> Vec<Option<EventValue>> {
    EventValue::Array(owner.clone())
        .array_items()
        .expect("fixture array Get")
        .expect("array")
}
pub fn replace(owner: &EventArray, items: Vec<Option<EventValue>>) {
    let array = EventValue::Array(owner.clone());
    array.set("length", EventValue::Number(0.0)).unwrap();
    array
        .set("length", EventValue::Number(items.len() as f64))
        .unwrap();
    for (index, value) in items.into_iter().enumerate() {
        if let Some(value) = value {
            array.define(&index.to_string(), value).unwrap();
        }
    }
}
