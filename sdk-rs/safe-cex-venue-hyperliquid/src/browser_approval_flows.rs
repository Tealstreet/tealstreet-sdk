//! Browser agent/builder approval source bodies. Provider/network/ethers/JSON
//! primitives are supplied bindings; async scope is immediate first-poll/host await.
use crate::{
    order_request_leaves::{OrderRequestHost, RequestComparison},
    spot_json::JsString,
};
pub trait BrowserApprovalHost: OrderRequestHost {
    /// typeof an absent global returns "undefined" rather than ReferenceError.
    fn global_typeof(&mut self, name: &str) -> Result<Self::Value, Self::Error>;
    fn construct_value(
        &mut self,
        c: &Self::Value,
        args: &[Self::Value],
    ) -> Result<Self::Value, Self::Error>;
    fn error_value(&self, error: &Self::Error) -> Self::Value;
}
fn lit<H: BrowserApprovalHost>(h: &H, s: &str) -> H::Value {
    h.literal(&JsString(s.encode_utf16().collect()))
}
fn method<H: BrowserApprovalHost>(
    h: &mut H,
    o: &H::Value,
    n: &str,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    let f = h.get(o, n)?;
    h.call(&f, o, args)
}
fn free<H: BrowserApprovalHost>(
    h: &mut H,
    n: &str,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    let f = h.global(n)?;
    let u = h.undefined();
    h.call(&f, &u, args)
}
fn text_field<H: BrowserApprovalHost>(
    h: &mut H,
    o: &H::Value,
    n: &str,
    s: &str,
) -> Result<(), H::Error> {
    let v = lit(h, s);
    h.define_data(o, n, &v)
}
fn default_false<H: BrowserApprovalHost>(h: &H, v: &H::Value) -> H::Value {
    if h.is_undefined(v) {
        h.boolean(false)
    } else {
        v.clone()
    }
}
fn default_global<H: BrowserApprovalHost>(
    h: &mut H,
    v: &H::Value,
    n: &str,
) -> Result<H::Value, H::Error> {
    if h.is_undefined(v) {
        h.global(n)
    } else {
        Ok(v.clone())
    }
}
fn ethereum<H: BrowserApprovalHost>(h: &mut H) -> Result<H::Value, H::Error> {
    let w = h.global("window")?;
    h.get(&w, "ethereum")
}
fn available<H: BrowserApprovalHost>(h: &mut H) -> Result<bool, H::Error> {
    let t = h.global_typeof("window")?;
    let undefined = lit(h, "undefined");
    if h.strict_equal(&t, &undefined) {
        return Ok(false);
    }
    let e = ethereum(h)?;
    Ok(h.truthy(&e))
}
fn success_only<H: BrowserApprovalHost>(h: &mut H) -> Result<H::Value, H::Error> {
    let o = h.object()?;
    let b = h.boolean(true);
    h.define_data(&o, "success", &b)?;
    Ok(o)
}
fn pair_failure<H: BrowserApprovalHost>(
    h: &mut H,
    status: &str,
    field: &str,
    message: H::Value,
) -> Result<H::Value, H::Error> {
    let o = h.object()?;
    let b = h.boolean(false);
    h.define_data(&o, status, &b)?;
    h.define_data(&o, field, &message)?;
    Ok(o)
}
fn catch_result<H: BrowserApprovalHost>(
    h: &mut H,
    e: &H::Error,
    status: &str,
    field: &str,
    fallback: &str,
) -> Result<H::Value, H::Error> {
    let o = h.object()?;
    let no = h.boolean(false);
    h.define_data(&o, status, &no)?;
    let message = error_message(h, e, fallback)?;
    h.define_data(&o, field, &message)?;
    Ok(o)
}
fn catch_browser<H: BrowserApprovalHost>(
    h: &mut H,
    e: &H::Error,
    field: &str,
    rejection: &str,
    fallback: &str,
) -> Result<H::Value, H::Error> {
    if rejected(h, e)? {
        let msg = lit(h, rejection);
        pair_failure(h, "success", field, msg)
    } else {
        catch_result(h, e, "success", field, fallback)
    }
}
fn error_instance<H: BrowserApprovalHost>(h: &mut H, error: &H::Error) -> Result<bool, H::Error> {
    let c = h.global("Error")?;
    h.instance_of(error, &c)
}
fn error_message<H: BrowserApprovalHost>(
    h: &mut H,
    error: &H::Error,
    fallback: &str,
) -> Result<H::Value, H::Error> {
    if error_instance(h, error)? {
        let v = h.error_value(error);
        h.get(&v, "message")
    } else {
        Ok(lit(h, fallback))
    }
}
async fn network_attempt<H: BrowserApprovalHost>(
    h: &mut H,
    method_name: &str,
    value: &H::Value,
) -> bool {
    let result = async {
        let e = ethereum(h)?;
        let request = h.get(&e, "request")?;
        let args = h.object()?;
        text_field(h, &args, "method", method_name)?;
        let params = h.array()?;
        h.array_define(&params, 0, value)?;
        h.define_data(&args, "params", &params)?;
        let p = h.call(&request, &e, &[args])?;
        h.await_value(p).await?;
        Ok::<(), H::Error>(())
    }
    .await;
    result.is_ok()
}
async fn switch_network<H: BrowserApprovalHost>(
    h: &mut H,
    testnet: &H::Value,
    agent: bool,
) -> Result<H::Value, H::Error> {
    if !available(h)? {
        let m = lit(h, "No Ethereum wallet detected");
        return pair_failure(h, "success", "error", m);
    }
    let target = h.global(match (agent, h.truthy(testnet)) {
        (true, true) => "agent.ARBITRUM_SEPOLIA_CHAIN",
        (true, false) => "agent.ARBITRUM_ONE_CHAIN",
        (false, true) => "builder.ARBITRUM_SEPOLIA_CHAIN",
        (false, false) => "builder.ARBITRUM_ONE_CHAIN",
    })?;
    let id = h.get(&target, "chainId")?;
    // Local helper closure allocation has no outward effect; own both bodies.
    async fn attempt_switch<H: BrowserApprovalHost>(h: &mut H, id: &H::Value) -> bool {
        let r = async {
            let e = ethereum(h)?;
            let request = h.get(&e, "request")?;
            let args = h.object()?;
            text_field(h, &args, "method", "wallet_switchEthereumChain")?;
            let params = h.array()?;
            let chain = h.object()?;
            h.define_data(&chain, "chainId", id)?;
            h.array_define(&params, 0, &chain)?;
            h.define_data(&args, "params", &params)?;
            let p = h.call(&request, &e, &[args])?;
            h.await_value(p).await?;
            Ok::<(), H::Error>(())
        }
        .await;
        r.is_ok()
    }
    if attempt_switch(h, &id).await {
        return success_only(h);
    }
    if network_attempt(h, "wallet_addEthereumChain", &target).await && attempt_switch(h, &id).await
    {
        return success_only(h);
    }
    let denied = h.object()?;
    let no = h.boolean(false);
    h.define_data(&denied, "success", &no)?;
    let name = h.get(&target, "chainName")?;
    let mut text = "Please manually switch to "
        .encode_utf16()
        .collect::<Vec<_>>();
    text.extend(h.template_string(&name)?.0);
    text.extend(
        if agent {
            " in your wallet before proceeding"
        } else {
            " in your wallet before approving"
        }
        .encode_utf16(),
    );
    let message = h.literal(&JsString(text));
    h.define_data(&denied, "error", &message)?;
    Ok(denied)
}
pub async fn agent_switch_to_correct_network<H: BrowserApprovalHost>(
    h: &mut H,
    testnet: &H::Value,
) -> Result<H::Value, H::Error> {
    switch_network(h, testnet, true).await
}
pub async fn builder_switch_to_correct_network<H: BrowserApprovalHost>(
    h: &mut H,
    testnet: &H::Value,
) -> Result<H::Value, H::Error> {
    switch_network(h, testnet, false).await
}
async fn sign<H: BrowserApprovalHost>(
    h: &mut H,
    signer: &H::Value,
    address: &H::Value,
    name_or_rate: &H::Value,
    testnet: &H::Value,
    nonce: &H::Value,
    agent: bool,
) -> Result<H::Value, H::Error> {
    let nonce = if h.is_nullish(nonce) {
        let date = h.global("Date")?;
        method(h, &date, "now", &[])?
    } else {
        nonce.clone()
    };
    let domain = h.global(if h.truthy(testnet) {
        "HYPERLIQUID_BUILDER_DOMAIN_TESTNET"
    } else {
        "HYPERLIQUID_BUILDER_DOMAIN"
    })?;
    let message = h.object()?;
    text_field(
        h,
        &message,
        "hyperliquidChain",
        if h.truthy(testnet) {
            "Testnet"
        } else {
            "Mainnet"
        },
    )?;
    if agent {
        h.define_data(&message, "agentAddress", address)?;
        h.define_data(&message, "agentName", name_or_rate)?;
    } else {
        h.define_data(&message, "maxFeeRate", name_or_rate)?;
        h.define_data(&message, "builder", address)?;
    }
    h.define_data(&message, "nonce", &nonce)?;
    let f = h.get(signer, "signTypedData")?;
    let types = h.global(if agent {
        "APPROVE_AGENT_TYPES"
    } else {
        "APPROVE_BUILDER_FEE_TYPES"
    })?;
    let p = h.call(&f, signer, &[domain, types, message])?;
    let raw = h.await_value(p).await?;
    let ethers = h.global("ethers")?;
    let signature = h.get(&ethers, "Signature")?;
    let parsed = method(h, &signature, "from", &[raw])?;
    let r = h.get(&parsed, "r")?;
    let s = h.get(&parsed, "s")?;
    let v = h.get(&parsed, "v")?;
    let result = h.object()?;
    let sig = h.object()?;
    h.define_data(&sig, "r", &r)?;
    h.define_data(&sig, "s", &s)?;
    h.define_data(&sig, "v", &v)?;
    h.define_data(&result, "signature", &sig)?;
    h.define_data(&result, "nonce", &nonce)?;
    Ok(result)
}
pub async fn sign_agent_approval<H: BrowserApprovalHost>(
    h: &mut H,
    signer: &H::Value,
    address: &H::Value,
    name: &H::Value,
    testnet: &H::Value,
    nonce: &H::Value,
) -> Result<H::Value, H::Error> {
    let net = default_false(h, testnet);
    sign(h, signer, address, name, &net, nonce, true).await
}
pub async fn sign_builder_fee_approval<H: BrowserApprovalHost>(
    h: &mut H,
    signer: &H::Value,
    address: &H::Value,
    rate: &H::Value,
    testnet: &H::Value,
    nonce: &H::Value,
) -> Result<H::Value, H::Error> {
    let rate = default_global(h, rate, "DEFAULT_BUILDER_FEE_RATE")?;
    let net = default_false(h, testnet);
    sign(h, signer, address, &rate, &net, nonce, false).await
}
fn base_url<H: BrowserApprovalHost>(h: &mut H, testnet: &H::Value) -> Result<H::Value, H::Error> {
    let urls = h.global("BASE_URLS")?;
    h.get(
        &urls,
        if h.truthy(testnet) {
            "TESTNET"
        } else {
            "PRODUCTION"
        },
    )
}
fn builder<H: BrowserApprovalHost>(
    h: &mut H,
    address: &H::Value,
    testnet: &H::Value,
) -> Result<H::Value, H::Error> {
    if h.is_nullish(address) {
        free(h, "getBuilderAddress", &[testnet.clone()])
    } else {
        Ok(address.clone())
    }
}
fn post_headers<H: BrowserApprovalHost>(h: &mut H, config: &H::Value) -> Result<(), H::Error> {
    text_field(h, config, "method", "POST")?;
    let headers = h.object()?;
    text_field(h, &headers, "Content-Type", "application/json")?;
    h.define_data(config, "headers", &headers)
}
async fn submit<H: BrowserApprovalHost>(
    h: &mut H,
    address: &H::Value,
    name_or_rate: &H::Value,
    nonce: &H::Value,
    signature: &H::Value,
    testnet: &H::Value,
    agent: bool,
) -> Result<H::Value, H::Error> {
    let base = base_url(h, testnet)?;
    let chain = lit(
        h,
        if h.truthy(testnet) {
            "Testnet"
        } else {
            "Mainnet"
        },
    );
    let chain_id = lit(
        h,
        if h.truthy(testnet) {
            "0x66eee"
        } else {
            "0xa4b1"
        },
    );
    let result = async {
        let fetch = h.global("fetch")?;
        let mut url = h.template_string(&base)?.0;
        url.extend("/exchange".encode_utf16());
        let url = h.literal(&JsString(url));
        let config = h.object()?;
        post_headers(h, &config)?;
        let json = h.global("JSON")?;
        let stringify = h.get(&json, "stringify")?;
        let body = h.object()?;
        let action = h.object()?;
        text_field(
            h,
            &action,
            "type",
            if agent {
                "approveAgent"
            } else {
                "approveBuilderFee"
            },
        )?;
        h.define_data(&action, "hyperliquidChain", &chain)?;
        h.define_data(&action, "signatureChainId", &chain_id)?;
        if agent {
            h.define_data(&action, "agentAddress", address)?;
            h.define_data(&action, "agentName", name_or_rate)?;
        } else {
            h.define_data(&action, "builder", address)?;
            h.define_data(&action, "maxFeeRate", name_or_rate)?;
        }
        h.define_data(&action, "nonce", nonce)?;
        h.define_data(&body, "action", &action)?;
        h.define_data(&body, "signature", signature)?;
        h.define_data(&body, "nonce", nonce)?;
        let body = h.call(&stringify, &json, &[body])?;
        h.define_data(&config, "body", &body)?;
        let u = h.undefined();
        let p = h.call(&fetch, &u, &[url, config])?;
        let response = h.await_value(p).await?;
        let p = method(h, &response, "json", &[])?;
        let parsed = h.await_value(p).await?;
        let status = h.get(&parsed, "status")?;
        let err = lit(h, "err");
        if h.strict_equal(&status, &err) {
            let denied = h.object()?;
            let no = h.boolean(false);
            h.define_data(&denied, "success", &no)?;
            let message = h.get(&parsed, "response")?;
            let message = if h.truthy(&message) {
                message
            } else {
                lit(
                    h,
                    if agent {
                        "Failed to approve agent wallet"
                    } else {
                        "Failed to approve builder fee"
                    },
                )
            };
            h.define_data(&denied, "message", &message)?;
            return Ok(denied);
        }
        let o = h.object()?;
        let yes = h.boolean(true);
        h.define_data(&o, "success", &yes)?;
        text_field(
            h,
            &o,
            "message",
            if agent {
                "Agent wallet approved successfully"
            } else {
                "Builder fee approved successfully"
            },
        )?;
        if agent {
            h.define_data(&o, "agentAddress", address)?;
        }
        Ok(o)
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => catch_result(
            h,
            &e,
            "success",
            "message",
            if agent {
                "Failed to submit agent approval"
            } else {
                "Failed to submit approval"
            },
        ),
    }
}

pub async fn submit_agent_approval<H: BrowserApprovalHost>(
    h: &mut H,
    address: &H::Value,
    name: &H::Value,
    nonce: &H::Value,
    signature: &H::Value,
    testnet: &H::Value,
) -> Result<H::Value, H::Error> {
    let net = default_false(h, testnet);
    submit(h, address, name, nonce, signature, &net, true).await
}
pub async fn submit_builder_fee_approval<H: BrowserApprovalHost>(
    h: &mut H,
    address: &H::Value,
    rate: &H::Value,
    nonce: &H::Value,
    signature: &H::Value,
    testnet: &H::Value,
) -> Result<H::Value, H::Error> {
    let net = default_false(h, testnet);
    submit(h, address, rate, nonce, signature, &net, false).await
}
pub fn create_builder_info<H: BrowserApprovalHost>(
    h: &mut H,
    testnet: &H::Value,
    address: &H::Value,
    fee: &H::Value,
) -> Result<H::Value, H::Error> {
    let net = default_false(h, testnet);
    let fee = default_global(h, fee, "DEFAULT_BUILDER_FEE_TENTHS_BPS")?;
    let builder = builder(h, address, &net)?;
    let o = h.object()?;
    let lower = method(h, &builder, "toLowerCase", &[])?;
    h.define_data(&o, "b", &lower)?;
    h.define_data(&o, "f", &fee)?;
    Ok(o)
}
pub async fn check_builder_fee_approved<H: BrowserApprovalHost>(
    h: &mut H,
    user: &H::Value,
    address: &H::Value,
    testnet: &H::Value,
    required: &H::Value,
) -> Result<H::Value, H::Error> {
    let net = default_false(h, testnet);
    let required = default_global(h, required, "TEALSTREET_BUILDER_FEE")?;
    let base = base_url(h, &net)?;
    let builder = builder(h, address, &net)?;
    let result = async {
        let fetch = h.global("fetch")?;
        let mut url = h.template_string(&base)?.0;
        url.extend("/info".encode_utf16());
        let url = h.literal(&JsString(url));
        let config = h.object()?;
        post_headers(h, &config)?;
        let json = h.global("JSON")?;
        let stringify = h.get(&json, "stringify")?;
        let body = h.object()?;
        text_field(h, &body, "type", "maxBuilderFee")?;
        let user = method(h, user, "toLowerCase", &[])?;
        h.define_data(&body, "user", &user)?;
        let builder = method(h, &builder, "toLowerCase", &[])?;
        h.define_data(&body, "builder", &builder)?;
        let body = h.call(&stringify, &json, &[body])?;
        h.define_data(&config, "body", &body)?;
        let u = h.undefined();
        let p = h.call(&fetch, &u, &[url, config])?;
        let response = h.await_value(p).await?;
        let ok = h.get(&response, "ok")?;
        if !h.truthy(&ok) {
            let o = h.object()?;
            let no = h.boolean(false);
            h.define_data(&o, "approved", &no)?;
            let status = h.get(&response, "status")?;
            let mut msg = "Hyperliquid returned ".encode_utf16().collect::<Vec<_>>();
            msg.extend(h.template_string(&status)?.0);
            let msg = h.literal(&JsString(msg));
            h.define_data(&o, "error", &msg)?;
            return Ok(o);
        }
        let p = method(h, &response, "text", &[])?;
        let raw = h.await_value(p).await?;
        let value = free(h, "parseFloat", &[raw])?;
        let nan = free(h, "isNaN", &[value.clone()])?;
        if h.truthy(&nan) {
            let m = lit(h, "Unexpected response from Hyperliquid");
            return pair_failure(h, "approved", "error", m);
        }
        let percentage = free(h, "builderFeeRateFromTenthsBps", &[value.clone()])?;
        let o = h.object()?;
        let approved = h.compare(RequestComparison::GreaterEqual, &value, &required)?;
        h.define_data(&o, "approved", &approved)?;
        h.define_data(&o, "maxFeeRate", &percentage)?;
        h.define_data(&o, "maxFeeRateTenthsBps", &value)?;
        Ok(o)
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            let console = h.global("console")?;
            let message = lit(h, "Failed to check builder fee approval:");
            let value = h.error_value(&e);
            method(h, &console, "error", &[message, value])?;
            catch_result(h, &e, "approved", "error", "Failed to reach Hyperliquid")
        }
    }
}

async fn provider<H: BrowserApprovalHost>(h: &mut H) -> Result<H::Value, H::Error> {
    let ethers = h.global("ethers")?;
    let constructor = h.get(&ethers, "BrowserProvider")?;
    let ethereum = ethereum(h)?;
    let provider = h.construct_value(&constructor, &[ethereum])?;
    let send = h.get(&provider, "send")?;
    let method = lit(h, "eth_requestAccounts");
    let args = h.array()?;
    let p = h.call(&send, &provider, &[method, args])?;
    h.await_value(p).await?;
    let p = method_call_signer(h, &provider)?;
    h.await_value(p).await
}
fn method_call_signer<H: BrowserApprovalHost>(
    h: &mut H,
    provider: &H::Value,
) -> Result<H::Value, H::Error> {
    method(h, provider, "getSigner", &[])
}
async fn address<H: BrowserApprovalHost>(
    h: &mut H,
    signer: &H::Value,
) -> Result<H::Value, H::Error> {
    let p = method(h, signer, "getAddress", &[])?;
    h.await_value(p).await
}
fn wrong_wallet<H: BrowserApprovalHost>(
    h: &mut H,
    expected: &H::Value,
    actual: &H::Value,
) -> Result<H::Value, H::Error> {
    let mut out = "Wrong wallet connected. Expected "
        .encode_utf16()
        .collect::<Vec<_>>();
    for (v, n, suffix) in [
        (expected, 8.0, "..."),
        (expected, -6.0, ", got "),
        (actual, 8.0, "..."),
        (actual, -6.0, ". Please connect the correct wallet."),
    ] {
        let first = h.number(if n > 0.0 { 0.0 } else { n });
        let args = if n > 0.0 {
            vec![first, h.number(n)]
        } else {
            vec![first]
        };
        let piece = method(h, v, "slice", &args)?;
        out.extend(h.template_string(&piece)?.0);
        out.extend(suffix.encode_utf16());
    }
    Ok(h.literal(&JsString(out)))
}
fn rejected<H: BrowserApprovalHost>(h: &mut H, e: &H::Error) -> Result<bool, H::Error> {
    if !error_instance(h, e)? {
        return Ok(false);
    }
    let e = h.error_value(e);
    let message = h.get(&e, "message")?;
    let pattern = lit(h, "user rejected");
    let yes = method(h, &message, "includes", &[pattern])?;
    Ok(h.truthy(&yes))
}
pub async fn connect_browser_wallet<H: BrowserApprovalHost>(
    h: &mut H,
) -> Result<H::Value, H::Error> {
    let yes = free(h, "hasEthereumWallet", &[])?;
    if !h.truthy(&yes) {
        let msg = lit(
            h,
            "No Ethereum wallet detected. Please install MetaMask or another wallet.",
        );
        return pair_failure(h, "success", "error", msg);
    }
    let result = async {
        let signer = provider(h).await?;
        let addr = address(h, &signer).await?;
        let o = h.object()?;
        let yes = h.boolean(true);
        h.define_data(&o, "success", &yes)?;
        h.define_data(&o, "address", &addr)?;
        Ok(o)
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => catch_browser(
            h,
            &e,
            "error",
            "Wallet connection was rejected",
            "Failed to connect wallet",
        ),
    }
}

async fn browser_flow<H: BrowserApprovalHost>(
    h: &mut H,
    expected: &H::Value,
    testnet: &H::Value,
    builder_or_name: &H::Value,
    rate: &H::Value,
    agent: bool,
) -> Result<H::Value, H::Error> {
    let result = async {
        let (agent_address, agent_key) = if agent {
            let ethers = h.global("ethers")?;
            let wallet = h.get(&ethers, "Wallet")?;
            let wallet = method(h, &wallet, "createRandom", &[])?;
            (h.get(&wallet, "address")?, h.get(&wallet, "privateKey")?)
        } else {
            (h.undefined(), h.undefined())
        };
        let network = free(h, "switchToCorrectNetwork", &[testnet.clone()])?;
        let network = h.await_value(network).await?;
        let success = h.get(&network, "success")?;
        if !h.truthy(&success) {
            let denied = h.object()?;
            let no = h.boolean(false);
            h.define_data(&denied, "success", &no)?;
            let msg = h.get(&network, "error")?;
            let msg = if h.truthy(&msg) {
                msg
            } else {
                lit(h, "Failed to switch to the correct network")
            };
            h.define_data(&denied, "message", &msg)?;
            return Ok(denied);
        }
        let signer = provider(h).await?;
        let actual = address(h, &signer).await?;
        let actual_lower = method(h, &actual, "toLowerCase", &[])?;
        let expected_lower = method(h, expected, "toLowerCase", &[])?;
        if !h.strict_equal(&actual_lower, &expected_lower) {
            let denied = h.object()?;
            let no = h.boolean(false);
            h.define_data(&denied, "success", &no)?;
            let msg = wrong_wallet(h, expected, &actual)?;
            h.define_data(&denied, "message", &msg)?;
            return Ok(denied);
        }
        let sign_args = if agent {
            vec![
                signer.clone(),
                agent_address.clone(),
                builder_or_name.clone(),
                testnet.clone(),
            ]
        } else {
            vec![
                signer.clone(),
                builder_or_name.clone(),
                rate.clone(),
                testnet.clone(),
            ]
        };
        let p = free(
            h,
            if agent {
                "signAgentApproval"
            } else {
                "signBuilderFeeApproval"
            },
            &sign_args,
        )?;
        let signed = h.await_value(p).await?;
        let signature = h.get(&signed, "signature")?;
        let nonce = h.get(&signed, "nonce")?;
        let submit_args = if agent {
            vec![
                agent_address.clone(),
                builder_or_name.clone(),
                nonce,
                signature,
                testnet.clone(),
            ]
        } else {
            vec![
                builder_or_name.clone(),
                rate.clone(),
                nonce,
                signature,
                testnet.clone(),
            ]
        };
        let p = free(
            h,
            if agent {
                "submitAgentApproval"
            } else {
                "submitBuilderFeeApproval"
            },
            &submit_args,
        )?;
        let result = h.await_value(p).await?;
        let success = h.get(&result, "success")?;
        if h.truthy(&success) {
            if agent {
                let o = h.object()?;
                let yes = h.boolean(true);
                h.define_data(&o, "success", &yes)?;
                text_field(h, &o, "message", "Agent wallet created successfully")?;
                h.define_data(&o, "agentAddress", &agent_address)?;
                h.define_data(&o, "agentPrivateKey", &agent_key)?;
                return Ok(o);
            }
            let report = h.global("reportEvent")?;
            let event = lit(h, "hyperliquid.builder-fee-approved");
            let payload = h.object()?;
            h.define_data(&payload, "wallet", &actual)?;
            h.define_data(&payload, "testnet", testnet)?;
            let u = h.undefined();
            h.call(&report, &u, &[event, payload])?;
        }
        Ok(result)
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => catch_browser(
            h,
            &e,
            "message",
            if agent {
                "Agent wallet creation was rejected by user"
            } else {
                "Approval was rejected by user"
            },
            if agent {
                "Failed to create agent wallet"
            } else {
                "Failed to approve builder fee"
            },
        ),
    }
}

pub async fn create_agent_wallet_with_browser_wallet<H: BrowserApprovalHost>(
    h: &mut H,
    expected: &H::Value,
    testnet: &H::Value,
    name: &H::Value,
) -> Result<H::Value, H::Error> {
    let net = default_false(h, testnet);
    if !available(h)? {
        let msg = lit(
            h,
            "No Ethereum wallet detected. Please install MetaMask or another wallet.",
        );
        return pair_failure(h, "success", "message", msg);
    }
    let name = if h.truthy(name) {
        name.clone()
    } else {
        free(h, "generateAgentWalletName", &[])?
    };
    let rate = h.undefined();
    browser_flow(h, expected, &net, &name, &rate, true).await
}
pub async fn approve_builder_fee_with_browser_wallet<H: BrowserApprovalHost>(
    h: &mut H,
    expected: &H::Value,
    testnet: &H::Value,
    builder_address: &H::Value,
    rate: &H::Value,
) -> Result<H::Value, H::Error> {
    let net = default_false(h, testnet);
    let rate = default_global(h, rate, "DEFAULT_BUILDER_FEE_RATE")?;
    let builder = builder(h, builder_address, &net)?;
    if !available(h)? {
        let msg = lit(
            h,
            "No Ethereum wallet detected. Please install MetaMask or another wallet.",
        );
        return pair_failure(h, "success", "message", msg);
    }
    browser_flow(h, expected, &net, &builder, &rate, false).await
}
