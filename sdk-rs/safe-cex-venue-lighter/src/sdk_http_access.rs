//! Nine API-client access bodies at required owner/Axios/language host boundaries.
use crate::{
    sdk_account_api::{AccountSemantics, AccountValue},
    transaction_promise::{NativePromise, TransactionRuntime},
    transaction_signing::SigningResult,
};
use std::rc::Rc;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpVerb {
    Get,
    Post,
    Put,
    Delete,
}
/// Actual captured callee AND receiver, including deferred noncallability.
pub trait AccessCall {
    fn call(&self, args: &[AccountValue]) -> SigningResult<AccountValue>;
}
pub trait HttpCall {
    /// Invoke actual Axios callee/receiver then perform source Await normalization
    /// of its return (PromiseResolve/Get(then)/thenable adoption with original
    /// exceptions). Subscriber continuations must run on the supplied FIFO queue.
    fn call(&self, args: &[AccountValue]) -> SigningResult<NativePromise<AccountValue>>;
}
pub trait HttpAccessHost {
    fn runtime(&self) -> Rc<dyn TransactionRuntime>;
    fn semantics(&self) -> Rc<dyn AccountSemantics>;
    /// Capture current axiosInstance and method callee before argument evaluation.
    fn http_callback(&self, verb: HttpVerb) -> SigningResult<Rc<dyn HttpCall>>;
    fn interceptors(&self) -> SigningResult<AccountValue>;
    fn property(&self, value: &AccountValue, name: &str) -> SigningResult<AccountValue>;
    fn set_property(
        &self,
        value: &AccountValue,
        name: &str,
        new_value: AccountValue,
    ) -> SigningResult<()>;
    fn empty_array(&self) -> SigningResult<AccountValue>;
    fn push_callback(&self, value: &AccountValue) -> SigningResult<Rc<dyn AccessCall>>;
    fn config(&self) -> SigningResult<AccountValue>;
    /// Fresh ordinary object + CopyDataProperties: symbols/getters, own enumerable
    /// fields only, no deep clone or setter assignment. Nullish sources are skipped.
    fn copy_data_properties(&self, value: &AccountValue) -> SigningResult<AccountValue>;
}
fn request(
    host: &dyn HttpAccessHost,
    verb: HttpVerb,
    args: &[AccountValue],
) -> NativePromise<AccountValue> {
    let runtime = host.runtime();
    let (promise, resolver) = NativePromise::pending(runtime.clone());
    let next = host.http_callback(verb).and_then(|call| call.call(args));
    match next {
        Err(error) => resolver.settle(Err(error)),
        Ok(next) => {
            let semantics = host.semantics();
            next.subscribe(move |result| {
                let value = result.and_then(|response| semantics.get_property(&response, "data"));
                match value {
                    Err(error) => resolver.settle(Err(error)),
                    Ok(value) => semantics.resolve_return(value, resolver, runtime),
                }
            });
        }
    }
    promise
}
// ts: LighterApiClient.get
pub fn get(
    host: &dyn HttpAccessHost,
    path: Vec<u16>,
    config: AccountValue,
) -> NativePromise<AccountValue> {
    request(host, HttpVerb::Get, &[AccountValue::String(path), config])
}
// ts: LighterApiClient.post
pub fn post(
    host: &dyn HttpAccessHost,
    path: Vec<u16>,
    data: AccountValue,
    config: AccountValue,
) -> NativePromise<AccountValue> {
    request(
        host,
        HttpVerb::Post,
        &[AccountValue::String(path), data, config],
    )
}
// ts: LighterApiClient.put
pub fn put(
    host: &dyn HttpAccessHost,
    path: Vec<u16>,
    data: AccountValue,
    config: AccountValue,
) -> NativePromise<AccountValue> {
    request(
        host,
        HttpVerb::Put,
        &[AccountValue::String(path), data, config],
    )
}
// ts: LighterApiClient.delete
pub fn delete(
    host: &dyn HttpAccessHost,
    path: Vec<u16>,
    config: AccountValue,
) -> NativePromise<AccountValue> {
    request(
        host,
        HttpVerb::Delete,
        &[AccountValue::String(path), config],
    )
}
fn add_interceptor(
    host: &dyn HttpAccessHost,
    name: &str,
    interceptor: AccountValue,
) -> SigningResult<()> {
    // Capture the LHS record before independently evaluating the RHS record/getter.
    let lhs = host.interceptors()?;
    let rhs = host.interceptors()?;
    let current = host.property(&rhs, name)?;
    let next = if current.truthy() {
        current
    } else {
        host.empty_array()?
    };
    host.set_property(&lhs, name, next)?;
    // Reacquire the current owner/array after assignment (setter may be reentrant).
    let current = host.interceptors()?;
    let array = host.property(&current, name)?;
    host.push_callback(&array)?.call(&[interceptor])?;
    Ok(())
}
// ts: LighterApiClient.addRequestInterceptor
pub fn add_request_interceptor(
    host: &dyn HttpAccessHost,
    interceptor: AccountValue,
) -> SigningResult<()> {
    add_interceptor(host, "request", interceptor)
}
// ts: LighterApiClient.addResponseInterceptor
pub fn add_response_interceptor(
    host: &dyn HttpAccessHost,
    interceptor: AccountValue,
) -> SigningResult<()> {
    add_interceptor(host, "response", interceptor)
}
// ts: LighterApiClient.addErrorInterceptor
pub fn add_error_interceptor(
    host: &dyn HttpAccessHost,
    interceptor: AccountValue,
) -> SigningResult<()> {
    add_interceptor(host, "error", interceptor)
}
// ts: LighterApiClient.getBaseUrl
pub fn get_base_url(host: &dyn HttpAccessHost) -> SigningResult<AccountValue> {
    let config = host.config()?;
    host.property(&config, "baseUrl")
}
// ts: LighterApiClient.getConfig
pub fn get_config(host: &dyn HttpAccessHost) -> SigningResult<AccountValue> {
    let config = host.config()?;
    host.copy_data_properties(&config)
}
