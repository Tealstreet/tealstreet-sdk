use super::rt::{Host, optional, rows, text};
use crate::{
    position_control as positions, rest_account_control as settings, rest_data as data,
    rest_order_control as cancel, session_control as session, start_order_formatting as order,
};
use safe_cex_exchange::js_runtime::{
    promise::JsFuture,
    scope::scope_of,
    value::{Js, Thrown, arg, from_typed},
};

pub(crate) fn table(name: &str) -> Result<Js, Thrown> {
    use crate::constants as c;
    fn str_table(t: &[(&str, &str)]) -> Js {
        Js::object(t.iter().map(|(k, v)| (*k, Js::str(v))).collect())
    }
    match name {
        "ORDER_TYPE" => Ok(str_table(&c::ORDER_TYPE)),
        "ORDER_SIDE" => from_typed(
            &c::ORDER_SIDE
                .iter()
                .map(|(k, v)| (*k, v))
                .collect::<std::collections::BTreeMap<_, _>>(),
        ),
        "TIME_IN_FORCE" => from_typed(
            &c::TIME_IN_FORCE
                .iter()
                .map(|(k, v)| (*k, v))
                .collect::<std::collections::BTreeMap<_, _>>(),
        ),
        "ORDER_STATUS" => from_typed(
            &c::ORDER_STATUS
                .iter()
                .map(|(k, v)| (*k, v))
                .collect::<std::collections::BTreeMap<_, _>>(),
        ),
        other => Err(Thrown::error(
            "ReferenceError",
            &format!("{other} is not defined"),
        )),
    }
}
pub(crate) fn install(h: &Host) -> Result<(), Thrown> {
    let weak = std::rc::Rc::downgrade(&h.rt.shared);
    for name in [
        "start",
        "getAccount",
        "validateAccount",
        "fetchMarkets",
        "fetchTickers",
        "fetchTickersRaw",
        "parseTickers",
        "mapPosition",
        "mapOrderUpdate",
        "formatSymbolIn",
        "formatSymbolOut",
        "fetchPositions",
        "fetchPositionForSymbol",
        "fetchRawPositionMode",
        "fetchOrders",
        "fetchPositionsAndBalance",
        "fetchHistoricalFills",
        "fetchOHLCV",
        "placeOrder",
        "placeOrders",
        "updateOrder",
        "cancelOrders",
        "cancelSymbolOrders",
        "changePositionMode",
        "changeMarginType",
        "setLeverage",
        "setLeverageHelper",
        "resolveLeverageContext",
        "getOrderPositionSide",
        "getOrFetchMarket",
        "formatCreateOrder",
        "formatCreateTrailingStopLossOrder",
        "_startPrivate",
        "_startPublicDataOnly",
        "tick",
        "dispose",
    ] {
        let weak = weak.clone();
        h.rt.this.define(
            name,
            Js::function(name, move |this, a| {
                let rt = scope_of(&weak)?.with_this(this);
                let mut host = Host { rt };
                match name {
                    "formatSymbolIn" => format_in(arg(&a, 0)),
                    "formatSymbolOut" => format_out(&host, arg(&a, 0)),
                    "parseTickers" => parse_tickers(&mut host, arg(&a, 0)),
                    "mapPosition" => data::map_position(&mut host, arg(&a, 0)),
                    "mapOrderUpdate" => data::map_order_update(&mut host, arg(&a, 0)),
                    "dispose" => {
                        session::dispose(&mut host)?;
                        Ok(Js::Undefined)
                    }
                    _ => {
                        let rt = host.rt.clone();
                        Ok(rt.spawn(async_method(host, name, a)))
                    }
                }
            }),
        )?;
    }
    Ok(())
}
fn async_method(mut h: Host, name: &'static str, a: Vec<Js>) -> JsFuture<'static> {
    Box::pin(async move {
        match name {
            "start" => {
                order::start(&mut h).await?;
                Ok(Js::Undefined)
            }
            "getAccount" => get_account(&h).await,
            "validateAccount" => validate_account(&h).await,
            "fetchMarkets" => data::fetch_markets(&mut h).await,
            "fetchTickersRaw" => fetch_tickers_raw(&h).await,
            "fetchTickers" => {
                let raw = fetch_tickers_raw(&h).await?;
                parse_tickers(&mut h, raw)
            }
            "fetchPositions" => data::fetch_positions(&mut h, arg(&a, 0)).await,
            "fetchPositionForSymbol" => Ok(Js::array(
                positions::fetch_position_for_symbol(&mut h, arg(&a, 0)).await?,
            )),
            "fetchRawPositionMode" => super::positions::raw_mode(&mut h, arg(&a, 0)).await,
            "fetchOrders" => data::fetch_orders(&mut h).await,
            "fetchPositionsAndBalance" => data::fetch_positions_and_balance(&mut h).await,
            "fetchHistoricalFills" => data::fetch_historical_fills(&mut h, arg(&a, 0)).await,
            "fetchOHLCV" => data::fetch_ohlcv(&mut h, arg(&a, 0)).await,
            "getOrderPositionSide" => positions::get_order_position_side(&mut h, arg(&a, 0)).await,
            "getOrFetchMarket" => Ok(cancel::get_or_fetch_market(&mut h, arg(&a, 0))
                .await?
                .unwrap_or(Js::Undefined)),
            "placeOrder" => order::place_order(&mut h, arg(&a, 0)).await,
            "placeOrders" => {
                h.call("assertCallerClientOrderIds", vec![arg(&a, 0)])?;
                let mut ids = vec![];
                for order in rows(&arg(&a, 0))? {
                    ids.extend(rows(&h.invoke("placeOrder", vec![order]).await?)?);
                }
                Ok(Js::array(ids))
            }
            "updateOrder" => order::update_order(&mut h, arg(&a, 0)).await,
            "formatCreateOrder" => order::format_create_order(&mut h, arg(&a, 0)).await,
            "formatCreateTrailingStopLossOrder" => {
                order::format_create_trailing_stop_loss_order(&mut h, arg(&a, 0)).await
            }
            "cancelOrders" => {
                cancel::cancel_orders(&mut h, arg(&a, 0)).await?;
                Ok(Js::Undefined)
            }
            "cancelSymbolOrders" => {
                let s = arg(&a, 0);
                let mut orders = vec![];
                for o in rows(&h.store("orders")?)? {
                    if o.get("symbol")?.strict_equals(&s) {
                        orders.push(o)
                    }
                }
                cancel::cancel_orders(&mut h, Js::array(orders)).await?;
                Ok(Js::Undefined)
            }
            "changePositionMode" => {
                settings::change_position_mode(&mut h, arg(&a, 0).truthy(), arg(&a, 1)).await?;
                Ok(Js::Undefined)
            }
            "changeMarginType" => {
                settings::change_margin_type(&mut h, arg(&a, 0).truthy(), arg(&a, 1)).await?;
                Ok(Js::Undefined)
            }
            "setLeverage" => {
                settings::set_leverage(
                    &mut h,
                    arg(&a, 0),
                    arg(&a, 1).to_number()?,
                    (!arg(&a, 2).is_undefined()).then(|| arg(&a, 2)),
                )
                .await?;
                Ok(Js::Undefined)
            }
            "setLeverageHelper" => {
                settings::set_leverage_helper(
                    &mut h,
                    arg(&a, 0),
                    arg(&a, 1).to_number()?,
                    (!arg(&a, 2).is_undefined()).then(|| arg(&a, 2)),
                    arg(&a, 3).truthy(),
                )
                .await?;
                Ok(Js::Undefined)
            }
            "resolveLeverageContext" => super::settings::context(&mut h, arg(&a, 0)).await,
            "_startPrivate" => {
                session::start_private(&mut h).await?;
                Ok(Js::Undefined)
            }
            "_startPublicDataOnly" => {
                session::start_public_only(&mut h).await?;
                Ok(Js::Undefined)
            }
            "tick" => {
                session::tick(&mut h).await?;
                Ok(Js::Undefined)
            }
            _ => Err(Thrown::type_error(&format!(
                "unknown Phemex async member {name}"
            ))),
        }
    })
}
pub(crate) fn call(h: &mut Host, name: &'static str, a: Vec<Js>) -> Result<Js, Thrown> {
    match name {
        "createAPI" => super::api::create(h, arg(&a, 0)),
        "publicSocket" => super::sockets::create(h, false),
        "privateSocket" => super::sockets::create(h, true),
        "clientRule" => Ok(Js::object(vec![
            ("label", Js::str("Phemex")),
            ("requirement", Js::str("match /^.{1,40}$/")),
        ])),
        "grafanaPeriodic" => from_typed(&crate::constants::GRAFANA_ENDPOINTS_PERIODIC),
        "grafanaEvery" => from_typed(&crate::constants::GRAFANA_ENDPOINTS_EVERY),
        "null" => Ok(Js::Null),
        "emit" => {
            h.emit(arg(&a, 0))?;
            Ok(Js::Undefined)
        }
        "resolveCaller" => h.call("resolveCallerClientOrderId", a),
        "assertSingle" => h.call("assertSingleNativeClientOrderId", a),
        "formatOut" => h.call("formatSymbolOut", a),
        "formatIn" => parent(h)?.call("formatSymbolIn", a),
        "mapOrder" => parent(h)?.call("mapOrderUpdate", a),
        "mapPosition" => parent(h)?.call("mapPosition", a),
        "mapPacked" => crate::websocket_data::map_packed_ticker_changes(h, arg(&a, 0)),
        "connectPublic" => h
            .get("publicWebsocket")?
            .call_method("connectAndSubscribe", a),
        "update" => h.get("store")?.call_method("update", a),
        "upsertOrders" => h.get("store")?.call_method("addOrUpdateOrders", a),
        "upsertPositions" => h.get("store")?.call_method("addOrUpdatePositions", a),
        "updateTicker" => h.get("store")?.call_method("updateTicker", a),
        "capitalize" => {
            let s = arg(&a, 0).call_method("toLowerCase", vec![])?;
            let units = s.to_js_string()?;
            if units.is_empty() {
                return Ok(s);
            }
            let upper = Js::utf16(&units[..1]).call_method("toUpperCase", vec![])?;
            let mut out = upper.to_js_string()?.to_vec();
            out.extend(&units[1..]);
            Ok(Js::utf16(&out))
        }
        "lodashAdd" => safe_cex_exchange::js_runtime::ops::add(&arg(&a, 0), &arg(&a, 1)),
        "uuid" | "nanoid" | "adjust" | "omitUndefined" | "roundUSD" => h.global(name, a),
        _ => {
            if h.get("parent")?.is_object() {
                super::sockets::call(h, name, a)
            } else {
                h.call(name, a)
            }
        }
    }
}
pub(crate) fn parent(h: &Host) -> Result<Host, Thrown> {
    let p = h.get("parent")?;
    Ok(if p.is_object() { h.child(p) } else { h.clone() })
}
fn format_in(s: Js) -> Result<Js, Thrown> {
    crate::utilities::format_symbol_in_value(s)
}
fn format_out(h: &Host, s: Js) -> Result<Js, Thrown> {
    crate::utilities::format_symbol_out_value(&h.store("markets")?, s)
}
async fn get_account(h: &Host) -> Result<Js, Thrown> {
    let r = h
        .http(
            "xhr",
            "get",
            "/g-accounts/accountPositions",
            vec![Js::object(vec![(
                "params",
                Js::object(vec![("currency", Js::str("USDT"))]),
            )])],
        )
        .await?;
    let id = optional(
        &optional(&optional(&r.get("data")?, "data")?, "account")?,
        "userID",
    )?;
    if id.is_nullish() {
        let m = Js::str("Phemex account-position response did not include a user ID");
        h.emit(m.clone())?;
        return Err(Thrown::error("Error", &text(&m)?));
    }
    Ok(Js::object(vec![("userId", Js::String(id.to_js_string()?))]))
}
async fn validate_account(h: &Host) -> Result<Js, Thrown> {
    let r = h
        .http(
            "xhr",
            "get",
            "/g-accounts/accountPositions",
            vec![Js::object(vec![(
                "params",
                Js::object(vec![("currency", Js::str("USDT"))]),
            )])],
        )
        .await;
    let parsed = (|| {
        let r = r?;
        let a = optional(&optional(&r.get("data")?, "data")?, "account")?;
        let total = optional(&a, "accountBalanceRv")?.parse_float()?;
        let balance = if total.is_finite() {
            let used = optional(&a, "totalUsedBalanceRv")?.parse_float()?;
            let used = if used == 0.0 || used.is_nan() {
                0.0
            } else {
                used
            };
            Js::object(vec![
                ("total", Js::Number(total)),
                ("used", Js::Number(used)),
                ("free", Js::Number(total - used)),
                ("upnl", Js::Number(0.0)),
            ])
        } else {
            Js::Undefined
        };
        Ok::<_, Thrown>(balance)
    })();
    match parsed {
        Ok(b) => Ok(Js::object(vec![("error", Js::str("")), ("balance", b)])),
        Err(e) => {
            let d = optional(&optional(&e.value, "response")?, "data")?;
            let mut emit = optional(&d, "message")?;
            if !emit.truthy() {
                emit = optional(&e.value, "message")?;
            }
            h.emit(emit)?;
            let mut error = optional(&d, "msg")?;
            if !error.truthy() {
                error = optional(&e.value, "message")?;
            }
            if !error.truthy() {
                error = Js::str("Invalid API key or secret");
            }
            Ok(Js::object(vec![("error", error)]))
        }
    }
}
async fn fetch_tickers_raw(h: &Host) -> Result<Js, Thrown> {
    crate::ticker_reads::fetch_tickers_raw_value(
        |endpoint| async move { h.http("xhr", "get", endpoint, vec![]).await?.get("data") },
        |data| h.global("v", vec![data.clone(), Js::str("error")]),
        |error| h.emit(error),
    )
    .await
}
fn parse_tickers(h: &mut Host, d: Js) -> Result<Js, Thrown> {
    if !d.truthy() || !d.get("result")?.truthy() {
        return h.store("tickers");
    }
    let mut out = vec![];
    for t in rows(&d.get("result")?)? {
        let raw = t.get("symbol")?;
        let s = format_in(raw.clone())?;
        let market = h.store("marketsBySymbol")?.get_key(&s.to_js_string()?)?;
        if !market.truthy() {
            continue;
        }
        if !raw.call_method("endsWith", vec![Js::str("USDT")])?.truthy()
            && !raw.call_method("endsWith", vec![Js::str("USDC")])?.truthy()
        {
            continue;
        }
        let n = |k| t.get(k)?.parse_float();
        out.push(Js::object(vec![
            ("id", market.get("id")?),
            ("symbol", s),
            ("bid", Js::Number(n("bidRp")?)),
            ("ask", Js::Number(n("askRp")?)),
            ("last", Js::Number(n("lastRp")?)),
            ("mark", Js::Number(n("markRp")?)),
            ("index", Js::Number(n("indexRp")?)),
            (
                "percentage",
                Js::Number(((n("lastRp")? - n("openRp")?) / n("openRp")?) * 100.0),
            ),
            ("openInterest", Js::Number(n("openInterestRv")?)),
            ("fundingRate", Js::Number(n("fundingRateRr")?)),
            ("fundingTime", Js::Number(0.0)),
            ("volume", Js::Number(n("volumeRq")?)),
            ("quoteVolume", Js::Number(n("volumeRq")? * n("lastRp")?)),
        ]));
    }
    Ok(Js::array(out))
}
pub(crate) fn date_seconds(h: &Host, v: Js) -> Result<Option<f64>, Thrown> {
    if v.is_nullish() {
        return Ok(None);
    }
    let s = v.to_js_string()?;
    if matches!(v, Js::String(_)) && !s.iter().all(|c| matches!(c, 48..=57)) {
        let n = h.global("Date.parse", vec![v])?.to_number()?;
        return Ok(n.is_finite().then(|| (n / 1000.0).floor()));
    }
    let n = h
        .global("parseInt", vec![Js::String(s), Js::Number(10.0)])?
        .to_number()?;
    Ok(if !n.is_finite() || n <= 0.0 {
        None
    } else if n > 1e15 {
        Some((n / 1e9).floor())
    } else if n > 1e12 {
        Some((n / 1000.0).floor())
    } else {
        Some(n)
    })
}
