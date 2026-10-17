use super::{Rt, error, host::Host, text};
use crate::{exchange_helpers as helpers, runtime_host::RuntimeHost};
use safe_cex_exchange::{
    js_runtime::{
        promise::await_js,
        value::{Js, Thrown, arg, from_json, from_typed, to_json, to_typed},
    },
    runtime::ExchangeError,
};
use safe_cex_types::*;
use serde_json::{Value, json};
use std::rc::Rc;

fn optional<T: serde::de::DeserializeOwned>(value: Js) -> Result<Option<T>, Thrown> {
    if value.is_undefined() {
        Ok(None)
    } else {
        to_typed(&value).map(Some)
    }
}
fn account(rt: &Rt) -> Rc<crate::account_fetch::AccountFetch> {
    rt.shared
        .extra
        .account
        .get()
        .expect("account assembled")
        .clone()
}
fn orders(rt: &Rt) -> Rc<crate::order_execution::OrderExecution> {
    rt.shared
        .extra
        .orders
        .get()
        .expect("orders assembled")
        .clone()
}
fn path(rt: &Rt, endpoint: &str) -> String {
    helpers::get_fapi_path(endpoint, &rt.shared.extra.version, None)
}
fn primitive_error((name, message): (&str, String)) -> Thrown {
    Thrown::error(name, &message)
}

pub(super) async fn request(
    rt: &Rt,
    client: &str,
    method: &str,
    url: String,
    params: Option<Value>,
) -> Result<Value, ExchangeError> {
    let xhr = rt.this.get(client)?;
    let url = Js::str(&url);
    let args = if method == "post" || method == "put" {
        vec![url, params.as_ref().map_or(Js::Undefined, from_json)]
    } else {
        vec![
            url,
            Js::object(
                params
                    .as_ref()
                    .map(|params| vec![("params", from_json(params))])
                    .unwrap_or_default(),
            ),
        ]
    };
    let response = await_js(xhr.call_method(method, args)?).await?;
    Ok(to_json(&response.get("data")?)?)
}

pub(super) fn invoke(rt: &Rt, name: &'static str, a: Vec<Js>) -> Result<Js, Thrown> {
    let value = |i| arg(&a, i);
    match name {
        "start" | "tick" => {
            let host = Rc::new(Host::new(rt));
            let task = if name == "start" {
                crate::exchange_runtime::start_exchange(host.clone(), rt.this.clone())
            } else {
                crate::exchange_runtime::tick(host.clone(), rt.this.clone())
            };
            return Ok(host.promise(task));
        }
        "dispose" => {
            return crate::exchange_runtime::dispose(&Host::new(rt), rt.this.clone())
                .map_err(Thrown::new);
        }
        "getFapiPath" => {
            return Ok(Js::str(&helpers::get_fapi_path(
                &text(&value(0))?,
                &rt.shared.extra.version,
                optional(value(1))?,
            )));
        }
        "getAccountEndpoint" => {
            return Ok(Js::str(&helpers::get_account_endpoint(
                &rt.shared.extra.version,
            )));
        }
        "getAccountPositionMode" => {
            return from_typed(&helpers::get_account_position_mode(
                rt.base().store.options().is_hedged,
            ));
        }
        "getPositionStateVersion" => return rt.this.get("positionStateVersion"),
        "getBalanceStateVersion" => return rt.this.get("balanceStateVersion"),
        "markPositionStateUpdatedByWebsocket" => {
            let n = rt.this.get("positionStateVersion")?.to_number()? + 1.0;
            rt.this.set("positionStateVersion", Js::Number(n))?;
            return Ok(Js::Undefined);
        }
        "markBalanceStateUpdatedByWebsocket" => {
            let n = rt.this.get("balanceStateVersion")?.to_number()? + 1.0;
            rt.this.set("balanceStateVersion", Js::Number(n))?;
            account(rt).balance_state_version.set(n);
            return Ok(Js::Number(n));
        }
        "getOrderPositionSide" => {
            return Ok(Js::str(helpers::get_order_position_side(
                &to_typed::<safe_cex_types::PreparedOrderOpts>(&value(0))?,
                rt.base().store.options().is_hedged,
            )));
        }
        "assertV3Feature" => {
            helpers::assert_v3_feature(
                rt.base(),
                rt.shared.extra.version.clone(),
                &text(&value(0))?,
            )
            .map_err(error)?;
            return Ok(Js::Undefined);
        }
        "startOpenInterestPolling" => {
            orders(rt).start_open_interest_polling();
            return Ok(Js::Undefined);
        }
        "collectOpenInterestFromPayload" => {
            let payload = if value(0).is_undefined() {
                None
            } else {
                Some(to_json(&value(0))?)
            };
            let mut entries = serde_json::Map::new();
            let result =
                helpers::collect_open_interest_from_payload(payload.as_ref(), &mut entries);
            for (key, item) in entries {
                value(1).call_method("set", vec![Js::str(&key), from_json(&item)])?;
            }
            result.map_err(|e| Thrown::error(e.name, e.message))?;
            return Ok(Js::Undefined);
        }
        "updateStoreOpenInterest" => {
            let map = crate::order_execution::OpenInterestMap::default();
            let Js::Map(entries) = value(0) else {
                return Err(Thrown::type_error("openInterestBySymbol must be a Map"));
            };
            for (key, item) in entries.borrow().iter() {
                map.insert(text(key)?, item.to_number()?);
            }
            orders(rt).update_store_open_interest(&map).map_err(error)?;
            return Ok(Js::Undefined);
        }
        "afterMethodCalledInWorker" => {
            helpers::after_method_called_in_worker(
                &text(&value(1))?,
                a.get(2).map(to_json).transpose()?.as_ref(),
                |key, value| Ok(rt.base().store.set_setting(key, value)?),
            )
            .map_err(error)?;
            return Ok(Js::Undefined);
        }
        "listenOHLCV" | "listenOrderBook" => return listen(rt, name, a),
        _ => {}
    }
    let rt = rt.clone();
    Ok(rt.clone().spawn(Box::pin(async move {
        let first = arg(&a, 0);
        let result=async { match name {
            "fetchMarkets" => from_typed(&account(&rt).fetch_markets().await.map_err(error)?),
            "fetchTickers" => from_typed(&account(&rt).fetch_tickers().await.map_err(error)?),
            "fetchPositions" => { let symbols: Option<Vec<String>> = optional(first)?; from_typed(&account(&rt).fetch_positions(symbols.as_deref()).await.map_err(error)?) },
            "fetchOrders" => { let symbols: Option<Vec<String>> = optional(first)?; from_typed(&account(&rt).fetch_orders(symbols.as_deref()).await.map_err(error)?) },
            "fetchBalance" => from_typed(&account(&rt).fetch_balance().await.map_err(error)?),
            "reconcileBalanceAfterPrivateUpdate" => {
                let account = account(&rt); account.balance_state_version.set(rt.this.get("balanceStateVersion")?.to_number()?);
                let applied = account.reconcile_balance_after_private_update(first.to_number()?).await.map_err(error)?;
                rt.this.set("lastBalanceReconciliationAt", Js::Number(account.last_balance_reconciliation_at.get()))?;
                Ok(Js::Bool(applied))
            }
            "validateAccount" => from_typed(&orders(&rt).validate_account().await.map_err(error)?),
            "getAccount" => {
                let data = request(&rt, "primaryAPI", "get", helpers::get_account_endpoint(&rt.shared.extra.version), None).await.map_err(error)?;
                if data.is_null() { return Err(Thrown::type_error("Cannot read properties of null (reading 'accountAlias')")); }
                let user=data.get("accountAlias").filter(|v|safe_cex_num::js_truthy(Some(v))).cloned().unwrap_or_else(||Value::String(rt.base().options().key.clone()));
                Ok(Js::object(vec![("userId",from_json(&user)),("subId",Js::Undefined)]))
            }
            "fetchPositionMode" => {
                let data = request(&rt, "primaryAPI", "get", path(&rt, "positionSide/dual"), None).await.map_err(error)?;
                Ok(Js::Bool(helpers::parse_position_mode_response(Some(&data)).map_err(primitive_error)?))
            }
            "syncPositionMode" => { account(&rt).sync_position_mode().await.map_err(error)?; Ok(Js::Undefined) },
            "setLeverage" => { account(&rt).set_leverage(&text(&first)?, arg(&a,1).to_number()?, optional(arg(&a,2))?).await.map_err(error)?; Ok(Js::Undefined) },
            "changePositionMode" => { let symbol: Option<String> = optional(arg(&a,1))?; account(&rt).change_position_mode(first.truthy(),symbol.as_deref()).await.map_err(error)?; Ok(Js::Undefined) },
            "changeMarginType" => {
                let symbol: Option<String> = optional(arg(&a,1))?;
                let effect=helpers::build_change_margin_type_request(first.truthy(),symbol.as_deref(),rt.shared.extra.version.clone()).map_err(primitive_error)?;
                request(&rt,"primaryAPI","post",effect["path"].as_str().unwrap().into(),Some(effect["body"].clone())).await.map_err(error)?;
                rt.base().log(&format!("Margin type changed to {} for {}", if first.truthy(){"Isolated"}else{"Cross"},symbol.unwrap_or_default()),"info").map_err(error)?;
                Ok(Js::Undefined)
            }
            "fetchSymbolMaxLeverage" => {
                let markets = account(&rt).fetch_markets().await.map_err(error)?;
                let max = helpers::fetch_symbol_max_leverage(&markets.iter().map(|m|(m.symbol.clone(),Some(m.limits.leverage.max))).collect::<Vec<_>>(),&text(&first)?).map_err(|m| Thrown::error("Error",&m))?;
                Ok(Js::Number(max))
            }
            "placeOrder" => {
                let input: crate::order_execution::ExecutionOrder = to_typed(&first)?;
                from_typed(&orders(&rt).place_order(&input,arg(&a,1).truthy()).await.map_err(error)?)
            }
            "placeOrders" => { let input: Vec<crate::order_execution::ExecutionOrder> = to_typed(&first)?; from_typed(&orders(&rt).place_orders(&input,arg(&a,1).truthy()).await.map_err(error)?) },
            "updateOrder" => from_typed(&orders(&rt).update_order(&to_typed(&first)?).await.map_err(error)?),
            "cancelOrders" => { orders(&rt).cancel_orders(&to_typed::<Vec<Order>>(&first)?).await.map_err(error)?; Ok(Js::Undefined) },
            "fetchPublicTrades" => {
                let symbol = text(&first)?; let limit = if arg(&a,1).is_undefined(){500.0}else{arg(&a,1).to_number()?};
                let data = request(&rt,"publicAPI","get",path(&rt,"trades"),Some(json!({"symbol":symbol,"limit":limit}))).await.map_err(error)?;
                let rows: Vec<crate::types::AsterDEXTradeResponse> = serde_json::from_value(data).map_err(|e| Thrown::error("RepresentationError",&e.to_string()))?;
                let mapped = rows.iter().map(|trade|helpers::map_public_trade(trade,&symbol).map_err(primitive_error)).collect::<Result<Vec<_>,_>>()?;
                from_typed(&mapped)
            }
            "fetchOHLCV" => {
                let opts: OHLCVOptions = to_typed(&first)?;
                let mut params = json!({"symbol":opts.symbol,"interval":helpers::map_timeframe(opts.interval),"limit":opts.limit.filter(|v|*v!=0.0).unwrap_or(500.0)});
                if let Some(from)=opts.from.filter(|v|*v!=0.0){params["startTime"]=json!(from);}
                let data=request(&rt,"publicAPI","get",path(&rt,"klines"),Some(params)).await.map_err(error)?;
                helpers::transform_klines(&from_json(&data))
            }
            "fetchHistoricalFills" => {
                let opts: ExecutionsOptions = to_typed(&first)?;
                if rt.this.get("isPublicDataOnly")?.truthy() || opts.symbol.as_ref().is_none_or(|s|s.is_empty()) { return from_typed(&Vec::<Execution>::new()); }
                let mut params=json!({"symbol":opts.symbol,"limit":opts.limit.unwrap_or(500.0).min(1000.0)});
                if let Some(id)=opts.order_id.filter(|id|!id.is_empty()){params["orderId"]=json!(id);}
                if let Some(from)=opts.from.filter(|v|*v!=0.0){params["startTime"]=json!(from);}
                if let Some(to)=opts.to.filter(|v|*v!=0.0){params["endTime"]=json!(to);}
                let data=request(&rt,"primaryAPI","get",path(&rt,"userTrades"),Some(params)).await.map_err(error)?;
                let rows=if data.is_array(){serde_json::from_value(data).map_err(|e|Thrown::error("RepresentationError",&e.to_string()))?}else{vec![]};
                from_typed(&helpers::historical_fills_from_trades(&rows))
            }
            "pollOpenInterest" => { orders(&rt).poll_open_interest().await.map_err(error)?; Ok(Js::Undefined) },
            "fetchOpenInterestMap" => {
                let adapter=safe_cex_exchange::js_runtime::client::JsRestAdapter::new(rt.this.get("publicAPI")?);
                let symbols:Option<Vec<String>>=if first.is_undefined(){None}else{optional(first.get("symbols")?)?};
                let fallback=if first.is_undefined(){Js::Undefined}else{first.get("allowPerSymbolFallback")?};
                let fallback=if fallback.is_undefined(){true}else{fallback.truthy()};
                let mut log_error=None;
                let map=helpers::fetch_open_interest_map(&adapter,&path(&rt,"openInterest"),symbols.as_deref(),fallback,|message|{
                    if let Err(error)=rt.base().log(&message,"warning"){log_error=Some(error);}
                }).await;
                if let Some(e)=log_error{return Err(error(e));}
                let result=Js::new_map();for (key,value) in map{result.call_method("set",vec![Js::str(&key),from_json(&value)])?;}Ok(result)
            },
            "fetchStpMode" | "changeStpMode" => {
                helpers::assert_v3_feature(rt.base(),rt.shared.extra.version.clone(),"self-trade prevention").map_err(error)?;
                if name=="fetchStpMode" {
                    let data=request(&rt,"primaryAPI","get",path(&rt,"stpMode"),None).await.map_err(error)?;
                    match helpers::parse_stp_mode_response(Some(&data)) {Ok(mode)=>from_typed(&mode),Err((_,message))=>rt.base().reject_preflight(message).map_err(error)}
                }else{
                    let mode=if first.is_undefined(){None}else{Some(to_json(&first)?)};
                    let effect=match helpers::build_change_stp_mode_request(mode.as_ref()){Ok(effect)=>effect,Err((_,message))=>return rt.base().reject_preflight(message).map_err(error)};
                    request(&rt,"primaryAPI","post",effect["path"].as_str().unwrap().into(),Some(effect["body"].clone())).await.map_err(error)?;Ok(Js::Undefined)
                }
            },
            "fetchOrderBook" => {
                let limit=if arg(&a,1).is_undefined(){100.0}else{arg(&a,1).to_number()?};
                let data=request(&rt,"publicAPI","get",path(&rt,"depth"),Some(json!({"symbol":text(&first)?,"limit":limit}))).await.map_err(error)?;
                let depth:crate::types::AsterDEXDepthResponse=serde_json::from_value(data).map_err(|e|Thrown::error("RepresentationError",&e.to_string()))?;
                from_typed(&helpers::transform_order_book(&depth.bids,&depth.asks))
            },
            other => Err(Thrown::error("Error",&format!("Unbound AsterDEX member {other}"))),
        }}.await;
        if let Err(e)=&result {
            let prefix=match name {"getAccount"=>Some("Failed to get account"),"changeMarginType"=>Some("Failed to change margin type"),"fetchSymbolMaxLeverage"=>Some("Failed to fetch max leverage"),"fetchPublicTrades"=>Some("Failed to fetch public trades"),"fetchOHLCV"=>Some("Failed to fetch OHLCV"),"fetchOrderBook"=>Some("Failed to fetch order book"),"fetchHistoricalFills"=>Some("Failed to fetch executions"),_=>None};
            if let Some(prefix)=prefix{rt.base().log(&format!("{prefix}: {}",e.message()),"error").map_err(error)?;}
            if name=="fetchHistoricalFills"{return from_typed(&Vec::<Execution>::new());}
        }
        result
    })))
}

fn listen(rt: &Rt, name: &str, a: Vec<Js>) -> Result<Js, Thrown> {
    let callback = arg(&a, 1);
    let cleanup = if name == "listenOrderBook" {
        rt.shared
            .extra
            .listeners
            .listen_order_book(
                &text(&arg(&a, 0))?,
                Rc::new(move |book| {
                    callback.call(Js::Undefined, vec![from_typed(&book)?])?;
                    Ok(())
                }),
            )
            .map_err(error)?
    } else {
        let opts: OHLCVOptions = to_typed(&arg(&a, 0))?;
        rt.shared
            .extra
            .listeners
            .listen_ohlcv(
                &opts.symbol,
                opts.interval,
                Rc::new(move |candle| {
                    callback.call(
                        Js::Undefined,
                        vec![Js::object(vec![
                            (
                                "timestamp",
                                candle.timestamp.as_ref().map_or(Js::Undefined, from_json),
                            ),
                            ("open", Js::Number(candle.open)),
                            ("high", Js::Number(candle.high)),
                            ("low", Js::Number(candle.low)),
                            ("close", Js::Number(candle.close)),
                            ("volume", Js::Number(candle.volume)),
                        ])],
                    )?;
                    Ok(())
                }),
            )
            .map_err(error)?
    };
    Ok(Js::function("", move |_, _| {
        cleanup().map_err(error)?;
        Ok(Js::Undefined)
    }))
}
