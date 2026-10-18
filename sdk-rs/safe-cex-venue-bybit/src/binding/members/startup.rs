//! start and the API-key metadata it caches.
use crate::{
    api_validation,
    binding::{
        promise::await_js,
        rt::{Rt, venue},
        value::{Js, Thrown, from_json, to_json},
    },
};
use serde_json::Value;
use std::rc::Rc;

fn to_error(error: Thrown) -> Rc<Value> {
    Rc::new(to_json(&error.value).unwrap_or(Value::Null))
}

fn to_thrown(error: Rc<Value>) -> Thrown {
    Thrown::new(from_json(&error))
}

fn log(rt: &Rt, message: &str) -> Result<(), Thrown> {
    rt.this.call_method("log", vec![Js::str(message)])?;
    Ok(())
}

// ts: BybitExchange.fetchApiKeyInfo
pub(crate) fn fetch_api_key_info(rt: &Rt) -> Result<Js, Thrown> {
    let rt = rt.clone();
    let body_rt = rt.clone();
    Ok(rt.spawn(Box::pin(async move {
        let rt = body_rt;
        let info = api_validation::fetch_api_key_info(
            |endpoint| {
                let request = rt
                    .this
                    .get("xhr")
                    .and_then(|xhr| xhr.call_method("get", vec![Js::str(endpoint)]));
                {
                    let await_runtime = rt.exchange_runtime();
                    async move {
                        let response = await_js(&await_runtime, request.map_err(to_error)?)
                            .await
                            .map_err(to_error)?;
                        Ok(Rc::new(
                            to_json(&response.get("data").map_err(to_error)?).map_err(to_error)?,
                        ))
                    }
                }
            },
            |message| {
                rt.this
                    .call_method(
                        "emitAndLogError",
                        vec![message.as_ref().map_or(Js::Undefined, from_json)],
                    )
                    .map_err(to_error)?;
                Ok(())
            },
            |info| {
                rt.this
                    .set(
                        "apiKeyInfo",
                        info.as_deref().map_or(Js::Undefined, from_json),
                    )
                    .map_err(to_error)
            },
        )
        .await
        .map_err(to_thrown)?;
        Ok(info.as_deref().map_or(Js::Undefined, from_json))
    })))
}

// ts: BybitExchange#updateBatchLimitBasedOnVipLevel
pub(crate) fn update_batch_limit_based_on_vip_level(rt: &Rt) -> Result<(), Thrown> {
    let venue = venue(rt)?;
    api_validation::update_batch_limit_based_on_vip_level(
        || {
            Ok(venue
                .api_key_info
                .borrow()
                .as_ref()
                .map(|info| info.vip_level.clone()))
        },
        || Ok(venue.account_type().to_owned()),
        || venue.batch_limit.get(),
        |limit| {
            venue.batch_limit.set(limit);
            Ok(())
        },
        |message| log(rt, message),
    )
}

// ts: BybitExchange.fetchAndCacheApiKeyInfo
pub(crate) fn fetch_and_cache_api_key_info(rt: &Rt) -> Result<Js, Thrown> {
    let rt = rt.clone();
    let body_rt = rt.clone();
    Ok(rt.spawn(Box::pin(async move {
        let rt = body_rt;
        let venue = venue(&rt)?;
        let cached = || {
            venue
                .api_key_info
                .borrow()
                .as_ref()
                .and_then(|info| serde_json::to_value(info).ok())
                .map(Rc::new)
        };
        api_validation::fetch_and_cache_api_key_info(
            || {
                let pending = rt
                    .this
                    .call_method("fetchApiKeyInfo", vec![])
                    .map_err(to_error)?;
                Ok({
                    let await_runtime = rt.exchange_runtime();
                    async move {
                        await_js(&await_runtime, pending)
                            .await
                            .map(|_| ())
                            .map_err(to_error)
                    }
                })
            },
            cached,
            || update_batch_limit_based_on_vip_level(&rt).map_err(to_error),
            |key, info| {
                let mut data = serde_json::Map::new();
                data.insert(key.into(), info.as_deref().cloned().unwrap_or(Value::Null));
                venue
                    .base
                    .store
                    .update_user_data(Some(Value::Object(data)))
                    .map_err(|e| {
                        to_error(Thrown::from(
                            safe_cex_exchange::runtime::ExchangeError::from(e),
                        ))
                    })
            },
            || venue.batch_limit.get(),
            |message| log(&rt, message).map_err(to_error),
        )
        .await
        .map_err(to_thrown)?;
        Ok(Js::Undefined)
    })))
}

// ts: BybitExchange.start
pub(crate) fn start(rt: &Rt) -> Js {
    crate::primary_startup::start(Rc::new(rt.clone()), rt.this.clone())
}
