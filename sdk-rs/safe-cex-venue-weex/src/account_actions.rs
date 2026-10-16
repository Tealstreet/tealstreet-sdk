//! Account actions over shared host handles and independently owned requests.
use crate::types::{ENDPOINTS, REVERSE_MARGIN_TYPE, REVERSE_POSITION_MODE};
use safe_cex_num::{js_number_string, math_max, math_min};
use safe_cex_types::{MarginType, OrderStatus, PositionMode, PositionSide};
use std::{future::Future, pin::Pin};

type ActionFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + 'a>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeexActionMethod {
    Delete,
    Post,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WeexActionValue {
    String(Vec<u16>),
    Strings(Vec<Vec<u16>>),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeexActionRequest {
    pub method: WeexActionMethod,
    pub endpoint: &'static str,
    pub body: Option<Vec<(&'static str, WeexActionValue)>>,
    pub params: Option<Vec<(&'static str, WeexActionValue)>>,
}

/// Request creation starts host-owned work; dropping its observation future cannot cancel it.
pub struct WeexAccountActionRuntime<H, O, C, M, P, E> {
    pub request: for<'a> fn(&'a H, WeexActionRequest) -> Result<ActionFuture<'a, (), E>, E>,
    /// Attach eagerly, retain first-observed rejection, and keep peer work alive.
    pub promise_all: for<'a> fn(
        &'a H,
        ActionFuture<'a, (), E>,
        ActionFuture<'a, (), E>,
    ) -> Result<ActionFuture<'a, (), E>, E>,
    pub is_algo_order: fn(&H, &O) -> Result<bool, E>,
    pub order_id: fn(&H, &O) -> Result<Vec<u16>, E>,
    pub order_symbol: fn(&H, &O) -> Result<Vec<u16>, E>,
    pub orders: fn(&H) -> Result<Vec<O>, E>,
    pub update_orders_status: fn(&H, Vec<Vec<u16>>, OrderStatus) -> Result<(), E>,
    pub market: fn(&H, &[u16]) -> Result<Option<M>, E>,
    pub leverage_min: fn(&H, &M) -> Result<f64, E>,
    pub leverage_max: fn(&H, &M) -> Result<f64, E>,
    pub config: for<'a> fn(&'a H, &[u16], bool) -> Result<ActionFuture<'a, C, E>, E>,
    pub margin_type: fn(&H, &C) -> Result<MarginType, E>,
    pub position_mode: fn(&H, &C) -> Result<PositionMode, E>,
    pub isolated_long_leverage: fn(&H, &C) -> Result<f64, E>,
    pub isolated_short_leverage: fn(&H, &C) -> Result<f64, E>,
    pub position_symbols: fn(&H) -> Result<Vec<Vec<u16>>, E>,
    pub market_symbols: fn(&H) -> Result<Vec<Vec<u16>>, E>,
    pub cache_with_mode: fn(&H, &[u16], &C, PositionMode) -> Result<(), E>,
    pub set_account_mode: fn(&H, PositionMode) -> Result<(), E>,
    pub set_hedged: fn(&H, bool) -> Result<(), E>,
    pub fetch_positions: for<'a> fn(&'a H) -> Result<ActionFuture<'a, P, E>, E>,
    pub update_positions: fn(&H, P) -> Result<(), E>,
    pub new_error: fn(&H, Vec<u16>) -> E,
}

fn ready<'a, T: 'a, E: 'a>(value: Result<T, E>) -> ActionFuture<'a, T, E> {
    Box::pin(std::future::ready(value))
}
fn endpoint(name: &str) -> &'static str {
    ENDPOINTS.iter().find(|(key, _)| *key == name).unwrap().1
}
fn text(value: &str) -> WeexActionValue {
    WeexActionValue::String(value.encode_utf16().collect())
}
fn symbol(value: &[u16]) -> WeexActionValue {
    WeexActionValue::String(value.to_vec())
}
fn delete(name: &str, params: Option<Vec<(&'static str, WeexActionValue)>>) -> WeexActionRequest {
    WeexActionRequest {
        method: WeexActionMethod::Delete,
        endpoint: endpoint(name),
        body: None,
        params,
    }
}
fn post(name: &str, body: Vec<(&'static str, WeexActionValue)>) -> WeexActionRequest {
    WeexActionRequest {
        method: WeexActionMethod::Post,
        endpoint: endpoint(name),
        body: Some(body),
        params: None,
    }
}
fn margin_code(value: MarginType) -> &'static str {
    REVERSE_MARGIN_TYPE
        .iter()
        .find(|(key, _)| *key == value)
        .unwrap()
        .1
}
fn mode_code(value: PositionMode) -> &'static str {
    REVERSE_POSITION_MODE
        .iter()
        .find(|(key, _)| *key == value)
        .unwrap()
        .1
}
fn base_id(id: Vec<u16>) -> Vec<u16> {
    let end = id
        .windows(2)
        .position(|p| p == [95, 95])
        .unwrap_or(id.len());
    id[..end].to_vec()
}

enum CancelJob<'a, O> {
    Normal(Vec<&'a O>),
    Algo(&'a O),
}
fn cancel_job<'a, H, O, C, M, P, E>(
    host: &'a H,
    runtime: &'a WeexAccountActionRuntime<H, O, C, M, P, E>,
    job: &CancelJob<'_, O>,
) -> Result<ActionFuture<'a, (), E>, E> {
    let request = match job {
        CancelJob::Normal(orders) => {
            let ids = orders
                .iter()
                .map(|o| (runtime.order_id)(host, o).map(base_id))
                .collect::<Result<Vec<_>, _>>()?;
            WeexActionRequest {
                method: WeexActionMethod::Delete,
                endpoint: endpoint("BATCH_ORDERS"),
                body: Some(vec![("orderIdList", WeexActionValue::Strings(ids))]),
                params: None,
            }
        }
        CancelJob::Algo(order) => delete(
            "ALGO_ORDER",
            Some(vec![(
                "orderId",
                symbol(&base_id((runtime.order_id)(host, order)?)),
            )]),
        ),
    };
    (runtime.request)(host, request)
}

pub fn cancel_orders<'a, H, O: 'a, C: 'a, M: 'a, P: 'a, E: 'a>(
    host: &'a H,
    runtime: &'a WeexAccountActionRuntime<H, O, C, M, P, E>,
    orders: &'a [O],
) -> ActionFuture<'a, (), E> {
    if orders.is_empty() {
        return ready(Ok(()));
    }
    let jobs = (|| {
        let mut algo = Vec::new();
        let mut normal = Vec::new();
        for order in orders {
            if (runtime.is_algo_order)(host, order)? {
                algo.push(order);
            }
        }
        for order in orders {
            if !(runtime.is_algo_order)(host, order)? {
                normal.push(order);
            }
        }
        let mut jobs: Vec<_> = normal
            .chunks(10)
            .map(|lot| CancelJob::Normal(lot.to_vec()))
            .collect();
        jobs.extend(algo.into_iter().map(CancelJob::Algo));
        Ok(jobs)
    })();
    let mut jobs = match jobs {
        Ok(jobs) => jobs.into_iter(),
        Err(error) => return ready(Err(error)),
    };
    let first = match jobs.next() {
        Some(job) => match cancel_job(host, runtime, &job) {
            Ok(request) => Some(request),
            Err(error) => return ready(Err(error)),
        },
        None => None,
    };
    if first.is_none() {
        return ready((|| {
            let ids = orders
                .iter()
                .map(|order| (runtime.order_id)(host, order))
                .collect::<Result<Vec<_>, _>>()?;
            (runtime.update_orders_status)(host, ids, OrderStatus::Canceled)
        })());
    }
    Box::pin(async move {
        if let Some(first) = first {
            first.await?;
        }
        for job in jobs {
            cancel_job(host, runtime, &job)?.await?;
        }
        let ids = orders
            .iter()
            .map(|order| (runtime.order_id)(host, order))
            .collect::<Result<Vec<_>, _>>()?;
        (runtime.update_orders_status)(host, ids, OrderStatus::Canceled)
    })
}

pub fn cancel_symbol_orders<'a, H, O: 'a, C: 'a, M: 'a, P: 'a, E: 'a>(
    host: &'a H,
    runtime: &'a WeexAccountActionRuntime<H, O, C, M, P, E>,
    target: &'a [u16],
) -> ActionFuture<'a, (), E> {
    let requests = (|| {
        Ok((
            (runtime.request)(
                host,
                delete(
                    "CANCEL_ALL_OPEN_ORDERS",
                    Some(vec![("symbol", symbol(target))]),
                ),
            )?,
            (runtime.request)(
                host,
                delete(
                    "CANCEL_ALL_ALGO_ORDERS",
                    Some(vec![("symbol", symbol(target))]),
                ),
            )?,
        ))
    })();
    let (first, second) = match requests {
        Ok(value) => value,
        Err(error) => return ready(Err(error)),
    };
    let aggregate = match (runtime.promise_all)(host, first, second) {
        Ok(future) => future,
        Err(error) => return ready(Err(error)),
    };
    Box::pin(async move {
        aggregate.await?;
        let orders = (runtime.orders)(host)?;
        let mut selected = Vec::new();
        for order in &orders {
            if (runtime.order_symbol)(host, order)? == target {
                selected.push(order);
            }
        }
        let ids = selected
            .into_iter()
            .map(|order| (runtime.order_id)(host, order))
            .collect::<Result<Vec<_>, _>>()?;
        (runtime.update_orders_status)(host, ids, OrderStatus::Canceled)
    })
}

pub fn cancel_all_orders<'a, H, O: 'a, C: 'a, M: 'a, P: 'a, E: 'a>(
    host: &'a H,
    runtime: &'a WeexAccountActionRuntime<H, O, C, M, P, E>,
) -> ActionFuture<'a, (), E> {
    let requests = (|| {
        Ok((
            (runtime.request)(host, delete("CANCEL_ALL_OPEN_ORDERS", None))?,
            (runtime.request)(host, delete("CANCEL_ALL_ALGO_ORDERS", None))?,
        ))
    })();
    let (first, second) = match requests {
        Ok(value) => value,
        Err(error) => return ready(Err(error)),
    };
    let aggregate = match (runtime.promise_all)(host, first, second) {
        Ok(future) => future,
        Err(error) => return ready(Err(error)),
    };
    Box::pin(async move {
        aggregate.await?;
        let ids = (runtime.orders)(host)?
            .iter()
            .map(|order| (runtime.order_id)(host, order))
            .collect::<Result<Vec<_>, _>>()?;
        (runtime.update_orders_status)(host, ids, OrderStatus::Canceled)
    })
}

pub fn update_symbol_metadata<'a, H, O: 'a, C: 'a, M: 'a, P: 'a, E: 'a>(
    host: &'a H,
    runtime: &'a WeexAccountActionRuntime<H, O, C, M, P, E>,
    target: &'a [u16],
) -> ActionFuture<'a, (), E> {
    let fetch = match (runtime.config)(host, target, true) {
        Ok(fetch) => fetch,
        Err(error) => return ready(Err(error)),
    };
    Box::pin(async move {
        fetch.await?;
        Ok(())
    })
}

pub fn set_leverage<'a, H, O: 'a, C: 'a, M: 'a, P: 'a, E: 'a>(
    host: &'a H,
    runtime: &'a WeexAccountActionRuntime<H, O, C, M, P, E>,
    target: &'a [u16],
    input: f64,
    side: Option<PositionSide>,
) -> ActionFuture<'a, (), E> {
    let start = (|| {
        let market = (runtime.market)(host, target)?.ok_or_else(|| {
            let mut message: Vec<u16> = "Market ".encode_utf16().collect();
            message.extend_from_slice(target);
            message.extend(" not found".encode_utf16());
            (runtime.new_error)(host, message)
        })?;
        Ok((market, (runtime.config)(host, target, false)?))
    })();
    let (market, fetch) = match start {
        Ok(value) => value,
        Err(error) => return ready(Err(error)),
    };
    Box::pin(async move {
        let config = fetch.await?;
        let leverage = math_min(
            math_max(input, (runtime.leverage_min)(host, &market)?),
            (runtime.leverage_max)(host, &market)?,
        );
        let mut payload = vec![
            ("symbol", symbol(target)),
            (
                "marginType",
                text(margin_code((runtime.margin_type)(host, &config)?)),
            ),
        ];
        if (runtime.margin_type)(host, &config)? == MarginType::Isolated {
            let long = if side == Some(PositionSide::Short) {
                (runtime.isolated_long_leverage)(host, &config)?
            } else {
                leverage
            };
            payload.push(("isolatedLongLeverage", text(&js_number_string(long))));
            let short = if side == Some(PositionSide::Long) {
                (runtime.isolated_short_leverage)(host, &config)?
            } else {
                leverage
            };
            payload.push(("isolatedShortLeverage", text(&js_number_string(short))));
        } else {
            payload.push(("crossLeverage", text(&js_number_string(leverage))));
        }
        (runtime.request)(host, post("SET_LEVERAGE", payload))?.await?;
        update_symbol_metadata(host, runtime, target).await
    })
}

pub fn change_margin_type<'a, H, O: 'a, C: 'a, M: 'a, P: 'a, E: 'a>(
    host: &'a H,
    runtime: &'a WeexAccountActionRuntime<H, O, C, M, P, E>,
    isolated: bool,
    target: Option<&'a [u16]>,
) -> ActionFuture<'a, (), E> {
    let Some(target) = target.filter(|s| !s.is_empty()) else {
        return ready(Err((runtime.new_error)(
            host,
            "Symbol is required".encode_utf16().collect(),
        )));
    };
    let fetch = match (runtime.config)(host, target, false) {
        Ok(fetch) => fetch,
        Err(error) => return ready(Err(error)),
    };
    Box::pin(async move {
        let config = fetch.await?;
        let payload = vec![
            ("symbol", symbol(target)),
            (
                "marginType",
                text(if isolated { "ISOLATED" } else { "CROSSED" }),
            ),
            (
                "separatedType",
                text(mode_code((runtime.position_mode)(host, &config)?)),
            ),
        ];
        (runtime.request)(host, post("SET_MARGIN_TYPE", payload))?.await?;
        update_symbol_metadata(host, runtime, target).await
    })
}

pub fn change_position_mode<'a, H, O: 'a, C: 'a, M: 'a, P: 'a, E: 'a>(
    host: &'a H,
    runtime: &'a WeexAccountActionRuntime<H, O, C, M, P, E>,
    hedged: bool,
    target: Option<&'a [u16]>,
) -> ActionFuture<'a, (), E> {
    let targets = (|| {
        let targets = if let Some(target) = target.filter(|s| !s.is_empty()) {
            vec![target.to_vec()]
        } else {
            let mut unique = Vec::new();
            for symbol in (runtime.position_symbols)(host)? {
                if !unique.contains(&symbol) {
                    unique.push(symbol);
                }
            }
            unique
        };
        if targets.is_empty() {
            (runtime.market_symbols)(host)
        } else {
            Ok(targets)
        }
    })();
    let targets = match targets {
        Ok(value) => value,
        Err(error) => return ready(Err(error)),
    };
    if targets.is_empty() {
        let mode = if hedged {
            PositionMode::Hedged
        } else {
            PositionMode::OneWay
        };
        return ready((|| {
            (runtime.set_account_mode)(host, mode)?;
            (runtime.set_hedged)(host, hedged)
        })());
    }
    let first = match targets.first() {
        Some(target) => match (runtime.config)(host, target, false) {
            Ok(fetch) => Some(fetch),
            Err(error) => return ready(Err(error)),
        },
        None => None,
    };
    Box::pin(async move {
        let mode = if hedged {
            PositionMode::Hedged
        } else {
            PositionMode::OneWay
        };
        let mut first = first;
        for (index, target) in targets.iter().enumerate() {
            let config = if index == 0 {
                first.take().unwrap().await?
            } else {
                (runtime.config)(host, target, false)?.await?
            };
            let payload = vec![
                ("symbol", symbol(target)),
                (
                    "marginType",
                    text(margin_code((runtime.margin_type)(host, &config)?)),
                ),
                (
                    "separatedType",
                    text(if hedged { "SEPARATED" } else { "COMBINED" }),
                ),
            ];
            (runtime.request)(host, post("SET_MARGIN_TYPE", payload))?.await?;
            (runtime.cache_with_mode)(host, target, &config, mode)?;
        }
        (runtime.set_account_mode)(host, mode)?;
        (runtime.set_hedged)(host, hedged)
    })
}

pub fn nuke_positions<'a, H, O: 'a, C: 'a, M: 'a, P: 'a, E: 'a>(
    host: &'a H,
    runtime: &'a WeexAccountActionRuntime<H, O, C, M, P, E>,
) -> ActionFuture<'a, (), E> {
    let request = match (runtime.request)(host, post("CLOSE_POSITIONS", Vec::new())) {
        Ok(request) => request,
        Err(error) => return ready(Err(error)),
    };
    Box::pin(async move {
        request.await?;
        let positions = (runtime.fetch_positions)(host)?.await?;
        (runtime.update_positions)(host, positions)
    })
}

pub fn nuke<'a, H, O: 'a, C: 'a, M: 'a, P: 'a, E: 'a>(
    host: &'a H,
    runtime: &'a WeexAccountActionRuntime<H, O, C, M, P, E>,
) -> ActionFuture<'a, (), E> {
    let closing = nuke_positions(host, runtime);
    Box::pin(async move {
        closing.await?;
        cancel_all_orders(host, runtime).await
    })
}
