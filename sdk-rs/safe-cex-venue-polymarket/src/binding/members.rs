//! Callable allocation for existing venue bodies on their live receivers.
use super::{Rt, error};
use safe_cex_exchange::js_runtime::{
    promise::await_js,
    scope::scope_of,
    value::{Js, Thrown, arg},
};
use std::rc::Rc;

pub(crate) fn exchange(rt: &Rt, n: &'static str, bound: Option<Js>) -> Result<Js, Thrown> {
    let weak = Rc::downgrade(&rt.shared);
    Ok(Js::function(n, move |this, a| {
        let rt = Rc::new(scope_of(&weak)?.with_this(bound.clone().unwrap_or(this)));
        let v = arg(&a, 0);
        let task: crate::exchange::PinTask<Js, Thrown> = match n {
            "name" => {
                return safe_cex_exchange::js_runtime::value::from_typed(
                    &crate::spot_contracts::name(),
                );
            }
            "mapOrder" => {
                return Ok(super::host::order(
                    &crate::order_adapters::map_exchange_order(rt.as_ref(), &v)?,
                ));
            }
            "dispose" => {
                crate::disposal::dispose_exchange(|| {
                    rt.base().dispose().map_err(|e| error(&e.message))
                })?;
                return Ok(Js::Undefined);
            }
            "getPositionMode" => Box::pin(async {
                safe_cex_exchange::js_runtime::value::from_typed(
                    &crate::spot_contracts::get_position_mode().await,
                )
            }),
            "getAccount" => Box::pin(crate::account_reads::get_account(rt.clone())),
            "validateAccount" => Box::pin(crate::account_reads::validate_account(rt.clone())),
            "deriveApiCredentials" => {
                Box::pin(crate::state_sync::derive_api_credentials(rt.clone()))
            }
            "start" => Box::pin(crate::state_sync::start_exchange(rt.clone())),
            "tick" => Box::pin(crate::state_sync::tick(rt.clone())),
            "fetchBalance" => Box::pin(crate::state_sync::fetch_balance(rt.clone())),
            "fetchPositions" => Box::pin(crate::state_sync::fetch_positions(rt.clone())),
            "fetchAndSetPositions" => {
                Box::pin(crate::state_sync::fetch_and_set_positions(rt.clone()))
            }
            "fetchOrders" => Box::pin(async move {
                let o = crate::order_adapters::fetch_orders(rt).await?;
                Ok(Js::holey_array(
                    o.into_iter()
                        .map(|o| o.map(|o| super::host::order(&o.borrow())))
                        .collect(),
                ))
            }),
            "cancelOrders" => Box::pin(async move {
                crate::order_adapters::cancel_orders(rt, v).await?;
                Ok(Js::Undefined)
            }),
            "cancelAllOrders" => Box::pin(async move {
                crate::order_adapters::cancel_all_orders(rt).await?;
                Ok(Js::Undefined)
            }),
            "fetchOrderBook" => Box::pin(async move {
                let book = crate::order_adapters::fetch_order_book(rt, &v.to_js_string()?).await?;
                Ok(book_value(book))
            }),
            "listenOrderBook" => {
                return crate::order_adapters::listen_order_book(
                    rt.as_ref(),
                    &v.to_js_string()?,
                    arg(&a, 1),
                );
            }
            "listenOHLCV" => {
                let cb = arg(&a, 1);
                let stop = crate::exchange::listen_ohlcv(
                    rt,
                    v,
                    Rc::new(move |c| {
                        cb.call(Js::Undefined, vec![c])?;
                        Ok(())
                    }),
                );
                return Ok(Js::function("", move |_, _| {
                    stop();
                    Ok(Js::Undefined)
                }));
            }
            "setLeverage" | "changePositionMode" | "changeMarginType" => Box::pin(async move {
                let mut log = |m: &str, s: &str| {
                    rt.this.call_method("log", vec![Js::str(m), Js::str(s)])?;
                    Ok::<_, Thrown>(())
                };
                match n {
                    "setLeverage" => crate::spot_contracts::set_leverage(&mut log, "", 0.).await?,
                    "changePositionMode" => {
                        crate::spot_contracts::change_position_mode(&mut log, false).await?
                    }
                    _ => crate::spot_contracts::change_margin_type(&mut log, false).await?,
                };
                Ok(Js::Undefined)
            }),
            "fetchSymbolMaxLeverage" => Box::pin(async {
                Ok(Js::Number(
                    crate::spot_contracts::fetch_symbol_max_leverage("").await,
                ))
            }),
            _ => return Err(error(&format!("Unknown Polymarket exchange member: {n}"))),
        };
        Ok(scope_of(&weak)?.spawn(task))
    }))
}
fn book_value(b: crate::order_adapters::NativeBook) -> Js {
    let levels = |v: Vec<Option<crate::order_adapters::BookLevel>>| {
        Js::holey_array(
            v.into_iter()
                .map(|v| {
                    v.map(|v| {
                        Js::object(vec![
                            ("price", Js::Number(v.price)),
                            ("amount", Js::Number(v.amount)),
                            ("total", Js::Number(v.total)),
                        ])
                    })
                })
                .collect(),
        )
    };
    Js::object(vec![("bids", levels(b.bids)), ("asks", levels(b.asks))])
}
pub(crate) fn class(
    rt: &Rt,
    c: &'static str,
    n: &'static str,
    bound: Option<Js>,
) -> Result<Js, Thrown> {
    let weak = Rc::downgrade(&rt.shared);
    Ok(Js::function(n, move |this, a| {
        let rt = Rc::new(scope_of(&weak)?.with_this(bound.clone().unwrap_or(this)));
        if c != "PolymarketAPI" {
            return super::socket::call(rt, c, n, a);
        }
        let task: crate::exchange::PinTask<Js, Thrown> = match n {
            "postOrder" => Box::pin(crate::account_reads::post_order(
                rt.clone(),
                arg(&a, 0),
                arg(&a, 1),
            )),
            "postOrders" => Box::pin(crate::account_reads::post_orders(rt.clone(), arg(&a, 0))),
            "getOpenOrders" => Box::pin(crate::account_reads::get_open_orders(
                rt.clone(),
                arg(&a, 0),
            )),
            "getTrades" => Box::pin(crate::account_reads::get_trades(
                rt.clone(),
                arg(&a, 0),
                arg(&a, 1),
            )),
            "getGammaMarkets" => Box::pin(crate::account_reads::get_gamma_markets(
                rt.clone(),
                arg(&a, 0),
            )),
            "getPositions" => Box::pin(crate::account_reads::get_positions(rt.clone(), arg(&a, 0))),
            _ => Box::pin(api_read(rt.clone(), n, a)),
        };
        Ok(scope_of(&weak)?.spawn(task))
    }))
}

// These thin bindings retain the source's raw response values. The earlier
// typed HTTP adapters deliberately accept a narrower wire-record domain.
async fn api_read(rt: Rc<Rt>, n: &str, a: Vec<Js>) -> Result<Js, Thrown> {
    let v = arg(&a, 0);
    let endpoint = |key: &str| -> Result<Js, Thrown> { rt.global("ENDPOINTS")?.get(key) };
    let mut owner = "xhr";
    let mut verb = "get";
    let config = Js::object(vec![]);
    let mut body = Js::Undefined;
    let mut scalar = None;
    let mut discard = false;
    let url = match n {
        "getMarkets" => endpoint("MARKETS")?,
        "getServerTime" => endpoint("TIME")?,
        "getSimplifiedMarkets" => endpoint("SIMPLIFIED_MARKETS")?,
        "getOrderBook" | "getMidpoint" | "getPrice" | "getLastTradePrice" | "getSpread" => {
            let params = Js::object(vec![("token_id", v)]);
            let key = match n {
                "getOrderBook" => "ORDER_BOOK",
                "getMidpoint" => {
                    scalar = Some("mid");
                    "MIDPOINT"
                }
                "getPrice" => {
                    params.define("side", arg(&a, 1))?;
                    scalar = Some("price");
                    "PRICE"
                }
                "getLastTradePrice" => {
                    scalar = Some("price");
                    "LAST_TRADE_PRICE"
                }
                _ => "SPREAD",
            };
            config.define("params", params)?;
            endpoint(key)?
        }
        "getOrderBooks" | "getMidpoints" => {
            verb = "post";
            let len = v.get("length")?.to_number()? as usize;
            let mut rows = vec![];
            for i in 0..len {
                rows.push(Js::object(vec![("token_id", v.get(&i.to_string())?)]));
            }
            body = Js::array(rows);
            endpoint(if n == "getOrderBooks" {
                "ORDER_BOOKS"
            } else {
                "MIDPOINTS"
            })?
        }
        "getPrices" => {
            config.define(
                "params",
                Js::object(vec![(
                    "token_ids",
                    v.call_method("join", vec![Js::str(",")])?,
                )]),
            )?;
            endpoint("PRICES")?
        }
        "getPriceHistory" => {
            config.define(
                "params",
                Js::object(vec![
                    ("market", v),
                    ("startTs", arg(&a, 1)),
                    ("endTs", arg(&a, 2)),
                    ("interval", arg(&a, 3)),
                    ("fidelity", arg(&a, 4)),
                ]),
            )?;
            endpoint("PRICE_HISTORY")?
        }
        "getOrder" => Js::str(&(super::text(&endpoint("GET_ORDER")?)? + &super::text(&v)?)),
        "getBalanceAllowance" => {
            config.define(
                "params",
                Js::object(vec![
                    ("asset_type", Js::str("COLLATERAL")),
                    ("signature_type", v),
                ]),
            )?;
            endpoint("BALANCE_ALLOWANCE")?
        }
        "deriveApiKey" => {
            config.define(
                "headers",
                Js::object(vec![
                    ("POLY_ADDRESS", v),
                    ("POLY_SIGNATURE", arg(&a, 2)),
                    (
                        "POLY_TIMESTAMP",
                        arg(&a, 3).call_method("toString", vec![])?,
                    ),
                    ("POLY_NONCE", arg(&a, 1).call_method("toString", vec![])?),
                ]),
            )?;
            endpoint("DERIVE_API_KEY")?
        }
        "cancelOrder" | "cancelOrders" | "cancelMarketOrders" | "cancelAllOrders"
        | "deleteApiKey" => {
            verb = "delete";
            discard = true;
            let key = match n {
                "cancelOrder" => {
                    config.define("data", Js::object(vec![("orderID", v)]))?;
                    "CANCEL"
                }
                "cancelOrders" => {
                    config.define("data", v)?;
                    "CANCEL_ORDERS"
                }
                "cancelMarketOrders" => {
                    config.define("data", Js::object(vec![("market", v)]))?;
                    "CANCEL_MARKET_ORDERS"
                }
                "cancelAllOrders" => "CANCEL_ALL",
                _ => "DELETE_API_KEY",
            };
            endpoint(key)?
        }
        "getGammaMarket" => {
            owner = "gammaXhr";
            Js::str(&format!("/markets/{}", super::text(&v)?))
        }
        "getGammaMarketBySlug" => {
            owner = "gammaXhr";
            Js::str(&format!("/markets/slug/{}", super::text(&v)?))
        }
        "searchGammaMarkets" => {
            owner = "gammaXhr";
            config.define("params", Js::object(vec![("search", v)]))?;
            Js::str("/markets")
        }
        _ => return Err(error(&format!("Unknown Polymarket API member: {n}"))),
    };
    let args = if verb == "post" {
        vec![url, body, config]
    } else {
        vec![url, config]
    };
    let response = await_js(&rt.exchange_runtime(), rt.this.get(owner)?.call_method(verb, args)?).await?;
    if discard {
        return Ok(Js::Undefined);
    }
    let data = response.get("data")?;
    if let Some(key) = scalar {
        return Ok(Js::Number(data.get(key)?.parse_float()?));
    }
    if n == "getMidpoints" {
        let o = Js::object(vec![]);
        for (k, v) in data.own_entries()? {
            o.define_key(&k, Js::Number(v.parse_float()?))?;
        }
        return Ok(o);
    }
    if n == "getPriceHistory" {
        let history = data.get("history")?;
        return Ok(if history.truthy() {
            history
        } else {
            Js::array(vec![])
        });
    }
    Ok(data)
}
