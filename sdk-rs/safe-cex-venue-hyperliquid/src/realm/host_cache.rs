//! The cross-context cache class (hyperliquid.cross-cache.ts) over the realm.
//!
//! CHOSEN: one module per realm, so the `getCrossCache` singleton is shared by
//! that exchange's own code but not across exchange instances. Native hosts
//! have no BroadcastChannel or sessionStorage, so the cache is memory-only,
//! which is the source's own behavior in a worker without those globals.
use super::{
    Realm,
    host_core::js,
    value::{Kind, PropKey, Value},
};
use crate::{
    cache_runtime::{self, CacheModule, CacheRuntimeHost, Callback},
    spot_json::JsString,
};
use std::{cell::RefCell, rc::Rc};

type ModuleSlot = Rc<RefCell<Option<Rc<CacheModule<Value>>>>>;

impl CacheRuntimeHost for Realm {
    fn number(&self, value: f64) -> Value {
        Value::Number(value)
    }
    fn boolean(&self, value: bool) -> Value {
        Value::Bool(value)
    }
    fn equal(&self, a: &Value, b: &Value) -> bool {
        a.strict_equal(b)
    }
    fn subtract(&mut self, a: &Value, b: &Value) -> Result<Value, Value> {
        Realm::binary(self, "-", a, b)
    }
    fn less(&mut self, a: &Value, b: &Value) -> Result<bool, Value> {
        Ok(Realm::binary(self, "<", a, b)?.truthy())
    }
    fn less_equal(&mut self, a: &Value, b: &Value) -> Result<bool, Value> {
        Ok(Realm::binary(self, "<=", a, b)?.truthy())
    }
    fn abstract_string(&mut self, value: &Value) -> Result<JsString, Value> {
        Ok(js(Realm::to_string(self, value)?))
    }
    fn array(&mut self, values: &[Value]) -> Result<Value, Value> {
        Ok(self.new_array(values.to_vec()))
    }
    fn class_definition(&mut self) -> Result<(Value, Value), Value> {
        let slot = self.module_slot();
        let base = Value::Null;
        self.derived_class(
            "HyperliquidCrossCache",
            0,
            &base,
            Rc::new(move |r, _, _, _| {
                let module = slot
                    .borrow()
                    .clone()
                    .ok_or_else(|| r.error("cross cache module not initialized"))?;
                Ok(cache_runtime::new_cache(r, &module)?.value)
            }),
        )
    }
    fn define_method(
        &mut self,
        prototype: &Value,
        name: &JsString,
        function: &Value,
    ) -> Result<(), Value> {
        self.define_hidden(
            prototype,
            PropKey::String(name.0.as_slice().into()),
            function.clone(),
        );
        Ok(())
    }
    fn instance(&mut self, prototype: &Value) -> Result<Value, Value> {
        Ok(self.alloc(Kind::Ordinary, prototype.object()))
    }
    fn new_map(&mut self) -> Result<Value, Value> {
        Ok(Realm::new_map(self))
    }
    fn new_set(&mut self) -> Result<Value, Value> {
        Ok(Realm::new_set(self))
    }
    fn function(&mut self, capture: Value, body: Callback) -> Result<Value, Value> {
        Ok(self.native("", 0, move |r, this, args| {
            cache_runtime::run_callback(r, body, &capture, this, args)
        }))
    }
    fn broadcast_constructor(&mut self) -> Result<Value, Value> {
        let globals = Value::Object(self.state.globals.clone());
        let key = PropKey::str("BroadcastChannel");
        if self.has_property(&globals, &key)? {
            self.get(&globals, &key)
        } else {
            Ok(Value::Undefined)
        }
    }
    fn construct_broadcast(&mut self, constructor: &Value, name: &Value) -> Result<Value, Value> {
        Realm::construct(self, constructor, std::slice::from_ref(name), None)
    }
    fn destructure(&mut self, value: &Value, keys: &[&str]) -> Result<Vec<Value>, Value> {
        self.object_pattern(value, keys)
    }
}

impl Realm {
    fn module_slot(&self) -> ModuleSlot {
        let holder = self.singleton("crossCacheModuleSlot", |r| {
            r.host_object(Rc::new(ModuleSlot::default()))
        });
        holder
            .host::<ModuleSlot>()
            .map(|s| (*s).clone())
            .expect("module slot host object")
    }
}

fn cache_module(realm: &mut Realm) -> Result<Rc<CacheModule<Value>>, Value> {
    let slot = realm.module_slot();
    let existing = slot.borrow().clone();
    match existing {
        Some(module) => Ok(module),
        None => {
            let module = Rc::new(cache_runtime::new_module(realm)?);
            *slot.borrow_mut() = Some(module.clone());
            Ok(module)
        }
    }
}

pub(crate) fn bind(realm: &Realm) {
    realm.bind_import(
        "getCrossCache",
        realm.native("getCrossCache", 0, |r, _, _| {
            let module = cache_module(r)?;
            cache_runtime::get_cross_cache(r, &module)
        }),
    );
    realm.bind_import(
        "hasSeenFill",
        realm.native("hasSeenFill", 1, |r, _, args| {
            let module = cache_module(r)?;
            let fill = args.first().cloned().unwrap_or(Value::Undefined);
            cache_runtime::has_seen_fill(r, &module, &fill)
        }),
    );
    realm.bind_import(
        "markFillSeen",
        realm.native("markFillSeen", 1, |r, _, args| {
            let module = cache_module(r)?;
            let fill = args.first().cloned().unwrap_or(Value::Undefined);
            cache_runtime::mark_fill_seen(r, &module, &fill)?;
            Ok(Value::Undefined)
        }),
    );
}
