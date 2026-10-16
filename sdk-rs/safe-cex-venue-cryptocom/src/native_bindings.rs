//! Language-level bindings used by the owning classes; no application member provider.
use crate::{
    rest_reads::ReadHost,
    socket_handlers::{SocketFuture, SocketOps},
};
use std::{
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
};

pub struct BindingOps<V, E> {
    pub null: Rc<dyn Fn() -> V>,
    pub is_string: Rc<dyn Fn(&V) -> bool>,
    pub value: Rc<SocketOps<V, E>>,
    pub binary: Rc<dyn Fn(&str, V, V) -> Result<V, E>>,
    pub construct: Rc<dyn Fn(V, Vec<V>) -> Result<V, E>>,
    pub throw_value: Rc<dyn Fn(V) -> E>,
    pub promise: Rc<dyn Fn(SocketFuture<V, E>) -> V>,
    /// Native GetIterator/IteratorStep/IteratorClose operations retain the live record.
    pub iterator: Rc<dyn Fn(V) -> Result<V, E>>,
    pub next: Rc<dyn Fn(&V) -> Result<Option<V>, E>>,
    pub close: Rc<dyn Fn(V) -> Result<(), E>>,
    pub close_throw: Rc<dyn Fn(V, E) -> E>,
}
pub struct BoundHost<V, E> {
    pub ops: Rc<BindingOps<V, E>>,
    pub receiver: V,
}
pub type Prepared<V> = (V, V);
pub struct Awaited<V, E> {
    original: Rc<V>,
    future: SocketFuture<V, E>,
}
impl<V, E> Future for Awaited<V, E> {
    type Output = Result<V, E>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.get_mut().future.as_mut().poll(cx)
    }
}
impl<V: Clone + 'static, E: 'static> BoundHost<V, E> {
    pub fn get(&self, object: &V, key: &str) -> Result<V, E> {
        (self.ops.value.get)(object, key)
    }
    pub fn prepare(&self, receiver: V, key: &str) -> Result<Prepared<V>, E> {
        Ok((self.get(&receiver, key)?, receiver))
    }
    pub fn call(&self, prepared: Prepared<V>, args: Vec<V>) -> Result<V, E> {
        (self.ops.value.invoke)(prepared.0, prepared.1, args)
    }
    pub fn global(&self, name: &str, args: Vec<V>) -> Result<V, E> {
        let f = (self.ops.value.global)(name)?;
        (self.ops.value.invoke)(f, (self.ops.value.undefined)(), args)
    }
    pub fn wait(&self, value: V) -> Awaited<V, E> {
        Awaited {
            original: Rc::new(value.clone()),
            future: (self.ops.value.await_value)(value),
        }
    }
    pub fn method(&self, receiver: V, key: &str, args: Vec<V>) -> Result<V, E> {
        self.call(self.prepare(receiver, key)?, args)
    }
    pub fn math(&self, key: &str, args: Vec<V>) -> Result<V, E> {
        let receiver = (self.ops.value.global)("Math")?;
        self.method(receiver, key, args)
    }
    pub fn each<F: FnMut(V) -> Result<(), E>>(
        &self,
        iterable: V,
        mut callback: F,
    ) -> Result<(), E> {
        let record = (self.ops.iterator)(iterable)?;
        while let Some(value) = (self.ops.next)(&record)? {
            if let Err(error) = callback(value) {
                return Err((self.ops.close_throw)(record, error));
            }
        }
        Ok(())
    }
    fn mapped<F: Fn(V) -> Result<V, E> + 'static>(
        &self,
        array: V,
        key: &str,
        callback: F,
    ) -> Result<V, E> {
        let method = self.prepare(array, key)?;
        let undefined = self.ops.value.undefined.clone();
        let callback = (self.ops.value.function)(
            "",
            1,
            Rc::new(move |args| callback(args.first().cloned().unwrap_or_else(|| undefined()))),
        );
        self.call(method, vec![callback])
    }
}
impl<V: Clone + 'static, E: Clone + 'static> ReadHost for BoundHost<V, E> {
    type Value = V;
    type Error = E;
    type Prepared = Prepared<V>;
    type PostFuture = Awaited<V, E>;
    type ModeFuture = Awaited<V, E>;
    fn root(&self) -> V {
        self.receiver.clone()
    }
    fn undefined(&self) -> V {
        (self.ops.value.undefined)()
    }
    fn number(&self, value: f64) -> V {
        (self.ops.value.number)(value)
    }
    fn boolean(&self, value: bool) -> V {
        (self.ops.value.boolean)(value)
    }
    fn string(&self, value: &'static str) -> V {
        (self.ops.value.literal)(value)
    }
    fn object(&self, fields: Vec<(&'static str, V)>) -> V {
        (self.ops.value.object)(fields)
    }
    fn array(&self, values: Vec<V>) -> V {
        (self.ops.value.array)(values)
    }
    fn get(&self, value: &V, key: &'static str) -> Result<V, E> {
        self.get(value, key)
    }
    fn index(&self, value: &V, index: usize) -> Result<V, E> {
        (self.ops.value.index)(value, &self.number(index as f64))
    }
    fn truthy(&self, value: &V) -> bool {
        (self.ops.value.truthy)(value)
    }
    fn nullish(&self, value: &V) -> bool {
        (self.ops.value.nullish)(value)
    }
    fn equal_string(&self, value: &V, text: &'static str) -> bool {
        (self.ops.value.equal)(value, &self.string(text))
    }
    fn zero_number(&self, value: &V) -> bool {
        (self.ops.value.equal)(value, &self.number(0.))
    }
    fn greater_zero(&self, value: V) -> Result<bool, E> {
        Ok(self.truthy(&(self.ops.binary)(">", value, self.number(0.))?))
    }
    fn divide(&self, value: V, denominator: f64) -> Result<f64, E> {
        (self.ops.value.numeric)(&(self.ops.binary)("/", value, self.number(denominator))?)
    }
    fn prepare(&self, receiver: &V, key: &'static str) -> Result<Prepared<V>, E> {
        self.prepare(receiver.clone(), key)
    }
    fn call(&self, prepared: Prepared<V>, args: Vec<V>) -> Result<V, E> {
        self.call(prepared, args)
    }
    fn endpoint(&self, key: &'static str) -> Result<V, E> {
        self.get(&(self.ops.value.global)("ENDPOINTS")?, key)
    }
    fn post(&self, prepared: Prepared<V>, endpoint: V, params: V) -> Result<Awaited<V, E>, E> {
        Ok(self.wait(self.call(prepared, vec![endpoint, params])?))
    }
    fn prepare_all(&self) -> Result<Prepared<V>, E> {
        self.prepare((self.ops.value.global)("Promise")?, "all")
    }
    fn all(&self, prepared: Prepared<V>, futures: Vec<Awaited<V, E>>) -> Result<Awaited<V, E>, E> {
        let values = futures
            .into_iter()
            .map(|f| f.original.as_ref().clone())
            .collect();
        Ok(self.wait(self.call(prepared, vec![self.array(values)])?))
    }
    fn position_mode(&self, prepared: Prepared<V>) -> Result<Awaited<V, E>, E> {
        Ok(self.wait(self.call(prepared, vec![])?))
    }
    fn imported(&self, module: &'static str, key: &'static str) -> Result<V, E> {
        self.get(&(self.ops.value.global)(module)?, key)
    }
    fn prepare_parse_float(&self) -> Result<Prepared<V>, E> {
        Ok(((self.ops.value.global)("parseFloat")?, self.undefined()))
    }
    fn call_parse_float(&self, prepared: Prepared<V>, value: V) -> Result<f64, E> {
        (self.ops.value.numeric)(&self.call(prepared, vec![value])?)
    }
    fn prepare_pow(&self) -> Result<Prepared<V>, E> {
        self.prepare((self.ops.value.global)("Math")?, "pow")
    }
    fn pow10_negative(&self, prepared: Prepared<V>, value: V) -> Result<f64, E> {
        let exponent = (self.ops.binary)("unary-", value, self.undefined())?;
        (self.ops.value.numeric)(&self.call(prepared, vec![self.number(10.), exponent])?)
    }
    fn abs(&self, value: V) -> Result<f64, E> {
        (self.ops.value.numeric)(&self.math("abs", vec![value])?)
    }
    fn max_zero(&self, value: f64) -> Result<f64, E> {
        (self.ops.value.numeric)(&self.math("max", vec![self.number(0.), self.number(value)])?)
    }
    fn now(&self) -> Result<V, E> {
        self.method(
            (self.ops.value.global)("virtualClock")?,
            "getCurrentTimeMs",
            vec![],
        )
    }
    fn template_string(&self, value: V) -> Result<V, E> {
        Ok((self.ops.value.utf16)((self.ops.value.text)(&value)?))
    }
    fn property_key(&self, value: V) -> Result<V, E> {
        (self.ops.value.key)(&value)
    }
    fn iterator_pair<F: FnMut(V) -> Result<V, E>>(
        &self,
        value: V,
        mut bind: F,
    ) -> Result<(V, V), E> {
        let record = (self.ops.iterator)(value)?;
        let mut done = false;
        let pair = (|| {
            let mut one = || -> Result<V, E> {
                let value = if done {
                    self.undefined()
                } else {
                    // IteratorStepValue marks Done on every abrupt completion.
                    // Binding errors, unlike step errors, close a live iterator.
                    match (self.ops.next)(&record) {
                        Ok(Some(value)) => value,
                        Ok(None) => {
                            done = true;
                            self.undefined()
                        }
                        Err(error) => {
                            done = true;
                            return Err(error);
                        }
                    }
                };
                bind(value)
            };
            Ok((one()?, one()?))
        })();
        match pair {
            Ok(pair) => {
                if !done {
                    (self.ops.close)(record)?;
                }
                Ok(pair)
            }
            Err(error) if !done => Err((self.ops.close_throw)(record, error)),
            Err(error) => Err(error),
        }
    }
    fn ticker_id(&self, instrument: V, time: V) -> Result<V, E> {
        Ok((self.ops.value.utf16)(
            (self.ops.value.text)(&instrument)?
                .into_iter()
                .chain([45])
                .chain((self.ops.value.text)(&time)?)
                .collect(),
        ))
    }
    fn interval(&self, interval: V) -> Result<V, E> {
        self.global("mapCandlestickInterval", vec![interval])
    }
    fn create_position_id(&self, symbol: V, side: V, mode: V) -> Result<V, E> {
        self.global("createPositionId", vec![symbol, side, mode])
    }
    fn map<F: Fn(V) -> Result<V, E> + 'static>(&self, array: V, callback: F) -> Result<V, E> {
        self.mapped(array, "map", callback)
    }
    fn filter<F: Fn(V) -> Result<V, E> + 'static>(&self, array: V, callback: F) -> Result<V, E> {
        self.mapped(array, "filter", callback)
    }
    fn spread(&self, array: V) -> Result<Vec<V>, E> {
        (self.ops.value.spread)(&array)
    }
    fn each<F: FnMut(V) -> Result<(), E>>(&self, iterable: V, callback: F) -> Result<(), E> {
        self.each(iterable, callback)
    }
    fn prepare_set(&self) -> Result<Prepared<V>, E> {
        Ok(((self.ops.value.global)("Set")?, self.undefined()))
    }
    fn set(&self, prepared: Prepared<V>, array: V) -> Result<V, E> {
        (self.ops.construct)(prepared.0, vec![array])
    }
    fn uniq_by(&self, array: V, key: &'static str) -> Result<V, E> {
        self.global("uniqBy", vec![array, self.string(key)])
    }
    fn assign(&self, object: &V, key: V, value: V) -> Result<(), E> {
        (self.ops.value.set_index)(object, &key, value)
    }
    fn error_value(&self, error: &E) -> V {
        (self.ops.value.error_value)(error.clone())
    }
    fn error_template(&self, prefix: &'static str, error: V) -> Result<V, E> {
        Ok((self.ops.value.utf16)(
            prefix
                .encode_utf16()
                .chain((self.ops.value.text)(&error)?)
                .collect(),
        ))
    }
    fn severity_error(&self) -> Result<V, E> {
        self.get(&(self.ops.value.global)("LogSeverity")?, "Error")
    }
}
impl<V: Clone + 'static, E: Clone + 'static> crate::order_writes::WriteHost for BoundHost<V, E> {
    type Value = V;
    type Error = E;
    type Prepared = Prepared<V>;
    type Iterator = V;
    type PostFuture = Awaited<V, E>;
    type CallFuture = Awaited<V, E>;
    type SeriesFuture = Awaited<V, E>;
    fn root(&self) -> V {
        self.receiver.clone()
    }
    fn undefined(&self) -> V {
        (self.ops.value.undefined)()
    }
    fn string(&self, text: &'static str) -> V {
        (self.ops.value.literal)(text)
    }
    fn object(&self, fields: Vec<(&'static str, V)>) -> V {
        (self.ops.value.object)(fields)
    }
    fn array(&self, values: Vec<V>) -> V {
        (self.ops.value.array)(values)
    }
    fn get(&self, value: &V, key: &'static str) -> Result<V, E> {
        self.get(value, key)
    }
    fn get_key(&self, value: V, key: V) -> Result<V, E> {
        (self.ops.value.index)(&value, &key)
    }
    fn assign(&self, object: &V, key: &'static str, value: V) -> Result<(), E> {
        (self.ops.value.set)(object, key, value)
    }
    fn has(&self, object: V, key: &'static str) -> Result<bool, E> {
        (self.ops.value.has)(&object, key)
    }
    fn truthy(&self, value: &V) -> bool {
        (self.ops.value.truthy)(value)
    }
    fn nullish(&self, value: &V) -> bool {
        (self.ops.value.nullish)(value)
    }
    fn is_undefined(&self, value: &V) -> bool {
        (self.ops.value.equal)(value, &(self.ops.value.undefined)())
    }
    fn equal_string(&self, value: &V, text: &'static str) -> bool {
        (self.ops.value.equal)(value, &(self.ops.value.literal)(text))
    }
    fn prepare(&self, receiver: &V, key: &'static str) -> Result<Prepared<V>, E> {
        self.prepare(receiver.clone(), key)
    }
    fn call(&self, prepared: Prepared<V>, args: Vec<V>) -> Result<V, E> {
        self.call(prepared, args)
    }
    fn call_async(&self, prepared: Prepared<V>, args: Vec<V>) -> Result<Awaited<V, E>, E> {
        Ok(self.wait(self.call(prepared, args)?))
    }
    fn endpoint(&self, key: &'static str) -> Result<V, E> {
        self.get(&(self.ops.value.global)("ENDPOINTS")?, key)
    }
    fn post(&self, prepared: Prepared<V>, endpoint: V, params: V) -> Result<Awaited<V, E>, E> {
        Ok(self.wait(self.call(prepared, vec![endpoint, params])?))
    }
    fn is_finite(&self, value: V) -> Result<bool, E> {
        Ok((self.ops.value.truthy)(&self.method(
            (self.ops.value.global)("Number")?,
            "isFinite",
            vec![value],
        )?))
    }
    fn less_equal_zero(&self, value: V) -> Result<bool, E> {
        Ok((self.ops.value.truthy)(&(self.ops.binary)(
            "<=",
            value,
            (self.ops.value.number)(0.),
        )?))
    }
    fn prepare_error(&self) -> Result<Prepared<V>, E> {
        Ok((
            (self.ops.value.global)("Error")?,
            (self.ops.value.undefined)(),
        ))
    }
    fn template(&self, prefix: &'static str, value: V) -> Result<V, E> {
        Ok((self.ops.value.utf16)(
            prefix
                .encode_utf16()
                .chain((self.ops.value.text)(&value)?)
                .collect(),
        ))
    }
    fn construct_error(&self, prepared: Prepared<V>, message: V) -> Result<E, E> {
        Ok((self.ops.throw_value)((self.ops.construct)(
            prepared.0,
            vec![message],
        )?))
    }
    fn error_value(&self, error: &E) -> V {
        (self.ops.value.error_value)(error.clone())
    }
    fn severity_error(&self) -> Result<V, E> {
        self.get(&(self.ops.value.global)("LogSeverity")?, "Error")
    }
    fn spread(&self, iterable: V) -> Result<Vec<V>, E> {
        (self.ops.value.spread)(&iterable)
    }
    fn iterator(&self, iterable: V) -> Result<V, E> {
        (self.ops.iterator)(iterable)
    }
    fn next(&self, iterator: &mut V) -> Result<Option<V>, E> {
        (self.ops.next)(iterator)
    }
    fn close_throw(&self, iterator: V, error: E) -> E {
        (self.ops.close_throw)(iterator, error)
    }
    fn series<F>(&self, orders: V, callback: F) -> Result<Awaited<V, E>, E>
    where
        F: Fn(V) -> crate::order_writes::BoxFuture<V, E> + 'static,
    {
        let callee = (self.ops.value.global)("forEachSeries")?;
        let callback = Rc::new(callback);
        let ops = self.ops.clone();
        let function = (self.ops.value.function)(
            "",
            1,
            Rc::new(move |args| {
                let value = args
                    .first()
                    .cloned()
                    .unwrap_or_else(|| (ops.value.undefined)());
                // The eager callback factory may reenter this retained function.
                let future = callback(value);
                Ok((ops.promise)(future))
            }),
        );
        let original =
            (self.ops.value.invoke)(callee, (self.ops.value.undefined)(), vec![orders, function])?;
        Ok(self.wait(original))
    }
}

#[cfg(test)]
#[path = "bridge_tests.rs"]
mod tests;
