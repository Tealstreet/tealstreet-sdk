//! Builder workflows against bound JS values, owned promises, fetch, timers and wallet operations.
use std::{future::Future, pin::Pin, rc::Rc};
pub type JsResult<V, T = V> = Result<T, V>;
pub type JsPromise<V> = Pin<Box<dyn Future<Output = JsResult<V>>>>;
pub type Reaction = Pin<Box<dyn Future<Output = ()>>>;
#[derive(Clone)]
pub struct FetchRequest<V> {
    pub url: V,
    pub method: &'static str,
    pub headers: V,
    pub payload: V,
    pub body: V,
    pub credentials: bool,
    pub signal: Option<V>,
}

/// start polls the prefix eagerly and owns continuation independently of the returned handle.
/// Foreign and returned helper promises bind JS scheduling; clones preserve value identity.
/// Timeout callbacks own their captured controller independently of workflow completion.
pub trait BuilderHost: 'static {
    type Value: Clone + 'static;
    type Iterator: 'static;
    fn start(&self, task: JsPromise<Self::Value>) -> JsPromise<Self::Value>;
    fn reaction(&self) -> Reaction;
    fn string(&self, value: &str) -> Self::Value;
    fn number(&self, value: f64) -> Self::Value;
    fn boolean(&self, value: bool) -> Self::Value;
    fn null(&self) -> Self::Value;
    fn undefined(&self) -> Self::Value;
    fn object(&self, fields: Vec<(&'static str, Self::Value)>) -> Self::Value;
    fn array(&self, values: Vec<Self::Value>) -> Self::Value;
    fn property(&self, value: &Self::Value, key: &str) -> JsResult<Self::Value>;
    fn set(
        &self,
        value: &Self::Value,
        key: &'static str,
        field: Self::Value,
    ) -> JsResult<Self::Value, ()>;
    fn spread(&self, target: &Self::Value, source: &Self::Value) -> JsResult<Self::Value, ()>;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn is_null(&self, value: &Self::Value) -> bool;
    fn is_undefined(&self, value: &Self::Value) -> bool;
    fn is_string(&self, value: &Self::Value) -> bool;
    fn equal(&self, left: &Self::Value, right: &Self::Value) -> bool;
    fn concat(&self, parts: Vec<Self::Value>) -> JsResult<Self::Value>;
    fn lowercase(&self, value: &Self::Value) -> JsResult<Self::Value>;
    fn stringify(&self, value: &Self::Value) -> JsResult<Self::Value>;
    fn to_string(&self, value: &Self::Value) -> JsResult<Self::Value>;
    fn parse_float(&self, value: &Self::Value) -> JsResult<Self::Value, f64>;
    fn parse_int(&self, value: &Self::Value, radix: i32) -> JsResult<Self::Value, f64>;
    fn to_fixed(&self, value: f64, digits: f64) -> JsResult<Self::Value>;
    fn error(&self, message: Self::Value) -> Self::Value;
    fn window(&self) -> JsResult<Self::Value, Option<Self::Value>>;
    fn read_window(&self) -> JsResult<Self::Value>;
    fn browser_provider(&self, ethereum: &Self::Value) -> JsResult<Self::Value>;
    fn provider_send(
        &self,
        provider: &Self::Value,
        method: &'static str,
        args: Self::Value,
    ) -> JsResult<Self::Value, JsPromise<Self::Value>>;
    fn signer(&self, provider: &Self::Value) -> JsResult<Self::Value, JsPromise<Self::Value>>;
    fn authorize_builder_types(&self) -> Self::Value;
    fn sign(
        &self,
        signer: &Self::Value,
        domain: Self::Value,
        types: Self::Value,
        message: Self::Value,
    ) -> JsResult<Self::Value, JsPromise<Self::Value>>;
    fn slice(&self, value: &Self::Value, start: usize, end: Option<usize>)
    -> JsResult<Self::Value>;
    fn edge_url(&self, testnet: bool) -> JsResult<Self::Value>;
    fn trading_url(&self, testnet: bool) -> JsResult<Self::Value>;
    fn proxy_headers(&self) -> JsResult<Self::Value, JsPromise<Self::Value>>;
    fn controller(&self) -> JsResult<Self::Value>;
    fn timeout(&self, controller: &Self::Value, delay: f64) -> JsResult<Self::Value>;
    fn clear_timeout(&self, timer: &Self::Value) -> JsResult<Self::Value, ()>;
    fn fetch(
        &self,
        request: FetchRequest<Self::Value>,
    ) -> JsResult<Self::Value, JsPromise<Self::Value>>;
    fn json(&self, response: &Self::Value) -> JsResult<Self::Value, JsPromise<Self::Value>>;
    fn response_header(&self, response: &Self::Value, name: &'static str) -> JsResult<Self::Value>;
    fn is_address(&self, value: &Self::Value) -> bool;
    fn iterator(&self, value: &Self::Value) -> JsResult<Self::Value, Self::Iterator>;
    fn next(&self, iterator: &Self::Iterator) -> JsResult<Self::Value, Option<Self::Value>>;
    fn close_iterator(
        &self,
        iterator: &Self::Iterator,
        result: JsResult<Self::Value>,
    ) -> JsResult<Self::Value>;
    fn random(&self) -> JsResult<Self::Value, f64>;
    fn now(&self) -> JsResult<Self::Value, f64>;
    fn fee_to_raw(&self, fee: &Self::Value) -> JsResult<Self::Value, f64>;
}
fn nullish<H: BuilderHost>(h: &H, v: &H::Value) -> bool {
    h.is_null(v) || h.is_undefined(v)
}
fn or<H: BuilderHost>(h: &H, v: &H::Value, a: &str, b: &str) -> JsResult<H::Value> {
    let first = h.property(v, a)?;
    if h.truthy(&first) {
        Ok(first)
    } else {
        h.property(v, b)
    }
}
fn failure<H: BuilderHost>(h: &H, message: H::Value) -> H::Value {
    h.object(vec![("success", h.boolean(false)), ("message", message)])
}
fn denied<H: BuilderHost>(h: &H) -> H::Value {
    h.object(vec![("authorized", h.boolean(false))])
}
fn headers<H: BuilderHost>(h: &H, proxy: &H::Value) -> JsResult<H::Value> {
    let value = h.object(vec![("Content-Type", h.string("application/json"))]);
    h.spread(&value, proxy)?;
    Ok(value)
}
fn session_headers<H: BuilderHost>(
    h: &H,
    proxy: &H::Value,
    auth: &H::Value,
    mandatory: bool,
) -> JsResult<H::Value> {
    let value = headers(h, proxy)?;
    // grvt.builder.ts has no identical-arm `if`: this port fuses three separate
    // TypeScript header sites into one `mandatory` flag, so the duplication was
    // the port's own. `||` short-circuits exactly as the else-if did, so the
    // truthy read still does not happen when mandatory is set.
    if mandatory || h.truthy(&h.property(auth, "accountId")?) {
        h.set(&value, "X-Grvt-Account-Id", h.property(auth, "accountId")?)?;
    }
    if h.truthy(&h.property(auth, "sessionCookie")?) {
        h.set(
            &value,
            "X-Ts-Grvt-Session",
            h.property(auth, "sessionCookie")?,
        )?;
    }
    Ok(value)
}
fn request<H: BuilderHost>(
    h: &H,
    url: H::Value,
    headers: H::Value,
    payload: H::Value,
    credentials: bool,
    controller: Option<&H::Value>,
) -> JsResult<H::Value, FetchRequest<H::Value>> {
    let body = h.stringify(&payload)?;
    let signal = controller.map(|v| h.property(v, "signal")).transpose()?;
    Ok(FetchRequest {
        url,
        method: "POST",
        headers,
        payload,
        body,
        credentials,
        signal,
    })
}
async fn json_or_empty<H: BuilderHost>(h: &H, response: &H::Value) -> JsResult<H::Value> {
    let result = h.json(response)?.await;
    let value = match result {
        Ok(v) => v,
        Err(_) => h.object(vec![]),
    };
    h.reaction().await;
    Ok(value)
}
fn valid_address<H: BuilderHost>(h: &H, values: Vec<H::Value>) -> Option<H::Value> {
    values
        .into_iter()
        .find(|v| !h.is_undefined(v) && h.is_address(v))
}
fn string_field<H: BuilderHost>(h: &H, value: H::Value) -> H::Value {
    if h.is_string(&value) {
        value
    } else {
        h.undefined()
    }
}
fn rejected<H: BuilderHost>(h: &H, code: &H::Value) -> bool {
    h.equal(code, &h.number(4001.0)) || h.equal(code, &h.string("ACTION_REJECTED"))
}
fn switch_code<H: BuilderHost>(h: &H, error: &H::Value) -> JsResult<H::Value> {
    let nested = h.property(error, "error")?;
    let code = if nullish(h, &nested) {
        h.undefined()
    } else {
        h.property(&nested, "code")?
    };
    if nullish(h, &code) {
        h.property(error, "code")
    } else {
        Ok(code)
    }
}

/// ts: getEthereumProvider.
pub fn get_ethereum_provider<H: BuilderHost>(h: &H) -> JsResult<H::Value> {
    if h.window()?.as_ref().is_none_or(|v| h.is_undefined(v)) {
        return Ok(h.null());
    }
    let window = h.read_window()?;
    let ethereum = h.property(&window, "ethereum")?;
    Ok(if h.truthy(&ethereum) {
        ethereum
    } else {
        h.null()
    })
}
/// ts: isBrowserWalletAvailable.
pub fn is_browser_wallet_available<H: BuilderHost>(h: &H) -> JsResult<H::Value, bool> {
    Ok(!h.is_null(&get_ethereum_provider(h)?))
}
/// ts: getConnectedWalletAddress.
pub fn get_connected_wallet_address<H: BuilderHost>(host: Rc<H>) -> JsPromise<H::Value> {
    let h = host.clone();
    host.start(Box::pin(async move {
        let ethereum = get_ethereum_provider(h.as_ref())?;
        if !h.truthy(&ethereum) {
            return Ok(h.null());
        }
        let result: JsResult<H::Value> = async {
            let provider = h.browser_provider(&ethereum)?;
            let accounts = h
                .provider_send(&provider, "eth_accounts", h.array(vec![]))?
                .await?;
            let first = h.property(&accounts, "0")?;
            Ok(if h.truthy(&first) { first } else { h.null() })
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(_) => Ok(h.null()),
        }
    }))
}
/// ts: requestWalletConnection.
pub fn request_wallet_connection<H: BuilderHost>(host: Rc<H>) -> JsPromise<H::Value> {
    let h = host.clone();
    host.start(Box::pin(async move {
        let ethereum = get_ethereum_provider(h.as_ref())?;
        if !h.truthy(&ethereum) {
            return Err(h.error(h.string(
                "No browser wallet detected. Please install MetaMask or another Web3 wallet.",
            )));
        }
        let result: JsResult<H::Value> = async {
            let provider = h.browser_provider(&ethereum)?;
            let accounts = h
                .provider_send(&provider, "eth_requestAccounts", h.array(vec![]))?
                .await?;
            let first = h.property(&accounts, "0")?;
            Ok(if h.truthy(&first) { first } else { h.null() })
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(error) => {
                if h.equal(&h.property(&error, "code")?, &h.number(4001.0)) {
                    return Err(h.error(h.string("Wallet connection rejected by user")));
                }
                let message = h.property(&error, "message")?;
                let message = if h.truthy(&message) {
                    message
                } else {
                    h.string("Unknown error")
                };
                Err(h.error(h.concat(vec![h.string("Failed to connect wallet: "), message])?))
            }
        }
    }))
}

/// ts: authenticateGrvt. Prefix errors reject; response/inspection failures use the source catch.
pub fn authenticate_grvt<H: BuilderHost>(
    host: Rc<H>,
    api_key: H::Value,
    testnet: bool,
    proxy: H::Value,
) -> JsPromise<H::Value> {
    let h = host.clone();
    host.start(Box::pin(async move {
        let edge = h.edge_url(testnet)?;
        let controller = h.controller()?;
        let timer = h.timeout(&controller, 10000.0)?;
        let url = h.concat(vec![edge, h.string("/auth/api_key/login")])?;
        let headers = headers(h.as_ref(), &proxy)?;
        h.set(&headers, "X-Grvt-Remember-Me", h.string("true"))?;
        let body = h.object(vec![("api_key", api_key)]);
        let result: JsResult<H::Value> = async {
            let response = h
                .fetch(request(
                    h.as_ref(),
                    url,
                    headers,
                    body,
                    true,
                    Some(&controller),
                )?)?
                .await?;
            h.clear_timeout(&timer)?;
            if !h.truthy(&h.property(&response, "ok")?) {
                let data = json_or_empty(h.as_ref(), &response).await?;
                let message = h.property(&data, "message")?;
                let message = if h.truthy(&message) {
                    message
                } else {
                    h.concat(vec![
                        h.string("Authentication failed: HTTP "),
                        h.property(&response, "status")?,
                    ])?
                };
                return Ok(failure(h.as_ref(), message));
            }
            let data = json_or_empty(h.as_ref(), &response).await?;
            let account = h.response_header(&response, "x-grvt-account-id")?;
            let mut funding =
                string_field(h.as_ref(), h.property(&data, "funding_account_address")?);
            for key in ["fundingAccountAddress", "main_account_id", "ma"] {
                if h.is_undefined(&funding) {
                    funding = string_field(h.as_ref(), h.property(&data, key)?);
                }
            }
            let cookie = h.response_header(&response, "x-ts-grvt-session")?;
            if !h.truthy(&account) {
                return Ok(failure(
                    h.as_ref(),
                    h.string("Authentication succeeded but no account ID returned"),
                ));
            }
            Ok(h.object(vec![
                ("success", h.boolean(true)),
                ("accountId", account),
                ("fundingAccountAddress", funding),
                (
                    "sessionCookie",
                    if h.truthy(&cookie) {
                        cookie
                    } else {
                        h.undefined()
                    },
                ),
            ]))
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(error) => {
                h.clear_timeout(&timer)?;
                let name = h.property(&error, "name")?;
                let message = if h.equal(&name, &h.string("AbortError")) {
                    h.string("GRVT authentication timed out (10s)")
                } else {
                    let message = h.property(&error, "message")?;
                    if h.truthy(&message) {
                        message
                    } else {
                        h.string("Failed to authenticate with GRVT")
                    }
                };
                Ok(failure(h.as_ref(), message))
            }
        }
    }))
}

/// ts: fetchGrvtMainAccountId. Finally cancellation can override a return or rejection.
pub fn fetch_grvt_main_account_id<H: BuilderHost>(
    host: Rc<H>,
    testnet: bool,
    proxy: H::Value,
    auth: H::Value,
) -> JsPromise<H::Value> {
    let h = host.clone();
    host.start(Box::pin(async move {
        let api = h.trading_url(testnet)?;
        let headers = session_headers(h.as_ref(), &proxy, &auth, false)?;
        let controller = h.controller()?;
        let timer = h.timeout(&controller, 10000.0)?;
        let result: JsResult<H::Value> = async {
            let url = h.concat(vec![api, h.string("/full/v1/aggregated_account_summary")])?;
            let payload = h.object(vec![]);
            let response = h
                .fetch(request(
                    h.as_ref(),
                    url,
                    headers,
                    payload,
                    false,
                    Some(&controller),
                )?)?
                .await?;
            if !h.truthy(&h.property(&response, "ok")?) {
                return Ok(h.undefined());
            }
            let data = json_or_empty(h.as_ref(), &response).await?;
            let result = or(h.as_ref(), &data, "result", "r")?;
            let result = if h.truthy(&result) {
                result
            } else {
                data.clone()
            };
            let candidates = [
                h.property(&result, "main_account_id")?,
                h.property(&result, "ma")?,
                h.property(&data, "main_account_id")?,
                h.property(&data, "ma")?,
            ]
            .into_iter()
            .map(|v| string_field(h.as_ref(), v))
            .collect();
            Ok(valid_address(h.as_ref(), candidates).unwrap_or_else(|| h.undefined()))
        }
        .await;
        h.clear_timeout(&timer)?;
        result
    }))
}

/// ts: checkGrvtBuilderAuthorized. Proxy credential rejection is outside the false-result catch.
pub fn check_grvt_builder_authorized<H: BuilderHost>(
    host: Rc<H>,
    api_key: H::Value,
    builder: Option<H::Value>,
    testnet: bool,
) -> JsPromise<H::Value> {
    let h = host.clone();
    host.start(Box::pin(async move {
        let builder = builder
            .filter(|v| !nullish(h.as_ref(), v))
            .unwrap_or_else(|| h.string(crate::builder::get_grvt_builder_address(testnet)));
        let proxy = h.proxy_headers()?.await?;
        let result: JsResult<H::Value> = async {
            let auth = authenticate_grvt(h.clone(), api_key, testnet, proxy.clone()).await?;
            if !h.truthy(&h.property(&auth, "success")?) {
                return Ok(denied(h.as_ref()));
            }
            let api = h.trading_url(testnet)?;
            let url = h.concat(vec![api, h.string("/full/v1/get_authorized_builders")])?;
            let headers = session_headers(h.as_ref(), &proxy, &auth, true)?;
            let payload = h.object(vec![]);
            let controller = h.controller()?;
            let timer = h.timeout(&controller, 10000.0)?;
            let response = h
                .fetch(request(
                    h.as_ref(),
                    url,
                    headers,
                    payload,
                    false,
                    Some(&controller),
                )?)?
                .await?;
            h.clear_timeout(&timer)?;
            if !h.truthy(&h.property(&response, "ok")?) {
                return Ok(denied(h.as_ref()));
            }
            let data = h.json(&response)?.await?;
            let builders = or(h.as_ref(), &data, "results", "r")?;
            let builders = if h.truthy(&builders) {
                builders
            } else {
                h.array(vec![])
            };
            let iterator = h.iterator(&builders)?;
            loop {
                let row = match h.next(&iterator) {
                    Ok(Some(v)) => v,
                    Ok(None) => break,
                    Err(e) => return Err(e),
                };
                let processed: JsResult<H::Value, Option<H::Value>> = (|| {
                    let addr = or(h.as_ref(), &row, "builder_account_id", "ba")?;
                    let addr = if h.truthy(&addr) { addr } else { h.string("") };
                    let addr = h.lowercase(&addr)?;
                    if !h.equal(&addr, &h.lowercase(&builder)?) {
                        return Ok(None);
                    }
                    let mut raw = h.property(&row, "max_futures_fee_rate")?;
                    if nullish(h.as_ref(), &raw) {
                        raw = h.property(&row, "max_fee_rate")?;
                    }
                    if nullish(h.as_ref(), &raw) {
                        raw = h.property(&row, "mfr")?;
                    }
                    let percent = if nullish(h.as_ref(), &raw) {
                        f64::NAN
                    } else {
                        h.parse_float(&h.to_string(&raw)?)?
                    };
                    let max_raw = if percent.is_finite() {
                        h.number(safe_cex_num::math_round(percent * 10000.0))
                    } else {
                        h.undefined()
                    };
                    let max_fee = if percent.is_finite() {
                        h.concat(vec![h.to_fixed(percent, 4.0)?, h.string("%")])?
                    } else {
                        h.undefined()
                    };
                    Ok(Some(h.object(vec![
                        ("authorized", h.boolean(true)),
                        ("maxFeeRate", max_fee),
                        ("maxFeeRateRaw", max_raw),
                    ])))
                })();
                match processed {
                    Ok(Some(v)) => return h.close_iterator(&iterator, Ok(v)),
                    Ok(None) => {}
                    Err(e) => return h.close_iterator(&iterator, Err(e)),
                }
            }
            Ok(denied(h.as_ref()))
        }
        .await;
        Ok(match result {
            Ok(v) => v,
            Err(_) => denied(h.as_ref()),
        })
    }))
}

/// ts: authorizeGrvtBuilderWithBrowserWallet.
pub fn authorize_grvt_builder_with_browser_wallet<H: BuilderHost>(
    host: Rc<H>,
    api_key: H::Value,
    builder: H::Value,
    fee: H::Value,
    testnet: bool,
) -> JsPromise<H::Value> {
    let h = host.clone();
    host.start(Box::pin(async move {
        let ethereum = get_ethereum_provider(h.as_ref())?;
        if !h.truthy(&ethereum) {
            return Ok(failure(h.as_ref(), h.string("No browser wallet detected")));
        }
        if !h.is_address(&builder) {
            return Ok(failure(h.as_ref(), h.string("Invalid builder address")));
        }
        if !h.truthy(&api_key) {
            return Ok(failure(
                h.as_ref(),
                h.string("API key is required for GRVT authentication"),
            ));
        }
        let proxy = h.proxy_headers()?.await?;
        let result: JsResult<H::Value> = async {
            let auth = authenticate_grvt(h.clone(), api_key, testnet, proxy.clone()).await?;
            if !h.truthy(&h.property(&auth, "success")?) {
                let message = h.property(&auth, "message")?;
                return Ok(failure(
                    h.as_ref(),
                    if h.truthy(&message) {
                        message
                    } else {
                        h.string("GRVT authentication failed")
                    },
                ));
            }
            let provider = h.browser_provider(&ethereum)?;
            let accounts = h
                .provider_send(&provider, "eth_requestAccounts", h.array(vec![]))?
                .await?;
            if !h.truthy(&accounts) || h.equal(&h.property(&accounts, "length")?, &h.number(0.0)) {
                return Ok(failure(h.as_ref(), h.string("No accounts available")));
            }
            let signer_address = h.property(&accounts, "0")?;
            let chain = if testnet { 326.0 } else { 325.0 };
            let hex = h.string(if testnet { "0x146" } else { "0x145" });
            let switched: JsResult<H::Value> = async {
                h.provider_send(
                    &provider,
                    "wallet_switchEthereumChain",
                    h.array(vec![h.object(vec![("chainId", hex.clone())])]),
                )?
                .await
            }
            .await;
            if let Err(error) = switched {
                if rejected(h.as_ref(), &switch_code(h.as_ref(), &error)?) {
                    return Ok(failure(
                        h.as_ref(),
                        h.string("Chain switch rejected by user"),
                    ));
                }
                let added: JsResult<H::Value> = async {
                    let params = h.object(vec![
                        ("chainId", hex),
                        (
                            "chainName",
                            h.string(if testnet {
                                "GRVT Testnet"
                            } else {
                                "GRVT Mainnet"
                            }),
                        ),
                        (
                            "nativeCurrency",
                            h.object(vec![
                                ("name", h.string("Ethereum")),
                                ("symbol", h.string("ETH")),
                                ("decimals", h.number(18.0)),
                            ]),
                        ),
                        (
                            "rpcUrls",
                            h.array(vec![h.string(if testnet {
                                "https://rpc.testnet.grvt.io"
                            } else {
                                "https://rpc.grvt.io"
                            })]),
                        ),
                    ]);
                    h.provider_send(&provider, "wallet_addEthereumChain", h.array(vec![params]))?
                        .await
                }
                .await;
                if let Err(error) = added {
                    if rejected(h.as_ref(), &switch_code(h.as_ref(), &error)?) {
                        return Ok(failure(
                            h.as_ref(),
                            h.string("Adding GRVT chain rejected by user"),
                        ));
                    }
                    let message = h.concat(vec![
                        h.string("Please add GRVT chain ("),
                        h.number(chain),
                        h.string(") to your wallet manually"),
                    ])?;
                    return Ok(failure(h.as_ref(), message));
                }
            }
            let nonce = (h.random()? * 1e9).floor();
            let expiration =
                h.to_string(&h.number((h.now()? + 24.0 * 60.0 * 60.0 * 1000.0) * 1000000.0))?;
            let raw = h.fee_to_raw(&fee)?;
            let domain = h.object(vec![
                ("name", h.string("GRVT Exchange")),
                ("version", h.string("0")),
                ("chainId", h.number(chain)),
            ]);
            let funding = h.property(&auth, "fundingAccountAddress")?;
            let account = h.property(&auth, "accountId")?;
            let main = match valid_address(h.as_ref(), vec![funding, account]) {
                Some(v) if h.truthy(&v) => v,
                _ => {
                    fetch_grvt_main_account_id(h.clone(), testnet, proxy.clone(), auth.clone())
                        .await?
                }
            };
            if !h.truthy(&main) {
                return Ok(failure(
                    h.as_ref(),
                    h.string("GRVT authentication did not return a valid main account ID"),
                ));
            }
            let message = h.object(vec![
                ("mainAccountID", main.clone()),
                ("builderAccountID", builder.clone()),
                ("maxFutureFeeRate", h.number(raw)),
                ("maxSpotFeeRate", h.number(raw)),
                ("nonce", h.number(nonce)),
                ("expiration", expiration.clone()),
            ]);
            let signer = h.signer(&provider)?.await?;
            let signature = h
                .sign(&signer, domain, h.authorize_builder_types(), message)?
                .await?;
            let bytes = h.slice(&signature, 2, None)?;
            let r = h.concat(vec![h.string("0x"), h.slice(&bytes, 0, Some(64))?])?;
            let s = h.concat(vec![h.string("0x"), h.slice(&bytes, 64, Some(128))?])?;
            let v = h.parse_int(&h.slice(&bytes, 128, Some(130))?, 16)?;
            let api = h.edge_url(testnet)?;
            let url = h.concat(vec![api, h.string("/auth/builder/authorize")])?;
            let headers = session_headers(h.as_ref(), &proxy, &auth, true)?;
            let signature = h.object(vec![
                ("signer", signer_address),
                ("r", r),
                ("s", s),
                ("v", h.number(v)),
                ("nonce", h.number(nonce)),
                ("expiration", expiration),
                ("chain_id", h.string(if testnet { "326" } else { "325" })),
            ]);
            let payload = h.object(vec![
                ("main_account_id", main),
                ("builder_account_id", builder),
                ("max_futures_fee_rate", fee.clone()),
                ("max_spot_fee_rate", fee),
                ("signature", signature),
            ]);
            let response = h
                .fetch(request(h.as_ref(), url, headers, payload, true, None)?)?
                .await?;
            let data = h.json(&response)?.await?;
            if !h.truthy(&h.property(&response, "ok")?) {
                let message = or(h.as_ref(), &data, "message", "m")?;
                let message = if h.truthy(&message) {
                    message
                } else {
                    h.concat(vec![h.string("HTTP "), h.property(&response, "status")?])?
                };
                return Ok(failure(h.as_ref(), message));
            }
            let success = or(h.as_ref(), &data, "success", "s")?;
            let success = if h.truthy(&success) {
                success
            } else {
                h.property(&response, "ok")?
            };
            if h.truthy(&success) {
                return Ok(h.object(vec![
                    ("success", h.boolean(true)),
                    ("message", h.string("Builder authorized successfully")),
                ]));
            }
            let message = or(h.as_ref(), &data, "message", "m")?;
            Ok(failure(
                h.as_ref(),
                if h.truthy(&message) {
                    message
                } else {
                    h.string("Unknown error")
                },
            ))
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(error) => {
                if h.equal(&h.property(&error, "code")?, &h.number(4001.0))
                    || h.equal(&h.property(&error, "code")?, &h.string("ACTION_REJECTED"))
                {
                    return Ok(failure(
                        h.as_ref(),
                        h.string("Signature request rejected by user"),
                    ));
                }
                let message = h.property(&error, "message")?;
                Ok(failure(
                    h.as_ref(),
                    if h.truthy(&message) {
                        message
                    } else {
                        h.string("Failed to authorize builder")
                    },
                ))
            }
        }
    }))
}
