//! Account members: getAccount, fetchMarginAccountInfos, validateAccount,
//! fetchBalance and formatBalance.
use crate::{
    account_readers, api_validation,
    binding::{
        promise::await_js,
        rt::{Rt, venue},
        value::{Js, Thrown, from_json, to_json},
    },
    exchange_helpers as helpers,
};
use safe_cex_exchange::runtime::ExchangeError;
use serde_json::{Map, Value};
use std::rc::Rc;

/// `(await this.xhr.get(endpoint)).data` as a JSON object; a non-object body is empty.
async fn get_data(rt: Rt, endpoint: String) -> Result<Map<String, Value>, Thrown> {
    let xhr = rt.this.get("xhr")?;
    let response = await_js(
        &rt.exchange_runtime(),
        xhr.call_method("get", vec![Js::str(&endpoint)])?,
    )
    .await?;
    Ok(match to_json(&response.get("data")?)? {
        Value::Object(map) => map,
        _ => Map::new(),
    })
}

fn native(error: Thrown) -> Rc<ExchangeError> {
    Rc::new(error.to_exchange_error())
}

fn thrown(error: Rc<ExchangeError>) -> Thrown {
    Thrown::from((*error).clone())
}

fn emit(rt: &Rt, message: Option<&Value>) -> Result<(), Thrown> {
    rt.this.call_method(
        "emitAndLogError",
        vec![message.map_or(Js::Undefined, from_json)],
    )?;
    Ok(())
}

// ts: BybitExchange.getAccount
pub(crate) fn get_account(rt: &Rt) -> Result<Js, Thrown> {
    let get_rt = rt.clone();
    let body = account_readers::get_account(move |endpoint| {
        let request = get_data(get_rt, endpoint.into());
        async move { request.await.map_err(native) }
    });
    Ok(rt.spawn(Box::pin(async move {
        let (user_id, affiliate_id) = body.await.map_err(thrown)?;
        Ok(Js::object(vec![
            ("userId", user_id.as_ref().map_or(Js::Undefined, from_json)),
            (
                "affiliateId",
                affiliate_id.as_ref().map_or(Js::Undefined, from_json),
            ),
        ]))
    })))
}

// ts: BybitExchange.fetchMarginAccountInfos
pub(crate) fn fetch_margin_account_infos(rt: &Rt) -> Result<Js, Thrown> {
    let (get_rt, emit_rt, venue) = (rt.clone(), rt.clone(), venue(rt)?);
    let margin = venue.clone();
    let body = async move {
        account_readers::fetch_margin_account_infos(
            move |endpoint| {
                let request = get_data(get_rt, endpoint.into());
                async move { request.await.map_err(native) }
            },
            move |message| emit(&emit_rt, message).map_err(native),
            move |margin_type| {
                margin.margin_type.set(margin_type);
                Ok(())
            },
            move |status| {
                *venue.unified_margin_status.borrow_mut() = status;
                Ok(())
            },
        )
        .await
    };
    Ok(rt.spawn(Box::pin(async move {
        body.await.map_err(thrown)?;
        Ok(Js::Undefined)
    })))
}

fn value_error(error: Thrown) -> Rc<Value> {
    Rc::new(to_json(&error.value).unwrap_or(Value::Null))
}

// ts: BybitExchange.validateAccount
pub(crate) fn validate_account(rt: &Rt) -> Result<Js, Thrown> {
    let (get_rt, balance_rt, emit_rt) = (rt.clone(), rt.clone(), rt.clone());
    let derived = matches!(
        venue(rt)?.base.name,
        safe_cex_types::ExchangeName::Bybitspot | safe_cex_types::ExchangeName::Bybitinverse
    );
    let body = async move {
        let get = move |endpoint: &str| {
            let request = get_data(get_rt, endpoint.into());
            async move { request.await.map_err(value_error) }
        };
        let balance = move || {
            let await_runtime = balance_rt.exchange_runtime();
            let pending = balance_rt
                .this
                .call_method("fetchBalance", vec![])
                .map_err(value_error)?;
            Ok(async move { await_js(&await_runtime, pending).await.map_err(value_error) })
        };
        let emit =
            move |message: Option<Value>| emit(&emit_rt, message.as_ref()).map_err(value_error);
        if derived {
            api_validation::validate_derived_account(get, balance, emit).await
        } else {
            api_validation::validate_account(get, balance, emit).await
        }
    };
    Ok(rt.spawn(Box::pin(async move {
        let (error, balance) = body.await.map_err(|value| Thrown::new(from_json(&value)))?;
        Ok(Js::object(vec![
            ("error", error.as_ref().map_or(Js::Undefined, from_json)),
            ("balance", balance.unwrap_or(Js::Undefined)),
        ]))
    })))
}

// ts: BybitExchange.fetchBalance
pub(crate) fn fetch_balance(rt: &Rt) -> Result<Js, Thrown> {
    let this = rt.this.clone();
    let raw = this.call_method("fetchRawBalance", vec![])?;
    let (disposed, stored, format) = (this.clone(), this.clone(), this);
    let runtime = rt.exchange_runtime();
    let body = account_readers::fetch_balance(
        move || await_js(&runtime, raw),
        move || disposed.get("isDisposed").is_ok_and(|v| v.truthy()),
        move || stored.get("store").and_then(|s| s.get("balance")),
        move |raw| Ok(format.call_method("formatBalance", vec![raw])),
    );
    Ok(rt.spawn(Box::pin(async move { body.await? })))
}

fn finite(value: Option<&Value>) -> Result<Option<f64>, Thrown> {
    helpers::parse_bybit_finite_number(value).map_err(|m| Thrown::error("TypeError", &m))
}

// ts: BybitExchange.formatBalance
pub(crate) fn format_balance(rt: &Rt, raw: Js) -> Result<Js, Thrown> {
    let this = &rt.this;
    let store = this.get("store")?;
    let rows = raw.array_values()?.unwrap_or_default();
    let Some(first) = rows.first().cloned() else {
        this.call_method(
            "log",
            vec![
                Js::str("Invalid or empty balance data received"),
                Js::str("warning"),
            ],
        )?;
        return store.get("balance");
    };
    let account = to_json(&first)?;
    let coins: Vec<Map<String, Value>> = account
        .get("coin")
        .and_then(Value::as_array)
        .map(|coins| {
            coins
                .iter()
                .map(|c| c.as_object().cloned().unwrap_or_default())
                .collect()
        })
        .unwrap_or_default();
    let positions: Vec<safe_cex_types::Position> =
        super::super::value::to_typed(&store.get("positions")?)?;
    let upnl = positions
        .iter()
        .map(crate::inverse_symbol::position_upnl_in_quote)
        .fold(0., |acc, p| acc + p);
    let field = |key: &str| finite(account.get(key));
    let total_equity = field("totalEquity")?;
    let total_wallet = field("totalWalletBalance")?;
    let total_margin = field("totalMarginBalance")?;
    let total_available = field("totalAvailableBalance")?;
    let total_initial = field("totalInitialMargin")?;
    let total_maintenance = field("totalMaintenanceMargin")?;
    let used_top = (total_initial.is_some() || total_maintenance.is_some())
        .then(|| total_maintenance.unwrap_or(0.) + total_initial.unwrap_or(0.));
    let used_coins = helpers::sum_bybit_coin_fields(&coins, &["totalPositionIM", "totalOrderIM"])
        .map_err(|m| Thrown::error("TypeError", &m))?;
    let derived_top = total_margin.map(|margin| match total_initial {
        Some(initial) => safe_cex_num::math_max(margin - initial, 0.),
        None => margin,
    });
    let derived_coins = helpers::derive_bybit_available_balance_from_coins(&coins)
        .map_err(|m| Thrown::error("TypeError", &m))?;
    let total = safe_cex_num::subtract(
        helpers::first_finite_number(&[total_equity, total_wallet]),
        upnl,
    );
    let coin_map = Js::object(vec![]);
    for coin in &coins {
        let amount = helpers::first_finite_number(&[
            finite(coin.get("equity"))?,
            finite(coin.get("walletBalance"))?,
        ]);
        let notional = helpers::first_finite_number(&[finite(coin.get("usdValue"))?, Some(amount)]);
        let name = coin.get("coin").map_or(Js::Undefined, from_json);
        coin_map.set_key(
            &name.to_js_string()?,
            Js::object(vec![
                ("base", name),
                ("amount", Js::Number(amount)),
                ("notional", Js::Number(notional)),
            ]),
        )?;
    }
    Ok(Js::object(vec![
        ("total", Js::Number(total)),
        ("upnl", Js::Number(upnl)),
        (
            "used",
            Js::Number(helpers::first_finite_number(&[used_top, used_coins])),
        ),
        (
            "free",
            Js::Number(helpers::first_finite_number(&[
                total_available,
                derived_top,
                derived_coins,
                total_wallet,
                total_equity,
            ])),
        ),
        ("coins", coin_map),
        ("info", first),
    ]))
}
