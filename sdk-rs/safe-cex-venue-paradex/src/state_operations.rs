//! Source control flow over the host's property, module and history capabilities.
use std::{cell::RefCell, future::Future, rc::Rc};

// ts: ParadexExchange.get isPublicDataOnly
pub fn is_public_data_only<E>(
    read_explicit_true: impl FnOnce() -> Result<bool, E>,
    read_key_truthy: impl FnOnce() -> Result<bool, E>,
    read_address_truthy: impl FnOnce() -> Result<bool, E>,
    read_subkey_truthy: impl FnOnce() -> Result<bool, E>,
) -> Result<bool, E> {
    if read_explicit_true()? {
        return Ok(true);
    }
    Ok(!read_key_truthy()? || !read_address_truthy()? || !read_subkey_truthy()?)
}

// ts: ParadexExchange#resetSDKStateAfterInitFailure
pub fn reset_sdk_state_after_init_failure<I, A, S, J, E>(
    write_init: impl FnOnce(Option<I>) -> Result<(), E>,
    write_account: impl FnOnce(Option<A>) -> Result<(), E>,
    write_signer: impl FnOnce(Option<S>) -> Result<(), E>,
    write_jwt: impl FnOnce(Option<J>) -> Result<(), E>,
) -> Result<(), E> {
    write_init(None)?;
    write_account(None)?;
    write_signer(None)?;
    write_jwt(None)?;
    Ok(())
}

// ts: ParadexExchange.fetchHistoricalPositions
pub async fn fetch_historical_positions<O, R, E, F>(
    opts: O,
    fold_closed_position_records: impl FnOnce(O, bool) -> Result<F, E>,
) -> Result<R, E>
where
    F: Future<Output = Result<R, E>>,
{
    fold_closed_position_records(opts, true)?.await
}

// ts: getParadexSDK
pub async fn get_paradex_sdk<S, E, F>(
    cache: &RefCell<Option<Rc<S>>>,
    load_module: impl FnOnce() -> Result<F, E>,
) -> Result<Rc<S>, E>
where
    F: Future<Output = Result<Rc<S>, E>>,
{
    if cache.borrow().is_none() {
        let sdk = load_module()?.await?;
        *cache.borrow_mut() = Some(sdk);
    }
    Ok(Rc::clone(
        cache.borrow().as_ref().expect("settled SDK module"),
    ))
}
