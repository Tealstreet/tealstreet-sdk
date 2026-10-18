//! SessionHost / SpotOrderHost over js_runtime values, for the ported order-write
//! bodies. A selected array method keeps its owner so the native iteration shims
//! (map, filter, some, find, reduce) walk the same array the source would.
use super::{
    host::text,
    promise::{Promise, await_js},
    rt::Rt,
    value::{Js, Thrown, error_object},
};
use crate::{
    hyro_private_session::{SessionFuture, SessionHost},
    spot_order_writes::SpotOrderHost,
};
use safe_cex_exchange::js_runtime::scope::bound_member;

const OWNER: &str = "__selectedOwner";
const NAME: &str = "__selectedName";

fn arg(args: &[Js], i: usize) -> Js {
    args.get(i).cloned().unwrap_or(Js::Undefined)
}

/// The array a selected method was read from, checked against the expected member.
fn selected_owner(method: &Js, expected: &str) -> Result<Vec<Js>, Thrown> {
    let name = method.get(NAME)?;
    if !name.strict_equals(&Js::str(expected)) {
        return Err(Thrown::type_error(&format!(
            "expected a selected Array.prototype.{expected}"
        )));
    }
    method
        .get(OWNER)?
        .array_values()
        .ok_or_else(|| Thrown::type_error(&format!("{expected} was not read from an array")))
}

impl SessionHost for Rt {
    type Value = Js;
    type Error = Thrown;
    fn get(&self, owner: &Js, key: &str) -> Result<Js, Thrown> {
        owner.get(key)
    }
    fn optional(&self, owner: &Js, key: &str) -> Result<Js, Thrown> {
        if owner.is_nullish() {
            Ok(Js::Undefined)
        } else {
            owner.get(key)
        }
    }
    fn set(&self, owner: &Js, key: &str, value: Js) -> Result<(), Thrown> {
        owner.set(key, value)
    }
    fn define(&self, owner: &Js, key: &str, value: Js) -> Result<(), Thrown> {
        owner.define(key, value)
    }
    fn truthy(&self, value: &Js) -> bool {
        value.truthy()
    }
    fn equal(&self, a: &Js, b: &Js) -> bool {
        a.strict_equals(b)
    }
    fn undefined(&self) -> Js {
        Js::Undefined
    }
    fn literal(&self, value: &str) -> Js {
        Js::str(value)
    }
    fn boolean(&self, value: bool) -> Js {
        Js::Bool(value)
    }
    fn number(&self, value: f64) -> Js {
        Js::Number(value)
    }
    fn numeric(&self, value: &Js) -> Result<f64, Thrown> {
        value.to_number()
    }
    fn units(&self, value: &Js) -> Result<Vec<u16>, Thrown> {
        Ok(value.to_js_string()?.to_vec())
    }
    fn text(&self, units: Vec<u16>) -> Js {
        Js::utf16(&units)
    }
    fn object(&self, fields: Vec<(&'static str, Js)>) -> Js {
        Js::object(fields)
    }
    fn array(&self, values: Vec<Js>) -> Js {
        Js::array(values)
    }
    fn global(&self) -> Js {
        self.shared.globals.clone()
    }
    fn select(&self, owner: Js, key: &str) -> Result<Js, Thrown> {
        if owner.is_nullish() {
            return owner.get(key);
        }
        let member = bound_member(&owner, key)?;
        if owner.is_array()
            && member.is_callable()
            && matches!(
                key,
                "map" | "filter" | "some" | "find" | "reduce" | "forEach"
            )
        {
            member.define(OWNER, owner)?;
            member.define(NAME, Js::str(key))?;
        }
        Ok(member)
    }
    fn receiver_call(&self, owner: Js, callable: Js, args: Vec<Js>) -> Result<Js, Thrown> {
        callable.call(owner, args)
    }
    fn apply(&self, selected: Js, args: Vec<Js>) -> Result<Js, Thrown> {
        if !selected.is_callable() {
            return Err(Thrown::type_error(&format!(
                "{} is not a function",
                selected.to_display()
            )));
        }
        selected.call(Js::Undefined, args)
    }
    fn wait(&self, value: Js) -> SessionFuture<'_, Js, Thrown> {
        Box::pin(await_js(value))
    }
    fn intrinsic(&self, name: &str, args: Vec<Js>) -> Result<Js, Thrown> {
        match name {
            "RegExp" => self.shared.globals.get("RegExp")?.call(Js::Undefined, args),
            "String" => self.shared.globals.get("String")?.call(Js::Undefined, args),
            "less than" => Ok(Js::Bool(safe_cex_exchange::js_runtime::ops::less_than(
                &arg(&args, 0),
                &arg(&args, 1),
            )?)),
            "instanceof Error" => Ok(Js::Bool(
                matches!(arg(&args, 0), Js::Object(ref object) if matches!(object.class, "Error" | "AxiosError")),
            )),
            "throw value" => Err(Thrown::new(arg(&args, 0))),
            "property in" => {
                let key = text(&arg(&args, 0))?;
                let mut owner = arg(&args, 1);
                if !owner.is_object() {
                    return Err(Thrown::type_error("right operand of in is not an object"));
                }
                loop {
                    if owner.has_own(&key) {
                        return Ok(Js::Bool(true));
                    }
                    let next = match &owner {
                        Js::Object(object) => object.proto.borrow().clone(),
                        _ => None,
                    };
                    match next {
                        Some(next) => owner = next,
                        None => return Ok(Js::Bool(false)),
                    }
                }
            }
            "construct" => arg(&args, 0).call(Js::Undefined, args.into_iter().skip(1).collect()),
            "iterable spread" => Ok(Js::array(arg(&args, 0).iterable_values()?)),
            "Number.isFinite" => self
                .shared
                .globals
                .get("Number")?
                .call_method("isFinite", args),
            "Number" => self.shared.globals.get("Number")?.call(Js::Undefined, args),
            "new Map" => self.shared.globals.get("Map")?.call(Js::Undefined, args),
            "Promise.resolve" => Ok(Js::Promise(Promise::settled(
                &self.exchange_runtime(),
                Ok(arg(&args, 0)),
            ))),
            "destructure three" => {
                let values = arg(&args, 0)
                    .array_values()
                    .ok_or_else(|| Thrown::type_error("value is not iterable"))?;
                Ok(Js::array(
                    (0..3)
                        .map(|i| values.get(i).cloned().unwrap_or(Js::Undefined))
                        .collect(),
                ))
            }
            "new Set" => self.shared.globals.get("Set")?.call(Js::Undefined, args),
            "super.mapPosition" => {
                super::members::mapping::map_position(self, &arg(&args, 0), &arg(&args, 1))
            }
            "null" => Ok(Js::Null),
            "template" => Ok(Js::String(arg(&args, 0).to_js_string()?)),
            "typeof" => Ok(Js::str(arg(&args, 0).type_of())),
            "subtract" => {
                safe_cex_exchange::js_runtime::ops::subtract(&arg(&args, 0), &arg(&args, 1))
            }
            "add" | "addition" => {
                safe_cex_exchange::js_runtime::ops::add(&arg(&args, 0), &arg(&args, 1))
            }
            "multiply" => {
                safe_cex_exchange::js_runtime::ops::multiply(&arg(&args, 0), &arg(&args, 1))
            }
            "divide" => safe_cex_exchange::js_runtime::ops::divide(&arg(&args, 0), &arg(&args, 1)),
            "remainder" => Ok(Js::Number(
                arg(&args, 0).to_number()? % arg(&args, 1).to_number()?,
            )),
            "greater" => Ok(Js::Bool(safe_cex_exchange::js_runtime::ops::greater_than(
                &arg(&args, 0),
                &arg(&args, 1),
            )?)),
            "lessEqual" => Ok(Js::Bool(safe_cex_exchange::js_runtime::ops::less_equal(
                &arg(&args, 0),
                &arg(&args, 1),
            )?)),
            "keyGet" | "computed property" => arg(&args, 0).get_key(&arg(&args, 1).to_js_string()?),
            "property key" => Ok(Js::String(arg(&args, 0).to_js_string()?)),
            "computed assignment" => {
                let object = arg(&args, 0);
                object.set_key(&arg(&args, 1).to_js_string()?, arg(&args, 2))?;
                Ok(object)
            }
            "suffixTemplate" => safe_cex_exchange::js_runtime::ops::add(
                &Js::String(arg(&args, 0).to_js_string()?),
                &arg(&args, 1),
            ),
            "has" => Ok(Js::Bool(arg(&args, 0).has_own(&text(&arg(&args, 1))?))),
            "arraySpread" => {
                let mut out = vec![];
                for value in args {
                    out.extend(value.iterable_values()?);
                }
                Ok(Js::array(out))
            }
            "capturedMember" => {
                let selected = arg(&args, 1);
                selected.define(OWNER, arg(&args, 0))?;
                selected.define(NAME, Js::str("map"))?;
                Ok(selected)
            }
            "Array.isArray" => self
                .shared
                .globals
                .get("Array")?
                .call_method("isArray", args),
            "destructure first" => {
                let values = arg(&args, 0)
                    .array_values()
                    .ok_or_else(|| Thrown::type_error("value is not iterable"))?;
                Ok(values.first().cloned().unwrap_or(Js::Undefined))
            }
            "iterator" => arg(&args, 0).iterator(),
            "IteratorClose normal" => {
                let iterator = arg(&args, 0);
                let method = iterator.get("return")?;
                if !method.is_nullish() {
                    let result = method.call(iterator, vec![])?;
                    if !result.is_object() {
                        return Err(Thrown::type_error("Iterator result is not an object"));
                    }
                }
                Ok(arg(&args, 1))
            }
            "IteratorClose abrupt" => {
                let iterator = arg(&args, 0);
                if let Ok(method) = iterator.get("return") {
                    if !method.is_nullish() {
                        let _ = method.call(iterator, vec![]);
                    }
                }
                Err(Thrown::new(arg(&args, 1)))
            }
            "destructure2" => {
                let values = arg(&args, 0)
                    .array_values()
                    .ok_or_else(|| Thrown::type_error("value is not iterable"))?;
                Ok(Js::array(vec![
                    values.first().cloned().unwrap_or(Js::Undefined),
                    values.get(1).cloned().unwrap_or(Js::Undefined),
                ]))
            }
            "greaterEqual" => Ok(Js::Bool(safe_cex_exchange::js_runtime::ops::greater_equal(
                &arg(&args, 0),
                &arg(&args, 1),
            )?)),
            "intervalRegex" => safe_cex_exchange::js_runtime::regexp::regexp(
                &r"(\d+)([a-zA-Z]+)".encode_utf16().collect::<Vec<_>>(),
                &[],
            ),
            "missingTicker" => {
                let mut message: Vec<u16> = "Ticker ".encode_utf16().collect();
                message.extend_from_slice(&arg(&args, 0).to_js_string()?);
                message.extend(" not found".encode_utf16());
                Ok(Js::utf16(&message))
            }
            "missingMarket" => {
                let mut message: Vec<u16> = "Market ".encode_utf16().collect();
                message.extend_from_slice(&arg(&args, 0).to_js_string()?);
                message.extend(" not found".encode_utf16());
                Ok(Js::utf16(&message))
            }
            "virtualClock" | "Date" | "Math" | "ENDPOINTS" | "JSON" | "LogSeverity" => {
                self.shared.globals.get(name)
            }
            // Object.assign(target, source) over own enumerable data properties.
            "copyOwnDataProperties" => {
                let target = arg(&args, 0);
                for (key, value) in arg(&args, 1).own_entries()? {
                    target.set_key(&key, value)?;
                }
                Ok(target)
            }
            other => crate::core_snapshot_readers::SnapshotHost::intrinsic(self, other, args),
        }
    }
    fn allocate(&self, parent: Js) -> Result<Js, Thrown> {
        let object = Js::object(vec![]);
        if let Js::Object(data) = &object {
            *data.proto.borrow_mut() = Some(parent);
        }
        Ok(object)
    }
    fn callback(&self, owner: Js, code: &'static str, captures: Vec<Js>) -> Result<Js, Thrown> {
        if matches!(
            code,
            "order"
                | "connect"
                | "close"
                | "ping"
                | "message"
                | "timeout"
                | "current"
                | "reconnect"
        ) {
            return super::hyro_socket::callback(self, owner, code, captures);
        }
        crate::core_snapshot_readers::SnapshotHost::callback(self, owner, code, captures)
    }
    fn promise(&self) -> Result<(Js, Js, Js), Thrown> {
        let promise = Promise::pending(&self.exchange_runtime());
        let (resolve, reject) = (promise.clone(), promise.clone());
        Ok((
            Js::Promise(promise),
            Js::function("resolve", move |_, a| {
                resolve.settle(Ok(arg(&a, 0)));
                Ok(Js::Undefined)
            }),
            Js::function("reject", move |_, a| {
                reject.settle(Err(Thrown::new(arg(&a, 0))));
                Ok(Js::Undefined)
            }),
        ))
    }
    fn error(&self, message: Js) -> Result<Thrown, Thrown> {
        let message = if message.is_undefined() {
            String::new()
        } else {
            text(&message)?
        };
        Ok(Thrown::new(error_object("Error", &message)))
    }
    fn error_value(&self, error: Thrown) -> Js {
        error.value
    }
    fn normalize_error(&self, error: Thrown) -> Result<Js, Thrown> {
        Ok(error.value)
    }
}

impl SpotOrderHost for Rt {
    fn spread(&self, value: Js) -> Result<Js, Thrown> {
        if value.is_nullish() {
            Ok(Js::object(vec![]))
        } else {
            value.spread_object()
        }
    }
    fn map<F>(&self, method: Js, mut body: F) -> Result<Js, Thrown>
    where
        F: FnMut(Js, usize) -> Result<Js, Thrown>,
    {
        let items = selected_owner(&method, "map")?;
        let mut out = Vec::with_capacity(items.len());
        for (i, item) in items.into_iter().enumerate() {
            out.push(body(item, i)?);
        }
        Ok(Js::array(out))
    }
    fn filter<F>(&self, method: Js, mut body: F) -> Result<Js, Thrown>
    where
        F: FnMut(Js, usize) -> Result<bool, Thrown>,
    {
        let mut out = vec![];
        for (i, item) in selected_owner(&method, "filter")?.into_iter().enumerate() {
            if body(item.clone(), i)? {
                out.push(item);
            }
        }
        Ok(Js::array(out))
    }
    fn some<F>(&self, method: Js, mut body: F) -> Result<bool, Thrown>
    where
        F: FnMut(Js, usize) -> Result<bool, Thrown>,
    {
        for (i, item) in selected_owner(&method, "some")?.into_iter().enumerate() {
            if body(item, i)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn find<F>(&self, method: Js, mut body: F) -> Result<Js, Thrown>
    where
        F: FnMut(Js, usize) -> Result<bool, Thrown>,
    {
        for (i, item) in selected_owner(&method, "find")?.into_iter().enumerate() {
            if body(item.clone(), i)? {
                return Ok(item);
            }
        }
        Ok(Js::Undefined)
    }
    fn reduce<F>(&self, method: Js, initial: Js, mut body: F) -> Result<Js, Thrown>
    where
        F: FnMut(Js, Js) -> Result<Js, Thrown>,
    {
        let mut acc = initial;
        for item in selected_owner(&method, "reduce")? {
            acc = body(acc, item)?;
        }
        Ok(acc)
    }
    fn for_of<F>(&self, values: Js, mut body: F) -> Result<(), Thrown>
    where
        F: FnMut(Js) -> Result<(), Thrown>,
    {
        let iterator = values.iterator()?;
        let next = iterator.get("next")?;
        loop {
            let step = next.call(iterator.clone(), vec![])?;
            if !step.is_object() {
                return Err(Thrown::type_error("Iterator result is not an object"));
            }
            if step.get("done")?.truthy() {
                break;
            }
            body(step.get("value")?)?;
        }
        Ok(())
    }
    fn series<'a, F>(
        &'a self,
        _callee: Js,
        values: Js,
        mut body: F,
    ) -> SessionFuture<'a, Js, Thrown>
    where
        F: FnMut(Js, usize) -> SessionFuture<'a, Js, Thrown> + 'a,
    {
        Box::pin(async move {
            let items = values.array_values().ok_or_else(|| {
                Thrown::type_error(&format!("{} is not iterable", values.to_display()))
            })?;
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.into_iter().enumerate() {
                let item = await_js(item).await?;
                out.push(body(item, i).await?);
            }
            Ok(Js::array(out))
        })
    }
}

impl crate::variant_account_refresh::AccountRefreshHost for Rt {
    fn spread(&self, base: Js, fields: Vec<(&'static str, Js)>) -> Result<Js, Thrown> {
        let out = <Self as SpotOrderHost>::spread(self, base)?;
        for (key, value) in fields {
            out.set(key, value)?;
        }
        Ok(out)
    }
    fn then(&self, promise: Js, fulfilled: Js) -> Result<Js, Thrown> {
        promise.call_method("then", vec![fulfilled])
    }
    fn all(&self, promises: Vec<Js>) -> Result<Js, Thrown> {
        Ok(Js::Promise(super::promise::all(
            &self.exchange_runtime(),
            promises,
        )))
    }
}

impl crate::spot_exchange_reads::SpotExchangeHost for Rt {
    fn destructure(&self, value: Js, skip: usize, count: usize) -> Result<Vec<Js>, Thrown> {
        let iterator = value.iterator()?;
        let next = iterator.get("next")?;
        let mut done = false;
        let mut values = Vec::with_capacity(count);
        for index in 0..skip + count {
            let value = if done {
                Js::Undefined
            } else {
                let step = next.call(iterator.clone(), vec![])?;
                if !step.is_object() {
                    return Err(Thrown::type_error("Iterator result is not an object"));
                }
                done = step.get("done")?.truthy();
                if done || index < skip {
                    Js::Undefined
                } else {
                    step.get("value")?
                }
            };
            if index >= skip {
                values.push(value);
            }
        }
        if !done {
            let close = iterator.get("return")?;
            if !close.is_nullish() && !close.call(iterator.clone(), vec![])?.is_object() {
                return Err(Thrown::type_error("Iterator result is not an object"));
            }
        }
        Ok(values)
    }
    fn for_each<F>(&self, method: Js, mut body: F) -> Result<(), Thrown>
    where
        F: FnMut(Js, usize) -> Result<(), Thrown>,
    {
        for (index, value) in selected_owner(&method, "forEach")?.into_iter().enumerate() {
            body(value, index)?;
        }
        Ok(())
    }
    fn times<F>(&self, callee: Js, count: Js, mut body: F) -> Result<Js, Thrown>
    where
        F: FnMut(Js) -> Result<Js, Thrown>,
    {
        // Invoke the shared imported lodash operation; this host only bridges its
        // synchronous callback into the borrowed generic body.
        let indices = callee.call(
            Js::Undefined,
            vec![
                count,
                Js::function("timesIndex", |_, args| Ok(arg(&args, 0))),
            ],
        )?;
        let mut out = vec![];
        for index in indices
            .array_values()
            .ok_or_else(|| Thrown::type_error("times result is not iterable"))?
        {
            out.push(body(index)?);
        }
        Ok(Js::array(out))
    }
    fn new_error(&self, constructor: Js, message: Js) -> Result<Thrown, Thrown> {
        Ok(Thrown::new(constructor.call(Js::Undefined, vec![message])?))
    }
    fn key_set(&self, object: &Js, key: Js, value: Js) -> Result<(), Thrown> {
        object.set_key(&key.to_js_string()?, value)
    }
}
