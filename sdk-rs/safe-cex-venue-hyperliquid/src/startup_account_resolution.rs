//! Primary account discovery and shared-flight spot collateral refresh bodies.
use crate::{
    helper_tasks::{self, HelperObserver},
    spot_account_reads::SpotAccountHost,
    spot_json::JsString,
};
fn lit<H: SpotAccountHost>(h: &H, s: &str) -> H::Value {
    h.literal(&JsString(s.encode_utf16().collect()))
}
fn method<H: SpotAccountHost>(
    h: &mut H,
    owner: &H::Value,
    key: &str,
    args: impl FnOnce(&mut H) -> Result<Vec<H::Value>, H::Error>,
) -> Result<H::Value, H::Error> {
    let callee = h.get(owner, key)?;
    let args = args(h)?;
    h.call(&callee, owner, &args)
}
fn imported<H: SpotAccountHost>(h: &mut H, owner: &str, key: &str) -> Result<H::Value, H::Error> {
    let object = h.imported(owner)?;
    h.get(&object, key)
}
fn user<H: SpotAccountHost>(h: &mut H, e: &H::Value) -> Result<H::Value, H::Error> {
    let options = h.get(e, "options")?;
    let vault = h.get(&options, "vault")?;
    if h.truthy(&vault) {
        Ok(vault)
    } else {
        let options = h.get(e, "options")?;
        h.get(&options, "applicationId")
    }
}
fn request<H: SpotAccountHost>(
    h: &mut H,
    kind: &str,
    user: &H::Value,
) -> Result<(H::Value, H::Value), H::Error> {
    let body = h.object()?;
    let kind = imported(h, "INFO_TYPES", kind)?;
    h.define_data(&body, "type", &kind)?;
    h.define_data(&body, "user", user)?;
    let config = h.object()?;
    let headers = h.object()?;
    h.define_data(&headers, "Content-Type", &lit(h, "application/json"))?;
    h.define_data(&config, "headers", &headers)?;
    Ok((body, config))
}
fn template<H: SpotAccountHost>(
    h: &mut H,
    prefix: &str,
    value: &H::Value,
    suffix: &str,
) -> Result<H::Value, H::Error> {
    let mut text: Vec<u16> = prefix.encode_utf16().collect();
    text.extend(h.template_string(value)?.0);
    text.extend(suffix.encode_utf16());
    Ok(h.literal(&JsString(text)))
}
async fn post<H: SpotAccountHost>(
    h: &mut H,
    e: &H::Value,
    body: H::Value,
    config: H::Value,
) -> Result<H::Value, H::Error> {
    let xhr = h.get(e, "infoXhr")?;
    let response = method(h, &xhr, "post", |h| {
        Ok(vec![imported(h, "ENDPOINTS", "INFO")?, body, config])
    })?;
    let response = h.await_value(response).await?;
    h.response_data(&response)
}
async fn abstraction_body<H: SpotAccountHost>(mut h: H, e: H::Value) -> Result<H::Value, H::Error>
where
    H::Value: 'static,
    H::Error: 'static,
{
    let user = user(&mut h, &e)?;
    if !h.truthy(&user) {
        return Ok(h.undefined());
    }
    let (body, config) = request(&mut h, "USER_ABSTRACTION", &user)?;
    let fallback = lit(&h, "standard");
    let timeout = h.get(&e, "withStartupTimeout")?;
    let mut inner = h.clone();
    let owner = e.clone();
    let fallback_inner = fallback.clone();
    let fetch = h.spawn_native(Box::pin(async move {
        let result = async {
            let data = post(&mut inner, &owner, body, config).await?;
            let mode = if inner.truthy(&data) {
                data
            } else {
                lit(&inner, "")
            };
            let mode = template(&mut inner, "", &mode, "")?;
            for allowed in [
                "unifiedAccount",
                "portfolioMargin",
                "disabled",
                "default",
                "dexAbstraction",
            ] {
                if inner.strict_equal(&mode, &lit(&inner, allowed)) {
                    return Ok(mode);
                }
            }
            method(&mut inner, &owner, "log", |h| {
                Ok(vec![
                    template(
                        h,
                        "userAbstraction returned unexpected mode=",
                        &mode,
                        "; defaulting to standard",
                    )?,
                    imported(h, "LogSeverity", "Warning")?,
                ])
            })?;
            Ok(fallback_inner.clone())
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(error) => {
                let error = inner.error_value(&error);
                method(&mut inner, &owner, "log", |h| {
                    Ok(vec![
                        template(h, "userAbstraction resolution failed: ", &error, "")?,
                        imported(h, "LogSeverity", "Warning")?,
                    ])
                })?;
                Ok(fallback_inner)
            }
        }
    }));
    let ms = imported(
        &mut h,
        "HyperliquidExchange",
        "USER_ABSTRACTION_RESOLVE_TIMEOUT_MS",
    )?;
    let pending = h.call(
        &timeout,
        &e,
        &[fetch, ms, lit(&h, "userAbstraction resolution"), fallback],
    )?;
    let mode = h.await_value(pending).await?;
    h.set(&e, "userAbstractionMode", &mode)?;
    method(&mut h, &e, "log", |h| {
        let mode = h.get(&e, "userAbstractionMode")?;
        let message = template(
            h,
            "Detected Hyperliquid account abstraction mode: ",
            &mode,
            "",
        )?;
        let use_spot = method(h, &e, "shouldUseSpotCollateralBalance", |_| Ok(vec![]))?;
        let severity = imported(
            h,
            "LogSeverity",
            if h.truthy(&use_spot) { "Info" } else { "Debug" },
        )?;
        Ok(vec![message, severity])
    })?;
    Ok(h.undefined())
}
async fn role_body<H: SpotAccountHost>(mut h: H, e: H::Value) -> Result<H::Value, H::Error>
where
    H::Value: 'static,
    H::Error: 'static,
{
    let options = h.get(&e, "options")?;
    let application = h.get(&options, "applicationId")?;
    if h.truthy(&application) {
        return Ok(h.undefined());
    }
    let options = h.get(&e, "options")?;
    let vault = h.get(&options, "vault")?;
    if h.truthy(&vault) {
        return Ok(h.undefined());
    }
    let options = h.get(&e, "options")?;
    let key = h.get(&options, "key")?;
    if !h.truthy(&key) {
        return Ok(h.undefined());
    }
    let body = h.object()?;
    let kind = imported(&mut h, "INFO_TYPES", "USER_ROLE")?;
    h.define_data(&body, "type", &kind)?;
    let options = h.get(&e, "options")?;
    let key = h.get(&options, "key")?;
    h.define_data(&body, "user", &key)?;
    let config = h.object()?;
    let headers = h.object()?;
    h.define_data(&headers, "Content-Type", &lit(&h, "application/json"))?;
    h.define_data(&config, "headers", &headers)?;
    let timeout = h.get(&e, "withStartupTimeout")?;
    let mut inner = h.clone();
    let owner = e.clone();
    let fetch = h.spawn_native(Box::pin(async move {
        match post(&mut inner, &owner, body, config).await {
            Ok(v) => Ok(v),
            Err(error) => {
                let error = inner.error_value(&error);
                method(&mut inner, &owner, "log", |h| {
                    Ok(vec![
                        template(h, "userRole resolution failed: ", &error, "")?,
                        imported(h, "LogSeverity", "Warning")?,
                    ])
                })?;
                Ok(inner.null())
            }
        }
    }));
    let ms = imported(
        &mut h,
        "HyperliquidExchange",
        "USER_ROLE_RESOLVE_TIMEOUT_MS",
    )?;
    let pending = h.call(
        &timeout,
        &e,
        &[fetch, ms, lit(&h, "userRole resolution"), h.null()],
    )?;
    let data = h.await_value(pending).await?;
    if !h.truthy(&data) {
        return Ok(h.undefined());
    }
    let opts = h.get(&e, "options")?;
    let role = if h.is_nullish(&data) {
        h.undefined()
    } else {
        h.get(&data, "role")?
    };
    if h.strict_equal(&role, &lit(&h, "user")) || h.strict_equal(&role, &lit(&h, "vault")) {
        let vault = h.strict_equal(&role, &lit(&h, "vault"));
        let options = h.get(&e, "options")?;
        let key = h.get(&options, "key")?;
        h.set(&opts, if vault { "vault" } else { "applicationId" }, &key)?;
        let message = if vault {
            "Resolved vault from userRole=vault (key is vault wallet)"
        } else {
            "Resolved applicationId from userRole=user (key is master wallet)"
        };
        method(&mut h, &e, "log", |h| {
            Ok(vec![lit(h, message), imported(h, "LogSeverity", "Info")?])
        })?;
    } else if h.strict_equal(&role, &lit(&h, "agent"))
        || h.strict_equal(&role, &lit(&h, "subAccount"))
    {
        let agent = h.strict_equal(&role, &lit(&h, "agent"));
        let nested = if h.is_nullish(&data) {
            h.undefined()
        } else {
            h.get(&data, "data")?
        };
        let master = if h.is_nullish(&nested) {
            h.undefined()
        } else {
            h.get(&nested, if agent { "user" } else { "master" })?
        };
        if h.truthy(&master) {
            h.set(&opts, "applicationId", &master)?;
            method(&mut h, &e, "log", |h| {
                Ok(vec![
                    template(
                        h,
                        if agent {
                            "Resolved applicationId from userRole=agent: master="
                        } else {
                            "Resolved applicationId from userRole=subAccount: master="
                        },
                        &master,
                        "",
                    )?,
                    imported(h, "LogSeverity", "Info")?,
                ])
            })?;
        } else {
            method(&mut h, &e, "emitAndLogError", |h| {
                Ok(vec![lit(
                    h,
                    if agent {
                        "Hyperliquid recognises this wallet as an agent but did not return its master address. Please re-add the account."
                    } else {
                        "Hyperliquid recognises this wallet as a subaccount but did not return its master address. Please re-add the account."
                    },
                )])
            })?;
        }
    } else if h.strict_equal(&role, &lit(&h, "missing")) {
        method(&mut h, &e, "emitAndLogError", |h| {
            let options = h.get(&e, "options")?;
            let key = h.get(&options, "key")?;
            Ok(vec![template(
                h,
                "Hyperliquid does not recognise wallet ",
                &key,
                ". The wallet has no deposits, agent approvals, or subaccount linkage on this network. Please re-add the account using the correct wallet address (and main wallet if this is an agent key).",
            )?])
        })?;
    } else {
        method(&mut h, &e, "log", |h| {
            Ok(vec![
                template(
                    h,
                    "userRole returned unexpected role=",
                    &role,
                    "; leaving applicationId unresolved",
                )?,
                imported(h, "LogSeverity", "Warning")?,
            ])
        })?;
    }
    Ok(h.undefined())
}
async fn collateral_body<H: SpotAccountHost>(
    mut h: H,
    e: H::Value,
    force: H::Value,
) -> Result<H::Value, H::Error>
where
    H::Value: 'static,
    H::Error: 'static,
{
    let track = method(&mut h, &e, "shouldTrackSpotHoldings", |_| Ok(vec![]))?;
    if !h.truthy(&track) {
        return Ok(h.undefined());
    }
    let user = user(&mut h, &e)?;
    if !h.truthy(&user) {
        return Ok(h.undefined());
    }
    let date = h.global("Date")?;
    let now = method(&mut h, &date, "now", |_| Ok(vec![]))?;
    if !h.truthy(&force) {
        let last = h.get(&e, "lastSpotCollateralRefreshAt")?;
        let diff = h.binary("-", &now, &last)?;
        let limit = imported(
            &mut h,
            "HyperliquidExchange",
            "SPOT_COLLATERAL_REFRESH_INTERVAL_MS",
        )?;
        let less = h.binary("<", &diff, &limit)?;
        if h.truthy(&less) {
            return Ok(h.undefined());
        }
    }
    let flight = h.get(&e, "spotCollateralRefreshInFlight")?;
    if h.truthy(&flight) {
        return h.get(&e, "spotCollateralRefreshInFlight");
    }
    h.set(&e, "lastSpotCollateralRefreshAt", &now)?;
    let mut inner = h.clone();
    let owner = e.clone();
    let promise = h.spawn_native(Box::pin(async move {
        let result = async {
            // The source builds this request inside its inner try, after selecting post.
            let xhr = inner.get(&owner, "infoXhr")?;
            let response = method(&mut inner, &xhr, "post", |h| {
                let url = imported(h, "ENDPOINTS", "INFO")?;
                let (body, config) = request(h, "SPOT_CLEARINGHOUSE_STATE", &user)?;
                Ok(vec![url, body, config])
            })?;
            let response = inner.await_value(response).await?;
            let data = inner.response_data(&response)?;
            let disposed = inner.get(&owner, "isDisposed")?;
            if inner.truthy(&disposed) {
                return Ok(inner.undefined());
            }
            method(
                &mut inner,
                &owner,
                "handleSpotClearinghouseStateUpdate",
                |h| {
                    let data = if h.truthy(&data) {
                        data
                    } else {
                        let object = h.object()?;
                        let empty = h.array(&[])?;
                        h.define_data(&object, "balances", &empty)?;
                        object
                    };
                    Ok(vec![data, lit(h, "rest")])
                },
            )?;
            Ok(inner.undefined())
        }
        .await;
        let result = match result {
            Ok(v) => Ok(v),
            Err(error) => {
                let error = inner.error_value(&error);
                method(&mut inner, &owner, "log", |h| {
                    Ok(vec![
                        template(h, "Unified spot collateral refresh failed: ", &error, "")?,
                        imported(h, "LogSeverity", "Warning")?,
                    ])
                })
                .map(|_| inner.undefined())
            }
        };
        inner.set(&owner, "spotCollateralRefreshInFlight", &inner.null())?;
        result
    }));
    h.set(&e, "spotCollateralRefreshInFlight", &promise)?;
    h.get(&e, "spotCollateralRefreshInFlight")
}
// ts: HyperliquidExchange.resolveUserAbstractionMode
pub fn resolve_user_abstraction_mode<H: SpotAccountHost>(
    host: H,
    owner: H::Value,
) -> HelperObserver<H::Value, H::Error>
where
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let h = host.clone();
    helper_tasks::start(&host, abstraction_body(h, owner))
}
// ts: HyperliquidExchange.refreshUnifiedSpotCollateralBalance
pub fn refresh_unified_spot_collateral_balance<H: SpotAccountHost>(
    host: H,
    owner: H::Value,
    force: H::Value,
) -> HelperObserver<H::Value, H::Error>
where
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let h = host.clone();
    helper_tasks::start(&host, collateral_body(h, owner, force))
}
// ts: HyperliquidExchange.resolveApplicationIdFromKey
pub fn resolve_application_id_from_key<H: SpotAccountHost>(
    host: H,
    owner: H::Value,
) -> HelperObserver<H::Value, H::Error>
where
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let h = host.clone();
    helper_tasks::start(&host, role_body(h, owner))
}
