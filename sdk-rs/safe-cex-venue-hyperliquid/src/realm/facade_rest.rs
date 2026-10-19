//! `createExchangeAPI` / `createInfoAPI` / `rateLimit` and the `post` clients
//! the bodies call, over the ported `ApiClient` (signing, weighted limiter,
//! response checks, read-only retry).
use super::{
    Realm,
    value::{PropKey, Thrown, Value},
};
use crate::{
    api::{ApiClient, create_exchange_api, create_info_api},
    rate_limit::RateLimiter,
};
use safe_cex_exchange::{
    error_reporting::ExchangeErrorInput, exchange::BaseExchange, runtime::ExchangeError,
};
use safe_cex_transport::{RestAdapter, RestResponseBody, rate_limit::RateLimitedAdapter};
use safe_cex_types::ExchangeOptions;
use std::{rc::Rc, sync::Arc};

/// The host transport the clients send through, plus the one process-wide
/// Hyperliquid weight limiter every client and account must share.
#[derive(Clone)]
pub(crate) struct RestDependencies {
    pub http: Arc<dyn RestAdapter>,
    pub limiter: Rc<RateLimiter>,
}

#[derive(Clone, Copy)]
enum ClientKind {
    Exchange,
    Info,
}

/// What a client object holds: enough to rebuild it under `rateLimit`.
struct Client {
    kind: ClientKind,
    options: ExchangeOptions,
    api: Rc<ApiClient>,
    error_owner: Rc<BaseExchange>,
}

fn arg(args: &[Value], i: usize) -> Value {
    args.get(i).cloned().unwrap_or(Value::Undefined)
}

pub(crate) fn thrown_from(r: &mut Realm, error: ExchangeError) -> Thrown {
    r.from_exchange_error(error)
}

#[allow(clippy::arc_with_non_send_sync)]
fn build(
    deps: &RestDependencies,
    base: &Rc<BaseExchange>,
    kind: ClientKind,
    options: ExchangeOptions,
    max_rps: Option<usize>,
) -> ApiClient {
    let adapter: Arc<dyn RestAdapter> = match max_rps {
        Some(n) => Arc::new(RateLimitedAdapter::new(
            deps.http.clone(),
            safe_cex_exchange::runtime::rate_limit_scheduler(base),
            n.max(1),
            1000,
        )),
        None => deps.http.clone(),
    };
    let reporter = base.clone();
    let emit: Arc<dyn Fn(&str)> = Arc::new(move |message: &str| {
        if let Err(error) =
            reporter.emit_and_log_error(ExchangeErrorInput::Message(message.into()), None, None)
        {
            reporter.runtime.report_exception(
                &error,
                &serde_json::json!({ "context": "hyperliquid api emit" }).into(),
            );
        }
    });
    match kind {
        ClientKind::Exchange => create_exchange_api(options, adapter, deps.limiter.clone(), emit),
        ClientKind::Info => create_info_api(options, adapter, deps.limiter.clone(), emit),
    }
}

fn client_object(realm: &Realm, client: Client) -> Value {
    let client = Rc::new(client);
    let o = realm.host_object(client.clone());
    let post = realm.native("post", 3, move |r, _, args| {
        let url = r.to_rust_string(&arg(args, 0))?;
        let body = r.to_serde(&arg(args, 1))?;
        let api = client.api.clone();
        let error_owner = client.error_owner.clone();
        let realm = r.clone();
        Ok(r.spawn_value(Box::pin(async move {
            let mut realm = realm;
            let request = api
                .build_request(&url, &body)
                .map_err(|error| thrown_from(&mut realm, ExchangeError::new(error.to_string())))?;
            match api
                .execute_with_signing_emitter(request, false, &|message| {
                    error_owner.emit_and_log_error(
                        ExchangeErrorInput::Message(message.into()),
                        None,
                        None,
                    )
                })
                .await
            {
                Ok(response) => {
                    let data = match &response.body {
                        RestResponseBody::Encoded(bytes) if bytes.is_empty() => Value::Null,
                        RestResponseBody::Encoded(bytes) => {
                            let text = String::from_utf8_lossy(bytes);
                            let units: Vec<u16> = text.encode_utf16().collect();
                            realm
                                .parse_json(&units)
                                .unwrap_or_else(|_| Value::str(&text))
                        }
                        RestResponseBody::Untransformed(v) | RestResponseBody::Transformed(v) => {
                            realm.restore_typed_value(
                                &safe_cex_exchange::js_runtime::value::from_retained_input(
                                    v.clone(),
                                ),
                            )?
                        }
                    };
                    let headers = realm.from_typed(&response.headers)?;
                    Ok(realm.object_from(vec![
                        ("data", data),
                        ("status", Value::Number(response.status as f64)),
                        ("headers", headers),
                    ]))
                }
                Err(failure) => Err(thrown_from(&mut realm, failure)),
            }
        })))
    });
    realm.define_hidden(&o, PropKey::str("post"), post);
    o
}

/// Bind the three API factory imports for one exchange instance.
pub(crate) fn bind_api_factories(realm: &Realm, base: Rc<BaseExchange>, deps: RestDependencies) {
    let deps_for_fetch = deps.http.clone();
    for (name, kind) in [
        ("createExchangeAPI", ClientKind::Exchange),
        ("createInfoAPI", ClientKind::Info),
    ] {
        let deps = deps.clone();
        let base = base.clone();
        let factory = realm.native(name, 2, move |r, _, args| {
            let options: ExchangeOptions = r.to_typed(&arg(args, 0))?;
            let api = Rc::new(build(&deps, &base, kind, options.clone(), None));
            Ok(client_object(
                r,
                Client {
                    kind,
                    options,
                    api,
                    error_owner: base.clone(),
                },
            ))
        });
        realm.bind_import(name, factory);
    }
    let rate_limit = realm.native("rateLimit", 2, move |r, _, args| {
        let client = arg(args, 0)
            .host::<Client>()
            .ok_or_else(|| r.type_error("rateLimit expects a Hyperliquid API client"))?;
        let max_rps = r.get_str(&arg(args, 1), "maxRPS")?;
        let max_rps = r.to_number(&max_rps)?;
        if !(max_rps.is_finite() && max_rps >= 1.0) {
            return Err(r.error_of("RangeError", "rateLimit maxRPS must be a positive number"));
        }
        let api = Rc::new(build(
            &deps,
            &base,
            client.kind,
            client.options.clone(),
            Some(max_rps as usize),
        ));
        Ok(client_object(
            r,
            Client {
                kind: client.kind,
                options: client.options.clone(),
                api,
                error_owner: base.clone(),
            },
        ))
    });
    realm.bind_import("rateLimit", rate_limit);
    bind_fetch(realm, deps_for_fetch);
}

/// WHATWG `fetch(url, { method, headers, body })` over the host transport, for
/// the few source paths that call it directly (builder-fee approval checks).
fn bind_fetch(realm: &Realm, http: Arc<dyn RestAdapter>) {
    let fetch = realm.native("fetch", 2, move |r, _, args| {
        let url = r.to_rust_string(&arg(args, 0))?;
        let init = arg(args, 1);
        let mut request = safe_cex_transport::RestRequest {
            method: "GET".into(),
            url,
            ..Default::default()
        };
        if init.object().is_some() {
            let method = r.get_str(&init, "method")?;
            if !method.is_nullish() {
                request.method = r.to_rust_string(&method)?.to_uppercase();
            }
            let headers = r.get_str(&init, "headers")?;
            if headers.object().is_some() {
                for (k, v) in r.own_entries(&headers)? {
                    let value = r.to_rust_string(&v)?;
                    request.headers.insert(
                        String::from_utf16_lossy(k.units().unwrap_or_default()),
                        value,
                    );
                }
            }
            let body = r.get_str(&init, "body")?;
            if !body.is_nullish() {
                let text = r.to_rust_string(&body)?;
                request.body = Some(safe_cex_transport::RestBody::Encoded(text.into_bytes()));
            }
        }
        let http = http.clone();
        let realm = r.clone();
        Ok(r.spawn_value(Box::pin(async move {
            let mut realm = realm;
            let response = http
                .execute(request)
                .await
                .map_err(|f| thrown_from(&mut realm, ExchangeError::from(f)))?;
            let text = match &response.body {
                RestResponseBody::Encoded(bytes) => String::from_utf8_lossy(bytes).into_owned(),
                RestResponseBody::Untransformed(v) | RestResponseBody::Transformed(v) => {
                    safe_cex_num::js_retained_json_stringify(v)
                        .map_err(|error| realm.error(&error.to_string()))?
                        .unwrap_or_default()
                }
            };
            let status = response.status;
            let out = realm.object_from(vec![
                ("ok", Value::Bool((200..300).contains(&status))),
                ("status", Value::Number(status as f64)),
            ]);
            let body = text.clone();
            let json = realm.native("json", 0, move |r, _, _| {
                let units: Vec<u16> = body.encode_utf16().collect();
                let parsed = r.parse_json(&units);
                Ok(r.settled_promise(parsed))
            });
            realm.define_hidden(&out, PropKey::str("json"), json);
            let text_fn = realm.native("text", 0, move |r, _, _| {
                Ok(r.settled_promise(Ok(Value::str(&text))))
            });
            realm.define_hidden(&out, PropKey::str("text"), text_fn);
            Ok(out)
        })))
    });
    realm.set_global("fetch", fetch);
}
