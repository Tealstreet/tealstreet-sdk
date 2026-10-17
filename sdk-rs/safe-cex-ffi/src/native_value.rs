use napi::{
    Env, JsValue, ValueType,
    bindgen_prelude::{
        Array, BigInt, FromNapiValue, JsObjectValue, KeyCollectionMode, KeyConversion, KeyFilter,
        Null, Object, ToNapiValue, TypeName, Undefined, Unknown, Utf16String, ValidateNapiValue,
    },
    sys,
};
use safe_cex_registry::native_value::NativeValue;
use std::ptr;

pub(crate) fn check_status(status: sys::napi_status) -> napi::Result<()> {
    match status {
        sys::Status::napi_ok => Ok(()),
        _ => Err(napi::Error::new(napi::Status::from(status), String::new())),
    }
}

#[derive(Clone)]
pub struct JsData(pub NativeValue);
impl TypeName for JsData {
    fn type_name() -> &'static str {
        "unknown"
    }
    fn value_type() -> ValueType {
        ValueType::Unknown
    }
}
impl ValidateNapiValue for JsData {}

unsafe fn capture(
    env: sys::napi_env,
    value: sys::napi_value,
    ancestors: &mut Vec<sys::napi_value>,
) -> napi::Result<NativeValue> {
    let unknown = unsafe { Unknown::from_napi_value(env, value)? };
    Ok(match unknown.get_type()? {
        ValueType::Undefined => NativeValue::Undefined,
        ValueType::Null => NativeValue::Null,
        ValueType::Boolean => NativeValue::Bool(unsafe { bool::from_napi_value(env, value)? }),
        ValueType::Number => NativeValue::Number(unsafe { f64::from_napi_value(env, value)? }),
        ValueType::String => {
            NativeValue::String(unsafe { Utf16String::from_napi_value(env, value)? }.to_vec())
        }
        ValueType::BigInt => {
            let number = unsafe { BigInt::from_napi_value(env, value)? };
            NativeValue::BigInt {
                negative: number.sign_bit,
                words: number.words,
            }
        }
        ValueType::Object => {
            if ancestors.len() >= 512 {
                return Err(napi::Error::from_reason(
                    "native account value nesting exceeds 512",
                ));
            }
            for ancestor in ancestors.iter() {
                let mut same = false;
                check_status(unsafe { sys::napi_strict_equals(env, *ancestor, value, &mut same) })?;
                if same {
                    return Err(napi::Error::from_reason(
                        "native account value contains a cycle",
                    ));
                }
            }
            ancestors.push(value);
            let mut array = false;
            check_status(unsafe { sys::napi_is_array(env, value, &mut array) })?;
            let captured = if array {
                let array = unsafe { Array::from_napi_value(env, value)? };
                let mut values = Vec::with_capacity(array.len() as usize);
                for index in 0..array.len() {
                    let mut item = ptr::null_mut();
                    check_status(unsafe { sys::napi_get_element(env, value, index, &mut item) })?;
                    values.push(unsafe { capture(env, item, ancestors)? });
                }
                NativeValue::Array(values)
            } else {
                let object = Object::from_raw(env, value);
                let keys = object.get_all_property_names(
                    KeyCollectionMode::OwnOnly,
                    KeyFilter::Enumerable,
                    KeyConversion::NumbersToStrings,
                )?;
                let keys = unsafe { Array::from_napi_value(env, keys.raw())? };
                let mut fields = Vec::with_capacity(keys.len() as usize);
                for index in 0..keys.len() {
                    let key = keys.get::<Unknown>(index)?.ok_or_else(|| {
                        napi::Error::from_reason("missing native data property name")
                    })?;
                    if key.get_type()? == ValueType::Symbol {
                        continue;
                    }
                    let name = unsafe { Utf16String::from_napi_value(env, key.raw())? }.to_vec();
                    let mut item = ptr::null_mut();
                    check_status(unsafe {
                        sys::napi_get_property(env, value, key.raw(), &mut item)
                    })?;
                    fields.push((name, unsafe { capture(env, item, ancestors)? }));
                }
                NativeValue::Object(fields)
            };
            ancestors.pop();
            captured
        }
        other => {
            return Err(napi::Error::from_reason(format!(
                "native account values cannot contain {other:?}"
            )));
        }
    })
}
impl FromNapiValue for JsData {
    unsafe fn from_napi_value(env: sys::napi_env, value: sys::napi_value) -> napi::Result<Self> {
        unsafe { capture(env, value, &mut vec![]).map(Self) }
    }
}
impl ToNapiValue for JsData {
    unsafe fn to_napi_value(env: sys::napi_env, value: Self) -> napi::Result<sys::napi_value> {
        match value.0 {
            NativeValue::Undefined => unsafe { Undefined::to_napi_value(env, ()) },
            NativeValue::Null => unsafe { Null::to_napi_value(env, Null) },
            NativeValue::Bool(value) => unsafe { bool::to_napi_value(env, value) },
            NativeValue::Number(value) => unsafe { f64::to_napi_value(env, value) },
            NativeValue::String(value) => unsafe { Utf16String::to_napi_value(env, value.into()) },
            NativeValue::BigInt { negative, words } => unsafe {
                BigInt::to_napi_value(
                    env,
                    BigInt {
                        sign_bit: negative,
                        words,
                    },
                )
            },
            NativeValue::Array(values) => unsafe {
                Vec::<Self>::to_napi_value(env, values.into_iter().map(Self).collect())
            },
            NativeValue::Object(fields) => {
                let bound = Env::from_raw(env);
                let object = Object::new(&bound)?;
                for (key, value) in fields {
                    let key = unsafe { Utf16String::to_napi_value(env, key.into())? };
                    let value = unsafe { Self::to_napi_value(env, Self(value))? };
                    let descriptor = sys::napi_property_descriptor {
                        utf8name: ptr::null(),
                        name: key,
                        method: None,
                        getter: None,
                        setter: None,
                        value,
                        attributes: sys::PropertyAttributes::writable
                            | sys::PropertyAttributes::enumerable
                            | sys::PropertyAttributes::configurable,
                        data: ptr::null_mut(),
                    };
                    check_status(unsafe {
                        sys::napi_define_properties(env, object.raw(), 1, &descriptor)
                    })?;
                }
                Ok(object.raw())
            }
        }
    }
}
