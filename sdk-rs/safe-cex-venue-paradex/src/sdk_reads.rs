//! SDK read orchestration through the existing loader, properties and methods.
use std::future::Future;

// ts: getParaclearBalance
pub struct TokenBalanceRequest<C, P, A, T> {
    pub config: C,
    pub provider: P,
    pub account: A,
    pub token: T,
}

// ts: getSocializedLossFactor
pub struct LossFactorRequest<C, P> {
    pub config: C,
    pub provider: P,
}

// ts: getParaclearBalance
pub async fn get_paraclear_balance<E, L, S, M, C, P, A, T, F, V, O>(
    load_sdk: impl FnOnce() -> Result<L, E>,
    read_method: impl FnOnce(S) -> Result<M, E>,
    read_fields: impl FnOnce() -> Result<(C, P, A, T), E>,
    call: impl FnOnce(M, TokenBalanceRequest<C, P, A, T>) -> Result<F, E>,
    stringify: impl FnOnce(V) -> Result<O, E>,
) -> Result<O, E>
where
    L: Future<Output = Result<S, E>>,
    F: Future<Output = Result<V, E>>,
{
    let sdk = load_sdk()?.await?;
    let method = read_method(sdk)?;
    let (config, provider, account, token) = read_fields()?;
    let balance = call(
        method,
        TokenBalanceRequest {
            config,
            provider,
            account,
            token,
        },
    )?
    .await?;
    stringify(balance)
}

// ts: getSocializedLossFactor
pub async fn get_socialized_loss_factor<E, L, S, M, C, P, F, V, O, T: ?Sized>(
    _token_symbol: &T,
    load_sdk: impl FnOnce() -> Result<L, E>,
    read_method: impl FnOnce(S) -> Result<M, E>,
    read_fields: impl FnOnce() -> Result<(C, P), E>,
    call: impl FnOnce(M, LossFactorRequest<C, P>) -> Result<F, E>,
    stringify: impl FnOnce(V) -> Result<O, E>,
) -> Result<O, E>
where
    L: Future<Output = Result<S, E>>,
    F: Future<Output = Result<V, E>>,
{
    let sdk = load_sdk()?.await?;
    let method = read_method(sdk)?;
    let (config, provider) = read_fields()?;
    let factor = call(method, LossFactorRequest { config, provider })?.await?;
    stringify(factor)
}
