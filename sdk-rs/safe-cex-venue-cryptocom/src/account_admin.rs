//! Account/configuration bodies with eager invocation and native await continuations.
//! Native futures must be driven by the caller; class/task scheduling adapters remain pending.
use safe_cex_types::PositionMode;
use std::{
    future::{Future, Ready, ready},
    rc::Rc,
};

pub trait ExchangeFields {
    type Error;
    type Flag;
    type PublicReturn;
    type PrivateReturn;
    fn suppress_flag(&self) -> Result<Self::Flag, Self::Error>;
    fn set_disposed(&self) -> Result<(), Self::Error>;
    fn dispose_public(&self) -> Result<Self::PublicReturn, Self::Error>;
    fn dispose_private(&self) -> Result<Self::PrivateReturn, Self::Error>;
}
pub fn suppress_cryptocom_validation_error_emission<T: ExchangeFields>(
    host: &T,
) -> Result<T::Flag, T::Error> {
    host.suppress_flag()
}
pub fn dispose<T: ExchangeFields>(host: &T) -> Result<(), T::Error> {
    host.set_disposed()?;
    host.dispose_public()?;
    host.dispose_private()?;
    Ok(())
}
pub fn get_position_mode() -> Ready<PositionMode> {
    ready(PositionMode::Hedged)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdminSeverity {
    Error,
    Warning,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdminParams {
    Empty,
    Leverage {
        instrument_name: Vec<u16>,
        leverage: Vec<u16>,
    },
}
pub trait AdminHost {
    type Error;
    type Prepared;
    type Response;
    type PostFuture: Future<Output = Result<Self::Response, Self::Error>>;
    type Log;
    type LogReturn;
    fn prepare_post(&self) -> Result<Self::Prepared, Self::Error>;
    fn endpoint(&self, key: &'static str) -> Result<Vec<u16>, Self::Error>;
    fn number_to_string(&self, value: f64) -> Result<Vec<u16>, Self::Error>;
    fn post(
        &self,
        prepared: Self::Prepared,
        path: Vec<u16>,
        params: AdminParams,
    ) -> Result<Self::PostFuture, Self::Error>;
    /// Resolve this.log before any template coercion of a caught error.
    fn prepare_log(&self) -> Result<Self::Log, Self::Error>;
    fn error_as_template(&self, error: &Self::Error) -> Result<Vec<u16>, Self::Error>;
    fn log(
        &self,
        prepared: Self::Log,
        message: Vec<u16>,
        severity: AdminSeverity,
    ) -> Result<Self::LogReturn, Self::Error>;
}
enum Begun<F, R, E> {
    Waiting(F),
    Done(Result<R, E>),
}
fn log_and_rethrow<T: AdminHost>(host: &T, error: T::Error, prefix: &str) -> Result<(), T::Error> {
    let log = host.prepare_log()?;
    let message = prefix
        .encode_utf16()
        .chain(host.error_as_template(&error)?)
        .collect();
    host.log(log, message, AdminSeverity::Error)?;
    Err(error)
}
fn begin_admin<T: AdminHost>(
    host: &T,
    key: &'static str,
    symbol: Option<Vec<u16>>,
    leverage: Option<f64>,
    prefix: &str,
) -> Begun<T::PostFuture, (), T::Error> {
    let started = (|| {
        let prepared = host.prepare_post()?;
        let path = host.endpoint(key)?;
        let params = if let Some(leverage) = leverage {
            AdminParams::Leverage {
                instrument_name: symbol.expect("leverage body supplies the symbol"),
                leverage: host.number_to_string(leverage)?,
            }
        } else {
            AdminParams::Empty
        };
        host.post(prepared, path, params)
    })();
    match started {
        Ok(future) => Begun::Waiting(future),
        Err(error) => Begun::Done(log_and_rethrow(host, error, prefix)),
    }
}
async fn finish_admin<T: AdminHost>(
    host: Rc<T>,
    started: Begun<T::PostFuture, (), T::Error>,
    prefix: &'static str,
) -> Result<(), T::Error> {
    match started {
        Begun::Done(result) => result,
        Begun::Waiting(future) => match future.await {
            Ok(_) => Ok(()),
            Err(error) => log_and_rethrow(&*host, error, prefix),
        },
    }
}
pub fn set_leverage<T: AdminHost>(
    host: Rc<T>,
    symbol: Vec<u16>,
    leverage: f64,
    _side: Option<safe_cex_types::PositionSide>,
) -> impl Future<Output = Result<(), T::Error>> {
    let prefix = "Error setting leverage: ";
    let started = begin_admin(
        &*host,
        "CHANGE_LEVERAGE",
        Some(symbol),
        Some(leverage),
        prefix,
    );
    finish_admin(host, started, prefix)
}
pub fn cancel_all_orders<T: AdminHost>(host: Rc<T>) -> impl Future<Output = Result<(), T::Error>> {
    let prefix = "Error canceling all orders: ";
    let started = begin_admin(&*host, "CANCEL_ALL_ORDERS", None, None, prefix);
    finish_admin(host, started, prefix)
}
pub fn change_position_mode<T: AdminHost, H>(host: &T, _hedged: H) -> Ready<Result<(), T::Error>> {
    ready((|| {
        let log = host.prepare_log()?;
        host.log(
            log,
            "Position mode is always hedged on Crypto.com"
                .encode_utf16()
                .collect(),
            AdminSeverity::Warning,
        )?;
        Ok(())
    })())
}
pub trait MarginErrorHost {
    type Error;
    type Constructor;
    fn prepare_error_constructor(&self) -> Result<Self::Constructor, Self::Error>;
    fn construct_error(
        &self,
        constructor: Self::Constructor,
        message: Vec<u16>,
    ) -> Result<Self::Error, Self::Error>;
}
pub fn change_margin_type<T: MarginErrorHost>(
    host: &T,
    isolated: bool,
    symbol: Option<Vec<u16>>,
) -> Ready<Result<(), T::Error>> {
    ready((|| {
        let constructor = host.prepare_error_constructor()?;
        let symbol = symbol.unwrap_or_else(|| "a position".encode_utf16().collect());
        let message = "Crypto.com does not support switching "
            .encode_utf16()
            .chain(symbol)
            .chain(" to ".encode_utf16())
            .chain(if isolated { "isolated" } else { "cross" }.encode_utf16())
            .chain(" margin through Exchange v1".encode_utf16())
            .collect();
        let error = host.construct_error(constructor, message)?;
        Err(error)
    })())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AffiliateId {
    Null,
}
#[derive(Clone, Debug)]
pub struct Account<U> {
    pub user_id: U,
    pub affiliate_id: AffiliateId,
}
pub trait AccountHost: AdminHost {
    type Data;
    type ThisAccount;
    type UserId;
    fn response_data(&self, response: Self::Response) -> Result<Self::Data, Self::Error>;
    fn this_account(&self, data: Self::Data) -> Result<Self::ThisAccount, Self::Error>;
    fn uuid(&self, account: Self::ThisAccount) -> Result<Self::UserId, Self::Error>;
}
pub fn get_account<T: AccountHost>(
    host: Rc<T>,
) -> impl Future<Output = Result<Account<T::UserId>, T::Error>> {
    let started = (|| {
        let prepared = host.prepare_post()?;
        let path = host.endpoint("ACCOUNTS")?;
        host.post(prepared, path, AdminParams::Empty)
    })();
    async move {
        let response = started?.await?;
        let data = host.response_data(response)?;
        let account = host.this_account(data)?;
        let user_id = host.uuid(account)?;
        Ok(Account {
            user_id,
            affiliate_id: AffiliateId::Null,
        })
    }
}

#[derive(Clone, Debug)]
pub enum BalanceResult<B> {
    Value(B),
    Undefined,
}
#[derive(Clone, Debug)]
pub enum ValidationResult<V, B> {
    Success {
        error: Vec<u16>,
        balance: BalanceResult<B>,
    },
    Failure {
        error: V,
    },
}
pub trait ValidationHost: AdminHost {
    type Flag;
    type Value: Clone;
    type Balance;
    type BalancePromise;
    type BalanceFuture: Future<Output = Result<BalanceResult<Self::Balance>, Self::Error>>;
    type EmitReturn;
    fn suppress_flag(&self) -> Result<Self::Flag, Self::Error>;
    fn set_suppress_true(&self) -> Result<(), Self::Error>;
    fn restore_suppress_flag(&self, original: Self::Flag) -> Result<(), Self::Error>;
    /// Error/value accessors preserve original native referents. Primitive builders are pure.
    fn error_value(&self, error: Self::Error) -> Self::Value;
    fn is_nullish(&self, value: &Self::Value) -> bool;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn undefined_value(&self) -> Self::Value;
    fn string_value(&self, value: Vec<u16>) -> Self::Value;
    fn get(&self, value: Self::Value, key: &'static str) -> Result<Self::Value, Self::Error>;
    fn authentication_error_code(&self, code: Self::Value) -> Result<bool, Self::Error>;
    fn authentication_failure_message(&self, message: &Self::Value) -> Result<bool, Self::Error>;
    fn emit_error(&self, message: Self::Value) -> Result<Self::EmitReturn, Self::Error>;
    fn fetch_balance(&self) -> Result<Self::BalancePromise, Self::Error>;
    /// Resolve/call .catch on the original promise, then await its original returned carrier.
    fn catch_balance<F: FnMut() -> BalanceResult<Self::Balance> + 'static>(
        &self,
        promise: Self::BalancePromise,
        catch: F,
    ) -> Result<Self::BalanceFuture, Self::Error>;
}
fn optional_chain<T: ValidationHost>(
    host: &T,
    mut value: T::Value,
    keys: &[&'static str],
) -> Result<T::Value, T::Error> {
    for key in keys {
        if host.is_nullish(&value) {
            return Ok(host.undefined_value());
        }
        value = host.get(value, key)?;
    }
    Ok(value)
}
fn validation_catch<T: ValidationHost>(
    host: &T,
    error: T::Error,
) -> Result<ValidationResult<T::Value, T::Balance>, T::Error> {
    let error = host.error_value(error);
    let mut message = optional_chain(host, error.clone(), &["response", "data", "message"])?;
    if !host.truthy(&message) {
        message = optional_chain(host, error.clone(), &["message"])?;
        if !host.truthy(&message) {
            message = host.string_value("Invalid API key or secret".encode_utf16().collect());
        }
    }
    let code = optional_chain(host, error, &["response", "data", "code"])?;
    let authenticated_error =
        host.authentication_error_code(code)? || host.authentication_failure_message(&message)?;
    if !authenticated_error {
        host.emit_error(message.clone())?;
    }
    Ok(ValidationResult::Failure { error: message })
}
fn restore_validation<T: ValidationHost>(
    host: &T,
    original: T::Flag,
    result: Result<ValidationResult<T::Value, T::Balance>, T::Error>,
) -> Result<ValidationResult<T::Value, T::Balance>, T::Error> {
    host.restore_suppress_flag(original)?;
    result
}
pub fn validate_account<T: ValidationHost>(
    host: Rc<T>,
) -> impl Future<Output = Result<ValidationResult<T::Value, T::Balance>, T::Error>> {
    // These two effects occur before entering the source try/finally and before returning.
    let previous = host
        .suppress_flag()
        .and_then(|previous| host.set_suppress_true().map(|_| previous));
    let started = match previous {
        Err(error) => Begun::Done(Err(error)),
        Ok(previous) => {
            let post = (|| {
                let prepared = host.prepare_post()?;
                let path = host.endpoint("BALANCE")?;
                host.post(prepared, path, AdminParams::Empty)
            })();
            match post {
                Ok(future) => Begun::Waiting((previous, future)),
                Err(error) => Begun::Done(restore_validation(
                    &*host,
                    previous,
                    validation_catch(&*host, error),
                )),
            }
        }
    };
    async move {
        match started {
            Begun::Done(result) => result,
            Begun::Waiting((previous, future)) => {
                let result = match future.await {
                    Err(error) => validation_catch(&*host, error),
                    Ok(_) => {
                        let balance = host.fetch_balance().and_then(|promise| {
                            host.catch_balance(promise, || BalanceResult::Undefined)
                        });
                        match balance {
                            Err(error) => validation_catch(&*host, error),
                            Ok(balance) => match balance.await {
                                Ok(balance) => Ok(ValidationResult::Success {
                                    error: Vec::new(),
                                    balance,
                                }),
                                Err(error) => validation_catch(&*host, error),
                            },
                        }
                    }
                };
                restore_validation(&*host, previous, result)
            }
        }
    }
}
