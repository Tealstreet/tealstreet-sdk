use super::{Rt, Shared, scope};
use crate::{
    exchange_runtime::AssemblyHost,
    runtime_host::{Function, ResultValue, RuntimeHost, Task},
};
use safe_cex_exchange::js_runtime::{
    base_exchange, ops,
    promise::{Promise, await_js},
    value::{Js, Thrown},
};
use std::rc::{Rc, Weak};

pub(super) struct Host {
    weak: Weak<Shared>,
    this: Js,
}
impl Host {
    pub(super) fn new(rt: &Rt) -> Self {
        Self {
            weak: Rc::downgrade(&rt.shared),
            this: rt.this.clone(),
        }
    }
    fn rt(&self) -> Result<Rt, Js> {
        scope(&self.weak, self.this.clone()).map_err(|error| error.value)
    }
}
fn value<T>(result: Result<T, Thrown>) -> Result<T, Js> {
    result.map_err(|error| error.value)
}
impl RuntimeHost for Host {
    type Value = Js;
    fn get(&self, object: &Js, key: &str) -> ResultValue<Js> {
        value(object.get(key))
    }
    fn destructure(&self, object: Js, keys: &[&str], binding: &str) -> ResultValue<Js, Vec<Js>> {
        if object.is_nullish() {
            return Err(Thrown::type_error(&format!(
                "Cannot destructure property '{}' of '{}' as it is {}.",
                keys.first().unwrap_or(&""),
                binding,
                object.to_display()
            ))
            .value);
        }
        keys.iter().map(|key| self.get(&object, key)).collect()
    }
    fn set(&self, object: &Js, key: &str, item: Js) -> ResultValue<Js, ()> {
        value(object.set(key, item))
    }
    fn define(&self, object: &Js, key: &str, item: Js) -> ResultValue<Js, ()> {
        value(object.define(key, item))
    }
    fn delete(&self, object: &Js, key: &str) -> ResultValue<Js, ()> {
        object.delete(&key.encode_utf16().collect::<Vec<_>>());
        Ok(())
    }
    fn call(&self, function: Js, receiver: Js, args: Vec<Js>) -> ResultValue<Js> {
        value(function.call(receiver, args))
    }
    fn function(&self, function: Function<Js>) -> Js {
        Js::function("", move |_, args| function(args).map_err(Thrown::new))
    }
    fn object(&self, fields: Vec<(&str, Js)>) -> Js {
        Js::object(fields)
    }
    fn spread(&self, target: &Js, source: Js) -> ResultValue<Js, ()> {
        for (key, item) in value(source.own_entries())? {
            value(target.define_key(&key, item))?;
        }
        Ok(())
    }
    fn array(&self, values: Vec<Js>) -> Js {
        Js::array(values)
    }
    fn text(&self, text: &str) -> Js {
        Js::str(text)
    }
    fn number(&self, number: f64) -> Js {
        Js::Number(number)
    }
    fn boolean(&self, boolean: bool) -> Js {
        Js::Bool(boolean)
    }
    fn undefined(&self) -> Js {
        Js::Undefined
    }
    fn truthy(&self, value: &Js) -> bool {
        value.truthy()
    }
    fn nullish(&self, value: &Js) -> bool {
        value.is_nullish()
    }
    fn strict_equal(&self, left: &Js, right: &Js) -> bool {
        left.strict_equals(right)
    }
    fn less(&self, left: Js, right: Js) -> ResultValue<Js, bool> {
        value(ops::less_than(&left, &right))
    }
    fn greater_equal(&self, left: Js, right: Js) -> ResultValue<Js, bool> {
        value(ops::greater_equal(&left, &right))
    }
    fn subtract(&self, left: Js, right: Js) -> ResultValue<Js> {
        value(ops::subtract(&left, &right))
    }
    fn template(&self, parts: Vec<Js>) -> ResultValue<Js> {
        let mut units = Vec::new();
        for part in parts {
            units.extend(value(part.to_js_string())?.iter().copied());
        }
        Ok(Js::utf16(&units))
    }
    fn error(&self, message: Js) -> Js {
        match message.to_js_string() {
            Ok(text) => safe_cex_exchange::js_runtime::value::error_object(
                "Error",
                &String::from_utf16_lossy(&text),
            ),
            Err(error) => error.value,
        }
    }
    fn import(&self, name: &str, args: Vec<Js>) -> ResultValue<Js> {
        value(
            self.rt()?
                .global(name)
                .and_then(|function| function.call(Js::Undefined, args)),
        )
    }
    fn constant(&self, name: &str) -> Js {
        self.rt()
            .ok()
            .and_then(|rt| rt.global(name).ok())
            .unwrap_or(Js::Undefined)
    }
    fn start(&self, task: Task<Js>) -> Task<Js> {
        let rt = match self.rt() {
            Ok(rt) => rt,
            Err(error) => return Box::pin(async move { Err(error) }),
        };
        let promise = Promise::spawn(
            &rt.exchange_runtime(),
            Box::pin(async move { task.await.map_err(Thrown::new) }),
        );
        Box::pin(async move { promise.wait().await.map_err(|error| error.value) })
    }
    fn promise(&self, task: Task<Js>) -> Js {
        match self.rt() {
            Ok(rt) => Js::Promise(Promise::spawn(
                &rt.exchange_runtime(),
                Box::pin(async move { task.await.map_err(Thrown::new) }),
            )),
            Err(error) => error,
        }
    }
    fn await_value(&self, item: Js) -> Task<Js> {
        let rt = self.rt();
        Box::pin(async move {
            await_js(&rt?.exchange_runtime(), item)
                .await
                .map_err(|error| error.value)
        })
    }
}
impl AssemblyHost for Host {
    fn base_exchange(&self, name: Js, options: Js, store: Js) -> ResultValue<Js> {
        let rt = self.rt()?;
        let parts = rt
            .shared
            .extra
            .parts
            .borrow_mut()
            .take()
            .ok_or_else(|| Thrown::error("Error", "AsterDEX base already constructed").value)?;
        let (this, base) = value(base_exchange::create(
            &rt.shared.sockets,
            parts,
            name,
            options,
            store,
            |_| Ok(()),
        ))?;
        let _ = rt.shared.base.set(base);
        let _ = rt.shared.extra.owner.set(this.clone());
        value(super::globals::initialize_clock(&rt))?;
        Ok(this)
    }
    fn member(&self, name: &'static str, owner: &Js) -> ResultValue<Js> {
        let weak = self.weak.clone();
        let owner = owner.clone();
        Ok(Js::function(name, move |_, args| {
            super::members::invoke(&scope(&weak, owner.clone())?, name, args)
        }))
    }
    fn construct(&self, name: &str, _: Vec<Js>) -> ResultValue<Js> {
        let private = name == "AsterDEXPrivateWebsocket";
        if !private && name != "AsterDEXPublicWebsocket" {
            return Err(Thrown::type_error(&format!("Unknown AsterDEX constructor {name}")).value);
        }
        Ok(super::sockets::facade(&self.weak, private))
    }
    fn base_dispose(&self, _: Js) -> ResultValue<Js> {
        self.rt()?
            .base()
            .dispose()
            .map_err(|error| super::error(error).value)?;
        Ok(Js::Undefined)
    }
}
