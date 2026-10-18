//! Host-trait adapters delegate to shared values and property operations.
use super::Host;
use safe_cex_exchange::js_runtime::{
    for_in::ForInRecord,
    globals::stringify_json,
    iterator::IteratorRecord,
    promise::{JsFuture, Promise, await_js},
    value::{self, Js, Thrown},
};
use std::rc::Rc;

impl Host {
    pub(crate) fn new_object(&self) -> Js {
        Js::object(vec![])
    }
    pub(crate) fn object_from(&self, fields: Vec<(&str, Js)>) -> Js {
        Js::object(fields)
    }
    pub(crate) fn new_array(&self, values: Vec<Js>) -> Js {
        Js::array(values)
    }
    pub(crate) fn new_map(&self) -> Js {
        Js::new_map()
    }
    pub(crate) fn new_set(&self) -> Js {
        Js::new_set()
    }
    pub(crate) fn host_object<T: 'static>(&self, resource: Rc<T>) -> Js {
        Js::host_object(resource)
    }
    pub(crate) fn native(
        &self,
        name: &str,
        length: u8,
        body: impl Fn(&mut Host, &Js, &[Js]) -> Result<Js, Thrown> + 'static,
    ) -> Js {
        let shared = Rc::downgrade(&self.shared);
        let lexical = self.lexical.clone();
        let module = self.module;
        let function = Js::function(name, move |receiver, args| {
            let shared = shared
                .upgrade()
                .ok_or_else(|| Thrown::error("Error", "Hyperliquid host disposed"))?;
            body(
                &mut Host {
                    shared,
                    lexical: lexical.clone(),
                    module,
                },
                &receiver,
                &args,
            )
        });
        function
            .define_property(
                &value::utf16("length"),
                &Js::object(vec![("value", Js::Number(length as f64))]),
            )
            .expect("fresh function length");
        function
    }
    pub(crate) fn error_of(&self, class: &str, message: &str) -> Thrown {
        Thrown::error(class, message)
    }
    pub(crate) fn type_error(&self, message: &str) -> Thrown {
        Thrown::type_error(message)
    }
    pub(crate) fn error(&self, message: &str) -> Thrown {
        Thrown::error("Error", message)
    }
    pub(crate) fn restore_error(&self, error: safe_cex_exchange::runtime::ExchangeError) -> Thrown {
        let thrown = Thrown::from(error.clone());
        let mut source = std::error::Error::source(&error);
        while let Some(cause) = source {
            if let Some(owned) = cause.downcast_ref::<value::owned::OwnedCallbackError>() {
                for (key, cell) in owned
                    .value()
                    .to_js()
                    .own_entries()
                    .expect("owned diagnostic data")
                {
                    if !matches!(
                        String::from_utf16_lossy(&key).as_str(),
                        "name" | "message" | "code" | "response"
                    ) {
                        thrown
                            .value
                            .define_key(&key, cell)
                            .expect("fresh diagnostic metadata");
                    }
                }
                break;
            }
            source = cause.source();
        }
        thrown
    }
    pub(crate) fn exchange_error(
        &self,
        error: &Thrown,
    ) -> safe_cex_exchange::runtime::ExchangeError {
        error
            .native
            .as_ref()
            .map(|native| (**native).clone())
            .unwrap_or_else(|| error.to_exchange_error())
    }
    pub(crate) fn get(&mut self, value: &Js, key: &Js) -> Result<Js, Thrown> {
        value.get_property(key)
    }
    pub(crate) fn get_str(&mut self, value: &Js, key: &str) -> Result<Js, Thrown> {
        value.get(key)
    }
    pub(crate) fn set(&mut self, target: &Js, key: &Js, value: Js) -> Result<(), Thrown> {
        target.set_key(&key.to_js_string()?, value)
    }
    pub(crate) fn set_str(&mut self, target: &Js, key: &str, value: Js) -> Result<(), Thrown> {
        target.set(key, value)
    }
    pub(crate) fn define(&mut self, target: &Js, key: Js, value: Js) -> Result<(), Thrown> {
        target.define_property_for(
            &key,
            &Js::object(vec![
                ("value", value),
                ("writable", Js::Bool(true)),
                ("enumerable", Js::Bool(true)),
                ("configurable", Js::Bool(true)),
            ]),
        )
    }
    pub(crate) fn define_str(&mut self, target: &Js, key: &str, value: Js) -> Result<(), Thrown> {
        target.define(key, value)
    }
    pub(crate) fn define_hidden(&self, target: &Js, key: Js, value: Js) {
        target
            .define_property(
                &key.to_js_string().expect("literal key"),
                &Js::object(vec![
                    ("value", value),
                    ("writable", Js::Bool(true)),
                    ("enumerable", Js::Bool(false)),
                    ("configurable", Js::Bool(true)),
                ]),
            )
            .expect("fresh host prototype descriptor");
    }
    pub(crate) fn define_getter(&self, target: &Js, key: Js, getter: Js) {
        target
            .define_property(
                &key.to_js_string().expect("literal key"),
                &Js::object(vec![
                    ("get", getter),
                    ("enumerable", Js::Bool(false)),
                    ("configurable", Js::Bool(true)),
                ]),
            )
            .expect("fresh host getter descriptor");
    }
    pub(crate) fn delete(&mut self, target: &Js, key: &Js) -> Result<bool, Thrown> {
        target.delete(&key.to_js_string()?)?;
        Ok(true)
    }
    pub(crate) fn has_property(&mut self, value: &Js, key: &Js) -> Result<bool, Thrown> {
        if !value.is_object() {
            return Err(Thrown::type_error(
                "Cannot use 'in' operator on a primitive",
            ));
        }
        Ok(value.has_property(&key.to_js_string()?))
    }
    pub(crate) fn call(&mut self, function: &Js, receiver: &Js, args: &[Js]) -> Result<Js, Thrown> {
        function.call(receiver.clone(), args.to_vec())
    }
    pub(crate) fn call_method(
        &mut self,
        object: &Js,
        name: &str,
        args: &[Js],
    ) -> Result<Js, Thrown> {
        object.call_method(name, args.to_vec())
    }
    pub(crate) fn construct(
        &mut self,
        class: &Js,
        args: &[Js],
        target: Option<&Js>,
    ) -> Result<Js, Thrown> {
        class.construct(args.to_vec(), target)
    }
    pub(crate) fn index(&mut self, object: &Js, key: &Js) -> Result<Js, Thrown> {
        object.get_property(key)
    }
    pub(crate) fn set_index(&mut self, object: &Js, key: &Js, value: Js) -> Result<(), Thrown> {
        self.set(object, key, value)
    }
    pub(crate) fn to_property_key(&self, value: &Js) -> Result<Js, Thrown> {
        if matches!(value, Js::SymbolIterator) {
            return Ok(value.clone());
        }
        Ok(Js::String(value.to_js_string()?))
    }
    pub(crate) fn to_string(&self, value: &Js) -> Result<Rc<[u16]>, Thrown> {
        value.to_js_string()
    }
    pub(crate) fn to_rust_string(&self, value: &Js) -> Result<String, Thrown> {
        Ok(String::from_utf16_lossy(&value.to_js_string()?))
    }
    pub(crate) fn to_number(&self, value: &Js) -> Result<f64, Thrown> {
        value.to_number()
    }
    pub(crate) fn template(&mut self, value: &Js) -> Result<Rc<[u16]>, Thrown> {
        value.to_js_string()
    }
    pub(crate) fn encode_typed<T: serde::Serialize + ?Sized>(
        &self,
        value: &T,
    ) -> Result<Js, Thrown> {
        value::from_typed(value)
    }
    pub(crate) fn to_typed<T: serde::de::DeserializeOwned>(&self, value: &Js) -> Result<T, Thrown> {
        value::to_typed_preserving_undefined(value)
    }
    pub(crate) fn to_serde(&self, value: &Js) -> Result<serde_json::Value, Thrown> {
        value::to_json(value)
    }
    pub(crate) fn encode_json(&self, value: &serde_json::Value) -> Js {
        value::from_json(value)
    }
    pub(crate) fn stringify(&mut self, value: &Js) -> Result<Option<Vec<u16>>, Thrown> {
        stringify_json(value)
    }
    pub(crate) fn parse_json(&mut self, text: &[u16]) -> Result<Js, Thrown> {
        let json = self.global_binding("JSON")?;
        json.call_method("parse", vec![Js::utf16(text)])
    }
    pub(crate) fn regexp(&self, literal: &str) -> Result<Js, Thrown> {
        let end = literal
            .rfind('/')
            .filter(|end| *end > 0)
            .ok_or_else(|| Thrown::error("SyntaxError", "Invalid regular expression literal"))?;
        safe_cex_exchange::js_runtime::regexp::regexp(
            &literal[1..end].encode_utf16().collect::<Vec<_>>(),
            &literal[end + 1..].encode_utf16().collect::<Vec<_>>(),
        )
    }
    pub(crate) fn collect_iterable(&mut self, value: &Js) -> Result<Vec<Js>, Thrown> {
        value.iterable_values()
    }
    pub(crate) fn own_entries(&mut self, value: &Js) -> Result<Vec<(Js, Js)>, Thrown> {
        Ok(value
            .own_entries()?
            .into_iter()
            .map(|(key, value)| (Js::String(key), value))
            .collect())
    }
    pub(crate) fn own_keys(&mut self, value: &Js, enumerable: bool) -> Result<Vec<Js>, Thrown> {
        let keys = if enumerable {
            value.own_keys()
        } else {
            value.own_property_names()?
        };
        Ok(keys.into_iter().map(Js::String).collect())
    }
    pub(crate) fn await_value(&self, value: Js) -> JsFuture<'static> {
        Box::pin(await_js(&self.runtime(), value))
    }
    pub(crate) fn settled_promise(&self, result: Result<Js, Thrown>) -> Js {
        Js::Promise(Promise::settled(&self.runtime(), result))
    }
    pub(crate) fn spawn_value(&self, body: JsFuture<'static>) -> Js {
        Js::Promise(Promise::spawn(&self.runtime(), body))
    }
    pub(crate) fn start(&self, task: std::pin::Pin<Box<dyn std::future::Future<Output = ()>>>) {
        let runtime = self.runtime();
        let _waiter = safe_cex_exchange::eager::start_eager(&runtime, async move {
            task.await;
            Ok(())
        });
    }
}

impl Host {
    pub(crate) fn constructor(
        &self,
        name: &str,
        length: u8,
        body: impl Fn(&mut Host, &[Js], &Js) -> Result<Js, Thrown> + 'static,
    ) -> Js {
        let shared = Rc::downgrade(&self.shared);
        let lexical = self.lexical.clone();
        let module = self.module;
        let class = Js::constructor(name, None, move |target, args| {
            let shared = shared
                .upgrade()
                .ok_or_else(|| Thrown::error("Error", "Hyperliquid host disposed"))?;
            body(
                &mut Host {
                    shared,
                    lexical: lexical.clone(),
                    module,
                },
                &args,
                &target,
            )
        });
        class
            .define_property(
                &value::utf16("length"),
                &Js::object(vec![("value", Js::Number(length as f64))]),
            )
            .expect("fresh constructor length");
        class
    }
    pub(crate) fn object_with_prototype(&self, prototype: Js) -> Js {
        let object = Js::object(vec![]);
        let Js::Object(data) = &object else {
            unreachable!()
        };
        *data.proto.borrow_mut() = Some(prototype);
        object
    }
    pub(crate) fn get_with_receiver(
        &mut self,
        value: &Js,
        key: &Js,
        receiver: &Js,
    ) -> Result<Js, Thrown> {
        value.get_key_receiver(&key.to_js_string()?, receiver)
    }
    pub(crate) fn get_iterator(&mut self, value: &Js) -> Result<Js, Thrown> {
        Ok(Js::host_object(Rc::new(IteratorRecord::get(value)?)))
    }
    pub(crate) fn iterator_step(&mut self, iterator: &Js) -> Result<Option<Js>, Thrown> {
        if let Some(record) = iterator.resource::<std::cell::RefCell<ForInRecord>>() {
            return Ok(record.borrow_mut().next_key()?.map(Js::String));
        }
        iterator
            .resource::<IteratorRecord>()
            .ok_or_else(|| Thrown::type_error("Expected an IteratorRecord"))?
            .step_value()
    }
    pub(crate) fn iterator_close(&mut self, iterator: &Js) -> Result<(), Thrown> {
        iterator
            .resource::<IteratorRecord>()
            .ok_or_else(|| Thrown::type_error("Expected an IteratorRecord"))?
            .close()
    }
    pub(crate) fn iterator_close_after_throw(&mut self, iterator: &Js) {
        match self.iterator_close(iterator) {
            Ok(()) | Err(_) => {}
        }
    }
    pub(crate) fn for_in_iterator(&mut self, value: &Js) -> Js {
        Js::host_object(Rc::new(std::cell::RefCell::new(ForInRecord::get(value))))
    }
    pub(crate) fn instance_of(&mut self, value: &Js, class: &Js) -> Result<bool, Thrown> {
        value.instance_of(class)
    }
    pub(crate) fn super_get(&mut self, home: &Js, receiver: &Js, name: &str) -> Result<Js, Thrown> {
        self.get_with_receiver(
            &home.prototype().unwrap_or(Js::Null),
            &Js::str(name),
            receiver,
        )
    }
}

impl Host {
    pub(crate) fn to_js_json(&self, value: &Js) -> Result<safe_cex_num::JsJsonValue, Thrown> {
        value::to_js_json(value)
    }
}
