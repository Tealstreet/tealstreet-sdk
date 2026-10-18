//! Source API errors. Native Error allocation/prototypes and value coercion are
//! host primitives; constructor writes and source error-routing live in Rust.
use crate::json_boundaries::JsPayloadHost;
use std::convert::Infallible;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiErrorClass {
    Core,
    Spot,
    Intrinsic,
}
pub trait ApiErrorHost: JsPayloadHost {
    /// Allocate Error(message) with the requested derived class prototype.
    /// Undefined message keeps the Error intrinsic's absent own-message slot.
    fn new_error(
        &mut self,
        class: ApiErrorClass,
        message: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    /// ESNext source parameter-property field: define own writable/enumerable/
    /// configurable code=undefined after super, before its body assignment.
    fn define_field(
        &mut self,
        target: &Self::Value,
        key: &str,
        value: &Self::Value,
    ) -> Result<(), Self::Error>;
    fn set_property(
        &mut self,
        target: &Self::Value,
        key: &str,
        value: &Self::Value,
    ) -> Result<(), Self::Error>;
    /// Primitive source string literal; this is not String(value) coercion.
    fn literal(&self, text: &str) -> Self::Value;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn exception_from_value(&self, value: Self::Value) -> Self::Error;
}
fn construct<H: ApiErrorHost>(
    host: &mut H,
    class: ApiErrorClass,
    code: &H::Value,
    message: &H::Value,
) -> Result<H::Value, H::Error> {
    let native = host.new_error(class, message)?;
    let undefined = host.undefined();
    host.define_field(&native, "code", &undefined)?;
    host.set_property(&native, "code", code)?;
    let name = host.literal("HyperliquidAPIError");
    host.set_property(&native, "name", &name)?;
    Ok(native)
}
fn route<H: ApiErrorHost>(
    host: &mut H,
    class: ApiErrorClass,
    error: &H::Value,
) -> Result<Infallible, H::Error> {
    let response = host.get(error, "response")?;
    let (code, message) = if host.truthy(&response) {
        // Source performs fresh response/data lookups for both arguments.
        let response = host.get(error, "response")?;
        let data = host.get(&response, "data")?;
        let code = host.get(&data, "code")?;
        let code = if host.truthy(&code) {
            code
        } else {
            host.literal("UNKNOWN_ERROR")
        };
        let response = host.get(error, "response")?;
        let data = host.get(&response, "data")?;
        let message = host.get(&data, "message")?;
        let message = if host.truthy(&message) {
            message
        } else {
            host.literal("An unknown error occurred")
        };
        (code, message)
    } else {
        let request = host.get(error, "request")?;
        if host.truthy(&request) {
            (
                host.literal("NETWORK_ERROR"),
                host.literal("No response received from the server"),
            )
        } else {
            (
                host.literal("REQUEST_SETUP_ERROR"),
                host.get(error, "message")?,
            )
        }
    };
    let native = construct(host, class, &code, &message)?;
    Err(host.exception_from_value(native))
}
pub mod core {
    use super::{ApiErrorClass, ApiErrorHost, Infallible, construct, route};
    #[derive(Debug, Clone)]
    pub struct HyperliquidAPIError<V> {
        pub native: V,
    }
    impl<V> HyperliquidAPIError<V> {
        pub fn new<H: ApiErrorHost<Value = V>>(
            host: &mut H,
            code: &V,
            message: &V,
        ) -> Result<Self, H::Error> {
            construct(host, ApiErrorClass::Core, code, message).map(|native| Self { native })
        }
    }
    pub fn handle_api_error<H: ApiErrorHost>(
        host: &mut H,
        error: &H::Value,
    ) -> Result<Infallible, H::Error> {
        route(host, ApiErrorClass::Core, error)
    }
}
pub mod spot {
    use super::{ApiErrorClass, ApiErrorHost, Infallible, construct, route};
    #[derive(Debug, Clone)]
    pub struct HyperliquidAPIError<V> {
        pub native: V,
    }
    impl<V> HyperliquidAPIError<V> {
        pub fn new<H: ApiErrorHost<Value = V>>(
            host: &mut H,
            code: &V,
            message: &V,
        ) -> Result<Self, H::Error> {
            construct(host, ApiErrorClass::Spot, code, message).map(|native| Self { native })
        }
    }
    pub fn handle_api_error<H: ApiErrorHost>(
        host: &mut H,
        error: &H::Value,
    ) -> Result<Infallible, H::Error> {
        route(host, ApiErrorClass::Spot, error)
    }
}
