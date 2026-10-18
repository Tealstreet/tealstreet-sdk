//! OKXExchange members whose ported bodies take typed values or host closures;
//! each closure performs exactly the TS expression it stands for.
use super::{
    members::{Variant, async_member, sync_member},
    ops::{OkxScope, capture},
};
use crate::{
    exchange as okx, family_policy, margin_conversion, market_policy, price_load, public_lifecycle,
    reference_price, rest_operations, rest_reads, socket_boundaries, state_transitions,
};
use safe_cex_exchange::{
    client_order_id::ClientOrderLookup,
    js_runtime::{
        client::JsRestAdapter,
        promise::await_js,
        scope::bound_member,
        value::{Js, Thrown, arg, from_json, from_typed, to_js_json, to_json, to_typed},
    },
    runtime::ExchangeError,
};
use safe_cex_num::{JsJsonValue, TableHit};
use safe_cex_transport::RestFailure;
use safe_cex_types::{Market, Order, OrderSide, OrderType, PositionMode};
use serde_json::{Map, Value};
use std::{cell::Cell, collections::HashSet, rc::Rc};

fn coercion(e: safe_cex_num::JsCoercionError) -> Thrown {
    Thrown::error(e.name, e.message)
}

fn units(v: &Js) -> Result<Vec<u16>, Thrown> {
    Ok(v.to_js_string()?.to_vec())
}

fn record(v: &Js) -> Result<Map<String, Value>, Thrown> {
    Ok(match to_json(v)? {
        Value::Object(map) => map,
        _ => Map::new(),
    })
}

/// `a?.b`
fn optional(v: &Js, key: &str) -> Result<Js, Thrown> {
    if v.is_nullish() {
        Ok(Js::Undefined)
    } else {
        v.get(key)
    }
}

/// `err?.response?.data?.msg || err?.message`
fn response_or_message(error: &Thrown) -> Result<Js, Thrown> {
    let v = &error.value;
    let msg = optional(&optional(&optional(v, "response")?, "data")?, "msg")?;
    if msg.truthy() {
        Ok(msg)
    } else {
        optional(v, "message")
    }
}

fn call(f: &Js, args: Vec<Js>) -> Result<Js, Thrown> {
    if !f.is_callable() {
        return Err(Thrown::type_error(&format!(
            "{} is not a function",
            f.to_display()
        )));
    }
    f.call(Js::Undefined, args)
}

fn this_call(scope: &OkxScope, path: &str, args: Vec<Js>) -> Result<Js, Thrown> {
    call(&capture(scope, &format!("this.{path}"))?, args)
}

/// `String(value).toUpperCase()` on a string receiver; other receivers throw.
fn upper(v: &Js, what: &str) -> Result<Vec<u16>, Thrown> {
    if !v.is_string() {
        return Err(Thrown::type_error(&format!(
            "{what}.toUpperCase is not a function"
        )));
    }
    units(&v.call_method("toUpperCase", vec![])?)
}

/// `market.info?.ctValCcy`-style fields read as numbers: `parseFloat(String(v ?? ''))` inputs.
fn optional_number(v: &Js) -> Result<Option<f64>, Thrown> {
    if v.is_nullish() {
        Ok(None)
    } else {
        v.parse_float().map(Some)
    }
}

/// A store ticker as the reference-price bodies read it, live on every read.
struct JsTicker(Js);

impl reference_price::ReferenceTicker<Thrown> for JsTicker {
    fn price(&self, field: reference_price::TickerPriceField) -> Result<Option<f64>, Thrown> {
        let key = match field {
            reference_price::TickerPriceField::Mark => "mark",
            reference_price::TickerPriceField::Index => "index",
            reference_price::TickerPriceField::Last => "last",
        };
        optional_number(&self.0.get(key)?)
    }
}

fn ticker_of(v: Js) -> Option<Rc<JsTicker>> {
    v.truthy().then(|| Rc::new(JsTicker(v)))
}

fn source_of(v: &Js) -> reference_price::PriceSource {
    if v.strict_equals(&Js::str("mark")) {
        reference_price::PriceSource::Mark
    } else {
        reference_price::PriceSource::Index
    }
}

fn source_name(source: reference_price::PriceSource) -> &'static str {
    match source {
        reference_price::PriceSource::Mark => "mark",
        reference_price::PriceSource::Index => "index",
    }
}

fn placement(scope: &OkxScope, name: &'static str, algo: bool) {
    async_member(scope, name, move |s, a| {
        Box::pin(async move {
            let payloads: Vec<JsJsonValue> = arg(&a, 0)
                .array_values()
                .unwrap_or_default()
                .iter()
                .map(to_js_json)
                .collect::<Result<_, _>>()?;
            let adapter = JsRestAdapter::new(s.exchange_runtime(), s.this.get("xhr")?);
            let emitter = capture(&s, "this.emitAndLogError")?;
            let emit = |message: &str| -> Result<(), ExchangeError> {
                call(&emitter, vec![Js::str(message)])
                    .map(|_| ())
                    .map_err(ExchangeError::from)
            };
            let ids = if algo {
                rest_operations::place_algo_order_batch(&adapter, &payloads, &emit).await
            } else {
                rest_operations::place_order_batch(&adapter, &payloads, &emit).await
            }?;
            from_typed(&ids)
        })
    });
}

fn find_order_by_client_order_id(scope: &OkxScope) {
    async_member(scope, "findOrderByClientOrderId", |s, a| {
        Box::pin(async move {
            let (symbol, id) = (arg(&a, 0), arg(&a, 1));
            let store = s.this.get("store")?;
            let orders: Vec<Order> = to_typed(&store.get("orders")?)?;
            let market = store
                .get("marketsBySymbol")?
                .get_key(&symbol.to_js_string()?)?;
            let market: Option<Market> = if market.is_nullish() {
                None
            } else {
                Some(to_typed(&market)?)
            };
            let adapter = JsRestAdapter::new(s.exchange_runtime(), s.this.get("xhr")?);
            let map_orders = |row: &Value,
                              state: TableHit<&'static str>|
             -> Result<Vec<Order>, RestFailure> {
                let failure = |name: &str, message: String| RestFailure {
                    code: None,
                    message,
                    transport_error: None,
                    response: None,
                    request: Box::default(),
                    context: Some(Box::new(serde_json::json!({ "name": name }))),
                };
                if let TableHit::Inherited(member) = state {
                    return Err(failure(
                        "RepresentationError",
                        format!(
                            "OKX inherited lookup state {member:?} cannot reach mapOrders as a JSON row"
                        ),
                    ));
                }
                let mapped = this_call(&s, "mapOrders", vec![Js::array(vec![from_json(row)])])
                    .and_then(|v| to_typed::<Vec<Order>>(&v))
                    .map_err(|e| {
                        let name = optional(&e.value, "name")
                            .ok()
                            .filter(Js::is_string)
                            .map(|n| {
                                String::from_utf16_lossy(&n.to_js_string().unwrap_or_default())
                            })
                            .unwrap_or_else(|| "Error".into());
                        failure(&name, e.message())
                    })?;
                Ok(mapped)
            };
            let lookup = rest_operations::find_order_by_client_order_id(
                &adapter,
                &orders,
                market.as_ref(),
                &String::from_utf16_lossy(&symbol.to_js_string()?),
                &String::from_utf16_lossy(&id.to_js_string()?),
                &map_orders,
            )
            .await
            .map_err(|f| adapter.thrown(f))?;
            Ok(match lookup {
                ClientOrderLookup::Found(order) => Js::object(vec![
                    ("found", Js::Bool(true)),
                    ("order", from_typed(&*order)?),
                ]),
                ClientOrderLookup::NotFound { definitive } => Js::object(vec![
                    ("found", Js::Bool(false)),
                    ("definitive", Js::Bool(definitive)),
                ]),
            })
        })
    });
}

fn market_shape(scope: &OkxScope, variant: Variant) {
    if variant.is_eu() {
        sync_member(scope, "getInstrumentTypes", |_, _| {
            from_typed(&family_policy::get_eu_instrument_types())
        });
        sync_member(scope, "getMarketInstrumentsEndpoint", |s, _| {
            let public = s.this.get("isPublicDataOnly")?.truthy();
            Ok(Js::str(family_policy::get_eu_market_instruments_endpoint(
                public,
            )))
        });
        sync_member(scope, "isSupportedMarketInstrument", |_, a| {
            Ok(Js::Bool(market_policy::is_supported_eu_market_instrument(
                &record(&arg(&a, 0))?,
            )))
        });
    } else {
        sync_member(scope, "getInstrumentTypes", |_, _| {
            from_typed(&okx::get_instrument_types())
        });
        sync_member(scope, "getMarketInstrumentsEndpoint", |_, _| {
            Ok(Js::str(family_policy::get_market_instruments_endpoint()))
        });
        sync_member(scope, "isSupportedMarketInstrument", |_, a| {
            Ok(Js::Bool(okx::is_supported_market_instrument(&record(
                &arg(&a, 0),
            )?)))
        });
    }
    sync_member(scope, "getMarketInstrumentType", |_, a| {
        from_typed(&okx::get_market_instrument_type(&record(&arg(&a, 0))?).map_err(coercion)?)
    });
    sync_member(scope, "getIndexTickerInstId", |_, a| {
        let market: Market = to_typed(&arg(&a, 0))?;
        Ok(Js::str(
            &okx::get_index_ticker_inst_id(&market).map_err(coercion)?,
        ))
    });
    sync_member(scope, "getMarketsForIndexTickerInstId", |s, a| {
        let js_markets = s
            .this
            .get("store")?
            .get("markets")?
            .array_values()
            .unwrap_or_default();
        let markets = js_markets
            .iter()
            .map(to_typed::<Market>)
            .collect::<Result<Vec<_>, _>>()?;
        let instrument = String::from_utf16_lossy(&arg(&a, 0).to_js_string()?);
        let matching =
            okx::get_markets_for_index_ticker_inst_id(&markets, &instrument).map_err(coercion)?;
        // Return the store's own market objects, as the source filter does.
        Ok(Js::array(
            matching
                .into_iter()
                .filter_map(|m| markets.iter().position(|x| std::ptr::eq(x, m)))
                .map(|i| js_markets[i].clone())
                .collect(),
        ))
    });
    sync_member(scope, "formatMarketSymbol", |_, a| {
        Ok(Js::str(
            &okx::format_market_symbol(&record(&arg(&a, 0))?).map_err(coercion)?,
        ))
    });
    sync_member(scope, "formatMarketPrettySymbol", |_, a| {
        let present = |i: usize| -> Result<Option<Value>, Thrown> {
            if a.len() <= i || arg(&a, i).is_undefined() {
                Ok(None)
            } else {
                to_json(&arg(&a, i)).map(Some)
            }
        };
        let (market, base, quote) = (present(0)?, present(1)?, present(2)?);
        Ok(
            okx::format_market_pretty_symbol(market.as_ref(), base.as_ref(), quote.as_ref())
                .map_err(coercion)?
                .map_or(Js::Undefined, |s| Js::str(&s)),
        )
    });
}

fn account(scope: &OkxScope, variant: Variant) {
    if variant.is_eu() {
        async_member(scope, "validateAccount", |s, _| {
            Box::pin(async move {
                let xhr = s.this.get("xhr")?;
                rest_reads::validate_eu_account(
                    |name| {
                        let p =
                            xhr.call_method("get", vec![s.global(&format!("ENDPOINTS.{name}"))?])?;
                        Ok(await_js(&s.exchange_runtime(), p))
                    },
                    || capture(&s, "this.emitAndLogError"),
                    |e, mode| {
                        if mode == "message" {
                            optional(&e.value, "message")
                        } else {
                            response_or_message(e)
                        }
                    },
                    |emitter, message| call(&emitter, vec![message]).map(|_| ()),
                    Js::truthy,
                    Js::str,
                    |fields| Js::object(fields),
                )
                .await
            })
        });
    } else {
        async_member(scope, "validateAccount", |s, _| {
            Box::pin(async move {
                let xhr = s.this.get("xhr")?;
                rest_reads::validate_account(
                    |name| {
                        let p =
                            xhr.call_method("get", vec![s.global(&format!("ENDPOINTS.{name}"))?])?;
                        Ok(await_js(&s.exchange_runtime(), p))
                    },
                    || Ok(await_js(&s.exchange_runtime(), this_call(&s, "fetchBalance", vec![])?)),
                    || capture(&s, "this.emitAndLogError"),
                    |e, mode| {
                        if mode == "message" {
                            optional(&e.value, "message")
                        } else {
                            response_or_message(e)
                        }
                    },
                    |emitter, message| call(&emitter, vec![message]).map(|_| ()),
                    Js::truthy,
                    Js::str,
                    || Js::Undefined,
                    |fields| Js::object(fields),
                )
                .await
            })
        });
        async_member(scope, "fetchAccountMarketOverlay", |_, _| {
            Box::pin(async move {
                Ok(from_json(
                    &state_transitions::fetch_account_market_overlay().await,
                ))
            })
        });
    }
    async_member(scope, "fetchTickers", |s, _| {
        Box::pin(async move {
            rest_reads::fetch_tickers(
                |name| Ok(await_js(&s.exchange_runtime(), this_call(&s, name, vec![])?)),
                |raw| this_call(&s, "parseTickers", vec![raw]),
            )
            .await
        })
    });
    async_member(scope, "fetchPositions", |s, a| {
        Box::pin(async move {
            rest_reads::fetch_positions(
                arg(&a, 0),
                |name, symbols| Ok(await_js(&s.exchange_runtime(), this_call(&s, name, vec![symbols])?)),
                |response, key| response.get(key),
            )
            .await
        })
    });
    async_member(scope, "fetchBalance", |s, _| {
        Box::pin(async move {
            rest_reads::fetch_balance(
                |name| Ok(await_js(&s.exchange_runtime(), this_call(&s, name, vec![])?)),
                |response, key| response.get(key),
            )
            .await
        })
    });
}

fn cache_field(name: &str) -> &'static str {
    if name == "positionMode" {
        "_accountPositionMode"
    } else {
        "_accountLevel"
    }
}

fn position_mode(scope: &OkxScope) {
    async_member(scope, "getOrFetchPositionModeAndAccLevel", |s, a| {
        Box::pin(async move {
            let force = (!a.is_empty() && !arg(&a, 0).is_undefined()).then(|| arg(&a, 0));
            let this = s.this.clone();
            let xhr = this.get("xhr")?;
            rest_reads::get_or_fetch_position_mode_and_account_level(
                force,
                Js::Bool,
                |name| this.get(cache_field(name)),
                Js::truthy,
                |name| {
                    let p =
                        xhr.call_method("get", vec![s.global(&format!("ENDPOINTS.{name}"))?])?;
                    Ok(await_js(&s.exchange_runtime(), p))
                },
                // const { data: { data: [{ posMode, acctLv }] } } = response
                |response| {
                    let first = response.get("data")?.get("data")?.get("0")?;
                    Ok((first.get("posMode")?, first.get("acctLv")?))
                },
                |v, text| v.strict_equals(&Js::str(text)),
                |name, value| this.set(cache_field(name), value),
                |v| Ok(Js::Number(v.parse_float()?)),
                Js::str,
                |fields| Js::object(fields),
                |name| {
                    if name == "Error" {
                        s.global(name)
                    } else {
                        capture(&s, &format!("this.{name}"))
                    }
                },
                response_or_message,
                |emitter, message| call(&emitter, vec![message]).map(|_| ()),
                |ctor, message| Ok(Thrown::new(call(&ctor, vec![message])?)),
            )
            .await
        })
    });
    // Both read their half of getOrFetchPositionModeAndAccLevel(force).
    let config = |s: &OkxScope, force: bool| {
        let pending = this_call(
            s,
            "getOrFetchPositionModeAndAccLevel",
            vec![Js::Bool(force)],
        );
        async move {
            let value = await_js(&s.exchange_runtime(), pending?).await?;
            let mode: PositionMode = to_typed(&value.get("positionMode")?)?;
            Ok::<_, Thrown>((mode, value.get("accountLevel")?.to_number()?))
        }
    };
    async_member(scope, "getOrFetchPositionMode", move |s, a| {
        Box::pin(async move {
            let force = (!arg(&a, 0).is_undefined()).then(|| arg(&a, 0).truthy());
            let mode =
                state_transitions::get_or_fetch_position_mode(force, |f| config(&s, f)).await?;
            from_typed(&mode)
        })
    });
    async_member(scope, "getOrFetchAccountLevel", move |s, a| {
        Box::pin(async move {
            let force = (!arg(&a, 0).is_undefined()).then(|| arg(&a, 0).truthy());
            Ok(Js::Number(
                state_transitions::get_or_fetch_account_level(force, |f| config(&s, f)).await?,
            ))
        })
    });
    async_member(scope, "changeMarginType", |s, _| {
        Box::pin(async move {
            state_transitions::change_margin_type(|m| {
                this_call(&s, "emitAndLogError", vec![Js::str(m)]).map(|_| ())
            })
            .await?;
            Ok(Js::Undefined)
        })
    });
    sync_member(scope, "afterMethodCalledInWorker", |s, a| {
        let field = s.this.get("_accountPositionMode")?;
        let current = if field.is_nullish() {
            None
        } else {
            to_typed::<PositionMode>(&field).ok()
        };
        let cell = Cell::new(current);
        let first = (a.len() > 2).then(|| arg(&a, 2).truthy());
        market_policy::after_method_called_in_worker(
            &cell,
            &units(&arg(&a, 1))?,
            first.or(Some(false)),
        );
        if cell.get() != current {
            s.this
                .set("_accountPositionMode", from_typed(&cell.get())?)?;
        }
        Ok(Js::Undefined)
    });
    sync_member(scope, "getPositionSide", |s, a| {
        let opts = arg(&a, 0);
        let field = s.this.get("_accountPositionMode")?;
        // `!mode` throws; any truthy mode other than oneway takes the hedged path.
        let mode = if !field.truthy() {
            None
        } else if field.strict_equals(&Js::str("oneway")) {
            Some(PositionMode::OneWay)
        } else {
            Some(PositionMode::Hedged)
        };
        let order_type: OrderType = to_typed(&opts.get("type")?).unwrap_or(OrderType::Market);
        let side = if opts.get("side")?.strict_equals(&Js::str("buy")) {
            OrderSide::Buy
        } else {
            OrderSide::Sell
        };
        let reduce = Some(opts.get("reduceOnly")?.truthy());
        let side = family_policy::get_position_side(mode, order_type, side, reduce)
            .map_err(Thrown::from)?;
        Ok(side.map_or(Js::Undefined, Js::str))
    });
}

fn price_loads(scope: &OkxScope) {
    // The Set fields hold market IDs; the typed body owns membership.
    fn read(set: &Js) -> Result<HashSet<Vec<u16>>, Thrown> {
        let Js::Set(values) = set else {
            return Ok(HashSet::new());
        };
        values.borrow().iter().map(units).collect()
    }
    for (name, field, mark) in [
        ("markPriceLoaded", "markPriceMarketIds", true),
        ("indexPriceLoaded", "indexPriceMarketIds", false),
    ] {
        sync_member(scope, name, move |s, a| {
            let set = s.this.get(field)?;
            let mut ids = read(&set)?;
            let id = units(&arg(&a, 0))?;
            let before = ids.len();
            if mark {
                price_load::mark_price_loaded(&mut ids, &id);
            } else {
                price_load::index_price_loaded(&mut ids, &id);
            }
            if ids.len() != before {
                set.call_method("add", vec![Js::utf16(&id)])?;
            }
            Ok(Js::Undefined)
        });
    }
    for (name, field, mark) in [
        ("hasMarkPriceLoaded", "markPriceMarketIds", true),
        ("hasIndexPriceLoaded", "indexPriceMarketIds", false),
    ] {
        sync_member(scope, name, move |s, a| {
            let ids = read(&s.this.get(field)?)?;
            let id = units(&arg(&a, 0))?;
            Ok(Js::Bool(if mark {
                price_load::has_mark_price_loaded(&ids, &id)
            } else {
                price_load::has_index_price_loaded(&ids, &id)
            }))
        });
    }
}

fn prices(scope: &OkxScope) {
    sync_member(scope, "getCtValCcy", |_, a| {
        let market = arg(&a, 0);
        let currency = margin_conversion::get_contract_currency(
            || {
                let info = market.get("info")?;
                if info.is_nullish() {
                    Ok(None)
                } else {
                    info.get("ctValCcy").map(Some)
                }
            },
            Js::truthy,
            |v| match v {
                None => Ok(vec![]),
                Some(v) => {
                    units(&Js::String(v.to_js_string()?).call_method("toUpperCase", vec![])?)
                }
            },
        )?;
        Ok(Js::utf16(&currency))
    });
    sync_member(scope, "toFiniteNumber", |_, a| {
        let v = arg(&a, 0);
        Ok(Js::Number(margin_conversion::to_finite_number(|| {
            if v.is_nullish() {
                Ok(vec![])
            } else {
                units(&v)
            }
        })?))
    });
    sync_member(scope, "getTicker", |s, a| {
        let market = arg(&a, 0);
        let store = s.this.get("store")?;
        let ticker = reference_price::get_ticker(
            || {
                Ok::<_, Thrown>(ticker_of(
                    store
                        .get("tickersById")?
                        .get_key(&market.get("id")?.to_js_string()?)?,
                ))
            },
            || {
                Ok(ticker_of(
                    store
                        .get("tickersBySymbol")?
                        .get_key(&market.get("symbol")?.to_js_string()?)?,
                ))
            },
        )?;
        // `a || b` keeps b's falsy value, which the source then returns.
        Ok(match ticker {
            Some(t) => t.0.clone(),
            None => store
                .get("tickersBySymbol")?
                .get_key(&market.get("symbol")?.to_js_string()?)?,
        })
    });
    sync_member(scope, "resolveReferencePrice", |s, a| {
        let market = arg(&a, 0);
        Ok(Js::Number(reference_price::resolve_reference_price(
            optional_number(&arg(&a, 1))?,
            || Ok(ticker_of(this_call(s, "getTicker", vec![market.clone()])?)),
        )?))
    });
    sync_member(scope, "hasExternalTriggerReferencePrice", |s, a| {
        let (market, source, ticker) = (arg(&a, 0), source_of(&arg(&a, 1)), arg(&a, 2));
        let config = s.this.get("options")?.get("externalPublicData")?;
        let kind = if config.is_nullish() {
            reference_price::ExternalConfig::Absent
        } else if optional(&config, "type")?.strict_equals(&Js::str("in-process")) {
            reference_price::ExternalConfig::InProcess
        } else {
            reference_price::ExternalConfig::Present
        };
        let ticker = ticker.is_object().then(|| JsTicker(ticker));
        Ok(Js::Bool(
            reference_price::has_external_trigger_reference_price(
                kind,
                source,
                ticker.as_ref(),
                |source| {
                    let method = if source == reference_price::PriceSource::Mark {
                        "hasMarkPriceLoaded"
                    } else {
                        "hasIndexPriceLoaded"
                    };
                    let f = optional(&config.get("source")?, method)?;
                    if f.is_nullish() {
                        return Ok(None);
                    }
                    Ok(Some(
                        call(
                            &bound_member(&config.get("source")?, method)?,
                            vec![market.get("id")?],
                        )?
                        .truthy(),
                    ))
                },
            )?,
        ))
    });
    sync_member(scope, "resolveTriggerReferencePrice", |s, a| {
        let (market, kind) = (arg(&a, 0), arg(&a, 1));
        let kind = kind.as_str().map(|k| k.to_vec());
        let loaded = |field: &str| -> Result<bool, Thrown> {
            Ok(s.this
                .get(field)?
                .call_method("has", vec![market.get("id")?])?
                .truthy())
        };
        Ok(Js::Number(
            reference_price::resolve_trigger_reference_price(
                kind.as_deref(),
                optional_number(&arg(&a, 2))?,
                || loaded("indexPriceMarketIds"),
                || loaded("markPriceMarketIds"),
                || Ok(ticker_of(this_call(s, "getTicker", vec![market.clone()])?)),
                |source, ticker| {
                    let ticker = ticker.map_or(Js::Undefined, |t| t.0.clone());
                    Ok(this_call(
                        s,
                        "hasExternalTriggerReferencePrice",
                        vec![market.clone(), Js::str(source_name(source)), ticker],
                    )?
                    .truthy())
                },
            )?,
        ))
    });
    for (name, to_usd) in [
        ("convertMarginValueToUsd", true),
        ("convertUsdValueToMarginCurrency", false),
    ] {
        sync_member(scope, name, move |s, a| {
            let (amount, currency, market, reference) =
                (arg(&a, 0), arg(&a, 1), arg(&a, 2), arg(&a, 3));
            let currency_units = if currency.truthy() {
                Some(units(&currency)?)
            } else {
                None
            };
            let finite = || this_call(s, "toFiniteNumber", vec![amount.clone()])?.to_number();
            let upper_margin = |_: &[u16]| upper(&currency, "marginCurrency");
            let inverse = || Ok(market.get("isInverse")?.truthy());
            let upper_base = || upper(&market.get("base")?, "market.base");
            let price = || {
                this_call(
                    s,
                    "resolveReferencePrice",
                    vec![market.clone(), reference.clone()],
                )?
                .to_number()
            };
            let convert = if to_usd {
                margin_conversion::margin_to_usd
            } else {
                margin_conversion::usd_to_margin
            };
            Ok(Js::Number(convert(
                finite,
                market.truthy(),
                currency_units.as_deref(),
                upper_margin,
                inverse,
                upper_base,
                price,
            )?))
        });
    }
    sync_member(scope, "contractsToBase", |s, a| {
        let (contracts, market, reference) = (arg(&a, 0), arg(&a, 1), arg(&a, 2));
        Ok(Js::Number(margin_conversion::contracts_to_base(
            contracts.to_number()?,
            || optional_number(&market.get("contractSize")?),
            || units(&this_call(s, "getCtValCcy", vec![market.clone()])?),
            || Ok(market.get("isInverse")?.truthy()),
            || upper(&market.get("quote")?, "market.quote"),
            || {
                this_call(
                    s,
                    "resolveReferencePrice",
                    vec![market.clone(), reference.clone()],
                )?
                .to_number()
            },
        )?))
    });
    sync_member(scope, "contractsToNotional", |s, a| {
        let (contracts, market, mark) = (arg(&a, 0), arg(&a, 1), arg(&a, 2));
        Ok(Js::Number(public_lifecycle::contracts_to_notional(
            contracts.to_number()?,
            mark.to_number()?,
            || optional_number(&market.get("contractSize")?),
            || units(&this_call(s, "getCtValCcy", vec![market.clone()])?),
            || Ok(market.get("isInverse")?.truthy()),
            || upper(&market.get("quote")?, "market.quote"),
        )?))
    });
}

fn orders(scope: &OkxScope) {
    sync_member(scope, "isAlgoOrder", |_, a| {
        Ok(Js::Bool(
            to_typed::<OrderType>(&arg(&a, 0)).is_ok_and(okx::is_algo_order),
        ))
    });
    sync_member(scope, "getAlgoOrderPlacementPriority", |_, a| {
        Ok(Js::Number(
            to_typed::<OrderType>(&arg(&a, 0)).map_or(3, okx::get_algo_order_placement_priority)
                as f64,
        ))
    });
    placement(scope, "placeOrderBatch", false);
    placement(scope, "placeAlgoOrderBatch", true);
    find_order_by_client_order_id(scope);
}

fn sockets(scope: &OkxScope) {
    let socket_member =
        |s: &OkxScope, owner: &str, member: &str| bound_member(&s.this.get(owner)?, member);
    sync_member(scope, "listenOHLCV", move |s, a| {
        socket_boundaries::listen_ohlcv(
            arg(&a, 0),
            arg(&a, 1),
            |o, m| socket_member(s, o, m),
            |f, x, y| call(&f, vec![x, y]),
        )
    });
    sync_member(scope, "listenOrderBook", move |s, a| {
        socket_boundaries::listen_order_book(
            arg(&a, 0),
            arg(&a, 1),
            |o, m| socket_member(s, o, m),
            |f, x, y| call(&f, vec![x, y]),
        )
    });
    // OKX's dispose replaces the inherited one, which it still calls as super.dispose.
    let inherited = scope.this.get("dispose").unwrap_or(Js::Undefined);
    let s = scope.clone();
    let _ = scope.this.define(
        "dispose",
        Js::function("dispose", move |_, _| {
            socket_boundaries::dispose(|path| {
                match path.split_once('.') {
                    Some(("super", _)) => call(&inherited, vec![]),
                    Some((owner, member)) => s.this.get(owner)?.call_method(member, vec![]),
                    None => Err(Thrown::type_error(path)),
                }
                .map(|_| ())
            })?;
            Ok(Js::Undefined)
        }),
    );
}

/// Every closure-typed OKXExchange member, with the EU overrides for EU variants.
pub(crate) fn install(scope: &OkxScope, variant: Variant) {
    market_shape(scope, variant);
    account(scope, variant);
    position_mode(scope);
    price_loads(scope);
    prices(scope);
    orders(scope);
    sockets(scope);
}
