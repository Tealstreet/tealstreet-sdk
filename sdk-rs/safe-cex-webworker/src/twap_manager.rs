use crate::task_types::{TwapOptions, TwapState, TwapStatus};
use indexmap::IndexMap;
use serde::{Serialize, ser::SerializeStruct};
use std::{cell::RefCell, rc::Rc};

/// The manager consumes these live fields; the TWAP placement/timer lifecycle is separate.
#[derive(Debug)]
pub struct ManagedTwap {
    pub id: String,
    pub options: Rc<RefCell<TwapOptions>>,
    pub state: Rc<RefCell<TwapState>>,
}

#[derive(Debug, Clone)]
pub struct SerializedTwap {
    pub options: Rc<RefCell<TwapOptions>>,
    pub state: Rc<RefCell<TwapState>>,
}
impl Serialize for SerializedTwap {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut record = serializer.serialize_struct("SerializedTwap", 2)?;
        record.serialize_field("options", &*self.options.borrow())?;
        record.serialize_field("state", &*self.state.borrow())?;
        record.end()
    }
}

// ts: TwapManager. IndexMap preserves Map insertion order, including replacement.
#[derive(Debug, Default)]
pub struct TwapManager {
    pub running_twaps: RefCell<IndexMap<String, Rc<RefCell<ManagedTwap>>>>,
}
impl TwapManager {
    pub fn has_id(&self, id: &str) -> bool {
        self.running_twaps.borrow().contains_key(id)
    }

    pub fn add(&self, instance: Rc<RefCell<ManagedTwap>>) -> String {
        let id = instance.borrow().id.clone();
        self.running_twaps.borrow_mut().insert(id.clone(), instance);
        id
    }

    pub fn remove(&self, id: &str) {
        self.running_twaps.borrow_mut().shift_remove(id);
    }

    pub fn get_by_id(&self, id: &str) -> Option<Rc<RefCell<ManagedTwap>>> {
        self.running_twaps.borrow().get(id).cloned()
    }

    pub fn serialize_state(&self) -> IndexMap<String, SerializedTwap> {
        let mut result = IndexMap::new();
        for instance in self.running_twaps.borrow().values() {
            let instance = instance.borrow();
            if instance.state.borrow().status != TwapStatus::Completed {
                // ts-bug: assigning __proto__ changes the source result's prototype,
                // not an own entry. The typed result omits that prototype (manifest CHOSEN).
                if instance.id == "__proto__" {
                    continue;
                }
                result.insert(
                    instance.id.clone(),
                    SerializedTwap {
                        options: instance.options.clone(),
                        state: instance.state.clone(),
                    },
                );
            }
        }
        safe_cex_num::js_own_keys(result.keys().map(String::as_str))
            .into_iter()
            .map(|key| (key.to_owned(), result[key].clone()))
            .collect()
    }
}

// ts: twapManager. One shared manager in each host thread; hosts may also own isolated managers.
pub fn twap_manager() -> Rc<TwapManager> {
    thread_local! {
        static TWAP_MANAGER: Rc<TwapManager> = Rc::new(TwapManager::default());
    }
    TWAP_MANAGER.with(Rc::clone)
}
