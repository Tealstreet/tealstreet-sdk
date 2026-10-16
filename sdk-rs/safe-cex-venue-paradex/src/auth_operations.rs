//! SDK/auth control flow over original property, signing and await capabilities.
use std::future::Future;

// ts: ParadexRequestSigner.constructor
pub fn initialize_request_signer<A, O, E>(
    account: &A,
    options: Option<O>,
    empty_options: impl FnOnce() -> Result<O, E>,
    write_account: impl FnOnce(&A) -> Result<(), E>,
    write_options: impl FnOnce(O) -> Result<(), E>,
) -> Result<(), E> {
    let options = match options {
        Some(value) => value,
        None => empty_options()?,
    };
    write_account(account)?;
    write_options(options)
}

// ts: ParadexJWTManager.constructor
pub fn initialize_jwt_manager<A, O, S, M, E>(
    account: &A,
    testnet: bool,
    subkey: S,
    options: Option<O>,
    empty_options: impl FnOnce() -> Result<O, E>,
    write_account: impl FnOnce(&A) -> Result<(), E>,
    write_options: impl FnOnce(O) -> Result<(), E>,
    write_subkey: impl FnOnce(S) -> Result<(), E>,
    read_address: impl FnOnce(&A) -> Result<M, E>,
    write_address: impl FnOnce(M) -> Result<(), E>,
    write_base_url: impl FnOnce(&'static str) -> Result<(), E>,
) -> Result<(), E> {
    let options = match options {
        Some(value) => value,
        None => empty_options()?,
    };
    write_account(account)?;
    write_options(options)?;
    write_subkey(subkey)?;
    write_address(read_address(account)?)?;
    write_base_url(if testnet {
        "https://api.testnet.paradex.trade/v1"
    } else {
        "https://api.prod.paradex.trade/v1"
    })
}

// ts: ParadexJWTManager.getToken
pub async fn get_token<T, P, F, E>(
    read_now: impl FnOnce() -> Result<f64, E>,
    read_token_truthy: impl FnOnce() -> Result<bool, E>,
    read_expiry: impl FnOnce() -> Result<f64, E>,
    mut read_token: impl FnMut() -> Result<T, E>,
    read_refresh_truthy: impl FnOnce() -> Result<bool, E>,
    generate: impl FnOnce() -> Result<P, E>,
    write_refresh: impl FnOnce(P) -> Result<(), E>,
    read_refresh: impl FnOnce() -> Result<F, E>,
    write_token: impl FnOnce(T) -> Result<(), E>,
    write_expiry: impl FnOnce(f64) -> Result<(), E>,
    log_error: impl FnOnce(&'static str, &E) -> Result<(), E>,
    clear_refresh: impl FnOnce() -> Result<(), E>,
) -> Result<T, E>
where
    F: Future<Output = Result<T, E>>,
{
    let now = read_now()?;
    if read_token_truthy()? && now < read_expiry()? - 120_000.0 {
        return read_token();
    }
    if read_refresh_truthy()? {
        return read_refresh()?.await;
    }
    write_refresh(generate()?)?;
    let result = async {
        write_token(read_refresh()?.await?)?;
        write_expiry(now + 300_000.0)?;
        read_token()
    }
    .await;
    let result = match result {
        Ok(token) => Ok(token),
        Err(error) => match log_error("[Paradex JWT] ✗ Failed to obtain token:", &error) {
            Ok(()) => Err(error),
            Err(replacement) => Err(replacement),
        },
    };
    clear_refresh()?;
    result
}

// ts: ParadexPrivateWebSocket.onOpen
pub async fn private_on_open<F, E>(
    read_disposed: impl FnOnce() -> Result<bool, E>,
    write_authenticated: impl FnOnce(bool) -> Result<(), E>,
    write_auth_id: impl FnOnce(Option<f64>) -> Result<(), E>,
    log: impl FnOnce(&'static str, &'static str) -> Result<(), E>,
    ping: impl FnOnce() -> Result<(), E>,
    authenticate: impl FnOnce() -> Result<F, E>,
) -> Result<(), E>
where
    F: Future<Output = Result<(), E>>,
{
    if !read_disposed()? {
        write_authenticated(false)?;
        write_auth_id(None)?;
        log("Paradex private WebSocket connected", "info")?;
        ping()?;
        authenticate()?.await?;
    }
    Ok(())
}

// ts: ParadexPrivateWebSocket#authenticate
pub struct AuthenticationRequest<T> {
    pub jsonrpc: &'static str,
    pub method: &'static str,
    pub bearer: T,
    pub id: f64,
}

// ts: ParadexPrivateWebSocket#authenticate
pub async fn authenticate<T, I, L, F, E, S>(
    read_init_truthy: impl FnOnce() -> Result<bool, E>,
    await_init: impl FnOnce() -> Result<L, E>,
    read_manager_truthy: impl FnOnce() -> Result<bool, E>,
    get_token: impl FnOnce() -> Result<F, E>,
    read_id: impl FnOnce() -> Result<f64, E>,
    write_id: impl FnOnce(f64) -> Result<(), E>,
    write_auth_id: impl FnOnce(f64) -> Result<(), E>,
    send: impl FnOnce(AuthenticationRequest<T>) -> Result<(), E>,
    // Select the parent log receiver/method before interpolating the error.
    prepare_error_log: impl FnOnce(&E) -> Result<S, E>,
    log_error: impl FnOnce(S, &'static str) -> Result<(), E>,
) -> Result<(), E>
where
    L: Future<Output = Result<I, E>>,
    F: Future<Output = Result<T, E>>,
{
    let result = async {
        if read_init_truthy()? {
            await_init()?.await?;
        }
        if read_manager_truthy()? {
            let token = get_token()?.await?;
            let id = read_id()?;
            write_id(id + 1.0)?;
            write_auth_id(id)?;
            send(AuthenticationRequest {
                jsonrpc: "2.0",
                method: "auth",
                bearer: token,
                id,
            })?;
        }
        Ok(())
    }
    .await;
    match result {
        Ok(()) => Ok(()),
        Err(error) => log_error(prepare_error_log(&error)?, "error"),
    }
}
