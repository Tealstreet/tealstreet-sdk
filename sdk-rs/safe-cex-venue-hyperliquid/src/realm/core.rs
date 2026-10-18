//! Object allocation, property access, calls and the iteration protocol.
//! No RefCell borrow is held across a call back into script.
use super::{
    Realm,
    value::{
        FunctionData, IterSource, Kind, NativeFn, Obj, ObjectData, Prop, PropKey, Slot, Thrown,
        Value,
    },
};
use crate::spot_json::JsString;
use std::{cell::RefCell, rc::Rc};

fn units(text: &str) -> Rc<[u16]> {
    text.encode_utf16().collect::<Vec<_>>().into()
}
fn index_key(i: usize) -> PropKey {
    PropKey::String(units(&i.to_string()))
}

impl Realm {
    pub(crate) fn alloc(&self, kind: Kind, proto: Option<&Obj>) -> Value {
        Value::Object(Rc::new(RefCell::new(ObjectData::new(kind, proto.cloned()))))
    }
    pub(crate) fn new_object(&self) -> Value {
        self.alloc(Kind::Ordinary, Some(&self.intrinsics().object_proto))
    }
    pub(crate) fn object_from(&self, fields: Vec<(&str, Value)>) -> Value {
        let o = self.new_object();
        {
            let mut data = o.object().unwrap().borrow_mut();
            for (k, v) in fields {
                data.put(PropKey::str(k), v, true);
            }
        }
        o
    }
    pub(crate) fn new_array(&self, values: Vec<Value>) -> Value {
        self.alloc(Kind::Array(values), Some(&self.intrinsics().array_proto))
    }
    pub(crate) fn new_map(&self) -> Value {
        self.alloc(Kind::Map(vec![]), Some(&self.intrinsics().map_proto))
    }
    pub(crate) fn new_set(&self) -> Value {
        self.alloc(Kind::Set(vec![]), Some(&self.intrinsics().set_proto))
    }
    pub(crate) fn host_object<T: 'static>(&self, resource: Rc<T>) -> Value {
        self.alloc(Kind::Host(resource), Some(&self.intrinsics().object_proto))
    }
    pub(crate) fn function(&self, name: &str, length: u8, call: NativeFn) -> Value {
        self.function_data(FunctionData {
            name: JsString(name.encode_utf16().collect()),
            length,
            call: Some(call),
            construct: None,
        })
    }
    pub(crate) fn function_data(&self, data: FunctionData) -> Value {
        let f = self.alloc(
            Kind::Function(Rc::new(data)),
            Some(&self.intrinsics().function_proto),
        );
        f
    }
    /// A native function from a closure.
    pub(crate) fn native(
        &self,
        name: &str,
        length: u8,
        body: impl Fn(&mut Realm, &Value, &[Value]) -> Result<Value, Thrown> + 'static,
    ) -> Value {
        self.function(name, length, Rc::new(body))
    }

    // ---- errors ----
    pub(crate) fn error_proto_for(&self, class: &str) -> Obj {
        let i = self.intrinsics();
        match class {
            "TypeError" => i.type_error_proto.clone(),
            "RangeError" => i.range_error_proto.clone(),
            "SyntaxError" => i.syntax_error_proto.clone(),
            "ReferenceError" => i.type_error_proto.clone(),
            _ => i.error_proto.clone(),
        }
    }
    /// A fresh Error instance of `class` (its `name` comes from the prototype,
    /// except ReferenceError, which shares TypeError's shape with its own name).
    pub(crate) fn error_of(&self, class: &str, message: &str) -> Value {
        let e = self.alloc(Kind::Error, Some(&self.error_proto_for(class)));
        {
            let mut data = e.object().unwrap().borrow_mut();
            data.put(PropKey::str("message"), Value::str(message), false);
            if class == "ReferenceError" {
                data.put(PropKey::str("name"), Value::str(class), false);
            }
        }
        e
    }
    pub(crate) fn type_error(&self, message: &str) -> Thrown {
        self.error_of("TypeError", message)
    }
    pub(crate) fn error(&self, message: &str) -> Thrown {
        self.error_of("Error", message)
    }
    pub(crate) fn from_exchange_error(
        &mut self,
        error: safe_cex_exchange::runtime::ExchangeError,
    ) -> Thrown {
        let native_error = error.origin.as_ref().map(|_| Rc::new(error.clone()));
        let cause = error.origin.as_ref().and_then(|origin| {
            let failure = origin
                .error()
                .downcast_ref::<safe_cex_transport::RestFailure>()?;
            let safe_cex_transport::TransportError::Rejected(origin) =
                failure.transport_error.as_deref()?
            else {
                return None;
            };
            origin
                .error()
                .downcast_ref::<safe_cex_exchange::js_runtime::value::owned::OwnedCallbackError>()
        });
        let thrown = match cause {
            Some(cause) => super::imports_venue::signing_error(self, cause.clone()),
            None => self.error(&error.message),
        };
        if error.name != "Error" {
            self.define_hidden(&thrown, PropKey::str("name"), Value::str(&error.name));
        }
        if let Some(code) = &error.code {
            let code = match self.restore_typed_value(code) {
                Ok(code) => code,
                Err(cause) => return cause,
            };
            self.define_hidden(&thrown, PropKey::str("code"), code);
        }
        if let Some(response) = error.retained_response_input() {
            let response = match self.restore_typed_value(
                &safe_cex_exchange::js_runtime::value::from_retained_input(response),
            ) {
                Ok(response) => response,
                Err(cause) => return cause,
            };
            self.define_hidden(&thrown, PropKey::str("response"), response);
        }
        thrown.object().unwrap().borrow_mut().native_error = native_error;
        thrown
    }

    // ---- properties ----
    fn nullish_read(&self, v: &Value, key: &PropKey) -> Thrown {
        let what = if matches!(v, Value::Null) {
            "null"
        } else {
            "undefined"
        };
        let name = key
            .units()
            .map(String::from_utf16_lossy)
            .unwrap_or_else(|| "Symbol()".into());
        self.type_error(&format!(
            "Cannot read properties of {what} (reading '{name}')"
        ))
    }
    fn primitive_proto(&self, v: &Value) -> Option<Obj> {
        let i = self.intrinsics();
        Some(match v {
            Value::String(_) => i.string_proto.clone(),
            Value::Number(_) => i.number_proto.clone(),
            Value::Bool(_) => i.boolean_proto.clone(),
            Value::Symbol(_) => i.symbol_proto.clone(),
            _ => return None,
        })
    }
    /// The own slot of `o` for `key`, including exotic array/string indices.
    fn own_slot(o: &Obj, key: &PropKey) -> Option<Slot> {
        let data = o.borrow();
        match &data.kind {
            Kind::Array(items) => {
                if let Some(i) = key.array_index() {
                    return items.get(i).map(|v| Slot::Data(v.clone()));
                }
                if key.units() == Some(&units("length")[..]) {
                    return Some(Slot::Data(Value::Number(items.len() as f64)));
                }
            }
            Kind::Function(f) => {
                if data.own(key).is_none() {
                    if key.units() == Some(&units("name")[..]) {
                        return Some(Slot::Data(Value::js(&f.name)));
                    }
                    if key.units() == Some(&units("length")[..]) {
                        return Some(Slot::Data(Value::Number(f.length as f64)));
                    }
                }
            }
            _ => {}
        }
        data.own(key).map(|p| p.slot.clone())
    }
    pub(crate) fn get(&mut self, v: &Value, key: &PropKey) -> Result<Value, Thrown> {
        self.get_with_receiver(v, key, v)
    }
    pub(crate) fn get_str(&mut self, v: &Value, key: &str) -> Result<Value, Thrown> {
        self.get(v, &PropKey::str(key))
    }
    pub(crate) fn get_with_receiver(
        &mut self,
        v: &Value,
        key: &PropKey,
        receiver: &Value,
    ) -> Result<Value, Thrown> {
        let mut current = match v {
            Value::Undefined | Value::Null => return Err(self.nullish_read(v, key)),
            Value::String(u) => {
                if let Some(i) = key.array_index() {
                    return Ok(u.get(i).map_or(Value::Undefined, |c| Value::units(&[*c])));
                }
                if key.units() == Some(&units("length")[..]) {
                    return Ok(Value::Number(u.len() as f64));
                }
                self.primitive_proto(v)
            }
            Value::Object(o) => Some(o.clone()),
            _ => self.primitive_proto(v),
        };
        while let Some(o) = current {
            let exotic = match &o.borrow().kind {
                Kind::Exotic(view) => Some(view.clone()),
                _ => None,
            };
            let missing_own = o.borrow().own(key).is_none();
            if let Some(view) = exotic
                && missing_own
                && let Some(v) = view.lookup(self, key)?
            {
                return Ok(v);
            }
            if let Some(slot) = Self::own_slot(&o, key) {
                return match slot {
                    Slot::Data(value) => Ok(value),
                    Slot::Accessor { get: Some(g), .. } => self.call(&g, receiver, &[]),
                    Slot::Accessor { get: None, .. } => Ok(Value::Undefined),
                };
            }
            current = o.borrow().proto.clone();
        }
        *self.state.last_missing.borrow_mut() = key.units().map(String::from_utf16_lossy);
        Ok(Value::Undefined)
    }
    /// Strict-mode `o[key] = value`.
    pub(crate) fn set(
        &mut self,
        target: &Value,
        key: &PropKey,
        value: Value,
    ) -> Result<(), Thrown> {
        self.set_property(target, key, value.clone())?;
        let after_set = target.object().and_then(|o| o.borrow().after_set.clone());
        if let Some(after_set) = after_set {
            after_set(self, key, &value)?;
        }
        Ok(())
    }
    fn set_property(&mut self, target: &Value, key: &PropKey, value: Value) -> Result<(), Thrown> {
        let o = match target {
            Value::Object(o) => o.clone(),
            Value::Undefined | Value::Null => {
                let what = if matches!(target, Value::Null) {
                    "null"
                } else {
                    "undefined"
                };
                let name = key
                    .units()
                    .map(String::from_utf16_lossy)
                    .unwrap_or_default();
                return Err(self.type_error(&format!(
                    "Cannot set properties of {what} (setting '{name}')"
                )));
            }
            _ => {
                let name = key
                    .units()
                    .map(String::from_utf16_lossy)
                    .unwrap_or_default();
                let shown = String::from_utf16_lossy(&self.to_string(target)?);
                return Err(self.type_error(&format!(
                    "Cannot create property '{name}' on {} '{shown}'",
                    target.type_of(),
                )));
            }
        };
        // Array exotic writes.
        {
            let mut data = o.borrow_mut();
            if let Kind::Array(items) = &mut data.kind {
                if let Some(i) = key.array_index() {
                    if i < items.len() {
                        items[i] = value;
                    } else {
                        items.resize(i, Value::Undefined);
                        items.push(value);
                    }
                    return Ok(());
                }
                if key.units() == Some(&units("length")[..]) {
                    drop(data);
                    let n = self.to_number(&value)?;
                    let mut data = o.borrow_mut();
                    let Kind::Array(items) = &mut data.kind else {
                        unreachable!()
                    };
                    if n < 0.0 || n.fract() != 0.0 || n > u32::MAX as f64 {
                        drop(data);
                        return Err(self.error_of("RangeError", "Invalid array length"));
                    }
                    items.resize(n as usize, Value::Undefined);
                    return Ok(());
                }
            }
        }
        // Own or inherited setter / read-only data.
        let mut current = Some(o.clone());
        while let Some(c) = current {
            let found = c.borrow().own(key).cloned();
            if let Some(p) = found {
                match p.slot {
                    Slot::Accessor { set: Some(s), .. } => {
                        self.call(&s, target, &[value])?;
                        return Ok(());
                    }
                    Slot::Accessor { set: None, .. } => {
                        let name = key
                            .units()
                            .map(String::from_utf16_lossy)
                            .unwrap_or_default();
                        return Err(self.type_error(&format!(
                            "Cannot set property {name} of #<Object> which has only a getter"
                        )));
                    }
                    Slot::Data(_) if !p.writable => {
                        let name = key
                            .units()
                            .map(String::from_utf16_lossy)
                            .unwrap_or_default();
                        return Err(self.type_error(&format!(
                            "Cannot assign to read only property '{name}' of object"
                        )));
                    }
                    Slot::Data(_) => {
                        if Rc::ptr_eq(&c, &o) {
                            o.borrow_mut().own_mut(key).unwrap().slot = Slot::Data(value);
                            return Ok(());
                        }
                        break;
                    }
                }
            }
            current = c.borrow().proto.clone();
        }
        self.define(target, key.clone(), value)
    }
    pub(crate) fn set_str(
        &mut self,
        target: &Value,
        key: &str,
        value: Value,
    ) -> Result<(), Thrown> {
        self.set(target, &PropKey::str(key), value)
    }
    /// CreateDataPropertyOrThrow.
    pub(crate) fn define(
        &mut self,
        target: &Value,
        key: PropKey,
        value: Value,
    ) -> Result<(), Thrown> {
        let Some(o) = target.object() else {
            return Err(self.type_error("CreateDataProperty called on non-object"));
        };
        let mut data = o.borrow_mut();
        if let Kind::Array(items) = &mut data.kind
            && let Some(i) = key.array_index()
        {
            if i < items.len() {
                items[i] = value;
            } else {
                items.resize(i, Value::Undefined);
                items.push(value);
            }
            return Ok(());
        }
        if let Some(p) = data.own_mut(&key) {
            if !p.configurable && !p.writable {
                drop(data);
                return Err(self.type_error("Cannot redefine property"));
            }
            p.slot = Slot::Data(value);
            p.enumerable = true;
            p.writable = true;
            p.configurable = true;
            return Ok(());
        }
        if !data.extensible {
            drop(data);
            return Err(self.type_error("Cannot define property, object is not extensible"));
        }
        data.props.push(Prop {
            key,
            slot: Slot::Data(value),
            enumerable: true,
            writable: true,
            configurable: true,
        });
        Ok(())
    }
    pub(crate) fn define_str(
        &mut self,
        target: &Value,
        key: &str,
        value: Value,
    ) -> Result<(), Thrown> {
        self.define(target, PropKey::str(key), value)
    }
    /// A non-enumerable own method/field (class prototype member).
    pub(crate) fn define_hidden(&self, target: &Value, key: PropKey, value: Value) {
        if let Some(o) = target.object() {
            let mut data = o.borrow_mut();
            if let Some(p) = data.own_mut(&key) {
                p.slot = Slot::Data(value);
                p.enumerable = false;
                return;
            }
            data.props.push(Prop {
                key,
                slot: Slot::Data(value),
                enumerable: false,
                writable: true,
                configurable: true,
            });
        }
    }
    pub(crate) fn define_getter(&self, target: &Value, key: PropKey, getter: Value) {
        if let Some(o) = target.object() {
            let mut data = o.borrow_mut();
            data.props.retain(|p| !p.key.same(&key));
            data.props.push(Prop {
                key,
                slot: Slot::Accessor {
                    get: Some(getter),
                    set: None,
                },
                enumerable: false,
                writable: false,
                configurable: true,
            });
        }
    }
    /// Strict `delete o[key]`; a non-configurable property throws.
    pub(crate) fn delete(&mut self, target: &Value, key: &PropKey) -> Result<bool, Thrown> {
        let Some(o) = target.object() else {
            if target.is_nullish() {
                return Err(self.type_error("Cannot convert undefined or null to object"));
            }
            return Ok(true);
        };
        let mut data = o.borrow_mut();
        if let Kind::Array(items) = &mut data.kind
            && let Some(i) = key.array_index()
        {
            if i < items.len() {
                items[i] = Value::Undefined;
            }
            return Ok(true);
        }
        if let Some(pos) = data.props.iter().position(|p| p.key.same(key)) {
            if !data.props[pos].configurable {
                drop(data);
                let name = key
                    .units()
                    .map(String::from_utf16_lossy)
                    .unwrap_or_default();
                return Err(
                    self.type_error(&format!("Cannot delete property '{name}' of #<Object>"))
                );
            }
            data.props.remove(pos);
        }
        Ok(true)
    }
    /// The `in` operator.
    pub(crate) fn has_property(&mut self, target: &Value, key: &PropKey) -> Result<bool, Thrown> {
        let Some(o) = target.object() else {
            let name = key
                .units()
                .map(String::from_utf16_lossy)
                .unwrap_or_default();
            let shown =
                String::from_utf16_lossy(&self.to_string(target).unwrap_or_else(|_| units("")));
            return Err(self.type_error(&format!(
                "Cannot use 'in' operator to search for '{name}' in {shown}"
            )));
        };
        let mut current = Some(o.clone());
        while let Some(c) = current {
            let exotic = match &c.borrow().kind {
                Kind::Exotic(view) => Some(view.clone()),
                _ => None,
            };
            if let Some(view) = exotic
                && view.keys(self)?.iter().any(|k| k.same(key))
            {
                return Ok(true);
            }
            if Self::own_slot(&c, key).is_some() {
                return Ok(true);
            }
            current = c.borrow().proto.clone();
        }
        Ok(false)
    }
    pub(crate) fn has_own(&mut self, target: &Value, key: &PropKey) -> Result<bool, Thrown> {
        let Some(o) = target.object() else {
            return Ok(false);
        };
        if Self::own_slot(o, key).is_some() {
            return Ok(true);
        }
        let view = match &o.borrow().kind {
            Kind::Exotic(view) => Some(view.clone()),
            _ => None,
        };
        Ok(match view {
            Some(view) => view.keys(self)?.iter().any(|k| k.same(key)),
            None => false,
        })
    }
    /// OrdinaryOwnPropertyKeys order: integer keys ascending, then strings by
    /// insertion. `enumerable_only` filters for Object.keys / for-in / spread.
    pub(crate) fn own_keys(
        &mut self,
        target: &Value,
        enumerable_only: bool,
    ) -> Result<Vec<PropKey>, Thrown> {
        let Some(o) = target.object() else {
            if let Value::String(u) = target {
                return Ok((0..u.len()).map(index_key).collect());
            }
            return Ok(vec![]);
        };
        let view = match &o.borrow().kind {
            Kind::Exotic(view) => Some(view.clone()),
            _ => None,
        };
        let view_keys = match view {
            Some(view) => view.keys(self)?,
            None => vec![],
        };
        let data = o.borrow();
        let mut ints: Vec<(usize, PropKey)> = vec![];
        let mut strings = vec![];
        if let Kind::Array(items) = &data.kind {
            ints.extend((0..items.len()).map(|i| (i, index_key(i))));
        }
        for key in view_keys {
            match key.array_index() {
                Some(i) => ints.push((i, key)),
                None => strings.push(key),
            }
        }
        for p in &data.props {
            if enumerable_only && !p.enumerable {
                continue;
            }
            match &p.key {
                PropKey::Symbol(_) => {}
                key => match key.array_index() {
                    Some(i) => ints.push((i, key.clone())),
                    None => strings.push(key.clone()),
                },
            }
        }
        ints.sort_by_key(|(i, _)| *i);
        Ok(ints.into_iter().map(|(_, k)| k).chain(strings).collect())
    }
    /// Own enumerable string-keyed values in key order (for spreads/Object.entries).
    pub(crate) fn own_entries(&mut self, target: &Value) -> Result<Vec<(PropKey, Value)>, Thrown> {
        let keys = self.own_keys(target, true)?;
        let mut out = Vec::with_capacity(keys.len());
        for k in keys {
            let v = self.get(target, &k)?;
            out.push((k, v));
        }
        Ok(out)
    }

    // ---- calls ----
    pub(crate) fn call(
        &mut self,
        f: &Value,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Thrown> {
        match f.function().and_then(|f| f.call.clone()) {
            Some(body) => body(self, this, args),
            None => {
                let name = match f.function() {
                    Some(func) => format!(
                        "Class constructor {} cannot be invoked without 'new'",
                        String::from_utf16_lossy(&func.name.0)
                    ),
                    None if matches!(f, Value::Undefined) => {
                        match self.state.last_missing.borrow().as_deref() {
                            Some(name) => format!("{name} is not a function"),
                            None => "undefined is not a function".into(),
                        }
                    }
                    None => format!("{f:?} is not a function"),
                };
                Err(self.type_error(&name))
            }
        }
    }
    pub(crate) fn call_method(
        &mut self,
        target: &Value,
        name: &str,
        args: &[Value],
    ) -> Result<Value, Thrown> {
        let f = self.get_str(target, name)?;
        if !f.is_callable() {
            return Err(self.type_error(&format!(
                "{}.{name} is not a function",
                if target.is_array() { "array" } else { "object" }
            )));
        }
        self.call(&f, target, args)
    }
    pub(crate) fn construct(
        &mut self,
        f: &Value,
        args: &[Value],
        new_target: Option<&Value>,
    ) -> Result<Value, Thrown> {
        let Some(body) = f.function().and_then(|d| d.construct.clone()) else {
            return Err(self.type_error(&format!("{f:?} is not a constructor")));
        };
        let target = new_target.cloned().unwrap_or_else(|| f.clone());
        body(self, args, &target)
    }
    /// The `prototype` of `new_target`, for allocating a constructed instance.
    pub(crate) fn prototype_of_target(
        &mut self,
        new_target: &Value,
        fallback: &Obj,
    ) -> Result<Obj, Thrown> {
        let p = self.get_str(new_target, "prototype")?;
        Ok(p.object().cloned().unwrap_or_else(|| fallback.clone()))
    }
    pub(crate) fn instance_of(&mut self, v: &Value, ctor: &Value) -> Result<bool, Thrown> {
        if !ctor.is_callable() && ctor.function().is_none() {
            return Err(self.type_error("Right-hand side of 'instanceof' is not callable"));
        }
        let p = self.get_str(ctor, "prototype")?;
        let Some(p) = p.object().cloned() else {
            return Err(self.type_error("Function has non-object prototype in instanceof check"));
        };
        let mut current = v.object().and_then(|o| o.borrow().proto.clone());
        while let Some(c) = current {
            if Rc::ptr_eq(&c, &p) {
                return Ok(true);
            }
            current = c.borrow().proto.clone();
        }
        Ok(false)
    }

    // ---- iteration ----
    pub(crate) fn iterator_object(&self, source: IterSource) -> Value {
        self.alloc(
            Kind::Iterator(source),
            Some(&self.intrinsics().iterator_proto),
        )
    }
    /// GetIterator for for-of / spread / destructuring.
    pub(crate) fn get_iterator(&mut self, v: &Value) -> Result<Value, Thrown> {
        let key = PropKey::Symbol(self.intrinsics().symbol_iterator.clone());
        let method = self.get(v, &key)?;
        if !method.is_callable() {
            let shown =
                String::from_utf16_lossy(&self.to_string(v).unwrap_or_else(|_| units("object")));
            return Err(self.type_error(&format!("{shown} is not iterable")));
        }
        let iterator = self.call(&method, v, &[])?;
        if iterator.object().is_none() {
            return Err(self.type_error("Result of the Symbol.iterator method is not an object"));
        }
        let next = self.get_str(&iterator, "next")?;
        Ok(self.iterator_object(IterSource::Protocol { iterator, next }))
    }
    pub(crate) fn iterator_step(&mut self, iterator: &Value) -> Result<Option<Value>, Thrown> {
        let Some(o) = iterator.object().cloned() else {
            return Err(self.type_error("iterator is not an object"));
        };
        let protocol = {
            let mut data = o.borrow_mut();
            let Kind::Iterator(source) = &mut data.kind else {
                drop(data);
                return Err(self.type_error("not an iterator"));
            };
            match source {
                IterSource::Protocol { iterator, next } => Some((iterator.clone(), next.clone())),
                IterSource::Done => return Ok(None),
                IterSource::Keys { keys, index } => {
                    let v = keys.get(*index).cloned();
                    *index += 1;
                    if v.is_none() {
                        *source = IterSource::Done;
                    }
                    return Ok(v);
                }
                IterSource::String { units, index } => {
                    if *index >= units.len() {
                        *source = IterSource::Done;
                        return Ok(None);
                    }
                    let unit = units[*index];
                    let width = if (0xd800..0xdc00).contains(&unit)
                        && units
                            .get(*index + 1)
                            .is_some_and(|u| (0xdc00..0xe000).contains(u))
                    {
                        2
                    } else {
                        1
                    };
                    let v = Value::units(&units[*index..*index + width]);
                    *index += width;
                    return Ok(Some(v));
                }
                _ => None,
            }
        };
        if let Some((target, next)) = protocol {
            let result = self.call(&next, &target, &[])?;
            if !result.object().is_some() {
                return Err(self.type_error("Iterator result is not an object"));
            }
            let done = self.get_str(&result, "done")?;
            if done.truthy() {
                if let Kind::Iterator(source) = &mut o.borrow_mut().kind {
                    *source = IterSource::Done;
                }
                return Ok(None);
            }
            return self.get_str(&result, "value").map(Some);
        }
        // Live collection iterators read the current contents each step.
        enum Step {
            Item(Value),
            Entry(Value, Value),
            Done,
        }
        let (step, project) = {
            let mut data = o.borrow_mut();
            let Kind::Iterator(source) = &mut data.kind else {
                unreachable!()
            };
            let step = match source {
                IterSource::Array { target, index } => {
                    let v = match &target.borrow().kind {
                        Kind::Array(items) => items.get(*index).cloned(),
                        _ => None,
                    };
                    *index += 1;
                    v.map_or(Step::Done, Step::Item)
                }
                IterSource::SetValues { target, index } => {
                    let v = match &target.borrow().kind {
                        Kind::Set(items) => items.get(*index).cloned(),
                        _ => None,
                    };
                    *index += 1;
                    v.map_or(Step::Done, Step::Item)
                }
                IterSource::MapEntries { target, index }
                | IterSource::MapKeys { target, index }
                | IterSource::MapValues { target, index } => {
                    let e = match &target.borrow().kind {
                        Kind::Map(entries) => entries.get(*index).cloned(),
                        _ => None,
                    };
                    *index += 1;
                    e.map_or(Step::Done, |(k, v)| Step::Entry(k, v))
                }
                _ => Step::Done,
            };
            let project = match source {
                IterSource::MapKeys { .. } => 1,
                IterSource::MapValues { .. } => 2,
                _ => 0,
            };
            if matches!(step, Step::Done) {
                *source = IterSource::Done;
            }
            (step, project)
        };
        Ok(match step {
            Step::Item(v) => Some(v),
            Step::Entry(k, v) => Some(match project {
                1 => k,
                2 => v,
                _ => self.new_array(vec![k, v]),
            }),
            Step::Done => None,
        })
    }
    /// IteratorClose after an abrupt body completion: errors from `return` are dropped.
    pub(crate) fn iterator_close_after_throw(&mut self, iterator: &Value) {
        let _ = self.iterator_close(iterator);
    }
    /// IteratorClose after break/return: a throwing `return` propagates.
    pub(crate) fn iterator_close(&mut self, iterator: &Value) -> Result<(), Thrown> {
        let protocol = iterator.object().and_then(|o| match &o.borrow().kind {
            Kind::Iterator(IterSource::Protocol { iterator, .. }) => Some(iterator.clone()),
            _ => None,
        });
        if let Some(o) = iterator.object()
            && let Kind::Iterator(source) = &mut o.borrow_mut().kind
        {
            *source = IterSource::Done;
        }
        if let Some(target) = protocol {
            let ret = self.get_str(&target, "return")?;
            if !ret.is_nullish() {
                let result = self.call(&ret, &target, &[])?;
                if result.object().is_none() {
                    return Err(self.type_error("Iterator result is not an object"));
                }
            }
        }
        Ok(())
    }
    /// Spread/Array.from over any iterable.
    pub(crate) fn collect_iterable(&mut self, v: &Value) -> Result<Vec<Value>, Thrown> {
        let iterator = self.get_iterator(v)?;
        let mut out = vec![];
        while let Some(item) = self.iterator_step(&iterator)? {
            out.push(item);
        }
        Ok(out)
    }
}
