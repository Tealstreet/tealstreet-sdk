//! Public data members: fees, markets (Trading + PreLaunch, inverse, options) and
//! tickers, over the ported DataValue bodies and the receiver's `xhr`.
use crate::{
    binding::{
        data::{from_data, to_data},
        host::{get_key, text},
        promise::await_js,
        rt::Rt,
        value::{Js, Thrown, error_object, from_json, from_typed, to_json, to_typed},
    },
    failure_diagnostics as diagnostics, public_data_recovery as recovery,
    ticker_readers::{self as tickers, TickerValue as DataValue},
};
use std::{cell::Cell, future::Future, rc::Rc};

const PRELAUNCH_GRACE_MS: u64 = 1_500;

fn type_error(message: &str) -> Thrown {
    Thrown::type_error(message)
}

/// `this.xhr.get(endpoint, config)` awaited: the Axios response.
async fn get(rt: Rt, endpoint: String, config: Js) -> Result<Js, Thrown> {
    let xhr = rt.this.get("xhr")?;
    await_js(xhr.call_method("get", vec![Js::str(&endpoint), config])?).await
}

fn params(fields: Vec<(&str, Js)>) -> Js {
    Js::object(vec![("params", Js::object(fields))])
}

/// A receiver member's returned promise, awaited and crossed into DataValue.
fn member_data(
    rt: &Rt,
    name: &'static str,
    args: Vec<Js>,
) -> Result<impl Future<Output = Result<DataValue, Thrown>> + 'static + use<>, Thrown> {
    let pending = rt.this.call_method(name, args)?;
    Ok(async move { Ok(to_data(&await_js(pending).await?)) })
}

fn log(rt: &Rt, message: &str, severity: &str) -> Result<(), Thrown> {
    rt.this
        .call_method("log", vec![Js::str(message), Js::str(severity)])?;
    Ok(())
}

// ts: BybitExchange.fetchFees
pub(crate) fn fetch_fees(rt: &Rt, category: Js) -> Result<Js, Thrown> {
    let category = text(&category)?;
    let get_rt = rt.clone();
    let body = async move {
        recovery::fetch_fees(&category, move |endpoint, category| {
            let response = get(
                get_rt.clone(),
                endpoint.into(),
                params(vec![("category", Js::str(category))]),
            );
            Ok::<_, Thrown>(async move { Ok(to_data(&response.await?.get("data")?)) })
        })
        .await
    };
    Ok(rt.spawn(Box::pin(async move { Ok(from_data(&body.await?)) })))
}

/// `new BybitTradingMarketsRequestError(cause, retryCount)` as a JS error object.
fn request_error(cause: &Thrown, retry: Option<u32>) -> Thrown {
    let built = to_json(&cause.value)
        .map_err(|e| e.message())
        .and_then(|cause| {
            diagnostics::BybitTradingMarketsRequestError::new(Some(&cause), retry.map(f64::from))
                .map_err(str::to_owned)
        });
    match built {
        Ok(error) => {
            let value = error_object(error.name, &error.message);
            let _ = value.define("cause", cause.value.clone());
            let _ = value.define(
                "safeCexSentryReportKey",
                Js::str(&error.safe_cex_sentry_report_key),
            );
            let _ = value.define(
                "safeCexSentryReportCooldownMs",
                Js::Number(error.safe_cex_sentry_report_cooldown_ms as f64),
            );
            let _ = value.define(
                "safeCexSentryTags",
                from_json(&serde_json::Value::Object(error.safe_cex_sentry_tags)),
            );
            let _ = value.define(
                "safeCexSentryExtra",
                from_json(&serde_json::Value::Object(error.safe_cex_sentry_extra)),
            );
            Thrown::new(value)
        }
        Err(message) => Thrown::error("TypeError", &message),
    }
}

fn is_request_error(error: &Thrown) -> bool {
    error
        .value
        .get("name")
        .is_ok_and(|name| name.strict_equals(&Js::str("BybitTradingMarketsRequestError")))
}

// ts: BybitExchange#fetchTradingMarketsResponse — one delayed retry for transient failures.
pub(crate) fn fetch_trading_markets_response(rt: &Rt, category: Js) -> Result<Js, Thrown> {
    let category = text(&category)?;
    let (get_rt, log_rt, sleep_rt) = (rt.clone(), rt.clone(), rt.clone());
    let body = async move {
        recovery::fetch_trading_markets_response(
            &category,
            move |endpoint, category, limit| {
                let response = get(
                    get_rt.clone(),
                    endpoint.into(),
                    params(vec![
                        ("category", Js::str(category)),
                        ("limit", Js::Number(limit as f64)),
                    ]),
                );
                Ok::<_, Thrown>(async move {
                    let response = response.await?;
                    Ok(DataValue::Object(Rc::new(std::cell::RefCell::new(
                        [(
                            "data".encode_utf16().collect(),
                            to_data(&response.get("data")?),
                        )]
                        .into_iter()
                        .collect(),
                    ))))
                })
            },
            |error| {
                let cause = to_json(&error.value)?;
                diagnostics::is_retryable_bybit_trading_markets_failure(Some(&cause))
                    .map_err(type_error)
            },
            |error| {
                let cause = to_json(&error.value)?;
                diagnostics::describe_bybit_markets_failure(Some(&cause)).map_err(type_error)
            },
            move |message, severity| log(&log_rt, message, severity),
            move |ms| {
                let sleep = sleep_rt.shared.runtime.sleep(ms);
                Ok::<_, Thrown>(async move { sleep.await.map_err(Thrown::from) })
            },
            |error, retry| request_error(&error, retry),
        )
        .await
    };
    Ok(rt.spawn(Box::pin(async move { Ok(from_data(&body.await?)) })))
}

// ts: BybitExchange.fetchMarketsHelper — Trading markets required, PreLaunch best-effort
// within the startup grace window, fees joined by symbol.
pub(crate) fn fetch_markets_helper(rt: &Rt, category: Js) -> Result<Js, Thrown> {
    let spawner = rt.clone();
    let rt = rt.clone();
    let this = rt.this.clone();
    let fees = if this.get("isPublicDataOnly")?.truthy() {
        Js::array(vec![])
    } else {
        this.call_method("fetchFees", vec![category.clone()])?
    };
    let body = async move {
        let fees = await_js(fees).await?;
        let category_text = text(&category)?;
        let timed_out = Rc::new(Cell::new(false));
        let prelaunch = {
            let rt = rt.clone();
            let timed_out = timed_out.clone();
            let category = category.clone();
            rt.clone().spawn(Box::pin(async move {
                let warn = |rt: &Rt| {
                    if !timed_out.get() {
                        log(
                            rt,
                            "Failed to fetch Bybit PreLaunch markets, continuing with Trading markets",
                            "warning",
                        )?;
                    }
                    Ok::<_, Thrown>(Js::array(vec![]))
                };
                let response = get(
                    rt.clone(),
                    crate::types::ENDPOINTS["MARKETS"].as_str().unwrap().into(),
                    params(vec![
                        ("category", category),
                        ("limit", Js::Number(1000.)),
                        ("status", Js::str("PreLaunch")),
                    ]),
                )
                .await;
                let list = response.and_then(|response| {
                    let data = response.get("data")?;
                    if get_key(&data, "retMsg")?.strict_equals(&Js::str("OK")) {
                        let list = data.get("result")?.get("list")?;
                        Ok(Some(if list.truthy() { list } else { Js::array(vec![]) }))
                    } else {
                        Ok(None)
                    }
                });
                match list {
                    Ok(Some(list)) => Ok(list),
                    _ => warn(&rt),
                }
            }))
        };
        let trading =
            await_js(this.call_method("fetchTradingMarketsResponse", vec![category.clone()])?)
                .await?;
        let data = trading.get("data")?;
        let ret_msg = get_key(&data, "retMsg")?;
        if !ret_msg.strict_equals(&Js::str("OK")) {
            let message = if ret_msg.truthy() {
                ret_msg
            } else {
                Js::str("Failed to fetch Bybit Trading markets")
            };
            this.call_method("emitAndLogError", vec![message.clone()])?;
            return Err(Thrown::new(
                rt.global("Error")?.construct(vec![message], None)?,
            ));
        }
        // ts-bug: the grace timer's `.then` runs even when PreLaunch won the race, so the
        // timeout warning is logged after 1.5s on every refresh.
        let grace = {
            let rt = rt.clone();
            let timed_out = timed_out.clone();
            let sleep = rt.shared.runtime.sleep(PRELAUNCH_GRACE_MS);
            rt.clone().spawn(Box::pin(async move {
                sleep.await?;
                timed_out.set(true);
                log(
                    &rt,
                    "Timed out waiting for Bybit PreLaunch markets, continuing with Trading markets",
                    "warning",
                )?;
                Ok(Js::array(vec![]))
            }))
        };
        let prelaunch = race(prelaunch, grace).await?;
        let mut rows = data
            .get("result")?
            .get("list")?
            .array_values()
            .ok_or_else(|| type_error("data.result.list is not iterable"))?;
        rows.extend(
            prelaunch
                .array_values()
                .ok_or_else(|| type_error("preLaunchMarkets is not iterable"))?,
        );
        let fees = super::super::value::to_retained_input(&fees)?;
        let fees = fees.as_array().unwrap_or_default();
        let mut mapped = Vec::with_capacity(rows.len());
        for row in rows {
            let market = crate::market_mapping::map_market(
                &super::super::value::to_retained_input(&row)?,
                fees,
                &category_text,
                crate::market_mapping::Family::Linear,
            )
            .map_err(|m| Thrown::error("TypeError", &m))?;
            mapped.push(market);
        }
        let mut markets = Vec::with_capacity(mapped.len());
        for market in mapped {
            match market {
                Ok(market) => markets.push(market),
                Err(message) => log(&rt, &message, "warning")?,
            }
        }
        from_typed(&markets)
    };
    Ok(spawner.spawn(Box::pin(body)))
}

/// Promise.race over two already-started promises.
async fn race(a: Js, b: Js) -> Result<Js, Thrown> {
    let (Js::Promise(a), Js::Promise(b)) = (a, b) else {
        return Err(type_error("race expects promises"));
    };
    let mut a = Box::pin(a.wait());
    let mut b = Box::pin(b.wait());
    std::future::poll_fn(move |cx| {
        if let std::task::Poll::Ready(v) = a.as_mut().poll(cx) {
            return std::task::Poll::Ready(v);
        }
        b.as_mut().poll(cx)
    })
    .await
}

// ts: BybitExchange.fetchOptionMarkets
pub(crate) fn fetch_option_markets(rt: &Rt) -> Result<Js, Thrown> {
    let (get_rt, log_rt, store_rt) = (rt.clone(), rt.clone(), rt.clone());
    let body = recovery::fetch_option_markets(
        move |endpoint, query| {
            let config = match query {
                Some((base_coin, cursor)) => params(vec![
                    ("category", Js::str("option")),
                    ("baseCoin", from_data(&base_coin)),
                    ("limit", Js::Number(1000.)),
                    ("cursor", from_data(&cursor)),
                ]),
                None => Js::Undefined,
            };
            let response = get(get_rt.clone(), endpoint.into(), config);
            Ok::<_, Thrown>(
                Box::pin(async move { Ok(to_data(&response.await?.get("data")?)) })
                    as std::pin::Pin<Box<dyn Future<Output = Result<DataValue, Thrown>>>>,
            )
        },
        |row| {
            let row = super::super::value::to_retained_input(&from_data(&row))?;
            match crate::options::map_bybit_option_instrument(&row)
                .map_err(|m| Thrown::error("TypeError", &m))?
            {
                Some(market) => Ok(to_data(&from_typed(&market)?)),
                None => Ok(DataValue::Undefined),
            }
        },
        |error| error.message(),
        move |message, severity| log(&log_rt, message, severity),
        move || Ok(to_data(&store_rt.this.get("store")?.get("markets")?)),
        type_error,
    );
    Ok(rt.spawn(Box::pin(async move { Ok(from_data(&body.await?)) })))
}

// ts: BybitExchange.fetchMarkets
pub(crate) fn fetch_markets(rt: &Rt) -> Result<Js, Thrown> {
    let (helper_rt, inverse_rt, options_rt, option_rt, store_rt, log_rt) = (
        rt.clone(),
        rt.clone(),
        rt.clone(),
        rt.clone(),
        rt.clone(),
        rt.clone(),
    );
    let body = recovery::fetch_markets(
        move |category| member_data(&helper_rt, "fetchMarketsHelper", vec![Js::str(category)]),
        move || {
            Ok(inverse_rt
                .this
                .call_method("inverseEnabled", vec![])?
                .truthy())
        },
        move || {
            Ok(options_rt
                .this
                .call_method("optionsEnabled", vec![])?
                .truthy())
        },
        move || member_data(&option_rt, "fetchOptionMarkets", vec![]),
        is_request_error,
        move || Ok(to_data(&store_rt.this.get("store")?.get("markets")?)),
        move |message, severity| log(&log_rt, message, severity),
        type_error,
    );
    Ok(rt.spawn(Box::pin(async move { Ok(from_data(&body.await?)) })))
}

fn emit(rt: &Rt, message: DataValue) -> Result<(), Thrown> {
    rt.this
        .call_method("emitAndLogError", vec![from_data(&message)])?;
    Ok(())
}

// ts: BybitExchange.fetchTickersRawHelper
pub(crate) fn fetch_tickers_raw_helper(rt: &Rt, category: Js) -> Result<Js, Thrown> {
    let category = text(&category)?;
    let (get_rt, emit_rt) = (rt.clone(), rt.clone());
    let body = async move {
        tickers::fetch_tickers_raw_helper(
            &category,
            move |endpoint, category| {
                let response = get(
                    get_rt.clone(),
                    endpoint.into(),
                    params(vec![("category", Js::str(category))]),
                );
                Ok::<_, Thrown>(async move { Ok(to_data(&response.await?.get("data")?)) })
            },
            move |message| emit(&emit_rt, message),
            type_error,
        )
        .await
    };
    Ok(rt.spawn(Box::pin(async move { Ok(from_data(&body.await?)) })))
}

// ts: BybitExchange.fetchTickersRaw
pub(crate) fn fetch_tickers_raw(rt: &Rt) -> Result<Js, Thrown> {
    let (helper_rt, inverse_rt, emit_rt) = (rt.clone(), rt.clone(), rt.clone());
    let body = tickers::fetch_tickers_raw(
        move |category| member_data(&helper_rt, "fetchTickersRawHelper", vec![Js::str(category)]),
        move || {
            Ok(inverse_rt
                .this
                .call_method("inverseEnabled", vec![])?
                .truthy())
        },
        move |message| emit(&emit_rt, message),
        type_error,
    );
    Ok(rt.spawn(Box::pin(async move { Ok(from_data(&body.await?)) })))
}

// ts: BybitExchange.fetchTickersWithoutParse
pub(crate) fn fetch_tickers_without_parse(rt: &Rt) -> Result<Js, Thrown> {
    let (raw_rt, disposed_rt) = (rt.clone(), rt.clone());
    let body = tickers::fetch_tickers_without_parse(
        move || member_data(&raw_rt, "fetchTickersRaw", vec![]),
        move || Ok(disposed_rt.this.get("isDisposed")?.truthy()),
    );
    Ok(rt.spawn(Box::pin(async move { Ok(from_data(&body.await?)) })))
}

// ts: BybitExchange.fetchTickers
pub(crate) fn fetch_tickers(rt: &Rt) -> Result<Js, Thrown> {
    let (raw_rt, disposed_rt, stored_rt, parse_rt) =
        (rt.clone(), rt.clone(), rt.clone(), rt.clone());
    let body = tickers::fetch_tickers(
        move || member_data(&raw_rt, "fetchTickersRaw", vec![]),
        move || Ok(disposed_rt.this.get("isDisposed")?.truthy()),
        move || Ok(to_data(&stored_rt.this.get("store")?.get("tickers")?)),
        move |raw| {
            Ok(to_data(
                &parse_rt
                    .this
                    .call_method("parseTickers", vec![from_data(&raw)])?,
            ))
        },
    );
    Ok(rt.spawn(Box::pin(async move { Ok(from_data(&body.await?)) })))
}

// ts: BybitExchange.parseTickers
pub(crate) fn parse_tickers(rt: &Rt, raw: Js) -> Result<Js, Thrown> {
    let store = rt.this.get("store")?;
    let markets: Vec<safe_cex_types::Market> = to_typed(&store.get("markets")?)?;
    let cached: Vec<safe_cex_types::Ticker> = to_typed(&store.get("tickers")?)?;
    let raw = if raw.truthy() {
        Some(
            to_json(&raw)?
                .as_array()
                .cloned()
                .ok_or_else(|| type_error("rawTickers is not iterable"))?,
        )
    } else {
        None
    };
    let parsed = crate::exchange_state::parse_tickers(raw.as_deref(), &markets, &cached)
        .map_err(type_error)?;
    from_typed(&parsed)
}
