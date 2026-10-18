//! The GrvtClient/TDG methods consumed by the venue, over its configured clients.
use super::{
    runtime::{Host, thrown},
    schema,
};
use crate::signing_runtime::SigningHost;
use safe_cex_exchange::js_runtime::{
    promise::{Promise, await_js},
    scope::scope_of,
    value::{Js, Thrown, arg},
};
use std::rc::Rc;

fn text(value: &Js) -> Result<String, Thrown> {
    Ok(String::from_utf16_lossy(&value.to_js_string()?))
}

fn parse_cookie(response: &Js, decode: impl Fn(Js) -> Result<Js, Thrown>) -> Result<Js, Thrown> {
    if !response.get("status")?.strict_equals(&Js::Number(200.0)) {
        return Ok(Js::Null);
    }
    let headers = response.get("headers")?;
    let mut raw = headers.get("set-cookie")?;
    if !raw.truthy() {
        raw = headers.get("Set-Cookie")?;
    }
    if raw.is_array() {
        raw = raw.get("0")?;
    }
    let Some(raw) = raw.as_str() else {
        return Ok(Js::Null);
    };
    let raw = String::from_utf16_lossy(raw);
    // set-cookie-parser splits a combined string only where the next token's
    // first separator is '='; the comma inside an Expires date is not a split.
    let mut cookies = Vec::new();
    let mut start = 0;
    for (at, ch) in raw.char_indices() {
        if ch != ',' {
            continue;
        }
        let tail = &raw[at + 1..];
        let next = safe_cex_num::js_trim_start(tail);
        if next.chars().find(|ch| matches!(ch, '=' | ';' | ',')) == Some('=') {
            cookies.push(&raw[start..at]);
            start = raw.len() - next.len();
        }
    }
    cookies.push(&raw[start..]);
    let mut selected = None;
    for cookie in cookies {
        let mut parts = cookie
            .split(';')
            .filter(|s| !safe_cex_num::js_trim(s).is_empty());
        let Some(first) = parts.next() else { continue };
        let Some((name, raw_value)) = first.split_once('=') else {
            continue;
        };
        if name != "gravity" {
            continue;
        }
        let mut expires = None;
        for part in parts {
            let (key, value) = part.split_once('=').unwrap_or((part, ""));
            if safe_cex_num::js_trim(key).eq_ignore_ascii_case("expires") {
                expires = Some(
                    safe_cex_num::js_date_parse(safe_cex_num::js_trim(value), |ms| ms) / 1000.0,
                );
            }
        }
        // Failed URI decoding leaves the parser's original value, including
        // quote characters. Missing expiry and Invalid Date stay distinct.
        let gravity = decode(Js::str(raw_value)).unwrap_or_else(|_| Js::str(raw_value));
        selected = expires.map(|expires| (gravity, expires));
        break;
    }
    let Some((gravity, expires)) = selected else {
        return Ok(Js::Null);
    };
    let mut account = headers.get("x-grvt-account-id")?;
    if !account.truthy() {
        account = headers.get("X-Grvt-Account-Id")?;
    }
    Ok(Js::object(vec![
        ("gravity", gravity),
        ("expires", Js::Number(expires)),
        (
            "XGrvtAccountId",
            if account.truthy() {
                account
            } else {
                Js::Undefined
            },
        ),
    ]))
}

async fn authenticated(host: Host, this: Js) -> Result<Js, Thrown> {
    let config = this.get("config")?;
    let key = config.get("apiKey")?;
    let cookie = this.get("cookie")?;
    let fresh = if cookie.truthy() {
        cookie.get("expires")?.to_number()? - host.0.now() / 1000.0 > 5.0
    } else {
        false
    };
    if key.truthy() && !fresh {
        let edge = config.get("customEdgeAxios")?;
        // The source helper catches every login/cookie failure and caches null.
        let result = async {
            let pending = edge.call_method(
                "post",
                vec![
                    Js::str("/auth/api_key/login"),
                    Js::object(vec![("api_key", key)]),
                    Js::object(vec![(
                        "headers",
                        Js::object(vec![("Content-Type", Js::str("application/json"))]),
                    )]),
                ],
            )?;
            parse_cookie(&await_js(&host.0.exchange_runtime(), pending).await?, |v| {
                thrown(host.global_call("decodeURIComponent", vec![v]))
            })
        }
        .await;
        let cookie = match result {
            Ok(cookie) => cookie,
            Err(error) => {
                host.0.shared.runtime.console(
                    "error",
                    &format!("Error getting cookie: {}", error.message()),
                );
                Js::Null
            }
        };
        this.set("cookie", cookie)?;
    }
    let headers = Js::object(vec![("Content-Type", Js::str("application/json"))]);
    let cookie = this.get("cookie")?;
    if !cookie.is_nullish() {
        let gravity = cookie.get("gravity")?;
        if gravity.truthy() {
            headers.define("Cookie", Js::str(&format!("gravity={}", text(&gravity)?)))?;
        }
        let account = cookie.get("XGrvtAccountId")?;
        if account.truthy() {
            headers.define("X-Grvt-Account-Id", account)?;
        }
    }
    Ok(Js::object(vec![("headers", headers)]))
}

fn cover_error(error: Thrown) -> Result<Js, Thrown> {
    if error.value.is_nullish() {
        return Err(error);
    }
    let response = error.value.get("response")?;
    if !response.is_nullish() {
        let data = response.get("data")?;
        if data.truthy() {
            response.set("data", schema::map(&data, "error", false)?)?;
        }
    }
    // The thrown value and its Axios metadata keep their identity.
    Err(error)
}

pub(super) fn create(host: &Host, config: Js) -> Result<Js, Thrown> {
    let env = text(&config.get("env")?)?;
    let domain = match env.as_str() {
        "PRODUCTION" => "grvt.io",
        "TESTNET" => "testnet.grvt.io",
        "STAGING" => "staging.gravitymarkets.io",
        "DEV" => "dev.gravitymarkets.io",
        _ => {
            return Err(Thrown::error(
                "Error",
                &format!("Unknown environment: {env}"),
            ));
        }
    };
    let secret = config.get("apiSecret")?;
    let wallet = if secret.truthy() {
        thrown(host.new_wallet(secret))?
    } else {
        Js::Undefined
    };
    let client = Js::object(vec![
        ("config", config.clone()),
        ("cookie", Js::Null),
        ("domain", Js::str(domain)),
        ("wallet", wallet),
    ]);
    let tdg = Js::object(vec![]);
    let xhr = config.get("customAxios")?;
    let weak = Rc::downgrade(&host.0.shared);
    let runtime = host.0.exchange_runtime();
    client.define(
        "authenticatedEndpoint",
        Js::function("authenticatedEndpoint", move |this, _| {
            let host = Host(scope_of(&weak)?);
            Ok(Js::Promise(Promise::spawn(
                &runtime,
                Box::pin(authenticated(host, this)),
            )))
        }),
    )?;
    for (name, path, request, response) in [
        (
            "createOrder",
            "create_order",
            "create_request",
            "order_response",
        ),
        ("cancelOrder", "cancel_order", "cancel", "order_response"),
        (
            "cancelAllOrders",
            "cancel_all_orders",
            "scope",
            "cancel_all_response",
        ),
        ("positions", "positions", "scope", "positions_response"),
        ("openOrders", "open_orders", "scope", "orders_response"),
        (
            "getAllInitialLeverage",
            "get_all_initial_leverage",
            "account",
            "leverage_response",
        ),
        (
            "setInitialLeverage",
            "set_initial_leverage",
            "set_leverage",
            "success_response",
        ),
        (
            "fundingAccountSummary",
            "funding_account_summary",
            "",
            "funding_response",
        ),
    ] {
        let xhr = xhr.clone();
        let runtime = host.0.exchange_runtime();
        tdg.define(
            name,
            Js::function(name, move |_, args| {
                let body = if name == "fundingAccountSummary" {
                    Js::Null
                } else {
                    schema::map(&arg(&args, 0), request, true)?
                };
                let config = arg(
                    &args,
                    if name == "fundingAccountSummary" {
                        0
                    } else {
                        1
                    },
                );
                let pending = xhr.call_method(
                    "post",
                    vec![Js::str(&format!("/lite/v1/{path}")), body, config],
                );
                let await_runtime = runtime.clone();
                Ok(Js::Promise(Promise::spawn(
                    &runtime,
                    Box::pin(async move {
                        let result = async {
                            let response_value = await_js(&await_runtime, pending?).await?;
                            schema::map(&response_value.get("data")?, response, false)
                        }
                        .await;
                        match result {
                            Ok(value) => Ok(value),
                            Err(error) => cover_error(error),
                        }
                    }),
                )))
            }),
        )?;
    }
    client.define("tdgClient", tdg)?;
    let runtime = host.0.exchange_runtime();
    client.define(
        "getFundingAccountSummary",
        Js::function("getFundingAccountSummary", move |this, _| {
            let auth = this.call_method("authenticatedEndpoint", vec![]);
            let await_runtime = runtime.clone();
            Ok(Js::Promise(Promise::spawn(
                &runtime,
                Box::pin(async move {
                    let config = await_js(&await_runtime, auth?).await?;
                    await_js(
                        &await_runtime,
                        this.get("tdgClient")?
                            .call_method("fundingAccountSummary", vec![config])?,
                    )
                    .await
                }),
            )))
        }),
    )?;
    Ok(client)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires Node and maintained TypeScript"]
    fn sdk_cookie_parser_matches_actual_helper() {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/binding-cookie-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let rows: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(rows.len(), 18);
        let globals = Js::object(vec![]);
        safe_cex_exchange::js_runtime::library::install_library(
            &globals,
            std::rc::Weak::<safe_cex_exchange::js_runtime::scope::ScopeShared<()>>::new(),
        );
        for row in rows {
            let actual = parse_cookie(
                &safe_cex_exchange::js_runtime::value::from_json(&row["response"]),
                |v| {
                    globals
                        .get("decodeURIComponent")?
                        .call(Js::Undefined, vec![v])
                },
            )
            .unwrap();
            let expected = &row["output"];
            if expected.is_null() {
                assert!(
                    matches!(actual, Js::Null),
                    "{} expected null: {actual:?}",
                    row["name"]
                );
                continue;
            }
            assert_eq!(
                text(&actual.get("gravity").unwrap()).unwrap(),
                expected["gravity"].as_str().unwrap(),
                "{} cookie value",
                row["name"]
            );
            let expires = actual.get("expires").unwrap().as_f64().unwrap();
            if expected["expires"].is_object() {
                assert!(expires.is_nan(), "{} invalid date", row["name"])
            } else {
                assert_eq!(
                    expires,
                    expected["expires"].as_f64().unwrap(),
                    "{} expiration",
                    row["name"]
                );
            }
            assert_eq!(
                text(&actual.get("XGrvtAccountId").unwrap()).unwrap(),
                expected["XGrvtAccountId"].as_str().unwrap(),
                "{} account",
                row["name"]
            );
        }
    }
}
