//! Small Bitrue entrypoints at their property-read, queue and emission boundaries.

/// `fill?.symbol` is read once, then forwarded as the queue's one argument.
/// A nullish fill skips the property read. Dependency failures retain their value.
pub fn handle_fill_for_fast_refresh<T, S, Q, R, E>(
    fill: Option<&T>,
    resolve_queue: impl FnOnce() -> Result<Q, E>,
    read_symbol: impl FnOnce(&T) -> Result<S, E>,
) -> Result<(), E>
where
    Q: FnOnce(Option<S>) -> Result<R, E>,
{
    // JS resolves the callee before evaluating fill?.symbol.
    let queue_fast_private_refresh = resolve_queue()?;
    let symbol = fill.map(read_symbol).transpose()?;
    queue_fast_private_refresh(symbol)?;
    Ok(())
}

/// The account payload is ignored, and the parent queue receives no arguments.
pub fn handle_account_update<T, Q, R, E>(
    _json: T,
    resolve_parent_queue: impl FnOnce() -> Result<Q, E>,
) -> Result<(), E>
where
    Q: FnOnce() -> Result<R, E>,
{
    let queue_fast_private_refresh = resolve_parent_queue()?;
    queue_fast_private_refresh()?;
    Ok(())
}

/// Emission and Error construction happen during invocation, before polling.
/// The constructor boundary retains the complete native thrown value unchanged.
pub fn change_margin_type<I, S, R, E>(
    _isolated: I,
    _symbol: S,
    emit_and_log_error: impl FnOnce(&str) -> Result<R, E>,
    construct_error: impl FnOnce(&str) -> Result<E, E>,
) -> std::future::Ready<Result<(), E>> {
    let msg = "Cannot change Margin Type on Tealstreet as API does not support it. Change it on bitrue.com";
    let result = emit_and_log_error(msg).and_then(|_| {
        let error = construct_error(msg)?;
        Err(error)
    });
    std::future::ready(result)
}
