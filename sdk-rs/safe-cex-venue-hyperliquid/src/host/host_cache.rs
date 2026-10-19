//! The cross-context cache class (hyperliquid.cross-cache.ts) over the realm.
//!
//! CHOSEN: one module per realm, so the `getCrossCache` singleton is shared by
//! that exchange's own code but not across exchange instances. Native hosts
//! have no BroadcastChannel or sessionStorage, so the cache is memory-only,
//! which is the source's own behavior in a worker without those globals.
use super::{Host, host_core::js};
use crate::{
    cache_runtime::{self, CacheModule, CacheRuntimeHost, Callback},
    spot_json::JsString,
};
use safe_cex_exchange::js_runtime::value::{Js, Thrown};
use std::{cell::RefCell, rc::Rc};

type ModuleSlot = Rc<RefCell<Option<Rc<CacheModule<Js>>>>>;

impl CacheRuntimeHost for Host {
    fn number(&self, value: f64) -> Js {
        Js::Number(value)
    }
    fn boolean(&self, value: bool) -> Js {
        Js::Bool(value)
    }
    fn equal(&self, a: &Js, b: &Js) -> bool {
        a.strict_equals(b)
    }
    fn subtract(&mut self, a: &Js, b: &Js) -> Result<Js, Thrown> {
        Host::binary(self, "-", a, b)
    }
    fn less(&mut self, a: &Js, b: &Js) -> Result<bool, Thrown> {
        Ok(Host::binary(self, "<", a, b)?.truthy())
    }
    fn less_equal(&mut self, a: &Js, b: &Js) -> Result<bool, Thrown> {
        Ok(Host::binary(self, "<=", a, b)?.truthy())
    }
    fn abstract_string(&mut self, value: &Js) -> Result<JsString, Thrown> {
        Ok(js(Host::to_string(self, value)?))
    }
    fn array(&mut self, values: &[Js]) -> Result<Js, Thrown> {
        Ok(self.new_array(values.to_vec()))
    }
    fn class_definition(&mut self) -> Result<(Js, Js), Thrown> {
        let slot = self.module_slot();
        let base = Js::Null;
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
        prototype: &Js,
        name: &JsString,
        function: &Js,
    ) -> Result<(), Thrown> {
        self.define_hidden(
            prototype,
            Js::String(name.0.as_slice().into()),
            function.clone(),
        );
        Ok(())
    }
    fn instance(&mut self, prototype: &Js) -> Result<Js, Thrown> {
        Ok(self.object_with_prototype(prototype.clone()))
    }
    fn new_map(&mut self) -> Result<Js, Thrown> {
        Ok(Host::new_map(self))
    }
    fn new_set(&mut self) -> Result<Js, Thrown> {
        Ok(Host::new_set(self))
    }
    fn function(&mut self, capture: Js, body: Callback) -> Result<Js, Thrown> {
        Ok(self.native("", 0, move |r, this, args| {
            cache_runtime::run_callback(r, body, &capture, this, args)
        }))
    }
    fn broadcast_constructor(&mut self) -> Result<Js, Thrown> {
        let globals = self.shared.globals.clone();
        let key = Js::str("BroadcastChannel");
        if self.has_property(&globals, &key)? {
            self.get(&globals, &key)
        } else {
            Ok(Js::Undefined)
        }
    }
    fn construct_broadcast(&mut self, constructor: &Js, name: &Js) -> Result<Js, Thrown> {
        Host::construct(self, constructor, std::slice::from_ref(name), None)
    }
    fn destructure(&mut self, value: &Js, keys: &[&str]) -> Result<Vec<Js>, Thrown> {
        self.object_pattern(value, keys)
    }
}

impl Host {
    fn module_slot(&self) -> ModuleSlot {
        let holder = self.singleton("crossCacheModuleSlot", |r| {
            r.host_object(Rc::new(ModuleSlot::default()))
        });
        holder
            .resource::<ModuleSlot>()
            .map(|s| (*s).clone())
            .expect("module slot host object")
    }
}

fn cache_module(realm: &mut Host) -> Result<Rc<CacheModule<Js>>, Thrown> {
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

pub(crate) fn bind(realm: &Host) {
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
            let fill = args.first().cloned().unwrap_or(Js::Undefined);
            cache_runtime::has_seen_fill(r, &module, &fill)
        }),
    );
    realm.bind_import(
        "markFillSeen",
        realm.native("markFillSeen", 1, |r, _, args| {
            let module = cache_module(r)?;
            let fill = args.first().cloned().unwrap_or(Js::Undefined);
            cache_runtime::mark_fill_seen(r, &module, &fill)?;
            Ok(Js::Undefined)
        }),
    );
}
