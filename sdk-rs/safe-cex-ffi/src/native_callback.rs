use crate::native_value::{JsData, check_status};
use napi::{
    Env, JsValue, Status, ValueType,
    bindgen_prelude::{
        Array, CallbackContext, FromNapiValue, Function, JsObjectValue, Object, PromiseRaw,
        ToNapiValue, TypeName, Unknown,
    },
    sys,
    threadsafe_function::ThreadsafeFunction,
};
use safe_cex_registry::native_value::NativeValue;
use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
};
use tokio::sync::oneshot;

pub type Callback<'env> = Function<'env, JsData, Unknown<'static>>;
pub type Request = ThreadsafeFunction<JsData, CallbackFuture, JsData, Status, false>;
type Outcome = std::result::Result<JsData, NativeValue>;

fn capture_error(env: &Env, value: Unknown<'_>) -> napi::Result<NativeValue> {
    let mut fields = vec![];
    if value.get_type()? == ValueType::Object {
        let object = Object::from_raw(env.raw(), value.raw());
        for key in ["name", "message"] {
            let field: Unknown = object.get_named_property(key)?;
            let value = if field.get_type()? == ValueType::Undefined {
                NativeValue::string(if key == "name" { "Error" } else { "" })
            } else {
                let text = field.coerce_to_string()?;
                unsafe { JsData::from_napi_value(env.raw(), text.raw())? }.0
            };
            fields.push((key, value));
        }
        if object.has_named_property("stack")? {
            let stack: Unknown = object.get_named_property("stack")?;
            fields.push((
                "stack",
                unsafe { JsData::from_napi_value(env.raw(), stack.raw())? }.0,
            ));
        }
        if object.has_named_property("code")? {
            let code: Unknown = object.get_named_property("code")?;
            fields.push((
                "code",
                unsafe { JsData::from_napi_value(env.raw(), code.raw())? }.0,
            ));
        }
    } else {
        let text = value.coerce_to_string()?;
        fields.push(("name", NativeValue::string("Error")));
        fields.push((
            "message",
            unsafe { JsData::from_napi_value(env.raw(), text.raw())? }.0,
        ));
    }
    Ok(NativeValue::object(fields))
}

fn capture_rejection(env: &Env, mut value: Unknown<'_>) -> napi::Result<NativeValue> {
    for _ in 0..8 {
        match capture_error(env, value) {
            Ok(error) => return Ok(error),
            Err(error) => {
                let mut pending = false;
                check_status(unsafe { sys::napi_is_exception_pending(env.raw(), &mut pending) })?;
                if !pending {
                    return Ok(NativeValue::object([
                        ("name", NativeValue::string("Error")),
                        ("message", NativeValue::string(&error.reason)),
                    ]));
                }
                let mut exception = std::ptr::null_mut();
                check_status(unsafe {
                    sys::napi_get_and_clear_last_exception(env.raw(), &mut exception)
                })?;
                value = unsafe { Unknown::from_napi_value(env.raw(), exception)? };
            }
        }
    }
    Ok(NativeValue::object([
        ("name", NativeValue::string("Error")),
        (
            "message",
            NativeValue::string("callback rejection inspection exceeds 8 nested exceptions"),
        ),
    ]))
}

pub fn wrap(env: &Env, callback: Callback<'_>) -> napi::Result<Request> {
    let reference = callback.create_ref()?;
    let wrapper = env.create_function_from_closure::<JsData, Unknown<'static>, _>(
        "nativeAccountRequest",
        move |context| {
            let result = reference
                .borrow_back(context.env)?
                .call(context.first_arg()?);
            let mut envelope = context.env.create_array(2)?;
            match result {
                Ok(value) => {
                    envelope.set(0, true)?;
                    envelope.set(1, value)?;
                }
                Err(error) => {
                    let value = unsafe { napi::Error::to_napi_value(context.env.raw(), error)? };
                    let value = unsafe { Unknown::from_napi_value(context.env.raw(), value)? };
                    envelope.set(0, false)?;
                    envelope.set(1, JsData(capture_rejection(context.env, value)?))?;
                }
            }
            unsafe { Unknown::from_napi_value(context.env.raw(), envelope.raw()) }
        },
    )?;
    let wrapper =
        unsafe { Function::<JsData, CallbackFuture>::from_napi_value(env.raw(), wrapper.raw())? };
    wrapper
        .build_threadsafe_function::<JsData>()
        .callee_handled::<false>()
        .build()
}

// Closing the account drops the receiver; late JS settlement owns no host work.
pub struct CallbackFuture(oneshot::Receiver<Outcome>);
impl TypeName for CallbackFuture {
    fn type_name() -> &'static str {
        "native callback result"
    }
    fn value_type() -> ValueType {
        ValueType::Unknown
    }
}
impl FromNapiValue for CallbackFuture {
    unsafe fn from_napi_value(env: sys::napi_env, value: sys::napi_value) -> napi::Result<Self> {
        let envelope = unsafe { Array::from_napi_value(env, value)? };
        let success: bool = envelope
            .get(0)?
            .ok_or_else(|| napi::Error::from_reason("missing callback outcome"))?;
        let value: Unknown = envelope
            .get(1)?
            .ok_or_else(|| napi::Error::from_reason("missing callback value"))?;
        let (sender, receiver) = oneshot::channel();
        if !success {
            let error = unsafe { JsData::from_napi_value(env, value.raw())? }.0;
            drop(sender.send(Err(error)));
        } else {
            let mut promise = false;
            check_status(unsafe { sys::napi_is_promise(env, value.raw(), &mut promise) })?;
            if promise {
                let sender = Rc::new(RefCell::new(Some(sender)));
                let fulfilled = sender.clone();
                let promise = unsafe { PromiseRaw::<JsData>::from_napi_value(env, value.raw())? };
                promise
                    .then(move |context| {
                        if let Some(sender) = fulfilled.borrow_mut().take() {
                            drop(sender.send(Ok(context.value)));
                        }
                        Ok(())
                    })?
                    .catch(move |context: CallbackContext<Unknown>| {
                        let outcome = capture_rejection(&context.env, context.value)
                            .unwrap_or_else(|error| {
                                NativeValue::object([
                                    ("name", NativeValue::string("Error")),
                                    ("message", NativeValue::string(&error.reason)),
                                ])
                            });
                        if let Some(sender) = sender.borrow_mut().take() {
                            drop(sender.send(Err(outcome)));
                        }
                        Ok(())
                    })?;
            } else {
                let value = unsafe { JsData::from_napi_value(env, value.raw())? };
                drop(sender.send(Ok(value)));
            }
        }
        Ok(Self(receiver))
    }
}
impl Future for CallbackFuture {
    type Output = Outcome;
    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.0).poll(context).map(|result| {
            result.unwrap_or_else(|_| {
                Err(NativeValue::object([
                    ("name", NativeValue::string("Error")),
                    (
                        "message",
                        NativeValue::string("native callback dropped its result"),
                    ),
                ]))
            })
        })
    }
}
