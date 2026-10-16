//! Aster Axios construction, signing interceptors and response recovery.
use crate::runtime_host::{ResultValue, RuntimeHost, awaited, copy, emit_log, invoke, opt, or};
use std::rc::Rc;
fn option<H: RuntimeHost>(h: &H, options: &H::Value, key: &str) -> ResultValue<H::Value> {
    h.get(options, key)
}
fn clock<H: RuntimeHost>(h: &H, key: &str, args: Vec<H::Value>) -> ResultValue<H::Value> {
    invoke!(h, h.constant("asterDEXVirtualClock"), key, args)
}
fn sync<H: RuntimeHost>(h: &H, options: &H::Value) -> ResultValue<H::Value> {
    invoke!(
        h,
        h.constant("asterDEXVirtualClock"),
        "syncClientServerTime",
        vec![
            option(h, options, "testnet")?,
            option(h, options, "apiVersion")?
        ]
    )
}

fn config<H: RuntimeHost>(h: &H, testnet: H::Value) -> ResultValue<H::Value> {
    let rest = h.get(&h.constant("ASTERDEX_ENDPOINTS"), "REST")?;
    Ok(h.object(vec![
        (
            "baseURL",
            h.get(
                &rest,
                if h.truthy(&testnet) {
                    "TESTNET"
                } else {
                    "PRODUCTION"
                },
            )?,
        ),
        ("timeout", h.number(10000.)),
        (
            "headers",
            h.object(vec![("Content-Type", h.text("application/json"))]),
        ),
    ]))
}
fn install_retry<H: RuntimeHost>(h: Rc<H>, api: H::Value) -> ResultValue<H::Value, ()> {
    let host = h.clone();
    let delay = h.function(Rc::new(move |a| {
        host.import(
            "multiply",
            vec![
                a.first().cloned().unwrap_or_else(|| host.undefined()),
                host.number(1000.),
            ],
        )
    }));
    let host = h.clone();
    let predicate = h.function(Rc::new(move |a| {
        let error = a.first().cloned().unwrap_or_else(|| host.undefined());
        let network = host.import(
            "axiosRetry.isNetworkOrIdempotentRequestError",
            vec![error.clone()],
        )?;
        if host.truthy(&network) {
            return Ok(network);
        }
        let response = host.get(&error, "response")?;
        if host.strict_equal(&opt(host.as_ref(), response, "status")?, &host.number(503.)) {
            return Ok(host.boolean(true));
        }
        Ok(host.boolean(host.strict_equal(
            &opt(host.as_ref(), host.get(&error, "response")?, "status")?,
            &host.number(504.),
        )))
    }));
    let read = h.constant("isAsterdexRead");
    let condition = h.import("onlyReads", vec![predicate, read])?;
    h.import(
        "axiosRetry",
        vec![
            api,
            h.object(vec![
                ("retries", h.number(3.)),
                ("retryDelay", delay),
                ("retryCondition", condition),
            ]),
        ],
    )?;
    Ok(())
}
fn rate<H: RuntimeHost>(h: &H, api: H::Value, max: f64) -> ResultValue<H::Value> {
    h.import(
        "axiosRateLimit",
        vec![
            api,
            h.object(vec![
                ("maxRequests", h.number(max)),
                ("perMilliseconds", h.number(1000.)),
            ]),
        ],
    )
}
/// ts: createPublicAPI.
pub fn create_public_api<H: RuntimeHost>(h: Rc<H>, exchange: H::Value) -> ResultValue<H::Value> {
    let testnet = opt(
        h.as_ref(),
        opt(h.as_ref(), exchange.clone(), "options")?,
        "testnet",
    )?;
    let settings = config(h.as_ref(), testnet)?;
    let api = if h.truthy(&exchange) {
        h.import(
            "safeCexAxiosCreate",
            vec![settings, h.get(&exchange, "options")?, exchange.clone()],
        )?
    } else {
        h.import("axios.create", vec![settings])?
    };
    if h.truthy(&opt(
        h.as_ref(),
        opt(h.as_ref(), exchange.clone(), "options")?,
        "tealstreetTunnel",
    )?) {
        h.import(
            "installTransportAdapter",
            vec![
                api.clone(),
                h.get(&exchange, "options")?,
                h.get(&exchange, "name")?,
            ],
        )?;
    }
    install_retry(h.clone(), api.clone())?;
    rate(h.as_ref(), api, 10.)
}
async fn signed_request<H: RuntimeHost>(
    h: Rc<H>,
    exchange: H::Value,
    options: H::Value,
    config: H::Value,
) -> ResultValue<H::Value> {
    let url = or(h.as_ref(), h.get(&config, "url")?, || Ok(h.text("")))?;
    let path = h.import("stripQueryString", vec![url])?;
    let public = h.import("isPublicEndpoint", vec![path.clone()])?;
    if h.truthy(&h.get(&h.get(&exchange, "options")?, "remoteSigner")?) && !h.truthy(&public) {
        return Ok(config);
    }
    if !h.truthy(&public) && h.truthy(&h.get(&options, "apiKey")?) {
        if h.truthy(&clock(h.as_ref(), "needsSync", vec![])?) {
            if h.truthy(&exchange) {
                emit_log!(
                    h.as_ref(),
                    exchange.clone(),
                    h.text("Time sync needed, syncing with server..."),
                    "Info",
                )?;
            }
            awaited(h.as_ref(), sync(h.as_ref(), &options)).await?;
        }
        let headers = or(h.as_ref(), h.get(&config, "headers")?, || {
            Ok(h.object(vec![]))
        })?;
        h.set(&config, "headers", headers.clone())?;
        h.set(
            &h.get(&config, "headers")?,
            "X-MBX-APIKEY",
            h.get(&options, "apiKey")?,
        )?;
        let timestamp = clock(h.as_ref(), "getCurrentTimeMs", vec![])?;
        // A hosted resend never recorded its unsigned params, so it still carries
        // the previous attempt's signing fields.
        let data = or(h.as_ref(), h.get(&config, "__asterUnsignedParams")?, || {
            let params = or(h.as_ref(), h.get(&config, "data")?, || {
                or(h.as_ref(), h.get(&config, "params")?, || {
                    Ok(h.object(vec![]))
                })
            })?;
            h.import("withoutSigningFields", vec![params])
        })?;
        h.set(
            &config,
            "__asterUnsignedParams",
            copy(h.as_ref(), data.clone())?,
        )?;
        let params = copy(h.as_ref(), data.clone())?;
        h.define(&params, "recvWindow", h.number(20000.))?;
        h.define(&params, "timestamp", timestamp)?;
        let url = h.get(&config, "url")?;
        let is_v3 = if h.nullish(&url) {
            h.boolean(false)
        } else {
            invoke!(h.as_ref(), url, "includes", vec![h.text("/fapi/v3/")])?
        };
        let signature;
        if h.truthy(&is_v3) {
            let user = h.get(&options, "v3UserAddress")?;
            let signer = h.get(&options, "v3SignerAddress")?;
            let key = h.get(&options, "v3SignerPrivateKey")?;
            if !h.truthy(&user) || !h.truthy(&signer) || !h.truthy(&key) {
                return Err(h.error(h.text("AsterDEX v3 signing requires auth.userAddress, auth.signerAddress, and signer private key (auth.privateKey or secret)")));
            }
            if !h.truthy(&h.import("isAsterDEXV3PrivateKey", vec![key.clone()])?) {
                return Err(
                    h.error(h.text("AsterDEX v3 signing requires a 64-byte hex private key"))
                );
            }
            let user = h.import("ethers.getAddress", vec![user])?;
            let signer = h.import("ethers.getAddress", vec![signer])?;
            let scope = h.template(vec![user.clone(), h.text(":"), signer.clone()])?;
            let nonce = h.import("buildV3Nonce", vec![scope])?;
            if h.truthy(&h.import(
                "isAsterDEXCodeSignedEndpoint",
                vec![path.clone(), h.get(&config, "method")?],
            )?) {
                let params = copy(h.as_ref(), data)?;
                h.define(
                    &params,
                    "asterChain",
                    h.import("getAsterDEXAsterChain", vec![h.get(&options, "testnet")?])?,
                )?;
                for (key, value) in [("user", user), ("signer", signer), ("nonce", nonce)] {
                    h.define(&params, key, value)?;
                }
                let query = h.import("buildAsterDEXCodeQueryString", vec![params])?;
                signature = h.import(
                    "generateAsterDEXCodeMessageSignature",
                    vec![query.clone(), key, h.get(&options, "testnet")?],
                )?;
                h.set(
                    &config,
                    "url",
                    h.template(vec![
                        path,
                        h.text("?"),
                        query,
                        h.text("&signature="),
                        signature,
                    ])?,
                )?;
                h.delete(&config, "params")?;
                if h.truthy(&h.get(&config, "data")?) {
                    h.delete(&config, "data")?;
                }
                return Ok(config);
            }
            signature = generate_v3_signature(
                h.as_ref(),
                params.clone(),
                user.clone(),
                signer.clone(),
                key,
                nonce.clone(),
            )?;
            for (key, value) in [("user", user), ("signer", signer), ("nonce", nonce)] {
                h.set(&params, key, value)?;
            }
        } else {
            if !h.truthy(&h.get(&options, "apiSecret")?) {
                return Err(h.error(h.text("AsterDEX v1 signing requires API secret")));
            }
            let query = h.import("buildQueryString", vec![params.clone()])?;
            signature = h.import(
                "generateV1Signature",
                vec![query, h.get(&options, "apiSecret")?],
            )?;
        }
        h.set(&params, "signature", signature)?;
        h.set(&config, "params", params)?;
        if h.truthy(&h.get(&config, "data")?) {
            h.delete(&config, "data")?;
        }
    }
    Ok(config)
}
fn detail<H: RuntimeHost>(h: &H, data: &H::Value, field: &str) -> ResultValue<H::Value> {
    h.import(
        "getAsterDEXErrorDetail",
        vec![h.get(data, field)?, opt(h, data.clone(), "code")?],
    )
}
// ts: resendV1SignatureRejection. V3 signs with a wallet key, where a rejection is
// not drift a resend fixes; the request interceptor re-signs from __asterUnsignedParams.
fn resend_v1_signature_rejection<H: RuntimeHost>(
    h: &H,
    api: &H::Value,
    config: H::Value,
    data: H::Value,
) -> ResultValue<H::Value> {
    if !h.truthy(&config) {
        return Ok(h.undefined());
    }
    let url = h.get(&config, "url")?;
    let v3 = if h.nullish(&url) {
        h.undefined()
    } else {
        invoke!(h, url, "includes", vec![h.text("/fapi/v3/")])?
    };
    if h.truthy(&v3) || h.truthy(&h.import("hasRetriedSignature", vec![config.clone()])?) {
        return Ok(h.undefined());
    }
    if !h.truthy(&h.import("isBinanceStyleSignatureRejection", vec![data])?) {
        return Ok(h.undefined());
    }
    let next = copy(h, config)?;
    h.define(
        &next,
        safe_cex_transport::signature_rejection::SIGNATURE_RETRY_CONFIG_KEY,
        h.boolean(true),
    )?;
    invoke!(h, api.clone(), "request", vec![next])
}
async fn response_ok<H: RuntimeHost>(
    h: Rc<H>,
    exchange: H::Value,
    api: H::Value,
    response: H::Value,
) -> ResultValue<H::Value> {
    let resend = resend_v1_signature_rejection(
        h.as_ref(),
        &api,
        h.get(&response, "config")?,
        h.get(&response, "data")?,
    )?;
    if h.truthy(&resend) {
        return Ok(resend);
    }
    let data = h.get(&response, "data")?;
    if h.truthy(&opt(h.as_ref(), data.clone(), "code")?)
        && h.less(h.get(&h.get(&response, "data")?, "code")?, h.number(0.))?
    {
        let text = h.template(vec![
            h.text("AsterDEX API Error: "),
            h.import(
                "getAsterDEXErrorDetail",
                vec![
                    h.get(&h.get(&response, "data")?, "msg")?,
                    h.get(&h.get(&response, "data")?, "code")?,
                ],
            )?,
            h.text(" (code: "),
            h.get(&h.get(&response, "data")?, "code")?,
            h.text(")"),
        ])?;
        let text = h.import("appendHttpFailure", vec![text, response.clone()])?;
        let suppress = h.import(
            "shouldSuppressUnknownOrderError",
            vec![
                h.get(&response, "config")?,
                h.get(&h.get(&response, "data")?, "code")?,
                h.get(&h.get(&response, "data")?, "msg")?,
            ],
        )?;
        if !h.truthy(&suppress) && h.truthy(&exchange) {
            invoke!(h.as_ref(), exchange, "emitAndLogError", vec![text.clone()])?;
        }
        return Err(h.error(text));
    }
    Ok(response)
}
async fn response_error<H: RuntimeHost>(
    h: Rc<H>,
    exchange: H::Value,
    options: H::Value,
    api: H::Value,
    error: H::Value,
) -> ResultValue<H::Value> {
    if h.truthy(&h.get(&error, "response")?) {
        let fields = h.destructure(
            h.get(&error, "response")?,
            &["status", "data"],
            "error.response",
        )?;
        let status = fields[0].clone();
        let data = fields[1].clone();
        let resend = resend_v1_signature_rejection(
            h.as_ref(),
            &api,
            h.get(&error, "config")?,
            data.clone(),
        )?;
        if h.truthy(&resend) {
            return Ok(resend);
        }
        if h.strict_equal(&opt(h.as_ref(), data.clone(), "code")?, &h.number(-1021.)) {
            let msg = opt(h.as_ref(), data.clone(), "msg")?;
            let includes = if h.nullish(&msg) {
                h.undefined()
            } else {
                invoke!(h.as_ref(), msg, "includes", vec![h.text("Timestamp")])?
            };
            if h.truthy(&includes) {
                if h.truthy(&exchange) {
                    emit_log!(
                        h.as_ref(),
                        exchange.clone(),
                        h.text("Timestamp error detected, resyncing with server time..."),
                        "Warning",
                    )?;
                }
                awaited(h.as_ref(), sync(h.as_ref(), &options)).await?;
                if !h.truthy(&h.get(&h.get(&error, "config")?, "__isRetryRequest")?) {
                    h.set(
                        &h.get(&error, "config")?,
                        "__isRetryRequest",
                        h.boolean(true),
                    )?;
                    if h.truthy(&h.get(&h.get(&error, "config")?, "params")?) {
                        for key in [
                            "timestamp",
                            "signature",
                            "recvWindow",
                            "user",
                            "signer",
                            "nonce",
                        ] {
                            h.delete(&h.get(&h.get(&error, "config")?, "params")?, key)?;
                        }
                    }
                    if h.truthy(&exchange) {
                        emit_log!(
                            h.as_ref(),
                            exchange.clone(),
                            h.text("Retrying request with synced timestamp..."),
                            "Info",
                        )?;
                    }
                    return invoke!(h.as_ref(), api, "request", vec![h.get(&error, "config")?]);
                }
            }
        }
        let mut text = h.template(vec![h.text("AsterDEX API Error: "), status.clone()])?;
        if h.truthy(&opt(h.as_ref(), data.clone(), "msg")?) {
            text = h.template(vec![text, h.text(" - "), detail(h.as_ref(), &data, "msg")?])?;
        } else if h.truthy(&opt(h.as_ref(), data.clone(), "error")?) {
            text = h.template(vec![
                text,
                h.text(" - "),
                detail(h.as_ref(), &data, "error")?,
            ])?;
        }
        if h.truthy(&opt(h.as_ref(), data.clone(), "code")?) {
            text = h.template(vec![
                text,
                h.text(" (code: "),
                h.get(&data, "code")?,
                h.text(")"),
            ])?;
        }
        let suppress = h.import(
            "shouldSuppressUnknownOrderError",
            vec![
                h.get(&error, "config")?,
                opt(h.as_ref(), data.clone(), "code")?,
                or(h.as_ref(), opt(h.as_ref(), data.clone(), "msg")?, || {
                    opt(h.as_ref(), data.clone(), "error")
                })?,
            ],
        )?;
        if !h.truthy(&suppress) && h.truthy(&exchange) {
            invoke!(
                h.as_ref(),
                exchange.clone(),
                "emitAndLogError",
                vec![text.clone()],
            )?;
            let selected = if h.strict_equal(&status, &h.number(401.)) {
                Some(("Authentication failed. Check API credentials.", "Error"))
            } else if h.strict_equal(&status, &h.number(429.)) {
                Some((
                    "Rate limit exceeded. Reducing request frequency.",
                    "Warning",
                ))
            } else if h.strict_equal(&status, &h.number(503.))
                || h.strict_equal(&status, &h.number(504.))
            {
                Some(("Service temporarily unavailable. Will retry.", "Warning"))
            } else {
                None
            };
            if let Some((text, severity)) = selected {
                emit_log!(h.as_ref(), exchange, h.text(text), severity)?;
            }
        }
        return Err(h.error(text));
    }
    Err(error)
}
/// ts: createAPI.
pub fn create_api<H: RuntimeHost>(
    h: Rc<H>,
    exchange: H::Value,
    options: H::Value,
    transport: H::Value,
) -> ResultValue<H::Value> {
    let options = if h.strict_equal(&options, &h.undefined()) {
        h.object(vec![])
    } else {
        options
    };
    let transport = if h.strict_equal(&transport, &h.undefined()) {
        h.get(&exchange, "options")?
    } else {
        transport
    };
    let api = h.import(
        "safeCexAxiosCreate",
        vec![
            config(h.as_ref(), h.get(&options, "testnet")?)?,
            transport.clone(),
            exchange.clone(),
        ],
    )?;
    if h.truthy(&h.get(&transport, "tealstreetTunnel")?) {
        h.import(
            "installTransportAdapter",
            vec![api.clone(), transport, h.get(&exchange, "name")?],
        )?;
    }
    let host = h.clone();
    let ex = exchange.clone();
    let opts = options.clone();
    let request = h.function(Rc::new(move |args| {
        let h = host.clone();
        let ex = ex.clone();
        let opts = opts.clone();
        let config = args.first().cloned().unwrap_or_else(|| h.undefined());
        let task = host.start(Box::pin(async move {
            signed_request(h, ex, opts, config).await
        }));
        Ok(host.promise(task))
    }));
    let host = h.clone();
    let rejected = h.function(Rc::new(move |args| {
        host.import(
            "Promise.reject",
            vec![args.first().cloned().unwrap_or_else(|| host.undefined())],
        )
    }));
    invoke!(
        h.as_ref(),
        h.get(&h.get(&api, "interceptors")?, "request")?,
        "use",
        vec![request, rejected],
    )?;
    let host = h.clone();
    let ex = exchange.clone();
    let a = api.clone();
    let fulfilled = h.function(Rc::new(move |args| {
        let h = host.clone();
        let ex = ex.clone();
        let api = a.clone();
        let response = args.first().cloned().unwrap_or_else(|| h.undefined());
        let task = host.start(Box::pin(
            async move { response_ok(h, ex, api, response).await },
        ));
        Ok(host.promise(task))
    }));
    let host = h.clone();
    let ex = exchange.clone();
    let a = api.clone();
    let rejected = h.function(Rc::new(move |args| {
        let h = host.clone();
        let ex = ex.clone();
        let opts = options.clone();
        let api = a.clone();
        let error = args.first().cloned().unwrap_or_else(|| h.undefined());
        let task = host.start(Box::pin(async move {
            response_error(h, ex, opts, api, error).await
        }));
        Ok(host.promise(task))
    }));
    invoke!(
        h.as_ref(),
        h.get(&h.get(&api, "interceptors")?, "response")?,
        "use",
        vec![fulfilled, rejected],
    )?;
    install_retry(h.clone(), api.clone())?;
    rate(h.as_ref(), api, 20.)
}
fn aster_options<H: RuntimeHost>(h: &H, options: H::Value) -> ResultValue<H::Value> {
    let auth = h.import("resolveAsterDEXV3AuthContext", vec![options.clone()])?;
    Ok(h.object(vec![
        ("apiKey", h.get(&options, "key")?),
        ("apiSecret", h.get(&options, "secret")?),
        (
            "apiVersion",
            h.import("resolveAsterDEXApiVersion", vec![options.clone()])?,
        ),
        ("v3UserAddress", h.get(&auth, "userAddress")?),
        ("v3SignerAddress", h.get(&auth, "signerAddress")?),
        ("v3SignerPrivateKey", h.get(&auth, "signerPrivateKey")?),
        ("testnet", h.get(&options, "testnet")?),
    ]))
}
/// ts: createAPIInstances.
pub fn create_api_instances<H: RuntimeHost>(h: Rc<H>, exchange: H::Value) -> ResultValue<H::Value> {
    let options = aster_options(h.as_ref(), h.get(&exchange, "options")?)?;
    let host = h.clone();
    let ex = exchange.clone();
    let factory = h.function(Rc::new(move |args| {
        let o = args.first().cloned().unwrap_or_else(|| host.undefined());
        create_api(
            host.clone(),
            ex.clone(),
            aster_options(host.as_ref(), o.clone())?,
            o,
        )
    }));
    let pair = h.import(
        "createApiWithSecondaryKey",
        vec![h.get(&exchange, "options")?, exchange.clone(), factory],
    )?;
    let primary = h.import("iteratorFirst", vec![pair])?;
    let secondary = rate(
        h.as_ref(),
        create_api(h.clone(), exchange.clone(), options.clone(), h.undefined())?,
        5.,
    )?;
    let unlimited = create_api(h.clone(), exchange.clone(), options, h.undefined())?;
    let public = create_public_api(h.clone(), exchange)?;
    Ok(h.object(vec![
        ("primary", primary),
        ("secondary", secondary),
        ("unlimited", unlimited),
        ("public", public),
    ]))
}

/// ts: generateV3Signature.
pub fn generate_v3_signature<H: RuntimeHost>(
    h: &H,
    params: H::Value,
    user: H::Value,
    signer: H::Value,
    key: H::Value,
    nonce: H::Value,
) -> ResultValue<H::Value> {
    let payload = h.import("buildV3SigningPayload", vec![params])?;
    let serialized = h.import("JSON.stringify", vec![payload])?;
    let encoded = h.import(
        "ethers.AbiCoder.encode",
        vec![
            h.array(vec![
                h.text("string"),
                h.text("address"),
                h.text("address"),
                h.text("uint256"),
            ]),
            h.array(vec![
                serialized,
                user,
                signer,
                h.import("BigInt", vec![nonce])?,
            ]),
        ],
    )?;
    let hash = h.import("ethers.keccak256", vec![encoded])?;
    let bytes = h.import("ethers.getBytes", vec![hash])?;
    let digest = h.import("ethers.hashMessage", vec![bytes])?;
    let prefixed = invoke!(h, key.clone(), "startsWith", vec![h.text("0x")])?;
    let normalized = if h.truthy(&prefixed) {
        key
    } else {
        h.template(vec![h.text("0x"), key])?
    };
    let signing_key = h.import("new ethers.SigningKey", vec![normalized])?;
    let signature = invoke!(h, signing_key, "sign", vec![digest])?;
    let signature = h.import("ethers.Signature.from", vec![signature])?;
    h.get(&signature, "serialized")
}
