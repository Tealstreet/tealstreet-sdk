//! Shared public frame dispatch. Handler arrays are retained/live across stats,
//! parsing and callback mutation. Captured realm referents retain inherited
//! identity, arity and mutations shared with the core statistics implementation.
use crate::{
    private_event_value::*,
    public_listeners::Parser,
    request_weight::{Failure, type_error},
};
use safe_cex_types::object_map::JsObjectMap;
use std::{cell::RefCell, rc::Rc};

pub type CallbackList = Rc<RefCell<Vec<Parser>>>;
pub type CoreInvoke =
    Rc<dyn Fn(safe_cex_exchange::websocket::MessageHandler, EventValue) -> Result<(), Failure>>;
#[derive(Clone)]
pub enum Handler {
    List(CallbackList),
    CoreList(safe_cex_exchange::websocket::HandlerList, CoreInvoke),
    Value(EventValue),
}
impl Handler {
    fn length(&self, optional: bool) -> Result<EventValue, Failure> {
        match self {
            Self::List(v) => Ok(number(v.borrow().len() as f64)),
            Self::CoreList(v, _) => Ok(number(v.borrow().len() as f64)),
            Self::Value(v) if optional && v.nullish() => Ok(EventValue::Undefined),
            Self::Value(v) => v.read("length"),
        }
    }
    fn run(&self, events: EventValue, expression: &str) -> Result<(), Failure> {
        match self {
            Self::CoreList(list, invoke) => {
                let mut i = 0;
                loop {
                    let next = list.borrow().get(i).cloned();
                    let Some(next) = next else { break };
                    invoke(next, events.clone())?;
                    i += 1;
                }
                Ok(())
            }
            Self::List(list) => {
                let mut i = 0;
                loop {
                    let next = list.borrow().get(i).cloned();
                    let Some(next) = next else { break };
                    next(events.clone())?;
                    i += 1;
                }
                Ok(())
            }
            Self::Value(_) => Err(type_error(format!("{expression} is not iterable"))),
        }
    }
}
pub struct Handlers {
    pub own: RefCell<JsObjectMap<Handler>>,
    /// Enumerable prototype arrays/values. Own keys shadow inherited keys.
    pub prototype: RefCell<JsObjectMap<Handler>>,
    pub realm: Rc<safe_cex_num::PrototypeRealm>,
}
impl Default for Handlers {
    fn default() -> Self {
        Self {
            own: Default::default(),
            prototype: Default::default(),
            realm: safe_cex_num::PrototypeRealm::current(),
        }
    }
}
impl Handlers {
    pub fn get(&self, key: &str) -> Handler {
        self.own
            .borrow()
            .get(key)
            .cloned()
            .or_else(|| self.prototype.borrow().get(key).cloned())
            .unwrap_or_else(|| {
                Handler::Value(
                    EventValue::object_in_realm(self.realm.clone(), Default::default())
                        .get(key)
                        .expect("own object lookup"),
                )
            })
    }
    pub fn keys(&self) -> Vec<String> {
        let own = self.own.borrow();
        let prototype = self.prototype.borrow();
        let mut result: Vec<String> = safe_cex_num::js_own_keys(own.keys().map(String::as_str))
            .into_iter()
            .map(str::to_owned)
            .collect();
        result.extend(
            safe_cex_num::js_own_keys(prototype.keys().map(String::as_str))
                .into_iter()
                .filter(|key| !own.contains_key(*key))
                .map(str::to_owned),
        );
        result.extend(self.realm.enumerable_keys().into_iter().filter(|key| {
            !own.contains_key(key.as_str()) && !prototype.contains_key(key.as_str())
        }));
        result
    }
}
// ts: dispatchBinanceMessage
pub fn dispatch_binance_message(
    handlers: &Handlers,
    data: &str,
    mut record_stats: impl FnMut(&str) -> Result<(), Failure>,
) -> Result<(), Failure> {
    let _binding = handlers.realm.bind();
    let Some(e_idx) = data.find("\"e\":\"") else {
        return Ok(());
    };
    let e_start = e_idx + 5;
    let Some(end) = data[e_start..].find('"') else {
        return Ok(());
    };
    if end == 0 {
        return Ok(());
    }
    let e_end = e_start + end;
    let event = &data[e_start..e_end];
    let broadcast = handlers.get(event);
    let mut exact = Handler::Value(EventValue::Undefined);
    if let Some(index) = data[e_end..].find("\"s\":\"") {
        let start = e_end + index + 5;
        if let Some(end) = data[start..].find('"') {
            if end > 0 {
                exact = handlers.get(&format!("{event}.{}", &data[start..start + end]));
            }
        }
    }
    let mut prefix_calls = vec![];
    if !broadcast.length(true)?.truthy() && !exact.length(true)?.truthy() {
        let prefix = format!("{event}.");
        for key in handlers.keys() {
            if key.len() > prefix.len() && key.starts_with(&prefix) {
                let arr = handlers.get(&key);
                if arr.length(false)?.truthy() {
                    prefix_calls.push(arr);
                }
            }
        }
    }
    if !broadcast.length(true)?.truthy() && !exact.length(true)?.truthy() && prefix_calls.is_empty()
    {
        return Ok(());
    }
    record_stats(event)?;
    let json = crate::private_session::parse_data(string(data));
    if !json.truthy() {
        return Ok(());
    }
    let events = if matches!(json, EventValue::Array(_)) {
        json
    } else {
        EventValue::array(vec![json])
    };
    if broadcast.length(true)?.truthy() {
        broadcast.run(events.clone(), "broadcast")?;
    }
    if exact.length(true)?.truthy() {
        exact.run(events.clone(), "exact")?;
    }
    for arr in prefix_calls {
        arr.run(events.clone(), "handlers")?;
    }
    Ok(())
}
