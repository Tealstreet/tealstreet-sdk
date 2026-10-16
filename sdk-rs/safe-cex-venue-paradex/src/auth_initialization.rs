//! Authentication bootstrap over original SDK, promise, property and header capabilities.
use std::{future::Future, pin::Pin};

// ts: initializeParadexSDK; ParadexExchange#initializeSDK; ParadexExchange#ensurePrivateHistoryAuth; ParadexExchange.validateAccount; authenticateRequest
pub type AuthFuture<'a, V, E> = Pin<Box<dyn Future<Output = Result<V, E>> + 'a>>;

// ts: initializeParadexSDK; ParadexExchange#initializeSDK; ParadexExchange#ensurePrivateHistoryAuth; ParadexExchange.validateAccount; authenticateRequest
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthOperation {
    LoadSdk,
    Config,
    ImportAccount,
    FromEthereum,
    InitializeSdk,
    Wait,
    GetAccount,
    GetToken,
}

// ts: initializeParadexSDK; ParadexExchange#initializeSDK
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthConstruction {
    Provider,
    StarknetAccount,
    Wallet,
    EthereumAdapter,
    RequestSigner,
    JwtManager,
}

// ts: initializeParadexSDK; ParadexExchange#initializeSDK; ParadexExchange#ensurePrivateHistoryAuth; ParadexExchange.validateAccount; authenticateRequest
pub trait AuthInitializationHost {
    type Value: Clone;
    type Error: Clone;
    type Log;
    type HeaderSetter;
    fn get(&self, value: &Self::Value, key: &str) -> Result<Self::Value, Self::Error>;
    fn optional_get(&self, value: &Self::Value, key: &str) -> Result<Self::Value, Self::Error>;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn literal(&self, text: &str) -> Self::Value;
    fn null(&self) -> Self::Value;
    fn boolean(&self, value: bool) -> Self::Value;
    fn object(&self, fields: Vec<(&'static str, Self::Value)>) -> Self::Value;
    fn template(&self, value: &Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn text_value(&self, text: Vec<u16>) -> Self::Value;
    fn lowercase(&self, value: Self::Value) -> Result<Self::Value, Self::Error>;
    fn length(&self, value: &Self::Value) -> Result<f64, Self::Error>;
    fn trim_empty(&self, value: &Self::Value) -> Result<bool, Self::Error>;
    fn state(&self, key: &str) -> Result<Self::Value, Self::Error>;
    fn write_state(&self, key: &str, value: Self::Value) -> Result<(), Self::Error>;
    fn operation(
        &self,
        op: AuthOperation,
        args: Vec<Self::Value>,
    ) -> Result<AuthFuture<'_, Self::Value, Self::Error>, Self::Error>;
    fn construct(
        &self,
        kind: AuthConstruction,
        args: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn await_value(
        &self,
        value: Self::Value,
    ) -> Result<AuthFuture<'_, Self::Value, Self::Error>, Self::Error>;
    fn initialize_promise(&self) -> Result<Self::Value, Self::Error>;
    fn observe_rejection(&self, promise: Self::Value) -> Result<(), Self::Error>;
    fn reset_sdk(&self) -> Result<(), Self::Error>;
    fn new_error(&self, message: Vec<u16>) -> Result<Self::Error, Self::Error>;
    fn history_error(
        &self,
        operation: &[u16],
        last: Option<Self::Error>,
    ) -> Result<Self::Error, Self::Error>;
    fn error_value(&self, error: &Self::Error) -> Self::Value;
    fn emit(&self, value: Self::Value) -> Result<(), Self::Error>;
    fn select_log(&self) -> Result<Self::Log, Self::Error>;
    fn log(
        &self,
        logger: Self::Log,
        text: Vec<u16>,
        severity: &'static str,
    ) -> Result<(), Self::Error>;
    fn console(
        &self,
        error: bool,
        prefix: &'static str,
        value: Option<Self::Value>,
    ) -> Result<(), Self::Error>;
    fn header_setter(&self, config: &Self::Value) -> Result<Self::HeaderSetter, Self::Error>;
    fn set_header(
        &self,
        setter: Self::HeaderSetter,
        name: &'static str,
        value: Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn signer_address(&self, signer: Self::Value) -> Result<Self::Value, Self::Error>;
    fn select_ethereum(&self, sdk: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn number(&self, value: f64) -> Self::Value;
}
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn cat(parts: &[&[u16]]) -> Vec<u16> {
    parts.iter().flat_map(|s| s.iter().copied()).collect()
}
fn raise<H: AuthInitializationHost, T>(h: &H, message: &str) -> Result<T, H::Error> {
    Err(h.new_error(u(message))?)
}
fn option<H: AuthInitializationHost>(h: &H, key: &str) -> Result<H::Value, H::Error> {
    h.get(&h.state("options")?, key)
}

// ts: ParadexPrivateHistoryAuthNotReadyError.constructor
pub fn initialize_history_auth_error<V, P, E>(
    operation: &[u16],
    original: &V,
    instance: impl FnOnce(&V) -> Result<bool, E>,
    mut message: impl FnMut(&V) -> Result<V, E>,
    truthy: impl FnOnce(&V) -> bool,
    template: impl FnOnce(V) -> Result<Vec<u16>, E>,
    base: impl FnOnce(Vec<u16>) -> Result<P, E>,
    name: impl FnOnce(&P, &'static str) -> Result<(), E>,
    write_original: impl FnOnce(&P, &V) -> Result<(), E>,
) -> Result<P, E> {
    let detail = if instance(original)? && truthy(&message(original)?) {
        cat(&[&u(": "), &template(message(original)?)?])
    } else {
        vec![]
    };
    let parent = base(cat(&[
        &u("Paradex private auth not ready for "),
        operation,
        &detail,
    ]))?;
    name(&parent, "ParadexPrivateHistoryAuthNotReadyError")?;
    write_original(&parent, original)?;
    Ok(parent)
}

// ts: isParadexPrivateHistoryAuthNotReadyError
pub fn is_history_auth_not_ready<E>(instance: impl FnOnce() -> Result<bool, E>) -> Result<bool, E> {
    instance()
}

// ts: wait
pub fn wait<'a, P, R, T, E>(
    ms: f64,
    create: impl FnOnce(Box<dyn FnOnce(R) -> Result<T, E> + 'a>) -> Result<P, E>,
    schedule: impl FnOnce(R, f64) -> Result<T, E> + 'a,
) -> Result<P, E> {
    create(Box::new(move |resolve| schedule(resolve, ms)))
}

// ts: initializeParadexSDK
pub async fn initialize_paradex_sdk<H: AuthInitializationHost>(
    h: &H,
    opts: H::Value,
) -> Result<H::Value, H::Error> {
    if !h.truthy(&h.get(&opts, "paradexAddress")?) || !h.truthy(&h.get(&opts, "key")?) {
        return Ok(h.null());
    }
    let result = async {
        let sdk = h.operation(AuthOperation::LoadSdk, vec![])?.await?;
        let env = h.literal(if h.truthy(&h.get(&opts, "testnet")?) {
            "testnet"
        } else {
            "prod"
        });
        let config = h
            .operation(AuthOperation::Config, vec![sdk.clone(), env])?
            .await?;
        let provider = h.construct(
            AuthConstruction::Provider,
            vec![sdk.clone(), config.clone()],
        )?;
        if h.truthy(&h.get(&opts, "payloadSigner")?) {
            let address = h.lowercase(h.get(&opts, "paradexAddress")?)?;
            return Ok(h.object(vec![
                ("account", h.object(vec![("address", address)])),
                ("config", config),
                ("provider", provider),
            ]));
        }
        let subkey = h.length(&h.get(&opts, "key")?)? == 66.0;
        let account = if subkey {
            h.console(
                false,
                "[Paradex SDK] Using subkey authentication mode",
                None,
            )?;
            h.console(
                false,
                "[Paradex SDK] Main account:",
                Some(h.get(&opts, "paradexAddress")?),
            )?;
            h.console(false, "[Paradex SDK] Subkey will be used for signing", None)?;
            let class = h.operation(AuthOperation::ImportAccount, vec![])?.await?;
            let node = h.object(vec![("nodeUrl", h.get(&config, "paradexFullNodeRpcUrl")?)]);
            h.construct(
                AuthConstruction::StarknetAccount,
                vec![
                    class,
                    node,
                    h.get(&opts, "paradexAddress")?,
                    h.get(&opts, "key")?,
                ],
            )?
        } else {
            h.console(
                false,
                "[Paradex SDK] Using Ethereum authentication mode",
                None,
            )?;
            let wallet = h.construct(AuthConstruction::Wallet, vec![h.get(&opts, "key")?])?;
            let signer =
                h.construct(AuthConstruction::EthereumAdapter, vec![sdk.clone(), wallet])?;
            let selected = h.select_ethereum(&sdk)?;
            let args = h.object(vec![
                (
                    "provider",
                    h.object(vec![("nodeUrl", h.get(&config, "paradexFullNodeRpcUrl")?)]),
                ),
                ("config", config.clone()),
                ("signer", signer),
            ]);
            h.operation(AuthOperation::FromEthereum, vec![selected, args])?
                .await?
        };
        h.console(
            false,
            "[Paradex SDK] Account initialized with address:",
            Some(h.get(&account, "address")?),
        )?;
        Ok(h.object(vec![
            ("account", account),
            ("config", config),
            ("provider", provider),
        ]))
    }
    .await;
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            h.console(
                true,
                "[Paradex SDK] Failed to initialize:",
                Some(h.error_value(&error)),
            )?;
            Ok(h.null())
        }
    }
}

// ts: ParadexExchange#initializeSDK
pub async fn initialize_exchange_sdk<H: AuthInitializationHost>(h: &H) -> Result<(), H::Error> {
    let result = async {
        if !h.truthy(&option(h, "key")?) {
            return raise(
                h,
                "Missing subkey private key (key field) for Paradex authentication",
            );
        }
        if !h.truthy(&option(h, "paradexAddress")?) {
            return raise(h, "Missing Paradex address for Paradex authentication");
        }
        if !h.truthy(&option(h, "subkeyAddress")?) {
            return raise(h, "Missing subkey address for Paradex authentication");
        }
        let logger = h.select_log()?;
        h.log(logger, u("Initializing Paradex SDK..."), "debug")?;
        let account = h
            .operation(AuthOperation::InitializeSdk, vec![h.state("options")?])?
            .await?;
        h.write_state("sdkAccount", account)?;
        if !h.truthy(&h.state("sdkAccount")?) {
            return raise(h, "Paradex SDK initialization returned null account");
        }
        let logger = h.select_log()?;
        let address = h.get(&h.get(&h.state("sdkAccount")?, "account")?, "address")?;
        h.log(
            logger,
            cat(&[
                &u("Paradex SDK initialized successfully - Account: "),
                &h.template(&address)?,
            ]),
            "info",
        )?;
        let signer = h.construct(
            AuthConstruction::RequestSigner,
            vec![h.state("sdkAccount")?, h.state("options")?],
        )?;
        h.write_state("requestSigner", signer)?;
        let logger = h.select_log()?;
        let address = h.signer_address(h.state("requestSigner")?)?;
        h.log(
            logger,
            cat(&[
                &u("Request signer initialized for account: "),
                &h.template(&address)?,
            ]),
            "debug",
        )?;
        let account = h.state("sdkAccount")?;
        let testnet = option(h, "testnet")?;
        let testnet = if h.truthy(&testnet) {
            testnet
        } else {
            h.boolean(false)
        };
        let manager = h.construct(
            AuthConstruction::JwtManager,
            vec![
                account,
                testnet,
                option(h, "subkeyAddress")?,
                h.state("options")?,
            ],
        )?;
        h.write_state("jwtManager", manager)?;
        let logger = h.select_log()?;
        h.log(
            logger,
            cat(&[
                &u("JWT manager initialized with subkey: "),
                &h.template(&option(h, "subkeyAddress")?)?,
            ]),
            "debug",
        )
    }
    .await;
    match result {
        Ok(()) => Ok(()),
        Err(e) => {
            let logger = h.select_log()?;
            h.log(
                logger,
                cat(&[
                    &u("Failed to initialize Paradex SDK: "),
                    &h.template(&h.error_value(&e))?,
                ]),
                "error",
            )?;
            Err(e)
        }
    }
}

// ts: ParadexExchange#ensurePrivateHistoryAuth
pub async fn ensure_private_history_auth<H: AuthInitializationHost>(
    h: &H,
    operation: &[u16],
) -> Result<(), H::Error> {
    let mut last = None;
    for attempt in 0..=2 {
        let result = async {
            if h.truthy(&h.state("jwtManager")?) {
                return Ok(true);
            }
            if !h.truthy(&h.state("sdkInitPromise")?) && h.truthy(&option(h, "paradexAddress")?) {
                let logger = h.select_log()?;
                h.log(
                    logger,
                    cat(&[&u("Retrying Paradex SDK initialization for "), operation]),
                    "debug",
                )?;
                h.write_state("sdkInitPromise", h.initialize_promise()?)?;
                h.observe_rejection(h.state("sdkInitPromise")?)?;
            }
            if h.truthy(&h.state("sdkInitPromise")?) {
                h.await_value(h.state("sdkInitPromise")?)?.await?;
            }
            if h.truthy(&h.state("jwtManager")?) {
                return Ok(true);
            }
            last = Some(h.new_error(u("JWT manager not initialized"))?);
            h.reset_sdk()?;
            Ok(false)
        }
        .await;
        match result {
            Ok(true) => return Ok(()),
            Ok(false) => {}
            Err(e) => {
                last = Some(e);
                h.reset_sdk()?;
            }
        }
        if let Some(delay) = [500.0, 1500.0].get(attempt) {
            h.operation(AuthOperation::Wait, vec![h.number(*delay)])?
                .await?;
        }
    }
    Err(h.history_error(operation, last)?)
}
fn validation_error<H: AuthInitializationHost>(
    h: &H,
    error: &H::Value,
) -> Result<H::Value, H::Error> {
    let response = h.optional_get(error, "response")?;
    let data = h.optional_get(&response, "data")?;
    let value = h.optional_get(&data, "error")?;
    let message = h.optional_get(&value, "message")?;
    if h.truthy(&message) {
        Ok(message)
    } else {
        h.optional_get(error, "message")
    }
}

// ts: ParadexExchange.validateAccount
pub async fn validate_account<H: AuthInitializationHost>(h: &H) -> Result<H::Value, H::Error> {
    let result = async {
        if !h.truthy(&h.state("sdkInitPromise")?) && h.truthy(&option(h, "paradexAddress")?) {
            let logger = h.select_log()?;
            h.log(
                logger,
                u("SDK not initialized yet, starting initialization..."),
                "debug",
            )?;
            h.write_state("sdkInitPromise", h.initialize_promise()?)?;
        }
        if h.truthy(&h.state("sdkInitPromise")?) {
            h.await_value(h.state("sdkInitPromise")?)?.await?;
        }
        if !h.truthy(&h.state("jwtManager")?) {
            for (key, message) in [
                (
                    "paradexAddress",
                    "Missing Paradex address - please enter your Paradex account address",
                ),
                (
                    "subkeyAddress",
                    "Missing subkey address - please enter your subkey public address",
                ),
                (
                    "key",
                    "Missing subkey private key - please enter your subkey private key",
                ),
            ] {
                if !h.truthy(&option(h, key)?) {
                    return Ok(h.object(vec![("error", h.literal(message))]));
                }
            }
            return Ok(h.object(vec![(
                "error",
                h.literal("Authentication failed - SDK initialization error"),
            )]));
        }
        h.operation(AuthOperation::GetAccount, vec![h.literal("/account")])?
            .await?;
        Ok(h.object(vec![("error", h.literal(""))]))
    }
    .await;
    match result {
        Ok(value) => Ok(value),
        Err(e) => {
            let error = h.error_value(&e);
            h.emit(validation_error(h, &error)?)?;
            let message = validation_error(h, &error)?;
            let message = if h.truthy(&message) {
                message
            } else {
                h.literal("Invalid API key or secret")
            };
            Ok(h.object(vec![("error", message)]))
        }
    }
}

// ts: authenticateRequest
pub async fn authenticate_request<H: AuthInitializationHost>(
    h: &H,
    config: H::Value,
) -> Result<H::Value, H::Error> {
    if !h.truthy(&h.state("jwtManager")?) {
        let logger = h.select_log()?;
        h.log(
            logger,
            u("[Paradex API] JWT manager not initialized - cannot authenticate request"),
            "error",
        )?;
        return raise(
            h,
            "JWT manager not initialized - cannot authenticate request",
        );
    }
    let result = async {
        let token = h.operation(AuthOperation::GetToken, vec![])?.await?;
        if !h.truthy(&token) || h.trim_empty(&token)? {
            let logger = h.select_log()?;
            h.log(
                logger,
                u("[Paradex API] JWT token is empty - cannot authenticate request"),
                "error",
            )?;
            return raise(h, "JWT token is empty - cannot authenticate request");
        }
        let setter = h.header_setter(&config)?;
        let authorization = h.text_value(cat(&[&u("Bearer "), &h.template(&token)?]));
        h.set_header(setter, "Authorization", authorization)?;
        let setter = h.header_setter(&config)?;
        h.set_header(setter, "Content-Type", h.literal("application/json"))?;
        let setter = h.header_setter(&config)?;
        h.set_header(setter, "Accept", h.literal("application/json"))?;
        Ok(config)
    }
    .await;
    match result {
        Ok(value) => Ok(value),
        Err(e) => {
            let logger = h.select_log()?;
            h.log(
                logger,
                cat(&[
                    &u("[Paradex API] Failed to get JWT token: "),
                    &h.template(&h.error_value(&e))?,
                ]),
                "error",
            )?;
            Err(e)
        }
    }
}
